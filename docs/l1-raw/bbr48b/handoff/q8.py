# Producer 1 ms backpressure timer: realized wait per paused-arm wake, per [DIAG] tick
# = paused% x (us - busy_us) / (wake[paused] delta). Ticks with >= 5 paused wakes.
import sys,glob,re,statistics as st
D=sys.argv[1]
for cell in ['c1s-400','c2-100','c8-100']:
    for arm in ['BASE','CAND']:
        v=[]
        for f in glob.glob(f'{D}/{cell}@def-{arm}-*-c.log'):
            prev=None
            for ln in open(f,encoding='utf8',errors='replace'):
                if not ln.startswith('[DIAG]'): continue
                w=re.search(r'wait\[tun=\d+% paused=(\d+)%.*? us=(\d+) busy=\d+% busy_us=(\d+)\]',ln)
                k=re.search(r'wake\[tun=\d+ paused=(\d+)',ln)
                if not (w and k): continue
                pw=int(k[1])
                if prev is not None and pw-prev>=5:
                    v.append(int(w[1])/100*(int(w[2])-int(w[3]))/(pw-prev)/1000)
                prev=pw
        if v:
            v.sort(); q=lambda x:v[min(len(v)-1,int(x*len(v)))]
            print(f"{cell} {arm}: ticks {len(v)}  ms per paused wake p50 {q(.5):.2f} p90 {q(.9):.2f} p99 {q(.99):.2f}")
