"""Per-row medians of client [DIAG] fields over the steady window (t >= 2 s, src > 0)."""
import re, sys, glob, os, statistics as S
d = sys.argv[1]; cell = sys.argv[2] if len(sys.argv) > 2 else 'c8-100'
PP = r' p%d:infl=(\d+)/sinfl=(\d+)/bdp(\d+)\S* sout=(\d+) khr=\S+ btlbw=(\d+).*?rtt=(\d+)/wrtt=(\d+)/rtp\d+ms rtp_us=(\d+).*?qcwnd=(\d+)'
def num(p, L, k=1):
    m = re.search(p, L); return float(m.group(k)) if m else None
cols = ['win', 'cap', 'head', 'hole', 'srelc', 'q0', 'q1', 'rtt0', 'wrtt0', 'rtp0', 'qcw0', 'qcw1', 'bdp0', 'bdp1', 'bw0', 'bw1', 'rtt1', 'wrtt1', 'rtp1', 'paused', 'sout0', 'sout1']
print('row', *cols, 'h0rate', 'h1rate', 'tx0rate', 'share0', sep='\t')
arms = {}
for f in sorted(glob.glob(os.path.join(d, cell + '@def-*-c.log'))):
    name = os.path.basename(f)[len(cell) + 5:-6]
    rows = []; cum = []
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'): continue
        t = num(r't=([\d.]+)s', L); src = num(r' src=(\d+)', L)
        d0 = re.search(r'dgq0\[hand=(\d+) tx=(\d+) full=\d+ err=\d+ sp=(\d+)', L)
        d1 = re.search(r'dgq1\[hand=(\d+) tx=(\d+) full=\d+ err=\d+ sp=(\d+)', L)
        p0 = re.search(PP % 0, L); p1 = re.search(PP % 1, L)
        if not (d0 and d1 and p0 and p1) or t < 2 or not src: continue
        w = re.search(r'win=(\d+)/(\d+)', L); w2 = re.search(r'wnd2=(\d+)/(\d+)', L); sr = re.search(r'srel=(\d+)/', L)
        r = dict(win=int(w.group(1)), cap=int(w.group(2)), head=int(w2.group(1)), hole=int(w2.group(2)),
                 srelc=int(sr.group(1)) if sr else 0,
                 q0=int(d0.group(1)) - int(d0.group(2)), q1=int(d1.group(1)) - int(d1.group(2)),
                 rtt0=int(p0.group(6)), wrtt0=int(p0.group(7)), rtp0=int(p0.group(8)) / 1000,
                 qcw0=int(p0.group(9)) / 1233, qcw1=int(p1.group(9)) / 1233, bdp0=int(p0.group(3)), bdp1=int(p1.group(3)),
                 bw0=int(p0.group(5)), bw1=int(p1.group(5)), rtt1=int(p1.group(6)), wrtt1=int(p1.group(7)), rtp1=int(p1.group(8)) / 1000,
                 paused=num(r'paused=(\d+)%', L), sout0=int(p0.group(4)), sout1=int(p1.group(4)))
        rows.append(r); cum.append((t, int(d0.group(1)), int(d1.group(1)), int(d0.group(2))))
    if len(cum) < 4: continue
    (ta, a0, a1, x0), (tb, b0, b1, y0) = cum[0], cum[-1]
    dt = tb - ta
    med = [S.median(r[c] for r in rows) for c in cols]
    extra = [(b0 - a0) / dt, (b1 - a1) / dt, (y0 - x0) / dt, (b0 - a0) / ((b0 - a0) + (b1 - a1))]
    arm = name.split('-')[0]
    arms.setdefault(arm, []).append(med + extra)
    print(name, *['%.0f' % v if abs(v) >= 10 else '%.2f' % v for v in med + extra], sep='\t')
print()
print('arm(range of row medians)')
for arm, M in arms.items():
    print(arm, *['%s..%s' % (('%.0f' % min(c)) if abs(min(c)) >= 10 else '%.2f' % min(c), ('%.0f' % max(c)) if abs(max(c)) >= 10 else '%.2f' % max(c)) for c in zip(*M)], sep='\t')
print()
print('arm(median of row medians)')
for arm, M in arms.items():
    print(arm, *[('%.0f' % S.median(c)) if abs(S.median(c)) >= 10 else '%.2f' % S.median(c) for c in zip(*M)], sep='\t')
