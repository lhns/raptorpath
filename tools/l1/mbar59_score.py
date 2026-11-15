#!/usr/bin/env python3
"""Status 59: the evicting-seat m-bar readout (plan 59), REPORTED ONLY, no build, ONE seat (plan 56 / 57's shape). docs/derivations/pfec-oracle-spec.md section 6.1 / 6 item 1.

    mbar59_score.py score59 <ledger> [...]     the report (the VERDICT59 line carries the counts and the readings)
    mbar59_score.py smoke59 <ledger>           the rehearsal / smoke check: PASS / FAIL per criterion; rc 1 on any FAIL

What is read (client = the bulk sender, `[DIAG]` ticks after the last warm-up marker, LATE = t >= 5 s with src > 0; `[SPAN]` lines on their own clock, t >= 5 s):
  * cum= is `src/cod/ack`, CUMULATIVE totals (diag.rs). wake[.. ack=] is CUMULATIVE (wakes of the sender loop's ack arm). wait[.. n=] is PER PERIOD (loop iterations of the tick's period,
    reset every tick), so the iteration count over LATE is the SUM of n over the ticks after the first LATE tick, never last minus first.
  * m-bar is sources per ADVANCE: the debt is cancelled once per sender-loop iteration, and only when the cumulative ack advanced (`cumulative_advance` in `on_ack_advance`,
    sender_phases.rs). So
      m-bar LOWER bound  mbar_lo = d(cum) / sum(wait n)      every loop iteration is counted, including those without an advance, so this is <= m-bar.
      release batch      = d(cum) / d(wake ack)              sources per ACK-ARM wake. It bounds NOTHING: an advance can land on a non-ack wake, and an ack wake need not advance.
                                                              Reported because pool57_score registered it (the 195 / 60 c1d values), not as a bracket of m-bar.
    The cum field is the ACK field (sources actually acked; they differ from sent under shedding) for mbar_lo, and the SRC field (the registered form) is reported beside it. The per-tick
    range (median / min / max over consecutive LATE ticks) is the gauge range the spec asks for.
  * W1 bracket (spec 6.2, W1 = emitted / (r x sources)): lower W1 >= max(0, 1 - 1/(rr x mbar_lo)) with rr the median [SPAN] rr and the LOWER-bound m-bar (a larger m-bar only raises it);
    upper W1 <= (cod/src) / rr, cod including margin and request repairs, so it over-counts the proactive share.
  * eps_hat = the median per-path `pl=` of [DIAG] (loss_rate(), the per-ack EWMA) over LATE. The engine prints NO sigma^2 token (burst_variance_factor is computed in emit_source.rs and
    not echoed). [FDIAG] (the receiver's rf= ru= counts) needs RWM_FDIAG=1 on the server, which this plan does not set (it times every symbol); its last rf / ru are read when present.
  * cod/src         = sum(cod rate) / sum(src rate) over LATE ticks with src > 1000 sym/s (the DIAG rates); the cumulative form d(cum cod) / d(cum src) is reported beside it.
  * [SPAN]          = medians of rr, debt, a_star, delta, bud over LATE; the last cumulative shed total / budget-refused count; goodput = the summary's mbps (None on a DNF) and the median
    [DIAG] good= over LATE (a DNF row has no summary).
The sweep {1, 4, 16, 64, 256} of the spec: the report prints which grid points lie between the median per-loop value and the median release batch (ACK form).

Nothing here is scored and nothing voids a row (a missing token reads None and the smoke names it)."""
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import pool54_score as p54  # noqa: E402
import pool56_score as ps  # noqa: E402

LATE_T = 5.0                     # status 59: LATE is t >= 5 s (pool56's per-cell LATE_T is 15 s at c2 and is not used here)
SRC_MIN = 1000.0                 # cod/src is read over ticks with src above this (sym/s), as the registered 0.080 reading
GRID = (1, 4, 16, 64, 256)       # the spec's m-bar sweep
PERF_TIMEOUT_S = 150
LIVE_TOKENS = ("mbar_lo", "cum_src_d", "cum_ack_d", "wake_ack_d", "loop_n", "rel_src", "rel_ack", "loop_src", "loop_ack", "cod_src_rate", "cod_src_cum")
SPAN_TOKENS = ("rr", "debt", "a_star", "delta", "bud", "shed_total")      # the [SPAN] readouts the smoke requires

