#!/usr/bin/env python3
"""Parser and scorer for THE ONE-STORE-LAW BATTERY (docs/status.md, "18. One
store law at every path count (ADR-0072)", pre-registration as amended).

One binary. Arms (parser names): LAW = shipped defaults; CTL =
RWM_STORE_PATHS=0 (scored at c1s-400 only); LAW-auto / LAW-realtime = the
delta leg (LAW at another dial point).

    storelaw_parse.py row   <cell> <arm> <seed> <rep> <rc> <wall_s> <drv> <cli> <srv> [cotenant] [sha]
    storelaw_parse.py smoke <ledger>
    storelaw_parse.py check <ledger>
    storelaw_parse.py score <ledger>...        (section 18)
    storelaw_parse.py score19 <ledger>...      (section 19: MAIN = ce60079 vs LAW, same session)

Scoring constants (one definition each, restated in the pre-registration):
"""
import json
import math
import os
import re
import statistics
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import stage3_parse as sp  # noqa: E402

for _a, _h in (("LAW", "bulk"), ("CTL", "bulk"), ("MAIN", "bulk"), ("LAW-auto", "auto"), ("LAW-realtime", "realtime")):
    sp.ARM_SPEC[_a] = ("window", "Rlc", _h, "ACTIVE")

PIN_MIN = 0.25            # PINNED: LAW [SUMCAP] pin >= this
SIGMA_K = 2.0             # the noise band: 2 x sigma_pooled
MIN_LIVE = 4              # live rows per scored (cell, arm), else UNSCOREABLE
WITNESS_FAIL_LIMIT = 2    # witness-failed rows that void an (arm, cell)
Q_OF = {"bulk": 0.1, "auto": 1.0 / 15.0, "realtime": 0.05}
WARM_S = 1.0              # DIAG windows before this are warm-up
# §15 MAIN goodput ranges (status.md, same cells, shipped default): the
# CONTROL-MOVED reference at the N >= 2 cells.
MAIN15 = {"c1d-400": (677.9, 755.1), "c2-100": (90.08, 90.32), "c8-100": (82.9, 102.9)}
SCORED_CELL = "c1s-400"
NOOP_CELLS = ["c1d-400", "c2-100", "c8-100"]      # section 18's control-moved leg, as scored there
# Section 19 (as amended): c2-100 is a SINGLE-path cell (stage3_parse.CELL_SPEC: c2/c2/single), so the
# change applies there; the N >= 2 no-op cells are c1d and c8 only.
SINGLE_CELLS = ["c1s-400", "c2-100"]
NOOP19_CELLS = ["c1d-400", "c8-100"]

_SUMCAP = re.compile(r"\[SUMCAP\] on=(\d) eng=(\d+)/(\d+) chg=(\d+)/(\d+) chg_frac=([\d.]+) "
                     r"pin=([\d.]+) floor=([\d.]+) cap=([\d.]+) ask=([\d.]+)")
_DCAP = re.compile(r"\[DCAP\] on=(\d) eng=(\d+)/(\d+) .*?q=([\d.]+) b=([\d.]+)")
_WIN = re.compile(r"\bwin=(\d+)/(\d+)")
_T = re.compile(r"\[DIAG\] t=([\d.]+)s")
_PAUSED = re.compile(r"\bpaused=(\d+)%")
_RTT = re.compile(r"\brtt=([\d.]+)ms")
_BTL = re.compile(r"\bbtlbw=(\d+)")
_RTPUS = re.compile(r"\brtp_us=(\d+)")
_AX = re.compile(r"ax_p50=(\d+) ax_p90=(\d+) ax_p95=(\d+)")
_GATE_SP = re.compile(r"RWM_STORE_PATHS=(\d)")


