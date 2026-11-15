"""store-law R6.7 / r67: the two floors, estimated honestly, on the static-cap window-minimum ledgers (p27, p34).

Usage: python -I r67floor.py <loop-diag/p27/diag-ld> <loop-diag/p34/run/diag-ld>

Per DEF row (RWM_STORE = W static, law off), per leg, per 250 ms window k (from [LOOPDIAG], paired with [DIAG] by t):
  tmin_k (window echo minimum), tp50_k (window median = T-hat), m_k (rtprop_us = Copa's running minimum),
  dlv_k (delivered /s), a_k (store occupancy, [DIAG] win=a/W).
LATE = t >= 4 s. Sections:
  (1) truth: late T-min against W and utilisation u = dlv / plateau; the boot cap 128 as utilisation + ms of rise.
  (2) the brake decomposition: l, b0p(K) = max_{k<=K}(tmin_k - m_k), and the ABSOLUTE first-K window minima.
      Shows that b0p(K) grows with K because m_k falls, while l + b0p is the absolute statistic up to m_K - l.
  (3) candidate floors, K swept 1..16 on the small-pool rows: bias and row sd against the row's own late T-min
      median, and the no-queue collapse margin  F - Tmin_late + sigma + q*l  (q = 0.05, Realtime, thinnest),
      with and without sigma.
  (4) each candidate on every pool: contamination (estimate - small-pool truth), i.e. what a static pool latches.
  (5) R crossing per cell with each candidate's floor taken at the small pools (session value) and, for contrast,
      with each row's own boot estimate; against the knee.
Candidates (per leg):
  E0  l + b0p(K)                     (R6.2 at anc 0, the gauge's l and b0p)
  E0a l + 2 b0p(K)                   (R6.2 at anc 1, the attractor once m-hat rides the standing queue)
  E1  median_{k<=K} tmin_k           (absolute window minimum, central, first K windows)
  E1x max_{k<=K} tmin_k              (absolute, one-sided)
  E2  median of tmin over the K windows of LOWEST occupancy a_k (ties: earliest) - no time index
"""
import glob, os, re, statistics as St, sys
from collections import defaultdict

P27, P34 = sys.argv[1], sys.argv[2]
LD = re.compile(r'\[LOOPDIAG\] t=([\d.]+)s p(\d) n=(\d+) .*?rtt_min_us=(\d+) rtt_p10_us=\d+ rtt_p50_us=(\d+) '
                r'rtprop_us=(\d+).*?dlv_rate=(\d+)')
DG = re.compile(r'^\[DIAG\] t=([\d.]+)s win=(\d+)/(\d+)')
SIG = {'c2-100': (1.8,), 'c8-100': (2.3, None), 'c1s-400': (0.55,), 'c1d-400': (0.55, 0.55)}
# c8 leg-1 sigma at load, s56 arm medians (s60boot.out, github/s60-design@8536f053), by W
SIG1 = [(80, 4.92), (112, 4.95), (160, 4.93), (256, 15.45), (384, 26.07), (640, 31.56), (1280, 31.56), (4096, 31.56)]
KNEE = {'c2-100': 169, 'c8-100': 340, 'c1s-400': 903}
SMALL = {'c2-100': 64, 'c8-100': 128, 'c1s-400': 256, 'c1d-400': 384}
LATE = {'c2-100': 4.0, 'c8-100': 4.0, 'c1s-400': 2.0, 'c1d-400': 2.0}   # c1 rows end in 4-6 s at large W


def interp(x, pts):
    for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
        if x0 <= x <= x1:
            return y0 + (y1 - y0) * (x - x0) / (x1 - x0)
    return pts[0][1] if x < pts[0][0] else pts[-1][1]


def load(d, tag):
    rows = defaultdict(list)
    for f in sorted(glob.glob(os.path.join(d, '*-DEF*-c.log'))):
        m = re.match(r'(c\w+-\d+)-DEF(\d+)-', os.path.basename(f))
        cell, cap = m.group(1), int(m.group(2))
        if cell not in SIG:
            continue
        occ, W = {}, defaultdict(list)
        for L in open(f, errors='replace'):
            g = DG.search(L)
            if g:
                occ[g.group(1)] = int(g.group(2))
                continue
            g = LD.search(L)
            if g:
                t, p, n, tmin, tp50, rp, dr = g.groups()
                W[int(p)].append(dict(t=float(t), ts=t, n=int(n), tmin=int(tmin) / 1e3, tp50=int(tp50) / 1e3,
                                      m=int(rp) / 1e3, dlv=int(dr)))
        if 0 not in W or len(W[0]) < 12:
            continue
        for v in W.values():
            for x in v:
                x['a'] = occ.get(x['ts'], cap)
                x['late'] = LATE[cell]
        rows[(cell, cap)].append(dict(src=tag, f=os.path.basename(f), legs=dict(W)))
    return rows


