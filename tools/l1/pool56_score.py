#!/usr/bin/env python3
"""Status 56: the store-pool sweep (plan 56), REPORTED ONLY, no build: row readouts (computed at row time into `t56`), the smoke check and the report.

    pool56_score.py score <ledger> [...] [--s54b <rep54b-diag-ld.tgz | dir>]   the sweep's report from a plan-56 ledger (the VERDICT56 line carries the falsifiers)
    pool56_score.py smoke <ledger>          the rehearsal / smoke check: PASS / FAIL per criterion; rc 1 on any FAIL
    pool56_score.py plan [smoke]            plan 56's table (`cell store arm seed rep` per line)

ONE seat: current main is both the pin and the candidate. Cells c8-100 (dual, N = 2 live paths), c2-100 and c1s-400 (single path, N = 1); arm `P<pool>` requests
RWM_STORE_PATH_POOL=<pool> (nothing at the shipped 2048), `P640D` is the c8 control pool plus RWM_SUCC_DUMP=1. The realised cap is max(N * pool, floor): W = N * pool at
c8, W = pool at c2 / c1s. Tables and rotation: place38_parse.plan56.

Definitions (status 56; `docs/derivations/store-law/little54b.py` and `pool54_score.py`, NOT forked silently -- test_pool56 reproduces little54b.out on section 54b's rows):
  * LATE window = client [DIAG] ticks after the last `"warmup":true` marker with t >= LATE_T[cell] (15 s; c1s 5 s) and `src=` > 0 (chosen by time, not by cap).
  * r_i = d(dgq_i tx)/dt between the first and last LATE tick; rtt_i / wrtt_i / q_i = (hand - tx) / rtp_us_i / qcwnd_i = LATE medians; boot rtp_us_i = the value on the first
    sending tick; P_i = median over LATE ticks of r_i * rtt_i / 1000 (Little's population of leg i); R = sum r_i (the Sigma send rate).
  * T-bar = 1000 * sum P_i / R (the rate-weighted LATE echo loop, little54b's Tbar_ms = sum r_i rtt_i / sum r_i); ell = sum r_i * boot rtp_us_i / R (the rate-weighted boot rtp, ms).
  * T0 (the no-queue loop) is NOT read from the first second (that carries start-up queue: 62-103 ms at c8 against ell ~ 15 ms). It is PER LEG: floor_i = the lowest arm-median LATE echo
    loop of leg i over the cell's arms (where goodput has broken the store holds no standing queue), and an arm's T0 = sum r_i floor_i / sum r_i with THAT arm's rates, so T0 <= T-bar at
    every arm and the leg mix never reads as a queue. The standing queue q = T-bar - T0 = sum r_i (rtt_i - floor_i) / sum r_i. At the knee (goodput 0.95 / 0.98 x control) q and T-bar are
    interpolated in W. T0_spread = max - min over the smallest-W arm's rows of the row T-bar (the noise of the floor); F10 is UNSCOREABLE when q at the 0.95 point is within it.
  * resid = median over LATE ticks of (cap - sum r_i * rtt_i / 1000) with cap = the tick's OWN `win=a/b` denominator; resid_rel = resid / median cap (the Little residual).
  * bind fraction = the share of LATE ticks with cap >= N * pool (the ceiling max(N * pool, floor) is the bind); the realised cap = the LATE-window mean of b.
  * old holes = median of the server [RANK] holes - tail_overcount over readouts[15:-6]; in-order delay late = the sending ticks at t >= LATE_T (inorder_delay(source_only=True)).
  * quinn-window collapse (c1s flag) = the share of [DIAG] ticks (t >= 1 s) with path 0's qcwnd below 1/4 of the row median > 0.1 (place43_parse.qcwnd_series; section 43b).
Row treatment: pool-echo mismatch (or the dump gate, or a foreign binary) = VOID, replaced once by the battery; control arms are exempt from the bind rule; a non-control row with bind
fraction < 0.95 is UNBOUND (kept, reported at its realised cap, plotted at that W, never replaced); a DNF row is LIVE with goodput 0 and every other readout from its [DIAG] lines.
Nothing here is a verdict: the falsifiers F1-F10 are evaluated and printed (FIRES / DOES-NOT-FIRE / UNSCOREABLE) and the sweep decides nothing by itself."""
import os
import re
import sys
import tarfile
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import pool54_score as p54  # noqa: E402
import snap53_score as s53  # noqa: E402
from threadp1_parse import inorder_delay  # noqa: E402

C8, C2, C1S = "c8-100", "c2-100", "c1s-400"
LATE_T = {C8: 15.0, C2: 15.0, C1S: 5.0}
LEGS = {C8: (0, 1), C2: (0,), C1S: (0,)}
NLIVE = {C8: 2, C2: 1, C1S: 1}                  # place38_parse.NLIVE56 (test_pool56 asserts the copy)
BIND_MIN = 0.95                                 # a non-control row binds iff its LATE bind fraction >= this
QC_FRAC, QC_SHARE = 0.25, 0.10                  # c1s collapse: qcwnd < QC_FRAC * row median on more than QC_SHARE of the windows
T0_END_S = 1.0
RANK_HEAD, RANK_TAIL = 15, 6
KNEE_POINTS = (0.95, 0.98)                      # the standing-queue readout's goodput fractions of the control
DUMP_MIN_RECORDS = 100000
PERF_TIMEOUT_S = 300
TOL_FLOOR = 0.01                                # re-basing tolerance floor, a share of the B640 median
F3_R0_REF = 9800.0
F10_RATIO = 3.0
F8_MIN_ROWS = 4                                 # unflagged rows needed in each n = 6 arm (P512, P2048) for F8 to be read
S54B_DEFAULT = os.path.join(HERE, "..", "..", "docs", "l1-raw", "s54", "rep54b-diag-ld.tgz")

_PP = (r" p%d:infl=\d+/sinfl=\d+/bdp\d+\S* sout=\d+ khr=\S+ btlbw=\d+.*?rtt=(\d+)/wrtt=(\d+)/rtp\d+ms rtp_us=(\d+).*?qcwnd=(\d+)")
_PP_RE = {i: re.compile(_PP % i) for i in (0, 1)}


def _pp():
    import place38_parse as pp
    return pp


def med(v):
    v = [x for x in v if x is not None]
    return lc.med(v) if v else None


def q_idx(v, p):
    v = sorted(v)
    return v[min(len(v) - 1, int(p * (len(v) - 1) + 0.5))] if v else None


# ── the [DIAG] ticks ──
def ticks56(cli):
    """[dict] of the client's [DIAG] ticks after the last warm-up marker: t, src, win (a), cap (b), dgq {leg: (hand, tx, full)}, rtt / wrtt (ms) / rtp (us) / qcw {leg}."""
    lines = list(cli or [])
    idx = [i for i, ln in enumerate(lines) if '"warmup":true' in ln]
    if idx:
        lines = lines[idx[-1] + 1:]
    out = []
    for ln in lines:
        mt = p54._T.match(ln)
        if not mt:
            continue
        ms, mw = p54._SRC.search(ln), p54._WIN.search(ln)
        mn = re.search(r" np=(\d+) np_act=", ln)
        d = {"np": int(mn.group(1)) if mn else None, "t": float(mt.group(1)), "src": int(ms.group(1)) if ms else None, "win": int(mw.group(1)) if mw else None, "cap": int(mw.group(2)) if mw else None,
             "dgq": {int(i): (int(h), int(x), int(f)) for i, h, x, f, _, _ in p54._DGQ.findall(ln)}, "rtt": {}, "wrtt": {}, "rtp": {}, "qcw": {}}
        for i, rx in _PP_RE.items():
            m = rx.search(ln)
            if m:
                d["rtt"][i], d["wrtt"][i], d["rtp"][i], d["qcw"][i] = (int(x) for x in m.groups())
        out.append(d)
    return out


def late_window(tk, cell):
    return [d for d in tk if d["t"] >= LATE_T[cell] and (d["src"] or 0) > 0]


def _usable(win, legs):
    return [d for d in win if d["cap"] is not None and all(i in d["dgq"] and i in d["rtt"] for i in legs)]


