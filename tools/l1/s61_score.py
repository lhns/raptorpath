#!/usr/bin/env python3
"""Status 61: the scorer of the c3 sweep (docs/derivations/s61-registration.md; derivations and predictions in docs/derivations/store-law/s61pred.py / .out). Built ON s60_score.py (not a fork): it reuses
`s60_score.row_metrics` (the [SLAW] / [WSTAT] / [DIAG] parse, the per-row replay witness, the CAPLOG replay) and RV / med / ols / cross_up / interp, and adds the s61 readings.

    s61_score.py smoke61 <61a|61b|61r> <ledger>      the smoke / rehearsal check: PASS / FAIL lines, GO-NOGO decisions, PILOT61, ending SMOKE-PASS | SMOKE-FAIL
    s61_score.py score61 <61a|61b> <ledger> [...]    every registered readout, one `LABEL61 <reading> <label>` line per scored reading, then `VERDICT61`
    s61_score.py pilot <ledger>                      the PILOT61 line from a rehearsal ledger

61a reads c3 (sections 3-5: the sample-count rule G(n*), CR61, floor validity and the two-sided gates, the u-matched floor, rho and the three-way label against c2's frozen value, D per candidate, the price and
landing tables, B1 / P1 and the native readings); 61b reads c2 (CR61 with the frozen X_own, the drift reading, the price and landing tables). c2's section-60 inputs are FROZEN below (C2_FROZEN, from s61pred.py on
rep60a-diag-ld.tgz); test_s61.py recomputes them from the tgz when the host has it. Every place the registration is silent is a DECISION (printed, and DECISIONS below)."""
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import plan61  # noqa: E402
import s60_score as s6  # noqa: E402

C3, C2 = plan61.C3, plan61.C2
med, ols, pq, cross_up, interp, f = s6.med, s6.ols, s6.pq, s6.cross_up, s6.interp, s6.f
Q_BULK, Q_AUTO, Q_RT = 0.10, 0.2 / 3, 0.05                    # q(delta) = (b + 1) / 30 at Bulk / Auto / Realtime (r68 section 1.4)
QS = (("Bulk", Q_BULK), ("Auto", Q_AUTO), ("Realtime", Q_RT))
NK_NATIVE = 10                                                # native windows: nk >= 10 (s60 section 2); G pools every window with nk >= 1
NSTAR_FLOOR, NSTAR_KNEE = 250, 2100                           # section 3: c3 is read at c2's native sample counts
PARITY = {"F": (226, 353), "K": (1874, 2928)}                 # section 3: [0.8, 1.25] x (282, 2342)
N_BOOT, ALPHA_LO, ALPHA_HI = 2000, 100, 1900                  # 90 % intervals: sorted draws [100] / [1900]
SEED_C3, SEED_C2 = 61, 60
TAU_FRAC = 0.05                                               # section 5.1: tau = 0.05 S_F,c
U2 = 0.112                                                    # c2's W 16 utilisation (section 5), the floor's matched position
W_STAR_SLACK = 6                                              # section 5: W* more than 6 slots below the lowest valid pool is UNSCOREABLE-FLOOR
MIN_FLOOR_ROWS, MIN_FLOOR_GROUPS = 3, 6                       # section 4.2
MIN_SCOREABLE = 5                                             # section 2.2 / 5.2
MIN_CTL_UNFLAGGED = 4                                         # section 4.1
CR_SHARE, CR_GP, CR_RATIO = 0.10, 0.95, 1.5                   # section 4.1
KNEE_FRAC = 0.95
X_OWN_C2 = 10.66                                              # section 4.1: frozen at c2 (s60's six W 16 rows: E50 4.95, E90 6.86)
DRIFT_TMIN, DRIFT_U, DRIFT_TOL_TMIN, DRIFT_TOL_U = 12.33, 0.930, 0.3, 0.02      # section 2.4
CTL_LOSS_PP = 0.01                                            # section 6: the control's loss truth <= 4.76 % + 1.0 pp
EPS_C3 = 0.0476
GO_CTL_RATIO = 0.98
FLOOR_NK_SHARE = 0.80
C_B, Q_WQ = 2.261, 0.260                                      # r68 section 1.4's frozen constants (Ac, wq)

# ── c2's section-60 inputs, frozen (s61pred.py on rep60a-diag-ld.tgz / s61pred.out section 2) ──
C2_FROZEN = {
    "knee_tmin": sorted([12.767032841891204, 12.689934045189505, 12.580477271483305, 12.689411198143313, 12.766327072649574, 12.792452814394938]),      # Tmin(W95), six replicates
    "floor_pairs": sorted([(5.147500000000001, 8.7135), (5.1985, 8.593499999999999), (4.915999999999999, 8.764), (5.077000000000001, 8.61), (5.089499999999999, 8.6765), (4.796999999999999, 8.682)]),      # (S_F, F) of the six W 16 rows, native
    "g800": {16: [(6.2829999999999995, 7.512), (5.950000000000001, 7.698), (6.0805, 7.640000000000001), (6.1594999999999995, 7.645), (6.253, 7.7330000000000005), (5.8294999999999995, 7.685499999999999)],
             32: [(5.829000000000001, 8.377), (5.915000000000001, 8.223), (5.911, 8.267), (5.824999999999999, 8.2635), (5.886499999999999, 8.397), (5.677, 8.405000000000001)],
             48: [(5.6645, 8.82), (5.944000000000001, 8.6), (6.0115, 8.619499999999999), (5.834, 8.681), (5.68, 8.849), (5.695, 8.778)]},      # (S_F, F) under G(800), in the loader's row order
    "sig": 1.785, "gp160_u": DRIFT_U, "tmin160": DRIFT_TMIN,
    "price": {16: (0.112, 8.68, 13.70), 32: (0.217, 8.95, 14.27), 48: (0.321, 9.04, 14.59), 160: (0.930, 12.32, 16.81), 176: (0.970, 13.09, 17.81), 192: (0.990, 14.05, 19.12), 256: (1.002, 19.75, 25.40)},      # u, tmin, tp50
}


# ───────────────────────────── the sample-count rule G(n*) (section 3) ─────────────────────────────
def wmed(pairs):
    """nk-weighted median of [(value, weight)] (s61pred.wmed)."""
    pairs = sorted((v, w) for v, w in pairs if v is not None and w)
    if not pairs:
        return None
    tot, acc = sum(w for _, w in pairs), 0.0
    for v, w in pairs:
        acc += w
        if acc >= tot / 2:
            return v
    return None


def groups(wins, nstar):
    """G(n*): consecutive windows pooled until sum nk >= n*; the last incomplete group is dropped. -> [(group tmin = the min of the minima (exact), nk-weighted median p50 (an approximation), n)].
    nstar None = native: one window each."""
    wins = [w for w in wins if w["tmin"] is not None and w["p50"] is not None]
    if nstar is None:
        return [(w["tmin"], w["p50"], w["nk"]) for w in wins]
    out, cur = [], []
    for w in wins:
        cur.append(w)
        if sum(x["nk"] for x in cur) >= nstar:
            out.append((min(x["tmin"] for x in cur), wmed([(x["p50"], x["nk"]) for x in cur]), sum(x["nk"] for x in cur)))
            cur = []
    return out


def reading(wins, nstar):
    """One row's (S_F, F, median realised n, number of groups): S_F = median over groups of (p50 - tmin), F = median of the group minima (section 3)."""
    g = groups(wins, nstar)
    if not g:
        return None, None, None, 0
    return med([p - t for t, p, _ in g]), med([t for t, _, _ in g]), med([n for _, _, n in g]), len(g)


def parity_label(n, band):
    """Realised median n against section 3's band: -> 'IN' | 'BELOW' | 'ABOVE' | None."""
    if n is None:
        return None
    return "IN" if band[0] <= n <= band[1] else ("BELOW" if n < band[0] else "ABOVE")


# ───────────────────────────── the pilot (section 2.2) ─────────────────────────────
pilot_knee = plan61.pilot_knee


