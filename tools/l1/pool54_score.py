#!/usr/bin/env python3
"""Status 54: the snapshot-order law's liveness witnesses (plan 54, the scored guard) and the store-budget block (plan 54b, REPORTED ONLY).

    pool54_score.py score <ledger> [...] [--s53 <rep53-diag-ld.tgz | dir>]   the block's report from a plan-54b ledger (rows carry `t54`, computed at row time)
    pool54_score.py smoke <ledger>          plan 54b's rehearsal / smoke check: PASS / FAIL per registered criterion; rc 1 on any FAIL
    pool54_score.py smoke54 <ledger>        plan 54's rehearsal: plan 41d's smoke criteria, then the three liveness witnesses on every smoke row; rc 1 on any FAIL
    pool54_score.py guard <ledger> [...]    plan 54's witness table (printed after score41d's verdict by `score54`; it never changes that verdict)
    pool54_score.py plan [smoke]            plan 54b's table (`cell store arm seed rep` per line)

PLAN 54 (the guard) is plan 41d, unchanged, plus three row-level liveness witnesses, switched on by LD_WITNESS54=1 (run_all carries it in SENV for LD_PLAN=54):
  * `bin_sha` = the sha256 of the arm's own build (BASE rows: LD_SHA_BASE, CAND rows: LD_SHA);
  * at c8, a CAND row's p0 `[HOLD] hn_n` < HN_COLLAPSE (the snapshot-order law collapses it: 11-409 with the filter, 9 557-10 659 without, status 53). A CAND c8 row without the
    collapse is VOID. A BASE row's hn is reported, never a witness;
  * `[SNAPV]` is present on every CAND row with pt_st + tt_st = 0 on every leg (the gauge is in the build; the law judges only the newest snapshot). A BASE row carries no `[SNAPV]`.
A DNF row keeps its worst rank (its witnesses are not applied, as the v2 VOID-SHORT rule): a dying candidate gets no free re-run. Plan 53's liveness (rep > 0, stale > 0, fed - det
<= cx, the RWM_RECOV_SNAPVER echo) is NOT used here: the arm no longer exists and main has no `[SNAPV]`.

PLAN 54b (the block) readouts, with the EXACT definitions of docs/derivations/p0-dwell-diag.md section 8 (reference scripts: p0-dwell-diag/{inorder,budget2,holes,rows}.py):
  * the LATE window is the client [DIAG] ticks with t >= LATE_T (15 s) and ` src=N sym/s` > 0, chosen by time and not by cap;
  * every per-path field is anchored on ` pN:infl=` (` pN:` alone first matches the `mpr[.. fp/on p0:.. p1:..]` token);
  * `inorder_mean_run` = threadp1_parse.inorder_delay_stats (every tick with ack > 0); `inorder_mean_late` / p50 / p95 = the same delay series restricted to the sending ticks at
    t >= 15 s. Quantiles use inorder.py's index rule sorted[min(n-1, int(p*(n-1)+0.5))]. `lag_end` = tail52's (src - ack at the last sending tick);
  * `q0_late` = median over LATE ticks of `dgq0 hand - tx` (the block table's p0 backlog); `evict_full` and `hand_minus_tx` per path from the LAST [DIAG] line (the eviction readout);
  * `rtt0_late`, `wrtt0_late` = LATE medians of p0's app-echo and wire rtt;
  * the budget residual: per LATE tick, b - (r0*rtt0 + r1*rtt1)/1000 with b = the tick's OWN cap (the `win=a/b` denominator, not the numerator `a` that budget2.py
    subtracts) and r_i = d(dgq_i tx)/dt between the first and last LATE tick; the row's value is the median over LATE ticks. `resid_rel` = residual / the row's median cap W;
  * old holes = median of the server [RANK] holes - tail_overcount over readouts[15:-6];
  * the cap bind fraction = the share of LATE ticks with b = 2*pool (reported, never voiding);
  * goodput, tail, H0, duplicates, retransmits/GB, CPU/GB and `[SNAPV]` are tail52_score's / snap53_score's, unchanged.
A DNF row has no goodput: the arm statistics count it (n, DNF) and the falsifiers read goodput with a DNF as 0, never over the survivors only.
Liveness (the pool, the ONLY check that voids a block row on the knob): the client and server [GATES] show RWM_STORE_PATH_POOL=<requested n> (2048 on the controls: no silent fallback)
and the INFO `pool_per_path=<n>` of the sender (and of the server, when it prints one) matches. The p1 `stale > 0` and the tail witness at W = 4096 (B tail >= 2 s, L tail <= 0.8 s) are
printed per row, not voided (a reduced W makes the tail an outcome).
Nothing here is a verdict: the falsifiers are evaluated and printed, the decision rule is read out, and the block decides nothing by itself."""
import io
import os
import re
import sys
import tarfile
import tempfile
from contextlib import redirect_stdout

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import snap53_score as s53  # noqa: E402
import tail52_score as t52  # noqa: E402
from threadp1_parse import inorder_delay, inorder_delay_stats  # noqa: E402

C8 = "c8-100"
LATE_T = 15.0
RANK_HEAD, RANK_TAIL = 15, 6
HN_COLLAPSE = 2000                     # plan 54: a CAND c8 row's p0 [HOLD] hn_n must be below this (status 54)
PERF_TIMEOUT_S = 300                               # plan 54b only (loopdiag_battery.sh INSTR_ENV): B640 under H-hold needs 240 s
BYTES = 600_000_000
DEFAULT_POOL = 2048

