#!/usr/bin/env python3
"""Report for the LATENCY-SCORED battery (goal-gate "Latency Lever").

    lat_report.py <log> [<log> ...]

Scrapes `LATRESULT ` lines and scores them against the PRE-REGISTERED
predictions, never against a number chosen afterwards. Sections:

  1 LIVENESS + ABORT/DNF/INSTRUMENT-FAIL   (disciplines 1, 15, and the
    abort-vs-DNF distinction the previous battery needed)
  2 THE HEADROOM TABLE                     (discipline 16: measured
    utilisation beside every cell, so no reader has to take it on trust)
  3 THE SCORE — delivered probe latency, with 2 sigma_pooled on every claim
  4 THE CONSTRAINT — goodput parity
  5 PREDICTION 1 — sign(delta p50) vs sign(R-1), R = cap_B/cap_A
  6 PREDICTION 3 — the wait attribution
  7 PREDICTION 4 — the eviction audit
"""
import json
import statistics as st
import sys
from collections import defaultdict

CAP = {"sc2": 100e6, "sc3": 20e6, "c2r100": 100e6, "c7": 200e6,
       "c2r200": 100e6, "c1": 1000e6, "c8": 120e6, "jit25": 100e6}
ORDER = ["sc2", "sc3", "c2r100", "c7", "c2r200", "c1"]
ARMS = ["A", "B", "D"]

rows = []
for p in sys.argv[1:]:
    for ln in open(p, errors="replace"):
        i = ln.find("LATRESULT ")
        if i < 0:
            continue
        try:
            rows.append(json.loads(ln[i + 10:]))
        except Exception:
            pass


def is_abort(r):
    """No [GATES] on EITHER endpoint and no run: the invocation died before
    the engine started. Contributes no datum. A DNF is a transfer that RAN
    and did not finish -- the previous battery would have reported dnf=111
    by conflating them."""
    return r["gates_lines_cli"] == 0 and r["gates_lines_srv"] == 0 and r["n_runs"] == 0


live = [r for r in rows if not is_abort(r)]
g = defaultdict(list)
for r in live:
    g[(r["cell"], r["arm"], r["seed"])].append(r)

CELLS = [c for c in ORDER if any(k[0] == c for k in g)] or sorted({k[0] for k in g})
SEEDS = sorted({k[2] for k in g})


def col(key, f):
    v = [r[f] for r in g.get(key, []) if r.get(f) is not None]
    return v


def ms(v):
    return (st.mean(v), st.pstdev(v), len(v)) if v else (None, None, 0)


def cmp2s(a, b):
    """|mean_b - mean_a| against 2 x pooled sigma. The ONLY licence to call
    an effect real (discipline 5)."""
    # n < 2 has NO spread, so a 2-sigma test on it reads `sigma = 0` and
    # calls every difference significant. That is false certainty of exactly
    # the kind discipline 5 exists to refuse, and a calibration log run
    # through this tool would otherwise print EXCEEDS on every row.
    if not a or not b or len(a) < 2 or len(b) < 2:
        return None
    d = abs(st.mean(b) - st.mean(a))
    sp = (st.pstdev(a) ** 2 + st.pstdev(b) ** 2) ** 0.5
    return d, 2 * sp, d > 2 * sp


print("=" * 100)
print("1. LIVENESS / ABORT / DNF / INSTRUMENT-FAIL")
print("=" * 100)
aborts = [r for r in rows if is_abort(r)]
real_dnf = [r for r in live if r["dnf"]]
print(f"  invocations={len(rows)}  live={len(live)}  ABORTS={len(aborts)} "
      f"({100.0*len(aborts)/max(len(rows),1):.1f}%)  REAL DNF={len(real_dnf)}")
bad = [r for r in live
       if (r["arm"] == "B" and (r["gates_cli_3t"] != 1 or r["gates_srv_3t"] != 1))
       or (r["arm"] != "B" and (r["gates_cli_3t"] != 0 or r["gates_srv_3t"] != 0))
       or (r["arm"] in ("B", "D") and r["gates_cli_rs"] != 1)
       or (r["arm"] == "A" and r["gates_cli_rs"] != 0)]