# ───────────────────────────── the row-time extras (the VM side) ─────────────────────────────
def row_extras(cli, srv, drv, cell):
    """The `t61` of one row, beside s60_score's `t60` (place38_parse calls both): after the last warm-up marker, the LATE [SLAW] windows of leg 0 with nk > 0 joined with their [WSTAT] p10 / p50 / p90 (the
    window quantiles s61pred reads), the LATE [DIAG] ticks' top-level `rtt=` (src > 0: CR61's e_own is taken against the row's tmin at score time, so the cross-row X_own is computable), the qshare of
    place38_parse.qcwnd_below_bdp, and the driver's [TRUTH] datagram loss of leg 0. Never raises on log content."""
    cli = list(cli or [])
    out = {"v": 1, "win": {k: [] for k in ("t", "nk", "tmin", "p10", "p50", "p90")}, "rtt": [], "qshare": None, "loss": None, "n_ticks": 0}
    try:
        slaw, wst, ticks = {}, {}, []
        for ln in cli:
            if s6.WARM in ln:
                slaw, wst, ticks = {}, {}, []
                continue
            if ln.startswith("[SLAW]"):
                d = dict(s6._TOK.findall(ln))
                try:
                    slaw[(int(d["k"]), int(d["p"]))] = d
                except (KeyError, ValueError):
                    pass
            elif ln.startswith("[WSTAT]"):
                d = dict(s6._TOK.findall(ln))
                try:
                    wst[(int(d["k"]), int(d["p"]))] = d
                except (KeyError, ValueError):
                    pass
            elif ln.startswith("[DIAG]"):
                tk = s6._tick(ln)
                if tk:
                    ticks.append(tk)
        t0 = plan61.LATE_T.get(cell, 15.0)
        for (k, p), s in sorted(slaw.items()):
            if p != 0 or s.get("nk", "0") == "0":
                continue
            t = s6._num(s.get("t"))
            if t is None or t < t0:
                continue
            ws = wst.get((k, p), {})
            for key, val in (("t", t), ("nk", s6._num(s.get("nk"))), ("tmin", s6._num(s.get("tmin"))), ("p10", s6._num(ws.get("p10"))), ("p50", s6._num(ws.get("p50"))), ("p90", s6._num(ws.get("p90")))):
                out["win"][key].append(val)
        out["rtt"] = [tk[2] for tk in ticks if tk[0] >= t0 and (tk[1] or 0) > 0 and tk[2] is not None]
        out["n_ticks"] = len(ticks)
        import place38_parse as pp
        q = pp.qcwnd_below_bdp(cli, cell)
        out["qshare"] = (q or {}).get("per_leg", [None])[0] if q else None
        import l1common as lc
        tr = lc.truth(drv)
        out["loss"] = (tr.get(0) or {}).get("loss")
    except Exception as e:      # a reported readout never voids a row
        out["error"] = f"{type(e).__name__}: {e}"
    return out


# ───────────────────────────── the row view ─────────────────────────────
class Row(s6.RV):
    """A ledger row with its t60 (s60_score.RV) and t61."""

    def __init__(self, r, order=0):
        super().__init__(r, order)
        self.x = r.get("t61") or {}
        self.live = self.live and bool((self.x.get("win") or {}).get("t"))
        self.flag = None             # CR61: None = not evaluated, else True / False
        self._native = None

    @property
    def key(self):
        return (self.rep, self.seed)

    def w61(self, nk_min=1):
        """The t61 window view (LATE windows with nk >= nk_min); RV.wins(leg) stays the t60 view."""
        w = self.x.get("win") or {}
        n = len(w.get("t", []))
        return [{k: w[k][i] for k in w} for i in range(n) if (w["nk"][i] or 0) >= nk_min]

    def native_tmin(self):
        v = [w["tmin"] for w in self.w61(NK_NATIVE) if w["tmin"] is not None]
        return med(v)

    def native_p90(self):
        return med([w["p90"] for w in self.w61(NK_NATIVE) if w["p90"] is not None])

    def native_tp50(self):
        return med([w["p50"] for w in self.w61(NK_NATIVE) if w["p50"] is not None])

    def sig_windows(self):
        return [w["sig"] for w in self.wins(0) if (w["nk"] or 0) >= NK_NATIVE and w["sig"] is not None]

    def sixty(self, key):
        """s60_score's per-leg LATE statistic (median over nk >= 10 windows): sig, l, err, tp, tmin."""
        return self.st(0)[key] if 0 in self.legs() else None

    def gp_(self):
        return 0.0 if self.dnf else (self.r.get("mbps") or 0.0)


def counting_rows(rows_all):
    import pool56_score as ps
    cnt, replaced = ps.counting(rows_all)
    idx = {id(r): i for i, r in enumerate(rows_all)}
    cnt.sort(key=lambda r: idx[id(r)])
    return cnt, replaced


# ───────────────────────────── CR61, the collapse rule (section 4.1) ─────────────────────────────
def e_own(row):
    """e_own(tick) = rtt - the row's median LATE tmin (the standing queue cancels: it is in both terms)."""
    tm = row.native_tmin()
    return [x - tm for x in row.x.get("rtt", [])] if tm is not None else []


def x_own_from(rows):
    """X_own = E90 + 2 (E90 - E50), E50 / E90 the medians over `rows` of each row's own percentile of e_own (a minority of collapsed rows cannot loosen it). -> (X, E50, E90, rows used)."""
    per = [(pq(e, 0.5), pq(e, 0.9)) for e in (e_own(r) for r in rows) if e]
    if not per:
        return None, None, None, 0
    e50, e90 = med([p[0] for p in per]), med([p[1] for p in per])
    return e90 + 2 * (e90 - e50), e50, e90, len(per)


def cr61(rows, k1, x_own):
    """Section 4.1 per row: below K1 (floor and below-knee arms) COLLAPSED iff share{LATE ticks: e_own > X_own} > 0.10 AND gp < 0.95 x the arm's median gp; at and above K1 (knee, P1, control):
    (qshare > 0.10 AND gp < 0.95 x the arm's median gp) OR E90_own / (p90 - tmin)_own > 1.5. Sets row.flag; -> {id: (flag, detail)}. A row without ticks reads None (not flagged: nothing to read)."""
    arm_gp = {}
    for r in rows:
        arm_gp.setdefault(r.W, []).append(r.gp_())
    med_gp = {w: med(v) for w, v in arm_gp.items()}
    out = {}
    for r in rows:
        e = e_own(r)
        low_gp = r.gp_() < CR_GP * (med_gp[r.W] or 0.0)
        if not e:
            r.flag, det = None, "no-ticks"
        elif r.W < k1:
            sh = sum(v > x_own for v in e) / len(e) if x_own is not None else None
            r.flag, det = bool(sh is not None and sh > CR_SHARE and low_gp), f"share>X={f(sh, 3)} gp_low={int(low_gp)}"
        else:
            q = r.x.get("qshare")
            p90, tm = r.native_p90(), r.native_tmin()
            ratio = pq(e, 0.9) / (p90 - tm) if p90 is not None and tm is not None and p90 > tm else None
            r.flag = bool((q is not None and q > CR_SHARE and low_gp) or (ratio is not None and ratio > CR_RATIO))
            det = f"qshare={f(q, 3)} gp_low={int(low_gp)} ratio={f(ratio, 2)}"
        out[id(r)] = (r.flag, det)
    return out


# ───────────────────────────── the floor: validity, gates, the u-matched reading (sections 4.2, 5, 5.1) ─────────────────────────────
def floor_valid(row, nstar=NSTAR_FLOOR):
    """Section 4.2: completes (no DNF), not COLLAPSED, >= 6 complete groups under G(250) in LATE, and live (no VOID). -> (ok, reason)."""
    if not row.live:
        return False, "void"
    if row.dnf:
        return False, "DNF"
    if row.flag:
        return False, "COLLAPSED"
    s, fm, n, g = reading(row.w61(1), nstar)
    if g < MIN_FLOOR_GROUPS:
        return False, f"{g}-groups"
    return True, ""


def floor_pool(rows_by_w, floors=plan61.FLOOR_W[C3]):
    """The cell's floor pool: the smallest pool with >= 3 valid rows (12, then 16, then 24). -> (W | None, {W: [valid rows]})."""
    valid = {w: [r for r in rows_by_w.get(w, []) if floor_valid(r)[0]] for w in floors}
    for w in floors:
        if len(valid[w]) >= MIN_FLOOR_ROWS:
            return w, valid
    return None, valid


def gate(points, w1, tau):
    """Section 5.1's two-sided floor gate: OLS slope b of the per-row value against W over the pools, E = b W1; CONVERGED iff |E| + 2 se <= tau, NOT-CONVERGED iff |E| - 2 se > tau, else UNRESOLVED;
    UNSCOREABLE with fewer than 2 pools (nothing is extrapolated below the floor pool) or no spread. -> dict(label, b, a, se, E, seE, pools)."""
    pools = sorted({w for w, _ in points})
    if len(pools) < 2:
        return {"label": "UNSCOREABLE", "b": None, "a": None, "se": None, "E": None, "seE": None, "pools": pools}
    fit = ols([w for w, _ in points], [y for _, y in points])
    if fit is None:
        return {"label": "UNSCOREABLE", "b": None, "a": None, "se": None, "E": None, "seE": None, "pools": pools}
    b, a, se = fit
    E, seE = b * w1, se * w1
    lab = "CONVERGED" if abs(E) + 2 * seE <= tau else ("NOT-CONVERGED" if abs(E) - 2 * seE > tau else "UNRESOLVED")
    return {"label": lab, "b": b, "a": a, "se": se, "E": E, "seE": seE, "pools": pools}


