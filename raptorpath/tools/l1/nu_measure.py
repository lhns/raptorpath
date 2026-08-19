#!/usr/bin/env python3
"""nu_measure.py — MEASURE nu, the cost-ratio memo's one missing input.

WHAT nu IS.  Option (d) of `docs/research/cost-ratio-memo.md` ("SYMMETRIC /
POWER — alpha where the marginal costs are equal") closes the recovery clock
with no fitted constant, on this stationarity condition:

    alpha^{3/2} * (1-alpha)^{1/2}  =  delta * p * sigma / (2 * nu * d)

Four of the five symbols on the right are contract-declared or already
measured.  The fifth is nu, and the memo describes it as

    "nu | DERIVABLE, NOT CURRENTLY REPORTED — fires per delivered symbol.
     `fired` is already counted (RackClockGauge, net/mod.rs:4192-4193); the
     delivered-symbol count already exists; their ratio is not printed."

nu decides two things and nothing else:

  1. WHETHER (d) HAS AN INTERIOR SOLUTION AT ALL.  max_alpha of the left side
     is 3*sqrt(3)/16 = 0.3248 at alpha = 0.75, so an interior optimum exists
     iff  nu >= delta*p*sigma / (0.6495*d).  Below that, (d) is a corner:
     "fire immediately, never wait".
  2. WHETHER OPTIONS (b) AND (d) ARE THE SAME OPTION.  The memo's chief
     technical residue: (b) and (d) agree at c8/Auto provided nu ~= 0.0097.
     "If a measurement of nu at c8 lands near 0.01, then (b) and (d) are the
     same option and there is materially less to decide than 16.69 implied.
     If nu lands an order of magnitude away, they diverge and the choice
     between them is real."

DOES THIS NEED A VM RUN?  NO.  That is this script's main finding.  Both
counters are already in COMMITTED ledgers:

  * `fired` — `rack_fa_d_cli`, the denominator of the `[RACK]` line's
    `fa=<spurious>/<fired>`.  It is fed on EVERY arm including the shipped
    control (16.68.1), so it is present in every Candidates Battery record.
  * the symbol count — `dgq_hand`, the datagram-queue HANDOFF counter summed
    over paths (`ccand_parse.py:705`), i.e. symbols the sender handed to the
    wire.

So nu is a ratio of two numbers already sitting in `docs/l1-raw/ccand-*.log`,
across 621 committed L1 records at five cells and two seeds.  No invocation,
no lock, no VM.

THE DENOMINATOR, AND WHY THERE ARE TWO OF THEM.  The memo says "per delivered
symbol"; `dgq_hand` counts SENT symbols, which at a lossy cell is the larger
number.  Rather than pick one and hide the choice, this script reports both:

  * `nu_sent  = fired / dgq_hand`               — needs NO constant at all.
  * `nu_good  = fired / delivered_symbols`,  where delivered_symbols is
    derived from the run's own goodput and its own implied symbol size:
        delivered_bytes  = mbps * 1e6 / 8 * seconds
        implied_sym_B    = delivered_bytes / dgq_hand      [reported, not used]
    and delivered_symbols = delivered_bytes / implied_sym_B == dgq_hand.

    That identity is the point: with the symbol size taken from the run
    itself rather than assumed, the goodput route COLLAPSES onto the handoff
    route, so `nu_sent` is not a proxy for `nu_good` — it is the same number
    computed two ways, and the only wedge between them is the sent-versus-
    delivered gap.  That gap is bounded by the run's own measured loss, which
    is reported as `pl_p0` and applied as `nu_good = nu_sent / (1 - loss)`.

    IMPLIED SYMBOL SIZE IS THEREFORE A CHECK, NOT AN INPUT.  If it does not
    land near the engine's MTU-class payload, one of the two counters is not
    counting what this script thinks it counts, and the row is suspect.  It is
    printed on every row for exactly that reason.

WHAT THIS SCRIPT DOES NOT DO.  It does not choose an option, does not compute
an alpha, and does not touch a gate or a default.  It measures one number the
memo asks for and puts it beside the two thresholds the memo states.  The
decision remains the user's.

USAGE
    python3 nu_measure.py raptorpath/docs/l1-raw/ccand-*.log
    python3 nu_measure.py --json ...          machine-readable rows
"""

