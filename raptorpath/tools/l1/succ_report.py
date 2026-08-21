#!/usr/bin/env python3
"""Report for the SUCCESSOR-ARRIVAL PASS (goal-gate "Successor Arrival").

    succ_report.py <log> [<log> ...]

Scrapes `SUWITNESS ` rows and scores them against the PRE-REGISTERED readings
and bars in the goal-gate section "THE SUCCESSOR-ARRIVAL PASS -
PRE-REGISTRATION", never against a number chosen afterwards. Sections:

  1 LIVENESS + ABORT/VOID       (abort-cause first: a void row is excluded
    from every denominator and SAID SO, rather than silently contributing a
    quantile of zero)
  2 READING (i)   THE DISTRIBUTION, per cell, per outcome
  3 READING (ii)  ORIGINAL vs REPAIR, and THE CROSSING POINT
  4 READING (iii) DIAL-DEPENDENCE - does the distribution move with the cell?
  5 READING (iv)  REP-TO-REP DISPERSION - the stability the next derivation
    is entitled to know about BEFORE it is built on this estimate
  6 THE CONSTRAINT - goodput bands (one shipped arm: every row)
  7 THE HANDOFF - exactly what the formula-first derivation gets

POOLING, STATED RATHER THAN ASSUMED. The gauge emits SUMMARIES, not per-bucket
counts, so a count-pooled quantile across reps is NOT AVAILABLE and this report
does not manufacture one. Counts (det, res, orig_n, rep_n, aban_n, open, over)
are SUMMED across reps. Quantiles are reported as the MEDIAN ACROSS REPS of the
per-rep value, ALWAYS beside their own rep-to-rep min and max, so no central
value is ever readable without its own dispersion. That is the sigma saga's
lesson applied at the reporting layer.
"""
import json
import statistics as st
import sys
from collections import defaultdict

CELLS = ["c1", "sc2", "c7", "c8", "c8L"]
OUTCOMES = ["orig", "rep", "aban"]

# ── THE PRE-REGISTERED BARS ──────────────────────────────────────────────
# Every one of these is stated in the pre-registration BEFORE the pass ran.
# A reading below its bar is UNSCOREABLE-thin -- a LEGAL OUTCOME, reported as
# such, never rounded into a number.
THIN_P50 = 100   # samples for a median to be scoreable
THIN_P90 = 100   # ... a p90
THIN_P99 = 300   # ... a p99 (below this a p99 is one or two samples)
THIN_FRAC = 100  # resolved holes for orig_frac to be scoreable
THIN_CROSS = 100 # EACH of orig_n and rep_n for a crossing point to be scoreable

# A quantile whose rep-to-rep max/min exceeds this is UNSTABLE, and the handoff
# must say so. 2.0 = "the estimate moved by more than a factor of two between
# identical invocations".
STABILITY_BAR = 2.0

# The cell medians' max/min above this ⇒ DIAL-DEPENDENT: the formula the next
# step derives must CARRY the dial. Below it ⇒ DIAL-FLAT and a constant is
# licensed. Stated at 2.0 for the same reason the stability bar is.
DIAL_BAR = 2.0

DASH = "-"


def num(v):
    """A `-`-or-number slot. `-` is the ABSENT reading and is never 0."""
    if v is None or v == DASH or v == "":
        return None
    try:
        return float(v)
    except (TypeError, ValueError):
        return None


rows = []
for p in sys.argv[1:]:
    for ln in open(p, errors="replace"):
        i = ln.find("SUWITNESS ")
        if i < 0:
            continue
        try:
            rows.append(json.loads(ln[i + 10:]))
        except Exception:
            pass

if not rows:
    print("NO SUWITNESS ROWS -- nothing to score.")
    sys.exit(0)


def void_cause(r):
    """Why this row carries no distribution, or None if it does. ABORT-CAUSE
    FIRST: the reason is NAMED, and a void row is never a measured zero."""
    if r["det"] == 0:
        return "W1 det=0 (no [SUCC] at the receiver / no hole detected)"
    if r["gen"] != "0":
        return f"W3 gen={r['gen']} (not plain window -- orig is structurally empty)"
    s = r["orig_n"] + r["rep_n"] + r["aban_n"] + r["open"] + r["over"]
    if r["det"] != s:
        return f"W2 det={r['det']} != sum({s}) (the outcomes do not partition the holes)"
    return None