def make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant=0, sha=None):
    row = sp.make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant)
    row["bin_sha"] = sha
    want_sp = 0 if arm == "CTL" else 1
    gs = [_GATE_SP.search(ln) for ln in cli if "[GATES]" in ln]
    gs = [m for m in gs if m]
    row["gate_store_paths"] = int(gs[-1].group(1)) if gs else None
    row["echo_pool"] = any("path-scaled outstanding pool ACTIVE" in ln for ln in cli)
    cap = [m for m in (_SUMCAP.search(ln) for ln in cli) if m]
    if cap:
        m = cap[-1]
        row["sumcap_eng"], row["sumcap_refresh"] = int(m.group(2)), int(m.group(3))
        row["sumcap_pin"], row["sumcap_floor"] = float(m.group(7)), float(m.group(8))
        row["sumcap_cap"], row["sumcap_ask"] = float(m.group(9)), float(m.group(10))
    else:
        row["sumcap_eng"] = row["sumcap_refresh"] = None
    dc = [m for m in (_DCAP.search(ln) for ln in cli) if m]
    row["dcap_q"] = float(dc[-1].group(4)) if dc else None
    row["dcap_eng"] = int(dc[-1].group(2)) if dc else None
    wins, paused, rtt, btl, occ = [], [], [], [], []
    for ln in cli:
        mt = _T.search(ln)
        if not mt or float(mt.group(1)) < WARM_S:
            continue
        mw = _WIN.search(ln)
        if mw:
            wins.append(int(mw.group(2)))
            if int(mw.group(2)) > 0:
                occ.append(int(mw.group(1)) / int(mw.group(2)))
        for rx, acc in ((_PAUSED, paused), (_RTT, rtt)):
            mm = rx.search(ln)
            if mm:
                acc.append(float(mm.group(1)))
        mb = _BTL.search(ln)
        if mb:
            btl.append(int(mb.group(1)))
    row["win_cap_n"] = len(wins)
    row["win_cap_distinct"] = len(set(wins))
    row["win_cap_med"] = lc.med(wins) if wins else None
    row["win_cap_max"] = max(wins) if wins else None
    row["occ_med"] = lc.med(occ) if occ else None
    row["paused_med"] = lc.med(paused) if paused else None
    row["rtt_med"] = lc.med(rtt) if rtt else None
    row["btlbw_med"] = lc.med(btl) if btl else None
    mbps = row.get("mbps")
    # btlbw over-read: the anchor (sym/s) against the delivered rate (sym/s at 1200 B)
    row["btlbw_overread"] = (row["btlbw_med"] / (mbps * 1e6 / 8 / 1200)
                             if row["btlbw_med"] and mbps else None)
    ax = [m for m in (_AX.search(ln) for ln in srv if "[LAT]" in ln) if m]
    if ax:
        row["ax_p50_us"], row["ax_p95_us"] = int(ax[-1].group(1)), int(ax[-1].group(3))
    else:
        row["ax_p50_us"] = row["ax_p95_us"] = None
    p = []
    if row["gate_store_paths"] != want_sp:
        p.append(f"gate-store-paths={row['gate_store_paths']}")
    if arm == "CTL":
        if row["sumcap_eng"] not in (None, 0):
            p.append(f"ctl-sumcap-eng={row['sumcap_eng']}")
    elif arm == "MAIN":
        # §19: main's single-path chain at c1s (eng=0/0, win constant 1024); the
        # same pooled law as LAW at every N >= 2 cell.
        if cell in SINGLE_CELLS:
            if row["sumcap_eng"] not in (None, 0):
                p.append(f"main-sumcap-eng={row['sumcap_eng']}")
            # main's single-path chain clamp(2*bdp, 10, 1024): the constant 1024 at c1s (the anchor
            # over-reads), up to 1024 and varying with the anchor at c2 (paper 6.2: 989-1024).
            if row["win_cap_n"] < 4 or row["win_cap_max"] is None or row["win_cap_max"] > 1024:
                p.append(f"main-win-above-1024={row['win_cap_max']}")
            if cell == "c1s-400" and (row["win_cap_distinct"] != 1 or row["win_cap_med"] != 1024):
                p.append(f"main-win-not-constant-1024={row['win_cap_med']}/{row['win_cap_distinct']}")
        else:
            if not row["sumcap_eng"]:
                p.append("main-sumcap-eng-zero-at-N2")
    else:
        if not row["sumcap_eng"]:
            p.append("law-sumcap-eng-zero")
        if row["win_cap_n"] >= 4 and row["win_cap_distinct"] < 2:
            p.append("law-win-constant")
        if not row["echo_pool"]:
            p.append("pool-echo-missing")
    row["problems"].extend(p)
    # A named hint (realtime) never prints the auto-selection line; accept it from
    # `[PIPE] pipeline=window hint=<h>` (already checked by sp.make_row against the
    # arm's hint) plus the `[DCAP] q=` echo showing the dial reached the seat.
    want_hint = sp.ARM_SPEC[arm][2]
    if (want_hint != "auto" and "window-line-missing-on-window" in row["problems"]
            and row.get("pipe_cli") and row["pipe_cli"][0] == "window"
            and row.get("pipe_srv") and row["pipe_srv"][0] == "window"
            and row.get("dcap_q") is not None):
        row["problems"].remove("window-line-missing-on-window")
        if row["status"] == "WITNESS-FAIL" and not row["problems"]:
            row["status"] = "LIVE"
    if row["problems"] and row["status"] == "LIVE":
        row["status"] = "WITNESS-FAIL"
    return row


