#!/usr/bin/env python3
"""Status 62: the scorer of c3 at jitter 0 (docs/derivations/s62-registration.md; the registered prototype is docs/derivations/store-law/s62pred.py / .out, which this module ports: test_s62.py reproduces its numbers
on the 61c / 61a / c1s ledgers). Built ON s61_score.py / s60_score.py (the row parse, the G(n*) grouping, e_own, CR61's pieces), not a fork.

    s62_score.py smoke62 <62r0|62a> <ledger>    the smoke / rehearsal check: PASS / FAIL lines, the first readings, `LABEL62 GO-c3j0 GO|NO-GO-<rule>`, PILOT62, ending SMOKE-PASS | SMOKE-FAIL
    s62_score.py score62 62a <ledger>           every registered readout and `LABEL62 ...` lines (CONTROL-c3j0, GATE-J0, NEED0, NEED0-W12 / NEED0-INT), then `VERDICT62`
    s62_score.py reprobe <ledger>               prints `LO HI` when the rehearsal's pilot case is not proportional and a re-probe is registered (section 3.3), else exits 1
    s62_score.py pilot <ledger>                 the PILOT62 line from a rehearsal ledger

The scored quantity is need_0 = median_r T-hat-min_r(W95,r) - F_0 in ms, against Delta_0 = 1/2 q_R F_0 (section 1.3); D rows (kind D, RWM_RTT_DUMP=1) are excluded from every scored median, from CR62's reference and from the
arm medians (a test asserts it) and feed only the reported items. Inputs: the ledger, LD_KNEE62 (the rehearsal's `PILOT62 W=`, required) and LD_CTL62=512 (required). Every place the registration is silent is a DECISION."""
import os
import random
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import plan62  # noqa: E402
import s60_score as s6  # noqa: E402
import s61_score as s1  # noqa: E402

C3J0 = plan62.C3J0
med, pq, f = s6.med, s6.pq, s6.f
Q_R, Q_B = 0.05, s1.Q_BULK                       # RFC 8289's band (section 1.3) and the Bulk q (decision 8: q(delta) = 0.10)
NF, NK = s1.NSTAR_FLOOR, s1.NSTAR_KNEE           # the floor's fixed count G(250) and the knee's G(2100) (section 1.2)
GATE_D0 = 0.5                                    # section 1.1: the floor's count drop must be below 0.5 ms
SHARE_MAX, GP_CUT, RATIO_MAX = 0.10, 0.95, 1.5   # CR62 (section 3.1)
FLOOR2 = 24                                      # the second floor arm (section 1.2)
N_BOOT, SEED = 2000, 62
MIN_CTL, MIN_FLOOR_ROWS, MIN_SCOREABLE = 4, 3, 5
CTL_GP_RATIO = 0.98                              # section 5 step 2: the control arm's median goodput >= 0.98 x max(K3, P1 arm medians)
CTL_LOSS_MAX = s1.EPS_C3 + s1.CTL_LOSS_PP        # and every control row's loss truth <= 4.76 % + 1 pp
MIN_LATE_WINDOWS = 100                           # section 3.3: >= 100 native LATE windows on every rehearsal row
DUMP_COVERAGE_MIN = 0.95                         # section 3.3
# J = 5's need at the three floor counts, from the 61c / 61a stand-ins (s62pred.out section 7: other sessions), reported beside J = 0's
J5_NEED = {"61c": (3.481, 7.455, 11.914), "61a": (3.770, 7.232, 10.841)}
J5_S_F_NATIVE = {"61c": 5.801, "61a": 5.941}
J5_SPREAD = {"61c": 8.169, "61a": 8.285}         # J = 5 floor windows' native p90 - p10 (ms), median over the six W 12 rows: computed from the 61c / 61a ledgers (s62pred.out prints only p50 - min); test_s62 recomputes them on the host
COUNT_RATIO_BAND = (0.95, 1.05)                  # the dump's count test (section 6): dumped samples / the [SLAW] windows' own sample count (the same record_rtt_sample stream)


def q(v, p):
    """s62pred.py's percentile: v[int(p (n - 1))] of the sorted non-None values (the 90 % interval is q(.05) / q(.95): sorted draws [99] / [1899] of 2 000, as the prototype)."""
    v = sorted(x for x in v if x is not None)
    return v[int(p * (len(v) - 1))] if v else None


def ols(xs, ys):
    """-> (b, a, se) or None (s60_score.ols)."""
    return s6.ols(xs, ys)


# ───────────────────────────── the row view ─────────────────────────────
class Row(s1.Row):
    """A ledger row with its t60 / t61 (s61_score.Row) and t62; arm G<W> (scored) or D<W> (the dump rows, kind D)."""

    def __init__(self, r, order=0):
        super().__init__(r, order)
        self.kind, self.W = plan62.arm_parse(self.arm)
        self.x2 = r.get("t62") or {}

    def late_n(self):
        """The number of native LATE windows (nk >= 10)."""
        return len(self.w61(s1.NK_NATIVE))


def counting_rows(rows_all):
    return s1.counting_rows(rows_all)


class Cell:
    """The scored rows of the cell: kind G only (D rows are kept apart and never enter a median), with the arm roles: floors (W 12, W 24), the K arms, P1, the control."""

    def __init__(self, name, rows, floor_w, karms, p1, ctl, floor2=FLOOR2):
        self.name, self.floor_w, self.floor2, self.karms, self.p1, self.ctl = name, floor_w, floor2, tuple(karms), p1, ctl
        self.rows = [r for r in rows if r.kind == "G"]
        self.dump_rows = [r for r in rows if r.kind == "D"]


# ───────────────────────────── the row-time extras (the VM side) ─────────────────────────────
_DUMP = re.compile(r"\[RTTDUMP\] p=(\d+) t0=(\d+) n=(\d+) d=(\S*)")
_SIGN = re.compile(r"\bp0:infl=.*?sig_us=\d+/n(\d+)")


