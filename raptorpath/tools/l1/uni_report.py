#!/usr/bin/env python3
"""Report for the STORE-CAP UNIFICATION attribution + flip battery
(goal-gate "Store-Cap Unification — ATTRIBUTION + FLIP BATTERY —
PRE-REGISTRATION" is the contract; this prints the gauges and scores
the PRE-REGISTERED clauses, never a number chosen afterwards).

    uni_report.py <log> [<log> ...]

Scrapes the `FLIPRESULT ` lines emitted by flip_parse.py (reused
byte-identical so numbers pool across sessions without a second
dialect). Arms (all on the new default, RWM_HONEST_ANCHOR ON):

  A    shipped default            AU   A + RWM_STORE_CAP_UNIFIED=1
  AL   RWM_HONEST_ANCHOR=0        ALU  AL + U  (the OLD battery's A+U arm)
  RU   RWM_PLAIN_RS=1 + U=1, c1 only (the goal's criterion-1 reader)

Pre-registered clauses (see the goal-gate block for the full text):
  U1  c8:  AU/A >= 0.95 BOTH seeds (point) and not >2sig down.
  U2  c1:  AU - A > 2 sigma_Delta on BOTH seeds (the banked class).
  U3  sc2: AU within 2 sigma of A goodput, probe p50 not >2sig WORSE.
  U4  CPU: AU sender CPU/byte <= 1.05x A at c1/c7 (point band).
  U5  c7:  AU not >2sig below A (no-regression).
  ATTR ALU must reproduce the c8 harm class (>2sig below AL, or point
       ALU/AL <= 0.85, on >= 1 seed, with collapse-mode reps < 60
       present) for the attribution to be SCORED; else era-dead and the
       flip case rests on the fresh A/AU contrast alone.
  ERA  AL within 2 sigma of A everywhere (value-identical statistic;
       beyond-2sig movement is an instrument alarm, not a result).
  C1RD RU within 2 sigma of AU at c1 (criterion 1's post-flip reading).
"""
import json
import statistics as st
import sys
from collections import defaultdict

from capbind_check import print_capbind

CAP = {"c1": 1000e6, "sc2": 100e6, "c7": 200e6, "c8": 120e6}
ORDER = ["c1", "sc2", "c7", "c8"]
ARMS = ["A", "AU", "AL", "ALU", "RU"]
EXPECT = {  # arm -> (3t, rs, ha, hk, u); HA defaults ON in this era
    "A": (0, 0, 1, 0, 0), "AU": (0, 0, 1, 0, 1),
    "AL": (0, 0, 0, 0, 0), "ALU": (0, 0, 0, 0, 1),
    "RU": (0, 1, 1, 0, 1),
}

rows = []
for p in sys.argv[1:]:
    for ln in open(p, errors="replace"):
        i = ln.find("FLIPRESULT ")
        if i < 0:
            continue
        try:
            rows.append(json.loads(ln[i + 11:]))
        except Exception:
            pass


def is_abort(r):
    return r["gates_lines_cli"] == 0 and r["gates_lines_srv"] == 0 and r["n_runs"] == 0


def gate_ok(r):
    e = EXPECT[r["arm"]]
    got = (r["gates_cli_3t"], r["gates_cli_rs"], r["gates_cli_ha"],
           r["gates_cli_hk"], r["gates_cli_u"])
    gots = (r["gates_srv_3t"], r["gates_srv_rs"], r["gates_srv_ha"],
            r["gates_srv_hk"], r["gates_srv_u"])
    if got != e or gots != e:
        return False
    for bit, c, s in ((e[2], "active_ha_cli", "active_ha_srv"),
                      (e[4], "active_u_cli", "active_u_srv")):
        if bit == 1 and (r[c] == 0 or r[s] == 0):
            return False
        if bit == 0 and (r[c] > 0 or r[s] > 0):
            return False
    if r["active_hk_cli"] > 0 or r["active_hk_srv"] > 0 or r["tt_eng1"] > 0:
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
    return [r["cpucli"] for r in g.get(key, []) if r.get("cpucli") is not None]


print("=" * 100)
print("1. LIVENESS / ABORT / VOID / DNF / INSTRUMENT-FAIL")
print("=" * 100)
aborts = [r for r in rows if is_abort(r)]
real_dnf = [r for r in scored if r["dnf"]]
nocpu = [r for r in live if r.get("cpucli") is None]
nosf = [r for r in live if r.get("sf_ticks") is None]
print(f"  invocations(parsed)={len(rows)}  live={len(live)}  ABORTS={len(aborts)}"
      f" ({100.0*len(aborts)/max(len(rows),1):.1f}%)  VOID(echo/gate)={len(voided)}"
      f"  scored={len(scored)}  REAL DNF={len(real_dnf)}  no-CPU-gauge={len(nocpu)}"
      f"  no-SF-gauge={len(nosf)}")
