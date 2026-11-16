"""store-law R6.8: the allowance and the lifetime minimum on the s60 rows (store-law-r68.md).

Usage:  python -I -X utf8 r68allow.py <rep60a-diag-ld.tgz> <rep60b-diag-ld.tgz>  > r68allow.out

Reads the committed tgz in place (tarfile; nothing is extracted). Per row: the client log's [SLAW] and [WSTAT]
lines after the last warm-up marker, LATE windows (t >= 15 s at c2 / c8, >= 5 s at c1s) with nk >= 10, and the
driver's goodput (`mean_mbps`). Section 0 reproduces the s60 score (F_c, l_c, every D_r, D per arm); every later
section is built on the same window set and calibration.

    D_r(A) = (Tmin_r(W95,r) - F_c) - A        the rise the knee needs, less what allowance A supplies
    R67(A) = sum_i w_i (F_i + 1/2 max(0, max_j F_j - F_i) + A_i + T_i - Tmin_i) / sum_i w_i T_i      (s60replay.r67_r with A in place of sig + q*l_c)
"""
import json
import re
import statistics as st
import sys
import tarfile

TOK = re.compile(r"(\w+)=(\S+)")
NAME = re.compile(r"diag-ld/(c\w+-\d+)@def-([GC])(\d+)-s(\d+)-r(\d+)-a\d+-(c\.log|drv\.out)$")
C2, C8, C1S = "c2-100", "c8-100", "c1s-400"
LATE_T = {C2: 15.0, C8: 15.0, C1S: 5.0}
FLOOR_W = {C2: (16, 32, 48), C8: (32, 64, 96), C1S: (64, 128, 192)}
CONTROL_W = {C2: 2048, C8: 1280, C1S: 2048}
KNEE_W = {C2: (160, 176, 192), C8: (256, 320, 384)}
P1_W = {C2: 256, C8: 512, C1S: 2048}
CAL_W = {(C2, 0): 16, (C8, 0): 32, (C8, 1): 256, (C1S, 0): 64}       # the scorer's floor pools (rep60a/b-score.txt CAL lines)
OMEGA = {(C2, 0): 0.90, (C8, 0): 1.8, (C8, 1): 6.3, (C1S, 0): 0.20}
QB, QA, QR = 0.10, 0.2 / 3, 0.05                                      # q(delta) = (b + 1)/30 at Bulk / Auto / Realtime
QS = (("Bulk", QB), ("Auto", QA), ("Realtime", QR))


def med(v):
    v = [x for x in v if x is not None]
    return st.median(v) if v else None


def num(d, k):
    v = d.get(k)
    try:
        return None if v in (None, "-") else float(v)
    except ValueError:
        return None


def interp(pts, x):
    pts = sorted(pts)
    if x <= pts[0][0]:
        return pts[0][1]
    if x >= pts[-1][0]:
        return pts[-1][1]
    for (w1, a), (w2, b) in zip(pts, pts[1:]):
        if w1 <= x <= w2:
            return a + (b - a) * (x - w1) / (w2 - w1)


def cross_up(pts, y):
    pts = sorted(pts)
    if pts[0][1] >= y:
        return float(pts[0][0])
    for (w1, a), (w2, b) in zip(pts, pts[1:]):
        if a < y <= b:
            return w1 + (y - a) * (w2 - w1) / (b - a)
    return None


def cross_down(pts):
    """W where [(W, R)] falls through 1; '<W0' if already below at the first point; '>Wn' if never."""
    pts = sorted(pts)
    if pts[0][1] < 1:
        return "<%d" % pts[0][0]
    for (w0, r0), (w1, r1) in zip(pts, pts[1:]):
        if r0 >= 1 > r1:
            return w0 + (r0 - 1) * (w1 - w0) / (r0 - r1)
    return ">%d" % pts[-1][0]


def fx(x, nd=2):
    if x is None:
        return "-"
    return f"{x:.{nd}f}" if isinstance(x, float) else str(x)


