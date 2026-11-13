"""Measured in-order delay per row: the horizontal distance between the cumulative source-sent curve
(`cum=src/..`) and the cumulative-ack curve (`cum=../../ack`) on the client [DIAG] lines (250 ms ticks).
delay(t) = t - t_src(ack(t)), with t_src interpolated linearly. Reported for the PIN phase (cap == 4096,
src > 0) and for the whole run (every tick with ack > 0). Also the first tick where src - ack > 16 384
(the [LAT] pending bound) and the [LAT] coverage."""
import re, sys, glob, os, bisect, statistics as S
d = sys.argv[1]; out = {}
def q(v, p):
    v = sorted(v); return v[min(len(v) - 1, int(p * (len(v) - 1) + 0.5))]
print('row\tpin_mean\tpin_p50\tpin_p95\trun_mean\trun_p50\trun_p95\tt_over16k\tlat_cov')
for f in sorted(glob.glob(os.path.join(d, 'c8-100@def-*-c.log'))):
    name = os.path.basename(f)[11:-6]; T = []; SRC = []; ACK = []; PIN = []
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'): continue
        m = re.search(r' cum=(\d+)/\d+/(\d+)', L); w = re.search(r'win=\d+/(\d+)', L)
        if not m: continue
        t = float(re.search(r't=([\d.]+)s', L).group(1)); src = int(re.search(r' src=(\d+)', L).group(1))
        T.append(t); SRC.append(int(m.group(1))); ACK.append(int(m.group(2)))
        PIN.append(bool(w) and int(w.group(1)) >= 4096 and src > 0)
    def t_src(a):
        i = bisect.bisect_left(SRC, a)
        if i == 0: return T[0]
        if i >= len(SRC): return None
        s0, s1 = SRC[i - 1], SRC[i]
        return T[i - 1] + (T[i] - T[i - 1]) * ((a - s0) / (s1 - s0) if s1 > s0 else 0)
    dl = []
    for t, a, p in zip(T, ACK, PIN):
        if a <= 0: continue
        ts = t_src(a)
        if ts is not None: dl.append((max(0.0, t - ts), p))
    pin = [x for x, p in dl if p]; run = [x for x, _ in dl]
    over = next((t for t, s, a in zip(T, SRC, ACK) if s - a > 16384), None)
    la = [L for L in open(f[:-6] + '-s.log', errors='replace') if L.startswith('[LAT]')][-1]
    n = int(re.search(r' n=(\d+)', la).group(1)); pn = sum(int(x) for x in re.findall(r' p\d:n=(\d+)', la))
    v = [S.mean(pin), q(pin, .5), q(pin, .95), S.mean(run), q(run, .5), q(run, .95), over if over is not None else -1, pn / n]
    out.setdefault(name.split('-')[0], []).append(v)
    print(name, *['%.2f' % x for x in v], sep='\t')
print()
for a, M in out.items():
    print(a, *['%.2f [%.2f..%.2f]' % (S.median(c), min(c), max(c)) for c in zip(*M)], sep='\t')
