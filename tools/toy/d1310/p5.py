"""Item 3's statistic with the law mapped to hand-over time (h = tau - w(tau)), beside the reader's (law at tau = window time)."""
import sys, os, math
TOY = r"C:\Users\pierr\AppData\Local\Temp\claude\c92d18e023ce8566\28b06ad0-9458-4799-81e9-72731fe0b356\scratchpad\s3e\tools\toy"
sys.path.insert(0, TOY); os.chdir(TOY)
import numpy as np
import confirm13q as Q, confirm13 as K, confirm3e as C, confirm_read as CR
cell = sys.argv[1]; delta = Q.Q_DELTA if cell == Q.Q else 0.1
jobs = Q.q_jobs() if cell == Q.Q else K.m_jobs()
def law_h(T, t0, x, o, R, ml, nw, sub=12, h_=1e-3):
    out = np.zeros(nw)
    w = lambda tau: Q.w_of(T, delta, max(tau - t0, 0.0), x, o, R)
    for k in range(nw):
        v = 0.0
        for i in range(sub):
            hh = 0.5 * k + 0.5 * (i + 0.5) / sub
            tau = hh + w(hh)
            for _ in range(40):
                tau = hh + w(tau)
            if tau - t0 > h_:
                wp = abs(w(tau + h_) - w(tau - h_)) / (2 * h_)
                v += wp / (1 + wp)
        out[k] = ml * v / sub
    return out
for bj in [j for j in jobs if K.is_bp(j) and j.key[1] != math.inf and j.key[2] == 0.0]:
    aj = [j for j in jobs if j.design == "A" and j.key == bj.key][0]
    A, Bp = C.recs(aj), C.recs(bj)
    n = min(len(A), len(Bp)); A, Bp = A[:n], Bp[:n]
    T = bj.key[1]
    inp = Q.overload_inputs(Bp, A, T)
    deficit = np.nanmean(K.window_shares(A) - K.window_shares(Bp), axis=0); nw = len(deficit)
    term, _ = Q.resend_term(A, Bp)
    on = np.mean([r["rev4"]["onset_t"] for r in Bp])
    a, b, _ = CR.pair_window(A[0], Bp[0], None)
    ser = sorted({row[0] for r in Bp for row in r["rev4"]["cap_series"]})
    ka = [int(round(t / 0.5)) for t in ser if t >= on - 0.5 and t >= a - 1e-9 and t + 0.5 <= b]
    ml = inp["mu"] / inp["lam"]
    lt = Q.law_deficit(T, delta, inp["t0"], inp["x"], Q.REG["o"], inp["R"], ml, nw) + np.maximum(term, 0.0)
    lh = law_h(T, inp["t0"], inp["x"], Q.REG["o"], inp["R"], ml, nw) + np.maximum(term, 0.0)
    pk = max(Q.centred_mean(deficit, k) for k in ka)
    pt = max(Q.centred_mean(lt, k) for k in ka); ph = max(Q.centred_mean(lh, k) for k in ka)
    print("T=%g measured %.4f | law at window time %.4f (ratio %.2f) | law in hand-over time %.4f (ratio %.2f)" % (T, pk, pt, pk / pt, ph, pk / ph))
