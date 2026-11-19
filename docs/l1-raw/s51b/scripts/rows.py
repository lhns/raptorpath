import sys, re, glob, os, statistics as st
d = sys.argv[1]
R = {}
for f in sorted(glob.glob(os.path.join(d, "*-c.log"))):
    arm = re.search(r"@def-([A-Z0-9]+)-", f).group(1)
    last = None; cws = []; warm=False
    for ln in open(f, errors="replace"):
        if '"warmup":true' in ln: cws=[]; last=None; continue
        if ln.startswith("[DIAG] t="):
            last = ln
            m = re.search(r" src=(\d+)sym/s.*? cwnd=(\d+) infl=(\d+) np=(\d+)", ln)
            if m and int(m.group(1))>0: cws.append(int(m.group(2)))
    if not last: continue
    cum = list(map(int, re.search(r"cum=(\d+)/(\d+)/(\d+)", last).groups()))
    taper = int(re.search(r" taper=(\d+)", last).group(1)); plost=int(re.search(r" plost=(\d+)", last).group(1))
    retx = int(re.search(r" retx=(\d+)", last).group(1))
    s,c = cum[0], cum[1]
    R.setdefault(arm, []).append(dict(src=s, cod=c, taper=taper, plost=plost, retx=retx, rc=(c+taper)/s, cs=c/(s+c), tap_r=taper/s, cod_r=c/s,
        cwmax=max(cws) if cws else None, eq10k=sum(1 for x in cws if x==10000)/len(cws) if cws else None, ge10k=sum(1 for x in cws if x>=10000)/len(cws) if cws else None, eq20k=sum(1 for x in cws if x==20000)/len(cws) if cws else None))
md = lambda v: st.median(v)
for arm in ["HB","BM","BP","I1","I2","I3","HA","NA","AM","AP","I4","RM","RP","HR"]:
    v = R.get(arm)
    if not v: continue
    print(f"{arm:3} n={len(v)} rc={md([x['rc'] for x in v]):.4f} cod/src={md([x['cod_r'] for x in v]):.4f} taper/src={md([x['tap_r'] for x in v]):.4f} cs={md([x['cs'] for x in v]):.4f} plost==taper:{all(x['plost']==x['taper'] for x in v)} taper={md([x['taper'] for x in v])} cod={md([x['cod'] for x in v])} retx={md([x['retx'] for x in v])} cwmax={md([x['cwmax'] for x in v])} eq10k={md([x['eq10k'] for x in v]):.3f} ge10k={md([x['ge10k'] for x in v]):.3f} eq20k={md([x['eq20k'] for x in v]):.3f}")
