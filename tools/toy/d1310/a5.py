import sys, os, math
TOY = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOY); os.chdir(TOY)
import numpy as np
import confirm13q as Q, confirm13 as K, confirm3e as C
cell, T, hi = sys.argv[1], float(sys.argv[2]), int(sys.argv[3])
jobs = Q.q_jobs() if cell == Q.Q else K.m_jobs()
bj = [j for j in jobs if K.is_bp(j) and j.key[1] == T and j.key[2] == 0.0][0]
aj = [j for j in jobs if j.design == "A" and j.key == bj.key][0]
A, Bp = C.recs(aj), C.recs(bj)
def arr(R, key):
    return np.array([[float(v) for v in r["rep3e"]["series"][key][:hi]] for r in R])
print("onset", np.mean([r["rev4"]["onset_t"] for r in Bp]))
for key in ("decoded", "repairs", "astar"):
    a, b = arr(A, key), arr(Bp, key)
    print(key)
    print("  A ", " ".join("%6.1f" % v for v in a.mean(0)))
    print("  B'", " ".join("%6.1f" % v for v in b.mean(0)))
