"""Knee-margin re-derivation (knee-margin-rederive.md): what need = T-hat-min(W95) - F scales with, across c2 and c3.

Usage (from docs/):  python -I -X utf8 derivations/store-law/kmr.py l1-raw/s60/rep60a-diag-ld.tgz l1-raw/s60/rep60b-diag-ld.tgz \
                         l1-raw/s61/rep61c-diag-ld.tgz l1-raw/s61/rep61a-diag-ld.tgz l1-raw/s61/rep61b-diag-ld.tgz  > derivations/store-law/kmr.out

Reads the committed ledgers in place (tarfile), with s61pred.py's window parse and grouping G(n*) (s61-registration.md section 3). Every
reading here is POST HOC: none was registered. Sections:
  0. witness: the scored c3 reading (rep61c-score.txt) and c2's frozen inputs, reproduced
  1. need at both cells under five sample-count / floor conventions, with a bootstrap interval on need_3 / need_2
  2. the decomposition need = L - N: L the load rise at the knee's sample count, N the floor's sample-count drop
  2b. the c3-shaped floor against load, with c8 leg 1's lowest-load rows
  2c. k = L / X per spread X and cell, and s61's falsifier re-read at equal n (rho_L)
  3. candidate scalings X: X at both cells, the ratio, the two-point exponent, and which convention's interval it falls in
  4. the per-leg link-utilisation instrument (netem Sent bytes): validated at the c2 / c3 knees, then c8's legs and c1s as bounds
  5. allowance forms replayed: D per hint at both cells, the R67 control sign at c2 / c3 / c1s, the c1s size; 5b form S under every convention
  Round 2 (status §61a re-run Result, Erratum 1; needs rep61a and rep61b, each loaded on its own because their row keys collide with s60's / 61c's):
  6. every ratio on both c3 sessions, with session-spanning bands, and the candidates against them
  7. what the floor minimum's drop follows: samples, time span or acks (doubling drops regressed on each), and count- against span-matched drops
  8. L at several equal counts (does L move with the count?)
  9. a count-free statistic (the window's own p10 / p50): within-arm check, need_q on both sessions with intervals, k_q per spread
"""
import json
import math
import os
import random
import re
import sys
import tarfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import r68allow as ra  # noqa: E402
import s61pred as sp  # noqa: E402  (installs the parse with aw / err; provides groups / ols)

C2, C3, C8, C1S = "c2-100", "c3-25", "c8-100", "c1s-400"
ra.LATE_T[C3] = 15.0                                   # s61: LATE is t >= 15 s at c3, as at c2 / c8
med, interp, cross_up = ra.med, ra.interp, ra.cross_up
QB, QA, QR = ra.QB, ra.QA, ra.QR
QS = ra.QS
U2 = 0.112                                             # c2's W 16 utilisation (s61 section 5)
NF, NK = 250, 2100                                     # s61 section 3: G(250) at the floor, G(2 100) at the knee (c3)
FLOOR = {C2: (16, 32, 48), C3: (12, 16, 24)}
KARMS = {C2: (160, 176, 192), C3: (100, 108, 120)}
P1 = {C2: 256, C3: 152}
CTL = {C2: 2048, C3: 512}
RATE = {C2: 100e6, C3: 20e6, C8: (100e6, 20e6), C1S: 1e9}
N_BOOT, SEED = 2000, 62
P = print
QD = re.compile(r"QDISC (\w+): .*?delay (\d+)ms(?:\s+(\d+)ms)?.*?rate (\d+)([GM])bit.*?Sent (\d+) bytes (\d+) pkt")
TRUTH = re.compile(r"\[TRUTH\] leg=(\d+) .*?egress_dgrams=(\d+) egress_skbs=(\d+) gso=([\d.]+)")
NAME = re.compile(r"diag-ld/(c\w+-\d+)@def-([GC])(\d+)-s(\d+)-r(\d+)-a\d+-drv\.out$")


# ───────────────────────────── loading ─────────────────────────────
def load(paths):
    rows = ra.load(paths)                              # windows (s61pred's parse) and goodput, keyed (cell, kind, W, seed, rep)
    for path in paths:                                 # the driver's netem counters and [TRUTH] (not read by ra.load)
        with tarfile.open(path, "r:gz") as tf:
            for mb in tf:
                g = NAME.search(mb.name)
                if not g or not mb.isfile():
                    continue
                key = (g.group(1), g.group(2), int(g.group(3)), g.group(4), int(g.group(5)))
                r = rows.get(key)
                if r is None:
                    continue
                text = tf.extractfile(mb).read().decode("utf-8", "replace")
                r.secs, r.qd, r.gso = None, {}, {}
                for ln in text.splitlines():
                    if ln.startswith("{") and '"summary":true' in ln:
                        r.secs = json.loads(ln).get("median_s")
                    m = QD.search(ln)
                    if m:
                        dev, dly, jit, rate, unit, by, pk = m.groups()
                        r.qd[dev] = {"delay": float(dly), "jitter": float(jit or 0), "rate": float(rate) * (1e9 if unit == "G" else 1e6),
                                     "bytes": int(by), "pkt": int(pk)}
                    m = TRUTH.search(ln)
                    if m:
                        r.gso[int(m.group(1))] = (int(m.group(2)), int(m.group(3)), float(m.group(4)))
    return rows


def arm(rows, cell, W, kind="G"):
    return [r for r in rows.values() if r.cell == cell and r.kind == kind and r.W == W]


def reps(rows, cell):
    m = {}
    for r in rows.values():
        if r.cell == cell and r.kind == "G":
            m.setdefault((r.rep, r.seed), {})[r.W] = r
    return dict(sorted(m.items()))


_RF = {}


def rowF(r, ns, leg=0):
    """(S_F, F, realised n) of one row: native windows (nk >= 10) when ns is None, else G(ns) over every LATE window with nk >= 1 (memoised)."""
    k = (id(r), ns, leg)
    if k not in _RF:
        s, f, n, g = sp.row_SF(r, leg, ns)
        _RF[k] = (s, f, n, g)
    return _RF[k][:3]


def rowG(r, ns, leg=0):
    """complete groups of one row under G(ns)"""
    rowF(r, ns, leg)
    return _RF[(id(r), ns, leg)][3]


def native_q(r, key, leg=0):
    return med([w[key] for w in r.late(leg) if w.get(key) is not None])


def link_u(r, dev):
    q = r.qd.get(dev) if getattr(r, "qd", None) else None
    if not q or not r.secs:
        return None
    return q["bytes"] * 8 / r.secs / q["rate"]


def ols(xs, ys):
    b, se = sp.ols(xs, ys)
    return b, sum(ys) / len(ys) - b * sum(xs) / len(xs), se


def q90(v):
    v = sorted(v)
    return v[int(0.05 * len(v))], v[int(0.95 * len(v)) - 1]


# ───────────────────────────── the readings (one function, every convention) ─────────────────────────────
class Cell:
    """One cell's rows, resampled or not: floor rows per pool, knee replicates. Readings are recomputed from these lists, so a bootstrap draw is a Cell
    built from resampled lists."""

    def __init__(self, cell, floor_rows, reps_, kappa, karms=None, p1=None, ctl=None):
        self.cell, self.floor_rows, self.reps, self.kappa = cell, floor_rows, reps_, kappa
        self.karms, self.p1, self.ctl = karms or KARMS[cell], p1 or P1[cell], ctl or CTL[cell]

    def w_star(self):
        return U2 / self.kappa if self.cell == C3 else 16.0

    def floor(self, ns, where="u"):
        """F and S_F of the floor at sample-count rule ns (None = native): c3 on the OLS line over its pools at W* (where='u') or at W -> 0 (where='0');
        c2 at W 16 (= u2 by definition) for 'u', on its own line's intercept for '0'."""
        pts = [(W, rowF(r, ns)) for W, rs in self.floor_rows.items() for r in rs]
        pts = [(W, s, f) for W, (s, f, n) in pts if f is not None]
        if self.cell == C2 and where == "u":
            v = [(s, f) for W, s, f in pts if W == 16]
            return med([f for _, f in v]), med([s for s, _ in v])
        bF, aF, _ = ols([W for W, _, _ in pts], [f for _, _, f in pts])
        bS, aS, _ = ols([W for W, _, _ in pts], [s for _, s, _ in pts])
        x = self.w_star() if where == "u" else 0.0
        return aF + bF * x, aS + bS * x

    def knees(self, ns):
        """Per replicate (W95, T-hat-min(W95)): W95 = cross_up of gp / own control over every non-control arm; T-hat-min interpolated over K + P1 row minima
        under ns (None = native windows)."""
        out = []
        for key, wm in self.reps.items():
            ctl = wm.get(self.ctl)
            if not ctl or not ctl.gp:
                continue
            pts = [(W, r.gp / ctl.gp) for W, r in wm.items() if W != self.ctl and r.gp]
            w95 = cross_up(pts, 0.95)
            arms = [W for W in tuple(self.karms) + (self.p1,) if W in wm]
            t = interp([(W, rowF(wm[W], ns)[1]) for W in arms], w95)
            if self.cell == C3 and not (self.karms[0] <= w95 <= self.karms[-1]):
                continue                                                   # s61 section 2.2's scoreability rule
            out.append((w95, t, key))
        return out


