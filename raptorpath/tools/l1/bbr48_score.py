#!/usr/bin/env python3
"""Status 48b: the `bbr_rs` battery on the VM (plan 48): row readouts from the client log, the guard, and the outcome ladder.

    bbr48_score.py score <ledger> [<replacement ledger>]   R1..R4, the ladder, the last line `VERDICT48 <label>`
    bbr48_score.py dry <ledger>                            R1 (the §41c guard, place41b_score's code) and p_rest only: the offline dry run
    bbr48_score.py plan [smoke]                            plan 48's table (`cell store arm seed rep` per line)

Row side (`row_readouts`, called by place38_parse.make_row when LD_PLAN=48; its problems void the row):
  * BASE: the stock-BBR echo (`quinn congestion controller: BBR`), no `burst-robust BBR` echo, no `qm=` field, no `[QCC]` line.
  * CAND: the `RWM_QUIC_CC=bbr_rs: burst-robust BBR` echo (no stock echo), a `[QCC]` line with `qm=` in {S, D, B, R} on EVERY path block, and a
    `[QCC]` block for every leg of the cell.
  * both: >= 20 s of post-warm-up [DIAG] lines (t >= 1 s, last t - first t).
  Formats are read from src/net/diag.rs, src/transport/quic.rs and bbr_rs.rs of `bbrfix48-impl`:
    `[QCC] t=<s>s p<id>:qm=<S|D|B|R> qrt_us=<n> qsr_us=<n> qbw=<n> qmin_us=<n> qcns=<last>/<reads>/<mean>` (one block per path),
    `[DIAG] ... dgq<id>[hand=<n> tx=<n> full=<n> err=<n> sp=<n>] ... p<id>:infl=..  rtt=<ms>/wrtt=<ms>/.. qcwnd=<B> qce=<n> qlp=<lost>/<sent> | ANCHOR ..`.

Interpretations the spec leaves open (listed in the harness report):
  * R2/R3 are read over the lines at t >= 1 s on which the leg is ACTIVE (its qlp sent counter advanced since the previous [DIAG] line);
  * a row's R2b is the MAX over its active legs (a collapse on either leg is the row's collapse); the per-(cell, leg) medians are printed too, and a
    line `R2B-LEG-READING` says whether reading the NOT-CONTROLLED median per leg would change the call;
  * R3's row statistic is the median over the row's active lines; arm and cell figures are means (and p50 = median) of the row medians;
  * the tail clause's median is over the CAND rows that completed; a CAND DNF or crash row counts as goodput 0 (fails it);
  * c8's slow leg is leg 1 (the c3 leg).
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import place38_parse as pp  # noqa: E402
import place41b_score as g41  # noqa: E402
import stage3_parse as sp  # noqa: E402

STOCK_ECHO = "quinn congestion controller: BBR"      # transport/quic.rs, QuicCcMode::Bbr
BRS_ECHO = "burst-robust BBR"                        # transport/quic.rs, QuicCcMode::BbrRs
QM_VALUES = ("S", "D", "B", "R")
MIN_SPAN_S = 20.0
WARM_S = pp.WARM_S
B_MIN = 98                       # R2b: the leg's B = hand - tx at least 98 datagrams
COLLAPSE_FRAC = 0.5              # R2b: qcwnd < 0.5 x BDP_leg
R2B_MEDIAN_MAX, R2B_ROW_MAX = 0.05, 0.25
TAIL_FRAC = 0.8
COST_GOODPUT, COST_WRTT = 0.90, 0.7
SLOW_LEG = {"c8-100": 1}         # c8's slow leg is the c3 leg
GUARD_CELL_NAMES = [c for c, _ in g41.CELLS_GUARD]
REST_EXCLUDE = (("c2-100", "goodput"), ("c8-100", "goodput"))

_QCC_BLOCK = re.compile(r"\bp(\d+):qm=(\S*) qrt_us=(\d+) qsr_us=(\d+) qbw=(\d+) qmin_us=(\d+)(?: qcns=(\d+)/(\d+)/(\d+))?")
_DGQ = re.compile(r"\bdgq(\d+)\[hand=(\d+) tx=(\d+) full=(\d+)")
_DGEV = re.compile(r"\bdgev(\d+)=(\d+)")
_QLP = re.compile(r"\bqlp=(\d+)/(\d+)")
_QCWND = re.compile(r"\bqcwnd=(\d+)")


def _after_warmup(lines):
    idx = [i for i, ln in enumerate(lines) if '"warmup":true' in ln]
    return lines[idx[-1] + 1:] if idx else list(lines)


def diag_series(cli):
    """[(t, {leg: dict(sent, qcwnd, rtt, wrtt)}, {leg: (hand, tx, full)}, {leg: dgev})] for the measured run's [DIAG] lines."""
    out = []
    for ln in _after_warmup(list(cli or [])):
        m = pp._DIAG_T.match(ln)
        if not m:
            continue
        legs = {}
        for part in re.split(r"(?= p\d+:infl=)", ln)[1:]:
            g, q, c = pp._LEG.search(part), _QLP.search(part), _QCWND.search(part)
            if g:       # the older [DIAG] shape has no q* gauges: the leg is kept (the span reads it), its sent / qcwnd are None
                legs[int(g.group(1))] = {"sent": int(q.group(2)) if q else None, "qcwnd": int(c.group(1)) if c else None,
                                         "rtt": int(g.group(3)), "wrtt": int(g.group(4))}
        if not legs:
            continue
        dgq = {int(a): (int(b), int(c), int(d)) for a, b, c, d in _DGQ.findall(ln)}
        dgev = {int(a): int(b) for a, b in _DGEV.findall(ln)}
        out.append((float(m.group(1)), legs, dgq, dgev))
    return out


