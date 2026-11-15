"""s60 ruling entry 6 (advisor check): is the row-to-row scatter of l the loop's own scatter, or sampling noise of
a minimum? If the ABSOLUTE late floor is tighter across rows than l is, the scatter is in l alone, and a session-level
calibration value removes it from R~ (sigma_R is then the row sd of the absolute T_min over T-bar).

Usage: python -I s60abs.py <s56 .../run/rep56/diag-ld> <loop-diag/p27/diag-ld>

s56 (per arm, leg 0): per row l = min rtp_us; LATE = ticks t >= 15 s (c1s 5 s) with src > 0; abs = median LATE tick echo
mean (top-level rtt at one leg, per-leg rtt at c8); rel = abs - l. Printed: row sd of l, of abs, and of rel; and the
Pearson correlation of l with rel (near -1: l's scatter is offset by rel, i.e. noise in l only).
p27 (per cap, leg 0): l = min window rtt_min_us; abs = median late (t >= 4 s) window rtt_min_us (a true window minimum).
"""
import glob, os, re, statistics as St, sys
from collections import defaultdict


def corr(x, y):
    if len(x) < 3 or St.pstdev(x) == 0 or St.pstdev(y) == 0:
        return float('nan')
    mx, my = St.mean(x), St.mean(y)
    return sum((a - mx) * (b - my) for a, b in zip(x, y)) / (len(x) * St.pstdev(x) * St.pstdev(y))


def show(tag, by):
    for k in sorted(by, key=lambda k: (k[0], k[1])):
        v = by[k]
        if len(v) < 2:
            continue
        l = [x[0] for x in v]
        a = [x[1] for x in v]
        r = [b - c for b, c in zip(a, l)]
        print('%s %s W%d n%d | sd l %.2f | sd abs %.2f | sd rel %.2f | corr(l, rel) %+.2f | abs %s'
              % (tag, k[0], k[1], len(v), St.stdev(l), St.stdev(a), St.stdev(r), corr(l, r),
                 ' '.join('%.2f' % x for x in a)))


S56, P27 = sys.argv[1], sys.argv[2]
LEG = r' p0:infl=.*?rtt=(\d+)/wrtt=\d+/rtp\d+ms rtp_us=(\d+) '
by = defaultdict(list)
for f in sorted(glob.glob(os.path.join(S56, '*-c.log'))):
    m = re.match(r'(c\w+-\d+)@def-P(\d+)-s\d+-r\d+-', os.path.basename(f))
    if not m:
        continue
    cell, P = m.group(1), int(m.group(2))
    W = 2 * P if cell.startswith('c8') else P
    t0 = 5.0 if cell.startswith('c1s') else 15.0
    T = []
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'):
            continue
        t = float(re.search(r't=([\d.]+)s', L).group(1))
        src = re.search(r' src=(\d+)', L)
        g = re.search(LEG, L)
        top = re.search(r' rtt=([\d.]+)ms', L)
        two = ' p1:infl=' in L
        if g:
            T.append((t, int(src.group(1)) if src else 0, int(g.group(2)) / 1e3,
                      float(g.group(1)) if two or not top else float(top.group(1))))
    late = [x for x in T if x[0] >= t0 and x[1] > 0]
    if T and late:
        by[(cell, W)].append((min(x[2] for x in T), St.median([x[3] for x in late]),
                              St.median([x[2] for x in late])))
show('s56', by)

# Under the session calibration the row's m-hat enters F iff m_row < l_c + b0_c (anc 0). Per row: the late median of
# rtp_us (Copa's m-hat, the gauge's m), its row sd per arm, and the anc state against the session threshold
# (l_c = the W 128 / P80 arm median l on s56; b0_c = p27's cap-128 b0p medians: c2 2.18, c8 leg 0 2.49).
THR = {'c2-100': (128, 2.18), 'c8-100': (160, 2.49)}
print()
print('s56 late m-hat (rtp_us) per row against the session anc threshold l_c + b0_c:')
for cell, (wc, b0c) in THR.items():
    lc = St.median([x[0] for x in by[(cell, wc)]])
    thr = lc + b0c
    for k in sorted(w for (c, w) in by if c == cell):
        v = by[(cell, k)]
        mm = [x[2] for x in v]
        a0 = sum(x < thr for x in mm)
        print('%s W%d | threshold %.2f | m-hat %s | sd %.2f | anc 0 on %d of %d rows'
              % (cell, k, thr, ' '.join('%.2f' % x for x in mm), St.stdev(mm) if len(mm) > 1 else 0.0, a0, len(mm)))

LD = re.compile(r'\[LOOPDIAG\] t=([\d.]+)s p0 n=\d+ .*?rtt_min_us=(\d+) ')
by = defaultdict(list)
for f in sorted(glob.glob(os.path.join(P27, '*-DEF*-c.log'))):
    m = re.match(r'(c\w+-\d+)-DEF(\d+)-', os.path.basename(f))
    v = [(float(a), int(b) / 1e3) for a, b in (g.groups() for g in map(LD.search, open(f, errors='replace')) if g)]
    late = [x[1] for x in v if x[0] >= 4.0]
    if v and late:
        by[(m.group(1), int(m.group(2)))].append((min(x[1] for x in v), St.median(late)))
show('p27', by)