def rows_of(paths):
    rows = []
    for path in paths:
        for ln in lc.read(path):
            if ln.startswith("SLROW "):
                try:
                    rows.append(json.loads(ln[len("SLROW "):]))
                except ValueError:
                    pass
    return rows


def mean(v):
    return sum(v) / len(v) if v else None


def sd(v):
    return statistics.stdev(v) if len(v) > 1 else 0.0


def f(x, nd=1):
    return "-" if x is None else f"{x:.{nd}f}"


def live(rows, cell, arm):
    return [r for r in rows if r["cell"] == cell and r["arm"] == arm and r["status"] == "LIVE"]


def col(rs, key):
    return [r[key] for r in rs if r.get(key) is not None]


def score(paths, out=print):
    rows = rows_of(paths)
    out("STORE-LAW BATTERY SCORE (pre-registration: status.md §18, as amended)")
    out("")
    out("ROWS by (cell, arm): live / total / statuses")
    keys = sorted({(r["cell"], r["arm"]) for r in rows})
    for c, a in keys:
        rs = [r for r in rows if r["cell"] == c and r["arm"] == a]
        st = {}
        for r in rs:
            st[r["status"]] = st.get(r["status"], 0) + 1
        out(f"  {c} {a}: live {len([r for r in rs if r['status']=='LIVE'])}/{len(rs)} {st}")
    out("")
    out("PER-ROW (seed rep: mbps pin floor cap win_cap_med occ paused rtt btlbw_overread ax_p50_us ax_p95_us status)")
    for r in sorted(rows, key=lambda r: (r["cell"], r["arm"], r["seed"], r["rep"])):
        out(f"  {r['cell']} {r['arm']} s{r['seed']} r{r['rep']}: mbps={f(r.get('mbps'))} "
            f"pin={f(r.get('sumcap_pin'),3)} floor={f(r.get('sumcap_floor'),3)} cap={f(r.get('sumcap_cap'),1)} "
            f"ask={f(r.get('sumcap_ask'),1)} win={f(r.get('win_cap_med'),0)} occ={f(r.get('occ_med'),2)} "
            f"paused={f(r.get('paused_med'),0)} rtt={f(r.get('rtt_med'))} over={f(r.get('btlbw_overread'),1)} "
            f"ax50={r.get('ax_p50_us')} ax95={r.get('ax_p95_us')} {r['status']} {','.join(r['problems'])}")
    out("")
    verdict = []
    # ── scored leg: c1s-400 LAW vs CTL ───────────────────────────────────
    L, C = live(rows, SCORED_CELL, "LAW"), live(rows, SCORED_CELL, "CTL")
    out(f"SCORED c1s-400: LAW n={len(L)} CTL n={len(C)}")
    unscoreable = len(L) < MIN_LIVE or len(C) < MIN_LIVE
    c1s = {"refuted": False}
    if unscoreable:
        out("  UNSCOREABLE (fewer than %d live rows in an arm)" % MIN_LIVE)
        verdict.append("UNSCOREABLE")
    else:
        gl, gc = col(L, "mbps"), col(C, "mbps")
        sig = math.sqrt((sd(gl) ** 2 + sd(gc) ** 2) / 2)
        d = mean(gl) - mean(gc)
        al, ac = col(L, "ax_p50_us"), col(C, "ax_p50_us")
        asig = math.sqrt((sd(al) ** 2 + sd(ac) ** 2) / 2) if al and ac else None
        ad = (mean(al) - mean(ac)) if al and ac else None
        pin = mean(col(L, "sumcap_pin"))
        out(f"  goodput LAW {f(mean(gl))} CTL {f(mean(gc))} (means; ranges "
            f"{f(min(gl))}-{f(max(gl))} / {f(min(gc))}-{f(max(gc))}) diff {f(d)} sigma_pooled {f(sig,2)} band 2s={f(SIGMA_K*sig,2)}")
        out(f"  A_x p50 us LAW {f(mean(al),0)} CTL {f(mean(ac),0)} diff {f(ad,0)} sigma_pooled {f(asig,0)}")
        out(f"  LAW pin {f(pin,3)} floor {f(mean(col(L,'sumcap_floor')),3)}; CTL win_cap_med {f(mean(col(C,'win_cap_med')),0)}")
        out(f"  btlbw over-read (anchor / delivered) LAW {f(mean(col(L,'btlbw_overread')),1)} CTL {f(mean(col(C,'btlbw_overread')),1)}")
        out(f"  rtt ms LAW {f(mean(col(L,'rtt_med')))} CTL {f(mean(col(C,'rtt_med')))}; paused% LAW {f(mean(col(L,'paused_med')),0)} CTL {f(mean(col(C,'paused_med')),0)}; occ LAW {f(mean(col(L,'occ_med')),2)} CTL {f(mean(col(C,'occ_med')),2)}")
        holds = []
        if d < -SIGMA_K * sig:
            holds.append("REFUTED")
            c1s["refuted"] = True
        if pin is not None and pin >= PIN_MIN:
            holds.append("PINNED")
        if abs(d) <= SIGMA_K * sig and ad is not None and asig is not None and ad > SIGMA_K * asig:
            holds.append("LATENCY-COST")
        if d > SIGMA_K * sig:
            holds.append("GOODPUT-UP")
        if abs(d) <= SIGMA_K * sig and ad is not None and asig is not None and abs(ad) <= SIGMA_K * asig \
                and (pin is None or pin < PIN_MIN):
            holds.append("INERT-AS-DERIVED")
        out(f"  c1s outcomes that hold: {holds or ['OUTSIDE THE PRE-REGISTERED SET']}")
        first = next((h for h in ("REFUTED", "PINNED", "LATENCY-COST", "GOODPUT-UP", "INERT-AS-DERIVED") if h in holds),
                     "OUTSIDE THE PRE-REGISTERED SET")
        out(f"  c1s verdict (first in order REFUTED, PINNED, LATENCY-COST, GOODPUT-UP, INERT-AS-DERIVED): {first}")
        verdict.append(first)
        c1s["first"] = first
    out("")
    # ── control-moved leg ────────────────────────────────────────────────
    moved_any = False
    for cell in NOOP_CELLS:
        rs = live(rows, cell, "LAW")
        lo, hi = MAIN15[cell]
        if len(rs) < MIN_LIVE:
            out(f"CONTROL {cell}: UNSCOREABLE (live {len(rs)})")
            continue
        g = col(rs, "mbps")
        med = statistics.median(g)
        eng_ok = all(r.get("sumcap_eng") and r["sumcap_eng"] == r["sumcap_refresh"] for r in rs)
        in_range = lo <= med <= hi
        moved = (not eng_ok) or (not in_range)
        moved_any = moved_any or moved
        out(f"CONTROL {cell}: n={len(rs)} goodput median {f(med)} [{f(min(g))}-{f(max(g))}] vs §15 MAIN [{lo}-{hi}] "
            f"in_range={in_range}; [SUMCAP] eng=N/N {eng_ok}; pin {f(mean(col(rs,'sumcap_pin')),3)} "
            f"floor {f(mean(col(rs,'sumcap_floor')),3)} cap {f(mean(col(rs,'sumcap_cap')),1)} -> "
            f"{'CONTROL-MOVED' if moved else 'not moved'}")
    out("")
    # ── delta leg ────────────────────────────────────────────────────────
    out("DELTA LEG (LAW at other dial points; witness: [DCAP] q reaches the seat; [SUMCAP] ceil identity when interior)")
    for cell in (SCORED_CELL, "c2-100"):
        for hint in ("auto", "realtime"):
            rs = live(rows, cell, f"LAW-{hint}")
            q = Q_OF[hint]
            for r in rs:
                qd = r.get("dcap_q")
                interior = r.get("sumcap_pin", 1) == 0 and r.get("sumcap_floor", 1) == 0
                ceil_ok = (0 <= r["sumcap_cap"] - r["sumcap_ask"] < 1) if r.get("sumcap_cap") is not None else None
                out(f"  {cell} {hint} s{r['seed']} r{r['rep']}: dcap_q={qd} (want {q:.4f}) "
                    f"ok={qd is not None and abs(qd-q) < 5e-4} pin={f(r.get('sumcap_pin'),3)} "
                    f"cap-ask={f((r.get('sumcap_cap') or 0)-(r.get('sumcap_ask') or 0),2)} interior={interior} ceil_ok={ceil_ok}")
            if not rs:
                out(f"  {cell} {hint}: no live rows")
    out("")
    refuted = c1s["refuted"] or moved_any
    out("DECISION RULE: the law ships unless REFUTED. REFUTED = c1s goodput below CTL by more than 2 sigma_pooled, "
        "or CONTROL-MOVED at any N >= 2 cell.")
    out(f"RESULT: {'REFUTED' if refuted else 'NOT REFUTED -> law ships'}; c1s: {verdict}")
    return 0


