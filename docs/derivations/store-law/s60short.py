"""s60 (main-session ruling folding R6.7, github/store-law-r67 4cff88d0): c2's scored reading, the ALLOWANCE
SHORTFALL at the in-session knee, its prediction and its resolution, computed before any arm is fixed.

Usage: python -I s60short.py      (no input: every number is cited per line)

Definition (per replicate r, leg 0, from printed [SLAW] fields; s60-registration.md section 3.1):
    D_r = ( Tmin_r(W95,r) - F_floor ) - ( sigma_r + q * l_c ),   q = 0.10 (s60 is Bulk only)
  Tmin_r(W95,r): that replicate's per-row median LATE `tmin`, interpolated in W at its own goodput knee W95,r;
  F_floor: the session floor at the floor pool (median over its valid rows of their median LATE `tmin`);
  sigma_r: median LATE `sig` over the knee-bracketing rows; l_c: the session median `l` at the floor pool.
  Pooled: the median over replicates. SHORT iff its 90 % interval > 0; COVERED iff < 0; else UNRESOLVED.
Prediction: R6.7 section 5 (p27): the rise needed above the W 64 reading 9.45 ms is ~3.5 ms at knee 169 (goodput,
the in-session knee) and ~4.0 at 181; sigma + q*l at q = 0.10 is ~2.6 ms (R6.7's 2.3-2.6 spans q 0.05-0.10).
So D(169) ~ +0.9 with the floor at W 64. The floor pool here is W 16 (s60 section 2): flat below 64 (R6.7 BELOW
flat) leaves D ~ +0.9; the measured secant below saturation (0.020-0.027 ms/slot, R6.7 1.2 / 4.3) extended to W 16
lowers the floor by 0.96-1.30 ms and raises D to ~ +1.9..+2.2.
Resolution: se of the pooled median from its terms; 90 % half-width 1.645 se. The knee-position term uses the slope
AT the knee: p27 ~0.043 ms/slot (192-256 window minima), s56's central curve ~0.094 (160-192).
"""
from math import sqrt

N = 6
MED = 1.253
terms = {
    'abs late Tmin at the knee arms, row sd': (0.10, 0.25),        # s60abs.out c2 W 128-384 (s56); p27 0.02-0.17
    'knee position: sigma_W95 x slope at the knee': (3 * 0.043, 3 * 0.094),
    'sigma at load, row sd': (0.10, 0.15),                         # s60boot.out c2 sig per arm 1.72-1.95
}
common = {
    'session floor F_floor (median of N rows)': (MED * 0.10 / sqrt(N), MED * 0.25 / sqrt(N)),
    'q * l_c (median of N rows, l row sd 1.4 ms)': (0.10 * MED * 1.4 / sqrt(N), 0.10 * MED * 1.4 / sqrt(N)),
}
for lab, (lo, hi) in list(terms.items()) + list(common.items()):
    print('%-46s %.3f - %.3f ms' % (lab, lo, hi))
for case, j in (('low', 0), ('high', 1)):
    per = sqrt(sum(v[j] ** 2 for v in terms.values()))
    se = sqrt((MED * per / sqrt(N)) ** 2 + sum(v[j] ** 2 for v in common.values()))
    print('%s: per-replicate sd %.2f ms -> pooled se %.2f ms, 90 %% half-width %.2f ms' % (case, per, se, 1.645 * se))
    for lab, d in (('floor flat below W 64', 0.9), ('floor on the secant to W 16', 1.9)):
        print('   predicted D at knee 169, %s: %+.1f ms -> interval lower edge %+.2f (SHORT iff > 0)'
              % (lab, d, d - 1.645 * se))
print('COVERED would need the pooled D <= -(half-width); a floor read too high biases D DOWN (toward COVERED), so')
print('SHORT stands as a lower bound without floor convergence and COVERED needs a converged floor.')

# The floor CONVERGENCE gate's resolution (entry 10, advisor). Two forms on 6 rows per pool, row sd s of median LATE
# `tmin` (0.10-0.25 ms at c2, s60abs.out; 0.25-0.50 at c8 leg 0):
#  (a) adjacent pools: Delta = tmin(next) - tmin(floor), SE = 1.253 * s * sqrt(2/6);
#  (b) a least-squares slope over the three floor pools (W1, W1+d, W1+2d): se_b = s / sqrt(6 * 2 * d^2), and the
#      floor's excess over the W -> 0 limit E = b * W1, se_E = W1 * se_b.
print()
print('convergence gate resolution:')
for cell, W1, d, slopes, sds, tau in (('c2', 16, 16, (0.020, 0.027), (0.10, 0.25), 0.3),
                                     ('c8 leg 0', 32, 32, (0.020, 0.027), (0.25, 0.50), 1.0)):
    for s in sds:
        se_adj = MED * s * sqrt(2 / 6)
        se_b = s / sqrt(6 * 2 * d * d)
        for b in (0.0,) + slopes:
            E, seE = b * W1, W1 * se_b
            print('   %s s %.2f slope %.3f: adjacent Delta %.2f +- 2SE %.2f | fit slope se %.4f, E = %.2f +- 2se %.2f ms'
                  ' (tau %.1f)' % (cell, s, b, b * d, 2 * se_adj, se_b, E, 2 * seE, tau))