def dump_reading(cli):
    """The RTT dump's reading of one row (`[RTTDUMP] p= t0= n= d=<dt,rtt;...>`, rttdump.rs): samples dumped on path 0, the estimators' own sample count (the last [DIAG] `sig_us=../n<count>`), coverage = dumped / seen,
    whether `[RTTDUMP-CAP]` printed, and the late samples' (t >= 15 s on the dump's own clock) interquartile range in ms. None without a dump line."""
    n, t_abs, rtts, cap, seen, slaw_nk = 0, None, [], False, None, 0
    for ln in cli or []:
        if s6.WARM in ln:
            slaw_nk = 0                  # the gauge's windows count from the last warm-up marker (the cut every s6x readout takes)
        if "[RTTDUMP-CAP]" in ln:
            cap = True
        if ln.startswith("[SLAW]") and re.search(r"\bp=0\b", ln):
            g = re.search(r"\bnk=(\d+)", ln)
            slaw_nk += int(g.group(1)) if g else 0
        m = _DUMP.search(ln)
        if m and m.group(1) == "0":
            n += int(m.group(3))
            t = int(m.group(2))
            for part in m.group(4).split(";"):
                if "," in part:
                    dt, rtt = part.split(",", 1)
                    t += int(dt)
                    t_abs = t if t_abs is None else t_abs
                    rtts.append((t, int(rtt) / 1000.0))
        elif ln.startswith("[DIAG]"):
            g = _SIGN.search(ln)
            if g:
                seen = int(g.group(1))
    if not n and not cap:
        return None
    t0 = rtts[0][0] if rtts else 0
    late = [v for t, v in rtts if t - t0 >= plan62.LATE_T[C3J0] * 1e6]
    return {"n": n, "diag_n": seen, "coverage": (n / seen) if seen else None, "cap": cap, "n_late": len(late), "iqr": (pq(late, 0.75) - pq(late, 0.25)) if len(late) >= 4 else None,
            "slaw_nk": slaw_nk, "count_ratio": (n / slaw_nk) if slaw_nk else None}


def netem_bad(r):
    """The reasons a row fails the live-netem witness (ABORT-NETEM): a WRONG read (a jitter term, another delay, no netem root) always; a MISSING read only on a live row (it completed, so its driver reached the post read);
    a row with no t62 at all. [] = clean."""
    x = r.x2
    if not x or x.get("netem_problems") is None:
        return ["no t62 netem witness"]
    return list(x.get("netem_wrong") or []) + (list(x.get("netem_missing") or []) if r.live else [])


def row_extras(cli, srv, drv, cell):
    """The `t62` of one row: `netem_problems` (the live-qdisc witness of both legs before and after the transfer: [] = clean, a list = what is wrong or missing) and the dump's reading. Never raises on log content."""
    import l1common as lc
    out = {"v": 1, "netem_problems": None, "netem_wrong": [], "netem_missing": [], "netem": {}, "dump": None}
    try:
        got, prob = lc.netem_witness(drv)
        out["netem"], out["netem_problems"] = {f"{t}/{leg}": ok for (t, leg), (ok, _) in got.items()}, prob
        out["netem_missing"] = [x for x in prob if x.endswith("no [NETEM] witness")]
        out["netem_wrong"] = [x for x in prob if x not in out["netem_missing"]]
        out["dump"] = dump_reading(cli)
    except Exception as e:      # a reported readout never voids a row
        out["error"] = f"{type(e).__name__}: {e}"
    return out


# ───────────────────────────── CR62 (section 3.1) ─────────────────────────────
def cr62(c, out=None):
    """CR61 made J = 0-safe: X_own = E90 + 2 (E90 - E50) over the floor arm's rows, X_min = q_R F_floor, X_use = max; floor arms (below K1) COLLAPSED iff share{e_own > X_use} > 0.10 AND gp < 0.95 x the arm's median
    goodput; at and above K1 COLLAPSED iff (qshare > 0.10 OR E90_own / max(p90 - tmin, X_min) > 1.5) AND gp < 0.95 x the arm's median; a clause that fires at full goodput is RULE-FINDING-CR62 and voids nothing.
    Sets row.flag; -> dict(x_own, e50, e90, x_min, x_use, binds, n_ratio, rows=[...]) (the raw s61 form rides beside each row)."""
    fl = [r for r in c.rows if r.W == c.floor_w]
    per = [(pq(e, 0.5), pq(e, 0.9)) for e in (s1.e_own(r) for r in fl) if e]
    e50, e90 = med([p[0] for p in per]), med([p[1] for p in per])
    x_own = e90 + 2 * (e90 - e50) if e90 is not None else None
    f_fl = med([s1.reading(r.w61(1), NF)[1] for r in fl])
    x_min = Q_R * f_fl if f_fl is not None else None
    x_use = max(v for v in (x_own, x_min) if v is not None) if (x_own is not None or x_min is not None) else None
    k1 = c.karms[0]
    arm_gp = {}
    for r in c.rows:
        arm_gp.setdefault(r.W, []).append(r.gp_())
    binds, rows = 0, []
    for r in sorted(c.rows, key=lambda r: (r.W, r.rep)):
        gp_low = r.gp_() < GP_CUT * med(arm_gp[r.W])
        eo = s1.e_own(r)
        if r.W < k1:
            sh_raw = sum(1 for x in eo if x > x_own) / len(eo) if eo and x_own is not None else 0.0
            sh = sum(1 for x in eo if x > x_use) / len(eo) if eo and x_use is not None else 0.0
            raw_flag, flag, finding = sh_raw > SHARE_MAX and gp_low, sh > SHARE_MAX and gp_low, False
            txt = f"share>X_own {sh_raw:.2f} / >X_use {sh:.2f}"
        else:
            e90r = pq(eo, 0.9) if eo else None
            den_raw = (r.native_p90() or 0) - (r.native_tmin() or 0)
            den = max(den_raw, x_min) if x_min is not None else den_raw
            binds += den_raw < (x_min or 0)
            qs = r.x.get("qshare") or 0.0
            rr = e90r / den_raw if e90r is not None and den_raw > 0 else None
            ru = e90r / den if e90r is not None and den > 0 else None
            fires = (qs > SHARE_MAX) or (ru is not None and ru > RATIO_MAX)
            raw_flag = (qs > SHARE_MAX and gp_low) or (rr is not None and rr > RATIO_MAX)
            flag, finding = gp_low and fires, (not gp_low) and fires      # every clause needs the goodput cut
            txt = ("  RULE-FINDING-CR62 (fires at full goodput: not a collapse)" if finding else "") + \
                  f" qshare {qs:.2f} ratio raw {rr if rr is None else round(rr, 2)} / floored {ru if ru is None else round(ru, 2)} (den {den_raw:.2f}{' -> ' + format(den, '.2f') if den != den_raw else ''})"
        r.flag = flag
        rows.append({"row": r, "raw_flag": raw_flag, "flag": flag, "finding": finding, "text": txt})
    info = {"x_own": x_own, "e50": e50, "e90": e90, "x_min": x_min, "x_use": x_use, "binds": binds, "n_ratio": sum(1 for r in c.rows if r.W >= k1), "rows": rows, "f_floor": f_fl,
            "findings": sum(1 for x in rows if x["finding"])}
    if out:
        out(f"CR62 {c.name}: X_own = {f(x_own, 2)} ms (E50 {f(e50, 2)}, E90 {f(e90, 2)}, floor W{c.floor_w}); X_min = q_R * F_floor = {f(x_min, 2)} ms -> X_use {f(x_use, 2)}"
            f"{' (X_min BINDS)' if (x_min is not None and (x_own is None or x_min > x_own)) else ''}; ratio denominators floored on {binds} of {info['n_ratio']} rows; RULE-FINDING-CR62 on {info['findings']} rows")
        by = {}
        for x in rows:
            a = by.setdefault(x["row"].W, [0, 0, 0, 0])
            a[0] += 1
            a[1] += x["raw_flag"]
            a[2] += x["flag"]
            a[3] += x["finding"]
        out("  flagged per arm (raw s61 form / CR62 / RULE-FINDING): " + " | ".join(f"W{w} {a[1]}/{a[0]} -> {a[2]}/{a[0]} / {a[3]}" for w, a in sorted(by.items())))
        for x in rows:
            r = x["row"]
            if x["raw_flag"] or x["flag"] or x["finding"] or r.W in (c.floor_w, c.ctl):
                out(f"  G{r.W} s{r.seed} r{r.rep}: {x['text']}{'  RAW-FLAG (s61 form)' if x['raw_flag'] else ''}{'  CR62-FLAG' if x['flag'] else ''}")
    return info


