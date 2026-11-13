"""Receiver open holes ([SUCC] open=, [RANK] holes=) per row: median over lines while the sender is
streaming (server [SUCC] lines are ~1 s apart; drop the first 2 and the last 6 lines to skip
start-up and the drained tail), plus [LAT] p0/p1 ax_p50, tot_p50/p95 and [SUCC] orig/rep quantiles
from the final line."""
import re, sys, glob, os, statistics as S
d = sys.argv[1]; cell = sys.argv[2] if len(sys.argv) > 2 else 'c8-100'
def g(p, L):
    m = re.search(p, L); return int(m.group(1)) if m and m.group(1) not in '-' else None
print('row\topen_med\topen_p90\trank_holes_med\tLATp0_ax50\tLATp0_tot50\tLATp0_tot95\tLATp1_ax50\tLATp1_tot50\tLATp1_tot95\tSUCC_orig50\torig90\torig99\trep50\trep90\tLATE_lo50\tlo90\tlo99')
arms = {}
for f in sorted(glob.glob(os.path.join(d, cell + '@def-*-s.log'))):
    name = os.path.basename(f)[len(cell) + 5:-6]
    succ = [L for L in open(f, errors='replace') if L.startswith('[SUCC]')]
    rank = [L for L in open(f, errors='replace') if L.startswith('[RANK]')]
    lat = [L for L in open(f, errors='replace') if L.startswith('[LAT]')]
    late = [L for L in open(f, errors='replace') if L.startswith('[LATE]')]
    op = [g(r'open=(\d+)', L) for L in succ[2:-6]]
    rh = [g(r'holes=(\d+)', L) for L in rank[2:-6]]
    if not op: continue
    op.sort()
    la = lat[-1]; ss = succ[-1]; lt = late[-1]
    def seg(p):
        i = la.index(' %s:' % p); return la[i:]
    p0 = seg('p0'); p1 = seg('p1')
    v = [S.median(op), op[int(0.9 * (len(op) - 1))], S.median(rh) if rh else -1,
         g(r'ax_p50=(\d+)', p0), g(r'tot_p50=(\d+)', p0), g(r'tot_p95=(\d+)', p0),
         g(r'ax_p50=(\d+)', p1), g(r'tot_p50=(\d+)', p1), g(r'tot_p95=(\d+)', p1),
         g(r'orig_p50_us=(\d+)', ss), g(r'orig_p90_us=(\d+)', ss), g(r'orig_p99_us=(\d+)', ss), g(r'rep_p50_us=(\d+)', ss), g(r'rep_p90_us=(\d+)', ss),
         g(r'lo_p50=(\d+)', lt), g(r'lo_p90=(\d+)', lt), g(r'lo_p99=(\d+)', lt)]
    v = [x if x is not None else -1 for x in v]
    v = v[:3] + [x / 1000 for x in v[3:]]
    arms.setdefault(name.split('-')[0], []).append(v)
    print(name, *['%.0f' % x for x in v], sep='\t')
print('\narm range (min..max of rows)')
for a, M in arms.items():
    print(a, *['%.0f..%.0f' % (min(c), max(c)) for c in zip(*M)], sep='\t')
print('\narm median of rows')
for a, M in arms.items():
    print(a, *['%.0f' % S.median(c) for c in zip(*M)], sep='\t')
