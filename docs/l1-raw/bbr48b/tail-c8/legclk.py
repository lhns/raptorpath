# POST HOC. Which leg sets the pooled recovery clock (max over legs of estimator app-echo RTT) in the main phase,
# per arm, 4 s bins (means over rows); per-leg estimator rtt and rtprop.
import os, re, sys, statistics as st
D = sys.argv[1]; acc = {}
for f in sorted(os.listdir(D)):
    if not (f.startswith("c8-100@") and f.endswith("-c.log")): continue
    arm = re.search(r"def-(\w+)-s", f).group(1)
    for l in open(os.path.join(D, f), errors="replace"):
        if not l.startswith("[DIAG]") or re.search(r"src=0sym", l): continue
        t = float(re.search(r"t=([0-9.]+)s", l).group(1))
        legs = re.findall(r" p(\d+):infl=.*? rtt=(\d+)/wrtt=(\d+)/rtp(\d+)ms", l)
        if len(legs) < 2: continue
        r = {int(p): (int(a), int(w), int(rp)) for p, a, w, rp in legs}
        acc.setdefault((arm, int(t // 4)), []).append((r[0][0], r[1][0], r[0][2], r[1][2], r[1][0] >= r[0][0]))
for arm in ("BASE", "CAND"):
    print(arm)
    for b in range(11):
        v = acc.get((arm, b))
        if not v: continue
        print(f"  {4*b:2d}-{4*b+4:2d}s leg0 rtt={st.mean(x[0] for x in v):5.0f}ms (rtprop {st.mean(x[2] for x in v):3.0f})  leg1 rtt={st.mean(x[1] for x in v):5.0f}ms (rtprop {st.mean(x[3] for x in v):3.0f})  share leg1 sets max={st.mean(x[4] for x in v):.2f}")
