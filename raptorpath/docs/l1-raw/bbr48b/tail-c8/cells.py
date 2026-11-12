# POST HOC. Per cell x arm: tail, frontier lag at the last source line, main-phase gap-loop stats
# (fires/report, cooldown-suppressed share of visited seqs, retx share placed on leg1), pooled clock,
# receiver hole-refresh period, receiver same-path hole resolution p50 ([SUCC] sp_p50).
import os, re, sys, statistics as st
D = sys.argv[1]
def I(p, l, d=0):
    m = re.search(p, l); return int(m.group(1)) if m else d
out = {}
for f in sorted(os.listdir(D)):
    if not f.endswith("-c.log"): continue
    cell = f.split("@")[0]; arm = re.search(r"def-(\w+)-s", f).group(1)
    drv = open(os.path.join(D, f.replace("-c.log", "-drv.out")), errors="replace").read()
    secs = float(re.search(r'"median_s":([0-9.]+)', drv).group(1))
    L = [l for l in open(os.path.join(D, f), errors="replace") if l.startswith("[DIAG]")]
    T = [float(re.search(r"t=([0-9.]+)s", l).group(1)) for l in L]
    ise = max(i for i in range(len(L)) if I(r"src=(\d+)sym", L[i]) > 0 and T[i] >= 1.0)
    s, c, a = map(int, re.search(r"cum=(\d+)/(\d+)/(\d+)", L[ise]).groups())
    i5 = [i for i in range(len(L)) if T[i] >= 5][0]
    d = lambda p: I(p, L[ise]) - I(p, L[i5])
    rep, seqs, retx, supc = d(r"mpr\[rep=(\d+)"), d(r" seqs=(\d+)"), d(r"retx=(\d+)"), d(r"supp=(\d+)/")
    on1 = d(r"p1:\d+/(\d+)\]"); on0 = d(r"fp/on p0:\d+/(\d+)")
    rtt = st.median(float(re.search(r" rtt=([0-9.]+)ms", L[i]).group(1)) for i in range(i5, ise + 1))
    sl = open(os.path.join(D, f.replace("-c.log", "-s.log")), errors="replace").read()
    succ = [l for l in sl.splitlines() if l.startswith("[SUCC]") and "final=1" in l]
    sp = I(r"sp_p50_us=(\d+)", succ[-1], -1) if succ else -1
    q = re.findall(r"\[QCLK\] site=receiver .*?w_us_p50=(\d+)", sl)
    o = out.setdefault((cell, arm), [])
    o.append(dict(tail=secs - T[ise], lag=s - a, fpr=retx / max(1, rep), supc=supc / max(1, seqs), on1=on1 / max(1, on0 + on1),
                  rtt=rtt, refr=int(q[-1]) / 1000 if q else -1, sp=sp / 1e3, retxs=retx / (T[ise] - 5)))
print("POST HOC. means over rows; tail = run end - last [DIAG] line with src>0 (250 ms quantized, so <=0.25 s reads as ~0)")
for k, rr in sorted(out.items()):
    m = lambda x: st.mean(r[x] for r in rr)
    print(f"{k[0]:8s} {k[1]:4s} n={len(rr):2d} tail={m('tail'):.2f}s lag_at_src_end={m('lag'):7.0f} main: retx={m('retxs'):4.0f}/s fires/report={m('fpr'):.2f} cooldown-supp share={m('supc'):.3f} retx on leg1={m('on1'):.2f} pooled_rtt={m('rtt'):5.0f}ms | rx refresh={m('refr'):.0f}ms sp_p50={st.median(r['sp'] for r in rr):6.0f}ms")
