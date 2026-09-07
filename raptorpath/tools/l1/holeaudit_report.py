import re, sys, json, math, collections

LOG = sys.argv[1]
CELLS = ['c1', 'c7', 'sc2', 'c8']

def f(line, key, default=None):
    for t in line.split():
        if t.startswith(key):
            v = t[len(key):]
            m = re.match(r'^[-+0-9.eE]*', v)
            v = m.group(0)
            if v in ('', '-'):
                return default
            return v
    return default

def i(line, key, default=0):
    v = f(line, key, None)
    try:
        return int(v)
    except (TypeError, ValueError):
        return default

def fl(line, key, default=None):
    v = f(line, key, None)
    try:
        return float(v)
    except (TypeError, ValueError):
        return default

def pct(a, b):
    return ('%.4f' % (a / b)) if b else '-'

def fmt(v):
    return ('%.4f' % v) if isinstance(v, float) else ('-' if v is None else str(v))

def wilson(k, n, z=1.96):
    if n == 0:
        return (None, None)
    p = k / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = z / d * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n))
    return (max(0.0, c - h), min(1.0, c + h))

def med(v):
    v = sorted(x for x in v if x is not None)
    if not v:
        return None
    n = len(v)
    return v[n // 2] if n % 2 else (v[n // 2 - 1] + v[n // 2]) / 2

hold = collections.defaultdict(list)     # cell -> [line]
succ = collections.defaultdict(list)
diag = collections.defaultdict(list)
fcau = collections.defaultdict(list)
ctld = collections.defaultdict(list)
gate = collections.defaultdict(list)
genw = collections.defaultdict(list)
good = collections.defaultdict(list)
rcs = collections.defaultdict(list)
cur = None

for ln in open(LOG, encoding='utf-8', errors='replace'):
    ln = ln.rstrip('\n')
    m = re.match(r'=== cell=(\S+) rep=(\d+)', ln)
    if m:
        cur = m.group(1)
        continue
    if ln.startswith('HOLD ') and ' cli ' in ln and 'path=-' not in ln:
        hold[ln.split()[1]].append(ln)
    elif ln.startswith('SUCC ') and ' srv ' in ln:
        succ[ln.split()[1]].append(ln)
    elif ln.startswith('DIAG '):
        diag[ln.split()[1]].append(ln)
    elif ln.startswith('FCAUSE '):
        fcau[ln.split()[1]].append(ln)
    elif ln.startswith('CTLD '):
        ctld[ln.split()[1] + '/' + ln.split()[3]].append(ln)
    elif ln.startswith('GATE '):
        gate[ln.split()[1]].append(ln)
    elif ln.startswith('GENW '):
        genw[ln.split()[1]].append(ln)
    elif '"summary":true' in ln and cur:
        try:
            good[cur].append(json.loads(ln[ln.index('{'):]))
        except Exception:
            pass
    elif ln.startswith('RC=') and cur:
        rcs[cur].append(ln)

print('=' * 78)
print('ABORT-CAUSE / WITNESS TABLE')
print('=' * 78)
for c in CELLS:
    bad = [g for g in gate[c] if not g.endswith('cli=0 srv=0')]
    g1 = [g for g in genw[c] if 'gen=0' not in g]
    print('%-4s reps=%d holdlines=%d succlines=%d contamination=%d gen1=%d'
          % (c, len(good[c]), len(hold[c]), len(succ[c]), len(bad), len(g1)))
    for b in bad[:4]:
        print('     CONTAMINATION: %s' % b)

print()
print('=' * 78)
print('PER-CELL: goodput, the closure-class table, same/cross, ripeness, waste')
print('=' * 78)
for c in CELLS:
    if not hold[c] and not succ[c]:
        print('%-4s NO ROWS' % c)
        continue
    mb = [g.get('mean_mbps') for g in good[c]]
    hn = sum(i(l, 'hn_n=') for l in hold[c])
    hy = sum(i(l, 'hy_n=') for l in hold[c])
    cx = sum(i(l, 'cx_n=') for l in hold[c])
    fed = sum(i(l, 'fed=') for l in hold[c])
    spH = sum(i(l, 'sp_n=') for l in hold[c])
    xpH = sum(i(l, 'xp_n=') for l in hold[c])
    upH = sum(i(l, 'up_n=') for l in hold[c])
    an = sum(i(l, 'age_n=') for l in hold[c])
    ar = sum(i(l, 'age_ripe=') for l in hold[c])
    ev = sum(i(l, 'evals=') for l in hold[c])
    su = sum(i(l, 'sup=') for l in hold[c])
    det = sum(i(l, 'det=') for l in succ[c])
    res = sum(i(l, 'res=') for l in succ[c])
    on = sum(i(l, 'orig_n=') for l in succ[c])
    rn = sum(i(l, 'rep_n=') for l in succ[c])
    ab = sum(i(l, 'aban_n=') for l in succ[c])
    op = sum(i(l, 'open=') for l in succ[c])
    ov = sum(i(l, 'over=') for l in succ[c])
    spS = sum(i(l, 'sp_n=') for l in succ[c])
    xpS = sum(i(l, 'xp_n=') for l in succ[c])
    taper = sum(i(l, 'taper=') for l in diag[c])
    retx = sum(i(l, 'retx=') for l in diag[c])
    fcn = sum(i(l, 'n=') for l in fcau[c])
    heal = hn + hy
    tot = heal + cx
    pi0 = heal / tot if tot else None
    lo, hi = wilson(heal, tot)
    print()
    print('--- %s  goodput med=%s Mbit  (n=%d reps) ---'
          % (c, ('%.1f' % med(mb)) if med(mb) else '-', len(good[c])))
    print('  [HOLD] fed=%d  hn=%d (%s)  hy=%d (%s)  cx=%d (%s)   identity %s'
          % (fed, hn, pct(hn, fed), hy, pct(hy, fed), cx, pct(cx, fed),
             'OK' if hn + hy + cx == fed else 'VIOLATED'))
    print('  pi0 (TRUE-HEAL share, corrected) = %s  [%s, %s]  n=%d'
          % (fmt(pi0), fmt(lo), fmt(hi), tot))
    print('  [HOLD] sp=%d xp=%d up=%d  (identity %s)  xp_frac=%s'
          % (spH, xpH, upH, 'OK' if spH + xpH + upH == fed else 'VIOLATED',
             fmt(xpH / (spH + xpH) if spH + xpH else None)))
    print('  RIPE AT FIRST REPORT: %d/%d = %s   age_p50=%s us  thr_p50=%s us'
          % (ar, an, fmt(ar / an if an else None),
             med([fl(l, 'age_p50_us=') for l in hold[c]]),
             med([fl(l, 'thr_p50_us=') for l in hold[c]])))
    print('  F (TRUE-HEAL CDF, heal classes only): hn p50=%s p90=%s | hy p50=%s p90=%s (us)'
          % (med([fl(l, 'hn_p50_us=') for l in hold[c]]),
             med([fl(l, 'hn_p90_us=') for l in hold[c]]),
             med([fl(l, 'hy_p50_us=') for l in hold[c]]),
             med([fl(l, 'hy_p90_us=') for l in hold[c]])))
    print('  cx (retransmit closure) p50=%s p90=%s (us)'
          % (med([fl(l, 'cx_p50_us=') for l in hold[c]]),
             med([fl(l, 'cx_p90_us=') for l in hold[c]])))
    print('  [SUCC] det=%d res=%d orig=%d rep=%d aban=%d open=%d over=%d  identity %s'
          % (det, res, on, rn, ab, op, ov,
             'OK' if on + rn + ab + op + ov == det else 'VIOLATED'))
    print('  [SUCC] orig_frac (the RECORD\'s bound) = %s   vs corrected pi0 = %s'
          % (fmt(on / (on + rn) if on + rn else None), fmt(pi0)))
    print('  [SUCC] sp=%d xp=%d  xp/det=%s  xp_frac=%s  (identity %s)'
          % (spS, xpS, fmt(xpS / det if det else None),
             fmt(xpS / (spS + xpS) if spS + xpS else None),
             'OK' if spS + xpS == res else 'VIOLATED'))
    print('  WASTE SPLIT: taper_copy=%d  gap-fire retx=%d  [FCAUSE] n=%d'
          % (taper, retx, fcn))
    print('  GATE: evals=%d sup=%d (must be 0)' % (ev, su))