live = [r for r in rows if void_cause(r) is None]
void = [(r, void_cause(r)) for r in rows if void_cause(r) is not None]

print("=" * 74)
print("1  LIVENESS + ABORT/VOID")
print("=" * 74)
print(f"rows={len(rows)}  live={len(live)}  void={len(void)}")
if void:
    vc = defaultdict(int)
    for r, c in void:
        vc[f"{r['cell']}: {c}"] += 1
    for k in sorted(vc):
        print(f"  VOID x{vc[k]}  {k}")
else:
    print("  no void rows -- every invocation produced a distribution")

# W4: a DECLARED RESOURCE BOUND that bound is a truncation, reported.
ov = [r for r in live if r["over"] != 0]
print(f"  W4 over=0 (no declared bound bound): {len(live)-len(ov)}/{len(live)}")
for r in ov:
    print(f"    BOUND-BOUND {r['cell']} rep={r['rep']} over={r['over']}/{r['det']}")

# W5: THE ROUTING GATE -- prove the mechanism under test executed. Three
# counters over the same loss, two of them NOT this gauge's.
print("  W5 routing gate (RFA fires>0 AND FCAUSE gap_data>0 at lossy cells):")
for r in live:
    if not r["lossy"]:
        continue
    ok = r["W5_rfa_fires"] > 0 and r["W5_fc_gap_data"] > 0
    if not ok:
        print(f"    ROUTING-FAIL {r['cell']} rep={r['rep']} "
              f"rfa_fires={r['W5_rfa_fires']} fc_gap_data={r['W5_fc_gap_data']}")
lossy_live = [r for r in live if r["lossy"]]
routed = [r for r in lossy_live if r["W5_rfa_fires"] > 0 and r["W5_fc_gap_data"] > 0]
print(f"    {len(routed)}/{len(lossy_live)} lossy rows routed")
if lossy_live:
    tot_gd = sum(r["W5_fc_gap_data"] for r in lossy_live)
    tot_n = sum(r["W5_fc_n"] for r in lossy_live)
    print(f"    pooled FCAUSE gap_data={tot_gd}/{tot_n} "
          f"({tot_gd/tot_n:.4f} of fires)" if tot_n else "    pooled FCAUSE n=0")

by_cell = defaultdict(list)
for r in live:
    by_cell[r["cell"]].append(r)


def reps(cell, key):
    """Per-rep values of a `-`-or-number slot at one cell, absent ones dropped."""
    return [num(r[key]) for r in by_cell[cell] if num(r[key]) is not None]


def med_min_max(vals):
    if not vals:
        return None
    return (st.median(vals), min(vals), max(vals))


def cross_scoreable(cell):
    """The crossing point's pre-stated bar, in ONE place so sections 3, 4 and 5
    cannot drift apart: EACH of orig_n and rep_n at or above THIN_CROSS. A
    crossing between a populated distribution and an empty one is not a
    crossing."""
    on = sum(r["orig_n"] for r in by_cell[cell])
    rn = sum(r["rep_n"] for r in by_cell[cell])
    return (on >= THIN_CROSS and rn >= THIN_CROSS), on, rn


def fmt_q(cell, key, n_sum, bar):
    """One quantile cell: median across reps, its own min/max beside it, and
    UNSCOREABLE-thin when the pooled sample count is below its pre-stated bar.
    n == 0 is ABSENT (`-`), not thin: nothing was measured, so there is no
    small sample to be cautious about."""
    if n_sum == 0:
        return DASH
    if n_sum < bar:
        return f"THIN(n={n_sum})"
    v = med_min_max(reps(cell, key))
    if v is None:
        return DASH
    m, lo, hi = v
    return f"{m/1000:.1f}ms [{lo/1000:.1f}-{hi/1000:.1f}]"


