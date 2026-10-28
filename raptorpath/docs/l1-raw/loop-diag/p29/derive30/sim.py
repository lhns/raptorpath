"""Window W*, decision time, and mean-field convergence/goodput of D with W* rounds per half.

Inputs are transcribed from pcap.py (sigma1^2 = var(Delta_1)/2, pooled over live PWR rows),
win.py (sigma^2 over 250 ms windows at the static cap nearest n*, T_med per cap) and the
p27 DEF static goodput curve; seeds from the p29 [PCAP] period-0 nbar and MAIN win_den_med."""
import math, sys
from statistics import NormalDist

Z = NormalDist()
q = 0.1
L = math.log((1 + q / 2) / (1 - q / 2))
target = q / 2


def m_step(e, se):
    """E[clamp(e + se*Z, -1, 1)] by quadrature."""
    n = 4000
    s = 0.0
    for i in range(n):
        z = Z.inv_cdf((i + 0.5) / n)
        s += max(-1.0, min(1.0, e + se * z))
    return s / n


def interp(curve, n):
    xs = sorted(curve)
    if n <= xs[0]:
        # below the grid: T flat (unloaded), G proportional to n
        return curve[xs[0]]
    if n >= xs[-1]:
        return curve[xs[-1]]
    for a, b in zip(xs, xs[1:]):
        if a <= n <= b:
            w = math.log(n / a) / math.log(b / a)
            return math.exp((1 - w) * math.log(curve[a]) + w * math.log(curve[b]))


CELLS = {
    # sigma1^2 (W=1), (W_win, sigma^2_win) at n*, n*, T(n) ms, G(n) Mbit/s, seeds, MAIN caps, bytes, MAIN goodput, band
    "c2-100": dict(s1=(0.1243 + 0.1289 + 0.1300) / 6, win=(13.1, 0.00137), nstar=192,
                   T={64: 14.87, 96: 15.44, 128: 16.06, 192: 19.18, 256: 25.47, 384: 38.28, 512: 51.25, 1024: 103.1},
                   G={64: 37.9, 96: 54.4, 128: 69.0, 192: 88.8, 256: 90.3, 384: 88.5, 512: 89.9, 1024: 90.3},
                   seeds=[112, 150, 306], main=[634, 1100, 405], B=100e6, gmain=90.2, band=4.7),
    "c1s-400": dict(s1=(0.6804 + 0.7354 + 0.7124) / 6, win=(52.5, 0.03642), nstar=384,
                    T={256: 3.89, 384: 4.79, 512: 5.80, 768: 8.46, 1024: 10.03, 1536: 15.25, 2048: 22.17},
                    G={256: 529.3, 384: 632.0, 512: 674.8, 768: 701.4, 1024: 740.6, 1536: 745.9, 2048: 721.7},
                    seeds=[55, 43, 62], main=[2048, 2010, 1802], B=400e6, gmain=748.3, band=92.1),
    "c1d-400": dict(s1=(0.903 + 0.7655 + 0.9863) / 6, win=(49.3, 0.02935), nstar=384,
                    T={384: 5.13, 512: 6.42, 768: 8.21, 1024: 11.99, 1536: 20.08, 2048: 24.61},
                    G={384: 604.3, 512: 633.6, 768: 699.8, 1024: 693.6, 1536: 675.2, 2048: 685.3},
                    seeds=[42, 63, 164], main=[1509, 2300, 1732], B=400e6, gmain=602.9, band=110.0),
    "c8-100": dict(s1=(0.1553 * 36 + 0.04194 * 44 + 0.03664 * 29) / (36 + 44 + 29) / 2, win=(5.1, 0.00608), nstar=384,
                   T={128: 46.0, 192: 45.35, 384: 49.79, 512: 66.96, 768: 92.19, 1024: 162.7, 2048: 306.2},
                   G={128: 65.4, 192: 79.0, 384: 102.9, 512: 102.0, 768: 102.3, 1024: 100.9, 2048: 99.0},
                   seeds=[158, 141, 335], main=[2290, 1325, 1568], B=100e6, gmain=96.6, band=12.9),
}


