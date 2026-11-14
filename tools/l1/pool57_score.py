#!/usr/bin/env python3
"""Status 57a / 57b: the emission-burst block at c1s (plan 57a) and the c1d pool sweep with the transfer test T (plan 57b), REPORTED ONLY, no build.

    pool57_score.py score57a|score57b <ledger> [...]     the report from a plan-57a / 57b ledger (the VERDICT57A / VERDICT57B line carries the readings)
    pool57_score.py smoke57a|smoke57b <ledger>           the rehearsal / smoke check: PASS / FAIL per criterion; rc 1 on any FAIL (57a: the realised burst of the B256-512 row must exceed 64)

The shared definitions are pool56_score's and are IMPORTED, not forked: the [DIAG] tick parse, the LATE window (t >= 5 s at c1s / c1d, sending ticks), the lap rate r_i = d(dgq_i tx)/dt
between the first and last LATE tick, the bind fraction (cap >= n_live x pool), the c1s quinn-collapse flag (qcwnd below the BDP in > 10 % of the windows, absolute), counting / slots /
medians. This module adds: the c1d cell to pool56's per-cell tables (additive; plan 56 never runs it), the 57 per-row readouts (`t57`), the 57a outcome table, the 57b transfer test T.

Registered definitions (status 57a / 57b, docs/derivations/store-law.md R6.4 / R6.5); where the text left a choice the INTERPRETATIONS block at the end of this docstring says which:
  * realised mean burst = d(eb_syms) / d(eb_bursts) between the first and last LATE tick, from the client [DIAG] `eb_bursts= eb_syms=` gauges (gates.rs echoes RWM_EMIT_BURST; emit_burst.rs
    prints the gauges whatever the gate).
  * release batch = d(cum src) / d(ack wakes) over LATE (`cum=<src>/..`, `wake[.. ack=<n>`), wakes per loop = ack wakes per s x the mean echo loop; busy % / ack-wait % = means of the
    `wait[.. ack=<n>% .. busy=<n>%` fields; a/W = the mean of a/b over LATE ticks (`win=a/b`).
  * Q = a - R x T0 (symbols) with a the mean counted store, R the mean of the per-tick-interval rates, T0 = 4.0 ms (57a, c1s); q = mean(top-level 0.1 ms `rtt=`) - 4.0 ms.
  * T transfer test: q_l = T-bar - l-hat, T-bar = the rate-weighted mean echo loop over LATE ticks from per-tick MEANS (one leg: the top-level 0.1 ms `rtt=`; two legs: sum r_i mean_t(rtt_i)
    / sum r_i over the legs' echo `rtt=`), l-hat = the rate-weighted lowest `rtp_us` of each leg in the row's first 2 s.

INTERPRETATIONS (the spec's text underdetermines these; each is pinned by a boundary test in test_pool57.py):
  1. SE = 1.253 sd / sqrt(n_u) with the SAMPLE sd (n - 1) of the arm's unflagged bound rows; SE_pred^2 = SE256^2 + (1-f)^2 SE512^2 + f^2 SE448^2 with f = dQ_b / 64 (R_pred is linear in the
     512 and 448 arms, f unclamped); the arm's b-bar / release batch / CPU are medians over the same rows.
  2. Outcome precedence: INERT (b256 <= 64), UNREAD (an arm below 8 unflagged rows, or no SE), RISING (z_null >= 2; it has no S condition and wins over UNDERPOWERED, whose note is kept),
     UNDERPOWERED, E1, E2, PARTIAL, INCONCLUSIVE. CPU-CONFOUNDED and BIASED annotate a reading, never replace it.
  3. The bind rule applies to all three 57a arms (all are reduced pools; the B64-512 control is the burst control, not the shipped pool); UNBOUND rows are reported and left out of the arm
     medians, as plan 56 does. 57b exempts the shipped-pool control.
  4. E5 reads the two W = 512 arms (B64-512, B256-512); B64-448 is reported against the same band (its W is 448) but does not fire E5.
  5. c1d collapse flag (57b): BOTH legs' below-BDP window shares > 0.10 (min over the legs), the form that reproduces the registered 5 of 30 (c1dflag.py prints FLAG on ANY leg, 16 of 30).
     The one-leg-only count (max > 0.10 >= min) is reported beside it.
  6. c1d T-bar: the top-level `rtt=` is the MAX over the live paths (diag.rs), not a rate-weighted loop, so at np = 2 T-bar uses the legs' integer-ms echo rtt, per-tick means,
     rate-weighted; the top-level form is reported as Ttop. The legs' integer print limits the resolution (the mean of ~400 ticks recovers part of it).
  7. T: (a) is read on the pools with mean a/W >= 0.80 and >= 3 unflagged bound rows; the control only supplies the denominator and the (c) readout. A scoreable point passes iff |d| < 0.08
     (fires at d <= -0.08 / >= +0.08). The two points: any fire is reported (LOW, HIGH, or MIXED when both directions); all pass = PASS; otherwise UNSCOREABLE. Offset axis (|d0(c1d) - d0(c1s)| > 0.5, d0(c1s) = 2.1 = the q_l of the smallest c1s reference point):
     c1d is interpolated at x_ref + (d0(c1d) - d0(c1s)); both axes must be scoreable; PASS on one and a fire on the other = OFFSET DIFFERS. The all-rows and unflagged-rows readings must agree,
     otherwise UNSCOREABLE.
  8. 57a with a replaced slot: the replacement (a second row of the same slot) counts, the VOID row does not (pool56 `counting`). Both plans replace a VOID pool / burst echo once.
"""
import math
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import pool54_score as p54  # noqa: E402
import pool56_score as ps  # noqa: E402

C1S, C1D = ps.C1S, "c1d-400"
# c1d joins pool56's per-cell tables (5 GB class: LATE from 5 s, two legs, N = 2). Additive: plan 56 never runs the cell.
ps.LATE_T.setdefault(C1D, 5.0)
ps.LEGS.setdefault(C1D, (0, 1))
ps.NLIVE.setdefault(C1D, 2)