# ───────────────────────────── the scored path (section 5 steps 2-5) ─────────────────────────────
def label(ci, delta0, d0, n_ok, n_floor_ok=True):
    """Section 1.3's table in its order (CONTROL-MOVED is decided before): UNSCOREABLE (D0 >= 0.5; fewer than 3 valid rows on a floor arm; fewer than 5 replicates), BELOW (CI upper < Delta_0), ABOVE (CI lower > Delta_0), UNRESOLVED."""
    lo, hi = ci
    if not n_floor_ok:
        return "UNSCOREABLE (floor: fewer than 3 valid rows on an arm)"
    if d0 is None or d0 >= GATE_D0:
        return "UNSCOREABLE (gate: D0 >= 0.5 ms)"
    if n_ok < MIN_SCOREABLE or lo is None:
        return "UNSCOREABLE (fewer than 5 replicates with W95 in [K1, K3])"
    if hi < delta0:
        return "BELOW"
    if lo > delta0:
        return "ABOVE"
    return "UNRESOLVED"


def final_label(ci, cii, delta0, d0, n_ok, n_floor_ok=True):
    """-> (label, W12 reading, intercept reading): FLOOR-SENSITIVE iff the W 12 and intercept readings give opposite definite labels (it replaces either)."""
    a, b = label(ci, delta0, d0, n_ok, n_floor_ok), label(cii, delta0, d0, n_ok, n_floor_ok)
    if {a, b} == {"BELOW", "ABOVE"}:
        return f"FLOOR-SENSITIVE (W12 {a}, intercept {b})", a, b
    return a, a, b


def control_check(c, live, out=None):
    """Section 5 step 2: >= 4 unflagged W 512 controls, the control arm's median goodput >= 0.98 x max(K3, P1 arm medians), and every control row's loss truth <= 4.76 % + 1 pp; else CONTROL-MOVED.
    -> (ok, ctl_rows, ctl_median, why)."""
    ctl = [r for r in live if r.W == c.ctl and r.gp_()]
    why = []
    if len(ctl) < MIN_CTL:
        why.append(f"unflagged {len(ctl)} < {MIN_CTL}")
    ctl_m = med([r.gp_() for r in ctl])
    ref = [med([r.gp_() for r in live if r.W == w]) for w in ((c.karms[-1], c.p1) if c.p1 else (c.karms[-1],)) if any(r.W == w for r in live)]
    if ctl_m is not None and ref and ctl_m < CTL_GP_RATIO * max(ref):
        why.append(f"median goodput {ctl_m:.2f} < {CTL_GP_RATIO} x max(K3, P1) = {CTL_GP_RATIO * max(ref):.2f}")
    hot = [f"s{r.seed}r{r.rep} {r.x['loss']:.4f}" for r in ctl if r.x.get("loss") is not None and r.x["loss"] > CTL_LOSS_MAX]
    if hot:
        why.append(f"loss truth above {CTL_LOSS_MAX:.4f}: " + ", ".join(hot))
    nol = [f"s{r.seed}r{r.rep}" for r in ctl if r.x.get("loss") is None]
    if nol:
        why.append("loss truth missing (read fail-closed, as the rehearsal's clause): " + ", ".join(nol))
    if out:
        out(f"CONTROL {c.name} W{c.ctl}: unflagged {len(ctl)}/{sum(1 for r in c.rows if r.W == c.ctl)}, median goodput {f(ctl_m, 2)} Mbit/s, reference max(K3, P1) {f(max(ref) if ref else None, 2)}"
            + ("  CONTROL-MOVED: " + "; ".join(why) if why else "  OK"))
    return not why, ctl, ctl_m, why