ARMS54B = ("B2048", "B1024", "B640", "L2048", "L1024", "L640")
# arm -> (binary seat: BASE = the tree in LD_BIN_BASE (cand54), CAND = LD_BIN (levb54), requested RWM_STORE_PATH_POOL). The battery's arm table is the shell copy (test_pool54 asserts it).
ARM_POOL54 = {"B2048": ("BASE", 2048), "B1024": ("BASE", 1024), "B640": ("BASE", 640), "L2048": ("CAND", 2048), "L1024": ("CAND", 1024), "L640": ("CAND", 640)}
ARM_SET_ENV = {"B2048": None, "B1024": 1024, "B640": 640, "L2048": None, "L1024": 1024, "L640": 640}     # the controls set nothing: the default is the registered 2048

_T = re.compile(r"^\[DIAG\] t=([0-9.]+)s ")
_SRC = re.compile(r" src=(\d+)sym/s")
_WIN = re.compile(r" win=(\d+)/(\d+)")
_DGQ = re.compile(r"dgq(\d)\[hand=(\d+) tx=(\d+) full=(\d+) err=(\d+) sp=(\d+)\]")
_PATH = r" p%d:infl=\d+/sinfl=\d+/bdp\d+\S* sout=\d+ khr=\S+ btlbw=\d+.*?rtt=(\d+)/wrtt=(\d+)"
_PATH_RE = {i: re.compile(_PATH % i) for i in (0, 1)}
_RANK_HOLES, _RANK_OVER = re.compile(r"holes=(\d+)"), re.compile(r"tail_overcount=(\d+)")
_POOL_ECHO = re.compile(r"(?<![\w])RWM_STORE_PATH_POOL=(\S+)")
_POOL_INFO = re.compile(r"pool_per_path=(\d+)")


def q_idx(v, p):
    """inorder.py's quantile: sorted[min(n-1, int(p*(n-1)+0.5))] (None on an empty sample)."""
    v = sorted(v)
    return v[min(len(v) - 1, int(p * (len(v) - 1) + 0.5))] if v else None


def med(v):
    v = [x for x in v if x is not None]
    return lc.med(v) if v else None


# ── the [DIAG] ticks ──
def ticks(cli):
    """[dict] of the client's [DIAG] lines that carry the fields the block reads: t, src (sym/s), win, cap (the denominator), dgq {leg: (hand, tx, full)}, rtt / wrtt {leg}."""
    out = []
    for ln in cli or []:
        mt = _T.match(ln)
        if not mt:
            continue
        ms, mw = _SRC.search(ln), _WIN.search(ln)
        d = {"t": float(mt.group(1)), "src": int(ms.group(1)) if ms else None, "win": int(mw.group(1)) if mw else None, "cap": int(mw.group(2)) if mw else None,
             "dgq": {int(i): (int(h), int(x), int(f)) for i, h, x, f, _, _ in _DGQ.findall(ln)}, "rtt": {}, "wrtt": {}}
        for i, rx in _PATH_RE.items():
            m = rx.search(ln)
            if m:
                d["rtt"][i], d["wrtt"][i] = int(m.group(1)), int(m.group(2))
        out.append(d)
    return out


def late(tk, t_min=LATE_T):
    """The LATE window: ticks with t >= t_min and src > 0."""
    return [d for d in tk if d["t"] >= t_min and (d["src"] or 0) > 0]


def late_budget(lt, legs=(0, 1)):
    """The budget identity over a window: -> {r: {leg: tx rate /s}, resid: median over ticks of cap - sum(r_i * rtt_i)/1000, cap: median cap, q: {leg: median hand - tx},
    rtt/wrtt: {leg: median}}; None when the window has fewer than 4 usable ticks. Ticks lacking a leg's dgq or rtt are skipped."""
    use = [d for d in lt if d["cap"] is not None and all(i in d["dgq"] and i in d["rtt"] for i in legs)]
    if len(use) < 4:
        return None
    dt = use[-1]["t"] - use[0]["t"]
    if dt <= 0:
        return None
    r = {i: (use[-1]["dgq"][i][1] - use[0]["dgq"][i][1]) / dt for i in legs}
    resid = [d["cap"] - sum(r[i] * d["rtt"][i] / 1000 for i in legs) for d in use]
    return {"n": len(use), "r": r, "resid": med(resid), "cap": med([d["cap"] for d in use]),
            "q": {i: med([d["dgq"][i][0] - d["dgq"][i][1] for d in use]) for i in legs},
            "rtt": {i: med([d["rtt"][i] for d in use]) for i in legs}, "wrtt": {i: med([d["wrtt"][i] for d in use]) for i in legs}}


def old_holes(srv):
    """Median of the server [RANK] holes - tail_overcount over the readouts[15:-6]; None without such readouts."""
    rk = [ln for ln in srv or [] if ln.startswith("[RANK]")]
    vals = []
    for ln in rk[RANK_HEAD:-RANK_TAIL]:
        h, o = _RANK_HOLES.search(ln), _RANK_OVER.search(ln)
        if h and o:
            vals.append(int(h.group(1)) - int(o.group(1)))
    return med(vals), len(rk)


def inorder_late(cli):
    """-> {mean, p50, p95, n}: the in-order delay over the sending ticks at t >= LATE_T (inorder_delay(source_only=True)); None if there is none."""
    d = [x for t, x in inorder_delay(cli, source_only=True) if t >= LATE_T]
    if not d:
        return None
    return {"mean": sum(d) / len(d), "p50": q_idx(d, 0.5), "p95": q_idx(d, 0.95), "n": len(d)}


def lat_coverage(srv):
    """[LAT] coverage = sum of the per-path `pN:n` over `n` of the receiver's last [LAT] line (reported, never the delay readout); None without one."""
    ln = lc.last_with([x for x in srv or [] if x.startswith("[LAT]")], "[LAT]")
    if not ln:
        return None
    n = lc.field(ln, "n")
    pn = [int(x) for x in re.findall(r" p\d:n=(\d+)", ln)]
    return sum(pn) / int(n) if n and n.isdigit() and int(n) and pn else None