rows = load(P27, 'p27')
for k, v in load(P34, 'p34').items():
    rows[k].extend(v)
cells = sorted({c for c, _ in rows})


def lstat(v):
    lv = [x for x in v if x['t'] >= v[0]['late']]
    if len(lv) < 3:
        return None
    return dict(tmin=St.median([x['tmin'] for x in lv]), tp=St.median([x['tp50'] for x in lv]),
                dlv=St.median([x['dlv'] for x in lv]), l=min(x['tmin'] for x in v))


def est(v, K, which):
    h = v[:K]
    l = min(x['tmin'] for x in v)
    b0p = max(x['tmin'] - x['m'] for x in h)
    if which == 'E0':
        return l + b0p
    if which == 'E0a':
        return l + 2 * b0p
    if which == 'E1':
        return St.median([x['tmin'] for x in h])
    if which == 'E1x':
        return max(x['tmin'] for x in h)
    if which == 'E2':
        o = sorted(range(len(v)), key=lambda i: (v[i]['a'], i))[:K]
        return St.median([v[i]['tmin'] for i in o])


CANDS = ['E0', 'E0a', 'E1', 'E1x', 'E2']

# ---------------------------------------------------------------- (1) truth
print('(1) TRUTH: late window minimum against the static pool (row medians, ms), utilisation u = dlv / plateau')
plateau = {}
for cell in cells:
    for leg in (0, 1):
        d = [lstat(r['legs'][leg])['dlv'] for (c, w), v in rows.items() if c == cell for r in v
             if leg in r['legs'] and lstat(r['legs'][leg])]
        if d:
            plateau[(cell, leg)] = max(d)
for cell in cells:
    for leg in (0, 1):
        if (cell, leg) not in plateau:
            continue
        print(' %s leg %d (plateau dlv %d /s)' % (cell, leg, plateau[(cell, leg)]))
        pts = []
        for cap in sorted(w for c, w in rows if c == cell):
            s = [lstat(r['legs'][leg]) for r in rows[(cell, cap)] if leg in r['legs']]
            s = [x for x in s if x]
            if not s:
                continue
            tm = St.median([x['tmin'] for x in s])
            u = St.median([x['dlv'] for x in s]) / plateau[(cell, leg)]
            pts.append((cap, u, tm))
            print('   W %5d n%d | u %.2f | late Tmin %.2f [%.2f-%.2f] | late T %.2f | l %.2f' % (
                cap, len(s), u, tm, min(x['tmin'] for x in s), max(x['tmin'] for x in s),
                St.median([x['tp'] for x in s]), St.median([x['l'] for x in s])))
        sub = [(w, u, t) for (w, u, t) in pts if u < 0.9 and not (cell == 'c1s-400' and w == 192)]
        if len(sub) < 3:
            print('   below saturation: %d pool(s) with u < 0.9 (W 192 at c1s is the quinn-collapse pool, excluded):'
                  ' no fit' % len(sub))
        else:
            xs = [w for w, _, _ in sub]
            ys = [t for _, _, t in sub]
            mx, my = St.mean(xs), St.mean(ys)
            sl = sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / sum((x - mx) ** 2 for x in xs)
            print('   below saturation (u < 0.9, %d pools): slope %.4f ms/slot, intercept at W=0 %.2f ms'
                  % (len(sub), sl, my - sl * mx))
        b = [p for p in pts if p[0] == 128]
        if b and pts[0][0] < 128:
            print('   boot cap 128: u %.2f, late Tmin %.2f = smallest pool + %.2f ms' % (b[0][1], b[0][2],
                                                                                     b[0][2] - pts[0][2]))
        elif b:
            print('   boot cap 128: u %.2f (no smaller pool measured)' % b[0][1])

# ---------------------------------------------------------------- truth per cell/leg (small pool)
truth = {}
for cell in cells:
    for leg in (0, 1):
        s = [lstat(r['legs'][leg]) for r in rows.get((cell, SMALL[cell]), []) if leg in r['legs']]
        s = [x for x in s if x]
        if s:
            truth[(cell, leg)] = St.median([x['tmin'] for x in s])