def build(rows, cell, karms=None, p1=None, ctl=None):
    rp = reps(rows, cell)
    fr = {W: arm(rows, cell, W) for W in FLOOR[cell]}
    kappa = None
    if cell == C3:
        ctl_m = med([r.gp for r in arm(rows, cell, ctl or CTL[cell])])
        kappa = med([r.gp / ctl_m / W for W, rs in fr.items() for r in rs])
    return Cell(cell, fr, rp, kappa, karms, p1, ctl)


def resample(c, rnd):
    fr = {W: [rs[rnd.randrange(len(rs))] for _ in rs] for W, rs in c.floor_rows.items()}
    keys = list(c.reps)
    rp = {i: c.reps[keys[rnd.randrange(len(keys))]] for i in range(len(keys))}
    return Cell(c.cell, fr, rp, c.kappa, c.karms, c.p1, c.ctl)


# The conventions: (label, c2 floor ns, c2 knee ns, c3 floor ns, c3 knee ns, floor position)
CONV = (("registered (s61: c2 native; c3 G(250) floor / G(2100) knee; u-matched)", None, None, NF, NK, "u"),
        ("engine-native (every window as the engine sees it; u-matched)", None, None, None, None, "u"),
        ("equal-n (floor and knee both at the knee's count: G(2100) floors, c2 knee native, c3 knee G(2100); u-matched)", NK, None, NK, NK, "u"),
        ("registered, floor at the intercept W -> 0", None, None, NF, NK, "0"),
        ("equal-n, floor at the intercept W -> 0", NK, None, NK, NK, "0"))


def floor_conv(c, fns, where):
    """(F, S_F) of a convention's floor, both on the same windows and floor position. c2's native W -> 0 is s61 section 5.1's: the matched-n (G(800))
    slopes applied to the native W 16 values."""
    if c.cell == C2 and where == "0" and fns is None:
        pts = [(W, rowF(r, 800)) for W, rs in c.floor_rows.items() for r in rs]
        bF = ols([W for W, _ in pts], [x[1] for _, x in pts])[0]
        bS = ols([W for W, _ in pts], [x[0] for _, x in pts])[0]
        F0, S0 = c.floor(None, "u")
        return F0 - bF * 16, S0 - bS * 16
    return c.floor(fns, where)


def need(c, fns, kns, where):
    F, S = floor_conv(c, fns, where)
    ks = c.knees(kns)
    return med([t - F for _, t, _ in ks]), F, S, ks