T0_MS = 4.0                      # 57a: the c1s no-queue loop (section 56)
C1S_REF = ((2.1, 0.79), (2.7, 0.89), (4.7, 0.94), (6.8, 0.96), (15.7, 1.0))      # c1s q_l -> goodput / control (section 56's ledger; test_pool57 recomputes it from the tgz)
DELTA0_C1S = C1S_REF[0][0]   # c1s floor offset d0, estimated like c1d's: the q_l of the smallest reference point (status 57 Amendment 1: 2.1 ms)
A_POINTS = C1S_REF[:2]           # (a) falsifying: q_l = 2.1 and 2.7
B_POINTS = C1S_REF[2:4]          # (b) reported only
BAND = 0.08
OFFSET_TOL_MS = 0.5
OCC_MIN = 0.80
Q_SMALL_MS = 3.0                 # at least one c1d arm at q_l <= 3.0 ms
MIN_ROWS_A, MIN_ROWS_B = 8, 3
CTRL_Q_RANGE, CTRL_Q_LOW = (13.0, 21.0), 10.0
BIND_MIN = ps.BIND_MIN
CPU_TOL, BIAS_GAP = 0.05, 0.25
REBASE_MED, REBASE_RANGE, REBASE_FLOOR = 82700.0, 3000.0, 0.01
E5_BAND = (0.17, 0.24)
SE_K = 1.253
Z2, Z1, S_MIN = 2.0, 1.0, 2.0
ELL_WINDOW_S = 2.0
LABEL57A = {"B64_512": "B64-512", "B64_448": "B64-448", "B256_512": "B256-512"}
PERF_TIMEOUT_S = ps.PERF_TIMEOUT_S

_RTT_TOP = re.compile(r" np=\d+ np_act=\d+ rtt=([\d.]+)ms")
_CUM = re.compile(r" cum=(\d+)/")
_WACK = re.compile(r"wake\[.*? ack=(\d+)")
_WAIT = re.compile(r"wait\[.*? ack=(\d+)%.*?busy=(\d+)%")
_EB = re.compile(r"eb_bursts=(\d+) eb_syms=(\d+)")


def _pp():
    import place38_parse as pp
    return pp


def mean(v):
    v = [x for x in v if x is not None]
    return sum(v) / len(v) if v else None


def sdev(v):
    v = [x for x in v if x is not None]
    if len(v) < 2:
        return None
    m = sum(v) / len(v)
    return math.sqrt(sum((x - m) ** 2 for x in v) / (len(v) - 1))


def se_of(sd, n):
    return SE_K * sd / math.sqrt(n) if (sd is not None and n and n >= 1) else None


def fmt(x, nd=1):
    return "-" if x is None else f"{x:.{nd}f}"


# ── the per-row readouts ──
def tick_extras(cli):
    """The fields the 57 readouts add to ps.ticks56's ticks, ALIGNED with them (same line filter, same order): top-level rtt (ms), cum src, ack wakes, ack-wait %, busy %, eb gauges."""
    lines = list(cli or [])
    idx = [i for i, ln in enumerate(lines) if '"warmup":true' in ln]
    if idx:
        lines = lines[idx[-1] + 1:]
    out = []
    for ln in lines:
        if not p54._T.match(ln):
            continue
        top, cum, wk, wt, eb = _RTT_TOP.search(ln), _CUM.search(ln), _WACK.search(ln), _WAIT.search(ln), _EB.search(ln)
        out.append({"top": float(top.group(1)) if top else None, "cum": int(cum.group(1)) if cum else None, "wack": int(wk.group(1)) if wk else None,
                    "ackw": int(wt.group(1)) if wt else None, "busy": int(wt.group(2)) if wt else None, "eb": (int(eb.group(1)), int(eb.group(2))) if eb else None})
    return out


def lifetime_min_rtp(tk, legs):
    """{leg: the lowest rtp_us (ms) of the leg in the row's first ELL_WINDOW_S seconds} (the lifetime echo minimum l-hat_i)."""
    out = {}
    for i in legs:
        v = [d["rtp"][i] for d in tk if d["t"] <= ELL_WINDOW_S and d["rtp"].get(i)]
        if v:
            out[i] = min(v) / 1000.0
    return out


def row_metrics57(cli, srv, drv, nbytes=None, cell=C1S, pool=None):
    """-> (t56, t57): pool56_score.row_metrics (goodput, lap rates, bind fraction, collapse share, ...) and the 57 readouts. Never raises on a missing field."""
    t56 = ps.row_metrics(cli, srv, drv, nbytes, cell, pool)
    legs = ps.LEGS[cell]
    tk, ex = ps.ticks56(cli), tick_extras(cli)
    keep = [i for i, d in enumerate(tk) if d["t"] >= ps.LATE_T[cell] and (d["src"] or 0) > 0]
    L, X = [tk[i] for i in keep], [ex[i] for i in keep]
    t57 = {"n_late": len(L)}
    keys = ("burst", "eb_bursts_d", "eb_syms_d", "release", "wakes_s", "wakes_loop", "aW", "a_mean", "busy", "ackw", "R_tick", "Q", "q_top", "Ttop_ms", "Tbar57_ms", "ell_hat_ms", "q_ell", "q_ell_top",
            "Tleg_ms", "burst_pre", "r_i")
    t57.update({k: None for k in keys})
    if len(L) < 4:
        return t56, t57
    span = L[-1]["t"] - L[0]["t"]
    a, b = X[0], X[-1]
    if a["eb"] and b["eb"] and b["eb"][0] > a["eb"][0]:
        t57["eb_bursts_d"], t57["eb_syms_d"] = b["eb"][0] - a["eb"][0], b["eb"][1] - a["eb"][1]
        t57["burst"] = t57["eb_syms_d"] / t57["eb_bursts_d"]
    onset = t56.get("qc_onset_s")
    if onset is not None:       # a collapse row: the burst before the collapse starts (reported only; the registered burst is the LATE one above)
        pre = [x for d, x in zip(L, X) if d["t"] < onset and x["eb"]]
        if len(pre) >= 2 and pre[-1]["eb"][0] > pre[0]["eb"][0]:
            t57["burst_pre"] = (pre[-1]["eb"][1] - pre[0]["eb"][1]) / (pre[-1]["eb"][0] - pre[0]["eb"][0])
    if a["cum"] is not None and b["cum"] is not None and a["wack"] is not None and b["wack"] is not None and b["wack"] > a["wack"]:
        t57["release"] = (b["cum"] - a["cum"]) / (b["wack"] - a["wack"])
        t57["wakes_s"] = (b["wack"] - a["wack"]) / span if span > 0 else None
    caps = [d for d in L if d["win"] is not None and d["cap"]]
    t57["aW"] = mean([d["win"] / d["cap"] for d in caps])
    t57["a_mean"] = mean([d["win"] for d in L])
    t57["busy"], t57["ackw"] = mean([x["busy"] for x in X]), mean([x["ackw"] for x in X])
    rates = []
    for u, v in zip(L, L[1:]):
        dt = v["t"] - u["t"]
        if dt > 0 and all(i in u["dgq"] and i in v["dgq"] for i in legs):
            rates.append(sum(v["dgq"][i][1] - u["dgq"][i][1] for i in legs) / dt)
    t57["R_tick"] = mean(rates)
    t57["Ttop_ms"] = mean([x["top"] for x in X])
    w = ps.window_stat(L, legs, 4)
    if w:
        t57["r_i"] = {str(i): w["r"][i] for i in legs}
        floors = lifetime_min_rtp(tk, legs)
        if all(i in floors for i in legs) and w["R"] > 0:
            t57["ell_hat_ms"] = sum(w["r"][i] * floors[i] for i in legs) / w["R"]
        leg_means = {i: mean([d["rtt"].get(i) for d in L]) for i in legs}
        if w["R"] > 0 and all(v is not None for v in leg_means.values()):
            t57["Tleg_ms"] = sum(w["r"][i] * leg_means[i] for i in legs) / w["R"]       # the legs' integer-ms echo rtt, per-tick means, rate-weighted (the only form with two legs)
        t57["Tbar57_ms"] = t57["Ttop_ms"] if len(legs) == 1 else t57["Tleg_ms"]
    if t57["Tbar57_ms"] is not None and t57["ell_hat_ms"] is not None:
        t57["q_ell"] = t57["Tbar57_ms"] - t57["ell_hat_ms"]
    if t57["Ttop_ms"] is not None and t57["ell_hat_ms"] is not None:
        t57["q_ell_top"] = t57["Ttop_ms"] - t57["ell_hat_ms"]
    if t57["Tbar57_ms"] is not None and t57["wakes_s"] is not None:
        t57["wakes_loop"] = t57["wakes_s"] * t57["Tbar57_ms"] / 1000.0
    if cell == C1S:
        if t57["Ttop_ms"] is not None:
            t57["q_top"] = t57["Ttop_ms"] - T0_MS
        if t57["a_mean"] is not None and t57["R_tick"] is not None:
            t57["Q"] = t57["a_mean"] - t57["R_tick"] * T0_MS / 1000.0
    return t56, t57