def qcc_series(cli):
    """[(t, {leg: dict(qm, qrt_us, qsr_us, qbw, qmin_us)})] for the `[QCC] t=` lines of the measured run."""
    out = []
    for ln in _after_warmup(list(cli or [])):
        if not ln.startswith("[QCC] t="):
            continue
        m = re.match(r"\[QCC\] t=([\d.]+)s", ln)
        blocks = {int(b.group(1)): {"qm": b.group(2), "qrt_us": int(b.group(3)), "qsr_us": int(b.group(4)), "qbw": int(b.group(5)), "qmin_us": int(b.group(6)),
                                    "qcns_mean": int(b.group(9)) if b.group(9) else None} for b in _QCC_BLOCK.finditer(ln)}
        # a block that does not parse as a full `qm=<x> qrt_us=..` block still counts as a block with an unreadable qm
        bad = len(re.findall(r"\bp\d+:", ln)) - len(blocks)
        out.append((float(m.group(1)) if m else -1.0, blocks, bad))
    return out


def _med(v):
    v = [x for x in v if x is not None]
    return lc.med(v) if v else None


def row_readouts(cli, srv, cell, arm):
    """-> (b48 dict, problems). The witnesses of §48 Step 1 and the per-row inputs of R2, R3 and R4."""
    cli = list(cli or [])
    nleg = sp.N_LEGS[cell]
    bdp = pp.bdp_bytes(cell)
    probs = []
    stock, brs = any(STOCK_ECHO in ln for ln in cli), any(BRS_ECHO in ln for ln in cli)
    has_qm = any(re.search(r"\bqm=", ln) for ln in cli + list(srv or []))
    has_qcc_line = any(ln.startswith("[QCC]") for ln in cli)
    series = diag_series(cli)
    qcc = qcc_series(cli)
    post = [x for x in series if x[0] >= WARM_S]
    span = (post[-1][0] - post[0][0]) if post else 0.0
    if span < MIN_SPAN_S:
        probs.append(f"diag-span={span:.1f}s<{MIN_SPAN_S:.0f}s")
    if arm == "BASE":
        if not stock:
            probs.append("base-no-stock-bbr-echo")
        if brs:
            probs.append("base-has-burst-robust-echo")
        if has_qm or has_qcc_line:
            probs.append("base-has-qm-or-QCC")
    elif arm == "CAND":
        if not brs:
            probs.append("cand-no-burst-robust-echo")
        if stock:
            probs.append("cand-has-stock-bbr-echo")
        bad_qm = sorted({b["qm"] for _, bl, _ in qcc for b in bl.values() if b["qm"] not in QM_VALUES})
        unparsed = sum(nb for _, _, nb in qcc)
        if bad_qm or unparsed:
            probs.append(f"qm-not-SDBR={bad_qm or '-'} unparsed-blocks={unparsed}")
        if not qcc:
            probs.append("cand-no-QCC-line")
        for i in range(nleg):
            if not any(i in bl for _, bl, _ in qcc):
                probs.append(f"cand-no-QCC-block-p{i}")
    legs = []
    for i in range(nleg):
        prev_sent, act = None, []
        missing_dgq = 0
        for t, L, dgq, _ in series:
            if i not in L:
                continue
            s = L[i]["sent"]
            adv = prev_sent is not None and s is not None and s > prev_sent
            prev_sent = s
            if t < WARM_S or not adv or L[i]["qcwnd"] is None:
                continue
            hand_tx = dgq.get(i)
            if hand_tx is None:
                missing_dgq += 1
            act.append((L[i]["qcwnd"], (hand_tx[0] - hand_tx[1]) if hand_tx else None, L[i]["wrtt"], L[i]["rtt"]))
        if not any(L[i]["qcwnd"] is not None and L[i]["sent"] is not None for _, L, _, _ in series if i in L):
            probs.append(f"no-q-gauges-p{i}")
        if missing_dgq:
            probs.append(f"no-dgq-on-{missing_dgq}-active-lines-p{i}")
        n = len(act)
        below = sum(1 for q, _, _, _ in act if q < bdp[i])
        coll = sum(1 for q, b, _, _ in act if q < COLLAPSE_FRAC * bdp[i] and b is not None and b >= B_MIN)
        last = series[-1] if series else None
        dq = (last[2].get(i) if last else None)
        legs.append({"active_n": n, "r2a": below / n if n else None, "r2b": coll / n if n else None,
                     "B_med": _med([b for _, b, _, _ in act]), "wrtt_med": _med([w for _, _, w, _ in act]), "rtt_med": _med([r for _, _, _, r in act]),
                     "dgq_full": dq[2] if dq else None, "dgev": (last[3].get(i) if last else None), "bdp": bdp[i]})
    r2bs = [lg["r2b"] for lg in legs if lg["r2b"] is not None]
    qlegs = []
    for i in range(nleg):
        bl = [b[i] for t, b, _ in qcc if i in b and t >= WARM_S]
        occ = {m: (sum(1 for b in bl if b["qm"] == m) / len(bl) if bl else None) for m in QM_VALUES}
        qlegs.append({"n": len(bl), "occ": occ, "qrt_us_med": _med([b["qrt_us"] for b in bl]), "qsr_us_med": _med([b["qsr_us"] for b in bl]),
                      "margin_med": _med([2.0 * b["qrt_us"] / b["qsr_us"] for b in bl if b["qsr_us"] > 0]), "qmin_us_med": _med([b["qmin_us"] for b in bl]),
                      "qcns_mean": _med([b["qcns_mean"] for b in bl])})
    return {"span_s": round(span, 2), "n_diag": len(post), "legs": legs, "r2b": max(r2bs) if r2bs else None, "qcc": qlegs,
            "echo": {"stock": stock, "burst_robust": brs, "qm_field": has_qm, "qcc_line": has_qcc_line}}, probs


