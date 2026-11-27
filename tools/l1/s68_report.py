#!/usr/bin/env python3
"""Status 68 (the hand-off gate's step 1): the `[HOFF]` parser, the per-row readouts (`t68`) and the REPORT. Reported only: nothing here prints a PASS / FAIL or a verdict label; a registered stop rule that
fires prints `STOP68 <reason>`, a "diagnose / attribute" rule prints `DIAG68 <reason>`, and a cell that was not built prints `UNBUILT68`.

    s68_report.py report <ledger> [<raw diag-ld dir> [<reference ledger>]]   per row ROW68 + per (cell, arm) medians, the ratios against BASE, the stop rules and the execution witnesses
    s68_report.py smoke <smoke|rehearsal> <ledger>      the smoke check (SMOKE-PASS | SMOKE-FAIL): every row ran, carries its t68 and its seat's binary; the readings are only echoed

`[HOFF]` (raptorpath/src/net/hoff.rs, paper 6.8):
    [HOFF] t=<s>s gate=on|off owner_timer_wakes=<n> probe_acks=<n> tau_floor_bind=<x> p<id>[B=p50/p95/max n=.. dgq=.. F=.. win=.. R=.. tau_o=<ms>ms(<tag>) .. beta=<x> hand_bind=<x> spare_i=0|inf spare_bind=<x>
        reopen=p50/max ms/n starve=.. app_limited=.. batch=.. g_batch=.. phat=.. fifo_wait_resend=<ms>ms/<n>] ... infl_vs_out=<eng>/<out>/x<ratio> store_pin=<x>
The parser is GENERIC: top-level `key=value` tokens, then `p<id>[...]` blocks split into `key=value`; a field it does not know (the `qdisc=` another worker is adding, anywhere) is kept and ignored. `-` is None,
`a/b/c` a list, `2.00ms(meas)` a number plus a `_tag`. The client log is the sender's (the perf client sends), so its `[HOFF]` lines are the readouts.

Witness fields (gate on only; an old ledger lacks them and reads `-`): fs_over=<n> fs_bypass=<defer>/<timer>/<shutdown>/<cold> ex_max=<total>/<first part>/<exempt part> ex_n=<ctrl>/<resend>/<repair>
beta_q=<p5>/<p50>/<p95> own_gap_max=<us>us pend_max=<n> (hoff.rs `witness_fields`).
Plan 68s adds T = the GATE twin on PH (an A/A pair: AA68 prints GATE-AA/GATE per readout) and the bursty cell BU (rule: repairs per source < 0.9 r stops; the frame completion time has no readout, UNWITNESSED68).
Arms: B, R4 (R), GATE (G) and, on the K cells of the rerun table (68r), Q = the gate alone (quinn's own BBR, RWM_HOFF_GATE=1). STOP68 fs-over fires on any GATE or Q row with fs_over > 0; the older B-over-beta
reading is REPORTED ONLY (`READ68 b-over-beta`), its rule being under amendment.

Settled readings (the registration leaves them open):
  * first-send refusals (the execution witness) = sum over the gate=on [HOFF] windows and paths of hand_bind x n: `hand_bind` is the share of the window's turns CLOSED with units waiting, and `n` its turn count
    (hoff.rs); a gate=off window prints the shadow model's hand_bind, which refused nothing, so it counts for none;
  * hand_bind at c2 = the median over windows of path 0's hand_bind (c2 has one path); the B bound is read per window: B p95 - beta <= the row's median batch (datagrams per hand-off);
  * the coded share and the repairs per source come from [DIAG] (mbar59_score: cod / src), coded share = c / (1 + c) with c = cod / src; r is the [SPAN] `rr` echo;
  * the `congestion_state() reads = 0` witness has no gauge (`qcns` is the controller's own read timing in [QCC]): printed UNWITNESSED68, never a pass;
  * "A/A" (the registration's section 6 leaves its resolution open) is not evaluated: the comparison is printed and marked A/A-UNRESOLVED."""
import json
import math
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import plan68  # noqa: E402

WARM_S = 2.0                       # [HOFF] windows before this are the warm-up and the first ticks
CELLS_ORDER = plan68.CELL_ORDER
_ANSI = re.compile(r"\x1b\[[0-9;]*m")
_PATH = re.compile(r"\bp(\d+)\[([^\]]*)\]")
_NUM = re.compile(r"^(-?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?|inf|-inf|nan)(?:ms|us|s|x)?(?:\(([^)]*)\))?$")


def _val(v):
    """`12.5` / `2.00ms(meas)` / `x1.02` / `-` / `a/b/c` -> float | None | list | str; a unit suffix is dropped, a (tag) is returned by `_tag`."""
    if v == "-":
        return None
    if "/" in v:
        return [_val(p) for p in v.split("/")]
    if re.match(r"^x[-\d.]", v):      # infl_vs_out's `x1.02` ratio
        v = v[1:]
        if v == "-":
            return None
    m = _NUM.match(v)
    if m:
        try:
            f = float(m.group(1))
        except ValueError:
            return v
        return f if math.isfinite(f) or m.group(1) in ("inf", "-inf") else None
    return v


def _tag(v):
    m = _NUM.match(v)
    return m.group(2) if m and m.group(2) else None


def _kv(text):
    out = {}
    for tok in text.split():
        if "=" not in tok:
            continue
        k, v = tok.split("=", 1)
        if not k or not re.match(r"^[A-Za-z_][\w]*$", k):
            continue
        out[k] = _val(v)
        tg = _tag(v)
        if tg is not None:
            out[k + "_tag"] = tg
    return out