def pool_of(lines):
    """-> (the [GATES] RWM_STORE_PATH_POOL token of the last [GATES] line or None, the last `pool_per_path=` of the log or None)."""
    g = [ln for ln in lines or [] if "[GATES]" in ln]
    tok = None
    if g:
        m = _POOL_ECHO.search(g[-1])
        tok = m.group(1) if m else None
    info = [int(x) for ln in lines or [] for x in _POOL_INFO.findall(ln)]
    return tok, (info[-1] if info else None)


def row_metrics(cli, srv, drv, nbytes=None, pool=None):
    """One row's readouts: snap53_score.row_metrics (tail52's goodput / tail / H0 / duplicates / retx / CPU, [HOLD], [SNAPV], fires) plus the block's fields. Never raises on a missing field.
    `pool` = the requested RWM_STORE_PATH_POOL (for the bind fraction)."""
    m = s53.row_metrics(cli, srv, drv, nbytes)
    tk = ticks(cli)
    lt = late(tk)
    b = late_budget(lt)
    m["late_n"] = len(lt)
    m["cap_med"] = b["cap"] if b else med([d["cap"] for d in lt])
    m["bind_frac"] = (sum(1 for d in lt if d["cap"] == 2 * pool) / len(lt)) if (lt and pool) else None
    m["q0_late"] = b["q"][0] if b else None
    m["q1_late"] = b["q"][1] if b else None
    m["rtt0_late"] = b["rtt"][0] if b else None
    m["wrtt0_late"] = b["wrtt"][0] if b else None
    m["rtt0_minus_wrtt0"] = m["rtt0_late"] - m["wrtt0_late"] if b else None
    m["r0"], m["r1"] = (b["r"][0], b["r"][1]) if b else (None, None)
    m["tx_sum_late"] = m["r0"] + m["r1"] if b else None
    m["resid_late"] = b["resid"] if b else None
    m["resid_rel"] = b["resid"] / b["cap"] if b and b["cap"] else None
    last = tk[-1] if tk else None
    for i in (0, 1):
        g = (last or {}).get("dgq", {}).get(i)
        m[f"evict_full_p{i}"] = g[2] if g else None
        m[f"hand_minus_tx_p{i}"] = g[0] - g[1] if g else None
    m["evict_full"] = sum(m[f"evict_full_p{i}"] for i in (0, 1)) if last and all(m[f"evict_full_p{i}"] is not None for i in (0, 1)) else None
    run = inorder_delay_stats(cli)
    m["inorder_mean_run"] = run["mean"] if run else None
    m["inorder_p95_run"] = run["p95"] if run else None
    lt_in = inorder_late(cli)
    m["inorder_mean_late"], m["inorder_p50_late"], m["inorder_p95_late"] = (lt_in["mean"], lt_in["p50"], lt_in["p95"]) if lt_in else (None, None, None)
    m["old_holes"], m["rank_readouts"] = old_holes(srv)
    m["lat_cov"] = lat_coverage(srv)
    m["hn_p0"] = (m.get("hold") or {}).get(0, {}).get("hn_n")
    return m


# ── plan 54: the guard's liveness witnesses ──
SEAT = {"BASE": "BASE", "CAND": "CAND"}


def sha_problems(arm, sha, sha_cand, sha_base):
    """The row's bin_sha against its arm's seat (BASE = LD_SHA_BASE, CAND = LD_SHA). A comparison that cannot run (the row has no sha, or the seat's sha did not reach the parser) is
    a PROBLEM, never a silent pass: a witness that can be skipped witnesses nothing. Arms of no seat (the other plans') are not checked."""
    if arm not in SEAT:
        return []
    want = {"BASE": sha_base, "CAND": sha_cand}[arm]
    if not want or not sha:
        return [f"bin-sha-uncheckable (row sha {str(sha)[:12] or None}, {arm} seat {str(want)[:12] or None}: LD_SHA / LD_SHA_BASE did not reach the parser)"]
    if sha != want:
        return [f"bin-sha={str(sha)[:12]} (want {want[:12]} for {arm})"]
    return []


def sha_checked(arm, sha, sha_cand, sha_base):
    """True iff the bin_sha comparison actually ran on this row (recorded in the ledger so a test can assert it executed on every row)."""
    return arm in SEAT and bool(sha) and bool({"BASE": sha_base, "CAND": sha_cand}[arm])


def guard_witness(arm, cell, cli, sha=None, sha_cand=None, sha_base=None):
    """-> (problems, readouts) of plan 54's three witnesses for a plan-41d row. readouts = {hn_p0, snapv_st, snapv_present} for the ledger and the report."""
    sv = s53.snapv(cli)
    st = sum(d["pt_st"] + d["tt_st"] for d in sv.values()) if sv else None
    hold = t52.hold_by_path(cli)
    hn = (hold.get(0) or {}).get("hn_n")
    rd = {"hn_p0": hn, "snapv_st": st, "snapv_present": sv is not None, "sha_checked": sha_checked(arm, sha, sha_cand, sha_base)}
    p = sha_problems(arm, sha, sha_cand, sha_base)
    if arm == "CAND":
        if sv is None:
            p.append("no-[SNAPV]-on-CAND")
        elif st:
            p.append(f"snapv-pt_st+tt_st={st}-on-CAND (want 0)")
        if cell == C8:
            if hn is None:
                p.append("no-[HOLD]-hn-on-CAND-c8")
            elif hn >= HN_COLLAPSE:
                p.append(f"hn-no-collapse={hn}>={HN_COLLAPSE}-on-CAND-c8")
    elif arm == "BASE" and sv is not None:
        p.append("[SNAPV]-on-BASE (main has no gauge: the trees are mixed up)")
    return p, rd


