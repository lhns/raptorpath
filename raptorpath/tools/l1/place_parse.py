#!/usr/bin/env python3
"""Per-invocation parser for the PLACEMENT BATTERY (Track A, paper §16.81).

    place_parse.py <cell> <arm> <seed> <rep> <client.log> <server.log>

Emits one `PLACERESULT ` + JSON line. A SEPARATE parser from `lat_parse.py`
on purpose: that one is the latency-lever battery's instrument and its output
is what those verdicts were read off, so it is left byte-identical.

THE LINES IT READS, AND THE ENGINE LINE THAT PRINTS EACH (matched LITERALLY;
`test_place_parse.py` feeds synthetic lines copied from these format strings):

  `[ETA] site=sender`     `net/eta.rs:424-427` head, `:441-443` per path.
                          Sender = the perf `--client` = `/tmp/rwm-c.log`
                          (perf_rwm_c.sh:198-201); printed on `[DIAG]`'s
                          250 ms cadence by `net/diag.rs:975-981`.
  `[ETA] site=receiver`   `net/eta.rs:574-580`; receiver = `--server` =
                          `/tmp/rwm-s.log`, on `[SUCC]`'s 1 s cadence
                          (`net/receiver.rs:2148-2156`).
  `[LAT] site=receiver`   `net/lat.rs:239-268`. RECEIVER ONLY -- there is no
                          sender-side `[LAT]` in the engine.
  `[SUCC]`                `net/succ.rs:775-778`.
  `[DIAG]`                the sender's `rtt=` fields (the surrogate `ref`).

Every gauge is CUMULATIVE and the LAST line of its kind is the reading. Each
site's line is looked for in its OWN endpoint log first and in the other one
second, so a swapped log pair is a parse and not a silent zero.

`final=1` -- THE EXIT-FLUSH RULE. A concurrent engine branch adds an exit-flush
line to the receiver's diagnostic block (goal-gate "OPERATOR SANCTION
(2026-09-08 ~14:00Z)", the owed flush) carrying a `final=1` field. The safe
rule, implemented in `last_with()` and `count_kind()`:

  * a scraper that takes the LAST line of a kind gets the complete counts
    automatically -- and `last_with()` PREFERS a `final=1` line wherever it
    sits, so an exit flush that is followed by an unrelated line still wins;
  * a scraper that COUNTS lines of a kind (the cadence counts `*_lines`) must
    SKIP `final=1`, or the flush would read as one extra cadence tick.

Both formats parse: a log with no `final=` field at all reads exactly as it
did before the flush existed.

THE FIELDS, AND WHY EACH IS HERE -- in the order the pre-registration reads
them, which is NOT the order of interest:

  1. `[LAT]` -- THE DECOMPOSITION, READ FIRST. `sh_ax` / `sh_xp` / `sh_sp` /
     `sh_rep` are the shares of delivered latency by cause, and the CTL arm's
     values decide, BEFORE any challenger is looked at, whether this battery
     is measuring the right law at all:

         PLACEMENT-INDICTED   sh_xp  >= 0.5
         QUEUE-DOMINATED      sh_ax  >= 0.5  and  sh_xp <= 0.2
         REPAIR-DOMINATED     sh_rep >= 0.5
         MIXED                none of the above

     `INSTRUMENT-INDICTS-QUEUE` is a LEGAL OUTCOME of the whole track and may
     re-route it. `rw_xp == 0` at N = 1 is the control: the same field reads
     0.87-0.97 two paths over, so a zero at one path is a property of the wire
     and not an unreached emission site.

  2. `[ETA]` -- THE S4 SCORE, computed OFFLINE and before any arm verdict:
     `sig_ref = sigma/ref`, against the shipped constant's own claim of
     **0.19238** (`T = 0.15 <=> sigma_e = 0.19238*ref`). Three outcomes:
     ~0.192 at every cell (the law WAS a dispersion and 0.15 was a lucky
     guess), stable but elsewhere (the FORM is right, the value is wrong by a
     measurable amount), or varying across cells (a fixed `T` cannot be right
     anywhere). `ref` is read THREE ways and all three are carried: the
     engine's own `t_eff=` inversion (preferred: one clock), the sender
     line's own `tau_us` (`eta_s4.py`'s clean reference), and the
     `[DIAG] rtt=` surrogate.

  3. THE ARM BIND GAUGES, off the SENDER's `[ETA]` head (`net/eta.rs:424`):
       `zero=`      the fraction of stamped placements that carried the
                    `eta = 0` "no prediction" sentinel
       `cold_r=`    cold-r binds over `place_n=` cost evaluations
       `cold_ge=`   cold-GE binds over `place_n=`
       `t_eff=`     the temperature actually used; `t_cold=` the fraction of
                    resolutions with NO measured dispersion; `t_n=` the count
       `hol_sh=`    the `s_i > H` fraction -- kappa's bind
                    (`scheduler/mod.rs:608`)
       `hol_mv=`    the fraction of `hol_calls=` in which THE TERM MOVED THE
                    ARGMIN -- the EXECUTION WITNESS. A frontier term that never
                    changed an argmin was computed, not measured.
       `hol_w=`     the derived W of 16.80.6(a2), PREDICTED INERT.

  4. `[SUCC] xp_n/det` -- the cross-path resolution fraction D0 measured. It
     must FALL at c8/c9h under a working placement arm and be identically 0 at
     c1. **A MOVING c1 VOIDS THE RUN**, which is why `xp_n` is carried on every
     row and not only on the duals.

  5. GOODPUT -- `mbps`, `seconds`, `dnf`. A GUARD, PRE-DECLARED UNDERPOWERED at
     n = 8 and, by the 5 h amendment, at n = 4: reported so a regression is
     visible, never scored.

Every field is `None` when its own denominator is zero -- `-` iff `n = 0`, the
engine's own convention carried through the parser rather than papered over
with a default, because "the gauge read zero" and "the gauge never ran" are
different findings and this battery's whole point is telling them apart.
"""
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
# `read` splits a `tracing` record glued onto a readout line before matching
# (MEASURED on the placement battery's first row: the receiver's `[LAT] ...
# final=1` flush was followed on one line by "cleaning up TUN interface").
from l1common import field, fnum, is_final, last_with, read, split_interleaved  # noqa: E402,F401

