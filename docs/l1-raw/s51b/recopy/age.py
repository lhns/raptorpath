# blocker age per [SPAN] tick: send time of seq ack+1 interpolated from win-end crossings
import sys,re,glob,os,statistics as st,math,bisect
def surv(z): return 0.5*math.erfc(z/math.sqrt(2))
def plost(age,eps,srtt,rv):
    if eps<=0: return 0.0
    rv=max(rv,0.001); d=eps+(1-eps)*surv((age-srtt)/rv); return 1.0 if d<1e-300 else min(1,eps/d)
for d in sys.argv[1:]:
  out={}
  for f in sorted(glob.glob(os.path.join(d,"*-c.log"))):
    arm=re.search(r"@def-([A-Z0-9]+)-",f).group(1)
    if arm not in ("HB","I2","I3","HA","RM"): continue
    L=open(f,errors="replace").read().splitlines()
    sp=[(float(m.group(1)),int(m.group(2)),int(m.group(3))) for m in (re.search(r"^\[SPAN\] t=([0-9.]+)s ack=(\d+) win=\[\d+,(\d+)\]",l) for l in L) if m]
    dg=[l for l in L if l.startswith("[DIAG] t=")]
    # worst-eps path srtt: take max of per-path rtt= among pN blocks (approx worst-eps = slow leg at c8)
    rtts=[]; 
    for l in dg:
        r=[int(x) for x in re.findall(r" p\d:infl=.*? rtt=(\d+)/",l)]
        pl=[float(x) for x in re.findall(r" plc=([0-9.]+)",l)]
        if r: rtts.append((max(r)/1000, max(pl) if pl else 0.0))
    srtt=st.median([x[0] for x in rtts]); eps=st.median([x[1] for x in rtts])
    T=[x[0] for x in sp]; E=[x[2] for x in sp]
    ages=[];ps=[]
    for t,ack,we in sp:
        b=ack+1; i=bisect.bisect_left(E,b)
        if i==0 or i>=len(E): continue
        ts=T[i-1]+(T[i]-T[i-1])*(b-E[i-1])/max(1,E[i]-E[i-1])
        a=t-ts; ages.append(a); ps.append(plost(a,eps,srtt,0.1*srtt))
    dl=dg[-1]; src,cod,_=map(int,re.search(r"cum=(\d+)/(\d+)/(\d+)",dl).groups()); tap=int(re.search(r" taper=(\d+)",dl).group(1))
    out.setdefault(arm,[]).append((st.median(ages),min(ages),st.mean(ps),tap/max(1,tap+cod),srtt,eps))
  print(d.split('/')[-4])
  for a,v in out.items():
    print(f" {a}: blocker age p50 {st.median([x[0] for x in v]):.2f}s min {min(x[1] for x in v):.2f}s  mean P_lost(pred) {st.median([x[2] for x in v]):.3f}  observed copy share {st.median([x[3] for x in v]):.3f}  srtt_w {st.median([x[4] for x in v])*1000:.0f}ms eps {st.median([x[5] for x in v]):.4f}")
