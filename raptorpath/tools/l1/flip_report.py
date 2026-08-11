#!/usr/bin/env python3
"""Report for the HONEST-INPUTS FLIP battery (goal-gate "Honest Inputs —
FLIP BATTERY — PRE-REGISTRATION").

    flip_report.py <log> [<log> ...]

Scrapes `FLIPRESULT ` lines and scores them against the PRE-REGISTERED
falsifiers, never against a number chosen afterwards:

  CTRL DH/A at c1 must REPRODUCE the -13% (0.84-0.90 class), both seeds,
       else the session reports an era-stability finding and cannot
       attribute the U-arm response to the store-cap mechanism chain.
  F1   c1: BHU/A - 1 > 2 sigma_Delta on BOTH seeds, else no flip for the
       RWM_THREE_TERM family.
  F2   c1: DHU/A inside 0.95-1.02 on both seeds (H1's completion).
  F3   mechanism gauge, two prongs: (a) the paired non-U arm's [SF]
       zero-tick fraction >= 20% at c1 (the trap is live); (b) the U-arm's
       cap-at-boot fraction < 5% where the paired arm reads >= 20%. If the
       cap gauge does not move, NO goodput number is attributed to the
       path-set fix.
  F4   CPU: DHU/BHU sender CPU/byte <= 1.05x A at c1/c7.
  F5   regressions: any cell > 2 sigma down on either seed in a candidate
       arm (H, DHU, BHU) denies that arm's flip. c8 is the pre-named risk.
  F6   sc2: BHU probe p50 <= 55 ms AND > 2 sigma below A, at goodput
       within 2 sigma.
  F7   H (RWM_HONEST_ANCHOR alone): goodput within 2 sigma of A and CPU
       within 2 sigma (point <= 1.05x) at EVERY cell, both seeds; any
       goodput movement beyond 2 sigma is an INSTRUMENT ALARM, not a
       result (the gate is value-identical by construction).
  jit25 parity in every arm + the [3T] limit in RELATION form only:
       per-rep slack/window inside [2.0, 2.3] on BH/BHU, BHU limit within
       2 sigma of same-session BH. No absolute band.
"""
import json
import statistics as st
import sys
from collections import defaultdict

CAP = {"c1": 1000e6, "jit25": 100e6, "sc2": 100e6, "c7": 200e6, "c8": 120e6}
ORDER = ["c1", "jit25", "sc2", "c7", "c8"]
ARMS = ["A", "H", "DH", "DHU", "BH", "BHU"]
EXPECT = {  # arm -> (3t, rs, ha, hk, u)
    "A": (0, 0, 0, 0, 0), "H": (0, 0, 1, 0, 0),
    "DH": (0, 1, 1, 0, 0), "DHU": (0, 1, 1, 0, 1),
    "BH": (1, 1, 1, 1, 0), "BHU": (1, 1, 1, 1, 1),
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
                      (e[3], "active_hk_cli", "active_hk_srv"),
                      (e[4], "active_u_cli", "active_u_srv")):
        if bit == 1 and (r[c] == 0 or r[s] == 0):
            return False
        if bit == 0 and (r[c] > 0 or r[s] > 0):
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
for k in sorted(ab):
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
print("3. GOODPUT (Mbit/s) + sender CPU (CPUCLI s; fixed bytes per cell)")
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
print("\n  sender CPU seconds (ratios between arms are CPU/byte ratios):")
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
print("4. RATIOS vs A (goodput, sender-CPU/byte), 2 sigma_pooled verdicts")
print("=" * 100)
for c in CELLS:
    for s in SEEDS:
        A = col((c, "A", s), "mbps")
        Ac = cpb((c, "A", s))
        if not A:
            continue
        for a in ["H", "DH", "DHU", "BH", "BHU"]:
            X = col((c, a, s), "mbps")
            Xc = cpb((c, a, s))
            if not X:
                continue
            k = cmp2s(A, X)
            verd = (f"|d|={k[0]:6.1f} vs 2sig={k[1]:6.1f} {'EXCEEDS' if k[2] else 'within'}"
                    if k else "-")
            cr = (st.mean(Xc) / st.mean(Ac)) if Ac and Xc else None
            crs = f"  CPU/byte={cr:5.3f}x" if cr else ""
            print(f"  {c:6s} s{s:<3d} {a:3s}/A goodput={st.mean(X)/st.mean(A):6.3f}  {verd}{crs}")