def window_stat(win, legs, min_ticks):
    """-> {n, r {leg}, P {leg}, R, Tbar_ms, resid, cap, ...} over a tick window (little54b's row()); None below `min_ticks` usable ticks or without a positive time span / rate."""
    use = _usable(win, legs)
    if len(use) < min_ticks:
        return None
    dt = use[-1]["t"] - use[0]["t"]
    if dt <= 0:
        return None
    r = {i: (use[-1]["dgq"][i][1] - use[0]["dgq"][i][1]) / dt for i in legs}
    R = sum(r.values())
    P = {i: med([r[i] * d["rtt"][i] / 1000 for d in use]) for i in legs}
    return {"n": len(use), "r": r, "R": R, "P": P, "cap": med([d["cap"] for d in use]), "cap_mean": sum(d["cap"] for d in use) / len(use),
            "Tbar_ms": 1000 * sum(P.values()) / R if R > 0 else None,
            "resid": med([d["cap"] - sum(r[i] * d["rtt"][i] / 1000 for i in legs) for d in use]),
            "rtt": {i: med([d["rtt"][i] for d in use]) for i in legs}, "wrtt": {i: med([d["wrtt"][i] for d in use]) for i in legs},
            "q": {i: med([d["dgq"][i][0] - d["dgq"][i][1] for d in use]) for i in legs},
            "rtp": {i: med([d["rtp"].get(i) for d in use]) for i in legs}, "qcw": {i: med([d["qcw"].get(i) for d in use]) for i in legs}}


def boot_rtp(tk, legs):
    """{leg: the rtp_us (ms) on the first sending tick that carries one}; the lifetime floor's start value."""
    out = {}
    for i in legs:
        for d in tk:
            if (d["src"] or 0) > 0 and d["rtp"].get(i):
                out[i] = d["rtp"][i] / 1000
                break
    return out


def old_holes(srv):
    """Median of the server [RANK] holes - tail_overcount over the readouts[15:-6] (pool54_score.old_holes)."""
    return p54.old_holes(srv)[0]


def inorder_late(cli, t_min):
    d = [x for t, x in inorder_delay(cli, source_only=True) if t >= t_min]
    if not d:
        return None
    return {"mean": sum(d) / len(d), "p50": q_idx(d, 0.5), "p95": q_idx(d, 0.95), "n": len(d)}


_HOLD_XP = re.compile(r"^\[HOLD\] site=sender path=(\d) .*? sp_n=(\d+) xp_n=(\d+) up_n=\d+ xp_frac=(\S+)")


def hold_xp(cli):
    """{leg: (sp_n, xp_n, xp_frac)} from the LAST sender [HOLD] line of each path: holes of that path closed by an arrival on the SAME path that exposed them (sp) / on a DIFFERENT path (xp,
    the cross-path skew class); xp_frac None when the line prints `-`."""
    out = {}
    for ln in cli or []:
        m = _HOLD_XP.match(ln)
        if m:
            out[int(m.group(1))] = (int(m.group(2)), int(m.group(3)), float(m.group(4)) if m.group(4) not in ("-", "") else None)
    return out


def succ_xp(srv):
    """(sp_n, xp_n, xp_frac) of the server's last final [SUCC] line (the receiver's view of the same split, all holes); None without one."""
    fin = [x for x in srv or [] if x.startswith("[SUCC]") and lc.is_final(x)]
    if not fin:
        return None
    sp, xp, fr = (lc.field(fin[-1], k) for k in ("sp_n", "xp_n", "xp_frac"))
    try:
        return int(sp), int(xp), (float(fr) if fr not in (None, "-") else None)
    except (TypeError, ValueError):
        return None


def qc_collapse(cli):
    """-> (share of windows with path 0's qcwnd < 1/4 of the row median, n windows); (None, 0) without a series (place43_parse.qcwnd_series: ticks with t >= 1 s)."""
    import place43_parse as p43
    qc = p43.qcwnd_series(cli)
    if not qc:
        return None, 0
    m = lc.med(qc)
    return sum(1 for x in qc if x < QC_FRAC * m) / len(qc), len(qc)


def row_metrics(cli, srv, drv, nbytes=None, cell=C8, pool=None):
    """One row's readouts: snap53_score.row_metrics (goodput / tail / H0 / duplicates / retx / CPU, [HOLD], [SNAPV], [SUCC] det) plus the sweep's fields. Never raises on a missing field.
    A single-path cell fills the leg-0 fields only (leg 1 reads None)."""
    legs = LEGS[cell]
    m = s53.row_metrics(cli, srv, drv, nbytes)
    tk = ticks56(cli)
    lt = late_window(tk, cell)
    w = window_stat(lt, legs, 4)
    m["cell"], m["pool"], m["late_n"] = cell, pool, len(lt)
    capvals = [d["cap"] for d in lt if d["cap"] is not None]
    m["cap_mean"] = sum(capvals) / len(capvals) if capvals else None
    m["cap_med"] = w["cap"] if w else med(capvals)
    # the ceiling is n_live * pool (store_cap.rs: max(n_live * pool, floor)) with the TICK's own live count (`np=`), the cell's N only when a tick prints none
    capn = [(d["cap"], d["np"] if d.get("np") else NLIVE[cell]) for d in lt if d["cap"] is not None]
    m["bind_frac"] = (sum(1 for c, n in capn if c >= n * pool) / len(capn)) if (capn and pool) else None
    npv = [d["np"] for d in lt if d.get("np")]
    m["nlive_med"] = med(npv)
    m["nlive_lt_share"] = (sum(1 for n in npv if n < NLIVE[cell]) / len(npv)) if npv else None      # share of LATE ticks with fewer live paths than the cell's N
    boot = boot_rtp(tk, legs)
    for i in (0, 1):
        for k in ("r", "rtt_late", "wrtt_late", "q_late", "rtp_late", "rtp_boot", "rtp_over", "qcw_late", "P"):
            m[f"{k}{i}"] = None
    if w:
        for i in legs:
            m[f"r{i}"], m[f"P{i}"] = w["r"][i], w["P"][i]
            m[f"rtt_late{i}"], m[f"wrtt_late{i}"], m[f"q_late{i}"], m[f"qcw_late{i}"] = w["rtt"][i], w["wrtt"][i], w["q"][i], w["qcw"][i]
            m[f"rtp_late{i}"] = w["rtp"][i] / 1000 if w["rtp"][i] is not None else None
            m[f"rtp_boot{i}"] = boot.get(i)
            m[f"rtp_over{i}"] = m[f"rtp_late{i}"] - boot[i] if (m[f"rtp_late{i}"] is not None and i in boot) else None
    m["tx_sum_late"] = w["R"] if w else None
    m["Tbar_ms"] = w["Tbar_ms"] if w else None
    m["TW_ms"] = 1000 * w["cap"] / w["R"] if (w and w["R"] > 0) else None
    m["resid_late"] = w["resid"] if w else None
    m["resid_rel"] = w["resid"] / w["cap"] if (w and w["cap"]) else None
    m["ell_ms"] = (sum(w["r"][i] * boot[i] for i in legs if i in boot) / w["R"]) if (w and w["R"] > 0 and all(i in boot for i in legs)) else None
    first = [d for d in tk if d["t"] <= T0_END_S and (d["src"] or 0) > 0]
    f0 = window_stat(first, legs, 2)
    m["T0_first1s_ms"] = (sum(f0["r"][i] * f0["rtt"][i] for i in legs) / f0["R"]) if (f0 and f0["R"] > 0) else None
    m["old_holes"], m["rank_readouts"] = old_holes(srv), len([ln for ln in srv or [] if ln.startswith("[RANK]")])
    il = inorder_late(cli, LATE_T[cell])
    m["inorder_mean_late"], m["inorder_p50_late"], m["inorder_p95_late"] = (il["mean"], il["p50"], il["p95"]) if il else (None, None, None)
    m["qc_share"], m["qc_n"] = qc_collapse(cli)
    hx = hold_xp(cli)
    for i in (0, 1):
        m[f"hold_sp{i}"], m[f"hold_xp{i}"], m[f"hold_xpf{i}"] = hx.get(i, (None, None, None))
    sx = succ_xp(srv)
    m["succ_sp"], m["succ_xp"], m["succ_xpf"] = sx if sx else (None, None, None)
    hold = m.get("hold") or {}
    for i in (0, 1):
        h = hold.get(i) or {}
        m[f"hn_p{i}"], m[f"fed_p{i}"], m[f"ripe_p{i}"] = h.get("hn_n"), h.get("fed"), h.get("ripe_frac")
    return m


