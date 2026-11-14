"""Rev-3 review checks on the status-56 sweep (rep56-diag-ld.tgz, extracted): one who-limits rule for every cell,
occupancy, the send rate per class, and the per-row spread of the law's point from each row's own boot inputs.

Usage: python -I s56cls.py <extracted .../run/rep56/diag-ld>

(1) ONE class rule, per LATE tick (sending ticks, t >= 15 s; c1s t >= 5 s), on every cell:
      store FULL  iff a >= 0.95 W        (a = counted store, W = the tick's cap)
      quinn BACKLOGGED iff B >= 98       (B = sum over legs of dgq hand - tx)
    -> four classes FULL+B, FULL, OPEN+B, OPEN. Per row: the class shares, the mean a/W, and the mean send rate
    sum_i d(tx_i)/dt over the ticks of each class.
(2) Per-row law point from the row's own boot inputs (first 8 [DIAG] ticks, i.e. 2 s):
      l_i   = the lowest rtp_us over those ticks (the lifetime echo minimum so far)
      sig_i = the highest qsp_us over those ticks (P90 - P50 of the leg's last 256 echo samples): sigma-hat+
    [DIAG] has no window minimum, so b0+ cannot be read per row: the cell's ledger over-read (b0bound.out
    medians: c2 0.41, c8 1.0, c1s 0.12 ms) stands in, and the count is repeated with the ledger max (0.90, 48.8, 0.20).
    target queue (Bulk q = 0.10, Realtime 0.05): b0x + sig_0 + q*l_0 (+ 1/2 * max(0, l_1 + b0_1 - l_0 - b0_0) at c8,
    b0 = 2.85 / 3.91 ms). The law's W is read off the cell's arm-median (W, q) curve (status 56 score lines
    260-266), and compared with the cell's 0.95 knee (c2 169, c8 340, c1s 903).
Rows listed in COLLAPSED are left out of (1) and reported in (2).
"""
import glob, os, re, statistics as St, sys
from collections import defaultdict

D = sys.argv[1]
COLLAPSED = {('c1s-400', 'P1024', 's7', 'r2'), ('c1s-400', 'P2048', 's42', 'r3'), ('c1s-400', 'P2048', 's42', 'r9'),
             ('c1s-400', 'P2048', 's7', 'r8'), ('c1s-400', 'P384', 's42', 'r1'), ('c1s-400', 'P512', 's42', 'r1'),
             ('c1s-400', 'P512', 's42', 'r5'), ('c1s-400', 'P512', 's42', 'r9'), ('c1s-400', 'P512', 's7', 'r2')}
CURVE = {'c8-100': [(160, 0.0), (224, 3.9), (320, 9.1), (512, 21.9), (768, 41.7), (1280, 84.2)],
         'c2-100': [(128, 0.0), (160, 0.0), (192, 3.0), (256, 9.0), (384, 22.0), (2048, 192.0)],
         'c1s-400': [(384, 0.0), (512, 1.0), (768, 3.0), (1024, 5.5), (2048, 15.0)]}
KNEE95 = {'c8-100': 340, 'c2-100': 169, 'c1s-400': 903}
B0X = {'c8-100': (1.0, 48.8), 'c2-100': (0.41, 0.90), 'c1s-400': (0.12, 0.20)}
B0 = {0: 2.85, 1: 3.91}
LEG = r' p%d:infl=.*?rtp_us=(\d+) .*?qsp_us=(\d+)/n'


def w_at(cell, q):
    c = CURVE[cell]
    if q <= c[0][1]:
        # flat below the first queue: the lowest W with that queue
        return max(w for w, qq in c if qq == c[0][1])
    for (w1, q1), (w2, q2) in zip(c, c[1:]):
        if q1 <= q <= q2 and q2 > q1:
            return w1 + (q - q1) * (w2 - w1) / (q2 - q1)
    return c[-1][0]


