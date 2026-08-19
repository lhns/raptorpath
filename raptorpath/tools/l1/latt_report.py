#!/usr/bin/env python3
"""THE LATENCY-TRUTH SCORER — goal-gate "Latency Truth — PRE-REGISTRATION",
MEASUREMENT TRUTH item 1.

  usage: latt_report.py <latt-s42.log> [<latt-s7.log> ...]

It scores the battery against the PRE-REGISTERED contract and against nothing
else. The contract's reading rules (i)/(ii)/(iii), its predictions D1-D6 with
their bands, and its guards were committed BEFORE any VM contact; this file
applies them and has no authority to reinterpret them.

WHAT IS NEW HERE, AND IT IS THE POINT OF THE BATTERY. The era battery scored a
delivered-latency claim on ONE leg of a two-leg system, with the probe's loss
never counted and its tail silently censored. This scorer:

  * reads the PER-LEG delivered percentiles, never a single "the" ping column;
  * prints the CENSORING FRACTION beside every percentile, always — there is no
    output mode that omits it;
  * applies the STRUCTURAL PLACEABILITY RULE (a percentile qq inside the
    unobservable top `c` of the distribution cannot be located at all) AND the
    coarse pre-registered CONTRACT BAR (censoring > 20 % kills the leg);
  * refuses to average the two legs, and refuses to average the two
    instruments.

THE ESTIMATORS ARE IMPORTED FROM `era_report.py`, NOT REIMPLEMENTED. `mean`,
`sd`, `pooled2s`, `verdict_two_sided`, `against_interval`, `live`, `void` and
`rows` keep ONE definition across the two batteries, so a row scored here is
scored by the same arithmetic that produced the era verdict this battery
adjudicates. A second dialect would make the comparison meaningless.
"""
import os
import sys
from collections import defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from era_report import (  # noqa: E402  ONE definition of every estimator
    against_interval, live, mean, pooled2s, rows, sd, verdict_two_sided, void,
)
from latt_probe import CONTRACT_BAR  # noqa: E402  ONE definition of the bar

#: The cells, and the two that carry the disagreement. TRANSCRIBED from the
#: contract, never redefined.
CELLS = ["c8", "c8L", "c7"]
LOADED_DUALS = ["c8", "c8L"]
CAPACITY = {"c7": 200.0, "c8": 120.0, "c8L": 120.0}
HEADROOM_BAR = 5.0

#: THE PRE-REGISTERED PREDICTIONS, transcribed from the contract with their
#: bands. `None` where the contract deliberately asserted NO band (D2), which is
#: itself a pre-commitment and is printed as one.
PRED = {
    "D1": ("c8 leg A delivered p50, s42", (3.0, 25.0)),
    "D3": ("c7 delivered p50, both legs", (-12.0, 0.0)),
    "D5_c8": ("c8 q_p50", (-260.0, -140.0)),
    "D5_c8L": ("c8L q_p50", (-400.0, -200.0)),
    "D5_c7": ("c7 q_p50", (-20.0, 2.0)),
}
D4_FLOOR = {("c7", 0): 2.5, ("c7", 1): 2.5, ("c8", 0): 2.5, ("c8", 1): 4.8,
            ("c8L", 0): 2.5, ("c8L", 1): 4.8}
D4_CEIL = {0: 15.0, 1: 20.0}

PCTS = ("p50", "p95")


def hdr(t):
    print("\n" + t)
    print("-" * len(t))


def group(rs, *keys):
    g = defaultdict(list)
    for r in rs:
        g[tuple(r.get(k) for k in keys)].append(r)
    return g


