"""One-sided upper bounds on b0, evaluated against each row's own no-queue brake.

Usage: python -I b0bound.py <loop-diag/p27/diag-ld> <loop-diag/p34/run/diag-ld>

No-queue rows: the static caps at or below each cell's BDP (c2 64-128, c8 128-192, c1s 192-256, c1d 384).
Per row and leg, brake_k = Tmin_k - m_k over its 250 ms [LOOPDIAG] windows (m_k = Copa's running 10 s min at that
window). The row's true no-queue brake b0_row = median brake over windows with t >= 1 s.
Candidates computed from the row's FIRST K windows only (boot calibration, K = 8, i.e. 2 s; the law starts at n0 = 128,
at or below the BDP at all four cells):
  med8  = median of the first 8 brakes          (an unbiased boot estimate)
  p90_8 = 90th percentile (nearest rank) of the first 8
  max8  = maximum of the first 8                 (sample maximum: no quantile constant)
and, over the whole row, max_row = the maximum window brake.
For each candidate: margin = candidate - b0_row + q*l (l = the row's boot anchor, q = 0.05, Realtime: the thinnest);
the law collapses at no queue iff margin < 0. Cost = max(0, candidate - b0_row): extra standing queue in ms.
"""
import glob, os, re, statistics as St, sys
from collections import defaultdict

LD = re.compile(r'\[LOOPDIAG\] t=([\d.]+)s p(\d) n=(\d+) .*?rtt_min_us=(\d+) .*?rtprop_us=(\d+)')
NOQ = {'c2-100': (64, 96, 128), 'c8-100': (128, 192), 'c1s-400': (192, 256), 'c1d-400': (384,)}
Q = 0.05
for D in sys.argv[1:]:
    print('##', os.path.relpath(D).replace(os.sep, '/'))
    res = defaultdict(list)
    for f in sorted(glob.glob(os.path.join(D, '*-DEF*-c.log'))):
        c, cap = re.match(r'(c\w+-\d+)-DEF(\d+)-', os.path.basename(f)).groups()
        if int(cap) not in NOQ.get(c, ()):
            continue
        w = defaultdict(list)
        for L in open(f, errors='replace'):
            x = LD.search(L)
            if x and int(x.group(3)) > 0:
                t, p, _, tmin, rtp = x.groups()
                w[int(p)].append((float(t), int(tmin) / 1e3, int(rtp) / 1e3))
        for p, xs in w.items():
            late = [a - r for t, a, r in xs if t >= 1.0]
            first = [a - r for t, a, r in xs[:8]]
            if len(late) < 4 or len(first) < 8:
                continue
            b0 = St.median(late)
            l = min(r for _, _, r in xs)
            ql = Q * l
            fs = sorted(first)
            cand = {'med8': St.median(first), 'p90_8': fs[min(7, int(0.9 * 8))], 'max8': fs[-1], 'max_row': max(late)}
            res[(c, p)].append((int(cap), b0, ql, cand))
    for (c, p), rows in sorted(res.items()):
        print('== %s p%d  rows %d  b0_row %.2f-%.2f ms  q*l %.2f-%.2f ms' % (
            c, p, len(rows), min(r[1] for r in rows), max(r[1] for r in rows), min(r[2] for r in rows),
            max(r[2] for r in rows)))
        for k in ('med8', 'p90_8', 'max8', 'max_row'):
            mg = [r[3][k] - r[1] + r[2] for r in rows]
            cost = [max(0.0, r[3][k] - r[1]) for r in rows]
            print('   %-7s  min margin %+.2f ms  collapses %d/%d rows  cost median %.2f max %.2f ms' % (
                k, min(mg), sum(m < 0 for m in mg), len(rows), St.median(cost), max(cost)))
