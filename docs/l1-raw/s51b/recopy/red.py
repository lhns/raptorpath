import sys,re,glob,os,statistics as st
ARMS=["HB","BM","BP","I1","I2","I3","HA","NA","AM","AP","I4","RM","RP"]
def gi(p,s): m=re.search(p,s); return int(m.group(1)) if m else 0
for d in sys.argv[1:]:
  rows=[]
  for f in sorted(glob.glob(os.path.join(d,"*-c.log"))):
    arm=re.search(r"@def-([A-Z0-9]+)-",f).group(1); b=f[:-6]
    cl=open(f,errors="replace").read().splitlines(); sl=open(b+"-s.log",errors="replace").read(); dv=open(b+"-drv.out",errors="replace").read()
    dg=[l for l in cl if l.startswith("[DIAG] t=")][-1]
    src,cod,_=map(int,re.search(r"cum=(\d+)/(\d+)/(\d+)",dg).groups()); tap=gi(r" taper=(\d+)",dg); retx=gi(r" retx=(\d+)",dg)
    rfa=max((l for l in sl.splitlines() if l.startswith("[RFA]")),key=lambda l:gi(r"src_n=(\d+)",l))
    srcn=gi(r"src_n=(\d+)",rfa); fc=gi(r"fill_coded=(\d+)",rfa)
    c8r=[l for l in sl.splitlines() if l.startswith("[C8CONV-R]")][-1]
    fst=sum(int(x) for x in re.findall(r"fst=(\d+)",c8r)); dup=sum(int(x) for x in re.findall(r"\bdup=(\d+)",c8r))
    bf=max(int(x) for x in re.findall(r"below_floor=(\d+)",sl))
    secs=float(re.search(r'"mean_s":([0-9.]+)',dv).group(1))
    legs=[int(x)*8/secs/1e6 for x in re.findall(r"QDISC cli\d: .*?Sent (\d+) bytes",dv)]
    rows.append(dict(arm=arm,src=src,tap=tap,retx=retx,red=srcn-(fst-fc),dup=dup,bf=bf,legs=legs,srcn=srcn,fst=fst,fc=fc))
  hb=[r for r in rows if r["arm"]=="HB"]; fhb=st.median([r["red"]/r["retx"] for r in hb])
  print(d.split('/')[-4], f"HB redundant/retx = {fhb:.3f}")
  for a in ARMS:
    v=[r for r in rows if r["arm"]==a]
    if not v or a in("HB","BM","BP"): 
        if v: print(f" {a}: legs Mbit/s {[round(st.median([r['legs'][i] for r in v]),1) for i in range(len(v[0]['legs']))]}")
        continue
    tr=[(r["red"]-fhb*r["retx"])/r["tap"] for r in v]
    cl=[(r["dup"]+r["bf"])/r["red"] for r in v]
    print(f" {a}: taper-redundant/taper med {st.median(tr):.3f} [{min(tr):.3f},{max(tr):.3f}]  (dup+below_floor)/redundant {st.median(cl):.3f}  legs Mbit/s {[round(st.median([r['legs'][i] for r in v]),1) for i in range(len(v[0]['legs']))]}")