# ── the registered per-row readouts (label, fn(row, t56), digits) ──
READOUTS = [
    ("goodput Mbit/s (DNF = 0)", lambda r, t: gp(r), 1),
    ("completion tail s", lambda r, t: t.get("tail_s"), 2),
    ("H0 holes at source end", lambda r, t: t.get("H0"), 0),
    ("sum tx rate R, LATE, /s", lambda r, t: t.get("tx_sum_late"), 0),
    ("tx rate r0 /s", lambda r, t: t.get("r0"), 0),
    ("tx rate r1 /s", lambda r, t: t.get("r1"), 0),
    ("app-echo rtt0 ms (LATE median)", lambda r, t: t.get("rtt_late0"), 0),
    ("app-echo rtt1 ms", lambda r, t: t.get("rtt_late1"), 0),
    ("wire rtt0 ms", lambda r, t: t.get("wrtt_late0"), 0),
    ("wire rtt1 ms", lambda r, t: t.get("wrtt_late1"), 0),
    ("rtp0 ms (LATE median)", lambda r, t: t.get("rtp_late0"), 1),
    ("rtp0 boot ms", lambda r, t: t.get("rtp_boot0"), 1),
    ("rtp0 - boot ms", lambda r, t: t.get("rtp_over0"), 1),
    ("rtp1 ms", lambda r, t: t.get("rtp_late1"), 1),
    ("rtp1 - boot ms", lambda r, t: t.get("rtp_over1"), 1),
    ("q0 = dgq0 hand - tx", lambda r, t: t.get("q_late0"), 0),
    ("q1 = dgq1 hand - tx", lambda r, t: t.get("q_late1"), 0),
    ("placement share p0 (egress)", lambda r, t: (r.get("shares") or [None])[0], 3),
    ("T-bar ms (sum r_i rtt_i / sum r_i)", lambda r, t: t.get("Tbar_ms"), 1),
    ("first-1s loop ms (start-up queue; NOT T0)", lambda r, t: t.get("T0_first1s_ms"), 1),
    ("T-bar / ell", lambda r, t: (t["Tbar_ms"] / t["ell_ms"]) if (t.get("Tbar_ms") is not None and t.get("ell_ms")) else None, 2),
    ("T0 ms (per-leg floors x this row's rates)", lambda r, t: t.get("T0b_ms"), 1),
    ("standing queue q = T-bar - T0 ms", lambda r, t: t.get("queue_ms"), 1),
    ("q / ell", lambda r, t: (t["queue_ms"] / t["ell_ms"]) if (t.get("queue_ms") is not None and t.get("ell_ms")) else None, 2),
    ("ell ms (rate-weighted boot rtp)", lambda r, t: t.get("ell_ms"), 1),
    ("W / sum r ms", lambda r, t: t.get("TW_ms"), 1),
    ("inorder_mean_late s", lambda r, t: t.get("inorder_mean_late"), 2),
    ("inorder_p95_late s", lambda r, t: t.get("inorder_p95_late"), 2),
    ("old holes (server [RANK])", lambda r, t: t.get("old_holes"), 0),
    ("[HOLD] hn_n p0", lambda r, t: t.get("hn_p0"), 0),
    ("[HOLD] fed p0", lambda r, t: t.get("fed_p0"), 0),
    ("[HOLD] ripe_frac p0", lambda r, t: t.get("ripe_p0"), 3),
    ("[HOLD] hn_n p1", lambda r, t: t.get("hn_p1"), 0),
    ("[HOLD] fed p1", lambda r, t: t.get("fed_p1"), 0),
    ("[HOLD] ripe_frac p1", lambda r, t: t.get("ripe_p1"), 3),
    ("[SUCC] det", lambda r, t: t.get("det"), 0),
    ("[SUCC] xp_frac (cross-path closes)", lambda r, t: t.get("succ_xpf"), 4),
    ("[HOLD] p0 xp_frac (cross-path closes)", lambda r, t: t.get("hold_xpf0"), 4),
    ("Little residual cap - sum r_i rtt_i", lambda r, t: t.get("resid_late"), 0),
    ("Little residual / W", lambda r, t: t.get("resid_rel"), 3),
    ("receiver duplicates", lambda r, t: t.get("rx_dup"), 0),
    ("retransmits per GB", lambda r, t: t.get("retx_per_gb"), 0),
    ("client CPU s/GB", lambda r, t: t.get("cpu_cli_s_per_gb"), 2),
    ("server CPU s/GB", lambda r, t: t.get("cpu_srv_s_per_gb"), 2),
    ("realised cap W (LATE mean of b)", lambda r, t: t.get("cap_mean"), 0),
    ("cap bind fraction (cap >= n_live x pool)", lambda r, t: t.get("bind_frac"), 3),
    ("n_live (LATE median of np=)", lambda r, t: t.get("nlive_med"), 1),
    ("share of LATE ticks with n_live < N", lambda r, t: t.get("nlive_lt_share"), 3),
    ("qcwnd collapse share", lambda r, t: t.get("qc_share"), 3),
]
# the readouts every smoke row must carry (no None); leg-1 fields are required at c8 only
REQUIRED = ["goodput_mbps", "tx_sum_late", "r0", "rtt_late0", "rtp_late0", "rtp_boot0", "q_late0", "Tbar_ms", "ell_ms", "resid_late", "resid_rel", "old_holes",
            "inorder_mean_late", "cap_mean", "bind_frac", "rx_dup", "retx_per_gb", "cpu_cli_s_per_gb", "qc_share"]
REQUIRED_C8 = ["r1", "rtt_late1", "rtp_late1"]


def gp(r):
    """Goodput of a row with a DNF read as 0."""
    if r.get("dnf"):
        return 0.0
    return (r.get("t56") or {}).get("goodput_mbps")


def tv(r, k):
    return (r.get("t56") or {}).get(k)


def missing(r):
    t = r.get("t56") or {}
    need = REQUIRED + (REQUIRED_C8 if r["cell"] == C8 else [])
    out = [k for k in need if t.get(k) is None]
    if t.get("H0") is None and t.get("tail_s") is None:
        out.append("H0")
    return out


# ── classification ──
def pool_of_arm(arm):
    return _pp().arm_pool56(arm)[0]


def is_control(row):
    pp = _pp()
    pool, dump = pp.arm_pool56(row["arm"])
    return pool == pp.CONTROL56[row["cell"]] and not dump          # the dump row is its own treatment, not a control replicate


def is_void(row):
    return row["status"] != "LIVE"


def is_unbound(row):
    """A non-control LIVE row whose LATE bind fraction is under BIND_MIN (reported at its realised cap, never replaced). The dump row (the c8 control pool) is exempt like the control."""
    pp = _pp()
    pool, dump = pp.arm_pool56(row["arm"])
    if is_void(row) or pool is None or pool == pp.CONTROL56[row["cell"]]:
        return False
    b = tv(row, "bind_frac")
    return b is None or b < BIND_MIN


def qc_flagged(row):
    """The c1s quinn-window collapse flag (reported in every table, excluded from F8)."""
    s = tv(row, "qc_share")
    return row["cell"] == C1S and s is not None and s > QC_SHARE


def slots(rows):
    """-> ({slot: [rows in ledger order]}); the counting row of a slot is its last, an earlier VOID one was replaced."""
    out = {}
    for r in rows:
        out.setdefault((r["cell"], r["arm"], str(r["seed"]), str(r["rep"])), []).append(r)
    return out


def counting(rows):
    """-> (counting rows, replaced rows): the last row of every slot / the earlier ones."""
    cnt, rep = [], []
    for rs in slots(rows).values():
        cnt.append(rs[-1])
        rep.extend(rs[:-1])
    return cnt, rep


def by_arm(rows):
    """{(cell, arm): [counting non-VOID rows]}."""
    out = {}
    for r in rows:
        if not is_void(r):
            out.setdefault((r["cell"], r["arm"]), []).append(r)
    return out


def arm_label(cell, arm):
    pp = _pp()
    pool, dump = pp.arm_pool56(arm)
    return f"{arm}(W{pool * pp.NLIVE56[cell]})"


def mval(rs, fn):
    return med([fn(r, r.get("t56") or {}) for r in rs])


# ── the standing-queue readout ──
def _legs_of(rd):
    """The legs a quantity dict carries: {0: x} at a single-path cell, {0: x, 1: y} at c8 (a leg whose value is None is absent)."""
    return {i: v for i, v in rd.items() if v is not None}