def guard_report(paths, out=print):
    """Plan 54's witness table from a plan-41d ledger run with LD_WITNESS54=1: per (cell, arm) the rows, the hn / [SNAPV] readings and the voided rows."""
    import place38_parse as pp
    rows, _ = pp.rows_of(paths)
    out("WITNESS54 (plan 54's liveness witnesses on the guard's rows; the verdict above is score41d's and does not read this)")
    for cell in ("c1s-400", "c1d-400", "c2-100", C8):
        for arm in ("BASE", "CAND"):
            rs = [r for r in rows if r["cell"] == cell and r["arm"] == arm]
            if not rs:
                continue
            w = [r.get("w54") or {} for r in rs]
            hn = [x.get("hn_p0") for x in w if x.get("hn_p0") is not None]
            bad = [r for r in rs if any(pr.startswith(("bin-sha", "no-[SNAPV]", "snapv-", "hn-", "no-[HOLD]", "[SNAPV]-on-BASE")) for pr in r["problems"])]
            out(f"  {cell:<8} {arm:<5} rows {len(rs):>2}  witness-void {len(bad)}  p0 hn_n " + (f"median {lc.med(hn):.0f} [{min(hn)}-{max(hn)}]" if hn else "-")
                + f"  [SNAPV] pt_st+tt_st {sorted({x.get('snapv_st') for x in w})}")
    for r in rows:
        if any(pr.startswith(("bin-sha", "no-[SNAPV]", "snapv-", "hn-", "no-[HOLD]", "[SNAPV]-on-BASE")) for pr in r["problems"]):
            out(f"  VOID {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']} status={r['status']}: {' '.join(r['problems'])}")


def smoke54(path, out=print):
    """Plan 54's rehearsal: plan 41d's smoke (three rows complete, the 600 MB c8 row carries its size and the in-order delay) with the criteria printed, then the witnesses."""
    import place38_parse as pp
    rows, _ = pp.rows_of([path])
    got = {(r["cell"], r["arm"]): r for r in rows}
    bad = []

    def crit(name, fails):
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)

    want = [(c, a) for c, _, a, _, _ in pp.SMOKE41D]
    out(f"SMOKE54 rows={len(rows)}/{len(want)} (plan 41d's smoke: its two rows + the 600 MB c8 CAND row; the values are not read, only the criteria below)")
    crit("every smoke row ran", [f"{c} {a}: no row" for c, a in want if (c, a) not in got])
    rs = [got[k] for k in want if k in got]
    for r in rs:
        out(f"  ROW {r['cell']} {r['arm']}: status={r['status']} mbps={r.get('mbps')} wall={r.get('wall_s')} bytes={r.get('bytes')} last-tick={r.get('tick_span_s')} "
            f"inorder_mean_ms={r.get('inorder_mean')} w54={r.get('w54')}")
    crit("no row is VOID-SHORT (a 600 MB c8 row carries >= 30 s of [DIAG] ticks with src= and cum=)", [f"{r['cell']} {r['arm']}: {r['status']}" for r in rs if r["status"] == "VOID-SHORT"])
    buf = io.StringIO()
    with redirect_stdout(buf):
        rc41 = pp.smoke(path, plan41c=True, plan41d=True)
    lines41 = [x for x in buf.getvalue().splitlines() if x]
    for x in lines41:
        if not x.startswith(("SMOKE-PASS", "SMOKE-FAIL")):
            out("  " + x)
    crit("every smoke row is LIVE with the readouts the guard reads, and the 600 MB c8 row carries its size and the in-order delay end to end (plan 41d's own smoke check)",
         [] if rc41 == 0 else [x for x in lines41 if x.startswith("SMOKE-FAIL")] or ["plan 41d's smoke failed"])
    crit("bin_sha matches the row's arm (BASE rows the BASE build, CAND rows the CAND build)", [f"{r['cell']} {r['arm']}: {x}" for r in rs for x in r["problems"] if x.startswith("bin-sha")])
    crit("CAND c8 row: p0 [HOLD] hn_n collapsed (< %d); BASE c8 row: hn reported" % HN_COLLAPSE, [f"{r['cell']} {r['arm']}: {x}" for r in rs for x in r["problems"] if x.startswith(("hn-", "no-[HOLD]"))])
    crit("[SNAPV] present on every CAND row with pt_st + tt_st = 0 (and absent on BASE)", [f"{r['cell']} {r['arm']}: {x}" for r in rs for x in r["problems"] if x.startswith(("no-[SNAPV]", "snapv-", "[SNAPV]-on-BASE"))])
    if bad:
        out("SMOKE-FAIL " + "; ".join(bad))
        return 1
    out("SMOKE-PASS")
    return 0


# ── plan 54b: the liveness ──
def pool_problems(arm, cli, srv):
    """The knob's witness on both ends: [GATES] RWM_STORE_PATH_POOL=<requested n> (an unparsable value silently falls back to 2048, so a reduced arm echoing 2048 is a failure) and
    the sender's INFO `pool_per_path=<n>` (the server's too when it prints one)."""
    if arm not in ARM_POOL54:
        return [f"unknown-arm-{arm}"]
    want = ARM_POOL54[arm][1]
    p = []
    for side, lines in (("cli", cli), ("srv", srv)):
        tok, info = pool_of(lines)
        if tok != str(want):
            p.append(f"gate-pool-{side}={tok} (want {want})")
        if side == "cli" and info != want:
            p.append(f"pool_per_path-{side}={info} (want {want})")
        if side == "srv" and info is not None and info != want:
            p.append(f"pool_per_path-{side}={info} (want {want})")
    return p


def row_problems54b(arm, cli, srv, sha=None, sha_cand=None, sha_base=None):
    """Plan 54b's row witnesses: the pool, and the binary seat (B* = the BASE seat = cand54, L* = the CAND seat = levb54)."""
    return pool_problems(arm, cli, srv) + sha_problems(ARM_POOL54.get(arm, (None,))[0], sha, sha_cand, sha_base)


