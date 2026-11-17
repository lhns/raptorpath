"""Knee-margin re-derivation (knee-margin-rederive.md): what need = T-hat-min(W95) - F scales with, across c2 and c3.

Usage (from docs/):  python -I -X utf8 derivations/store-law/kmr.py l1-raw/s60/rep60a-diag-ld.tgz l1-raw/s60/rep60b-diag-ld.tgz \
                         l1-raw/s61/rep61c-diag-ld.tgz  > derivations/store-law/kmr.out

Reads the committed ledgers in place (tarfile), with s61pred.py's window parse and grouping G(n*) (s61-registration.md section 3). Every
reading here is POST HOC: none was registered. Sections:
  0. witness: the scored c3 reading (rep61c-score.txt) and c2's frozen inputs, reproduced
  1. need at both cells under five sample-count / floor conventions, with a bootstrap interval on need_3 / need_2
  2. the decomposition need = L - N: L the load rise at the knee's sample count, N the floor's sample-count drop
  2b. the c3-shaped floor against load, with c8 leg 1's lowest-load rows
  2c. k = L / X per spread X and cell, and s61's falsifier re-read at equal n (rho_L)
  3. candidate scalings X: X at both cells, the ratio, the two-point exponent, and which convention's interval it falls in
  4. the per-leg link-utilisation instrument (netem Sent bytes): validated at the c2 / c3 knees, then c8's legs and c1s as bounds
  5. the proposed allowance forms replayed: D per hint at both cells, the R67 control sign at c2 / c3 / c1s, the c1s size
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
        _RF[k] = (s, f, n)
    return _RF[k]


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

    def __init__(self, cell, floor_rows, reps_, kappa):
        self.cell, self.floor_rows, self.reps, self.kappa = cell, floor_rows, reps_, kappa

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
            ctl = wm.get(CTL[self.cell])
            if not ctl or not ctl.gp:
                continue
            pts = [(W, r.gp / ctl.gp) for W, r in wm.items() if W != CTL[self.cell] and r.gp]
            w95 = cross_up(pts, 0.95)
            arms = [W for W in KARMS[self.cell] + (P1[self.cell],) if W in wm]
            t = interp([(W, rowF(wm[W], ns)[1]) for W in arms], w95)
            if self.cell == C3 and not (KARMS[C3][0] <= w95 <= KARMS[C3][-1]):
                continue                                                   # s61 section 2.2's scoreability rule
            out.append((w95, t, key))
        return out


def build(rows, cell):
    rp = reps(rows, cell)
    fr = {W: arm(rows, cell, W) for W in FLOOR[cell]}
    kappa = None
    if cell == C3:
        ctl_m = med([r.gp for r in arm(rows, cell, CTL[cell])])
        kappa = med([r.gp / ctl_m / W for W, rs in fr.items() for r in rs])
    return Cell(cell, fr, rp, kappa)


def resample(c, rnd):
    fr = {W: [rs[rnd.randrange(len(rs))] for _ in rs] for W, rs in c.floor_rows.items()}
    keys = list(c.reps)
    rp = {i: c.reps[keys[rnd.randrange(len(keys))]] for i in range(len(keys))}
    return Cell(c.cell, fr, rp, c.kappa)


# The conventions: (label, c2 floor ns, c2 knee ns, c3 floor ns, c3 knee ns, floor position)
CONV = (("registered (s61: c2 native; c3 G(250) floor / G(2100) knee; u-matched)", None, None, NF, NK, "u"),
        ("engine-native (every window as the engine sees it; u-matched)", None, None, None, None, "u"),
        ("equal-n (floor and knee both at the knee's count: G(2100) floors, c2 knee native, c3 knee G(2100); u-matched)", NK, None, NK, NK, "u"),
        ("registered, floor at the intercept W -> 0", None, None, NF, NK, "0"),
        ("equal-n, floor at the intercept W -> 0", NK, None, NK, NK, "0"))


def need(c, fns, kns, where):
    F, S = c.floor(fns, where)
    if c.cell == C2 and where == "0" and fns is None:
        # s61 section 5.1: c2's W -> 0 applies its matched-n (G(800)) slope to the native floor
        pts = [(W, rowF(r, 800)[1]) for W, rs in c.floor_rows.items() for r in rs]
        b, a, _ = ols([W for W, _ in pts], [f for _, f in pts])
        F0, S0 = c.floor(None, "u")
        F = F0 - b * 16
    ks = c.knees(kns)
    return med([t - F for _, t, _ in ks]), F, S, ks


def main(argv):
    rows = load(argv[1:])
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
    F1a = med([rowF(r, NF, 1)[1] for r in leg1])
    F1b = med([rowF(r, NK, 1)[1] for r in leg1])
    T3 = med([t for _, t, _ in k3])
    P(f"  with c8 leg 1's floor (u_link 0.03-0.04) in place of c3's u-matched one: registered-n need_3 = {T3:.3f} - {F1a:.3f} = {T3 - F1a:.3f}"
      f" (ratio to c2's 4.049 / intercept 4.585: {(T3 - F1a) / 4.049:.2f} / {(T3 - F1a) / 4.585:.2f}); equal-n L_3 = {T3 - F1b:.3f}"
      f" (ratio to c2's L 5.661 / intercept 6.183: {(T3 - F1b) / 5.661:.2f} / {(T3 - F1b) / 6.183:.2f})")

    # ── 2c. the knee term against each spread, per cell, at equal n ──
    P("\n== 2c. k = L / X per cell at equal n (X a floor spread or the netem jitter range 2J), with the floor at c3's u-matched line, at its intercept, and"
      " (c3 only) at c8 leg 1's no-load floor; k_3 / k_2 is the shape test (1 = the spread carries the knee at both cells) ==")
    Lc = {}
    for c in (c2, c3):
        T = med([t for _, t, _ in c.knees(None if c.cell == C2 else NK)])
        Lc[c.cell] = {"u": T - c.floor(NK, "u")[0], "0": T - c.floor(NK, "0")[0]}
    Lc[C3]["c8"] = T3 - F1b
    Lc[C2]["c8"] = Lc[C2]["0"]
    jr = {C2: 6.0, C3: 10.0}
    p80 = {c: med([med([w["p90"] - w["p10"] for w in r.late(0)]) for r in arm(rows, c, FLOOR[c][0])]) for c in (C2, C3)}
    Xs = (("S_F (registered counts, u-matched)", {C2: c2.floor(None, "u")[1], C3: c3.floor(NF, "u")[1]}),
          ("S_F at n 2 100 (u-matched)", {C2: c2.floor(NK, "u")[1], C3: c3.floor(NK, "u")[1]}),
          ("floor p90 - p10 (native)", p80),
          ("netem 2J (one way)", jr))
    for nm, X in Xs:
        parts = []
        for where, lab in (("u", "u-matched"), ("0", "intercept"), ("c8", "c3 floor from c8 leg 1")):
            k2_, k3_ = Lc[C2][where] / X[C2], Lc[C3][where] / X[C3]
            parts.append(f"{lab}: k_2 {k2_:.2f} k_3 {k3_:.2f} ratio {k3_ / k2_:.2f}")
        P(f"  {nm:<36} c2 {X[C2]:.3f} c3 {X[C3]:.3f} | " + " | ".join(parts))
    P("  the S_F row as s61's falsifier, re-read at equal n: rho_L = L / S_F (S_F at the registered counts, both on the same floor line), per cell, and"
      " d = rho_L,3 - rho_L,2 with its 90 % bootstrap (the draws of section 1); s61's margin Delta = 0.085")
    for where, lab in (("u", "u-matched"), ("0", "intercept")):
        def rl_(c, kns, fs):
            T = med([t for _, t, _ in c.knees(kns)])
            return (T - c.floor(NK, where)[0]) / c.floor(fs, where)[1]
        pt = rl_(c3, NK, NF) - rl_(c2, None, None)
        d = sorted(rl_(d3, NK, NF) - rl_(d2, None, None) for d2, d3 in draws)
        P(f"    {lab}: rho_L,2 {rl_(c2, None, None):.3f}  rho_L,3 {rl_(c3, NK, NF):.3f}  d {pt:+.3f} 90 % [{q90(d)[0]:+.3f}, {q90(d)[1]:+.3f}]")

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


if __name__ == "__main__":
    main(sys.argv)
