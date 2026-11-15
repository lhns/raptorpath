"""s60 registration input: row lengths per cell, and the G/C pair sizing from s57a's c1s rows.

Usage: python -I s60size.py <docs/l1-raw>

(1) Row wall time (P38ROW wall_s, the driver's whole invocation) per cell and arm, s56 / s57a / s57b ledgers, plus
    the slot-to-slot spacing from the '=== rep=' headers (wall time + the harness's between-row work).
(2) s57a c1s 5 GB rows (rep57a-ld.log P38ROW + the collapse tags of rep57a-score.txt's per-row table):
    per arm, over unflagged rows: sd of client CPU s/GB, server CPU s/GB, goodput (as a fraction of the arm median);
    within-replicate pairs of unflagged rows across arms: the sd of the paired log-ratio (a constant arm effect does
    not change an sd), the quantity a G/C pair's difference has.
(3) Sizing for a 5 % G/C difference, two-sided alpha 0.05, power 0.8, with t quantiles: the smallest n of
    unflagged pairs with (t_{0.975,n-1} + t_{0.80,n-1}) * sd_d / sqrt(n) <= 0.05; sd_d is the LARGER of the measured
    cross-arm pair sd and sqrt(2) x the row sd (row sd 3-4 % goodput as the brief states and s57a reads; CPU
    from the per-arm sds above). Launched pairs = n / (1 - c)^2, c the collapse share (s56 P2048 3/9 = 0.33;
    s57a 0.29-0.48), since a pair counts only when both rows are unflagged.
"""
import glob, json, math, os, re, statistics as St, sys
from collections import defaultdict

L1 = sys.argv[1]


def rows(path):
    out = []
    for L in open(path, errors='replace'):
        if L.startswith('P38ROW '):
            out.append(json.loads(L[7:]))
    return out


def spacing(path):
    ts = []
    for L in open(path, errors='replace'):
        m = re.match(r'=== rep=\S+ seed=\S+ cell=\S+ arm=\S+ attempt=\S+ .* (\d\d):(\d\d):(\d\d)$', L.strip())
        if m:
            t = int(m.group(1)) * 3600 + int(m.group(2)) * 60 + int(m.group(3))
            if not ts or ts[-1] != t:
                ts.append(t)
    return [b - a for a, b in zip(ts, ts[1:]) if b > a]


print('(1) row wall time, s (median [min-max]) per cell/arm; slot spacing per ledger')
for name, p in (('s56', 's56/rep56-ld.log'), ('s57a', 's57/rep57a-ld.log'), ('s57b', 's57/rep57b-ld.log')):
    f = os.path.join(L1, p)
    by = defaultdict(list)
    for r in rows(f):
        if r.get('wall_s') is not None:
            by[(r['cell'], r['arm'])].append(r['wall_s'])
    for k in sorted(by):
        v = by[k]
        print('  %s %s %s n%d  %.0f [%.0f-%.0f]' % (name, k[0], k[1], len(v), St.median(v), min(v), max(v)))
    sp = spacing(f)
    if sp:
        print('  %s slot spacing n%d median %.0f s [%.0f-%.0f], total %.1f min' % (
            name, len(sp), St.median(sp), min(sp), max(sp), sum(sp) / 60))

# (2) s57a
flag = {}
for L in open(os.path.join(L1, 's57/rep57a-score.txt'), errors='replace'):
    m = re.match(r'\s+(B\d+-\d+)\s+s(\d+)\s+r(\d+) \|.*\|([^|]*)$', L.rstrip('\n'))
    if m:
        flag[(m.group(1).replace('-', '_'), m.group(2), int(m.group(3)))] = 'COLLAPSE' in m.group(4) or 'DNF' in m.group(4)
R = rows(os.path.join(L1, 's57/rep57a-ld.log'))
GB = 5.0
arm = defaultdict(list)
rep = defaultdict(dict)
for r in R:
    k = (r['arm'], str(r['seed']), int(r['rep']))
    fl = flag.get(k, True)
    x = dict(cc=r['cpu_cli'] / GB, cs=r['cpu_srv'] / GB, g=r['mbps'] or 0.0, fl=fl)
    arm[r['arm']].append(x)
    rep[int(r['rep'])][r['arm']] = x