def contrast(rows, cell, a, b, out):
    """LAW (a) vs reference (b) at one cell: goodput, A_x, pin, outcome list."""
    A, B = live(rows, cell, a), live(rows, cell, b)
    res = {"cell": cell, "n": (len(A), len(B)), "unscoreable": len(A) < MIN_LIVE or len(B) < MIN_LIVE}
    out(f"{cell}: {a} n={len(A)} {b} n={len(B)}")
    if res["unscoreable"]:
        out("  UNSCOREABLE (fewer than %d live rows in an arm)" % MIN_LIVE)
        return res
    ga, gb = col(A, "mbps"), col(B, "mbps")
    sig = math.sqrt((sd(ga) ** 2 + sd(gb) ** 2) / 2)
    d = mean(ga) - mean(gb)
    xa, xb = col(A, "ax_p50_us"), col(B, "ax_p50_us")
    xsig = math.sqrt((sd(xa) ** 2 + sd(xb) ** 2) / 2) if xa and xb else None
    xd = (mean(xa) - mean(xb)) if xa and xb else None
    pin = mean(col(A, "sumcap_pin"))
    out(f"  goodput {a} {f(mean(ga))} [{f(min(ga))}-{f(max(ga))}] median {f(statistics.median(ga))}; "
        f"{b} {f(mean(gb))} [{f(min(gb))}-{f(max(gb))}] median {f(statistics.median(gb))}; "
        f"diff {f(d)} sigma_pooled {f(sig,2)} 2s={f(SIGMA_K*sig,2)}")
    out(f"  A_x p50 us {a} {f(mean(xa),0)} {b} {f(mean(xb),0)} diff {f(xd,0)} sigma_pooled {f(xsig,0)}")
    out(f"  {a} pin {f(pin,3)} floor {f(mean(col(A,'sumcap_floor')),3)} cap {f(mean(col(A,'sumcap_cap')),1)}; "
        f"{b} pin {f(mean(col(B,'sumcap_pin')),3)} cap {f(mean(col(B,'sumcap_cap')),1)} win {f(mean(col(B,'win_cap_med')),0)}")
    out(f"  rtt ms {a} {f(mean(col(A,'rtt_med')))} {b} {f(mean(col(B,'rtt_med')))}; paused% {f(mean(col(A,'paused_med')),0)}/{f(mean(col(B,'paused_med')),0)}; "
        f"occ {f(mean(col(A,'occ_med')),2)}/{f(mean(col(B,'occ_med')),2)}; over-read {f(mean(col(A,'btlbw_overread')),1)}/{f(mean(col(B,'btlbw_overread')),1)}")
    holds = []
    if d < -SIGMA_K * sig:
        holds.append("REFUTED")
    if pin is not None and pin >= PIN_MIN:
        holds.append("PINNED")
    if abs(d) <= SIGMA_K * sig and xd is not None and xsig is not None and xd > SIGMA_K * xsig:
        holds.append("LATENCY-COST")
    if d > SIGMA_K * sig:
        holds.append("GOODPUT-UP")
    if abs(d) <= SIGMA_K * sig and xd is not None and xsig is not None and abs(xd) <= SIGMA_K * xsig \
            and (pin is None or pin < PIN_MIN):
        holds.append("INERT-AS-DERIVED")
    first = next((h for h in ("REFUTED", "PINNED", "LATENCY-COST", "GOODPUT-UP", "INERT-AS-DERIVED") if h in holds),
                 "OUTSIDE THE PRE-REGISTERED SET")
    res.update({"d": d, "sig": sig, "holds": holds, "first": first, "noop_fail": abs(d) > SIGMA_K * sig})
    out(f"  outcomes that hold: {holds or ['OUTSIDE THE PRE-REGISTERED SET']}; first in order: {first}")
    return res


