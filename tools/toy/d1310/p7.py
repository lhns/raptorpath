"""The whole-run bill as fluid parts + the hole account per phase (pre-onset and post-onset rates, from the run's gauges)."""
import sys, os, math
TOY = os.environ["TOY"]; sys.path.insert(0, TOY); os.chdir(TOY)
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import numpy as np
import confirm13q as Q, confirm13 as K, confirm3e as C, confirm_read as CR, toy2
from item2_account import rates, d_ss, CAP, LAM
jobs = Q.q_jobs()
for bj in [j for j in jobs if K.is_bp(j) and j.key[1] != math.inf and j.key[2] == 0.0]:
    aj = [j for j in jobs if j.design == "A" and j.key == bj.key][0]
    A, Bp = C.recs(aj), C.recs(bj); T = bj.key[1]
    inp = Q.overload_inputs(Bp, A, T); x, c, R = inp["x"], inp["c"], inp["R"]; o = Q.REG["o"]
    on = np.mean([r["rev4"]["onset_t"] for r in Bp]); t0 = np.mean([r["rev4"]["ov_t0"] for r in Bp]); tL = Bp[0]["last_hand"]
    w = lambda tau: toy2.wait_limit_w(T, R, o, Q.Q_DELTA, x, max(tau - t0, 0.0))
    wend = w(tL)
    a, b, _ = CR.pair_window(A[0], Bp[0], None)
    rpre = rates(A, Bp, math.ceil(a), math.floor(on)); rpost = rates(A, Bp, math.ceil(on) + 2, 38)
    dpre, dpost = d_ss(rpre), d_ss(rpost)
    F = CAP - rpost["S"][1] - rpost["P"][1]
    h_on = on - c / x
    parts = [F * (c - c / x), LAM * (dpre * max(h_on - a, 0) + dpost * (tL - wend - max(h_on, a))), F * (c / x - wend), -(LAM - F) * wend]
    meas = np.mean([r["delivered"] for r in A]) - np.mean([r["delivered"] for r in Bp])
    print("T=%g: d_ss pre %.5f post %.5f (S_B post %.0f) | free band %.0f, hole account %.0f, descent %.0f, flush %.0f -> %.0f against measured %.0f (miss %+.0f); 13.9.5's form missed %s" % (
        T, dpre, dpost, rpost["S"][1], *parts, sum(parts), meas, meas - sum(parts), {0.4: -18, 0.6: -56, 1.0: -152, 2.0: -190}[T]))

print("\nsteady share on the A-paired window:")
for bj in [j for j in jobs if K.is_bp(j) and j.key[1] != math.inf and j.key[2] == 0.0]:
    aj = [j for j in jobs if j.design == "A" and j.key == bj.key][0]
    A, Bp = C.recs(aj), C.recs(bj); T = bj.key[1]
    inp = Q.overload_inputs(Bp, A, T); x, c, R = inp["x"], inp["c"], inp["R"]; o = Q.REG["o"]
    on = np.mean([r["rev4"]["onset_t"] for r in Bp]); t0 = np.mean([r["rev4"]["ov_t0"] for r in Bp]); tL = Bp[0]["last_hand"]
    w = lambda tau: toy2.wait_limit_w(T, R, o, Q.Q_DELTA, x, max(tau - t0, 0.0))
    wend = w(tL)
    a, b, _ = CR.pair_window(A[0], Bp[0], None)
    h1 = math.ceil(2 * a - 1e-9) / 2; h2 = math.floor(2 * b + 1e-9) / 2; L = h2 - h1
    rpre = rates(A, Bp, math.ceil(a), math.floor(on)); rpost = rates(A, Bp, math.ceil(on) + 2, 38)
    F = (CAP - rpost["S"][1] - rpost["P"][1]) / LAM
    def tau_of(h):
        t = h + w(h)
        for _ in range(60):
            t = h + w(t)
        return min(t, tL)
    h_on = on - c / x
    dss = (d_ss(rpre) * max(min(h_on, h2) - h1, 0) + d_ss(rpost) * max(h2 - max(h_on, h1), 0)) / L
    pred = dss + F * (w(tau_of(h1)) - w(tau_of(h2))) / L - (1 - F) * max(h2 - (tL - wend), 0) / L
    sa = np.mean([CR.readouts(r["series"], CR.window_ids(len(r["series"]["handed"]), *CR.pair_window(r, rb, None)), 0.4)[0] for r, rb in zip(A, Bp)]) if False else None
    print("T=%g: window [%.1f, %.1f], B' - A predicted %+.4f (d_ss %.4f, descent %.4f, flush %.4f)" % (T, h1, h2, -pred, dss, F * (w(tau_of(h1)) - w(tau_of(h2))) / L, (1 - F) * max(h2 - (tL - wend), 0) / L))
