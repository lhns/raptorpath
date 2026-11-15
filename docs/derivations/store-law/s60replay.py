"""s60: the replayed R~ (s60-registration.md section 1.4) - slaw.rs `slaw_r`, transcribed, fed from printed [SLAW] fields.

Usage:
  python -I s60replay.py selftest                       the engine's hand anchors (slaw.rs r_pinned_at_hand_computed_anchors)
  python -I s60replay.py witness <client.log>           PER-ROW gate: replay with the row's OWN b0p must equal printed
                                                        R and Rb on every defined window (rc 1 on any mismatch)
  python -I s60replay.py negcontrol <client.log>        the same with every tmin raised by 1 us: must FAIL (rc 0 iff it does)
  python -I s60replay.py replay <client.log> <cal>      R~ per window: b0 per leg from the replicate's own calibration
                                                        row, cal = 'p:b0,p:b0' in ms; every other input the row's own

    R = sum_i w_i (F_i + sig_i + q*l_i + T_i - Tmin_i) / sum_i w_i T_i
    F_i = min(m_i, l_i + b0_i) + b0_i + 1/2 * max(0, max_j (l_j + b0_j) - (l_i + b0_i))
w = nk, T = tp50, Tmin = tmin, m = m, l = l, sig = sig (load; Rb, which the witness also checks, uses sigb; Rb is not
scored), q = the printed q. A window's legs are the [SLAW] lines with the same k and nk > 0, after the last
'"warmup":true' marker; R exists only when every such leg has b0p, m and sigb (slaw.rs render_window's `ready`).
"""
import re
import sys

TOK = re.compile(r'(\w+)=(\S+)')
TOL = 6e-6                   # R is printed with 5 decimals; every input is printed exactly (integer us as ms.3f)


def slaw_r(legs, q):
    """legs: [(w, t_hat, t_min, m_hat, l_hat, b0, sigma)] -> R (slaw.rs slaw_r; empty set -> 1.0)."""
    live = [x for x in legs if x[0] > 0]
    if not live:
        return 1.0
    floor_all = max(x[4] + x[5] for x in live)
    num = den = 0.0
    for w, t, tmin, m, l, b0, sig in live:
        own = l + b0
        f = min(m, own) + b0 + 0.5 * max(0.0, floor_all - own)
        num += w * (f + sig + q * l + (t - tmin))
        den += w * t
    return num / den


def windows(path):
    """{k: [fields of each [SLAW] line with nk > 0]}, after the last warm-up marker."""
    out = {}
    for L in open(path, errors='replace'):
        if '"warmup":true' in L:
            out = {}
            continue
        if not L.startswith('[SLAW]'):
            continue
        d = dict(TOK.findall(L))
        if d.get('nk', '0') == '0':
            continue
        out.setdefault(int(d['k']), []).append(d)
    return out


def num(d, k):
    v = d.get(k, '-')
    return None if v == '-' else float(v)


def ready(ls):
    return all(num(d, 'b0p') is not None and num(d, 'm') is not None and num(d, 'sigb') is not None for d in ls)


def legs_of(ls, cal=None, boot=False, dtmin=0.0):
    out = []
    for d in ls:
        b0 = cal[int(d['p'])] if cal is not None else num(d, 'b0p')
        out.append((num(d, 'nk'), num(d, 'tp50'), num(d, 'tmin') + dtmin, num(d, 'm'), num(d, 'l'), b0,
                    num(d, 'sigb' if boot else 'sig')))
    return out


def selftest():
    a = (100, 20.0, 18.0, 10.0, 9.0, 2.0, 1.5)
    b = (50, 40.0, 36.0, 30.0, 25.0, 3.0, 2.0)
    assert abs(slaw_r([a], 0.1) - 0.82) < 1e-12
    assert abs(slaw_r([a, b], 0.1) - 1.11625) < 1e-12
    assert slaw_r([], 0.1) == 1.0
    print('SELFTEST PASS (0.82, 1.11625, empty set 1.0)')


def witness(path, dtmin=0.0):
    n = bad = 0
    worst = 0.0
    for k, ls in sorted(windows(path).items()):
        if not ready(ls) or ls[0].get('R', '-') == '-':
            continue
        q = num(ls[0], 'q')
        for boot, key in ((False, 'R'), (True, 'Rb')):
            e = abs(slaw_r(legs_of(ls, boot=boot, dtmin=dtmin), q) - num(ls[0], key))
            worst = max(worst, e)
            n += 1
            bad += e > TOL
    ok = n > 0 and bad == 0
    print('WITNESS %s windows %d mismatches %d worst |dR| %.2e%s'
          % ('PASS' if ok else 'FAIL', n, bad, worst, ' (tmin +%.3f ms)' % dtmin if dtmin else ''))
    return ok


def replay(path, cal):
    for k, ls in sorted(windows(path).items()):
        if ready(ls) and all(int(d['p']) in cal for d in ls):
            print('k=%d t=%s Rt=%.5f R=%s' % (k, ls[0]['t'], slaw_r(legs_of(ls, cal), num(ls[0], 'q')), ls[0].get('R')))


if __name__ == '__main__':
    cmd = sys.argv[1]
    if cmd == 'selftest':
        selftest()
    elif cmd == 'witness':
        sys.exit(0 if witness(sys.argv[2]) else 1)
    elif cmd == 'negcontrol':
        sys.exit(1 if witness(sys.argv[2], dtmin=0.001) else 0)
    elif cmd == 'replay':
        replay(sys.argv[2], {int(p): float(b) for p, b in (x.split(':') for x in sys.argv[3].split(','))})