# ───────────────────────────── parsing ─────────────────────────────
class Row:
    def __init__(self, cell, kind, W, seed, rep):
        self.cell, self.kind, self.W, self.seed, self.rep = cell, kind, W, seed, rep
        self.win = {}        # leg -> [window dicts, every window after the warm-up cut]
        self.gp = None
        self.dnf = None

    def late(self, leg):
        return [w for w in self.win.get(leg, []) if w["t"] >= LATE_T[self.cell] and w["nk"] >= 10]

    def m(self, leg, key):
        return med([w[key] for w in self.late(leg) if w.get(key) is not None])

    def boot(self, leg):
        for w in self.win.get(leg, []):
            if w.get("b0k"):
                return w["b0k"], w["b0kn"]
        return None, None


def parse_log(lines, row):
    slaw, wst = {}, {}
    for ln in lines:
        if '"warmup":true' in ln:
            slaw, wst = {}, {}
            continue
        if ln.startswith("[SLAW]"):
            d = dict(TOK.findall(ln))
            slaw[(int(d["k"]), int(d["p"]))] = d
        elif ln.startswith("[WSTAT]"):
            d = dict(TOK.findall(ln))
            wst[(int(d["k"]), int(d["p"]))] = d
    for (k, p), s in sorted(slaw.items()):
        if s.get("nk", "0") == "0":
            continue
        ws = wst.get((k, p), {})
        w = {"k": k, "t": num(s, "t"), "nk": num(s, "nk"), "tmin": num(s, "tmin"), "tp50": num(s, "tp50"), "m": num(s, "m"),
             "l": num(s, "l"), "sig": num(s, "sig"), "sigw": num(s, "sigw"), "b0p": num(s, "b0p"), "q": num(s, "q"),
             "p10": num(ws, "p10"), "p50": num(ws, "p50"), "p90": num(ws, "p90")}
        if s.get("b0k", "-") != "-":
            w["b0k"] = [float(x) for x in s["b0k"].split(",")]
            w["b0kn"] = [float(x) for x in s["b0kn"].split(",")]
        row.win.setdefault(p, []).append(w)


def load(paths):
    rows = {}
    for path in paths:
        with tarfile.open(path, "r:gz") as tf:
            for mb in tf:
                g = NAME.search(mb.name)
                if not g or not mb.isfile():
                    continue
                cell, kind, W, seed, rep, what = g.group(1), g.group(2), int(g.group(3)), g.group(4), int(g.group(5)), g.group(6)
                key = (cell, kind, W, seed, rep)
                row = rows.setdefault(key, Row(cell, kind, W, seed, rep))
                text = tf.extractfile(mb).read().decode("utf-8", "replace").splitlines()
                if what == "c.log":
                    parse_log(text, row)
                else:
                    for ln in text:
                        if ln.startswith("{") and '"summary":true' in ln:
                            j = json.loads(ln)
                            row.gp, row.dnf = j.get("mean_mbps"), j.get("dnf")
    return rows


# ───────────────────────────── the s60 calibration and knee, reproduced ─────────────────────────────
def latched(r, leg):
    ks, kn = r.boot(leg)
    lw = r.late(leg)
    br = med([w["tmin"] - w["m"] for w in lw if w["m"] is not None])
    if not ks or br is None:
        return None, "no-boot"
    if leg == 0:
        if len(ks) < 8 or any(x < 10 for x in kn[:8]):
            return None, "boot-incomplete"
        b0 = max(ks[:8])
    else:
        v = [x for x, n in zip(ks[:8], kn[:8]) if n >= 10]
        if len(v) < 4:
            return None, "boot-incomplete"
        b0 = max(v)
    q = med([w["q"] for w in lw]) or QB
    return b0 - br > q * r.m(leg, "l") + OMEGA[(r.cell, leg)], b0


def calibrate(rows, cell, leg):
    pool = [r for r in rows.values() if r.cell == cell and r.kind == "G" and r.W == CAL_W[(cell, leg)] and r.late(leg)]
    valid = [r for r in pool if latched(r, leg)[0] is False]
    keys = ("tmin", "l", "p10", "p50", "p90", "sig", "sigw")
    per = {k: [r.m(leg, k) for r in valid] for k in keys}
    per["S"] = [med([w["p50"] - w["tmin"] for w in r.late(leg)]) for r in valid]           # the floor windows' median - minimum
    per["H"] = [med([w["p90"] - w["tmin"] for w in r.late(leg)]) for r in valid]           # their 90th percentile - minimum
    c = {k: med(v) for k, v in per.items()}
    c["F"], c["n"], c["rows"] = c["tmin"], len(valid), valid
    return c