def score19(paths, out=print):
    rows = rows_of(paths)
    out("SAME-SESSION CONTROL SCORE (pre-registration: status.md §19)")
    out("")
    keys = sorted({(r["cell"], r["arm"]) for r in rows})
    out("ROWS by (cell, arm): live / total / statuses")
    for c, a in keys:
        rs = [r for r in rows if r["cell"] == c and r["arm"] == a]
        st = {}
        for r in rs:
            st[r["status"]] = st.get(r["status"], 0) + 1
        out(f"  {c} {a}: live {len([r for r in rs if r['status']=='LIVE'])}/{len(rs)} {st}")
    out("")
    out("PER-ROW (mbps pin floor cap ask win occ paused rtt over ax50 ax95 status)")
    for r in sorted(rows, key=lambda r: (r["cell"], r["arm"], r["seed"], r["rep"])):
        out(f"  {r['cell']} {r['arm']} s{r['seed']} r{r['rep']}: mbps={f(r.get('mbps'))} "
            f"pin={f(r.get('sumcap_pin'),3)} floor={f(r.get('sumcap_floor'),3)} cap={f(r.get('sumcap_cap'),1)} "
            f"ask={f(r.get('sumcap_ask'),1)} win={f(r.get('win_cap_med'),0)} occ={f(r.get('occ_med'),2)} "
            f"paused={f(r.get('paused_med'),0)} rtt={f(r.get('rtt_med'))} over={f(r.get('btlbw_overread'),1)} "
            f"ax50={r.get('ax_p50_us')} ax95={r.get('ax_p95_us')} {r['status']} {','.join(r['problems'])}")
    out("")
    out("== single-path cells c1s-400 and c2-100: LAW vs MAIN (section 18's outcome list and order) ==")
    single = {}
    for cell in SINGLE_CELLS:
        single[cell] = contrast(rows, cell, "LAW", "MAIN", out)
    c1s = single[SCORED_CELL]
    out("")
    out("== N >= 2 no-op cells (c1d, c8): LAW vs MAIN goodput, REFUTED-NOOP if |mean diff| > 2 sigma_pooled ==")
    noop_fail, unscoreable = [], []
    for cell in NOOP19_CELLS:
        r = contrast(rows, cell, "LAW", "MAIN", out)
        if r["unscoreable"]:
            unscoreable.append(cell)
        elif r["noop_fail"]:
            noop_fail.append(cell)
            out(f"  -> REFUTED-NOOP at {cell}")
        else:
            out(f"  -> no-op holds at {cell}")
    out("")
    out("== VM-drift reading (not scored): goodput medians against the cross-session references ==")
    refs = {"c1d-400": "section 15 MAIN 677.9-755.1; section 16 688-729", "c2-100": "section 15 MAIN 90.08-90.32",
            "c8-100": "section 15 MAIN 82.9-102.9", "c1s-400": "section 15 MAIN 721.9-831.4 (other binary era)"}
    for cell in ["c1d-400", "c2-100", "c8-100", "c1s-400"]:
        parts = []
        for a in ("LAW", "MAIN"):
            g = col(live(rows, cell, a), "mbps")
            parts.append(f"{a} median {f(statistics.median(g)) if g else '-'}")
        out(f"  {cell}: {'; '.join(parts)}  [{refs[cell]}]")
    out("")
    out("== delta leg: Realtime rows (LAW) ==")
    for cell in (SCORED_CELL, "c2-100"):
        for r in sorted([r for r in rows if r["cell"] == cell and r["arm"] == "LAW-realtime"], key=lambda r: r["rep"]):
            out(f"  {cell} realtime r{r['rep']}: {r['status']} dcap_q={r.get('dcap_q')} (want 0.05) "
                f"mbps={f(r.get('mbps'))} pin={f(r.get('sumcap_pin'),3)} {','.join(r['problems'])}")
    out("")
    s_ref = [c for c, r in single.items() if (not r["unscoreable"]) and "REFUTED" in r.get("holds", [])]
    s_uns = [c for c, r in single.items() if r["unscoreable"]]
    ships = (not noop_fail) and (not s_ref) and not s_uns and not unscoreable
    out("DECISION RULE (section 19, as amended): ships iff no REFUTED-NOOP (c1d, c8) and neither single-path cell "
        "(c1s, c2) is REFUTED; PINNED and LATENCY-COST do not block.")
    if noop_fail:
        out(f"RESULT: REFUTED-NOOP at {noop_fail}")
    elif s_ref:
        out(f"RESULT: REFUTED at the single-path cell(s) {s_ref} -> does not ship")
    elif unscoreable or s_uns:
        out(f"RESULT: UNSCOREABLE at {unscoreable + s_uns}")
    else:
        out("N >= 2 (c1d, c8): the section 18 CONTROL-MOVED at those cells is attributed to the cross-session reference.")
        out(f"RESULT: SHIPS (c1s: {single['c1s-400']['first']}; c2: {single['c2-100']['first']})")
    return 0