# ── scorer ────────────────────────────────────────────────────────────────
def _live(rows, cell, arm):
    return [r for r in rows if r["cell"] == cell and r["arm"] == arm and r["status"] == "LIVE"]


def _slot_rows(rows, cell, arm):
    """The slots that count at a cell: LIVE rows (DNF included) and crash rows (never replaced; the worst rank)."""
    return [r for r in rows if r["cell"] == cell and r["arm"] == arm and (r["status"] == "LIVE" or r["status"] in g41.CRASH_STATUSES)]


def p_rest(rows, n_perm=None):
    """The same statistic and relabellings as the guard over the 24 clauses that exclude the goodput clauses of c2 and c8."""
    kw = {"n_perm": n_perm} if n_perm else {}
    return g41.statistic_rank(rows, "BASE", "CAND", cells=g41.CELLS_GUARD, exclude=REST_EXCLUDE, design=g41.GUARD_DESIGN, **kw)


def _fm(x, nd=3):
    return "-" if x is None else f"{x:.{nd}f}"


def _mm(v, nd=3):
    return pp.med_mm(v, nd)


def r2b_table(rows, out):
    """R2a / R2b per cell and arm; returns the NOT-CONTROLLED reading {not_controlled, why, unreadable, leg_reading_differs}."""
    why, unread, leg_alt = [], [], []
    out("R2 (the §47 witness): per row and ACTIVE leg (qlp sent advanced), lines at t >= 1 s. BDP_leg: c1 250000 B, c2 125000, c3 100000")
    out("  R2a = share with qcwnd < BDP_leg (reported both arms, not scored); R2b = share with qcwnd < 0.5 x BDP_leg AND B = hand - tx >= 98 (scored)")
    for cell in GUARD_CELL_NAMES:
        for arm in ("BASE", "CAND"):
            rs = [r for r in _live(rows, cell, arm) if r.get("b48")]
            if not rs:
                continue
            nleg = sp.N_LEGS[cell]
            for i in range(nleg):
                a = [r["b48"]["legs"][i]["r2a"] for r in rs]
                b = [r["b48"]["legs"][i]["r2b"] for r in rs]
                out(f"  {cell} {arm:<4} leg{i}: R2a {_mm(a)}  R2b {_mm(b)}  active lines {_mm([r['b48']['legs'][i]['active_n'] for r in rs], 0)}")
            out(f"  {cell} {arm:<4} row R2b (max over active legs): {_mm([r['b48']['r2b'] for r in rs])}")
    out("")
    for cell in GUARD_CELL_NAMES:
        rs = _live(rows, cell, "CAND")
        v = [r["b48"]["r2b"] for r in rs if r.get("b48") and r["b48"].get("r2b") is not None]
        if len(v) < len(rs) or not rs:
            unread.append(f"{cell}: {len(v)} of {len(rs)} CAND rows have an R2b")
        if v:
            m = lc.med(v)
            hi = [(r["seed"], r["rep"], r["b48"]["r2b"]) for r in rs if r.get("b48") and (r["b48"].get("r2b") or 0) > R2B_ROW_MAX]
            out(f"  NOT-CONTROLLED test {cell}: median CAND R2b {m:.4f} (limit {R2B_MEDIAN_MAX}); rows above {R2B_ROW_MAX}: {hi or 'none'}")
            if m > R2B_MEDIAN_MAX:
                why.append(f"{cell} median R2b {m:.4f} > {R2B_MEDIAN_MAX}")
            for s, rp, x in hi:
                why.append(f"{cell} s{s} r{rp} R2b {x:.4f} > {R2B_ROW_MAX}")
            for i in range(sp.N_LEGS[cell]):
                lv = [r["b48"]["legs"][i]["r2b"] for r in rs if r.get("b48") and r["b48"]["legs"][i]["r2b"] is not None]
                if lv and lc.med(lv) > R2B_MEDIAN_MAX and m <= R2B_MEDIAN_MAX:
                    leg_alt.append(f"{cell} leg{i}")
    nc = bool(why)
    out(f"R2B-LEG-READING: reading the median test per leg instead of on the row maximum "
        + (f"would ALSO call NOT-CONTROLLED at {', '.join(leg_alt)}" if leg_alt else "changes nothing")
        + " (the row maximum is the scored reading; per-leg medians never exceed the median of the row maximum)")
    return {"not_controlled": nc, "why": why, "unreadable": unread}