def scored(c, rnd, out=None, ctl_by_goodput=False):
    """The scored path on a Cell whose CR62 flags are set (cr62 ran). `ctl_by_goodput` lifts CR62 on the control arm only (an exercise of the prototype's). -> dict with res['label'] (the section 1.3 label or
    CONTROL-MOVED / UNSCOREABLE) and every reading; `rnd` is the bootstrap's random.Random (seed 62), drawn in the prototype's call order."""
    live = [r for r in c.rows if r.live and (not r.flag or (ctl_by_goodput and r.W == c.ctl))]
    res = {"name": c.name}
    ok_ctl, ctl, ctl_m, why = control_check(c, live, out)
    res["ctl_m"], res["ctl_why"] = ctl_m, why
    if not ok_ctl:
        res["label"] = "CONTROL-MOVED"
        return res
    fa = {W: [r for r in live if r.W == W and not r.dnf and s1.reading(r.w61(1), NF)[3] >= s1.MIN_FLOOR_GROUPS] for W in (c.floor_w, c.floor2)}
    fl, fl2 = fa[c.floor_w], fa[c.floor2]
    res["n_floor"] = (len(fl), len(fl2))
    if len(fl) < MIN_FLOOR_ROWS or len(fl2) < MIN_FLOOR_ROWS:
        res["label"] = "UNSCOREABLE (floor: fewer than 3 valid rows on an arm)"
        return res

    def fread(rs):
        return {"nat": med([r.native_tmin() for r in rs]), "g250": med([s1.reading(r.w61(1), NF)[1] for r in rs]), "g2100": med([s1.reading(r.w61(1), NK)[1] for r in rs])}

    f1, f2 = fread(fl), fread(fl2)
    res.update(f1=f1, f2=f2)
    if None in (*f1.values(), *f2.values()):
        res["label"] = "UNSCOREABLE (floor: no group at a count: G(250) / G(2100) / native)"
        return res
    d0 = max(f1["nat"], f1["g250"]) - f1["g2100"]
    d0b = max(f2["nat"], f2["g250"]) - f2["g2100"]
    reps = {}
    for r in live:
        reps.setdefault((r.rep, r.seed), {})[r.W] = r
    knees = []
    for key, wm in sorted(reps.items()):
        c_ = wm.get(c.ctl)
        gref = c_.gp_() if c_ and c_.gp_() else ctl_m
        pts = [(W, r.gp_() / gref) for W, r in wm.items() if W != c.ctl and r.gp_()]
        w95 = s6.cross_up(pts, 0.95)[0]
        arms = [W for W in tuple(c.karms) + ((c.p1,) if c.p1 else ()) if W in wm]
        if w95 is None or len(arms) < 2 or not (c.karms[0] <= w95 <= c.karms[-1]):
            knees.append((key, w95, None))
            continue
        knees.append((key, w95, s6.interp([(W, wm[W].native_tmin()) for W in arms], w95)[0]))
    ok = [t for k, w, t in knees if t is not None]
    a250 = [s1.reading(r.w61(1), NF)[1] for r in fl]
    b250 = [s1.reading(r.w61(1), NF)[1] for r in fl2]

    def intercept(xa, xb):
        xs = [c.floor_w] * len(xa) + [c.floor2] * len(xb)
        b, a, _ = ols(xs, xa + xb)
        return a

    F0, Fi = med(a250), intercept(a250, b250)
    need = med([t - F0 for t in ok]) if ok else None
    need_i = med([t - Fi for t in ok]) if ok else None
    dr, di = [], []
    for _ in range(N_BOOT):
        xa = [a250[rnd.randrange(len(a250))] for _ in a250]
        xb = [b250[rnd.randrange(len(b250))] for _ in b250]
        kb = [ok[rnd.randrange(len(ok))] for _ in ok] if ok else []
        if kb:
            dr.append(med([t - med(xa) for t in kb]))
            di.append(med([t - intercept(xa, xb) for t in kb]))
    ci = (q(dr, 0.05), q(dr, 0.95)) if dr else (None, None)
    cii = (q(di, 0.05), q(di, 0.95)) if di else (None, None)
    s_nat = med([med([w["p50"] - w["tmin"] for w in r.w61(s1.NK_NATIVE) if w["p50"] is not None and w["tmin"] is not None]) for r in fl])
    s_g250 = med([s1.reading(r.w61(1), NF)[0] for r in fl])
    res.update(F0=F0, Fi=Fi, f1=f1, f2=f2, d0=d0, d0b=d0b, need=need, ci=ci, need_i=need_i, cii=cii, n_ok=len(ok), knees=knees, S_nat=s_nat, S_g250=s_g250, delta0=0.5 * Q_R * F0,
               n3={k: (med([t - f1[k] for t in ok]) if ok else None) for k in ("nat", "g250", "g2100")}, ctl_rows=len(ctl), live=live)
    lab, a, b = final_label(ci, cii, res["delta0"], d0, len(ok))
    res.update(label=lab, label_w12=a, label_int=b)
    if out:
        out(f"FLOOR W{c.floor_w} (valid {len(fl)}): native {f(f1['nat'], 3)} | G250 {f(f1['g250'], 3)} | G2100 {f(f1['g2100'], 3)} -> D0 {d0:.3f} ms, gate {'PASS' if d0 < GATE_D0 else 'FAIL'}"
            f" | FLOOR W{c.floor2} (valid {len(fl2)}): native {f(f2['nat'], 3)} | G250 {f(f2['g250'], 3)} | G2100 {f(f2['g2100'], 3)} -> D0 {d0b:.3f} (reported)")
        out("KNEES: " + " | ".join(f"s{k[1]}r{k[0]} W95 {'-' if w is None else format(w, '.1f')} {'Tmin ' + format(t, '.3f') if t is not None else 'UNSCOREABLE (outside [K1, K3])'}" for k, w, t in knees))
        if need is not None:
            out(f"NEED0 (native knee - F(G250) at W{c.floor_w}) {need:.3f} ms, 90 % [{ci[0]:.3f}, {ci[1]:.3f}]; at the intercept (F {Fi:.3f}) {need_i:.3f} [{cii[0]:.3f}, {cii[1]:.3f}];"
                f" Delta_0 = 1/2 q_R F0 = {res['delta0']:.3f} ms; {len(ok)} scoreable replicates")
    return res


