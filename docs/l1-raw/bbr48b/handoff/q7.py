# Store pin: share of [DIAG] lines at t >= 1 s whose store cap equals N x pool (2048 per path) and
# whose occupancy is within 2 % of the cap (win >= 0.98 cap); also cap-at-N*P share alone.
import sys,glob,re
D=sys.argv[1]
for cell,N in [('c1s-400',1),('c1d-400',2),('c2-100',1),('c8-100',2)]:
    for arm in ['BASE','CAND']:
        n=capk=pin=0
        for f in sorted(glob.glob(f'{D}/{cell}@def-{arm}-*-c.log')):
            for ln in open(f,encoding='utf8',errors='replace'):
                if not ln.startswith('[DIAG]'): continue
                t=float(re.search(r't=([\d.]+)s',ln).group(1))
                if t<1: continue
                m=re.search(r'win=(\d+)/(\d+)',ln); w,c=int(m[1]),int(m[2]); n+=1
                if c>=N*2048: capk+=1
                if c>=N*2048 and w>=0.98*c: pin+=1
        print(f"{cell} {arm}: lines {n}  cap at N*P {capk/n:.3f}  cap at N*P and occupancy >= 0.98 cap {pin/n:.3f}")