ab = defaultdict(int)
for r in aborts:
    ab[(r["cell"], r["arm"], r["seed"])] += 1
for k in sorted(ab, key=str):
    print(f"    abort {k[0]}-{k[1]} s{k[2]}: {ab[k]}")
for r in voided:
    print(f"    VOID {r['cell']}-{r['arm']} s{r['seed']} rep{r['rep']}")

print()
print("=" * 100)
print("2. HEADROOM (discipline 16) -- tc-measured, arm A, every cell, THIS session")
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
print("3. GOODPUT (Mbit/s, per-arm mean +- sigma (n)) + sender CPU seconds")
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
print("\n  sender CPU seconds (fixed bytes per cell => ratios are CPU/byte):")
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
print("4. THE RATIOS THE CLAUSES ARE SCORED ON (2 sigma pooled verdicts)")
print("=" * 100)
PAIRS = [("AU", "A"), ("AL", "A"), ("ALU", "AL"), ("ALU", "A"),
         ("RU", "AU"), ("RU", "A")]
for c in CELLS:
    for s in SEEDS:
        for a, b in PAIRS:
            X = col((c, a, s), "mbps")
            B = col((c, b, s), "mbps")
            Xc, Bc = cpb((c, a, s)), cpb((c, b, s))
            if not X or not B:
                continue
            k = cmp2s(B, X)
            verd = (f"|d|={k[0]:6.1f} vs 2sig={k[1]:6.1f} {'EXCEEDS' if k[2] else 'within'}"
                    if k else "-")
            cr = (st.mean(Xc) / st.mean(Bc)) if Bc and Xc else None
            crs = f"  CPU/byte={cr:5.3f}x" if cr else ""
            print(f"  {c:6s} s{s:<3d} {a:3s}/{b:<3s} goodput="
                  f"{st.mean(X)/st.mean(B):6.3f}  {verd}{crs}")

print()
print("=" * 100)
print("5. THE MECHANISM GAUGES: [SF] population + the consumed cliff, per arm")
print("=" * 100)
print("  ([SF] zero% = active_paths() EMPTY at refresh (the filter, downstream-")
print("   coupled); capboot% = steady DIAG samples with effective cap <= 128 (the")
print("   CONSUMED cliff); occcap_p50 = the effective cap the transfer ran under.)")
for c in CELLS:
    for s in SEEDS:
        for a in ARMS:
            key = (c, a, s)
            E = col(key, "sf_E")
            sh = col(key, "sf_short")
            ze = col(key, "sf_zero")
            cb = col(key, "capboot_frac")
            oc = col(key, "occcap_p50")
            pa = col(key, "wait_paused")
            rx = col(key, "retx")
            if not E and not cb:
                continue
            print(f"  {c:6s} s{s:<3d} {a:4s}"
                  + (f" E={st.mean(E):5.3f}" if E else " E=  -  ")
                  + (f" short={100*st.mean(sh):5.1f}%" if sh else "")
                  + (f" zero={100*st.mean(ze):5.1f}%" if ze else "")
                  + (f" capboot={100*st.mean(cb):5.1f}%" if cb else "")
                  + (f" occcap_p50={st.mean(oc):6.0f}" if oc else "")
                  + (f" paused={st.mean(pa):3.0f}%" if pa else "")
                  + (f" retx={st.mean(rx):6.0f}" if rx else ""))

print()
print("=" * 100)
print("5b. BIND FRACTION -- is the store-cap LAW varying, or is it a CONSTANT?")
print("=" * 100)
print("  (ADR-0070 prevention kit item 2. Section 5's occcap_p50 is the cap the")
print("   transfer RAN UNDER; this asks how often that number was one of the")
print("   chain's own clamps, i.e. how often the law contributed nothing. The")
print("   0.5 warn level is a REPORTING AID -- no clause below reads it.)")
print_capbind(scored)