def u_matched(fitF, fitS, kappa, u2=U2, w_lowest=None):
    """Section 5: F_c = a_F + b_F W*, S_F,c = a_S + b_S W*, W* = u2 / kappa. -> (W*, F_c, S_c, floor_ok) with floor_ok False when W* lies more than 6 slots below the lowest valid pool."""
    w_star = u2 / kappa if kappa else None
    if w_star is None or fitF["a"] is None or fitS["a"] is None:
        return w_star, None, None, True
    ok = w_lowest is None or w_star >= w_lowest - W_STAR_SLACK
    return w_star, fitF["a"] + fitF["b"] * w_star, fitS["a"] + fitS["b"] * w_star, ok


# ───────────────────────────── rho and the three-way label (section 5.2) ─────────────────────────────
def rho_label(lo, hi, delta):
    """AGREES iff CI(d_rho) inside (-Delta, +Delta); DISAGREES iff CI is outside [-Delta, +Delta]; UNRESOLVED otherwise (a null without power: NEEDS-MORE-replicates, never a refutation)."""
    if lo > -delta and hi < delta:
        return "AGREES"
    if lo > delta or hi < -delta:
        return "DISAGREES"
    return "UNRESOLVED"


def d_label(lo, hi):
    return "SHORT" if lo > 0 else ("COVERED" if hi < 0 else "UNRESOLVED")


def opposite(a, b):
    return {a, b} in ({"AGREES", "DISAGREES"}, {"SHORT", "COVERED"})


def rho_of(needs_t, f_c, s_c):
    """rho = need / S_F with need = Tmin(W95) - F: the median over replicates of (Tmin_r - F_c) / S_F,c (an absolute reading: a two-row fixture pins it)."""
    return med([(t - f_c) / s_c for t in needs_t]) if s_c else None


def c2_draws(frozen=C2_FROZEN):
    """c2's rho_2 draws, ported draw for draw from s61pred.py (same RNG calls, the sorted six-row inputs): -> (point, [2000 rho draws], point_W0, [2000 W->0 draws]). The W->0 reading applies the G(800) floor slope
    (refitted per draw, rows resampled within pools) to the native F and S_F: an approximation, labelled."""
    kt, fl = frozen["knee_tmin"], sorted(frozen["floor_pairs"])
    s_c, f_c = med([x[0] for x in fl]), med([x[1] for x in fl])
    point = med([(t - f_c) / s_c for t in kt])
    rnd = random.Random(SEED_C2)
    rho = []
    for _ in range(N_BOOT):
        it = [kt[rnd.randrange(len(kt))] for _ in kt]
        fb = [fl[rnd.randrange(len(fl))] for _ in fl]
        sb, fb_ = med([x[0] for x in fb]), med([x[1] for x in fb])
        rho.append(med([(t - fb_) / sb for t in it]))
    g8 = frozen["g800"]
    xs = [w for w in g8 for _ in g8[w]]
    ef = ols(xs, [p[1] for w in g8 for p in g8[w]])[0] * 16
    es = ols(xs, [p[0] for w in g8 for p in g8[w]])[0] * 16
    point0 = med([(t - (f_c - ef)) / (s_c - es) for t in kt])
    bypool = {w: [(w, p[0], p[1]) for p in g8[w]] for w in g8}
    rnd = random.Random(SEED_C2)
    rho0 = []
    for _ in range(N_BOOT):
        it = [kt[rnd.randrange(len(kt))] for _ in kt]
        fb = [fl[rnd.randrange(len(fl))] for _ in fl]
        pts = [v[rnd.randrange(len(v))] for v in bypool.values() for _ in v]
        bfb = ols([x[0] for x in pts], [x[2] for x in pts])[0]
        bsb = ols([x[0] for x in pts], [x[1] for x in pts])[0]
        sb, fb_ = med([x[0] for x in fb]) - bsb * 16, med([x[1] for x in fb]) - bfb * 16
        rho0.append(med([(t - fb_) / sb for t in it]))
    return point, rho, point0, rho0


def ci(draws):
    d = sorted(draws)
    return d[ALPHA_LO], d[ALPHA_HI]


def c2_frozen_summary(frozen=C2_FROZEN):
    fl = sorted(frozen["floor_pairs"])
    s_c, f_c = med([x[0] for x in fl]), med([x[1] for x in fl])
    return {"S": s_c, "F": f_c, "delta": Q_RT * f_c / s_c}


# ───────────────────────────── the knee per replicate (section 5 symbols) ─────────────────────────────
def ctl_gp(rows, ctl_w):
    """-> (ctl: {(rep, seed): gp used}, n_unflagged, moved): a flagged / missing control is replaced for its replicate's u by the median goodput of the unflagged controls; < 4 unflagged = CONTROL-MOVED."""
    cs = [r for r in rows if r.W == ctl_w and r.live]
    good = [r for r in cs if not r.flag and not r.dnf and r.gp_() > 0]
    m = med([r.gp_() for r in good])
    out = {}
    for r in cs:
        out[r.key] = r.gp_() if (not r.flag and not r.dnf and r.gp_() > 0) else m
    return out, len(good), len(good) < MIN_CTL_UNFLAGGED, m


def knee_of(reps_rows, ctl, k1, k3, tmin_of, knee_arms, p1_arm, ctl_w, ctl_median):
    """One replicate's knee: W95 = where gp / gp_ctl crosses 0.95 over every non-control arm (cross_up), Tmin(W95) interpolated over the K and P1 arms' row minima. -> dict | None.
    `reps_rows` = {W: Row} of the replicate, flagged rows already removed. Scoreable iff K1 <= W95 <= K3 and the interpolation has >= 2 points."""
    pts = [(w, r.gp_() / (ctl or ctl_median)) for w, r in reps_rows.items() if w != ctl_w and r.live and (ctl or ctl_median)]
    w95, clamped = cross_up(pts, KNEE_FRAC)
    kp = sorted(w for w in set(knee_arms) | {p1_arm} if w in reps_rows and tmin_of(reps_rows[w]) is not None)
    if w95 is None or len(kp) < 2:
        return {"w95": w95, "scoreable": False, "why": "no-crossing" if w95 is None else "few-knee-arms", "clamped": clamped}
    t = interp([(w, tmin_of(reps_rows[w])) for w in kp], w95)[0]
    ka = sorted(w for w in knee_arms if w in reps_rows)
    lo = max([w for w in ka if w <= w95], default=ka[0] if ka else None)
    hi = min([w for w in ka if w >= w95], default=ka[-1] if ka else None)
    sig = [x for w in {lo, hi} if w in reps_rows for x in reps_rows[w].sig_windows()]
    p80 = [x["p90"] - x["p10"] for w in {lo, hi} if w in reps_rows for x in reps_rows[w].w61(NK_NATIVE) if x["p90"] is not None and x["p10"] is not None]
    inside = k1 <= w95 <= k3
    return {"w95": w95, "tmin": t, "sig": med(sig), "p80": med(p80), "scoreable": inside, "why": "" if inside else ("below-K1" if w95 < k1 else "above-K3"), "clamped": clamped,
            "lo": lo, "hi": hi}


def candidates(sig, p80, l_c, f_c, s_c, q):
    """r68 section 1.4's allowances: S (the decided form at g = 1), A0, A0f, Ac, wq, W80."""
    return {"S": s_c + q * f_c, "A0": sig + q * l_c, "A0f": sig + q * f_c, "Ac": sig + C_B * q / Q_BULK, "wq": sig + Q_WQ * (q / Q_BULK) * f_c,
            "W80": (p80 if p80 is not None else float("nan")) + q * f_c}


def sigma_discrimination(rho2, sig3, s3, sig2=C2_FROZEN["sig"], s2=None):
    """Section 5.2: rho_2 * (sigma_3 / S_F,3) / (sigma_2 / S_F,2) - rho_2: how far the sigma-scaled sentence puts rho_3 from rho_2."""
    s2 = s2 if s2 is not None else c2_frozen_summary()["S"]
    return rho2 * (sig3 / s3) / (sig2 / s2) - rho2


# ───────────────────────────── pilot, control and the rehearsal's decisions (section 6) ─────────────────────────────
# ───────────────────────────── bootstrap helpers ─────────────────────────────
def boot_rows(rnd, by_pool):
    """Rows resampled within pools: -> [(W, row_values)]."""
    out = []
    for w, rs in sorted(by_pool.items()):
        out += [(w, rs[rnd.randrange(len(rs))]) for _ in rs]
    return out


