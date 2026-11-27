import sys, os, math
TOY = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOY); os.chdir(TOY)
import numpy as np
import confirm13q as Q, confirm13 as K, confirm3e as C
cell = sys.argv[1]
jobs = Q.q_jobs() if cell == Q.Q else K.m_jobs()
keys = ["rep_sent", "rep_members_sum", "rep_cut_edge", "rep_drop_edge", "rep_drop_young", "rep_lost", "n_dec", "dec_units", "dec_on_time", "dec_late", "rep_used", "rep_innov", "rep_redundant", "dup_after_dec", "repair_sends"]
for bj in [j for j in jobs if K.is_bp(j) and j.key[2] == 0.0]:
    aj = [j for j in jobs if j.design == "A" and j.key == bj.key][0]
    print("T", bj.key[1])
    for nm, j in (("A ", aj), ("B'", bj)):
        R = C.recs(j)
        print("  ", nm, " ".join("%s=%.0f" % (k, np.mean([r["gauges"][k] for r in R])) for k in keys), "members_mean=%.1f" % np.mean([r["rep3e"]["rep_members_mean"] for r in R]))
