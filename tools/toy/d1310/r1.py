"""Run one seed of A and B' of OV-long-q at T, keep the per-seq arrays (send time, hand-over time, lost, delivered) to compare in seq space."""
import sys, os, math, time, pickle
TOY = r"C:\Users\pierr\AppData\Local\Temp\claude\c92d18e023ce8566\28b06ad0-9458-4799-81e9-72731fe0b356\scratchpad\s3e\tools\toy"
sys.path.insert(0, TOY); os.chdir(TOY)
import numpy as np
import confirm13q as Q, confirm13 as K, toy2
cell, T, seed = sys.argv[1], float(sys.argv[2]), int(sys.argv[3])
jobs = Q.q_jobs() if cell == Q.Q else K.m_jobs()
bj = [j for j in jobs if K.is_bp(j) and j.key[1] == T and j.key[2] == 0.0][0]
aj = [j for j in jobs if j.design == "A" and j.key == bj.key][0]
res = {}
for nm, j in (("A", aj), ("B", bj)):
    cfg = j.builder(seed)
    t1 = time.time()
    s = toy2.Sim(cfg)
    out = s.run()
    n = s.n_sent
    res[nm] = dict(h=np.asarray(s.h_seq[:n]), st=np.asarray(s.sent_t[:n]), dl=np.asarray(s.dlv[:n]), got=np.asarray(s.got[:n]),
                   copies=np.asarray(s.copies[:n]), delivered=out["delivered"], units=s.units_total, dt=s.dt, last_hand=out.get("last_hand"),
                   rev4=out.get("rev4"), cfgrepr=repr(cfg))
    print(nm, "time %.0f s" % (time.time() - t1), "n_sent", n, "delivered", out["delivered"], flush=True)
pickle.dump(res, open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "run_%s_%g_%d.pkl" % (cell, T, seed)), "wb"))
