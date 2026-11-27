"""Whole-run hole account: delivered = F (1 - p0) + S (1 - pc) - dup + dec, checked on stored counts; then the bill as
   fluid + [dS (pc - p0) + dP (1 - p0) + d dup - d dec] against fluid + 0.065 dS."""
import sys, os, math
TOY = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOY); os.chdir(TOY)
import numpy as np
import confirm13q as Q, confirm13 as K, confirm3e as C
p0, pc = 0.03, 0.10
cell = sys.argv[1]
jobs = Q.q_jobs() if cell == Q.Q else (K.m_jobs() if cell == K.M else None)
FL = {0.4: 2399, 0.6: 3683, 1.0: 5556, 2.0: 7159} if cell == Q.Q else {}
for bj in [j for j in jobs if K.is_bp(j) and j.key[1] != math.inf and j.key[2] == 0.0]:
    aj = [j for j in jobs if j.design == "A" and j.key == bj.key][0]
    A, Bp = C.recs(aj), C.recs(bj)
    n = min(len(A), len(Bp)); A, Bp = A[:n], Bp[:n]
    T = bj.key[1]
    def cnt(R):
        g = lambda k: np.mean([r["gauges"][k] for r in R])
        F = np.mean([r["n_src"] for r in R]); D = np.mean([r["delivered"] for r in R])
        return dict(F=F, D=D, S=g("repair_sends"), P=g("rep_sent"), dup=g("dup"), dec=g("dec_units"), qd=g("queue_drop"))
    a, b = cnt(A), cnt(Bp)
    for nm, x in (("A", a), ("B'", b)):
        x["Dhat"] = x["F"] * (1 - p0) + x["S"] * (1 - pc) - x["dup"] + x["dec"]
    dS, dP, ddup, ddec = b["S"] - a["S"], b["P"] - a["P"], b["dup"] - a["dup"], b["dec"] - a["dec"]
    hole = dS * (pc - p0) + dP * (1 - p0) + ddup - ddec
    print("T=%g: identity A D %.0f vs %.0f, B' D %.0f vs %.0f (qdrop %.0f/%.0f) | A-B' meas %.0f | dF %.0f | hole terms %.0f (dS %.0f dP %.0f ddup %.0f ddec %.0f) vs 0.065 dS %.0f%s" % (
        T, a["D"], a["Dhat"], b["D"], b["Dhat"], a["qd"], b["qd"], a["D"] - b["D"], a["F"] - b["F"], hole, dS, dP, ddup, ddec, 0.065 * dS,
        (" | fluid %d: fluid+hole %.0f (miss %+.0f), fluid+kappa %.0f (miss %+.0f)" % (FL[T], FL[T] + hole, a["D"] - b["D"] - FL[T] - hole, FL[T] + 0.065 * dS, a["D"] - b["D"] - FL[T] - 0.065 * dS)) if T in FL else ""))