def v57(r, k):
    return (r.get("t57") or {}).get(k)


# ── row classes ──
def flag_shares_b(row):
    """-> (both-leg flag?, one-leg-only flag?, (share0, share1)) of a c1d row from its `qcwnd_below` per-leg shares (windows with qcwnd below the leg's BDP)."""
    qb = row.get("qcwnd_below")
    per = qb.get("per_leg") if qb else None
    if not per or len(per) < 2:
        return False, False, None
    lo, hi = min(per[:2]), max(per[:2])
    return lo > ps.QC_SHARE, (hi > ps.QC_SHARE and not lo > ps.QC_SHARE), (per[0], per[1])


def is_flagged(row):
    """57a (c1s): pool56's quinn-collapse flag. 57b (c1d): BOTH legs below the BDP in more than 10 % of the windows."""
    if row["cell"] == C1D:
        return flag_shares_b(row)[0]
    return ps.qc_flagged(row)


def row_pool(plan, row):
    return _pp().arm_knobs57(plan, row["arm"])[0]


def is_unbound(plan, row):
    """A LIVE row whose LATE bind fraction is under BIND_MIN. 57a: every arm (all are reduced pools). 57b: the shipped-pool control is exempt."""
    if ps.is_void(row):
        return False
    pool = row_pool(plan, row)
    if plan == "57b" and pool == _pp().CONTROL57B:
        return False
    b = ps.tv(row, "bind_frac")
    return b is None or b < BIND_MIN


def pool_arm_rows(plan, rows, arm):
    """-> (live rows of the arm, flagged ones, unbound ones, the READ rows = unflagged and bound)."""
    live = [r for r in rows if r["arm"] == arm and not ps.is_void(r)]
    fl = [r for r in live if is_flagged(r)]
    ub = [r for r in live if is_unbound(plan, r)]
    rd = [r for r in live if not is_flagged(r) and not is_unbound(plan, r)]
    return live, fl, ub, rd


# ── 57a: the outcome table ──
def arm_stats_a(rows, arm):
    """One 57a arm's inputs to the outcome table from its counting rows. n is the unflagged bound rows' count."""
    live, fl, ub, rd = pool_arm_rows("57a", rows, arm)
    R = [ps.tv(r, "r0") for r in rd if ps.tv(r, "r0") is not None]
    sd = sdev(R)
    st = {"arm": arm, "live": len(live), "flagged": len(fl), "unbound": len(ub), "n": len(R), "R": lc.med(R) if R else None, "sd": sd, "se": se_of(sd, len(R)),
          "rate": (len(fl) / len(live)) if live else None}
    for key, fn in (("b", lambda r: v57(r, "burst")), ("release", lambda r: v57(r, "release")), ("wakes_loop", lambda r: v57(r, "wakes_loop")), ("aW", lambda r: v57(r, "aW")),
                    ("busy", lambda r: v57(r, "busy")), ("ackw", lambda r: v57(r, "ackw")), ("Q", lambda r: v57(r, "Q")), ("q", lambda r: v57(r, "q_top")),
                    ("cpu", lambda r: ps.tv(r, "cpu_cli_s_per_gb")), ("cpu_srv", lambda r: ps.tv(r, "cpu_srv_s_per_gb")), ("gp", lambda r: ps.gp(r)),
                    ("bind", lambda r: ps.tv(r, "bind_frac"))):
        st[key] = ps.med([fn(r) for r in rd])
    return st


def outcome57a(st):
    """The registered outcome table on the three arms' stats ({arm: arm_stats_a}); -> dict(label, notes, z_null, z_hyp, S, k, dQ, chord, R_pred, se_null, se_pred, ratio, cpu, bias)."""
    A, B, C = st["B64_512"], st["B64_448"], st["B256_512"]
    o = {"label": None, "notes": [], "z_null": None, "z_hyp": None, "S": None, "k": None, "dQ": None, "chord": None, "R_pred": None, "se_null": None, "se_pred": None, "ratio": None,
         "cpu": {}, "bias": None}
    low = [LABEL57A[a] for a in ("B64_512", "B64_448", "B256_512") if st[a]["n"] < MIN_ROWS_A]
    for a in ("B64_448", "B256_512"):
        c0, c1 = st["B64_512"]["cpu"], st[a]["cpu"]
        o["cpu"][a] = (abs(c1 / c0 - 1) > CPU_TOL) if (c0 and c1 is not None) else None
    ra, rc = A["rate"], C["rate"]
    o["bias"] = (abs(rc - ra) > BIAS_GAP) if (ra is not None and rc is not None) else None
    if low:
        o["label"], o["notes"] = "UNREAD", [f"fewer than {MIN_ROWS_A} unflagged bound rows in " + ", ".join(low)]
    elif None in (A["se"], B["se"], C["se"], A["b"], C["b"]) or min(A["se"], B["se"], C["se"]) <= 0:
        o["label"], o["notes"] = "UNREAD", ["an arm has no SE (fewer than 2 rows or no spread) or no realised burst"]
    elif C["b"] <= 64:
        o["label"], o["notes"] = "INERT", [f"the realised mean burst of B256-512 is {C['b']:.1f} <= 64: the knob does not act"]
    else:
        o["dQ"] = (C["b"] - A["b"]) / 2.0
        o["k"] = k = o["dQ"] / 64.0
        o["chord"] = A["R"] - B["R"]
        o["R_pred"] = A["R"] - k * o["chord"]
        o["se_null"] = math.hypot(C["se"], A["se"])
        o["se_pred"] = math.sqrt(C["se"] ** 2 + ((1 - k) * A["se"]) ** 2 + (k * B["se"]) ** 2)
        o["z_null"] = (C["R"] - A["R"]) / o["se_null"]
        o["z_hyp"] = (C["R"] - o["R_pred"]) / o["se_pred"]
        o["S"] = k * o["chord"] / o["se_null"]
        zn, zh, S = o["z_null"], o["z_hyp"], o["S"]
        if o["R_pred"] != A["R"]:
            o["ratio"] = (C["R"] - A["R"]) / (o["R_pred"] - A["R"])
        if zn >= Z2:
            o["label"] = "RISING"
            if S < S_MIN:
                o["notes"].append("S < 2: E1/E2 not read")
        elif S < S_MIN:
            o["label"] = "UNDERPOWERED"
        elif zn <= -Z2 and abs(zh) < Z2:
            o["label"] = "E1"
        elif abs(zn) < Z1 and zh >= Z2:
            o["label"] = "E2"
        elif zn <= -Z2 and abs(zh) >= Z2:
            o["label"] = "PARTIAL"
        else:
            o["label"] = "INCONCLUSIVE"
    return o


