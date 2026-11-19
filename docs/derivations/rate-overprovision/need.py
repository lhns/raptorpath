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


def pmf(p, q, g, n_dgrams, kmax=400):
    t = tail(p, q, g, n_dgrams, kmax)
    c = 1.0 - t
    return np.diff(np.concatenate([[0.0], c]))


def r_needed_2leg(legs, shares, m, kmax=400):
    """Window mass summed over two legs carrying `shares` of the N slots."""
    R = 0
    for _ in range(6):
        N = W + R
        tot = np.array([1.0])
        for (p, q, g), s in zip(legs, shares):
            tot = np.convolve(tot, pmf(p, q, g, s * N, kmax))[: kmax + 1]
        t = 1.0 - np.cumsum(tot)
        R_new = int(np.argmax(t <= m)) if (t <= m).any() else kmax
        if R_new == R:
            break
        R = R_new
    return R / W


def selfcheck():
    """Paper §4.7: exact r* = 0.170 at WiFi (eps 2.5 %, sigma2 2.9 -> p 0.0128, q 0.5),
    W = 50, t/eps = 1e-2, per-symbol GE (g = 1)."""
    global W
    w0, W = W, 50
    r = r_needed(0.0128, 0.5, 1.0, 1e-2)
    W = w0
    return r


if __name__ == "__main__":
    print(f"self-check vs paper 4.7 exact r* 0.170 (WiFi, W=50, m=1e-2, g=1): {selfcheck():.3f}")
    legs = {"c2 leg / c8 p0": (0.013, 0.5, 4.5, 0.0253), "c8 p1": (0.02, 0.4, 3.2, 0.0476)}
    ms = (1.0, 0.91, 0.67, 0.5, 0.21, 0.194, 0.1, 0.091, 0.082, 0.05, 0.021, 0.0194, 0.01, 0.0091,
          4e-3, 2.1e-3, 1e-3, 8.2e-4, 4e-4, 2.1e-4, 1e-4, 8.2e-5, 4e-6)
    for name, (p, q, g, eps) in legs.items():
        s2skb = 1 + 2 * (1 - p - q) / (p + q)
        ge_fit = 2 * g * (1 - eps) / q - 1
        print(f"{name}: eps {eps:.4f} sigma2_skb {s2skb:.2f} honest g*sigma2 {g*s2skb:.1f} per-datagram GE fit {ge_fit:.1f}")
        print("   m        r_needed   (IT floor eps/(1-eps) = %.4f)" % (eps / (1 - eps)))
        for m in ms:
            print(f"   {m:8.1e} {r_needed(p, q, g, m):8.3f}")
    print("c8 two-leg window (shares p0 0.82 / p1 0.18 of N, the [TRUTH] egress split):")
    for m in ms:
        print(f"   {m:8.1e} {r_needed_2leg([(0.013, 0.5, 5.4), (0.02, 0.4, 3.2)], (0.82, 0.18), m):8.3f}")
