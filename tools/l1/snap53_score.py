#!/usr/bin/env python3
"""Status 53: stale gap reports under lever B, REPORTED ONLY. The `[SNAPV]` snapshot-order instrument, the RWM_RECOV_SNAPVER filter arm, W0 on CAND and a long c1d block.

    snap53_score.py score <ledger> [...]     the session's report from a plan-53 ledger (rows carry `t53`, computed at row time by `row_metrics`)
    snap53_score.py smoke <ledger>           the rehearsal / smoke check: PASS / FAIL lines for the registration's rehearsal criteria; rc 1 on any FAIL
    snap53_score.py plan [smoke]             plan 53's table (`cell store arm seed rep` per line)

Nothing here is a verdict: no clause, no threshold, no pass / fail on a measured value. The row's LIVENESS (below) decides whether a row counts, nothing else.

ONE parser per tag. The tags tail52_score.py already reads ([DIAG] `mpr[..]`, [HOLD], [C8CONV-R], the driver's CPU line) stay there (plan 53 added the fire counters
`fired / y / r / fast`, the `hy_n` / `hy_p50_us` / `ripe_frac` fields and the per-arrival-leg duplicates to it); this module adds the tags plan 53 introduces:
  * `[SNAPV] site=sender p<leg>:rep= stale= pt_st= tt_st= ... fa_class=` (client log, printed once at the sender's exit; the last line wins). `p-` is the sender's own
    tail sweep: it carries no snapshot, is never stale, and is kept apart: Sigma rep / stale / pt_st in E run over the numeric return legs only.
  * `[FCAUSE]` (client log, last line): n, timer, gap_data, gap_refresh, other, fired, unattr.
  * `[SUCC] ... final=1` in the SERVER log: `det` (the receiver's detected holes). The client log's [SUCC] lines are zeros and are never read.
Derived:
  * fed - det = Sigma over paths of the sender [HOLD] `fed` (last line per path, client log) minus `det`; compared with cx = Sigma `cx_n` (the retransmit-closed holes).
  * E = (Sigma pt_st / packet-threshold fires) / (Sigma stale / Sigma rep). The packet-threshold fire count is the `fast=` counter of the sender's [DIAG] `mpr[..]` block
    (the flag that also feeds pt_st). The registration's `y` is the fires whose live flight was younger than the time threshold: it equals `fast` unless a fire came
    through another door, and a row where they differ is flagged. E is a derived ratio, None when fast = 0 or Sigma stale = 0.
LIVENESS (registration): a row counts only if (1) the arm's env is in [GATES] on both ends (place38_parse: the row's status), (2) [SNAPV] is present with rep > 0 on every
active return leg and pt_st + tt_st <= the row's fires, and (3) on CANDS / BASES rows pt_st + tt_st = 0 on every leg, stale > 0 on p1 (c8) and fed - det <= cx.
Rank-sum: place41b_score.statistic_rank's per-clause p (one-sided, worse side; 1 - p reads the other side), at the design's sizes, beside the median ratio. The min-p
gate that function also computes is not printed."""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import stage3_parse as sp  # noqa: E402
import tail52_score as t52  # noqa: E402

C8, C1D = "c8-100", "c1d-400"
ARMS_ON = ("CANDS", "BASES")                       # the filter-arm rows
PERF_TIMEOUT_S = 200                               # RWM_PERF_TIMEOUT_S of plan 48 / 52 / 53's INSTR_ENV (loopdiag_battery.sh)

_SNAPV_LEG = re.compile(r" p(-|\d+):rep=(\d+) stale=(\d+) pt_st=(\d+) tt_st=(\d+)")


def snapv(cli):
    """{leg: {rep, stale, pt_st, tt_st}} of the sender's LAST `[SNAPV] site=sender` line (legs are strings: "0", "1", "-" = the tail sweep); None when the line is absent."""
    last = None
    for ln in cli or []:
        if ln.startswith("[SNAPV] site=sender"):
            last = ln
    if last is None:
        return None
    return {g: {"rep": int(a), "stale": int(b), "pt_st": int(c), "tt_st": int(d)} for g, a, b, c, d in _SNAPV_LEG.findall(last)}