def check(path):
    t = open(path, errors="replace").read()
    ok = "SL-BATTERY-DONE" in t
    n = t.count("\nSLROW ") + (1 if t.startswith("SLROW ") else 0)
    print(f"CHECK done={ok} rows={n}")
    return 0 if ok else 1


def smoke(path):
    rows = rows_of([path])
    got = {(r["cell"], r["arm"]): r for r in rows}
    need = [("c1s-400", "LAW"), ("c1s-400", "CTL"), ("c2-100", "LAW")]
    bad = []
    for k in need:
        r = got.get(k)
        if r is None:
            bad.append(f"{k}: no row")
            continue
        if r["status"] != "LIVE":
            bad.append(f"{k}: {r['status']} {r['problems']}")
        if not r.get("gates_cli"):
            bad.append(f"{k}: no [GATES] echo")
        if k[1] == "LAW" and not r.get("sumcap_eng"):
            bad.append(f"{k}: [SUMCAP] eng=0 on LAW")
        if k[1] == "CTL" and r.get("sumcap_eng") not in (None, 0):
            bad.append(f"{k}: [SUMCAP] eng>0 on CTL")
        if k[1] == "LAW" and r.get("win_cap_distinct", 0) < 2:
            bad.append(f"{k}: win= constant on LAW")
    for k in need:
        r = got.get(k)
        if r:
            print(f"SMOKE {k}: mbps={r.get('mbps')} eng={r.get('sumcap_eng')}/{r.get('sumcap_refresh')} "
                  f"pin={r.get('sumcap_pin')} win_distinct={r.get('win_cap_distinct')} status={r['status']}")
    if bad:
        print("SMOKE-FAIL " + "; ".join(bad))
        return 1
    print("SMOKE-PASS")
    return 0