# ───────────────────────────── the scorer ─────────────────────────────
class Score61:
    def __init__(self, rows_cnt, replaced, plan, out):
        self.rows_all = [Row(r, i) for i, r in enumerate(rows_cnt)]
        self.replaced, self.plan, self.out = replaced, plan, out
        self.labels, self.notes = [], []
        self.cell = C3 if plan == "61a" else C2
        self.rows = [r for r in self.rows_all if r.cell == self.cell]
        self.knee_env = None

    def label(self, name, lab):
        self.labels.append((name, lab))

    # ── arm classes ──
    def classes(self):
        """-> (floors, k_arms, b_arms, p1, ctl_w, k_source). c3: K arms from LD_KNEE61 (the pilot), else derived from the ledger (7 non-floor non-control arms: B x3, K x3, P1). c2: K1 = 160."""
        ws = sorted({r.W for r in self.rows})
        if self.cell == C2:
            return (), (160,), tuple(w for w in ws if w < 160 and w != 2048), None, 2048, "c2: K1 = 160"
        ctl_w = 2048 if 2048 in ws else (512 if 512 in ws else None)
        floors = tuple(w for w in ws if w in plan61.FLOOR_W[C3])
        k = None
        try:
            k = plan61.knee_env()
        except ValueError:
            k = None
        if k is not None:
            a = plan61.arms(k)
            return floors, tuple(a["K"]), tuple(a["B"]), a["P1"], ctl_w, f"LD_KNEE61={k:g}"
        rest = [w for w in ws if w not in floors and w != ctl_w]
        if len(rest) == 7:
            return floors, tuple(rest[3:6]), tuple(rest[:3]), rest[6], ctl_w, "derived from the ledger (7 non-floor arms: B x3, K x3, P1)"
        return floors, tuple(rest[-4:-1]), tuple(rest[:-4]), rest[-1] if rest else None, ctl_w, "derived from the ledger (UNUSUAL arm count)"

    def by_w(self, rows=None):
        d = {}
        for r in (rows if rows is not None else self.rows):
            d.setdefault(r.W, []).append(r)
        return d

    def run(self):
        return self.run_c3() if self.cell == C3 else self.run_c2()

    # ── tmin of a row for the knee side ──
    def tmin_c3(self, r):
        s, fmin, n, g = reading(r.w61(1), NSTAR_KNEE)
        return fmin

    # ─────────────────────────── c2 (61b) ───────────────────────────
    def run_c2(self):
        out, rows = self.out, self.rows
        floors, kA, bA, p1, ctl_w, ksrc = self.classes()
        out(f"S61 SCORE plan=61b cell={C2} rows={len(rows)} (c2 add-on: the price below c2's knee; section 2.4 / 5.4); K1 = 160 ({ksrc})")
        # CR61 with c2's frozen X_own
        cr61(rows, 160, X_OWN_C2)
        nflag = sum(1 for r in rows if r.flag)
        out(f"CR61 c2: X_own = {X_OWN_C2} (frozen: s60's six W 16 rows, E50 4.95, E90 6.86); COLLAPSED rows {nflag}/{len(rows)}" + "".join(f"; {r.arm} s{r.seed} r{r.rep}" for r in rows if r.flag))
        ctl, n_good, moved, ctl_m = ctl_gp(rows, ctl_w)
        out(f"CONTROL c2 W{ctl_w}: unflagged {n_good}/{sum(1 for r in rows if r.W == ctl_w)}, median goodput {f(ctl_m, 2)}" + ("  CONTROL-MOVED: nothing is scored" if moved else ""))
        self.label("CONTROL-c2", "CONTROL-MOVED" if moved else "OK")
        price = self.price_table(rows, ctl, ctl_m, ctl_w, "native", None)
        # the drift reading
        a160 = [r for r in rows if r.W == 160 and r.live and not r.flag]
        t160 = med([r.native_tmin() for r in a160])
        u160 = med([r.gp_() / (ctl.get(r.key) or ctl_m) for r in a160 if (ctl.get(r.key) or ctl_m)])
        if t160 is None or u160 is None:
            drift = "UNSCOREABLE"
            out("DRIFT c2: no unflagged W 160 row")
        else:
            drift = "DRIFT-OK" if abs(t160 - DRIFT_TMIN) <= DRIFT_TOL_TMIN and abs(u160 - DRIFT_U) <= DRIFT_TOL_U else "DRIFT"
            out(f"DRIFT c2: overlap arm W160 tmin {t160:.3f} vs s60 {DRIFT_TMIN} (|d| {abs(t160 - DRIFT_TMIN):.3f} <= {DRIFT_TOL_TMIN}), u {u160:.3f} vs {DRIFT_U} (|d| {abs(u160 - DRIFT_U):.3f} <= {DRIFT_TOL_U}) -> {drift}"
                + ("" if drift == "DRIFT-OK" else "; the c2 price rows are their own session, s60's arms are not merged"))
        self.label("DRIFT-c2", drift)
        if drift == "DRIFT-OK":
            out("PRICE c2 merged with s60 (DRIFT-OK): s60's arms " + " | ".join(f"W{w}: u {v[0]:.3f} tmin {v[1]:.2f} tp50 {v[2]:.2f}" for w, v in sorted(C2_FROZEN["price"].items())))
        fz = c2_frozen_summary()
        self.landing(price, fz["F"], fz["S"], None, None, "c2")
        self.monotone(price, "c2")
        return price

    # ─────────────────────────── c3 (61a) ───────────────────────────
    def price_table(self, rows, ctl, ctl_m, ctl_w, mode, nstar):
        """Section 5.4 per arm: u = median_r gp / gp_ctl, the window minimum (median LATE tmin: native; at c3 also under G(n*)), Tp50 (median LATE tp50), each with a 90 % bootstrap interval over replicates."""
        out = self.out
        rnd = random.Random(SEED_C3)
        table = {}
        out(f"PRICE {rows[0].cell if rows else '-'} per arm (u, window minimum, Tp50; median over replicates, 90 % [bootstrap, 2000, seed {SEED_C3}]); flagged rows leave the medians:")
        for w, rs in sorted(self.by_w([r for r in rows if r.live and not r.flag]).items()):
            if w == ctl_w:
                continue
            us = [r.gp_() / (ctl.get(r.key) or ctl_m) for r in rs if (ctl.get(r.key) or ctl_m)]
            tm = [r.native_tmin() for r in rs]
            tg = [self.tmin_c3(r) for r in rs] if nstar else []
            tp = [r.native_tp50() for r in rs]

            def band(v):
                v = [x for x in v if x is not None]
                if len(v) < 2:
                    return None
                d = sorted(med([v[rnd.randrange(len(v))] for _ in v]) for _ in range(N_BOOT))
                return d[ALPHA_LO], d[ALPHA_HI]
            row = {"u": med(us), "tmin": med(tm), "tmin_g": med(tg) if tg else None, "tp50": med(tp), "n": len(rs), "ci": {"u": band(us), "tmin": band(tm), "tp50": band(tp)}}
            table[w] = row
            cs = lambda k: ("[%s, %s]" % (f(row["ci"][k][0], 3), f(row["ci"][k][1], 3))) if row["ci"][k] else "[-]"
            out(f"  W{w:<5} n={row['n']} u {f(row['u'], 3)} {cs('u')} | tmin {f(row['tmin'], 3)} {cs('tmin')}" + (f" (G({nstar}): {f(row['tmin_g'], 3)})" if nstar else "") + f" | tp50 {f(row['tp50'], 3)} {cs('tp50')}")
        return table

    def monotone(self, table, tag):
        ws = sorted(table)
        bad = [w for a, w in zip(ws, ws[1:]) if (table[w]["u"] or 0) < (table[a]["u"] or 0) - 1e-9 or (table[w]["tmin"] or 0) < (table[a]["tmin"] or 0) - 1e-9]
        self.out(f"MONOTONE {tag}: u and tmin non-decreasing in W over the arms: " + ("yes" if not bad else "NO, non-monotone at W " + ",".join(map(str, bad))))

    def landing(self, table, f_c, s_c, g_knee, rho_bar, tag):
        """W*(g, delta) = the W where T-hat-min(W) - F_c = g S_F,c + q(delta) F_c (linear interpolation over the arm medians); with u(W*) and Tp50(W*). Decides nothing (section 5.4)."""
        out = self.out
        if not table or f_c is None or s_c is None:
            out(f"LANDING {tag}: not computed (no floor reading)")
            return
        key = "tmin"
        pts = sorted((w, v[key]) for w, v in table.items() if v[key] is not None)
        gs = [("0", 0.0), ("0.25", 0.25), ("0.5", 0.5)] + ([("g_knee", g_knee)] if g_knee is not None else []) + ([("rho_bar", rho_bar)] if rho_bar is not None else []) + [("1", 1.0)]
        out(f"LANDING {tag}: W*(g, delta) with F_c {f(f_c, 3)} S_F,c {f(s_c, 3)} (no threshold is scored; the owner's call follows the table)")
        for nm, g in gs:
            for dn, q in (("Bulk", Q_BULK), ("Realtime", Q_RT)):
                target = f_c + g * s_c + q * f_c
                w_, _ = cross_up(pts, target)
                if w_ is None:
                    out(f"  g={nm:<7} {dn:<8} target {target:.2f}: " + ("below the lowest arm" if pts and pts[0][1] >= target else "beyond the highest arm"))
                    continue
                u_ = interp([(w, v["u"]) for w, v in table.items() if v["u"] is not None], w_)[0]
                tp_ = interp([(w, v["tp50"]) for w, v in table.items() if v["tp50"] is not None], w_)[0]
                out(f"  g={nm:<7} {dn:<8} target {target:.2f}: W* {w_:.1f} u {f(u_, 3)} tmin {target:.2f} tp50 {f(tp_, 2)}")

    def run_c3(self):
        out, rows = self.out, self.rows
        floors, kA, bA, p1, ctl_w, ksrc = self.classes()
        k1, k3 = (min(kA), max(kA)) if kA else (None, None)
        out(f"S61 SCORE plan=61a cell={C3} rows={len(rows)} replaced={len(self.replaced)}; floors {floors} B {bA} K {kA} P1 {p1} control W{ctl_w}; arms {ksrc}")
        if ctl_w is None or k1 is None:
            out("UNSCOREABLE: no control arm or no knee arms in the ledger")
            self.label("RHO", "UNSCOREABLE")
            return
        # ── CR61: X_own in session from the W 12 rows ──
        ref_w = floors[0] if floors else None
        x_own, e50, e90, nref = x_own_from([r for r in rows if r.W == ref_w and r.live and not r.dnf])
        flags = cr61([r for r in rows if r.live], k1, x_own)
        out(f"CR61 c3: X_own = {f(x_own, 2)} ms (E50 {f(e50, 2)}, E90 {f(e90, 2)}, from the {nref} W {ref_w} rows); COLLAPSED rows: "
            + (", ".join(f"{r.arm} s{r.seed} r{r.rep}" for r in rows if r.flag) or "none"))
        for r in rows:
            if r.flag is not None and (r.flag or r.W in (k1, p1)):
                out(f"  CR61 {r.arm} s{r.seed} r{r.rep}: {flags[id(r)][1]} -> {'COLLAPSED' if r.flag else 'ok'}")
        # ── control ──
        ctl, n_good, moved, ctl_m = ctl_gp(rows, ctl_w)
        out(f"CONTROL c3 W{ctl_w}: unflagged {n_good}/{sum(1 for r in rows if r.W == ctl_w)}, median goodput {f(ctl_m, 2)}" + ("  CONTROL-MOVED: nothing at c3 is scored" if moved else ""))
        self.label("CONTROL-c3", "CONTROL-MOVED" if moved else "OK")
        if moved:
            self.label("RHO", "NEEDS-MORE-control-pool")
            return
        ok_rows = [r for r in rows if r.live and not r.flag]
        # ── floor pool, validity ──
        fl_by = self.by_w([r for r in rows if r.W in floors])
        fpool, valid = floor_pool(fl_by, floors)
        for w in floors:
            vs = fl_by.get(w, [])
            out(f"FLOOR W{w}: valid {len(valid[w])}/{len(vs)} " + "; ".join(f"s{r.seed}r{r.rep} {floor_valid(r)[1]}" for r in vs if not floor_valid(r)[0]))
        if fpool is None:
            out("UNSCOREABLE: no floor pool holds 3 valid rows")
            self.label("RHO", "NEEDS-MORE-floor-rows")
            return
        moved_pool = fpool != floors[0]
        out(f"FLOOR POOL W{fpool}" + (f" (moved from W{floors[0]}: reported, section 4.2)" if moved_pool else ""))
        pools_up = [w for w in floors if w >= fpool and valid[w]]
        # ── per-row readings under G(250), native beside ──
        rd = {id(r): reading(r.w61(1), NSTAR_FLOOR) for w in pools_up for r in valid[w]}
        nat = {id(r): reading(r.w61(NK_NATIVE), None) for w in pools_up for r in valid[w]}
        n_floor = med([rd[i][2] for i in rd])
        par_f = parity_label(n_floor, PARITY["F"])
        for w in pools_up:
            out(f"  W{w}: G(250) S_F {f(med([rd[id(r)][0] for r in valid[w]]), 3)} F {f(med([rd[id(r)][1] for r in valid[w]]), 3)} n/group {f(med([rd[id(r)][2] for r in valid[w]]), 0)}"
                f" | native S_F {f(med([nat[id(r)][0] for r in valid[w]]), 3)} F {f(med([nat[id(r)][1] for r in valid[w]]), 3)}")
        pts_f = [(r.W, rd[id(r)][1]) for w in pools_up for r in valid[w]]
        pts_s = [(r.W, rd[id(r)][0]) for w in pools_up for r in valid[w]]
        s_pool = med([x[1] for x in pts_s if x[0] == fpool])
        tau = TAU_FRAC * s_pool
        gF, gS = gate(pts_f, fpool, tau), gate(pts_s, fpool, tau)
        for nm, g in (("F", gF), ("S_F", gS)):
            out(f"GATE {nm}: {g['label']} (tau {tau:.2f}; pools {g['pools']}" + (f"; b {g['b']:+.4f} +- {g['se']:.4f} ms/slot, E = b*W1 {g['E']:+.2f} +- {g['seE']:.2f})" if g["b"] is not None else ")"))
        self.label("FLOOR-F-c3", gF["label"])
        self.label("FLOOR-S-c3", gS["label"])
        # ── u-matched floor ──
        kappa = med([r.gp_() / (ctl.get(r.key) or ctl_m) / r.W for w in pools_up for r in valid[w] if (ctl.get(r.key) or ctl_m)])
        w_star, f_c, s_c, floor_ok = u_matched(gF, gS, kappa, U2, w_lowest=min(pools_up))
        if f_c is None:           # fewer than 2 pools: no line; the floor pool's medians stand (DECISION 5)
            f_c, s_c, w_star = med([x[1] for x in pts_f if x[0] == fpool]), s_pool, float(fpool)
            floor_ok = True
            out(f"FLOOR READING: fewer than 2 pools, the W {fpool} medians stand (DECISION 5): F_c {f_c:.3f} S_F,c {s_c:.3f}")
        else:
            out(f"FLOOR READING at c2's utilisation: kappa {kappa:.5f} per slot, W* = {U2}/kappa = {w_star:.1f} (lowest valid pool W{min(pools_up)}); F_c {f_c:.3f} S_F,c {s_c:.3f}" + ("" if floor_ok else "  UNSCOREABLE-FLOOR"))
        l_c = med([r.sixty("l") for w in pools_up for r in valid[w]])
        # ── parity at the knee side, the knee per replicate ──
        reps = {}
        for r in ok_rows:
            reps.setdefault(r.key, {})[r.W] = r
        kn = {}
        kside = [r for r in ok_rows if r.W in set(kA) | {p1}]
        n_knee = med([reading(r.w61(1), NSTAR_KNEE)[2] for r in kside])
        par_k = parity_label(n_knee, PARITY["K"])
        out(f"PARITY floor n {f(n_floor, 0)} ({par_f}, band {PARITY['F']}); knee n {f(n_knee, 0)} ({par_k}, band {PARITY['K']})")
        for key, rw in sorted(reps.items()):
            kn[key] = knee_of(rw, ctl.get(key), k1, k3, self.tmin_c3, kA, p1, ctl_w, ctl_m)
            k = kn[key]
            out(f"  KNEE s{key[1]}r{key[0]}: W95 {f(k['w95'], 1)}" + (f" Tmin(W95) {k['tmin']:.3f} sig {f(k['sig'], 3)} " if k.get("tmin") is not None else " ") + ("SCOREABLE" if k["scoreable"] else f"UNSCOREABLE ({k['why']})")
                + ("" if k["scoreable"] or k.get("tmin") is None else f" (chord upper bound need {k['tmin'] - f_c:.3f})"))
        scoreable = {k: v for k, v in kn.items() if v["scoreable"]}
        n_sc = len(scoreable)
        out(f"SCOREABLE replicates {n_sc} (need >= {MIN_SCOREABLE})")
        self.native_block(rows, reps, kA, p1)
        # ── the verdict-bearing readings ──
        stop = None
        if n_sc < MIN_SCOREABLE:
            stop = "UNSCOREABLE"
        elif par_f != "IN" or par_k != "IN":
            stop = "UNSCOREABLE-N"
        elif not floor_ok:
            stop = "UNSCOREABLE-FLOOR"
        price = self.price_table(ok_rows, ctl, ctl_m, ctl_w, "G", NSTAR_KNEE)
        self.monotone({w: v for w, v in price.items() if v["tmin_g"] is not None} if price else {}, "c3")
        fz = c2_frozen_summary()
        if stop:
            self.label("RHO", stop)
            self.label("D-S-Bulk", stop)
            out(f"RHO not scored: {stop}" + (f" (NEEDS-MORE: {'replicates' if stop == 'UNSCOREABLE' else 'a smaller floor n / the floor pool' })" if stop != "UNSCOREABLE-N" else "; the native reading is printed beside"))
            self.landing({w: dict(v, tmin=v["tmin_g"] if v["tmin_g"] is not None else v["tmin"]) for w, v in price.items()}, f_c, s_c, None, None, "c3")
            return
        needs_t = [v["tmin"] for v in scoreable.values()]
        sigs = [v["sig"] for v in scoreable.values()]
        rho3 = rho_of(needs_t, f_c, s_c)
        # ── one bootstrap: rho, W->0, D per candidate ──
        c2pt, c2d, c2pt0, c2d0 = c2_draws()
        rnd = random.Random(SEED_C3)
        keys = sorted(scoreable)
        ent = [scoreable[k] for k in keys]
        pool_rows = {w: [(rd[id(r)][0], rd[id(r)][1], r.sixty("l")) for r in valid[w]] for w in pools_up}
        drho, drho0, dd, dd0, rho3d, rho3d0 = [], [], {}, {}, [], []
        for b in range(N_BOOT):
            it = [ent[rnd.randrange(len(ent))] for _ in ent]
            pts = boot_rows(rnd, {w: [(r[0], r[1], r[2]) for r in rs] for w, rs in pool_rows.items()})
            xs = [w for w, _ in pts]
            fit_f, fit_s = ols(xs, [p[1] for _, p in pts]), ols(xs, [p[0] for _, p in pts])
            if fit_f is None or fit_s is None:
                fb_, sb_, ef, es = med([p[1] for _, p in pts]), med([p[0] for _, p in pts]), 0.0, 0.0
            else:
                fb_, sb_ = fit_f[1] + fit_f[0] * w_star, fit_s[1] + fit_s[0] * w_star
                ef, es = fit_f[0] * fpool, fit_s[0] * fpool
            lb = med([p[2] for _, p in pts])
            r3 = med([(e["tmin"] - fb_) / sb_ for e in it])
            r30 = med([(e["tmin"] - (fb_ - ef)) / (sb_ - es) for e in it]) if (sb_ - es) else r3
            rho3d.append(r3)
            rho3d0.append(r30)
            drho.append(r3 - c2d[b])
            drho0.append(r30 - c2d0[b])
            for nm, q in QS:
                for cn in ("S", "A0", "A0f", "Ac", "wq", "W80"):
                    vals = [(e["tmin"] - fb_) - candidates(e["sig"], e["p80"], lb, fb_, sb_, q)[cn] for e in it]
                    dd.setdefault((nm, cn), []).append(med(vals))
                    dd0.setdefault((nm, cn), []).append(med(vals) + ef)
        lo, hi = ci(drho)
        delta = min(fz["delta"], Q_RT * f_c / s_c)
        lab = rho_label(lo, hi, delta)
        lo0, hi0 = ci(drho0)
        lab0 = rho_label(lo0, hi0, delta)
        rlo, rhi = ci(rho3d)
        out(f"RHO c3 = {rho3:.3f} 90 % [{rlo:.3f}, {rhi:.3f}] (need per replicate " + " ".join(f"{t - f_c:.3f}" for t in needs_t) + f"); c2 rho_2 = {c2pt:.3f} 90 % [{ci(c2d)[0]:.3f}, {ci(c2d)[1]:.3f}] (frozen)")
        out(f"DELTA-RHO = {rho3 - c2pt:+.3f} 90 % [{lo:+.3f}, {hi:+.3f}]; margin Delta = min(c2 {fz['delta']:.3f}, c3 {Q_RT * f_c / s_c:.3f}) = {delta:.3f} -> {lab}")
        out(f"W->0: rho_3,0 {f(med([(t - (f_c - (gF['E'] or 0.0))) / (s_c - (gS['E'] or 0.0)) for t in needs_t]), 3)} (point); d_rho_0 90 % [{lo0:+.3f}, {hi0:+.3f}] vs c2 rho_2,0 {c2pt0:.3f} -> {lab0}")
        fs = opposite(lab, lab0)
        out(f"FLOOR-SENSITIVE: {'YES' if fs else 'no'} (scored {lab}, W->0 {lab0})")
        sig3 = med(sigs)
        sd_ = sigma_discrimination(c2pt, sig3, s_c)
        out(f"SIGMA-DISCRIMINATION = {sd_:+.3f} (sigma_3 {sig3:.3f}, S_F,3 {s_c:.3f}); " + (f"within +-{delta:.3f}: AGREES would not separate S from the sigma-scaled sentence" if abs(sd_) <= delta else "outside the margin: the sigma-scaled sentence is separable"))
        final = lab + ("" if abs(sd_) > delta or lab != "AGREES" else " (S or sigma-scaled)")
        self.label("RHO", "FLOOR-SENSITIVE" if fs else final)
        self.label("RHO-RAW", lab)
        self.label("SIGMA-DISCRIMINATION", "SEPARABLE" if abs(sd_) > delta else "NOT-SEPARABLE")
        # ── D per candidate ──
        out(f"D per candidate (need_r - A, median over {n_sc} replicates; 90 % bootstrap): Bulk is scored, Auto and Realtime reported")
        for nm, q in QS:
            line = []
            for cn in ("S", "A0", "A0f", "Ac", "wq", "W80"):
                pt = med([(e["tmin"] - f_c) - candidates(e["sig"], e["p80"], l_c, f_c, s_c, q)[cn] for e in ent])
                lo_, hi_ = ci(dd[(nm, cn)])
                lo0_, hi0_ = ci(dd0[(nm, cn)])
                lb_ = d_label(lo_, hi_)
                lb0 = d_label(lo0_, hi0_)
                fsd = opposite(lb_, lb0)
                line.append(f"{cn} {pt:+.2f} [{lo_:+.2f},{hi_:+.2f}] {lb_}{' FLOOR-SENSITIVE' if fsd else ''}")
                if nm == "Bulk":
                    self.label(f"D-{cn}-Bulk", "FLOOR-SENSITIVE" if fsd else lb_)
            out(f"  D {nm:<8} q={q:.4f}: " + " | ".join(line))
        # ── g handed to the owner, price and landing ──
        rho_bar = med([(t - f_c) / s_c for t in needs_t] + [(t - fz["F"]) / fz["S"] for t in C2_FROZEN["knee_tmin"]])
        g_knee = {C3: rho3 - Q_RT * f_c / s_c, C2: c2pt - fz["delta"]}
        out(f"HANDS TO THE OWNER (AGREES): rho_bar (pooled over both cells' replicates) {rho_bar:.3f}; g_knee c2 {g_knee[C2]:.3f} c3 {g_knee[C3]:.3f}")
        self.landing({w: dict(v, tmin=v["tmin_g"] if v["tmin_g"] is not None else v["tmin"]) for w, v in price.items()}, f_c, s_c, g_knee[C3], rho_bar, "c3")
        self.pending = (lab, lab0)

    # ── B1, P1, native readings (section 5.5) ──
    def native_block(self, rows, reps, kA, p1):
        out = self.out
        fl = [r for r in rows if r.W in plan61.FLOOR_W[C3] and r.live and not r.dnf]
        errs = [(r, r.sixty("err")) for r in fl]
        fires = sum(1 for _, e in errs if e is not None and e > 0)
        out(f"B1 (reported only): a floor row's median LATE err > 0: {fires}/{len(errs)} fire (predicted 0-3 of 18); margins -err: " + " ".join(f"{-e:+.2f}" for _, e in errs if e is not None))
        out("NATIVE (decides nothing): per-window estimator as the engine would see it: " + " | ".join(f"W{w}: tmin {f(med([r.native_tmin() for r in rs]), 3)} tp50 {f(med([r.native_tp50() for r in rs]), 3)}"
                                                                                              for w, rs in sorted(self.by_w([r for r in rows if r.live and not r.flag]).items()) if rs))
        out("L (printed per arm): " + " | ".join(f"W{w}: l {f(med([r.sixty('l') for r in rs]), 3)}" for w, rs in sorted(self.by_w([r for r in rows if r.live]).items())))
        b0 = {w: med([(r.b0(0)) for r in rs]) for w, rs in self.by_w([r for r in rows if r.live]).items()}
        fb, pb = med([v for w, v in b0.items() if w in plan61.FLOOR_W[C3] and v is not None]), b0.get(p1)
        out(f"P1 (reported): b0p at P1 {f(pb, 3)} over b0p at the floor pool {f(fb, 3)}" + (f" = {pb / fb:.2f}" if pb is not None and fb else ""))