print()
print("=" * 100)
print("6. DELIVERED PROBE LATENCY (independent ICMP flow, ms)")
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
print("7. PRE-REGISTERED CLAUSES, SCORED (per seed; the goal-gate block adjudicates)")
print("=" * 100)
for s in SEEDS:
    print(f"  -- seed {s} --")
    A8, AU8 = col(("c8", "A", s), "mbps"), col(("c8", "AU", s), "mbps")
    if A8 and AU8:
        r = st.mean(AU8) / st.mean(A8)
        k = cmp2s(A8, AU8)
        down = k and k[2] and st.mean(AU8) < st.mean(A8)
        print(f"  U1  c8 AU/A = {r:.3f} (need >= 0.95, not >2sig down)"
              f" -> {'PASS' if r >= 0.95 and not down else 'FAIL'}")
        print(f"      c8 AU per-rep: {sorted(round(x,1) for x in AU8)}")
    A1, AU1 = col(("c1", "A", s), "mbps"), col(("c1", "AU", s), "mbps")
    if A1 and AU1:
        k = cmp2s(A1, AU1)
        up = st.mean(AU1) > st.mean(A1)
        print(f"  U2  c1 AU-A = {st.mean(AU1)-st.mean(A1):+.1f}"
              f" ({st.mean(AU1)/st.mean(A1):.3f}x), |d| {k[0]:.1f} vs 2sig {k[1]:.1f}"
              f" -> {'PASS' if up and k[2] else 'FAIL'}")
    A2c, AU2c = col(("sc2", "A", s), "mbps"), col(("sc2", "AU", s), "mbps")
    if A2c and AU2c:
        k = cmp2s(A2c, AU2c)
        print(f"  U3  sc2 AU/A = {st.mean(AU2c)/st.mean(A2c):.3f}"
              f" (parity: within 2sig) -> {'PASS' if not k[2] else 'FAIL'}")
        pA = col(("sc2", "A", s), "ping_p50")
        pU = col(("sc2", "AU", s), "ping_p50")
        if pA and pU:
            kp = cmp2s(pA, pU)
            worse = kp and kp[2] and st.mean(pU) > st.mean(pA)
            print(f"      sc2 probe p50 A={st.mean(pA):.1f} AU={st.mean(pU):.1f}"
                  f" -> {'FAIL (worse >2sig)' if worse else 'PASS'}")
    for c in ("c1", "c7"):
        Ac_, AUc_ = cpb((c, "A", s)), cpb((c, "AU", s))
        if Ac_ and AUc_:
            r = st.mean(AUc_) / st.mean(Ac_)
            print(f"  U4  {c} AU CPU/byte = {r:.3f}x A (need <= 1.05)"
                  f" -> {'PASS' if r <= 1.05 else 'FAIL'}")
    A7, AU7 = col(("c7", "A", s), "mbps"), col(("c7", "AU", s), "mbps")
    if A7 and AU7:
        k = cmp2s(A7, AU7)
        down = k and k[2] and st.mean(AU7) < st.mean(A7)
        print(f"  U5  c7 AU/A = {st.mean(AU7)/st.mean(A7):.3f}"
              f" (no >2sig regression) -> {'PASS' if not down else 'FAIL'}")
    AL8, ALU8 = col(("c8", "AL", s), "mbps"), col(("c8", "ALU", s), "mbps")
    if AL8 and ALU8:
        r = st.mean(ALU8) / st.mean(AL8)
        k = cmp2s(AL8, ALU8)
        harm = (k and k[2] and st.mean(ALU8) < st.mean(AL8)) or r <= 0.85
        coll = sorted(x for x in ALU8 if x < 60)
        print(f"  ATTR c8 ALU/AL = {r:.3f} (harm class: >2sig down or <=0.85,"
              f" collapse reps <60: {coll if coll else 'none'})"
              f" -> {'HARM REPRODUCED' if harm else 'NOT REPRODUCED (era question)'}")
        print(f"      c8 ALU per-rep: {sorted(round(x,1) for x in ALU8)}")
    for c in CELLS:
        Ax, ALx = col((c, "A", s), "mbps"), col((c, "AL", s), "mbps")
        if Ax and ALx:
            k = cmp2s(Ax, ALx)
            print(f"  ERA {c} AL/A = {st.mean(ALx)/st.mean(Ax):.3f}"
                  f" {'INSTRUMENT ALARM (>2sig on value-identical arm)' if k and k[2] else 'within 2sig'}")
    RU1, AU1b = col(("c1", "RU", s), "mbps"), col(("c1", "AU", s), "mbps")
    if RU1 and AU1b:
        k = cmp2s(AU1b, RU1)
        print(f"  C1RD c1 RU/AU = {st.mean(RU1)/st.mean(AU1b):.3f}"
              f" |d|={k[0]:.1f} vs 2sig={k[1]:.1f}"
              f" -> {'PARITY' if not k[2] else 'NOT AT PARITY'}"
              f"  (RU/A = {st.mean(RU1)/st.mean(A1):.3f})" if A1 else "")
