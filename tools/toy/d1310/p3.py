"""d_ss from the stored rates over the clear pre-onset span (send time): S_B (pc - p0) + dP (1 - p0) + dup_B - dec_B + dec_A - dup_A, per lam."""
import sys, os, math
TOY = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOY); os.chdir(TOY)
import numpy as np
import confirm13q as Q, confirm13 as K, confirm3e as C, confirm_read as CR
p0, pc, lam = 0.03, 0.10, 12000.0
jobs = Q.q_jobs()
for bj in [j for j in jobs if K.is_bp(j) and j.key[1] != math.inf and j.key[2] == 0.0]:
    aj = [j for j in jobs if j.design == "A" and j.key == bj.key][0]
    A, Bp = C.recs(aj), C.recs(bj)
    T = bj.key[1]
    on = np.mean([r["rev4"]["onset_t"] for r in Bp])
    a, b, _ = CR.pair_window(A[0], Bp[0], None)
    c = T - 0.104; x = 1.238
    # send-time span of the clear pre-onset hand-over windows: [a + c/x, on - 0.1] for B'
    lo, hi = math.ceil(a + c / x), math.floor(on - 0.2)
    if hi - lo < 1:
        lo, hi = math.ceil(a), math.floor(on)
    k0, k1 = int(lo / 0.5), int(hi / 0.5)
    g = lambda R, col: Q.gauge_series(R, col)[:, k0:k1].sum(1).mean() / (hi - lo)
    dec = lambda R: np.mean([sum(r["rep3e"]["series"]["decoded"][lo:hi]) for r in R]) / (hi - lo)
    S_B, S_A = g(Bp, 1), g(A, 1); dup_B, dup_A = g(Bp, 2), g(A, 2); P_B, P_A = g(Bp, 3), g(A, 3)
    dB, dA = dec(Bp), dec(A)
    dss = ((S_B - S_A) * (pc - p0) + (P_B - P_A) * (1 - p0) + (dup_B - dup_A) - (dB - dA)) / lam
    print("T=%g span [%d, %d) s: S_B %.0f S_A %.0f dup_B %.1f dup_A %.1f P_B %.1f P_A %.1f dec_B %.1f dec_A %.1f -> d_ss %.5f ; kappa-form 0.065 dS/lam %.5f" % (
        T, lo, hi, S_B, S_A, dup_B, dup_A, P_B, P_A, dB, dA, dss, 0.065 * (S_B - S_A) / lam))