def tail_witness(arm, t):
    """Printed per row, never voided: the binary witness at W = 4096 (B tail >= 2 s, L tail <= 0.8 s); at a reduced W the tail is an outcome."""
    tail = (t or {}).get("tail_s")
    if tail is None or ARM_POOL54.get(arm, (None, 0))[1] != DEFAULT_POOL:
        return None
    return tail >= 2.0 if arm.startswith("B") else tail <= 0.8


# ── the registered readouts ──
READOUTS = [
    ("goodput Mbit/s", lambda r, t: t.get("goodput_mbps"), 1),
    ("completion tail s", lambda r, t: t.get("tail_s"), 2),
    ("H0 holes at source end", lambda r, t: t.get("H0"), 0),
    ("frontier lag at source end (symbols)", lambda r, t: t.get("lag_end"), 0),
    ("receiver duplicates", lambda r, t: t.get("rx_dup"), 0),
    ("retransmits per GB", lambda r, t: t.get("retx_per_gb"), 0),
    ("client CPU s/GB", lambda r, t: t.get("cpu_cli_s_per_gb"), 2),
    ("server CPU s/GB", lambda r, t: t.get("cpu_srv_s_per_gb"), 2),
    ("sum tx rate, LATE, /s", lambda r, t: t.get("tx_sum_late"), 0),
    ("p0 backlog q0 = dgq0 hand - tx, LATE median", lambda r, t: t.get("q0_late"), 0),
    ("p0 app-echo rtt0 ms, LATE median", lambda r, t: t.get("rtt0_late"), 0),
    ("rtt0 - wrtt0 ms, LATE median", lambda r, t: t.get("rtt0_minus_wrtt0"), 0),
    ("quinn evictions dgq0 full (last [DIAG])", lambda r, t: t.get("evict_full_p0"), 0),
    ("quinn evictions dgq1 full (last [DIAG])", lambda r, t: t.get("evict_full_p1"), 0),
    ("quinn evictions, both paths (last [DIAG])", lambda r, t: t.get("evict_full"), 0),
    ("dgq0 hand - tx (last [DIAG])", lambda r, t: t.get("hand_minus_tx_p0"), 0),
    ("dgq1 hand - tx (last [DIAG])", lambda r, t: t.get("hand_minus_tx_p1"), 0),
    ("inorder_mean_run s (whole run)", lambda r, t: t.get("inorder_mean_run"), 2),
    ("inorder_p95_run s", lambda r, t: t.get("inorder_p95_run"), 2),
    ("inorder_mean_late s (sending ticks t >= 15 s)", lambda r, t: t.get("inorder_mean_late"), 2),
    ("inorder_p50_late s", lambda r, t: t.get("inorder_p50_late"), 2),
    ("inorder_p95_late s", lambda r, t: t.get("inorder_p95_late"), 2),
    ("old holes (server [RANK], readouts 15..-6)", lambda r, t: t.get("old_holes"), 0),
    ("budget residual cap - sum r_i*rtt_i, LATE", lambda r, t: t.get("resid_late"), 0),
    ("budget residual / W", lambda r, t: t.get("resid_rel"), 3),
    ("cap W (LATE median)", lambda r, t: t.get("cap_med"), 0),
    ("cap bind fraction (b = 2*pool)", lambda r, t: t.get("bind_frac"), 3),
    ("[LAT] coverage sum pN:n / n", lambda r, t: t.get("lat_cov"), 3),
    ("[HOLD] p0 hn_n", lambda r, t: t.get("hn_p0"), 0),
]
for _g_ in ("0", "1"):
    READOUTS += [
        (f"[SNAPV] p{_g_} rep", lambda r, t, g=_g_: s53._leg(t.get("snapv"), g, "rep"), 0),
        (f"[SNAPV] p{_g_} stale", lambda r, t, g=_g_: s53._leg(t.get("snapv"), g, "stale"), 0),
        (f"[SNAPV] p{_g_} pt_st+tt_st", lambda r, t, g=_g_: (None if s53._leg(t.get("snapv"), g, "pt_st") is None else s53._leg(t.get("snapv"), g, "pt_st") + s53._leg(t.get("snapv"), g, "tt_st")), 0),
    ]
# The registered readouts the rehearsal requires (no None): the block table's and the readout list's.
REQUIRED = ["inorder_mean_run", "inorder_mean_late", "inorder_p50_late", "inorder_p95_late", "lag_end", "evict_full_p0", "evict_full_p1", "hand_minus_tx_p0", "hand_minus_tx_p1",
            "old_holes", "resid_late", "resid_rel", "q0_late", "rtt0_late", "goodput_mbps", "rx_dup", "retx_per_gb", "cpu_cli_s_per_gb"]


def missing(r):
    """The registered block readouts that read None on a row, [SNAPV] included. H0 is undefined (not missing) when the tail is shorter than one tick."""
    t = r.get("t54") or {}
    out = [k for k in REQUIRED if t.get(k) is None]
    if t.get("H0") is None and t.get("tail_s") is None:
        out.append("H0")
    if t.get("snapv") is None:
        out.append("snapv")
    return out