print(f"  two-sided gate mismatches (discipline 15c): {len(bad)}")
noinst = [r for r in live if not r["wait_lines"] or not r["ping_n"] or r["dgq_hand"] is None]
print(f"  INSTRUMENT-FAIL (engine ran, a gauge missing): {len(noinst)}")
warm = [r for r in live if r["arm"] == "B" and r["tt_eng1"] == 0]
print(f"  arm-B invocations with no [3T] eng=1 (warm-up failure): {len(warm)}")

print()
print("=" * 100)
print("2. HEADROOM (MEASUREMENT DISCIPLINE 16) -- tc-measured, arm A, every cell")
print("=" * 100)
print(f"  {'cell':8s} {'seed':>4s} {'shaped Mbit':>11s} {'tc Mbit/s':>10s} {'util %':>8s} {'headroom %':>10s}  target permitted?")
for c in CELLS:
    for s in SEEDS:
        rs = g.get((c, "A", s), [])
        u = [r["tc_bytes"] * 8 / r["seconds"] for r in rs
             if r.get("tc_bytes") and r.get("seconds")]
        if not u:
            continue
        m = st.mean(u)
        util = m / CAP[c] * 100
        hr = 100 - util
        print(f"  {c:8s} {s:>4d} {CAP[c]/1e6:>11.0f} {m/1e6:>10.1f} {util:>8.1f} {hr:>10.1f}"
              f"  {'YES' if hr >= 5 else 'NO -- latency only'}")

print()
print("=" * 100)
print("3. THE SCORE -- delivered probe latency (independent ICMP flow, ms)")
print("=" * 100)
for f in ("ping_p50", "ping_p95", "ping_p99"):
    print(f"\n  --- {f} ---")
    print(f"  {'cell':8s} {'seed':>4s} " + " ".join(f"{a:>16s}" for a in ARMS)
          + f" {'B/A':>7s} {'|B-A| vs 2sig':>22s}")
    for c in CELLS:
        for s in SEEDS:
            cells = []
            for a in ARMS:
                m, sd, n = ms(col((c, a, s), f))
                cells.append(f"{m:7.1f}+-{sd:4.1f}({n:d})" if m is not None else f"{'-':>16s}")
            A, B = col((c, "A", s), f), col((c, "B", s), f)
            ratio = (st.mean(B) / st.mean(A)) if A and B else None
            k = cmp2s(A, B)
            verd = (f"{k[0]:7.1f} vs {k[1]:6.1f} {'EXCEEDS' if k[2] else 'within'}"
                    if k else "-")
            print(f"  {c:8s} {s:>4d} " + " ".join(cells)
                  + (f" {ratio:7.3f}" if ratio else f" {'-':>7s}")
                  + f" {verd:>22s}")

print()
print("=" * 100)
print("4. THE CONSTRAINT -- goodput parity (Mbit/s). A latency win bought with")
print("   goodput is a TRADE, and F-LOSE-1 denies a default on it.")
print("=" * 100)
print(f"  {'cell':8s} {'seed':>4s} " + " ".join(f"{a:>16s}" for a in ARMS)
      + f" {'B/A':>7s} {'D/A':>7s} {'|B-A| vs 2sig':>22s}")
for c in CELLS:
    for s in SEEDS:
        cells = []
        for a in ARMS:
            m, sd, n = ms(col((c, a, s), "mbps"))
            cells.append(f"{m:7.1f}+-{sd:4.1f}({n:d})" if m is not None else f"{'-':>16s}")
        A, B, D = (col((c, x, s), "mbps") for x in ARMS)
        ba = st.mean(B) / st.mean(A) if A and B else None
        da = st.mean(D) / st.mean(A) if A and D else None
        k = cmp2s(A, B)
        verd = f"{k[0]:7.1f} vs {k[1]:6.1f} {'EXCEEDS' if k[2] else 'within'}" if k else "-"
        print(f"  {c:8s} {s:>4d} " + " ".join(cells)
              + (f" {ba:7.3f}" if ba else f" {'-':>7s}")
              + (f" {da:7.3f}" if da else f" {'-':>7s}") + f" {verd:>22s}")