_CUM = re.compile(r" cum=(\d+)/(\d+)/(\d+)")
_WAKE_ACK = re.compile(r"wake\[[^\]]*? ack=(\d+)")
_WAIT_N = re.compile(r"wait\[[^\]]*? n=(\d+) ")
_SRC = re.compile(r" src=([\d.]+)sym/s")
_COD = re.compile(r" cod=([\d.]+)sym/s")
_PL0 = re.compile(r" p0:infl=\S+ .*? pl=([\d.]+)")
_FDIAG = re.compile(r"\[FDIAG\].* rf=(\d+) ru=(\d+)")
_GOOD = re.compile(r" good=([\d.]+)Mbit")
_SHED = re.compile(r" shed=(\d+)/(\d+)")
_OPT = r"(?:Some\(([-+\d.eE]+)\)|None)"
_SPAN = re.compile(r"\[SPAN\] t=([\d.]+)s ack=\d+ win=\[\d+,\d+\] wsize=\d+ a_star=" + _OPT + r" delta=" + _OPT +
                   r" owed=[-+\d.eE]+ rr=([-+\d.eE]+) debt=([-+\d.eE]+) retx_buf=\d+(.*)$")
_SPAN_SHED = re.compile(r" shed=(\d+)/(\d+) bud=([-+\d.eE]+) D=\d+ms")
SHED_ECHO_SENDER = "unified overload shedding ACTIVE ("
SHED_ECHO_RECEIVER = "unified overload shedding ACTIVE at receiver"
RELIABLE_ECHO = "reliable window mode (retain until acked)"
REALTIME_ECHO = "RWM_UNIFIED: Realtime rides the RLC span machine"