# ---------------------------------------------------------------- (2) decomposition
print()
print('(2) DECOMPOSITION on the small-pool rows: b0p(K) = max_k<=K (tmin_k - m_k) against the absolute first-K')
print('    window minima; m_K - l is the part of l + b0p that is not a window minimum')
for cell in cells:
    for leg in (0, 1):
        rs = [r['legs'][leg] for r in rows.get((cell, SMALL[cell]), []) if leg in r['legs']]
        if not rs:
            continue
        for K in (1, 2, 4, 8):
            b = [max(x['tmin'] - x['m'] for x in v[:K]) for v in rs]
            mk = [v[K - 1]['m'] - min(x['tmin'] for x in v) for v in rs]
            ab = [max(x['tmin'] for x in v[:K]) for v in rs]
            print(' %s leg %d W %d K %d | b0p %.2f [%.2f-%.2f] | m_K - l %.2f | l + b0p %.2f | max abs tmin %.2f'
                  ' | truth %.2f' % (cell, leg, SMALL[cell], K, St.median(b), min(b), max(b), St.median(mk),
                                     St.median([min(x['tmin'] for x in v) + bb for v, bb in zip(rs, b)]),
                                     St.median(ab), truth[(cell, leg)]))

print()
print('(2b) WHY b0p GROWS WITH K: c2 pools 64/96/128 (the six rows of s60flip), per row: the window k <= 8 where')
print('     b0p = max(tmin_k - m_k) is attained, the last k <= 8 where the running minimum m drops, and')
print('     max_{k<=8} tmin_k - m_8 (what b0p would be against the converged anchor)')
for cap in (64, 96, 128):
    for r in rows[('c2-100', cap)]:
        v = r['legs'][0][:8]
        br = [x['tmin'] - x['m'] for x in v]
        am = max(range(8), key=lambda i: br[i]) + 1
        drops = [i + 1 for i in range(1, 8) if v[i]['m'] < v[i - 1]['m']]
        print('  W %d %s | b0p %.2f at k=%d | m drops at k=%s | m_1 - m_8 %.2f | max tmin - m_8 %.2f | brakes %s' % (
            cap, r['f'][:-6], max(br), am, drops or '-', v[0]['m'] - v[7]['m'], max(x['tmin'] for x in v) - v[7]['m'],
            ' '.join('%.1f' % b for b in br)))

# ---------------------------------------------------------------- (3) K sweep on small pools
print()
print('(3) K SWEEP on small-pool rows (per row: estimate - own late Tmin median). bias = median, sd = row sd,')
print('    collapse = rows with F - Tmin_late + sigma + q*l < 0 (q = 0.05); [no sigma] = the same without sigma')
for cell in cells:
    for leg in (0, 1):
        rs = [r['legs'][leg] for r in rows.get((cell, SMALL[cell]), []) if leg in r['legs']]
        rs = [v for v in rs if lstat(v)]
        if not rs:
            continue
        sig = SIG[cell][leg] if leg < len(SIG[cell]) and SIG[cell][leg] else interp(SMALL[cell], SIG1)
        print(' %s leg %d, W %d, %d rows, sigma %.2f' % (cell, leg, SMALL[cell], len(rs), sig))
        for c in CANDS:
            line = []
            for K in (1, 2, 3, 4, 6, 8, 12, 16):
                e = [est(v, K, c) - lstat(v)['tmin'] for v in rs]
                mg = [x + sig + 0.05 * lstat(v)['l'] for x, v in zip(e, rs)]
                mg0 = [x + 0.05 * lstat(v)['l'] for x, v in zip(e, rs)]
                line.append('K%d %+.2f/%.2f c%d[%d]' % (K, St.median(e), St.pstdev(e) if len(e) > 1 else 0,
                                                       sum(m < 0 for m in mg), sum(m < 0 for m in mg0)))
            print('   %-3s %s' % (c, ' '.join(line)))

# ---------------------------------------------------------------- (4) contamination on every pool
print()
print('(4) WHAT EACH CANDIDATE READS ON EVERY STATIC POOL, K = 8 (median over rows, ms above small-pool truth)')
for cell in cells:
    for leg in (0, 1):
        if (cell, leg) not in truth:
            continue
        line = []
        for cap in sorted(w for c, w in rows if c == cell):
            rs = [r['legs'][leg] for r in rows[(cell, cap)] if leg in r['legs'] and len(r['legs'][leg]) >= 8]
            if rs:
                line.append('W%d: ' % cap + '/'.join('%.1f' % (St.median([est(v, 8, c) for v in rs])
                                                                - truth[(cell, leg)]) for c in CANDS))
        print(' %s leg %d (%s): %s' % (cell, leg, '/'.join(CANDS), ' | '.join(line)))

