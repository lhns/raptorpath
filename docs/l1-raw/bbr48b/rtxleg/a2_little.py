# POST HOC. Little's law on the gap horizon, per (row, 2 s bin) in the main phase (src > 0, t >= 2 s):
#   N_h = mean gap seqs visited per processed report (the horizon's occupancy: first MAX_NACK_GAPS ranges),
#   mu  = SACK-gap fires/s (fires ~= losses at c8, so ~one fire per hole),
#   T_h = N_h / mu = mean time a hole stays in the horizon (fire -> the sender's report no longer lists it).
# Per-leg clocks c0, c1 = estimator app-echo rtt per leg (median of ticks in bin), C = pooled clock, s1 = share of
# the bin's fires SENT on leg1. Fits (least squares, through origin unless noted):
#   M_pool: T_h = a*C        M_leg: T_h = a*(s1*c1 + (1-s1)*c0)        M_free: T_h = a*c0 + b*s1*(c1-c0)
# A send-leg effect shows as M_leg beating M_pool and b > 0 in M_free (b ~ a means a slow-leg retx holds its
# slot for the slow clock). Also a within-time-bin check: rows with high vs low s1 at matched clocks.
import sys, statistics as st
sys.path.insert(0, sys.argv[1]); from parse import rows
D = sys.argv[2]; cell = sys.argv[3]; BW = float(sys.argv[4]) if len(sys.argv) > 4 else 2.0

def lsq(X, y):
    # tiny normal-equation solver
    n = len(X[0]); A = [[sum(x[i] * x[j] for x in X) for j in range(n)] for i in range(n)]
    b = [sum(x[i] * yy for x, yy in zip(X, y)) for i in range(n)]
    for i in range(n):
        p = A[i][i]
        for j in range(i + 1, n):
            f = A[j][i] / p
            A[j] = [a - f * c for a, c in zip(A[j], A[i])]; b[j] -= f * b[i]
    x = [0.0] * n
    for i in reversed(range(n)):
        x[i] = (b[i] - sum(A[i][j] * x[j] for j in range(i + 1, n))) / A[i][i]
    res = [yy - sum(c * v for c, v in zip(x, xx)) for xx, yy in zip(X, y)]
    my = st.mean(y); sst = sum((yy - my) ** 2 for yy in y)
    return x, 1 - sum(r * r for r in res) / sst, (sum(r * r for r in res) / len(y)) ** 0.5

obs = {}
for r in rows(D, cell):
    T = r["T"]
    ise = max(i for i, x in enumerate(T) if x["src"] > 0)
    tse = T[ise]["t"]
    nb = int(tse // BW)
    for b in range(1, nb):
        idx = [i for i in range(ise + 1) if b * BW <= T[i]["t"] < (b + 1) * BW]
        if len(idx) < 3: continue
        i0, i1 = idx[0], idx[-1]; dt = T[i1]["t"] - T[i0]["t"]
        d = lambda k: T[i1][k] - T[i0][k]
        if d("rep") < 5 or d("retx") < 20: continue
        Nh = d("seqs") / d("rep"); mu = d("retx") / dt
        c0 = st.median(T[i]["c0"] for i in idx) / 1e3; c1 = st.median(T[i]["c1"] for i in idx) / 1e3
        C = st.median(T[i]["C"] for i in idx) / 1e3
        on0, on1 = d("on0"), d("on1"); s1 = on1 / max(1, on0 + on1)
        ro1 = d("ro1") / max(1, d("ro0") + d("ro1")) if "ro1" in T[i1] else 0
        obs.setdefault(r["arm"], []).append(dict(row=r["row"], b=b, Nh=Nh, Nh_live=(d("seqs") - d("stale")) / d("rep"), mu=mu,
            Th=Nh / mu, c0=c0, c1=c1, C=C, s1=s1, ro1=ro1, supp=d("scool") / max(1, d("seqs")), fpr=d("retx") / d("rep")))
for arm, v in sorted(obs.items()):
    y = [o["Th"] for o in v]
    (a,), R2p, ep = lsq([[o["C"]] for o in v], y)
    (al,), R2l, el = lsq([[o["s1"] * o["c1"] + (1 - o["s1"]) * o["c0"]] for o in v], y)
    (af, bf), R2f, ef = lsq([[o["c0"], o["s1"] * (o["c1"] - o["c0"])] for o in v], y)
    (k0, k1, k2), R2c, ec = lsq([[1, o["C"], o["s1"]] for o in v], y)
    m = lambda k: st.mean(o[k] for o in v)
    print(f"== {cell} {arm}: n={len(v)} (row x {BW:.0f}s bins, main phase)  means: N_h={m('Nh'):.1f} (live {m('Nh_live'):.1f}) mu={m('mu'):.0f}/s T_h={m('Th')*1e3:.0f}ms "
          f"C={m('C')*1e3:.0f} c0={m('c0')*1e3:.0f} c1={m('c1')*1e3:.0f} ms s1(sent on leg1)={m('s1'):.2f} orig-leg1 share={m('ro1'):.2f} cool-supp/visit={m('supp'):.3f} fires/report={m('fpr'):.2f}")
    print(f"   M_pool T_h={a:.2f}*C            R2={R2p:.3f} rmse={ep*1e3:.0f}ms")
    print(f"   M_leg  T_h={al:.2f}*(s1 c1+(1-s1)c0) R2={R2l:.3f} rmse={el*1e3:.0f}ms")
    print(f"   M_free T_h={af:.2f}*c0 + {bf:.2f}*s1*(c1-c0)  R2={R2f:.3f} rmse={ef*1e3:.0f}ms")
    print(f"   M_ctl  T_h={k0*1e3:.0f}ms + {k1:.2f}*C + {k2*1e3:.0f}ms*s1  R2={R2c:.3f}   (s1 coefficient at fixed pooled clock)")
    if len(sys.argv) > 5:
        for o in sorted(v, key=lambda o: (o["b"], o["row"])):
            print(f"     {o['row']} b{o['b']:2d} Nh={o['Nh']:5.1f} mu={o['mu']:4.0f} Th={o['Th']*1e3:4.0f} C={o['C']*1e3:4.0f} c0={o['c0']*1e3:4.0f} c1={o['c1']*1e3:4.0f} s1={o['s1']:.2f}")
