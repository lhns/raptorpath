"""R6.5: validate the §56 Amendment 2 quinn-collapse flag on c1d (§53's 30 c1d rows, 5 GB).

Usage: python -I c1dflag.py <extracted s53 diag-ld>

Flag (as §56 Amendment 2 at c1s): the share of [DIAG] windows (t >= 1 s) whose leg qcwnd is below the leg's BDP
(250 000 B, the c1-class link: 1 Gbit/s x 2 ms wire RTT), per leg; flagged iff the share > 0.10 on any leg.
Printed per row: both legs' shares, goodput (driver mean_mbps), and the per-leg delivered-rate share
(rate-relative variant: qcwnd below 2 x the leg's own delivered bytes per 2 ms, i.e. BBR's healthy 2-BDP window at the
rate the leg actually carries, so a leg running at half rate is not flagged for needing half the window).
"""
import glob, json, os, re, statistics as St, sys

for f in sorted(glob.glob(os.path.join(sys.argv[1], '**', 'c1d*-c.log'), recursive=True)):
    n = 0; a = [0, 0]; rel = [0, 0]; prev = None
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'):
            continue
        t = float(re.search(r't=([\d.]+)s', L).group(1))
        x = {}
        for i in (0, 1):
            q = re.search(r' p%d:infl=.*?qcwnd=(\d+)' % i, L)
            d = re.search(r'dgq%d\[hand=\d+ tx=(\d+)' % i, L)
            if q and d:
                x[i] = (int(q.group(1)), int(d.group(1)))
        if prev and t >= 1 and len(x) == 2 and len(prev[1]) == 2:
            dt = t - prev[0]
            n += 1
            for i in (0, 1):
                if x[i][0] < 250000:
                    a[i] += 1
                r = (x[i][1] - prev[1][i][1]) / dt if dt > 0 else 0
                if x[i][0] < 2 * r * 1233 * 0.002:
                    rel[i] += 1
        prev = (t, x)
    g = None
    for L in open(f[:-6] + '-drv.out', errors='replace'):
        if L.startswith('{') and '"summary"' in L:
            g = json.loads(L)['mean_mbps']
    if n:
        print('%-40s goodput %6.1f  abs share %.3f %.3f  rel share %.3f %.3f  %s' % (
            os.path.basename(f)[len('c1d-400@def-'):-6], g or 0, a[0] / n, a[1] / n, rel[0] / n, rel[1] / n,
            'FLAG' if max(a) / n > 0.10 else ''))
