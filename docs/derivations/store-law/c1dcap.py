"""R6.5 c1d bind check: the late-window cap and live path count on §53's c1d 5 GB rows (shipped law, pool 2048).

Usage: python -I c1dcap.py <extracted s53 diag-ld>

Per row, LATE ticks (sending, t >= 5 s): the cap b (win=a/b) median, p10 and p90, the mean a/W, and the live path
count np (np= field) median. A static pool W binds iff the shipped law's ask exceeds W; here the shipped cap's own
late distribution is read as the law's ask below the ceiling. Pools W with W > the cap's p10 would not bind on
some ticks.
"""
import glob, os, re, statistics as St, sys

caps, rows = [], []
for f in sorted(glob.glob(os.path.join(sys.argv[1], '**', 'c1d*-c.log'), recursive=True)):
    b, aw, npl = [], [], []
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'):
            continue
        t = float(re.search(r't=([\d.]+)s', L).group(1))
        src = re.search(r' src=(\d+)', L)
        w = re.search(r'win=(\d+)/(\d+)', L)
        n = re.search(r' np=(\d+)', L)
        if t >= 5 and src and int(src.group(1)) > 0 and w:
            b.append(int(w.group(2))); aw.append(int(w.group(1)) / int(w.group(2)))
            if n:
                npl.append(int(n.group(1)))
    if b:
        s = sorted(b)
        rows.append((os.path.basename(f)[len('c1d-400@def-'):-6], St.median(b), s[len(s) // 10], s[9 * len(s) // 10],
                     St.mean(aw), St.median(npl) if npl else -1))
for r in rows:
    print('%-22s cap median %5.0f  p10 %5.0f  p90 %5.0f  a/W %.2f  np %d' % r)
print('all rows: cap median of medians %.0f, min p10 %.0f, median p10 %.0f'
      % (St.median(r[1] for r in rows), min(r[2] for r in rows), St.median(r[2] for r in rows)))
