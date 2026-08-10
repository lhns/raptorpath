#!/usr/bin/env python3
"""Report for the HONEST-INPUTS scored battery (goal-gate "Honest Inputs —
PRE-REGISTRATION", commit 6f6f2a9).

    hi_report.py <log> [<log> ...]

Scrapes `HIRESULT ` lines and scores them against the PRE-REGISTERED
criteria, never against a number chosen afterwards:

  H1  c1: DH/A goodput parity within 2 sigma, both seeds (point 0.95-1.02;
      < 0.90 falsifies fold-dominance). D/A must reproduce <= 0.75.
  H2  c1: DH sender CPU/byte <= 1.15x A (D measured x1.60/1.62); parity
      with CPU/byte > 1.25x A is flagged, not banked.
  H3  jit25: BH [3T] limit inside 1300-1430, window term <= 500; B must
      reproduce >= 1800-class. > 1600 falsifies smoothing-as-mechanism;
      < 900 = the raw floor undershoots (expected-failure 1). khr/kraw
      decomposes the bias in-cell.
  H4  sc2: BH and DH goodput within sigma of A; BH probe p50 in the
      <= 55 ms class at parity ("RTT halved at parity" must survive).
  H5  c7: DH > D by > 2 sigma, point DH/A >= 0.95; the DH/D vs c1 split
      answers "is D/A 0.88 pure CPU?". c8: DH within 2 sigma of D,
      aborts counted per arm, no asymmetric top-up.
"""
import json
import statistics as st
import sys
from collections import defaultdict

CAP = {"c1": 1000e6, "jit25": 100e6, "sc2": 100e6, "c7": 200e6, "c8": 120e6}
ORDER = ["c1", "jit25", "sc2", "c7", "c8"]
ARMS = ["A", "D", "DH", "B", "BH"]
EXPECT = {  # arm -> (3t, rs, ha, hk)
    "A": (0, 0, 0, 0), "D": (0, 1, 0, 0), "DH": (0, 1, 1, 0),
    "B": (1, 1, 0, 0), "BH": (1, 1, 1, 1),
}

rows = []
for p in sys.argv[1:]:
    for ln in open(p, errors="replace"):
        i = ln.find("HIRESULT ")
        if i < 0:
            continue
        try:
            rows.append(json.loads(ln[i + 9:]))
        except Exception:
            pass


def is_abort(r):
    return r["gates_lines_cli"] == 0 and r["gates_lines_srv"] == 0 and r["n_runs"] == 0


def gate_ok(r):
    e = EXPECT[r["arm"]]
    got = (r["gates_cli_3t"], r["gates_cli_rs"], r["gates_cli_ha"], r["gates_cli_hk"])
    gots = (r["gates_srv_3t"], r["gates_srv_rs"], r["gates_srv_ha"], r["gates_srv_hk"])
    if got != e or gots != e:
        return False
    # the fix gates' ACTIVE echoes, two-sided presence/absence
    if e[2] == 1 and (r["active_ha_cli"] == 0 or r["active_ha_srv"] == 0):
        return False
    if e[2] == 0 and (r["active_ha_cli"] > 0 or r["active_ha_srv"] > 0):
        return False
    if e[3] == 1 and (r["active_hk_cli"] == 0 or r["active_hk_srv"] == 0):
        return False
    if e[3] == 0 and (r["active_hk_cli"] > 0 or r["active_hk_srv"] > 0):
        return False
    if e[0] == 1 and r["tt_eng1"] == 0:
        return False
    return True


live = [r for r in rows if not is_abort(r)]
voided = [r for r in live if not gate_ok(r)]
scored = [r for r in live if gate_ok(r)]
g = defaultdict(list)
for r in scored:
    g[(r["cell"], r["arm"], r["seed"])].append(r)

CELLS = [c for c in ORDER if any(k[0] == c for k in g)] or sorted({k[0] for k in g})
SEEDS = sorted({k[2] for k in g})