def fcause(cli):
    """The sender's last `[FCAUSE]` line as {n, timer, gap_data, gap_refresh, other, fired, unattr}; None when absent."""
    ln = lc.last_with([x for x in cli or [] if x.startswith("[FCAUSE]")], "[FCAUSE]")
    if ln is None:
        return None
    out = {}
    for k in ("n", "timer", "gap_data", "gap_refresh", "other", "fired", "unattr"):
        v = lc.field(ln, k)
        out[k] = int(v) if v is not None and v.isdigit() else None
    return out


def det_of(srv):
    """`det` of the SERVER log's last `[SUCC] ... final=1` line; None when there is none (a bare last [SUCC] is not read: it is a running value)."""
    fin = [x for x in srv or [] if x.startswith("[SUCC]") and lc.is_final(x)]
    v = lc.field(fin[-1], "det") if fin else None
    return int(v) if v is not None and v.isdigit() else None


def enrichment(sv, fast):
    """E = (Sigma pt_st / fast) / (Sigma stale / Sigma rep) over the numeric return legs; None when undefined."""
    if not sv or not fast:
        return None
    legs = [d for g, d in sv.items() if g != "-"]
    rep, stale, pt = (sum(d[k] for d in legs) for k in ("rep", "stale", "pt_st"))
    if not rep or not stale:
        return None
    return (pt / fast) / (stale / rep)


def row_metrics(cli, srv, drv, nbytes=None):
    """One row's readouts: tail52_score.row_metrics plus the plan-53 fields; never raises on a missing field."""
    m = t52.row_metrics(cli, srv, drv, nbytes)
    ticks = t52.diag_ticks(cli)
    last = ticks[-1][1] if ticks else {}
    m["fires"] = {k: last.get(k) for k in ("fired", "y", "r", "fast")}
    m["snapv"] = snapv(cli)
    m["fcause"] = fcause(cli)
    hold = m.get("hold") or {}
    feds = [h["fed"] for h in hold.values() if h.get("fed") is not None]
    cxs = [h["cx_n"] for h in hold.values() if h.get("cx_n") is not None]
    m["fed_sum"] = sum(feds) if feds else None
    m["cx_sum"] = sum(cxs) if cxs else None
    m["det"] = det_of(srv)
    m["fed_minus_det"] = m["fed_sum"] - m["det"] if m["fed_sum"] is not None and m["det"] is not None else None
    m["E"] = enrichment(m["snapv"], m["fires"]["fast"])
    return m


# ── liveness ──
def n_legs(cell):
    return sp.N_LEGS[cell]


def snapv_problems(t, cell):
    """The [SNAPV] half of the liveness: present, rep > 0 on EVERY active leg (a missing leg fails, never passes vacuously), pt_st + tt_st <= the row's fires."""
    sv = t.get("snapv")
    if sv is None:
        return ["no-[SNAPV]"]
    p = []
    for leg in range(n_legs(cell)):
        if not (sv.get(str(leg)) or {}).get("rep"):
            p.append(f"rep<=0-on-p{leg}")
    fired = (t.get("fires") or {}).get("fired")
    st = sum(d["pt_st"] + d["tt_st"] for d in sv.values())
    if fired is None:
        p.append("no-fires")
    elif st > fired:
        p.append(f"pt_st+tt_st={st}>fires={fired}")
    return p


