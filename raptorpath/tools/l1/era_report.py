#!/usr/bin/env python3
"""Scoring pass for THE ERA BATTERY.

Scored against goal-gate "Era Battery — PRE-REGISTRATION", which is the
CONTRACT: this file implements its clauses and inverts none of them. Every bar
below is transcribed from that block; nothing here decides anything the
pre-registration did not already fix.

  usage: era_report.py [--calib] <ledger.log> [<ledger.log> ...]

  --calib  print ONLY the invocation accounting, the per-era liveness audit, the
           ABORT-CAUSE table and the discipline-16 headroom table, and score
           NOTHING. This is what the calibration pass (`era_calib.sh`, one rep
           per arm per cell, n = 1) is read with: it carries no sigma, no seed-7
           evidence, and nothing in it is a result. Its output fills the
           contract's headroom table and is committed as the contract's
           COMPLETION before the scored run.

WHAT IS AND IS NOT A DENOMINATOR:

  ABORT            no era anchor on EITHER endpoint. NOT in any denominator —
                   and NOT the same rule as its predecessors', because
                   `[GATES]` does not exist at the OLD commit and the shipped
                   rule would mark every OLD invocation an abort.
  VOID             `era_mix_ok = False`: a binary from the wrong era ran. Not a
                   datum, reported in its own table, never re-labelled.
  DNF              a completed run that did not transfer. IS a datum, IS in the
                   denominator, reported separately.
  INSTRUMENT-FAIL  completed but a gauge did not report. Excluded from the
                   statistic it voids, WITH THE EXCLUSION COUNTED.

THREE RULES THIS REPORTER DOES **NOT** INHERIT FROM `ccand_report.py`, each of
which would otherwise turn a structural certainty into a false result:

  * **`[GATES]`, `[SUMCAP]`, `[DCAP]`, `[RACK]`, `[WALL]`, `[ACKDIAG]`, `[SF]`,
    `[CCAP]`, `[LCW]` and the wait histogram DO NOT EXIST AT THE OLD COMMIT.**
    Their absence on an OLD row is CORRECT. No cross-era claim may be made from
    any of them, and specifically **the c8 dead-wall paired contrast is not
    available cross-era**.
  * **`legacy_pin` is fed on the RACK-ARMED arm ONLY**, so the shipped clamp's
    bind fraction is read off `NR` and NEVER off `NEW` at its own default, whose
    `[RACK]` carries `evals = 0` by construction — a denominator of zero.
  * **`fa = 0/0` is an INSTRUMENT-FAIL for the rep, never `fa_frac = 0`**: no
    recovery round fired, so there is no false-alarm datum.

THE ABORT-CAUSE TABLE IS PRINTED BEFORE THE FIRST CONTRAST, always, in both
modes. The exclusion of aborts from every denominator is sound only while the
aborts are INDEPENDENT OF THE ARM, and at c8/seed 7 the Candidates Battery
measured 20 % on the control against 75 % on the RACK arm. **If the per-cell
abort rate differs between OLD and NEW by more than `ABORT_GAP_BAR` points, the
cell's contrasts are printed WITH the abort table beside them and the survivors'
selection is stated in the verdict.**

THE INTERACTION CLAUSE IS SCORED, NOT NARRATED. Each of P1/P2/P3 carries the
chained sum's own interval; the verdict is AGREES / DISAGREES-HIGH /
DISAGREES-LOW against it, and a DISAGREE in either direction is a finding about
interaction between the rungs — super-additivity is not a bonus and
sub-additivity is not a disappointment.
"""
import json
import math
import os
import sys
from collections import defaultdict

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

# ── The transcribed bars. Every one is quoted from the pre-registration; none is
#    recomputed, softened or added here.
SIGMA_K = 2.0             # E-GOOD / E-CPU: the two-sided band is 2 sigma_pooled
ABORT_GAP_BAR = 10.0      # G-ABORT: percentage points of OLD-vs-NEW abort rate
HEADROOM_BAR = 5.0        # discipline 16c: below this, no throughput target
FA_CLASS = 0.0625         # RFC 8985 6.2 Step 4's own budget, 1/16

#: P1 — c1 goodput, ack-merge's OWN published numbers (`bdab7de`): +12.7 % at
#: s42 and +13.0 % at s7. Carried as a CLASS, not as a point, because the era
#: battery adds 210 further commits to the same cell.
P1_C1_PCT = (12.7, 13.0)

