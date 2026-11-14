"""Numerical check for docs/derivations/pfec-continuous.md.

Compares, over the operating grid, the forms of P_fec:
  cur   : raptorpath_math::p_fec_normal as shipped (cut to 0 at r(1-e) <= e)
  phi   : the paper's plain Phi form (sec 4.2), no cut
  ccn   : continuity-corrected normal on the per-loss (Palm) event
  win   : exact discrete WINDOW event P(C >= K) (the paper's event, iid)
  palm  : RECOMMENDED -- exact per-loss event, GE loss count from (e, s2),
          dithered repair count, iid repair erasure (the paper's assumption)
plus a Monte Carlo oracle with GE-correlated repair erasures, the r_saturation
argmin under cur vs palm, and the engine pin's assertions.

Run: python -I pfec_check.py > pfec_check.out   (numpy, scipy; no engine code)
"""
import math
import numpy as np
from scipy.stats import binom

# ---------------------------------------------------------------- shipped forms
def nsurv(z):  # normal_survival
    return 0.5 * math.erfc(z / math.sqrt(2.0))

def p_cur(r, e, W, s2):  # raptorpath-math/src/lib.rs:885-893, verbatim logic
    if W <= 0 or e <= 0 or e >= 1 or r <= 0:
        return 0.0
    num = r * (1 - e) - e
    if num <= 0:
        return 0.0
    den = math.sqrt(e * (1 - e) * (r + s2))
    return 1.0 - nsurv(math.sqrt(W) * num / den)

def p_phi(r, e, W, s2):  # paper sec 4.2 (fec-arq-model.md:699), no cut
    if W <= 0 or e <= 0 or e >= 1:
        return 0.0
    num = r * (1 - e) - e
    den = math.sqrt(e * (1 - e) * (r + s2))
    return 1.0 - nsurv(math.sqrt(W) * num / den)

def p_ccn(r, e, W, s2):
    """Per-loss event, normal with continuity correction:
    P(C - K' >= 1) ~ Phi((rW(1-e) - (W-1)e - 1/2) / sqrt(rWe(1-e) + (W-1)e(1-e)s2))."""
    mu = r * W * (1 - e) - (W - 1) * e - 0.5
    var = r * W * e * (1 - e) + (W - 1) * e * (1 - e) * s2
    return 1.0 - nsurv(mu / math.sqrt(var))

# ---------------------------------------------------------------- GE count law
def ge_pq(e, s2):
    """(e, s2) -> GE (p, q), loss iff Bad (netem gemodel 1-h 100%, 1-k 0%).
    s2 = 1 + 2(1-p-q)/(p+q)  =>  p+q = 2/(s2+1);  e = p/(p+q)."""
    t = 2.0 / (s2 + 1.0)
    return e * t, (1 - e) * t

_kcache = {}
def k_pmf(e, s2, W):
    """pmf of the source-loss count K over W consecutive symbols of the
    stationary GE chain (exact DP, O(W^2)); s2 = 1 gives Binomial(W, e)."""
    key = (e, s2, W)
    if key in _kcache:
        return _kcache[key]
    p, q = ge_pq(e, s2)
    g = np.zeros(W + 1); b = np.zeros(W + 1)
    g[0] = 1 - e; b[1] = e              # slot 1: Good (no loss) / Bad (loss)
    for _ in range(W - 1):
        ng = g * (1 - p) + b * q
        nb = np.zeros(W + 1)
        nb[1:] = (g * p + b * (1 - q))[:-1]
        g, b = ng, nb
    pmf = g + b
    _kcache[key] = pmf
    return pmf

def G(n, e, s2, W):
    """Per-loss recovery with exactly n repairs: E[K 1{Bin(n,1-e) >= K}] / E[K]."""
    pmf = k_pmf(e, s2, W)
    k = np.arange(W + 1)
    surv = binom.sf(k - 1, n, 1 - e)    # P(Bin(n,1-e) >= k); 0 for k > n
    return float((k * pmf * surv).sum() / (k * pmf).sum())