def smoke(path, out=print):
    import place38_parse as pp
    rows, _ = pp.rows_of([path])
    got = {r["arm"]: r for r in rows if r["cell"] == C8}
    out(f"SMOKE54B rows={len(rows)}/{len(pp.SMOKE54B)} (one row per block arm, unscored; the values are not read, only the criteria below)")
    bad = []

    def crit(name, fails):
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)

    crit("one row per block arm ran", [f"{a}: no row" for a in ARMS54B if a not in got])
    rs = [got[a] for a in ARMS54B if a in got]
    for r in rs:
        t = r.get("t54") or {}
        out(f"  ROW {r['arm']}: status={r['status']} mbps={r.get('mbps')} wall={r.get('wall_s')} gates={r.get('gate_pool_cli')}/{r.get('gate_pool_srv')} "
            f"pool_per_path={r.get('pool_info_cli')}/{r.get('pool_info_srv')} W={t.get('cap_med')} bind={t.get('bind_frac')} "
            f"q0={t.get('q0_late')} resid={t.get('resid_late')} old_holes={t.get('old_holes')} tail_witness={tail_witness(r['arm'], t)}")
    crit("each arm's pool is in [GATES] on both ends and pool_per_path matches on the sender (and the server when it prints one)",
         [f"{r['arm']}: {' '.join(x for x in r['problems'] if x.startswith(('gate-pool', 'pool_per_path')))}" for r in rs if any(x.startswith(("gate-pool", "pool_per_path")) for x in r["problems"])])
    crit("each row ran the build of its seat (B = BASE tree, L = CAND tree)", [f"{r['arm']}: {x}" for r in rs for x in r["problems"] if x.startswith("bin-sha")])
    crit("[SNAPV] parsed on every row (the gauge is in both builds)", [f"{r['arm']}: no [SNAPV]" for r in rs if (r.get("t54") or {}).get("snapv") is None])
    crit("in-order delay (run and late), evictions, old holes and the budget residual computed (no None)", [f"{r['arm']}: {', '.join(missing(r))}" for r in rs if missing(r)])
    # RWM_PERF_TIMEOUT_S is 300 for plan 54b (B640's H-hold floor of 20 Mbit/s needs 240 s for 600 MB): every row, B640 included, must finish inside it, no exemption.
    late_fail = []
    for r in rs:
        if r.get("dnf") or r.get("rc") != 0 or not r.get("summary"):
            late_fail.append(f"{r['arm']}: rc={r.get('rc')} dnf={r.get('dnf')} summary={r.get('summary')} wall={r.get('wall_s')}")
        elif (r.get("seconds") or 0) >= PERF_TIMEOUT_S:
            late_fail.append(f"{r['arm']}: {r.get('seconds')} s >= {PERF_TIMEOUT_S}")
        if r.get("bytes") != BYTES:
            late_fail.append(f"{r['arm']}: bytes={r.get('bytes')}")
    crit(f"the c8 600 MB rows finished inside RWM_PERF_TIMEOUT_S={PERF_TIMEOUT_S} (rc 0, a summary, not DNF)", late_fail)
    for r in rs:
        out(f"  LIVENESS {r['arm']} s{r['seed']} rep{r['rep']}: {'LIVE' if r['status'] == 'LIVE' else 'VOID'} {' '.join(r['problems'])}"
            f" | p1 stale={s53._leg((r.get('t54') or {}).get('snapv'), '1', 'stale')} tail-witness@W4096={tail_witness(r['arm'], r.get('t54'))}")
    if bad:
        out("SMOKE-FAIL " + "; ".join(bad))
        return 1
    out("SMOKE-PASS")
    return 0


# ── aggregation ──
def _fmt(x, nd):
    return "-" if x is None else f"{x:.{nd}f}"


def values(rows, fn):
    out = []
    for r in rows:
        try:
            x = fn(r, r.get("t54") or {})
        except Exception:
            x = None
        if x is not None:
            out.append(x)
    return out


def arm_stats(rows, fn, nd):
    v = values(rows, fn)
    if not v:
        return "-"
    return f"mean {_fmt(sum(v) / len(v), nd)}  median {_fmt(lc.med(v), nd)}  range [{_fmt(min(v), nd)} - {_fmt(max(v), nd)}]  (n {len(v)})"


def goodput_dnf0(rows):
    """Goodput values of an arm's rows with a DNF read as 0 (a completed row's mbps; a DNF row never counts as missing)."""
    return [0.0 if r.get("dnf") else (r.get("t54") or {}).get("goodput_mbps") for r in rows if r.get("dnf") or (r.get("t54") or {}).get("goodput_mbps") is not None]


def arm_med(rows, fn):
    v = values(rows, fn)
    return lc.med(v) if v else None


def falsifiers(by):
    """-> [(id, text, state, detail)] with state FIRES / NOT-FIRED / UNDEFINED. `by` = {arm: [LIVE ledger rows]}. Goodput medians read a DNF as 0."""
    def gp(a):
        v = goodput_dnf0(by.get(a, []))
        return lc.med(v) if v else None

    def oh(a):
        return arm_med(by.get(a, []), lambda r, t: t.get("old_holes"))
    out = []
    b640, b2048, l640, l2048 = gp("B640"), gp("B2048"), gp("L640"), gp("L2048")
    h640 = oh("B640")
    det = f"B640 goodput {_fmt(b640, 1)} vs 0.95 x B2048 {_fmt(None if b2048 is None else 0.95 * b2048, 1)}; B640 old holes {_fmt(h640, 0)}"
    if None in (b640, b2048, h640):
        out += [(1, "B640 goodput >= 0.95 x B2048 with old holes >= 700: the mechanism is refuted", "UNDEFINED", det),
                (2, "B640 goodput >= 0.95 x B2048 with old holes <= 300: H-elastic (a finding)", "UNDEFINED", det)]
    else:
        ok = b640 >= 0.95 * b2048
        out += [(1, "B640 goodput >= 0.95 x B2048 with old holes >= 700: the mechanism is refuted", "FIRES" if ok and h640 >= 700 else "NOT-FIRED", det),
                (2, "B640 goodput >= 0.95 x B2048 with old holes <= 300: H-elastic (a finding)", "FIRES" if ok and h640 <= 300 else "NOT-FIRED", det)]
    det3 = f"L640 goodput {_fmt(l640, 1)} vs 0.97 x L2048 {_fmt(None if l2048 is None else 0.97 * l2048, 1)}"
    out.append((3, "L640 goodput < 0.97 x L2048: CAND's edge is above 1 280; read L1024 alone", "UNDEFINED" if None in (l640, l2048) else ("FIRES" if l640 < 0.97 * l2048 else "NOT-FIRED"), det3))
    bad, n = [], 0
    for a in ("L2048", "L1024", "L640"):
        for r in by.get(a, []):
            t = r.get("t54") or {}
            if t.get("resid_late") is None or not t.get("cap_med"):
                continue
            n += 1
            if abs(t["resid_late"]) > 0.1 * t["cap_med"]:
                bad.append(f"{a} s{r['seed']} r{r['rep']} resid {t['resid_late']:.0f} W {t['cap_med']:.0f}")
    out.append((4, "|W - sum r_i*rtt_i| > 0.1*W on any LEVB row: the identity fails", "UNDEFINED" if not n else ("FIRES" if bad else "NOT-FIRED"),
                f"{n} L rows read; " + ("; ".join(bad) if bad else "none above 0.1*W")))
    return out