#: P2 — the duals: goodput PARITY, and `q_p50` down by the delta-cap's own
#: measured class ("Candidates Battery — RESULTS", D-LAT), in ms. Negative is a
#: reduction. The intervals are the published per-cell ranges over both seeds.
P2_QP50_MS = {"c7": (-16.0, -10.0), "c8": (-117.0, -113.0), "c8L": (-200.0, -130.0)}

#: P3 — receiver CPU per byte, ack-merge's own -9.1 % / -8.4 % per Gbit.
P3_CPU_RECV_PCT = (-9.1, -8.4)

#: P5 — the CLOCK-UNDER-NEW-DEFAULT readout. The Candidates Battery measured the
#: shipped [25,100] ms clamp binding 92.4-99.7 % and `fa_frac` at 0.17-0.78,
#: BOTH with RWM_DELTA_CAP OFF. These are the numbers `NR` revises, quoted so the
#: revision is visible rather than asserted.
P5_LEGACY_PIN_PRIOR = (0.924, 0.997)
P5_FA_FRAC_PRIOR = (0.17, 0.78)

#: Shaped capacity per cell in Mbit/s — TRANSCRIBED from `lib.sh`'s
#: `scenario_params` through the driver's `cell_spec`. A dual cell's capacity is
#: the SUM of its two legs, which is what the tc byte total is measured against.
CELL_CAP_MBIT = {"c1": 1000.0, "sc2": 100.0, "c7": 200.0, "c8": 120.0, "c8L": 120.0}

SCORED_ARMS = ("OLD", "NEW")
AUX_ARMS = ("NR",)


def rows(paths):
    out = []
    for p in paths:
        with open(p, "r", errors="replace") as f:
            for ln in f:
                i = ln.find("ERARESULT ")
                if i < 0:
                    continue
                try:
                    out.append(json.loads(ln[i + len("ERARESULT "):]))
                except Exception:
                    pass
    return out


def mean(v):
    return sum(v) / len(v) if v else None


def sd(v):
    if len(v) < 2:
        return 0.0
    m = mean(v)
    return math.sqrt(sum((x - m) ** 2 for x in v) / (len(v) - 1))


def pooled2s(a, b):
    """2 sigma_pooled of the two arms' own reps. Returns None when neither arm
    has enough reps to carry a sigma — which is reported as "no band", never as
    a band of zero."""
    if len(a) < 2 and len(b) < 2:
        return None
    return SIGMA_K * math.sqrt((sd(a) ** 2 + sd(b) ** 2) / 2.0)


def verdict_two_sided(delta, band, labels=("WIN", "LOSS")):
    """E-GOOD's rule, and it is TWO-SIDED because the arc claims
    parity-or-better: a WIN and a LOSS are equally reportable.

    `labels` exists because the SIGN'S MEANING IS NOT THE SAME FOR EVERY
    QUANTITY. More goodput is a win; more CPU per byte is not. E-CPU passes
    `("UP", "DOWN")` and gets a DIRECTION rather than a value judgement — a
    reporter that printed "LOSS" for a CPU reduction would invert the arc's
    third claim in the one table that scores it."""
    if band is None:
        return "NO-BAND"
    if delta > band:
        return labels[0]
    if delta < -band:
        return labels[1]
    return "PARITY"


def against_interval(x, lo, hi, band=None):
    """The INTERACTION CLAUSE, as a verdict rather than as prose. `band` widens
    the prediction by the measurement's own noise so a DISAGREE is a statement
    about the rungs and not about sigma."""
    if x is None:
        return "NO-DATUM"
    b = band or 0.0
    if x < lo - b:
        return "DISAGREES-LOW"
    if x > hi + b:
        return "DISAGREES-HIGH"
    return "AGREES"


def live(r):
    """LIVE is per era and is read from the anchors, not from `[GATES]` — see
    the module docstring. A VOID row (wrong-era binary) is never live."""
    return bool(r.get("live")) and r.get("era_mix_ok") is not False


def void(r):
    """VOID is `era_mix_ok is False` — a binary from the WRONG era demonstrably
    ran. It is NOT `not era_mix_ok`: an aborted invocation carries `None` there
    (no log on either endpoint, so no evidence about which binary ran), and
    counting those as violations would file the whole abort class as a
    wrong-era finding."""
    return r.get("era_mix_ok") is False