def parse_hoff_line(line):
    """One `[HOFF]` line -> {"t", "gate", ..top-level fields.., "paths": {id: {field: value}}}; None when the line is not one. Tolerant of unknown fields and of a malformed block."""
    s = _ANSI.sub("", line)
    i = s.find("[HOFF] ")
    if i < 0:
        return None
    body = s[i + len("[HOFF] "):]
    paths = {}
    for m in _PATH.finditer(body):
        d = _kv(m.group(2))
        b = d.get("B")
        if isinstance(b, list) and len(b) == 3:
            d["B_p50"], d["B_p95"], d["B_max"] = b
        paths[int(m.group(1))] = d
    top_text = _PATH.sub(" ", body)
    raw_gate = re.search(r"\bgate=(\w+)", top_text)
    top = _kv(top_text)
    top["gate"] = raw_gate.group(1) if raw_gate else None
    top["paths"] = paths
    if "t" not in top or not isinstance(top.get("t"), float):
        return None
    iv = top.get("infl_vs_out")
    top["infl_ratio"] = iv[2] if isinstance(iv, list) and len(iv) == 3 else None
    return top


def parse_hoff(lines):
    return [d for d in (parse_hoff_line(ln) for ln in lines or [] if "[HOFF]" in ln) if d]


def _num(x):
    return isinstance(x, (int, float)) and not isinstance(x, bool) and math.isfinite(x)