SHIPPED_SIGMA_OVER_REF = 0.19238  # T = 0.15 <=> sigma_e = 0.19238*ref (16.81.1)
SQRT6_OVER_PI = 6.0 ** 0.5 / 3.141592653589793


def count_kind(lines, pat):
    """How many CADENCE lines of a kind fired. The exit flush is not a
    cadence tick and is skipped."""
    return sum(1 for ln in lines if pat in ln and not is_final(ln))


def num(line, key):
    """A numeric gauge field. `-` (the engine's `n = 0` rendering) is None,
    and so is an absent key -- which is what an OLD ENGINE looks like."""
    return fnum(field(line, key))


def slots(line):
    """The per-path slots of a gauge line, each rendered so `field()` reads
    it: the `p<id>:` prefix becomes its own token."""
    if not line:
        return []
    out = []
    for t in line.split():
        head = t.split(":", 1)
        is_head = (
            len(head) == 2
            and head[0].startswith("p")
            and len(head[0]) > 1
            and head[0][1:].isdigit()
        )
        if is_head:
            out.append(t.replace(":", " ", 1))
        elif out:
            out[-1] += " " + t
    return out


def slot_id(slot):
    return slot.split(" ", 1)[0]


def sigma_pair(slot):
    """`sig_us=<v|->/n<pairs>` -> (value_us or None, pairs)."""
    raw = field(slot, "sig_us=")
    if raw is None or "/n" not in raw:
        return (None, 0)
    v, n = raw.split("/n", 1)
    try:
        pairs = int(n)
    except ValueError:
        pairs = 0
    val = None if v == "-" else float(v)
    return (val, pairs)