# ───────────────────────────── entry points ─────────────────────────────
DECISIONS = [
    "1. tp50 / p50: the window p50 is [WSTAT] p50 (s61pred's), tmin is [SLAW] tmin, nk is [SLAW] nk; G pools windows with nk >= 1, native readings use nk >= 10 (s61pred's two window sets). row_extras (t61) carries them; the rehearsal's smoke check prints the SLAW tp50 vs WSTAT p50 agreement.",
    "2. Pilot-row bytes: with no LD_KNEE61 (the rehearsal) no row is 'at the knee bracket', so the pilot rows W 88 / 128 take section 1's formula (110 / 115 MB, no threshold the registration does not print); with LD_KNEE61 set, bytes are 120 MB at and above K1 and at the control, else the formula.",
    "3. The pilot's rule outside its four registered cases: u88 >= 0.95 > u128 (a non-monotone pair) falls to the last case, 88 * 0.95 / u88; every result is clamped to [64, 192]. The pilot control is R1 (2 048) when the control go/no-go passed, else R6 (512).",
    "4. The knee window: T-hat-min_r(W95,r) interpolates the row minima of the K and P1 arms (r68allow.at_knee); at c3 the row minimum is G(2 100)'s median group minimum. W95,r is cross_up over every non-control arm incl. the floors (r68allow.knee). sigma_r pools the sig windows of the two knee arms bracketing W95 (r68allow.bracket).",
    "5. Fewer than 2 floor pools (the gate reads UNSCOREABLE): no line is fitted, the floor pool's medians stand as F_c and S_F,c, W* = the pool, and the reading is flagged. W* itself is evaluated on the gate's OLS lines of all valid pools at and above the floor pool.",
    "6. The W->0 reading: the registration's sentence ('the extrapolated W -> 0 floor') and its formula (E = b W1) agree only at W* = W1; the scorer applies the formula, F - E_F and S_F - E_S with E = b * W1 (as at c2), refitted per bootstrap draw with rows resampled within pools. D at W->0 is D + E_F (the allowance's own F / S_F terms are not recomputed: the registration's literal reading).",
    "7. The c2 side of rho / d_rho is FROZEN (C2_FROZEN): the six knee tmins, the six floor (S_F, F) pairs and the G(800) rows of s60's c2, with draws ported draw for draw from s61pred.py (seed 60). The c3 side draws with seed 61; the paired d_rho draw b is c3's draw b minus c2's draw b.",
    "8. SIGMA-DISCRIMINATION's sigma_3 is the median over scoreable replicates of sigma_r (native sig at the bracketing knee arms); sigma_2 = 1.785 and S_F,2 = 5.083 are frozen. Delta is min(c2's frozen 0.085, c3's q_R F_c / S_F,c).",
    "9. CR61's e_own is taken against the row's own native median LATE tmin; X_own at c3 comes from the W 12 rows in session (live, not DNF), at c2 it is frozen at 10.66. A row with no [DIAG] ticks is not flagged (nothing to read). The arm's median gp for the goodput cut is over the arm's live rows (DNF = 0).",
    "10. CONTROL-MOVED: fewer than 4 unflagged controls (live, not DNF); a flagged / missing control is replaced for its replicate's u by the median goodput of the unflagged ones. kappa = median over valid floor rows of u_row / W.",
    "11. The rehearsal's floor go/no-go reads R2 / R3 on their own: X_own from the pair, 'goodput against their pair' = 0.95 x the pair's median; the control go/no-go uses [TRUTH] leg-0 loss (a fraction) against 0.0476 + 0.01. NOGO-CONTROL is a printed decision (LD_NOGO=control makes 61a refuse); a floor NO-GO sets LD_NOGO=floor (the floor moves to W 16 by the registered rule: the scorer's floor-pool rule already reads W 16 when W 12 fails).",
    "12. Labels: LABEL61 RHO (AGREES / DISAGREES / UNRESOLVED / FLOOR-SENSITIVE / UNSCOREABLE / UNSCOREABLE-N / UNSCOREABLE-FLOOR / NEEDS-MORE-*), RHO-RAW (the scored label before FLOOR-SENSITIVE), FLOOR-F-c3 / FLOOR-S-c3 (the gates), D-<cand>-Bulk, SIGMA-DISCRIMINATION, CONTROL-<cell>, DRIFT-c2.",
    "13. An AGREES label whose |SIGMA-DISCRIMINATION| <= Delta is printed 'AGREES (S or sigma-scaled)' in the RHO line's text; the label word stays AGREES for the tools that read it (RHO-RAW).",
    "14. The tables' per-arm 90 % intervals bootstrap the per-replicate values (2 000 draws, seed 61); a monotonicity check lists non-monotone arms; no threshold is scored in the price or landing tables.",
]
__doc__ += "\nDECISIONS (every ambiguity and the reading taken):\n" + "\n".join("  " + d for d in DECISIONS) + "\n"


