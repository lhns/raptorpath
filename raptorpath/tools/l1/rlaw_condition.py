#!/usr/bin/env python3
"""r-law consistency condition — goal #100 item 3, the per-cell evaluation.

STRICTLY LOCAL. Reads no ledger, contacts no VM, runs no engine. Every input
is a literal transcribed from a COMMITTED goal-gate section, cited beside it.
Nothing here is a measurement; this script only evaluates a condition that is
written formula-first in `docs/fec-arq-model.md` Section 16.73 and in
`docs/goal-gate.md` ("THE r-LAW CONSISTENCY CONDITION").

THE CONDITION (paper Section 16.73, the interior form):

    alpha^(3/2) * (1-alpha)^(1/2)
        = (p * sigma * G(u)) / (2 * nu * D_arq * (1 - eps_hat))
          ... with the proactive leg INTERIOR, i.e. r* > 0

and, at the shipped Bulk operating point where r* = 0 identically, the
COMPLEMENTARY-SLACKNESS form, which is an INEQUALITY:

    alpha^(3/2) * (1-alpha)^(1/2)
        <= (p * sigma * G(u)) / (2 * nu * D_arq * (1 - eps_hat))

    G(u) = sqrt(2*pi) * exp(u/2) / sqrt(u),   u = W*eps_hat/((1-eps_hat)*s2_burst)

G(u) >= G(1) = sqrt(2*pi*e) = 4.1327 for every u > 0, so the three
proactive-side inputs with no measured provenance (W, eps_hat, s2_burst)
enter ONLY through u and can only ever make the bound LOOSER. The u = 1
evaluation is therefore the TIGHTEST the condition can be, on measured
inputs alone.
"""

import math

SQ2PE = math.sqrt(2 * math.pi * math.e)   # = G(1), the analytic floor of G
FMAX = 3 * math.sqrt(3) / 16              # = 0.32476, max of a^1.5*(1-a)^0.5


def G(u):
    """Proactive-leg shape factor. Minimised at u = 1 with G(1) = sqrt(2*pi*e)."""
    return math.sqrt(2 * math.pi) * math.exp(u / 2) / math.sqrt(u)


def inv_f(R):
    """Solve a^1.5*(1-a)^0.5 = R on (0, 0.75]. None when R exceeds FMAX."""
    if R > FMAX:
        return None
    lo, hi = 1e-15, 0.75
    for _ in range(200):
        m = (lo + hi) / 2
        if m ** 1.5 * (1 - m) ** 0.5 < R:
            lo = m
        else:
            hi = m
    return (lo + hi) / 2


# ---------------------------------------------------------------------------
# INPUTS.  srtt_ms and sigma_ms: goal-gate "THE ALPHA-SWEEP - PRE-REGISTRATION"
# Section 4 table (srtt_wire, sigma median).  d_ms, nu, p per leg: goal-gate
# "THE PASSIVE PRIMITIVES - PLAIN WINDOW, THE SCORED RESULT" Section 7,
# PLAIN-WINDOW rows.  s2_burst: DERIVED from tools/l1/lib.sh::scenario_params
# via paper Section 8.3, s2 = 1 + 2(1-p-q)/(p+q), on the WORST leg (the seat
# net/emit_source.rs:614-618 picks the max-loss path's estimator).
#   c1 = GE 0.05%/50% -> 2.996 ; c2 = 1.3%/50% -> 2.899 ; c3 = 2%/40% -> 3.762
# p_mean is the across-leg mean, the same convention the pre-registration used
# when it wrote p = 0.011215 at c8.
# ---------------------------------------------------------------------------
CELLS = {
    #        srtt_ms  d_ms   sigma_ms   nu       p_mean    p_worst  s2_burst
    "c1":   (2.0,     1.048, 0.035,     0.00183, 0.00015,  0.00015, 2.996),
    "c7":   (72.0,    0.777, 0.499,     0.02966, 0.00545,  0.0056,  2.899),
    "c8":   (77.0,    3.298, 3.140,     0.03776, 0.011215, 0.0184,  3.762),
    "c8L":  (82.0,    9.038, 0.665,     0.03006, 0.0102,   0.0165,  3.762),
    "sc2":  (101.0,   4.370, 0.492,     0.03789, 0.0040,   0.0040,  2.899),
}

