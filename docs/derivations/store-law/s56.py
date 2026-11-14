"""Rev-3 readouts on the status-56 store-pool sweep (docs/l1-raw/s56/rep56-diag-ld.tgz, extracted).

Usage: python -I s56.py <extracted .../run/rep56/diag-ld>

Per row, LATE window (sending ticks, t >= 15 s; c1s t >= 5 s), tick by tick (consecutive [DIAG] lines):
  r_i      = d(dgq_i tx)/dt over the tick (datagrams/s)
  rtt_i    = per-leg app-echo EWMA, printed rounded to 1 ms ({:.0}); at N = 1 also the top-level est rtt ({:.1} ms)
  resid    = a - sum_i r_i * rtt_i  (a = counted store, win=a/b), then MEANS over ticks, not medians
  resid_ms = resid / sum_i r_i : the store time outside the echo loop, in ms
Attribution (W1.1 classes, status 48b plan; per tick): QUINN-LIMITED if quinn's backlog B = dgq hand - tx >= 98
datagrams; STORE-LIMITED if paused >= 50 % and B < 98; NEITHER otherwise. Because `paused` reads < 50 % at c1s even with the store at its cap (the sender wakes on
every ack batch), a second split replaces the pause test by the store itself: FULL if a >= 0.95 W and B < 98,
OPEN if a < 0.95 W and B < 98 (the store is not the limiter that tick).
Knee-scaling candidates per row (LATE medians): qsp (P90 - P50 of the leg's last 256 echo samples), rvar, the wire
RTT, quinn's window in datagrams, B/r0 (quinn backlog time), the c8 leg-floor gap.
Rows listed in COLLAPSED (status 56 score: quinn-collapse flag) are marked and left out of every arm median.
"""
import glob, os, re, statistics as St, sys
from collections import defaultdict

D = sys.argv[1]
COLLAPSED = {('c1s-400', 'P1024', 's7', 'r2'), ('c1s-400', 'P2048', 's42', 'r3'), ('c1s-400', 'P2048', 's42', 'r9'),
             ('c1s-400', 'P2048', 's7', 'r8'), ('c1s-400', 'P384', 's42', 'r1'), ('c1s-400', 'P512', 's42', 'r1'),
             ('c1s-400', 'P512', 's42', 'r5'), ('c1s-400', 'P512', 's42', 'r9'), ('c1s-400', 'P512', 's7', 'r2')}
PP = (r' p%d:infl=.*?rtt=(\d+)/wrtt=(\d+)/rtp\d+ms rtp_us=(\d+) sig_us=(\d+)/n\d+ rvar_us=(\d+)/n\d+ '
      r'qsp_us=(\d+)/n\d+ .*?qcwnd=(\d+)')