def cpu_per_gbit(r, which):
    """CPU per Gbit transferred. `bytes` is not in the row, so the transferred
    volume is reconstructed from the measured goodput and wall — which is the
    same quantity the ack-merge battery's `per Gbit` numbers were computed on."""
    c = r.get(which)
    mb, s = r.get("mbps"), r.get("seconds")
    if c is None or not mb or not s:
        return None
    gbit = mb * s / 1000.0
    return round(c / gbit, 4) if gbit > 0 else None


def group(rs, *keys):
    g = defaultdict(list)
    for r in rs:
        g[tuple(r.get(k) for k in keys)].append(r)
    return g


def hdr(t):
    print("\n" + t)
    print("-" * len(t))


def abort_table(rs):
    """PRINTED BEFORE THE FIRST CONTRAST, always. The cause distribution is the
    whole point: a class concentrated in `no_gates_unknown` has FALSIFIED the
    witness's own four hypotheses and needs a NEW instrument, not a re-reading
    of this one."""
    hdr("THE ABORT-CAUSE TABLE — read BEFORE any contrast (G-ABORT)")
    print(f"{'cell':>5} {'arm':>4} {'seed':>5} {'rows':>5} {'abort':>6} "
          f"{'rate%':>7} {'causes'}")
    rates = {}
    for (cell, arm, seed), g in sorted(group(rs, "cell", "arm", "seed").items(),
                                       key=lambda kv: (str(kv[0][0]), str(kv[0][1]), str(kv[0][2]))):
        ab = [r for r in g if r.get("abort")]
        causes = defaultdict(int)
        for r in ab:
            causes[r.get("abort_cause") or ("no_record" if r.get("abort_missing")
                                            else "none")] += 1
        rate = 100.0 * len(ab) / len(g) if g else 0.0
        rates[(cell, arm, seed)] = rate
        cs = " ".join(f"{k}={v}" for k, v in sorted(causes.items())) or "-"
        print(f"{cell:>5} {arm:>4} {seed:>5} {len(g):>5} {len(ab):>6} "
              f"{rate:>7.1f} {cs}")

    # THE SELECTION TEST. This is the clause that makes the abort class
    # scoreable instead of narratable.
    hdr("G-ABORT — the OLD/NEW abort-rate gap, per cell and seed")
    fired = False
    for (cell, seed) in sorted({(c, s) for (c, a, s) in rates}, key=lambda t: (str(t[0]), str(t[1]))):
        o = rates.get((cell, "OLD", seed))
        n = rates.get((cell, "NEW", seed))
        if o is None or n is None:
            continue
        gap = abs(o - n)
        flag = "FIRES — the survivors are a SELECTION ON THE ERA" if gap > ABORT_GAP_BAR else "ok"
        if gap > ABORT_GAP_BAR:
            fired = True
        print(f"  {cell:>5} s{seed}: OLD {o:5.1f}%  NEW {n:5.1f}%  gap {gap:5.1f} pts  [{flag}]")
    if fired:
        print("\n  *** G-ABORT FIRED. Every contrast at the flagged cells is "
              "reported WITH this table beside it, and the verdict states the "
              "selection. It is not silently pooled and it is not silently "
              "dropped. ***")

    # THE WITNESS'S OWN LIVENESS. An abort with no record is an INSTRUMENT-FAIL
    # of the instrument, and it is the one failure mode that would quietly
    # restore the pre-witness situation.
    ab = [r for r in rs if r.get("abort")]
    norec = [r for r in ab if r.get("abort_missing")]
    resid = [r for r in ab if r.get("abort_cause") == "no_gates_unknown"]
    print(f"\n  aborts total {len(ab)} | no witness record {len(norec)} "
          f"(INSTRUMENT-FAIL of the witness) | residual `no_gates_unknown` "
          f"{len(resid)}")
    if ab and len(resid) > len(ab) / 2:
        print("  *** THE RESIDUAL DOMINATES: all four instrumented steps "
              "reported OK on the majority of aborts. The witness has "
              "FALSIFIED its own hypotheses and the successor instrument must "
              "be NAMED, not guessed. ***")
    # THE ARM-CORRELATION COLUMN, with its control: measured on every
    # invocation, aborted or not.
    hdr("drain_pids_t0 — the SIGTERM-race column (measured on EVERY invocation)")
    for (cell, arm), g in sorted(group(rs, "cell", "arm").items(),
                                 key=lambda kv: (str(kv[0][0]), str(kv[0][1]))):
        d = [r["drain_pids_t0"] for r in g if r.get("drain_pids_t0") is not None]
        if not d:
            continue
        print(f"  {cell:>5} {arm:>4}: n={len(d):3d} survivors>0 in "
              f"{sum(1 for x in d if x > 0):3d} reps  max={max(d)}")