def loop_and_queue(r, rtt, floors):
    """T-bar = sum r_i rtt_i / sum r_i and the standing queue q = sum r_i (rtt_i - floor_i) / sum r_i, from per-leg rates r, LATE echo loops rtt and per-leg floors (ms); (None, None) without a rate.
    Because a floor is the per-leg MINIMUM over arms, q >= 0 at every arm by construction (a negative q is a VOID, never printed as a queue)."""
    legs = [i for i in r if r[i] is not None and rtt.get(i) is not None and floors.get(i) is not None]
    tot = sum(r[i] for i in legs)
    if not legs or tot <= 0:
        return None, None
    return sum(r[i] * rtt[i] for i in legs) / tot, sum(r[i] * (rtt[i] - floors[i]) for i in legs) / tot


def arm_quantities(rs):
    """Arm medians of the per-leg LATE rate and echo loop: ({leg: r}, {leg: rtt})."""
    r = {i: mval(rs, lambda x, t, i=i: t.get("r%d" % i)) for i in (0, 1)}
    rtt = {i: mval(rs, lambda x, t, i=i: t.get("rtt_late%d" % i)) for i in (0, 1)}
    return _legs_of(r), _legs_of(rtt)


def leg_floors(arms, cell):
    """T0, per leg: the lowest arm-median LATE echo loop of the leg over the cell's arms (where goodput has already broken the store holds no standing queue: the loop there is propagation +
    the leg's fixed service). No start-up ticks are read. -> {leg: ms}"""
    pp = _pp()
    fl = {}
    for pool in pp.POOLS56[cell]:
        rs = arms.get((cell, "P%d" % pool), [])
        if not rs:
            continue
        r, rtt = arm_quantities(rs)
        for i in rtt:
            if (r.get(i) or 0) > 0 and (i not in fl or rtt[i] < fl[i]):
                fl[i] = rtt[i]
    return fl


def row_queue(row, floors):
    """One row's standing queue q = sum r_i (rtt_i - floor_i) / sum r_i against the cell's per-leg floors (ms); None when undefined."""
    t = row.get("t56") or {}
    return loop_and_queue({i: t.get("r%d" % i) for i in (0, 1)}, {i: t.get("rtt_late%d" % i) for i in (0, 1)}, floors)[1]


def curve(arms, cell, floors=None):
    """The cell's (W, goodput, T-bar, q) points: each control / bound arm at its arm medians, each UNBOUND row on its own at its realised cap; sorted by W. T-bar and q are computed from
    the per-leg rates and echo loops (loop_and_queue). -> [(W, gp, Tbar, q)]"""
    pp = _pp()
    floors = floors if floors is not None else leg_floors(arms, cell)
    pts = []
    for pool in pp.POOLS56[cell]:
        rs = [r for r in arms.get((cell, "P%d" % pool), [])]
        bound = [r for r in rs if not is_unbound(r)]
        if bound:
            w = mval(bound, lambda r, t: t.get("cap_mean"))
            g = med([gp(r) for r in bound])
            r_, rtt_ = arm_quantities(bound)
            tb, q = loop_and_queue(r_, rtt_, floors)
            if None not in (w, g, tb, q):
                pts.append((w, g, tb, q))
        for r in rs:
            if is_unbound(r) and tv(r, "cap_mean") is not None and gp(r) is not None:
                t = r["t56"]
                tb, q = loop_and_queue({i: t.get("r%d" % i) for i in (0, 1)}, {i: t.get("rtt_late%d" % i) for i in (0, 1)}, floors)
                if tb is not None:
                    pts.append((tv(r, "cap_mean"), gp(r), tb, q))
    return sorted(pts)


def knee_point(pts, thr):
    """The W where goodput first reaches `thr` scanning upward in W, linearly interpolated from the point below it; -> (W, Tbar, q, note). A first point already at/above thr is reported as-is."""
    for i, (w, g, tb, q) in enumerate(pts):
        if g >= thr:
            if i == 0:
                return w, tb, q, "at the smallest W measured (the knee is at or below it)"
            w0, g0, tb0, q0 = pts[i - 1]
            f = (thr - g0) / (g - g0) if g > g0 else 1.0
            return w0 + f * (w - w0), tb0 + f * (tb - tb0), q0 + f * (q - q0), "interpolated"
    return None


def standing_queue(arms, cell):
    """-> {control_gp, floors, ell, T0_spread, T0_n, curve, points: {thr: (W, Tbar, q, q / ell, note, T0)}}. q = T-bar - T0 with T0 the per-leg floors weighted by the arm's OWN rates
    (so T0 <= T-bar at every arm, the mix of legs cannot masquerade as a queue); ell = median over the cell's rows. T0_spread = max - min over the smallest-W arm's rows of the row T-bar:
    the row-to-row noise of the statistic that defines the floors (0 with one row) -- F10 is UNSCOREABLE when q at the 0.95 point is not above it."""
    pp = _pp()
    ctrl = [r for r in arms.get((cell, "P%d" % pp.CONTROL56[cell]), [])]
    cg = med([gp(r) for r in ctrl])
    small = arms.get((cell, "P%d" % min(pp.POOLS56[cell])), [])
    ell = med([tv(r, "ell_ms") for rs in arms.values() for r in rs if r["cell"] == cell])
    floors = leg_floors(arms, cell)
    pts = curve(arms, cell, floors)
    tbv = [x for x in (tv(r, "Tbar_ms") for r in small) if x is not None]
    out = {"control_gp": cg, "floors": floors, "T0": None, "T0_spread": (max(tbv) - min(tbv)) if tbv else None, "T0_n": len(tbv), "ell": ell, "points": {}, "curve": pts}
    if cg is None or not pts:
        return out
    for thr in KNEE_POINTS:
        k = knee_point(pts, thr * cg)
        if k is None:
            out["points"][thr] = None
            continue
        w, tb, q, note = k
        out["points"][thr] = (w, tb, q, (q / ell) if ell else None, note, tb - q)
    return out


# ── the falsifiers ──
C8_W = {1280: 640, 768: 384, 512: 256, 320: 160, 224: 112, 160: 80}         # c8 W -> pool
FIRES, NOFIRE, UNSC = "FIRES", "DOES-NOT-FIRE", "UNSCOREABLE"


def test_rows(arms, cell, pool, drop_flagged=False):
    """The rows an F-test may read at (cell, pool): the control's all, a non-control arm's BOUND rows (an UNBOUND row measures its realised cap, not this W); -> (rows, n_unbound, n_flagged)."""
    rs = arms.get((cell, "P%d" % pool), [])
    ub = [r for r in rs if is_unbound(r)]
    rs = [r for r in rs if not is_unbound(r)]
    fl = [r for r in rs if qc_flagged(r)]
    if drop_flagged:
        rs = [r for r in rs if not qc_flagged(r)]
    return rs, len(ub), len(fl)


