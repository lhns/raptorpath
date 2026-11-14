"""Can b0 (the no-queue brake) be estimated from queue-invariant window statistics?

Usage: python -I b0est.py <loop-diag/p27/diag-ld> [<loop-diag/p34/run/diag-ld>]

b0 = E[Tmin_k - m] at no own queue: the 250 ms window minimum sits above the 10 s minimum because the long
minimum catches the jitter's extreme low tail. If the window median Tp50_k is constant at no queue, then
Tmin_k = Tp50 - S_k, so mean_k Tmin_k - min_k Tmin_k = max_k S_k - mean_k S_k. The candidate
  b0_hat = max_k S_k - mean_k S_k    (over the row's windows, t >= 1 s; S_k = Tp50_k - Tmin_k)
reads only the spread, so it is queue-invariant exactly where S is. Printed per cell, cap and leg, beside the
ledger b0 (median brake at the two smallest caps) and the row's measured brake.
"""
import glob, os, re, statistics as St, sys
from collections import defaultdict

LD = re.compile(r'\[LOOPDIAG\] t=([\d.]+)s p(\d) n=(\d+) .*?rtt_min_us=(\d+) .*?rtt_p50_us=(\d+) rtprop_us=(\d+)')
for D in sys.argv[1:]:
    est = defaultdict(lambda: defaultdict(list))
    for f in glob.glob(os.path.join(D, '*-DEF*-c.log')):
        c, cap = re.match(r'(c\w+-\d+)-DEF(\d+)-', os.path.basename(f)).groups()
        w = defaultdict(list)
        for L in open(f, errors='replace'):
            x = LD.search(L)
            if x and float(x.group(1)) >= 1.0 and int(x.group(3)) > 0:
                w[int(x.group(2))].append((int(x.group(4)) / 1e3, int(x.group(5)) / 1e3, int(x.group(6)) / 1e3))
        for p, xs in w.items():
            if len(xs) < 8:
                continue
            S = [b - a for a, b, _ in xs]
            est[c][(int(cap), p)].append((max(S) - St.mean(S), St.median([a - r for a, _, r in xs])))
    print('##', D)
    for c in sorted(est):
        print('==', c, ' cap leg: b0_hat [rows]  measured brake [rows]')
        for (cap, p) in sorted(est[c]):
            v = est[c][(cap, p)]
            print('  %5d p%d  b0_hat %s   brake %s' % (cap, p, ' '.join('%.2f' % a for a, _ in v),
                                                       ' '.join('%.2f' % b for _, b in v)))
