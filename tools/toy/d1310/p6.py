"""Where the whole-run A - B' sits in hand-over time: sums of (deliv_A - deliv_B) over segments, against the fluid parts
   F (c - c/x) [the free band, early], d_ss lam span + F (c/x - w_end) [steady + descent], -(lam - F) w_end [the flush], and the paired-window share."""
import sys, os, math
TOY = os.environ["TOY"]; sys.path.insert(0, TOY); os.chdir(TOY)
import numpy as np
import confirm13q as Q, confirm13 as K, confirm3e as C, confirm_read as CR, toy2
DSS = {0.4: 0.00089, 0.6: 0.00113, 1.0: 0.00115, 2.0: 0.00172}
FB = {0.4: 9947.3, 0.6: 9659.2, 1.0: 9662.5, 2.0: 9635.0}     # C - S_B - P_B from item2_account (pre-onset rates)
jobs = Q.q_jobs(); lam = 12000.0
for bj in [j for j in jobs if K.is_bp(j) and j.key[1] != math.inf and j.key[2] == 0.0]:
    aj = [j for j in jobs if j.design == "A" and j.key == bj.key][0]
    A, Bp = C.recs(aj), C.recs(bj); T = bj.key[1]
    inp = Q.overload_inputs(Bp, A, T); x, c, R = inp["x"], inp["c"], inp["R"]; o = Q.REG["o"]
    dA = np.mean([r["series"]["deliv"] for r in A], 0); dB = np.mean([r["series"]["deliv"] for r in Bp], 0)
    diff = dA - dB
    on = np.mean([r["rev4"]["onset_t"] for r in Bp]); t0 = np.mean([r["rev4"]["ov_t0"] for r in Bp]); tL = Bp[0]["last_hand"]
    w = lambda tau: toy2.wait_limit_w(T, R, o, Q.Q_DELTA, x, max(tau - t0, 0.0))
    wend = w(tL)
    a, b, _ = CR.pair_window(A[0], Bp[0], None)
    k = lambda t: int(round(t / 0.5))
    seg = [("early [0, a)", 0, k(math.ceil(2 * a) / 2)), ("[a, tL - w_end)", k(math.ceil(2 * a) / 2), k(math.floor(2 * (tL - wend)) / 2)), ("[tL - w_end, end]", k(math.floor(2 * (tL - wend)) / 2), len(diff))]
    F = FB[T]
    span = math.floor(2 * (tL - wend)) / 2 - math.ceil(2 * a) / 2
    model = [F * (c - c / x), DSS[T] * lam * span + F * (c / x - wend), -(lam - F) * wend]
    tot = diff.sum()
    print("T=%g: total A - B' %.0f | %s | model sum %.0f (w_end %.3f)" % (T, tot, " | ".join("%s meas %.0f model %.0f" % (s[0], diff[s[1]:s[2]].sum(), m) for s, m in zip(seg, model)), sum(model), wend))