def reported(c, res, cr, out):
    """Section 6's reported items, deciding nothing."""
    if "need" not in res or res.get("need") is None:
        out("REPORTED: no need_0 (the readings below need a scoreable cell)")
        return
    n3 = res["n3"]
    out(f"REPORTED need_0 at the W{c.floor_w} floor read native {f(n3['nat'], 3)} | G250 {f(n3['g250'], 3)} | G2100 {f(n3['g2100'], 3)} ms; at the intercept {f(res['need_i'], 3)} ms")
    for k, v in J5_NEED.items():
        out(f"REPORTED J = 5 need ({k}, another session) native {v[0]:.3f} | G250 {v[1]:.3f} | G2100 {v[2]:.3f} ms; native S_F {J5_S_F_NATIVE[k]:.3f}")
    a0 = res["S_g250"] + Q_B * res["F0"]
    a0n = res["S_nat"] + Q_B * res["F0"]
    out(f"REPORTED S_F(J0): native (p50 - min) {f(res['S_nat'], 3)} ms, at G(250) {f(res['S_g250'], 3)} ms; decision 8's Bulk supply A0 = S_F(J0, G250) + q_B F0 = {a0:.3f} ms (q_B {Q_B}, F0 {res['F0']:.3f}); "
        f"D = need_0 - A0 = {res['need'] - a0:+.3f} ms (with the native S_F: A0 {a0n:.3f}, D {res['need'] - a0n:+.3f})")
    fl = [r for r in res["live"] if r.W == c.floor_w]
    k2 = [r for r in res["live"] if r.W == c.karms[1]] if len(c.karms) > 1 else []
    for nm in ("p10", "p50"):
        rise = (med([med([w[nm] - w["tmin"] for w in r.w61(s1.NK_NATIVE) if w[nm] is not None and w["tmin"] is not None]) for r in k2]), med([med([w[nm] - w["tmin"] for w in r.w61(s1.NK_NATIVE) if w[nm] is not None and w["tmin"] is not None]) for r in fl]))
        out(f"REPORTED {nm} - tmin (native windows' median): K2 arm {f(rise[0], 3)} ms, W{c.floor_w} floor {f(rise[1], 3)} ms, rise {f(None if None in rise else rise[0] - rise[1], 3)} ms")
    dumps = {r.W: r for r in c.dump_rows if r.live}
    for w, r in sorted(dumps.items()):
        d = r.x2.get("dump") or {}
        cr_ = d.get("count_ratio")
        out(f"REPORTED dump count test D{w}: dumped {d.get('n')} samples vs the [SLAW] windows' own sample count {d.get('slaw_nk')}: ratio {f(cr_, 3)} -> "
            + ("PASS" if cr_ is not None and COUNT_RATIO_BAND[0] <= cr_ <= COUNT_RATIO_BAND[1] else "FAIL") + f" (band {COUNT_RATIO_BAND[0]} .. {COUNT_RATIO_BAND[1]}: one stream)")
        out(f"REPORTED dump D{w} s{r.seed} r{r.rep}: dumped {d.get('n')} samples of {d.get('diag_n')} seen (coverage {f(d.get('coverage'), 3)}), cap {'HIT' if d.get('cap') else 'no'}, IQR(late) {f(d.get('iqr'), 3)} ms"
            + ("" if d.get("coverage") is not None and d["coverage"] >= DUMP_COVERAGE_MIN and not d.get("cap") else "  DUMP-READING-MISSING (coverage below 0.95 or [RTTDUMP-CAP])"))
    iq = {w: (r.x2.get("dump") or {}).get("iqr") for w, r in dumps.items()}
    if len(iq) == 2 and None not in iq.values():
        lo_w, hi_w = sorted(iq)
        out(f"REPORTED dump-IQR rise D{hi_w} - D{lo_w}: {iq[hi_w] - iq[lo_w]:+.3f} ms")
    out(f"REPORTED D0 at W{c.floor2} {res['d0b']:.3f} ms; reordering: not measured (no counter exists)")


# ───────────────────────────── the scorer entry ─────────────────────────────
def arms_from(rows_by_w, knee=None):
    """-> (floors, karms, p1, ctl_w): from LD_KNEE62 (the rehearsal's PILOT62 W-hat; required for a launch)."""
    if knee is None:
        raise ValueError("LD_KNEE62 is unset: the scorer needs the rehearsal's PILOT62 W-hat (ABORT-PILOT)")
    a = plan62.arms(knee)
    return plan62.FLOOR_W, tuple(a["K"]), a["P1"], plan62.CTL_W


def score(paths, plan, out=print):
    import place38_parse as pp
    if plan != "62a":
        out(f"unknown plan {plan}")
        return 2
    if os.environ.get("LD_CTL62") != "512":
        out(f"ABORT-CONTROL62 LD_CTL62='{os.environ.get('LD_CTL62', '')}' (the control is W 512): nothing is scored")
        return 3
    try:
        knee = plan62.knee_env()
        _, karms, p1, ctl_w = arms_from(None, knee)
    except ValueError as e:
        out(f"ABORT-PILOT {e}")
        return 3
    rows_all, tokens = pp.rows_of(paths)
    rows_all = [r for r in rows_all if r.get("cell") == C3J0 and plan62.valid_slot(r.get("cell"), r.get("arm"))]
    cnt, replaced = counting_rows(rows_all)
    rows = [Row(r, i) for i, r in enumerate(cnt)]
    n_exp = len(plan62.plan("62a", knee=knee))
    out(f"S62 SCORE plan=62a cell={C3J0} rows={len(rows)} replaced={len(replaced)}; floors {plan62.FLOOR_W} K {karms} P1 {p1} control W{ctl_w} (LD_KNEE62={knee:g}); ABORT-TOKENS {' '.join(sorted({t for t in tokens if t in pp.ABORT_TOKENS})) or 'none'}")
    nem = [f"{r.arm} s{r.seed} r{r.rep}: {', '.join(netem_bad(r))}" for r in rows if netem_bad(r)]
    if nem:
        out("ABORT-NETEM the live qdisc was not delay 20 ms with no jitter term on both legs before and after every row (a harness fault: nothing is scored): " + "; ".join(nem))
        out("LABEL62 NEED0 ABORT-NETEM")
        out(f"VERDICT62 ABORT-NETEM slots={len(cnt)}/{n_exp}")
        return 0
    c = Cell("c3j0", rows, plan62.FLOOR_W[0], karms, p1, ctl_w)
    cr = cr62(c, out)
    rnd = random.Random(SEED)
    res = scored(c, rnd, out)
    lab = res["label"]
    out(f"LABEL62 CONTROL-c3j0 {'CONTROL-MOVED' if lab == 'CONTROL-MOVED' else 'OK'}")
    if "d0" in res:
        out(f"LABEL62 GATE-J0 {'PASS' if res['d0'] < GATE_D0 else 'FAIL'} (D0 {res['d0']:.3f} ms; W{FLOOR2} D0 {res['d0b']:.3f})")
        out(f"LABEL62 NEED0-W12 {res['label_w12']}")
        out(f"LABEL62 NEED0-INT {res['label_int']}")
    out(f"LABEL62 NEED0 {lab}")
    reported(c, res, cr, out)
    nvoid = sum(1 for r in rows if not r.live and not r.dnf)
    out(f"VERDICT62 {lab.split(' ')[0]} slots={len(cnt)}/{n_exp} live={sum(1 for r in rows if r.live)} void={nvoid} replaced={len(replaced)} dnf={sum(1 for r in rows if r.dnf)} "
        f"rule_finding={cr['findings']} dump_rows={len(c.dump_rows)}")
    return 0


# ───────────────────────────── the rehearsal's go/no-go (section 3.3) ─────────────────────────────
NOGO_ORDER = ("NO-GO-FLOOR", "NO-GO-GATE", "NO-GO-PILOT", "NO-GO-CONTROL", "NO-GO-SHORT")


def probe_pairs(by):
    """-> [(p_lo, p_hi)] of the probe pairs the ledger holds a live row for: the base pair first, then a registered re-probe's."""
    out = []
    for pr in (plan62.PILOT62, plan62.REPROBE_LOW, plan62.REPROBE_HIGH):
        if all(any(r.live and not r.dnf for r in by.get(w, [])) for w in pr):
            out.append(pr)
    return out


def _one(by, w, kind="G"):
    v = [r for r in by.get(w, []) if r.kind == kind and r.live and not r.dnf]
    return v[-1] if v else None


