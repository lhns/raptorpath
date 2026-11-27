import pickle, sys, numpy as np
r = pickle.load(open(sys.argv[1], "rb")); A, B = r["A"], r["B"]
lo_t, hi_t = float(sys.argv[2]), float(sys.argv[3])
for nm, X in (("A", A), ("B", B)):
    m = (X["st"] >= lo_t) & (X["st"] < hi_t) & (X["copies"] == 0)
    ok = X["dl"] >= 0
    got = X["got"] > 0
    print(nm, "seqs %d  delivered %d  got %d  not-delivered %d  frac %.5f" % (m.sum(), (m & ok).sum(), (m & got).sum(), (m & ~ok).sum(), (m & ok).sum() / m.sum()))