def score(paths, plan, out=print):
    import place38_parse as pp
    if plan not in ("61a", "61b"):
        out(f"unknown plan {plan}")
        return 2
    rows_all, tokens = pp.rows_of(paths)
    cell = C3 if plan == "61a" else C2
    rows_all = [r for r in rows_all if r.get("cell") == cell and plan61.valid_slot(r.get("cell"), r.get("arm"))]
    cnt, replaced = counting_rows(rows_all)
    n_exp = len(plan61.plan(plan)) if (plan == "61b" or os.environ.get("LD_KNEE61")) else len(cnt)
    out(f"S61 SCORE plan={plan} (c3 sweep; registration docs/derivations/s61-registration.md); ABORT-TOKENS {' '.join(sorted({t for t in tokens if t in pp.ABORT_TOKENS})) or 'none'}")
    sc = Score61(cnt, replaced, plan, out)
    sc.run()
    nvoid = sum(1 for r in sc.rows_all if not r.live and not r.dnf)
    ndnf = sum(1 for r in sc.rows_all if r.dnf)
    terr = sum(1 for r in sc.rows_all if r.r.get("t60_error") or r.r.get("t61_error") or not r.t)
    out(f"VERDICT61 REPORTED-WITH-SCORED slots={len(cnt)}/{n_exp} live={sum(1 for r in sc.rows_all if r.live)} void={nvoid} replaced={len(replaced)} dnf={ndnf} t60_missing={terr} labels={len(sc.labels)}")
    for name, lab in sc.labels:
        out(f"LABEL61 {name} {lab}")
    return 0


