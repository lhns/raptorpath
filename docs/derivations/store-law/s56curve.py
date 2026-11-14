"""R6.1: the utilisation-delay product with ONE rate definition and ONE capacity, control arm included.

Usage: python -I s56curve.py <extracted .../run/rep56/diag-ld>

One rate definition for every row: R = (dgq tx at the last LATE tick - at the first) / elapsed (the status-56 score's
"sum tx rate R"). LATE = sending ticks, t >= 5 s at c1s. q = mean over LATE ticks of (top-level est rtt, 0.1 ms) - T0,
T0 = 4.0 ms (§56 score). Unflagged rows only (the §56 quinn-collapse list).
The product q * (C - R) (symbols) is printed for three capacities, each held fixed across every arm:
  C_ctl  = the control arm's median R (the score's 92 992 /s): the control is then at u = 1 by construction;
  C_rev5 = 96 836 /s (rev 5's value: a median of per-tick-mean rates, a second definition);
  C_wire = the 1 Gbit/s wire rate in datagrams (1e9 / 8 / 1 233 = 101 379 /s), an upper bound no row reaches.
The curve hypothesis q = K / (C (1 - u)) requires the product to be constant across W, control included.
"""
import glob, os, re, statistics as St, sys
from collections import defaultdict

D = sys.argv[1]
COLLAPSED = {('P1024', 's7', 'r2'), ('P2048', 's42', 'r3'), ('P2048', 's42', 'r9'), ('P2048', 's7', 'r8'),
             ('P384', 's42', 'r1'), ('P512', 's42', 'r1'), ('P512', 's42', 'r5'), ('P512', 's42', 'r9'),
             ('P512', 's7', 'r2')}
arms = defaultdict(list)
for f in sorted(glob.glob(os.path.join(D, 'c1s-400@def-*-c.log'))):
    m = re.match(r'c1s-400@def-(P\d+)-(s\d+)-(r\d+)-', os.path.basename(f))
    if not m or m.groups() in COLLAPSED:
        continue
    late = []
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'):
            continue
        t = float(re.search(r't=([\d.]+)s', L).group(1))
        src = re.search(r' src=(\d+)', L)
        tx = re.search(r'dgq0\[hand=\d+ tx=(\d+)', L)
        rtt = re.search(r' rtt=([\d.]+)ms', L)
        if t >= 5 and src and int(src.group(1)) > 0 and tx and rtt:
            late.append((t, int(tx.group(1)), float(rtt.group(1))))
    if len(late) < 5:
        continue
    R = (late[-1][1] - late[0][1]) / (late[-1][0] - late[0][0])
    q = St.mean(r for _, _, r in late) - 4.0
    arms[m.group(1)].append((R, q))
med = St.median
Rctl = med(R for R, _ in arms['P2048'])
caps = {'C_ctl': Rctl, 'C_rev5': 96836.0, 'C_wire': 1e9 / 8 / 1233}
print('C_ctl (control median R) = %.0f /s' % Rctl)
print('arm n | R /s | q ms | q*(C-R) symbols for C_ctl / C_rev5 / C_wire')
for a in sorted(arms, key=lambda k: int(k[1:])):
    v = arms[a]
    R, q = med(x[0] for x in v), med(x[1] for x in v)
    print('%s n%d | %.0f | %.2f | ' % (a, len(v), R, q) + ' / '.join('%.1f' % (q * (C - R) / 1e3) for C in caps.values()))