def main(argv):
    rows = load(argv[1:4])
    c2, c3 = build(rows, C2), build(rows, C3)

    # ── 0. witness ──
    P("== 0. Witness: the scored c3 reading (rep61c-score.txt) and c2's frozen inputs (s61pred.out section 2), reproduced ==")
    F3, S3 = c3.floor(NF, "u")
    k3 = c3.knees(NK)
    P(f"  c3: kappa {c3.kappa:.5f}, W* {c3.w_star():.1f}, F_c {F3:.3f}, S_F,c {S3:.3f}   (score: kappa 0.00962, W* 11.6, F_c 38.998, S_F,c 9.709)")
    P("  c3 per replicate W95 / Tmin(W95) / need: " + "; ".join(f"{w:.1f} / {t:.3f} / {t - F3:.3f}" for w, t, _ in k3)
      + f"  -> rho_3 {med([(t - F3) / S3 for _, t, _ in k3]):.3f}   (score: needs 4.256 4.498 4.404 4.464 3.610 4.478; rho_3 0.457)")
    F2, S2 = c2.floor(None, "u")
    k2 = c2.knees(None)
    P(f"  c2: F_c {F2:.3f}, S_F,c {S2:.3f}; Tmin(W95) " + " ".join(f"{t:.3f}" for _, t, _ in k2)
      + f" -> need {med([t - F2 for _, t, _ in k2]):.3f}, rho_2 {med([(t - F2) / S2 for _, t, _ in k2]):.3f}   (frozen: F 8.679, S 5.083, need 4.05, rho_2 0.797)")
    Fn, Sn = c3.floor(None, "u")
    kn = c3.knees(None)
    P(f"  c3 native: F {Fn:.3f}, S_F {Sn:.3f}, rho {med([(t - Fn) / Sn for _, t, _ in kn]):.3f}   (score NATIVE: F 42.523, S_F 5.861, rho_3 0.633)")

    # ── 1. conventions ──
    P("\n== 1. need under each sample-count / floor convention; ratio need_3 / need_2 with a 90 % bootstrap (2 000 draws, seed 62: replicates and floor rows"
      " resampled within pools at both cells, floor lines refitted per draw) ==")
    rnd = random.Random(SEED)
    draws = [(resample(c2, rnd), resample(c3, rnd)) for _ in range(N_BOOT)]
    conv_ci = {}
    for lab, f2, k2n, f3, k3n, where in CONV:
        n2, F2c, S2c, kk2 = need(c2, f2, k2n, where)
        n3, F3c, S3c, kk3 = need(c3, f3, k3n, where)
        rat = []
        for d2, d3 in draws:
            a, b = need(d2, f2, k2n, where)[0], need(d3, f3, k3n, where)[0]
            if a and b:
                rat.append(b / a)
        lo, hi = q90(rat)
        conv_ci[lab] = (n3 / n2, lo, hi, n2, n3)
        P(f"  {lab}")
        P(f"     c2: F {F2c:.3f}, need {n2:.3f} (per replicate {min(t - F2c for _, t, _ in kk2):.2f}-{max(t - F2c for _, t, _ in kk2):.2f})"
          f" | c3: F {F3c:.3f}, need {n3:.3f} (per replicate {min(t - F3c for _, t, _ in kk3):.2f}-{max(t - F3c for _, t, _ in kk3):.2f})"
          f" | need_3/need_2 = {n3 / n2:.3f}, 90 % [{lo:.3f}, {hi:.3f}]")

    # ── 2. decomposition ──
    P("\n== 2. need = L - N.  L = T-hat-min(W95) at the knee's count - F at the knee's count (the load rise at equal n);"
      " N = F at the floor's count - F at the knee's count (the floor's sample-count drop). Registered counts, u-matched floor ==")
    dec = {}
    for c, fns, kns in ((c2, None, None), (c3, NF, NK)):
        Ff, _ = c.floor(fns, "u")
        Fk, _ = c.floor(NK, "u")
        ks = c.knees(kns)
        T = med([t for _, t, _ in ks])
        nF = med([rowF(r, fns)[2] for r in c.floor_rows[FLOOR[c.cell][0]]])
        nK = med([rowF(r, NK)[2] for r in c.floor_rows[FLOOR[c.cell][0]]])
        dec[c.cell] = (T - Fk, Ff - Fk)
        P(f"  {c.cell}: T-hat-min(W95) {T:.3f}; F(n_F ~{nF:.0f}) {Ff:.3f}; F(n_K ~{nK:.0f}) {Fk:.3f}  ->  L {T - Fk:.3f}, N {Ff - Fk:.3f}, need = L - N {T - Ff:.3f}")
    L2, N2 = dec[C2]
    L3, N3 = dec[C3]
    rl, rn = [], []
    for d2, d3 in draws:
        a = med([t for _, t, _ in d2.knees(None)]) - d2.floor(NK, "u")[0]
        b = med([t for _, t, _ in d3.knees(NK)]) - d3.floor(NK, "u")[0]
        rl.append(b / a)
        rn.append((d3.floor(NF, "u")[0] - d3.floor(NK, "u")[0]) / (d2.floor(None, "u")[0] - d2.floor(NK, "u")[0]))
    P(f"  L_3/L_2 = {L3 / L2:.3f} 90 % [{q90(rl)[0]:.3f}, {q90(rl)[1]:.3f}];  N_3/N_2 = {N3 / N2:.3f} 90 % [{q90(rn)[0]:.3f}, {q90(rn)[1]:.3f}]")
    P("  floor F against the sample count (median over the floor pool's rows; the floor pool W16 at c2, W12 at c3):")
    for c in (c2, c3):
        W = FLOOR[c.cell][0]
        P(f"    {c.cell} W{W}: " + " | ".join(f"G({ns}) F {med([rowF(r, ns)[1] for r in c.floor_rows[W]]):.3f} n {med([rowF(r, ns)[2] for r in c.floor_rows[W]]):.0f}"
                                          for ns in (None, 250, 600, 1200, 2100, 4200)))
    P("  complete groups per row under G(2100) at the floor pools (median over rows; the registration's floor rule asks >= 6 groups under G(250)):")
    for c in (c2, c3):
        P(f"    {c.cell}: " + " | ".join(f"W{W} {med([rowG(r, NK) for r in rs]):.0f} (min {min(rowG(r, NK) for r in rs)})" for W, rs in c.floor_rows.items()))

    # ── 2d. replication of N and L on 61a's first session (rep61a; another c3 session, same floor pools; knee arms 104 / 112 / 124, P1 136 as the
    #        path exercise counted them, control W2048 by goodput: s61a Result's post-hoc reading) ──
    if len(argv) > 4:
        rowsA = load(argv[4:5])
        cA = build(rowsA, C3, karms=(104, 112, 124), p1=136, ctl=2048)
        P("\n== 2d. Replication on rep61a (s61's first c3 session; post hoc: knee arms 104 / 112 / 124 + P1 136 as the path exercise counted them,"
          " control W2048 by goodput, CR61's ratio clause lifted) ==")
        FfA, SfA = cA.floor(NF, "u")
        FkA, _ = cA.floor(NK, "u")
        kA = cA.knees(NK)
        TA = med([t for _, t, _ in kA])
        P(f"  kappa {cA.kappa:.5f}, W* {cA.w_star():.1f}; scoreable replicates {len(kA)} (W95 " + " ".join(f"{w:.1f}" for w, _, _ in kA) + ")")
        P(f"  T-hat-min(W95) {TA:.3f}; F(G250) {FfA:.3f} (S_F {SfA:.3f}); F(G2100) {FkA:.3f}  ->  need {TA - FfA:.3f} (rho {(TA - FfA) / SfA:.3f}), L {TA - FkA:.3f}, N {FfA - FkA:.3f}"
              f"   (rep61c: need 4.434, L 8.872, N 4.438)")
        P(f"  ratios to c2: need {(TA - FfA) / 4.049:.3f}, L {(TA - FkA) / L2:.3f}, N {(FfA - FkA) / N2:.3f}")
        P("  floor F against the sample count, W12: " + " | ".join(f"G({ns}) {med([rowF(r, ns)[1] for r in cA.floor_rows[12]]):.3f}" for ns in (None, 250, 600, 1200, 2100, 4200)))

    # ── 2b. the c3-shaped floor against load, with c8 leg 1's lowest-load rows (same netem line, another session, a coupled leg) ──
    P("\n== 2b. The c3-shaped floor F against link utilisation at n 250 and n 2 100: c8 leg 1 (cli1, the same netem line; s60, coupled to leg 0) and c3's own"
      " floor pools. Post hoc: c8 leg 1's rows are another session and a leg that shares its sender ==")
    leg1 = []
    for W in (64, 96):
        rs = [r for r in rows.values() if r.cell == C8 and r.kind == "G" and r.W == W]
        leg1 += rs
        P(f"  c8 leg 1 W{W}: u_link {med([link_u(r, 'cli1') for r in rs]):.3f}  F(G250) {med([rowF(r, NF, 1)[1] for r in rs]):.3f} (n {med([rowF(r, NF, 1)[2] for r in rs]):.0f})"
          f"  F(G2100) {med([rowF(r, NK, 1)[1] for r in rs]):.3f} (n {med([rowF(r, NK, 1)[2] for r in rs]):.0f})")
    for W in FLOOR[C3]:
        rs = c3.floor_rows[W]
        P(f"  c3 W{W}:      u_link {med([link_u(r, 'cli0') for r in rs]):.3f}  F(G250) {med([rowF(r, NF)[1] for r in rs]):.3f}  F(G2100) {med([rowF(r, NK)[1] for r in rs]):.3f}")
    # leg 1's own slope over its low-load arms (W 64 / 96 / 256), against c3's fitted floor line converted to ms per unit u (b / kappa)
    for ns, lab in ((NF, "G250"), (NK, "G2100")):
        xs, ys = [], []
        for W in (64, 96, 256):
            for r in [r for r in rows.values() if r.cell == C8 and r.kind == "G" and r.W == W]:
                u, f_ = link_u(r, "cli1"), rowF(r, ns, 1)[1]
                if u is not None and f_ is not None:
                    xs.append(u)
                    ys.append(f_)
        b1, a1, se1 = ols(xs, ys)
        pts = [(W, rowF(r, ns)[1]) for W, rs in c3.floor_rows.items() for r in rs]
        b3, a3, se3 = ols([W for W, _ in pts], [f for _, f in pts])
        P(f"  {lab}: c8 leg 1 floor slope over u_link 0.03-0.31 {b1:.2f} +- {se1:.2f} ms per unit u (intercept {a1:.2f});"
          f" c3's floor line {b3:.4f} ms/slot / kappa = {b3 / c3.kappa:.2f} +- {se3 / c3.kappa:.2f} ms per unit u (intercept {a3:.2f})")
    P("  level at matched u_link (G2100): c8 leg 1 sits 2.2-2.6 ms below c3 at u 0.55-0.9 (section 4 lines), so leg 1's floor is a leg offset, not c3's"
      " no-load floor; it is not substituted for c3's")
    T3 = med([t for _, t, _ in k3])

    # ── 2c. the knee term against each spread, per cell, at equal n ──
    P("\n== 2c. k = L / X per cell at equal n (X a floor spread or the netem jitter range 2J), with the floor at the u-matched position and at the"
      " intercept; k_3 / k_2 is the shape test (1 = X carries L at both cells). The S_F rows read S_F at their own count (stated), L at n 2 100 ==")
    Lc = {}
    for c in (c2, c3):
        T = med([t for _, t, _ in c.knees(None if c.cell == C2 else NK)])
        Lc[c.cell] = {"u": T - c.floor(NK, "u")[0], "0": T - c.floor(NK, "0")[0]}
    jr = {C2: 6.0, C3: 10.0}
    p80 = {c: med([med([w["p90"] - w["p10"] for w in r.late(0)]) for r in arm(rows, c, FLOOR[c][0])]) for c in (C2, C3)}
    Xs = (("S_F (registered counts, u-matched)", {C2: c2.floor(None, "u")[1], C3: c3.floor(NF, "u")[1]}),
          ("S_F at n 2 100 (u-matched)", {C2: c2.floor(NK, "u")[1], C3: c3.floor(NK, "u")[1]}),
          ("floor p90 - p10 (native)", p80),
          ("netem 2J (one way)", jr))
    for nm, X in Xs:
        parts = []
        for where, lab in (("u", "u-matched"), ("0", "intercept")):
            k2_, k3_ = Lc[C2][where] / X[C2], Lc[C3][where] / X[C3]
            parts.append(f"{lab}: k_2 {k2_:.2f} k_3 {k3_:.2f} ratio {k3_ / k2_:.2f}")
        P(f"  {nm:<36} c2 {X[C2]:.3f} c3 {X[C3]:.3f} | " + " | ".join(parts))
    P("  s61's falsifier re-read with L: rho_L = L / S_F, d = rho_L,3 - rho_L,2 with its 90 % bootstrap (the draws of section 1); s61's margin Delta = 0.085."
      " MIXED counts: L at n 2 100, S_F at the registered counts (c2 native, c3 G(250)), both at the same floor position; then S_F at n 2 100 (one count)")
    for sfc, slab in (("reg", "S_F at registered counts (mixed)"), ("nk", "S_F at n 2 100 (one count)")):
        for where, lab in (("u", "u-matched"), ("0", "intercept")):
            def rl_(c, kns, fs):
                T = med([t for _, t, _ in c.knees(kns)])
                S_ = floor_conv(c, fs, where)[1] if sfc == "reg" else c.floor(NK, where)[1]
                return (T - c.floor(NK, where)[0]) / S_
            pt = rl_(c3, NK, NF) - rl_(c2, None, None)
            d = sorted(rl_(d3, NK, NF) - rl_(d2, None, None) for d2, d3 in draws)
            P(f"    {slab}, {lab}: rho_L,2 {rl_(c2, None, None):.3f}  rho_L,3 {rl_(c3, NK, NF):.3f}  d {pt:+.3f} 90 % [{q90(d)[0]:+.3f}, {q90(d)[1]:+.3f}]")

    # ── 3. candidates ──
    P("\n== 3. Candidate scalings X (need = k X, k fitted at c2): X at both cells, X_3/X_2, the two-point exponent alpha = ln(need ratio)/ln(X ratio) under the"
      " registered convention, and the conventions whose 90 % interval on need_3/need_2 contains X_3/X_2 ==")

    def knee_rows(cell, field):
        return med([native_q(r, field) for W in KARMS[cell] for r in arm(rows, cell, W)])

    def at_knee_rows(cell, fn):
        """median over the knee arm nearest the median W95 (c2 W176 ~ 168, c3 W108 ~ 108.7)"""
        W = 176 if cell == C2 else 108
        return med([fn(r) for r in arm(rows, cell, W) if fn(r) is not None])

    def dgram_ms(r):
        q = r.qd.get("cli0")
        return q["bytes"] / q["pkt"] * 8 / q["rate"] * 1e3

    def skb_ms(r):
        return dgram_ms(r) * r.gso[0][2]

    def skb_gap_ms(r):
        q = r.qd.get("cli0")
        return r.secs / (q["pkt"] / r.gso[0][2]) * 1e3

    def ack_gap_ms(r):
        q = r.qd.get("srv0")
        return r.secs / q["pkt"] * 1e3

    jit = {c: med([r.qd["cli0"]["jitter"] for r in arm(rows, c, FLOOR[c][0])]) for c in (C2, C3)}
    dly = {c: med([r.qd["cli0"]["delay"] for r in arm(rows, c, FLOOR[c][0])]) for c in (C2, C3)}
    p8010 = {c: med([med([w["p90"] - w["p10"] for w in r.late(0)]) for r in arm(rows, c, FLOOR[c][0])]) for c in (C2, C3)}
    SK = {c.cell: c.floor(NK, "u")[1] for c in (c2, c3)}
    w95 = {C2: med([w for w, _, _ in c2.knees(None)]), C3: med([w for w, _, _ in c3.knees(NK)])}
    sig = {C2: 1.785, C3: 4.778}                       # frozen sigma_2; sigma_3 as printed by the score (SIGMA-DISCRIMINATION)
    need_reg = {C2: conv_ci[CONV[0][0]][3], C3: conv_ci[CONV[0][0]][4]}
    cand = [
        ("constant in ms", 1.0, 1.0, "the A(c) family's sentence"),
        ("S_F (floor p50 - min, registered counts)", S2, S3, "s61 registered"),
        ("S_F at the knee's count (G(2100) floors)", SK[C2], SK[C3], "post hoc"),
        ("floor central spread p90 - p10 (native windows)", p8010[C2], p8010[C3], "[WSTAT] p10/p90"),
        ("sigma at the knee (P90 - P50 at load)", sig[C2], sig[C3], "frozen / score"),
        ("netem jitter half-range J (one way, each direction)", jit[C2], jit[C3], "QDISC line"),
        ("loop floor F_c", F2, F3, "registered floors"),
        ("netem propagation RTT 2 x delay", 2 * dly[C2], 2 * dly[C3], "QDISC line"),
        ("knee window W95 in slots (BDP in slots)", w95[C2], w95[C3], "knee crossing"),
        ("datagram service time at the link rate", at_knee_rows(C2, dgram_ms), at_knee_rows(C3, dgram_ms), "QDISC bytes/pkt / rate"),
        ("per-skb service time (gso x datagram time)", at_knee_rows(C2, skb_ms), at_knee_rows(C3, skb_ms), "QDISC + [TRUTH] gso"),
        ("skb inter-arrival at the knee", at_knee_rows(C2, skb_gap_ms), at_knee_rows(C3, skb_gap_ms), "QDISC + [TRUTH]"),
        ("ack inter-arrival at the knee (srv0 packets)", at_knee_rows(C2, ack_gap_ms), at_knee_rows(C3, ack_gap_ms), "QDISC srv0"),
        ("need - sigma (A(c)'s c_B, constant if A(c) held)", need_reg[C2] - sig[C2], need_reg[C3] - sig[C3], "derived"),
    ]
    nr = need_reg[C3] / need_reg[C2]
    P(f"  need ratio (registered) {nr:.3f}; L ratio (equal n) {L3 / L2:.3f}")
    for name, x2, x3, src in cand:
        xr = x3 / x2 if x2 else float("nan")
        alpha = math.log(nr) / math.log(xr) if xr > 0 and abs(math.log(xr)) > 1e-9 else float("nan")
        inside = [lab.split(" (")[0].split(",")[0] + ("@W0" if "intercept" in lab else "") for lab, (r_, lo, hi, _, _) in conv_ci.items() if lo <= xr <= hi]
        inL = q90(rl)[0] <= xr <= q90(rl)[1]
        P(f"  {name:<52} c2 {x2:8.3f}  c3 {x3:8.3f}  X3/X2 {xr:6.3f}  alpha {alpha:6.2f}  [{src}]  inside: {', '.join(inside) or 'none'}{' | L-ratio CI: yes' if inL else ''}")

    # ── 4. link utilisation ──
    P("\n== 4. Per-leg link utilisation u_link = netem Sent bytes x 8 / (transfer s x rate) (counts FEC repair and retransmissions; post hoc instrument) ==")
    for cell in (C2, C3):
        cc = c2 if cell == C2 else c3
        per = []
        for w, t, key in cc.knees(None if cell == C2 else NK):
            wm = cc.reps[key]
            pts = [(W, link_u(r, "cli0")) for W, r in wm.items() if link_u(r, "cli0") is not None and W != CTL[cell]]
            per.append(interp(pts, w))
        P(f"  {cell} validation: u_link at each replicate's W95 " + " ".join(f"{x:.3f}" for x in per) + f" (median {med(per):.3f})")
    for cell, devs in ((C8, ("cli0", "cli1")), (C1S, ("cli0",))):
        Ws = sorted({r.W for r in rows.values() if r.cell == cell})
        for leg, dev in enumerate(devs):
            line = []
            for W in Ws:
                rs = [r for r in rows.values() if r.cell == cell and r.W == W]
                u = med([link_u(r, dev) for r in rs])
                if u is None:
                    continue
                gF = med([rowF(r, NF, leg)[1] for r in rs])
                gK = med([rowF(r, NK, leg)[1] for r in rs])
                nat = med([native_q(r, "tmin", leg) for r in rs])
                line.append(f"W{W} u {u:.3f} tmin nat {nat:.2f} G250 {gF:.2f} G2100 {gK:.2f}" if gF and gK else f"W{W} u {u:.3f} tmin nat {nat:.2f}")
            jl = med([r.qd[dev]["jitter"] for r in rows.values() if r.cell == cell and r.qd.get(dev)])
            P(f"  {cell} leg {leg} ({dev}, netem jitter +-{jl:g} ms): " + " | ".join(line))

    # ── 5. forms replayed ──
    P("\n== 5. Allowance forms replayed (post hoc). D = need - A per replicate (registered convention), median and range; R67 at the controls (single leg,"
      " r68allow's formula: sum nk (F + A + tp50 - tmin) / sum nk tp50 over LATE windows, median; must stay < 1); c1s: A at its floor pool and R67 at its"
      " W2048 controls (direction only: s60 flags every c1s control COLLAPSED) ==")
    nmax = max(max(t - F2 for _, t, _ in k2), max(t - F3 for _, t, _ in k3))
    c1 = ra.calibrate(rows, C1S, 0)
    c1_8010 = med([med([w["p90"] - w["p10"] for w in r.late(0)]) for r in arm(rows, C1S, 64)])
    floors = {C2: (F2, S2, p8010[C2]), C3: (F3, S3, p8010[C3]), C1S: (c1["F"], c1["S"], c1_8010)}
    kJ = need_reg[C2] / p8010[C2]
    forms = (("K   g*c_K + q*F, c_K = max replicate need %.2f ms (UNDERIVED constant)" % nmax, lambda F, S, J, g, q: g * nmax + q * F),
             ("S   g*S_F + q*F (decision 8, for reference)", lambda F, S, J, g, q: g * S + q * F),
             ("Jc  g*k_J*(p90-p10)_F + q*F, k_J = %.3f fitted at c2" % kJ, lambda F, S, J, g, q: g * kJ * J + q * F))

    def r67_single(r, F, A):
        vals = []
        for w in r.late(0):
            vals.append((F + A + w["tp50"] - w["tmin"]) / w["tp50"])
        return med(vals)

    for name, fA in forms:
        P(f"  {name}")
        for qn, q in QS:
            g = 1.0
            parts = []
            for cell, ks in ((C2, k2), (C3, k3)):
                F, S, J = floors[cell]
                A = fA(F, S, J, g, q)
                D = [t - F - A for _, t, _ in ks]
                parts.append(f"{cell} A {A:.2f} D {med(D):+.2f} [{min(D):+.2f}, {max(D):+.2f}]")
            P(f"     g=1 {qn:<8} " + " | ".join(parts))
        for cell in (C2, C3, C1S):
            F, S, J = floors[cell]
            A = fA(F, S, J, 1.0, QB)
            ctl = arm(rows, cell, 2048 if cell != C3 else 512)
            R = [r67_single(r, F, A) for r in ctl if r.late(0)]
            P(f"     Bulk {cell}: floor F {F:.3f}, A {A:.3f} ms (A / F {A / F:.2f}); control R67 median {med(R):.3f} max {max(R):.3f}")

    # ── 5b. form S (g = 1) under every convention, S_F and F-hat on the same windows and floor position ──
    P("\n== 5b. Form S at g = 1, A = S_F + q*F-hat, under each convention of section 1 with S_F and F-hat read on the same windows and floor position"
      " (decision 8: S_F on the windows F-hat is read on). D = need - A per replicate: median [min, max]; Bulk and Realtime ==")
    for lab, f2, k2n, f3, k3n, where in CONV:
        parts = []
        for c, fns, kns in ((c2, f2, k2n), (c3, f3, k3n)):
            n_, F, S, ks = need(c, fns, kns, where)
            for qn, q in (("Bulk", QB), ("Realtime", QR)):
                D = [t - F - (S + q * F) for _, t, _ in ks]
                parts.append(f"{c.cell} {qn} S {S:.2f} F {F:.2f} D {med(D):+.2f} [{min(D):+.2f}, {max(D):+.2f}]")
        P(f"  {lab}\n     " + " | ".join(parts))
    if len(argv) > 4:
        round2(rows, c2, c3, argv)


