import sys,glob,os,re,statistics as st
D=sys.argv[1]
bins=[(0,1),(1,4),(4,8),(8,12),(12,16),(16,24),(24,32),(32,40)]
for arm in ['BASE','CAND']:
    res={b:[] for b in bins}
    for f in sorted(glob.glob(f'{D}/c8-100@def-{arm}-*-c.log')):
        pts=[]
        for ln in open(f,encoding='utf8',errors='replace'):
            if not ln.startswith('[DIAG]'): continue
            t=float(re.search(r't=([\d.]+)s',ln).group(1))
            h={int(m[1]):(int(m[2]),int(m[3])) for m in re.finditer(r'dgq(\d)\[hand=(\d+) tx=(\d+)',ln)}
            if 0 in h and 1 in h: pts.append((t,h))
        for b in bins:
            s=[p for p in pts if b[0]<=p[0]<b[1]]
            if len(s)<2: continue
            a,z=s[0],s[-1]; dt=z[0]-a[0]
            dh0=z[1][0][0]-a[1][0][0]; dh1=z[1][1][0]-a[1][1][0]; dx0=z[1][0][1]-a[1][0][1]; dx1=z[1][1][1]-a[1][1][1]
            res[b].append((dh1/(dh0+dh1) if dh0+dh1 else 0, dh0/dt, dh1/dt, dx0/dt, dx1/dt))
    print('==',arm,'median over rows: leg1 share of hand, hand0/s, hand1/s, tx0/s, tx1/s')
    for b in bins:
        if not res[b]: continue
        cols=list(zip(*res[b]))
        print(f"[{b[0]},{b[1]})", ' '.join(f"{st.median(c):.3f}" if i==0 else f"{st.median(c):.0f}" for i,c in enumerate(cols)))