def full_label57a(o):
    out = o["label"]
    conf = [LABEL57A[a] for a, v in o["cpu"].items() if v]
    if conf:
        out += " CPU-CONFOUNDED(" + ",".join(conf) + ")"
    if o["bias"]:
        out += " BIASED"
    return out


def rebase57a(R512):
    """-> (re-based?, tol, drift) of the in-session B64-512 median lap rate against section 56's P512 (82.7 k +- max(3.0 k, 1 %))."""
    if R512 is None:
        return None, None, None
    tol = max(REBASE_RANGE, REBASE_FLOOR * REBASE_MED)
    return abs(R512 - REBASE_MED) > tol, tol, R512 - REBASE_MED


def e5_57a(st):
    """E5: release batch / W at W = 512 outside 0.17-0.24 in a 512 arm -> (fires?, {arm: ratio})."""
    r = {}
    for a in ("B64_512", "B256_512"):
        r[a] = (st[a]["release"] / 512.0) if st[a]["release"] is not None else None
    vals = [v for v in r.values() if v is not None]
    return (any(v < E5_BAND[0] or v > E5_BAND[1] for v in vals) if vals else None), r


# ── 57b: the transfer test T ──
def interp(points, x):
    """Linear interpolation of sorted [(q, ratio)] at x; None outside [min q, max q] (no extrapolation)."""
    pts = sorted(points)
    if not pts or x < pts[0][0] - 1e-12 or x > pts[-1][0] + 1e-12:
        return None
    for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
        if x0 - 1e-12 <= x <= x1 + 1e-12:
            return y0 if x1 == x0 else y0 + (y1 - y0) * (x - x0) / (x1 - x0)
    return pts[0][1]


def point_state(d):
    """A scoreable (a) point: PASS iff |d| < BAND; LOW at d <= -BAND, HIGH at d >= +BAND (compared to 9 places)."""
    d = round(d, 9)
    return "PASS" if abs(d) < BAND else ("LOW" if d < 0 else "HIGH")


def combine_points(states):
    """Two (a) points -> PASS | LOW | HIGH | MIXED | UNSCOREABLE."""
    sc = [s for s in states if s != "UNSCOREABLE"]
    if "LOW" in sc and "HIGH" in sc:
        return "MIXED"
    if "LOW" in sc:
        return "LOW"
    if "HIGH" in sc:
        return "HIGH"
    return "PASS" if (sc and len(sc) == len(states)) else "UNSCOREABLE"


def eval_a(points, shift=0.0):
    """(a) on c1d's arm medians [(q_l, goodput/control)]: at x_ref + shift for the two registered points. -> (state, [(x, value or None, state)])."""
    if not points or min(q for q, _ in points) > Q_SMALL_MS:
        return "UNSCOREABLE", [(x + shift, None, "UNSCOREABLE") for x, _ in A_POINTS]
    det, states = [], []
    for xr, ref in A_POINTS:
        x = xr + shift
        v = interp(points, x)
        s = "UNSCOREABLE" if v is None else point_state(v - ref)
        det.append((x, v, s))
        states.append(s)
    return combine_points(states), det


def read_a(points, delta0):
    """One reading's (a) on the raw axis and, when |d0(c1d) - d0(c1s)| > 0.5 ms, on the offset-corrected axis. -> dict(state, raw, corrected, shift, detail)."""
    raw, d_raw = eval_a(points, 0.0)
    out = {"raw": raw, "corrected": None, "shift": None, "detail": d_raw, "detail_corrected": None}
    if delta0 is None or not points:
        out["state"] = raw
        return out
    shift = delta0 - DELTA0_C1S
    out["shift"] = shift
    if abs(shift) <= OFFSET_TOL_MS:
        out["state"] = raw
        return out
    cor, d_cor = eval_a(points, shift)
    out["corrected"], out["detail_corrected"] = cor, d_cor
    if "UNSCOREABLE" in (raw, cor):
        out["state"] = "UNSCOREABLE"
    elif raw == cor:
        out["state"] = raw
    elif "PASS" in (raw, cor):
        out["state"] = "OFFSET DIFFERS"
    else:
        out["state"] = "MIXED"
    return out


def combine_readings(s_all, s_unf):
    """T from the all-rows and unflagged-rows readings: equal states stand, any disagreement is UNSCOREABLE."""
    return s_all if s_all == s_unf else "UNSCOREABLE"