import argparse
import json
import re
import statistics
import sys

# ── The memo's own numbers, transcribed. Nothing here is fitted or invented. ──

# cost-ratio-memo 3(d): the closure condition's constant,
#   max_alpha alpha^{3/2}(1-alpha)^{1/2} = 3*sqrt(3)/16, attained at alpha=0.75.
CLOSURE_MAX = 3 * (3 ** 0.5) / 16  # 0.32476

# cost-ratio-memo 3(d): delta = COPA_DELTA/zeta(hint), scheduler/mod.rs:129-132.
DELTA_BY_HINT = {"Realtime": 50.0, "Auto": 0.5, "Bulk": 0.005}

# cost-ratio-memo 3(d), "AND (d) AGREES WITH (b) AT AUTO": the fires-per-symbol
# rate at which options (b) and (d) become the same option at c8/Auto.
NU_STAR_C8_AUTO = 0.0097

# cost-ratio-memo 2.3, the sigma ESTIMATE obtained by inverting Cantelli
# against the shipped record. A LOWER BOUND reported as a point value, and the
# memo says so; superseded the moment the [DIAG] sig_us= field is run at L1.
SIGMA_EST_MS = {"c1": 8.1, "c7": 15.2, "c8": 18.1, "c8L": 10.6}
# cost-ratio-memo 2.2, receiver-site srtt (ms) — the clock (d) is stated on.
SRTT_WIRE_MS = {"c1": 2.0, "c7": 72.0, "c8": 77.0, "c8L": 82.0, "sc2": 101.0}

RESULT_RE = re.compile(r"^[A-Z0-9_]*RESULT\s+(\{.*\})\s*$")


def rows_from(path):
    """Every ledger record in one committed L1 log that carries both counters."""
    out = []
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        for line in fh:
            m = RESULT_RE.match(line.rstrip("\n"))
            if not m:
                continue
            try:
                r = json.loads(m.group(1))
            except json.JSONDecodeError:
                continue
            fired = r.get("rack_fa_d_cli")
            hand = r.get("dgq_hand")
            # A record without BOTH counters is SKIPPED and counted, never
            # imputed: a missing instrument is a missing instrument.
            if not fired or not hand:
                out.append({"cell": r.get("cell"), "skipped": True})
                continue
            mbps, secs = r.get("mbps"), r.get("seconds")
            delivered_B = (mbps * 1e6 / 8.0 * secs) if (mbps and secs) else None
            loss = r.get("pl_p0") or 0.0
            nu_sent = fired / hand
            out.append({
                "cell": r.get("cell"),
                "arm": r.get("arm"),
                "seed": r.get("seed"),
                "rep": r.get("rep"),
                "skipped": False,
                "fired": fired,
                "spurious": r.get("rack_fa_n_cli"),
                "hand": hand,
                "loss": loss,
                "nu_sent": nu_sent,
                # sent -> delivered: the ONLY wedge between the two routes.
                "nu_good": nu_sent / (1.0 - loss) if loss < 1.0 else None,
                "implied_sym_B": (delivered_B / hand) if delivered_B else None,
                "src": path,
            })
    return out