def r3_table(rows, out):
    out("R3 (authority and delay): per leg, row medians of B = hand - tx, wrtt (quinn wire srtt, ms) and A_x = app-echo rtt (ms); mean over rows [and p50 of the row medians]")
    for cell in GUARD_CELL_NAMES:
        for arm in ("BASE", "CAND"):
            rs = [r for r in _live(rows, cell, arm) if r.get("b48")]
            for i in range(sp.N_LEGS[cell] if rs else 0):
                cols = {k: [r["b48"]["legs"][i][k] for r in rs] for k in ("B_med", "wrtt_med", "rtt_med")}
                def mean_p50(v):
                    v = [x for x in v if x is not None]
                    return "-" if not v else f"mean {sum(v) / len(v):.1f} p50 {lc.med(v):.1f}"
                out(f"  {cell} {arm:<4} leg{i}: B {mean_p50(cols['B_med'])} | wrtt {mean_p50(cols['wrtt_med'])} | A_x {mean_p50(cols['rtt_med'])} (n={len(rs)})")


def r4_table(rows, out):
    out("R4 (CAND bind gauges, [QCC]): per leg qm occupancy (share of blocks at t >= 1 s), median qrt_us (m-hat; the netem base: c1 2 ms, c2 10 ms, c3 40 ms), "
        "median loop margin 2*qrt_us/qsr_us, median qmin_us beside m-hat")
    for cell in GUARD_CELL_NAMES:
        rs = [r for r in _live(rows, cell, "CAND") if r.get("b48")]
        for i in range(sp.N_LEGS[cell] if rs else 0):
            q = [r["b48"]["qcc"][i] for r in rs]
            occ = " ".join(f"{m}={_fm(sum(x['occ'][m] or 0 for x in q) / len(q), 3)}" for m in QM_VALUES)
            out(f"  {cell} CAND leg{i}: qm {occ} | qrt_us {_mm([x['qrt_us_med'] for x in q], 0)} | margin {_mm([x['margin_med'] for x in q], 2)} "
                f"| qmin_us {_mm([x['qmin_us_med'] for x in q], 0)} | qcns mean {_mm([x['qcns_mean'] for x in q], 0)}")