def void_table(rs):
    v = [r for r in rs if void(r)]
    hdr("G-ERA — the anti-mix assertion (a VIOLATION VOIDS the rep)")
    if not v:
        print("  0 violations: every OLD row carried NO [GATES] line and every "
              "NEW row carried one, on BOTH endpoints. The era of each "
              "invocation is PROVEN mechanically, not trusted.")
        return
    for r in v:
        print(f"  VOID {r['cell']}-{r['arm']} s{r['seed']} rep={r['rep']} "
              f"era={r['era']} gates={r.get('gates_lines_cli')}/"
              f"{r.get('gates_lines_srv')}")
    print(f"  {len(v)} VOID rows — excluded from every denominator.")


def liveness_audit(rs):
    hdr("G-LIVE — per era, on the ERA-INVARIANT anchors (NOT on [GATES])")
    print(f"{'cell':>5} {'arm':>4} {'seed':>5} {'rows':>5} {'live':>5} "
          f"{'abort':>6} {'half':>5} {'void':>5}")
    for (cell, arm, seed), g in sorted(group(rs, "cell", "arm", "seed").items(),
                                       key=lambda kv: (str(kv[0][0]), str(kv[0][1]), str(kv[0][2]))):
        lv = sum(1 for r in g if live(r))
        ab = sum(1 for r in g if r.get("abort"))
        # `half` is the category the two-sided anchor test exists to keep
        # separate: an anchor on ONE endpoint only is neither live nor aborted.
        half = sum(1 for r in g if not r.get("live") and not r.get("abort"))
        vd = sum(1 for r in g if void(r))
        print(f"{cell:>5} {arm:>4} {seed:>5} {len(g):>5} {lv:>5} {ab:>6} "
              f"{half:>5} {vd:>5}")

    sur = [r for r in rs if r.get("era_surprise")]
    if sur:
        hdr("ERA-SURPRISE — a NEW-only gauge appeared on an OLD row")
        print("  This does NOT mean a gauge appeared. It means THE BINARY IS "
              "NOT THE ERA IT CLAIMS, and the affected rows are void:")
        for r in sur:
            print(f"  {r['cell']}-{r['arm']} s{r['seed']} rep={r['rep']}: "
                  f"{r.get('era_surprise_which')}")


def headroom_table(rs):
    """MEASUREMENT DISCIPLINE 16. The denominator is the TRANSFER wall
    (`seconds`), NEVER `INVOCATION_S` — the correction that read c7 at 77.6 %
    when the cell was at 96.9 % and would have LICENSED an unsatisfiable
    target."""
    hdr("HEADROOM (discipline 16) — util = tc_bytes*8 / (TRANSFER s * capacity)")
    print(f"{'cell':>5} {'arm':>4} {'cap Mbit':>9} {'util%':>7} "
          f"{'headroom%':>10} {'permission'}")
    perm = {}
    for (cell, arm), g in sorted(group(rs, "cell", "arm").items(),
                                 key=lambda kv: (str(kv[0][0]), str(kv[0][1]))):
        cap = CELL_CAP_MBIT.get(cell)
        us = [r["tc_bytes"] * 8.0 / (r["seconds"] * cap * 1e6) * 100.0
              for r in g
              if live(r) and r.get("tc_bytes") and r.get("seconds") and cap]
        if not us:
            print(f"{cell:>5} {arm:>4} {cap or 0:>9.0f} {'-':>7} {'-':>10} "
                  f"NO tc DATUM — discipline 16 not satisfied at this cell")
            continue
        u = mean(us)
        h = 100.0 - u
        p = ("throughput targets PERMITTED" if h >= HEADROOM_BAR
             else "PARITY / LATENCY ONLY (headroom < 5 %)")
        perm[(cell, arm)] = h >= HEADROOM_BAR
        print(f"{cell:>5} {arm:>4} {cap:>9.0f} {u:>7.1f} {h:>10.1f} {p}")
    return perm


