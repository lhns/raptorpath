import pickle, sys, numpy as np
r = pickle.load(open(sys.argv[1], "rb")); A, B = r["A"], r["B"]
n = min(len(A["st"]), len(B["st"]))
dst = np.abs(A["st"][:n] - B["st"][:n])
first = np.argmax(dst > 1e-9) if (dst > 1e-9).any() else None
print("first seq with differing send time:", first, "at t =", A["st"][first] if first is not None else None)
dg = (A["dl"][:n] >= 0) != (B["dl"][:n] >= 0)
fd = np.argmax(dg) if dg.any() else None
print("first seq with differing delivered flag:", fd, "send t", A["st"][fd] if fd is not None else None)
# per hand-over window share for each, by seq mapping
dt = A["dt"]; units = A["units"]; W = 0.5
nw = int(np.ceil(units * dt / W))
handed = np.bincount(np.minimum((np.arange(units) * dt / W + 1e-9).astype(int), nw - 1), minlength=nw)
def share(X):
    ws = np.minimum((X["h"] / W + 1e-9).astype(int), nw - 1)
    ok = X["dl"] >= 0
    return np.bincount(ws[ok], minlength=nw) / handed, np.bincount(ws, minlength=nw) / handed
sA, nA = share(A); sB, nB = share(B)
for k in range(2, 14):
    print("%4.1f  share A %.4f B %.4f  d %.4f | sent frac A %.4f B %.4f d %.4f" % (k * W, sA[k], sB[k], sA[k] - sB[k], nA[k], nB[k], nA[k] - nB[k]))