# ═════════════════════════════ round 2 (status §61a re-run Result, Erratum 1) ═════════════════════════════
def span_groups(wins, k):
    """k consecutive LATE windows pooled (time span k x 250 ms), the last incomplete group dropped -> [(min tmin, sum nk)]."""
    wins = [w for w in wins if w["tmin"] is not None]
    return [(min(w["tmin"] for w in wins[i:i + k]), sum(w["nk"] for w in wins[i:i + k])) for i in range(0, len(wins) - k + 1, k)]


def ack_rate(r):
    q = r.qd.get("srv0") if getattr(r, "qd", None) else None
    return q["pkt"] / r.secs if q and r.secs else None


def row_q(r, key="p10"):
    """count-free statistic of one row: the median over its native windows (nk >= 10) of the window's own quantile `key`."""
    return med([w[key] for w in r.late(0) if w.get(key) is not None])


def q_floor(c, key, where="u"):
    pts = [(W, row_q(r, key)) for W, rs in c.floor_rows.items() for r in rs]
    pts = [(W, v) for W, v in pts if v is not None]
    if c.cell == C2 and where == "u":
        return med([v for W, v in pts if W == 16])
    b, a, _ = ols([W for W, _ in pts], [v for _, v in pts])
    return a + b * (c.w_star() if where == "u" else 0.0)