print()
print('(2) s57a c1s 5 GB: per arm over unflagged rows (sd as a fraction of the arm median)')
med = {}
for a in sorted(arm):
    u = [x for x in arm[a] if not x['fl']]
    c = 1 - len(u) / len(arm[a])
    med[a] = {k: St.median([x[k] for x in u]) for k in ('cc', 'cs', 'g')}
    sd = {k: St.stdev([x[k] for x in u]) / med[a][k] for k in ('cc', 'cs', 'g')}
    print('  %s n %d unflagged %d collapse %.2f | client CPU s/GB %.2f sd %.3f | server %.2f sd %.3f | goodput %.1f sd %.3f'
          % (a, len(arm[a]), len(u), c, med[a]['cc'], sd['cc'], med[a]['cs'], sd['cs'], med[a]['g'], sd['g']))
print('  within-replicate pairs (both unflagged), sd of the log-ratio:')
arms = sorted(arm)
sdd = defaultdict(list)
for i in range(len(arms)):
    for j in range(i + 1, len(arms)):
        a, b = arms[i], arms[j]
        d = defaultdict(list)
        for rp, v in rep.items():
            if a in v and b in v and not v[a]['fl'] and not v[b]['fl']:
                for k in ('cc', 'cs', 'g'):
                    d[k].append(math.log(v[b][k] / v[a][k]))
        if len(d['cc']) >= 3:
            s = {k: St.stdev(d[k]) for k in d}
            for k in s:
                sdd[k].append(s[k])
            print('    %s vs %s: n %d  sd client CPU %.3f  server CPU %.3f  goodput %.3f'
                  % (b, a, len(d['cc']), s['cc'], s['cs'], s['g']))
# t quantiles (one-sided upper tail), df 1..40: 0.975 and 0.80
T975 = [12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160, 2.145, 2.131,
        2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056, 2.052, 2.048, 2.045, 2.042,
        2.040, 2.037, 2.035, 2.032, 2.030, 2.028, 2.026, 2.024, 2.023, 2.021]
T80 = [1.376, 1.061, 0.978, 0.941, 0.920, 0.906, 0.896, 0.889, 0.883, 0.879, 0.876, 0.873, 0.870, 0.868, 0.866,
       0.865, 0.863, 0.862, 0.861, 0.860, 0.859, 0.858, 0.858, 0.857, 0.856, 0.856, 0.855, 0.855, 0.854, 0.854,
       0.853, 0.853, 0.853, 0.852, 0.852, 0.852, 0.851, 0.851, 0.851, 0.851]


def n_pairs(sd, d=0.05):
    for n in range(2, 41):
        if (T975[n - 2] + T80[n - 2]) * sd / math.sqrt(n) <= d:
            return n
    return None


def mdd(sd, n):
    return (T975[n - 2] + T80[n - 2]) * sd / math.sqrt(n)


print()
print('(3) G/C sizing for a 5 percent difference (t quantiles; sd_d = max(measured pair sd, sqrt(2) x row sd))')
ROWSD = {'cc': max(St.stdev([x['cc'] for x in arm[a] if not x['fl']]) / med[a]['cc'] for a in arm),
         'cs': max(St.stdev([x['cs'] for x in arm[a] if not x['fl']]) / med[a]['cs'] for a in arm),
         'g': 0.04}
for k, lab in (('cc', 'client CPU/GB'), ('cs', 'server CPU/GB'), ('g', 'goodput')):
    sd = max(max(sdd[k]), math.sqrt(2) * ROWSD[k])
    n = n_pairs(sd)
    print('  %s: pair sd max(%.3f measured, %.3f = sqrt2 x row sd %.3f) = %.3f -> %d unflagged pairs; launched at c 0.29 / 0.33 / 0.48: %.0f / %.0f / %.0f'
          % (lab, max(sdd[k]), math.sqrt(2) * ROWSD[k], ROWSD[k], sd, n, n / 0.71 ** 2, n / 0.67 ** 2, n / 0.52 ** 2))
    print('     minimum detectable difference at 4 / 6 / 8 unflagged pairs: %.3f / %.3f / %.3f'
          % (mdd(sd, 4), mdd(sd, 6), mdd(sd, 8)))
