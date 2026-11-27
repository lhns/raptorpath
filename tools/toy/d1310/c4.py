"""Witness: first-sent fraction of the units handed over, per 0.05 s of hand-over time, A and B' (one seed), around onset - c/x."""
import pickle, sys, numpy as np
r = pickle.load(open(sys.argv[1], "rb")); A, B = r["A"], r["B"]
lo, hi, W = float(sys.argv[2]), float(sys.argv[3]), 0.05
lam = 12000.0
print("onset (B')", B["rev4"]["onset_t"])
edges = np.arange(lo, hi + 1e-9, W)
for nm, X in (("A", A), ("B'", B)):
    c, _ = np.histogram(X["h"], bins=edges)
    print(nm, " ".join("%.3f" % v for v in c / (lam * W)))
print("t ", " ".join("%5.2f" % v for v in edges[:-1]))
