"""One seed of A and B' (OV-long-q, T given): log every repair's span (time, members, unknowns at arrival) and D, R, o_hat at staging. Reads only."""
import sys, os, math, time
TOY = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOY); os.chdir(TOY)
import numpy as np
import confirm13q as Q, confirm13 as K, toy2
cell, T, seed, lo_t, hi_t = sys.argv[1], float(sys.argv[2]), int(sys.argv[3]), float(sys.argv[4]), float(sys.argv[5])
jobs = Q.q_jobs() if cell == Q.Q else K.m_jobs()
bj = [j for j in jobs if K.is_bp(j) and j.key[1] == T and j.key[2] == 0.0][0]
aj = [j for j in jobs if j.design == "A" and j.key == bj.key][0]
for nm, j in (("A", aj), ("B'", bj)):
    cfg = j.builder(seed)
    s = toy2.Sim(cfg)
    log, rlog = [], []
    orig_stage = s.stage_repair
    def stage(now, s=s, log=log):
        n0 = s.n_rep
        orig_stage(now)
        if s.n_rep > n0 and lo_t <= now < hi_t:
            log.append((now, s.R_now(), s.o_hat(), s.delta_off(now), s.astar_cur))
    s.stage_repair = stage
    orig_rep = s.on_repair
    def onrep(now, pl, s=s, rlog=rlog):
        j_, lo, end, ts = pl
        if lo_t <= now < hi_t:
            U = sum(1 for q in range(lo, end) if not s.got[q])
            rlog.append((now, end - lo, U))
        orig_rep(now, pl)
    s.on_repair = onrep
    orig_dec = s.ser
    t1 = time.time()
    try:
        out = s.run()
    except Exception as e:
        print("run ended:", repr(e)[:200])
    rl = np.array(rlog); lg = np.array(log)
    print(nm, "%.0f s" % (time.time() - t1), "repairs staged", len(lg), "arrived", len(rl))
    print("  R %.4f o %.4f R-o %.4f D %.1f A* %.1f mu %.0f" % (lg[:,1].mean(), lg[:,2].mean(), (lg[:,1]-lg[:,2]).mean(), lg[:,3].mean(), lg[:,4].mean(), 0))
    print("  members mean %.1f  p10 %.0f p50 %.0f p90 %.0f ; unknowns: P(U=0) %.3f P(U=1) %.3f P(U>=2) %.3f" % (rl[:,1].mean(), *np.percentile(rl[:,1], [10, 50, 90]),
          (rl[:,2] == 0).mean(), (rl[:,2] == 1).mean(), (rl[:,2] >= 2).mean()))
    n = rl[:,1]; p = 0.03
    print("  iid P1 = n p (1-p)^(n-1) averaged over spans: %.3f ; P0 %.3f" % (np.mean(n * p * (1 - p) ** (n - 1)), np.mean((1 - p) ** n)))
    print("  decoded per s over [%.1f, %.1f): %s" % (lo_t, hi_t, [s.ser["dec"].get(k, 0) for k in range(int(lo_t), int(hi_t))]))
