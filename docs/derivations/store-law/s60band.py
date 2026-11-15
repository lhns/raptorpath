"""s60 registration: the frozen crossing BANDS for the replayed R~ (s60-registration.md sections 1.4 and 1.6), by
propagating each fixed input of the gauge's known-answer test (slaw.rs known_answer_r_on_the_s56_maps) over its
measured range. Bulk only: q = 0.10.

Usage: python -I s60band.py      (no input: every number below is copied from a committed .out, cited per line)

The frozen recipe: b0 (and, at c2 and c8, l-hat) per leg from the SAME replicate's calibration row at n0 = 128;
every other input the row's own. l-hat is substituted at c2 / c8 because it rises with W row by row (s60ell.out:
Spearman p 0.007 / 0.014 on s56, 0.001 / 0.046 on p27); at c1s it is flat (s56 p 0.43) and the row's own is kept.
Because one calibration ROW feeds each replicate, every b0 / l_c corner is a ROW range, not a median.

R~ crosses 1 where, on the weight-carrying leg 0,
    T_min(W)  =  l_c + (1 or 2)*b0 + sig + q*l_c + gap + shift1      (anc 0 / anc 1)
with l_c the calibration row's l ('sub') or the row's own ('own', the sensitivity at c2 / c8, the recipe at c1s).
Two readings of T_min(W):
  A  the s56 central curve (rep56-score.txt, arm-median T-bar - T0, integer-ms ticks), read as T_min(W) - (l + b0_true):
     target (b0 - b0_true) + sig + q*l + gap [+ b0] + shift1, the over-read b0 - b0_true over a single row's range
     (0 .. the b0bound.out max). Corners: the curve +-0.5 ms at c2 / c8; R6.1's 0.1 ms curve at c1s.
  B  the p27 / p34 window minima (s60p27.out, s60p34.out): T_min(W) = l_row(W) + (T_min - l)(W) for 'sub', or
     (T_min - l)(W) against a target without l for 'own'; 'sub' takes l_c from p27's own cap-128 rows, since
     absolute echo floors differ between ledgers. At c1s these are PREFIX minima (ov > 0 on 88 % of p34's
     c1s windows) and over-read T_min.
"""
import itertools

Q = 0.10
CELLS = {
    'c2': dict(
        A=[[(128, 0.0), (160, 0.0), (192, 3.0), (256, 9.0), (384, 22.0), (2048, 192.0)]], Ares=0.5,
        B={'p27': [(64, 2.59, 6.86), (96, 2.17, 7.92), (128, 2.41, 8.77), (192, 3.76, 10.18), (256, 8.96, 10.07),
                   (384, 21.47, 10.75), (512, 33.76, 11.44), (1024, 84.60, 11.17)]},
        lc=(6.30, 9.09),                 # l on single W 128 rows, s56: s60boot.out P128 6.30-9.09 (q*l in method A)
        lcB=(8.64, 8.90),                # the same, p27's own cap-128 rows (s60p27.out): method B compares p27 to p27
        lown=(8.07, 10.13),              # row's own l across the arms (s60boot.out medians P128..P2048)
        sig=(1.72, 1.95),                # s60boot.out sig, c2 arms
        b0=(1.27, 3.09),                 # s60p27.out b0p ROW range at cap 128
        over=(0.0, 0.90),                # a single row's over-read: 0 .. b0bound.out p27 c2 max8 cost max
        gap=(0.0,), shift1=(0.0,), sub=True, ka=194.2),
    'c8': dict(
        A=[[(160, 0.0), (224, 3.9), (320, 9.1), (512, 21.9), (768, 41.7), (1280, 84.2)]], Ares=0.5,
        B={'p27': [(128, 2.43, 8.10), (192, 3.25, 9.33), (256, 5.80, 11.80), (384, 15.58, 9.96), (512, 23.60, 10.88),
                   (768, 39.31, 11.23)]},
        lc=(7.54, 8.66),                 # leg-0 l on single cap-128 rows (s60p27.out per-row table)
        lcB=(7.54, 8.66),
        lown=(8.22, 11.13),              # s60boot.out leg-0 l medians P80..P640
        sig=(2.00, 2.67),                # s60boot.out sig, c8 leg 0
        b0=(2.07, 2.90),                 # s60p27.out c8 cap-128 leg-0 b0p row range
        over=(0.0, 1.8),                 # 0 .. b0bound.out p27 c8 p0 max_row cost median
        # 1/2*((l1_c + b0_1) - (l0_c + b0_0)) on one calibration row: l1 - l0 = 23.0..24.1 (s60boot P80 rows),
        # b0_1 - b0_0 = 9.37 - 2.07 = 7.3 / 6.12 - 2.90 = 3.2 (the cap-128 rows, paired): 13.1..15.7
        gap=(13.1, 15.7),
        # own-l sensitivity: l1 - l0 = 22.3..25.6 across the arms: 12.8..16.5
        gap_own=(12.8, 16.5),
        # leg 1's pull: w1/w0 0.11..0.18 (s56 r1/r0 at W 320-512) x D1 ~ 13..18 ms: 1.4..3.2 ms
        shift1=(1.4, 3.2), sub=True, ka=478.2),
    'c1s': dict(
        A=[[(384, 0.0), (512, 1.0), (768, 3.0), (1024, 5.5), (2048, 15.0)],
           [(384, 0.61), (512, 1.16), (768, 3.18), (1024, 5.29), (2048, 14.19)]], Ares=0.0,
        B={'p34': [(256, 0.13, 2.50), (384, 0.53, 2.55), (512, 0.98, 2.65), (768, 0.49, 2.82), (1024, 1.91, 2.62),
                   (1536, 2.94, 2.59), (2048, 4.81, 2.85)],
           'p27': [(256, 0.24, 2.38), (384, 0.57, 2.44), (512, 0.71, 2.53), (768, 1.78, 2.57), (1024, 2.19, 2.44),
                   (1536, 4.69, 2.52), (2048, 5.53, 2.56)]},
        lc=None, lown=(2.43, 2.6),
        sig=(0.46, 0.67), b0=(0.21, 0.37),   # b0p row range at cap 256; c1s at 128 is unmeasured
        over=(0.0, 0.20), gap=(0.0,), shift1=(0.0,), sub=False, ka=495.4),
}


