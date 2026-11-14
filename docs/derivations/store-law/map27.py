"""The derived store law (store-law.md section 3) on the static-cap ledgers p27 (c2, c8, c1s, c1d; 2026-10-27,
100 MB rows) and p34 (c1s, c1d after the status-32 receiver change; 3 rows per cap; truncated c1 windows).

Usage: python -I map27.py <loop-diag/p27/diag-ld or loop-diag/p34/run/diag-ld>

Per cell and static cap n (RWM_STORE=n, the law off), per leg i, medians over [LOOPDIAG] windows (250 ms) with
t >= 1 s of: Tmin = rtt_min_us, Tp50 = rtt_p50_us, m = rtprop_us (Copa's 10 s echo min; the rows are shorter than
10 s, so m is the fresh boot anchor), srtt, lam = dlv_rate (the leg's own delivered rate: the only rate the law
reads). S = Tp50 - Tmin (window spread; NOT queue-invariant at c8/c1, see the printed table), brake = Tmin - m.
Row goodput = driver mean_mbps. Rows below 0.6 x the cap's median goodput are dropped (the s43res outlier rule).
b0 per leg = median brake over the two smallest caps (a ledger constant: no engine source, see the doc).

Two forms of one sentence (G = 1 is the derived form; G < 1 is shown because the record's closed-loop pass used 0.9):
  'rate'  ask(n) = H + G * sum_i lam_i * ( (1 + q) * (m_i + b0_i) + S_i )                    (lam_i read as a rate)
  'ratio' ask(n) = H + G * (n - H) * sum_i lam_i*((1+q)*(m_i+b0_i) + S_i) / sum_i lam_i*Tp50_i  (lam_i only a weight)
  'ratioL' as 'ratio' with the allowance q*l_i on the lifetime echo minimum l_i (the fresh boot anchor m_i) in place
  of q*(m_i + b0_i):  sum_i lam_i*(m_i + b0_i + q*l_i + S_i) / sum_i lam_i*Tp50_i   (the derived form, doc sec. 3)
  'ratioX' as 'ratioL' with each leg's floor target raised to the slowest leg's lifetime floor (the in-order
  coupling, owner option O1): target_i = max(m_i + b0_i, max_j (l_j + b0_j)) + q*l_i
  The ratio form is the rate form with lam_i := (n - H) * w_i / Tp50-weighted (Little); it reads no absolute rate and
  is immune to the store-vs-echo accounting offset (the release lag; column 'Little' = n / sum lam*srtt).
  H0 = 0 ;  H1 = sum_i eps_i * lam_i * 2 * srtt_i   (a lost symbol holds its slot ~ two loops: detect + repair)
q = 0.05 / 0.0667 / 0.10 (Realtime / Auto / Bulk). 'expired' replaces m_i by the leg's lowest window Tmin over the
row (an expired 10 s filter under a standing queue). Crossings of ask(n) = n by linear interpolation between grid
caps: S = stable (ask/n falls through 1 as n rises), U = unstable (an upper trap above it).
"""
import glob, json, os, re, statistics as St, sys
from collections import defaultdict

D = sys.argv[1]
LD = re.compile(r'\[LOOPDIAG\] t=([\d.]+)s p(\d) n=(\d+) .*?rtt_min_us=(\d+) .*?rtt_p50_us=(\d+) rtprop_us=(\d+)'
                r' .*?srtt_us=(\d+) dlv=\d+ dlv_rate=(\d+)')
EPS = {'c2-100': {0: 0.026}, 'c8-100': {0: 0.027, 1: 0.0407}, 'c1s-400': {0: 0.001}, 'c1d-400': {0: 0.001, 1: 0.001}}
med = St.median


def row(f):
    w = defaultdict(list)
    for L in open(f, errors='replace'):
        x = LD.search(L)
        if not x:
            continue
        t, p, n, tmin, tp50, rtp, srtt, lam = x.groups()
        if float(t) < 1.0 or int(n) == 0:
            continue
        w[int(p)].append((int(tmin) / 1e3, int(tp50) / 1e3, int(rtp) / 1e3, int(srtt) / 1e3, int(lam)))
    g = None
    for L in open(f[:-6] + '-drv.out', errors='replace'):
        if L.startswith('{') and '"summary"' in L:
            g = json.loads(L)['mean_mbps']
    if not w or g is None:
        return None
    o = {'g': g, 'legs': {}}
    for p, xs in w.items():
        o['legs'][p] = dict(Tmin=med([a[0] for a in xs]), Tp50=med([a[1] for a in xs]), m=med([a[2] for a in xs]),
                            srtt=med([a[3] for a in xs]), lam=med([a[4] for a in xs]), floor=min(a[0] for a in xs))
    return o