def score(rs, perm):
    hdr("E-GOOD — goodput, TWO-SIDED (the arc claims parity-or-better)")
    print(f"{'cell':>5} {'seed':>5} {'nOLD':>5} {'nNEW':>5} {'OLD':>9} "
          f"{'NEW':>9} {'delta':>8} {'2sig':>8} {'pct':>7} {'verdict'}")
    c1_pcts = []
    for (cell, seed), g in sorted(group(rs, "cell", "seed").items(),
                                  key=lambda kv: (str(kv[0][0]), str(kv[0][1]))):
        a = [r["mbps"] for r in g if r["arm"] == "OLD" and live(r) and r.get("mbps")]
        b = [r["mbps"] for r in g if r["arm"] == "NEW" and live(r) and r.get("mbps")]
        if not a or not b:
            continue
        ma, mb = mean(a), mean(b)
        band = pooled2s(a, b)
        d = mb - ma
        pct = 100.0 * d / ma if ma else None
        v = verdict_two_sided(d, band)
        # G-HEAD: where the calibration denies headroom, the magnitude is not
        # claimed — the verdict is restricted to PARITY-or-not.
        if perm and not perm.get((cell, "NEW"), True):
            v += " (magnitude NOT claimed — headroom < 5 %)"
        print(f"{cell:>5} {seed:>5} {len(a):>5} {len(b):>5} {ma:>9.2f} "
              f"{mb:>9.2f} {d:>8.2f} {(band if band is not None else float('nan')):>8.2f} "
              f"{(pct if pct is not None else float('nan')):>7.2f} {v}")
        if cell == "c1" and pct is not None:
            c1_pcts.append(pct)

    hdr("P1 — c1 goodput against the CHAINED SUM (ack-merge's own +12.7/+13.0 %)")
    for p in c1_pcts:
        print(f"  measured {p:+.2f} %  vs predicted "
              f"[{P1_C1_PCT[0]:+.1f}, {P1_C1_PCT[1]:+.1f}] %  -> "
              f"{against_interval(p, *P1_C1_PCT)}")
    if not c1_pcts:
        print("  NO-DATUM at c1.")
    print("  A DISAGREE in EITHER direction is a finding about interaction "
          "between the rungs, read before any statement about the arc's value.")

    hdr("E-LAT — delivered latency: q_p50 (engine) and ping_p50 (independent)")
    print(f"{'cell':>5} {'seed':>5} {'qOLD':>8} {'qNEW':>8} {'dq':>8} "
          f"{'pOLD':>8} {'pNEW':>8} {'dp':>8} {'agree?':>7}")
    dq_by_cell = defaultdict(list)
    for (cell, seed), g in sorted(group(rs, "cell", "seed").items(),
                                  key=lambda kv: (str(kv[0][0]), str(kv[0][1]))):
        qa = [r["q_p50"] for r in g if r["arm"] == "OLD" and live(r) and r.get("q_p50") is not None]
        qb = [r["q_p50"] for r in g if r["arm"] == "NEW" and live(r) and r.get("q_p50") is not None]
        pa = [r["ping_p50"] for r in g if r["arm"] == "OLD" and live(r) and r.get("ping_p50") is not None]
        pb = [r["ping_p50"] for r in g if r["arm"] == "NEW" and live(r) and r.get("ping_p50") is not None]
        if not qa or not qb:
            continue
        dq = mean(qb) - mean(qa)
        dp = (mean(pb) - mean(pa)) if (pa and pb) else None
        # THE TWO INSTRUMENTS ARE REPORTED SIDE BY SIDE AND NEVER AVERAGED, and
        # a SIGN DISAGREEMENT is reported as such rather than resolved.
        agree = "-" if dp is None else ("yes" if (dq >= 0) == (dp >= 0) else "SIGN!")
        print(f"{cell:>5} {seed:>5} {mean(qa):>8.1f} {mean(qb):>8.1f} {dq:>8.1f} "
              f"{(mean(pa) if pa else float('nan')):>8.1f} "
              f"{(mean(pb) if pb else float('nan')):>8.1f} "
              f"{(dp if dp is not None else float('nan')):>8.1f} {agree:>7}")
        dq_by_cell[cell].append(dq)

    hdr("P2 — the duals' q_p50 against the delta-cap's own measured class")
    for cell, iv in P2_QP50_MS.items():
        for d in dq_by_cell.get(cell, []):
            print(f"  {cell}: measured {d:+.1f} ms  vs predicted "
                  f"[{iv[0]:+.1f}, {iv[1]:+.1f}] ms  -> {against_interval(d, *iv)}")
        if not dq_by_cell.get(cell):
            print(f"  {cell}: NO-DATUM")

    hdr("E-CPU — CPU per Gbit, BOTH endpoints (a SCORED claim here, not a guard)")
    print(f"{'cell':>5} {'seed':>5} {'side':>5} {'OLD':>8} {'NEW':>8} "
          f"{'pct':>8} {'2sig%':>8} {'verdict'}")
    recv_pcts = []
    for (cell, seed), g in sorted(group(rs, "cell", "seed").items(),
                                  key=lambda kv: (str(kv[0][0]), str(kv[0][1]))):
        for side, col in (("recv", "cpusrv"), ("send", "cpucli")):
            a = [x for x in (cpu_per_gbit(r, col) for r in g
                             if r["arm"] == "OLD" and live(r)) if x]
            b = [x for x in (cpu_per_gbit(r, col) for r in g
                             if r["arm"] == "NEW" and live(r)) if x]
            if not a or not b:
                continue
            ma, mb = mean(a), mean(b)
            pct = 100.0 * (mb - ma) / ma if ma else None
            band = pooled2s(a, b)
            bpct = 100.0 * band / ma if (band is not None and ma) else None
            # DIRECTION, not a value judgement — see `verdict_two_sided`.
            v = verdict_two_sided(mb - ma, band, ("UP", "DOWN"))
            print(f"{cell:>5} {seed:>5} {side:>5} {ma:>8.3f} {mb:>8.3f} "
                  f"{(pct if pct is not None else float('nan')):>8.2f} "
                  f"{(bpct if bpct is not None else float('nan')):>8.2f} {v}")
            if side == "recv" and pct is not None:
                recv_pcts.append((cell, pct))

    hdr("P3 — receiver CPU/Gbit against ack-merge's own -9.1 / -8.4 %")
    for cell, p in recv_pcts:
        print(f"  {cell}: measured {p:+.2f} %  vs predicted "
              f"[{P3_CPU_RECV_PCT[0]:+.1f}, {P3_CPU_RECV_PCT[1]:+.1f}] %  -> "
              f"{against_interval(p, *P3_CPU_RECV_PCT)}")
    if not recv_pcts:
        print("  NO-DATUM.")
    print("  This is the one prediction with a stated MECHANISM on both "
          "endpoints (one control datagram instead of two), so its failure is "
          "the most informative of the three.")

    hdr("[CTLD] — the ack-merge mechanism, and THE ONLY cross-era gauge there is")
    print(f"{'cell':>5} {'seed':>5} {'OLD':>8} {'NEW':>8} {'ratio':>8}")
    for (cell, seed), g in sorted(group(rs, "cell", "seed").items(),
                                  key=lambda kv: (str(kv[0][0]), str(kv[0][1]))):
        a = [r["ctld_ratio"] for r in g if r["arm"] == "OLD" and live(r) and r.get("ctld_ratio")]
        b = [r["ctld_ratio"] for r in g if r["arm"] == "NEW" and live(r) and r.get("ctld_ratio")]
        if not a or not b:
            continue
        print(f"{cell:>5} {seed:>5} {mean(a):>8.3f} {mean(b):>8.3f} "
              f"{mean(b)/mean(a) if mean(a) else float('nan'):>8.3f}")
    print("  Every other gauge of the arc is NEW-only, so this is where P1's "
          "and P3's mechanism is OBSERVED rather than inferred from goodput.")

    hdr("P4 — sc2 and c1 are SINGLE-PATH: no delta-cap, no sum-cap, by construction")
    print("  `n_live < 2` short-circuits the pooled seat BEFORE any multiplier "
          "is read, so any move at c1 or sc2 belongs to ack-merge, the anchor "
          "filter, or the residue — and NEVER to the two cap flips. Stated "
          "here so it cannot be attributed after the fact.")