def med(v):
    v = sorted(x for x in v if x is not None)
    if not v:
        return None
    n = len(v)
    return v[n // 2] if n % 2 else (v[n // 2 - 1] + v[n // 2]) / 2


def _f(m, i):
    return float(m.group(i)) if m and m.group(i) is not None else None


def diag_ticks(cli):
    """[dict] of the client's [DIAG] ticks after the last warm-up marker: t, src / cod rates, cum (src, cod, ack), wake_ack (cumulative), n (per period), good, shed (total, denied)."""
    lines = list(cli or [])
    idx = [i for i, ln in enumerate(lines) if '"warmup":true' in ln]
    if idx:
        lines = lines[idx[-1] + 1:]
    out = []
    for ln in lines:
        mt = p54._T.match(ln)
        if not mt:
            continue
        cum, wk, wn, sr, cd, gd, sh = _CUM.search(ln), _WAKE_ACK.search(ln), _WAIT_N.search(ln), _SRC.search(ln), _COD.search(ln), _GOOD.search(ln), _SHED.search(ln)
        out.append({"t": float(mt.group(1)), "src": _f(sr, 1), "cod": _f(cd, 1), "good": _f(gd, 1), "pl": _f(_PL0.search(ln), 1),
                    "cum": tuple(int(x) for x in cum.groups()) if cum else None,
                    "wack": int(wk.group(1)) if wk else None, "n": int(wn.group(1)) if wn else None,
                    "shed": (int(sh.group(1)), int(sh.group(2))) if sh else None})
    return out


def span_lines(cli):
    """[dict] of the client's [SPAN] lines (own clock): t, a_star, delta, rr, debt and, when the shed law is armed, shed / denied / bud (else None)."""
    out = []
    for ln in cli or []:
        m = _SPAN.search(ln)
        if not m:
            continue
        sh = _SPAN_SHED.search(m.group(6))
        out.append({"t": float(m.group(1)), "a_star": _f(m, 2), "delta": _f(m, 3), "rr": float(m.group(4)), "debt": float(m.group(5)),
                    "shed": int(sh.group(1)) if sh else None, "denied": int(sh.group(2)) if sh else None, "bud": float(sh.group(3)) if sh else None})
    return out


def ratio(num, den):
    return num / den if (num is not None and den) else None


def row_metrics(cli, srv, drv, nbytes=None, cell="c2-100", mbps=None):
    """One row's readouts (`t59`). Never raises on a missing field; a missing token reads None."""
    tk = diag_ticks(cli)
    late = [d for d in tk if d["t"] >= LATE_T and (d["src"] or 0) > 0]
    sp_ = [s for s in span_lines(cli) if s["t"] >= LATE_T]
    m = {k: None for k in LIVE_TOKENS + ("rel_ack_med", "rel_ack_min", "rel_ack_max", "loop_ack_med", "loop_ack_min", "loop_ack_max", "good_med", "src_med", "cod_med", "eff_src_s",
                                         "grid_between", "late_n", "span_n")}
    m.update({k: None for k in ("rr", "debt", "a_star", "delta", "bud", "shed_total", "shed_denied", "shed_diag_total", "mbar_lo", "mbar_lo_src", "w1_lo", "w1_hi", "grid_above", "eps_hat")})
    m["late_n"], m["span_n"] = len(late), len(sp_)
    m["goodput_summary"] = mbps
    m["goodput_diag"] = med([d["good"] for d in late])
    m["good_med"] = m["goodput_diag"]
    m["src_med"], m["cod_med"] = med([d["src"] for d in late]), med([d["cod"] for d in late])
    m["eps_hat"] = med([d["pl"] for d in late])
    fd = [x for x in (_FDIAG.search(ln) for ln in srv or []) if x]
    m["fdiag_rf"], m["fdiag_ru"] = (int(fd[-1].group(1)), int(fd[-1].group(2))) if fd else (None, None)
    m["shed_armed_span"] = any(s["shed"] is not None for s in span_lines(cli))
    m["shed_armed_diag"] = any(d["shed"] is not None for d in tk)
    m["sender_shed_echo"] = any(SHED_ECHO_SENDER in ln for ln in cli or [])
    m["receiver_shed_echo"] = any(SHED_ECHO_RECEIVER in ln for ln in srv or [])
    if len(late) >= 2:
        a, b = late[0], late[-1]
        span = b["t"] - a["t"]
        if a["cum"] and b["cum"]:
            m["cum_src_d"], m["cum_ack_d"] = b["cum"][0] - a["cum"][0], b["cum"][2] - a["cum"][2]
            m["cum_cod_d"] = b["cum"][1] - a["cum"][1]
        if a["wack"] is not None and b["wack"] is not None:
            m["wake_ack_d"] = b["wack"] - a["wack"]
        ns = [d["n"] for d in late[1:]]
        m["loop_n"] = sum(ns) if ns and all(x is not None for x in ns) else None      # per-period counts: the periods AFTER the first LATE tick
        m["rel_src"], m["rel_ack"] = ratio(m["cum_src_d"], m["wake_ack_d"]), ratio(m["cum_ack_d"], m["wake_ack_d"])
        m["loop_src"], m["loop_ack"] = ratio(m["cum_src_d"], m["loop_n"]), ratio(m["cum_ack_d"], m["loop_n"])
        m["cod_src_cum"] = ratio(m.get("cum_cod_d"), m["cum_src_d"])
        m["eff_src_s"] = ratio(m["cum_src_d"], span)
        rel, loop = [], []
        for u, v in zip(late, late[1:]):
            if u["cum"] and v["cum"] and u["wack"] is not None and v["wack"] is not None and v["wack"] > u["wack"]:
                rel.append((v["cum"][2] - u["cum"][2]) / (v["wack"] - u["wack"]))
            if u["cum"] and v["cum"] and v["n"]:
                loop.append((v["cum"][2] - u["cum"][2]) / v["n"])
        m["rel_ack_med"], m["rel_ack_min"], m["rel_ack_max"] = med(rel), (min(rel) if rel else None), (max(rel) if rel else None)
        m["loop_ack_med"], m["loop_ack_min"], m["loop_ack_max"] = med(loop), (min(loop) if loop else None), (max(loop) if loop else None)
    hi = [d for d in late if (d["src"] or 0) > SRC_MIN and d["cod"] is not None]
    m["cod_src_rate"] = ratio(sum(d["cod"] for d in hi), sum(d["src"] for d in hi)) if hi else None
    m["cod_src_n"] = len(hi)
    for k in ("rr", "debt", "a_star", "delta", "bud"):
        m[k] = med([s[k] for s in sp_])
    shed = [s for s in sp_ if s["shed"] is not None]
    if shed:
        m["shed_total"], m["shed_denied"] = shed[-1]["shed"], shed[-1]["denied"]
    dsh = [d["shed"] for d in tk if d["shed"] is not None]
    m["shed_diag_total"] = dsh[-1][0] if dsh else None
    m["mbar_lo"], m["mbar_lo_src"] = m["loop_ack"], m["loop_src"]
    rr = m["rr"]
    m["w1_lo"] = max(0.0, 1.0 - 1.0 / (rr * m["mbar_lo"])) if (rr and m["mbar_lo"]) else None
    m["w1_hi"] = m["cod_src_rate"] / rr if (rr and m["cod_src_rate"] is not None) else None
    m["grid_above"] = [g for g in GRID if m["mbar_lo"] is not None and g >= m["mbar_lo"]]      # the sweep points not excluded by the lower bound
    return m


def tv(r, k):
    return (r.get("t59") or {}).get(k)


def missing(r):
    """The readouts a smoke row must carry (a None is a token the engine did not print with these flags)."""
    t = r.get("t59") or {}
    return [k for k in LIVE_TOKENS + SPAN_TOKENS if t.get(k) is None]


def is_void(row):
    return row["status"] != "LIVE"


def fmt(x, nd=1):
    return "-" if x is None else f"{x:.{nd}f}"


def smoke(path, out=print):
    import place38_parse as pp
    rows, _ = pp.rows_of([path])
    want = [(c, a) for c, _, a, _, _ in pp.plan59(smoke=True)]
    got = {(r["cell"], r["arm"]): r for r in rows}
    out(f"SMOKE59 rows={len(rows)}/{len(want)} (one evicting-seat c2 row; the values are not read, only the criteria below; a DNF is the expected outcome of an evicting seat and is not a failure)")
    bad = []

    def crit(name, fails):
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)

    crit("one row per smoke slot ran", [f"{c} {a}: no row" for c, a in want if (c, a) not in got])
    rs = [got[k] for k in want if k in got]
    for r in rs:
        t = r.get("t59") or {}
        out(f"  ROW {r['cell']} {r['arm']}: status={r['status']} dnf={r.get('dnf')} mbps={r.get('mbps')} wall={r.get('wall_s')} late_n={t.get('late_n')} span_n={t.get('span_n')} "
            f"rel_ack={fmt(t.get('rel_ack'), 2)} loop_ack={fmt(t.get('loop_ack'), 2)} cod/src={fmt(t.get('cod_src_rate'), 3)} rr={fmt(t.get('rr'), 3)} shed={t.get('shed_total')} "
            f"good={fmt(t.get('goodput_diag'), 1)}")
    crit("the evicting seat is armed on every row (sender 'unified overload shedding ACTIVE' echo, shed= on [SPAN], RWM_UNIFIED=1 RWM_UNIFIED_SHED=1 on both ends, no retain-until-acked echo)",
         [f"{r['cell']} {r['arm']}: {' '.join(x for x in r['problems'] if pp.is_evict_problem59(x))}" for r in rs if pp.is_evict_void59(r)])
    crit("every row ran the one seat's binary (LD_SHA)", [f"{r['cell']} {r['arm']}: {x}" for r in rs for x in r["problems"] if x.startswith("bin-sha")])
    crit("the Realtime hint reached both ends ([PIPE] hint=realtime, header hint=realtime, the Realtime span-machine echo)",
         [f"{r['cell']} {r['arm']}: {x}" for r in rs for x in r["problems"] if x.startswith(("pipe-echo", "header=", "no-pipe-echo", "window-line-missing"))])
    crit("every [DIAG] / [SPAN] readout computed on every row (cum=, wake[ack=], wait[n=], src=, cod=, rr, debt, a_star, delta, shed, bud: no None)",
         [f"{r['cell']} {r['arm']}: {', '.join(missing(r))}" for r in rs if missing(r)])
    for r in rs:
        out(f"  GO/NO-GO {r['cell']} {r['arm']}: rr median {fmt(tv(r, 'rr'), 3)}  a_star median {fmt(tv(r, 'a_star'), 1)}  mbar_lo {fmt(tv(r, 'mbar_lo'), 3)}  eps_hat {fmt(tv(r, 'eps_hat'), 4)}")
    crit("the debt loop runs: the median [SPAN] rr over LATE is > 0 (a seat with rr <= 0 never reaches the cancellation; nothing to read)",
         [f"{r['cell']} {r['arm']}: rr median {tv(r, 'rr')}" for r in rs if not (tv(r, "rr") is not None and tv(r, "rr") > 0)])
    late = []
    for r in rs:
        if r.get("rc") != 0:
            late.append(f"{r['cell']} {r['arm']}: rc={r.get('rc')} (a crash is not a DNF)")
        elif (r.get("wall_s") or 0) > PERF_TIMEOUT_S + 120:
            late.append(f"{r['cell']} {r['arm']}: wall {r.get('wall_s')} s")
        if r.get("bytes") != pp.BYTES59[r["cell"]]:
            late.append(f"{r['cell']} {r['arm']}: bytes={r.get('bytes')} (want {pp.BYTES59[r['cell']]})")
    crit(f"every smoke row returned (rc 0; a DNF at RWM_PERF_TIMEOUT_S={PERF_TIMEOUT_S} is accepted) at {pp.BYTES59[pp.CELL59] // 1000000} MB", late)
    for r in rs:
        out(f"  LIVENESS {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']}: {'LIVE' if r['status'] == 'LIVE' else 'VOID'} {' '.join(r['problems'])}")
    if bad:
        out("SMOKE-FAIL " + "; ".join(bad))
        return 1
    out("SMOKE-PASS")
    return 0