def rehearsal(rows, out):
    """The go/no-go of 62r0 (section 3.3), the first readings and PILOT62. Rows by slot: R1 / R6 G512, R2 / R3 G12, R4 / R5 the probes (G80 / G128; a re-probe's rows when the ledger holds them), D1 D12.
    -> (decisions {rule: bool}, w_hat | None, failing rules in NOGO_ORDER)."""
    by = {}
    for r in rows:
        by.setdefault(r.W, []).append(r)
    dec = {k: True for k in NOGO_ORDER}
    # the floor pair R2 / R3
    fl = [r for r in by.get(12, []) if r.kind == "G"]
    flv = [r for r in fl if r.live]
    x_own, e50, e90, nref = s1.x_own_from([r for r in flv if not r.dnf])
    f_pair = med([s1.reading(r.w61(1), NF)[1] for r in flv])
    x_min = Q_R * f_pair if f_pair is not None else None
    x_use = max(v for v in (x_own, x_min) if v is not None) if (x_own is not None or x_min is not None) else None
    pair_gp = med([r.gp_() for r in flv])
    ok_floor = []
    for r in fl:
        s, fm, n, g = s1.reading(r.w61(1), NF)
        e = s1.e_own(r)
        sh = sum(v > x_use for v in e) / len(e) if e and x_use is not None else None
        coll = bool(sh is not None and sh > SHARE_MAX and r.gp_() < GP_CUT * (pair_gp or 0.0))
        crit = {"live": r.live and not r.dnf, "groups": g >= s1.MIN_FLOOR_GROUPS, "not-collapsed": not coll}
        out(f"  GO-NOGO-FLOOR {r.arm} s{r.seed} r{r.rep}: live/no DNF {crit['live']} | G(250) groups {g} >= {s1.MIN_FLOOR_GROUPS} {crit['groups']} | not COLLAPSED under CR62 {crit['not-collapsed']} (share>X_use {f(sh, 3)})")
        ok_floor.append(all(crit.values()))
    dec["NO-GO-FLOOR"] = len(fl) >= 2 and all(ok_floor)
    # the gate's pre-read
    pair = [r for r in flv if not r.dnf]
    d0 = None
    if pair:
        fn, f250, f2100 = (med([r.native_tmin() for r in pair]), med([s1.reading(r.w61(1), NF)[1] for r in pair]), med([s1.reading(r.w61(1), NK)[1] for r in pair]))
        if None in (fn, f250, f2100):
            out(f"FIRST-READING D0: not computable (F_native {f(fn, 3)}, F_G250 {f(f250, 3)}, F_G2100 {f(f2100, 3)}: no group at a count): the gate reads NO-GO-GATE")
        else:
            d0 = max(fn, f250) - f2100
            out(f"FIRST-READING D0 (the gate's pre-read on the floor pair) {d0:.3f} ms = max(F_native {fn:.3f}, F_G250 {f250:.3f}) - F_G2100 {f2100:.3f}; gate {'PASS' if d0 < GATE_D0 else 'FAIL'} (< {GATE_D0} ms)")
    dec["NO-GO-GATE"] = d0 is not None and d0 < GATE_D0
    # the control, the pilot
    ctl_rows = [r for r in by.get(512, []) if r.live and not r.dnf]
    ctl_gp = med([r.gp_() for r in ctl_rows])
    pairs = probe_pairs(by)
    w_hat, case = None, None
    pilot_gp = []
    if pairs and ctl_gp:
        p_lo, p_hi = pairs[-1]                                         # a registered re-probe replaces the base pair
        rl, rh = _one(by, p_lo), _one(by, p_hi)
        u_lo, u_hi = rl.gp_() / ctl_gp, rh.gp_() / ctl_gp
        pilot_gp = [rl.gp_(), rh.gp_()]
        det = plan62.pilot_detail(u_lo, u_hi, p_lo, p_hi)
        if det is not None:
            w_hat, case = det[0], det[1]
            out(f"PILOT62 W={w_hat:.1f} u_lo={u_lo:.3f} u_hi={u_hi:.3f} arms={plan62.arms(w_hat)} case={case} clamped={'yes' if det[2] else 'no'} probes={p_lo}/{p_hi} (control W512 median of {len(ctl_rows)} rows)")
        out(f"FIRST-READING u{p_lo} {u_lo:.3f} u{p_hi} {u_hi:.3f}")
    dec["NO-GO-PILOT"] = case == "proportional"
    if case != "proportional":
        out(f"NO-GO-PILOT detail: case={case}" if case else "NO-GO-PILOT detail: no pilot (a probe row or the control is missing / DNF)")
    # control health R1 / R6
    clause = {"gp": False, "loss": False, "pair-within-1pct": False}
    if len(ctl_rows) >= 2 and pilot_gp:
        gps = [r.gp_() for r in ctl_rows[:2]]
        clause["gp"] = min(gps) >= 0.98 * max(pilot_gp)
        clause["loss"] = all(r.x.get("loss") is not None and r.x["loss"] <= CTL_LOSS_MAX for r in ctl_rows[:2])
        clause["pair-within-1pct"] = abs(gps[0] - gps[1]) <= 0.01 * max(gps)
        out(f"  GO-NOGO-CONTROL R1 / R6 W512: (a) goodput {f(gps[0], 2)} / {f(gps[1], 2)} >= 0.98 x max(probes) = {f(0.98 * max(pilot_gp), 2)} {clause['gp']}; "
            f"(b) loss truth {f(ctl_rows[0].x.get('loss'), 4)} / {f(ctl_rows[1].x.get('loss'), 4)} <= {CTL_LOSS_MAX:.4f} {clause['loss']}; (c) R1 / R6 within 1 % {clause['pair-within-1pct']}")
    else:
        out("  GO-NOGO-CONTROL R1 / R6 W512: fewer than two live controls or no probe rows")
    dec["NO-GO-CONTROL"] = all(clause.values())
    # LATE length, first readings
    short = [f"{r.arm} s{r.seed} r{r.rep}: {r.late_n()}" for r in rows if r.live and r.late_n() < MIN_LATE_WINDOWS]
    dec["NO-GO-SHORT"] = bool(rows) and not short and all(r.live for r in rows)
    if short or not all(r.live for r in rows):
        out("NO-GO-SHORT detail: " + ("; ".join(short) or "a row is not live"))
    for r in [x for x in rows if x.kind == "G" and (x.W in (12, 512) or x.W in plan62.PILOT62 + plan62.REPROBE_LOW + plan62.REPROBE_HIGH)]:
        e = s1.e_own(r)
        den = (r.native_p90() or 0) - (r.native_tmin() or 0)
        out(f"FIRST-READING G{r.W} s{r.seed} r{r.rep}: E90_own {f(pq(e, 0.9) if e else None, 2)} ms, p90 - tmin {den:.2f} ms, LATE windows {r.late_n()}, native p50 - min {f(med([w['p50'] - w['tmin'] for w in r.w61(s1.NK_NATIVE) if w['p50'] is not None and w['tmin'] is not None]), 3)} ms,"
            f" p90 - p10 {f(med([w['p90'] - w['p10'] for w in r.w61(s1.NK_NATIVE) if w['p90'] is not None and w['p10'] is not None]), 3)} ms")
    out(f"FIRST-READING X_own on the pair {f(x_own, 2)} ms (E50 {f(e50, 2)}, E90 {f(e90, 2)}); X_min = q_R F_floor = {f(x_min, 2)} ms; X_min {'BINDS' if (x_min is not None and (x_own is None or x_min > x_own)) else 'does not bind'}")
    sp_ = [med([w["p90"] - w["p10"] for w in r.w61(s1.NK_NATIVE) if w["p90"] is not None and w["p10"] is not None]) for r in flv]
    ps_ = [med([w["p50"] - w["tmin"] for w in r.w61(s1.NK_NATIVE) if w["p50"] is not None and w["tmin"] is not None]) for r in flv]
    out(f"FIRST-READING floor window spread (the second witness that the jitter is off): J = 0 native p50 - min {f(med(ps_), 3)} ms, p90 - p10 {f(med(sp_), 3)} ms; beside J = 5 (61c) p50 - min {J5_S_F_NATIVE['61c']:.3f} ms, p90 - p10 {J5_SPREAD['61c']:.3f} ms "
        f"(61a: {J5_S_F_NATIVE['61a']:.3f} / {J5_SPREAD['61a']:.3f}); E90 9.34 ms at J = 5 (s62pred.out)")
    out("FIRST-READING reordering: not measured (no counter exists: tools/l1 has only lib.sh's comment about an earlier offline measurement; the binary's out_of_order is a config field; netem's stats carry none)")
    for r in rows:
        if r.kind == "D" and r.live:
            d = r.x2.get("dump") or {}
            out(f"FIRST-READING dump D{r.W}: coverage {f(d.get('coverage'), 3)} ({d.get('n')} of {d.get('diag_n')}), cap {'HIT' if d.get('cap') else 'no'}"
                + ("" if d.get("coverage") is not None and d["coverage"] >= DUMP_COVERAGE_MIN and not d.get("cap") else "  DUMP-READING-MISSING (not a NO-GO)"))
    failing = [k for k in NOGO_ORDER if not dec[k]]
    out(f"LABEL62 GO-c3j0 {'GO' if not failing else failing[0]}")
    if failing:
        out(f"NO-GO-LIST {','.join(failing)}")
    return dec, w_hat, failing


