# POST HOC, not registered. Per-row tail decomposition from sender [DIAG] lines.
# H0 = store outstanding (win) at the first tail line (src=0); mu = retx/s over the tail lines;
# drain = -d(win)/dt; lag0 = src-ack at src end; on1 = share of tail retx placed on leg1;
# fl0 = share of tail retx whose flight path was leg0; rtt = pooled recovery clock ([DIAG] rtt=) median in tail.
import os, re, sys, statistics as st
D = sys.argv[1]; cells = sys.argv[2].split(",")
def I(p, l): return int(re.search(p, l).group(1))
res = {}
for f in sorted(os.listdir(D)):
    if not f.endswith("-c.log") or f.split("@")[0] not in cells: continue
    cell = f.split("@")[0]; arm, seed, rep = re.search(r"def-(\w+)-s(\d+)-r(\d+)-", f).groups()
    drv = open(os.path.join(D, f.replace("-c.log", "-drv.out")), errors="replace").read()
    secs = float(re.search(r'"median_s":([0-9.]+)', drv).group(1))
    L = [l for l in open(os.path.join(D, f), errors="replace") if l.startswith("[DIAG]")]
    T = [float(re.search(r"t=([0-9.]+)s", l).group(1)) for l in L]
    srcs = [I(r"src=(\d+)sym", l) for l in L]
    ise = max(i for i in range(len(L)) if srcs[i] > 0 and T[i] >= 1.0)   # last line emitting new source
    tse = T[ise]
    tl = [i for i in range(ise + 1, len(L)) if T[i] <= secs]
    if not tl: continue
    i0, i1 = tl[0], tl[-1]
    win0 = I(r"win=(\d+)/", L[i0]); win1 = I(r"win=(\d+)/", L[i1])
    s, c, a = map(int, re.search(r"cum=(\d+)/(\d+)/(\d+)", L[ise]).groups())
    dt = T[i1] - T[ise]
    retx = I(r"retx=(\d+)", L[i1]) - I(r"retx=(\d+)", L[ise])
    cod = int(re.search(r"cum=\d+/(\d+)/", L[i1]).group(1)) - c
    def fp(l):
        m = re.search(r"fp/on p0:(\d+)/(\d+) p1:(\d+)/(\d+)", l)
        if m: return [int(x) for x in m.groups()]
        m = re.search(r"fp/on p0:(\d+)/(\d+)", l)
        return [int(m.group(1)), int(m.group(2)), 0, 0] if m else [0, 0, 0, 0]
    a0, b0 = fp(L[ise]), fp(L[i1]); d = [y - x for x, y in zip(a0, b0)]
    on1 = d[3] / max(1, d[1] + d[3]); fl0 = d[0] / max(1, d[0] + d[2])
    rtt = st.median(float(re.search(r" rtt=([0-9.]+)ms", L[i]).group(1)) for i in tl)
    rtt_main = st.median(float(re.search(r" rtt=([0-9.]+)ms", L[i]).group(1)) for i in range(len(L)) if 5 <= T[i] <= tse)
    infl_main = st.median(I(r" infl=(\d+)", L[i]) for i in range(len(L)) if 5 <= T[i] <= tse)
    win_main = st.median(I(r"win=(\d+)/", L[i]) for i in range(len(L)) if 5 <= T[i] <= tse)
    rmain = (I(r"retx=(\d+)", L[ise]) - I(r"retx=(\d+)", L[[i for i in range(len(L)) if T[i] >= 5][0]])) / (tse - 5)
    age = float(re.search(r"age=(\d+)ms", L[ise]).group(1))
    res.setdefault((cell, arm), []).append(dict(row=f"s{seed}r{rep}", tail=secs - tse, H0=win0, lag0=s - a, mu=retx / dt if dt else 0,
        drain=(win0 - win1) / (T[i1] - T[i0]) if T[i1] > T[i0] else 0, cod=cod / dt if dt else 0, on1=on1, fl0=fl0, rtt=rtt,
        rtt_main=rtt_main, infl_main=infl_main, win_main=win_main, rmain=rmain, age=age, secs=secs))
print("POST HOC. per-arm means (rows)")
for k, rr in sorted(res.items()):
    m = lambda x: st.mean(r[x] for r in rr)
    print(f"{k[0]:8s} {k[1]:4s} n={len(rr)} tail={m('tail'):.2f}s H0={m('H0'):.0f} lag0={m('lag0'):.0f} H0/tail={m('H0')/m('tail'):.0f}/s mu_retx={m('mu'):.0f}/s drain={m('drain'):.0f}/s cod={m('cod'):.0f}/s on_leg1={m('on1'):.2f} flight_leg0={m('fl0'):.2f} rtt_tail={m('rtt'):.1f}ms | main: rtt={m('rtt_main'):.0f}ms infl={m('infl_main'):.0f} win={m('win_main'):.0f} retx={m('rmain'):.0f}/s cumage={m('age'):.0f}ms")
if len(sys.argv) > 3:
    for k, rr in sorted(res.items()):
        for r in sorted(rr, key=lambda r: r["tail"]):
            print(f"  {k[0]} {k[1]} {r['row']}: tail {r['tail']:.2f} H0 {r['H0']} lag0 {r['lag0']} mu {r['mu']:.0f} drain {r['drain']:.0f} on1 {r['on1']:.2f} rtt {r['rtt']:.0f} infl_main {r['infl_main']:.0f} cumage {r['age']:.0f}")