def reading_b(rows_by_pool, unflagged_only):
    """T's inputs for one reading of the 57b rows ({pool: counting rows}). Control = the shipped pool. Returns dict with the control stats, per-pool stats, the read points and delta0.
    A pool is READ iff >= 3 unflagged bound rows (both readings), mean a/W >= 0.80 on the reading's rows."""
    pp = _pp()
    pools = {}
    for p in pp.POOLS57B:
        live, fl, ub, rd = pool_arm_rows("57b", rows_by_pool.get(p, []), "P%d" % p)
        use = rd if unflagged_only else [r for r in live if not is_unbound("57b", r)]
        gps = [ps.gp(r) for r in use]
        aw = ps.med([v57(r, "aW") for r in use])
        n_u = len(rd)
        both = sum(1 for r in live if flag_shares_b(r)[0])
        one = sum(1 for r in live if flag_shares_b(r)[1])
        pools[p] = {"pool": p, "W": 2 * p, "live": len(live), "flagged": len(fl), "both": both, "one_leg": one, "unbound": len(ub), "n_u": n_u, "n": len(use),
                    "gp": ps.med(gps), "q_ell": ps.med([v57(r, "q_ell") for r in use]), "ell": ps.med([v57(r, "ell_hat_ms") for r in use]),
                    "R": ps.med([ps.tv(r, "tx_sum_late") for r in use]), "aW": aw, "release": ps.med([v57(r, "release") for r in use]),
                    "wakes_loop": ps.med([v57(r, "wakes_loop") for r in use]), "cpu": ps.med([ps.tv(r, "cpu_cli_s_per_gb") for r in use]),
                    "q_ell_top": ps.med([v57(r, "q_ell_top") for r in use]),
                    "status": "READ" if (n_u >= MIN_ROWS_B) else "UNREAD"}
    ctl = pools[pp.CONTROL57B]
    ctl_ok = ctl["status"] == "READ" and ctl["gp"]
    pts, d0 = [], None
    for p in sorted(pp.POOLS57B):
        if p == pp.CONTROL57B:
            continue
        s = pools[p]
        if s["status"] == "READ" and (s["aW"] is None or s["aW"] < OCC_MIN):
            s["status"] = "NOT-STORE-BOUND"
        s["ratio"] = (s["gp"] / ctl["gp"]) if (ctl_ok and s["gp"] is not None) else None
        if s["status"] == "READ" and s["ratio"] is not None and s["q_ell"] is not None:
            pts.append((s["q_ell"], s["ratio"], p))
    smallest = min((p for q, r, p in pts), default=None)
    if smallest is not None:
        d0 = pools[smallest]["q_ell"]
    ctl["ratio"] = 1.0 if ctl_ok else None
    return {"pools": pools, "control": ctl, "control_ok": bool(ctl_ok), "points": [(q, r) for q, r, _ in sorted(pts)], "points_pools": sorted(p for _, _, p in pts), "delta0": d0,
            "smallest": smallest}


def score_b(rows_by_pool):
    """-> dict(all=reading, unflagged=reading, a_all, a_unf, T, ctrl_q, ...) for the 57b ledger rows grouped by pool."""
    ra, ru = reading_b(rows_by_pool, False), reading_b(rows_by_pool, True)
    out = {"all": ra, "unf": ru}
    for k, rd in (("a_all", ra), ("a_unf", ru)):
        out[k] = read_a(rd["points"], rd["delta0"]) if rd["control_ok"] else {"state": "UNSCOREABLE", "raw": "UNSCOREABLE", "corrected": None, "shift": None, "detail": [], "detail_corrected": None}
    out["T"] = combine_readings(out["a_all"]["state"], out["a_unf"]["state"])
    cq = ps.med([v57(r, "q_ell") for r in rows_by_pool.get(_pp().CONTROL57B, []) if not ps.is_void(r)])
    out["ctrl_q"] = cq
    out["ctrl_q_state"] = None if cq is None else ("LOW" if cq < CTRL_Q_LOW else ("IN" if CTRL_Q_RANGE[0] <= cq <= CTRL_Q_RANGE[1] else "OUT"))
    return out


def crossing(points, level):
    """The q_l where the interpolated goodput/control first reaches `level` (scanning up in q_l); None when not bracketed."""
    pts = sorted(points)
    for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
        if (y0 - level) * (y1 - level) <= 0 and y1 != y0:
            return x0 + (level - y0) * (x1 - x0) / (y1 - y0)
    return None


# ── the §56 c1s reference (calibration helper: the registered constants are C1S_REF) ──
def c1s_reference(rows56):
    """The c1s curve from §56 rows ({'arm', 'cell', 't56', 't57'}, unflagged only): [(arm, n, q_l median, goodput/control)] with control = P2048. Used by test_pool57 to reproduce C1S_REF."""
    by = {}
    for r in rows56:
        if not ps.qc_flagged(r):
            by.setdefault(r["arm"], []).append(r)
    ctl = ps.med([ps.gp(r) for r in by.get("P2048", [])])
    out = []
    for arm, rs in by.items():
        out.append((arm, len(rs), ps.med([v57(r, "q_ell") for r in rs]), (ps.med([ps.gp(r) for r in rs]) / ctl) if ctl else None))
    return sorted(out, key=lambda x: (x[2] is None, x[2]))


# ── smoke ──
def _need57(plan):
    k = ["aW", "Tbar57_ms", "ell_hat_ms", "q_ell", "release", "wakes_loop", "busy", "ackw"]
    return k + (["burst", "Q", "q_top"] if plan == "57a" else [])


def missing57(plan, r):
    miss = list(ps.missing(r))
    if r["cell"] == C1D:
        t = r.get("t56") or {}
        miss += [k for k in ps.REQUIRED_C8 if t.get(k) is None]
        if is_flagged(r) and r.get("dnf"):        # pool56's exemption of a collapsed row that hit the timeout (no summary) is c1s-only; the c1d flag is the both-leg one
            miss = [k for k in miss if k not in ps.COLLAPSE_EXEMPT and k != "H0"]
    t57 = r.get("t57") or {}
    return miss + [k for k in _need57(plan) if t57.get(k) is None]


