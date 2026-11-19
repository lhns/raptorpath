import sys, re, glob, os, statistics as st
d = sys.argv[1]
out = {}
for f in sorted(glob.glob(os.path.join(d, "*-c.log"))):
    arm = re.search(r"@def-([A-Z0-9]+)-", f).group(1)
    if arm not in ("HR","RM","RP","HA","HB"): continue
    sp=[]; infl=[]
    for ln in open(f, errors="replace"):
        if '"warmup":true' in ln: sp=[]; infl=[]; continue
        if not ln.startswith("[DIAG] t="): continue
        m = re.search(r" src=(\d+)sym/s.*? cwnd=(\d+) infl=(\d+) ", ln)
        if not m or int(m.group(1))<=1000: continue
        legs = [int(x) for x in re.findall(r" p\d:infl=(\d+)/", ln)]
        infl.append(legs)
        if int(m.group(2))==20000 and all(x>0 for x in legs):
            sp.append(min((10000-x)/x for x in legs))
    out.setdefault(arm, []).append((len(sp), st.median(sp) if sp else None, sum(1 for x in sp if x<0.01)/len(sp) if sp else None, st.median([l[0] for l in infl]), st.median([l[1] for l in infl])))
for a,v in out.items():
    print(a, [(n, None if m is None else round(m,3), None if z is None else round(z,2), i0, i1) for n,m,z,i0,i1 in v])
