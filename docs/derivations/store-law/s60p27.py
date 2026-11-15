"""s60 registration input: the printed R of the slaw gauge, rebuilt per static cap on the p27 ledger, which (unlike
s56's [DIAG]) carries each 250 ms window's echo MINIMUM.

Usage: python -I s60p27.py <loop-diag/p27/diag-ld>  [sigma per cell, ms: c2 1.8, c8 2.3 (leg 0), c1s 0.55]

Per DEF row (RWM_STORE = n static, the law off; 100 MB rows, about 9 s), per leg, from [LOOPDIAG]
(rtt_min_us = window T_min, rtt_p50_us = window T-hat, rtprop_us = Copa's m-hat):
  l     = the row's lowest window T_min (the gauge's lifetime minimum)
  b0p   = max over the first K = 8 windows of (T_min,k - m_k): the gauge's b0p (slaw.rs WinSampler), taken under
          THIS row's static cap
  late  = windows with t >= 4 s: medians of T_min, T-hat, m-hat
  Rlo   = the gauge's R with m-hat = l-hat      : (l + b0p + sig + q*l + T-hat - T_min) / T-hat
  Rhi   = with the anchor bound (m-hat >= l + b0p): (l + 2*b0p + sig + q*l + T-hat - T_min) / T-hat
  Rcom  = Rlo with ONE b0p per cell (the median b0p at the cell's two smallest caps): what the law's own boot would
          give, the KA test's premise
At c8 only leg 0 is used, with the coupling half-gap 1/2*max(0, l1 + b0p1 - l0 - b0p0) from the row's own legs.
The rows are shorter than Copa's 10 s window, so the late m-hat column shows m-hat = the boot anchor here; on
s56's 600 MB rows m-hat tracks the standing queue (s60boot.out, column lm), so the s60 rows sit between Rlo and Rhi.
"""
import glob, os, re, statistics as St, sys
from collections import defaultdict

D = sys.argv[1]
SIG = {'c2-100': 1.8, 'c8-100': 2.3, 'c1s-400': 0.55}
LD = re.compile(r'\[LOOPDIAG\] t=([\d.]+)s p(\d) n=(\d+) .*?rtt_min_us=(\d+) rtt_p10_us=\d+ rtt_p50_us=(\d+) '
                r'rtprop_us=(\d+)')
rows = defaultdict(list)
for f in sorted(glob.glob(os.path.join(D, '*-DEF*-c.log'))):
    m = re.match(r'(c\w+-\d+)-DEF(\d+)-', os.path.basename(f))
    cell, cap = m.group(1), int(m.group(2))
    if cell not in SIG:
        continue
    W = defaultdict(list)
    for L in open(f, errors='replace'):
        g = LD.search(L)
        if g:
            t, p, _, tmin, tp50, rp = g.groups()
            W[int(p)].append((float(t), int(tmin) / 1e3, int(tp50) / 1e3, int(rp) / 1e3))
    if 0 not in W or len(W[0]) < 12:
        continue
    leg = {}
    for p, v in W.items():
        l = min(x[1] for x in v)
        b0p = max(x[1] - x[3] for x in v[:8])
        late = [x for x in v if x[0] >= 4.0]
        if not late:
            continue
        leg[p] = dict(l=l, b0p=b0p, tmin=St.median([x[1] for x in late]), that=St.median([x[2] for x in late]),
                      m=St.median([x[3] for x in late]))
    if 0 in leg:
        rows[(cell, cap)].append(leg)

com = {}
for cell in sorted({ce for (ce, _) in rows}):
    caps = sorted(c for (ce, c) in rows if ce == cell)[:2]
    com[cell] = St.median([r[0]['b0p'] for c in caps for r in rows[(cell, c)]])


def rr(cell, g, b0, q, hi, gap):
    sig = SIG[cell]
    f = g['l'] + (2 if hi else 1) * b0 + gap
    return (f + sig + q * g['l'] + g['that'] - g['tmin']) / g['that']


print('cell cap n | l | b0p [range] | late Tmin-l | late S=That-Tmin | late m-l | gap | q=0.10: Rlo Rhi Rcom | q=0.05: Rlo Rhi Rcom')
for (cell, cap) in sorted(rows, key=lambda k: (k[0], k[1])):
    v = rows[(cell, cap)]
    out = []
    for q in (0.10, 0.05):
        lo, hi, cm = [], [], []
        for r in v:
            g = r[0]
            gap = 0.0
            if cell == 'c8-100' and 1 in r:
                gap = 0.5 * max(0.0, r[1]['l'] + r[1]['b0p'] - g['l'] - g['b0p'])
            lo.append(rr(cell, g, g['b0p'], q, False, gap))
            hi.append(rr(cell, g, g['b0p'], q, True, gap))
            gapc = 0.0
            if cell == 'c8-100' and 1 in r:
                gapc = 0.5 * max(0.0, r[1]['l'] + com[cell] - g['l'] - com[cell])
            cm.append(rr(cell, g, com[cell], q, False, gapc))
        out.append('%.3f %.3f %.3f' % (St.median(lo), St.median(hi), St.median(cm)))
    b = [r[0]['b0p'] for r in v]
    gp = [0.5 * max(0.0, r[1]['l'] + r[1]['b0p'] - r[0]['l'] - r[0]['b0p']) for r in v if 1 in r] or [0.0]
    print('%s %d n%d | %.2f | %.2f [%.2f-%.2f] | %.2f | %.2f | %.2f | %.2f | %s | %s'
          % (cell, cap, len(v), St.median([r[0]['l'] for r in v]), St.median(b), min(b), max(b),
             St.median([r[0]['tmin'] - r[0]['l'] for r in v]), St.median([r[0]['that'] - r[0]['tmin'] for r in v]),
             St.median([r[0]['m'] - r[0]['l'] for r in v]),
             St.median(gp), out[0], out[1]))
print('common b0p (median at the two smallest caps):', ', '.join('%s %.2f' % (c, com[c]) for c in com))
caps8 = sorted(c for (ce, c) in rows if ce == 'c8-100')[:2]
if caps8:
    b1 = [r[1]['b0p'] for c in caps8 for r in rows[('c8-100', c)] if 1 in r]
    l1 = [r[1]['l'] for c in caps8 for r in rows[('c8-100', c)] if 1 in r]
    l0 = [r[0]['l'] for c in caps8 for r in rows[('c8-100', c)]]
    print('c8 leg 1 at the two smallest caps: b0p median %.2f [%.2f-%.2f], l1 median %.2f, l0 median %.2f'
          % (St.median(b1), min(b1), max(b1), St.median(l1), St.median(l0)))