def G_of(c, n):
    xs = sorted(c["G"])
    if n < xs[0]:
        return c["G"][xs[0]] * n / xs[0]  # below the grid: D proportional to n (store-bound, T flat)
    return interp(c["G"], n)


def T_of(c, n):
    xs = sorted(c["T"])
    if n < xs[0]:
        return min(c["T"].values())
    return interp(c["T"], n)


def run(c, n0, W, se):
    """Mean-field: expected clamped step per period; returns (t_conv, t_total, goodput)."""
    n, t, bytes_ = n0, 0.0, 0.0
    t_conv = None
    lnstep = math.log(1 + q)
    while bytes_ < c["B"]:
        Tn = T_of(c, n) / 1e3
        per = 2 * (W + 1) * Tn
        rate = G_of(c, n) * 1e6 / 8
        if bytes_ + rate * per >= c["B"]:
            t += (c["B"] - bytes_) / rate
            bytes_ = c["B"]
        else:
            bytes_ += rate * per
            t += per
        x = math.log(n / c["nstar"])
        if t_conv is None and abs(x) <= lnstep:
            t_conv = t
        if t_conv is None:
            e = 2.0 if x < 0 else -1.0  # below the knee e ~ alpha, above ~ -1 (paper 6.1)
            n *= math.exp(lnstep * m_step(e, se))
        if t > 600:
            break
    return t_conv, t, c["B"] * 8 / t / 1e6


for name, c in CELLS.items():
    s1 = c["s1"]
    Ww, sw = c["win"]
    gamma = math.log(s1 / sw) / math.log(Ww)
    target2 = target ** 2
    Wg = (s1 / target2) ** (1 / gamma)
    Wwh = Ww * sw / target2
    if sw <= target2:
        Wstar, how = Wg, "interp inside data"
    else:
        Wstar, how = min(Wg, Wwh), f"beyond data: power-law {Wg:.0f}, white-from-window {Wwh:.0f}; min used"
    W_white1 = s1 / target2
    se = math.sqrt(2) * target / L
    Tst = T_of(c, c["nstar"]) / 1e3
    tdec = 2 * (Wstar + 1) * Tst
    t_main = c["B"] * 8 / (c["gmain"] * 1e6)
    print(f"== {name}: sigma1^2={s1:.4f} sigma1={math.sqrt(s1):.3f}  sigma^2_win={sw:.5f}@W={Ww}  gamma={gamma:.2f}")
    print(f"   W* = {Wstar:.0f} rounds ({how}); white-from-W=1 would give {W_white1:.0f};"
          f" SE(e)={se:.3f}; m(+2)={m_step(2, se):.3f} m(-1)={m_step(-1, se):.3f}")
    print(f"   period at n* = {tdec:.2f} s (T(n*)={Tst*1e3:.1f} ms); MAIN transfer {t_main:.2f} s"
          f" -> {t_main/tdec:.1f} periods per transfer; static G(n*)={G_of(c, c['nstar']):.1f} vs MAIN {c['gmain']} band {c['band']}")
    for lab, seeds in (("p29 seed", c["seeds"]), ("MAIN cap seed", c["main"])):
        for n0 in seeds:
            tc, tt, g = run(c, n0, Wstar, se)
            lam = abs(math.log(n0 / c["nstar"]))
            tcs = f"{tc:.2f}s" if tc else "never"
            print(f"   {lab} n0={n0}: Lambda={lam:.2f} conv {tcs} of transfer {tt:.2f}s"
                  f" ({(tc or tt)/tt*100:.0f} %), goodput {g:.1f} (MAIN {c['gmain']}, diff {g-c['gmain']:+.1f}, band {c['band']})")
    # one-round (as built) sign stats
    for e in (2.0, -1.0):
        p = Z.cdf(abs(e) * L / (math.sqrt(2 * s1)))
        print(f"   W=1, e={e:+.0f}: P(correct sign)={p:.3f}, sign agreement={p*p+(1-p)*(1-p):.3f}")