def first_w(curve, target):
    if target <= curve[0][1]:
        return float(curve[0][0])
    for (w1, q1), (w2, q2) in zip(curve, curve[1:]):
        if q1 < target <= q2:
            return w1 + (target - q1) * (w2 - w1) / (q2 - q1)
    return float('inf')


def corners(c, mode):
    """(label, anc, [W]) per method for mode 'sub' (calibration l) or 'own' (row's l)."""
    out = []
    ls = c['lc'] if mode == 'sub' else c['lown']
    gaps = c['gap'] if mode == 'sub' else c.get('gap_own', c['gap'])
    for ai, curve in enumerate(c['A']):
        for res in sorted({-c['Ares'], 0.0, c['Ares']}):
            cv = [(w, y + res) for w, y in curve]
            for anc in (0, 1):
                ws = [first_w(cv, over + sig + Q * l + gap + s1 + (b0 if anc else 0.0))
                      for sig, b0, over, gap, s1, l in itertools.product(c['sig'], c['b0'], c['over'], gaps,
                                                                         c['shift1'], ls)]
                out.append(('A%s res%+.1f' % ('' if ai == 0 else '0.1ms', res), anc, ws))
    for led, pts in c['B'].items():
        for anc in (0, 1):
            ws = []
            lsB = c['lcB'] if mode == 'sub' else ls
            for sig, b0, gap, s1, l in itertools.product(c['sig'], c['b0'], gaps, c['shift1'], lsB):
                if mode == 'sub':
                    cv = [(w, lr + x) for w, x, lr in pts]
                    t = l + (2 if anc else 1) * b0 + sig + Q * l + gap + s1
                else:
                    cv = [(w, x) for w, x, _ in pts]
                    t = (2 if anc else 1) * b0 + sig + Q * l + gap + s1
                ws.append(first_w(cv, t))
            out.append(('B %s' % led, anc, ws))
    return out


print('cell | l-hat | method corner anc | W range over corners | KA point (fixed inputs)')
for name, c in CELLS.items():
    for mode in (('sub', 'own') if c['sub'] else ('own',)):
        allw = []
        for lab, anc, ws in corners(c, mode):
            allw += ws
            print('%s | %s | %s anc=%d | %.0f-%.0f | %.1f' % (name, mode, lab, anc, min(ws), max(ws), c['ka']))
        tag = 'BAND (scored recipe)' if (mode == 'sub') == c['sub'] else 'SENSITIVITY (row l)'
        print('%s | %s | %s | %.0f-%.0f |' % (name, mode, tag, min(allw), max(allw)))
