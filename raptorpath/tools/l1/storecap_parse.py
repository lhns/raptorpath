#!/usr/bin/env python3
"""goal-gate "Store-Cap Triplication" battery collector.

Parses `battery-s<seed>.log` into, per arm:
  - goodput mean +/- SAMPLE sigma with n (never a mean without its n --
    MEASUREMENT DISCIPLINE item 8), plus the per-run values
  - the same-session Sigma ratio for the dual cells, Sigma taken from THAT
    SEED's OWN singles arms in the SAME session (Sigma_c7 = 2*sc2,
    Sigma_c8 = sc2 + sc3), per arm
  - THE MECHANISM: the `[SF]` saturation-filter population (E =
    active_sum/live_sum, short%, zero%) and the effective store cap read
    off the DIAG `win=<occupancy>/<cap>` pair -- P1 is a claim about the
    cap, so the cap is reported per arm
  - retx / sweeps (P5's no-regression gauges)
  - run health: invocations / summaries / RUN-RETRY / RUN-LOST / liveness

  usage: storecap_parse.py <battery-s42.log> [battery-s7.log ...]
"""
import re
import sys
from statistics import mean, stdev

RE_HDR = re.compile(r'^=== rep=(\d+) arm=(\S+) attempt=(\d+) ')
RE_MBPS = re.compile(r'"mean_mbps":\s*([0-9.]+)')
RE_DNF = re.compile(r'"dnf":\s*(\d+)')
RE_CPU = re.compile(r'CPUSRV=([0-9.]+)s CPUCLI=([0-9.]+)s')
RE_SF = re.compile(
    r'^SF .*ticks=(\d+) live_sum=(\d+) active_sum=(\d+) '
    r'short_ticks=(\d+) zero_ticks=(\d+)')
RE_WIN = re.compile(r'win=(\d+)/(\d+)')
RE_RETX = re.compile(r'retx=(\d+)')
RE_SWEEPS = re.compile(r'sweeps=(\d+)')

CELLS = ['c7', 'c8', 'sc2', 'sc3', 'c1']
ARMS = ['def', 'uni']


def fmt(vals, prec=1):
    if not vals:
        return 'n=0'
    m = mean(vals)
    s = stdev(vals) if len(vals) > 1 else 0.0
    return '%.*f +/- %.*f (%d)' % (prec, m, prec, s, len(vals))


def parse(path):
    runs = {}
    health = {'inv': 0, 'summ': 0, 'retry': 0, 'lost': 0, 'flag': 0}
    cur = None
    with open(path, errors='replace') as fh:
        for line in fh:
            m = RE_HDR.match(line)
            if m:
                cur = {'arm': m.group(2), 'rep': int(m.group(1))}
                health['inv'] += 1
                continue
            if line.startswith('RUN-RETRY'):
                health['retry'] += 1
                continue
            if line.startswith('RUN-LOST'):
                health['lost'] += 1
                continue
            if line.startswith('ARM-LIVENESS-FAIL') or line.startswith('ARM-CONTAMINATION'):
                health['flag'] += 1
                continue
            if cur is None:
                continue
            m = RE_MBPS.search(line)
            if m and '"summary":true' in line:
                cur['mbps'] = float(m.group(1))
                d = RE_DNF.search(line)
                cur['dnf'] = int(d.group(1)) if d else -1
                health['summ'] += 1
                runs.setdefault(cur['arm'], []).append(cur)
                continue
            m = RE_CPU.search(line)
            if m:
                cur['cpusrv'] = float(m.group(1))
                cur['cpucli'] = float(m.group(2))
                continue
            m = RE_SF.match(line)
            if m:
                t, lv, ac, sh, ze = (int(x) for x in m.groups())
                cur['sf_ticks'] = t
                cur['sf_E'] = ac / lv if lv else float('nan')
                cur['sf_short'] = sh / t if t else float('nan')
                cur['sf_zero'] = ze / t if t else float('nan')
                continue
            if line.startswith('MECH '):
                m = RE_WIN.search(line)
                if m:
                    cur['occ'] = int(m.group(1))
                    cur['cap'] = int(m.group(2))
                m = RE_RETX.search(line)
                if m:
                    cur['retx'] = int(m.group(1))
                m = RE_SWEEPS.search(line)
                if m:
                    cur['sweeps'] = int(m.group(1))
                continue
    return runs, health


