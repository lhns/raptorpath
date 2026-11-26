"""Item 2 as the reader computes it, beside the hand-over-shift term: per seed, the overlap of the reader's pre-onset windows with [t_on,i - c/x, t_on,i],
times D0 = (mu/lam) g/(1+g), g = c / (2 x s_b).  Also the measured deficit in the pre windows split into 'inside the shift band' and 'outside'."""
import sys, os, math
TOY = r"C:\Users\pierr\AppData\Local\Temp\claude\c92d18e023ce8566\28b06ad0-9458-4799-81e9-72731fe0b356\scratchpad\s3e\tools\toy"
sys.path.insert(0, TOY); os.chdir(TOY)
import numpy as np
import confirm13q as Q, confirm13 as K, confirm3e as C, confirm_read as CR
cell = sys.argv[1]; delta = Q.Q_DELTA if cell == Q.Q else 0.1
jobs = Q.q_jobs() if cell == Q.Q else K.m_jobs()
for bj in [j for j in jobs if K.is_bp(j) and j.key[1] != math.inf and j.key[2] == 0.0]:
    aj = [j for j in jobs if j.design == "A" and j.key == bj.key][0]
    A, Bp = C.recs(aj), C.recs(bj)
    n = min(len(A), len(Bp)); A, Bp = A[:n], Bp[:n]
    T = bj.key[1]
    inp = Q.overload_inputs(Bp, A, T)
    x, c, R = inp["x"], inp["c"], inp["R"]; o = Q.REG["o"]
    mu_lam = inp["mu"] / inp["lam"]
    sb = (c + x * o) / delta
    g = c / (2 * x * sb); D0 = mu_lam * g / (1 + g)
    sa, sbh = K.window_shares(A), K.window_shares(Bp)
    deficit = sa - sbh
    term, have = Q.resend_term(A, Bp)
    ons = np.array([r["rev4"]["onset_t"] for r in Bp]); on = ons.mean()
    a, b, _ = CR.pair_window(A[0], Bp[0], None)
    ser = sorted({row[0] for r in Bp for row in r["rev4"]["cap_series"]})
    pre = [t for t in ser if a - 1e-9 <= t and t + 0.5 <= on]
    ids = [int(round(t / 0.5)) for t in pre]
    L = 0.5 * len(pre); Wend = pre[-1] + 0.5 if pre else None
    ov = np.array([max(0.0, Wend - max(pre[0], oi - c / x)) for oi in ons]) if pre else np.zeros(n)
    shift = D0 * ov / L if pre else np.zeros(n)
    dmeas = np.array([deficit[i, ids].mean() for i in range(n)]) if pre else None
    rterm = float(np.mean(term[ids])) if pre else float('nan')
    # split measured: windows fully before every seed's band start vs the rest
    far = [k for k, t in zip(ids, pre) if t + 0.5 <= ons.min() - c / x]
    print("T=%g x=%.4f c=%.4f c/x=%.3f s_b=%.2f g=%.4f D0=%.4f mu/lam=%.4f onset %.3f (sd %.3f) pre windows %d [%.1f, %.1f]" % (T, x, c, c / x, sb, g, D0, mu_lam, on, ons.std(), len(pre), pre[0] if pre else 0, Wend or 0))
    if pre:
        print("   measured item 2 %.4f (seed se %.4f) | resend term (reader) %.4f | shift term %.4f (per-seed overlap %.3f s) | sum %.4f" % (
            dmeas.mean(), dmeas.std(ddof=1) / math.sqrt(n), rterm, shift.mean(), ov.mean(), rterm + shift.mean()))
        if far:
            dfar = np.array([deficit[i, far].mean() for i in range(n)])
            print("   windows clear of the band (%d): measured %.4f (se %.4f), resend term there %.4f" % (len(far), dfar.mean(), dfar.std(ddof=1) / math.sqrt(n), float(np.mean(term[far]))))