def arm_on_problems(t, cell):
    """The extra liveness of a CANDS / BASES row: the filter removed every stale judgement, stale reports exist on p1 (c8), and fed - det <= cx."""
    sv, p = t.get("snapv") or {}, []
    for leg, d in sv.items():
        if d["pt_st"] + d["tt_st"]:
            p.append(f"pt_st+tt_st={d['pt_st'] + d['tt_st']}-on-p{leg}")
    if cell == C8 and not (sv.get("1") or {}).get("stale"):
        p.append("stale=0-on-p1")
    fmd, cx = t.get("fed_minus_det"), t.get("cx_sum")
    if fmd is None or cx is None:
        p.append("fed-det-unavailable")
    elif fmd > cx:
        p.append(f"fed-det={fmd}>cx={cx}")
    return p


def liveness(r):
    """-> (verdict, reasons) of a ledger row: VOID when its status is not LIVE (an arm env missing from [GATES], a failed parse, a crash...) or a rule above fails."""
    t = r.get("t53")
    reasons = [] if r.get("status") == "LIVE" else [f"status={r.get('status')}"]
    if not t:
        return "VOID", reasons + ["no-t53" + (f"({r['t53_error']})" if r.get("t53_error") else "")]
    reasons += snapv_problems(t, r["cell"])
    if r["arm"] in ARMS_ON:
        reasons += arm_on_problems(t, r["cell"])
    return ("LIVE" if not reasons else "VOID"), reasons


# ── the registered readouts of a row: (label, getter(row, t) -> value|None, decimals) ──
def _leg(d, g, k):
    return ((d or {}).get(g) or {}).get(k)


def _ratio(a, b):
    return a / b if a is not None and b else None


def _hold(t, p, k):
    return t52._hold(t, p).get(k)


READOUTS = [
    ("goodput Mbit/s", lambda r, t: t.get("goodput_mbps"), 1),
    ("completion tail s", lambda r, t: t.get("tail_s"), 2),
    ("H0 holes at source end", lambda r, t: t.get("H0"), 0),
    ("receiver duplicates", lambda r, t: t.get("rx_dup"), 0),
    ("  dup, arrival leg p0", lambda r, t: (t.get("rx_dup_legs") or {}).get("0"), 0),
    ("  dup, arrival leg p1", lambda r, t: (t.get("rx_dup_legs") or {}).get("1"), 0),
    ("retransmits per GB", lambda r, t: t.get("retx_per_gb"), 0),
    ("retx send-leg-1 share", lambda r, t: t.get("send_leg1_share"), 3),
    ("client CPU s/GB", lambda r, t: t.get("cpu_cli_s_per_gb"), 2),
    ("server CPU s/GB", lambda r, t: t.get("cpu_srv_s_per_gb"), 2),
    ("lag_E_cli us", lambda r, t: r.get("cli_lag_E"), 0),
    ("lag_E_srv us", lambda r, t: r.get("srv_lag_E"), 0),
    ("rtp_floor_p0 us", lambda r, t: r.get("rtp_floor_p0"), 0),
    ("rtp_floor_p1 us", lambda r, t: r.get("rtp_floor_p1"), 0),
    ("fires (y + r)", lambda r, t: (t.get("fires") or {}).get("fired"), 0),
    ("  packet-threshold fires (fast)", lambda r, t: (t.get("fires") or {}).get("fast"), 0),
    ("  y (young fires)", lambda r, t: (t.get("fires") or {}).get("y"), 0),
    ("  time-threshold fires (r)", lambda r, t: (t.get("fires") or {}).get("r"), 0),
    ("[FCAUSE] gap_data", lambda r, t: (t.get("fcause") or {}).get("gap_data"), 0),
    ("[FCAUSE] gap_refresh", lambda r, t: (t.get("fcause") or {}).get("gap_refresh"), 0),
    ("[FCAUSE] timer", lambda r, t: (t.get("fcause") or {}).get("timer"), 0),
    ("sender fed (sum over paths)", lambda r, t: t.get("fed_sum"), 0),
    ("receiver det", lambda r, t: t.get("det"), 0),
    ("fed - det", lambda r, t: t.get("fed_minus_det"), 0),
    ("cx (sum cx_n)", lambda r, t: t.get("cx_sum"), 0),
]
for _p in (0, 1):
    READOUTS += [
        (f"[HOLD] p{_p} fed", lambda r, t, p=_p: _hold(t, p, "fed"), 0),
        (f"[HOLD] p{_p} hn", lambda r, t, p=_p: _hold(t, p, "hn_n"), 0),
        (f"[HOLD] p{_p} hy", lambda r, t, p=_p: _hold(t, p, "hy_n"), 0),
        (f"[HOLD] p{_p} hy_p50 us", lambda r, t, p=_p: _hold(t, p, "hy_p50_us"), 0),
        (f"[HOLD] p{_p} cx", lambda r, t, p=_p: _hold(t, p, "cx_n"), 0),
        (f"[HOLD] p{_p} ripe_frac", lambda r, t, p=_p: _hold(t, p, "ripe_frac"), 4),
    ]
