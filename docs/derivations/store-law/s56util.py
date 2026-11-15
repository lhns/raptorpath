"""Is the receiver-bound 'knee' a utilisation-delay curve? Tests q*(1 - u) on the status-56 rows (R5.2).

Usage: python -I s56util.py <extracted .../run/rep56/diag-ld>

Capacity C (stated): the cell's control-arm median LATE send rate over unflagged rows (sum of dgq tx deltas per s):
  c1s P2048, c2 P2048, c8 P640. u = R / C.
q = T-bar - T0 with T-bar the rate-weighted app-echo loop. At N = 1 the top-level est rtt ({:.1} ms) is used, at c8 the
per-leg {:.0} ms rtt. T0 = the cell's no-queue floor from the status 56 score (c1s 4.0, c2 16.0; c8 legs 16.0 / 45.5
weighted by the tick's own leg rates).
Per row: (a) the row-level product q_row * (1 - u_row) from LATE means; (b) the mean over ticks of q_t * (1 - u_t), with
u_t clipped to [0, 1]. Both in ms and in symbols (x C). Beside them, the two batch sizes the hypothesis names:
  emission burst = eb_syms / eb_bursts (the mean burst; the cap is RWM_EMIT_BURST = 64), and
  release batch = symbols sent / ack wakes over the LATE window (d src_cum / d wake_ack), i.e. symbols per ack-driven wake.
For an M/D/1-like queue fed in batches of b, q*(1 - u)*C ~ (b - 1)/2 (Kingman's batch term); it is printed as b_eq = 2x + 1.
Rows in COLLAPSED are left out.
"""
import glob, os, re, statistics as St, sys
from collections import defaultdict

D = sys.argv[1]
COLLAPSED = {('c1s-400', 'P1024', 's7', 'r2'), ('c1s-400', 'P2048', 's42', 'r3'), ('c1s-400', 'P2048', 's42', 'r9'),
             ('c1s-400', 'P2048', 's7', 'r8'), ('c1s-400', 'P384', 's42', 'r1'), ('c1s-400', 'P512', 's42', 'r1'),
             ('c1s-400', 'P512', 's42', 'r5'), ('c1s-400', 'P512', 's42', 'r9'), ('c1s-400', 'P512', 's7', 'r2')}
CTRL = {'c1s-400': 'P2048', 'c2-100': 'P2048', 'c8-100': 'P640'}
T0 = {'c1s-400': {0: 4.0}, 'c2-100': {0: 16.0}, 'c8-100': {0: 16.0, 1: 45.5}}
rows = {}
for f in sorted(glob.glob(os.path.join(D, '*-c.log'))):
    m = re.match(r'(c\w+-\d+)@def-(P\d+)-(s\d+)-(r\d+)-', os.path.basename(f))
    if not m or m.groups() in COLLAPSED:
        continue
    key = m.groups()
    cell = key[0]
    t0 = 5.0 if cell.startswith('c1s') else 15.0
    tk = []
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'):
            continue
        t = float(re.search(r't=([\d.]+)s', L).group(1))
        src = re.search(r' src=(\d+)', L)
        x = {'t': t, 'src': int(src.group(1)) if src else 0, 'top': float(re.search(r' rtt=([\d.]+)ms', L).group(1))}
        cum = re.search(r' cum=(\d+)/', L)
        wk = re.search(r'wake\[.*? ack=(\d+)', L)
        eb = re.search(r'eb_bursts=(\d+) eb_syms=(\d+)', L)
        x['cum'] = int(cum.group(1)) if cum else None
        x['wack'] = int(wk.group(1)) if wk else None
        x['eb'] = (int(eb.group(1)), int(eb.group(2))) if eb else None
        for i in (0, 1):
            d = re.search(r'dgq%d\[hand=\d+ tx=(\d+)' % i, L)
            r = re.search(r' p%d:infl=.*?rtt=(\d+)/wrtt' % i, L)
            if d:
                x['tx%d' % i] = int(d.group(1))
            if r:
                x['rtt%d' % i] = int(r.group(1))
        tk.append(x)
    late = [x for x in tk if x['t'] >= t0 and x['src'] > 0]
    if len(late) < 5:
        continue
    legs = [i for i in (0, 1) if 'tx%d' % i in late[0]]
    per = []
    for a, b in zip(late, late[1:]):
        dt = b['t'] - a['t']
        if dt <= 0 or any('tx%d' % i not in a or 'tx%d' % i not in b for i in legs):
            continue
        r = {i: (b['tx%d' % i] - a['tx%d' % i]) / dt for i in legs}
        R = sum(r.values())
        if R <= 0:
            continue
        if len(legs) == 1:
            tb, t0b = b['top'], T0[cell][0]
        else:
            if any('rtt%d' % i not in b for i in legs):
                continue
            tb = sum(r[i] * b['rtt%d' % i] for i in legs) / R
            t0b = sum(r[i] * T0[cell][i] for i in legs) / R
        per.append((R, tb - t0b))
    a, b = late[0], late[-1]
    rel = ((b['cum'] - a['cum']) / (b['wack'] - a['wack'])) if a['cum'] and b['wack'] and b['wack'] > a['wack'] else float('nan')
    ebd = (b['eb'][1] - a['eb'][1]) / (b['eb'][0] - a['eb'][0]) if a['eb'] and b['eb'] and b['eb'][0] > a['eb'][0] else float('nan')
    rows[key] = dict(per=per, rel=rel, eb=ebd)
cap = {}
for c, arm in CTRL.items():
    v = [St.mean(R for R, _ in o['per']) for k, o in rows.items() if k[0] == c and k[1] == arm]
    cap[c] = St.median(v)
print('capacity C (control-arm median LATE send rate, /s):', ', '.join('%s %.0f' % kv for kv in sorted(cap.items())))
print('cell arm n | u | q ms | (a) q(1-u) ms, symbols | (b) mean_t q(1-u) ms, symbols | b_eq from (a) | emission burst | release batch')
arms = defaultdict(list)
for k, o in rows.items():
    C = cap[k[0]]
    u = St.mean(R for R, _ in o['per']) / C
    q = St.mean(qq for _, qq in o['per'])
    pa = q * max(0.0, 1 - u)
    pb = St.mean(qq * max(0.0, 1 - min(1.0, R / C)) for R, qq in o['per'])
    arms[k[:2]].append((u, q, pa, pb, o['eb'], o['rel'], C))
for k in sorted(arms, key=lambda k: (k[0], int(k[1][1:]))):
    v = arms[k]
    md = lambda j: St.median(x[j] for x in v if x[j] == x[j])
    C = v[0][6] / 1e3
    print('%s %s n%d | %.3f | %.2f | %.3f, %.1f | %.3f, %.1f | %.0f | %.1f | %.1f' % (
        k[0], k[1], len(v), md(0), md(1), md(2), md(2) * C, md(3), md(3) * C, 2 * md(2) * C + 1, md(4), md(5)))
