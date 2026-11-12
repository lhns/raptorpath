# POST HOC, not registered. Follow-ups: (a) receiver-task busy in the c8 tail (all [RDIAG] lines, split by msgs/s);
# CI of the within-arm CPU-on-tail slope. (b) tail-line state per arm/leg: standing B, infl vs qcwnd, dtx/line,
# UNDER/R2a timing vs tail; main-phase qcwnd/BDP BASE vs CAND; where R2a-only lines sit.
import os, re, sys, statistics as st
import numpy as np
D = sys.argv[1]; L = os.path.join(D, "full/diag-ld")
LINK = [12.5e6, 2.5e6]; BDP = [125000, 100000]
out = []
def P(s=""): out.append(s); print(s)
rows = []
for f in sorted(os.listdir(L)):
    if not (f.startswith("c8-100@") and f.endswith("-c.log")): continue
    arm, seed, rep = re.search(r"def-(\w+)-s(\d+)-r(\d+)-", f).groups()
    drv = open(os.path.join(L, f.replace("-c.log", "-drv.out")), errors="replace").read()
    secs = float(re.search(r'"median_s":([0-9.]+)', drv).group(1))
    cpu_srv = float(re.search(r"CPUSRV=([0-9.]+)s", drv).group(1))
    qcc, dl = [], []
    for ln in open(os.path.join(L, f), errors="replace"):
        m = re.match(r"\[QCC\] t=([0-9.]+)s", ln)
        if m: qcc.append((float(m.group(1)), {int(a): (b, int(c)) for a, b, c in re.findall(r"p(\d+):qm=(\S) qrt_us=\d+ qsr_us=\d+ qbw=(\d+)", ln)}))
        m = re.match(r"\[DIAG\] t=([0-9.]+)s .*? src=(\d+)sym/s", ln)
        if m:
            legs = {}
            trip = {int(a): (int(b), int(c)) for a, b, c in re.findall(r"dgq(\d+)\[hand=(\d+) tx=(\d+)", ln)}
            for part in re.split(r"(?= p\d+:infl=)", ln)[1:]:
                g_ = re.match(r" p(\d+):infl=(\d+)", part); q_ = re.search(r"\bqcwnd=(\d+)", part)
                if g_:
                    i = int(g_.group(1))
                    legs[i] = dict(infl=int(g_.group(2)), cw=int(q_.group(1)) if q_ else None,
                                   B=trip[i][0] - trip[i][1] if i in trip else None, tx=trip[i][1] if i in trip else None)
            dl.append((float(m.group(1)), int(m.group(2)), legs))
    tse = max(t for t, s, _ in dl if 1.0 <= t <= secs and s > 0)
    # receiver task: all [RDIAG] lines of the transfer object (msgs>=50/s), split at msgs/s 3000 (main ~10.7k/s, tail ~0.4-0.7k/s)
    rd = [(int(a), int(b)) for a, b in re.findall(r"\[RDIAG\] busy=(\d+)% msgs=(\d+)/s", open(os.path.join(L, f.replace("-c.log", "-s.log")), errors="replace").read())]
    rmain = [b for b, m in rd if m >= 3000]; rtail = [b for b, m in rd if 50 <= m < 3000]
    rows.append(dict(arm=arm, seed=seed, rep=int(rep), secs=secs, tse=tse, tail=secs - tse, cpu=cpu_srv, dl=dl, qcc=qcc,
                     rx_main=st.mean(rmain) if rmain else float("nan"), rx_tail=st.mean(rtail) if rtail else float("nan"), n_rtail=len(rtail)))

P("POST HOC / NOT REGISTERED. c8 follow-ups")
P("\n(a) server receiver task busy, main vs tail ([RDIAG] split at msgs/s 3000; tail lines 50-3000 msgs/s), means over rows")
for arm in ("BASE", "CAND"):
    rr = [r for r in rows if r["arm"] == arm]
    m = lambda k: st.mean(r[k] for r in rr if r[k] == r[k])
    P(f"  {arm}: busy main {m('rx_main'):.1f}%  tail {m('rx_tail'):.1f}% (lines/row {m('n_rtail'):.1f})  tail {m('tail'):.2f}s -> tail receiver-task s = {st.mean(r['rx_tail']/100*r['tail'] for r in rr if r['rx_tail']==r['rx_tail']):.3f}")
X, Y = [], []
for arm in ("BASE", "CAND"):
    for s in ("42", "7"):
        rr = [r for r in rows if r["arm"] == arm and r["seed"] == s]
        x = np.array([r["tail"] for r in rr]); y = np.array([r["cpu"] for r in rr]); X += list(x - x.mean()); Y += list(y - y.mean())
