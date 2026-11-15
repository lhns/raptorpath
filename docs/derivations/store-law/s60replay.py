"""s60: the replayed R~ (s60-registration.md section 1.4) - slaw.rs `slaw_r`, transcribed, fed from printed [SLAW] fields.

Usage:
  python -I s60replay.py selftest                       the engine's hand anchors (slaw.rs r_pinned_at_hand_computed_anchors)
  python -I s60replay.py witness <client.log>           PER-ROW gate: replay with the row's OWN b0p must equal printed
                                                        R and Rb on every defined window (rc 1 on any mismatch)
  python -I s60replay.py negcontrol <client.log>        the same with every tmin raised by 1 us: must FAIL (rc 0 iff it does)
  python -I s60replay.py replay <client.log> <cal>      R~ per window: cal = 'p:b0[:l],...' in ms per leg, the session
                                                        calibration (b0 always; l at c2 / c8 = T_c - b0_c; omitted at c1s)

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


def r67_r(legs, q):
    """The R6.7-form ratio (store-law-r67.md section 4.2; r67floor.py rcell), REPORTED beside R~, frozen for s60:
    legs: [(w, t_hat, t_min, sigma, F, l_c)] with w = `nk` (r67floor's dlv), t_hat = `tp50`, t_min = `tmin`,
    sigma = printed `sig` (r67floor's constant table), F = the session floor, l_c = the session lifetime minimum.
        R67 = sum_i w_i (F_i + 1/2*max(0, max_j F_j - F_i) + sigma_i + q*l_c,i + T_i - Tmin_i) / sum_i w_i T_i
    """
    live = [x for x in legs if x[0] > 0]
    if not live:
        return 1.0
    mx = max(x[4] for x in live)
    num = den = 0.0
    for w, t, tmin, sig, F, lc in live:
        num += w * (F + 0.5 * max(0.0, mx - F) + sig + q * lc + (t - tmin))
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
        # cal[p] = (b0_c, l_c or None): b0 substituted; l substituted when given (c2 / c8: l_c = T_c - b0_c),
        # the row's own `l` otherwise (c1s). s60-registration.md section 1.1.
        b0, l = (cal[int(d['p'])] if cal is not None else (num(d, 'b0p'), None))
        out.append((num(d, 'nk'), num(d, 'tp50'), num(d, 'tmin') + dtmin, num(d, 'm'),
                    l if l is not None else num(d, 'l'), b0, num(d, 'sigb' if boot else 'sig')))
    return out


def selftest():
    a = (100, 20.0, 18.0, 10.0, 9.0, 2.0, 1.5)
    b = (50, 40.0, 36.0, 30.0, 25.0, 3.0, 2.0)
    assert abs(slaw_r([a], 0.1) - 0.82) < 1e-12
    assert abs(slaw_r([a, b], 0.1) - 1.11625) < 1e-12
    assert slaw_r([], 0.1) == 1.0
    # R6.7 form, hand-computed: one leg (w 100, T 20, Tmin 18, sig 1.5, F 16, l_c 9): 100*(16+1.5+0.9+2)/2000 = 1.02.
    # Two legs, B (w 50, T 40, Tmin 36, sig 2, F 30, l_c 25): A gets 1/2*(30-16) = 7 -> 100*(16+7+1.5+0.9+2) = 2740;
    # B: 50*(30+0+2+2.5+4) = 1925; den 4000 -> 4665/4000 = 1.16625.
    a67 = (100, 20.0, 18.0, 1.5, 16.0, 9.0)
    b67 = (50, 40.0, 36.0, 2.0, 30.0, 25.0)
    assert abs(r67_r([a67], 0.1) - 1.02) < 1e-12
    assert abs(r67_r([a67, b67], 0.1) - 1.16625) < 1e-12
    print('SELFTEST PASS (0.82, 1.11625, empty set 1.0; R6.7 form 1.02, 1.16625)')


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
        cal = {}
        for x in sys.argv[3].split(','):
            f = x.split(':')
            cal[int(f[0])] = (float(f[1]), float(f[2]) if len(f) > 2 else None)
        replay(sys.argv[2], cal)
