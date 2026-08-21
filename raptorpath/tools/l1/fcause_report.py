#!/usr/bin/env python3
"""Report for the FIRE-CAUSE DIAGNOSTIC PASS (goal-gate "Fire-Cause").

    fcause_report.py <log> [<log> ...]

Scrapes `FCWITNESS ` rows and scores them against the PRE-REGISTERED readings
in the goal-gate section "FIRE-CAUSE PASS - PRE-REGISTRATION", never against a
number chosen afterwards. Sections:

  1 LIVENESS + ABORT/VOID          (abort-cause first: an aborted or void row
    is excluded from every denominator and SAID SO, rather than silently
    contributing a timer_frac of zero)
  2 READING (i)  THE CAUSE DISTRIBUTION, per cell per arm
  3 READING (ii) DOES ARMING THE CLOCK MOVE THE MIX?
  4 THE MEASURAND VERDICT - the pre-stated decision rule, applied
  5 THE CONSTRAINT - goodput bands, OFF arm only
"""
import json
import statistics as st
import sys
from collections import defaultdict

CELLS = ["c1", "sc2", "c7", "c8", "c8L"]
ARMS = ["OFF", "Q009"]

# ── THE PRE-REGISTERED DECISION THRESHOLD ────────────────────────────────
# Stated in the pre-registration BEFORE the pass ran. A minority is < 0.5 of
# the classified fires; DOMINATES is > 0.5. The band between the two readings
# does not exist -- they partition -- so the rule is total by construction and
# no cell can land outside it.
MINORITY = 0.5

rows = []
for p in sys.argv[1:]:
    for ln in open(p, errors="replace"):
        i = ln.find("FCWITNESS ")
        if i < 0:
            continue
        try:
            rows.append(json.loads(ln[i + 10:]))
        except Exception:
            pass

if not rows:
    print("NO FCWITNESS ROWS -- nothing to score.")
    sys.exit(0)


def void_cause(r):
    """Why this row carries no cause mix, or None if it does. ABORT-CAUSE
    FIRST: the reason is named, and a void row is never a measured zero."""
    if r["n"] == 0:
        return "W1 n=0 (no [FCAUSE] / the gap loop never fired)"
    if r["W3_gen"] != "0":
        return f"W3 gen={r['W3_gen']} (not plain window)"
    if r["other"] != 0:
        return f"W2 other={r['other']} (unclassified fires)"
    if r["n"] < r["W4_retx_max"]:
        return f"W4 n={r['n']} < retx={r['W4_retx_max']} (undercount)"
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
        vc[f"{r['cell']}-{r['arm']}: {c}"] += 1
    for k in sorted(vc):
        print(f"  VOID x{vc[k]}  {k}")
else:
    print("  no void rows -- every invocation produced a classified cause mix")
# The arm-liveness witness: the clock must be ARMED at the LAW on Q009.
bad = [r for r in live if r["arm"] == "Q009" and r["w6_winn_cli"] != "1112"]
print(f"  W6 Q009 sender win_n=1112 at the law: {len(live and [r for r in live if r['arm']=='Q009'])-len(bad)}"
      f"/{len([r for r in live if r['arm']=='Q009'])} rows"
      + ("" if not bad else "   <-- ARM DID NOT TAKE ON SOME ROWS"))
bad_off = [r for r in live if r["arm"] == "OFF" and r["w6_form_cli"] != "cantelli"]
print(f"  W6 OFF sender form=cantelli (the SHIPPED clamp): "
      f"{len([r for r in live if r['arm']=='OFF'])-len(bad_off)}"
      f"/{len([r for r in live if r['arm']=='OFF'])} rows")

g = defaultdict(list)
for r in live:
    g[(r["cell"], r["arm"])].append(r)


def pooled(rs, key):
    """Pool a cause across reps on the POOLED COUNTS, not on the mean of the
    per-rep fractions: a rep that fired ten times must not weigh the same as
    one that fired ten thousand."""
    return sum(x[key] for x in rs)


print()
print("=" * 74)
print("2  READING (i) -- THE CAUSE DISTRIBUTION, per cell per arm")
print("=" * 74)
print(f"{'cell':>5} {'arm':>5} {'n':>3} {'fires':>9} {'timer':>8} {'gap_data':>9}"
      f" {'gap_ref':>8} {'timer_f':>8} {'gap_f':>7} {'unattr':>7}")
dist = {}
for c in CELLS:
    for a in ARMS:
        rs = g.get((c, a), [])
        if not rs:
            continue
        N = pooled(rs, "n")
        t, gd, gr = pooled(rs, "timer"), pooled(rs, "gap_data"), pooled(rs, "gap_refresh")
        ua = pooled(rs, "unattr")
        tf, gf = t / N, (gd + gr) / N
        dist[(c, a)] = (N, t, gd, gr, tf, gf)
        print(f"{c:>5} {a:>5} {len(rs):>3} {N:>9} {t:>8} {gd:>9} {gr:>8}"
              f" {tf:>8.4f} {gf:>7.4f} {ua:>7}")

