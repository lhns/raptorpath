#!/usr/bin/env python3
"""Status §33 offline reading: how much of a row's [LAG] E = p99 - min is the
tokio timer quantization floor, and what lies above it.

    lagfloor_read.py <row-log> ...   # client or server -c.log / -s.log files

Per row, over the [LAGRAW] samples of the xfer window, x = late - min:
  p50/p90/p95/p99 of x; share of ticks with x >= 1000 / 1500 / 2000 us.
  pre_*: the same on the samples BEFORE xfer (server: an idle process, the
         quantization floor with no engine work), when >= 50 exist.
  With [OSLAGRAW] in the file: OS stalls = OS-clock ticks with excess >= 300
  us, each spanning [at - excess, at]; big = [LAG] ticks with x >= 1500, each
  spanning [at - x, at]; big_os = those overlapping an OS stall by >= 300 us.
Read-only on committed ledgers; ships nothing.
"""
import re
import sys


def parse(path):
    out = {}
    for line in open(path, errors='replace'):
        for tag in ('[LAGRAW]', '[OSLAGRAW]'):
            if line.startswith(tag):
                m = re.search(r'xfer=(\d+)-(\d+)', line)
                s = re.search(r' s=(\S*)', line)[1]
                vals = [tuple(int(v) for v in x.split(':')) for x in s.split(',') if x]
                out[tag] = ((int(m[1]), int(m[2])) if m else None, vals)
    return out


def q(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(p * len(xs)))] if xs else None


def overlaps(span, stalls, need=300):
    return any(min(span[1], o1) - max(span[0], o0) >= need for o0, o1 in stalls)


def row(path):
    d = parse(path)
    if '[LAGRAW]' not in d or d['[LAGRAW]'][0] is None:
        return None
    (a, b), vals = d['[LAGRAW]']
    xs = [(t, l) for t, l in vals if a <= t <= b]
    mn = min(l for _, l in xs)
    ex = [l - mn for _, l in xs]
    r = {'n': len(ex), 'min': mn}
    for p in (.5, .9, .95, .99):
        r[f'p{int(p * 100)}'] = q(ex, p)
    for k in (1000, 1500, 2000):
        r[f'ge{k}'] = round(sum(e >= k for e in ex) / len(ex), 4)
    pre = [l for t, l in vals if t < a]
    if len(pre) >= 50:
        pm = min(pre)
        r['pre_n'] = len(pre)
        r['pre_p50'] = q([l - pm for l in pre], .5)
        r['pre_p95'] = q([l - pm for l in pre], .95)
    if '[OSLAGRAW]' in d:
        oxs = [(t, l) for t, l in d['[OSLAGRAW]'][1] if a <= t <= b]
        omn = min(l for _, l in oxs)
        stalls = [(t - (l - omn), t) for t, l in oxs if l - omn >= 300]
        big = [(t - e, t) for (t, _), e in zip(xs, ex) if e >= 1500]
        r['os_p99'] = q([l - omn for _, l in oxs], .99)
        r['big'] = len(big)
        r['big_os'] = sum(overlaps(s, stalls) for s in big)
    return r


def main(paths):
    rows = []
    for p in paths:
        r = row(p)
        if r:
            rows.append(r)
            name = p.replace('\\', '/').split('/')[-1]
            print(name, ' '.join(f'{k}={v}' for k, v in r.items()))
    if not rows:
        return
    med = lambda k: q([r[k] for r in rows if k in r], .5)
    print('MEDIANS rows=%d p95=%s p99=%s ge1500=%s ge2000=%s pre_p50=%s pre_p95=%s'
          % (len(rows), med('p95'), med('p99'), med('ge1500'), med('ge2000'), med('pre_p50'), med('pre_p95')))
    big = sum(r.get('big', 0) for r in rows)
    if big:
        print('POOLED big=%d big_os=%d share_os=%.2f' % (big, sum(r.get('big_os', 0) for r in rows),
                                                         sum(r.get('big_os', 0) for r in rows) / big))


if __name__ == '__main__':
    main(sys.argv[1:])
