"""Little's-law delivered delay per row (PIN phase: cap == 4096, src > 0).
retained = win + gate (store_len = retained - released_for_gate, and `gate=` prints released_for_gate),
i.e. the sent store above the receiver's cumulative point, divided by the summed tx rate; plus the
[LAT] coverage (p0:n + p1:n over n)."""
import re, sys, glob, os, statistics as S
d = sys.argv[1]; out = {}
print('row\tretained\trate\tlittle_s\tlat_cov')
for f in sorted(glob.glob(os.path.join(d, 'c8-100@def-*-c.log'))):
    name = os.path.basename(f)[11:-6]; ret = []; cum = []
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'): continue
        w = re.search(r'win=(\d+)/(\d+)', L); sr = re.search(r' gate=(\d+)', L); src = re.search(r' src=(\d+)', L)
        d0 = re.search(r'dgq0\[hand=\d+ tx=(\d+)', L); d1 = re.search(r'dgq1\[hand=\d+ tx=(\d+)', L)
        t = float(re.search(r't=([\d.]+)s', L).group(1))
        if not (w and sr and d0 and d1 and src) or int(src.group(1)) == 0 or int(w.group(2)) < 4096: continue
        ret.append(int(w.group(1)) + int(sr.group(1))); cum.append((t, int(d0.group(1)) + int(d1.group(1))))
    rate = (cum[-1][1] - cum[0][1]) / (cum[-1][0] - cum[0][0])
    la = [L for L in open(f[:-6] + '-s.log', errors='replace') if L.startswith('[LAT]')][-1]
    n = int(re.search(r' n=(\d+)', la).group(1)); pn = sum(int(x) for x in re.findall(r' p\d:n=(\d+)', la))
    v = [S.median(ret), rate, S.median(ret) / rate, pn / n]
    out.setdefault(name.split('-')[0], []).append(v)
    print(name, '%.0f' % v[0], '%.0f' % v[1], '%.3f' % v[2], '%.2f' % v[3], sep='\t')
for a, M in out.items():
    print(a, *['%.3g [%.3g..%.3g]' % (S.median(c), min(c), max(c)) for c in zip(*M)], sep='\t')
