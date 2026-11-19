import sys, re, glob, os, statistics as st
ARMS=["HB","BM","BP","I1","I2","I3","HA","NA","AM","AP","I4","RM","RP","HR"]
def last(lines, pfx):
    v=[l for l in lines if l.startswith(pfx)]
    return v[-1] if v else None
def gi(pat, s, d=None):
    m=re.search(pat, s) if s else None
    return int(m.group(1)) if m else d
for d in sys.argv[1:]:
    cell=os.path.basename(os.path.dirname(os.path.dirname(os.path.dirname(d.rstrip('/')))))
    R={}
    for f in sorted(glob.glob(os.path.join(d,"*-c.log"))):
        arm=re.search(r"@def-([A-Z0-9]+)-",f).group(1)
        base=f[:-6]
        cl=open(f,errors="replace").read().splitlines()
        sl=open(base+"-s.log",errors="replace").read().splitlines()
        dl=open(base+"-drv.out",errors="replace").read().splitlines()
        dg=[l for l in cl if l.startswith("[DIAG] t=")][-1]
        src,cod,_=map(int,re.search(r"cum=(\d+)/(\d+)/(\d+)",dg).groups())
        taper=gi(r" taper=(\d+)",dg); retx=gi(r" retx=(\d+)",dg); fired=gi(r"fired=(\d+)",dg); sweeps=gi(r"sweeps=(\d+)",dg)
        lost=sum(int(x) for x in re.findall(r"\[TRUTH\].*? lost=(\d+)", "\n".join(dl)))
        c8r=last(sl,"[C8CONV-R]"); rxdup=sum(int(x) for x in re.findall(r"\bdup=(\d+)",c8r)) if c8r else None
        fst=sum(int(x) for x in re.findall(r"\bfst=(\d+)",c8r)) if c8r else None
        rfa=[l for l in sl if l.startswith("[RFA]") and "src_n=" in l]
        rfa=max(rfa,key=lambda l:gi(r"src_n=(\d+)",l)) if rfa else None
        srcn=gi(r"src_n=(\d+)",rfa); dupsrc=gi(r"dup_src=(\d+)",rfa); fills=gi(r"fill_src=(\d+)",rfa)
        rd=[l for l in sl if l.startswith("[RDIAG]") and "below_floor=" in l]
        bf=max((gi(r"below_floor=(\d+)",l) for l in rd),default=None)
        c8s=last(cl,"[C8CONV-S]")
        stalls=sum(int(x) for x in re.findall(r"st=\d+ms/(\d+)",c8s)) if c8s else None
        sp=sum(int(x) for x in re.findall(r"sp=(\d+)",c8s)) if c8s else None
        sp1=gi(r"p1:sp=(\d+)",c8s,0)
        hands=[int(x) for x in re.findall(r"dgq\d\[hand=(\d+)",dg)]
        p1corr=(hands[1]-sp1) if len(hands)>1 else 0
        mb=[float(x) for x in re.findall(r'"mbps":([0-9.]+)',"\n".join(cl))]
        R.setdefault(arm,[]).append(dict(src=src,cod=cod,taper=taper,retx=retx,fired=fired,lost=lost,rxdup=rxdup,srcn=srcn,dupsrc=dupsrc,bf=bf,stalls=stalls,
            p1corr_frac=p1corr/max(1,taper+retx+cod), mbps=mb[-1] if mb else None,
            redund=(srcn-fst) if (srcn and fst) else None))
    print(f"== {cell} ({sum(len(v) for v in R.values())} rows)")
    print("arm  n  src      cod/src taper/src retx/src lost/src  taper/lost (taper+retx)/lost  rx_dup  belowfl  redundSrcArr/(taper+retx)  copies/stall  p1share  Mbit/s")
    md=lambda k,v:st.median([x[k] for x in v])
    for arm in ARMS:
        v=R.get(arm)
        if not v: continue
        f=lambda e: st.median([e(x) for x in v])
        print(f"{arm:3} {len(v):2} {md('src',v):7.0f}  {f(lambda x:x['cod']/x['src']):.4f}  {f(lambda x:x['taper']/x['src']):.4f}   {f(lambda x:x['retx']/x['src']):.4f}  {f(lambda x:x['lost']/x['src']):.4f}   {f(lambda x:x['taper']/max(1,x['lost'])):6.2f}     {f(lambda x:(x['taper']+x['retx'])/max(1,x['lost'])):6.2f}      {md('rxdup',v):6.0f}  {md('bf',v):6.0f}   {f(lambda x:(x['redund'] or 0)/max(1,x['taper']+x['retx'])):.3f}      {f(lambda x:x['taper']/max(1,x['stalls'])):6.1f}   {f(lambda x:x['p1corr_frac']):.2f}  {md('mbps',v):.1f}")
    # sanity: retx==fired, plost==taper
    allr=[x for v in R.values() for x in v]
    print("retx==fired all:", all(x['retx']==x['fired'] for x in allr))