def eviction_table(rows, out):
    out("DATAGRAM EVICTIONS per row (reported): last dgq full= and dgev (hand - tx) per leg, median [min-max] over rows")
    for cell in GUARD_CELL_NAMES:
        for arm in ("BASE", "CAND"):
            rs = [r for r in _live(rows, cell, arm) if r.get("b48")]
            for i in range(sp.N_LEGS[cell] if rs else 0):
                out(f"  {cell} {arm:<4} leg{i}: dgq full {_mm([r['b48']['legs'][i]['dgq_full'] for r in rs], 0)} | dgev {_mm([r['b48']['legs'][i]['dgev'] for r in rs], 0)}")


def tail_clause(rows, out):
    """CAND's lowest row (a DNF or crash counts as 0) against 0.8 x CAND's cell median over its completed rows. -> (holds at every cell, [failing cells])"""
    bad = []
    out(f"TAIL CLAUSE (CAND's lowest row >= {TAIL_FRAC} x CAND's cell median; a CAND DNF or crash row fails it):")
    for cell in GUARD_CELL_NAMES:
        sl = _slot_rows(rows, cell, "CAND")
        done = [r["mbps"] for r in sl if r["status"] == "LIVE" and not r.get("dnf") and r.get("mbps") is not None]
        m = _med(done)
        if m is None or not sl:
            bad.append(cell)
            out(f"  {cell}: no completed CAND row -> FAILS")
            continue
        lows = [(r["mbps"] if (r["status"] == "LIVE" and not r.get("dnf") and r.get("mbps") is not None) else 0.0, r) for r in sl]
        lo, lr = min(lows, key=lambda x: x[0])
        ok = lo >= TAIL_FRAC * m
        out(f"  {cell}: lowest {lo:.1f} Mbit/s (s{lr['seed']} r{lr['rep']} {'DNF' if lr.get('dnf') else lr['status']}) = {lo / m:.3f} x CAND median {m:.1f} -> {'holds' if ok else 'FAILS'}")
        if not ok:
            bad.append(cell)
    return not bad, bad


def base_collapse(rows, out):
    out("BASE-COLLAPSE (reported): BASE DNF / crash rows and R2b per BASE row")
    for cell in GUARD_CELL_NAMES:
        sl = _slot_rows(rows, cell, "BASE")
        dn = [f"s{r['seed']} r{r['rep']} {'DNF' if r.get('dnf') else r['status']}" for r in sl if r.get("dnf") or r["status"] != "LIVE"]
        out(f"  {cell}: BASE DNF/crash rows {len(dn)} of {len(sl)}: {', '.join(dn) or 'none'}")
        out("    BASE R2b per row: " + (" ".join(f"s{r['seed']}r{r['rep']}={_fm((r.get('b48') or {}).get('r2b'), 3)}" for r in _live(rows, cell, "BASE")) or "-"))


def _goodput_med(rows, cell, arm):
    return _med([r["mbps"] for r in _live(rows, cell, arm) if not r.get("dnf") and r.get("mbps") is not None])


def _wrtt_p50(rows, cell, arm, leg):
    return _med([r["b48"]["legs"][leg]["wrtt_med"] for r in _live(rows, cell, arm) if r.get("b48")])