def closure_threshold(cell, hint):
    """cost-ratio-memo 3(d): nu >= delta*p*sigma/(0.6495*d) for an interior (d).

    `p` is the realized per-path loss and is supplied by the caller's rows, so
    it is passed in rather than read from a table.  Returns None where the
    memo has no sigma or srtt for the cell (sc2's is vacuous by 2.2).
    """
    sigma = SIGMA_EST_MS.get(cell)
    d = SRTT_WIRE_MS.get(cell)
    if sigma is None or d is None:
        return None
    return lambda p: DELTA_BY_HINT[hint] * p * sigma / (2.0 * CLOSURE_MAX * d)


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("logs", nargs="+", help="committed L1 ledger files")
    ap.add_argument("--json", action="store_true", help="emit rows as JSON")
    args = ap.parse_args()

    rows = []
    for p in args.logs:
        try:
            rows.extend(rows_from(p))
        except OSError as e:
            print(f"skip {p}: {e}", file=sys.stderr)

    live = [r for r in rows if not r["skipped"]]
    skipped = len(rows) - len(live)
    if not live:
        print("NO RECORD carried both counters — nu is not measurable from "
              "these ledgers.", file=sys.stderr)
        return 1

    if args.json:
        json.dump(live, sys.stdout, indent=1)
        print()
        return 0

    print("nu = FIRES PER SYMBOL, measured off COMMITTED L1 ledgers "
          "(cost-ratio-memo option (d))")
    print(f"records: {len(live)} usable, {skipped} without both counters\n")

    cells = sorted({r["cell"] for r in live}, key=lambda c: (len(c), c))
    print(f"{'cell':>5} {'n':>4} {'fired':>10} {'symbols':>12} "
          f"{'nu_sent':>10} {'nu_good':>10} {'loss':>7} {'sym_B':>7}")
    print("-" * 74)
    per_cell = {}
    for c in cells:
        rs = [r for r in live if r["cell"] == c]
        nu_s = statistics.median(r["nu_sent"] for r in rs)
        nu_g = statistics.median(r["nu_good"] for r in rs if r["nu_good"] is not None)
        loss = statistics.median(r["loss"] for r in rs)
        symB = statistics.median(r["implied_sym_B"] for r in rs
                                 if r["implied_sym_B"] is not None)
        per_cell[c] = (nu_g, loss)
        print(f"{c:>5} {len(rs):>4} {sum(r['fired'] for r in rs):>10} "
              f"{sum(r['hand'] for r in rs):>12} {nu_s:>10.5f} {nu_g:>10.5f} "
              f"{loss:>7.4f} {symB:>7.0f}")

    print("\nAGAINST THE MEMO'S TWO THRESHOLDS "
          "(both transcribed from cost-ratio-memo 3(d); nothing fitted here)\n")
    print("1. IS (d) INTERIOR, OR A CORNER?  interior iff "
          "nu >= delta*p*sigma/(0.6495*d)\n")
    print(f"{'cell':>5} {'nu_good':>10} " +
          " ".join(f"{h:>22}" for h in DELTA_BY_HINT))
    print("-" * 80)
    for c in cells:
        nu_g, loss = per_cell[c]
        cols = []
        for hint in DELTA_BY_HINT:
            f = closure_threshold(c, hint)
            if f is None or loss <= 0.0:
                cols.append(f"{'no sigma/loss on record':>22}"
                            if f is None else f"{'p=0 -> interior':>22}")
                continue
            thr = f(loss)
            verdict = "INTERIOR" if nu_g >= thr else "corner"
            cols.append(f"{thr:>12.5f} {verdict:>9}")
        print(f"{c:>5} {nu_g:>10.5f} " + " ".join(cols))

    print("\n2. ARE (b) AND (d) THE SAME OPTION?  they coincide at c8/Auto "
          f"iff nu ~= {NU_STAR_C8_AUTO}\n")
    if "c8" in per_cell:
        nu_g, _ = per_cell["c8"]
        ratio = nu_g / NU_STAR_C8_AUTO
        print(f"   measured nu at c8 = {nu_g:.5f}   vs   nu* = "
              f"{NU_STAR_C8_AUTO}   ->  {ratio:.2f}x")
        if 0.5 <= ratio <= 2.0:
            print("   WITHIN A FACTOR OF 2: (b) and (d) are near enough the "
                  "same option that the choice between them is small.")
        else:
            print("   MORE THAN A FACTOR OF 2 AWAY: (b) and (d) DIVERGE and "
                  "the choice between them is real.")
    else:
        print("   no c8 record in these ledgers.")

    print("\nCAVEATS, stated rather than buried:")
    print(" * sigma here is the memo's 2.3 CANTELLI INVERSION — a lower bound")
    print("   reported as a point value, not a measurement. The [DIAG] sig_us=")
    print("   field supersedes it at the next L1 run; the thresholds above")
    print("   scale LINEARLY in sigma, so a 1.8x sigma is a 1.8x threshold.")
    print(" * `fired` is a SENDER-site count and the loss `p` is the sender's")
    print("   per-path estimate; both come from the same record, so unlike the")
    print("   memo's 2.3 this ratio does not cross sites.")
    print(" * dgq_hand counts SENT symbols; nu_good divides out the run's own")
    print("   measured loss to reach delivered. At a lossless cell they are")
    print("   the same number.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
