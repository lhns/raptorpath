# Check 3, per path: share of [DIAG] lines at t>=1 s with np_act < np (a path with cwnd - in_flight <= 0,
# i.e. the engine's send gate / available() closed on some path), and the coded rate (cod=) as the repair witness.
import sys,glob,re,statistics as st
D=sys.argv[1]
print('cell arm | lines | share np_act<np | median cod sym/s | median src sym/s | rows with any np_act<np')
for cell in ['c1s-400','c1d-400','c2-100','c8-100']:
    for arm in ['BASE','CAND']:
        n=k=0; cods=[]; srcs=[]; rows=0
        for f in sorted(glob.glob(f'{D}/{cell}@def-{arm}-*-c.log')):
            hit=False
            for ln in open(f,encoding='utf8',errors='replace'):
                if not ln.startswith('[DIAG]'): continue
                t=float(re.search(r't=([\d.]+)s',ln).group(1))
                if t<1: continue
                m=re.search(r' np=(\d+) np_act=(\d+)',ln); n+=1
                if int(m[2])<int(m[1]): k+=1; hit=True
                c=re.search(r'src=(\d+)sym/s cod=(\d+)sym/s',ln); srcs.append(int(c[1])); cods.append(int(c[2]))
            rows+=hit
        print(f"{cell} {arm} | {n} | {k/n:.4f} | {st.median(cods)} | {st.median(srcs)} | {rows}/12")