# ───────────────────────────── the smoke / rehearsal (section 6) ─────────────────────────────
def rehearsal(rows, out):
    """The go/no-go decisions of 61r and PILOT61. Rows by slot: R1 G2048, R2 / R3 G12, R4 G88, R5 G128, R6 G512. -> (decisions dict, w_hat | None)."""
    by = {}
    for r in rows:
        by.setdefault(r.W, []).append(r)
    dec = {}
    # floor
    fl = [r for r in by.get(12, []) if r.live]
    x_own, *_ = x_own_from([r for r in fl if not r.dnf])
    pair_gp = med([r.gp_() for r in fl])
    ok_n = 0
    for r in fl:
        s, fm, n, g = reading(r.w61(1), NSTAR_FLOOR)
        e = e_own(r)
        sh = sum(v > x_own for v in e) / len(e) if e and x_own is not None else None
        coll = bool(sh is not None and sh > CR_SHARE and r.gp_() < CR_GP * (pair_gp or 0.0))
        t60n = (r.t.get("late", {}).get("0", {}) or {}).get("nk", [])
        share_nk = (sum(1 for v in t60n if (v or 0) > 0) / len(t60n)) if t60n else None
        crit = {"a": not r.dnf, "b": parity_label(n, PARITY["F"]) == "IN", "c": g >= MIN_FLOOR_GROUPS, "d": not coll, "e": share_nk is not None and share_nk >= FLOOR_NK_SHARE}
        out(f"  GO-NOGO-FLOOR {r.arm} s{r.seed} r{r.rep}: (a) no DNF {crit['a']} (b) realised n/group {f(n, 0)} in {PARITY['F']} {crit['b']} (c) groups {g} >= {MIN_FLOOR_GROUPS} {crit['c']} (d) not COLLAPSED {crit['d']} (share>X {f(sh, 3)}) "
            f"(e) nk>0 on {f(share_nk, 2)} of LATE windows {crit['e']}")
        ok_n += all(crit.values())
    dec["floor"] = ok_n >= 1
    out("GO-NOGO floor " + ("GO" if dec["floor"] else "NO-GO (the floor moves to W 16 for the whole of 61a, reported; set LD_NOGO=floor)") + f" ({ok_n} of {len(fl)} floor rehearsal rows pass)")
    # control
    def loss_ok(r):
        return r is not None and r.x.get("loss") is not None and r.x["loss"] <= EPS_C3 + CTL_LOSS_PP

    def one(w):
        v = [r for r in by.get(w, []) if r.live and not r.dnf]
        return v[-1] if v else None
    r1, r4, r5, r6 = one(2048), one(88), one(128), one(512)
    others = [r.gp_() for r in (r4, r5, r6) if r is not None]
    go1 = bool(r1 is not None and others and r1.gp_() >= GO_CTL_RATIO * max(others) and loss_ok(r1))
    out(f"  GO-NOGO-CONTROL R1 W2048: goodput {f(r1.gp_() if r1 else None, 2)} vs {GO_CTL_RATIO} x max(R4, R5, R6) = {f(GO_CTL_RATIO * max(others) if others else None, 2)}; loss truth {f(r1.x.get('loss') if r1 else None, 4)} <= {EPS_C3 + CTL_LOSS_PP:.4f}: {go1}")
    if r6 is not None and r1 is not None:
        out(f"  R6 W512 goodput {f(r6.gp_(), 2)} vs R1 {f(r1.gp_(), 2)}: ratio {r6.gp_() / r1.gp_() if r1.gp_() else float('nan'):.3f} (a healthy 2 048 control matches it within 2 %)")
    ctl_row, ctl_w = (r1, 2048) if go1 else ((r6, 512) if loss_ok(r6) else (None, None))
    dec["control"] = go1
    if go1:
        out("GO-NOGO control GO (control W 2048)")
    elif ctl_row is not None:
        out("GO-NOGO control NO-GO: the control arm becomes W 512 for 61a (registered fallback, R6 passes the loss test); set LD_CTL61=512")
    else:
        out("NOGO-CONTROL: neither R1 nor R6 passes: 61a does not launch (set LD_NOGO=control)")
    # pilot
    w_hat = None
    if ctl_row is not None and r4 is not None and r5 is not None and ctl_row.gp_() > 0:
        u88, u128 = r4.gp_() / ctl_row.gp_(), r5.gp_() / ctl_row.gp_()
        w_hat = pilot_knee(u88, u128)
        out(f"PILOT61 W={w_hat:.1f} u88={u88:.3f} u128={u128:.3f} arms={plan61.arms(w_hat)} (control W{ctl_w})")
    else:
        out("PILOT61 none: no valid control or a pilot row is missing / DNF: 61a does not launch (ABORT-PILOT)")
    for r in (r4, r5):
        if r is not None:
            out(f"  QSHARE {r.arm}: {f(r.x.get('qshare'), 3)} (reported; section 4.1)")
    # witnesses on R2 / R3 (reported)
    for r in fl:
        b = r.boot(0).get("b0k")
        ml = r.t.get("ml", {}).get("0")
        out(f"  WITNESS {r.arm} s{r.seed} r{r.rep} (REPORTED): b0k[0]={f(b[0], 3) if b else '-'} (expected 0); m = l for t < {s6.ML_T:g} s on {ml['eq'] if ml else '-'}/{ml['n'] if ml else '-'} windows")
    return dec, w_hat


