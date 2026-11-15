"""store-law r67: the law in closed loop on a static map - does a boot from n0 avoid the latch, and which floor
estimator is mode-free and honest? A WINDOW-LEVEL REPLAY ON THE p27/p34 STATIC MAP, not a measurement.

Usage: python -I r67loop.py <loop-diag/p27/diag-ld> <loop-diag/p34/run/diag-ld>

Model (N = 1 cells only: c2, c1s; c8's two-leg coupling is not modelled):
  - the store size n_k sets window k's echo statistics through the static map: late medians Tmin(W), T(W) per pool,
    linear in W between pools, FLAT below the smallest pool (unmeasured there: stated, not assumed physical);
  - per-window noise: a joint (tmin, tp50) residual drawn from the late windows of the nearest pool's rows;
  - window 1 of a run carries the measured start-up transient: its (tmin, tp50) offset from the late median, drawn
    from the first windows of the smallest-pool rows;
  - m_k = min of tmin over the last 40 windows (Copa's 10 s filter at 250 ms windows); l_k = running min of tmin;
  - the law: R_k = (F_k + sigma + q l_k + T_k - Tmin_k) / T_k,  n_{k+1} = clamp(n_k R_k, 10, 2048), q = 0.10.
Estimators for F_k:
  A    R6.2 as written: b_k = running max of (tmin - m) over windows k <= 8, then frozen; F = min(m, l + b) + b
  Ah   A with the boot hold (n held at n0 while k <= 8): a phase, shown only as the comparison R6.7 asked about
  B    absolute window minimum, median of windows k <= 4 (time-indexed: "first K")
  C    absolute window minimum, median over the 4 windows of LOWEST store size n seen so far (ties: the most
       recent) - no time index; refreshed whenever the loop revisits a lower n
A last block steps the path floor (tmin and tp50 both) by +D ms at window 120 (a route increase) and reports C's
and A's response: n and F before/after, and whether C refreshes.
Below the smallest pool the map is unmeasured; two readings are run and reported side by side:
  BELOW = flat   the smallest pool's statistics hold down to n = 10 (this makes a low start honest by construction)
  BELOW = slope  T_min (and T, with S = T - T_min kept) continue the secant of the two smallest pools down to n,
                 clamped at the smallest pool's lifetime minimum l (c2: 0.020 ms/slot from W 64-96; c1s 0.003)
A's F before its K windows close is a modelling choice: b = the running max so far (the gauge's printed b0p).
Reported per (cell, n0, estimator): latched F against the small-pool truth, the landing n* (median n over the last
80 of 240 windows, 20 seeds: median [min-max]), and windows to first reach 0.95 of n*.
"""
import glob, os, random, re, statistics as St, sys
from collections import defaultdict

P27, P34 = sys.argv[1], sys.argv[2]
LD = re.compile(r'\[LOOPDIAG\] t=([\d.]+)s p0 n=\d+ .*?rtt_min_us=(\d+) rtt_p10_us=\d+ rtt_p50_us=(\d+)')
CELLS = {'c2-100': dict(sig=1.8, late=4.0, knee=169), 'c1s-400': dict(sig=0.55, late=2.0, knee=903)}
SMALL = {'c2-100': 64, 'c1s-400': 256}
EXCL = {('c1s-400', 192)}   # the s41b quinn-collapse pool (s56/p27 note), not a store-size state


def load():
    rows = defaultdict(list)
    for d in (P27, P34):
        for f in sorted(glob.glob(os.path.join(d, '*-DEF*-c.log'))):
            m = re.match(r'(c\w+-\d+)-DEF(\d+)-', os.path.basename(f))
            cell, cap = m.group(1), int(m.group(2))
            if cell not in CELLS or (cell, cap) in EXCL:
                continue
            v = [(float(t), int(a) / 1e3, int(b) / 1e3) for t, a, b in LD.findall(open(f, errors='replace').read())]
            if len(v) >= 8:
                rows[(cell, cap)].append(v)
    return rows


rows = load()
MAP, RES, BOOT = {}, {}, {}
for (cell, cap), vs in rows.items():
    lt = CELLS[cell]['late']
    late = [[x for x in v if x[0] >= lt] for v in vs]
    late = [l for l in late if len(l) >= 3]
    if not late:
        continue
    tm = St.median([x[1] for l in late for x in l])
    tp = St.median([x[2] for l in late for x in l])
    MAP.setdefault(cell, []).append((cap, tm, tp))
    RES[(cell, cap)] = [(x[1] - tm, x[2] - tp) for l in late for x in l]
    if cap == SMALL[cell]:
        BOOT[cell] = [(v[0][1] - tm, v[0][2] - tp) for v in vs]
for c in MAP:
    MAP[c].sort()


LSMALL = {}
for (cell, cap), vs in rows.items():
    if cap == SMALL[cell]:
        LSMALL[cell] = St.median([min(x[1] for x in v) for v in vs])
BELOW = ['flat']