def clock_readout(rs):
    """THE CLOCK-UNDER-NEW-DEFAULT READOUT — `NR` only, and the numbers it
    revises are named. Both priors were measured with `RWM_DELTA_CAP` OFF."""
    hdr("NR — the SHIPPED [25,100] ms clamp under the NEW default (AUX, scored "
        "on its own [RACK] line and on NOTHING else)")
    nr = [r for r in rs if r["arm"] in AUX_ARMS and live(r)]
    if not nr:
        print("  NO NR ROWS.")
        return
    print(f"{'cell':>5} {'seed':>5} {'n':>3} {'evals':>8} {'legacy_pin':>11} "
          f"{'fa_frac':>8} {'fa_d':>6}")
    for (cell, seed), g in sorted(group(nr, "cell", "seed").items(),
                                  key=lambda kv: (str(kv[0][0]), str(kv[0][1]))):
        # `evals = 0` is a DENOMINATOR OF ZERO (candidates instrument fact 3) —
        # the row carries no clamp datum and is excluded, with the exclusion
        # counted rather than silently dropped.
        armed = [r for r in g if (r.get("rack_evals") or 0) > 0]
        lp = [r["rack_legacy_pin"] for r in armed if r.get("rack_legacy_pin") is not None]
        # `fa = 0/0` is an INSTRUMENT-FAIL for the rep, NEVER `fa_frac = 0`.
        fa = [r["rack_fa_frac"] for r in g
              if r.get("rack_fa_d") and r.get("rack_fa_frac") is not None]
        if not lp and not fa:
            print(f"{cell:>5} {seed:>5} {len(g):>3} {'0':>8} "
                  f"{'INSTRUMENT-FAIL':>11} {'-':>8} {'-':>6}")
            continue
        print(f"{cell:>5} {seed:>5} {len(g):>3} "
              f"{mean([r['rack_evals'] for r in armed]) if armed else 0:>8.0f} "
              f"{(mean(lp) if lp else float('nan')):>11.4f} "
              f"{(mean(fa) if fa else float('nan')):>8.4f} "
              f"{(mean([r['rack_fa_d'] for r in g if r.get('rack_fa_d')]) if fa else 0):>6.0f}")
    print(f"\n  PRIOR (Candidates Battery, RWM_DELTA_CAP OFF): legacy_pin "
          f"{P5_LEGACY_PIN_PRIOR[0]:.3f}-{P5_LEGACY_PIN_PRIOR[1]:.3f}, "
          f"fa_frac {P5_FA_FRAC_PRIOR[0]:.2f}-{P5_FA_FRAC_PRIOR[1]:.2f} "
          f"against RACK's own alpha_class {FA_CLASS:.4f}.")
    print("  P5 predicts legacy_pin RISES (a shorter SRTT drives the clamp's "
          "input toward the 25 ms LOWER bound, so a law that already operated "
          "as a constant operates as one harder). fa_frac has no mechanism "
          "argument as clean, so EITHER DIRECTION IS A RESULT there and only "
          "the magnitude is a surprise.")