def _rng(rs, k, nd):
    v = [tv(r, k) for r in rs if tv(r, k) is not None]
    return "-" if not v else f"{med(v):.{nd}f} [{min(v):.{nd}f}-{max(v):.{nd}f}] n={len(v)}"


READOUTS = [
    ("mbar_lo  d(cum ack)/sum(wait n)  [<= m-bar]", "mbar_lo", 3), ("mbar_lo  d(cum src)/sum(wait n)  [<= m-bar]", "mbar_lo_src", 3),
    ("W1 lower  1-1/(rr x mbar_lo)", "w1_lo", 3), ("W1 upper  (cod/src)/rr", "w1_hi", 3),
    ("per-tick mbar_lo (ack): median", "loop_ack_med", 3), ("   min over ticks", "loop_ack_min", 3), ("   max over ticks", "loop_ack_max", 3),
    ("release batch d(cum src)/d(wake ack) [no bound]", "rel_src", 2), ("release batch d(cum ack)/d(wake ack) [no bound]", "rel_ack", 2),
    ("per-tick release batch (ack): median [no bound]", "rel_ack_med", 2), ("   min over ticks", "rel_ack_min", 2), ("   max over ticks", "rel_ack_max", 2),
    ("eps_hat (median p0 pl=)", "eps_hat", 4), ("FDIAG rf (server, needs RWM_FDIAG=1)", "fdiag_rf", 0), ("FDIAG ru", "fdiag_ru", 0),
    ("cod/src (DIAG rates, src > 1000/s)", "cod_src_rate", 3), ("cod/src (cumulative d cod / d src)", "cod_src_cum", 3),
    ("[SPAN] rr", "rr", 3), ("[SPAN] debt", "debt", 3), ("[SPAN] a_star", "a_star", 1), ("[SPAN] delta", "delta", 3), ("[SPAN] bud", "bud", 4),
    ("shed total (last [SPAN])", "shed_total", 0), ("shed budget-refused (last [SPAN])", "shed_denied", 0),
    ("src rate /s (median)", "src_med", 0), ("cod rate /s (median)", "cod_med", 0), ("goodput Mbit/s ([DIAG] good=, median)", "goodput_diag", 1), ("goodput Mbit/s (summary)", "goodput_summary", 1),
]


