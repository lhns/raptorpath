# POST HOC model. (a) Per-row slopes: within each row, OLS over its 2 s main-phase bins of T_h = al + be*s1*(c1-c0) + ga*c0
#     (be = 1: a slow-leg retx holds its horizon slot for the full slow-leg clock excess). 12 rows -> mean +- se, sign count.
# (b) Counterfactual sensitivity: per-leg hold model T_q = c0 + k*[q=1]*(c1-c0), k in {0.6, 0.8, 1.0};
#     mu' = mu * T(s1)/T(s1') with s1' = original-leg1 share (retx stay on original leg) or 0 (all retx on leg0);
#     mu' capped at 10 x reports/s of the bin (the per-report quantum). Backlog integrated as in a3_cf.py.
import sys, statistics as st
sys.path.insert(0, sys.argv[1]); from parse import rows
D = sys.argv[2]; cell = sys.argv[3]; BW = 2.0
def ols(X, y):
    n = len(X[0]); A = [[sum(x[i]*x[j] for x in X) for j in range(n)] for i in range(n)]; b = [sum(x[i]*yy for x, yy in zip(X, y)) for i in range(n)]
    for i in range(n):
        for j in range(i+1, n):
            f = A[j][i]/A[i][i]; A[j] = [p-f*q for p, q in zip(A[j], A[i])]; b[j] -= f*b[i]
    c = [0.0]*n
    for i in reversed(range(n)): c[i] = (b[i]-sum(A[i][j]*c[j] for j in range(i+1, n)))/A[i][i]
    return c
slopes = {}; out = {}
for r in rows(D, cell):
    T = r["T"]; arm = r["arm"]
    ise = max(i for i, x in enumerate(T) if x["src"] > 0); tse = T[ise]["t"]
    tl = [i for i in range(ise+1, len(T)) if T[i]["t"] <= r["secs"]]
    H0m = T[tl[0]]["win"]; rho = H0m / max(1, T[ise]["cs"] - T[ise]["ca"])
    mut = (T[tl[-1]]["retx"] - T[ise]["retx"]) / max(0.25, T[tl[-1]]["t"] - tse)
    bins = []; prevHm = 0.0
    for b in range(0, int(tse // BW) + 1):
        idx = [i for i in range(ise+1) if b*BW <= T[i]["t"] < (b+1)*BW]
        if len(idx) < 2: continue
        i0, i1 = idx[0], idx[-1]; dt = T[i1]["t"] - T[i0]["t"]
        if dt <= 0: continue
        d = lambda k: T[i1][k] - T[i0][k]
        mu = d("retx")/dt; Hm = rho*(T[i1]["cs"]-T[i1]["ca"]); lam = mu + (Hm-prevHm)/dt; prevHm = Hm
        ok = d("rep") >= 5 and d("retx") >= 20
        c0 = st.median(T[i]["c0"] for i in idx)/1e3; c1 = st.median(T[i]["c1"] for i in idx)/1e3
        bins.append(dict(b=b, dt=dt, mu=mu, lam=lam, ok=ok, reps=d("rep")/dt, c0=c0, c1=c1,
            s1=d("on1")/max(1, d("on0")+d("on1")), so=d("ro1")/max(1, d("ro0")+d("ro1")),
            Th=(d("seqs")/d("rep"))/mu if ok else 0))
    mb = [x for x in bins if x["ok"] and x["b"] >= 1]
    be = ols([[1, x["s1"]*(x["c1"]-x["c0"]), x["c0"]] for x in mb], [x["Th"] for x in mb])[1]
    slopes.setdefault(arm, []).append(be)
    for k in (0.6, 0.8, 1.0):
        for lab in ("orig", "leg0"):
            H = 0.0
            for x in bins:
                mu2 = x["mu"]
                if x["ok"]:
                    s2 = x["so"] if lab == "orig" else 0.0
                    Tm = x["c0"] + k*x["s1"]*(x["c1"]-x["c0"]); Tp = x["c0"] + k*s2*(x["c1"]-x["c0"])
                    mu2 = min(x["mu"]*Tm/Tp, max(x["mu"], 10*x["reps"]))
                H = max(0.0, H + (x["lam"]-mu2)*x["dt"])
            out.setdefault((arm, k, lab), []).append((H0m, H, max(0.15, H/mut), r["secs"]-tse))
for arm in ("BASE", "CAND"):
    v = slopes[arm]; m = st.mean(v); se = st.stdev(v)/len(v)**0.5
    print(f"== {cell} {arm}: per-row slope be = {m:.2f} +- {se:.2f} (se over 12 rows), positive in {sum(1 for x in v if x > 0)}/12; rows: " + " ".join(f"{x:.2f}" for x in v))
    for lab in ("orig", "leg0"):
        for k in (0.6, 0.8, 1.0):
            o = out[(arm, k, lab)]
            print(f"   cf retx on {'ORIGINAL leg' if lab=='orig' else 'leg0 always '} k={k}: H0 {st.mean(x[0] for x in o):.0f} -> {st.mean(x[1] for x in o):.0f} "
                  f"(rows min {min(x[1] for x in o):.0f} max {max(x[1] for x in o):.0f}); tail {st.mean(x[3] for x in o):.2f}s -> {st.mean(x[2] for x in o):.2f}s")
