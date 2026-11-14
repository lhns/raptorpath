"""Round-6 bracket table: every candidate vs the sliding decode, split by
caller window. The sliding decode has NO deadline and decodes by generic rank,
so it is an UPPER bound on per-loss FEC recovery -- conditional on its model
inputs (uniform debt-accrual spacing, one GE chain over the wire incl. repairs).
A form above the bound is refuted (under those premises); a form below it is
NOT ORDERED (no lower bound is derived).

  A* 3-7 : the shed budget's window (evicting seat), eps .048, s2 3.76
  W 201  : r_saturation's window on L1 (SPAN wsize p50 201; sec 57b client logs
           on main 7c5872f5 read p50 201 too)

Run: python -I pfec_bracket.py > pfec_bracket.out
"""
import math, os
import numpy as np
from scipy.stats import beta as _beta

HERE = os.path.dirname(os.path.abspath(__file__))
src = open(os.path.join(HERE, "pfec_review.py"), encoding="utf-8").read()
exec(src.rsplit("# ================================================================= output", 1)[0])

def sl(e, s2, r, A, T, seeds=(7, 11, 13), deltas=(1, 2), rep_erased=True):
    """max over Delta of the mean over seeds (the bound is the larger), with SE."""
    best = (-1.0, 0.0, 0)
    for d in deltas:
        res = [sliding_decode(e, s2, r, A, d, T=T, seed=s, rep_erased=rep_erased) for s in seeds]
        v = [x[0] for x in res]; n = sum(x[1] for x in res)
        m = float(np.mean(v)); se = float(np.std(v, ddof=1) / math.sqrt(len(v)))
        if m > best[0]:
            best = (m, se, n)
    return best

def verdict(P, m, se):
    return "REFUTED (above bound)" if P > m + 3 * se else "not ordered"

FORMS = (("shipped", p_cur), ("phi", p_phi), ("C6", p_palm))

print("== T1. Body, A* 3-7 (shed budget window), eps .048, s2 3.76")
print("   sliding = max over Delta in {1,2} of the 3-seed mean (SE); verdict vs bound + 3 SE")
print(f"   {'A*':>3} {'r':>4} | {'shipped':>7} {'phi':>6} {'C6':>6} {'C5':>6} | {'sliding':>7} {'SE':>6} | verdicts shipped / C6 / C5")
for A in (3, 4, 5, 6, 7):
    for r in (0.2, 0.29, 0.34, 0.5):
        m, se, n = sl(0.048, 3.76, r, A, 40_000)
        Ps = {nm: f(r, .048, A, 3.76) for nm, f in FORMS}
        c5 = palm_joint_exact(r, .048, 3.76, A)
        tag = "  <- engine pin point" if (A == 4 and r == 0.34) else ""
        print(f"   {A:3d} {r:4.2f} | {Ps['shipped']:7.4f} {Ps['phi']:6.4f} {Ps['C6']:6.4f} {c5:6.4f} | "
              f"{m:7.4f} {se:6.4f} | {verdict(Ps['shipped'],m,se)} / {verdict(Ps['C6'],m,se)} / "
              f"{verdict(c5,m,se)}{tag}")

print("\n== T2. Placement premise: repairs never erased (a loss-free second path), Delta in {1,2}")
for A, r in ((4, 0.34), (7, 0.5)):
    m, se, n = sl(0.048, 3.76, r, A, 40_000, rep_erased=False)
    print(f"   A* {A} r {r}: sliding (repairs never erased) {m:.4f} (SE {se:.4f})  shipped {p_cur(r,.048,A,3.76):.4f}")

print("\n== T3. Body, W 201 (r_sat window), nominal c2 / c3 inputs off r_c")
for name, e, s2v in [("c2", 0.013/0.513, 1+2*(1-0.513)/0.513), ("c3", 0.02/0.42, 1+2*(1-0.42)/0.42)]:
    rc = e / (1 - e)
    for k in (1.5, 2.0):
        r = k * rc
        m, se, n = sl(e, s2v, r, 201, 30_000, deltas=(1,))
        c5 = palm_joint_exact(r, e, s2v, 201)
        Ps = {nm: f(r, e, 201, s2v) for nm, f in FORMS}
        print(f"   {name} r = {k} r_c = {r:.4f}: shipped {Ps['shipped']:.4f} phi {Ps['phi']:.4f} C6 {Ps['C6']:.4f} "
              f"C5 {c5:.4f} | sliding {m:.4f} (SE {se:.4f}, {n} holes) | verdicts shipped {verdict(Ps['shipped'],m,se)}, "
              f"C6 {verdict(Ps['C6'],m,se)}")

print("\n== T4. Tail, W 201, closed-form Auto r*: per-loss MISS (sliding = LOWER bound on the miss)")
def cp_upper(k, n, a=0.05):
    return float(_beta.ppf(1 - a / 2, k + 1, n - k)) if k < n else 1.0
for name, e, s2v, r, k, n in [("c2", 0.013/0.513, 1+2*(1-0.513)/0.513, 0.0910, 0, 4101),
                              ("c3", 0.02/0.42, 1+2*(1-0.42)/0.42, 0.1579, 2, 7472)]:
    print(f"   {name}: sliding {k}/{n} misses, 95% upper {cp_upper(k, n):.1e} | target t/eps {1e-5/e:.1e} | "
          f"shipped {1-p_cur(r,e,201,s2v):.1e} phi {1-p_phi(r,e,201,s2v):.1e} C6 {1-p_palm(r,e,201,s2v):.1e} "
          f"C5 {1-palm_joint_exact(r,e,s2v,201):.1e}")
print("   (c3: 2 misses = 2.7e-4 counted against the 1.6 expected at the target. Neither sample resolves the target.")
print("    Since the sliding miss is a LOWER bound, no form is refuted in the tail.)")