def abort_table(rs):
    """G-ABORT. PRINTED FIRST BY CONSTRUCTION, and read first, because every
    contrast below is conditioned on which invocations survived to be measured.
    """
    hdr("1 — THE ABORT-CAUSE TABLE (G-ABORT), READ BEFORE ANY CONTRAST")
    print(f"{'cell':>4} {'arm':>4} {'seed':>5} {'rows':>5} {'abort':>6} "
          f"{'rate%':>7}  causes")
    tot = ab = 0
    norec = 0
    gaps = {}
    for k in sorted(group(rs, "cell", "arm", "seed")):
        g = group(rs, "cell", "arm", "seed")[k]
        a = [r for r in g if r.get("abort")]
        tot += len(g)
        ab += len(a)
        causes = defaultdict(int)
        for r in a:
            c = r.get("abort_cause") or "no_record"
            causes[c] += 1
            if c == "no_record":
                norec += 1
        rate = 100.0 * len(a) / len(g) if g else 0.0
        gaps[k] = rate
        cs = ", ".join(f"{c}={n}" for c, n in sorted(causes.items())) or "-"
        print(f"{k[0]:>4} {k[1]:>4} {k[2]:>5} {len(g):>5} {len(a):>6} "
              f"{rate:>7.1f}  {cs}")
    print(f"\n  {ab} aborts of {tot} ({100.0*ab/tot if tot else 0:.1f} %) | "
          f"no witness record {norec} (INSTRUMENT-FAIL of the witness)")

    hdr("   G-ABORT's 10-POINT RULE — the OLD/NEW gap per cell and seed")
    fired = []
    for cell in CELLS:
        for seed in sorted({r["seed"] for r in rs}):
            o = gaps.get((cell, "OLD", seed))
            n = gaps.get((cell, "NEW", seed))
            if o is None or n is None:
                continue
            gap = abs(n - o)
            tag = "**FIRES**" if gap > 10.0 else "ok"
            if gap > 10.0:
                fired.append((cell, seed, gap))
            print(f"   {cell:>4} s{seed}: OLD {o:>5.1f}%  NEW {n:>5.1f}%  "
                  f"gap {gap:>5.1f} pts  [{tag}]")
    if not fired:
        print("\n   NO CELL-SEED FIRES. Every contrast below is an unselected "
              "census of its arm's reps.")
    return fired


def liveness(rs):
    hdr("2 — G-LIVE / G-ERA")
    print(f"{'cell':>4} {'arm':>4} {'seed':>5} {'rows':>5} {'live':>5} "
          f"{'abort':>6} {'half':>5} {'void':>5}")
    nv = 0
    for k in sorted(group(rs, "cell", "arm", "seed")):
        g = group(rs, "cell", "arm", "seed")[k]
        lv = [r for r in g if live(r)]
        ab = [r for r in g if r.get("abort")]
        vd = [r for r in g if void(r)]
        half = len(g) - len(lv) - len(ab) - len(vd)
        nv += len(vd)
        print(f"{k[0]:>4} {k[1]:>4} {k[2]:>5} {len(g):>5} {len(lv):>5} "
              f"{len(ab):>6} {half:>5} {len(vd):>5}")
    surprise = sum(1 for r in rs if r.get("era_surprise"))
    print(f"\n  G-ERA: {nv} anti-mix violations | {surprise} ERA-SURPRISE "
          f"(a NEW-only gauge on an OLD row)")
    return nv


def headroom(rs):
    hdr("3 — G-HEAD (discipline 16): util = tc_bytes*8 / (TRANSFER s * capacity)")
    print(f"{'cell':>4} {'arm':>4} {'cap':>6} {'util%':>7} {'headroom%':>10}  "
          f"permission")
    perm = {}
    for cell in CELLS:
        for arm in ("OLD", "NEW"):
            g = [r for r in rs if r["cell"] == cell and r["arm"] == arm
                 and live(r) and r.get("tc_bytes") and r.get("seconds")]
            if not g:
                continue
            us = [r["tc_bytes"] * 8.0 / (r["seconds"] * CAPACITY[cell] * 1e6)
                  * 100.0 for r in g]
            u = mean(us)
            h = 100.0 - u
            perm[(cell, arm)] = h >= HEADROOM_BAR
            p = ("throughput targets PERMITTED" if h >= HEADROOM_BAR
                 else "PARITY / LATENCY ONLY")
            print(f"{cell:>4} {arm:>4} {CAPACITY[cell]:>6.0f} {u:>7.1f} "
                  f"{h:>10.1f}  {p}")
    return perm