for _g in ("0", "1"):
    READOUTS += [
        (f"[SNAPV] p{_g} rep", lambda r, t, g=_g: _leg(t.get("snapv"), g, "rep"), 0),
        (f"[SNAPV] p{_g} stale", lambda r, t, g=_g: _leg(t.get("snapv"), g, "stale"), 0),
        (f"[SNAPV] p{_g} stale/rep", lambda r, t, g=_g: _ratio(_leg(t.get("snapv"), g, "stale"), _leg(t.get("snapv"), g, "rep")), 3),
        (f"[SNAPV] p{_g} pt_st", lambda r, t, g=_g: _leg(t.get("snapv"), g, "pt_st"), 0),
        (f"[SNAPV] p{_g} tt_st", lambda r, t, g=_g: _leg(t.get("snapv"), g, "tt_st"), 0),
    ]
READOUTS += [
    ("[SNAPV] p1 share of rep", lambda r, t: _ratio(_leg(t.get("snapv"), "1", "rep"), sum(d["rep"] for g, d in (t.get("snapv") or {}).items() if g != "-") or None), 3),
    ("E (enrichment factor)", lambda r, t: t.get("E"), 2),
]
def missing(r):
    """The registered readouts that read None on a row (the rehearsal's no-None criterion). Exempt as undefined, not missing: `hy_p50` of a path with no hy, H0 when
    the tail is shorter than one [DIAG] tick, and E without packet-threshold fires or stale reports. (Calibrated on the s52 c8 rows and the guard's c1d rows: nothing else is
    None there once [SNAPV] exists.)"""
    t = r.get("t53") or {}
    out = []
    for label, fn, _ in READOUTS:
        try:
            v = fn(r, t)
        except Exception:
            v = None
        if v is not None:
            continue
        lab = label.strip()
        if lab.startswith("H0") and t.get("tail_s") is not None:
            continue        # no [DIAG] tick after the source end: lever B's tail can be shorter than one tick (1 of 6 s52 CAND rows) and c1d's always is (24 of 24 guard rows)
        if lab.startswith("E (enrichment"):
            if (t.get("fires") or {}).get("fast") and sum(d["stale"] for g, d in (t.get("snapv") or {}).items() if g != "-"):
                out.append(lab)
            continue
        m = re.match(r"\[HOLD\] p(\d) hy_p50", lab)
        if m and not _hold(t, int(m.group(1)), "hy_n"):
            continue
        out.append(lab)
    return out