# ---------------------------------------------------------------- (5) crossings
print()
print('(5) R CROSSING per cell. R(W) = sum_i w_i (F_i + c_i + sigma_i + q l_i + S_i) / sum_i w_i T_i over LATE')
print('    window medians, w = dlv, S = T - Tmin, c_i = 1/2 max(0, max_j F_j - F_i); q = 0.10 (Bulk).')
print('    "session": F per leg = median of the estimator over the cell small-pool rows (K = 8; E1 also K = 4).')
print('    "own": each row its own boot estimate (what a static pool latches). Crossing = first W where median R < 1.')


def rcell(cell, F, q=0.10, own=None):
    out = []
    for cap in sorted(w for c, w in rows if c == cell):
        R = []
        for r in rows[(cell, cap)]:
            legs = {i: v for i, v in r['legs'].items() if lstat(v)}
            if 0 not in legs:
                continue
            Fi = {i: (own(v) if own else F[i]) for i, v in legs.items() if (own or i in F)}
            top = bot = 0.0
            mxF = max(list(Fi.values()) + ([] if own else list(F.values())))
            for i, v in legs.items():
                if i not in Fi:
                    continue
                s = lstat(v)
                sig = SIG[cell][i] if SIG[cell][i] else interp(cap, SIG1)
                w = s['dlv']
                top += w * (Fi[i] + 0.5 * max(0.0, mxF - Fi[i]) + sig + q * lF[(cell, i)] + s['tp'] - s['tmin'])
                bot += w * s['tp']
            R.append(top / bot)
        if R:
            out.append((cap, St.median(R)))
    cross = None
    out = [p for p in out if p[0] >= SMALL[cell]]
    for (w0, r0), (w1, r1) in zip(out, out[1:]):
        if r0 >= 1 > r1:
            cross = w0 + (r0 - 1) * (w1 - w0) / (r0 - r1)
            break
    if cross is None and out and out[0][1] < 1:
        cross = '< %d' % out[0][0]
    if cross is None and out:
        cross = '> %d' % out[-1][0]
    return out, cross


lF = {}
for (cell, leg) in truth:
    rs = [r['legs'][leg] for r in rows[(cell, SMALL[cell])] if leg in r['legs']]
    lF[(cell, leg)] = St.median([min(x['tmin'] for x in v) for v in rs])
def dknee(cell):
    pts = []
    for cap in sorted(w for c, w in rows if c == cell):
        d = []
        for r in rows[(cell, cap)]:
            s = [lstat(v) for v in r['legs'].values()]
            if all(s):
                d.append(sum(x['dlv'] for x in s))
        if d:
            pts.append((cap, St.median(d)))
    top = max(d for _, d in pts)
    for (w0, d0), (w1, d1) in zip(pts, pts[1:]):
        if d0 < 0.95 * top <= d1:
            return w0 + (0.95 * top - d0) * (w1 - w0) / (d1 - d0)


for cell in ('c2-100', 'c8-100', 'c1s-400'):
    dk = dknee(cell)
    print(' %s (crossing shown vs both knees; knee: s56 goodput %d; this ledger, 0.95 of the delivery plateau, %.0f)' % (cell, KNEE[cell],
                                                                                         dknee(cell)))
    variants = [('truth (small-pool late Tmin)', {i: truth[(cell, i)] for i in (0, 1) if (cell, i) in truth}, None)]
    for c, K in [('E0', 8), ('E0a', 8), ('E1', 8), ('E1', 4), ('E1x', 8), ('E2', 8)]:
        F = {}
        for i in (0, 1):
            rs = [r['legs'][i] for r in rows[(cell, SMALL[cell])] if i in r['legs'] and len(r['legs'][i]) >= K]
            if rs:
                F[i] = St.median([est(v, K, c) for v in rs])
        variants.append(('%s K%d session' % (c, K), F, None))
    for c in ('E0', 'E0a', 'E1'):
        variants.append(('%s K8 own' % c, None, (lambda v, c=c: est(v, 8, c))))
    for name, F, own in variants:
        out, cross = rcell(cell, F, own=own)
        fs = ' '.join('F%d %.2f' % (i, F[i]) for i in sorted(F)) if F else 'per row'
        cs = ('%.0f (%+.0f/%+.0f)' % (cross, cross - KNEE[cell], cross - dk)) if isinstance(cross, float) else cross
        print('   %-30s %-20s crossing %-18s | %s' % (name, fs, cs, ' '.join('%d:%.3f' % x for x in out)))