def q_knees(c, key):
    out = []
    for w95, _, k_ in c.knees(None):
        wm = c.reps[k_]
        arms = [W for W in tuple(c.karms) + (c.p1,) if W in wm]
        out.append(interp([(W, row_q(wm[W], key)) for W in arms], w95))
    return out


def round2(rows, c2, c3c, argv):
    rowsA = load(argv[4:5])
    c3a = build(rowsA, C3, karms=(104, 112, 124), p1=136, ctl=2048)
    rowsB = load(argv[5:6]) if len(argv) > 5 else {}          # rep61b on its own: its W160 / W2048 keys collide with s60's c2 rows
    sess = (("61c", c3c, rows), ("61a", c3a, rowsA))
    rnd = random.Random(63)
    NB = 1000

    # ── 6. session-spanning bands ──
    P("\n== 6. Round 2: every ratio on both c3 sessions (61c the scored re-run; 61a the first session, post hoc: K 104 / 112 / 124, P1 136, control"
      " W2048 by goodput). Per session: point and 90 % bootstrap (1 000 draws, seed 63, c2 and that session resampled). BAND = the union of the two"
      " sessions' intervals: a session-to-session spread, not one session's bootstrap ==")
    dr = {lab: [(resample(c2, rnd), resample(c3, rnd)) for _ in range(NB)] for lab, c3, _ in sess}
    bands = {}
    for clab, f2, k2n, f3, k3n, where in CONV:
        parts, los, his = [], [], []
        for lab, c3, _ in sess:
            pt = need(c3, f3, k3n, where)[0] / need(c2, f2, k2n, where)[0]
            lo, hi = q90([need(d3, f3, k3n, where)[0] / need(d2, f2, k2n, where)[0] for d2, d3 in dr[lab]])
            los.append(lo)
            his.append(hi)
            parts.append(f"{lab} {pt:.3f} [{lo:.3f}, {hi:.3f}] (need_3 {need(c3, f3, k3n, where)[0]:.3f})")
        bands[clab] = (min(los), max(his))
        P(f"  {clab}\n     " + " | ".join(parts) + f"  ->  BAND [{min(los):.3f}, {max(his):.3f}]")
    for nm, fn in (("L (u-matched)", lambda c, kn, fs: med([t for _, t, _ in c.knees(kn)]) - c.floor(NK, "u")[0]),
                   ("N (u-matched)", lambda c, kn, fs: floor_conv(c, fs, "u")[0] - c.floor(NK, "u")[0])):
        parts, los, his = [], [], []
        for lab, c3, _ in sess:
            pt = fn(c3, NK, NF) / fn(c2, None, None)
            lo, hi = q90([fn(d3, NK, NF) / fn(d2, None, None) for d2, d3 in dr[lab]])
            los.append(lo)
            his.append(hi)
            parts.append(f"{lab} {pt:.3f} [{lo:.3f}, {hi:.3f}] ({fn(c3, NK, NF):.3f} ms against c2's {fn(c2, None, None):.3f})")
        bands[nm] = (min(los), max(his))
        P(f"  {nm} ratio c3/c2: " + " | ".join(parts) + f"  ->  BAND [{min(los):.3f}, {max(his):.3f}]")
    P("  candidates: X_3/X_2 per session (61c / 61a), and whether that pair overlaps each BAND, in the order: registered / native / equal-n /"
      " registered@W0 / equal-n@W0 / L")

    def p80(c):
        W = FLOOR[c.cell][0]
        return med([med([w["p90"] - w["p10"] for w in r.late(0)]) for r in c.floor_rows[W]])

    def sigk(c):
        return med([med([w["sig"] for w in wm[W].late(0) if w.get("sig") is not None]) for wm in c.reps.values() for W in c.karms if W in wm])

    Xf = (("constant", lambda c: 1.0),
          ("S_F (registered counts)", lambda c: floor_conv(c, None if c.cell == C2 else NF, "u")[1]),
          ("S_F at n 2 100", lambda c: c.floor(NK, "u")[1]),
          ("floor p90 - p10 (native)", p80),
          ("netem J", lambda c: 3.0 if c.cell == C2 else 5.0),
          ("sigma (median window sig over the K arms)", sigk),
          ("loop floor F (registered counts)", lambda c: floor_conv(c, None if c.cell == C2 else NF, "u")[0]))
    keys = [x[0] for x in CONV] + ["L (u-matched)"]
    for nm, fx_ in Xf:
        rs_ = [fx_(c3) / fx_(c2) for _, c3, _ in sess]
        lo, hi = min(rs_), max(rs_)
        ov = ["yes" if not (hi < bands[k][0] or lo > bands[k][1]) else "no" for k in keys]
        P(f"    {nm:<44} X3/X2 {' / '.join(f'{x:.3f}' for x in rs_)}  overlaps: " + " ".join(ov))

    # ── 7. the floor's drop: sample count, time span, or ack count ──
    P("\n== 7. What the floor minimum's drop follows. Per arm, k consecutive LATE windows are pooled (span k x 250 ms). The doubling drop d = F(k) - F(2k)"
      " is regressed on ln of each candidate abscissa at the step (samples n, span k, acks a = the arm's srv0 packet rate x span), pooled over a"
      " cell's arms below 0.8 u. The arm's load cancels in d. The smallest residual sd is the better descriptor ==")
    cellsets = (("c2 (s60 16 / 32 / 48 + 61b 96 / 128 / 144)", C2, ((rows, (16, 32, 48)), (rowsB, (96, 128, 144)))),
                ("c3 61c", C3, ((rows, (12, 16, 24, 60, 80)),)), ("c3 61a", C3, ((rowsA, (12, 16, 24, 68)),)))
    for lab, cell, rsets in cellsets:
        pts = {"n": [], "k": [], "a": []}
        lines = []
        for rs_dict, Ws in rsets:
            for W in Ws:
                rs = [r for r in rs_dict.values() if r.cell == cell and r.kind == "G" and r.W == W]
                if not rs:
                    continue
                ar = med([ack_rate(r) for r in rs])
                Fk = {}
                for k in (1, 2, 4, 8, 16, 32, 64):
                    v = [span_groups(sp.late_all(r, 0), k) for r in rs]
                    if any(len(g) < 2 for g in v):
                        break
                    Fk[k] = (med([med([t for t, _ in g]) for g in v]), med([med([n for _, n in g]) for g in v]))
                lines.append(f"    W{W} (acks {ar:.0f}/s): " + " | ".join(f"k{k} {f_:.2f} (n {n_:.0f})" for k, (f_, n_) in Fk.items()))
                ks = sorted(Fk)
                for k1, k2_ in zip(ks, ks[1:]):
                    d = Fk[k1][0] - Fk[k2_][0]
                    km = math.sqrt(k1 * k2_)
                    pts["n"].append((math.log(math.sqrt(Fk[k1][1] * Fk[k2_][1])), d))
                    pts["k"].append((math.log(km), d))
                    pts["a"].append((math.log(ar * 0.25 * km), d))
        P(f"  {lab}:")
        for ln_ in lines:
            P(ln_)
        res = []
        for v, nm in (("n", "samples"), ("k", "span"), ("a", "acks")):
            xs, ys = [x for x, _ in pts[v]], [y for _, y in pts[v]]
            b, a, _ = ols(xs, ys)
            sd = math.sqrt(sum((y - a - b * x) ** 2 for x, y in zip(xs, ys)) / (len(xs) - 2))
            res.append(f"{nm}: resid sd {sd:.3f} ms (slope {b:+.3f} per ln)")
        P(f"    doubling drops pooled over arms ({len(pts['n'])} steps): " + " | ".join(res))
    P("  per floor pool: the count-matched drop F(G250) - F(G2100) against the span-matched drop F(k 4) - F(k 32) (1 s -> 8 s), median over rows:")
    for lab, c, _ in sess + (("c2", c2, rows),):
        parts = []
        for W, rs in c.floor_rows.items():
            cm = med([rowF(r, NF)[1] - rowF(r, NK)[1] for r in rs])
            sm = med([med([t for t, _ in span_groups(sp.late_all(r, 0), 4)]) - med([t for t, _ in span_groups(sp.late_all(r, 0), 32)]) for r in rs])
            parts.append(f"W{W} count {cm:.2f} span {sm:.2f}")
        P(f"    {lab}: " + " | ".join(parts))

    # ── 8. does L move with the count? ──
    P("\n== 8. SUPERSEDED by 8b (G(n*) closes on whole windows, so at n* <= 1 200 the c3 knee realises more samples than the floor). L at several nominal counts n* (floor on its u-matched line at G(n*), knee arms at G(n*)). c3 knee windows hold ~440-480 natively and c2's"
      " ~2 250-2 400, so c2 starts at its native knee against a G(2 100) floor ==")
    for lab, c3, _ in sess:
        parts = []
        for ns in (600, 1200, 2100, 4200, 8400):
            T = med([t for _, t, _ in c3.knees(ns)])
            F = c3.floor(ns, "u")[0]
            parts.append(f"n*{ns}: T {T:.2f} F {F:.2f} L {T - F:.2f}")
        P(f"  c3 {lab}: " + " | ".join(parts))
    parts = []
    for kn, fns in ((None, NK), (4800, 4800), (9600, 9600)):
        T = med([t for _, t, _ in c2.knees(kn)])
        F = c2.floor(fns, "u")[0]
        parts.append(f"n*{fns}: T {T:.2f} F {F:.2f} L {T - F:.2f}")
    P("  c2: " + " | ".join(parts))

    # ── 9. a count-free statistic ──
    P("\n== 9. A count-free statistic: the median over a row's native windows of the window's own p10 (or p50) from [WSTAT], a fixed rank fraction."
      " Check 1: within an arm, windows below against above the row's median nk: the p10 shift beside the tmin shift (count-free -> ~0)."
      " Check 2: need_q = the quantile at the knee - at the floor, on both sessions ==")
    for lab, c, _ in sess + (("c2", c2, rows),):
        parts = []
        for W, rs in c.floor_rows.items():
            dq, dt, dn = [], [], []
            for r in rs:
                ws = [w for w in r.late(0) if w.get("p10") is not None]
                mn = med([w["nk"] for w in ws])
                lo_ = [w for w in ws if w["nk"] < mn]
                hi_ = [w for w in ws if w["nk"] > mn]
                if len(lo_) < 4 or len(hi_) < 4:
                    continue
                dq.append(med([w["p10"] for w in lo_]) - med([w["p10"] for w in hi_]))
                dt.append(med([w["tmin"] for w in lo_]) - med([w["tmin"] for w in hi_]))
                dn.append(med([w["nk"] for w in hi_]) / med([w["nk"] for w in lo_]))
            if dn:
                parts.append(f"W{W}: nk x{med(dn):.2f} -> p10 {med(dq):+.2f}, tmin {med(dt):+.2f}")
        P(f"  check 1 {lab}: " + " | ".join(parts))
    for key in ("p10", "p50"):
        q2f = q_floor(c2, key)
        q2 = med([t - q2f for t in q_knees(c2, key)])
        parts = [f"c2 floor {q2f:.2f} need_q {q2:.3f}"]
        for lab, c3, _ in sess:
            f3 = q_floor(c3, key)
            n3 = med([t - f3 for t in q_knees(c3, key)])
            f30 = q_floor(c3, key, "0")
            n30 = med([t - f30 for t in q_knees(c3, key)])
            parts.append(f"c3 {lab} floor {f3:.2f} need_q {n3:.3f} (x{n3 / q2:.3f}; at the intercept {n30:.3f}, x{n30 / q2:.3f})")
        P(f"  check 2, {key}: " + " | ".join(parts))
        for lab, c3, _ in sess:
            def nq(c):
                f_ = q_floor(c, key)
                return med([t - f_ for t in q_knees(c, key)])
            lo, hi = q90([nq(d3) / nq(d2) for d2, d3 in dr[lab]])
            P(f"    {key} {lab}: need_q ratio 90 % [{lo:.3f}, {hi:.3f}] (the section-6 draws)")
        for lab, c3, _ in sess:
            f3 = q_floor(c3, key)
            n3 = med([t - f3 for t in q_knees(c3, key)])
            P(f"    {key} {lab}: k_q = need_q / X, c3/c2: S_F {(n3 / floor_conv(c3, NF, 'u')[1]) / (q2 / 5.083):.3f}"
              f" | J {(n3 / 5) / (q2 / 3):.3f} | p90-p10 {(n3 / p80(c3)) / (q2 / p80(c2)):.3f} | constant {n3 / q2:.3f}")
    round2b(rows, c2, c3c, c3a, rowsA, dr, sess)