def col(key, f):
    return [r[f] for r in g.get(key, []) if r.get(f) is not None]


def ms(v):
    return (st.mean(v), st.pstdev(v), len(v)) if v else (None, None, 0)


def cmp2s(a, b):
    """|mean_b - mean_a| vs 2 x pooled sigma (discipline 5); None if n < 2."""
    if not a or not b or len(a) < 2 or len(b) < 2:
        return None
    d = abs(st.mean(b) - st.mean(a))
    sp = (st.pstdev(a) ** 2 + st.pstdev(b) ** 2) ** 0.5
    return d, 2 * sp, d > 2 * sp


def cpb(key):
    """sender CPU seconds per delivered byte, relative form: cpucli/(mbps*s).
    Bytes delivered per run are fixed per cell, so cpucli alone is CPU/byte
    up to the cell's constant; ratios between arms cancel it."""
    return [r["cpucli"] for r in g.get(key, []) if r.get("cpucli") is not None]


print("=" * 100)
print("1. LIVENESS / ABORT / VOID / DNF / INSTRUMENT-FAIL")
print("=" * 100)
aborts = [r for r in rows if is_abort(r)]
real_dnf = [r for r in scored if r["dnf"]]
nocpu = [r for r in live if r.get("cpucli") is None]
print(f"  invocations(parsed)={len(rows)}  live={len(live)}  ABORTS={len(aborts)}"
      f" ({100.0*len(aborts)/max(len(rows),1):.1f}%)  VOID(echo/gate)={len(voided)}"
      f"  scored={len(scored)}  REAL DNF={len(real_dnf)}  no-CPU-gauge={len(nocpu)}")
ab = defaultdict(int)
for r in aborts:
    ab[(r["cell"], r["arm"], r["seed"])] += 1
for k in sorted(ab):
    print(f"    abort {k[0]}-{k[1]} s{k[2]}: {ab[k]}")
for r in voided:
    print(f"    VOID {r['cell']}-{r['arm']} s{r['seed']} rep{r['rep']}")

print()
print("=" * 100)
print("2. HEADROOM (discipline 16) -- tc-measured, arm A, every cell")
print("=" * 100)
for c in CELLS:
    for s in SEEDS:
        rs = g.get((c, "A", s), [])
        u = [r["tc_bytes"] * 8 / r["seconds"] for r in rs
             if r.get("tc_bytes") and r.get("seconds")]
        if not u:
            continue
        m = st.mean(u)
        print(f"  {c:6s} s{s:<3d} shaped={CAP[c]/1e6:5.0f} tc={m/1e6:6.1f} Mbit/s"
              f"  util={m/CAP[c]*100:5.1f}%  headroom={100-m/CAP[c]*100:5.1f}%")

print()
print("=" * 100)
print("3. GOODPUT (Mbit/s) with the CPU gauge beside it -- per cell/arm/seed")
print("=" * 100)
hdr = "  {:6s} {:>4s} " + " ".join("{:>19s}" for _ in ARMS)
print(hdr.format("cell", "seed", *ARMS))
for c in CELLS:
    for s in SEEDS:
        cells = []
        for a in ARMS:
            m, sd, n = ms(col((c, a, s), "mbps"))
            cells.append(f"{m:7.1f}+-{sd:4.1f}({n:d})" if m is not None else "-")
        print(hdr.format(c, str(s), *cells))
print("\n  sender CPU seconds (CPUCLI; fixed bytes per cell so ratios are CPU/byte):")
print(hdr.format("cell", "seed", *ARMS))
for c in CELLS:
    for s in SEEDS:
        cells = []
        for a in ARMS:
            m, sd, n = ms(cpb((c, a, s)))
            cells.append(f"{m:7.2f}+-{sd:4.2f}({n:d})" if m is not None else "-")
        print(hdr.format(c, str(s), *cells))