def stat(cell, n, rng, first):
    pts = MAP[cell]
    if n <= pts[0][0]:
        tm, tp, cap = pts[0][1], pts[0][2], pts[0][0]
        if BELOW[0] == 'slope':
            d = (n - pts[0][0]) * (pts[1][1] - pts[0][1]) / (pts[1][0] - pts[0][0])
            d = max(d, LSMALL[cell] - tm)
            tm, tp = tm + d, tp + d
    elif n >= pts[-1][0]:
        tm, tp, cap = pts[-1][1] + (n - pts[-1][0]) * (pts[-1][1] - pts[-2][1]) / (pts[-1][0] - pts[-2][0]), \
            pts[-1][2] + (n - pts[-1][0]) * (pts[-1][2] - pts[-2][2]) / (pts[-1][0] - pts[-2][0]), pts[-1][0]
    else:
        for (w0, a0, b0), (w1, a1, b1) in zip(pts, pts[1:]):
            if w0 <= n <= w1:
                f = (n - w0) / (w1 - w0)
                tm, tp = a0 + f * (a1 - a0), b0 + f * (b1 - b0)
                cap = w0 if f < 0.5 else w1
                break
    dm, dp = rng.choice(BOOT[cell] if first else RES[(cell, cap)])
    tmin = max(0.1, tm + dm)
    return tmin, max(tmin, tp + dp)


def run(cell, n0, est, seed, nw=240, q=0.10, step=0.0, K=None):
    rng = random.Random(seed)
    sig = CELLS[cell]['sig']
    KA = K or 8
    KC = K or 4
    n, hist, l, bmax, traj = float(n0), [], 1e9, 0.0, []
    for k in range(1, nw + 1):
        tmin, tp = stat(cell, n, rng, k == 1)
        if k > 120:
            tmin, tp = tmin + step, tp + step
        hist.append((n, k, tmin))
        l = min(l, tmin)
        m = min(x[2] for x in hist[-40:])
        if est in ('A', 'Ah'):
            if k <= KA:
                bmax = max(bmax, tmin - m)
            F = min(m, l + bmax) + bmax
        elif est == 'B':
            F = St.median([x[2] for x in hist[:KC]])
        else:
            F = St.median([x[2] for x in sorted(hist, key=lambda x: (x[0], -x[1]))[:KC]])
        R = (F + sig + q * l + tp - tmin) / tp
        traj.append((n, F, tmin, est in ('A', 'Ah') and m >= l + bmax))
        if est == 'Ah' and k <= KA:
            continue
        n = min(2048.0, max(10.0, n * R))
    return traj


truth = {c: [p for p in MAP[c] if p[0] == SMALL[c]][0][1] for c in MAP}
print('static map (late medians, ms):')
for c in sorted(MAP):
    print(' %s: %s' % (c, ' '.join('%d:%.2f/%.2f' % p for p in MAP[c])))
print(' small-pool truth (late Tmin): %s' % ', '.join('%s %.2f' % (c, truth[c]) for c in sorted(truth)))
for mode in ('flat', 'slope'):
  BELOW[0] = mode
  print()
  print('BELOW = %s' % mode)
  print('cell n0 est | F latched - truth (ms), median [min-max] | landing n* median [min-max] vs knee | '
      'windows to 0.95 n* | windows 1-8: max n, max Tmin - truth')
  for cell in sorted(MAP):
    for n0 in (10, 128):
        for est in ('A', 'Ah', 'B', 'C'):
            Fs, ns, ramp, mx, mq = [], [], [], [], []
            for s in range(20):
                tr = run(cell, n0, est, s)
                ns.append(St.median([x[0] for x in tr[-80:]]))
                Fs.append(tr[-1][1] - truth[cell])
                ramp.append(next(i for i, x in enumerate(tr) if x[0] >= 0.95 * ns[-1]))
                mx.append(max(x[0] for x in tr[:8]))
                mq.append(max(x[2] for x in tr[:8]) - truth[cell])
            print(' %s %3d %-2s | %+.2f [%+.2f %+.2f] | %4.0f [%4.0f-%4.0f] (%+.0f) | %3d | %4.0f, %+.2f' % (
                cell, n0, est, St.median(Fs), min(Fs), max(Fs), St.median(ns), min(ns), max(ns),
                St.median(ns) - CELLS[cell]['knee'], St.median(ramp), St.median(mx), St.median(mq)))

  print()
  print('A (R6.2 as written) swept over K, BELOW = %s: landing n* median [min-max] and the anc bind fraction'
        ' (m >= l + b) over the last 80 windows; s60flip (github/s60-design@f8ea7d6c) is the static counterpart' % mode)
  for cell in sorted(MAP):
    for n0 in (10, 128):
        line = []
        for K in range(1, 9):
            ns, anc = [], []
            for s in range(20):
                tr = run(cell, n0, 'A', s, K=K)
                ns.append(St.median([x[0] for x in tr[-80:]]))
                anc.append(sum(x[3] for x in tr[-80:]) / 80)
            line.append('K%d %4.0f [%4.0f-%4.0f] anc %.2f' % (K, St.median(ns), min(ns), max(ns), St.median(anc)))
        print(' %s n0 %3d (knee %d) | %s' % (cell, n0, CELLS[cell]['knee'], ' | '.join(line)))

BELOW[0] = 'flat'
print()
print('route increase +D ms at window 120 (n0 = 10): n median over windows 100-120 -> 200-240, min n after the step,'
      ' F - truth at the end')
for cell in sorted(MAP):
    for D in (1.0, 3.0, 10.0):
        for est in ('A', 'C'):
            a, b, lo, F = [], [], [], []
            for s in range(20):
                tr = run(cell, 10, est, s, step=D)
                a.append(St.median([x[0] for x in tr[99:120]]))
                b.append(St.median([x[0] for x in tr[199:]]))
                lo.append(min(x[0] for x in tr[120:]))
                F.append(tr[-1][1] - truth[cell])
            print(' %s D %+4.1f %s | n %4.0f -> %4.0f [%4.0f-%4.0f] | min n after %4.0f | F - truth %+.2f' % (
                cell, D, est, St.median(a), St.median(b), min(b), max(b), St.median(lo), St.median(F)))