def p_palm(r, e, W, s2):
    """RECOMMENDED. Repair count N = floor(rW + U), U ~ U[0,1) (debt accrual
    with uniform phase): N = n0 w.p. 1-f, n0+1 w.p. f, n0 = floor(rW)."""
    if W <= 0 or e <= 0 or e >= 1 or r <= 0:
        return 0.0  # e, W out of domain; r <= 0 is the value of the formula (G(0) = 0)
    x = r * W
    n0 = math.floor(x); f = x - n0
    return (1 - f) * G(n0, e, s2, W) + f * G(n0 + 1, e, s2, W)

def p_win(r, e, W, s2):
    """The paper's WINDOW event P(C >= K), exact iid, same dithered count."""
    pmf = k_pmf(e, s2, W); k = np.arange(W + 1)
    x = r * W; n0 = math.floor(x); f = x - n0
    s = lambda n: float((pmf * binom.sf(k - 1, n, 1 - e)).sum())
    return (1 - f) * s(n0) + f * s(n0 + 1)

FORMS = {"cur": p_cur, "phi": p_phi, "ccn": p_ccn, "palm": p_palm}

# ---------------------------------------------------------------- r_saturation
def r_sat(e, s2, W, srtt, tsym, pf):  # raptorpath-math/src/lib.rs:917-944
    l_arq = 1.5 * srtt; b = (s2 + 1) / 2
    best_r, best = 1.0, math.inf
    for i in range(199):
        r = 0.01 + 0.005 * i
        c = (1 - pf(r, e, W, s2)) * l_arq + b * tsym * (1 + r) / (r * (1 - e)) \
            + 0.5 * (1 + r) * W * tsym
        if c < best:
            best, best_r = c, r
    return best_r

def soft_sat(rate, rs):  # lib.rs:1083-1093
    s = 0.1 * rs
    sp = lambda x: max(x, 0) + math.log1p(math.exp(-abs(x)))
    return max(rs - s * sp((rs - rate) / s), 0.0)

