"""s60 ruling entry 7, item 2: the c2 anchor flip against the knee, from s56's rows, for each boot-window count K.

Usage: python -I s60flip.py <s56 .../run/rep56/diag-ld> <loop-diag/p27/diag-ld> <l1-raw/s56/rep56-ld.log>

Under session calibration the replay's anchor binds on a row iff its late Copa minimum m-hat >= T := l_c + b0_c(K),
with l_c the session median l at W 128 and b0_c(K) the session median over W 128 rows of max(first K brakes).
  * b0_c(K): p27's cap 64-128 c2 rows (the only c2 window minima at queue-free caps; s56 [DIAG] has none): per row
    max over the first K windows of (rtt_min - rtprop), then the median over rows; its uncertainty as a session
    median of 6 rows = 1.253 * sd_rows / sqrt(6).
  * l_c: s56's P128 rows' l; same session-median uncertainty.
  * the flip W_flip,r per s56 replicate r: where that replicate's late m-hat (median rtp_us over LATE ticks) crosses T,
    interpolated linearly over the arms 128 / 160 / 192 / 256 (s60 adds 176).
  * the knee W95,r: where that replicate's goodput over its own control crosses 0.95, interpolated over 128-256.
Monte Carlo (seed 60, 20 000 draws) over T ~ Normal(T_hat, se_T), one draw per session (T is common to all
replicates), and over the replicate's arm rows (resampled from s56's 3 replicates, 6 per session):
  STRUCTURAL iff W_flip,r >= W95,r on >= 5 of 6; NOT iff < on >= 5 of 6; MIXED otherwise; FLAT-AT-KNEE iff the
  pooled median flip is at or above the knee (the knee arms then sit at anc 0, where R~ is flat in W). Also the pooled
  median over replicates of (W_flip - W95), with its 10 / 50 / 90 % points.
"""
import glob, json, os, random, re, statistics as St, sys
from collections import defaultdict

S56, P27 = sys.argv[1], sys.argv[2]
LD = re.compile(r'\[LOOPDIAG\] t=([\d.]+)s p0 n=\d+ .*?rtt_min_us=(\d+) .*?rtprop_us=(\d+)')

# b0_c(K) from p27 queue-free c2 rows
rowsK = []
for f in sorted(glob.glob(os.path.join(P27, 'c2-100-DEF*-c.log'))):
    cap = int(re.search(r'DEF(\d+)-', f).group(1))
    if cap not in (64, 96, 128):
        continue
    br = [(int(m.group(2)) - int(m.group(3))) / 1e3 for m in map(LD.search, open(f, errors='replace')) if m]
    if len(br) >= 8:
        rowsK.append([max(br[:k]) for k in range(1, 9)])
b0K = [St.median([r[k] for r in rowsK]) for k in range(8)]
seK = [1.253 * St.stdev([r[k] for r in rowsK]) / 6 ** 0.5 for k in range(8)]

# s56 per replicate: late m-hat per arm, goodput per arm, l at P128
LEG = r' p0:infl=.*?rtp_us=(\d+) '
mhat = defaultdict(dict)
lc_rows = []
for f in sorted(glob.glob(os.path.join(S56, 'c2-100@def-P*-c.log'))):
    m = re.match(r'c2-100@def-P(\d+)-s(\d+)-r(\d+)-', os.path.basename(f))
    P, rep = int(m.group(1)), int(m.group(3))
    T = []
    for L in open(f, errors='replace'):
        if L.startswith('[DIAG]'):
            t = float(re.search(r't=([\d.]+)s', L).group(1))
            src = re.search(r' src=(\d+)', L)
            g = re.search(LEG, L)
            if g:
                T.append((t, int(src.group(1)) if src else 0, int(g.group(1)) / 1e3))
    late = [x[2] for x in T if x[0] >= 15 and x[1] > 0]
    if late:
        mhat[rep][P] = St.median(late)
    if P == 128 and T:
        lc_rows.append(min(x[2] for x in T))
good = defaultdict(dict)
for L in open(sys.argv[3], errors='replace'):
    if L.startswith('P38ROW '):
        r = json.loads(L[7:])
        if r['cell'] == 'c2-100' and r.get('mbps'):
            good[int(r['rep'])][int(r['arm'][1:])] = r['mbps']
lc = St.median(lc_rows)
se_lc = 1.253 * St.stdev(lc_rows) / 6 ** 0.5


def cross(pts, y):
    """First W where the piecewise-linear [(W, v)] reaches y (rising); None if never."""
    pts = sorted(pts)
    if pts[0][1] >= y:
        return float(pts[0][0])
    for (w1, a), (w2, b) in zip(pts, pts[1:]):
        if a < y <= b:
            return w1 + (y - a) * (w2 - w1) / (b - a)
    return None


reps = sorted(set(mhat) & set(good))
knee = {}
for r in reps:
    ctl = good[r][2048]
    knee[r] = cross([(w, good[r][w] / ctl) for w in (128, 160, 192, 256) if w in good[r]], 0.95)
print('b0_c(K) ms (p27 c2 caps 64-128, %d rows):' % len(rowsK), ' '.join('K%d %.2f+-%.2f' % (k + 1, b0K[k], seK[k]) for k in range(8)))
print('l_c %.2f +- %.2f ms (s56 P128 rows %s)' % (lc, se_lc, ' '.join('%.2f' % x for x in lc_rows)))
print('s56 late m-hat per replicate:', {r: {w: round(v, 2) for w, v in sorted(mhat[r].items()) if w <= 256} for r in reps})
print('s56 knee W95 per replicate:', {r: round(knee[r], 1) for r in reps})
print()
print('K | T = l_c + b0_c(K) | flip per s56 replicate | P(STRUCTURAL) P(NOT) P(MIXED) | P(FLAT-AT-KNEE: pooled flip >= knee) | pooled median(flip - knee) 10/50/90 %')
rnd = random.Random(60)
for k in range(8):
    T0 = lc + b0K[k]
    seT = (se_lc ** 2 + seK[k] ** 2) ** 0.5
    flips = {r: cross([(w, v) for w, v in mhat[r].items() if w <= 256], T0) for r in reps}
    cnt = defaultdict(int)
    pooled = []
    for _ in range(20000):
        T = rnd.gauss(T0, seT)
        d = []
        for _ in range(6):
            r = rnd.choice(reps)
            fw = cross([(w, v) for w, v in mhat[r].items() if w <= 256], T)
            fw = fw if fw is not None else 256.0
            d.append(fw - knee[r])
        up = sum(x >= 0 for x in d)
        cnt['S' if up >= 5 else ('N' if up <= 1 else 'M')] += 1
        pooled.append(St.median(d))
    pooled.sort()
    q = lambda p: pooled[int(p * len(pooled))]
    flat = sum(x >= 0 for x in pooled) / len(pooled)
    print('K%d | %.2f +- %.2f | %s | %.2f %.2f %.2f | %.2f | %+.0f / %+.0f / %+.0f slots'
          % (k + 1, T0, seT, ' '.join('%.0f' % flips[r] if flips[r] else '-' for r in reps),
             cnt['S'] / 20000, cnt['N'] / 20000, cnt['M'] / 20000, flat, q(0.1), q(0.5), q(0.9)))
