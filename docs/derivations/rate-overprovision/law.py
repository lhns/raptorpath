"""Python port of the shipped rate law (raptorpath-math controller_rate + the
r(beta) mix, fec_rate.rs RateSnapshot::rate), mass term excluded (MassStats
not printed), evaluated at the s51b sweep's measured inputs.

usage: python -I law.py
Port: lib.rs:99-121 compute_r_star_with_z, :735 controller_rate, :840
z_for_tail_target, :885 p_fec_normal, :917 r_saturation, :1082 soft_saturate,
:168 zeta_of_delta, :196 bulkness_of_delta; fec_rate.rs:331-333 tail target.
"""
import math
from scipy.stats import norm

BASE_T = 1e-5          # config.rs:337 target_tail_loss default
MAX_OVH = 0.5          # config.rs:338 max_fec_overhead default
CODEC = 0.004          # fec_rate.rs RLC codec overhead
SYM = 1200.0           # symbol_size (client log)


def zeta(d): return 0.5 / max(d, 1e-12)


def beta(d):
    a, b, x = math.log10(0.5), math.log10(0.005), math.log10(max(d, 1e-12))
    return min(1.0, max(0.0, (a - x) / (a - b)))


def tail_target(d): return min(0.1, max(1e-9, BASE_T * zeta(d)))


def z_for(t, p):
    if p <= 0 or t / p >= 1.0: return -math.inf
    return norm.ppf(1.0 - max(t / p, 1e-15))


def p_fec_normal(r, e, W, s2):
    if W <= 0 or e <= 0 or e >= 1 or r <= 0: return 0.0
    num = r * (1 - e) - e
    if num <= 0: return 0.0
    den = math.sqrt(e * (1 - e) * (r + s2))
    return 1.0 - 0.5 * math.erfc(math.sqrt(W) * num / den / math.sqrt(2))


def r_sat(e, s2, W, srtt, tsym):
    larq, b = 1.5 * srtt, (s2 + 1) / 2
    best, bc = 1.0, math.inf
    for i in range(199):
        r = 0.01 + 0.005 * i
        c = (1 - p_fec_normal(r, e, W, s2)) * larq + b * tsym * (1 + r) / (r * (1 - e)) + 0.5 * (1 + r) * W * tsym
        if c < bc: bc, best = c, r
    return best


def soft_sat(rate, rs):
    s = 0.1 * rs
    x = (rs - rate) / s
    sp = max(x, 0) + math.log1p(math.exp(-abs(x)))
    return max(0.0, rs - s * sp)


def terms(p, s2, W, srtt, tput, t_eff):
    """(core+codec, burst, uncapped, r_sat, capped) of controller_rate, no mass term."""
    if p < 1e-10: return (0, 0, 0, 1, 0)
    codec = CODEC * (1 - (1 - p) ** int(W))
    z = z_for(t_eff, p)
    core = max(0.0, p / (1 - p) + z * math.sqrt(p * s2 / (W * (1 - p)))) if z != -math.inf else 0.0
    rnd = core + codec if core > 0 else 0.0
    B = (s2 + 1) / 2   # mean burst consistent with sigma2 = 2B - 1
    T = max(1.0, srtt * tput / SYM)
    frac = min(1.0, max(0.0, 1 - t_eff / p))
    burst = B / T * frac
    unc = max(rnd, burst)
    rs = r_sat(p, s2, W, srtt, SYM / tput)
    return (rnd, burst, unc, rs, min(MAX_OVH, max(0.0, soft_sat(unc, rs))))


def law(d, p, s2, W, srtt, tput):
    b = beta(d)
    ra = terms(p, s2, W, srtt, tput, tail_target(d))
    rb = terms(p, s2, W, srtt, tput, p)       # late-is-fine, chi = 0: delta_eff = p
    return (1 - b) * ra[4] + b * rb[4], ra


# per-arm medians from inputs.py (plu_w, rtt_w ms, rr_med); tput = the leg's wire rate
C2 = dict(tput=100e6 / 8, W=201, arms=[
    ("I1", 0.0075, 0.0150, 206.5, 0.0107), ("I2", 0.0158, 0.0173, 210.5, 0.0350),
    ("I3", 0.05, 0.0183, 221.0, 0.0840), ("AM", 0.49, 0.0187, 244.5, 0.2245),
    ("HA", 0.5, 0.0193, 244.75, 0.2410), ("AP", 0.51, 0.0199, 245.0, 0.2290),
    ("I4", 5.0, 0.0195, 249.0, 0.2585), ("RM", 49.0, 0.0210, 251.0, 0.2595), ("RP", 51.0, 0.0192, 251.0, 0.2450)])
C8 = dict(tput=20e6 / 8, W=201, arms=[
    ("I1", 0.0075, 0.0306, 268.25, 0.0180), ("I2", 0.0158, 0.0386, 265.75, 0.0600),
    ("I3", 0.05, 0.0369, 429.0, 0.1115), ("AM", 0.49, 0.0367, 563.75, 0.2670),
    ("HA", 0.5, 0.0394, 575.75, 0.2550), ("AP", 0.51, 0.0381, 586.0, 0.2800),
    ("I4", 5.0, 0.0379, 599.25, 0.2850), ("RM", 49.0, 0.0378, 564.0, 0.2790), ("RP", 51.0, 0.0363, 593.5, 0.2800)])


def fit(cell, s2grid):
    best = None
    for s2 in s2grid:
        err = sum((law(d, p, s2, cell["W"], rtt / 1e3, cell["tput"])[0] - rr) ** 2 for _, d, p, rtt, rr in cell["arms"])
        if best is None or err < best[1]: best = (s2, err)
    return best


def table(name, cell, s2):
    print(f"\n{name}: sigma2 = {s2:.1f}")
    print("arm   delta   beta   t_tail     plu  srtt  core+cod  burst  r_sat  r_anchor  law r  rr_med  pressure")
    for a, d, p, rtt, rr in cell["arms"]:
        r, ra = law(d, p, s2, cell["W"], rtt / 1e3, cell["tput"])
        press = 1 / (1 + math.exp(-(ra[2] - ra[3]) / (0.1 * ra[3])))
        print(f"{a:3} {d:7.4f} {beta(d):6.3f} {tail_target(d):8.1e} {p:7.4f} {rtt/1e3:5.3f} {ra[0]:8.3f} {ra[1]:6.3f} "
              f"{ra[3]:6.3f} {ra[4]:8.3f} {r:6.3f} {rr:7.4f} {press:8.2f}")


if __name__ == "__main__":
    grid = [float(x) for x in range(1, 100)]
    for name, cell in (("c2", C2), ("c8 (worst leg p1)", C8)):
        s2, err = fit(cell, grid)
        table(name, cell, s2)
        print(f"fit rms {math.sqrt(err / len(cell['arms'])):.4f}")
        for s2x in (13.0, 16.4, 18.6) if name == "c2" else (6.0, 8.6, 10.6):
            table(name + " (fixed)", cell, s2x)
