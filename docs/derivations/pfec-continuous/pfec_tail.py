"""Round-5 check: does the block law's tail (the r* finding) survive the
sliding-window decoder? Sliding generic-rank decode (no deadline, upper bound
on recovery) at the c2 / c3 nominal L1 inputs, span A = 201 (the measured L1
window), r = the closed-form Auto r* (pfec_review.out R3b).

Run: python -I pfec_tail.py > pfec_tail.out
"""
import math, os
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
src = open(os.path.join(HERE, "pfec_review.py"), encoding="utf-8").read()
exec(src.rsplit("# ================================================================= output", 1)[0])

for name, e, s2, r in [("c2", 0.013 / 0.513, 1 + 2 * (1 - 0.513) / 0.513, 0.0910),
                       ("c3", 0.02 / 0.42, 1 + 2 * (1 - 0.42) / 0.42, 0.1579)]:
    vals = []
    holes = 0
    for sd in (7, 11):
        v, n = sliding_decode(e, s2, r, 201, 1, T=80_000, seed=sd)
        vals.append(v); holes += n
    miss = 1 - float(np.mean(vals))
    se = math.sqrt(max(miss * (1 - miss), 1e-12) / holes)
    print(f"{name}: eps {e:.4f} s2 {s2:.3f} A 201 r {r}: sliding per-loss miss {miss:.2e} "
          f"(binomial SE {se:.1e}, {holes} holes)  C6 miss {1 - p_palm(r, e, 201, s2):.2e}  "
          f"shipped miss {1 - p_cur(r, e, 201, s2):.2e}  target t/eps {1e-5 / e:.1e}")
