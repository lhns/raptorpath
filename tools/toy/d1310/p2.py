"""Item 2 per seed: measured deficit over the reader's pre-onset windows against
   d_ss + (F/lam) (w(tau1) - w(tau2)) / L, tau = h + w(tau), w from the run's own cap_series (the law's output, 13.9.2), and against the
   ex-ante form d_ss + D0 * overlap / L.  d_ss: kappa_r S / lam where B' resends before the onset, the decode term where neither build resends."""
import sys, os, math
TOY = r"C:\Users\pierr\AppData\Local\Temp\claude\c92d18e023ce8566\28b06ad0-9458-4799-81e9-72731fe0b356\scratchpad\s3e\tools\toy"
sys.path.insert(0, TOY); os.chdir(TOY)
import numpy as np
import confirm13q as Q, confirm13 as K, confirm3e as C, confirm_read as CR
cell = sys.argv[1]; delta = Q.Q_DELTA if cell == Q.Q else 0.1
DSS = {0.4: 0.00081}            # decode term (no resends in either build), derived in 13.10
KS = 0.065 * 320.0 / 12000.0     # resend term where B' is protected: kappa_r x S_B / lam, S_B = p0 F/(1 - pc) ~ 320/s
jobs = Q.q_jobs() if cell == Q.Q else K.m_jobs()
for bj in [j for j in jobs if K.is_bp(j) and j.key[1] != math.inf and j.key[2] == 0.0]:
    aj = [j for j in jobs if j.design == "A" and j.key == bj.key][0]
    A, Bp = C.recs(aj), C.recs(bj)
    n = min(len(A), len(Bp)); A, Bp = A[:n], Bp[:n]
    T = bj.key[1]
    inp = Q.overload_inputs(Bp, A, T)
    x, c = inp["x"], inp["c"]; o = Q.REG["o"]; F_lam = inp["mu"] / inp["lam"]
    sb = (c + x * o) / delta; g = c / (2 * x * sb); D0 = F_lam * g / (1 + g)
    dss = DSS.get(T, KS)
    deficit = K.window_shares(A) - K.window_shares(Bp)
    ons = np.array([r["rev4"]["onset_t"] for r in Bp]); on = ons.mean()
    a, b, _ = CR.pair_window(A[0], Bp[0], None)
    ser = sorted({row[0] for r in Bp for row in r["rev4"]["cap_series"]})
    pre = [t for t in ser if a - 1e-9 <= t and t + 0.5 <= on]
    ids = [int(round(t / 0.5)) for t in pre]
    L = 0.5 * len(pre); h1, h2 = pre[0], pre[-1] + 0.5
    meas, drift, exante = [], [], []
    for i in range(n):
        cs = {row[0]: row[2] for row in Bp[i]["rev4"]["cap_series"] if row[2] is not None}
        ts = np.array(sorted(cs)); ws = np.array([cs[t] for t in ts])
        wf = lambda tau: float(np.interp(tau, ts + 0.25, ws))
        def tau_of(h):
            t = h + c / x
            for _ in range(30):
                t = h + wf(t)
            return t
        drift.append(dss + F_lam * (wf(tau_of(h1)) - wf(tau_of(h2))) / L)
        exante.append(dss + D0 * max(0.0, h2 - max(h1, ons[i] - c / x)) / L)
        meas.append(deficit[i, ids].mean())
    meas, drift, exante = map(np.array, (meas, drift, exante))
    res = meas - drift
    print("T=%g: measured %.4f (se %.4f) | ex-ante (law at registered inputs) %.4f | with the run's own cap drift %.4f | residual meas - drift %.4f (se %.4f) | per-seed corr %.2f" % (
        T, meas.mean(), meas.std(ddof=1) / math.sqrt(n), exante.mean(), drift.mean(), res.mean(), res.std(ddof=1) / math.sqrt(n), np.corrcoef(meas, drift)[0, 1]))