def censoring(rs):
    """G-CENSOR + D4. The censoring is a SCORED QUANTITY here, not a footnote:
    it decides which percentiles exist at all."""
    hdr("4 — G-CENSOR: the delivered probe's loss accounting, PER LEG")
    print(f"{'cell':>4} {'arm':>4} {'seed':>5} {'leg':>4} {'n':>4} "
          f"{'sent':>6} {'recv':>6} {'censor%':>8} {'D4 band':>12}  verdict")
    unscoreable = []
    fallbacks = 0
    for cell in CELLS:
        for arm in ("OLD", "NEW"):
            for seed in sorted({r["seed"] for r in rs}):
                g = [r for r in rs if r["cell"] == cell and r["arm"] == arm
                     and r["seed"] == seed and live(r)]
                for leg in (0, 1):
                    cf = [r[f"leg{leg}_censor_frac"] for r in g
                          if r.get(f"leg{leg}_censor_frac") is not None]
                    st = [r[f"leg{leg}_sent"] for r in g
                          if r.get(f"leg{leg}_sent")]
                    rc = [r[f"leg{leg}_recv"] for r in g
                          if r.get(f"leg{leg}_recv") is not None]
                    fallbacks += sum(
                        1 for r in g
                        if r.get(f"leg{leg}_sent_source") not in (None, "summary"))
                    if not cf:
                        continue
                    c = 100.0 * mean(cf)
                    lo = D4_FLOOR[(cell, leg)]
                    hi = D4_CEIL[leg]
                    v = ("BELOW FLOOR (instrument!)" if c < lo
                         else "ABOVE BAR — UNSCOREABLE" if c > 100 * CONTRACT_BAR
                         else "in band" if c <= hi else "above D4 ceiling")
                    if c > 100 * CONTRACT_BAR:
                        unscoreable.append((cell, arm, seed, leg, c))
                    print(f"{cell:>4} {arm:>4} {seed:>5} {leg:>4} {len(cf):>4} "
                          f"{int(mean(st)) if st else 0:>6} "
                          f"{int(mean(rc)) if rc else 0:>6} {c:>8.2f} "
                          f"{f'[{lo},{hi}]':>12}  {v}")
    print(f"\n  sent_source fallbacks to the max_icmp_seq LOWER BOUND: "
          f"{fallbacks} (0 = ping wrote its own summary on every leg)")
    if unscoreable:
        print(f"  {len(unscoreable)} leg-cell-arm-seed groups exceed the "
              f"{100*CONTRACT_BAR:.0f} % CONTRACT BAR — reading rule (iii) FIRES")
    else:
        print(f"  NO group exceeds the {100*CONTRACT_BAR:.0f} % CONTRACT BAR — "
              f"reading rule (iii) DOES NOT FIRE")
    return unscoreable


def placeable(rs, cell, arm, seed, leg, pct):
    """THE STRUCTURAL RULE, applied to a GROUP rather than a row: a percentile
    is reported only if it was placeable on EVERY rep that contributes to it.
    A mean over reps where the quantity did not exist on some of them is not a
    mean of that quantity."""
    g = [r for r in rs if r["cell"] == cell and r["arm"] == arm
         and r["seed"] == seed and live(r)]
    flags = [r.get(f"leg{leg}_{pct}_scoreable") for r in g
             if r.get(f"leg{leg}_{pct}") is not None]
    if not flags:
        return None, 0
    return (all(flags), sum(1 for f in flags if not f))


def engine_gauge(rs):
    hdr("5 — E-LAT-ENGINE: q_p50, two-sided (a REPRODUCTION check, not a claim)")
    print(f"{'cell':>4} {'seed':>5} {'nOLD':>5} {'nNEW':>5} {'OLD':>8} "
          f"{'NEW':>8} {'delta':>9} {'2sig':>8}  verdict / vs D5")
    out = {}
    for cell in CELLS:
        for seed in sorted({r["seed"] for r in rs}):
            a = [r["q_p50"] for r in rs if r["cell"] == cell and r["arm"] == "OLD"
                 and r["seed"] == seed and live(r) and r.get("q_p50") is not None]
            b = [r["q_p50"] for r in rs if r["cell"] == cell and r["arm"] == "NEW"
                 and r["seed"] == seed and live(r) and r.get("q_p50") is not None]
            if not a or not b:
                continue
            d = mean(b) - mean(a)
            band = pooled2s(a, b)
            v = verdict_two_sided(d, band, ("UP", "DOWN"))
            key = f"D5_{cell}"
            lo, hi = PRED[key][1]
            agree = against_interval(d, lo, hi, band)
            out[(cell, seed)] = (d, band, v)
            print(f"{cell:>4} {seed:>5} {len(a):>5} {len(b):>5} {mean(a):>8.1f} "
                  f"{mean(b):>8.1f} {d:>9.1f} "
                  f"{'-' if band is None else f'{band:>8.1f}'}  {v} / "
                  f"D5[{lo},{hi}] {agree}")
    return out