print()
print("=" * 74)
print("3  READING (ii) -- DOES ARMING THE CLOCK MOVE THE MIX?")
print("=" * 74)
print("If timer_frac is materially the same OFF and armed, the timer's")
print("irrelevance is not an artifact of the shipped clamp.")
print(f"{'cell':>5} {'timer_f OFF':>13} {'timer_f Q009':>14} {'delta':>9}")
deltas = []
for c in CELLS:
    if (c, "OFF") in dist and (c, "Q009") in dist:
        a, b = dist[(c, "OFF")][4], dist[(c, "Q009")][4]
        deltas.append(b - a)
        print(f"{c:>5} {a:>13.4f} {b:>14.4f} {b-a:>+9.4f}")
if deltas:
    print(f"  max |delta timer_frac| across cells = {max(abs(d) for d in deltas):.4f}")

print()
print("=" * 74)
print("4  THE MEASURAND VERDICT -- the pre-stated decision rule, applied")
print("=" * 74)
print(f"Rule (pre-registered): timer_frac < {MINORITY} at a cell means the")
print("recovery fires at that cell are NOT timer-driven, so fa _|_ W is")
print("EXPLAINED and 16.69's measurand (the ack-arrival distribution) is")
print("refuted as the quantity to position a waiting time on. timer_frac >")
print(f"{MINORITY} means the timer DOMINATES and the sweep's F2 is UNEXPLAINED --")
print("which is itself the finding, and names a contradiction rather than a")
print("successor.")
print()
minority, dominant = [], []
for (c, a), (N, t, gd, gr, tf, gf) in sorted(dist.items()):
    (minority if tf < MINORITY else dominant).append((c, a, tf))
for c, a, tf in minority:
    print(f"  MINORITY  {c}-{a}: timer_frac={tf:.4f} -- fires are NOT timer-driven")
for c, a, tf in dominant:
    print(f"  DOMINATES {c}-{a}: timer_frac={tf:.4f} -- the sweep's F2 is UNEXPLAINED")
print()
if dist and not dominant:
    # THE SUCCESSOR MEASURAND, named FROM THE DATA per the pre-registration:
    # whichever gap arm carries the majority names the quantity.
    GD = sum(v[2] for v in dist.values())
    GR = sum(v[3] for v in dist.values())
    T = sum(v[1] for v in dist.values())
    NN = sum(v[0] for v in dist.values())
    print(f"POOLED: n={NN} timer={T} ({T/NN:.4f}) gap_data={GD} ({GD/NN:.4f}) "
          f"gap_refresh={GR} ({GR/NN:.4f})")
    print()
    print("VERDICT: the recovery fires are NOT timer-driven. fa _|_ W is")
    print("explained, and 16.69's measurand is refuted with a count.")
    print()
    print("THE SUCCESSOR MEASURAND, read off the majority cause (pre-stated):")
    if GD >= GR:
        print("  MAJORITY = gap_data -- fires driven by the receiver's DATA-arm")
        print("  SACK report (the dupack analog). A gap report is emitted when a")
        print("  HIGHER seq arrives while a hole is outstanding, so the waiting")
        print("  time must be positioned on the SAME-FLOW SUCCESSOR-ARRIVAL")
        print("  distribution: P(the next in-flight symbol for this flow arrives")
        print("  by t | a hole is outstanding). NOT the ack-arrival distribution")
        print("  -- the sender's clock never gets to decide these fires at all;")
        print("  the receiver's gap-report cadence does.")
    else:
        print("  MAJORITY = gap_refresh -- fires driven by the RECEIVER's own")
        print("  hole-refresh timer (hole_refresh_all). The waiting time must be")
        print("  positioned on the HOLE-RESIDENCY distribution at the RECEIVER:")
        print("  P(a hole is closed by t | it was advertised), which is the")
        print("  receiver's clock, not the sender's.")
elif dominant:
    print("VERDICT: the timer DOMINATES at the cells listed above, and the")
    print("sweep's F2 (fa flat across a 200x alpha span) is therefore")
    print("UNEXPLAINED. That contradiction IS the finding. Resolving it needs")
    print("a reading this pass does not take: the DISTRIBUTION of the fired")
    print("deadlines themselves, to show whether the realized inter-fire")
    print("interval tracked W or was pinned by a bound above it (the budget")
    print("gate cached_nack_budget, or the per-seq cooldown).")

print()
print("=" * 74)
print("5  THE CONSTRAINT -- goodput bands (OFF arm only; Q009 is a RESULT)")
print("=" * 74)
for c in CELLS:
    for a in ARMS:
        rs = g.get((c, a), [])
        if not rs:
            continue
        mb = [r["mbps"] for r in rs]
        lo, hi = rs[0]["band"]
        scope = "BAND" if rs[0]["band_applies"] else "result"
        oob = sum(1 for r in rs if not r["in_band"])
        med = st.median(mb) if mb else 0.0
        print(f"{c:>5} {a:>5}  median={med:>8.1f} Mbit/s  band=[{lo},{hi}]"
              f"  {scope}  out_of_band={oob}/{len(rs)}")