def ladder(rows, r1_pass, p_rest_value, complete, nc, tail_ok, out, no_verdict=None):
    """§48's outcome ladder, applied in order. -> label."""
    if no_verdict:
        return "NO VERDICT"
    if not complete:
        return "NO VERDICT"
    if nc["unreadable"]:
        out("  R2b unreadable: " + "; ".join(nc["unreadable"]))
        return "NO VERDICT"
    if nc["not_controlled"]:
        out("  1. NOT-CONTROLLED: " + "; ".join(nc["why"]))
        return "NOT-CONTROLLED"
    out("  1. NOT-CONTROLLED: no (every cell's CAND median R2b <= 0.05 and no CAND row above 0.25)")
    if r1_pass and tail_ok:
        out("  2. PROMOTE-ELIGIBLE: R1 PASS and the tail clause holds at every cell")
        return "PROMOTE-ELIGIBLE"
    out(f"  2. PROMOTE-ELIGIBLE: no (R1 {'PASS' if r1_pass else 'FAIL'}, tail clause {'holds' if tail_ok else 'FAILS'})")
    if not r1_pass and p_rest_value > g41.P_PASS:
        conds = []
        for cell in ("c2-100", "c8-100"):
            c, b = _goodput_med(rows, cell, "CAND"), _goodput_med(rows, cell, "BASE")
            ok = c is not None and b is not None and c >= COST_GOODPUT * b
            conds.append(ok)
            out(f"     goodput median CAND/BASE at {cell}: {_fm(c, 1)} / {_fm(b, 1)} = {_fm(c / b if (c is not None and b) else None)} (need >= {COST_GOODPUT}) -> {'ok' if ok else 'NO'}")
        for cell, leg, nm in (("c2-100", 0, "c2"), ("c8-100", SLOW_LEG["c8-100"], "c8 slow leg")):
            c, b = _wrtt_p50(rows, cell, "CAND", leg), _wrtt_p50(rows, cell, "BASE", leg)
            ok = c is not None and b is not None and c <= COST_WRTT * b
            conds.append(ok)
            out(f"     wrtt p50 CAND/BASE at {nm}: {_fm(c, 1)} / {_fm(b, 1)} ms = {_fm(c / b if (c is not None and b) else None)} (need <= {COST_WRTT}) -> {'ok' if ok else 'NO'}")
        conds.append(tail_ok)
        out(f"     tail clause -> {'ok' if tail_ok else 'NO'}")
        if all(conds):
            out("  3. CONTROLLED-WITH-COST: R1 fails, p_rest > 0.05 and the three conditions hold (a measured trade, for the user to rule on)")
            return "CONTROLLED-WITH-COST"
        out("  3. CONTROLLED-WITH-COST: no (a condition fails)")
    else:
        out(f"  3. CONTROLLED-WITH-COST: no ({'R1 passed' if r1_pass else 'p_rest = %.4f <= 0.05' % p_rest_value})")
    out("  4. REGRESSION")
    return "REGRESSION"