print()
print("=" * 74)
print("2  READING (i)  THE DISTRIBUTION, per cell, per outcome")
print("=" * 74)
print("   quantiles: MEDIAN ACROSS REPS [min-max across reps], ms.")
print("   counts: SUMMED across reps.  THIN(n) = below the pre-stated bar.")
for oc in OUTCOMES:
    print()
    print(f"  -- outcome `{oc}` --")
    print(f"  {'cell':>5} {'n':>8} {'p50':>22} {'p90':>22} {'p99':>22} {'mx':>12}")
    for c in CELLS:
        if c not in by_cell:
            continue
        n_sum = sum(r[f"{oc}_n"] for r in by_cell[c])
        mx = med_min_max(reps(c, f"{oc}_mx"))
        print(f"  {c:>5} {n_sum:>8} "
              f"{fmt_q(c, f'{oc}_p50', n_sum, THIN_P50):>22} "
              f"{fmt_q(c, f'{oc}_p90', n_sum, THIN_P90):>22} "
              f"{fmt_q(c, f'{oc}_p99', n_sum, THIN_P99):>22} "
              f"{(f'{mx[0]/1000:.1f}ms' if mx else DASH):>12}")

print()
print("  CENSUS (not outcomes): det = orig + rep + aban + open + over")
print(f"  {'cell':>5} {'det':>9} {'res':>9} {'open':>8} {'over':>6} {'aban':>6}")
for c in CELLS:
    if c not in by_cell:
        continue
    s = lambda k: sum(r[k] for r in by_cell[c])
    print(f"  {c:>5} {s('det'):>9} {s('res'):>9} {s('open'):>8} "
          f"{s('over'):>6} {s('aban_n'):>6}")

print()
print("=" * 74)
print("3  READING (ii)  ORIGINAL vs REPAIR, and THE CROSSING POINT")
print("=" * 74)
print("  orig_frac = of the holes that RESOLVED, the fraction the ORIGINAL closed.")
print("  cross_us  = the smallest t at which more holes have closed by REPAIR")
print("              within t than by their ORIGINAL within t. `-` is LEGAL and")
print("              reads 'the original leads at every horizon'.")
print(f"  {'cell':>5} {'res':>8} {'orig_frac':>26} {'cross':>26}")
for c in CELLS:
    if c not in by_cell:
        continue
    res_sum = sum(r["res"] for r in by_cell[c])
    ok_cross, on, rn = cross_scoreable(c)
    if res_sum < THIN_FRAC:
        of = f"THIN(n={res_sum})"
    else:
        v = med_min_max(reps(c, "orig_frac"))
        of = f"{v[0]:.4f} [{v[1]:.4f}-{v[2]:.4f}]" if v else DASH
    if not ok_cross:
        cr = f"THIN(o={on},r={rn})"
    else:
        vals = reps(c, "cross_us")
        nabs = len(by_cell[c]) - len(vals)
        if not vals:
            cr = f"- x{nabs} (original leads at every horizon)"
        else:
            v = med_min_max(vals)
            cr = f"{v[0]/1000:.1f}ms [{v[1]/1000:.1f}-{v[2]/1000:.1f}]"
            if nabs:
                cr += f" +{nabs}x-"
    print(f"  {c:>5} {res_sum:>8} {of:>26} {cr:>26}")

print()
print("=" * 74)
print("4  READING (iii)  DIAL-DEPENDENCE -- must the formula CARRY the dial?")
print("=" * 74)
print(f"  PRE-STATED RULE: cell-median max/min > {DIAL_BAR} => DIAL-DEPENDENT")
print("  (the derived waiting time must be a function of the cell's condition),")
print("  otherwise DIAL-FLAT (a constant is licensed by this pass).")
for key, bar_n, label in [
    ("orig_p50", THIN_P50, "orig p50"),
    ("rep_p50", THIN_P50, "rep p50"),
    ("cross_us", THIN_CROSS, "cross"),
]:
    meds = {}
    for c in CELLS:
        if c not in by_cell:
            continue
        if key == "cross_us":
            if not cross_scoreable(c)[0]:
                continue
        else:
            base = "orig_n" if key.startswith("orig") else "rep_n"
            if sum(r[base] for r in by_cell[c]) < bar_n:
                continue
        v = med_min_max(reps(c, key))
        if v:
            meds[c] = v[0]
    if len(meds) < 2:
        print(f"  {label:>10}: UNSCOREABLE -- fewer than 2 scoreable cells {list(meds)}")
        continue
    lo, hi = min(meds.values()), max(meds.values())
    ratio = hi / lo if lo > 0 else float("inf")
    verdict = "DIAL-DEPENDENT" if ratio > DIAL_BAR else "DIAL-FLAT"
    cells = " ".join(f"{c}={meds[c]/1000:.1f}" for c in CELLS if c in meds)
    print(f"  {label:>10}: {verdict}  max/min={ratio:.2f}  ({cells} ms)")