X, Y = np.array(X), np.array(Y); b = (X @ Y) / (X @ X); res = Y - b * X
se = np.sqrt((res @ res) / (len(X) - 4 - 1) / (X @ X))   # 4 group means removed + slope
P(f"  within-arm seed-centred slope srv CPU (whole invocation) on tail s: {b:+.3f} +/- {1.96*se:.3f} (95% CI {b-1.96*se:+.2f}..{b+1.96*se:+.2f}); tail sd {X.std():.2f}s")
dt = st.mean(r["tail"] for r in rows if r["arm"] == "CAND") - st.mean(r["tail"] for r in rows if r["arm"] == "BASE")
P(f"  arm d_tail {dt:+.3f}s -> CPU carried by the tail at the CI upper bound {dt*(b+1.96*se):+.2f}s; at the run-average 0.57 cores {dt*0.57:+.2f}s")

P("\n(b) tail-line state per arm and leg (lines after t_src_end, t <= run end), means over lines; infl in datagrams, qcwnd in bytes")
for arm in ("BASE", "CAND"):
    for i in (0, 1):
        Bs, inf, cwr, dtx, below = [], [], [], [], []
        for r in [r for r in rows if r["arm"] == arm]:
            prev = None
            for t, s, legs in r["dl"]:
                if t > r["tse"] and t <= r["secs"] and i in legs:
                    l = legs[i]
                    Bs.append(l["B"]); inf.append(l["infl"])
                    if l["cw"]: cwr.append(l["infl"] * 1300 / l["cw"]); below.append(l["cw"] < BDP[i])
                    if prev is not None: dtx.append(l["tx"] - prev)
                prev = legs.get(i, {}).get("tx")
        P(f"  {arm} leg{i}: tail lines {len(Bs)}  B(hand-tx) mean {st.mean(Bs):.1f} p90 {sorted(Bs)[int(.9*(len(Bs)-1))]}  infl mean {st.mean(inf):.1f}  infl*1300/qcwnd mean {st.mean(cwr) if cwr else float('nan'):.3f}  share qcwnd<BDP {st.mean(below) if below else float('nan'):.3f}  dtx/line mean {st.mean(dtx):.1f}")
P("\n(b2) CAND rows: tail length vs UNDER onset (first non-R UNDER line after t_src_end), per row")
for r in sorted([r for r in rows if r["arm"] == "CAND"], key=lambda r: r["tail"]):
    on = {}
    for i in (0, 1):
        ts = [t for t, q in r["qcc"] if t > r["tse"] and i in q and q[i][1] < 0.8 * LINK[i] and q[i][0] != "R"]
        on[i] = (min(ts) - r["tse"]) if ts else None
    Bend = sum(v["B"] or 0 for v in [l for t, s, l in r["dl"] if abs(t - r["tse"]) < 1e-9][0].values())
    P(f"  s{r['seed']} r{r['rep']}: tail {r['tail']:.2f}s  UNDER onset after src end: L0 {on[0] if on[0] is None else round(on[0],2)}  L1 {on[1] if on[1] is None else round(on[1],2)}  B at src end {Bend}")
P("\n(c) MAIN-phase leg qcwnd / BDP and infl*1300/qcwnd, per arm (medians over lines)")
for arm in ("BASE", "CAND"):
    for i in (0, 1):
        cw = []; fr = []; below = []
        for r in [r for r in rows if r["arm"] == arm]:
            for t, s, legs in r["dl"]:
                if 1.0 <= t <= r["tse"] and i in legs and legs[i]["cw"]:
                    cw.append(legs[i]["cw"] / BDP[i]); fr.append(legs[i]["infl"] * 1300 / legs[i]["cw"]); below.append(legs[i]["cw"] < BDP[i])
        cw.sort(); fr.sort()
        P(f"  {arm} leg{i}: qcwnd/BDP p10 {cw[len(cw)//10]:.2f} p50 {cw[len(cw)//2]:.2f}  infl/qcwnd p50 {fr[len(fr)//2]:.3f} p90 {fr[int(.9*(len(fr)-1))]:.3f}  share qcwnd<BDP {st.mean(below):.3f}")
P("\n(d) CAND R2a lines (qcwnd < BDP, t >= 1): main vs tail counts per leg (all lines, incl. R)")
for i in (0, 1):
    mn = tl = 0
    for r in [r for r in rows if r["arm"] == "CAND"]:
        for t, s, legs in r["dl"]:
            if t < 1.0 or t > r["secs"] or i not in legs or not legs[i]["cw"]: continue
            if legs[i]["cw"] < BDP[i]:
                if t <= r["tse"]: mn += 1
                else: tl += 1
    P(f"  leg{i}: R2a lines main {mn}  tail {tl}")
open(os.path.join(D, "diag-c8/tail2_out.txt"), "w").write("\n".join(out) + "\n")
