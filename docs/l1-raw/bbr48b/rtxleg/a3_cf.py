# POST HOC model, not a measurement. (1) Within-time-bin check of the send-leg effect: in each 2 s bin, rows are
# demeaned (T_h, s1, C, c0, c1 minus the bin's cross-row mean), so the common time profile is removed; the slope of
# T_h on s1*(c1-c0) at fixed C is the send-leg effect identified from row-to-row variation only.
# (2) Counterfactual "retransmits stay on the original leg": per row and 2 s bin, T_h' = s1'*c1 + (1-s1')*c0 with
# s1' = the bin's measured share of fires whose ORIGINAL was on leg1 (ro deltas), and T_h scaled by the fitted
# M_leg coefficient a (T_h = a*(s1 c1 + (1-s1) c0)); mu' = N_h / T_h' (horizon occupancy N_h unchanged).
# Backlog: H(t) measured = rho * frontier lag (rho = H0/lag0 per row at source end); arrivals lambda = mu + dH/dt;
# H'(t) integrated with mu', floored at 0. Predicted H0' and tail' = H0'/mu_tail (mu_tail measured, unchanged).
# Clocks c0, c1 are held at their measured values (leg1 sheds ~10 % of its datagrams, leg0 gains ~1.5 %: second order, unpriced).
import sys, statistics as st
sys.path.insert(0, sys.argv[1]); from parse import rows
D = sys.argv[2]; cell = sys.argv[3]; BW = 2.0; A = {"BASE": 0.94, "CAND": 0.98}
dm = {}; cf = {}
for r in rows(D, cell):
    T = r["T"]; arm = r["arm"]
    ise = max(i for i, x in enumerate(T) if x["src"] > 0); tse = T[ise]["t"]
    tl = [i for i in range(ise + 1, len(T)) if T[i]["t"] <= r["secs"]]
    H0m = T[tl[0]]["win"] if tl else 0
    lag0 = T[ise]["cs"] - T[ise]["ca"]; rho = H0m / max(1, lag0)
    mut = (T[tl[-1]]["retx"] - T[ise]["retx"]) / max(0.25, T[tl[-1]]["t"] - tse) if tl else 500
    H = 0.0; Hp = 0.0; prevHm = 0.0
    for b in range(0, int(tse // BW) + 1):
        idx = [i for i in range(ise + 1) if b * BW <= T[i]["t"] < (b + 1) * BW]
        if len(idx) < 2: continue
        i0, i1 = idx[0], idx[-1]; dt = T[i1]["t"] - T[i0]["t"]
        if dt <= 0: continue
        d = lambda k: T[i1][k] - T[i0][k]
        mu = d("retx") / dt
        Hm = rho * (T[i1]["cs"] - T[i1]["ca"]); lam = mu + (Hm - prevHm) / dt; prevHm = Hm
        if d("rep") < 5 or d("retx") < 20:
            Hp = max(0.0, Hp + (lam - mu) * dt); continue
        Nh = d("seqs") / d("rep")
        c0 = st.median(T[i]["c0"] for i in idx) / 1e3; c1 = st.median(T[i]["c1"] for i in idx) / 1e3
        C = st.median(T[i]["C"] for i in idx) / 1e3
        s1 = d("on1") / max(1, d("on0") + d("on1")); so = d("ro1") / max(1, d("ro0") + d("ro1"))
        Th = Nh / mu
        Thp = A[arm] * (so * c1 + (1 - so) * c0); Thm = A[arm] * (s1 * c1 + (1 - s1) * c0)
        mup = mu * Thm / Thp      # scale the measured service by the model's ratio (keeps the measured level)
        Hp = max(0.0, Hp + (lam - mup) * dt)
        if b >= 1: dm.setdefault((arm, b), []).append(dict(row=r["row"], Th=Th, x=s1 * (c1 - c0), C=C, c0=c0))
    cf.setdefault(arm, []).append(dict(row=r["row"], H0=H0m, H0p=Hp, mut=mut, tail=r["secs"] - tse, Hend=prevHm))
for arm in ("BASE", "CAND"):
    X, Y = [], []
    for (a, b), v in dm.items():
        if a != arm or len(v) < 4: continue
        mT = st.mean(o["Th"] for o in v); mx = st.mean(o["x"] for o in v); mC = st.mean(o["C"] for o in v); m0 = st.mean(o["c0"] for o in v)
        for o in v: X.append((o["x"] - mx, o["C"] - mC, o["c0"] - m0)); Y.append(o["Th"] - mT)
    # 3-regressor OLS via normal equations (no intercept: demeaned)
    n = 3; M = [[sum(x[i] * x[j] for x in X) for j in range(n)] for i in range(n)]; v_ = [sum(x[i] * y for x, y in zip(X, Y)) for i in range(n)]
    import copy; Mi = copy.deepcopy(M); bb = v_[:]
    for i in range(n):
        for j in range(i + 1, n):
            f = Mi[j][i] / Mi[i][i]; Mi[j] = [p - f * q for p, q in zip(Mi[j], Mi[i])]; bb[j] -= f * bb[i]
    co = [0.0] * n
    for i in reversed(range(n)): co[i] = (bb[i] - sum(Mi[i][j] * co[j] for j in range(i + 1, n))) / Mi[i][i]
    res = [y - sum(c * xx for c, xx in zip(co, x)) for x, y in zip(X, Y)]
    s2 = sum(e * e for e in res) / (len(Y) - n)
    # standard error of the first coefficient: sqrt(s2 * (M^-1)[0][0]) via cofactor
    def det3(m): return (m[0][0]*(m[1][1]*m[2][2]-m[1][2]*m[2][1]) - m[0][1]*(m[1][0]*m[2][2]-m[1][2]*m[2][0]) + m[0][2]*(m[1][0]*m[2][1]-m[1][1]*m[2][0]))
    inv00 = (M[1][1] * M[2][2] - M[1][2] * M[2][1]) / det3(M)
    print(f"== {cell} {arm} within-bin (row-demeaned, n={len(Y)}): T_h slope on s1*(c1-c0) = {co[0]:.2f} +- {(s2*inv00)**0.5:.2f} (1.0 = slow-leg retx holds its slot for the full slow-leg clock; 0 = no send-leg effect); on C {co[1]:.2f}, on c0 {co[2]:.2f}")
    v = cf[arm]; m = lambda k: st.mean(o[k] for o in v)
    print(f"   counterfactual (retx stay on original leg): H0 measured {m('H0'):.0f} (rho*lag model end {m('Hend'):.0f}) -> model {m('H0p'):.0f}; tail measured {m('tail'):.2f}s -> H0'/mu_tail {st.mean(max(0.15, o['H0p']/o['mut']) for o in v):.2f}s (floor 0.15 s)")
    for o in sorted(v, key=lambda o: o["row"]):
        print(f"     {o['row']}: H0 {o['H0']} -> {o['H0p']:.0f}  tail {o['tail']:.2f} -> {max(0.15, o['H0p']/o['mut']):.2f}")
