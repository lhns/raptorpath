# POST HOC. Backlog build-up per arm (means over 12 rows), 4 s bins: frontier lag (src-ack, symbols),
# retx/s, coded/s, share of retx placed on leg1, pooled clock, fires per processed report, cooldown-suppressed share.
import os, re, sys, statistics as st
D = sys.argv[1]
def I(p, l):
    m = re.search(p, l); return int(m.group(1)) if m else 0
acc = {}
for f in sorted(os.listdir(D)):
    if not (f.startswith("c8-100@") and f.endswith("-c.log")): continue
    arm = re.search(r"def-(\w+)-s", f).group(1)
    L = [l for l in open(os.path.join(D, f), errors="replace") if l.startswith("[DIAG]")]
    T = [float(re.search(r"t=([0-9.]+)s", l).group(1)) for l in L]
    for b in range(0, 11):
        idx = [i for i in range(len(L)) if 4 * b <= T[i] < 4 * b + 4 and I(r"src=(\d+)sym", L[i]) > 0]
        if len(idx) < 2: continue
        i0, i1 = idx[0], idx[-1]; dt = T[i1] - T[i0]
        d = lambda p: I(p, L[i1]) - I(p, L[i0])
        s, c, a = map(int, re.search(r"cum=(\d+)/(\d+)/(\d+)", L[i1]).groups())
        on0, on1 = d(r"fp/on p0:\d+/(\d+)"), d(r"p1:\d+/(\d+)\]")
        e = dict(lag=s - a, retx=d(r"retx=(\d+)") / dt, cod=d(r"cum=\d+/(\d+)/") / dt, on1=on1 / max(1, on0 + on1),
                 rtt=st.median(float(re.search(r" rtt=([0-9.]+)ms", L[i]).group(1)) for i in idx),
                 fpr=d(r"retx=(\d+)") / max(1, d(r"mpr\[rep=(\d+)")), supc=d(r"supp=(\d+)/") / max(1, d(r" seqs=(\d+)")))
        acc.setdefault((arm, b), []).append(e)
for arm in ("BASE", "CAND"):
    print(arm)
    for b in range(11):
        rr = acc.get((arm, b))
        if not rr: continue
        m = lambda k: st.mean(r[k] for r in rr)
        print(f"  {4*b:2d}-{4*b+4:2d}s lag={m('lag'):6.0f} retx={m('retx'):4.0f}/s cod={m('cod'):3.0f}/s on_leg1={m('on1'):.2f} pooled_rtt={m('rtt'):4.0f}ms fires/rep={m('fpr'):.2f} cool_supp={m('supc'):.3f}")
