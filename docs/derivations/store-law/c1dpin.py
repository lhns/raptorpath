"""Evaluation side of the held-out c1d prediction (R4.2): the s53 c1d block at the shipped pin.

Usage: python -I c1dpin.py <extracted s53 diag-ld>

LATE window (sending ticks, t >= 5 s), per row and leg, medians of: qsp_us (sigma at load), the app-echo rtt, the
wire rtt, the quinn backlog dwell B_i / r_i (ms, r_i from tick-to-tick dgq tx), and a/W. Arm medians of rows.
"""
import glob, os, re, statistics as St, sys
from collections import defaultdict

LEG = r' p%d:infl=.*?rtt=(\d+)/wrtt=(\d+)/rtp\d+ms rtp_us=(\d+) .*?qsp_us=(\d+)/n'
arms = defaultdict(list)
for f in sorted(glob.glob(os.path.join(sys.argv[1], '**', 'c1d*-c.log'), recursive=True)):
    m = re.search(r'@def-(\w+?)-s\d+', os.path.basename(f))
    arm = m.group(1) if m else '?'
    prev = None
    acc = defaultdict(list)
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'):
            continue
        t = float(re.search(r't=([\d.]+)s', L).group(1))
        src = re.search(r' src=(\d+)', L)
        w = re.search(r'win=(\d+)/(\d+)', L)
        x = {'t': t}
        for i in (0, 1):
            d = re.search(r'dgq%d\[hand=(\d+) tx=(\d+)' % i, L)
            lg = re.search(LEG % i, L)
            if d and lg:
                x[i] = (int(d.group(2)), int(d.group(1)) - int(d.group(2)), int(lg.group(1)), int(lg.group(2)),
                        int(lg.group(4)) / 1e3)
        if prev and t >= 5 and src and int(src.group(1)) > 0 and w:
            dt = t - prev['t']
            acc['aW'].append(int(w.group(1)) / int(w.group(2)))
            for i in (0, 1):
                if i in x and i in prev and dt > 0:
                    r = (x[i][0] - prev[i][0]) / dt
                    if r > 0:
                        acc['dwell%d' % i].append(1e3 * x[i][1] / r)
                    acc['rtt%d' % i].append(x[i][2]); acc['wrtt%d' % i].append(x[i][3]); acc['sig%d' % i].append(x[i][4])
        prev = x
    if acc:
        arms[arm].append({k: St.median(v) for k, v in acc.items() if v})
for arm, rows in sorted(arms.items()):
    print('== c1d %s rows %d' % (arm, len(rows)))
    for k in sorted(rows[0]):
        v = [r[k] for r in rows if k in r]
        print('  %-7s median %.2f  range %.2f-%.2f' % (k, St.median(v), min(v), max(v)))