# ───────────────────────────── round 2, after review: tests with power ─────────────────────────────
def strided_groups(wins, k, s):
    """groups of k windows taken at stride s inside blocks of k*s consecutive windows (s interleaved groups per block): same count per group as
    contiguous k-pooling, span k*s windows. -> [min tmin]"""
    wins = [w for w in wins if w["tmin"] is not None]
    out = []
    for b in range(0, len(wins) - k * s + 1, k * s):
        blk = wins[b:b + k * s]
        for o in range(s):
            out.append(min(w["tmin"] for w in blk[o::s]))
    return out


def shuffled_groups(wins, k, seed):
    wins = [w for w in wins if w["tmin"] is not None]
    rr = random.Random(seed)
    ws = wins[:]
    rr.shuffle(ws)
    return [min(w["tmin"] for w in ws[i:i + k]) for i in range(0, len(ws) - k + 1, k)]


def lag1(v):
    m = sum(v) / len(v)
    num = sum((a - m) * (b - m) for a, b in zip(v, v[1:]))
    den = sum((a - m) ** 2 for a in v)
    return num / den if den else None


def rowS(r, k):
    """median over contiguous k-window groups of the group minimum, and the median realised count."""
    g = span_groups(sp.late_all(r, 0), k)
    return (med([t for t, _ in g]), med([n for _, n in g]), len(g)) if g else (None, None, 0)