# The six swept arms, goal-gate "THE ALPHA-SWEEP - PRE-REGISTRATION" Section 3.
ARMS = [("Q002", 0.002), ("Q009", 0.009), ("Q050", 0.05),
        ("Q184", 0.184), ("Q400", 0.40)]


def rhs(p, sigma_ms, nu, D_ms, eps, Gu):
    """RHS of the condition. Dimensionless: (1)*(s) / ((1)*(s)) * (1)."""
    return p * (sigma_ms / 1e3) * Gu / (2 * nu * (D_ms / 1e3) * (1 - eps))


def main():
    print("=" * 78)
    print("A - THE FLOOR BOUND: u = 1 (G = sqrt(2*pi*e) = %.4f), D_arq = srtt + d" % SQ2PE)
    print("    The TIGHTEST the condition can be. Measured inputs only.")
    print("=" * 78)
    print(f"{'cell':<5} {'D_arq ms':>9} {'RHS':>10} {'alpha_max':>10}   admits")
    for c, (srtt, d, sg, nu, pm, pw, s2b) in CELLS.items():
        D = srtt + d
        R = rhs(pm, sg, nu, D, pw, SQ2PE)
        a = inv_f(R)
        adm = "ALL (bound vacuous)" if a is None else \
            ",".join(n for n, v in ARMS if v <= a) or "(none of the swept arms)"
        print(f"{c:<5} {D:9.2f} {R:10.5f} "
              f"{('%.4f' % a) if a is not None else 'none':>10}   {adm}")

    print()
    print("=" * 78)
    print("B - SENSITIVITY IN u.  u = W*eps_hat/((1-eps_hat)*s2_burst)")
    print("    W in [16,200] (math clamp x MAX_WINDOW_SIZE), eps_hat = BOCD 95%% upper.")
    print("    G is MINIMISED at u = 1: every other u makes the bound LOOSER.")
    print("=" * 78)
    print(f"{'u':>7} {'G(u)':>9} {'G(u)/G(1)':>10}")
    for u in (0.1, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0):
        print(f"{u:7.2f} {G(u):9.3f} {G(u) / SQ2PE:10.2f}")

    print()
    print("=" * 78)
    print("C - PER-CELL OVER THE (W, eps_hat) GRID, D_arq = srtt + d")
    print("    eps_hat swept as 1x/2x/3x the worst leg's realized p.")
    print("=" * 78)
    for c, (srtt, d, sg, nu, pm, pw, s2b) in CELLS.items():
        D = srtt + d
        print(f"  {c}")
        for W in (16, 56, 200):
            out = []
            for mult in (1.0, 2.0, 3.0):
                e = pw * mult
                u = W * e / ((1 - e) * s2b)
                a = inv_f(rhs(pm, sg, nu, D, e, G(u)))
                out.append(f"eps={mult:.0f}p: u={u:6.3f} a_max="
                           f"{('%.4f' % a) if a is not None else '  NONE'}")
            print(f"    W={W:<4} " + "  ".join(out))

    print()
    print("=" * 78)
    print("D - sigma PROPAGATION AT c8 (three reps, 287x spread), floor bound u = 1")
    print("=" * 78)
    srtt, d, _, nu, pm, pw, s2b = CELLS["c8"]
    D = srtt + d
    for sg in (0.191, 3.140, 54.836):
        R = rhs(pm, sg, nu, D, pw, SQ2PE)
        a = inv_f(R)
        print(f"  sigma = {sg:8.3f} ms   RHS = {R:9.5f}   alpha_max = "
              f"{('%.4f' % a) if a is not None else 'NO BOUND (RHS > 0.32476)'}")

    print()
    print("=" * 78)
    print("E - EXACT D_arq(alpha) = srtt + k(alpha)*sigma + d, floor u = 1")
    print("    (the detection wait is itself part of the round the FEC saves)")
    print("=" * 78)
    for c, (srtt, d, sg, nu, pm, pw, s2b) in CELLS.items():
        a, ok = 0.1, True
        for _ in range(500):
            k = math.sqrt((1 - a) / a)
            na = inv_f(rhs(pm, sg, nu, srtt + k * sg + d, pw, SQ2PE))
            if na is None:
                ok = False
                break
            a = 0.5 * a + 0.5 * na
        print(f"  {c:<5} alpha_max = {('%.4f' % a) if ok else 'NO BOUND'}")


if __name__ == "__main__":
    main()
