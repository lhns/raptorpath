"""Boot inputs ONLY for the held-out c1d prediction (store-law R4.1): the first 8 [DIAG] ticks (2 s) of each c1d row.

Usage: python -I c1dboot.py <extracted s53 diag-ld>

Reads nothing after a row's 8th [DIAG] tick: per leg the lowest rtp_us (l) and the highest qsp_us (sigma+), and the
boot send rate. This is the input side of the prediction; the map side is read only after it is committed.
"""
import glob, os, re, statistics as St, sys

LEG = r' p%d:infl=.*?rtp_us=(\d+) .*?qsp_us=(\d+)/n'
rows = []
for f in sorted(glob.glob(os.path.join(sys.argv[1], '**', 'c1d*-c.log'), recursive=True)):
    ticks = []
    for L in open(f, errors='replace'):
        if L.startswith('[DIAG]'):
            ticks.append(L)
            if len(ticks) == 8:
                break
    o = {}
    for i in (0, 1):
        v = [re.search(LEG % i, L) for L in ticks]
        v = [(int(x.group(1)) / 1e3, int(x.group(2)) / 1e3) for x in v if x]
        if v:
            o['l%d' % i] = min(a for a, _ in v if a > 0)
            o['s%d' % i] = max(b for _, b in v)
    if o:
        rows.append(o)
print('c1d rows', len(rows))
for k in ('l0', 'l1', 's0', 's1'):
    v = [r[k] for r in rows if k in r]
    print('%s median %.2f range %.2f-%.2f ms' % (k, St.median(v), min(v), max(v)))
