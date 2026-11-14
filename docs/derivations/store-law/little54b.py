"""Little's law per leg on the section-54b store-budget block (status 54, launch 2).

Usage: python -I little54b.py <extracted diag-ld dir>   (from docs/l1-raw/s54/rep54b-diag-ld.tgz)

LATE window = client [DIAG] ticks with t >= 15 s and src > 0 (the block's registered selector).
Per row and leg i: r_i = d(dgq_i tx)/dt over the window (datagrams/s, ~ symbols/s at c8);
rtt_i, wrtt_i = medians of the app-echo / quinn wire RTT; q_i = median dgq_i hand - tx;
qcw_i = median quinn cwnd in datagrams (1233 B). P_i = r_i * rtt_i (Little: the leg's population).
W = the tick's own cap (win=a/b denominator); a = counted store; T_W = W / sum r (the store's
residence time if the whole store were the queue). resid = a - sum P_i (holes + slack).
Server: [RANK] holes (all open) and old holes (holes - tail_overcount), readouts 15..-6;
[SUCC] det / orig_n / rep_n (holes detected; filled by the original = reorder; filled by repair).
Client [HOLD] hn_n per path (self-heal with no retransmit) and [SNAPV] reports per path.
"""
import glob, os, re, statistics as S, sys

D = sys.argv[1]
PP = (r' p%d:infl=\d+/sinfl=\d+/bdp(\d+)\S* sout=\d+ khr=\S+ btlbw=(\d+).*?rtt=(\d+)/wrtt=(\d+)/rtp\d+ms'
      r' rtp_us=(\d+).*?qcwnd=(\d+)')
m = S.median


def row(f):
    ticks = []
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'):
            continue
        t = float(re.search(r't=([\d.]+)s', L).group(1))
        src = re.search(r' src=(\d+)', L)
        dq = [re.search(r'dgq%d\[hand=(\d+) tx=(\d+)' % i, L) for i in (0, 1)]
        pp = [re.search(PP % i, L) for i in (0, 1)]
        w = re.search(r'win=(\d+)/(\d+)', L)
        if not (src and all(dq) and all(pp) and w) or t < 15 or int(src.group(1)) == 0:
            continue
        x = dict(t=t, a=int(w.group(1)), W=int(w.group(2)))
        for i in (0, 1):
            x['tx%d' % i] = int(dq[i].group(2))
            x['q%d' % i] = int(dq[i].group(1)) - int(dq[i].group(2))
            bdp, bw, rtt, wrtt, rtp, qcw = (int(v) for v in pp[i].groups())
            x.update({'bdp%d' % i: bdp, 'bw%d' % i: bw, 'rtt%d' % i: rtt, 'wrtt%d' % i: wrtt,
                      'rtp%d' % i: rtp / 1000, 'qcw%d' % i: qcw / 1233})
        ticks.append(x)
    if len(ticks) < 4:
        return None
    a, b = ticks[0], ticks[-1]
    dt = b['t'] - a['t']
    o = {'n': len(ticks)}
    for i in (0, 1):
        o['r%d' % i] = (b['tx%d' % i] - a['tx%d' % i]) / dt
        for k in ('rtt', 'wrtt', 'q', 'qcw', 'rtp', 'bw', 'bdp'):
            o['%s%d' % (k, i)] = m([x['%s%d' % (k, i)] for x in ticks])
        o['P%d' % i] = m([o['r%d' % i] * x['rtt%d' % i] / 1000 for x in ticks])
        o['wire%d' % i] = m([o['r%d' % i] * x['wrtt%d' % i] / 1000 for x in ticks])
    o['W'] = m([x['W'] for x in ticks])
    o['a'] = m([x['a'] for x in ticks])
    o['R'] = o['r0'] + o['r1']
    o['TW_ms'] = 1000 * o['W'] / o['R']
    o['Tbar_ms'] = 1000 * (o['P0'] + o['P1']) / o['R']      # rate-weighted mean echo RTT
    o['resid'] = m([x['a'] - o['r0'] * x['rtt0'] / 1000 - o['r1'] * x['rtt1'] / 1000 for x in ticks])
    o['ask'] = m([1.1 * (x['bw0'] * x['rtp0'] + x['bw1'] * x['rtp1']) / 1000 for x in ticks])
    # server side
    s = f[:-6] + '-s.log'
    rk = [L for L in open(s, errors='replace') if L.startswith('[RANK]')][15:-6]
    if rk:
        h = [int(re.search(r'holes=(\d+)', L).group(1)) for L in rk]
        o['holes'] = m(h)
        o['old'] = m([v - int(re.search(r'tail_overcount=(\d+)', L).group(1)) for v, L in zip(h, rk)])
    su = [L for L in open(s, errors='replace') if L.startswith('[SUCC]') and 'final=1' in L and ' det=0 ' not in L]
    if su:
        L = su[-1]
        for k in ('det', 'orig_n', 'rep_n', 'xp_n', 'sp_n'):
            o[k] = int(re.search(r' %s=(\d+)' % k, L).group(1))
    for L in open(f, errors='replace'):
        mm = re.match(r'\[HOLD\] site=sender path=(\d) .*? hn_n=(\d+) .*? cx_n=(\d+)', L)
        if mm:
            o['hn%s' % mm.group(1)] = int(mm.group(2))
            o['cx%s' % mm.group(1)] = int(mm.group(3))
        mm = re.match(r'\[SNAPV\].*? p0:rep=(\d+) .*? p1:rep=(\d+) stale=(\d+)', L)
        if mm:
            o['rep0'], o['rep1'], o['st1'] = (int(v) for v in mm.groups())
    return o


COLS = ['W', 'a', 'r0', 'r1', 'rtt0', 'wrtt0', 'q0', 'qcw0', 'rtt1', 'wrtt1', 'q1', 'qcw1',
        'P0', 'P1', 'resid', 'TW_ms', 'Tbar_ms', 'rtp0', 'rtp1', 'bw0', 'bw1', 'ask',
        'holes', 'old', 'det', 'orig_n', 'rep_n', 'hn0', 'hn1', 'cx0', 'cx1', 'rep0', 'rep1']
arms = {}
for f in sorted(glob.glob(os.path.join(D, 'c8-100@def-*-c.log'))):
    name = os.path.basename(f)[len('c8-100@def-'):-6]
    o = row(f)
    if o:
        arms.setdefault(name.split('-')[0], []).append(o)
order = ['B2048', 'B1024', 'B640', 'L2048', 'L1024', 'L640']
print('arm medians of per-row values (LATE window); [min..max] in the second table')
print('col', *order, sep='\t')
for c in COLS:
    vals = []
    for a in order:
        xs = [o[c] for o in arms.get(a, []) if c in o]
        vals.append('%.0f' % m(xs) if xs else '-')
    print(c, *vals, sep='\t')
print()
for c in ('r0', 'r1', 'R', 'resid', 'TW_ms', 'Tbar_ms', 'rtt0', 'rtt1', 'rtp0', 'rtp1', 'old', 'hn0', 'hn1', 'det', 'orig_n', 'rep_n'):
    print(c, *['[%.0f..%.0f]' % (min(o[c] for o in arms[a] if c in o), max(o[c] for o in arms[a] if c in o))
               for a in order], sep='\t')
