#!/usr/bin/env python3
"""Parser and scorer for THE C1 ANCHOR OVER-READ DIAGNOSTIC (docs/status.md,
"23. The c1 anchor over-read and the store law's input -- diagnosis --
pre-registration").

One binary. Arms: MAIN (shipped), PRS (RWM_PLAIN_RS=1 RWM_HONEST_CAP=0),
S<n> (RWM_STORE=<n>).

    anchordiag_parse.py row   <cell> <arm> <seed> <rep> <rc> <wall_s> <drv> <cli> <srv> [cotenant] [sha]
    anchordiag_parse.py smoke <ledger>
    anchordiag_parse.py check <ledger>
    anchordiag_parse.py score <ledger>...

`row` prints one `ADROW {json}` line. Raw sample lists are never stored,
only summaries. Nothing here is a ship verdict.
"""
import json
import math
import re
import sys
import os

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import stage3_parse as sp  # noqa: E402
import storelaw_parse as sl  # noqa: E402  (regexes only)
from storelaw_parse import col, f, live, mean  # noqa: E402

GRID_C1S = [192, 256, 384, 512, 768, 1024, 1536, 2048]
GRID_C2 = [64, 96, 128, 192, 256, 384, 512, 1024]
GRID = {"c1s-400": GRID_C1S, "c2-100": GRID_C2}
EXPECT_ROWS = 64
WARM_S = 1.0                  # [DIAG] windows before this are warm-up
LINK_RATE = {                 # link datagram rate per leg (sym/s), indexed by engine path id
    "c1s-400": [97700], "c1d-400": [97700, 97700],
    "c2-100": [9770], "c8-100": [9770, 1955],   # c8 = c2/c3: p0 = scenA (c2), p1 = scenB (c3)
}
Q1_CELLS = ["c1s-400", "c2-100"]
ALL_CELLS = ["c1s-400", "c2-100", "c1d-400", "c8-100"]
Q1_OVER = 3.0                 # (a) median btlbw >= 3 L
Q1_FRAC = 0.90                # (b) >= 90 % of RSTRACE-LEGACY samples with elapsed_ms < Q1_FLOOR_MS
Q1_FLOOR_MS = 2.0
Q2_HONEST = 1.2               # o = btlbw / L <= 1.2
Q3_KNEE = 0.95
Q3_GAIN = 1.1                 # (1 + q), q = 0.1 at Bulk
Q3_SLACK = 1.5                # n* <= 1.5 ask_h
Q3_MIN_ROWS = 2
Q3_MAX_THIN = 2               # more than this many grid points under Q3_MIN_ROWS: UNSCOREABLE
SIGMA_K = 2.0

SAMPLER_ECHO = "plain-mode send-interval SAMPLER ACTIVE"
HONEST_ECHO = "honest floor-clock store caps ACTIVE"

for _a in ["MAIN", "PRS"] + [f"S{n}" for n in GRID_C1S + GRID_C2]:
    sp.ARM_SPEC[_a] = ("window", "Rlc", "bulk", "ACTIVE")

_PSEG = re.compile(r"(?:^|\s)p(\d+):infl=")
_BTL = re.compile(r"\bbtlbw=(\d+)")
_RTPUS = re.compile(r"\brtp_us=(\d+)")
_LEG = re.compile(r"\[RSTRACE-LEGACY\] path=(\S+) rate=([\d.]+) delta=(\d+) elapsed_ms=([\d.]+)")
_RS = re.compile(r"\[RSTRACE\] path=(\S+) seq=\d+ rate=([\d.]+) delivered=\d+ "
                 r"interval_ms=([\d.]+) send_ms=([\d.]+) ack_ms=([\d.]+)")
_AD_HEAD = re.compile(r"\[ACKDIAG\]\s+p(\d+)\s")
_AD = {
    "gap": re.compile(r"gap_us\[p50=(\S+) p90=(\S+) p99=(\S+) n=([^\]\s]+)\]"),
    "drecv": re.compile(r"drecv\[p50=(\S+) p90=(\S+) max=(\S+) n="),
    "rd": re.compile(r"rd\[acc=(\S+) rej=(\S+) cnt="),
    "x": re.compile(r"(?:^|\s)x\[p50=(\S+) p90=(\S+) p99=(\S+)\]"),
}
_AD_RATE = re.compile(r"rate_lr=([\d.]+)sym/s")
_AD_XA = re.compile(r"xanchor=(\S+)")
_AD_A = re.compile(r"(?:^|\s)anchor=([\d.]+)sym")


