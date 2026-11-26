import re,sys,glob,os,statistics as st
D=sys.argv[1]
def parse(f):
    rows=[];q={}
    for ln in open(f,encoding='utf8',errors='replace'):
        if ln.startswith('[QCC]'):
            t=float(re.search(r't=([\d.]+)s',ln).group(1))
            d={}
            for m in re.finditer(r'p(\d):qm=(\w) qrt_us=(\d+) qsr_us=(\d+) qbw=(\d+)',ln):
                d[int(m.group(1))]=dict(qm=m.group(2),qrt=int(m.group(3)),qsr=int(m.group(4)),qbw=int(m.group(5)))
            q[t]=d
        if not ln.startswith('[DIAG]'): continue
        r={}
        r['t']=float(re.search(r't=([\d.]+)s',ln).group(1))
        m=re.search(r'win=(\d+)/(\d+) paused=(\d+)% good=([\d.]+)Mbit',ln); r['win'],r['cap'],r['paused'],r['good']=int(m[1]),int(m[2]),int(m[3]),float(m[4])
        m=re.search(r'src=(\d+)sym/s cod=(\d+)sym/s',ln); r['src'],r['cod']=int(m[1]),int(m[2])
        m=re.search(r' cwnd=(\d+) infl=(\d+) ',ln); r['cw'],r['fl']=int(m[1]),int(m[2])
        r['p']={}
        for m in re.finditer(r' p(\d):infl=(\d+)/sinfl=(\d+)/bdp(\d+).*?rtt=(\d+)/wrtt=(\d+)/rtp(\d+)ms.*?qcwnd=(\d+) qce=(\d+) qlp=(\d+)/(\d+)',ln):
            r['p'][int(m[1])]=dict(infl=int(m[2]),bdp=int(m[4]),rtt=int(m[5]),wrtt=int(m[6]),rtp=int(m[7]),qcwnd=int(m[8]),qsent=int(m[11]))
        for m in re.finditer(r'dgq(\d)\[hand=(\d+) tx=(\d+) full=(\d+)',ln):
            r['p'].setdefault(int(m[1]),{})['B']=int(m[2])-int(m[3])
        r['qcc']=q.get(r['t'],{})
        rows.append(r)
    return rows