# ---------------------------------------------------------------- Monte Carlo
def mc_palm(r, e, s2, W, trials=200_000, seed=1):
    """Oracle: one GE chain over the interleaved wire sequence of W sources
    and N = floor(rW+U) repairs (repairs evenly spaced, as p_fec_exact),
    repair erasures GE-correlated. Per-loss recovery = sum K 1{C>=K} / sum K."""
    rng = np.random.default_rng(seed)
    p, q = ge_pq(e, s2)
    x = r * W; n0 = math.floor(x); f = x - n0
    num = den = 0.0
    for n in (n0, n0 + 1):
        m = int(round(trials * ((1 - f) if n == n0 else f)))
        if m == 0:
            continue
        N = W + n
        is_rep = np.array([((i + 1) * n) // N > (i * n) // N for i in range(N)]) if n else np.zeros(N, bool)
        bad = rng.random(m) < e
        K = np.zeros(m); C = np.zeros(m)
        for i in range(N):
            if i:
                u = rng.random(m)
                bad = np.where(bad, u >= q, u < p)
            if is_rep[i]:
                C += ~bad
            else:
                K += bad
        num += (K * (C >= K)).sum(); den += K.sum()
    return num / den if den else float("nan")

# ================================================================= output
EPS = [0.005, 0.025, 0.05, 0.1, 0.2, 0.3]
# Windows: A* 3-5 (fec_rate/tests.rs:277 fixture; emit_source.rs:578-580),
# 64 (sim golden w and the r_sat test fixtures), 200 (engine MAX_WINDOW_SIZE,
# net/mod.rs:205: the encoder window r_saturation reads).
WS = [3, 4, 5, 64, 200]
# s2: 1 = iid floor (math burst_variance_factor :91), 2.9 c2 GE 1.3/50,
# 3.0 pin + c1 GE 0.05/50, 3.76 c3 GE 2/40 (pin ladder), 5.0 r_sat C4 fixture.
S2 = [1.0, 2.9, 3.0, 3.76, 5.0]

def grid(h):
    return [round(i * h, 10) for i in range(int(round(1 / h)) + 1)]

print("== 1. Values at r = 0 (the engine pin: P_fec(0) = 0 => residual = eps)")
print(f"{'eps':>6} {'W':>4} {'s2':>5} | {'cur':>8} {'phi':>8} {'ccn':>8} {'win':>8} {'palm':>8}")
for e, W, s2 in [(0.048, 5, 3.0), (0.048, 4, 3.76), (0.025, 64, 2.9), (0.05, 200, 3.0), (0.1, 64, 1.0)]:
    vals = [p_cur(0, e, W, s2), p_phi(0, e, W, s2), p_ccn(0, e, W, s2), p_win(0, e, W, s2), p_palm(0, e, W, s2)]
    print(f"{e:6.3f} {W:4d} {s2:5.2f} | " + " ".join(f"{v:8.4f}" for v in vals))

print("\n== 2. The engine pin, every assertion (fec_rate/tests.rs:262-282), residual = eps(1-P)")
eps = 0.048
for name, pf in FORMS.items():
    R0 = eps * (1 - pf(0.0, eps, 5, 3.0))
    ladder = [eps * (1 - pf(r, eps, 5, 3.76)) for r in [0.05, 0.1, 0.2, 0.34, 0.5, 1.0]]
    mono = all(b <= a + 1e-12 for a, b in zip([R0] + ladder, ladder))
    c3 = eps * (1 - pf(0.34, eps, 4, 3.76))
    ok = abs(R0 - eps) < 1e-12 and mono and 0.001 < c3 < eps
    print(f"{name:>5}: R(0)={R0:.6f} (want {eps}) ladder={[round(v,5) for v in ladder]} "
          f"mono={mono} R(0.34,A*4)={c3:.5f} in (0.001,{eps}) -> {'PASS' if ok else 'FAIL'}")

print("\n== 3. Max neighbour jump |P(r+h)-P(r)| over r in [0,1], all (eps, W, s2) in the grid")
print("   (a law continuous in r has jump -> 0 linearly in h: ratio h=0.02/h=0.01 ~ 2; a step keeps ratio ~ 1)")
for name, pf in FORMS.items():
    worst = {}
    for h in (0.02, 0.01, 0.005):
        mj, arg = 0.0, None
        for e in EPS:
            for W in WS:
                for s2 in S2:
                    rs = grid(h) + [e / (1 - e)]
                    rs = sorted(set(rs + [e / (1 - e) - 1e-9, e / (1 - e) + 1e-9]))
                    v = [pf(r, e, W, s2) for r in rs]
                    # neighbour on the h-grid: compare r and r+h (the crossing points use +/- 1e-9 too)
                    for r in rs:
                        a, b = pf(r, e, W, s2), pf(min(r + h, 1.0), e, W, s2)
                        if abs(b - a) > mj:
                            mj, arg = abs(b - a), (e, W, s2, round(r, 6))
        worst[h] = (mj, arg)
    print(f"{name:>5}: " + "  ".join(f"h={h}: {m:.4f} at (eps,W,s2,r)={a}" for h, (m, a) in worst.items())
          + f"  ratio(.02/.01)={worst[0.02][0]/worst[0.01][0]:.2f}")

print("\n   per-W max jump at h = 0.02 (palm | cur | phi)")
for W in WS:
    row = []
    for pf in (p_palm, p_cur, p_phi):
        m = 0.0
        for e in EPS:
            for s2 in S2:
                rs = sorted(set(grid(0.02) + [e / (1 - e) - 1e-9]))
                m = max(m, max(abs(pf(min(r + 0.02, 1), e, W, s2) - pf(r, e, W, s2)) for r in rs))
        row.append(m)
    print(f"   W={W:4d}: palm {row[0]:.4f} | cur {row[1]:.4f} | phi {row[2]:.4f}")

print("\n   jump across the crossing r_c = e/(1-e), P(r_c+1e-9) - P(r_c-1e-9), worst over the grid")
for name, pf in FORMS.items():
    m = max(abs(pf(e / (1 - e) + 1e-9, e, W, s2) - pf(e / (1 - e) - 1e-9, e, W, s2))
            for e in EPS for W in WS for s2 in S2)
    print(f"   {name:>5}: {m:.2e}")

print("\n== 4. Monotone non-decreasing in r on the h = 0.005 grid (worst decrease)")
for name, pf in FORMS.items():
    worst = 0.0
    for e in EPS:
        for W in WS:
            for s2 in S2:
                v = [pf(r, e, W, s2) for r in grid(0.005)]
                worst = min(worst, min(b - a for a, b in zip(v, v[1:])))
    print(f"   {name:>5}: {worst:.2e}")

print("\n== 5. Continuity in eps and s2 (palm): max |dP| for a 0.1 % relative nudge, over the grid")
m_e = m_s = 0.0
for e in EPS:
    for W in WS:
        for s2 in S2:
            for r in grid(0.05):
                m_e = max(m_e, abs(p_palm(r, e * 1.001, W, s2) - p_palm(r, e, W, s2)))
                m_s = max(m_s, abs(p_palm(r, e, W, s2 * 1.001) - p_palm(r, e, W, s2)))
print(f"   eps nudge: {m_e:.2e}   s2 nudge: {m_s:.2e}")

print("\n== 6. Limits (palm)")
for W in (4, 64):
    print(f"   eps -> 0 (1e-6), s2 1, W {W}: " + " ".join(
        f"r={r}: {p_palm(r, 1e-6, W, 1.0):.4f} (min(1,rW)={min(1, r*W):.4f})" for r in (0.005, 0.01, 0.1, 0.5)))
print("   eps -> 0 (1e-6), s2 3.76, W 64: " + " ".join(
    f"r={r}: {p_palm(r, 1e-6, 64, 3.76):.4f}" for r in (0.01, 0.02, 0.05, 0.1)))
print("   eps -> 1 (0.999), W 64, s2 3: " + " ".join(f"r={r}: {p_palm(r, 0.999, 64, 3.0):.2e}" for r in (0.5, 1.0)))
print("   s2 dependence (eps .05, W 64, r .1): " + " ".join(f"s2={s}: {p_palm(0.1, 0.05, 64, s):.4f}" for s in S2))

print("\n== 7. Agreement with the normal form where it is valid (large W r), |palm - phi|")
for e, W, s2 in [(0.025, 200, 2.9), (0.05, 200, 3.0), (0.1, 200, 3.0), (0.05, 64, 3.0), (0.1, 64, 5.0)]:
    rc = e / (1 - e)
    row = []
    for r in (rc, 1.5 * rc, 2 * rc, 3 * rc):
        row.append(f"r={r:.3f}: palm {p_palm(r, e, W, s2):.4f} phi {p_phi(r, e, W, s2):.4f}")
    print(f"   eps {e} W {W} s2 {s2}: " + " | ".join(row))
# far tail where r* lives: 1-P at large margins
print("   tail 1-P at r = rc + 4 sd (sd = sqrt(e s2/(W(1-e)))):")
for e, W, s2 in [(0.025, 200, 2.9), (0.05, 200, 3.0), (0.05, 64, 3.0)]:
    r = e / (1 - e) + 4 * math.sqrt(e * s2 / (W * (1 - e)))
    print(f"     eps {e} W {W} s2 {s2} r {r:.3f}: 1-palm {1-p_palm(r,e,W,s2):.2e}  1-phi {1-p_phi(r,e,W,s2):.2e}")

print("\n== 8. Monte Carlo oracle (GE-correlated repair erasures; 2e5 windows) vs palm / phi / cur")
for e, s2, W, r in [(0.048, 3.76, 4, 0.34), (0.048, 3.76, 5, 0.2), (0.048, 3.0, 5, 0.0),
                    (0.025, 2.9, 64, 0.0267), (0.025, 2.9, 64, 0.06), (0.05, 3.0, 64, 0.1),
                    (0.05, 3.0, 200, 0.0526), (0.1, 5.0, 64, 0.2)]:
    mc = mc_palm(r, e, s2, W)
    print(f"   eps {e} s2 {s2} W {W} r {r}: MC {mc:.4f}  palm {p_palm(r,e,W,s2):.4f}  "
          f"phi {p_phi(r,e,W,s2):.4f}  cur {p_cur(r,e,W,s2):.4f}")

print("\n== 9. r_saturation argmin under cur vs palm (lib.rs:917; tests lib.rs:1449-1471)")
cases = [
    ("C4 fixture (band [0.2,0.45])", 0.09, 5.0, 64, 0.21, 1225 / 2.5e6),
    ("C2 fixture (> 0.2)", 0.025, 3.0, 64, 0.013, 1e-4),
    ("fec_rate test :410 style, eps .05 s2 3 W 64", 0.05, 3.0, 64, 0.05, 1200 / 1.25e6),
    # Nominal L1 inputs (NOT estimator readings): eps, s2 from the netem GE
    # (tools/l1/lib.sh:180-182), W = 200 encoder cap, srtt = 2 x one-way,
    # t_sym = 1200 B at the link rate.
    ("c1s nominal", 0.0005 / 0.5005, 1 + 2 * (1 - 0.5005) / 0.5005, 200, 0.002, 1200 * 8 / 1e9),
    ("c2 nominal", 0.013 / 0.513, 1 + 2 * (1 - 0.513) / 0.513, 200, 0.010, 1200 * 8 / 1e8),
    ("c3 (c8 slow leg) nominal", 0.02 / 0.42, 1 + 2 * (1 - 0.42) / 0.42, 200, 0.040, 1200 * 8 / 2e7),
    # Sim golden cells (raptorpath-wasm lib.rs:1843-1848): w 64, TICK 1 ms,
    # capacity 4 => t_sym 2.5e-4 s; srtt = rtt; s2 from (eps, q).
    ("sim golden eps .05 q .5 rtt 50", 0.05, None, 64, 0.050, 2.5e-4),
    ("sim golden eps .10 q .3 rtt 80", 0.10, None, 64, 0.080, 2.5e-4),
]
for name, e, s2, W, srtt, tsym in cases:
    if s2 is None:
        qq = 0.5 if e == 0.05 else 0.3
        pp = e * qq / (1 - e)
        s2 = 1 + 2 * (1 - pp - qq) / (pp + qq)
    a, b = r_sat(e, s2, W, srtt, tsym, p_cur), r_sat(e, s2, W, srtt, tsym, p_palm)
    print(f"   {name}: eps {e:.4f} s2 {s2:.3f} W {W}: r_sat cur {a:.3f}  palm {b:.3f}")

print("\n== 10. L1 cells run Bulk (beta = 1, chi = 0): pre-cap rate = 0 => soft_saturate(0, r_sat) for every r_sat")
print("   max over r_sat in [0.01, 1] of soft_saturate(0, r_sat): "
      f"{max(soft_sat(0.0, 0.01 + 0.005 * i) for i in range(199)):.1e}")

print("\n== 11. Tests whose expectations read p_fec_normal (values under cur / palm)")
for (r, e, W, s2, what) in [
    (0.15, 0.1, 50, 1.0, "engine_fold_pins.rs:41 bit pin"),
    (0.15, 0.1, 50, 3.0, "engine_fold_pins.rs:42 bit pin"),
    (0.12, 0.1, 50, 1.0, "engine_fold_pins.rs:43 bit pin; fec_rate/tests.rs:1143 (0 < p2 < p)"),
    (0.2, 0.025, 384, 1.5, "engine_fold_pins.rs:44 bit pin (== 1.0)"),
    (0.1 / 0.9 + 0.001, 0.1, 50, 3.0, "formula_verification.rs:143 (0.4 < p < 0.7)"),
    (0.5, 0.1, 50, 3.0, "formula_verification.rs:147 (> 0.99)"),
    (0.05, 0.1, 50, 3.0, "formula_verification.rs:151 (< 0.1)"),
    (0.20, 0.1, 50, 1.0, "fec_rate/tests.rs:1139 (> 0.9)"),
    (0.05, 0.1, 50, 1.0, "fec_rate/tests.rs:1147 (< 0.01)"),
]:
    print(f"   {what}: cur {p_cur(r,e,W,s2):.6f} palm {p_palm(r,e,W,s2):.6f}")
print("   fec_rate/tests.rs:1101 iid > burst at (0.15,0.1,50): palm "
      f"{p_palm(0.15,0.1,50,1.0):.4f} > {p_palm(0.15,0.1,50,3.0):.4f}")
print("   fec_rate/tests.rs:1154 W 20 < W 200 at (0.15,0.1,s2 1): palm "
      f"{p_palm(0.15,0.1,20,1.0):.4f} < {p_palm(0.15,0.1,200,1.0):.4f}")
v = [p_palm(0.15, 0.1, w, 3.0) for w in (5, 10, 20, 50, 100, 200)]
print(f"   formula_verification.rs:264 monotone in W (r .15 eps .1 s2 3): palm {[round(x,4) for x in v]} "
      f"-> {'monotone' if all(b >= a for a, b in zip(v, v[1:])) else 'NOT monotone'}")
v = [p_palm(k / 100, 0.1, 50, 3.0) for k in range(1, 50)]
print(f"   formula_verification.rs:251 monotone in r: {'yes' if all(b >= a for a, b in zip(v, v[1:])) else 'NO'}")

print("\n== 12. Continuity proper: max |P(r+h)-P(r)| at h = 1e-6 over r on the 0.005 grid and the crossings")
print("   (the 2 % grid jump above mixes steepness with steps; at h -> 0 a continuous law's jump -> 0)")
for name, pf in FORMS.items():
    m = 0.0
    for e in EPS:
        for W in WS:
            for s2 in S2:
                for r in grid(0.005) + [e / (1 - e) - 5e-7]:
                    m = max(m, abs(pf(r + 1e-6, e, W, s2) - pf(r, e, W, s2)))
    print(f"   {name:>5}: {m:.2e}   (palm bound W*h = {200*1e-6:.1e} at W 200)")

print("\n== 13. Convergence of palm to phi as W grows (eps .05, s2 3, r = rc + 1 sd)")
for W in (64, 200, 1000, 5000):
    e, s2 = 0.05, 3.0
    r = e / (1 - e) + math.sqrt(e * s2 / (W * (1 - e)))
    a, b = p_palm(r, e, W, s2), p_phi(r, e, W, s2)
    print(f"   W {W:5d}: r {r:.4f} palm {a:.4f} phi {b:.4f} |diff| {abs(a-b):.4f}  |diff|*sqrt(W) {abs(a-b)*math.sqrt(W):.2f}")

print("\n== 14. Monte Carlo at the tail point (eps .05, s2 3, W 64, r .251): 1-P")
mc = mc_palm(0.251, 0.05, 3.0, 64, trials=400_000)
print(f"   1-MC {1-mc:.2e}  1-palm {1-p_palm(0.251,0.05,64,3.0):.2e}  1-phi {1-p_phi(0.251,0.05,64,3.0):.2e}")
print(f"   wasm lib.rs:2707-2708 shed_budget_residual(.05, 0/.30, 64, 2.9): palm "
      f"{0.05*(1-p_palm(0,0.05,64,2.9)):.5f} / {0.05*(1-p_palm(0.30,0.05,64,2.9)):.2e}  "
      f"cur {0.05*(1-p_cur(0,0.05,64,2.9)):.5f} / {0.05*(1-p_cur(0.30,0.05,64,2.9)):.2e}")