def ratio_pair(slot, key):
    """`n=<matched>/<stamped>` (eta.rs:441) -> (matched, stamped) or Nones."""
    raw = field(slot, key)
    if raw is None or "/" not in raw:
        return (None, None)
    a, b = raw.split("/", 1)
    try:
        return (float(a), float(b))
    except ValueError:
        return (None, None)


def _either(own, other, pat):
    """A site's line off its OWN endpoint log first, the other second."""
    ln = last_with(own, pat)
    return ln if ln is not None else last_with(other, pat)


def parse(cell, arm, seed, rep, cli, srv):
    """The row. `cli` / `srv` are the SENDER's and RECEIVER's log lines."""
    row = {
        "cell": cell,
        "arm": arm,
        "seed": int(seed),
        "rep": int(rep),
    }

    # -- 5. GOODPUT: THE GUARD (read first only because it decides ABORT) --
    # Same abort rule every parser in this tree uses: NO summary at all means
    # the invocation died before the engine started, which is an ABORT and
    # not a DNF. Conflating them reports a dnf count that is really a harness
    # failure. `perf.rs:284-300` prints TWO summary shapes: an acked object
    # carries `"mbps"`, a DNF carries `"dnf": true` and NO `mbps` -- so a DNF
    # row is recognised by its own key, or every DNF would read as ABORT.
    runs, dnf = [], False
    for ln in cli:
        i = ln.find("{")
        if i < 0:
            continue
        try:
            o = json.loads(ln[i:])
        except ValueError:
            continue
        if not isinstance(o, dict):
            continue
        if o.get("dnf"):
            dnf = True
            runs.append(o)
        elif "mbps" in o:
            runs.append(o)
    if not runs:
        row["abort"] = True
        return row
    acked = [o for o in runs if "mbps" in o]
    if acked:
        best = max(acked, key=lambda o: o.get("mbps", 0.0))
        row["mbps"] = round(float(best.get("mbps", 0.0)), 3)
        row["seconds"] = round(float(best.get("seconds", 0.0)), 3)
    else:
        row["mbps"] = None
        row["seconds"] = None
    row["dnf"] = dnf
    row["runs_n"] = len(runs)
    row["acked_n"] = len(acked)

    # -- THE CADENCE COUNTS AND THE EXIT-FLUSH WITNESS ---------------------
    # `*_lines` are CADENCE ticks (the flush is skipped); `recv_final` says
    # whether the receiver's block was exit-flushed at all, so a report can
    # tell "complete counts" from "last cadence tick before SIGKILL".
    row["lat_lines"] = count_kind(srv, "[LAT] site=receiver")
    row["succ_lines"] = count_kind(srv, "[SUCC]")
    row["eta_recv_lines"] = count_kind(srv, "[ETA] site=receiver")
    row["eta_sender_lines"] = count_kind(cli, "[ETA] site=sender")

    # -- 1. [LAT]: THE DECOMPOSITION, AND THE PRE-REGISTERED FIRST READING --
    lat = _either(srv, cli, "[LAT] site=receiver")
    row["lat_present"] = lat is not None
    if lat:
        row["lat_final"] = is_final(lat)
        row["lat_n"] = num(lat, "n=")
        row["lat_over"] = num(lat, "over=")
        # Shares are per-path; the battery reads the POOLED share, weighted by
        # each path's own total wait, because a per-path mean of shares would
        # let a path that delivered ten symbols outvote one that delivered a
        # million.
        sums = {k: 0.0 for k in ("ax", "rwxp", "rwsp", "rwrep", "rep")}
        tot = 0.0
        per_path = []
        for sl in slots(lat):
            s = {k: num(sl, f"{k}_sum=") or 0.0 for k in sums}
            w = sum(s.values())
            tot += w
            for k in sums:
                sums[k] += s[k]
            per_path.append(
                {
                    "path": slot_id(sl),
                    "n": num(sl, "n="),
                    "nowait": num(sl, "nowait="),
                    "minrst": num(sl, "minrst="),
                    "ax_p95": num(sl, "ax_p95="),
                    "ax_p99": num(sl, "ax_p99="),
                    "rwxp_p95": num(sl, "rwxp_p95="),
                    "rwxp_n": num(sl, "rwxp_n="),
                    "rwsp_p95": num(sl, "rwsp_p95="),
                    "rwrep_p95": num(sl, "rwrep_p95="),
                    "rep_p95": num(sl, "rep_p95="),
                    "tot_p50": num(sl, "tot_p50="),
                    "tot_p95": num(sl, "tot_p95="),
                    "tot_p99": num(sl, "tot_p99="),
                    "sh_ax": num(sl, "sh_ax="),
                    "sh_rwxp": num(sl, "sh_rwxp="),
                    "sh_rwsp": num(sl, "sh_rwsp="),
                    "sh_rwrep": num(sl, "sh_rwrep="),
                    "sh_rep": num(sl, "sh_rep="),
                }
            )
        row["lat_paths"] = per_path
        if tot > 0:
            row["sh_ax"] = round(sums["ax"] / tot, 4)
            row["sh_xp"] = round(sums["rwxp"] / tot, 4)
            row["sh_sp"] = round(sums["rwsp"] / tot, 4)
            row["sh_rep"] = round((sums["rwrep"] + sums["rep"]) / tot, 4)
            # THE PRE-REGISTERED READING. Emitted on every row; it is only
            # BINDING on the CTL arm, and the report reads it there first.
            if row["sh_xp"] >= 0.5:
                row["lat_reading"] = "PLACEMENT-INDICTED"
            elif row["sh_ax"] >= 0.5 and row["sh_xp"] <= 0.2:
                row["lat_reading"] = "QUEUE-DOMINATED"
            elif row["sh_rep"] >= 0.5:
                row["lat_reading"] = "REPAIR-DOMINATED"
            else:
                row["lat_reading"] = "MIXED"
        # The p95 the arms are scored on, worst leg -- a multipath latency
        # claim is about the leg that hurts, not about the average of the legs.
        p95s = [p["rwxp_p95"] for p in per_path if p["rwxp_p95"] is not None]
        ax95 = [p["ax_p95"] for p in per_path if p["ax_p95"] is not None]
        t99 = [p["tot_p99"] for p in per_path if p["tot_p99"] is not None]
        row["rwxp_p95_worst"] = max(p95s) if p95s else None
        row["ax_p95_worst"] = max(ax95) if ax95 else None
        row["tot_p99_worst"] = max(t99) if t99 else None
        # THE c1 INSTRUMENT CONTROL on the [LAT] side (pre-registration 2):
        # `rw_xp == 0` at N = 1. Carried as a count so a report can void.
        xp_n = [p["rwxp_n"] for p in per_path if p["rwxp_n"] is not None]
        row["lat_rwxp_n"] = sum(xp_n) if xp_n else None

    # -- 2. [ETA]: THE S4 SCORE, OFFLINE AND BEFORE ANY ARM VERDICT ---------
    es = _either(cli, srv, "[ETA] site=sender")
    er = _either(srv, cli, "[ETA] site=receiver")
    row["eta_present"] = es is not None
    row["eta_recv_present"] = er is not None
    tau_ref_us = None
    if es:
        row["eta_final"] = is_final(es)
        row["fhat_us"] = num(es, "fhat_us=")
        row["eta_stamped"] = num(es, "n=")
        row["eta_zero"] = num(es, "zero=")
        row["place_n"] = num(es, "place_n=")
        row["cold_r"] = num(es, "cold_r=")
        row["cold_ge"] = num(es, "cold_ge=")
        # 3. THE ARM BIND GAUGES.
        row["t_eff"] = num(es, "t_eff=")
        row["t_cold"] = num(es, "t_cold=")
        row["t_n"] = num(es, "t_n=")
        row["hol_sh"] = num(es, "hol_sh=")
        row["hol_n"] = num(es, "hol_n=")
        row["hol_mv"] = num(es, "hol_mv=")
        row["hol_calls"] = num(es, "hol_calls=")
        row["hol_w"] = num(es, "hol_w=")
        # THE EXECUTION WITNESS, promoted to its own boolean so a report can
        # refuse to score an arm that never decided anything.
        if row["hol_calls"]:
            row["hol_executed"] = bool(row["hol_mv"] and row["hol_mv"] > 0.0)
        sig_s = []
        eta_paths = []
        for sl in slots(es):
            v, n = sigma_pair(sl)
            if v is not None:
                sig_s.append(v)
            matched, stamped = ratio_pair(sl, "n=")
            eta_paths.append(
                {
                    "path": slot_id(sl),
                    "matched": matched,
                    "stamped": stamped,
                    "drop": num(sl, "drop="),
                    "tau_us": num(sl, "tau_us="),
                    "e_p50": num(sl, "e_p50="),
                    "e_p99": num(sl, "e_p99="),
                    "late": num(sl, "late="),
                    "sig_us": v,
                    "pairs": n,
                }
            )
        row["eta_paths"] = eta_paths
        taus = [p["tau_us"] for p in eta_paths if p["tau_us"]]
        tau_ref_us = min(taus) if taus else None
        # TWO poolings, because they answer two questions. The RMS over the
        # candidate set is the one 16.81.1's variance match uses and
        # therefore the one the S4 score is computed on; the MAX is the
        # witness's left-hand side (16.81.6), where the claim is about the
        # worst leg.
        row["sig_sender_us"] = max(sig_s) if sig_s else None
        row["sig_sender_rms_us"] = (
            round((sum(v * v for v in sig_s) / len(sig_s)) ** 0.5, 1) if sig_s else None
        )
        # THE S4 SCORE WITHOUT A SECOND INSTRUMENT. When the T-sigma arm ran,
        # the engine already divided the pooled sigma by its OWN `ref` -- the
        # exact quantity the placement law de-dimensionalises by -- so
        # inverting `T = (sqrt6/pi)*sigma/ref` recovers `sigma/ref` with no
        # [DIAG] parse and no possibility of the two `ref`s disagreeing.
        if row.get("t_eff") is not None and row.get("t_n"):
            row["s4_from_teff"] = round(row["t_eff"] / SQRT6_OVER_PI, 5)
    if er:
        row["eta_recv_final"] = is_final(er)
        row["eta_recv_n"] = num(er, "n=")
        sig_r = [v for (v, n) in (sigma_pair(sl) for sl in slots(er)) if v is not None]
        row["sig_recv_us"] = max(sig_r) if sig_r else None
        # `bind=` is the receiver line's own coverage gauge (the fraction of
        # arrivals carrying the `eta_rel = 0` sentinel); `eta_s4.py` trips
        # UNREADABLE at >= 0.5, so it is carried here as the worst path.
        binds = [num(sl, "bind=") for sl in slots(er)]
        binds = [b for b in binds if b is not None]
        row["eta_bind_max"] = max(binds) if binds else None
        row["lat_p95_recv"] = None
        ls = [num(sl, "l_p95=") for sl in slots(er)]
        ls = [x for x in ls if x is not None]
        if ls:
            row["lat_p95_recv"] = max(ls)

    # `ref` for the S4 score is the FASTEST path's SRTT -- the same quantity
    # the placement law de-dimensionalises by, read off the sender's own
    # [DIAG]. The sender `[ETA]` line's own `tau_us` is the CLEAN reference
    # (`eta_s4.py`: the dispersion was already measured against it) and is
    # carried beside it so the two can be held against each other.
    dg = last_with(cli, "[DIAG]")
    ref_us = None
    if dg:
        rtts = [float(m) for m in re.findall(r"\brtt=(\d+(?:\.\d+)?)", dg)]
        if rtts:
            ref_us = min(rtts) * 1000.0  # [DIAG] prints ms
    row["ref_us"] = ref_us
    row["tau_ref_us"] = tau_ref_us
    for who, key in (("sender", "sig_sender_rms_us"), ("recv", "sig_recv_us")):
        s = row.get(key)
        if s is not None and ref_us:
            row[f"sig_ref_{who}"] = round(s / ref_us, 5)
    if row.get("sig_sender_rms_us") is not None and tau_ref_us:
        row["sig_ref_sender_tau"] = round(row["sig_sender_rms_us"] / tau_ref_us, 5)
    # THE S4 VERDICT, per row. The shipped 0.15 ASSERTS sigma_e/ref = 0.19238
    # at every cell; a 20 % band is the pre-registered "confirms" window. The
    # engine's own `t_eff` inversion is preferred over the [DIAG]-derived one
    # when it exists, because the two cannot then disagree about `ref`; the
    # `tau_us` route is next for the same reason.
    s4 = row.get("s4_from_teff", row.get("sig_ref_sender_tau", row.get("sig_ref_sender")))
    if s4 is not None:
        row["s4_shipped_claim"] = SHIPPED_SIGMA_OVER_REF
        row["s4_sigma_over_ref"] = s4
        row["s4_ratio"] = round(s4 / SHIPPED_SIGMA_OVER_REF, 4)
        row["s4_confirms_0p15"] = abs(s4 - SHIPPED_SIGMA_OVER_REF) <= 0.2 * SHIPPED_SIGMA_OVER_REF
    # The pre-stated witness of 16.81.6, REPORTED and not asserted: the
    # sender estimates over its whole candidate set, the receiver only over
    # symbols the placement itself selected, and selection on a cost
    # containing the prediction can only narrow the realized spread.
    if row.get("sig_sender_us") is not None and row.get("sig_recv_us") is not None:
        row["witness_sender_ge_recv"] = row["sig_sender_us"] >= row["sig_recv_us"]

    # -- 4. [SUCC]: THE CROSS-PATH RESOLUTION FRACTION ----------------------
    sc = _either(srv, cli, "[SUCC]")
    row["succ_present"] = sc is not None
    if sc:
        row["succ_final"] = is_final(sc)
        row["succ_det"] = num(sc, "det=")
        row["succ_xp_n"] = num(sc, "xp_n=")
        row["succ_sp_n"] = num(sc, "sp_n=")
        row["succ_xp_frac"] = num(sc, "xp_frac=")
        if row["succ_det"]:
            row["xp_over_det"] = round((row["succ_xp_n"] or 0.0) / row["succ_det"], 4)
        # THE C1 CONTROL, ASSERTED IN THE ROW. N = 1 collapses the softmax to
        # an identity, so a nonzero cross-path resolution at a single-path
        # cell is not a small effect -- it means the run is not what it says
        # it is. The `[LAT]` side of the same control (`rw_xp`) voids too.
        if cell in ("c1", "sc2", "sc3") and (
            (row["succ_xp_n"] or 0) > 0 or (row.get("lat_rwxp_n") or 0) > 0
        ):
            row["control_violated"] = True

    # The receiver's block was exit-flushed iff its gauges say so.
    row["recv_final"] = bool(
        row.get("lat_final") or row.get("succ_final") or row.get("eta_recv_final")
    )
    return row


def main(argv):
    cell, arm, seed, rep, clog, slog = argv[:6]
    row = parse(cell, arm, seed, rep, read(clog), read(slog))
    print("PLACERESULT " + json.dumps(row))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