def _r(x, nd=3):
    return None if x is None else round(x, nd)


def diag_paths(cli):
    """Post-warm-up [DIAG] windows -> ({path: [btlbw]}, {path: [rtp_us]})."""
    btl, rtp = {}, {}
    for ln in cli:
        mt = sl._T.search(ln)
        if not mt or float(mt.group(1)) < WARM_S:
            continue
        hits = list(_PSEG.finditer(ln))
        for i, m in enumerate(hits):
            seg = ln[m.start(): hits[i + 1].start() if i + 1 < len(hits) else len(ln)]
            for rx, acc in ((_BTL, btl), (_RTPUS, rtp)):
                mm = rx.search(seg)
                if mm:
                    acc.setdefault(m.group(1), []).append(int(mm.group(1)))
    return btl, rtp


def ackdiag_last(cli):
    """The LAST [ACKDIAG] line of each path -> {path: summary}; plus the line count."""
    last, n = {}, 0
    for ln in cli:
        m = _AD_HEAD.search(ln)
        if m:
            last[m.group(1)] = ln
            n += 1
    out = {}
    for p, ln in last.items():
        d = {}
        for key, names in (("gap", ("p50", "p90", "p99", "n")), ("drecv", ("p50", "p90", "max")),
                           ("rd", ("acc", "rej")), ("x", ("p50", "p90", "p99"))):
            m = _AD[key].search(ln)
            for i, nm in enumerate(names):
                d[f"{key}_{nm}"] = lc.fnum(m.group(i + 1)) if m else None
        for key, rx in (("rate_lr", _AD_RATE), ("xanchor", _AD_XA), ("anchor", _AD_A)):
            m = rx.search(ln)
            d[key] = lc.fnum(m.group(1)) if m else None
        out[p] = d
    return out, n


def legacy_summary(cli):
    ms = [m for m in (_LEG.search(ln) for ln in cli) if m]
    el = [float(m.group(4)) for m in ms]
    dl = [int(m.group(3)) for m in ms]
    rt = [float(m.group(2)) for m in ms]
    return {
        "n": len(ms),
        "frac_lt2": _r(sum(1 for e in el if e < Q1_FLOOR_MS) / len(el), 4) if el else None,
        "el_p50": _r(lc.q(el, 0.5)), "el_p90": _r(lc.q(el, 0.9)), "el_max": _r(max(el)) if el else None,
        "delta_p50": _r(lc.q(dl, 0.5)), "delta_p90": _r(lc.q(dl, 0.9)), "delta_max": max(dl) if dl else None,
        "rate_max": _r(max(rt), 0) if rt else None,
    }


def rstrace_summary(cli):
    ms = [m for m in (_RS.search(ln) for ln in cli) if m]
    return {
        "n": len(ms),
        "interval_ms_p50": _r(lc.q([float(m.group(3)) for m in ms], 0.5)),
        "send_ms_p50": _r(lc.q([float(m.group(4)) for m in ms], 0.5)),
        "ack_ms_p50": _r(lc.q([float(m.group(5)) for m in ms], 0.5)),
        "rate_max": _r(max(float(m.group(2)) for m in ms), 0) if ms else None,
    }


def arm_n(arm):
    m = re.fullmatch(r"S(\d+)", arm)
    return int(m.group(1)) if m else None