def falsifiers(arms, sq=None, dump_rows=()):
    """-> [(id, text, state, detail)]. `arms` = {(cell, arm): counting non-VOID rows}. Goodput reads a DNF as 0; an arm with no bound row at the tested W is UNSCOREABLE."""
    out = []

    def m(cell, pool, fn, **kw):
        rs, nu, nf = test_rows(arms, cell, pool, **kw)
        return mval(rs, fn), len(rs), nu, nf

    def fmt(x, nd=1):
        return "-" if x is None else f"{x:.{nd}f}"

    def rec(i, text, state, det):
        out.append((i, text, state, det))

    # F1
    a, na, ua, _ = m(C8, 160, lambda r, t: t.get("r1"))
    b, nb, ub, _ = m(C8, 640, lambda r, t: t.get("r1"))
    det = f"r1 at W 320 (P160, n {na}, unbound {ua}) {fmt(a, 0)} vs 0.8 x r1 at 1280 (P640, n {nb}) {fmt(None if b is None else 0.8 * b, 0)}"
    rec(1, "c8 r1 at W = 320 >= 0.8 x r1 at 1280: leg 1 is fed without a fast-leg queue", UNSC if None in (a, b) else (FIRES if a >= 0.8 * b else NOFIRE), det)
    # F2
    a, na, ua, _ = m(C8, 384, lambda r, t: t.get("tx_sum_late"))
    b, nb, ub, _ = m(C8, 640, lambda r, t: t.get("tx_sum_late"))
    det = f"sum rate at 768 (P384, n {na}, unbound {ua}) {fmt(a, 0)} vs 0.97 x control {fmt(None if b is None else 0.97 * b, 0)} (n {nb})"
    rec(2, "c8 sum rate at W = 768 < 0.97 x control: the knee is above 768", UNSC if None in (a, b) else (FIRES if a < 0.97 * b else NOFIRE), det)
    # F3
    a, na, ua, _ = m(C8, 112, lambda r, t: t.get("r0"))
    det = f"r0 at W 224 (P112, n {na}, unbound {ua}) {fmt(a, 0)} vs 0.97 x {F3_R0_REF:.0f} = {0.97 * F3_R0_REF:.0f}"
    rec(3, "c8 r0 < 0.97 x 9 800 at W = 224: the leg-0 shoulder is above prediction", UNSC if a is None else (FIRES if a < 0.97 * F3_R0_REF else NOFIRE), det)
    # F4
    read, over = [], []
    for pool in (160, 112, 80):
        a, na, ua, _ = m(C8, pool, lambda r, t: t.get("rtp_over0"))
        if a is not None:
            read.append(pool)
            if a > 10.0:
                over.append(f"P{pool} {a:.1f} ms")
    det = f"rtp0 - boot at W 320 / 224 / 160 read for pools {read or 'none'}: " + (", ".join(over) if over else "none above 10 ms")
    rec(4, "c8 rtp0 at W <= 320 > boot + 10 ms: E03's queue persists even at small W", FIRES if over else (NOFIRE if read else UNSC), det)
    # F5 / F6 read the arm medians at every measured W
    bad5, read5, bad6, read6 = [], 0, [], 0
    for (cell, arm), rs in sorted(arms.items()):
        pool, dump = _pp().arm_pool56(arm)
        if dump or pool is None:
            continue
        rs = [r for r in rs if not is_unbound(r)]
        if not rs:
            continue
        W = mval(rs, lambda r, t: t.get("cap_mean"))
        oh = mval(rs, lambda r, t: t.get("old_holes"))
        if W is not None and oh is not None and W <= 1280:
            read6 += 1
            if oh > 50:
                bad6.append(f"{cell} {arm} W {W:.0f} old holes {oh:.0f}")
        bound = [r for r in rs if (tv(r, "bind_frac") or 0) >= BIND_MIN]
        if W is not None and W >= 320 and bound:
            x = med([abs(tv(r, "resid_late") - (tv(r, "old_holes") or 0)) / tv(r, "cap_med") for r in bound
                     if tv(r, "resid_late") is not None and tv(r, "cap_med")])
            if x is not None:
                read5 += 1
                if x > 0.1:
                    bad5.append(f"{cell} {arm} W {W:.0f} |resid - old holes|/W {x:.3f}")
    rec(5, "|W - sum r_i rtt_i - old holes| > 0.1 W at any bound W >= 320: Little fails",
        FIRES if bad5 else (NOFIRE if read5 else UNSC), f"{read5} arm medians read (bound rows, W >= 320); " + ("; ".join(bad5) if bad5 else "none above 0.1 W"))
    rec(6, "old holes > 50 at any W <= 1 280: P_h is lower than the derivation says",
        FIRES if bad6 else (NOFIRE if read6 else UNSC), f"{read6} arm medians read (W <= 1280); " + ("; ".join(bad6) if bad6 else "none above 50"))
    # F7
    a, na, ua, _ = m(C2, 192, lambda r, t: gp(r))
    b, nb, ub, _ = m(C2, 2048, lambda r, t: gp(r))
    rec(7, "c2 goodput median at 192 < 0.95 x control: the c2 knee moved up in this era", UNSC if None in (a, b) else (FIRES if a < 0.95 * b else NOFIRE),
        f"P192 {fmt(a)} (n {na}, unbound {ua}) vs 0.95 x P2048 {fmt(None if b is None else 0.95 * b)} (n {nb})")
    # F8
    a, na, ua, fa = m(C1S, 512, lambda r, t: gp(r), drop_flagged=True)
    b, nb, ub, fb = m(C1S, 2048, lambda r, t: gp(r), drop_flagged=True)
    rec(8, "c1s goodput median at 512 >= 0.95 x control over the unflagged rows: the c1s cost is overstated",
        UNSC if (None in (a, b) or na < F8_MIN_ROWS or nb < F8_MIN_ROWS) else (FIRES if a >= 0.95 * b else NOFIRE),
        f"(needs >= {F8_MIN_ROWS} unflagged rows in each arm) P512 {fmt(a)} (unflagged n {na}, unbound {ua}, quinn-collapse excluded {fa}) vs 0.95 x P2048 {fmt(None if b is None else 0.95 * b)} (unflagged n {nb}, excluded {fb})")
    # F9: the dump's records are (outcome, us): no path, no seq
    dn = [(r.get("succdump") or {}).get("records") for r in dump_rows]
    ctl = arms.get((C8, "P%d" % _pp().CONTROL56[C8]), [])
    side = ("; beside it, the counters that DO carry the path split: [HOLD] p0 (holes closed by an arrival on the same path that exposed them / on a different one) "
            + ", ".join(f"dump row sp {tv(r, 'hold_sp0')} xp {tv(r, 'hold_xp0')} xp_frac {fmt(tv(r, 'hold_xpf0'), 4)} ([SUCC] all holes xp_frac {fmt(tv(r, 'succ_xpf'), 4)})" for r in dump_rows)
            + (f"; control P640 medians p0 xp_frac {fmt(mval(ctl, lambda r, t: t.get('hold_xpf0')), 4)}, [SUCC] xp_frac {fmt(mval(ctl, lambda r, t: t.get('succ_xpf')), 4)} (n {len(ctl)})" if ctl else ""))
    rec(9, "the SUCC_DUMP row shows p0 holes overtaken by later-placed p1 seqs: a real delay inversion",
        UNSC, "the dump holds (outcome, us) records only -- no path and no seq -- so 'p0 holes overtaken by p1 seqs' cannot be computed from it (a path-tagged dump needs an engine change "
        f"this block excludes). Dump rows read: {len(dump_rows)}, records {dn or '-'}" + (side if dump_rows or ctl else ""))
    # F10
    mult, miss, noisy = {}, [], []
    for cell in (C1S, C2, C8):
        s_ = (sq or {}).get(cell) or {}
        p95 = s_.get("points", {}).get(0.95)
        if p95 is None or p95[3] is None:
            miss.append(cell)
        else:
            mult[cell] = p95[3]
            sp_ = s_.get("T0_spread")
            if p95[2] is not None and sp_ is not None and abs(p95[2]) <= sp_:
                noisy.append(f"{cell} (T-bar - T0 = {p95[2]:.1f} ms within T0's spread {sp_:.1f} ms over {s_.get('T0_n')} smallest-W rows)")
    if noisy:
        rec(10, "the standing-queue multiple (T-bar - T0)/ell at the 0.95 point differs across c1s, c2, c8 by more than x3: the q*ell form is wrong",
            UNSC, "T-bar - T0 at the 0.95 point is not distinguishable from T0's own row-to-row spread (per-leg floors are only as good as this spread): " + "; ".join(noisy)
            + (f"; not computable for {miss}" if miss else "") + "; multiples read: " + ", ".join(f"{c} {v:.2f}" for c, v in mult.items()))
    elif miss:
        rec(10, "the standing-queue multiple (T-bar - T0)/ell at the 0.95 point differs across c1s, c2, c8 by more than x3: the q*ell form is wrong",
            UNSC, f"the 0.95 point is not computable for {miss}; read: " + ", ".join(f"{c} {v:.2f}" for c, v in mult.items()))
    else:
        lo, hi = min(mult.values()), max(mult.values())
        det = ", ".join(f"{c} {v:.2f}" for c, v in mult.items()) + (f"; max/min = {hi / lo:.2f}" if lo > 0 else "; a non-positive multiple: max/min undefined")
        rec(10, "the standing-queue multiple (T-bar - T0)/ell at the 0.95 point differs across c1s, c2, c8 by more than x3: the q*ell form is wrong",
            UNSC if lo <= 0 else (FIRES if hi / lo > F10_RATIO else NOFIRE), det)
    return out