def smoke(path, plan, out=print):
    """Section 6 / 7: PASS / FAIL lines ending SMOKE-PASS | SMOKE-FAIL. 61r = the 6 rehearsal rows (adds the go/no-go and PILOT61); 61a = the c3 smoke (G at K2); 61b = the c2 G160 smoke."""
    import place38_parse as pp
    if plan not in ("61a", "61b", "61r"):
        out(f"unknown plan {plan}")
        return 2
    slots = plan61.plan("61r") if plan == "61r" else plan61.plan(plan, smoke=True)
    rows, _ = pp.rows_of([path])
    got = {}
    for r in rows:
        got[(r.get("cell"), r.get("arm"), str(r.get("seed")), str(r.get("rep")))] = r         # the last row of a slot
    bad = []

    def crit(name, fails):
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)

    out(f"SMOKE61 plan={plan} rows={len(rows)} slots={len(slots)}")
    rvs, miss = [], []
    for (c, _, a, sd_, rp) in slots:
        r = got.get((c, a, str(sd_), str(rp)))
        if r is None:
            miss.append(f"{c} {a} s{sd_} r{rp}: no row")
        else:
            rvs.append(Row(r, len(rvs)))
    crit("one row per slot ran with a t60 and a t61", miss + [f"{rv.cell} {rv.arm}: no t60/t61 ({rv.r.get('t60_error')} {rv.r.get('t61_error')})" for rv in rvs if not rv.t or not rv.x])
    rvs = [rv for rv in rvs if rv.t and rv.x]
    lib = s6._lib()
    crit("the store-law replay modules (s60replay, caplog_replay) import", [lib[2]] if lib[2] else [])
    for rv in rvs:
        T = rv.t
        out(f"  ROW {rv.cell} {rv.arm} s{rv.seed} r{rv.rep}: status={rv.r.get('status')} dnf={int(rv.dnf)} tags(cli)={T['tags']['cli']} witness={T['witness']} caplog_ok={T['caplog'].get('ok')} q={T['q']} "
            f"windows={len(rv.x.get('win', {}).get('t', []))} ticks={rv.x.get('n_ticks')} qshare={f(rv.x.get('qshare'), 3)} loss={f(rv.x.get('loss'), 4)}")
    crit("every row carries ALL gauge tags (WSTAT, SLAW, WSTORE, SLAWC, CAPLOG) on the client after the warm-up cut", [f"{rv.cell} {rv.arm}: no {k}" for rv in rvs for k, v in rv.t["tags"]["cli"].items() if not v])
    crit("RWM_SLAW echoed 1 on both ends", [f"{rv.cell} {rv.arm}: cli={rv.t['gate_slaw']['cli']} srv={rv.t['gate_slaw']['srv']}" for rv in rvs if rv.t["gate_slaw"]["cli"] != "1" or rv.t["gate_slaw"]["srv"] != "1"])
    crit("[SLAW] q = 0.100000 on every row (every s61 row is Bulk; 0.050000 would be Realtime)", [f"{rv.cell} {rv.arm}: q={rv.t['q']}" for rv in rvs if rv.t["q"] != ["0.100000"]])
    crit("the per-row replay witness PASS (R~ with the row's own b0p and l reproduces printed R and Rb to 6e-6)", [f"{rv.cell} {rv.arm}: {rv.t['witness']}" for rv in rvs if not rv.dnf and rv.t["witness"].get("ok") is not True])
    crit("the CAPLOG replay PASS (records > 0 and every replayable cap reproduced)", [f"{rv.cell} {rv.arm}: {rv.t['caplog']}" for rv in rvs if rv.t["caplog"].get("ok") is not True])
    crit("t61 carries LATE windows with [WSTAT] p10 / p50 / p90 (G and CR61 read them)", [f"{rv.cell} {rv.arm}: windows={len(rv.x.get('win', {}).get('t', []))}" for rv in rvs if not rv.dnf and not any(v is not None for v in rv.x.get("win", {}).get("p50", []))])
    for rv in rvs:
        cl = rv.t["caplog"]
        share = (cl["replayed"] / cl["records"]) if cl["records"] else None
        out(f"  CAPLOG {rv.cell} {rv.arm}: records={cl['records']} replayable share={f(share, 3)} ({cl['replayed']} replayed, {cl['mismatches']} mismatches) rate={f(cl['rate_hz'], 1)}/s pooled={cl['n_pooled']} "
            f"bind(ceiling)={f(cl['ub'], 3)} bind(floor)={f(cl['fb'], 3)} seats={cl['seats']}")
        sl = [(a, b) for a, b in zip(rv.x.get("win", {}).get("p50", []), [w["tp50"] for w in rv.wins(0)]) if a is not None and b is not None]
        if sl:
            out(f"  SLAW-VS-WSTAT {rv.arm}: max |[WSTAT] p50 - [SLAW] tp50| over {len(sl)} windows = {max(abs(a - b) for a, b in sl):.4f} ms (DECISION 1)")
    if plan == "61r":
        rehearsal(rvs, out)
    if bad:
        out("SMOKE-FAIL " + "; ".join(bad))
        return 1
    out("SMOKE-PASS")
    return 0


def pilot_line(path, out=print):
    import place38_parse as pp
    rows, _ = pp.rows_of([path])
    cnt, _ = counting_rows(rows)
    _, w = rehearsal([Row(r, i) for i, r in enumerate(cnt) if plan61.valid_slot(r.get("cell"), r.get("arm"))], out)
    return 0 if w is not None else 1


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd == "smoke61" and len(a) == 2:
        return smoke(a[1], a[0])
    if cmd == "score61" and len(a) >= 2:
        return score(a[1:], a[0])
    if cmd == "pilot" and len(a) == 1:
        return pilot_line(a[0])
    print(f"usage: {sys.argv[0]} smoke61 <61a|61b|61r> <ledger> | score61 <61a|61b> <ledger> [...] | pilot <ledger>")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