def make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant=0, sha=None):
    row = sp.make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant)
    row["bin_sha"] = sha
    row["gate_plain_rs"] = lc.gate_tok(cli, "RWM_PLAIN_RS")
    row["gate_honest_cap"] = lc.gate_tok(cli, "RWM_HONEST_CAP")
    row["gate_store"] = lc.gate_tok(cli, "RWM_STORE")
    row["echo_sampler"] = any(SAMPLER_ECHO in ln for ln in cli)
    row["echo_honest"] = any(HONEST_ECHO in ln for ln in cli)
    cap = [m for m in (sl._SUMCAP.search(ln) for ln in cli) if m]
    if cap:
        m = cap[-1]
        row["sumcap_eng"], row["sumcap_refresh"] = int(m.group(2)), int(m.group(3))
        row["sumcap_pin"], row["sumcap_floor"] = float(m.group(7)), float(m.group(8))
        row["sumcap_cap"], row["sumcap_ask"] = float(m.group(9)), float(m.group(10))
    else:
        row["sumcap_eng"] = row["sumcap_refresh"] = None
    # post-warm-up windows
    wins, rtt = [], []
    for ln in cli:
        mt = sl._T.search(ln)
        if not mt or float(mt.group(1)) < WARM_S:
            continue
        mw = sl._WIN.search(ln)
        if mw:
            wins.append(int(mw.group(2)))
        mr = sl._RTT.search(ln)
        if mr:
            rtt.append(float(mr.group(1)))
    row["win_n"] = len(wins)
    row["win_den_med"] = lc.med(wins) if wins else None
    row["win_den_min"] = min(wins) if wins else None
    row["win_den_max"] = max(wins) if wins else None
    row["win_den_distinct"] = len(set(wins))
    row["rtt_med"] = lc.med(rtt) if rtt else None
    btl, rtp = diag_paths(cli)
    row["btlbw_p"] = {p: lc.med(v) for p, v in sorted(btl.items())}
    row["rtp_us_p"] = {p: lc.med(v) for p, v in sorted(rtp.items())}
    row["rtp_us_med"] = row["rtp_us_p"].get("0")
    ax = [m for m in (sl._AX.search(ln) for ln in srv if "[LAT]" in ln) if m]
    row["ax_p50_us"] = int(ax[-1].group(1)) if ax else None
    row["ax_p95_us"] = int(ax[-1].group(3)) if ax else None
    row["ackdiag"], row["ackdiag_n"] = ackdiag_last(cli)
    row["legacy"] = legacy_summary(cli)
    row["rstrace"] = rstrace_summary(cli)
    nbytes = sp.CELL_SPEC[cell][3]
    row["cpu_cli_gb"] = _r(row["cpu_cli"] / (nbytes / 1e9)) if row.get("cpu_cli") is not None else None

    p = []
    if not row["gates_cli"]:
        pass                                   # base row already carries no-gates-cli
    if row["ackdiag_n"] < 1:
        p.append("no-ackdiag")
    n = arm_n(arm)
    eng = row["sumcap_eng"]
    if arm == "MAIN":
        if not eng:
            p.append("main-sumcap-eng-zero")
        if row["echo_sampler"]:
            p.append("main-sampler-echo-present")
        if row["gate_plain_rs"] != "0":
            p.append(f"gate-plain-rs={row['gate_plain_rs']}")
        if row["gate_store"] != "unset":
            p.append(f"gate-store={row['gate_store']}")
    elif arm == "PRS":
        if not row["echo_sampler"]:
            p.append("prs-sampler-echo-missing")
        if row["gate_plain_rs"] != "1":
            p.append(f"gate-plain-rs={row['gate_plain_rs']}")
        if row["gate_honest_cap"] != "0":
            p.append(f"gate-honest-cap={row['gate_honest_cap']}")
        if row["echo_honest"]:
            p.append("prs-honest-cap-echo-present")
        if not eng:
            p.append("prs-sumcap-eng-zero")
    elif n is not None:
        if eng:
            p.append(f"s-sumcap-eng={eng}")
        if row["gate_store"] != str(n):
            p.append(f"gate-store={row['gate_store']}")
        if row["win_n"] < 2:
            p.append(f"s-win-windows={row['win_n']}")
        elif row["win_den_min"] != n or row["win_den_max"] != n:
            p.append(f"s-win-den-not-n={row['win_den_min']}..{row['win_den_max']}")
    row["problems"].extend(p)
    if row["problems"] and row["status"] == "LIVE":
        row["status"] = "WITNESS-FAIL"
    return row


# ── scoring helpers ──────────────────────────────────────────────────────
def rows_of(paths):
    rows = []
    for path in paths:
        for ln in lc.read(path):
            if ln.startswith("ADROW "):
                try:
                    rows.append(json.loads(ln[len("ADROW "):]))
                except ValueError:
                    pass
    return rows