# ── the rehearsal / smoke ──
def smoke(path, out=print):
    import place38_parse as pp
    rows, _ = pp.rows_of([path])
    got = {(r["cell"], r["arm"]): r for r in rows}
    out(f"SMOKE53 rows={len(rows)}/{len(pp.SMOKE53)} (one row of each arm per block, unscored; the values are not read, only the criteria below)")
    bad = []

    def crit(name, fails):
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)

    miss_row = [f"{c} {a}: no row" for c, _, a, _, _ in pp.SMOKE53 if (c, a) not in got]
    crit("every smoke row of the table ran", miss_row)
    rs = [got[(c, a)] for c, _, a, _, _ in pp.SMOKE53 if (c, a) in got]
    for r in rs:
        t = r.get("t53") or {}
        sv = t.get("snapv") or {}
        out(f"  ROW {r['cell']} {r['arm']}: status={r['status']} mbps={r.get('mbps')} wall={r.get('wall_s')} fires={(t.get('fires') or {})} "
            f"snapv={ {g: (d['rep'], d['stale'], d['pt_st'], d['tt_st']) for g, d in sv.items()} } fed-det={t.get('fed_minus_det')} cx={t.get('cx_sum')} E={t.get('E')}")
    crit("[SNAPV] parsed on every smoke row, rep > 0 per active leg, pt_st + tt_st <= fires",
         [f"{r['cell']} {r['arm']}: {' '.join(snapv_problems(r.get('t53') or {}, r['cell']))}" for r in rs if snapv_problems(r.get("t53") or {}, r["cell"])])
    crit("fed - det computed (sender [HOLD] fed over paths, receiver's final [SUCC] det)",
         [f"{r['cell']} {r['arm']}: fed={(r.get('t53') or {}).get('fed_sum')} det={(r.get('t53') or {}).get('det')}" for r in rs if (r.get("t53") or {}).get("fed_minus_det") is None])
    crit("each arm's env in [GATES] on both ends (RWM_RECOV_SNAPVER: 1 on CANDS / BASES, 0 elsewhere; RWM_PLACE_WDIV=0 on CW0, absent elsewhere)",
         [f"{r['cell']} {r['arm']}: {' '.join(x for x in r['problems'] if x.startswith('gate-'))}" for r in rs if any(x.startswith("gate-") for x in r["problems"])])
    crit("the filter arms' liveness (CANDS / BASES: pt_st + tt_st = 0 on every leg, stale > 0 on p1 at c8, fed - det <= cx)",
         [f"{r['cell']} {r['arm']}: {' '.join(arm_on_problems(r.get('t53') or {}, r['cell']))}" for r in rs
          if r["arm"] in ARMS_ON and arm_on_problems(r.get("t53") or {}, r["cell"])])
    crit(f"the c1d 5 GB row(s) finished inside RWM_PERF_TIMEOUT_S={PERF_TIMEOUT_S} (rc 0, a summary, not DNF)",
         [f"{r['cell']} {r['arm']}: rc={r.get('rc')} dnf={r.get('dnf')} status={r['status']} wall={r.get('wall_s')}" for r in rs
          if r["cell"] == C1D and (r.get("rc") != 0 or r.get("dnf") or not r.get("summary") or r.get("seconds") is None)])
    crit("no None among the registered readouts (hy_p50 without a hy and E without fast fires / stale reports are undefined, not missing)",
         [f"{r['cell']} {r['arm']}: {', '.join(missing(r))}" for r in rs if missing(r)])
    for r in rs:
        v, why = liveness(r)
        out(f"  LIVENESS {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']}: {v} {' '.join(why)}")
    if bad:
        out("SMOKE-FAIL " + "; ".join(bad))
        return 1
    out("SMOKE-PASS")
    return 0


# ── aggregation / report ──
def _fmt(x, nd):
    return "-" if x is None else f"{x:.{nd}f}"


def arm_stats(rows, fn, nd):
    """-> 'median [min - max] (n)' of a readout over the rows, or '-' when no row has it."""
    v = []
    for r in rows:
        try:
            x = fn(r, r.get("t53") or {})
        except Exception:
            x = None
        if x is not None:
            v.append(x)
    if not v:
        return "-"
    return f"median {_fmt(lc.med(v), nd)}  [{_fmt(min(v), nd)} - {_fmt(max(v), nd)}]  (n {len(v)})"