print()
print("=" * 100)
print("4. RATIOS vs A (goodput and sender-CPU/byte), with 2 sigma_pooled verdicts")
print("=" * 100)
for c in CELLS:
    for s in SEEDS:
        A = col((c, "A", s), "mbps")
        Ac = cpb((c, "A", s))
        if not A:
            continue
        for a in ["D", "DH", "B", "BH"]:
            X = col((c, a, s), "mbps")
            Xc = cpb((c, a, s))
            if not X:
                continue
            k = cmp2s(A, X)
            verd = (f"|d|={k[0]:6.1f} vs 2sig={k[1]:6.1f} {'EXCEEDS' if k[2] else 'within'}"
                    if k else "-")
            # CPU/byte ratio: cpucli ratio x (bytes ratio = 1) -> cpucli ratio
            cr = (st.mean(Xc) / st.mean(Ac)) if Ac and Xc else None
            crs = f"  CPU/byte={cr:5.3f}x" if cr else ""
            print(f"  {c:6s} s{s:<3d} {a:3s}/A goodput={st.mean(X)/st.mean(A):6.3f}  {verd}{crs}")

print()
print("=" * 100)
print("5. THE [3T] LIMIT + K DECOMPOSITION (H3, jit25) -- cap/window/khr/kraw")
print("=" * 100)
for c in CELLS:
    for s in SEEDS:
        for a in ARMS:
            key = (c, a, s)
            cm, _, cn = ms(col(key, "cap_med"))
            wm, _, _ = ms(col(key, "window_med"))
            sm, _, _ = ms(col(key, "slack_med"))
            km, _, kn = ms(col(key, "khr_med"))
            rm, _, rn = ms(col(key, "kraw_med"))
            if cm is None and km is None:
                continue
            print(f"  {c:6s} s{s:<3d} {a:3s} cap={cm and round(cm) or '-':>5} "
                  f"window={wm and round(wm) or '-':>5} slack={sm and round(sm) or '-':>5} "
                  f"khr={km and round(km,3) or '-':>6}({kn}) kraw={rm and round(rm,3) or '-':>6}({rn})")

print()
print("=" * 100)
print("6. DELIVERED PROBE LATENCY (H4, sc2) -- independent ICMP flow, ms")
print("=" * 100)
for c in CELLS:
    for s in SEEDS:
        A = col((c, "A", s), "ping_p50")
        if not A:
            continue
        line = f"  {c:6s} s{s:<3d}"
        for a in ARMS:
            X = col((c, a, s), "ping_p50")
            if not X:
                line += f" {a}=-"
                continue
            m, sd, n = ms(X)
            k = cmp2s(A, X) if a != "A" else None
            v = ("*" if k and k[2] else "")
            line += f" {a}={m:.1f}+-{sd:.1f}({n}){v}"
        print(line + "   [* = exceeds 2sig vs A]")
        for f in ("ping_p95", "ping_p99"):
            A2 = col((c, "A", s), f)
            if not A2:
                continue
            l2 = f"    {f}:"
            for a in ARMS:
                X = col((c, a, s), f)
                if X:
                    l2 += f" {a}={st.mean(X):.1f}"
            print(l2)

print()
print("=" * 100)
print("7. WAIT ATTRIBUTION + EVICTION (mechanism corroboration)")
print("=" * 100)
for c in CELLS:
    for s in SEEDS:
        for a in ARMS:
            key = (c, a, s)
            tun = ms(col(key, "wait_tun"))[0]
            pau = ms(col(key, "wait_paused"))[0]
            fl = col(key, "dgq_full")
            gp = col(key, "dgq_gap")
            hd = col(key, "dgq_hand")
            if tun is None:
                continue
            frac = (st.mean(gp) / st.mean(hd)) if hd and gp and st.mean(hd) else 0
            print(f"  {c:6s} s{s:<3d} {a:3s} wait[tun={tun:3.0f}% paused={pau:3.0f}%]"
                  f" dgq[full={st.mean(fl) if fl else 0:6.1f} gapfrac={frac:.5f}]")
