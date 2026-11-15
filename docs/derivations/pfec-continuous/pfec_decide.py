"""Round 8: apply the registered refutation rule to the frozen predictions
themselves -- which pairs of forms can the oracle tell apart at each cell --
and the tail rule in miss counts.

A form f is refuted iff |P_oracle - P_f| > B, B = 3*SE + tau. Two forms a, b
are SEPARABLE (every oracle value refutes at least one) iff |P_a - P_b| > 2B.
SE: 5 seeds x 1e3 holes, burst-inflated binomial per seed,
sd_seed = sqrt(P(1-P) * bbar / 1000), bbar = mean holes per GE burst = 1/q.

Run: python -I pfec_decide.py > pfec_decide.out
"""
import math, os
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
src = open(os.path.join(HERE, "pfec_review.py"), encoding="utf-8").read()
exec(src.rsplit("# ================================================================= output", 1)[0])

def bbar(e, s2):
    p, q = ge_pq(e, s2)
    return 1.0 / q

def B(P, e, s2, tau, holes=1000, seeds=5):
    sd = math.sqrt(max(P * (1 - P), 1e-4) * bbar(e, s2) / holes)
    return 3 * sd / math.sqrt(seeds) + tau

def L_r(e, s2, W):
    Gs = [G(n, e, s2, W) for n in range(W + 2)]
    return W * max(b - a for a, b in zip(Gs, Gs[1:]))

FORMS = (("shipped", p_cur), ("phi", p_phi), ("C6", p_palm))

def row(name, e, s2, W, r, tau):
    P = {nm: f(r, e, W, s2) for nm, f in FORMS}
    P["C5"] = palm_joint_exact(r, e, s2, W)
    b = max(B(v, e, s2, tau) for v in P.values())
    pairs = []
    for a, x in (("shipped", P["shipped"]), ("phi", P["phi"])):
        for c in ("C6", "C5"):
            g = abs(x - P[c]); pairs.append(f"{a}-{c} {g:.3f}:{'SEP' if g > 2*b else 'no'}")
    g = abs(P["C6"] - P["C5"]); pairs.append(f"C6-C5 {g:.3f}:{'SEP' if g > 2*b else 'no'}")
    g = abs(P["shipped"] - P["phi"]); pairs.append(f"shipped-phi {g:.3f}:{'SEP' if g > 2*b else 'no'}")
    print(f"   {name}: B {b:.3f} (2B {2*b:.3f}) | " + "  ".join(pairs))

print("== Shed budget rows (tau = 1e-3/eps = 0.021 at eps .048), A* = W of the block forms")
tau_s = 1e-3 / 0.048
for A in (3, 4, 5, 6, 7):
    for r in (0.2, 0.29, 0.34, 0.5):
        row(f"A* {A} r {r}", 0.048, 3.76, A, r, tau_s)
row("A* 4 r 0.5 r_c", 0.048, 3.76, 4, 0.5 * 0.048 / 0.952, tau_s)

print("\n== r_sat rows, W 201 (tau = 0.005 * L_r)")
for name, e, s2v in [("c2", 0.013/0.513, 1+2*(1-0.513)/0.513), ("c3", 0.02/0.42, 1+2*(1-0.42)/0.42)]:
    Lr = L_r(e, s2v, 201); tau = 0.005 * Lr
    print(f"   {name}: L_r {Lr:.2f}  tau {tau:.3f}")
    rc = e / (1 - e)
    for k in (0.5, 1.5, 2.0):
        row(f"{name} {k} r_c", e, s2v, 201, k * rc, tau)

print("\n== Tail rule in miss counts (N = 1e5 holes at the Auto r*, d = L_arq)")
print("   a form f is refuted iff |k - mu_f| > 3*sqrt(mu_f * bbar)  (burst-inflated Poisson); no unassigned range:")
print("   every k either refutes or keeps each form, independently")
for name, e, s2v, r in [("c2", 0.013/0.513, 1+2*(1-0.513)/0.513, 0.0910), ("c3", 0.02/0.42, 1+2*(1-0.42)/0.42, 0.1579)]:
    bb = bbar(e, s2v); N = 1e5
    out = []
    for nm, mu in (("shipped/phi", N * (1 - p_cur(r, e, 201, s2v))), ("C6", N * (1 - p_palm(r, e, 201, s2v))),
                   ("C5", N * (1 - palm_joint_exact(r, e, s2v, 201))), ("target", N * 1e-5 / e)):
        h = 3 * math.sqrt(mu * bb)
        out.append(f"{nm} mu {mu:.0f}, kept iff k in [{max(0, math.ceil(mu - h))}, {math.floor(mu + h)}]")
    print(f"   {name} (bbar {bb:.2f}): " + " | ".join(out))
