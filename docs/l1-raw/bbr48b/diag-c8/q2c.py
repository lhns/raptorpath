# POST HOC, not registered. Q2c: split c8/c2 by phase (main: src>0 still emitted; tail: after the last src>0 line).
# Wall decomposition (means, additive) and UNDER / R2a restricted to the main phase.
import os, re, sys, statistics as st
D = sys.argv[1]; L = os.path.join(D, "full/diag-ld")
LINK = {"c2-100": [12.5e6], "c8-100": [12.5e6, 2.5e6]}; BDP = {"c2-100": [125000], "c8-100": [125000, 100000]}
out = []
def P(s=""): out.append(s); print(s)
P("POST HOC / NOT REGISTERED. Phase split: main = t <= t_src_end (last [DIAG] line with src>0), tail = after it; lines t >= 1 s")
for cell in ("c2-100", "c8-100"):
    W = {}; U = {}
    for arm in ("BASE", "CAND"):
        W[arm] = []; cnt = {i: [0, 0, 0] for i in range(len(LINK[cell]))}
        for f in sorted(os.listdir(L)):
            if not (f.startswith(f"{cell}@def-{arm}-") and f.endswith("-c.log")): continue
            qcc, diag = [], []
            for ln in open(os.path.join(L, f), errors="replace"):
                m = re.match(r"\[QCC\] t=([0-9.]+)s", ln)
                if m: qcc.append((float(m.group(1)), {int(a): (b, int(c)) for a, b, c in re.findall(r"p(\d+):qm=(\S) qrt_us=\d+ qsr_us=\d+ qbw=(\d+)", ln)}))
                m = re.match(r"\[DIAG\] t=([0-9.]+)s .*? src=(\d+)sym/s", ln)
                if m:
                    cw = {}
                    for part in re.split(r"(?= p\d+:infl=)", ln)[1:]:
                        g_, q_ = re.match(r" p(\d+):", part), re.search(r"\bqcwnd=(\d+)", part)
                        if g_ and q_: cw[int(g_.group(1))] = int(q_.group(1))
                    diag.append((float(m.group(1)), int(m.group(2)), cw))
            secs = float(re.search(r'"median_s":([0-9.]+)', open(os.path.join(L, f.replace("-c.log", "-drv.out")), errors="replace").read()).group(1))
            tse = max(t for t, s, _ in diag if 1.0 <= t <= secs and s > 0)
            W[arm].append((secs, tse, secs - tse))
            for t, s, cw in diag:
                if t < 1.0 or t > tse or not qcc: continue
                q = min(qcc, key=lambda x: abs(x[0] - t))[1]
                if any(v[0] == "R" for v in q.values()): continue
                for i in cnt:
                    if i not in q: continue
                    u = q[i][1] < 0.8 * LINK[cell][i]; w = cw.get(i, 10**12) < BDP[cell][i]
                    cnt[i][0] += 1; cnt[i][1] += u; cnt[i][2] += u and w
        U[arm] = cnt
    mb = {a: [st.mean(x[k] for x in W[a]) for k in range(3)] for a in W}
    P(f"\n== {cell} wall (MEANS): BASE seconds {mb['BASE'][0]:.2f} = main {mb['BASE'][1]:.2f} + tail {mb['BASE'][2]:.2f};  CAND {mb['CAND'][0]:.2f} = {mb['CAND'][1]:.2f} + {mb['CAND'][2]:.2f}")
    d = [mb["CAND"][k] - mb["BASE"][k] for k in range(3)]
    P(f"   delta seconds {d[0]:+.3f} = main {d[1]:+.3f} ({100*d[1]/d[0]:.0f}%) + tail {d[2]:+.3f} ({100*d[2]/d[0]:.0f}%);  goodput deficit {100*(mb['BASE'][0]/mb['CAND'][0]-1):+.2f}% of which tail {100*d[2]/mb['CAND'][0]:.2f} pts, main {100*d[1]/mb['CAND'][0]:.2f} pts")
    P(f"   per seed tails: " + "; ".join(f"{a} " + ",".join(f"{x[2]:.2f}" for x in W[a]) for a in W))
    for i, (n, u, uw) in U["CAND"].items():
        if n: P(f"   CAND MAIN-PHASE non-R lines leg{i}: n={n} UNDER share {u/n:.4f}  UNDER&R2a {uw}")
print("\n".join([])); open(os.path.join(D, "diag-c8/q2c_out.txt"), "w").write("\n".join(out) + "\n")
