"""Store-population budget, split by phase.
PIN phase: client ticks with cap (win= denominator) == 4096 and src > 0; PRE phase: 2 s <= t < 10 s with
cap < 4096. Per tick: pathpop = r0*rtt0 + r1*rtt1 (row-mean tx rates of the phase, app-echo rtt),
resid = win - pathpop. Server [RANK]: holes and old holes (holes - tail_overcount); readouts are ~1 s
apart from server start, so PRE = readouts 2..9 and PIN = readouts 12..(last-6) (approximate alignment).
"""
import re, sys, glob, os, statistics as S
d = sys.argv[1]; cell = 'c8-100'
PP = r' p%d:infl=\d+/sinfl=\d+/bdp\d+\S* sout=\d+ khr=\S+ btlbw=\d+.*?rtt=(\d+)/wrtt=(\d+)/rtp\d+ms rtp_us=(\d+).*?qcwnd=(\d+)'
def phase_stats(ticks):
    (ta, a0, a1), (tb, b0, b1) = [(x['t'], x['tx0'], x['tx1']) for x in (ticks[0], ticks[-1])]
    r0 = (b0 - a0) / (tb - ta); r1 = (b1 - a1) / (tb - ta)
    m = S.median
    return dict(n=len(ticks), cap=m([x['cap'] for x in ticks]), win=m([x['win'] for x in ticks]),
                q0=m([x['q0'] for x in ticks]), wire0=m([r0 * x['wrtt0'] / 1000 for x in ticks]),
                qcw0=m([x['qcw0'] / 1233 for x in ticks]),
                path0=m([r0 * x['rtt0'] / 1000 for x in ticks]), path1=m([r1 * x['rtt1'] / 1000 for x in ticks]),
                resid=m([x['win'] - r0 * x['rtt0'] / 1000 - r1 * x['rtt1'] / 1000 for x in ticks]),
                rtp0=m([x['rtp0'] / 1000 for x in ticks]), r0=r0, r1=r1)
out = {}
cols = ['n', 'cap', 'win', 'q0', 'wire0', 'qcw0', 'path0', 'path1', 'resid', 'hol', 'old', 'rtp0', 'r0', 'r1']
print('row phase', *cols, sep='\t')
for f in sorted(glob.glob(os.path.join(d, cell + '@def-*-c.log'))):
    name = os.path.basename(f)[len(cell) + 5:-6]
    pin, pre = [], []
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'): continue
        t = float(re.search(r't=([\d.]+)s', L).group(1)); src = re.search(r' src=(\d+)', L)
        d0 = re.search(r'dgq0\[hand=(\d+) tx=(\d+)', L); d1 = re.search(r'dgq1\[hand=(\d+) tx=(\d+)', L)
        p0 = re.search(PP % 0, L); p1 = re.search(PP % 1, L)
        if not (d0 and d1 and p0 and p1 and src) or t < 2 or int(src.group(1)) == 0: continue
        w = re.search(r'win=(\d+)/(\d+)', L)
        x = dict(t=t, win=int(w.group(1)), cap=int(w.group(2)), q0=int(d0.group(1)) - int(d0.group(2)),
                 tx0=int(d0.group(2)), tx1=int(d1.group(2)), rtt0=int(p0.group(1)), wrtt0=int(p0.group(2)),
                 rtp0=int(p0.group(3)), qcw0=int(p0.group(4)), rtt1=int(p1.group(1)))
        if x['cap'] >= 4096: pin.append(x)
        elif t < 10: pre.append(x)
    rk = [L for L in open(f[:-6] + '-s.log', errors='replace') if L.startswith('[RANK]')]
    def hs(lines):
        h = [int(re.search(r'holes=(\d+)', L).group(1)) for L in lines]
        o = [a - int(re.search(r'tail_overcount=(\d+)', L).group(1)) for a, L in zip(h, lines)]
        return (S.median(h), S.median(o)) if h else (float('nan'), float('nan'))
    for ph, ticks, lines in (('PIN', pin, rk[12:-6]), ('PRE', pre, rk[2:10])):
        if len(ticks) < 4: continue
        s = phase_stats(ticks); s['hol'], s['old'] = hs(lines)
        out.setdefault((name.split('-')[0], ph), []).append([s[c] for c in cols])
        print(name, ph, *['%.0f' % s[c] for c in cols], sep='\t')
print('\narm phase: median of rows [min..max]')
for k in sorted(out):
    print(*k, *['%.0f [%.0f..%.0f]' % (S.median(c), min(c), max(c)) for c in zip(*out[k])], sep='\t')
