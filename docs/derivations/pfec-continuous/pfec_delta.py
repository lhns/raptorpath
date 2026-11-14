"""Round-6b: the sliding bound's dependence on the trailing offset Delta
(engine: Delta = clamp(ceil(rate*J), 1, 64), emit_source.rs / trailing_offset_delta),
and one row below r_c per window (where shipped reads 0 and plain Phi does not).

Run: python -I pfec_delta.py > pfec_delta.out
"""
import math, os
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
src = open(os.path.join(HERE, "pfec_review.py"), encoding="utf-8").read()
exec(src.rsplit("# ================================================================= output", 1)[0])

print("== D1. Sliding bound vs Delta (eps .048, s2 3.76; 3 seeds, 40k sources)")
for A, r in ((4, 0.34), (7, 0.5)):
    row = []
    for d in (1, 2, 3, 4, 8):
        v = [sliding_decode(0.048, 3.76, r, A, d, T=40_000, seed=s)[0] for s in (7, 11, 13)]
        row.append(f"D{d} {np.mean(v):.4f} ({np.std(v, ddof=1)/math.sqrt(3):.4f})")
    print(f"   A* {A} r {r}: " + "  ".join(row) + f"  | shipped {p_cur(r,.048,A,3.76):.4f}")

print("\n== D2. One row below r_c per window (r = 0.5 r_c): shipped = 0 by the cut, Phi > 0")
for name, e, s2v, A in [("A* 4 pin inputs", 0.048, 3.76, 4), ("W 201 c2", 0.013/0.513, 1+2*(1-0.513)/0.513, 201),
                        ("W 201 c3", 0.02/0.42, 1+2*(1-0.42)/0.42, 201)]:
    r = 0.5 * e / (1 - e)
    T = 40_000 if A < 10 else 30_000
    v = [sliding_decode(e, s2v, r, A, 1, T=T, seed=s)[0] for s in (7, 11, 13)]
    m = float(np.mean(v)); se = float(np.std(v, ddof=1) / math.sqrt(3))
    ph = p_phi(r, e, A, s2v)
    print(f"   {name}: r {r:.4f} shipped {p_cur(r,e,A,s2v):.4f} phi {ph:.4f} C6 {p_palm(r,e,A,s2v):.4f} "
          f"C5 {palm_joint_exact(r,e,s2v,A):.4f} | sliding {m:.4f} (SE {se:.4f}) | phi "
          f"{'REFUTED' if ph > m + 3*se else 'not ordered'}")