def smoke(plan, path, out=print):
    pp = _pp()
    rows, _ = pp.rows_of([path])
    table = pp.plan57(plan, smoke=True)
    want = [(c, a) for c, _, a, _, _ in table]
    got = {(r["cell"], r["arm"]): r for r in rows}
    out(f"SMOKE{plan.upper()} rows={len(rows)}/{len(want)} (" + ("one B256-512 row: its realised mean burst must exceed 64" if plan == "57a" else "pools 192 and 2048 per path") + "; the values are not read, only the criteria below)")
    bad = []

    def crit(name, fails):
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)

    crit("one row per smoke slot ran", [f"{c} {a}: no row" for c, a in want if (c, a) not in got])
    rs = [got[k] for k in want if k in got]
    for r in rs:
        t, t7 = r.get("t56") or {}, r.get("t57") or {}
        out(f"  ROW {r['cell']} {r['arm']}: status={r['status']} mbps={r.get('mbps')} wall={r.get('wall_s')} gates pool={r.get('gate_pool_cli')}/{r.get('gate_pool_srv')} "
            f"pool_per_path={r.get('pool_info_cli')}/{r.get('pool_info_srv')} W={t.get('cap_mean')} bind={t.get('bind_frac')} late_n={t.get('late_n')} R={t.get('tx_sum_late')} "
            f"burst={t7.get('burst')} (d_bursts {t7.get('eb_bursts_d')}, d_syms {t7.get('eb_syms_d')}) release={t7.get('release')} aW={t7.get('aW')} q_ell={t7.get('q_ell')} ell={t7.get('ell_hat_ms')} qc={t.get('qc_share')}")
    crit("each row's pool AND burst are in [GATES] on both ends and pool_per_path matches on the sender (and the server when it prints one)",
         [f"{r['cell']} {r['arm']}: {' '.join(x for x in r['problems'] if x.startswith(('gate-pool', 'pool_per_path', 'gate-burst')))}" for r in rs if pp.is_echo_void57(r)])
    crit("the successor dump is off on both ends", [f"{r['cell']} {r['arm']}: {x}" for r in rs for x in r["problems"] if x.startswith("gate-succ-dump")])
    crit("every row ran the one seat's binary (LD_SHA)", [f"{r['cell']} {r['arm']}: {x}" for r in rs for x in r["problems"] if x.startswith("bin-sha")])
    crit("the LATE-window readouts (rates, rtt, rtp and its lifetime minimum, T-bar, q_l, burst gauges, release batch, ack wakes, a/W, busy, ack-wait, collapse share) computed on every row (no None)",
         [f"{r['cell']} {r['arm']}: {', '.join(missing57(plan, r))}" for r in rs if missing57(plan, r)])
    late = []
    for r in rs:
        if is_flagged(r):
            continue            # a collapse row may run into the timeout without failing the smoke; an UNFLAGGED DNF still does
        if r.get("dnf") or r.get("rc") != 0 or not r.get("summary"):
            late.append(f"{r['cell']} {r['arm']}: rc={r.get('rc')} dnf={r.get('dnf')} summary={r.get('summary')} wall={r.get('wall_s')}")
        elif (r.get("seconds") or 0) >= PERF_TIMEOUT_S:
            late.append(f"{r['cell']} {r['arm']}: {r.get('seconds')} s >= {PERF_TIMEOUT_S}")
        if r.get("bytes") != pp.BYTES57[r["cell"]]:
            late.append(f"{r['cell']} {r['arm']}: bytes={r.get('bytes')} (want {pp.BYTES57[r['cell']]})")
    crit(f"every smoke row finished inside RWM_PERF_TIMEOUT_S={PERF_TIMEOUT_S} (rc 0, a summary, not DNF) at 5 GB", late)
    if plan == "57a":
        r = got.get((pp.CELL57A, "B256_512"))
        b = v57(r, "burst") if r else None
        why = ""
        if r and is_flagged(r):
            why = (f" -- NOTE the row is collapse-flagged (below-BDP share {fmt(ps.qc_value(r), 3)}, onset {fmt(ps.tv(r, 'qc_onset_s'), 1)} s): a collapsed c1s row sends short bursts (section 56's collapsed P512 rows read 17.8 - 26.7 against 47.6 - 48.8), "
                   f"so this may be the collapse and not the knob; burst before the onset {fmt(v57(r, 'burst_pre'), 1)}. The registered rule still aborts; re-run the smoke")
        crit("the knob acts: the B256-512 row's realised mean burst (d eb_syms / d eb_bursts over LATE) exceeds 64",
             [] if (b is not None and b > 64) else [f"realised mean burst {b} <= 64 or unreadable (RWM_EMIT_BURST=256 does not act){why}"])
    for r in rs:
        out(f"  LIVENESS {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']}: {'LIVE' if r['status'] == 'LIVE' else 'VOID'} {' '.join(r['problems'])}")
    if bad:
        out("SMOKE-FAIL " + "; ".join(bad))
        return 1
    out("SMOKE-PASS")
    return 0


# ── the reports ──
def row_tags(plan, r):
    t = []
    if is_unbound(plan, r):
        t.append("UNBOUND")
    if is_flagged(r):
        t.append("COLLAPSE")
    if r.get("dnf"):
        t.append("DNF")
    return t


def _prologue(plan, paths, out):
    pp = _pp()
    rows_all, tokens = pp.rows_of(paths)
    aborted = sorted({t for t in tokens if t in pp.ABORT_TOKENS})
    cell = pp.CELL57A if plan == "57a" else pp.CELL57B
    arms = set(pp.ARM57A) if plan == "57a" else {"P%d" % p for p in pp.POOLS57B}
    rows_all = [r for r in rows_all if r["cell"] == cell and r["arm"] in arms]
    cnt, replaced = ps.counting(rows_all)
    voids = [r for r in cnt if ps.is_void(r)]
    live = [r for r in cnt if not ps.is_void(r)]
    out(f"SLOTS {len(ps.slots(rows_all))}/{pp.expect_rows57(plan)}  ROWS {len(rows_all)} (replaced {len(replaced)})  ABORT-TOKENS {' '.join(aborted) or 'none'}")
    for r in replaced:
        out(f"  REPLACED {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} {' '.join(r['problems'])}")
    for r in voids:
        out(f"  VOID {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} rc={r['rc']} {' '.join(r['problems'])}")
    for r in cnt:
        if r.get("t57_error"):
            out(f"  T57-ERROR {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']}: {r['t57_error']}")
    return rows_all, cnt, replaced, voids, live


