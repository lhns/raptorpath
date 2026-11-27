"""13.10: item 2 (the deficit before the onset) as the reader computes it, against
     d2 = d_ss + (F_B / lam) (w(tau1) - w(tau2)) / L ,   tau = h + w(tau)            (the hand-over shift: exact for a head held at the cap)
     d_ss = [dS (pc - p0) + dP (1 - p0) + d dup - d dec] / lam                       (the hole account, B' minus A, per second)
   w is the law (toy2.wait_limit_w) on the registered inputs (13.9.6) and on each seed's own t0 and s_b.  The steady rates come from the
   resend-gauge runs: OV-long-q's own rows (6 seeds), and for OV-long-m the RESEND-GAUGE reruns (3 seeds).  Reads stored runs only."""
import sys, os, math
TOY = os.environ["TOY"]
sys.path.insert(0, TOY); os.chdir(TOY)
import numpy as np
import confirm13q as Q, confirm13 as K, confirm3e as C, confirm_read as CR, toy2
P0, PC, LAM, CAP = 0.03, 0.10, 12000.0, 10000.0          # cell I2 (toy.CELLS) and OV-long's link C (declared)


def rates(A, Bp, lo, hi):
    """Per-second rates over send-time bins [lo, hi) s, seed means: resends, duplicate arrivals, repairs (gauge, 0.5 s) and decodes (rep3e, 1 s)."""
    k0, k1 = int(lo / 0.5), int(hi / 0.5)
    g = lambda R, col: float(Q.gauge_series(R, col)[:, k0:k1].sum(1).mean()) / (hi - lo)
    d = lambda R: float(np.mean([sum(r["rep3e"]["series"]["decoded"][lo:hi]) for r in R])) / (hi - lo)
    return dict(S=(g(A, 1), g(Bp, 1)), dup=(g(A, 2), g(Bp, 2)), P=(g(A, 3), g(Bp, 3)), dec=(d(A), d(Bp)))


def d_ss(r):
    dS = r["S"][1] - r["S"][0]; dP = r["P"][1] - r["P"][0]; ddup = r["dup"][1] - r["dup"][0]; ddec = r["dec"][1] - r["dec"][0]
    return (dS * (PC - P0) + dP * (1 - P0) + ddup - ddec) / LAM


def p1(n):
    return n * P0 * (1 - P0) ** (n - 1)


def shift(T, delta, x, o, R, t0, h1, h2, F_lam):
    w = lambda tau: toy2.wait_limit_w(T, R, o, delta, x, max(tau - t0, 0.0))
    def tau_of(h):
        t = h + w(h)
        for _ in range(60):
            t = h + w(t)
        return t
    return F_lam * (w(tau_of(h1)) - w(tau_of(h2))) / (h2 - h1)