def reprobe_rows(rows):
    """Section 3.3's one re-probe on Row objects: -> (lo, hi) iff the base probes' pilot case is not proportional, a re-probe pair is registered for it (both below 0.95: 112 / 160; both at or above: 64 / 96) and the
    rows do not hold the re-probe yet; else None (a mixed pair, u_lo >= 0.95 > u_hi, has none: NO-GO-PILOT at once)."""
    by = {}
    for r in rows:
        by.setdefault(r.W, []).append(r)
    pairs = probe_pairs(by)
    ctl_gp = med([r.gp_() for r in by.get(512, []) if r.live and not r.dnf])
    if plan62.PILOT62 not in pairs or len(pairs) > 1 or not ctl_gp:
        return None
    u_lo, u_hi = (_one(by, w).gp_() / ctl_gp for w in plan62.PILOT62)
    det = plan62.pilot_detail(u_lo, u_hi)
    return None if det is None or det[1] == "proportional" else plan62.reprobe_pair(u_lo, u_hi)


def reprobe(path, out=print):
    """`s62_score.py reprobe <ledger>`: prints `LO HI` and returns 0 iff reprobe_rows asks for a re-probe, else returns 1."""
    import place38_parse as pp
    rows, _ = pp.rows_of([path])
    cnt, _ = counting_rows(rows)
    pair = reprobe_rows([Row(x, i) for i, x in enumerate(cnt) if plan62.valid_slot(x.get("cell"), x.get("arm"))])
    if pair is None:
        return 1
    out(f"{pair[0]} {pair[1]}")
    return 0