def round2b(rows, c2, c3c, c3a, rowsA, dr, sess):
    # ── 7b. span against count at a fixed count (within arm) ──
    P("\n== 7b. Span against count at a FIXED count, within each floor arm: k windows pooled contiguously (span k), at stride 2 and 4 (span 2k, 4k;"
      " interleaved, so the same count and group number), and after a seeded shuffle (span = the whole row). Median over rows of the median group"
      " minimum. Equal => the count sets the minimum; strided / shuffled lower => the span (time correlation) matters. Beside it the lag-1"
      " autocorrelation of the native window minima, and acks per echo sample ==")
    cases = (("c2", c2, rows, 8), ("61c", c3c, rows, 8), ("61a", c3a, rowsA, 8))
    for lab, c, rs_dict, k in cases:
        parts = []
        for W, rs in c.floor_rows.items():
            con = med([med(strided_groups(sp.late_all(r, 0), k, 1)) for r in rs])
            s2 = med([med(strided_groups(sp.late_all(r, 0), k, 2)) for r in rs])
            s4 = med([med(strided_groups(sp.late_all(r, 0), k, 4)) for r in rs])
            sh = med([med(shuffled_groups(sp.late_all(r, 0), k, 7 + i)) for i, r in enumerate(rs)])
            ac = med([lag1([w["tmin"] for w in sp.late_all(r, 0) if w["tmin"] is not None]) for r in rs])
            aps = med([ack_rate(r) / (med([w["nk"] for w in r.late(0)]) / 0.25) for r in rs])
            parts.append(f"W{W}: contiguous {con:.2f} | stride2 {s2:.2f} | stride4 {s4:.2f} | shuffled {sh:.2f} | lag-1 r {ac:+.2f} | acks/sample {aps:.2f}")
        P(f"  {lab} (k = {k} windows):")
        for p_ in parts:
            P(f"    {p_}")
    P("  c1s (netem jitter 0; s60 floor arms, native windows hold ~5 300-12 100 samples): the floor minimum against contiguous span k:")
    for W in (64, 128, 192):
        rs = arm(rows, C1S, W)
        P(f"    W{W}: " + " | ".join(f"k{k} {med([rowS(r, k)[0] for r in rs]):.3f}" for k in (1, 2, 4, 8, 16, 32) if all(rowS(r, k)[2] >= 2 for r in rs)))
    P("  count-matched drop at the floor pools, both ways: median over rows of F(G250) - F(G2100) | difference of the arm medians:")
    for lab, c, _ in (("61c", c3c, None), ("61a", c3a, None), ("c2", c2, None)):
        P(f"    {lab}: " + " | ".join(f"W{W} {med([rowF(r, NF)[1] - rowF(r, NK)[1] for r in rs]):.2f} | {med([rowF(r, NF)[1] for r in rs]) - med([rowF(r, NK)[1] for r in rs]):.2f}"
                                    for W, rs in c.floor_rows.items()))

    # ── 8b. L on a window ladder ──
    P("\n== 8b. L on a window ladder: the knee arms pooled over k contiguous windows (k = 1 is native), the floor grouped to the knee's realised count"
      " (G(n_K)); realised n on both sides and the floor's groups per W12 / W16 row. The c3 knee cannot be read below ~460 samples, nor c2's below"
      " ~2 250 (one window) ==")
    for lab, c, ks in (("61c", c3c, (1, 2, 4, 8, 16)), ("61a", c3a, (1, 2, 4, 8, 16)), ("c2", c2, (1, 2, 4))):
        parts = []
        for k in ks:
            Ts, nks = [], []
            for w95, _, key in c.knees(None):
                wm = c.reps[key]
                arms_ = [W for W in tuple(c.karms) + (c.p1,) if W in wm]
                vals = [(W, rowS(wm[W], k)) for W in arms_]
                if any(v[0] is None for _, v in vals):
                    continue
                Ts.append(interp([(W, v[0]) for W, v in vals], w95))
                nks += [v[1] for W_, v in vals if W_ in c.karms]
            if not Ts:
                continue
            nK = med(nks)
            ns = int(nK)
            F = c.floor(ns, "u")[0]
            nF = med([rowF(r, ns)[2] for r in c.floor_rows[FLOOR[c.cell][0]]])
            gF = [med([rowG(r, ns) for r in c.floor_rows[W]]) for W in FLOOR[c.cell][:2]]
            T = med(Ts)
            parts.append(f"k{k}: n_K {nK:.0f} n_F {nF:.0f} (groups {gF[0]:.0f}/{gF[1]:.0f}) T {T:.2f} F {F:.2f} L {T - F:.2f}")
        P(f"  {lab}: " + " | ".join(parts))

    # ── 9b. the statistics side by side ──
    P("\n== 9b. Each candidate statistic in one table: the knee rise c3/c2 per session with its 90 % interval (the section-6 draws) and the BAND over"
      " sessions; then, within each floor arm, the shift of p10 / p50 / p90 / tmin between windows below and above the row's median nk (a location"
      " shift moves all quantiles alike; a low-rank effect moves p10 alone) ==")

    def st_min_eq(c, d=None):
        x = d or c
        return med([t for _, t, _ in x.knees(None if c.cell == C2 else NK)]) - x.floor(NK, "u")[0]

    def st_min_reg(c, d=None):
        x = d or c
        return need(x, None if c.cell == C2 else NF, None if c.cell == C2 else NK, "u")[0]

    def st_min_nat(c, d=None):
        x = d or c
        return need(x, None, None, "u")[0]

    def st_q(key):
        def f(c, d=None):
            x = d or c
            f_ = q_floor(x, key)
            return med([t - f_ for t in q_knees(x, key)])
        return f

    stats = (("window minimum, registered counts", st_min_reg), ("window minimum, engine-native", st_min_nat),
             ("window minimum, equal count (L)", st_min_eq), ("window p10", st_q("p10")), ("window p50", st_q("p50")), ("window p90", st_q("p90")))
    for nm, fn in stats:
        parts, los, his = [], [], []
        for lab, c3, _ in sess:
            pt = fn(c3) / fn(c2)
            lo, hi = q90([fn(c3, d3) / fn(c2, d2) for d2, d3 in dr[lab]])
            los.append(lo)
            his.append(hi)
            parts.append(f"{lab} {pt:.3f} [{lo:.3f}, {hi:.3f}] (c3 {fn(c3):.2f} ms)")
        P(f"  {nm:<36} c2 {fn(c2):.2f} ms | " + " | ".join(parts) + f"  ->  BAND [{min(los):.3f}, {max(his):.3f}]")
    for lab, c, _ in sess + (("c2", c2, rows),):
        parts = []
        for W, rs in c.floor_rows.items():
            sh = {k_: [] for k_ in ("p10", "p50", "p90", "tmin")}
            for r in rs:
                ws = [w for w in r.late(0) if w.get("p10") is not None and w.get("p90") is not None]
                mn = med([w["nk"] for w in ws])
                lo_ = [w for w in ws if w["nk"] < mn]
                hi_ = [w for w in ws if w["nk"] > mn]
                if len(lo_) < 4 or len(hi_) < 4:
                    continue
                for k_ in sh:
                    sh[k_].append(med([w[k_] for w in lo_]) - med([w[k_] for w in hi_]))
            if sh["p10"]:
                parts.append(f"W{W}: " + " ".join(f"{k_} {med(v):+.2f}" for k_, v in sh.items()))
        P(f"  nk split {lab}: " + " | ".join(parts))
    round2c(rows, c2, c3c, c3a, rowsA, dr, sess)


