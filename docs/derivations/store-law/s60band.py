"""s60 registration: the frozen crossing BANDS for the replayed R~ (one b0 and one l-hat per cell and leg, taken on
the cell's calibration arm; s60-registration.md section 1), by propagating each fixed input of the gauge's
known-answer test (slaw.rs known_answer_r_on_the_s56_maps) over its measured range. Bulk only: q = 0.10.

Usage: python -I s60band.py      (no input: every number below is copied from a committed .out or status line, cited)

R~ crosses 1 where, on the weight-carrying leg 0,
    T_min(W)  =  F + sig + q*l_c + gap + shift1
    F = l_c + b0        (anc = 0: m-hat <= l_c + b0; the KA premise m-hat = l)
      = l_c + 2*b0      (anc = 1: m-hat >= l_c + b0; on 600 MB rows m-hat tracks the standing queue, s60boot.out lm)
l_c and b0 are the calibration arm's (the law boots at n0 = 128, at or below the BDP); shift1 = (w1/w0)*D1 is leg
1's pull at c8 (zero in the KA test, where leg 1 had weight 1e-9).
Two readings of T_min(W), both carried into the band:
  A  the s56 central curve (rep56-score.txt 'STANDING QUEUE AT THE KNEE': arm-median T-bar - T0, integer-ms ticks),
     which the KA test used as T_min(W) - (l + b0_true): target (b0 - b0_true) + sig + q*l + gap [+ b0] + shift1.
     Corners: the curve +-0.5 ms (integer-ms resolution) at c2 / c8; at c1s also R6.1's 0.1 ms curve.
  B  the window minima of the p27 / p34 static-cap ledgers (s60p27.out, s60p34.out: 'late Tmin-l' and 'l'), read
     as T_min(W) = l_row(W) + (T_min - l)(W) against the absolute target, with l_c the calibration caps' l ('sub')
     and, as a corner, the row's own l ('row': the gauge's printed R uses the row's l). At c1s these minima are of
     a PREFIX of each window (ov > 0 on 88 % of p34's c1s windows, s60 doc section 1), so they over-read T_min and
     B's c1s corners are a lower edge for the minima reading.
The band is the min..max over methods, anc states and the corners of each input's range.
"""
import itertools