def score(paths, out=print):
    import place38_parse as pp
    rows_all, tokens = pp.rows_of(paths)
    rows_all = [r for r in rows_all if r["cell"] == pp.CELL59 and r["arm"] == pp.ARM59]
    cnt, replaced = ps.counting(rows_all)
    n_exp = pp.expect_rows59()
    voids = [r for r in cnt if is_void(r)]
    live = [r for r in cnt if not is_void(r)]
    out(f"EVICTING-SEAT m-bar READOUT (status 59), REPORTED ONLY: one seat (current main), {pp.CELL59} at {pp.BYTES59[pp.CELL59] // 1000000} MB, --protocol-hint realtime, NO --window-reliable, "
        f"RWM_UNIFIED=1, RWM_DIAG=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S={PERF_TIMEOUT_S}; LATE t >= {LATE_T:g} s; nothing here is scored")
    out(f"SLOTS {len(ps.slots(rows_all))}/{n_exp}  ROWS {len(rows_all)} (replaced {len(replaced)})  ABORT-TOKENS {' '.join(sorted({t for t in tokens if t in pp.ABORT_TOKENS})) or 'none'}")
    for r in replaced:
        out(f"  REPLACED {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} {' '.join(r['problems'])}")
    for r in voids:
        out(f"  VOID {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} rc={r['rc']} {' '.join(r['problems'])}")
    for r in cnt:
        if r.get("t59_error"):
            out(f"  T59-ERROR {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']}: {r['t59_error']}")
    out("ROWS (counting): seed rep | status DNF | release batch / per loop (ack field) | cod/src | rr | shed | goodput")
    for r in sorted(live, key=lambda x: (int(x["rep"]), x["seed"])):
        t = r.get("t59") or {}
        out(f"  s{r['seed']:<3} rep{r['rep']} {r['status']:<5} dnf={str(bool(r.get('dnf'))):<5} rel={fmt(t.get('rel_ack'), 2):>8} loop={fmt(t.get('loop_ack'), 2):>7} cod/src={fmt(t.get('cod_src_rate'), 3):>6} "
            f"rr={fmt(t.get('rr'), 3):>6} shed={t.get('shed_total')} good={fmt(t.get('goodput_diag'), 1)}")
    out(f"READOUTS over the {len(live)} live row(s): median [min-max] n")
    for label, k, nd in READOUTS:
        out(f"  {label:<42} {_rng(live, k, nd)}")
    lo, hi = med([tv(r, "mbar_lo") for r in live]), med([tv(r, "rel_ack") for r in live])
    wl, wh = med([tv(r, "w1_lo") for r in live]), med([tv(r, "w1_hi") for r in live])
    if lo is not None:
        out(f"m-bar LOWER BOUND (ack field, median over rows): m-bar >= {lo:.3f} (sources per loop iteration; only an upper limit is missing: no token bounds m-bar from above); "
            f"sweep points {{1, 4, 16, 64, 256}} not excluded by it: {', '.join(str(g) for g in GRID if g >= lo) or 'none'}")
        out(f"W1 BRACKET (medians over rows): {fmt(wl, 3)} <= W1 <= {fmt(wh, 3)}  (lower = max(0, 1 - 1/(rr x mbar_lo)); upper = (cod/src)/rr, cod includes margin and request repairs)")
    else:
        out("m-bar LOWER BOUND: not computable (no live row with the per-loop value)")
    n_dnf = sum(1 for r in live if r.get("dnf"))
    out(f"VERDICT59 REPORTED-ONLY slots={len(ps.slots(rows_all))}/{n_exp} live={len(live)} void={len(voids)} replaced={len(replaced)} dnf={n_dnf} mbar_lo_med={fmt(lo, 3)} w1_lo_med={fmt(wl, 3)} w1_hi_med={fmt(wh, 3)} release_ack_med={fmt(hi, 2)}")
    return 0


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd == "score59":
        return score(a)
    if cmd == "smoke59":
        return smoke(a[0])
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