def flatten(r):
    """A ledger row with the plan-53 readouts at its top level (what place41b_score.Pool reads by key)."""
    t = r.get("t53") or {}
    d = dict(r)
    d["dup_total"], d["dup_p0"], d["dup_p1"] = t.get("rx_dup"), (t.get("rx_dup_legs") or {}).get("0"), (t.get("rx_dup_legs") or {}).get("1")
    d["retx_per_gb"] = t.get("retx_per_gb")
    return d


EXTRA_DEFS = [("dup_total", "dup_total", 1), ("dup_p0", "dup_p0", 1), ("dup_p1", "dup_p1", 1), ("retx/GB", "retx_per_gb", 1)]


def compare(rows, cell, ref, test, out, n_perm=None):
    """CAND-style comparison of two arms at a cell: per readout the rank-sum p (place41b_score.statistic_rank's per-clause p, one-sided worse side, rows stratified by
    seed) and the median ratio test / ref. Returns the p table."""
    import place41b_score as g41
    flat = [flatten(r) for r in rows]
    n = {a: sum(1 for r in flat if r["cell"] == cell and r["arm"] == a) for a in (ref, test)}
    res = g41.statistic_rank(flat, ref, test, n_perm=n_perm or g41.N_PERM, cells=[(cell, "def")], extra_defs=EXTRA_DEFS)
    pool = res["pools"][0]
    out(f"  {cell}: {test} (n {n[test]}) against {ref} (n {n[ref]}); p = rank-sum permutation p, ONE-SIDED worse side (higher value; lower for goodput), within-seed ranks, "
        f"{res['n_perm']} relabellings; a p near 1 means {test} sits on the better side; ratio = median {test} / median {ref}")
    out(f"    {'readout':<14} {'p(worse)':>9} {'ratio':>8}   median {ref} -> {test}")
    for name, key, _ in pool.defs:
        tr = set(pool.true)
        a = [v for i, v in enumerate(pool.vals[name]) if v is not None and i in tr]
        b = [v for i, v in enumerate(pool.vals[name]) if v is not None and i not in tr]
        ma, mb = (lc.med(a) if a else None), (lc.med(b) if b else None)
        ratio = ma / mb if ma is not None and mb else None
        out(f"    {name:<14} {_fmt(res['p_true'].get((cell, name)), 4):>9} {_fmt(ratio, 3):>8}   {_fmt(mb, 3)} -> {_fmt(ma, 3)}")
    return res["p_true"]


def report_cell(rows, cell, arms, out):
    out(f"== {cell}")
    for arm in arms:
        rs = [r for r in rows if r["cell"] == cell and r["arm"] == arm]
        out(f"-- {arm}: n = {len(rs)} counting rows")
        for label, fn, nd in READOUTS:
            out(f"  {label:<36} " + arm_stats(rs, fn, nd))