# ── the re-basing against section 54b's B640 ──
REBASE_KEYS = (("sum rate R", "tx_sum_late"), ("r0", "r0"), ("r1", "r1"), ("T-bar ms", "Tbar_ms"), ("rtt0 ms", "rtt_late0"), ("rtt1 ms", "rtt_late1"),
               ("goodput", "goodput_mbps"), ("in-order late s", "inorder_mean_late"), ("old holes", "old_holes"))


def s54b_rows(src):
    """Section 54b's B640 rows (rep54b-diag-ld.tgz or an unpacked dir) -> [metrics] computed with THIS module's definitions; [] when the source is absent."""
    if not src or not os.path.exists(src):
        return []
    out = []

    def one(d, name):
        m = re.match(re.escape(C8) + r"@def-B640-s(\d+)-r(\d+)-a\d+-c\.log$", name)
        if not m:
            return
        base = os.path.join(d, name[:-len("-c.log")])
        out.append(row_metrics(lc.read(os.path.join(d, name)), lc.read(base + "-s.log"), lc.read(base + "-drv.out"), 600_000_000, C8, 640))
    if os.path.isdir(src):
        for n in sorted(os.listdir(src)):
            one(src, n)
        return out
    with tempfile.TemporaryDirectory() as td, tarfile.open(src) as tf:
        for mem in tf.getmembers():
            b = os.path.basename(mem.name)
            if mem.isfile() and b.startswith(C8 + "@def-B640-") and "smoke" not in os.path.dirname(mem.name):      # the archive also holds the smoke rows' diag dir (same file names)
                with open(os.path.join(td, b), "wb") as f:
                    f.write(tf.extractfile(mem).read())
        for n in sorted(os.listdir(td)):
            one(td, n)
    return out


def ref_stats(ms):
    """{key: (median, lo, hi, tol)} of the B640 reference rows; tol = the full range floored at TOL_FLOOR x |median|."""
    out = {}
    for _, k in REBASE_KEYS:
        v = [x[k] for x in ms if x.get(k) is not None]
        if v:
            md = lc.med(v)
            out[k] = (md, min(v), max(v), max(max(v) - min(v), TOL_FLOOR * abs(md)))
    return out


def rebase(ctrl_rows, ref, out=print):
    """The re-basing rule: a readout is re-based iff |control median - B640 median| > tol. -> {key: offset (control median - B640 median)} for the re-based readouts, or None when the
    reference or the control is unavailable."""
    if not ref:
        out("REBASE-CHECK UNAVAILABLE: no section-54b logs")
        return None
    if not ctrl_rows:
        out("REBASE-CHECK UNAVAILABLE: no c8 control (P640) rows")
        return None
    offs = {}
    for label, k in REBASE_KEYS:
        if k not in ref:
            continue
        rm, lo, hi, tol = ref[k]
        vals = [x for x in ((gp(r) if k == "goodput_mbps" else tv(r, k)) for r in ctrl_rows) if x is not None]
        cm = lc.med(vals) if vals else None
        if cm is None:
            out(f"  REBASE {label:<16} control median unavailable (B640 {rm:.4g} [{lo:.4g} - {hi:.4g}] tol {tol:.4g})")
            continue
        d = cm - rm
        flag = abs(d) > tol
        out(f"  REBASE {label:<16} control {cm:.4g} vs B640 {rm:.4g} [{lo:.4g} - {hi:.4g}]  tol {tol:.4g}  {'RE-BASED by %+.4g' % d if flag else 'within tol'}")
        if flag:
            offs[k] = d
    out("REBASE-NEEDED: " + (("YES for " + ", ".join(sorted(offs)) + ": the bands of those readouts at the other W shift by the same offset") if offs else "NO (every readout within tol)"))
    return offs


# c8 control-column bands from the B640 rows, and the pre-registered bands at the other W: W -> (lo, hi) | None; an open end is None
C8_BANDS = {
    "tx_sum_late": {1280: (11629, 11732), 768: (11400, 11650), 512: (11050, 11600), 320: (10300, 10900), 224: (9700, 10100), 160: (None, 9300)},
    "r0": {1280: (9776, 9840), 768: (9750, None), 512: (9700, None), 320: (9600, None), 224: (9400, 9800), 160: (None, 9300)},
    "r1": {1280: (1839, 1913), 768: (1650, 1900), 512: (1400, 1800), 320: (None, 1000), 224: (None, 400), 160: (None, 300)},
    "Tbar_ms": {1280: (103, 107), 768: (60, 70), 512: (42, 48), 320: (28, 32)},          # W = 224 / 160 are '~22' / '~17' (approximate, no band)
    "goodput_mbps": {1280: (106.5, 107.6)},
    "inorder_mean_late": {1280: (0.34, 0.39), 768: (0.18, 0.26), 512: (0.13, 0.18), 320: (0.08, 0.12), 224: (0.06, 0.08)},
    "old_holes": {W: (0, 0) for W in C8_W},
    "rtt_late1": {1280: (138, 161)},
}
BAND_LABEL = {"tx_sum_late": "sum rate R /s", "r0": "r0 /s", "r1": "r1 /s", "Tbar_ms": "T-bar ms", "goodput_mbps": "goodput Mbit/s", "inorder_mean_late": "in-order late mean s",
              "old_holes": "old holes", "rtt_late1": "rtt1 ms"}


def band_report(arms, offs, out=print):
    """Each pre-registered c8 band against the arm's median (shifted by the re-basing offset of that readout, control column unshifted); reported, decides nothing."""
    out("C8 BANDS (pre-registered; the control column is B640's own range, the other columns shift by the control's re-basing offset where the readout re-based):")
    for k, byw in C8_BANDS.items():
        cells = []
        for W, band in sorted(byw.items(), reverse=True):
            rs, nu, _ = test_rows(arms, C8, C8_W[W])
            fn = (lambda r, t: gp(r)) if k == "goodput_mbps" else (lambda r, t, k=k: t.get(k))
            v = mval(rs, fn)
            if band is None or v is None:
                cells.append(f"W{W}: {'-' if v is None else format(v, '.4g')} (no band)")
                continue
            lo, hi = band
            sh = (offs or {}).get(k, 0.0) if W != 1280 else 0.0
            lo, hi = (None if lo is None else lo + sh), (None if hi is None else hi + sh)
            ok = (lo is None or v >= lo) and (hi is None or v <= hi)
            cells.append(f"W{W}: {v:.4g} {'IN' if ok else 'OUT'} [{'-' if lo is None else format(lo, '.4g')} .. {'-' if hi is None else format(hi, '.4g')}]" + (f" shifted {sh:+.4g}" if sh else ""))
        out(f"  {BAND_LABEL[k]:<22} " + " | ".join(cells))


def other_predictions(arms, out=print):
    out("C2 PREDICTIONS (goodput / control): >= 0.97 at W >= 192; 0.88-0.95 at 160; 0.72-0.82 at 128; T-bar ~ W / 9.6k")
    cg = mval(test_rows(arms, C2, 2048)[0], lambda r, t: gp(r))
    for pool, band in ((384, (0.97, None)), (256, (0.97, None)), (192, (0.97, None)), (160, (0.88, 0.95)), (128, (0.72, 0.82))):
        rs, nu, _ = test_rows(arms, C2, pool)
        v = mval(rs, lambda r, t: gp(r))
        if v is None or not cg:
            out(f"  W{pool}: unavailable (bound n {len(rs)}, unbound {nu})")
            continue
        x = v / cg
        ok = (x >= band[0]) and (band[1] is None or x <= band[1])
        tb = mval(rs, lambda r, t: t.get("Tbar_ms"))
        out(f"  W{pool}: goodput/control {x:.3f} {'IN' if ok else 'OUT'} [{band[0]} .. {'-' if band[1] is None else band[1]}]; T-bar {'-' if tb is None else format(tb, '.1f')} ms vs W/9.6k = {pool / 9.6:.1f}")
    out("C1S PREDICTIONS (goodput / control, +- 0.05, flagged rows excluded): 0.94 (1024), 0.92 (768), 0.84 (512), 0.80 (384)")
    cg = mval(test_rows(arms, C1S, 2048, drop_flagged=True)[0], lambda r, t: gp(r))
    for pool, ref in ((1024, 0.94), (768, 0.92), (512, 0.84), (384, 0.80)):
        rs, nu, nf = test_rows(arms, C1S, pool, drop_flagged=True)
        v = mval(rs, lambda r, t: gp(r))
        if v is None or not cg:
            out(f"  W{pool}: unavailable (bound unflagged n {len(rs)}, unbound {nu})")
            continue
        x = v / cg
        out(f"  W{pool}: goodput/control {x:.3f} {'IN' if abs(x - ref) <= 0.05 else 'OUT'} [{ref - 0.05:.2f} .. {ref + 0.05:.2f}] (n {len(rs)}, collapse-excluded {nf})")
    out("ANCHOR (rtp_us - boot): c8 p0 <= 10 ms at W <= 320; c8 p1 <= 8 ms at every W")
    for pool in sorted(_pp().POOLS56[C8]):
        rs, _, _ = test_rows(arms, C8, pool)
        a, b = mval(rs, lambda r, t: t.get("rtp_over0")), mval(rs, lambda r, t: t.get("rtp_over1"))
        W = 2 * pool
        ok0 = None if a is None else (a <= 10.0 if W <= 320 else None)
        ok1 = None if b is None else b <= 8.0
        out(f"  W{W}: rtp0-boot {'-' if a is None else format(a, '.1f')} ms {'' if ok0 is None else ('IN' if ok0 else 'OUT')}; rtp1-boot {'-' if b is None else format(b, '.1f')} ms {'' if ok1 is None else ('IN' if ok1 else 'OUT')}")