# ───────────────────────────── round 2c: one quantile statistic for floor, knee and spread ─────────────────────────────
def wq(w, key):
    """a window's quantile statistic: 'p10' / 'p50' / 'p90', or a spread 'p90-p10' / 'p50-p10' / 'p90-p50'."""
    if "-" in key:
        a, b = key.split("-")
        return None if w.get(a) is None or w.get(b) is None else w[a] - w[b]
    return w.get(key)


def row_qk(r, key):
    return med([wq(w, key) for w in r.late(0) if wq(w, key) is not None])


def group_stat(r, nstar, key):
    """the count table's reading of one row: native windows (nstar None, nk >= 10), or G(nstar) groups over LATE windows with nk >= 1; per group
    the minimum ('min') or the nk-weighted median of the windows' own statistic (a per-window statistic carried to the group)."""
    wins = r.late(0) if nstar is None else sp.late_all(r, 0)
    wins = [w for w in wins if w["tmin"] is not None and (key == "min" or wq(w, key) is not None)]
    out, cur = [], []
    for w in wins:
        cur.append(w)
        if nstar is None or sum(x["nk"] for x in cur) >= nstar:
            out.append(min(x["tmin"] for x in cur) if key == "min" else sp.wmed([(wq(x, key), x["nk"]) for x in cur]))
            cur = []
    return med(out)


def qf(c, key, where="u"):
    pts = [(W, row_qk(r, key)) for W, rs in c.floor_rows.items() for r in rs]
    pts = [(W, v) for W, v in pts if v is not None]
    if c.cell == C2 and where == "u":
        return med([v for W, v in pts if W == 16])
    b, a, _ = ols([W for W, _ in pts], [v for _, v in pts])
    return a + b * (c.w_star() if where == "u" else 0.0)


def qk(c, key):
    out = []
    for w95, _, k_ in c.knees(None):
        wm = c.reps[k_]
        arms = [W for W in tuple(c.karms) + (c.p1,) if W in wm]
        out.append(interp([(W, row_qk(wm[W], key)) for W in arms], w95))
    return out


def rho_q(c, loc, spr, where="u"):
    f_ = qf(c, loc, where)
    s_ = qf(c, spr, where)
    return med([(t - f_) / s_ for t in qk(c, loc)])


def round2c(rows, c2, c3c, c3a, rowsA, dr, sess):
    P("\n== 10. One quantile statistic for all three inputs (floor, knee reading, spread), computed on native windows ==")
    P("  10a. The count table where the minimum failed: the floor pool's median over rows (c2 W16, c3 W12) of each statistic, native and carried to"
      " G(n*) groups. For a quantile the group value is the nk-weighted median of its windows' own quantile, so it is flat by construction unless"
      " the windows' quantiles depend on their count; a true pooled quantile needs the raw samples (RWM_RTT_DUMP), which these ledgers lack ==")
    for lab, c in (("c2 W16", c2), ("c3 61c W12", c3c), ("c3 61a W12", c3a)):
        W = FLOOR[c.cell][0]
        for key in ("min", "p10", "p50", "p90", "p90-p10"):
            vals = [med([group_stat(r, ns, key) for r in c.floor_rows[W]]) for ns in (None, 250, 600, 1200, 2100, 4200)]
            P(f"    {lab:<11} {key:<8} " + " | ".join(f"{x:.2f}" for x in vals) + f"   (native -> G(4200): {vals[-1] - vals[0]:+.2f} ms)")
    P("  10b. The independent draws per window, and the quantiles they support: a sample p-quantile behaves as a central (count-free to O(1/m))"
      " quantile when m*min(p, 1-p) >= 5 independent draws lie beyond it, and as an extreme otherwise (m = acks per native floor window here,"
      " each ack one ack-path jitter draw; skbs per window similar):")
    for lab, c in (("c2", c2), ("c3 61c", c3c), ("c3 61a", c3a)):
        W = FLOOR[c.cell][0]
        m_ = med([ack_rate(r) * 0.25 for r in c.floor_rows[W]])
        sk = med([(r.qd["cli0"]["pkt"] / r.gso[0][2]) / r.secs * 0.25 for r in c.floor_rows[W] if r.gso.get(0)])
        P(f"    {lab} W{W}: m ~ {m_:.0f} acks, {sk:.0f} skbs per window -> m*0.10 = {m_ * 0.1:.1f}, m*0.25 = {m_ * 0.25:.1f}, m*0.50 = {m_ * 0.5:.1f}")
    P("  10c. rho_q = (Q(W95) - Q_F) / spread_F, every input a per-window quantile (u-matched floor; the intercept beside). d = rho_q,3 - rho_q,2 with"
      " its 90 % interval (the section-6 draws), against s61's Delta = 0.085: AGREES iff the interval lies inside (-0.085, +0.085), DISAGREES iff"
      " outside [-0.085, +0.085] ==")
    for loc, spr in (("p50", "p90-p10"), ("p10", "p90-p10"), ("p50", "p50-p10"), ("p50", "p90-p50")):
        parts = [f"rho_2 {rho_q(c2, loc, spr):.3f}"]
        for lab, c3, _ in sess:
            r3 = rho_q(c3, loc, spr)
            d = [rho_q(d3, loc, spr) - rho_q(d2, loc, spr) for d2, d3 in dr[lab][:400]]
            lo, hi = q90(d)
            lab_ = "AGREES" if -0.085 < lo and hi < 0.085 else ("DISAGREES" if hi < -0.085 or lo > 0.085 else "UNRESOLVED")
            parts.append(f"{lab} rho_3 {r3:.3f} (intercept {rho_q(c3, loc, spr, '0'):.3f}) d {r3 - rho_q(c2, loc, spr):+.3f} [{lo:+.3f}, {hi:+.3f}] {lab_}")
        P(f"    loc {loc}, spread {spr:<8}: " + " | ".join(parts))


if __name__ == "__main__":
    main(sys.argv)
