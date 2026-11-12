"""Lock-out onset and start-up spike per two-leg row (status §38 amendment 2).

For every two-leg sender log (`*-c.log` or `*-c.log.gz`) given:
  - store arm: the `RWM_STORE=<n>` value on `[GATES]` ("def" when empty);
  - LOCKED: a leg with `[TRUTH]` egress share < 10 % and median
    post-warm-up (t >= 1 s) `[DIAG]` `rtt` >= 3 x median `wrtt` (§38);
  - onset: for a LOCKED leg, the first `[DIAG]` time from which its `infl`
    stays 0 and its `rtt` stays >= 3 x `wrtt` up to the last in-transfer
    line (the last line where any leg has `infl` > 0);
  - spike: the largest per-leg `rtt` (ms) on `[DIAG]` lines with t <= 1.0 s,
    and the same leg's `wrtt` there.

Usage: python3 place38_onset.py <glob> [<glob> ...]
"""
import glob
import gzip
import re
import statistics as st
import sys

KV = re.compile(r'(\w+)=([-\w.]+)')
LEG = re.compile(r' p(\d):infl=(\d+)/.*?rtt=(\d+)/wrtt=(\d+)/')


def lines(path):
    op = gzip.open if path.endswith('.gz') else open
    with op(path, 'rt', errors='replace') as f:
        yield from f


def row(clog):
    base = re.sub(r'-c\.log(\.gz)?$', '', clog)
    truth = {}
    for f in glob.glob(base + '-*'):
        if re.search(r'-c\.log', f):
            continue
        for line in lines(f):
            if '[TRUTH] leg=' in line:
                d = dict(KV.findall(line))
                truth[int(d['leg'])] = int(d['egress_dgrams'])
    store, diag = None, []
    for line in lines(clog):
        if '[TRUTH] leg=' in line:
            d = dict(KV.findall(line))
            truth[int(d['leg'])] = int(d['egress_dgrams'])
        if store is None:
            m = re.search(r'RWM_STORE=(\d*)', line)
            if m:
                store = m.group(1) or 'def'
        if '[DIAG] t=' in line and 'p0:infl=' in line:
            t = float(re.search(r'\[DIAG\] t=([\d.]+)s', line).group(1))
            legs = {int(m.group(1)): (int(m.group(2)), int(m.group(3)), int(m.group(4)))
                    for m in LEG.finditer(line)}
            if len(legs) >= 2:
                diag.append((t, legs))
    if len(truth) < 2 or not diag:
        return None
    last_live = max((i for i, (_, L) in enumerate(diag) if any(v[0] > 0 for v in L.values())), default=len(diag) - 1)
    live = diag[:last_live + 1]
    tot = sum(truth.values()) or 1
    out = {'store': store, 'locked': [], 'onset': None}
    for p in sorted(diag[0][1]):
        post = [L[p] for t, L in diag if t >= 1.0 and p in L] or [L[p] for _, L in diag if p in L]
        r = st.median(x[1] for x in post)
        w = st.median(x[2] for x in post)
        if truth.get(p, 0) / tot < 0.10 and r >= 3 * max(w, 1):
            out['locked'].append(p)
            onset = None
            for i in range(len(live)):
                if all(L[p][0] == 0 and L[p][1] >= 3 * max(L[p][2], 1) for _, L in live[i:]):
                    onset = live[i][0]
                    break
            out['onset'] = onset
    early = [(L[p][1], L[p][2], p, t) for t, L in diag if t <= 1.0 for p in L]
    out['spike'] = max(early) if early else None
    return out


if __name__ == '__main__':
    for pat in sys.argv[1:]:
        for f in sorted(glob.glob(pat)):
            r = row(f)
            if r is None:
                continue
            name = re.sub(r'-a1-c\.log(\.gz)?$', '', f.split('/')[-1])
            sp = r['spike']
            print('%-34s store=%-5s %-8s onset=%-5s spike rtt=%d wrtt=%d (p%d t=%.1f)' % (
                name, r['store'], 'LOCK' + ''.join(map(str, r['locked'])) if r['locked'] else '-',
                r['onset'], sp[0], sp[1], sp[2], sp[3]))