def sd(v):
    return sl.sd(v) if len(v) > 1 else None


def btl0(r):
    return (r.get("btlbw_p") or {}).get("0")


def xanchor0(r):
    return ((r.get("ackdiag") or {}).get("0") or {}).get("xanchor")


def band(a, b):
    """(mean(a) - mean(b), 2 sigma_pooled), None where not computable (n < 2 in an arm)."""
    if not a or not b:
        return None, None
    d = mean(a) - mean(b)
    sa, sb = sd(a), sd(b)
    if sa is None or sb is None:
        return d, None
    return d, SIGMA_K * math.sqrt((sa ** 2 + sb ** 2) / 2)


def ranks(v):
    order = sorted(range(len(v)), key=lambda i: v[i])
    rk = [0.0] * len(v)
    i = 0
    while i < len(order):
        j = i
        while j + 1 < len(order) and v[order[j + 1]] == v[order[i]]:
            j += 1
        for k in range(i, j + 1):
            rk[order[k]] = (i + j) / 2 + 1
        i = j + 1
    return rk


def spearman(x, y):
    if len(x) < 3 or len(x) != len(y):
        return None
    rx, ry = ranks(x), ranks(y)
    mx, my = mean(rx), mean(ry)
    num = sum((a - mx) * (b - my) for a, b in zip(rx, ry))
    den = math.sqrt(sum((a - mx) ** 2 for a in rx) * sum((b - my) ** 2 for b in ry))
    return num / den if den else None


# ── Q1 / Q2 / Q3 (each returns its verdict string so tests can assert) ───
def q1(rows, cell, out):
    rs = live(rows, cell, "MAIN")
    L = LINK_RATE[cell][0]
    out(f"Q1 {cell} (MAIN live n={len(rs)}; L={L}/s; need btlbw >= {Q1_OVER:g} L and >= {Q1_FRAC:.0%} of samples elapsed_ms < {Q1_FLOOR_MS:g})")
    if not rs:
        out("  UNSCOREABLE (no live MAIN rows)")
        return "UNSCOREABLE"
    ok = True
    for r in sorted(rs, key=lambda r: (r["seed"], r["rep"])):
        b, lg = btl0(r), r.get("legacy") or {}
        a_ok = b is not None and b >= Q1_OVER * L
        b_ok = lg.get("frac_lt2") is not None and lg["frac_lt2"] >= Q1_FRAC
        ok = ok and a_ok and b_ok
        ad = (r.get("ackdiag") or {}).get("0") or {}
        out(f"  s{r['seed']} r{r['rep']}: btlbw={f(b,0)} ({f(b / L if b is not None else None,1)} L) a={a_ok}; "
            f"M n={lg.get('n')} frac<2ms={f(lg.get('frac_lt2'),3)} b={b_ok}; elapsed_ms p50/p90/max "
            f"{f(lg.get('el_p50'),2)}/{f(lg.get('el_p90'),2)}/{f(lg.get('el_max'),2)}; delta p50/p90/max "
            f"{f(lg.get('delta_p50'),0)}/{f(lg.get('delta_p90'),0)}/{f(lg.get('delta_max'),0)}")
        out(f"    ackdiag p0: gap_us p50/p90/p99 {f(ad.get('gap_p50'),0)}/{f(ad.get('gap_p90'),0)}/{f(ad.get('gap_p99'),0)}; "
            f"drecv p50/p90/max {f(ad.get('drecv_p50'),0)}/{f(ad.get('drecv_p90'),0)}/{f(ad.get('drecv_max'),0)}; "
            f"x p50/p90/p99 {f(ad.get('x_p50'),2)}/{f(ad.get('x_p90'),2)}/{f(ad.get('x_p99'),2)}; "
            f"xanchor {f(ad.get('xanchor'),2)}; rd acc/rej {f(ad.get('rd_acc'),0)}/{f(ad.get('rd_rej'),0)}")
    v = "MECHANISM-CONFIRMED" if ok else "MECHANISM-OPEN"
    out(f"  -> {v}")
    return v