def score(paths, out=print, dry=False):
    rows_all, tokens = pp.rows_of(paths)
    rows, rinfo = pp.apply_replacements(rows_all)
    aborted = sorted({t for t in tokens if t in pp.ABORT_TOKENS})
    out("BBR_RS BATTERY (status 48b): BASE = main at launch (quinn BBR, n = 12 per cell), CAND = BASE + the §48 commits with RWM_QUIC_CC=bbr_rs (n = 12 per cell); "
        "cells c1s-400 / c1d-400 (5 GB), c2-100 / c8-100 (600 MB)")
    extra = f" (+{rinfo['n_extra']} replacement row{'s' if rinfo['n_extra'] != 1 else ''} in the ledger)" if rinfo["n_extra"] else ""
    out(f"ROWS {rinfo['n_slots']}/{pp.EXPECT_ROWS41C}{extra}; ABORT-TOKENS {' '.join(aborted) or 'none'}")
    out("STATUS-TABLE (every invocation row)")
    for st in ("LIVE", "NO_DATA", "VOID-RC", "VOID-COTENANT", "VOID-ONDISK", "CONTAMINATED", "WITNESS-FAIL"):
        out(f"  {st:<13} {sum(1 for r in rows if r['status'] == st)}")
    for r in rows_all:
        if r["status"] != "LIVE" or r.get("io_high"):
            out(f"  NONLIVE {r['cell']}@{r['store']} {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} rc={r['rc']} {' '.join(r['problems'])} io_full10={r.get('io_full10')}")
    sl = lambda k: f"{k[0]} {k[1]} s{k[2]} r{k[3]}"  # noqa: E731
    out(f"REPLACED SLOTS: {len(rinfo['replaced'])} of at most {pp.MAX_REPLACEMENTS} (slots voided by a witness: {len(rinfo['void_slots'])})" + ("" if rinfo["replaced"] else " - none"))
    for k, o, n in rinfo["replaced"]:
        out(f"  {sl(k)} {o} -> {n}")
    unrep = [k for k in rinfo["void_slots"] if k not in {x[0] for x in rinfo["replaced"]}]
    if unrep:
        out("  voided and NOT replaced: " + ", ".join(sl(k) for k in unrep))
    reason = (f"more than {pp.MAX_REPLACEMENTS} voided slots ({len(rinfo['void_slots'])}): beyond the replacement cap" if rinfo["too_many"] else None)
    out("")
    out("R1 (the guard): §41c's rank-minp over 26 clauses, place41b_score.report_guard unchanged")
    res = g41.report_guard(rows, out, g41.GUARD_DESIGN, no_verdict_reason=reason)
    out("")
    pr = p_rest(rows)
    out(f"P_REST (status 48b item 2): the same statistic and relabellings over the {len(pr['p_true'])} clauses without the goodput clauses of c2 and c8")
    out(f"  min-p (true) = {pr['T_true']:.5f} at {pr['argmin'][0]} {pr['argmin'][1]}")
    out(f"  p_rest = {pr['p']:.4f}")
    sys.stdout.flush()
    if dry:
        return {"res": res, "p_rest": pr}
    out("")
    tail_ok, tail_bad = tail_clause(rows, out)
    out("")
    base_collapse(rows, out)
    out("")
    nc = r2b_table(rows, out)
    out("")
    r3_table(rows, out)
    out("")
    r4_table(rows, out)
    out("")
    eviction_table(rows, out)
    out("")
    complete = res["gate"] != "NO VERDICT" and not pr["incomplete"]
    out("LADDER (§48, applied in order)")
    if aborted:
        label = "NO VERDICT"
        out(f"  abort tokens {', '.join(aborted)}")
    else:
        label = ladder(rows, res["gate"] == "PASS", pr["p"], complete, nc, tail_ok, out, no_verdict=reason)
    out("REGISTERED PREDICTION (status 48b item 7): CONTROLLED-WITH-COST (failing R1 on c2/c8 goodput, wrtt at c2 <= 0.3 x BASE)")
    out("RESOLUTION (reported only) is NOT computed here. On the host, from the kept ledger:  "
        f"python3 raptorpath/tools/l1/place38_parse.py resolution41c {' '.join(paths)}")
    out(f"VERDICT48 {label}")
    sys.stdout.flush()
    return {"label": label, "res": res, "p_rest": pr, "r2b": nc}


def smoke48(path):
    """rc 0 iff the CAND check row (c8, two legs) and the BASE check row are LIVE: for CAND a `qm=` in {S,D,B,R} on every path block and a `[QCC]` block per leg
    (already row witnesses, so a LIVE CAND row has them); otherwise ABORT-SMOKE. Unscored."""
    rows, _ = pp.rows_of([path])
    got = {(r["cell"], r["arm"]): r for r in rows}
    bad = []
    for cell, store, arm, _, _ in pp.SMOKE48:
        r = got.get((cell, arm))
        if r is None:
            bad.append(f"{arm}: no row")
            continue
        miss = [] if r["status"] == "LIVE" else [r["status"]]
        b = r.get("b48")
        if not b:
            miss.append("no-b48")
        elif arm == "CAND":
            if not all(q["n"] > 0 for q in b["qcc"]) or len(b["qcc"]) != sp.N_LEGS[cell]:
                miss.append("no-QCC-block-per-leg")
            if not b["echo"]["burst_robust"] or not b["echo"]["qcc_line"]:
                miss.append("no-echo-or-QCC")
        print(f"SMOKE48 {cell} {arm}: status={r['status']} mbps={r.get('mbps')} span={(b or {}).get('span_s')} "
              f"qcc_legs={[ (q['n'], q['occ']) for q in (b or {}).get('qcc', [])]} {' '.join(r['problems'] + miss)}")
        if miss:
            bad.append(f"{arm}: {miss}")
    if bad:
        print("SMOKE-FAIL " + "; ".join(bad))
        return 1
    print("SMOKE-PASS")
    return 0


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd == "plan":
        for c, s, ar, seed, rep in pp.plan48(smoke=bool(a) and a[0] == "smoke"):
            print(f"{c} {s} {ar} {seed} {rep}")
        return 0
    if cmd == "score":
        score(a)
        return 0
    if cmd == "dry":
        score(a, dry=True)
        return 0
    if cmd == "smoke":
        return smoke48(a[0])
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
