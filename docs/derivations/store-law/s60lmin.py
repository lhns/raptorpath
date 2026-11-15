"""s60 entry 9 (advisor): are the boot brakes taken against l? On s56's c2 / c8 small-pool rows, the [DIAG] tick at
which rtp_us (Copa's min_rtt = the gauge's m) first reaches the row's minimum (= l). If that is within the first 8
ticks (~2 s, the gauge's K = 8 boot windows), the boot brakes T_min,k - m_k are taken against l itself, so
l + b0p = the largest boot-window minimum and the row noise of l cancels in the sum; if it is later, m during boot is
above l and the sum keeps l's noise.

Usage: python -I s60lmin.py <extracted .../run/rep56/diag-ld>
"""
import glob, os, re, sys

D = sys.argv[1]
SMALL = {'c2-100': (128, 160), 'c8-100': (80, 112)}
for f in sorted(glob.glob(os.path.join(D, '*-c.log'))):
    m = re.match(r'(c\w+-\d+)@def-P(\d+)-s(\d+)-r(\d+)-', os.path.basename(f))
    if not m or int(m.group(2)) not in SMALL.get(m.group(1), ()):
        continue
    ticks = []
    for L in open(f, errors='replace'):
        if L.startswith('[DIAG]'):
            t = float(re.search(r't=([\d.]+)s', L).group(1))
            g = re.search(r' p0:infl=.*?rtp_us=(\d+) ', L)
            if g:
                ticks.append((t, int(g.group(1))))
    lo = min(v for _, v in ticks if v > 0)
    k = next(i for i, (_, v) in enumerate(ticks) if v == lo)
    print('%s P%s s%s r%s | l %.2f ms reached at tick %d (t %.2f s) | rtp_us at tick 1 / 8: %.2f / %.2f ms'
          % (m.group(1), m.group(2), m.group(3), m.group(4), lo / 1e3, k + 1, ticks[k][0],
             ticks[0][1] / 1e3, ticks[min(7, len(ticks) - 1)][1] / 1e3))
