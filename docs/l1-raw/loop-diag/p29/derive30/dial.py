"""W*(delta) at the three named points: sigma1^2(alpha) from alpha.py, window sigma^2(alpha) from win.py at n*."""
import math
pts = {'Realtime': (0.5, 0.05), 'Auto': (1.0, 1 / 15), 'Bulk': (2.0, 0.1)}
s1 = {'c2-100': {0.5: 0.0083, 1: 0.0198, 2: 0.0638}, 'c8-100': {0.5: 0.0146, 1: 0.0195, 2: 0.039},
      'c1s-400': {0.5: 0.0808, 1: 0.1406, 2: 0.3552}, 'c1d-400': {0.5: 0.1058, 1: 0.1813, 2: 0.4432}}
sw = {'c2-100': (13.1, {0.5: 0.00029, 1: 0.00048, 2: 0.0014}), 'c8-100': (5.1, {0.5: 0.00335, 1: 0.00404, 2: 0.00608}),
      'c1s-400': (52.5, {0.5: 0.00505, 1: 0.01139, 2: 0.03642}), 'c1d-400': (49.3, {0.5: 0.00626, 1: 0.01119, 2: 0.02935})}
T = {'c2-100': 19.18, 'c8-100': 49.79, 'c1s-400': 4.79, 'c1d-400': 5.13}
ttr = {'c2-100': 8.87, 'c8-100': 8.28, 'c1s-400': 4.28, 'c1d-400': 5.31}
m = 0.719
for c in s1:
    for nm, (a, q) in pts.items():
        t2 = (q / 2) ** 2
        Ww, d = sw[c]
        s, w = s1[c][a], d[a]
        g = math.log(s / w) / math.log(Ww)
        Wg = (s / t2) ** (1 / g)
        Wwh = Ww * w / t2
        W = Wg if w <= t2 else min(Wg, Wwh)
        per = 2 * (W + 1) * T[c] / 1e3
        reach = ttr[c] / 3 / per * m * math.log(1 + q)
        print(f"{c:8s} {nm:8s} q={q:.4f} s1^2={s:.4f} s_win^2={w:.5f} gamma={g:.2f} W*={W:5.0f} "
              f"({'interp' if w <= t2 else 'beyond'}; pl {Wg:.0f} white {Wwh:.0f}) period={per:5.2f}s "
              f"periods/transfer={ttr[c] / per:5.1f} reach(1/3)={reach:.3f} (x{math.exp(reach):.2f})")
