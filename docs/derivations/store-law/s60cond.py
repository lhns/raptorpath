"""s60 ruling entry 6, items 1-2: the conditioning arithmetic of R~ at each cell, before any arm is kept.

Usage: python -I s60cond.py      (no input: every number is copied from a committed .out, cited per line)

R~ - 1 = sum_i w_i (F_i + sig_i + q*l_i - Tmin_i) / sum_i w_i T_i, so near a W where the inputs are fixed,
    dR~/dW = - (w0 / sum w) * (dTmin_0/dW) / T-bar
and an input scatter s (ms, on the weighted numerator) moves R~ by s / T-bar. The miss B2 can resolve, in slots, is
2*sigma_R / |dR~/dW|; B2 can fire only if the knee lies more than that below the crossing.

c1s: the verdict probabilities of the control-sign test (sign test, two-sided 0.05) for a prior on the true
mean mu = E[R~ - 1] at W 2048 and a per-row sigma_R, with n_s ~ Binomial(12, 0.64) scoreable replicates.
"""
from math import comb, erf, sqrt


def phi(x):
    return 0.5 * (1 + erf(x / sqrt(2)))


def report(name, dtmin, tbar, w0, scat, knee, low_edge):
    slope = w0 * dtmin / tbar
    print('%s: dTmin/dW %s ms/slot, T-bar %.0f ms, w0 share %.2f -> |dR/dW| %.5f /slot' % (name, dtmin, tbar, w0, slope))
    for lab, s in scat:
        sr = sqrt(sum((x / tbar) ** 2 for x in s))
        print('   sigma_R (%s) = %.3f -> 2 sigma_R = %.0f slots; B2 fires iff the crossing lies below %.0f (knee %d, band low edge %d)'
              % (lab, sr, 2 * sr / slope, knee - 2 * sr / slope, knee, low_edge))


# Scatter terms (ms, on the numerator), per row. s60abs.out shows the row scatter of l is sampling noise of a minimum:
# at c2 the ABSOLUTE late floor has row sd 0.10-0.25 ms (s56 echo, W 128-384; p27 window minima 0.02-0.17 at W <= 256)
# while l has 0.55-2.94, with corr(l, floor - l) = -0.96..-1.00. So:
#  - per-replicate calibration: the calibration row's l and b0p enter (l sd up to 1.4 ms, sqrt2 for two rows; b0p sd
#    ~0.9, x2 at anc 1), plus the arm row's absolute T_min scatter;
#  - session calibration: the calibration values are one number per cell. Where anc = 1 (m_row >= l_c + b0_c) the arm
#    row then enters only through its absolute T_min, T-hat and sigma (~0.1 ms). Where anc = 0 the row's m-hat enters
#    F instead and R~ - 1 >= sigma/T-hat > 0 whenever the late brake T_min - m <= b0_c + q*l_c, so these slope /
#    sigma numbers hold only on anc-1 arms (s60abs.out: c2 is anc 0 at W 128-160 and anc 1 from 192; c8 is anc 1
#    from W 320, and on 3 of 4 rows at 224).
# c2: Tmin climbs 0.021 ms/slot (p27 minima, W 128 -> 192: 2.41 -> 3.76 above l) to 0.047 (s56 central curve 0 -> 3.0);
#     T-bar ~18 ms at W 160-192 (rep56 T-bar 16-19).
print('c2 (the conditioning readout, ruling item 1):')
for d in (0.021, 0.047):
    report('c2', d, 18.0, 1.0,
           [('per-replicate calibration', (1.98, 1.8, 0.25)),
            ('session calibration, abs T_min sd 0.10', (0.10, 0.10)),
            ('session calibration, abs T_min sd 0.25', (0.25, 0.10))], 169, 158)
# c8 leg 0 between the knee (340) and the band's low edge (433): Tmin climbs 0.063 / 0.076 ms/slot (p27 minima over
# 256-384-512) to 0.067 (s56 central curve); T-bar ~33 ms weighted over legs (rep56 rtt0 26 at W 320, 39 at 512; leg 1
# ~47-61 at weight ~0.13); w0 share 0.87. Absolute floor row sd on s56 (integer-ms per-leg echo): 0.25-0.50 ms at
# W 320-512 (s60abs.out); p27's 9 s c8 rows (sd 1.3-26 ms) do not settle and are not used.
print('c8 (ruling item 2):')
for d in (0.063, 0.076):
    report('c8', d, 33.0, 0.87,
           [('per-replicate calibration', (0.4, 0.8, 0.8, 0.5)),
            ('session calibration, abs T_min sd 0.25', (0.25, 0.10)),
            ('session calibration, abs T_min sd 0.50', (0.50, 0.10))], 340, 433)

# c1s control sign (ruling item 2), PER READING (no prior averaged over): mu = E[R~ - 1] at W 2048 is ~ -0.2 on the
# prefix minima (p27/p34) and ~0 on s56's exact Copa lift (0.96 ms against an anchor-bound target of 1.4-1.7 ms, if
# the late brake is < 0.5-0.8 ms); +0.05 is shown as the PINNED side. sigma_R per row: the absolute late floor at c1s
# W 2048 on unflagged s56 rows reads sd ~0.6-1.5 ms (s60abs.out, the 80.75 ms row is a collapse) over T-bar 19 ms:
# 0.03-0.08. Scoreable replicates n_s ~ Binomial(12, 0.67) under session calibration (an unflagged control G row).
K = {6: 6, 7: 7, 8: 8, 9: 8, 10: 9, 11: 10, 12: 10}      # two-sided sign test at 0.05 (s60-registration.md)


def verdicts(mu, sr, p_score=0.67, n=12):
    pin = cro = und = 0.0
    p_up = 1 - phi(-mu / sr)
    for ns in range(n + 1):
        pn = comb(n, ns) * p_score ** ns * (1 - p_score) ** (n - ns)
        if ns < 6:
            und += pn
            continue
        k = K[ns]
        pin += pn * sum(comb(ns, j) * p_up ** j * (1 - p_up) ** (ns - j) for j in range(k, ns + 1))
        cro += pn * sum(comb(ns, j) * (1 - p_up) ** j * p_up ** (ns - j) for j in range(k, ns + 1))
    return pin, cro, und


print('c1s control sign (ruling item 2): P(PINNED / CROSSES / SPLIT / UNDERPOWERED) per reading')
for lab, mu in (('prefix minima', -0.20), ('midway', -0.10), ('Copa lift', 0.0), ('pinned side', 0.05)):
    for sr in (0.03, 0.08):
        a, b, u = verdicts(mu, sr)
        print('   %-13s mu %+.2f sigma_R %.2f: PINNED %.2f  CROSSES %.2f  SPLIT %.2f  UNDERPOWERED %.2f'
              % (lab, mu, sr, a, b, 1 - a - b - u, u))