def row(cell, delta, T, A, Bp, GA, GB):
    n = min(len(A), len(Bp)); A, Bp = A[:n], Bp[:n]
    inp = Q.overload_inputs(Bp, A, T)
    x, c, R = inp["x"], inp["c"], inp["R"]; o = Q.REG["o"]
    deficit = K.window_shares(A) - K.window_shares(Bp)
    r4 = [r["rev4"] for r in Bp]
    ons = np.array([r["onset_t"] for r in r4]); t0s = np.array([r["ov_t0"] for r in r4]); sbs = np.array([r["onset_tov"] for r in r4])
    on = ons.mean()
    a, b, _ = CR.pair_window(A[0], Bp[0], None)
    ser = sorted({row[0] for r in Bp for row in r["rev4"]["cap_series"]})
    pre = [t for t in ser if a - 1e-9 <= t and t + 0.5 <= on]
    ids = [int(round(t / 0.5)) for t in pre]
    h1, h2 = pre[0], pre[-1] + 0.5
    lo, hi = math.ceil(h1), math.floor(on)
    rt = rates(GA, GB, lo, hi)
    F_lam = (CAP - rt["S"][1] - rt["P"][1]) / LAM
    dss = d_ss(rt)
    sh_reg = shift(T, delta, Q.REG["x"], o, Q.REG["R"], Q.REG["t0"], h1, h2, F_lam)
    sh_seed = np.array([shift(T, delta, x, o, R, t0s[i], h1, h2, F_lam) for i in range(n)])
    meas = np.array([deficit[i, ids].mean() for i in range(n)])
    clear = [k for k, t in zip(ids, pre) if t + 0.5 <= ons.min() - c / x]
    mclear = np.array([deficit[i, clear].mean() for i in range(n)]) if clear else None
    se = lambda v: v.std(ddof=1) / math.sqrt(len(v))
    # the decode term where neither build resends: n_A = mu_F (R - o) - D (the worth edge), n_B = min(A*, mu_F (R - o + c (1 - 1/x)) - D)
    muF, D, Astar = CAP - rt["P"][0], 10, 200
    nA = muF * (R - o) - D; nB = min(Astar, muF * (R - o + c * (1 - 1 / x)) - D)
    ddec_formula = rt["P"][0] * (1 - P0) * (p1(nA) - p1(nB))
    print("%s T=%g: s_b %.2f, onset %.3f (sd %.3f), c/x %.3f; windows [%.1f, %.1f] (%d), band from %.2f" % (cell, T, sbs.mean(), on, ons.std(), c / x, h1, h2, len(pre), on - c / x))
    print("   rates [%d, %d) s, A / B': S %.0f / %.0f, dup %.1f / %.1f, P %.1f / %.1f, dec %.1f / %.1f -> d_ss %.5f (kappa_r = d_ss lam / dS = %.3f)" % (
        lo, hi, *rt["S"], *rt["dup"], *rt["P"], *rt["dec"], dss, dss * LAM / max(rt["S"][1] - rt["S"][0], 1e-9) if rt["S"][1] > 1 else float("nan")))
    print("   decode term by formula (no resends): n_A %.0f, n_B %.0f, P1 %.3f / %.3f -> dec_A - dec_B %.1f /s = %.5f (measured %.1f /s)" % (
        nA, nB, p1(nA), p1(nB), ddec_formula, ddec_formula / LAM, rt["dec"][0] - rt["dec"][1]))
    print("   shift term: registered inputs %.5f, seeds' own t0 and s_b %.5f | predicted d2 %.4f (registered inputs) / %.4f (seeds) | measured %.4f (se %.4f) | clear of the band: %s" % (
        sh_reg, sh_seed.mean(), dss + sh_reg, dss + sh_seed.mean(), meas.mean(), se(meas),
        "%.4f (se %.4f, %d windows) against d_ss %.4f" % (mclear.mean(), se(mclear), len(clear), dss) if mclear is not None else "none"))
    return dict(T=T, pred_reg=dss + sh_reg, pred_seed=dss + sh_seed.mean(), meas=meas.mean(), se=se(meas), dss=dss)


def main():
    out = []
    jobs = Q.q_jobs()
    for bj in [j for j in jobs if K.is_bp(j) and j.key[1] != math.inf and j.key[2] == 0.0]:
        aj = [j for j in jobs if j.design == "A" and j.key == bj.key][0]
        A, Bp = C.recs(aj), C.recs(bj)
        out.append(("OV-long-q",) + tuple(row("OV-long-q", Q.Q_DELTA, bj.key[1], A, Bp, A, Bp).values()))
    gj = {j.name: j for j in Q.gauge_jobs()}
    for cell, aj, bj in Q.gauge_rows():
        if cell != K.M:
            continue
        GA, GB = C.recs(gj[aj.name + " | gauge"]), C.recs(gj[bj.name + " | gauge"])
        out.append((cell,) + tuple(row(cell, 0.1, aj.key[1], C.recs(aj), C.recs(bj), GA, GB).values()))
    print("\ncell | T | predicted (registered inputs) | predicted (seeds' t0, s_b) | measured (se) | d_ss")
    for r in out:
        print("%s | %g | %.4f | %.4f | %.4f (%.4f) | %.4f | z reg %+.2f | z seeds %+.2f" % (r + ((r[4] - r[2]) / r[5], (r[4] - r[3]) / r[5])))


if __name__ == "__main__":
    main()