def score_a(paths, out=print):
    pp = _pp()
    out("EMISSION-BURST BLOCK (status 57a), REPORTED ONLY: c1s-400 at 5 GB, one seat (current main); arms B64-512 (control) / B64-448 / B256-512 by RWM_STORE_PATH_POOL and RWM_EMIT_BURST; "
        f"RWM_DIAG=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S={PERF_TIMEOUT_S}; nothing here is a verdict")
    rows_all, cnt, replaced, voids, live = _prologue("57a", paths, out)
    st = {a: arm_stats_a([r for r in cnt if r["arm"] == a], a) for a in pp.ORDER57A}
    out("LIVENESS per arm (rows counting / live / void / DNF / UNBOUND / collapse-flagged / unflagged bound = n_u; bind = LATE share with cap >= pool, the realised mean burst b-bar):")
    for a in pp.ORDER57A:
        s = st[a]
        rs = [r for r in cnt if r["arm"] == a]
        out(f"  {LABEL57A[a]:<9} (pool {pp.ARM57A[a][0]}, burst {pp.ARM57A[a][1]}) rows {len(rs):>2}/{pp.N57A}  live {s['live']}  void {len(rs) - s['live']}  DNF {sum(1 for r in rs if not ps.is_void(r) and r.get('dnf'))}  "
            f"UNBOUND {s['unbound']}  COLLAPSE {s['flagged']}  n_u {s['n']}  bind {fmt(s['bind'], 3)}  b-bar {fmt(s['b'], 1)}")
    out("")
    out("ARM MEDIANS over the unflagged bound rows (R = the lap rate d(dgq0 tx)/dt over LATE; SE = 1.253 sd / sqrt(n_u)):")
    cols = [LABEL57A[a] for a in pp.ORDER57A]
    out("  " + f"{'readout':<44}" + "".join(f"{c:>12}" for c in cols))
    for label, key, nd in (("n_u (unflagged bound rows)", "n", 0), ("R lap rate /s (median)", "R", 0), ("R sd /s (sample)", "sd", 0), ("R SE /s", "se", 0), ("goodput Mbit/s (DNF = 0)", "gp", 1),
                           ("realised mean burst b-bar (d eb_syms / d eb_bursts)", "b", 1), ("release batch (symbols per ack wake)", "release", 1),
                           ("ack wakes per echo loop", "wakes_loop", 2), ("mean a/W", "aW", 3), ("sender busy %", "busy", 0), ("ack-wait %", "ackw", 0),
                           ("Q = a - R x 4.0 ms (symbols)", "Q", 0), ("q = mean(0.1 ms rtt) - 4.0 ms", "q", 2), ("client CPU s/GB", "cpu", 2), ("server CPU s/GB", "cpu_srv", 2),
                           ("collapse share of rows", "rate", 3)):
        out("  " + f"{label:<44}" + "".join(f"{fmt(st[a][key], nd):>12}" for a in pp.ORDER57A))
    out("  " + f"{'release batch / W':<44}" + "".join(f"{fmt((st[a]['release'] / pp.ARM57A[a][0]) if st[a]['release'] is not None else None, 3):>12}" for a in pp.ORDER57A))
    out("")
    o = outcome57a(st)
    out("OUTCOME (every reading uses the realised b-bar and the in-session chord):")
    out(f"  b-bar64 {fmt(st['B64_512']['b'], 1)}  b-bar256 {fmt(st['B256_512']['b'], 1)}  dQ_b = (b256 - b64)/2 = {fmt(o['dQ'], 1)} slots  k = dQ_b/64 = {fmt(o['k'], 3)}")
    out(f"  chord R64(512) - R64(448) = {fmt(o['chord'], 0)} /s  R_pred = R64(512) - k x chord = {fmt(o['R_pred'], 0)} /s  (observed R256 {fmt(st['B256_512']['R'], 0)})")
    out(f"  SE_null {fmt(o['se_null'], 0)}  SE_pred {fmt(o['se_pred'], 0)}  z_null {fmt(o['z_null'], 2)}  z_hyp {fmt(o['z_hyp'], 2)}  S {fmt(o['S'], 2)}  observed / predicted shift {fmt(o['ratio'], 2)}")
    out(f"  READING {full_label57a(o)}" + (" -- " + "; ".join(o["notes"]) if o["notes"] else ""))
    cpu = {LABEL57A[a]: fmt(st[a]["cpu"], 2) for a in ("B64_512", "B64_448", "B256_512")}
    out(f"  CPU GATE (each test arm within {CPU_TOL:.0%} of B64-512's client CPU/GB): {cpu}; confounded: " + (", ".join(LABEL57A[a] for a, v in o["cpu"].items() if v) or "none")
        + ("" if all(v is not None for v in o["cpu"].values()) else " (a CPU/GB was unreadable)"))
    out(f"  COLLAPSE GATE (|rate B256-512 - rate B64-512| <= {BIAS_GAP}): rates " + ", ".join(f"{LABEL57A[a]} {fmt(st[a]['rate'], 3)}" for a in pp.ORDER57A) + f"; BIASED = {o['bias']}")
    rb, tol, drift = rebase57a(st["B64_512"]["R"])
    out(f"  RE-BASING of B64-512 against section 56's P512 (same lap-rate definition): in-session median {fmt(st['B64_512']['R'], 0)} vs {REBASE_MED:.0f} +- {tol if tol else REBASE_RANGE:.0f}: "
        + ("UNAVAILABLE" if rb is None else (f"RE-BASED (drift {drift:+.0f} /s; reported, no reading changes)" if rb else f"within tolerance (drift {drift:+.0f} /s)")))
    e5, r5 = e5_57a(st)
    out(f"  E5 (release batch / W at W 512 outside {E5_BAND[0]}-{E5_BAND[1]} in a 512 arm): " + ("UNREADABLE" if e5 is None else ("FIRES" if e5 else "DOES-NOT-FIRE"))
        + " -- " + ", ".join(f"{LABEL57A[a]} {fmt(v, 3)}" for a, v in r5.items()) + f"; B64-448 (W 448) {fmt((st['B64_448']['release'] / 448.0) if st['B64_448']['release'] is not None else None, 3)} reported only")
    out("")
    out("PER-ROW TABLE (LIVE rows): arm seed rep | goodput | R lap /s | b-bar | release | a/W | q ms | collapse share | tags")
    for r in sorted(live, key=lambda x: (pp.ORDER57A.index(x["arm"]), x["seed"], x["rep"])):
        t, t7 = r.get("t56") or {}, r.get("t57") or {}
        out(f"  {LABEL57A[r['arm']]:<9} s{r['seed']:<3} r{r['rep']} | {'DNF' if r.get('dnf') else fmt(t.get('goodput_mbps'), 1)} | {fmt(t.get('r0'), 0)} | {fmt(t7.get('burst'), 1)} | {fmt(t7.get('release'), 1)} | "
            f"{fmt(t7.get('aW'), 3)} | {fmt(t7.get('q_top'), 2)} | {fmt(ps.qc_value(r), 3)} | {' '.join(row_tags('57a', r))}")
    out(f"VERDICT57A REPORTED-ONLY slots={len(ps.slots(rows_all))}/{pp.expect_rows57('57a')} live={len(live)} void={len(voids)} replaced={len(replaced)} dnf={sum(1 for r in live if r.get('dnf'))} "
        f"unbound={sum(1 for r in live if is_unbound('57a', r))} collapse={sum(1 for r in live if is_flagged(r))} outcome={o['label']} label={full_label57a(o).replace(' ', '_')} "
        f"z_null={fmt(o['z_null'], 2)} z_hyp={fmt(o['z_hyp'], 2)} S={fmt(o['S'], 2)} b256={fmt(st['B256_512']['b'], 1)} e5={'NA' if e5 is None else ('FIRES' if e5 else 'NO')}")