def _med(v):
    v = sorted(x for x in v if _num(x))
    if not v:
        return None
    n = len(v)
    return v[n // 2] if n % 2 else 0.5 * (v[n // 2 - 1] + v[n // 2])


def _mean(v):
    v = [x for x in v if _num(x)]
    return sum(v) / len(v) if v else None


def _min(v):
    v = [x for x in v if _num(x)]
    return min(v) if v else None


def _max(v):
    v = [x for x in v if _num(x)]
    return max(v) if v else None


def witness_of(wins):
    """The gate's witness counters over the given windows (all of a row's windows: a cold-start bypass or an early leak lands in the warm-up). None where no window carries the field (an old ledger)."""
    col = lambda k: [w.get(k) for w in wins]      # noqa: E731
    ex = [w["ex_max"] for w in wins if isinstance(w.get("ex_max"), list) and len(w["ex_max"]) == 3 and _num(w["ex_max"][0])]
    exm = max(ex, key=lambda e: e[0]) if ex else None
    byp = [w["fs_bypass"] for w in wins if isinstance(w.get("fs_bypass"), list) and len(w["fs_bypass"]) == 4]
    exn = [w["ex_n"] for w in wins if isinstance(w.get("ex_n"), list) and len(w["ex_n"]) == 3]
    ocs = [w["fs_over_cs"] for w in wins if isinstance(w.get("fs_over_cs"), list) and len(w["fs_over_cs"]) == 2]
    bmx = [w["beta_max"] for w in wins if isinstance(w.get("beta_max"), list) and len(w["beta_max"]) == 5 and _num(w["beta_max"][0])]
    bmm = max(bmx, key=lambda e: e[0]) if bmx else None
    uo = [w["u_over"] for w in wins if _num(w.get("u_over"))]
    bq = [w["beta_q"] for w in wins if isinstance(w.get("beta_q"), list) and len(w["beta_q"]) == 3]
    fso = [w["fs_over"] for w in wins if _num(w.get("fs_over"))]
    fsn = [w["fs_n"] for w in wins if _num(w.get("fs_n"))]
    return {
        "fs_over": int(sum(fso)) if fso else None, "fs_n": int(sum(fsn)) if fsn else None,
        "fs_bypass": [int(sum(b[i] for b in byp if _num(b[i]))) for i in range(4)] if byp else None,
        "fs_over_cs": [int(sum(c[i] for c in ocs if _num(c[i]))) for i in range(2)] if ocs else None,
        "beta_max": list(bmm) if bmm else None, "b_max": _max(col("b_max")), "u_over": int(sum(uo)) if uo else None, "u_windows": len(uo) if uo else None,
        "ack_bind": _mean(col("ack_bind")),
        "ex_max": list(exm) if exm else None, "ex_n": [int(sum(e[i] for e in exn if _num(e[i]))) for i in range(3)] if exn else None,
        "beta_p5_min": _min([b[0] for b in bq]), "beta_p50_med": _med([b[1] for b in bq]), "beta_p95_max": _max([b[2] for b in bq]),
        "own_gap_max": _max(col("own_gap_max")), "pend_max": _max(col("pend_max")),
    }


def path_summary(wins):
    """One path's readouts over its [HOFF] windows (a list of the path dicts, in order)."""
    col = lambda k: [w.get(k) for w in wins]      # noqa: E731
    # a refusal is a REAL closed turn: only windows whose line says gate=on count (with the gate off, hand_bind is the shadow model's reading, nothing was refused)
    closed = sum(round(w["hand_bind"] * w["n"]) for w in wins if w.get("_gate") == "on" and _num(w.get("hand_bind")) and _num(w.get("n")))
    turns = sum(w["n"] for w in wins if _num(w.get("n")))
    excess = [w["B_p95"] - w["beta"] for w in wins if _num(w.get("B_p95")) and _num(w.get("beta"))]
    reo = [w["reopen"] for w in wins if isinstance(w.get("reopen"), list) and len(w["reopen"]) == 3]
    fifo = [w["fifo_wait_resend"] for w in wins if isinstance(w.get("fifo_wait_resend"), list) and len(w["fifo_wait_resend"]) == 2]
    return {
        **witness_of(wins),
        "n_win": len(wins), "B_p50_med": _med(col("B_p50")), "B_p95_max": _max(col("B_p95")), "B_max": _max(col("B_max")),
        "hand_bind_med": _med(col("hand_bind")), "hand_bind_mean": _mean(col("hand_bind")), "spare_bind_mean": _mean(col("spare_bind")), "starve_mean": _mean(col("starve")),
        "beta_med": _med(col("beta")), "L_med": _med(col("L")), "R_med": _med(col("R")), "win_med": _med(col("win")), "F_med": _med(col("F")), "dgq_med": _med(col("dgq")),
        "tau_o_med": _med(col("tau_o")), "tau_o_tag": next((w.get("tau_o_tag") for w in reversed(wins) if w.get("tau_o_tag")), None), "tau_p_med": _med(col("tau_p")),
        "l_o_med": _med(col("l_o")), "l_p_med": _med(col("l_p")), "batch_med": _med(col("batch")), "app_limited_mean": _mean(col("app_limited")),
        "reopen_p50_med": _med([r[0] for r in reo]), "reopen_max": _max([r[1] for r in reo]), "reopen_n": int(sum(r[2] for r in reo if _num(r[2]))),
        "fifo_p95_max": _max([f[0] for f in fifo]), "fifo_n": int(sum(f[1] for f in fifo if _num(f[1]))),
        "refusals": closed, "turns": turns, "b_excess_max": _max(excess),
    }


_QCC_F = re.compile(r"\bp(\d+):.*?\bqcns=(\d+)/(\d+)/(\d+).*?\bqrn=(\d+) qrus=(\d+) qrb=(\d+) qab=(\d+)")


def qcc_last(lines):
    """The last [QCC] line's per-path ProbeRtt counters {id: {qrn, qrus, qrb, qab, qcns_reads}}; {} when there is none (BASE has no [QCC])."""
    last = [ln for ln in lines or [] if "[QCC]" in ln]
    if not last:
        return {}
    out = {}
    for blk in re.split(r"(?= p\d+:qm=)", _ANSI.sub("", last[-1]))[1:]:
        m = _QCC_F.search(blk)
        if m:
            out[int(m.group(1))] = {"qcns_reads": int(m.group(3)), "qrn": int(m.group(5)), "qrus": int(m.group(6)), "qrb": int(m.group(7)), "qab": int(m.group(8))}
    return out


_QM0 = re.compile(r"^\[QCC\] t=([\d.]+)s.*?\bp0:qm=(\S)")


def qm_b_share(lines, t_min=10.0):
    """The share of the sender's [QCC] lines at t >= t_min (the burst phase: the lead-in is over) whose leg 0 reads qm = B (ProbeBw); None without such a line (BASE has no [QCC])."""
    n = b = 0
    for ln in lines or []:
        m = _QM0.match(_ANSI.sub("", ln).strip())
        if m and float(m.group(1)) >= t_min:
            n += 1
            b += m.group(2) == "B"
    return b / n if n else None


def hoff_summary(lines):
    """-> the row's `[HOFF]` readouts from the SENDER's log: the gate token, the end-of-run counters and a path_summary per path."""
    ws = parse_hoff(lines)
    post = [w for w in ws if w["t"] >= WARM_S]
    out = {"n_lines": len(ws), "n_post": len(post), "gate": ws[-1]["gate"] if ws else None, "gates": sorted({w["gate"] for w in ws if w["gate"]}),
           "owner_timer_wakes": ws[-1].get("owner_timer_wakes") if ws else None, "probe_acks": ws[-1].get("probe_acks") if ws else None,
           "tau_floor_bind": ws[-1].get("tau_floor_bind") if ws else None, "store_pin": ws[-1].get("store_pin") if ws else None,
           "infl_ratio_med": _med([w["infl_ratio"] for w in post]), "paths": {}}
    ids = sorted({i for w in ws for i in w["paths"]})
    for i in ids:
        out["paths"][i] = path_summary([dict(w["paths"][i], _gate=w["gate"]) for w in post if i in w["paths"]])
        # the witness counters are totals of the whole run, warm-up windows included
        out["paths"][i].update(witness_of([w["paths"][i] for w in ws if i in w["paths"]]))
    out["refusals"] = sum(p["refusals"] for p in out["paths"].values())
    return out


def row_metrics(cli, srv, drv, nbytes=None, cell="c2-100", arm="GK2", mbps=None, row=None):
    """The `t68` of one row. Never raises on log content. `row` (the parser's partial row) lends cpu / in-order readouts when it has them."""
    cli, srv = list(cli or []), list(srv or [])
    out = {"v": 1, "arm": arm, "hoff": hoff_summary(cli), "hoff_srv_lines": len(parse_hoff(srv)), "qcc": qcc_last(cli), "qm_b_share": qm_b_share(cli), "goodput": mbps, "bytes": nbytes}
    try:
        import l1common as lc
        out["gate_echo_cli"], out["gate_echo_srv"] = lc.gate_tok(cli, "RWM_HOFF_GATE"), lc.gate_tok(srv, "RWM_HOFF_GATE")
        out["wr_cli"], out["wr_srv"] = _wr(cli), _wr(srv)
        out["cc"] = "bbr_rs" if any("burst-robust BBR" in ln for ln in cli) else ("bbr" if any("quinn congestion controller: BBR" in ln for ln in cli) else None)
    except Exception as e:      # a reported readout never voids a row
        out["error"] = f"{type(e).__name__}: {e}"
    try:
        import place38_parse as pp
        st = pp.inorder_delay_stats(cli)
        out["inorder_mean_ms"] = st["mean"] * 1e3 if st else None
    except Exception:
        out["inorder_mean_ms"] = None
    out["inorder_p95_ms"] = out["inorder_p99_ms"] = out["inorder_max_ms"] = None
    try:      # the tail of the same delay series (the client's [DIAG] ticks)
        import threadp1_parse as t1
        d = sorted(x for _, x in t1.inorder_delay(cli))
        if d:
            q = lambda f: d[min(len(d) - 1, int(f * (len(d) - 1) + 0.5))] * 1e3      # noqa: E731
            out["inorder_p95_ms"], out["inorder_p99_ms"], out["inorder_max_ms"] = q(0.95), q(0.99), d[-1] * 1e3
    except Exception:
        pass
    try:
        import mbar59_score
        m = mbar59_score.row_metrics(cli, srv, drv or [], nbytes, cell, mbps)
        c = m.get("cod_src_cum")
        out["cod_src"], out["cod_src_rate"], out["rr"] = c, m.get("cod_src_rate"), m.get("rr")
        out["coded_share"] = None if c is None else c / (1.0 + c)
        out["src_med"], out["cod_med"] = m.get("src_med"), m.get("cod_med")
    except Exception:
        out["cod_src"] = out["cod_src_rate"] = out["rr"] = out["coded_share"] = None
    for k in ("cpu_cli", "cpu_srv", "rtt_wrtt_med", "legs"):
        if row and row.get(k) is not None:
            out[k] = row.get(k)
    if nbytes and row:
        for k in ("cpu_cli", "cpu_srv"):
            if _num(row.get(k)):
                out[k + "_gb"] = row[k] / (nbytes / 1e9)
    return out


def _wr(lines):
    v = None
    for ln in lines:
        m = re.search(r"window_reliable: (true|false)", ln)
        if m:
            v = m.group(1) == "true"
    return v


# ── the report ──
def _lst(v, nd=0):
    return "-" if not isinstance(v, list) else "/".join(_f(float(x), nd) if _num(x) else "-" for x in v)


def _f(x, nd=3):
    return "-" if x is None or (isinstance(x, float) and not math.isfinite(x)) else (f"{x:.{nd}f}" if isinstance(x, float) else str(x))


def _ratio(a, b):
    return a / b if _num(a) and _num(b) and b else None


def _rows(path):
    import place38_parse as pp
    import pool56_score as ps
    rows, tokens = pp.rows_of([p for p in str(path).split(",") if p])
    cnt, _ = ps.counting([r for r in rows if plan68.valid_slot(r.get("cell"), r.get("arm"))])
    return cnt, tokens


def _by(rows):
    g = {}
    for r in rows:
        letter, code = plan68.arm_parse(r["arm"])
        g.setdefault(code, {}).setdefault(letter, []).append(r)
    return g


def _col(rs, f):
    return [x for x in (f(r) for r in rs) if _num(x)]


def _t(r):
    return r.get("t68") or {}


def _paths(r):
    """The row's per-path summaries with INT keys (the ledger is JSON: its keys come back as strings)."""
    return {int(k): v for k, v in ((_t(r).get("hoff") or {}).get("paths") or {}).items()}


def _p0(r):
    return _paths(r).get(0) or {}


def _pm(rs, key, pid=0):
    return _med([(_paths(r).get(pid) or {}).get(key) for r in rs])


def report(path, out=print, raw=None, ref=None):
    cnt, _ = _rows(path)
    refrows = []
    ref = ref or os.environ.get("LD_REF68") or None
    if ref:
        # a reference ledger (an earlier run) lends only its BASE and R4 rows, to the cells where this ledger has none: the rerun table has no such arm on the K cells
        have = {(plan68.arm_parse(r["arm"])[1], plan68.arm_parse(r["arm"])[0]) for r in cnt}
        refrows = [r for r in _rows(ref)[0] if plan68.arm_parse(r["arm"])[0] in ("B", "R") and (plan68.arm_parse(r["arm"])[1], plan68.arm_parse(r["arm"])[0]) not in have]
        out(f"REF68 {len(refrows)} BASE / R4 rows from {ref} (reference only: ratios, no stop rule reads them)")
    stops = []

    def stop(msg):
        stops.append(msg)
        out("STOP68 " + msg)
    out(f"S68 REPORT rows={len(cnt)}/{plan68.expect_rows()} (step 1 of the hand-off gate: reported only, no label)")
    for r in sorted(cnt, key=lambda r: (int(r["rep"]), plan68.CELL_ORDER.index(plan68.arm_parse(r["arm"])[1]), r["arm"])):
        t = (_reparse(r, raw) if raw else None) or r.get("t68") or {}
        if raw and t:
            r = dict(r, t68=t)
        if not t:
            out(f"ROW68 {r['arm']} s{r['seed']} r{r['rep']}: status={r.get('status')} no t68 ({r.get('t68_error')})")
            continue
        h, p0 = t.get("hoff") or {}, _p0(r)
        out(f"ROW68 {r['arm']} s{r['seed']} r{r['rep']}: status={r.get('status')} gp={_f(r.get('mbps'), 1)} cpu_cli/GB={_f(t.get('cpu_cli_gb'), 2)} cpu_srv/GB={_f(t.get('cpu_srv_gb'), 2)} "
            f"inorder_ms mean/p95/p99/max={_f(t.get('inorder_mean_ms'), 1)}/{_f(t.get('inorder_p95_ms'), 1)}/{_f(t.get('inorder_p99_ms'), 1)}/{_f(t.get('inorder_max_ms'), 1)} Ax_ms={t.get('rtt_wrtt_med')} gate={h.get('gate')} gate_echo={t.get('gate_echo_cli')}/{t.get('gate_echo_srv')} cc={t.get('cc')} "
            f"hoff_lines={h.get('n_lines')} wakes={h.get('owner_timer_wakes')} probe_acks={h.get('probe_acks')} refusals={h.get('refusals')} "
            f"p0[B p50/p95/max={_f(p0.get('B_p50_med'), 1)}/{_f(p0.get('B_p95_max'), 1)}/{_f(p0.get('B_max'), 1)} beta={_f(p0.get('beta_med'), 1)} L_ms={_f(p0.get('L_med'), 2)} hand_bind={_f(p0.get('hand_bind_med'))} "
            f"spare_bind={_f(p0.get('spare_bind_mean'))} batch={_f(p0.get('batch_med'), 1)} starve={_f(p0.get('starve_mean'))} reopen_max_ms={_f(p0.get('reopen_max'))} tau_o={_f(p0.get('tau_o_med'))}({p0.get('tau_o_tag')}) "
            f"app_limited={_f(p0.get('app_limited_mean'))}] {_wit_paths(r)} cod/src={_f(t.get('cod_src'))} coded_share={_f(t.get('coded_share'))} rr={_f(t.get('rr'))} qcc={t.get('qcc') or '-'} "
            f"bin_sha={str(r.get('bin_sha'))[:16]}")
    groups = _by(cnt)
    rgroups = _by(cnt + refrows)      # the per-cell lines and ratios; the stop rules and witnesses read this ledger's rows only
    out("CELLS68 built=" + ",".join(plan68.built_in()) + " unbuilt=" + ",".join(plan68.unbuilt_in()))
    shown = plan68.cells_of(plan68.plan_name())
    for code in shown + tuple(c for c in plan68.unbuilt_in() if c not in shown):      # an UNBUILT cell outside the table is still named
        if code in plan68.unbuilt_in():
            out(f"UNBUILT68 {code} ({plan68.CELLS[code][1]}): {plan68.UNBUILT_WHY[code]}; no row ran, nothing read, nothing cleared")
            continue
        g = rgroups.get(code, {})
        lab = plan68.CELLS[code][1]
        out(f"CELL68 {code} {lab}: rows " + " ".join(f"{plan68.ARM_NAME[a]}={len(g.get(a, []))}" for a in plan68.arms_of(code)))
        for a in plan68.arms_of(code):
            rs = [x for x in g.get(a, []) if x.get("status") == "LIVE"]
            if rs:
                out(f"  MED68 {code} {plan68.ARM_NAME[a]:<5} n={len(rs)} gp={_f(_med(_col(rs, lambda x: x.get('mbps'))), 1)} cpu_cli/GB={_f(_med(_col(rs, lambda x: _t(x).get('cpu_cli_gb'))), 2)} "
                    f"cpu_srv/GB={_f(_med(_col(rs, lambda x: _t(x).get('cpu_srv_gb'))), 2)} inorder_ms mean/p95/p99/max={_f(_med(_col(rs, lambda x: _t(x).get('inorder_mean_ms'))), 1)}/{_f(_med(_col(rs, lambda x: _t(x).get('inorder_p95_ms'))), 1)}/"
                    f"{_f(_med(_col(rs, lambda x: _t(x).get('inorder_p99_ms'))), 1)}/{_f(_max(_col(rs, lambda x: _t(x).get('inorder_max_ms'))), 1)} "
                    f"coded_share={_f(_med(_col(rs, lambda x: _t(x).get('coded_share'))))} hand_bind={_f(_pm(rs, 'hand_bind_med'))} spare_bind={_f(_pm(rs, 'spare_bind_mean'))} batch={_f(_pm(rs, 'batch_med'), 1)}")
        for a in plan68.arms_of(code):
            rs = [x for x in g.get(a, []) if x.get("status") == "LIVE"]
            if rs and a in ("G", "Q", "T"):
                for pid in sorted({i for x in rs for i in _paths(x)}):
                    out(f"  WIT68 {code} {plan68.ARM_NAME[a]:<5} p{pid} n={len(rs)} fs_over={_sumpaths(rs, 'fs_over', pid)} fs_over_cs(closed/spent)={_lst(_sumlist(rs, 'fs_over_cs', 2, pid))} fs_n={_sumpaths(rs, 'fs_n', pid)} fs_bypass={_lst(_sumlist(rs, 'fs_bypass', 4, pid))} ex_max(max)={_lst(_maxex(rs, pid), 0)} "
                        f"ex_n={_lst(_sumlist(rs, 'ex_n', 3, pid))} beta_q(p5min/p50med/p95max)={_f(_min(_col(rs, lambda x: (_paths(x).get(pid) or {}).get('beta_p5_min'))), 1)}/{_f(_pm(rs, 'beta_p50_med', pid), 1)}/"
                        f"{_f(_max(_col(rs, lambda x: (_paths(x).get(pid) or {}).get('beta_p95_max'))), 1)} own_gap_max_us={_f(_max(_col(rs, lambda x: (_paths(x).get(pid) or {}).get('own_gap_max'))))} "
                        f"pend_max={_f(_max(_col(rs, lambda x: (_paths(x).get(pid) or {}).get('pend_max'))))}")
        _ratios(code, g, out)
    _stop_rules(groups, stop, out)
    _witnesses(groups, stop, out)
    _window_reads(groups, out)
    out(f"REPORT68 REPORTED-ONLY rows={len(cnt)}/{plan68.expect_rows()} stops={len(stops)} unbuilt={','.join(plan68.unbuilt_in()) or '-'}")
    return 0


def _wit_one(pid, p):
    return (f"w{pid}[fs_over={_f(p.get('fs_over'))} fs_n={_f(p.get('fs_n'))} fs_bypass={_lst(p.get('fs_bypass'))} ex_max={_lst(p.get('ex_max'), 0)} ex_n={_lst(p.get('ex_n'))} "
            f"beta_q={_f(p.get('beta_p5_min'), 1)}/{_f(p.get('beta_p50_med'), 1)}/{_f(p.get('beta_p95_max'), 1)} own_gap_max_us={_f(p.get('own_gap_max'))} pend_max={_f(p.get('pend_max'))}]")


def _wit_paths(r):
    """The witness fields of EVERY path of the row (the slow leg of a two-path cell is the one to look at)."""
    ps = _paths(r)
    return " ".join(_wit_one(i, ps[i]) for i in sorted(ps)) or "w-"


def _sumpaths(rs, key, pid):
    """Sum of path `pid`'s counter over the rows; `-` when no row carries it (an old ledger)."""
    v = [(_paths(r).get(pid) or {}).get(key) for r in rs]
    v = [x for x in v if _num(x)]
    return str(int(sum(v))) if v else "-"


def _sumlist(rs, key, n, pid):
    v = [(_paths(r).get(pid) or {}).get(key) for r in rs]
    v = [x for x in v if isinstance(x, list) and len(x) == n]
    return [sum(x[i] for x in v if _num(x[i])) for i in range(n)] if v else None


def _maxex(rs, pid):
    v = [(_paths(r).get(pid) or {}).get("ex_max") for r in rs]
    v = [x for x in v if isinstance(x, list) and len(x) == 3 and _num(x[0])]
    return max(v, key=lambda e: e[0]) if v else None


def _fs_over(x):
    return sum(p["fs_over"] for p in _paths(x).values() if _num(p.get("fs_over")))


def _live(g, a):
    return [x for x in g.get(a, []) if x.get("status") == "LIVE"]


def _ratios(code, g, out):
    base, r4, gate, qa, twin = _live(g, "B"), _live(g, "R"), _live(g, "G"), _live(g, "Q"), _live(g, "T")
    for name, f, nd in (("goodput", lambda x: x.get("mbps"), 3), ("cpu_cli/GB", lambda x: _t(x).get("cpu_cli_gb"), 3), ("cpu_srv/GB", lambda x: _t(x).get("cpu_srv_gb"), 3),
                        ("inorder_mean", lambda x: _t(x).get("inorder_mean_ms"), 3), ("inorder_p95", lambda x: _t(x).get("inorder_p95_ms"), 3),
                        ("inorder_p99", lambda x: _t(x).get("inorder_p99_ms"), 3), ("inorder_max", lambda x: _t(x).get("inorder_max_ms"), 3), ("coded_share", lambda x: _t(x).get("coded_share"), 3)):
        mb, mr, mg, mq = _med(_col(base, f)), _med(_col(r4, f)), _med(_col(gate, f)), _med(_col(qa, f))
        out(f"  RATIO68 {code} {name}: R4/BASE={_f(_ratio(mr, mb), nd)} GATE/BASE={_f(_ratio(mg, mb), nd)} GATE/R4={_f(_ratio(mg, mr), nd)} Q/BASE={_f(_ratio(mq, mb), nd)}")
        if twin:      # the A/A pair: the same arm twice; its ratio is the run-to-run spread every other ratio is read against
            out(f"  AA68 {code} {name}: GATE-AA/GATE={_f(_ratio(_med(_col(twin, f)), mg), nd)} (n={len(twin)}/{len(gate)})")
    nleg = 2 if code == "K8" else 1
    for i in range(nleg):
        ax = {a: _med([(_t(x).get("rtt_wrtt_med") or [None] * 2)[i] if len(_t(x).get("rtt_wrtt_med") or []) > i else None for x in _live(g, a)]) for a in plan68.ARM_LETTERS}
        out(f"  RATIO68 {code} A_x leg{i}: R4/BASE={_f(_ratio(ax['R'], ax['B']))} GATE/BASE={_f(_ratio(ax['G'], ax['B']))}")
        bb = {a: _pm(_live(g, a), "batch_med", i) for a in plan68.ARM_LETTERS}
        out(f"  RATIO68 {code} batch_i leg{i}: R4={_f(bb['R'], 1)} GATE={_f(bb['G'], 1)} GATE/R4={_f(_ratio(bb['G'], bb['R']))}")
    # the registration's "diagnose / attribute" rules: reported under DIAG68, never STOP68
    if code == "K8":
        c = _ratio(_med(_col(gate, lambda x: _t(x).get("cpu_srv_gb"))), _med(_col(base, lambda x: _t(x).get("cpu_srv_gb"))))
        if c is not None and c > 1.03:
            out(f"  DIAG68 c8-cpu_srv GATE/BASE={c:.3f} > 1.03: diagnose before step 2 (finer I/O from admission, or the window's pacer bursts)")
        for i in range(2):
            bg, br = _pm(gate, "batch_med", i), _pm(r4, "batch_med", i)
            if _num(bg) and _num(br) and bg < 0.9 * br:
                out(f"  DIAG68 c8-batch leg{i}: GATE batch {bg:.1f} < 0.9 x R4's {br:.1f}: diagnose before step 2")
    if code == "K2":
        gp = _ratio(_med(_col(gate, lambda x: x.get("mbps"))), _med(_col(base, lambda x: x.get("mbps"))))
        if gp is not None and gp < 0.995:
            out(f"  DIAG68 c2-goodput GATE/BASE={gp:.4f} < 0.995: attribute to starvation (starve={_f(_pm(gate, 'starve_mean'))}), reopen latency (max {_f(_pm(gate, 'reopen_max'))} ms vs L {_f(_pm(gate, 'L_med'), 2)} ms), "
                f"ProbeRtt (qcc {[_t(x).get('qcc') for x in gate][:1]}) or the unattributed rest, then fix or price it")
        out(f"  READ68 K2 tau_o measured={_f(_pm(gate, 'tau_o_med'))} ms ({(_p0(gate[0]) if gate else {}).get('tau_o_tag')}) against the assumed 2 ms; app_limited GATE={_f(_pm(gate, 'app_limited_mean'))} R4={_f(_pm(r4, 'app_limited_mean'))}")


def K_CELLS_BUILT():
    return [c for c in plan68.K_CELLS if c in plan68.built_in()]


def _stop_rules(groups, stop, out):
    # Gate executes (the A/A twin T is a GATE row: a twin with the gate off would print a false A/A)
    for code in plan68.built_in():
        for x in groups.get(code, {}).get("G", []) + groups.get(code, {}).get("T", []):
            h = _t(x).get("hoff") or {}
            if x.get("status") != "LIVE":
                continue
            if h.get("gate") != "on":
                stop(f"gate-not-on {x['arm']} r{x['rep']}: [HOFF] gate={h.get('gate')} (lines={h.get('n_lines')}): a harness or build fault")
            if _t(x).get("gate_echo_cli") not in ("1", 1):
                stop(f"gate-echo {x['arm']} r{x['rep']}: [GATES] RWM_HOFF_GATE={_t(x).get('gate_echo_cli')} on the sending end, want 1")
            if not _num(h.get("probe_acks")) or h.get("probe_acks") <= 0:
                stop(f"routeprobe-reads-zero {x['arm']} r{x['rep']}: probe_acks={h.get('probe_acks')}")
            for pid, p in _paths(x).items():
                be, bt = p.get("b_excess_max"), p.get("batch_med")
                if _num(be) and _num(bt) and be > bt:      # reported only: the rule is under amendment
                    out(f"READ68 b-over-beta {x['arm']} r{x['rep']} p{pid}: max(B p95 - beta) = {be:.1f} > one batch ({bt:.1f})")
    # first sends past the bound through no tagged bypass site: a leak in the gate (GATE and the gate-alone arm)
    for code in plan68.built_in():
        for a in ("G", "Q", "T"):
            for x in groups.get(code, {}).get(a, []):
                if x.get("status") == "LIVE":
                    n = _fs_over(x)
                    if n > 0:
                        stop(f"fs-over {x['arm']} r{x['rep']}: {n} first sends staged with B >= beta through no bypass site")
    for code in K_CELLS_BUILT():
        for x in groups.get(code, {}).get("Q", []):
            h = _t(x).get("hoff") or {}
            if x.get("status") == "LIVE" and h.get("gate") != "on":
                stop(f"gate-not-on {x['arm']} r{x['rep']}: [HOFF] gate={h.get('gate')} (lines={h.get('n_lines')}): a harness or build fault")
    for x in groups.get("K2", {}).get("G", []):
        hb = _p0(x).get("hand_bind_med")
        if x.get("status") == "LIVE" and not (_num(hb) and hb > 0.5):
            stop(f"hand_bind-c2-bulk {x['arm']} r{x['rep']}: hand_bind median {_f(hb)} <= 0.5 at c2 Bulk (the gate does not bind)")
    for code in plan68.built_in():
        for x in groups.get(code, {}).get("R", []):
            h = _t(x).get("hoff") or {}
            if x.get("status") == "LIVE" and h.get("gate") == "on":
                stop(f"r4-gate-on {x['arm']} r{x['rep']}: the R4 arm printed gate=on (the arm must be off)")
    # Clamp at Auto
    g = groups.get("A2", {})
    gate, r4 = _live(g, "G"), _live(g, "R")
    cs_g, cs_r = _med(_col(gate, lambda x: _t(x).get("coded_share"))), _med(_col(r4, lambda x: _t(x).get("coded_share")))
    hb = _pm(gate, "hand_bind_med")
    out(f"READ68 A2 clamp: spare_bind GATE={_f(_pm(gate, 'spare_bind_mean'))} coded_share GATE={_f(cs_g)} R4={_f(cs_r)} hand_bind={_f(hb)}")
    if _num(cs_g) and _num(cs_r) and cs_g >= cs_r and _num(hb) and hb > 0.5:
        stop(f"clamp-inert A2: coded share GATE {cs_g:.3f} >= R4's {cs_r:.3f} while hand_bind {hb:.3f} > 0.5")
    # The cells that need the pacer: evaluated only when they were built and ran
    for code, label in (("PC", "clamp-with-headroom"), ("PH", "clamp-with-headroom"), ("LR", "low-rate"), ("BU", "bursts-with-headroom")):
        if code not in plan68.built_in():
            continue
        g = groups.get(code, {})
        gate, r4 = _live(g, "G"), _live(g, "R")
        if code in ("PC", "PH"):
            sb = _pm(gate, "spare_bind_mean")
            if _num(sb) and sb > 0.05:
                stop(f"clamp-with-headroom {code}: spare_bind {sb:.3f} > 0.05 (the clamp binds with headroom)")
            out(f"READ68 {code} coded_share GATE={_f(_med(_col(gate, lambda x: _t(x).get('coded_share'))))} R4={_f(_med(_col(r4, lambda x: _t(x).get('coded_share'))))}: A/A-UNRESOLVED (the registered +-A/A band is not evaluated)")
        elif code == "BU":
            # registration section 3: repairs per source < 0.9 r means the registered limit binds (the frame completion time against R4 has no readout here: UNWITNESSED68)
            for x in gate:
                c, rr = _t(x).get("cod_src"), _t(x).get("rr")
                if _num(c) and _num(rr) and rr > 0 and c / rr < 0.9:
                    stop(f"bursts-with-headroom BU {x['arm']} r{x['rep']}: repairs per source {c:.4f} < 0.9 r ({rr:.4f}): the limit binds")
            out(f"READ68 BU hand_bind GATE={_f(_pm(gate, 'hand_bind_med'))} repairs/source GATE={_f(_med(_col(gate, lambda x: _t(x).get('cod_src'))), 4)} R4={_f(_med(_col(r4, lambda x: _t(x).get('cod_src'))), 4)}")
            out("UNWITNESSED68 BU frame completion time (GATE against R4): no per-frame readout exists in the harness")
        else:
            for x in gate:
                c, rr = _t(x).get("cod_src"), _t(x).get("rr")
                bm = _p0(x).get("B_max")
                if _num(c) and _num(rr) and rr > 0 and abs(c / rr - 1.0) > 0.05:
                    stop(f"low-rate LR {x['arm']} r{x['rep']}: repairs per source {c:.4f} outside r {rr:.4f} +- 5 %")
                if _num(bm) and bm > 2:
                    stop(f"low-rate LR {x['arm']} r{x['rep']}: B max {bm:.1f} > 2")


def _window_reads(groups, out):
    """Plan 68t's reported-only readings (never a stop): the window gauges per GATE cell (beta_max with its tuple, the U witness, ack_bind) and the BW witnesses (ProbeBw share of [QCC] after the lead-in,
    refusals per frame)."""
    for code in plan68.cells_of(plan68.plan_name()):
        for a in ("G", "R", "B"):
            rs = [x for x in groups.get(code, {}).get(a, []) if x.get("status") == "LIVE"]
            if not rs:
                continue
            if a == "G":
                for pid in sorted({i for x in rs for i in _paths(x)}):
                    bm = max([b for b in ((_paths(x).get(pid) or {}).get("beta_max") for x in rs) if b], key=lambda e: e[0], default=None)
                    uo = [(_paths(x).get(pid) or {}).get("u_over") for x in rs]
                    uw = [(_paths(x).get(pid) or {}).get("u_windows") for x in rs]
                    out(f"READ68 {code} GATE p{pid} u_over={int(sum(v for v in uo if _num(v)))}/{int(sum(v for v in uw if _num(v)))} windows beta_max(max; beta/W/srtt_ms/L_ms/R_MBs)={_lst(bm, 2)} "
                        f"b_max(max)={_f(_max(_col(rs, lambda x: (_paths(x).get(pid) or {}).get('b_max'))))} ack_bind(mean)={_f(_pm(rs, 'ack_bind', pid), 3)} fs_over_cs(closed/spent)={_lst(_sumlist(rs, 'fs_over_cs', 2, pid))}")
            if code in ("W1", "W2", "W4"):
                k = 10 * int(code[1])
                refs = [sum(p["refusals"] for p in (((_t(x).get("hoff") or {}).get("paths") or {}).values())) for x in rs]
                out(f"READ68 {code} {plan68.ARM_NAME[a]} k={k} qm=B share of [QCC] lines after 10 s={_f(_med(_col(rs, lambda x: _t(x).get('qm_b_share'))), 2)} (BW needs >= 0.90) "
                    f"refusals/frame={_f(_med([r / plan68.BW_FRAMES for r in refs]), 3) if a == 'G' else '-'} (BW needs frames with a refusal >= 0.5 at k = 20)")


def _witnesses(groups, stop, out):
    tot, per = 0, []
    for code in plan68.built_in():
        for x in groups.get(code, {}).get("G", []):
            n = (_t(x).get("hoff") or {}).get("refusals") or 0
            tot += n
            per.append(f"{x['arm']}r{x['rep']}={n}")
    out(f"WITNESS68 first-send refusals on GATE rows (turns closed with a first send waiting = sum of hand_bind x n): total={tot} " + " ".join(per))
    if per and tot <= 0:
        stop("first-send-refusals-zero: no GATE row refused a first send (the gate did not execute)")
    if not per:
        out("WITNESS68 no GATE row in the ledger: the execution witness is absent, not met")
    out("UNWITNESSED68 congestion_state() reads = 0: no gauge exists (qcns is the controller's own read timing, not quinn's congestion_state()); read the source, not this report")


def _reparse(r, raw):
    base = os.path.join(raw, f"{r['cell']}@def-{r['arm']}-s{r['seed']}-r{r['rep']}-a1-")
    try:
        rd = lambda suf: open(base + suf, errors="replace").read().splitlines()      # noqa: E731
        return row_metrics(rd("c.log"), rd("s.log"), rd("drv.out"), r.get("bytes"), r["cell"], r["arm"], r.get("mbps"), row=r)
    except OSError:
        return None


def smoke(kind, path, out=print):
    """The smoke / rehearsal check: one row per arm ran with its seat's binary right and its t68 computed. The readings (gate token, probe acks, refusals) are echoed, and the report's stop rules are NOT applied to a
    smoke (a 3-row smoke is no step-1 reading)."""
    cnt, _ = _rows(path)
    slots = plan68.plan(plan68.plan_name(), smoke=True)
    got = {(r["arm"], str(r["seed"]), str(r["rep"])): r for r in cnt}
    bad = []

    def crit(name, fails):
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)
    out(f"SMOKE68 kind={kind} rows={len(cnt)} slots={len(slots)}")
    crit("every smoke row ran", [f"{a} s{s} r{r}: no row" for (_, _, a, s, r) in slots if (a, s, str(r)) not in got])
    for (_, _, a, s, r) in slots:
        row = got.get((a, s, str(r)))
        if row is None:
            continue
        t = row.get("t68") or {}
        crit(f"{a}: the row carries its t68 readouts", [] if t and not row.get("t68_error") else [f"{a}: no t68 ({row.get('t68_error')})"])
        crit(f"{a}: the row ran its seat's binary (no bin-sha problem)", [x for x in row.get("problems") or [] if x.startswith("bin-sha")])
        h = t.get("hoff") or {}
        crit(f"{a}: [HOFF] lines on the sending end", [] if h.get("n_lines") else [f"{a}: no [HOFF] line (the gauge is in every arm)"])
        out(f"  REPORTED {a}: gate={h.get('gate')} gate_echo={t.get('gate_echo_cli')} probe_acks={h.get('probe_acks')} refusals={h.get('refusals')} status={row.get('status')}")
    if bad:
        out("SMOKE-FAIL " + "; ".join(bad))
        return 1
    out("SMOKE-PASS")
    return 0


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd == "report":
        return report(a[0], raw=(a[1] or None) if len(a) > 1 else None, ref=a[2] if len(a) > 2 else None)
    if cmd == "smoke":
        return smoke(a[0], a[1])
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