Q = 0.10
CELLS = {
    'c2': dict(
        A=[[(128, 0.0), (160, 0.0), (192, 3.0), (256, 9.0), (384, 22.0), (2048, 192.0)]],
        Ares=0.5,
        # (W, T_min - l, l) per p27 cap, s60p27.out
        B={'p27': [(64, 2.59, 6.86), (96, 2.17, 7.92), (128, 2.41, 8.77), (192, 3.76, 10.18), (256, 8.96, 10.07),
                   (384, 21.47, 10.75), (512, 33.76, 11.44), (1024, 84.60, 11.17)]},
        lc=(8.07, 8.77),                 # l on the calibration arm W 128: s60boot.out P128 / s60p27.out cap 128
        lA=(8.07, 10.0),                 # s60boot.out c2 P128 l / rep56 'ell 10.0' (q*l in method A)
        sig=(1.72, 1.95),                # s60boot.out sig, c2 arms
        b0=(2.18, 2.87),                 # s60p27.out b0p medians at caps 64-128
        over=(0.41, 0.90),               # b0bound.out p27 c2 max8 cost median / max (the KA's B0X)
        gap=(0.0,), shift1=(0.0,), ka=194.2),
    'c8': dict(
        A=[[(160, 0.0), (224, 3.9), (320, 9.1), (512, 21.9), (768, 41.7), (1280, 84.2)]],
        Ares=0.5,
        B={'p27': [(128, 2.43, 8.10), (192, 3.25, 9.33), (256, 5.80, 11.80), (384, 15.58, 9.96), (512, 23.60, 10.88),
                   (768, 39.31, 11.23)]},
        lc=(8.10, 8.62),                 # p27 leg-0 l at caps 128 / the two-cap median (s60p27.out)
        lA=(8.22, 14.4),                 # s60boot.out c8 P80 leg-0 l / rep56 'ell 14.4'
        sig=(2.00, 2.67),                # s60boot.out sig, c8 leg 0
        b0=(2.49, 3.43),                 # s60p27.out c8 cap-128 b0p median / the two-cap median
        over=(1.0, 1.8),                 # b0bound.out p27 c8 p0 max8 / max_row cost medians
        # 1/2*((l1 + b0_1) - (l0 + b0_0)) on calibration values: l1 - l0 = 23.4..23.9 (s60boot P80; s60p27 caps
        # 128-192), b0_1 - b0_0 = 3.6..5.4 (s60p27 leg-1 b0p 6.12..7.87 against leg 0 2.49): 13.5..14.7; KA 14.75
        gap=(13.5, 14.8),
        # leg 1's pull: w1/w0 0.11..0.18 (s56 r1/r0 at W 320-512) x D1 = b0_1 + sig1 + q*l1 + min(m1-l1, b0_1) - x1
        # ~ 13..18 ms (s60boot sig1 4.9..15.5, m1-l1 1.1..2.0, x1 0..12.6): 1.4..3.2 ms
        shift1=(1.4, 3.2), ka=478.2),
    'c1s': dict(
        A=[[(384, 0.0), (512, 1.0), (768, 3.0), (1024, 5.5), (2048, 15.0)],
           [(384, 0.61), (512, 1.16), (768, 3.18), (1024, 5.29), (2048, 14.19)]],   # R6.1's 0.1 ms curve
        Ares=0.0,
        B={'p34': [(256, 0.13, 2.50), (384, 0.53, 2.55), (512, 0.98, 2.65), (768, 0.49, 2.82), (1024, 1.91, 2.62),
                   (1536, 2.94, 2.59), (2048, 4.81, 2.85)],
           'p27': [(256, 0.24, 2.38), (384, 0.57, 2.44), (512, 0.71, 2.53), (768, 1.78, 2.57), (1024, 2.19, 2.44),
                   (1536, 4.69, 2.52), (2048, 5.53, 2.56)]},
        lc=(2.38, 2.50),                 # l at cap 256 (s60p27.out / s60p34.out)
        lA=(2.43, 2.6),                  # s60boot.out c1s l / rep56 'ell 2.6'
        sig=(0.46, 0.67),                # s60boot.out sig, c1s
        b0=(0.29, 0.40),                 # b0p at cap 256 (s60p34.out 0.29; p34 two-cap median 0.40)
        over=(0.12, 0.20),               # b0bound.out p34 c1s max8 / max_row cost medians
        gap=(0.0,), shift1=(0.0,), ka=495.4),
}


def first_w(curve, target):
    """First W at which the piecewise-linear curve [(W, y)] reaches target; inf if never."""
    if target <= curve[0][1]:
        return float(curve[0][0])
    for (w1, q1), (w2, q2) in zip(curve, curve[1:]):
        if q1 < target <= q2:
            return w1 + (target - q1) * (w2 - w1) / (q2 - q1)
    return float('inf')


print('cell | method corner anc | W range over corners | KA point (fixed inputs)')
for name, c in CELLS.items():
    allw = []
    rows = []
    for ai, curve in enumerate(c['A']):
        for res in sorted({-c['Ares'], 0.0, c['Ares']}):
            cv = [(w, y + res) for w, y in curve]
            for anc in (0, 1):
                ws = [first_w(cv, over + sig + Q * l + gap + s1 + (b0 if anc else 0.0))
                      for sig, b0, over, gap, s1, l in itertools.product(c['sig'], c['b0'], c['over'], c['gap'],
                                                                         c['shift1'], c['lA'])]
                rows.append(('A%s res%+.1f' % ('' if ai == 0 else '0.1ms', res), anc, ws))
    for led, pts in c['B'].items():
        for lmode in ('sub', 'row'):
            for anc in (0, 1):
                ws = []
                for sig, b0, gap, s1, lc in itertools.product(c['sig'], c['b0'], c['gap'], c['shift1'], c['lc']):
                    if lmode == 'sub':
                        cv = [(w, l + x) for w, x, l in pts]
                        t = lc + (2 if anc else 1) * b0 + sig + Q * lc + gap + s1
                    else:
                        cv = [(w, x) for w, x, l in pts]
                        t = (2 if anc else 1) * b0 + sig + Q * lc + gap + s1
                    ws.append(first_w(cv, t))
                rows.append(('B %s l=%s' % (led, lmode), anc, ws))
    for lab, anc, ws in rows:
        allw += ws
        print('%s | %s anc=%d | %.0f-%.0f | %.1f' % (name, lab, anc, min(ws), max(ws), c['ka']))
    print('%s | BAND | %.0f-%.0f |' % (name, min(allw), max(allw)))
