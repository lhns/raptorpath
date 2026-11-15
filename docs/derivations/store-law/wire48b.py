"""The wire-clock anchor candidates on the status-48b ledger (bbr48b/c2c8-diag-qcc.tgz, extracted).

Usage: python -I wire48b.py <extracted dir>

Per row, per leg, LATE window (t >= 15 s) medians and the boot value (first line) of:
  qrt_us  = bbr_rs m-hat (its windowed min of the ack-clock loop r_e, ProbeRTT-refreshed)  [CAND rows only]
  qmin_us = quinn RttEstimator::min() (lifetime min of the wire RTT)
  rtp_us  = Copa's 10 s app-echo min (the shipped store anchor, from [DIAG])
  wrtt    = quinn smoothed wire RTT (from [DIAG]): the wire clock including the network queue
BASE = quinn's BBR, CAND = bbr_rs (status 48b). Prints arm medians of per-row values.
"""
import glob, os, re, statistics as St, sys
from collections import defaultdict

D = sys.argv[1]
out = defaultdict(lambda: defaultdict(list))
for f in sorted(glob.glob(os.path.join(D, '**', '*-c.log'), recursive=True)):
    m = re.match(r'(c\w+-\d+)@def-(\w+)-s', os.path.basename(f))
    if not m:
        continue
    key = m.groups()
    q = defaultdict(list); boot = {}
    for L in open(f, errors='replace'):
        t = re.search(r't=([\d.]+)s', L)
        if not t:
            continue
        t = float(t.group(1))
        if L.startswith('[QCC]'):
            for p, rt, mn in re.findall(r'p(\d):qm=\S+ qrt_us=(\d+) .*?qmin_us=(\d+)', L):
                for k, v in (('qrt', rt), ('qmin', mn)):
                    boot.setdefault((k, p), int(v) / 1e3)
                    if t >= 15:
                        q[(k, p)].append(int(v) / 1e3)
        elif L.startswith('[DIAG]'):
            for p in ('0', '1'):
                x = re.search(r' p%s:infl=.*?rtt=(\d+)/wrtt=(\d+)/rtp\d+ms rtp_us=(\d+)' % p, L)
                if x:
                    for k, v in (('wrtt', int(x.group(2))), ('rtp', int(x.group(3)) / 1e3)):
                        boot.setdefault((k, p), v)
                        if t >= 15:
                            q[(k, p)].append(v)
    for k, v in q.items():
        if v:
            out[key][k].append((St.median(v), boot[k]))
for key in sorted(out):
    print('==', *key, '  [late median (boot)] per leg, arm median of rows')
    for k in sorted(out[key]):
        v = out[key][k]
        print('  %-5s p%s  %.1f (%.1f)  rows %d' % (k[0], k[1], St.median(a for a, _ in v),
                                                    St.median(b for _, b in v), len(v)))