def score_b_report(paths, out=print):
    pp = _pp()
    out("C1D POOL SWEEP AND THE TRANSFER TEST T (status 57b), REPORTED ONLY: c1d-400 at 5 GB, one seat (current main); arm P<pool> = RWM_STORE_PATH_POOL per path (W = 2 x pool; the shipped 2048 sets nothing); "
        f"RWM_DIAG=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S={PERF_TIMEOUT_S}; nothing here is a verdict")
    rows_all, cnt, replaced, voids, live = _prologue("57b", paths, out)
    by_pool = {p: [r for r in cnt if r["arm"] == "P%d" % p] for p in pp.POOLS57B}
    S = score_b(by_pool)
    out("LIVENESS per pool (rows counting / live / void / DNF / UNBOUND / both-leg collapse / one-leg-only collapse / unflagged bound; the control is exempt from the bind rule):")
    for p in pp.POOLS57B:
        rs = by_pool[p]
        s = S["unf"]["pools"][p]
        out(f"  P{p:<5} (W {2 * p:>4}{', shipped' if p == pp.CONTROL57B else ''}) rows {len(rs):>2}/{pp.N57B}  live {s['live']}  void {len(rs) - s['live']}  DNF {sum(1 for r in rs if not ps.is_void(r) and r.get('dnf'))}  "
            f"UNBOUND {s['unbound']}  both-leg {s['both']} ({fmt(s['both'] / s['live'] if s['live'] else None, 2)})  one-leg-only {s['one_leg']} ({fmt(s['one_leg'] / s['live'] if s['live'] else None, 2)})  n_u {s['n_u']}  {s['status']}")
    out("")
    out("PER-POOL READOUTS (arm medians; 'unf' = unflagged bound rows, 'all' = all bound rows; goodput / control within the reading; a pool is READ iff >= 3 unflagged rows and mean a/W >= 0.80):")
    out("  " + f"{'readout':<38}" + "".join(f"{'P%d' % p:>10}" for p in pp.POOLS57B))
    for tag in ("unf", "all"):
        pools = S[tag]["pools"]
        for label, key, nd in ((f"goodput Mbit/s ({tag})", "gp", 1), (f"goodput / control ({tag})", "ratio", 3)):
            out("  " + f"{label:<38}" + "".join(f"{fmt(pools[p].get(key), nd):>10}" for p in pp.POOLS57B))
    pools = S["unf"]["pools"]
    for label, key, nd in (("q_l = T-bar - l-hat ms (unf)", "q_ell", 2), ("q_l from the top-level rtt ms (unf)", "q_ell_top", 2), ("l-hat ms (unf)", "ell", 2), ("R sum lap rate /s (unf)", "R", 0), ("mean a/W (unf)", "aW", 3),
                           ("release batch (unf)", "release", 1), ("ack wakes per loop (unf)", "wakes_loop", 2), ("client CPU s/GB (unf)", "cpu", 2)):
        out("  " + f"{label:<38}" + "".join(f"{fmt(pools[p].get(key), nd):>10}" for p in pp.POOLS57B))
    out("  " + f"{'status':<38}" + "".join(f"{pools[p]['status']:>10}"[-10:] for p in pp.POOLS57B))
    out("")
    ctl = S["unf"]["control"]
    out(f"(c) THE C1D CONTROL'S OWN QUEUE (reported before (a) is read): q_l = {fmt(S['ctrl_q'], 1)} ms at W 4096 (predicted {CTRL_Q_RANGE[0]:.0f}-{CTRL_Q_RANGE[1]:.0f} ms) -> "
        + ("UNREADABLE" if S["ctrl_q_state"] is None else ("CONTROL-QUEUE-LOW: below 10 ms the sweep's reference is a different operating point" if S["ctrl_q_state"] == "LOW" else S["ctrl_q_state"])))
    out(f"  control mean a/W {fmt(ctl['aW'], 3)}; control goodput median {fmt(ctl['gp'], 1)} Mbit/s (n_u {ctl['n_u']}); control {'readable' if S['unf']['control_ok'] else 'UNREAD (< 3 unflagged rows)'}")
    out("")
    out("OFFSETS d0 = q_l of the smallest reference point / store-bound pool: c1s " + f"{DELTA0_C1S:.1f} ms (registered)" + "; c1d (the smallest store-bound pool's q_l): "
        + ", ".join(f"{tag} {fmt(S[tag]['delta0'], 2)} ms (P{S[tag]['smallest']})" for tag in ("unf", "all")))
    for tag, key in (("unflagged rows", "a_unf"), ("all rows", "a_all")):
        a = S[key]
        rd = S["unf" if tag.startswith("unf") else "all"]
        out(f"(a) {tag}: c1d points (q_l, goodput/control) {[(round(q, 2), round(v, 3)) for q, v in rd['points']]} from pools {rd['points_pools']}")
        for x, v, s in a["detail"]:
            out(f"     raw axis: at q_l {x:.2f} ms c1d reads {fmt(v, 3)} vs c1s {dict(A_POINTS).get(round(x, 1))} +- {BAND} -> {s}")
        if a["detail_corrected"]:
            for x, v, s in a["detail_corrected"]:
                out(f"     offset-corrected axis (shift {a['shift']:+.2f} ms): at q_l {x:.2f} ms c1d reads {fmt(v, 3)} -> {s}")
        out(f"     (a) {tag}: raw {a['raw']}" + (f", corrected {a['corrected']}" if a["corrected"] else "") + f" => {a['state']}")
    for tag, key in (("unflagged rows", "unf"), ("all rows", "all")):
        pts = S[key]["points"]
        bs = [(x, interp(pts, x), ref) for x, ref in B_POINTS]
        out(f"(b) {tag} (reported only): " + "; ".join(f"q_l {x} ms: c1d {fmt(v, 3)} vs c1s {ref}" for x, v, ref in bs) + f"; c1d q_l at the 0.95 point {fmt(crossing(pts, 0.95), 2)} ms")
    out("")
    out(f"T = {S['T']} (a, all rows: {S['a_all']['state']}; a, unflagged rows: {S['a_unf']['state']}; the two readings must agree, otherwise UNSCOREABLE)")
    out("PER-ROW TABLE (LIVE rows): pool seed rep | goodput | R | q_l ms | a/W | share leg0 / leg1 below BDP | tags")
    for r in sorted(live, key=lambda x: (pp.POOLS57B.index(int(x["arm"][1:])), x["seed"], x["rep"])):
        t, t7 = r.get("t56") or {}, r.get("t57") or {}
        sh = flag_shares_b(r)[2]
        tags = row_tags("57b", r) + (["ONE-LEG"] if flag_shares_b(r)[1] else [])
        out(f"  {r['arm']:<6} s{r['seed']:<3} r{r['rep']} | {'DNF' if r.get('dnf') else fmt(t.get('goodput_mbps'), 1)} | {fmt(t.get('tx_sum_late'), 0)} | {fmt(t7.get('q_ell'), 2)} | {fmt(t7.get('aW'), 3)} | "
            f"{'-' if not sh else '%.3f / %.3f' % sh} | {' '.join(tags)}")
    out(f"VERDICT57B REPORTED-ONLY slots={len(ps.slots(rows_all))}/{pp.expect_rows57('57b')} live={len(live)} void={len(voids)} replaced={len(replaced)} dnf={sum(1 for r in live if r.get('dnf'))} "
        f"unbound={sum(1 for r in live if is_unbound('57b', r))} collapse={sum(1 for r in live if is_flagged(r))} T={S['T'].replace(' ', '_')} a_all={S['a_all']['state'].replace(' ', '_')} "
        f"a_unf={S['a_unf']['state'].replace(' ', '_')} control_q={fmt(S['ctrl_q'], 1)} d0={fmt(S['unf']['delta0'], 2)}")


def score(plan, paths, out=print):
    return score_a(paths, out) if plan == "57a" else score_b_report(paths, out)


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd in ("score57a", "score57b"):
        score(cmd[-3:], a)
        return 0
    if cmd in ("smoke57a", "smoke57b"):
        return smoke(cmd[-3:], a[0])
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
