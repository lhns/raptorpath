# Check 2: does leg 1's quinn backlog B1 grow where the store cap steps, and is hand1 ~ tx1 otherwise?
# Per CAND c8 row: the store-cap steps (cap rises by >= 10 %) with dB1 over the step tick, and
# dB1 over all other ticks; plus B1 at 4-16 s.
import sys,glob,re,statistics as st
D=sys.argv[1]
for arm in ['CAND','BASE']:
    print('==',arm)
    for f in sorted(glob.glob(f'{D}/c8-100@def-{arm}-*-c.log')):
        pts=[]
        for ln in open(f,encoding='utf8',errors='replace'):
            if not ln.startswith('[DIAG]'): continue
            t=float(re.search(r't=([\d.]+)s',ln).group(1))
            cap=int(re.search(r'win=\d+/(\d+)',ln)[1])
            h={int(m[1]):(int(m[2]),int(m[3])) for m in re.finditer(r'dgq(\d)\[hand=(\d+) tx=(\d+)',ln)}
            if 1 in h: pts.append((t,cap,h[1][0]-h[1][1]))
        step=0;other=0;steps=[]
        for a,b in zip(pts,pts[1:]):
            d=b[2]-a[2]
            if b[1]>=1.1*a[1] and b[0]<20: step+=d; steps.append(f"{b[0]:.1f}s cap {a[1]}->{b[1]} dB1 {d:+d}")
            elif b[0]<16: other+=d
        b416=[p[2] for p in pts if 4<=p[0]<16]; peak=max(p[2] for p in pts if p[0]<20)
        print(f"{f.split('@def-')[1][:12]} sum dB1 at cap steps (t<20) {step:+d}; at other ticks (t<16) {other:+d}; B1 p50 4-16 s {st.median(b416):.0f}; peak {peak}; steps: {'; '.join(steps)}")
