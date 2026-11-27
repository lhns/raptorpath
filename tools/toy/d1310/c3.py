import pickle, sys, numpy as np
r = pickle.load(open(sys.argv[1], "rb")); A, B = r["A"], r["B"]
h0, h1 = float(sys.argv[2]), float(sys.argv[3])
lam = 12000.0
for nm, X in (("A", A), ("B'", B)):
    m = (X["h"] >= h0) & (X["h"] < h1)
    ok = X["dl"] >= 0
    sent = m.sum(); dl = (m & ok).sum(); handed = lam * (h1 - h0)
    st = X["st"][m]
    print("%s handed %.0f first-sent %d (frac %.5f) delivered %d (share %.5f); their send times [%.3f, %.3f] -> first-send rate %.1f/s; resent %d" % (
        nm, handed, sent, sent / handed, dl, dl / handed, st.min(), st.max(), sent / (st.max() - st.min()), (X["copies"][m] > 0).sum()))
# first sends per second of send time, both
for nm, X in (("A", A), ("B'", B)):
    st = X["st"]
    print(nm, "first sends per s:", [int(((st >= k) & (st < k + 1)).sum()) for k in range(8, 20)])
