import sys,glob,re,bisect,statistics as st
D=sys.argv[1]
def iod(f):
    T=[];S=[];A=[]
    for ln in open(f,encoding='utf8',errors='replace'):
        if not ln.startswith('[DIAG]'): continue
        t=float(re.search(r't=([\d.]+)s',ln).group(1)); m=re.search(r' cum=(\d+)/(\d+)/(\d+)',ln)
        T.append(t);S.append(int(m[1]));A.append(int(m[3]))
    ds=[]
    for t,a in zip(T,A):
        if a<=0 or a>S[-1]: continue
        j=bisect.bisect_left(S,a)
        if j==0: ts=T[0]
        else:
            s0,s1=S[j-1],S[j]; ts=T[j-1]+(T[j]-T[j-1])*((a-s0)/(s1-s0) if s1>s0 else 0)
        ds.append(t-ts)
    return st.mean(ds)*1000 if ds else float('nan')
for cell in ['c1s-400','c1d-400','c2-100','c8-100']:
    for arm in ['BASE','CAND']:
        v=[iod(f) for f in sorted(glob.glob(f'{D}/{cell}@def-{arm}-*-c.log'))]
        print(f"{cell} {arm}: in-order delay mean ms  median {st.median(v):.0f}  range {min(v):.0f}-{max(v):.0f}")