def delivered(rs, unscoreable):
    hdr("6 — D-LAT: DELIVERED LATENCY, PER LEG, WITH CENSORING BESIDE EVERY "
        "PERCENTILE")
    print("   leg 0 = path A (10.77.0.2) — THE ONLY LEG THE ERA BATTERY SAW")
    print("   leg 1 = path B (10.78.0.2) — never measured before this battery\n")
    print(f"{'cell':>4} {'seed':>5} {'leg':>4} {'pct':>4} {'nOLD':>5} "
          f"{'nNEW':>5} {'OLD':>8} {'NEW':>8} {'delta':>9} {'2sig':>8} "
          f"{'cenOLD%':>8} {'cenNEW%':>8}  verdict")
    out = {}
    dead = set((c, a, s, l) for c, a, s, l, _ in unscoreable)
    for cell in CELLS:
        for seed in sorted({r["seed"] for r in rs}):
            for leg in (0, 1):
                for pct in PCTS:
                    col = f"leg{leg}_{pct}"
                    a = [r[col] for r in rs if r["cell"] == cell
                         and r["arm"] == "OLD" and r["seed"] == seed
                         and live(r) and r.get(col) is not None]
                    b = [r[col] for r in rs if r["cell"] == cell
                         and r["arm"] == "NEW" and r["seed"] == seed
                         and live(r) and r.get(col) is not None]
                    if not a or not b:
                        continue
                    ca = mean([r[f"leg{leg}_censor_frac"] for r in rs
                               if r["cell"] == cell and r["arm"] == "OLD"
                               and r["seed"] == seed and live(r)
                               and r.get(f"leg{leg}_censor_frac") is not None])
                    cb = mean([r[f"leg{leg}_censor_frac"] for r in rs
                               if r["cell"] == cell and r["arm"] == "NEW"
                               and r["seed"] == seed and live(r)
                               and r.get(f"leg{leg}_censor_frac") is not None])
                    d = mean(b) - mean(a)
                    band = pooled2s(a, b)
                    v = verdict_two_sided(d, band, ("SLOWER", "FASTER"))
                    pa, na = placeable(rs, cell, "OLD", seed, leg, pct)
                    pb, nb = placeable(rs, cell, "NEW", seed, leg, pct)
                    notes = []
                    if (cell, "OLD", seed, leg) in dead or \
                       (cell, "NEW", seed, leg) in dead:
                        notes.append("UNSCOREABLE(contract bar)")
                        v = "UNSCOREABLE"
                    if pa is False or pb is False:
                        notes.append(f"STRUCTURALLY CENSORED on "
                                     f"{na}+{nb} reps")
                        v = "UNSCOREABLE"
                    out[(cell, seed, leg, pct)] = (d, band, v)
                    print(f"{cell:>4} {seed:>5} {leg:>4} {pct:>4} {len(a):>5} "
                          f"{len(b):>5} {mean(a):>8.1f} {mean(b):>8.1f} "
                          f"{d:>9.1f} "
                          f"{'-' if band is None else f'{band:>8.1f}'} "
                          f"{100*ca:>8.2f} {100*cb:>8.2f}  {v}"
                          + ("  " + "; ".join(notes) if notes else ""))
    return out


def goodput(rs, perm):
    hdr("7 — G-GOOD: goodput, carried as a GUARD (the load that produced the "
        "latency), not as a claim")
    print(f"{'cell':>4} {'seed':>5} {'OLD':>9} {'NEW':>9} {'delta':>9} "
          f"{'2sig':>8} {'%':>8}  verdict")
    for cell in CELLS:
        for seed in sorted({r["seed"] for r in rs}):
            a = [r["mbps"] for r in rs if r["cell"] == cell and r["arm"] == "OLD"
                 and r["seed"] == seed and live(r) and r.get("mbps")]
            b = [r["mbps"] for r in rs if r["cell"] == cell and r["arm"] == "NEW"
                 and r["seed"] == seed and live(r) and r.get("mbps")]
            if not a or not b:
                continue
            d = mean(b) - mean(a)
            band = pooled2s(a, b)
            v = verdict_two_sided(d, band)
            note = "" if perm.get((cell, "NEW"), True) else "  (magnitude not claimed)"
            print(f"{cell:>4} {seed:>5} {mean(a):>9.2f} {mean(b):>9.2f} "
                  f"{d:>9.2f} {'-' if band is None else f'{band:>8.2f}'} "
                  f"{100*d/mean(a):>7.2f}%  {v}{note}")