# ───────────────────────────── the smoke / rehearsal check ─────────────────────────────
def smoke(path, plan, out=print):
    """PASS / FAIL lines ending SMOKE-PASS | SMOKE-FAIL. 62r0 = the 7 rehearsal rows (+ a re-probe's two): the go/no-go and PILOT62 print as decisions and do not fail the smoke (a NO-GO ends s62: it is a Result);
    62a = the K2 smoke row. Every row carries the live-netem witness (ABORT-NETEM otherwise)."""
    import place38_parse as pp
    if plan not in ("62r0", "62a"):
        out(f"unknown plan {plan}")
        return 2
    slots = plan62.plan("62r0") if plan == "62r0" else plan62.plan("62a", smoke=True)
    rows, _ = pp.rows_of([path])
    got = {}
    for r in rows:
        got[(r.get("cell"), r.get("arm"), str(r.get("seed")), str(r.get("rep")))] = r
    if plan == "62r0":
        for pr in (plan62.REPROBE_LOW, plan62.REPROBE_HIGH):
            sl = plan62.reprobe_slots(pr)
            if all((c, a, str(sd), str(rp)) in got for c, _, a, sd, rp in sl):
                slots = slots + sl
    bad = []

    def crit(name, fails):
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)

    out(f"SMOKE62 plan={plan} rows={len(rows)} slots={len(slots)}")
    rvs, miss = [], []
    for (c, _, a, sd_, rp) in slots:
        r = got.get((c, a, str(sd_), str(rp)))
        if r is None:
            miss.append(f"{c} {a} s{sd_} r{rp}: no row")
        else:
            rvs.append(Row(r, len(rvs)))
    crit("one row per slot ran with a t60 and a t61", miss + [f"{rv.cell} {rv.arm}: no t60/t61 ({rv.r.get('t60_error')} {rv.r.get('t61_error')})" for rv in rvs if not rv.t or not rv.x])
    rvs = [rv for rv in rvs if rv.t and rv.x]
    crit("every row read delay 20 ms with no jitter term on both legs before and after the transfer (the live `tc qdisc show`; ABORT-NETEM otherwise)",
         [f"{rv.arm} s{rv.seed} r{rv.rep}: {'; '.join(netem_bad(rv))}" for rv in rvs if netem_bad(rv)])
    lib = s6._lib()
    crit("the store-law replay modules (s60replay, caplog_replay) import", [lib[2]] if lib[2] else [])
    for rv in rvs:
        T = rv.t
        out(f"  ROW {rv.cell} {rv.arm} s{rv.seed} r{rv.rep}: status={rv.r.get('status')} dnf={int(rv.dnf)} tags(cli)={T['tags']['cli']} witness={T['witness']} caplog_ok={T['caplog'].get('ok')} q={T['q']} "
            f"windows={len(rv.x.get('win', {}).get('t', []))} ticks={rv.x.get('n_ticks')} qshare={f(rv.x.get('qshare'), 3)} loss={f(rv.x.get('loss'), 4)}")
    crit("every row carries ALL gauge tags (WSTAT, SLAW, WSTORE, SLAWC, CAPLOG) on the client after the warm-up cut", [f"{rv.cell} {rv.arm}: no {k}" for rv in rvs for k, v in rv.t["tags"]["cli"].items() if not v])
    crit("RWM_SLAW echoed 1 on both ends", [f"{rv.cell} {rv.arm}: cli={rv.t['gate_slaw']['cli']} srv={rv.t['gate_slaw']['srv']}" for rv in rvs if rv.t["gate_slaw"]["cli"] != "1" or rv.t["gate_slaw"]["srv"] != "1"])
    crit("[SLAW] q = 0.100000 on every row (every s62 row is Bulk)", [f"{rv.cell} {rv.arm}: q={rv.t['q']}" for rv in rvs if rv.t["q"] != ["0.100000"]])
    crit("the per-row replay witness PASS (R~ with the row's own b0p and l reproduces printed R and Rb to 6e-6)", [f"{rv.cell} {rv.arm}: {rv.t['witness']}" for rv in rvs if not rv.dnf and rv.t["witness"].get("ok") is not True])
    crit("the CAPLOG replay PASS (records > 0 and every replayable cap reproduced)", [f"{rv.cell} {rv.arm}: {rv.t['caplog']}" for rv in rvs if rv.t["caplog"].get("ok") is not True])
    crit("t61 carries LATE windows with [WSTAT] p10 / p50 / p90", [f"{rv.cell} {rv.arm}: windows={len(rv.x.get('win', {}).get('t', []))}" for rv in rvs if not rv.dnf and not any(v is not None for v in rv.x.get("win", {}).get("p50", []))])
    if plan == "62r0":
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
    _, w, _ = rehearsal([Row(r, i) for i, r in enumerate(cnt) if plan62.valid_slot(r.get("cell"), r.get("arm"))], out)
    return 0 if w is not None else 1


DECISIONS = [
    "1. Reuse: the row parse, G(n*), e_own, qshare and the loss truth are s61_score's (Row / row_extras / reading); the prototype's ra / sp readers are the same quantities on the ledger rows (test_s62.py reproduces s62pred.out on the tgz).",
    "2. The 90 % interval is the prototype's: q(.05) / q(.95) = sorted draws [99] / [1899] of 2 000, seed 62, the draws in the prototype's order (floor arm, second arm, replicates), the intercept refitted per draw.",
    "3. Floor validity: a floor row counts iff live, not DNF, not COLLAPSED and it holds >= 6 G(250) groups. The control arm's absolute test (section 5 step 2) uses the arm medians of live, unflagged rows.",
    "4. Section 6's A0 and D use S_F at G(250), the count of F_0 and need_0; the native-S_F variant is printed beside it. 'The p10 / p50 / dump-IQR rises' are read as the K2 arm's median (p10 - tmin) and (p50 - tmin) over native windows less the W 12 floor's; the dump IQR is the late samples' (t >= 15 s on the dump's own clock) q75 - q25.",
    "5. The rehearsal: R1 / R6 are two W 512 rows (seeds 42 / 7), the pilot's control is the median of the two; control health (a) is min(R1, R6) >= 0.98 x max(probes), (b) both losses <= 4.76 % + 1 pp, (c) R1 / R6 within 1 % of the larger. The label line names the FIRST failing rule in the table's order (NO-GO-FLOOR, GATE, PILOT, CONTROL, SHORT); NO-GO-LIST names all.",
    "6. The re-probe (section 3.3) happens inside the rehearsal launch: after the 7 rows the battery asks `s62_score.py reprobe`; a mixed pair (u_lo >= 0.95 > u_hi, non-monotone) has no registered re-probe and reads NO-GO-PILOT at once. The scored pilot is the LAST probe pair the ledger holds.",
    "7. ABORT-NETEM is decided per row by the battery from the row's own [NETEM] lines (pre and post, both legs); a launch whose rows all pass has had the first row's pre and the last row's post read. The scorer refuses to score (ABORT-NETEM) any ledger with a row lacking them.",
    "8. A floor-arm DNF and an echo-VOID slot are re-run once (plan 61's rule at this cell); in the rehearsal so is a probe-row DNF.",

    "9. The dump's count test (section 6, reported only): dumped samples (path 0, [RTTDUMP] n=) against the sum of nk over the row's [SLAW] path-0 windows after the last warm-up marker (the dump and the sampler share record_rtt_sample: one stream), ratio printed, PASS iff 0.95 <= ratio <= 1.05 (the dump leaves at most 255 tail samples unwritten); coverage (dumped / the last [DIAG] sig n) and the cap are the registered missing-reading tests.",
    "10. ABORT-NETEM (section 3.3) reads a row's own [NETEM] lines: a WRONG read aborts always; a MISSING read aborts only on a row that completed (a summary line: its driver reached the post read) and the scorer counts it only on a live row, so a bring-up failure that never reached the qdisc is a VOID row, not a harness fault; the registration says only 'a harness fault'.",
    "11. J = 5's floor-window p90 - p10 (8.169 / 8.285 ms for 61c / 61a, which s62pred.out does not print) is computed from the two ledgers and pinned by test_s62's host block; the control arm's loss truth is read fail-closed (a missing [TRUTH] is CONTROL-MOVED).",
]
__doc__ += "\nDECISIONS (every ambiguity and the reading taken):\n" + "\n".join("  " + d for d in DECISIONS) + "\n"


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd == "smoke62" and len(a) == 2:
        return smoke(a[1], a[0])
    if cmd == "score62" and len(a) >= 2:
        return score(a[1:], a[0])
    if cmd == "reprobe" and len(a) == 1:
        return reprobe(a[0])
    if cmd == "pilot" and len(a) == 1:
        return pilot_line(a[0])
    print(f"usage: {sys.argv[0]} smoke62 <62r0|62a> <ledger> | score62 62a <ledger> [...] | reprobe <ledger> | pilot <ledger>")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
