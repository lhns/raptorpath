"""Store-population budget per row.
Client: counted store (win=) median; path population per tick = rate0*rtt0 + rate1*rtt1 (app-echo,
per-path row-mean tx rates), and its p0 split into quinn send queue q0 (hand-tx), wire0 = rate0*wrtt0.
Server: [RANK] holes and holes older than the previous ~1 s readout (holes - tail_overcount), medians
over the streaming window (drop first 2 / last 6 readouts).
Also per row: (rtt0 - wrtt0) against q0/rate0 (does the app-echo excess equal the quinn queue dwell?)."""
import re, sys, glob, os, statistics as S
d = sys.argv[1]; cell = 'c8-100'
PP = r' p%d:infl=\d+/sinfl=\d+/bdp\d+\S* sout=\d+ khr=\S+ btlbw=\d+.*?rtt=(\d+)/wrtt=(\d+)/rtp\d+ms rtp_us=(\d+)'
out = {}
print('row\twin\tq0\twire0\tpath0\tpath1\tpathpop\t4096-pathpop\thole_med\told_hole_med\tex0_ms(rtt0-wrtt0)\tq0dwell_ms\trtp0_ms\twrtt0_ms')
for f in sorted(glob.glob(os.path.join(d, cell + '@def-*-c.log'))):
    name = os.path.basename(f)[len(cell) + 5:-6]
    ticks = []; cum = []
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'): continue
        t = float(re.search(r't=([\d.]+)s', L).group(1)); src = re.search(r' src=(\d+)', L)
        d0 = re.search(r'dgq0\[hand=(\d+) tx=(\d+)', L); d1 = re.search(r'dgq1\[hand=(\d+) tx=(\d+)', L)
        p0 = re.search(PP % 0, L); p1 = re.search(PP % 1, L)
        if not (d0 and d1 and p0 and p1 and src) or t < 2 or int(src.group(1)) == 0: continue
        w = int(re.search(r'win=(\d+)/', L).group(1))
        ticks.append((w, int(d0.group(1)) - int(d0.group(2)), int(p0.group(1)), int(p0.group(2)), int(p1.group(1)), int(p0.group(3))))
        cum.append((t, int(d0.group(2)), int(d1.group(2))))
    (ta, a0, a1), (tb, b0, b1) = cum[0], cum[-1]
    r0 = (b0 - a0) / (tb - ta); r1 = (b1 - a1) / (tb - ta)
    sv = f[:-6] + '-s.log'
    rk = [L for L in open(sv, errors='replace') if L.startswith('[RANK]')][2:-6]
    hol = [int(re.search(r'holes=(\d+)', L).group(1)) for L in rk]
    old = [int(re.search(r'holes=(\d+)', L).group(1)) - int(re.search(r'tail_overcount=(\d+)', L).group(1)) for L in rk]
    m = lambda xs: S.median(xs)
    v = [m([x[0] for x in ticks]), m([x[1] for x in ticks]), m([r0 * x[3] / 1000 for x in ticks]),
         m([r0 * x[2] / 1000 for x in ticks]), m([r1 * x[4] / 1000 for x in ticks]),
         m([r0 * x[2] / 1000 + r1 * x[4] / 1000 for x in ticks])]
    v.append(4096 - v[5]); v += [m(hol), m(old), m([x[2] - x[3] for x in ticks]), m([1000 * x[1] / r0 for x in ticks]), m([x[5] / 1000 for x in ticks]), m([x[3] for x in ticks])]
    out.setdefault(name.split('-')[0], []).append(v)
    print(name, *['%.0f' % x for x in v], sep='\t')
print('\narm: median of rows [min..max]')
for a, M in out.items():
    print(a, *['%.0f [%.0f..%.0f]' % (S.median(c), min(c), max(c)) for c in zip(*M)], sep='\t')