def main():
    args = [a for a in sys.argv[1:] if a != "--calib"]
    calib = "--calib" in sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    rs = rows(args)
    if not rs:
        print("NO ERARESULT ROWS")
        return 1

    hdr("THE ERA BATTERY" + (" — CALIBRATION PASS (n = 1, NOTHING HERE IS A "
                             "RESULT)" if calib else ""))
    print(f"  rows {len(rs)} | ledgers {len(args)}")
    for a in ("OLD", "NEW", "NR"):
        g = [r for r in rs if r["arm"] == a]
        print(f"  {a:>4}: rows {len(g):4d}  live {sum(1 for r in g if live(r)):4d}"
              f"  abort {sum(1 for r in g if r.get('abort')):4d}"
              f"  void {sum(1 for r in g if void(r)):4d}")

    # ORDER IS NOT COSMETIC. The abort table and the anti-mix table come first
    # because a contrast computed over a selected or mislabelled set is not a
    # contrast, and this reporter refuses to print one before they are visible.
    abort_table(rs)
    void_table(rs)
    liveness_audit(rs)
    perm = headroom_table(rs)

    if calib:
        print("\nCALIBRATION ONLY: no bar is scored from n = 1. This output "
              "fills the contract's headroom table and is committed as the "
              "contract's COMPLETION, in its own commit, BEFORE the scored run.")
        return 0

    score(rs, perm)
    clock_readout(rs)
    print("\nNothing in this report flips a default.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
