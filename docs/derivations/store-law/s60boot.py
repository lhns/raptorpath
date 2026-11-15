"""s60 registration input: what b0p (the gauge's b0-hat-plus) and m-hat will read on static rows, from the s56 ledger.

Usage: python -I s60boot.py <extracted .../run/rep56/diag-ld>

The gauge's b0p is the max over the first K = 8 windows (2 s) of the brake T_min,k - m_k (slaw.rs WinSampler).
[DIAG] has no window minimum, so this reads PROXIES from the 250 ms [DIAG] ticks, per row and leg:
  l      = the lowest rtp_us over the row (Copa's min_rtt; its lifetime low = the gauge's l-hat)
  bootq  = max over the first 8 ticks of (tick echo mean - l): an UPPER proxy for b0p (a window's minimum is
           below its mean); bootq_med = their median
  lateq  = median over LATE ticks (t >= 15 s; c1s 5 s) of (tick echo mean - l): the standing queue as s56 read it
  lm     = median over LATE ticks of (rtp_us - l): m-hat's lift above l-hat. lm > b0p means the anchor binds
           (anc = 1), and the law's floor becomes l-hat + b0p, so its target gains one more b0p (R6.2's F).
  sigb   = max qsp_us over the first 8 ticks (the gauge's sigb); sig = median qsp_us over LATE ticks (sig at load)
The tick echo mean is the top-level rtt (0.1 ms) on single-leg cells, the per-leg rtt (1 ms) at c8.
"""
import glob, os, re, statistics as St, sys
from collections import defaultdict

D = sys.argv[1]
LEG = r' p%d:infl=.*?rtt=(\d+)/wrtt=(\d+)/rtp\d+ms rtp_us=(\d+) .*?qsp_us=(\d+)/n'
out = defaultdict(list)
for f in sorted(glob.glob(os.path.join(D, '*-c.log'))):
    m = re.match(r'(c\w+-\d+)@def-(P\d+)-(s\d+)-(r\d+)-', os.path.basename(f))
    if not m:
        continue
    cell, P = m.group(1), m.group(2)
    T = []
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'):
            continue
        t = float(re.search(r't=([\d.]+)s', L).group(1))
        top = re.search(r' rtt=([\d.]+)ms', L)
        x = dict(t=t, top=float(top.group(1)) if top else None)
        for i in (0, 1):
            g = re.search(LEG % i, L)
            if g:
                x[i] = tuple(int(v) for v in g.groups())
        T.append(x)
    if not T:
        continue
    legs = [i for i in (0, 1) if i in T[0]]
    t0 = 5.0 if cell.startswith('c1s') else 15.0
    for i in legs:
        l = min(x[i][2] for x in T if i in x) / 1e3

        def rt(x):
            return x['top'] if (len(legs) == 1 and x['top']) else float(x[i][0])

        boot = [x for x in T[:8] if i in x]
        late = [x for x in T if x['t'] >= t0 and i in x]
        if not late:
            continue
        bq = max(rt(x) - l for x in boot)
        bqm = St.median([rt(x) - l for x in boot])
        lq = St.median([rt(x) - l for x in late])
        lm = St.median([x[i][2] / 1e3 - l for x in late])
        sb = max(x[i][3] for x in boot) / 1e3
        sl = St.median([x[i][3] for x in late]) / 1e3
        out[(cell, int(P[1:]), i)].append((l, bq, bqm, lq, lm, sb, sl))

print('cell P leg n | l | bootq max [range] (median) | bootq_med | lateq [range] | lm [range] | sigb [range] | sig')
for k in sorted(out):
    v = out[k]

    def md(j):
        return St.median([r[j] for r in v])

    def rng(j):
        return '%.2f-%.2f' % (min(r[j] for r in v), max(r[j] for r in v))

    print('%s P%d p%d n%d | %.2f | %s (%.2f) | %.2f | %.2f [%s] | %.2f [%s] | %.2f [%s] | %.2f'
          % (k[0], k[1], k[2], len(v), md(0), rng(1), md(1), md(2), md(3), rng(3), md(4), rng(4), md(5), rng(5),
             md(6)))