def rules(eng, deliv, unscoreable):
    """THE PRE-COMMITTED READING RULES, APPLIED MECHANICALLY.

    Rule (i) requires BOTH legs' delivered p50 to FALL at BOTH loaded duals.
    The contract is explicit that a win on one leg and a rise on the other is
    outcome (ii), not a split decision — which is exactly the arithmetic the
    single-leg probe could not do.
    """
    hdr("8 — THE PRE-COMMITTED READING RULES (i)/(ii)/(iii)")
    if unscoreable:
        print("  (iii) FIRES on: " + ", ".join(
            f"{c}/{a}/s{s}/leg{l} ({v:.1f} %)" for c, a, s, l, v in unscoreable))
    else:
        print(f"  (iii) does NOT fire: no leg exceeds the "
              f"{100*CONTRACT_BAR:.0f} % contract bar.")
    fell, rose, dead = [], [], []
    for cell in LOADED_DUALS:
        for seed in sorted({k[1] for k in deliv}):
            for leg in (0, 1):
                k = (cell, seed, leg, "p50")
                if k not in deliv:
                    continue
                d, band, v = deliv[k]
                if v == "UNSCOREABLE":
                    dead.append(k)
                elif v == "FASTER":
                    fell.append((k, d))
                elif v == "SLOWER":
                    rose.append((k, d))
    print(f"\n  loaded-dual delivered p50 contrasts: "
          f"{len(fell)} FASTER, {len(rose)} SLOWER, "
          f"{len([k for k in deliv if k[0] in LOADED_DUALS and k[3] == 'p50']) - len(fell) - len(rose) - len(dead)} PARITY, "
          f"{len(dead)} unscoreable")
    for k, d in rose:
        print(f"    SLOWER: {k[0]} s{k[1]} leg{k[2]}  {d:+.1f} ms")
    for k, d in fell:
        print(f"    FASTER: {k[0]} s{k[1]} leg{k[2]}  {d:+.1f} ms")
    # RULE (i) IS APPLIED LITERALLY, and the literal text is strict: it requires
    # BOTH legs' delivered p50 to FALL, at BOTH loaded duals, RESOLVED at 2
    # sigma. "Mostly parity with one resolved improvement" does NOT satisfy it,
    # and a scorer that treated `fell and not rose` as sufficient would be
    # rewriting the contract in the arc's favour after seeing the numbers —
    # which is the exact failure this whole goal exists to prevent.
    need = [(c, s, l) for c in LOADED_DUALS
            for s in sorted({k[1] for k in deliv}) for l in (0, 1)
            if (c, s, l, "p50") in deliv]
    got = {(k[0], k[1], k[2]) for k, _ in fell}
    i_holds = bool(need) and all(x in got for x in need)
    print(f"\n  rule (i) requires all {len(need)} loaded-dual leg-seed p50 "
          f"contrasts to resolve FASTER; {len(got)} do.")
    if rose:
        print("\n  ==> RULE (ii) FIRES. Delivered p50 RISES in NEW on at least "
              "one leg of a loaded dual, resolved at 2 sigma.")
        print("      THE ERA CLAIM IS REVISED by the pre-registered three-site "
              "pointer edit.")
    elif i_holds:
        print("\n  ==> RULE (i) HOLDS. Delivered p50 falls in NEW on every "
              "loaded-dual leg; the claim SURVIVES and is restated with BOTH "
              "magnitudes.")
    else:
        print("\n  ==> NEITHER (i) NOR (ii) FIRES, AND THE CONTRACT DID NOT "
              "ANTICIPATE THIS OUTCOME.")
        print("      No delivered p50 contrast at a loaded dual resolves "
              "SLOWER (so the era battery's own delivered reading does NOT "
              "reproduce), and not all of them resolve FASTER (so the claim is "
              "not established in delivered terms either).")
        print("      This is a SPECIFICATION GAP in the pre-registration and is "
              "recorded as one, not resolved by picking the nearer rule.")
    return fell, rose, dead, i_holds


def main():
    paths = [a for a in sys.argv[1:] if not a.startswith("-")]
    if not paths:
        print(__doc__.strip().splitlines()[2].strip(), file=sys.stderr)
        return 2
    rs = rows(paths)
    print("THE LATENCY-TRUTH BATTERY — SCORED AGAINST THE PRE-REGISTRATION")
    print("=" * 62)
    print(f"  {len(rs)} rows | ledgers {len(paths)}")
    fired = abort_table(rs)
    liveness(rs)
    perm = headroom(rs)
    unsc = censoring(rs)
    eng = engine_gauge(rs)
    deliv = delivered(rs, unsc)
    goodput(rs, perm)
    rules(eng, deliv, unsc)
    print("\nNothing in this report flips a default, and neither gauge is "
          "promoted over the other.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