def rep_map(rows, cell):
    m = {}
    for r in rows.values():
        if r.cell == cell and r.kind == "G":
            m.setdefault((r.rep, r.seed), {})[r.W] = r
    return dict(sorted(m.items()))


def knee(cell, wm):
    ctl = wm.get(CONTROL_W[cell])
    if not ctl or not ctl.gp:
        return None
    pts = [(W, r.gp / ctl.gp) for W, r in wm.items() if W != CONTROL_W[cell] and r.gp is not None]
    return cross_up(pts, 0.95)


def at_knee(cell, wm, w95, fn, leg=0):
    arms = [W for W in KNEE_W[cell] + (P1_W[cell],) if W in wm]
    return interp([(W, fn(wm[W])) for W in arms], w95)


def bracket(cell, wm, w95):
    ka = sorted(W for W in KNEE_W[cell] if W in wm)
    lo = max([W for W in ka if W <= w95], default=ka[0])
    hi = min([W for W in ka if W >= w95], default=ka[-1])
    return lo, hi


def main(argv):
    rows = load(argv[1:3])
    P = print
    P("store-law R6.8 (r68allow.py): the s60 rows read in place from " + " and ".join(a.replace("\\", "/").split("/")[-1] for a in argv[1:3]))
    P(f"rows parsed: {len(rows)} (G {sum(r.kind == 'G' for r in rows.values())}, C {sum(r.kind == 'C' for r in rows.values())})")
    cal = {cl: calibrate(rows, *cl) for cl in CAL_W}

    # ── 0. reproduce the score ──
    P("\n== 0. Reproduction of the s60 score (witness: rep60a-score.txt CAL / C2-D lines) ==")
    for (cell, leg), c in cal.items():
        P(f"  CAL {cell} leg{leg} W{CAL_W[(cell, leg)]}: valid {c['n']}  F_c={c['F']:.3f}  l_c={c['l']:.3f}   (floor windows: p10 {fx(c['p10'], 3)} p50 {fx(c['p50'], 3)} p90 {fx(c['p90'], 3)};"
          f" S_F = med(p50 - tmin) {fx(c['S'], 3)}; H_F = med(p90 - tmin) {fx(c['H'], 3)}; sig_F {fx(c['sig'], 3)}; sigw_F {fx(c['sigw'], 3)})")
    c = cal[(C2, 0)]
    Ds, arm = [], {W: [] for W in KNEE_W[C2]}
    for (rep, seed), wm in rep_map(rows, C2).items():
        w95 = knee(C2, wm)
        tm = at_knee(C2, wm, w95, lambda r: r.m(0, "tmin"))
        lo, hi = bracket(C2, wm, w95)
        sig = med([w["sig"] for W in {lo, hi} for w in wm[W].late(0)])
        D = (tm - c["F"]) - (sig + QB * c["l"])
        Ds.append(D)
        P(f"  C2-D s{seed}r{rep}: W95={w95:.1f} Tmin(W95)={tm:.3f} sig={sig:.3f} -> D_r={D:+.3f}")
        for W in KNEE_W[C2]:
            r = wm[W]
            arm[W].append((r.m(0, "tmin") - c["F"]) - (r.m(0, "sig") + QB * c["l"]))
    P(f"  C2-D pooled median {med(Ds):+.3f}; per arm " + ", ".join(f"W{W} {med(v):+.3f}" for W, v in arm.items()) + "   (score: +1.692; 1.309 / 2.069 / 3.096)")

    # ── 1. the rise, split: common shift against low-tail erasure ──
    P("\n== 1. Window quantiles per arm (median over replicates of each row's LATE median; ms). rise = arm - floor-pool value ==")
    for cell, legs in ((C2, (0,)), (C8, (0, 1)), (C1S, (0,))):
        for leg in legs:
            P(f"  {cell} leg{leg}: W | gp/ctl | tmin p10 p50 p90 | p50-tmin p90-p10 | sig sigw | l")
            arms = sorted({r.W for r in rows.values() if r.cell == cell and r.kind == "G"})
            ctl = {(r.rep, r.seed): r.gp for r in rows.values() if r.cell == cell and r.kind == "G" and r.W == CONTROL_W[cell]}
            for W in arms:
                rs = [r for r in rows.values() if r.cell == cell and r.kind == "G" and r.W == W and r.late(leg)]
                if not rs:
                    continue
                g = lambda k: med([r.m(leg, k) for r in rs])
                sp = med([med([w["p50"] - w["tmin"] for w in r.late(leg)]) for r in rs])
                wd = med([med([w["p90"] - w["p10"] for w in r.late(leg)]) for r in rs])
                u = med([r.gp / ctl[(r.rep, r.seed)] for r in rs if ctl.get((r.rep, r.seed)) and r.gp])
                P(f"    W{W:<5} {fx(u, 3):>6} | {fx(g('tmin'), 3)} {fx(g('p10'), 3)} {fx(g('p50'), 3)} {fx(g('p90'), 3)} | {fx(sp, 3)} {fx(wd, 3)} | {fx(g('sig'), 3)} {fx(g('sigw'), 3)} | {fx(g('l'), 3)}  (n {len(rs)})")

    # ── 2. the needed rise at the knee against each family, per replicate ──
    P("\n== 2. c2: what the knee needs, per replicate, against what each allowance supplies (Bulk q = 0.10) ==")
    P("  need = Tmin(W95) - F_c. Shipped supply A0 = sig + q*l_c. Each family's free constant, solved so that D = 0, is printed: a value solved this way is a FIT, not a provenance.")
    need, sigk = [], []
    for (rep, seed), wm in rep_map(rows, C2).items():
        w95 = knee(C2, wm)
        tm = at_knee(C2, wm, w95, lambda r: r.m(0, "tmin"))
        lo, hi = bracket(C2, wm, w95)
        sig = med([w["sig"] for W in {lo, hi} for w in wm[W].late(0)])
        p50k = at_knee(C2, wm, w95, lambda r: r.m(0, "p50"))
        need.append(tm - c["F"])
        sigk.append(sig)
        P(f"  s{seed}r{rep}: W95 {w95:.1f}  need {tm - c['F']:.3f}  sig {sig:.3f}  q*l_c {QB * c['l']:.3f}  D {tm - c['F'] - sig - QB * c['l']:+.3f}"
          f" | Tmin(W95) {tm:.3f} vs floor-window p50 {c['p50']:.3f} (diff {tm - c['p50']:+.3f}); knee p50 {p50k:.3f}")
    n_, s_ = med(need), med(sigk)
    P(f"  median need {n_:.3f} ms; sig at the knee {s_:.3f}; q*l_c {QB * c['l']:.3f}; q*F_c {QB * c['F']:.3f}")
    P(f"  A(c)-type  sig + c_B*q/q_B:     c_B = {n_ - s_:.3f} ms  (W1's 2.7 ms is b0 + c at G = 0.9 on a different clock, not this quantity)")
    P(f"  wider q    sig + q'*l_c:        q' = {(n_ - s_) / c['l']:.3f};  sig + q'*F_c: q' = {(n_ - s_) / c['F']:.3f}   (RFC 8289 band 0.05-0.10)")
    P(f"  sig scale  k*sig + q*l_c:       k = {(n_ - QB * c['l']) / s_:.3f}")
    P(f"  floor spread S_F + q*F_c supplies {c['S'] + QB * c['F']:.3f} (S_F {c['S']:.3f}); H_F + q*F_c supplies {c['H'] + QB * c['F']:.3f}")

    # ── 3. replay of each candidate: D per arm, crossings, controls, across the dial ──
    def allow(name, c_, q, w, cB=None):
        if name == "A0":
            return w["sig"] + q * c_["l"]
        if name == "A0f":
            return w["sig"] + q * c_["F"]
        if name == "Ac":
            return w["sig"] + cB * q / QB
        if name == "S":
            return c_["S"] + q * c_["F"]
        if name == "Sd":
            return c_["S"] * q / QB + q * c_["F"]
        if name == "W80":
            return (w["p90"] - w["p10"]) + q * c_["F"]
        if name == "wq":
            return w["sig"] + QW * q / QB * c_["F"]
        raise KeyError(name)

    cB = n_ - s_
    QW = (n_ - s_) / c["F"]
    NAMES = (("A0", "sig + q*l_c            (R6.7 form as registered)"),
             ("A0f", "sig + q*F_c            (finding 2's fix alone)"),
             ("Ac", f"sig + c_B*q/q_B, c_B={cB:.2f} (A(c)-type; c_B fitted at c2 Bulk)"),
             ("S", "S_F + q*F_c            (floor-window spread, delta-free part)"),
             ("Sd", "S_F*q/q_B + q*F_c      (floor-window spread, delta-scaled)"),
             ("W80", "(p90 - p10)_window + q*F_c (central 80 % window spread at load)"),
             ("wq", f"sig + q'*F_c, q' = {QW:.3f}*q/q_B (a wider q band, fitted at c2 Bulk)"))

    def r67(rr, cell, legs, name, q):
        """Per-window R67 with allowance `name`, median over LATE windows (nk >= 10 on every live leg)."""
        ws = {leg: {w["k"]: w for w in rr.late(leg)} for leg in legs}
        ks = sorted(set().union(*[set(x) for x in ws.values()]))
        vals = []
        for k in ks:
            live = [(leg, ws[leg][k]) for leg in legs if k in ws[leg]]
            if not live:
                continue
            mx = max(cal[(cell, leg)]["F"] for leg, _ in live)
            num_ = den = 0.0
            for leg, w in live:
                c_ = cal[(cell, leg)]
                a = allow(name, c_, q, w, cB)
                num_ += w["nk"] * (c_["F"] + 0.5 * max(0.0, mx - c_["F"]) + a + w["tp50"] - w["tmin"])
                den += w["nk"] * w["tp50"]
            vals.append(num_ / den)
        return med(vals)

    for cell, legs in ((C2, (0,)), (C8, (0, 1))):
        P(f"\n== 3. {cell}: each candidate replayed (R67 form, session floors; per replicate: knee W95, crossing W*, W* - W95; control R) ==")
        for name, desc in NAMES:
            for qn, q in QS:
                res, ctlR, Darm, ncross = [], [], {W: [] for W in KNEE_W[cell]}, []
                for (rep, seed), wm in rep_map(rows, cell).items():
                    w95 = knee(cell, wm)
                    pts = [(W, r67(wm[W], cell, legs, name, q)) for W in sorted(wm) if W not in FLOOR_W[cell] and W != CONTROL_W[cell]]
                    cr = cross_down(pts)
                    ncross.append(sum(1 for a_, b_ in zip(sorted(pts), sorted(pts)[1:]) if (a_[1] >= 1) != (b_[1] >= 1)))
                    res.append((w95, cr))
                    ctlR.append(r67(wm[CONTROL_W[cell]], cell, legs, name, q))
                    if cell == C2:
                        for W in KNEE_W[cell]:
                            r = wm[W]
                            Darm[W].append(med([(w["tmin"] - cal[(cell, 0)]["F"]) - allow(name, cal[(cell, 0)], q, w, cB) for w in r.late(0)]))
                txt = " ".join((f"{cr:.0f}" if isinstance(cr, float) else cr) for _, cr in res)
                gap = med([cr - w for w, cr in res if isinstance(cr, float)])
                P(f"  {name:<4} {qn:<8} W* {txt:<36} median W*-W95 {fx(gap, 1):>6} (W95 median {med([w for w, _ in res]):.1f}) | R=1 crossings/row max {max(ncross)} | control R max {max(ctlR):.4f}"
                  + (" | D per arm " + ", ".join(f"W{W} {med(v):+.2f}" for W, v in Darm.items()) if cell == C2 else ""))
            P(f"       ({desc})")

    c1 = cal[(C1S, 0)]
    P("\n== 3c. c1s (no knee bracket in s60; direction and size only): each allowance at the floor pool W64 and the control's R67 at Bulk ==")
    for name, desc in NAMES:
        fl = [r for r in rows.values() if r.cell == C1S and r.kind == "G" and r.W == 64]
        a64 = med([allow(name, c1, QB, w, cB) for r in fl for w in r.late(0)])
        ct = [r67(r, C1S, (0,), name, QB) for r in rows.values() if r.cell == C1S and r.kind == "G" and r.W == 2048]
        P(f"  {name:<4} A(W64) {a64:.3f} ms | control W2048 R67 median {med(ct):.4f} max {max(ct):.4f}")

    # ── 4. uniqueness: d(Tmin - A)/dW > 0 per leg, per replicate, over the knee arms, the P1 arm and the control (A read at load) ──
    P("\n== 4. Uniqueness gate (R6.2): Tmin - A must rise with W on the legs carrying the weight. A read at the floor windows is constant in W, so only"
      " the at-load reads are checked: sig (shipped) and the window p90 - p10 (W80), per leg and replicate, over the knee arms, the P1 arm and the control ==")
    for cell, legs in ((C2, (0,)), (C8, (0, 1))):
        for leg in legs:
            for nm, fa in (("sig", lambda r: r.m(leg, "sig")), ("p90-p10", lambda r: med([w["p90"] - w["p10"] for w in r.late(leg)]))):
                bad = []
                for (rep, seed), wm in rep_map(rows, cell).items():
                    arms = [W for W in KNEE_W[cell] + (P1_W[cell], CONTROL_W[cell]) if W in wm and wm[W].late(leg)]
                    v = [(W, wm[W].m(leg, "tmin") - fa(wm[W])) for W in arms]
                    if not all(b[1] > a[1] for a, b in zip(v, v[1:])):
                        bad.append(f"s{seed}r{rep} " + " ".join(f"W{W}:{x:.1f}" for W, x in v))
                P(f"  {cell} leg{leg} Tmin - {nm:<8}: rising on {6 - len(bad)}/6 replicates" + ("" if not bad else "; NOT on " + " | ".join(bad)))

    # ── 5. finding 2: where the lifetime minimum is attained ──
    P("\n== 5. The lifetime minimum l: the window at which each row's final l is first printed, the samples fed by then, l at t = 15 s and at the end ==")
    P("  cell leg W: per row 'k@t/N' = window k (time t s) at which l first reaches its final value, N = echo samples fed up to that window; then l(15 s) -> l(end)")
    for cell, legs in ((C2, (0,)), (C8, (0, 1)), (C1S, (0,))):
        for leg in legs:
            arms = sorted({r.W for r in rows.values() if r.cell == cell and r.kind == "G"})
            for W in arms:
                rs = sorted([r for r in rows.values() if r.cell == cell and r.kind == "G" and r.W == W and r.win.get(leg)], key=lambda r: (r.rep, r.seed))
                parts, ks, drift = [], [], []
                for r in rs:
                    ws = r.win[leg]
                    lf = ws[-1]["l"]
                    cum, at = 0, None
                    for w in ws:
                        cum += w["nk"]
                        if w["l"] == lf:
                            at = (w["k"], w["t"], cum)
                            break
                    l15 = next((w["l"] for w in ws if w["t"] >= 15.0), None)
                    parts.append(f"{at[0]}@{at[1]:.1f}/{at[2]}")
                    ks.append(at[0])
                    if l15 is not None:
                        drift.append(l15 - lf)
                P(f"  {cell} leg{leg} W{W:<5} l med {med([r.win[leg][-1]['l'] for r in rs]):.3f} | " + " ".join(parts)
                  + f" | attained in boot (k <= 8) on {sum(k <= 8 for k in ks)}/{len(ks)}; l(15 s) - l(end) median {fx(med(drift), 3)}")

    # ── 6. finding 2 against finding 1 ──
    c = cal[(C2, 0)]
    P("\n== 6. Does fixing finding 2 close finding 1? c2, Bulk ==")
    P(f"  q*l_c = {QB * c['l']:.3f}, q*F_c = {QB * c['F']:.3f}: the swap adds {QB * (c['F'] - c['l']):.3f} ms; D {med(Ds):+.3f} -> {med(Ds) - QB * (c['F'] - c['l']):+.3f}")
    P(f"  delta's whole authority over the allowance at c2: q*l_c spans {QR * c['l']:.3f} (Realtime) to {QB * c['l']:.3f} (Bulk) = {(QB - QR) * c['l']:.3f} ms; with F_c {(QB - QR) * c['F']:.3f} ms")
    sl = []
    for (rep, seed), wm in rep_map(rows, C2).items():
        sl.append((wm[192].m(0, "tmin") - wm[160].m(0, "tmin")) / 32.0)
    P(f"  knee slope dTmin/dW over W160-192 (median over replicates) {med(sl):.4f} ms/slot -> delta's span = {(QB - QR) * c['l'] / med(sl):.1f} slots (l_c) / {(QB - QR) * c['F'] / med(sl):.1f} slots (F_c); the shortfall = {med(Ds) / med(sl):.1f} slots")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