def score(paths, out=print):
    import place38_parse as pp
    rows_all, tokens = pp.rows_of(paths)
    aborted = sorted({t for t in tokens if t in pp.ABORT_TOKENS})
    rows = [r for r in rows_all if (r["cell"], r["arm"]) in {(pp.CELL53_C8, a) for a in pp.ARMS53_C8} | {(pp.CELL53_C1D, a) for a in pp.ARMS53_C1D}]
    n_exp = pp.expect_rows53()
    out("STALE GAP REPORTS UNDER LEVER B (status 53), REPORTED ONLY: c8-100 at 600 MB (BASE, CAND, CANDS = CAND + RWM_RECOV_SNAPVER=1, BASES = BASE + RWM_RECOV_SNAPVER=1, "
        "CW0 = CAND + RWM_PLACE_WDIV=0) and c1d-400 at 5 GB (BASE, CAND, CANDS); nothing here is scored")
    out(f"ROWS {len(rows)}/{n_exp}; ABORT-TOKENS {' '.join(aborted) or 'none'}")
    verdicts = {id(r): liveness(r) for r in rows}
    out("LIVENESS per row (VOID rows are listed and do not enter the readouts; a void row means that arm is re-run)")
    for cell in (pp.CELL53_C8, pp.CELL53_C1D):
        for arm in sorted({r["arm"] for r in rows if r["cell"] == cell}, key=lambda a: pp.ARMS53.index(a)):
            rs = [r for r in rows if r["cell"] == cell and r["arm"] == arm]
            nv = sum(1 for r in rs if verdicts[id(r)][0] == "VOID")
            out(f"  {cell} {arm:<6} rows {len(rs)} LIVE {len(rs) - nv} VOID {nv}")
    for r in rows:
        v, why = verdicts[id(r)]
        if v == "VOID":
            out(f"  VOID {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']} rc={r.get('rc')}: {' '.join(why)}")
    for r in rows:
        f = (r.get("t53") or {}).get("fires") or {}
        if f.get("y") != f.get("fast"):
            out(f"  Y-NE-FAST {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']}: y={f.get('y')} fast={f.get('fast')} (E reads `fast`, the exact packet-threshold counter)")
    out("WITNESS (the arm's env in [GATES], client/server): " + "; ".join(
        f"{a} snapver={sorted({str(r.get('gate_snapver_cli')) + '/' + str(r.get('gate_snapver_srv')) for r in rows if r['arm'] == a})} "
        f"wdiv={sorted({str(r.get('gate_wdiv_cli')) + '/' + str(r.get('gate_wdiv_srv')) for r in rows if r['arm'] == a})}" for a in pp.ARMS53))
    live = [r for r in rows if verdicts[id(r)][0] == "LIVE"]
    out("")
    report_cell(live, pp.CELL53_C8, pp.ARMS53_C8, out)
    out("")
    out("== c8 comparisons (rank-sum p, n = 6 vs 6: reported beside the row ranges; nothing is scored)")
    for ref, test in (("BASE", "CAND"), ("CAND", "CANDS"), ("BASE", "BASES"), ("CAND", "CW0")):
        compare(live, pp.CELL53_C8, ref, test, out)
    out("")
    report_cell(live, pp.CELL53_C1D, pp.ARMS53_C1D, out)
    out("")
    out("== c1d: CAND against BASE (n = 12 + 12), per readout the rank-sum p and the median ratio, to be read beside section 41c's c1d resolution "
        "(host: python3 tools/l1/place38_parse.py resolution41c <pulled run dir of the guard>/ld.log; the lag clauses are blind at 20-100 %, so a lag null says nothing)")
    compare(live, pp.CELL53_C1D, "BASE", "CAND", out)
    out("== c1d: CANDS against CAND (n = 6 vs 12; not predicted numerically)")
    compare(live, pp.CELL53_C1D, "CAND", "CANDS", out)
    out("")
    out("== direction check (no derived bound): stale/rep on the c1d legs against the c8 p1 leg, median per arm")
    for arm in pp.ARMS53_C1D:
        c8 = [r for r in live if r["cell"] == pp.CELL53_C8 and r["arm"] == arm]
        c1 = [r for r in live if r["cell"] == pp.CELL53_C1D and r["arm"] == arm]
        sr = lambda rs, g: arm_stats(rs, lambda r, t: _ratio(_leg(t.get("snapv"), g, "stale"), _leg(t.get("snapv"), g, "rep")), 3)  # noqa: E731
        out(f"  {arm:<6} c8 p1 {sr(c8, '1')}\n         c1d p0 {sr(c1, '0')}\n         c1d p1 {sr(c1, '1')}")
    out(f"VERDICT53 REPORTED-ONLY rows={len(rows)}/{n_exp} live={len(live)} void={len(rows) - len(live)}")


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd == "score":
        score(a)
        return 0
    if cmd == "smoke":
        return smoke(a[0])
    if cmd == "plan":
        import place38_parse as pp
        for c, s_, ar, seed, rep in pp.plan53(smoke=bool(a) and a[0] == "smoke"):
            print(f"{c} {s_} {ar} {seed} {rep}")
        return 0
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