def q2(rows, cell, out):
    rs = live(rows, cell, "PRS")
    Ls = LINK_RATE[cell]
    out(f"Q2 {cell} (PRS live n={len(rs)}; L per path {Ls})")
    if not rs:
        out("  UNSCOREABLE (no live PRS rows)")
        return "UNSCOREABLE"
    ok, missing = True, False
    for r in sorted(rs, key=lambda r: (r["seed"], r["rep"])):
        os_ = []
        for i, L in enumerate(Ls):
            b = (r.get("btlbw_p") or {}).get(str(i))
            o = b / L if b is not None else None
            os_.append(o)
            if o is None:
                missing = True
            elif o > Q2_HONEST:
                ok = False
        out(f"  s{r['seed']} r{r['rep']}: o per path " + " ".join(f"p{i}={f(o,2)}" for i, o in enumerate(os_)))
    v = "OVER-READ" if not ok else ("UNSCOREABLE" if missing else "HONEST")
    out(f"  -> {v}")
    return v


def q2_compare(rows, cell, out):
    P, M = live(rows, cell, "PRS"), live(rows, cell, "MAIN")
    out(f"  PRS - MAIN (not scored) {cell}: PRS n={len(P)} MAIN n={len(M)}; diff / 2 sigma_pooled")
    for label, key, nd in (("goodput Mbit/s", "mbps", 1), ("A_x p50 us", "ax_p50_us", 0),
                           ("SUMCAP cap", "sumcap_cap", 1), ("SUMCAP pin", "sumcap_pin", 3),
                           ("rtt ms", "rtt_med", 1), ("client CPU s/GB", "cpu_cli_gb", 2)):
        a, b = col(P, key), col(M, key)
        d, bd = band(a, b)
        out(f"    {label}: PRS {f(mean(a),nd)} MAIN {f(mean(b),nd)} diff {f(d,nd)} 2s {f(bd,nd)}")


def grid_stats(rows, cell):
    """[(n, live rows)] over the cell's grid."""
    return [(n, live(rows, cell, f"S{n}")) for n in GRID[cell]]


def q3(rows, cell, out):
    st = grid_stats(rows, cell)
    thin = [n for n, rs in st if len(rs) < Q3_MIN_ROWS]
    out(f"Q3 {cell} (grid points with < {Q3_MIN_ROWS} live rows: {thin or 'none'})")
    out("  n     live  goodput mean [min-max]   A_x p50 us  A_x p95 us  rtt ms  xanchor(p0)")
    xs, ys = [], []
    for n, rs in st:
        g = col(rs, "mbps")
        xa = mean([v for v in map(xanchor0, rs) if v is not None])
        out(f"  {n:<5} {len(rs):<5} {f(mean(g),1):>7} [{f(min(g) if g else None,1)}-{f(max(g) if g else None,1)}]"
            f"{'':<4}{f(mean(col(rs,'ax_p50_us')),0):>10}  {f(mean(col(rs,'ax_p95_us')),0):>10}  "
            f"{f(mean(col(rs,'rtt_med')),1):>6}  {f(xa,2):>6}")
        if xa is not None:
            xs.append(n)
            ys.append(xa)
    out(f"  Spearman(xanchor, n) over grid means (reported only): {f(spearman(xs, ys),2)} on {len(xs)} points")
    if len(thin) > Q3_MAX_THIN:
        out("  -> UNSCOREABLE (more than two grid points under 2 live rows)")
        return "UNSCOREABLE", {}
    G = [(n, mean(col(rs, "mbps"))) for n, rs in st if col(rs, "mbps")]
    if not G:
        out("  -> UNSCOREABLE (no goodput readings)")
        return "UNSCOREABLE", {}
    gmax = max(g for _, g in G)
    nstar = min(n for n, g in G if g >= Q3_KNEE * gmax)
    P = live(rows, cell, "PRS")
    bws = [btl0(r) for r in P if btl0(r) is not None]
    rtp = [r["rtp_us_med"] for r in P if r.get("rtp_us_med") is not None and btl0(r) is not None]
    out(f"  Gmax {f(gmax,1)}  n* = {nstar} (smallest n with G >= {Q3_KNEE:g} Gmax)")
    if not bws or not rtp:
        out("  ask_h: - (no live PRS reading)")
        out("  -> UNSCOREABLE (no honest-law input)")
        return "UNSCOREABLE", {"nstar": nstar}
    bw_h, rtprop = lc.med(bws), lc.med(rtp) / 1e6
    ask_h = Q3_GAIN * bw_h * rtprop
    info = {"nstar": nstar, "ask_h": ask_h, "bw_h": bw_h, "rtprop_s": rtprop}
    out(f"  bw_h {f(bw_h,0)} sym/s  RTprop {f(rtprop*1e3,2)} ms  ask_h = {Q3_GAIN:g} bw_h RTprop = {f(ask_h,1)} symbols; "
        f"1.5 ask_h = {f(Q3_SLACK*ask_h,1)}")
    if nstar <= Q3_SLACK * ask_h:
        out("  -> LAW-SUFFICIENT")
        return "LAW-SUFFICIENT", info
    H = nstar - ask_h
    info["H"] = H
    out(f"  H = n* - ask_h = {f(H,1)} symbols; H / bw_h = {f(H / bw_h * 1e3,2)} ms")
    out("  -> HEADROOM-MISSING")
    return "HEADROOM-MISSING", info