print()
print("=" * 100)
print("5. PREDICTION 1 -- sign(delta p50) must match sign(R-1), R = cap_B/cap_A.")
print("   ONE formula, no cell-specific term; the direction was written in")
print("   advance at every cell INCLUDING the two where it predicts a loss.")
print("=" * 100)
print(f"  {'cell':8s} {'seed':>4s} {'cap_A':>7s} {'cap_B':>7s} {'R':>6s} "
      f"{'p50_A':>7s} {'p50_B':>7s} {'p50 B/A':>8s}  predicted   MATCH?")
hits = tot = 0
for c in CELLS:
    for s in SEEDS:
        ca, cb = col((c, "A", s), "occcap_p50"), col((c, "B", s), "occcap_p50")
        pa, pb = col((c, "A", s), "ping_p50"), col((c, "B", s), "ping_p50")
        if not (ca and cb and pa and pb):
            continue
        R = st.mean(cb) / st.mean(ca)
        pr = st.mean(pb) / st.mean(pa)
        pred = "DOWN" if R < 0.9 else ("UP" if R > 1.1 else "FLAT")
        got = "DOWN" if pr < 0.9 else ("UP" if pr > 1.1 else "FLAT")
        ok = pred == got
        hits += ok
        tot += 1
        print(f"  {c:8s} {s:>4d} {st.mean(ca):7.0f} {st.mean(cb):7.0f} {R:6.2f} "
              f"{st.mean(pa):7.1f} {st.mean(pb):7.1f} {pr:8.3f}  {pred:9s}   "
              f"{'YES' if ok else 'NO (got ' + got + ')'}")
print(f"\n  SIGN MATCHED AT {hits}/{tot} cell-seeds "
      f"(F-WIN-2 needs >= 5 of 6 per seed; F-LOSE-3 fires on a between-seed split)")

print()
print("=" * 100)
print("6. PREDICTION 3 -- the wait attribution (% of sender wall time by select! arm).")
print("   Did not exist before this branch: `stall[` was in 0 of the previous")
print("   battery's 1 116 logs, so `sidle` was one bucket attributed to nothing.")
print("=" * 100)
W = ["wait_tun", "wait_paused", "wait_nack", "wait_pace", "wait_gen", "wait_tail"]
print(f"  {'cell':8s} {'a':2s} {'seed':>4s} " + " ".join(f"{w[5:]:>7s}" for w in W))
for c in CELLS:
    for a in ARMS:
        for s in SEEDS:
            v = [ms(col((c, a, s), w))[0] for w in W]
            if v[0] is None:
                continue
            print(f"  {c:8s} {a:2s} {s:>4d} "
                  + " ".join(f"{x:7.0f}" if x is not None else f"{'-':>7s}" for x in v))

print()
print("=" * 100)
print("7. PREDICTION 4 -- the datagram eviction audit (H5). Pre-registered:")
print("   full = 0 and (hand-tx)/hand < 0.005 in EVERY arm of every cell.")
print("=" * 100)
print(f"  {'cell':8s} {'a':2s} {'seed':>4s} {'hand':>10s} {'gap=hand-tx':>12s} {'gap frac':>10s} {'full':>6s} {'err':>5s}")
h5 = False
for c in CELLS:
    for a in ARMS:
        for s in SEEDS:
            hd, gp = col((c, a, s), "dgq_hand"), col((c, a, s), "dgq_gap")
            fl, er = col((c, a, s), "dgq_full"), col((c, a, s), "dgq_err")
            if not hd:
                continue
            H, G = st.mean(hd), st.mean(gp) if gp else 0
            F = st.mean(fl) if fl else 0
            frac = G / H if H else 0
            if F > 0 or frac >= 0.005:
                h5 = True
            print(f"  {c:8s} {a:2s} {s:>4d} {H:10.0f} {G:12.1f} {frac:10.5f} "
                  f"{F:6.1f} {st.mean(er) if er else 0:5.1f}")
print(f"\n  H5 (silent datagram eviction): {'CONFIRMED somewhere -- see rows above' if h5 else 'REFUTED at every arm of every cell'}")