# ── smoke ──
def smoke(path, out=print):
    pp = _pp()
    rows, _ = pp.rows_of([path])
    want = [(c, a) for c, _, a, _, _ in pp.SMOKE56]
    got = {(r["cell"], r["arm"]): r for r in rows}
    out(f"SMOKE56 rows={len(rows)}/{len(want)} (one row per cell at its smallest pool + the SUCC_DUMP row; the values are not read, only the criteria below)")
    bad = []

    def crit(name, fails):
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)

    crit("one row per smoke slot ran", [f"{c} {a}: no row" for c, a in want if (c, a) not in got])
    rs = [got[k] for k in want if k in got]
    for r in rs:
        t = r.get("t56") or {}
        out(f"  ROW {r['cell']} {r['arm']}: status={r['status']} mbps={r.get('mbps')} wall={r.get('wall_s')} gates={r.get('gate_pool_cli')}/{r.get('gate_pool_srv')} "
            f"pool_per_path={r.get('pool_info_cli')}/{r.get('pool_info_srv')} W={t.get('cap_mean')} bind={t.get('bind_frac')} late_n={t.get('late_n')} "
            f"R={t.get('tx_sum_late')} Tbar={t.get('Tbar_ms')} T0first1s={t.get('T0_first1s_ms')} old_holes={t.get('old_holes')} qc={t.get('qc_share')} succdump={(r.get('succdump') or {}).get('records')}")
    crit("each row's pool is in [GATES] on both ends and pool_per_path matches on the sender (and the server when it prints one)",
         [f"{r['cell']} {r['arm']}: {' '.join(x for x in r['problems'] if x.startswith(('gate-pool', 'pool_per_path')))}" for r in rs if pp.is_pool_void56(r)])
    crit("the dump gate reads RWM_SUCC_DUMP=1 on both ends of the dump row and 0 / absent on every other row", [f"{r['cell']} {r['arm']}: {x}" for r in rs for x in r["problems"] if x.startswith("gate-succ-dump")])
    crit("every row ran the one seat's binary (LD_SHA)", [f"{r['cell']} {r['arm']}: {x}" for r in rs for x in r["problems"] if x.startswith("bin-sha")])
    crit("the LATE-window readouts (rates, rtt, rtp and its boot, T-bar, ell, Little residual, old holes, in-order late, bind, collapse share) computed on every row (no None)",
         [f"{r['cell']} {r['arm']}: {', '.join(missing(r))}" for r in rs if missing(r)])
    late = []
    for r in rs:
        if r.get("dnf") or r.get("rc") != 0 or not r.get("summary"):
            late.append(f"{r['cell']} {r['arm']}: rc={r.get('rc')} dnf={r.get('dnf')} summary={r.get('summary')} wall={r.get('wall_s')}")
        elif (r.get("seconds") or 0) >= PERF_TIMEOUT_S:
            late.append(f"{r['cell']} {r['arm']}: {r.get('seconds')} s >= {PERF_TIMEOUT_S}")
        if r.get("bytes") != pp.BYTES56[r["cell"]]:
            late.append(f"{r['cell']} {r['arm']}: bytes={r.get('bytes')} (want {pp.BYTES56[r['cell']]})")
    crit(f"every smoke row finished inside RWM_PERF_TIMEOUT_S={PERF_TIMEOUT_S} (rc 0, a summary, not DNF) at its cell's size", late)
    d = got.get((pp.DUMP_CELL56, pp.DUMP_ARM56))
    sd = (d or {}).get("succdump") or {}
    crit("the dump row wrote a raw dump (at least one [SUCCDUMP] record; the registered >= 100 000 is the scored row's rule, re-run once)",
         [] if (sd.get("records") or 0) > 0 else [f"no [SUCCDUMP] record in the dump row (records {sd.get('records')})"])
    out(f"  SUCCDUMP smoke row: records {sd.get('records')} (cli {(sd.get('cli') or {}).get('records')}, srv {(sd.get('srv') or {}).get('records')}), {sd.get('bytes')} bytes in the log lines, "
        f"cap-announced {sd.get('capped')}; the registered rule on the scored row: >= {pp.DUMP_MIN_RECORDS56}")
    for r in rs:
        out(f"  LIVENESS {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']}: {'LIVE' if r['status'] == 'LIVE' else 'VOID'} {' '.join(r['problems'])}")
    if bad:
        out("SMOKE-FAIL " + "; ".join(bad))
        return 1
    out("SMOKE-PASS")
    return 0


# ── the report ──
def _fmt(x, nd):
    return "-" if x is None else f"{x:.{nd}f}"


def row_tags(r):
    pp = _pp()
    t = []
    if is_unbound(r):
        t.append("UNBOUND")
    if qc_flagged(r):
        t.append("QCOLLAPSE")
    if r["cell"] == C8 and r.get("locked"):
        t.append("LOCKOUT")                 # section 3.9's classifier (a leg with egress share < 10 % AND median rtt >= 3 x wrtt)
    elif r["cell"] == C8 and r.get("one_leg"):
        t.append("ONE-LEG")                 # egress share < 10 % without the rtt condition: by design at small W (r1 falls to ~300 /s of ~9 300), informational
    if r.get("dnf"):
        t.append("DNF")
    return t


