"""s60 ruling entry 6, item 4: do the floors (l, and the boot-queue proxy for b0p) drift with ROW ORDER on s56's small
pools? If not, a session-level calibration statistic (one value per cell and leg, over all valid calibration rows)
is as good as per-replicate pairing and removes the calibration-row scatter.

Usage: python -I s60drift.py <extracted .../run/rep56/diag-ld>

Per row of the small pools (c2 P128/P160, c8 P80/P112, c1s P384/P512): the replicate index r (the s56 run order:
replicates ran in sequence), l = min rtp_us over the row (ms), and bootq_med = the median over the first 8 ticks of
(echo mean - l) (the boot-queue proxy; [DIAG] has no window minimum). Within each pool both are centred on the pool
median, then pooled per cell and leg: Spearman of the centred value against r, with a one-sided-agnostic two-sided
permutation p (20 000 shuffles, seed 60).
"""
import glob, os, random, re, statistics as St, sys
from collections import defaultdict

D = sys.argv[1]
SMALL = {'c2-100': (128, 160), 'c8-100': (80, 112), 'c1s-400': (384, 512)}
LEG = r' p%d:infl=.*?rtt=(\d+)/wrtt=\d+/rtp\d+ms rtp_us=(\d+) '


def ranks(v):
    o = sorted(range(len(v)), key=lambda i: v[i])
    r = [0.0] * len(v)
    i = 0
    while i < len(o):
        j = i
        while j + 1 < len(o) and v[o[j + 1]] == v[o[i]]:
            j += 1
        for k in range(i, j + 1):
            r[o[k]] = (i + j) / 2.0
        i = j + 1
    return r


def spearman(x, y):
    rx, ry = ranks(x), ranks(y)
    mx, my = St.mean(rx), St.mean(ry)
    num = sum((a - mx) * (b - my) for a, b in zip(rx, ry))
    den = (sum((a - mx) ** 2 for a in rx) * sum((b - my) ** 2 for b in ry)) ** 0.5
    return num / den if den else 0.0


rows = defaultdict(list)
for f in sorted(glob.glob(os.path.join(D, '*-c.log'))):
    m = re.match(r'(c\w+-\d+)@def-P(\d+)-s(\d+)-r(\d+)-', os.path.basename(f))
    if not m or int(m.group(2)) not in SMALL.get(m.group(1), ()):
        continue
    cell, P, rep = m.group(1), int(m.group(2)), int(m.group(4))
    T = []
    for L in open(f, errors='replace'):
        if L.startswith('[DIAG]'):
            top = re.search(r' rtt=([\d.]+)ms', L)
            x = {'top': float(top.group(1)) if top else None}
            for i in (0, 1):
                g = re.search(LEG % i, L)
                if g:
                    x[i] = (int(g.group(1)), int(g.group(2)))
            T.append(x)
    for i in (0, 1):
        v = [x for x in T if i in x]
        if not v:
            continue
        l = min(x[i][1] for x in v) / 1e3
        boot = v[:8]
        one = len([k for k in (0, 1) if k in T[0]]) == 1
        bq = St.median([(x['top'] if one and x['top'] else x[i][0]) - l for x in boot])
        rows[(cell, i)].append((P, rep, l, bq))

print('cell leg | rows (P r l bootq_med) | Spearman(centred l, r) p | Spearman(centred bootq, r) p')
rnd = random.Random(60)
for k in sorted(rows):
    v = rows[k]
    cen = []
    for P in sorted({x[0] for x in v}):
        g = [x for x in v if x[0] == P]
        ml, mb = St.median([x[2] for x in g]), St.median([x[3] for x in g])
        cen += [(x[1], x[2] - ml, x[3] - mb) for x in g]
    r = [c[0] for c in cen]
    out = []
    for j in (1, 2):
        y = [c[j] for c in cen]
        rho = spearman(r, y)
        hits = sum(abs(spearman(rnd.sample(r, len(r)), y)) >= abs(rho) for _ in range(20000))
        out.append('%+.2f p %.3f' % (rho, hits / 20000))
    print('%s p%d | %s | %s | %s' % (k[0], k[1], ' '.join('P%d/r%d/%.2f/%.1f' % x for x in sorted(v, key=lambda x: (x[0], x[1]))),
                                    out[0], out[1]))
