"""R7: c1s per-row readouts on §56 for sizing the burst arm and reading the release batch.

Usage: python -I c1srows.py <extracted .../run/rep56/diag-ld>

Per unflagged c1s row, LATE window (sending ticks, t >= 5 s):
  R          lap rate, d(dgq0 tx)/dt
  q          mean of the 0.1 ms top-level est rtt minus §56's T0 = 4.0 ms (per-tick means, not integer medians)
  Q = q*R    standing queue in symbols
  occ        mean counted store a (win=a/b), and a/W
  release batch = d(cum src) / d(ack wakes): symbols sent per ack-driven sender wake
  wakes/s and wakes per loop (wakes/s x mean T-bar): how many release batches the store receives per echo loop
  busy %, ack-wait %: the sender loop's own busy share and share of its waits spent waiting on acks ([DIAG] wait[])
"""
import glob, os, re, statistics as St, sys

D = sys.argv[1]
COL = {('P1024', 's7', 'r2'), ('P2048', 's42', 'r3'), ('P2048', 's42', 'r9'), ('P2048', 's7', 'r8'),
       ('P384', 's42', 'r1'), ('P512', 's42', 'r1'), ('P512', 's42', 'r5'), ('P512', 's42', 'r9'), ('P512', 's7', 'r2')}
print('arm seed rep | R /s | q ms | Q sym | occ (a/W) | release batch | wakes/s | wakes/loop | busy % | ack-wait %')
for f in sorted(glob.glob(os.path.join(D, 'c1s-400@def-*-c.log')), key=lambda p: int(re.search(r'-P(\d+)-', p).group(1))):
    m = re.match(r'c1s-400@def-(P\d+)-(s\d+)-(r\d+)-', os.path.basename(f))
    if not m or m.groups() in COL:
        continue
    late = []
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'):
            continue
        t = float(re.search(r't=([\d.]+)s', L).group(1))
        src = re.search(r' src=(\d+)', L)
        if t < 5 or not src or int(src.group(1)) == 0:
            continue
        w = re.search(r'win=(\d+)/(\d+)', L)
        wt = re.search(r'wait\[.*? ack=(\d+)%.*?busy=(\d+)%', L)
        late.append(dict(t=t, tx=int(re.search(r'dgq0\[hand=\d+ tx=(\d+)', L).group(1)),
                         rtt=float(re.search(r' rtt=([\d.]+)ms', L).group(1)),
                         cum=int(re.search(r' cum=(\d+)/', L).group(1)),
                         wack=int(re.search(r'wake\[.*? ack=(\d+)', L).group(1)),
                         a=int(w.group(1)), W=int(w.group(2)),
                         ackw=int(wt.group(1)) if wt else 0, busy=int(wt.group(2)) if wt else 0))
    if len(late) < 5:
        continue
    a, b = late[0], late[-1]
    dt = b['t'] - a['t']
    R = (b['tx'] - a['tx']) / dt
    Tb = St.mean(x['rtt'] for x in late)
    q = Tb - 4.0
    wr = (b['wack'] - a['wack']) / dt
    rb = (b['cum'] - a['cum']) / max(1, b['wack'] - a['wack'])
    occ = St.mean(x['a'] for x in late)
    print('%s %s %s | %.0f | %.2f | %.0f | %.0f (%.3f) | %.0f | %.0f | %.1f | %.0f | %.0f' % (
        *m.groups(), R, q, q * R / 1e3, occ, occ / b['W'], rb, wr, wr * Tb / 1e3,
        St.mean(x['busy'] for x in late), St.mean(x['ackw'] for x in late)))