cls_out = defaultdict(list)
pts = defaultdict(list)
for f in sorted(glob.glob(os.path.join(D, '*-c.log'))):
    m = re.match(r'(c\w+-\d+)@def-(P\d+)-(s\d+)-(r\d+)-', os.path.basename(f))
    if not m:
        continue
    key = m.groups()
    cell = key[0]
    t0 = 5.0 if cell.startswith('c1s') else 15.0
    allt = []
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'):
            continue
        t = float(re.search(r't=([\d.]+)s', L).group(1))
        w = re.search(r'win=(\d+)/(\d+)', L)
        src = re.search(r' src=(\d+)', L)
        x = dict(t=t, a=int(w.group(1)), W=int(w.group(2)), src=int(src.group(1)) if src else 0)
        for i in (0, 1):
            d = re.search(r'dgq%d\[hand=(\d+) tx=(\d+)' % i, L)
            lg = re.search(LEG % i, L)
            if d:
                x['tx%d' % i] = int(d.group(2)); x['B%d' % i] = int(d.group(1)) - int(d.group(2))
            if lg:
                x['rtp%d' % i] = int(lg.group(1)) / 1e3; x['qsp%d' % i] = int(lg.group(2)) / 1e3
        allt.append(x)
    legs = [i for i in (0, 1) if allt and 'tx%d' % i in allt[0]]
    # (2) boot inputs
    boot = allt[:8]
    l = {i: min(x['rtp%d' % i] for x in boot if x.get('rtp%d' % i, 0) > 0) for i in legs
         if any(x.get('rtp%d' % i, 0) > 0 for x in boot)}
    sg = {i: max(x.get('qsp%d' % i, 0) for x in boot) for i in legs}
    if cell in CURVE and 0 in l:
        for q in (0.05, 0.10):
            for which, b0x in (('med', B0X[cell][0]), ('max', B0X[cell][1])):
                tq = b0x + sg[0] + q * l[0]
                if cell == 'c8-100' and 1 in l:
                    tq += 0.5 * max(0.0, l[1] + B0[1] - l[0] - B0[0])
                pts[(cell, q, which)].append((w_at(cell, tq), tq, key[1:], key in COLLAPSED))
    if key in COLLAPSED:
        continue
    # (1) one class rule
    late = [x for x in allt if x['t'] >= t0 and x['src'] > 0]
    c = defaultdict(int); rate = defaultdict(list); occ = []
    for a, b in zip(late, late[1:]):
        dt = b['t'] - a['t']
        if dt <= 0 or any('tx%d' % i not in a or 'tx%d' % i not in b for i in legs):
            continue
        R = sum((b['tx%d' % i] - a['tx%d' % i]) / dt for i in legs)
        B = sum(b['B%d' % i] for i in legs)
        k = ('FULL' if b['a'] >= 0.95 * b['W'] else 'OPEN') + ('+B' if B >= 98 else '')
        c[k] += 1; rate[k].append(R); occ.append(b['a'] / b['W'])
    n = sum(c.values()) or 1
    cls_out[key[:2]].append(({k: c[k] / n for k in c}, {k: St.mean(v) for k, v in rate.items()}, St.mean(occ)))

print('(1) ONE class rule: FULL iff a >= 0.95 W; +B iff quinn backlog >= 98. Arm medians over unflagged rows.')
print('cell arm n | mean a/W | share FULL+B FULL OPEN+B OPEN | mean send rate /s in FULL+B FULL OPEN+B OPEN')
K = ('FULL+B', 'FULL', 'OPEN+B', 'OPEN')
for key in sorted(cls_out, key=lambda k: (k[0], int(k[1][1:]))):
    rows = cls_out[key]
    sh = ['%.2f' % St.median([r[0].get(k, 0.0) for r in rows]) for k in K]
    rt = []
    for k in K:
        v = [r[1][k] for r in rows if k in r[1]]
        rt.append('%.0f' % St.median(v) if v else '-')
    print('%s %s n%d | %.3f | %s | %s' % (key[0], key[1], len(rows), St.median([r[2] for r in rows]),
                                         ' '.join(sh), ' '.join(rt)))
print()
print('(2) the law\'s point per row from its own boot inputs (l, sigma+), b0+ over-read from the ledger (med / max)')
for (cell, q, which) in sorted(pts):
    v = pts[(cell, q, which)]
    ws = [p[0] for p in v]
    below = sum(p[0] < KNEE95[cell] for p in v)
    print('%s q=%.2f b0x=%s: rows %d  W %.0f-%.0f (median %.0f)  target queue %.1f-%.1f ms  below the 0.95 knee (%d): %d'
          % (cell, q, which, len(v), min(ws), max(ws), St.median(ws), min(p[1] for p in v), max(p[1] for p in v),
             KNEE95[cell], below))
