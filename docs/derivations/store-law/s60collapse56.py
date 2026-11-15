"""s60 ruling entry 9, item 2: the absolute collapse rule per cell and leg, derived from each cell's own queue-free
spread, and run on s56's small-pool rows (which must NOT be flagged).

Usage: python -I s60collapse56.py <extracted .../run/rep56/diag-ld>

Per row and leg, LATE ticks (t >= 15 s; c1s 5 s; src > 0): excess = tick echo mean - l (top-level 0.1 ms rtt at one
leg, per-leg 1 ms rtt at c8; l = min rtp_us of the leg over the row).
  E90(cell, leg) = the 90th percentile of the excess over all LATE ticks of the cell's smallest pools (queue-free
                   spread: c2 W 128/160, c8 W 160/224, c1s W 384).
  s56's collapse-flagged rows are left out of E90. The margin is the one validated on s57a's c1s rows: X = 5 ms there, and E90 at c1s = e (printed), so
  delta = 5 - e. The rule per cell and leg: X = E90(cell, leg) + delta.
Printed per small-pool row: the share of its LATE ticks above X (the rule flags at > 0.10).
"""
import glob, os, re, statistics as St, sys
from collections import defaultdict

D = sys.argv[1]
SMALL = {'c2-100': (128, 160), 'c8-100': (80, 112), 'c1s-400': (384,)}
COLLAPSED = {('c1s-400', 384, '42', '1')}     # s56's quinn-collapse-flagged rows among these (s56cls.py COLLAPSED)
LEG = r' p%d:infl=.*?rtt=(\d+)/wrtt=\d+/rtp\d+ms rtp_us=(\d+) '
ex = defaultdict(list)
rows = []
for f in sorted(glob.glob(os.path.join(D, '*-c.log'))):
    m = re.match(r'(c\w+-\d+)@def-P(\d+)-s(\d+)-r(\d+)-', os.path.basename(f))
    if not m or int(m.group(2)) not in SMALL.get(m.group(1), ()):
        continue
    cell, P = m.group(1), int(m.group(2))
    t0 = 5.0 if cell.startswith('c1s') else 15.0
    T = []
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'):
            continue
        t = float(re.search(r't=([\d.]+)s', L).group(1))
        src = re.search(r' src=(\d+)', L)
        top = re.search(r' rtt=([\d.]+)ms', L)
        x = {'t': t, 'src': int(src.group(1)) if src else 0, 'top': float(top.group(1)) if top else None}
        for i in (0, 1):
            g = re.search(LEG % i, L)
            if g:
                x[i] = (int(g.group(1)), int(g.group(2)) / 1e3)
        T.append(x)
    legs = [i for i in (0, 1) if T and i in T[0]]
    for i in legs:
        l = min(x[i][1] for x in T if i in x and x[i][1] > 0)
        late = [x for x in T if x['t'] >= t0 and x['src'] > 0 and i in x]
        e = [((x['top'] if len(legs) == 1 else float(x[i][0])) - l) for x in late]
        if (cell, P, m.group(3), m.group(4)) not in COLLAPSED:
            ex[(cell, i)] += e
        rows.append((cell, P, m.group(3), m.group(4), i, e))


def pq(v, p):
    v = sorted(v)
    return v[int(p * (len(v) - 1))]


def p90(v):
    return pq(v, 0.9)


# Ruling entry 9 (advisor): the margin from each cell's OWN tick spread, not c1s's: X = E90 + 2*(E90 - E50), i.e.
# the queue-free 90th percentile plus twice its upper half-spread. It must reproduce the s57a-validated 5 ms at c1s
# (printed; s60collapse.py re-checks the c1s value on s57a's rows).
X = {k: p90(v) + 2 * (p90(v) - pq(v, 0.5)) for k, v in ex.items()}
for k in sorted(X):
    v = ex[k]
    print('%s leg %d: E50 %.2f E90 %.2f E99 %.2f ms -> X = E90 + 2(E90 - E50) = %.2f ms'
          % (k[0], k[1], pq(v, 0.5), p90(v), pq(v, 0.99), X[k]))
print()
print('s56 small-pool rows: share of LATE ticks above X (flag at > 0.10); and above the c1s-fitted 5 ms')
for cell, P, s, r, i, e in rows:
    x = X[(cell, i)]
    print('%s P%d s%s r%s leg %d: share > X %.3f %s | share > 5 ms %.3f'
          % (cell, P, s, r, i, sum(v > x for v in e) / len(e), 'FLAG' if sum(v > x for v in e) / len(e) > 0.10 else '',
             sum(v > 5 for v in e) / len(e)))
