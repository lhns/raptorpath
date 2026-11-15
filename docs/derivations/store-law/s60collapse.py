"""s60 ruling entry 8, item 3: an ABSOLUTE collapse rule for calibration rows, on a field the gauge prints, checked
against s57a's collapse-flagged c1s rows.

Usage: python -I s60collapse.py <extracted .../rep57a/diag-ld> <docs/l1-raw/s57/rep57a-score.txt>

Per row (client [DIAG] ticks, LATE = t >= 5 s with src > 0): l = min rtp_us over the row; per tick the echo excess
x = top-level rtt (0.1 ms) - l; the share of LATE ticks with x > X for X = 2 / 5 / 10 ms; the LATE median qcwnd (p0,
bytes). The gauge prints, per 250 ms window, `tmin` and `l` ([SLAW]) and `qcwnd` ([WSTAT]); the registered rule uses
`tmin - l`, which is <= the tick mean excess used here, so on s57a this check is the rule's UPPER bound on flags (a
tick-mean rule flags at least every row the window-minimum rule flags).
The s57a score's per-row table supplies the registered collapse tags (COLLAPSE / DNF).
"""
import glob, os, re, statistics as St, sys

D, SCORE = sys.argv[1], sys.argv[2]
tag = {}
for L in open(SCORE, errors='replace'):
    m = re.match(r'\s+(B\d+-\d+)\s+s(\d+)\s+r(\d+) \|.*\|([^|]*)$', L.rstrip('\n'))
    if m:
        tag[(m.group(1).replace('-', '_'), m.group(2), m.group(3))] = ('COLLAPSE' in m.group(4) or 'DNF' in m.group(4))
rows = []
for f in sorted(glob.glob(os.path.join(D, '*-c.log'))):
    m = re.match(r'c1s-400@def-(B\d+_\d+)-s(\d+)-r(\d+)-', os.path.basename(f))
    if not m:
        continue
    T = []
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'):
            continue
        t = float(re.search(r't=([\d.]+)s', L).group(1))
        src = re.search(r' src=(\d+)', L)
        top = re.search(r' rtt=([\d.]+)ms', L)
        rp = re.search(r' rtp_us=(\d+)', L)
        qc = re.search(r' qcwnd=(\d+)', L)
        if top and rp:
            T.append((t, int(src.group(1)) if src else 0, float(top.group(1)), int(rp.group(1)) / 1e3,
                      int(qc.group(1)) if qc else None))
    if not T:
        continue
    l = min(x[3] for x in T if x[3] > 0)
    late = [x for x in T if x[0] >= 5 and x[1] > 0]
    if not late:
        continue
    xs = [x[2] - l for x in late]
    sh = {X: sum(v > X for v in xs) / len(xs) for X in (2, 5, 6.1, 10)}
    qcw = St.median([x[4] for x in late if x[4] is not None] or [0])
    rows.append((m.groups(), tag.get(m.groups(), None), St.median(xs), sh, qcw))

print('arm seed rep | registered collapse | median excess ms | share > 2 / 5 / 6.1 / 10 ms | median qcwnd B')
for k, tg, mx, sh, q in rows:
    print('%s s%s r%s | %s | %.2f | %.2f / %.2f / %.2f / %.2f | %d' % (k[0], k[1], k[2], 'COLLAPSE' if tg else '-', mx,
                                                               sh[2], sh[5], sh[6.1], sh[10], q))
print()
for X in (2, 5, 6.1, 10):
    for S in (0.10, 0.25):
        tp = sum(1 for r in rows if r[1] and r[3][X] > S)
        fp = sum(1 for r in rows if r[1] is False and r[3][X] > S)
        print('rule: share of LATE ticks with excess > %g ms above %.2f -> flags %d of %d registered-collapse rows, '
              '%d of %d unflagged rows' % (X, S, tp, sum(1 for r in rows if r[1]), fp,
                                           sum(1 for r in rows if r[1] is False)))