def col(runs, arm, key):
    return [r[key] for r in runs.get(arm, []) if key in r]


def main():
    for path in sys.argv[1:]:
        runs, health = parse(path)
        print('\n' + '=' * 78)
        print('FILE %s' % path)
        print('health: invocations=%(inv)d summaries=%(summ)d RUN-RETRY=%(retry)d '
              'RUN-LOST=%(lost)d liveness/contamination flags=%(flag)d' % health)
        dnfs = [r.get('dnf', -1) for a in runs for r in runs[a]]
        print('dnf: max=%s over %d completed runs' % (max(dnfs) if dnfs else 'n/a', len(dnfs)))

        print('\n--- GOODPUT (Mbit/s), mean +/- sample sigma (n), per arm')
        print('%-6s %-26s %-26s %10s' % ('cell', 'def', 'uni', 'delta'))
        means = {}
        for c in CELLS:
            d = col(runs, c + '-def', 'mbps')
            u = col(runs, c + '-uni', 'mbps')
            means[c] = (mean(d) if d else None, mean(u) if u else None)
            delta = ('%+.1f%%' % ((mean(u) / mean(d) - 1) * 100) if d and u else '-')
            print('%-6s %-26s %-26s %10s' % (c, fmt(d), fmt(u), delta))

        print('\n--- SAME-SESSION Sigma (Sigma_c7 = 2*sc2, Sigma_c8 = sc2 + sc3), per arm')
        for arm in ARMS:
            sc2 = col(runs, 'sc2-' + arm, 'mbps')
            sc3 = col(runs, 'sc3-' + arm, 'mbps')
            c7 = col(runs, 'c7-' + arm, 'mbps')
            c8 = col(runs, 'c8-' + arm, 'mbps')
            if sc2 and c7:
                print('  %-4s c7 %.2f  Sigma %.2f  -> %.3f x Sigma'
                      % (arm, mean(c7), 2 * mean(sc2), mean(c7) / (2 * mean(sc2))))
            if sc2 and sc3 and c8:
                print('  %-4s c8 %.2f  Sigma %.2f  -> %.3f x Sigma'
                      % (arm, mean(c8), mean(sc2) + mean(sc3),
                         mean(c8) / (mean(sc2) + mean(sc3))))

        print('\n--- THE MECHANISM: sf= population and the effective store cap')
        print('%-9s %8s %8s %8s %14s %12s' %
              ('arm', 'E', 'short%', 'zero%', 'cap (win=x/CAP)', 'occupancy'))
        for c in CELLS:
            for arm in ARMS:
                a = '%s-%s' % (c, arm)
                E = col(runs, a, 'sf_E')
                sh = col(runs, a, 'sf_short')
                ze = col(runs, a, 'sf_zero')
                cap = col(runs, a, 'cap')
                occ = col(runs, a, 'occ')
                if not E and not cap:
                    continue
                print('%-9s %8s %8s %8s %14s %12s' % (
                    a,
                    '%.3f' % mean(E) if E else '-',
                    '%.1f' % (100 * mean(sh)) if sh else '-',
                    '%.1f' % (100 * mean(ze)) if ze else '-',
                    fmt(cap, 0) if cap else '-',
                    fmt(occ, 0) if occ else '-'))

        print('\n--- P5 no-regression gauges (end-of-run totals)')
        print('%-9s %22s %18s %18s' % ('arm', 'retx', 'sweeps', 'CPUSRV (s)'))
        for c in CELLS:
            for arm in ARMS:
                a = '%s-%s' % (c, arm)
                if a not in runs:
                    continue
                print('%-9s %22s %18s %18s' % (
                    a, fmt(col(runs, a, 'retx'), 0),
                    fmt(col(runs, a, 'sweeps'), 0),
                    fmt(col(runs, a, 'cpusrv'), 2)))

        print('\n--- per-run values')
        for c in CELLS:
            for arm in ARMS:
                a = '%s-%s' % (c, arm)
                v = col(runs, a, 'mbps')
                if v:
                    print('  %-9s %s' % (a, ' '.join('%.2f' % x for x in v)))


if __name__ == '__main__':
    main()