print()
print("=" * 100)
print("5. F3 -- THE MECHANISM GAUGE: [SF] population + the consumed cliff, per arm")
print("=" * 100)
print("  ([SF] zero% counts active_paths() EMPTY at the 5 ms refresh — the filter;")
print("   capboot% counts steady DIAG samples with effective cap <= 128 — the")
print("   CONSUMED cliff. The filter gauge is downstream-coupled to its consumer,")
print("   so the U-arms are scored on capboot%, the non-U arms on either.)")
for c in CELLS:
    for s in SEEDS:
        for a in ARMS:
            key = (c, a, s)
            E = col(key, "sf_E")
            sh = col(key, "sf_short")
            ze = col(key, "sf_zero")
            cb = col(key, "capboot_frac")
            oc = col(key, "occcap_p50")
            if not E and not cb:
                continue
            print(f"  {c:6s} s{s:<3d} {a:4s} E={st.mean(E):5.3f}" if E else
                  f"  {c:6s} s{s:<3d} {a:4s} E=  -  ", end="")
            print(f" short={100*st.mean(sh):5.1f}%" if sh else " short=  -  ", end="")
            print(f" zero={100*st.mean(ze):5.1f}%" if ze else " zero=  -  ", end="")
            print(f" capboot={100*st.mean(cb):5.1f}%" if cb else " capboot=  -  ", end="")
            print(f" occcap_p50={st.mean(oc):6.0f}({len(oc)})" if oc else "")

print()
print("=" * 100)
print("6. jit25 RELATION FORM -- [3T] cap/window/slack, slack/window, khr/kraw/rtp")
print("=" * 100)
for c in CELLS:
    for s in SEEDS:
        for a in ARMS:
            key = (c, a, s)
            cm, csd, cn = ms(col(key, "cap_med"))
            wm, _, _ = ms(col(key, "window_med"))
            sm, _, _ = ms(col(key, "slack_med"))
            rm_, _, _ = ms(col(key, "swratio_med"))
            km, _, kn = ms(col(key, "khr_med"))
            krm, _, krn = ms(col(key, "kraw_med"))
            rtp, _, _ = ms(col(key, "rtp_med"))
            if cm is None and km is None:
                continue
            print(f"  {c:6s} s{s:<3d} {a:4s} cap={cm and round(cm) or '-':>5}"
                  f"+-{csd and round(csd) or 0:>4}({cn})"
                  f" window={wm and round(wm) or '-':>5} slack={sm and round(sm) or '-':>5}"
                  f" sl/win={rm_ and round(rm_,3) or '-':>6}"
                  f" khr={km and round(km,3) or '-':>6}({kn})"
                  f" kraw={krm and round(krm,3) or '-':>6}({krn})"
                  f" rtp={rtp and round(rtp) or '-':>4}ms")
# BHU limit vs same-session BH (the U-composition tripwire)
for s in SEEDS:
    a = col(("jit25", "BH", s), "cap_med")
    b = col(("jit25", "BHU", s), "cap_med")
    k = cmp2s(a, b)
    if k:
        print(f"  jit25 s{s} BHU-vs-BH limit |d|={k[0]:.0f} vs 2sig={k[1]:.0f}"
              f" {'EXCEEDS (instrument alarm)' if k[2] else 'within'}")

print()
print("=" * 100)
print("7. F6 -- DELIVERED PROBE LATENCY (sc2), independent ICMP flow, ms")
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
print("8. WAIT ATTRIBUTION (mechanism corroboration: paused = the cliff's shadow)")
print("=" * 100)
for c in CELLS:
    for s in SEEDS:
        for a in ARMS:
            key = (c, a, s)
            tun = ms(col(key, "wait_tun"))[0]
            pau = ms(col(key, "wait_paused"))[0]
            if tun is None:
                continue
            oc = ms(col(key, "occ_p50"))[0]
            print(f"  {c:6s} s{s:<3d} {a:4s} wait[tun={tun:3.0f}% paused={pau:3.0f}%]"
                  f" occ_p50={oc:6.0f}" if oc is not None else
                  f"  {c:6s} s{s:<3d} {a:4s} wait[tun={tun:3.0f}% paused={pau:3.0f}%]")