def score(paths, out=print, s54b_src=S54B_DEFAULT):
    pp = _pp()
    rows_all, tokens = pp.rows_of(paths)
    aborted = sorted({t for t in tokens if t in pp.ABORT_TOKENS})
    rows_all = [r for r in rows_all if r["cell"] in pp.CELLS56 and pp.arm_pool56(r["arm"])[0] is not None]
    cnt, replaced = counting(rows_all)
    n_exp = pp.expect_rows56()
    voids = [r for r in cnt if is_void(r)]
    live = [r for r in cnt if not is_void(r)]
    arms = by_arm(live)
    for cell_ in pp.CELLS56:                # per-row T0 (per-leg floors x the row's own rates) and standing queue, for the tables
        fl_ = leg_floors(arms, cell_)
        for r_ in live:
            t_ = r_.get("t56")
            if r_["cell"] == cell_ and t_:
                tb_, q_ = loop_and_queue({i: t_.get("r%d" % i) for i in (0, 1)}, {i: t_.get("rtt_late%d" % i) for i in (0, 1)}, fl_)
                t_["queue_ms"], t_["T0b_ms"] = q_, (tb_ - q_ if tb_ is not None else None)
    out("STORE-POOL SWEEP (status 56), REPORTED ONLY: one seat (current main), RWM_STORE_PATH_POOL by arm; c8-100 and c2-100 at 600 MB, c1s-400 at 5 GB, RWM_DIAG=1 RWM_RDIAG=1 "
        f"RWM_PERF_TIMEOUT_S={PERF_TIMEOUT_S}; nothing here is scored")
    out(f"SLOTS {len(slots(rows_all))}/{n_exp}  ROWS {len(rows_all)} (replaced {len(replaced)})  ABORT-TOKENS {' '.join(aborted) or 'none'}")
    for r in replaced:
        out(f"  REPLACED {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} {' '.join(r['problems'])}")
    for r in voids:
        out(f"  VOID {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} rc={r['rc']} {' '.join(r['problems'])}")
    for r in cnt:
        if r.get("t56_error"):
            out(f"  T56-ERROR {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']}: {r['t56_error']}")
    out("LIVENESS per arm (rows counting / DNF / UNBOUND / quinn-collapse flagged / c8 lock-out (section 3.9) / one-leg (informational); the bind fraction is the LATE share with cap >= N x pool, controls exempt)")
    for cell in pp.CELLS56:
        for pool in pp.POOLS56[cell]:
            arm = "P%d" % pool
            rs = [r for r in cnt if r["cell"] == cell and r["arm"] == arm]
            lv = [r for r in rs if not is_void(r)]
            bf = mval(lv, lambda r, t: t.get("bind_frac"))
            out(f"  {cell:<8} {arm_label(cell, arm):<12} rows {len(rs):>2}/{pp.N56[cell][pool]}  live {len(lv)}  void {len(rs) - len(lv)}  DNF {sum(1 for r in lv if r.get('dnf'))}  "
                f"UNBOUND {sum(1 for r in lv if is_unbound(r))}{' (control, exempt)' if pool == pp.CONTROL56[cell] else ''}  QCOLLAPSE {sum(1 for r in lv if qc_flagged(r))}  "
                f"LOCKOUT {sum(1 for r in lv if cell == C8 and r.get('locked'))}  ONE-LEG {sum(1 for r in lv if cell == C8 and r.get('one_leg') and not r.get('locked'))}  bind {_fmt(bf, 3)}  realised cap {_fmt(mval(lv, lambda r, t: t.get('cap_mean')), 0)}")
    nqc = sum(1 for r in live if qc_flagged(r))
    out(f"QUINN-COLLAPSE flagged c1s rows: {nqc} (reported in every table, excluded from F8)")
    out("")
    for cell in pp.CELLS56:
        out(f"== {cell}: arm medians (LATE window t >= {LATE_T[cell]:g} s); rows UNBOUND are in the arm and reported at their realised cap (W row)")
        cols = [("P%d" % p) for p in pp.POOLS56[cell] if arms.get((cell, "P%d" % p))]
        if cell == pp.DUMP_CELL56 and arms.get((cell, pp.DUMP_ARM56)):
            cols.append(pp.DUMP_ARM56)
        out("  " + f"{'readout (arm, nominal W)':<38}" + "".join(f"{(arm_label(cell, c) if c != pp.DUMP_ARM56 else c + '(dump)'):>13}" for c in cols))
        out("  " + f"{'n (live rows)':<38}" + "".join(f"{len(arms[(cell, c)]):>13}" for c in cols))
        for label, fn, nd in READOUTS:
            vals = [mval(arms[(cell, c)], fn) for c in cols]
            if all(v is None for v in vals):
                continue
            out("  " + f"{label:<38}" + "".join(f"{_fmt(v, nd):>13}" for v in vals))
        out("")
    out("PER-ROW TABLE (LIVE rows): cell arm seed rep | goodput | R | r0 | r1 | T-bar | W realised | bind | old holes | inorder late | qcollapse share | tags")
    for r in sorted(live, key=lambda x: (pp.CELLS56.index(x["cell"]), x["arm"], x["seed"], x["rep"])):
        t = r.get("t56") or {}
        out(f"  {r['cell']:<8} {r['arm']:<6} s{r['seed']:<3} r{r['rep']} | {'DNF' if r.get('dnf') else _fmt(t.get('goodput_mbps'), 1)} | {_fmt(t.get('tx_sum_late'), 0)} | {_fmt(t.get('r0'), 0)} | "
            f"{_fmt(t.get('r1'), 0)} | {_fmt(t.get('Tbar_ms'), 1)} | {_fmt(t.get('cap_mean'), 0)} | {_fmt(t.get('bind_frac'), 2)} | {_fmt(t.get('old_holes'), 0)} | "
            f"{_fmt(t.get('inorder_mean_late'), 2)} | {_fmt(t.get('qc_share'), 3)} | {' '.join(row_tags(r))}")
    out("")
    dump_rows = [r for r in live if pp.arm_pool56(r["arm"])[1]]
    out("SUCC_DUMP row(s): " + ("; ".join(f"{r['cell']} {r['arm']} s{r['seed']} rep{r['rep']}: records {(r.get('succdump') or {}).get('records')} "
                                         f"(cli {(r.get('succdump') or {}).get('cli', {}).get('records')}, srv {(r.get('succdump') or {}).get('srv', {}).get('records')}), "
                                         f"{(r.get('succdump') or {}).get('bytes')} bytes of [SUCCDUMP] lines, capped {(r.get('succdump') or {}).get('capped')}, "
                                         f"{'>= ' if ((r.get('succdump') or {}).get('records') or 0) >= pp.DUMP_MIN_RECORDS56 else 'UNDER '}{pp.DUMP_MIN_RECORDS56} records; "
                                         f"p0 hn_n {tv(r, 'hn_p0')}, [SUCC] det {tv(r, 'det')}" for r in dump_rows) or "none"))
    out("")
    sq = {c: standing_queue(arms, c) for c in pp.CELLS56}
    out("STANDING QUEUE AT THE KNEE (q = T-bar - T0, T0 = per-leg echo floors weighted by the arm's own rates; at the W where the arm-median goodput first reaches 0.95 / 0.98 of the control, "
        "interpolated in W; UNBOUND rows are points at their realised cap)")
    for cell in pp.CELLS56:
        s = sq[cell]
        out(f"  {cell}: control goodput {_fmt(s['control_gp'], 1)}  per-leg floors (ms) {({i: round(v, 1) for i, v in s['floors'].items()} or '-')}  ell {_fmt(s['ell'], 1)} ms  "
            f"T0 spread {_fmt(s['T0_spread'], 1)} ms over {s['T0_n']} smallest-W rows  curve (W, goodput, T-bar, q): " + ", ".join(f"({w:.0f}, {g:.1f}, {tb:.1f}, {q:.1f})" for w, g, tb, q in s["curve"]))
        for thr in KNEE_POINTS:
            p = s["points"].get(thr)
            if p is None:
                out(f"    {thr:.2f} x control: not reached / not computable")
            else:
                w, tb, dq, mult, note, t0k = p
                out(f"    {thr:.2f} x control: W {w:.0f}  T-bar {tb:.1f} ms  T0 {t0k:.1f} ms  q = {_fmt(dq, 1)} ms = {_fmt(mult, 2)} x ell  ({note})")
    c8p = (sq[C8]["points"].get(0.95) or (None,) * 6)
    out(f"  C8 prediction: knee at W ~ 500-700, q ~ 25-35 ms: measured W {_fmt(c8p[0], 0)}, {_fmt(c8p[2], 1)} ms")
    out("")
    ctrl_rows = arms.get((C8, "P%d" % pp.CONTROL56[C8]), [])
    ref = ref_stats(s54b_rows(s54b_src))
    out("RE-BASING (c8 control P640 against section 54b's B640 medians; tol = the B640 full range floored at 1% of its median)")
    offs = rebase(ctrl_rows, ref, out)
    band_report(arms, offs, out)
    other_predictions(arms, out)
    out("")
    out("FALSIFIERS (registered; goodput medians read a DNF as 0; an UNBOUND arm is read on its bound rows only; FIRES / DOES-NOT-FIRE / UNSCOREABLE):")
    fal = falsifiers(arms, sq, dump_rows)
    for i, text, state, det in fal:
        out(f"  F{i} {state:<13} {text} -- {det}")
    out(f"VERDICT56 REPORTED-ONLY slots={len(slots(rows_all))}/{n_exp} live={len(live)} void={len(voids)} replaced={len(replaced)} dnf={sum(1 for r in live if r.get('dnf'))} "
        f"unbound={sum(1 for r in live if is_unbound(r))} qcollapse={nqc} falsifiers=" + ",".join(f"F{i}:{s}" for i, _, s, _ in fal))


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    src = S54B_DEFAULT
    if "--s54b" in a:
        i = a.index("--s54b")
        src = a[i + 1]
        a = a[:i] + a[i + 2:]
    if cmd == "score":
        score(a, s54b_src=src)
        return 0
    if cmd == "smoke":
        return smoke(a[0])
    if cmd == "plan":
        for c, s_, ar, seed, rep in _pp().plan56(smoke=bool(a) and a[0] == "smoke"):
            print(f"{c} {s_} {ar} {seed} {rep}")
        return 0
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
