"""Main-session review items for docs/derivations/pfec-continuous.md (round 4).

R1  decoder at the A* windows: exact joint-GE per-loss block value (C5) at
    W 3-7, a sliding-window generic-rank decode simulation of the engine's
    trailing span, and a port of temporal_oracle.rs `unified_rstar_n1_reduces_to_84`.
R2  cost at the measured L1 window (wsize ~ 201) with truncation at f64 mass.
R3  paper sec 4.7 exact r* reproduced; per-loss r* shift; off-Bulk r* at the
    engine tail targets; the audit's rho-inert / T_cut gauges recomputed.
R4  the derived slope bound dP/dr = W (G(n0+1) - G(n0)).

Run: python -I pfec_review.py > pfec_review.out   (imports pfec_check.py's forms)
"""
import math, os, sys, time
import numpy as np
from scipy.stats import binom, norm

HERE = os.path.dirname(os.path.abspath(__file__))
_src = open(os.path.join(HERE, "pfec_check.py"), encoding="utf-8").read()
exec(_src.split("# ================================================================= output")[0])

# ------------------------------------------------------------ C5 exact joint
def palm_joint_exact_n(e, s2, W, n, layout="interleaved"):
    """Exact per-loss recovery with exactly n repairs, GE-correlated repair
    erasures: DP over wire slots tracking (state, K, C). layout 'interleaved'
    = p_fec_exact's schedule (lib.rs:1129-1160); 'after' = repairs after the
    window's sources (temporal_oracle.rs:1397-1411)."""
    p, q = ge_pq(e, s2)
    N = W + n
    if layout == "interleaved":
        is_rep = [((i + 1) * n) // N > (i * n) // N for i in range(N)] if n else [False] * N
    else:
        is_rep = [False] * W + [True] * n
    # f[x][K][C]
    f = np.zeros((2, W + 1, n + 1))
    first = True
    for i in range(N):
        if first:
            g_, b_ = 1 - e, e  # stationary start
            nf = np.zeros_like(f)
            if is_rep[i]:
                nf[0, 0, 1] = g_; nf[1, 0, 0] = b_
            else:
                nf[0, 0, 0] = g_; nf[1, 1, 0] = b_
            f = nf; first = False
            continue
        tg = f[0] * (1 - p) + f[1] * q
        tb = f[0] * p + f[1] * (1 - q)
        nf = np.zeros_like(f)
        if is_rep[i]:
            nf[0, :, 1:] += tg[:, :-1]; nf[1] += tb
        else:
            nf[0] += tg; nf[1, 1:, :] += tb[:-1, :]
        f = nf
    tot = f[0] + f[1]
    K = np.arange(W + 1)[:, None]; C = np.arange(n + 1)[None, :]
    num = (tot * K * (C >= K)).sum(); den = (tot * K).sum()
    return num / den

def palm_joint_exact(r, e, s2, W, layout="interleaved"):
    x = r * W; n0 = math.floor(x); f = x - n0
    a = palm_joint_exact_n(e, s2, W, n0, layout)
    return a if f == 0 else (1 - f) * a + f * palm_joint_exact_n(e, s2, W, n0 + 1, layout)

# ------------------------------------------------------------ sliding decode
def sliding_decode(e, s2, r, A, delta=1, T=60_000, seed=7, tol=1e-8):
    """Engine trailing span (emit_source.rs:784-806): after source t, while
    debt >= 1 emit a repair over [max(0, end-A), end), end = t+1-delta, debt += r
    per source (debt accrual, :738-741). One GE chain over the wire. Decode
    without a deadline (upper bound: ARQ races it in the engine): a lost source
    is recovered iff e_j is in the row space of the generic (random real)
    coefficient matrix of the surviving repairs over the unknowns."""
    rng = np.random.default_rng(seed)
    p, q = ge_pq(e, s2)
    bad = rng.random() < e
    lost = np.zeros(T, bool)
    reps = []
    debt = rng.random()  # uniform phase
    def step():
        nonlocal bad
        u = rng.random()
        bad = (u >= q) if bad else (u < p)
        return bad
    for t in range(T):
        lost[t] = step()
        debt += r
        while debt >= 1.0:
            debt -= 1.0
            end = t + 1 - delta; start = max(0, end - A)
            l = step()
            if end > start and not l:
                reps.append((start, end))
    # unknowns and components (union-find over unknowns sharing a repair)
    idx = -np.ones(T, int); unk = np.flatnonzero(lost); idx[unk] = np.arange(len(unk))
    parent = list(range(len(unk)))
    def find(a):
        while parent[a] != a:
            parent[a] = parent[parent[a]]; a = parent[a]
        return a
    rep_unk = []
    for (s, en) in reps:
        u = [idx[j] for j in range(s, en) if lost[j]]
        if u:
            rep_unk.append(u)
            for v in u[1:]:
                ra, rb = find(u[0]), find(v)
                if ra != rb: parent[ra] = rb
    comp = {}
    for i in range(len(unk)):
        comp.setdefault(find(i), []).append(i)
    rows_of = {}
    for u in rep_unk:
        rows_of.setdefault(find(u[0]), []).append(u)
    rec = 0
    for root, cols in comp.items():
        rows = rows_of.get(root, [])
        if not rows:
            continue
        pos = {c: k for k, c in enumerate(cols)}
        M = np.zeros((len(rows), len(cols)))
        for i, u in enumerate(rows):
            for v in u:
                M[i, pos[v]] = rng.standard_normal()
        # e_j in rowspace(M)  <=>  row j of a null-space basis is 0
        _, sv, vt = np.linalg.svd(M)
        rank = int((sv > 0.1 * tol * max(1.0, sv.max())).sum())
        Nb = vt[rank:].T  # cols x nullity
        if Nb.shape[1] == 0:
            rec += len(cols)
        else:
            rec += int((np.abs(Nb).max(axis=1) < tol).sum())
    return rec / max(len(unk), 1), len(unk)

# ------------------------------------------------------------ oracle port
def oracle_port(e, q, W, r, k=1_200_000, seed=0xD00D):
    """temporal_oracle.rs:1361-1423, single path: chain starts Good, each
    window draws W sources then floor(rW)+Bernoulli(frac) repairs; a lost
    symbol of an uncovered window is a fec_miss. Returns fec_miss / k."""
    rng = np.random.default_rng(seed)
    p = e * q / (1 - e)
    nwin = k // W
    nrep = np.floor(r * W).astype(int) + (rng.random(nwin) < (r * W - math.floor(r * W)))
    L = nwin * W + int(nrep.sum())
    u = rng.random(L)
    st = np.empty(L, bool); bad = False
    for i in range(L):
        bad = (u[i] >= q) if bad else (u[i] < p)
        st[i] = bad
    miss = 0; pos = 0
    for w in range(nwin):
        nl = int(st[pos:pos + W].sum()); pos += W
        nr = int(nrep[w]); sv = nr - int(st[pos:pos + nr].sum()); pos += nr
        if sv < nl:
            miss += nl
    return miss / (nwin * W)

def r_star_closed(e, s2, W, t):
    z = norm.ppf(1 - t / e)
    return max(0.0, e / (1 - e) + z * math.sqrt(e * s2 / (W * (1 - e))))

# ------------------------------------------------------------ p_fec_exact port
def p_fec_exact_port(p, q, r, W):  # lib.rs:1129-1160, window event, round(rW)
    n = int(round(max(r, 0) * W)); N = W + n
    pi_b = p / (p + q)
    off = n; dmax = W + n + 1
    f = np.zeros((dmax, 2)); f[off] = [1 - pi_b, pi_b]
    for i in range(N):
        is_rep = (i + 1) * n // N > i * n // N
        tg = f[:, 0] * (1 - p) + f[:, 1] * q
        tb = f[:, 0] * p + f[:, 1] * (1 - q)
        nx = np.zeros_like(f)
        if is_rep:
            nx[:-1, 0] += tg[1:]; nx[0, 0] += tg[0]; nx[:, 1] += tb
        else:
            nx[:, 0] += tg; nx[1:, 1] += tb[:-1]; nx[-1, 1] += tb[-1]
        f = nx
    return 1 - f[off + 1:].sum()

def palm_exact_rounded(p, q, r, W):  # per-loss, same rounding/schedule as p_fec_exact
    e = p / (p + q); s2 = 1 + 2 * (1 - p - q) / (p + q)
    return palm_joint_exact_n(e, s2, W, int(round(max(r, 0) * W)))

def min_r(fail, target, hi=2.0):
    if fail(hi) > target:
        return float("inf")
    lo = 0.0
    for _ in range(40):
        mid = (lo + hi) / 2
        if fail(mid) > target: lo = mid
        else: hi = mid
    return hi

EPS = [0.005, 0.025, 0.05, 0.1, 0.2, 0.3]
WS = [3, 4, 5, 64, 200]
S2 = [1.0, 2.9, 3.0, 3.76, 5.0]
def grid(h):
    return [round(i * h, 10) for i in range(int(round(1 / h)) + 1)]

# ================================================================= output
print("== R1a. Block value at the A* windows (eps .048, s2 3.76): per-loss P under each form")
print("   measured band (8e77ba67 ledger, one FDIAG rep, ~65 holes): decode 51 vs ARQ 10-21 -> 0.71-0.84")
print(f"   {'A*':>3} {'r':>5} | {'shipped':>8} {'C6':>7} {'C5 exact (interleaved)':>23} {'C5 (after)':>10}")
for A in (3, 4, 5, 6, 7):
    for r in (0.29, 0.34):
        print(f"   {A:3d} {r:5.2f} | {p_cur(r,.048,A,3.76):8.4f} {p_palm(r,.048,A,3.76):7.4f} "
              f"{palm_joint_exact(r,.048,3.76,A):23.4f} {palm_joint_exact(r,.048,3.76,A,'after'):10.4f}")

print("\n== R1b. Sliding-window generic-rank decode, engine trailing span, NO deadline (upper bound)")
print("   3 seeds x 2 rank tolerances (1e-8, 1e-6): mean, SE over the 6 runs, max-min spread")
def sl_stats(e, s2, r, A, d, T=60_000):
    v = [sliding_decode(e, s2, r, A, d, T=T, seed=sd, tol=tl)[0] for sd in (7, 11, 13) for tl in (1e-8, 1e-6)]
    return float(np.mean(v)), float(np.std(v, ddof=1) / math.sqrt(len(v))), max(v) - min(v)
print(f"   {'A*':>3} {'r':>5} {'Delta':>5} | {'sliding':>8} {'SE':>6} {'spread':>6} | C6(W=A*)  C5   shipped")
for A in (4, 5, 7):
    for r in (0.29, 0.34):
        for d in (1, 2):
            m, se, sp = sl_stats(0.048, 3.76, r, A, d)
            print(f"   {A:3d} {r:5.2f} {d:5d} | {m:8.4f} {se:6.4f} {sp:6.4f} | {p_palm(r,.048,A,3.76):.4f}  "
                  f"{palm_joint_exact(r,.048,3.76,A):.4f} {p_cur(r,.048,A,3.76):.4f}")
for A, r in ((64, 0.1), (200, 0.0526)):
    m, se, sp = sl_stats(0.05, 3.0, r, A, 1, T=40_000)
    print(f"   eps .05 s2 3 A* {A} r {r}: sliding {m:.4f} (SE {se:.4f})  C6 {p_palm(r,.05,A,3.0):.4f}  "
          f"shipped {p_cur(r,.05,A,3.0):.4f}")

print("\n== R1d. Which inputs reach the measured band 0.71-0.84? (eps .048; ledger inputs A* 4-7, r 0.2-0.5)")
print(f"   {'s2':>4} {'A*':>3} {'r':>4} | {'shipped':>7} {'C6':>6} {'C5':>6} {'sliding':>7} (Delta 1, 3 seeds; * = in band)")
for s2 in (1.0, 2.0, 3.76):
    for A in (4, 7):
        for r in (0.2, 0.34, 0.5):
            v = [sliding_decode(0.048, s2, r, A, 1, T=40_000, seed=sd)[0] for sd in (7, 11, 13)]
            vals = [p_cur(r, .048, A, s2), p_palm(r, .048, A, s2), palm_joint_exact(r, .048, s2, A), float(np.mean(v))]
            tag = "".join("*" if 0.71 <= x <= 0.84 else "." for x in vals)
            print(f"   {s2:4.2f} {A:3d} {r:4.2f} | {vals[0]:7.4f} {vals[1]:6.4f} {vals[2]:6.4f} {vals[3]:7.4f}  {tag}")

print("\n== R1c. Port of temporal_oracle.rs:1476 (W 64, q .5, K 1.2M, D in the ARQ band): ratio oracle/(eps(1-P))")
print("   test passes iff ratio in (0.6, 1.7) for the form it uses (Phi = shipped above r_c)")
for e, d in [(0.05, 0.02), (0.05, 0.03), (0.10, 0.04), (0.10, 0.06), (0.08, 0.03)]:
    q = 0.5; p = e * q / (1 - e); s2 = 1 + 2 * (1 - p - q) / (p + q)
    rs = r_star_closed(e, s2, 64, d)
    o = oracle_port(e, q, 64, rs)
    rat = {nm: o / (e * (1 - pf(rs, e, 64, s2))) for nm, pf in (("shipped", p_cur), ("phi", p_phi), ("C6", p_palm))}
    print(f"   eps {e} delta {d}: r* {rs:.4f} oracle {o:.5f} | " +
          " ".join(f"{k} {v:.3f}" for k, v in rat.items()) +
          f" | C5(after) {o/(e*(1-palm_joint_exact(rs,e,s2,64,'after'))):.3f}")

print("\n== R2. Cost at the measured L1 window (SPAN wsize p50 201, max 247; docs/l1-raw/anchor-diag/diag-ad)")
for name, e, s2 in [("c1", 0.0005/0.5005, 1+2*(1-0.5005)/0.5005), ("c2", 0.013/0.513, 1+2*(1-0.513)/0.513),
                    ("c3", 0.02/0.42, 1+2*(1-0.42)/0.42)]:
    for W in (201, 247):
        pm = k_pmf(e, s2, W)
        tail = np.cumsum(pm[::-1])[::-1]
        kmax = int(np.flatnonzero(tail > 1e-17).max())
        full = 8 * W * W + 6 * W * W
        trunc = 8 * W * kmax + 2 * 199 * 3 * kmax
        print(f"   {name} W {W}: k_max(mass > 1e-17) {kmax:3d}  ops/r_sat call full {full:.1e}  "
              f"truncated {trunc:.1e}  per rate eval (x2) {2*trunc:.1e}")

print("\n== R3a. Paper sec 4.7 exact r* reproduced (W 50, window fail <= 1e-2), then the per-loss event")
for nm, p, q, paper in [("WiFi", 0.013, 0.5, 0.170), ("LTE", 0.02, 0.4, 0.270), ("Sat", 0.03, 0.3, 0.450)]:
    rw = min_r(lambda r: 1 - p_fec_exact_port(p, q, r, 50), 1e-2)
    rp = min_r(lambda r: 1 - palm_exact_rounded(p, q, r, 50), 1e-2)
    e = p / (p + q); s2 = 1 + 2 * (1 - p - q) / (p + q)
    rc6 = min_r(lambda r: 1 - p_palm(r, e, 50, s2), 1e-2)
    print(f"   {nm}: window-event exact r* {rw:.3f} (paper {paper})  per-loss exact {rp:.3f}  C6 {rc6:.3f}  "
          f"closed form {r_star_closed(e, s2, 50, 1e-2*e):.3f}")

print("\n== R3b. Off-Bulk r* (random term only) at the engine tail targets, W 201, nominal L1 inputs")
print("   constraint: closed form (window event, normal) vs per-loss C6: 1 - P_C6(r) <= t/eps; max_overhead 0.5")
for name, e, s2 in [("c1", 0.0005/0.5005, 1+2*(1-0.5005)/0.5005), ("c2", 0.013/0.513, 1+2*(1-0.513)/0.513),
                    ("c3", 0.02/0.42, 1+2*(1-0.42)/0.42)]:
    for hint, t in (("Auto", 1e-5), ("Realtime", 1e-7)):
        rc = r_star_closed(e, s2, 201, t)
        r6 = min_r(lambda r: 1 - p_palm(r, e, 201, s2), t / e, hi=1.0)
        ach = e * (1 - p_palm(rc, e, 201, s2)) / t
        print(f"   {name} {hint:8s} t {t:.0e}: closed {rc:.4f}  C6 {r6 if r6 < 9 else float('inf'):.4f}  "
              f"tail at the closed r* under C6 = {ach:.0f} x target"
              f"{'  (> max_overhead 0.5)' if r6 > 0.5 else ''}")

print("\n== R3c. Audit F3 rho-inert gauge recomputed (cartoon inputs, interactive-visualizer.html:2617-2642:")
print("   rate = cap (1-eps), cap 4, D = min(b(delta) RTT, 2 RTT), b(.5) = 1, b(50) = .5, A* = rate D (the W")
print("   clamp does not bind at the Auto-slider W*: R shipped reproduces the audit's R(code)); r* from the audit)")
print("   R side binds = #{j = 1..100 : j e-3 >= R}  (rho in [0.900, 0.999])")
rows = [(.05, .5, 50, .5, .149, 1.9e-5), (.05, .5, 50, 50, .178, 5.2e-5), (.10, .3, 80, .5, .266, 2.9e-5),
        (.10, .3, 80, 50, .309, 9.9e-5), (.025, .5, 13, .5, .152, 1.6e-5), (.025, .5, 13, 50, .194, 3.2e-5),
        (.20, .1, 100, .5, .627, 6.5e-5), (.20, .1, 100, 50, .724, 2.5e-4), (.30, .05, 150, .5, 1.03, 5.9e-5),
        (.30, .05, 150, 50, 1.18, 2.8e-4)]
for e, q, rtt, d, rs, Raud in rows:
    p = e * q / (1 - e); s2 = 1 + 2 * (1 - p - q) / (p + q)
    b = 1.0 if d == .5 else 0.5
    A = max(4 * (1 - e) * min(b * rtt, 2 * rtt), 1)
    Rc = e * (1 - p_cur(rs, e, A, s2))
    R6 = e * (1 - p_palm_realW(rs, e, A, s2))
    cnt = lambda R: sum(1 for j in range(1, 101) if j * 1e-3 >= R)
    print(f"   {e}/{q}/{rtt} delta {d}: A* {A:.1f} s2 {s2:.2f} r* {rs}: R(audit) {Raud:.1e}  R shipped {Rc:.1e} ({cnt(Rc)}/100)  "
          f"R C6 {R6:.1e} ({cnt(R6)}/100)")
print("   Bulk rows (r* = 0): R = eps under shipped and C6 (P(0) = 0), so 51/0/76/0/0 and the T_cut")
print("   condition 1-rho < R eps^(D/RTT) = eps^3 at D = 2 RTT are identities:")
for e in (0.2, 0.3):
    th = e * e ** 2
    print(f"   eps {e}: threshold {th:.3e} -> T_cut binds at {sum(1 for j in range(1,101) if j*1e-3 < th)}/100")

print("\n== R3d. rho < 1 shed budget at the paper sec 5.6 c3 fixture (eps .048, r .34, A* 4-7, s2 3.76)")
for A in (4, 5, 7):
    print(f"   A* {A}: budget shipped {.048*(1-p_cur(.34,.048,A,3.76)):.4f}  C6 {.048*(1-p_palm(.34,.048,A,3.76)):.4f}  "
          f"C5 {.048*(1-palm_joint_exact(.34,.048,3.76,A)):.4f}   (measured shed share 0.25 %)")

print("\n== R4. Slope bound: dP/dr = W (G(n0+1) - G(n0)) <= W max_n dG ; check on the grid")
worst = 0.0
for e in EPS:
    for W in WS:
        for s2 in S2:
            Gs = [G(n, e, s2, W) for n in range(W + 2)]
            L = W * max(b - a for a, b in zip(Gs, Gs[1:]))
            h = 1e-4
            for r in grid(0.01)[:-1]:
                sl = (p_palm(r + h, e, W, s2) - p_palm(r, e, W, s2)) / h
                worst = max(worst, sl / L if L > 0 else 0)
print(f"   max over grid of (finite-difference slope) / (W max dG): {worst:.4f}  (<= 1 required)")
for e, W, s2 in [(0.048, 4, 3.76), (0.05, 64, 2.8), (0.025, 51, 2.8), (0.025, 201, 2.9)]:
    Gs = [G(n, e, s2, W) for n in range(W + 2)]
    print(f"   eps {e} W {W} s2 {s2}: W max dG = {W*max(b-a for a,b in zip(Gs,Gs[1:])):.3f} per unit r")
