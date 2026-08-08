#!/usr/bin/env python3
"""feat/ack-merge-flip battery collector (goal-gate "Ack-Merge Flip").

Parses `battery-s<seed>.log` into, per arm:
  - goodput mean +/- SAMPLE sigma with n (never a mean without its n --
    MEASUREMENT DISCIPLINE item 8), plus the per-run values
  - RECEIVER CPU (CPUSRV) mean, and CPU per gigabit -- THE second half of
    the pre-registered clause (the mechanism removes receiver-side
    per-message work, so a real win must show here)
  - the same-session Sigma ratios for the dual cells, Sigma taken from
    THAT ARM's OWN singles
  - the [CTLD] control-datagram density: tx/rx = control datagrams per data
    message (1.000 merged vs 1.038/1.053 default) -- the mechanism check
  - run health: invocations / summaries / RUN-RETRY / RUN-LOST / liveness
    flags

  usage: ackflip_parse.py <battery-s42.log> [battery-s7.log ...]
"""
import re
import sys
from statistics import mean, stdev

RE_HDR = re.compile(r'^=== rep=(\d+) arm=(\S+) attempt=(\d+) ')
RE_MBPS = re.compile(r'"mean_mbps":\s*([0-9.]+)')
RE_DNF = re.compile(r'"dnf":\s*(\d+)')
RE_CPU = re.compile(r'CPUSRV=([0-9.]+)s CPUCLI=([0-9.]+)s')
RE_CTLD = re.compile(r'\[CTLD\] p(\d+) tx=(\d+) rx=(\d+)')

# bytes per cell, for CPU/bit
BYTES = {
    'c1': 400e6, 'c7': 200e6, 'c8': 25e6, 'sc2': 100e6, 'sc3': 25e6,
    'c1-am-1200M': 1200e6, 'c1-prior-1200M': 1200e6,
}


def cell_of(arm):
    if arm.endswith('-1200M'):
        return arm
    return arm.rsplit('-', 1)[0]


def fmt(vals):
    if not vals:
        return 'n=0'
    m = mean(vals)
    s = stdev(vals) if len(vals) > 1 else 0.0
    return '%.1f +/- %.1f (%d)' % (m, s, len(vals))


def parse(path):
    runs = {}          # arm -> list of dicts
    health = {'inv': 0, 'summ': 0, 'retry': 0, 'lost': 0, 'live': 0}
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
                cur = None
                continue
            if line.startswith('RUN-LOST'):
                health['lost'] += 1
                cur = None
                continue
            if line.startswith('ARM-LIVENESS-FAIL') or line.startswith('ARM-CONTAMINATION'):
                health['live'] += 1
                continue
            if cur is None:
                continue
            m = RE_MBPS.search(line)
            if m:
                cur['mbps'] = float(m.group(1))
                health['summ'] += 1
                runs.setdefault(cur['arm'], []).append(cur)
            m = RE_CPU.search(line)
            if m:
                cur['cpusrv'] = float(m.group(1))
                cur['cpucli'] = float(m.group(2))
            m = RE_DNF.search(line)
            if m:
                cur['dnf'] = max(cur.get('dnf', 0), int(m.group(1)))
            for pid, tx, rx in RE_CTLD.findall(line):
                cur.setdefault('ctld', {})[pid] = (int(tx), int(rx))
    return runs, health


def report(path):
    runs, health = parse(path)
    print('=' * 78)
    print(path)
    print('RUN HEALTH: invocations=%(inv)d summaries=%(summ)d RUN-RETRY=%(retry)d '
          'RUN-LOST=%(lost)d liveness-flags=%(live)d' % health)
    dnfs = [r.get('dnf', 0) for rs in runs.values() for r in rs]
    print('dnf: max=%d  nonzero=%d/%d' % (max(dnfs or [0]),
                                          sum(1 for d in dnfs if d), len(dnfs)))
    print()

    # per-arm goodput + receiver CPU
    order = ['c1-am', 'c1-prior', 'c7-am', 'c7-prior', 'c8-am', 'c8-prior',
             'sc2-am', 'sc2-prior', 'sc3-am', 'sc3-prior',
             'c1-am-1200M', 'c1-prior-1200M']
    print('%-16s %-24s %-22s %-12s' % ('arm', 'goodput Mbit/s', 'CPUSRV s', 'CPUSRV/Gbit'))
    means = {}
    for arm in order + [a for a in sorted(runs) if a not in order]:
        rs = runs.get(arm)
        if not rs:
            continue
        g = [r['mbps'] for r in rs if 'mbps' in r]
        c = [r['cpusrv'] for r in rs if 'cpusrv' in r]
        means[arm] = mean(g) if g else None
        nbits = BYTES.get(cell_of(arm), 0) * 8 / 1e9
        perg = (mean(c) / nbits) if (c and nbits) else float('nan')
        print('%-16s %-24s %-22s %-12.2f' % (arm, fmt(g), fmt(c), perg))
        print('%-16s   runs: %s' % ('', ' '.join('%.1f' % v for v in g)))

    # deltas: candidate vs its OWN same-session control
    print()
    print('%-10s %10s %10s %10s %8s' % ('cell', 'am', 'prior', 'delta', 'delta%'))
    for cell in ['c1', 'c7', 'c8', 'sc2', 'sc3']:
        a, p = means.get(cell + '-am'), means.get(cell + '-prior')
        if a is None or p is None:
            continue
        print('%-10s %10.1f %10.1f %+10.1f %+7.1f%%'
              % (cell, a, p, a - p, 100.0 * (a - p) / p))
    a, p = means.get('c1-am-1200M'), means.get('c1-prior-1200M')
    if a and p:
        print('%-10s %10.1f %10.1f %+10.1f %+7.1f%%'
              % ('c1-1.2GB', a, p, a - p, 100.0 * (a - p) / p))

    # same-session Sigma, from each arm's OWN singles
    print()
    for suffix in ['am', 'prior']:
        sc2, sc3 = means.get('sc2-' + suffix), means.get('sc3-' + suffix)
        c7, c8 = means.get('c7-' + suffix), means.get('c8-' + suffix)
        if sc2 and c7:
            print('c7-%-6s = %.3f x Sigma  (Sigma = 2 x sc2 = %.1f)'
                  % (suffix, c7 / (2 * sc2), 2 * sc2))
        if sc2 and sc3 and c8:
            print('c8-%-6s = %.3f x Sigma  (Sigma = sc2 + sc3 = %.1f)'
                  % (suffix, c8 / (sc2 + sc3), sc2 + sc3))

    # the mechanism check
    print()
    print('CTLD control datagrams per data message (receiver, last sample):')
    for arm in order:
        rs = runs.get(arm)
        if not rs:
            continue
        dens = []
        for r in rs:
            for tx, rx in r.get('ctld', {}).values():
                if rx > 1000:
                    dens.append(tx / rx)
        if dens:
            print('  %-16s %s  (n=%d paths)  [%.3f .. %.3f]'
                  % (arm, '%.3f' % mean(dens), len(dens), min(dens), max(dens)))
        else:
            print('  %-16s NO CTLD DATA' % arm)


if __name__ == '__main__':
    for p in sys.argv[1:]:
        report(p)
