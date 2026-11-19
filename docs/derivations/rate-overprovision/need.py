"""Exact window loss-mass tail of the netem channel (GE per skb, GSO-batched
datagrams), and the r it needs for a per-window allowance m = P(K_N > R).

usage: python -I need.py
Channel: netem gemodel p, r(=q), 1-h = 100 %, 1-k = 0 % per skb; each skb
carries g datagrams (g mixed between floor/ceil to match the measured GSO
mean). Window: W sources + R repairs = N datagrams; failure iff K_N > R
(paper §4.3, the exact criterion). Prints r(m) = R/W and the honest
variance factor g*sigma2_skb next to the per-datagram GE fit.
"""
import math
import numpy as np

W = 200


def tail(p, q, gmean, n_dgrams, kmax=400):
    g0 = math.floor(gmean); f1 = gmean - g0       # P(g = g0+1) = f1
    n_skb = int(round(n_dgrams / gmean))
    eps = p / (p + q)
    G = np.zeros(kmax + 1); B = np.zeros(kmax + 1)
    G[0], B[0] = 1 - eps, 0.0
    # first skb: stationary
    first = True
    for _ in range(n_skb):
        if first:
            nG, nB = G.copy(), np.zeros(kmax + 1); nB[0] = eps; nG[0] = 1 - eps
            first = False
        else:
            nG = G * (1 - p) + B * q
            nB = G * p + B * (1 - q)
        # a Bad skb loses g datagrams
        lost = np.zeros(kmax + 1)
        lost[g0:] += (1 - f1) * nB[: kmax + 1 - g0]
        lost[g0 + 1:] += f1 * nB[: kmax - g0]
        G, B = nG, lost
    pmf = G + B
    return 1.0 - np.cumsum(pmf)        # P(K > R) for R = 0..kmax


def r_needed(p, q, g, m):
    R = 0
    for _ in range(4):                   # fixed point: N = W + R
        t = tail(p, q, g, W + R)
        R_new = int(np.argmax(t <= m)) if (t <= m).any() else 400
        if R_new == R: break
        R = R_new
    return R / W


if __name__ == "__main__":
    legs = {"c2 leg / c8 p0": (0.013, 0.5, 4.5, 0.0253), "c8 p1": (0.02, 0.4, 3.2, 0.0476)}
    for name, (p, q, g, eps) in legs.items():
        s2skb = 1 + 2 * (1 - p - q) / (p + q)
        ge_fit = 2 * g * (1 - eps) / q - 1
        print(f"{name}: eps {eps:.4f} sigma2_skb {s2skb:.2f} honest g*sigma2 {g*s2skb:.1f} per-datagram GE fit {ge_fit:.1f}")
        print("   m        r_needed   (IT floor eps/(1-eps) = %.4f)" % (eps / (1 - eps)))
        for m in (1.0, 0.5, 0.2, 0.1, 0.05, 0.02, 0.01, 4e-3, 1e-3, 4e-4, 1e-4, 4e-5, 4e-6):
            print(f"   {m:8.1e} {r_needed(p, q, g, m):8.3f}")