med = St.median
arms = defaultdict(list)
for f in sorted(glob.glob(os.path.join(D, '*-c.log'))):
    m = re.match(r'(c\w+-\d+)@def-(P\d+)-(s\d+)-(r\d+)-', os.path.basename(f))
    if not m:
        continue
    key = m.groups()
    t0 = 5.0 if key[0].startswith('c1s') else 15.0
    ticks = []
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'):
            continue
        t = float(re.search(r't=([\d.]+)s', L).group(1))
        src = re.search(r' src=(\d+)', L)
        w = re.search(r'win=(\d+)/(\d+)', L)
        pa = re.search(r'paused=(\d+)%', L)
        top = re.search(r' rtt=([\d.]+)ms', L)
        dq = [re.search(r'dgq%d\[hand=(\d+) tx=(\d+)' % i, L) for i in (0, 1)]
        pp = [re.search(PP % i, L) for i in (0, 1)]
        if not (src and w and pa and top and dq[0] and pp[0]) or t < t0 or int(src.group(1)) == 0:
            continue
        legs = [i for i in (0, 1) if dq[i] and pp[i]]
        x = dict(t=t, a=int(w.group(1)), W=int(w.group(2)), paused=int(pa.group(1)), top=float(top.group(1)))
        for i in legs:
            x['tx%d' % i] = int(dq[i].group(2))
            x['B%d' % i] = int(dq[i].group(1)) - int(dq[i].group(2))
            rtt, wrtt, rtp, sig, rvar, qsp, qcw = (int(v) for v in pp[i].groups())
            x.update({'rtt%d' % i: rtt, 'wrtt%d' % i: wrtt, 'rtp%d' % i: rtp / 1e3, 'qsp%d' % i: qsp / 1e3,
                      'rvar%d' % i: rvar / 1e3, 'qcw%d' % i: qcw / 1233})
        x['legs'] = legs
        ticks.append(x)
    if len(ticks) < 5:
        continue
    legs = ticks[0]['legs']
    res, resT, resms, cls, cls2 = [], [], [], defaultdict(int), defaultdict(int)
    for a, b in zip(ticks, ticks[1:]):
        dt = b['t'] - a['t']
        if dt <= 0 or any('tx%d' % i not in b or 'tx%d' % i not in a for i in legs):
            continue
        r = {i: (b['tx%d' % i] - a['tx%d' % i]) / dt for i in legs}
        R = sum(r.values())
        if R <= 0:
            continue
        pop = sum(r[i] * b['rtt%d' % i] / 1e3 for i in legs)
        res.append(b['a'] - pop)
        resms.append(1e3 * (b['a'] - pop) / R)
        if len(legs) == 1:
            resT.append(1e3 * (b['a'] - R * b['top'] / 1e3) / R)
        B = b['B0'] + (b.get('B1', 0) if 1 in legs else 0)
        cls['QUINN' if B >= 98 else ('STORE' if b['paused'] >= 50 else 'NEITHER')] += 1
        full = b['a'] >= 0.95 * b['W']
        cls2['QUINN' if B >= 98 else ('FULL' if full else 'OPEN')] += 1
    n = sum(cls.values()) or 1
    o = dict(W=med([x['W'] for x in ticks]), resid=St.mean(res), resid_W=St.mean(res) / med([x['W'] for x in ticks]),
             resid_ms=St.mean(resms), resid_ms_top=St.mean(resT) if resT else float('nan'),
             quinn=cls['QUINN'] / n, store=cls['STORE'] / n, neither=cls['NEITHER'] / n,
             q2=cls2['QUINN'] / n, full=cls2['FULL'] / n, open=cls2['OPEN'] / n,
             BW=med([(x['B0'] + x.get('B1', 0)) / x['W'] for x in ticks]),
             collapsed=key in COLLAPSED)
    for i in legs:
        for k in ('rtt', 'wrtt', 'rtp', 'qsp', 'rvar', 'qcw'):
            o['%s%d' % (k, i)] = med([x['%s%d' % (k, i)] for x in ticks if '%s%d' % (k, i) in x])
    arms[key[:2]].append(o)

COLS = ['W', 'resid_W', 'resid_ms', 'resid_ms_top', 'quinn', 'store', 'neither', 'full', 'open', 'BW', 'rtt0', 'wrtt0', 'rtp0', 'qsp0',
        'rvar0', 'qcw0', 'rtt1', 'wrtt1', 'rtp1', 'qsp1']
print('arm medians over unflagged rows (n); flagged rows shown separately for c1s')
print('cell arm n\t' + '\t'.join(COLS))
for (c, a) in sorted(arms, key=lambda k: (k[0], int(k[1][1:]))):
    for flag in (False, True):
        rows = [o for o in arms[(c, a)] if o['collapsed'] == flag]
        if not rows:
            continue
        vals = []
        for k in COLS:
            xs = [o[k] for o in rows if k in o and o[k] == o[k]]
            vals.append('%.3f' % med(xs) if xs and k in ('resid_W', 'quinn', 'store', 'neither', 'full', 'open', 'BW') else
                        ('%.2f' % med(xs) if xs else '-'))
        print('%s %s%s n%d\t' % (c, a, ' FLAGGED' if flag else '', len(rows)) + '\t'.join(vals))