def score(paths, out=print):
    rows = rows_of(paths)
    out("ANCHOR-DIAG SCORE (pre-registration: status.md §23; diagnostic, nothing here ships)")
    out("")
    out("ROWS by (cell, arm): live / total / statuses")
    arms = ["MAIN", "PRS"] + [f"S{n}" for n in sorted(set(GRID_C1S + GRID_C2))]
    for c in ALL_CELLS:
        for a in arms:
            rs = [r for r in rows if r["cell"] == c and r["arm"] == a]
            if not rs:
                continue
            st = {}
            for r in rs:
                st[r["status"]] = st.get(r["status"], 0) + 1
            out(f"  {c} {a}: live {len([r for r in rs if r['status'] == 'LIVE'])}/{len(rs)} {st}")
    out("")
    out("VOID TABLE (rows not LIVE)")
    void = [r for r in rows if r["status"] != "LIVE"]
    for r in sorted(void, key=lambda r: (r["cell"], r["arm"], r["seed"], r["rep"])):
        out(f"  {r['cell']} {r['arm']} s{r['seed']} r{r['rep']}: {r['status']} {','.join(r['problems'])}")
    if not void:
        out("  none")
    out("")
    out("== Q1 (MAIN rows) ==")
    for cell in Q1_CELLS:
        q1(rows, cell, out)
    out("")
    out("== Q2 (PRS rows) ==")
    for cell in ALL_CELLS:
        q2(rows, cell, out)
        q2_compare(rows, cell, out)
    out("")
    out("== Q3 (store sweep) ==")
    for cell in Q1_CELLS:
        q3(rows, cell, out)
    return 0


def check(path):
    t = open(path, errors="replace").read()
    ok = "AD-BATTERY-DONE" in t
    n = len([ln for ln in t.splitlines() if ln.startswith("ADROW ")])
    good = ok and n == EXPECT_ROWS
    print(f"CHECK done={ok} rows={n}/{EXPECT_ROWS}")
    return 0 if good else 1


def smoke(path):
    got = {(r["cell"], r["arm"]): r for r in rows_of([path])}
    need = [("c1s-400", "MAIN"), ("c1s-400", "PRS"), ("c2-100", "S256")]
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
        if not r.get("ackdiag_n"):
            bad.append(f"{k}: no [ACKDIAG]")
        print(f"SMOKE {k}: mbps={r.get('mbps')} eng={r.get('sumcap_eng')} btlbw={r.get('btlbw_p')} "
              f"win_den={r.get('win_den_min')}..{r.get('win_den_max')} legacy_n={(r.get('legacy') or {}).get('n')} "
              f"rstrace_n={(r.get('rstrace') or {}).get('n')} status={r['status']}")
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
        print("ADROW " + json.dumps(row, sort_keys=True))
        return 0
    if cmd == "smoke":
        return smoke(argv[2])
    if cmd == "check":
        return check(argv[2])
    if cmd == "score":
        return score(argv[2:])
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