print()
print("=" * 74)
print("5  READING (iv)  REP-TO-REP DISPERSION -- IS THIS ESTIMATE STABLE?")
print("=" * 74)
print(f"  PRE-STATED BAR: max/min across reps > {STABILITY_BAR} => UNSTABLE.")
print("  THE SIGMA SAGA'S LESSON, APPLIED BEFORE THE DERIVATION AND NOT AFTER:")
print("  a quantile whose central value is printed without its dispersion is an")
print("  estimate nobody can tell apart from a rep.")
print(f"  {'cell':>5} {'slot':>10} {'reps':>5} {'max/min':>9}  verdict")
unstable = []
for c in CELLS:
    if c not in by_cell:
        continue
    for key, base, bar in [
        ("orig_p50", "orig_n", THIN_P50),
        ("orig_p90", "orig_n", THIN_P90),
        ("orig_p99", "orig_n", THIN_P99),
        ("rep_p50", "rep_n", THIN_P50),
        ("rep_p90", "rep_n", THIN_P90),
        ("cross_us", "res", THIN_CROSS),
        ("orig_frac", "res", THIN_FRAC),
    ]:
        if key == "cross_us":
            if not cross_scoreable(c)[0]:
                continue
        elif sum(r[base] for r in by_cell[c]) < bar:
            continue
        vals = reps(c, key)
        if len(vals) < 2:
            print(f"  {c:>5} {key:>10} {len(vals):>5} {'-':>9}  UNSCOREABLE-thin (<2 reps)")
            continue
        lo, hi = min(vals), max(vals)
        ratio = hi / lo if lo > 0 else float("inf")
        bad = ratio > STABILITY_BAR
        if bad:
            unstable.append(f"{c}/{key} ({ratio:.2f}x)")
        print(f"  {c:>5} {key:>10} {len(vals):>5} {ratio:>9.2f}  "
              f"{'UNSTABLE' if bad else 'stable'}")
print()
if unstable:
    print(f"  UNSTABLE SLOTS ({len(unstable)}): " + ", ".join(unstable))
    print("  The handoff MUST carry these -- a derivation built on an unstable")
    print("  estimate is the defect this bar exists to catch.")
else:
    print("  NO UNSTABLE SLOT -- every scoreable quantile held within the bar")
    print("  across identical invocations.")

print()
print("=" * 74)
print("6  THE CONSTRAINT -- GOODPUT (one shipped arm: every row in scope)")
print("=" * 74)
for c in CELLS:
    if c not in by_cell:
        continue
    rs = by_cell[c]
    mb = [r["mbps"] for r in rs]
    ib = sum(1 for r in rs if r["in_band"])
    b = rs[0]["band"]
    print(f"  {c:>5} median={st.median(mb):7.1f} Mbit/s  band=[{b[0]},{b[1]}]  "
          f"in-band {ib}/{len(rs)}")
oob = [r for r in live if not r["in_band"]]
if oob:
    print(f"  OUT OF BAND at {len(oob)} rows -- an observation-only gauge that")
    print("  moved the transfer it observed is a DEFECT FINDING, not a footnote.")
else:
    print("  IN BAND AT EVERY ROW -- the pass did not perturb the machine it measured.")

print()
print("=" * 74)
print("7  THE HANDOFF -- what the formula-first derivation gets")
print("=" * 74)
print("  * the MEASURAND: P(successor arrives by t | hole outstanding), timed")
print("    from DETECTION (the gap_data trigger), at the receiver, per hole.")
print("  * its SHAPE: section 2's per-cell quantiles by outcome.")
print("  * its ANCHOR: section 3's crossing point -- the false-repair boundary")
print("    in time -- with its own thin/absent verdicts intact.")
print("  * whether the formula must CARRY THE DIAL: section 4.")
print("  * whether the estimate is STABLE ENOUGH TO BUILD ON: section 5.")
print("  * WHAT REMAINS UNMEASURED, and it is NOT nothing: every duration here")
print("    excludes creation-to-detection (not receiver-observable); `orig`")
print("    cannot separate a late reorder from a retransmit; the `open` census")
print("    is right-censored by the harness SIGKILL; and one seed was run.")