def smoke19(path):
    """Section 19 smoke: both binaries witnessed, and the named-hint fix live."""
    rows = rows_of([path])
    got = {(r["cell"], r["arm"]): r for r in rows}
    need = [("c1s-400", "MAIN"), ("c1s-400", "LAW"), ("c2-100", "MAIN"), ("c2-100", "LAW"),
            ("c1s-400", "LAW-realtime")]
    bad = []
    for k in need:
        r = got.get(k)
        if r is None:
            bad.append(f"{k}: no row")
            continue
        if r["status"] != "LIVE":
            bad.append(f"{k}: {r['status']} {r['problems']}")
        if not r.get("gates_cli"):
            bad.append(f"{k}: no [GATES] echo")
        if k[0] in SINGLE_CELLS and k[1] == "MAIN" and r.get("sumcap_eng") not in (None, 0):
            bad.append(f"{k}: [SUMCAP] eng>0 on MAIN at a single-path cell")
        if k[1] != "MAIN" and not r.get("sumcap_eng"):
            bad.append(f"{k}: [SUMCAP] eng=0 on a LAW arm")
        if k[1] == "LAW-realtime" and r.get("dcap_q") != 0.05:
            bad.append(f"{k}: dcap_q={r.get('dcap_q')}")
    for k in need:
        r = got.get(k)
        if r:
            print(f"SMOKE {k}: mbps={r.get('mbps')} eng={r.get('sumcap_eng')}/{r.get('sumcap_refresh')} "
                  f"pin={r.get('sumcap_pin')} win_med={r.get('win_cap_med')} win_distinct={r.get('win_cap_distinct')} "
                  f"status={r['status']}")
    shas = {r.get("bin_sha") for r in rows if r["arm"] == "MAIN"}, {r.get("bin_sha") for r in rows if r["arm"] != "MAIN"}
    if not shas[0] or not shas[1] or shas[0] & shas[1]:
        bad.append(f"binary shas not two distinct builds: {shas}")
    if bad:
        print("SMOKE-FAIL " + "; ".join(bad))
        return 1
    print("SMOKE-PASS")
    return 0


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    cmd = argv[1]
    if cmd == "row":
        cell, arm, seed, rep, rc, wall, drv, cli, srv = argv[2:11]
        cot = int(argv[11]) if len(argv) > 11 else 0
        sha = argv[12] if len(argv) > 12 else None
        row = make_row(cell, arm, seed, rep, rc, wall, lc.read(drv), lc.read(cli), lc.read(srv), cot, sha)
        print("SLROW " + json.dumps(row, sort_keys=True))
        return 0
    if cmd == "smoke":
        return smoke(argv[2])
    if cmd == "smoke19":
        return smoke19(argv[2])
    if cmd == "check":
        return check(argv[2])
    if cmd == "score":
        return score(argv[2:])
    if cmd == "score19":
        return score19(argv[2:])
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