def decision_rule(fal, by):
    """The registered reading (reported, decides nothing): F1 or F4 fires -> refuted; F2 fires -> H-elastic; F1 and F4 not fired and B640 below L640 by more than both arms' ranges -> confirmed."""
    st = {i: s for i, _, s, _ in fal}
    if "FIRES" in (st[1], st[4]):
        return "refuted (falsifier 1 or 4 fires): back to diagnosis"
    if st[2] == "FIRES":
        return "H-elastic (falsifier 2 fires): lever B's gain would come from the oversized pool"
    if "UNDEFINED" in (st[1], st[4]):
        return "undetermined (falsifier 1 or 4 not evaluable)"
    b, l = goodput_dnf0(by.get("B640", [])), goodput_dnf0(by.get("L640", []))
    if b and l and max(b) < min(l):
        return "confirmed (falsifiers 1 and 4 do not fire; B640 below L640 on goodput by more than both arms' ranges)"
    return "no reading (falsifiers 1 and 4 do not fire, but B640 does not sit below L640 outside both ranges)"


# ── the re-basing rule's reference: section 53's BASES / CANDS rows ──
S53_DEFAULT = os.path.join(HERE, "..", "..", "docs", "l1-raw", "s53", "rep53-diag-ld.tgz")
REBASE_KEYS = (("goodput", "goodput_mbps"), ("tail", "tail_s"), ("old holes", "old_holes"), ("duplicates", "rx_dup"))
REF_ARM = {"B2048": "BASES", "L2048": "CANDS"}


def s53_rows(src, arms=("BASES", "CANDS")):
    """Section 53's c8-100 rows of `arms` (from rep53-diag-ld.tgz or an unpacked dir) -> {arm: [metrics]} with THIS module's definitions; {} when the source is absent."""
    if not src or not os.path.exists(src):
        return {}
    out = {a: [] for a in arms}

    def one(d, name):
        m = re.match(re.escape(C8) + r"@def-(\w+)-s(\d+)-r(\d+)-a\d+-c\.log$", name)
        if not m or m.group(1) not in arms:
            return
        base = os.path.join(d, name[:-len("-c.log")])
        out[m.group(1)].append(row_metrics(lc.read(os.path.join(d, name)), lc.read(base + "-s.log"), lc.read(base + "-drv.out"), BYTES, 2048))
    if os.path.isdir(src):
        for n in sorted(os.listdir(src)):
            one(src, n)
        return out
    with tempfile.TemporaryDirectory() as td, tarfile.open(src) as tf:
        for mem in tf.getmembers():
            b = os.path.basename(mem.name)
            if mem.isfile() and b.startswith(C8 + "@def-") and any(f"def-{a}-" in b for a in arms):
                with open(os.path.join(td, b), "wb") as f:
                    f.write(tf.extractfile(mem).read())
        for n in sorted(os.listdir(td)):
            one(td, n)
    return out


def ranges(ms):
    """{key: (min, max)} of REBASE_KEYS and the extra reference readouts over metric dicts; keys with no value are absent."""
    out = {}
    for _, k in REBASE_KEYS + (("q0", "q0_late"), ("evictions", "evict_full"), ("inorder_mean_late", "inorder_mean_late"), ("resid", "resid_late"), ("hn", "hn_p0")):
        v = [m[k] for m in ms if m.get(k) is not None]
        if v:
            out[k] = (min(v), max(v))
    return out


def rebase_check(by, src, out):
    """The re-basing rule: a control row (B2048 against BASES, L2048 against CANDS) outside its section-53 reference range on goodput, tail, old holes or duplicates means the
    reduced-W bands are re-based on the new control before reading. -> True when re-basing is needed; None when the reference is unavailable."""
    ref = s53_rows(src)
    if not ref or not any(ref.values()):
        out(f"REBASE-CHECK UNAVAILABLE: no section-53 logs at {src}")
        return None
    needed = False
    for arm, rarm in REF_ARM.items():
        rng = ranges(ref[rarm])
        rs = by.get(arm, [])
        out(f"REBASE {arm} against section 53's {rarm} (n {len(ref[rarm])} rows), ranges: " + "  ".join(f"{k} [{_fmt(lo, 2)} - {_fmt(hi, 2)}]" for k, (lo, hi) in rng.items()))
        for r in rs:
            t = r.get("t54") or {}
            outside = []
            for label, k in REBASE_KEYS:
                v = 0.0 if (k == "goodput_mbps" and r.get("dnf")) else t.get(k)
                if k in rng and v is not None and not (rng[k][0] <= v <= rng[k][1]):
                    outside.append(f"{label} {_fmt(v, 2)} outside [{_fmt(rng[k][0], 2)} - {_fmt(rng[k][1], 2)}]")
            if outside:
                needed = True
                out(f"  OUTSIDE {arm} s{r['seed']} rep{r['rep']}: " + "; ".join(outside))
        if not rs:
            out(f"  {arm}: no live rows")
    out("REBASE-NEEDED: " + ("YES (a control row lands outside its section-53 range: re-base the reduced-W bands on the new control before reading, and report it)" if needed
                             else "NO (every control row inside its section-53 range on goodput, tail, old holes and duplicates)"))
    return needed