cells = defaultdict(lambda: defaultdict(list))
for f in glob.glob(os.path.join(D, '*-DEF*-c.log')):
    c, cap = re.match(r'(c\w+-\d+)-DEF(\d+)-', os.path.basename(f)).groups()
    o = row(f)
    if o:
        cells[c][int(cap)].append(o)
for c in cells:
    for n, rows in cells[c].items():
        gm = med([r['g'] for r in rows])
        cells[c][n] = [r for r in rows if r['g'] >= 0.6 * gm]


def agg(rows):
    out = {'g': med([r['g'] for r in rows]), 'n_rows': len(rows), 'legs': {}}
    for p in sorted({p for r in rows for p in r['legs']}):
        rr = [r['legs'][p] for r in rows if p in r['legs']]
        out['legs'][p] = {k: med([x[k] for x in rr]) for k in rr[0]}
        out['legs'][p]['floor'] = min(x['floor'] for x in rr)
    return out


def ask(n, a, q, G, b0, anchor, hole, eps, form):
    H = sum(eps.get(p, 0.0) * L['lam'] * 2 * L['srtt'] / 1e3 for p, L in a['legs'].items()) if hole else 0.0
    s = 0.0
    for p, L in a['legs'].items():
        mm = L['m'] if anchor == 'fresh' else L['floor']
        if form == 'ratioX':
            F = max(LL['m'] + b0.get(pp, 0.0) for pp, LL in a['legs'].items())
            s += L['lam'] * (max(mm + b0.get(p, 0.0), F) + q * L['m'] + (L['Tp50'] - L['Tmin'])) / 1e3
        elif form == 'ratioL':
            s += L['lam'] * (mm + b0.get(p, 0.0) + q * L['m'] + (L['Tp50'] - L['Tmin'])) / 1e3
        else:
            s += L['lam'] * ((1 + q) * (mm + b0.get(p, 0.0)) + (L['Tp50'] - L['Tmin'])) / 1e3
    if form in ('ratio', 'ratioL', 'ratioX'):
        return H + G * max(n - H, 0.0) * s / (sum(L['lam'] * L['Tp50'] for L in a['legs'].values()) / 1e3)
    return H + G * s


for c in sorted(cells):
    caps = sorted(cells[c])
    A = {n: agg(cells[c][n]) for n in caps}
    gmax = max(A[n]['g'] for n in caps)
    legs = sorted({p for n in caps for p in A[n]['legs']})
    b0 = {p: med([A[n]['legs'][p]['Tmin'] - A[n]['legs'][p]['m'] for n in caps[:2] if p in A[n]['legs']]) for p in legs}
    print('=== %s  Gmax %.1f  b0 (ms): %s' % (c, gmax, ' '.join('p%d %.2f' % kv for kv in sorted(b0.items()))))
    print('n\trows\tgood\tg/Gmax\tLittle n/sum(lam*srtt)\t' +
          '\t'.join('p%d: lam/s m Tmin Tp50 S brake srtt' % p for p in legs))
    for n in caps:
        a = A[n]
        lt = sum(L['lam'] * L['srtt'] / 1e3 for L in a['legs'].values())
        print('%d\t%d\t%.1f\t%.3f\t%.2f\t' % (n, a['n_rows'], a['g'], a['g'] / gmax, n / lt) + '\t'.join(
            '%.0f %.1f %.1f %.1f %.1f %.1f %.1f' % (L['lam'], L['m'], L['Tmin'], L['Tp50'], L['Tp50'] - L['Tmin'],
                                                     L['Tmin'] - L['m'], L['srtt'])
            for p, L in sorted(a['legs'].items())))
    for form, G, hole, anchor, q in [(f, G, h, an, q) for f in ('ratioL', 'ratioX', 'ratio', 'rate') for G in (1.0, 0.9)
                                     for h in (False, True) for an in ('fresh', 'expired')
                                     for q in (0.05, 0.0667, 0.10)]:
                    r = [ask(n, A[n], q, G, b0, anchor, hole, EPS.get(c, {}), form) / n for n in caps]
                    xs = []
                    for i in range(len(caps) - 1):
                        (n1, r1), (n2, r2) = (caps[i], r[i]), (caps[i + 1], r[i + 1])
                        if (r1 - 1) * (r2 - 1) < 0:
                            xs.append('%s%.0f' % ('S' if r1 > 1 else 'U', n1 + (1 - r1) * (n2 - n1) / (r2 - r1)))
                    print('  %-5s G=%.1f H=%s %-7s q=%.4f  ask/n: %s  crossings: %s' % (
                        form, G, 'eps' if hole else '0  ', anchor, q, ' '.join('%d:%.2f' % (n, v) for n, v in zip(caps, r)),
                        ' '.join(xs) or 'none'))
