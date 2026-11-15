"""Score candidate placement prices on the committed ledgers (paper 5.7,
Table 5.7a; status 38).

Usage (from docs/l1-raw/loop-diag):
  python3 ../../../tools/l1/place38_ledger_score.py \n    'p34/run/diag-ld/c1d-400-DEF*-c.log' 'p27/diag-ld/c8-100-DEF*-c.log'

For every two-leg row (sender `-c.log`), per leg: post-warm-up (t >= 1 s)
medians of the `[DIAG]` per-path `rtt` (app-echo srtt, ms), `wrtt` (quinn,
ms), `infl`; the last `rtp_us`; the `[LOOPDIAG]` spread S = p50 - min, as the
post-warm-up median over windows with samples, and for a leg that stops
sampling, its last window with samples (what "keep S when idle" keeps).
Loss per leg from `[TRUTH]`.

A leg is LOCKED iff its egress share < 2 % and its median rtt >= 3x its
median wrtt.

Gap g = (c_L - c_A)/T at x = I_A/W_A = 0 (the active leg's queue term
omitted: the conservative case for the idle leg), T = 0.15, ref = s_A,
Bulk w_bw = 1, r = eps/(1-eps):
  SHIP : (1/2+e)*s
  B1   : (1/2+e)*(RTprop + S),  S = p50 - min      (kept when idle)
  B2   : (1/2+e)*(RTprop + S),  S = p50 - RTprop   (= p50)
  WIRE : (1/2+e)*w   (quinn srtt)
  RTP  : (1/2+e)*RTprop
"""
import glob, os, re, sys, statistics as st

T = 0.15
KV = re.compile(r'(\w+)=([-\w.]+)')


def num(v):
    try:
        return float(v)
    except (TypeError, ValueError):
        return None


def parse(clog):
    diag, loop, truth, loss = {}, {}, {}, {}
    good = None
    base = clog[:-len('-c.log')]
    for f in glob.glob(base + '-*'):
        for line in open(f, errors='replace'):
            if '[TRUTH] leg=' in line:
                d = dict(KV.findall(line))
                truth[int(d['leg'])] = int(d['egress_dgrams'])
                loss[int(d['leg'])] = float(d['loss'])
            elif '"summary":true' in line:
                good = num(re.search(r'"mean_mbps":([\d.]+)', line).group(1))
    for line in open(clog, errors='replace'):
        if '[LOOPDIAG]' in line:
            m = re.search(r't=([\d.]+)s p(\d) (.*)', line)
            if m:
                d = {k: num(v) for k, v in KV.findall(m.group(3))}
                loop.setdefault(int(m.group(2)), []).append((float(m.group(1)), d))
        elif '[DIAG] t=' in line and 'p0:infl=' in line:
            t = float(re.search(r'\[DIAG\] t=([\d.]+)s', line).group(1))
            for m in re.finditer(r' p(\d):infl=(\d+)/.*?rtt=(\d+)/wrtt=(\d+)/rtp\d+ms rtp_us=(\d+)', line):
                diag.setdefault(int(m.group(1)), []).append(
                    (t, int(m.group(2)), int(m.group(3)), int(m.group(4)), int(m.group(5))))
    return diag, loop, truth, loss, good


def leg_state(diag, loop, p):
    post = [x for x in diag[p] if x[0] >= 1.0] or diag[p]
    s = st.median(x[2] for x in post)
    w = st.median(x[3] for x in post)
    infl = st.median(x[1] for x in post)
    rtp = [x[4] for x in diag[p] if x[4] > 0][-1] / 1000.0
    wins = [d for (t, d) in loop.get(p, []) if d.get('n') and t >= 1.0]
    allw = [d for (t, d) in loop.get(p, []) if d.get('n')]
    if wins:
        smin = st.median(d['rtt_p50_us'] - d['rtt_min_us'] for d in wins) / 1000.0
        p50 = st.median(d['rtt_p50_us'] for d in wins) / 1000.0
    else:  # sampling stopped: keep the last window with samples
        d = allw[-1]
        smin = (d['rtt_p50_us'] - d['rtt_min_us']) / 1000.0
        p50 = d['rtt_p50_us'] / 1000.0
    return dict(s=s, w=w, infl=infl, rtp=rtp, S1=smin, p50=p50, kept=not wins)


def price(kind, L, e):
    base = {'SHIP': L['s'], 'B1': L['rtp'] + L['S1'], 'B2': L['p50'],
            'WIRE': L['w'], 'RTP': L['rtp']}[kind]
    return (0.5 + e) * base


KINDS = ['SHIP', 'B1', 'B2', 'WIRE', 'RTP']


def score(clog):
    diag, loop, truth, loss, good = parse(clog)
    if len(diag) < 2 or len(truth) < 2:
        return None
    tot = sum(truth.values())
    share = {p: truth[p] / tot for p in truth}
    legs = {p: leg_state(diag, loop, p) for p in (0, 1)}
    # The idle (or slower-share) leg is L, the other A.
    L = min((0, 1), key=lambda p: share[p])
    A = 1 - L
    sL, sA = legs[L], legs[A]
    locked = share[L] < 0.02 and sL['s'] >= 3 * sL['w']
    ref = min(sA['s'], sL['s'])
    eL, eA = loss.get(L, 0.0), loss.get(A, 0.0)
    rL, rA = eL / (1 - eL), eA / (1 - eA)
    gaps = {k: ((price(k, sL, eL) - price(k, sA, eA)) / ref + (rL - rA)) / T for k in KINDS}
    return dict(row=os.path.basename(clog)[:-len('-a1-c.log')], good=good, L=L,
                shareL=share[L], locked=locked, sL=sL, sA=sA, ref=ref, gaps=gaps)


if __name__ == '__main__':
    for pat in sys.argv[1:]:
        for f in sorted(glob.glob(pat)):
            r = score(f)
            if not r:
                continue
            sL, sA = r['sL'], r['sA']
            g = ' '.join('%s=%+.1f' % (k, r['gaps'][k]) for k in KINDS)
            print('%-28s %s good=%6.1f L=p%d sh=%.3f | L s=%.0f w=%.0f rtp=%.1f S=%.1f p50=%.1f%s | A s=%.0f w=%.0f rtp=%.1f S=%.1f | %s'
                  % (r['row'], 'LOCK' if r['locked'] else '    ', r['good'] or 0, r['L'], r['shareL'],
                     sL['s'], sL['w'], sL['rtp'], sL['S1'], sL['p50'], '*' if sL['kept'] else ' ',
                     sA['s'], sA['w'], sA['rtp'], sA['S1'], g))