def rows_table(rows, out):
    out("  per-row table (LIVE and DNF rows): arm seed rep | goodput | tail | old holes | q0 late | rtt0 | resid/W | inorder late | evict | stale p1 | tail-witness@4096")
    for r in sorted(rows, key=lambda x: (x["arm"], x["seed"], x["rep"])):
        t = r.get("t54") or {}
        out(f"    {r['arm']:<6} s{r['seed']:<3} r{r['rep']} | {'DNF' if r.get('dnf') else _fmt(t.get('goodput_mbps'), 1)} | {_fmt(t.get('tail_s'), 2)} | {_fmt(t.get('old_holes'), 0)} | "
            f"{_fmt(t.get('q0_late'), 0)} | {_fmt(t.get('rtt0_late'), 0)} | {_fmt(t.get('resid_rel'), 3)} | {_fmt(t.get('inorder_mean_late'), 2)} | {t.get('evict_full')} | "
            f"{s53._leg(t.get('snapv'), '1', 'stale')} | {tail_witness(r['arm'], t)}")


def score(paths, out=print, s53_src=S53_DEFAULT):
    import place38_parse as pp
    rows_all, tokens = pp.rows_of(paths)
    aborted = sorted({t for t in tokens if t in pp.ABORT_TOKENS})
    rows = [r for r in rows_all if r["cell"] == C8 and r["arm"] in ARMS54B]
    n_exp = pp.expect_rows54b()
    out("STORE-BUDGET BLOCK (status 54), REPORTED ONLY: c8-100 at 600 MB, RWM_DIAG=1, arms B2048/B1024/B640 on the BASE seat (main + snapshot-order law) and L2048/L1024/L640 on the "
        "CAND seat (+ lever B), RWM_STORE_PATH_POOL = default / 1024 / 640; nothing here is scored")
    out(f"ROWS {len(rows)}/{n_exp}; ABORT-TOKENS {' '.join(aborted) or 'none'}")
    out("LIVENESS per arm (the pool echo on both ends and pool_per_path decide; a DNF row is LIVE and counts)")
    for a in ARMS54B:
        rs = [r for r in rows if r["arm"] == a]
        c = {}
        for r in rs:
            c[r["status"]] = c.get(r["status"], 0) + 1
        out(f"  {a:<6} rows {len(rs)}  " + (" ".join(f"{k}={v}" for k, v in sorted(c.items())) or "no rows") + f"  DNF {sum(1 for r in rs if r.get('dnf'))}")
    for r in rows:
        if r["status"] != "LIVE":
            out(f"  NONLIVE {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} rc={r['rc']} {' '.join(r['problems'])}")
        if r.get("t54_error"):
            out(f"  T54-ERROR {r['arm']} s{r['seed']} rep{r['rep']}: {r['t54_error']}")
    out("WITNESS (pool in [GATES] client/server, pool_per_path client/server): " + "; ".join(
        f"{a} {sorted({str(r.get('gate_pool_cli')) + '/' + str(r.get('gate_pool_srv')) + ' ' + str(r.get('pool_info_cli')) + '/' + str(r.get('pool_info_srv')) for r in rows if r['arm'] == a})}"
        for a in ARMS54B))
    live = [r for r in rows if r["status"] == "LIVE"]
    by = {a: [r for r in live if r["arm"] == a] for a in ARMS54B}
    out("")
    for a in ARMS54B:
        rs = by[a]
        out(f"-- {a}: n = {len(rs)} counting rows (DNF {sum(1 for r in rs if r.get('dnf'))}); goodput incl. DNF as 0: "
            + (f"median {_fmt(lc.med(goodput_dnf0(rs)), 1)}" if goodput_dnf0(rs) else "-"))
        for label, fn, nd in READOUTS:
            out(f"  {label:<46} {arm_stats(rs, fn, nd)}")
        tw = [tail_witness(a, r.get("t54")) for r in rs]
        out(f"  tail witness at W = 4096 (B tail >= 2 s, L tail <= 0.8 s), printed not voided: " + ("n/a at a reduced W" if ARM_POOL54[a][1] != DEFAULT_POOL else f"{sum(1 for x in tw if x)}/{len([x for x in tw if x is not None])} rows hold"))
    out("")
    rows_table(live, out)
    out("")
    out("FALSIFIERS (registered; goodput medians read a DNF as 0):")
    fal = falsifiers(by)
    for i, text, state, det in fal:
        out(f"  F{i} {state:<10} {text} -- {det}")
    out(f"DECISION-RULE reading (reported only; the block decides nothing): {decision_rule(fal, by)}")
    out("")
    rebase_check(by, s53_src, out)
    out(f"VERDICT54B REPORTED-ONLY rows={len(rows)}/{n_exp} live={len(live)} dnf={sum(1 for r in live if r.get('dnf'))} falsifiers=" + ",".join(f"F{i}:{s}" for i, _, s, _ in fal))


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    src = S53_DEFAULT
    if "--s53" in a:
        i = a.index("--s53")
        src = a[i + 1]
        a = a[:i] + a[i + 2:]
    if cmd == "score":
        score(a, s53_src=src)
        return 0
    if cmd == "smoke":
        return smoke(a[0])
    if cmd == "smoke54":
        return smoke54(a[0])
    if cmd == "guard":
        guard_report(a)
        return 0
    if cmd == "plan":
        import place38_parse as pp
        for c, s_, ar, seed, rep in pp.plan54b(smoke=bool(a) and a[0] == "smoke"):
            print(f"{c} {s_} {ar} {seed} {rep}")
        return 0
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
