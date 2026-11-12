# POST HOC pricing model (not a measurement). Per arm, 4 s bins, means over 12 rows:
#   measured: mu = retx/s, C = pooled clock (max over legs of app-echo rtt), c0/c1 per-leg app-echo rtt, s1 = share of retx placed on leg1,
#             H = rho * frontier lag (rho = H0/lag0 per arm, measured), lambda = mu + dH/dt (hole arrival implied by the balance).
#   law under test: mu = K / C_eff (K = mu*C measured per bin).
#   per-flight-path clock: C_eff = s1*c1 + (1-s1)*c0 (live-flight path; the cooldown/ripeness of a seq runs on the clock of the path its
#             live flight is on), variant O: C_eff = 0.24*c1 + 0.76*c0 (original path shares from [C8CONV-S] ro=).
#   lever B: K x 64/20.  H'(t) integrated bin by bin from 0, floored at 0.  tail' = H0'/mu_tail (mu_tail measured, unchanged: lever A parked),
#   floored at the measured c2-class tail 0.15 s.
import os, re, sys, statistics as st
D = sys.argv[1]
def I(p, l):
    m = re.search(p, l); return int(m.group(1)) if m else 0
rows = {}
for f in sorted(os.listdir(D)):
    if not (f.startswith("c8-100@") and f.endswith("-c.log")): continue
    arm = re.search(r"def-(\w+)-s", f).group(1)
    L = [l for l in open(os.path.join(D, f), errors="replace") if l.startswith("[DIAG]")]
    T = [float(re.search(r"t=([0-9.]+)s", l).group(1)) for l in L]
    for b in range(11):
        idx = [i for i in range(len(L)) if 4 * b <= T[i] < 4 * b + 4 and I(r"src=(\d+)sym", L[i]) > 0]
        if len(idx) < 2: continue
        i0, i1 = idx[0], idx[-1]; dt = T[i1] - T[i0]
        d = lambda p: I(p, L[i1]) - I(p, L[i0])
        s, c, a = map(int, re.search(r"cum=(\d+)/(\d+)/(\d+)", L[i1]).groups())
        legs = [{int(p): int(x) for p, x in re.findall(r" p(\d+):infl=.*? rtt=(\d+)/wrtt=", L[i])} for i in idx]
        on0, on1 = d(r"fp/on p0:\d+/(\d+)"), d(r"p1:\d+/(\d+)\]")
        rows.setdefault((arm, b), []).append(dict(lag=s - a, mu=d(r"retx=(\d+)") / dt,
            C=st.median(float(re.search(r" rtt=([0-9.]+)ms", L[i]).group(1)) for i in idx) / 1e3,
            c0=st.median(x[0] for x in legs if len(x) == 2) / 1e3, c1=st.median(x[1] for x in legs if len(x) == 2) / 1e3,
            s1=on1 / max(1, on0 + on1)))
RHO = {"BASE": 1890 / 73179, "CAND": 2246 / 83189}; MUT = {"BASE": 498.0, "CAND": 501.0}
for arm in ("BASE", "CAND"):
    print(f"== {arm}  (rho={RHO[arm]:.4f} holes/lag-symbol, mu_tail={MUT[arm]:.0f}/s)")
    H = {k: 0.0 for k in ("ship", "pf", "pfO", "B", "B+pf")}; Hprev_meas = 0.0
    print("  bin     C    c0    c1   s1     K  | K/C  K/Cpf K/CpfO | lambda | H_meas  H_ship  H_pf  H_pfO  H_B  H_B+pf")
    for b in range(11):
        rr = rows.get((arm, b))
        if not rr: continue
        m = lambda k: st.mean(r[k] for r in rr)
        C, c0, c1, s1, mu = m("C"), m("c0"), m("c1"), m("s1"), m("mu")
        K = mu * C; Cpf = s1 * c1 + (1 - s1) * c0; CpfO = 0.24 * c1 + 0.76 * c0
        Hm = RHO[arm] * m("lag"); lam = mu + (Hm - Hprev_meas) / 4.0; Hprev_meas = Hm
        mus = dict(ship=K / C, pf=K / Cpf, pfO=K / CpfO, B=3.2 * K / C, **{"B+pf": 3.2 * K / Cpf})
        for k in H: H[k] = max(0.0, H[k] + (lam - mus[k]) * 4.0)
        print(f"  {4*b:2d}-{4*b+4:2d}s {C:.3f} {c0:.3f} {c1:.3f} {s1:.2f} {K:5.0f} | {K/C:4.0f} {K/Cpf:5.0f} {K/CpfO:5.0f}  | {lam:5.0f}  | {Hm:6.0f} {H['ship']:6.0f} {H['pf']:6.0f} {H['pfO']:6.0f} {H['B']:5.0f} {H['B+pf']:5.0f}")
    print("  predicted tail (H0/mu_tail, floor 0.15 s): " + "  ".join(f"{k}={max(0.15, H[k]/MUT[arm]):.2f}s(H0={H[k]:.0f})" for k in H))
