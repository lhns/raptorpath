#!/usr/bin/env python3
"""Plan 67 (VM batch 1, docs/derivations/vm-batch-1-registration.md sections 3, 4, 5.1, 9): the row parser, the witnesses and labels, every clause and readout, the smokes, block1 and scoreT.
The tables and constants are plan67.py's; the synthetic log shapes are s67_fixtures.py's.

    s67_score.py row <plan> <cell> <arm> <seed> <rep> <rc> <wall> <drv> <cli> <srv> <cot> <sha>    one `P67ROW {json}` line (env LD_* as place38_parse; LD_SHA_<SEAT> = the seat's expected sha)
    s67_score.py score <plan> <ledger> [<repl ledger>]        the stage's score text (plan: 67a 67r 67p 67o 67d), ending in its VERDICT lines
    s67_score.py smoke <plan> <smoke.log>                     PASS / FAIL criteria, then SMOKE-PASS | SMOKE-FAIL (rc 0 / 1)
    s67_score.py block1 <plan> <ledger>                       rc 0 iff every arm's FIRST row is LIVE with its precondition witnesses
    s67_score.py tun <ledger> | smokeT <ledger>               stage T
    s67_score.py scoreg <g1|g2> <ld.log> [<repl.log>] | smokeg <g1|g2> <smoke.log>      stage G (plan 41d's P38ROW ledgers; score41d's output byte for byte, then the G witness table and GATE-67G<n>)
    hooks for place38_parse (stage G): g_row(row, arm, cli, srv, drv, sha, sha_cand, sha_base, which), g_status(row), score67g(paths, out, which), smoke67g(path, which)
    rows_of(paths) -> (rows, tokens): the P67ROW lines in order and the ABORT tokens.
Status precedence on a row: infrastructure voids (VOID-COTENANT, VOID-ONDISK, CONTAMINATED, VOID-SEAT = a seat / sha problem, VOID-RC / NO_DATA crashes), S06-FAIL (outcome witnesses on S2 rows, never replaced),
WITNESS-FAIL / VOID-WITNESS (a precondition witness: replaced once), VOID-SHORT, VOID-RHO (R1), LIVE. A DNF is LIVE with dnf true (it keeps its worst rank; precondition witnesses never void it)."""
import itertools
import json
import math
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import plan67  # noqa: E402
import place38_parse as pp  # noqa: E402
import place41b_score as pb  # noqa: E402
import stage3_parse as sp  # noqa: E402
from threadp1_parse import inorder_delay_stats, last_tick_s  # noqa: E402

C2, C8 = plan67.C2, plan67.C8
CELLS_A = [(C2, "def"), (C8, "def")]
INFRA = ("VOID-COTENANT", "VOID-ONDISK", "CONTAMINATED", "VOID-SEAT")
CRASH = ("VOID-RC", "NO_DATA")
GAUGE4 = ("scw", "plx", "rlaw", "rsat")                       # the four gauge prints of registration 3.2 (pfec is read beside them)
REALTIME_ECHO = "RWM_UNIFIED: Realtime rides the RLC span machine"
HB_BELOW_FLOOR = {C2: 854, C8: 122}                            # H-B's below-floor level: c2 from registration 4.2, c8 = H-B's median belowfl in docs/l1-raw/s51b/recopy/acct-c8.txt (122)
FLOOR_FRAC = 0.25
D80 = {C2: 0.08, C8: 0.12}                                     # the owner flag's d80 (registration 4.2: 8 % c2, 12 % c8)
T_TAIL = 1e-5 * 0.5 / 0.5                                      # DERIVED (Addendum 3): t_tail = 1e-5 * 0.5 / delta at the preset delta = 0.5
EXACT_CAP = 200000                                             # the exact enumeration's cap on labellings per cell (else the guard's seeded Monte Carlo, said so)
NO_ACCT = "no [ACCT]/[ACCT-RX] line"
ACCT_KEYS = ("reliable", "S_sent", "A", "H", "E", "O_serv", "O_strand", "O_max", "closes", "inert_sweeps", "sweeps_nosend", "sweeps", "oldest_unchanged", "oldest_unchanged_max", "O_hwm",
             "cap_at_hwm", "store_cap", "shed_slot", "shed_nack", "shed_visit_slot", "shed_visit_nack", "clamp_evals", "clamp_excl")
_RLAW = re.compile(r"a:(core|burst|mass)/(sat|cap|free),l:(core|burst|mass)/(sat|cap|free),b=([0-9.]+)")
_PACE_ECHO = re.compile(r"perf offered-load pacing ACTIVE \(RWM_PERF_RATE_MBIT=(\S+?)\)")
_SHED = re.compile(r"(?:^|\s)shed=(\d+)/(\d+)")
_WIN = re.compile(r"window_reliable: (true|false)")


def _register():
    arms = set(plan67.SEAT_ORDER) | {f"{s}P{p}" for s in ("S1", "S2") for p in plan67.POOLS} | {"S1D", "S2D"}
    for a in arms:
        pp.AFAM.setdefault(a, "CAND")           # the seat's own witnesses are this module's; place38_parse reads them as a promoted build's


_register()


def _num(x):
    return None if x is None else lc.fnum(x)


def _q3(v):
    v = [x for x in v if x is not None]
    return [lc.q(v, 0.1), lc.q(v, 0.5), lc.q(v, 0.9)] if v else [None, None, None]


def _stride(v, cap=400):
    v = list(v)
    if len(v) <= cap:
        return v
    k = math.ceil(len(v) / cap)
    return v[::k]


def _r(v, nd=5):
    return None if v is None else round(v, nd)


# ── the per-row readings ──
def window_reliable(lines):
    v = None
    for ln in lines or []:
        s = lc.strip(ln)
        if "perf configuration" in s:
            m = _WIN.search(s)
            if m:
                v = m.group(1) == "true"
    return v


def _acct_of(lines, tag, keys):
    """The sender's / receiver's account line: the `final=1` line when there is one, else the last periodic line (labelled ACCT-LAST). -> (label, {key: int}) or (None, None)."""
    ls = [ln for ln in lines if tag in ln]
    if tag == "[ACCT]":
        ls = [ln for ln in ls if "[ACCT-RX]" not in ln]
    if not ls:
        return None, None
    fin = [ln for ln in ls if lc.is_final(ln)]
    ln = fin[-1] if fin else ls[-1]
    d = {k: lc.inum(lc.field(ln, k)) for k in keys}
    return ("final" if fin else "ACCT-LAST"), d


def readings(cli, srv, samples=True):
    """Every witness reading and reported-only readout of a row (JSON-able). A reading that fails lands in `<name>_error`; nothing raises."""
    w = {}

    def guard(name, fn):
        try:
            fn()
        except Exception as e:      # a reported readout never voids a row, a parser that dies deletes the rows the abort accounting is made of
            w[name + "_error"] = f"{type(e).__name__}: {e}"

    diag = [ln for ln in cli if ln.startswith("[DIAG] t=")]
    span = [ln for ln in cli if ln.startswith("[SPAN] t=")]

    def d_counters():
        w["diag_n"] = len(diag)
        last = diag[-1] if diag else None
        m = re.search(r"(?:^|\s)cum=(\d+)/(\d+)/(\d+)", last or "")
        src, cod = (int(m.group(1)), int(m.group(2))) if m else (None, None)
        w.update({"src": src, "cod": cod, "taper": lc.inum(lc.field(last, "taper")), "plost": lc.inum(lc.field(last, "plost")), "pslot": lc.inum(lc.field(last, "pslot")),
                  "retx": lc.inum(lc.field(last, "retx")), "fired": lc.inum(lc.field(last, "fired"))})
        tp = w["taper"] or 0
        w["consumed_r"] = _r((cod + tp) / src, 6) if (src and cod is not None) else None
        w["coded_share"] = _r(cod / (src + cod), 6) if src and cod is not None and src + cod > 0 else None
        w["taper_share"] = _r(tp / (tp + cod), 6) if cod is not None and tp + cod > 0 else None          # copies per repair slot (taper / (taper + coded))
        ms = _SHED.search(last or "")
        w["shed_total"], w["shed_denied"] = (int(ms.group(1)), int(ms.group(2))) if ms else (None, None)
        w["shed_slot"], w["shed_nack"] = lc.inum(lc.field(last, "shed_slot")), lc.inum(lc.field(last, "shed_nack"))
    guard("diag", d_counters)

    def gauges():
        w["g_scw"] = any(re.search(r"\sscw=\d", ln) for ln in diag)
        w["g_plx"] = any(re.search(r"\splx=[0-9.eE+-]+", ln) for ln in diag)
        w["g_rlaw"] = any(_RLAW.search(ln) for ln in span)
        w["g_rsat"] = any(lc.field(ln, "rsat") is not None for ln in span)
        w["g_pfec"] = any(lc.field(ln, "pfec") is not None for ln in span)
        w["g_four"] = all(w[k] for k in ("g_scw", "g_plx", "g_rlaw", "g_rsat"))
        w["g_any"] = any(w[k] for k in ("g_scw", "g_plx", "g_rlaw", "g_rsat", "g_pfec"))
    guard("gauges", gauges)

    def spans():
        rr = [lc.fnum(lc.field(ln, "rr")) for ln in span]
        rr = [x for x in rr if x is not None]
        rs = [lc.fnum(lc.field(ln, "rsat")) for ln in span]
        pf = [lc.fnum(lc.field(ln, "pfec")) for ln in span]
        lw = [_RLAW.search(ln) for ln in span]
        lw = [m for m in lw if m]
        cnt = lambda idx, keys: {k: sum(1 for m in lw if m.group(idx) == k) for k in keys}      # noqa: E731
        w["span_n"] = len(span)
        w["rr_med"], w["rr_max"] = (lc.med(rr), max(rr)) if rr else (None, None)
        w["rlaw_n"] = len(lw)
        w["a_win"], w["a_bound"] = cnt(1, ("core", "burst", "mass")), cnt(2, ("sat", "cap", "free"))
        w["l_win"], w["l_bound"] = cnt(3, ("core", "burst", "mass")), cnt(4, ("sat", "cap", "free"))
        w["beta_med"] = lc.med([float(m.group(5)) for m in lw]) if lw else None
        w["rsat_q"], w["rr_q"], w["pfec_q"] = _q3([x for x in rs]), _q3(rr), _q3([x for x in pf])
        w["pfec_n"] = sum(1 for x in pf if x is not None)
        if samples:
            w["rsat_s"], w["rr_s"], w["pfec_s"] = [_r(x) for x in _stride([x for x in rs if x is not None])], [_r(x) for x in _stride(rr)], [_r(x, 7) for x in _stride([x for x in pf if x is not None])]
    guard("span", spans)

    def succ():
        ln = lc.last_with(srv, "[SUCC]")
        g = lambda k: (lambda v: None if v is None else v / 1000.0)(lc.fnum(lc.field(ln, k)))      # noqa: E731
        w.update({"rep_p50_ms": g("rep_p50_us"), "rep_p90_ms": g("rep_p90_us"), "orig_p50_ms": g("orig_p50_us"), "orig_p90_ms": g("orig_p90_us")})
        w["succ_ok"] = w["rep_p50_ms"] is not None and w["orig_p50_ms"] is not None
        w["succ_line"] = ln is not None
    guard("succ", succ)

    def rcv():
        rfa = [ln for ln in srv if ln.startswith("[RFA]") and lc.inum(lc.field(ln, "src_n")) is not None]
        best = max(rfa, key=lambda ln: lc.inum(lc.field(ln, "src_n"))) if rfa else None
        for k in ("src_n", "dup_src", "fill_src", "fill_coded"):
            w[k] = lc.inum(lc.field(best, k))
        bf = [lc.inum(lc.field(ln, "below_floor")) for ln in srv if ln.startswith("[RDIAG]") and "below_floor=" in ln]
        bf = [x for x in bf if x is not None]
        w["below_floor"] = max(bf) if bf else None
        c8 = [ln for ln in srv if ln.startswith("[C8CONV-R]")]
        w["c8conv_dup"] = sum(int(x) for x in re.findall(r"\bdup=(\d+)", c8[-1])) if c8 else None
        w["succdump_records"] = sum(lc.inum(lc.field(ln, "n")) or 0 for ln in srv if ln.startswith("[SUCCDUMP]"))
    guard("rcv", rcv)

    def acct():
        lab, d = _acct_of(cli, "[ACCT]", ACCT_KEYS)
        w["acct_label"], w["acct"] = lab, d
        if d:
            u = (d["O_serv"] or 0) + (d["O_strand"] or 0)
            w["acct_U"] = u
            ok = all(d[k] is not None for k in ("S_sent", "A", "H", "E", "O_serv", "O_strand"))
            w["acct_identity"] = ok and d["S_sent"] == d["A"] + d["H"] + d["E"] + u
            w["acct_delivered"] = _r(d["A"] / d["S_sent"], 6) if d["S_sent"] else None
        lab, d = _acct_of(srv, "[ACCT-RX]", ("frontier", "holes_given_up"))
        w["acct_rx_label"], w["acct_rx"] = lab, d
    guard("acct", acct)

    def pace():
        echo = [m.group(1) for ln in list(cli) for m in [_PACE_ECHO.search(ln)] if m]
        lines = []
        for ln in cli:
            if ln[:1] == "{" and '"pace"' in ln:
                try:
                    lines.append(json.loads(ln))
                except ValueError:
                    pass
        w["pace_echo"], w["pace_lines"] = echo, len(lines)
        w["pace_gate_cli"], w["pace_gate_srv"] = lc.gate_tok(cli, "RWM_PERF_RATE_MBIT"), lc.gate_tok(srv, "RWM_PERF_RATE_MBIT")
    guard("pace", pace)
    w["pool_gate_cli"], w["pool_gate_srv"] = lc.gate_tok(cli, "RWM_STORE_PATH_POOL"), lc.gate_tok(srv, "RWM_STORE_PATH_POOL")
    w["wr_cli"], w["wr_srv"] = window_reliable(cli), window_reliable(srv)
    w["realtime_echo"] = any(REALTIME_ECHO in ln for ln in cli) and any(REALTIME_ECHO in ln for ln in srv)
    return w


# ── the witnesses ──
def _seat(arm, plan_id=None):
    return plan67.seat_of(arm)


def sha_problems(seat, sha):
    want = os.environ.get("LD_SHA_" + (seat or ""))
    if not want or not sha:
        return [f"bin-sha-uncheckable (row sha {str(sha)[:12] or None}, seat {seat} expects {str(want)[:12] or None}: LD_SHA_{seat} did not reach the parser)"]
    if sha != want:
        return [f"bin-sha={str(sha)[:12]} (want {want[:12]} for seat {seat})"]
    return []


def preconditions(plan_id, seat, w):
    """-> the failed PRECONDITION witnesses (VOID-WITNESS, replaced once)."""
    p = []
    gauge_seat = seat != "B0"
    if seat == "B0" and w.get("g_any"):
        p.append("b0-carries-gauge-prints")
    if seat in ("S2", "P2", "P2T"):
        if w.get("pslot") is None:
            p.append("no-pslot-on-S2-seat")
    elif w.get("pslot") is not None:
        p.append(f"pslot-on-{seat}")
    if plan_id in ("67a", "67o", "67d") and gauge_seat and not w.get("g_four"):
        p.append("no-gauge-prints:" + ",".join(k for k, g in zip(GAUGE4, ("g_scw", "g_plx", "g_rlaw", "g_rsat")) if not w.get(g)))
    if plan_id == "67a":
        if seat in ("S1", "B0"):
            if not ((w.get("taper") or 0) > 0 and w.get("plost") == w.get("taper")):
                p.append(f"copy-branch-not-run taper={w.get('taper')} plost={w.get('plost')}")
        if seat == "S1" and not (w.get("rr_med") or 0) > 0:
            p.append(f"repair-path-not-run rr_med={w.get('rr_med')}")
        if seat == "S2" and not ((w.get("pslot") or 0) > 0 and (w.get("rr_med") or 0) > 0):
            p.append(f"no-slots-taken pslot={w.get('pslot')} rr_med={w.get('rr_med')}")
    if plan_id == "67r" and seat == "S2" and w.get("pslot") is not None and not (w.get("pslot") or 0) > 0:
        p.append("no-slots-taken pslot=0")
    if plan_id == "67p":
        if seat == "P1" and False:
            pass
        if seat in ("P1T", "P2T") and (not w.get("acct") or not w.get("acct_rx")):
            p.append(NO_ACCT + f" (acct={bool(w.get('acct'))} acct_rx={bool(w.get('acct_rx'))})")
        if seat in ("P1", "P2", "P1T", "P2T"):
            if not w.get("pace_echo"):
                p.append("pace-echo-missing")
            if w.get("pace_gate_cli") != plan67.PACE_MBIT:
                p.append(f"pace-gate-cli={w.get('pace_gate_cli')} (want {plan67.PACE_MBIT})")
            if w.get("pace_gate_srv") not in (None, "unset", plan67.PACE_MBIT):
                p.append(f"pace-gate-srv={w.get('pace_gate_srv')}")
            if not w.get("pace_lines"):
                p.append("pace-line-missing")
    return p


def s06_outcome(plan_id, seat, w):
    """Outcome witnesses on an S2 row whose preconditions hold: taper > 0 or plost > 0 (S06-FAIL, never replaced)."""
    if seat != "S2" and not (plan_id == "67p" and False):
        return []
    if not (w.get("pslot") or 0) > 0 or (plan_id == "67a" and not (w.get("rr_med") or 0) > 0):
        return []
    why = []
    if (w.get("taper") or 0) > 0:
        why.append(f"taper={w['taper']}")
    if (w.get("plost") or 0) > 0:
        why.append(f"plost={w['plost']}")
    return why


def row_of(plan_id, cell, arm, seed, rep, rc, wall, drv, cli, srv, cot=0, sha=None):
    seat = _seat(arm)
    hint = "realtime" if plan_id in ("67r", "67p") else "auto"
    sp.ARM_SPEC[arm] = ("window", "Rlc", hint, "ACTIVE")
    row = pp.make_row(cell, arm, seed, rep, rc, wall, drv, cli, srv, int(cot or 0), sha, "def")
    row["plan67"], row["plan"], row["seat"] = True, plan_id, seat
    spec = plan67.armspec(plan_id, cell, arm)
    row["bytes"] = int(os.environ["LD_ROW_BYTES"]) if os.environ.get("LD_ROW_BYTES") else (spec[0] if spec else None)
    if (row.get("pipe_cli") == ["window", "Rlc", "realtime"] and row.get("pipe_srv") == ["window", "Rlc", "realtime"] and any(REALTIME_ECHO in ln for ln in cli)
            and any(REALTIME_ECHO in ln for ln in srv) and "window-line-missing-on-window" in row["problems"]):
        row["problems"].remove("window-line-missing-on-window")      # a Realtime row never prints the reliable-window line (plan 59's rule)
        if row["status"] == "WITNESS-FAIL" and not row["problems"]:
            row["status"] = "LIVE"
    st = inorder_delay_stats(cli)
    row["inorder_mean"] = st["mean"] * 1e3 if st else None
    row["inorder_p95"] = st["p95"] * 1e3 if st else None
    row["inorder_n"] = st["n"] if st else 0
    row["tick_span_s"] = last_tick_s(cli)
    w = readings(cli, srv)
    row["w67"] = w
    dnf = bool(row.get("dnf"))
    sha_p = sha_problems(seat, sha)
    pre = preconditions(plan_id, seat, w)
    w["pre_fail"], w["pre_ok"] = pre, not pre
    pool_p = []
    if plan_id == "67o":
        want = str(plan67.pool_of(arm))
        for side in ("cli", "srv"):
            if w.get(f"pool_gate_{side}") != want:
                pool_p.append(f"pool-echo-{side}={w.get('pool_gate_' + side)} (want {want})")
        w["pool_echo_ok"] = not pool_p
    s06 = s06_outcome(plan_id, seat, w) if plan_id in ("67a", "67r") else []
    w["s06"] = s06
    cur = row["status"]
    row["status_make_row"] = cur
    if sha_p:
        row["problems"].extend(sha_p)
        row["status"] = cur if cur in INFRA + CRASH else "VOID-SEAT"
        if cur in INFRA:
            pass
    elif cur in INFRA + CRASH:
        pass
    elif s06:
        row["status_41d"], row["status"] = cur, "S06-FAIL"
        row["problems"].append("s06:" + ",".join(s06))
    elif (pre and not dnf) or pool_p:
        row["problems"].extend(pre if not dnf else []); row["problems"].extend(pool_p)
        row["status"] = "VOID-WITNESS"
    elif cur == "LIVE" and plan_id in ("67a", "67o", "67d") and not dnf and row["tick_span_s"] < pb.MIN_TICK_S:
        row["status"] = "VOID-SHORT"
        row["problems"].append(f"short-row last-tick={row['tick_span_s']}s (< {pb.MIN_TICK_S:g} s)")
    elif cur == "LIVE" and plan_id == "67r" and not (w.get("wr_cli") is True and w.get("wr_srv") is True):
        row["status"] = "VOID-RHO"
        row["problems"].append(f"rho-not-one-witness cli={w.get('wr_cli')} srv={w.get('wr_srv')} (want window_reliable: true on both ends)")
    return row


def error_row(plan_id, cell, arm, seed, rep, rc, e):
    return {"plan67": True, "plan": plan_id, "cell": cell, "arm": arm, "store": "def", "seed": str(seed), "rep": int(rep), "rc": int(rc) if str(rc).lstrip("-").isdigit() else 1,
            "status": "NO_DATA", "problems": [f"parser-error {type(e).__name__}: {e}"], "dnf": None, "w67": {"row_error": f"{type(e).__name__}: {e}"}, "mbps": None, "cpu_cli": None, "cpu_srv": None}


# ── the ledger ──
def rows_of(paths):
    rows, tokens = [], []
    for path in paths:
        for ln in lc.read(path):
            if ln.startswith("P67ROW "):
                try:
                    rows.append(json.loads(ln[len("P67ROW "):]))
                except ValueError:
                    tokens.append("MALFORMED-P67ROW")
            for t in pp.ABORT_TOKENS:
                if ln.startswith(t):
                    tokens.append(t)
    return rows, tokens


def _slot(r):
    return (r["cell"], r["arm"], str(r["seed"]), str(r["rep"]))


def apply_repl(rows, plan_id):
    """The first row of a slot is the original; a later row replaces it only if the original is a replaceable void. -> (rows to score, info)."""
    first, extras, order = {}, {}, []
    for r in rows:
        k = _slot(r)
        if k not in first:
            first[k] = r
            order.append(k)
        else:
            extras.setdefault(k, []).append(r)
    out, replaced, ignored, void = [], [], [], []
    for k in order:
        o, ex = first[k], extras.get(k, [])
        if o["status"] in plan67.REPLACEABLE:
            void.append(k)
            if ex:
                out.append(ex[0])
                replaced.append((k, o["status"], ex[0]["status"]))
                ignored += [(k, x["status"]) for x in ex[1:]]
            else:
                out.append(o)
        else:
            out.append(o)
            ignored += [(k, x["status"]) for x in ex]
    cap = plan67.replace_cap(plan_id)
    return out, {"replaced": replaced, "ignored": ignored, "void_slots": void, "too_many": len(void) > cap, "cap": cap, "n_slots": len(order)}


def view(rows):
    """S06-FAIL rows are scored under the label they would have had (a DNF stays its worst rank); a replaceable label reads as a crash (never replaced, never missing)."""
    out = []
    for r in rows:
        if r["status"] == "S06-FAIL":
            st = r.get("status_41d") or "VOID-RC"
            r = dict(r, status="VOID-RC" if st in plan67.REPLACEABLE else st)
        out.append(r)
    return out


def s06_dnf_rows(rows, s1="S1", s2="S2"):
    """A DNF on an S2 row of a cell where every S1 row completes (registration 4.2): S06-FAIL at scoring time (it needs the other rows)."""
    out = []
    for cell in sorted({r["cell"] for r in rows}):
        a = [r for r in rows if r["cell"] == cell and r["arm"] == s1 and r["status"] in ("LIVE", "S06-FAIL")]
        if a and all(not r.get("dnf") for r in a):
            out += [r for r in rows if r["cell"] == cell and r["arm"] == s2 and r.get("dnf") and r["status"] == "LIVE"]
    return out


def live(rows, cell, arm):
    return [r for r in rows if r["cell"] == cell and r["arm"] == arm and r["status"] == "LIVE"]


def slotfill(rows, cell, arm):
    """The rows that fill slots in the rank statistic: LIVE (a DNF included) and the crashes."""
    return [r for r in rows if r["cell"] == cell and r["arm"] == arm and r["status"] in ("LIVE",) + CRASH]


def m(rows, key):
    """The non-None readings of `key` over rows: the row's w67 dict first, else the row's own field."""
    out = []
    for r in rows:
        w = r.get("w67") or {}
        x = w[key] if key in w else r.get(key)
        if x is not None:
            out.append(x)
    return out


def fm(v, nd=2):
    return "-" if v is None else (f"{v:.{nd}f}" if isinstance(v, float) else str(v))


def fq(q, nd=3):
    return "/".join(fm(x, nd) for x in q)


# ── the exact rank-sum helpers ──
def exact_mw_p(ref, test):
    """One-sided exact rank-sum (Mann-Whitney) p that `test` is WORSE (larger) than `ref`: ALL C(n1+n2, n1) relabellings, midranks for ties. -> (p, n_relabellings, smallest attainable p)."""
    vals = list(ref) + list(test)
    rk = pb._midranks(vals)
    n1, n2 = len(ref), len(test)
    obs = sum(rk[n1:])
    ge = tot = 0
    for c in itertools.combinations(range(n1 + n2), n2):
        tot += 1
        if sum(rk[i] for i in c) >= obs - 1e-9:
            ge += 1
    return ge / tot, tot, 1.0 / tot


def _clause_sets(names):
    return {"floor": (), "inorder": ()}, tuple(n for n in ("goodput", "cpu_cli/GB", "cpu_srv/GB", "lag_E_cli", "lag_E_srv") if n not in names)


def rank_minp_mc(rows, ref, test, names):
    cs, excl = _clause_sets(names)
    return pb.statistic_rank(rows, ref, test, cells=CELLS_A, exclude=excl, clause_set=cs)


def rank_minp_exact(rows, ref, test, names):
    """The guard's min-p over the clauses of both cells with an EXACT null: the cells are relabelled independently, so P(min over both <= t) = 1 - prod_c (1 - F_c(t)), F_c the exact share of cell c's
    stratified relabellings (per-seed counts fixed as the guard's draws) whose min clause-p <= t. Returns None when a cell's enumeration exceeds EXACT_CAP (the caller uses the Monte Carlo)."""
    cs, excl = _clause_sets(names)
    pools = [pb.Pool(rows, cell, ref, test, excl, with_crash=True, clause_set=cs) for cell, _ in CELLS_A]
    per = []
    for pool in pools:
        sc = pb.rank_scores(pool)
        seeds = pool.seeds
        n = 1
        for s in seeds:
            n *= math.comb(len(pool.groups[s]), pool.k[s])
        if n > EXACT_CAP or not pool.defs:
            return None
        names_ = [d[0] for d in pool.defs]
        parts = []
        for s in seeds:
            parts.append([tuple(sum(sc[nm][i] for i in c) for nm in names_) for c in itertools.combinations(pool.groups[s], pool.k[s])])
        tot = [tuple(map(sum, zip(*combo))) for combo in itertools.product(*parts)]
        true = tuple(sum(sc[nm][i] for i in pool.true) for nm in names_)
        pk = []
        for j in range(len(names_)):
            col = sorted(t[j] for t in tot)
            pk.append(lambda v, col=col: (len(col) - _bl(col, v - 1e-9)) / len(col))
        minp = [min(pk[j](t[j]) for j in range(len(names_))) for t in tot]
        t_true = min(pk[j](true[j]) for j in range(len(names_)))
        per.append({"cell": pool.cell, "n": len(tot), "F": sum(1 for x in minp if x <= t_true + 1e-12) / len(tot), "t_true": t_true,
                    "p_true": {nm: pk[j](true[j]) for j, nm in enumerate(names_)}, "pool": pool})
    surv = 1.0
    for d in per:
        surv *= 1 - d["F"]
    return {"p": 1 - surv, "per": per, "n_total": math.prod(d["n"] for d in per)}


def _bl(col, v):
    import bisect
    return bisect.bisect_left(col, v)


# ── stage A ──
def below_floor(rows, cell):
    """Per row and per cell the below-floor hold flags of registration 4.2: an S2 cell median above 0.25 x the S1 cell median, or above S2's own median fill_coded + 2 x H-B's level. A HOLD, never a FAIL."""
    s1 = lc.med(m(live(rows, cell, "S1"), "below_floor"))
    s2r = live(rows, cell, "S2")
    s2 = lc.med(m(s2r, "below_floor"))
    fc = lc.med(m(s2r, "fill_coded"))
    hb = HB_BELOW_FLOOR[cell]
    lim_a = FLOOR_FRAC * s1 if s1 is not None else None
    lim_b = (fc + 2 * hb) if fc is not None else None
    cell_flag = s2 is not None and ((lim_a is not None and s2 > lim_a) or (lim_b is not None and s2 > lim_b))
    per = []
    for r in s2r:
        b, f = r["w67"].get("below_floor"), r["w67"].get("fill_coded")
        flag = b is not None and ((lim_a is not None and b > lim_a) or (f is not None and b > f + 2 * hb))
        per.append((r, b, f, (b / f) if (b is not None and f) else None, flag))
    return {"cell": cell, "s1_med": s1, "s2_med": s2, "fill_coded_med": fc, "hb": hb, "lim_a": lim_a, "lim_b": lim_b, "flag": bool(cell_flag), "per": per}


def rate_table(rows, cell, arm, out):
    """THE REPAIR-RATE SPLIT TABLE (registration 4.2 Amendment 1) for one (arm, cell): pooled over the rows' per-tick [SPAN] readings."""
    rs = live(rows, cell, arm)
    if not rs:
        return
    ws = [r["w67"] for r in rs]
    tot = lambda k, sub: sum((w.get(k) or {}).get(sub, 0) for w in ws)      # noqa: E731
    n = sum(w.get("rlaw_n") or 0 for w in ws)
    sh = lambda k, keys: " ".join(f"{s}={(tot(k, s) / n if n else float('nan')):.3f}" for s in keys)      # noqa: E731
    pool = lambda key: [x for w in ws for x in (w.get(key) or [])]      # noqa: E731
    npf = sum(1 for w in ws if not w.get("g_pfec"))
    out(f"  RATE {arm:<3} {cell}: rows {len(rs)} rlaw ticks {n}  anchor winner {sh('a_win', ('core', 'burst', 'mass'))}  bound {sh('a_bound', ('sat', 'cap', 'free'))}  "
        f"late-is-fine winner {sh('l_win', ('core', 'burst', 'mass'))}  bound {sh('l_bound', ('sat', 'cap', 'free'))}  beta med {fm(lc.med(m(rs, 'beta_med')), 3)}")
    out(f"       rsat p10/p50/p90 {fq(_q3(pool('rsat_s')))}  granted rr p10/p50/p90 {fq(_q3(pool('rr_s')))}  consumed r med {fm(lc.med(m(rs, 'consumed_r')), 4)}  "
        f"pfec p10/p50/p90 {fq(_q3(pool('pfec_s')), 6) if npf < len(ws) else 'NOT-AVAILABLE'}" + (f" ({npf} row(s) without pfec=: NOT-AVAILABLE for P_fec)" if 0 < npf < len(ws) else "")
        + f"  t_tail {T_TAIL:g} (DERIVED from 1e-5*0.5/delta at delta 0.5, not measured)  r_sat bind fraction (share of ticks bound `sat`) {(tot('a_bound', 'sat') / n if n else float('nan')):.3f}")


def reported_a(rows, out):
    out("REPORTED ONLY (every one goes to the owner)")
    for cell in (C2, C8):
        for arm in ("S1", "S2", "B0"):
            rs = live(rows, cell, arm)
            if not rs:
                continue
            out(f"  {cell} {arm:<3} n={len(rs)} goodput med {fm(lc.med(m(rs, 'mbps')), 1)}  cpu_cli/GB {fm(lc.med([r['cpu_cli'] / (r['bytes'] / 1e9) for r in rs if r.get('cpu_cli') is not None and r.get('bytes')]), 2)}  "
                f"cpu_srv/GB {fm(lc.med([r['cpu_srv'] / (r['bytes'] / 1e9) for r in rs if r.get('cpu_srv') is not None and r.get('bytes')]), 2)}  in-order mean ms {fm(lc.med(m(rs, 'inorder_mean')), 1)}  "
                f"hole-close decode(rep) p50/p90 ms {fm(lc.med(m(rs, 'rep_p50_ms')), 2)}/{fm(lc.med(m(rs, 'rep_p90_ms')), 2)}  resend(orig) p50/p90 ms {fm(lc.med(m(rs, 'orig_p50_ms')), 2)}/{fm(lc.med(m(rs, 'orig_p90_ms')), 2)}  "
                f"retx {fm(lc.med(m(rs, 'retx')), 0)} fill_coded {fm(lc.med(m(rs, 'fill_coded')), 0)} cod {fm(lc.med(m(rs, 'cod')), 0)} taper {fm(lc.med(m(rs, 'taper')), 0)} pslot {fm(lc.med(m(rs, 'pslot')), 0)}")
    for cell in (C2, C8):
        a, b = live(rows, cell, "S1"), live(rows, cell, "S2")
        if not (a and b):
            continue
        n2 = [x for r in b for x in ([r["inorder_mean"]] if r.get("inorder_mean") is not None else [])]
        n1 = [r["inorder_mean"] for r in a if r.get("inorder_mean") is not None]
        if cell == C8 and n1 and n2:
            d = (lc.med(n2) - lc.med(n1)) / lc.med(n1) if lc.med(n1) else None
            out(f"  c8 in-order delay, TWO-SIDED, S2 against S1 med {fm(lc.med(n2), 1)} vs {fm(lc.med(n1), 1)} ms (rel {fm(d * 100 if d is not None else None, 1)} %): resolution 50 % at 6 v 6, a move below that is UNRESOLVED, not 'no effect'")
        for nm, key, per_gb in (("cpu_cli/GB", "cpu_cli", True), ("cpu_srv/GB", "cpu_srv", True)):
            xs = lambda rr: [r[key] / (r["bytes"] / 1e9) for r in rr if r.get(key) is not None and r.get("bytes") and not r.get("dnf")]      # noqa: E731
            if xs(a) and xs(b):
                x = (lc.med(xs(b)) - lc.med(xs(a))) / lc.med(xs(a))
                try:
                    cs, excl = _clause_sets((nm,))
                    p = pb.statistic_rank(rows, "S1", "S2", cells=[(cell, "def")], exclude=excl, clause_set=cs)["p"]
                except Exception:
                    p = None
                flag = x > D80[cell] and p is not None and p <= 0.05
                out(f"  {cell} {nm} S2 vs S1 {x * 100:+.1f} %  rank p {fm(p, 4)}  OWNER-FLAG {'YES' if flag else 'no'} (S2 worse than d80 {D80[cell] * 100:.0f} % AND rank p <= 0.05)")
    out("")
    out("THE REPAIR-RATE SPLIT TABLE (Amendment 1; per arm and cell, pooled over the rows' per-tick [SPAN] readings)")
    for cell in (C2, C8):
        for arm in ("S1", "S2"):
            rate_table(rows, cell, arm, out)
        s1, s2 = live(rows, cell, "S1"), live(rows, cell, "S2")
        if s1 and s2:
            out(f"  COST-SPLIT {cell}: consumed r S2 {fm(lc.med(m(s2, 'consumed_r')), 4)} vs S1 {fm(lc.med(m(s1, 'consumed_r')), 4)}; S2 coded share {fm(lc.med(m(s2, 'coded_share')), 4)}; "
                f"S1 taper share (copies per repair slot) {fm(lc.med(m(s1, 'taper_share')), 3)} [expected about 0.64 c2 / 0.98 c8]; goodput S1 {fm(lc.med(m(s1, 'mbps')), 1)} S2 {fm(lc.med(m(s2, 'mbps')), 1)}")
    sc = [r for r in rows if r["cell"] in (C2, C8) and r["arm"] in ("S1", "S2", "B0") and r["status"] == "LIVE"]
    ok = sum(1 for r in sc if r["w67"].get("succ_ok"))
    out(f"  [SUCC] CHECK: {ok}/{len(sc)} live rows carry both rep_p50_us and orig_p50_us on the final line without RWM_SUCC_DUMP" + ("" if ok == len(sc) else "  DUMP-TWINS-NEEDED"))


def score_a(paths, out, plan_id="67a", repl=None):
    rows_all, tokens = rows_of(paths)
    rows0, info = apply_repl(rows_all, plan_id)
    aborted = sorted({t for t in tokens if t in pp.ABORT_TOKENS})
    out(f"PLAN 67 STAGE A (registration 4.2 / 3.2): S1 12, S2 12, B0 6 per cell at c2-100 / c8-100 Auto, 600 MB; ledger slots {info['n_slots']}; ABORT-TOKENS {' '.join(aborted) or 'none'}")
    out("STATUS-TABLE")
    for st in ("LIVE", "S06-FAIL", "VOID-WITNESS", "WITNESS-FAIL", "VOID-SHORT", "VOID-SEAT", "CONTAMINATED", "VOID-COTENANT", "VOID-ONDISK", "VOID-RC", "NO_DATA"):
        out(f"  {st:<13} {sum(1 for r in rows0 if r['status'] == st)}")
    for r in rows_all:
        if r["status"] != "LIVE":
            out(f"  NONLIVE {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} rc={r.get('rc')} {' '.join(r.get('problems') or [])}")
    out(f"REPLACED SLOTS: {len(info['replaced'])} of at most {info['cap']} (slots voided by a witness: {len(info['void_slots'])})" + ("" if info["replaced"] else " - none"))
    for k, o, n in info["replaced"]:
        out(f"  {k[0]} {k[1]} s{k[2]} r{k[3]} {o} -> {n}")
    s06 = [r for r in rows0 if r["status"] == "S06-FAIL" and r["arm"] == "S2"]
    s06d = s06_dnf_rows(rows0)
    for r in s06:
        out(f"  S06-FAIL {r['cell']} S2 s{r['seed']} rep{r['rep']}: {'; '.join(x for x in r['problems'] if x.startswith('s06:'))}")
    for r in s06d:
        out(f"  S06-FAIL {r['cell']} S2 s{r['seed']} rep{r['rep']}: DNF where every S1 row of the cell completes")
    n06 = len(s06) + len(s06d)
    rows = view(rows0)
    out("")
    bf = {c: below_floor(rows, c) for c in (C2, C8)}
    out("BELOW-FLOOR ARRIVALS (reported, a HOLD flag, never a FAIL): per-row below_floor (receiver [RDIAG] max), its ratio to fill_coded")
    for c in (C2, C8):
        b = bf[c]
        out(f"  {c}: S2 cell median {fm(b['s2_med'], 0)}; S1 cell median {fm(b['s1_med'], 0)} (limit 0.25 x = {fm(b['lim_a'], 0)}); S2 fill_coded median {fm(b['fill_coded_med'], 0)} + 2 x H-B {b['hb']} = {fm(b['lim_b'], 0)}"
            + ("" if c == C2 else " (c8's H-B level 122 is H-B's median belowfl of docs/l1-raw/s51b/recopy/acct-c8.txt)") + f"  -> BELOW-FLOOR-HOLD {c} {'HOLD' if b['flag'] else 'clear'}")
        for r, bb, ff, ratio, flag in b["per"]:
            out(f"    S2 s{r['seed']} r{r['rep']} below_floor={bb} fill_coded={ff} ratio={fm(ratio, 2)}{' HOLD' if flag else ''}")
    out("")
    verdict = {}
    # UNSCOREABLE: fewer than MIN_LIVE_A live S1 or S2 rows per cell (MIN_LIVE_B0 live B0 rows for A1)
    short_as = [f"{c} {a} {len(slotfill(rows, c, a))}<{plan67.MIN_LIVE_A}" for c in (C2, C8) for a in ("S1", "S2") if len(slotfill(rows, c, a)) < plan67.MIN_LIVE_A]
    short_a1 = short_as + [f"{c} B0 {len(slotfill(rows, c, 'B0'))}<{plan67.MIN_LIVE_B0}" for c in (C2, C8) if len(slotfill(rows, c, "B0")) < plan67.MIN_LIVE_B0]

    def verdict_of(name, short, fn):
        if aborted:
            return f"{name} UNSCOREABLE (abort {','.join(aborted)})"
        if short:
            return f"{name} UNSCOREABLE (fewer live rows than the minimum: {'; '.join(short)})"
        if info["too_many"]:
            return f"{name} NO VERDICT (more than {info['cap']} replaced slots: {len(info['void_slots'])} voided)"
        return fn()
    # clause AS
    def run_as():
        res = rank_minp_mc(rows, "S1", "S2", ("goodput",))
        out("CLAUSE AS (S2 against S1, one-sided S2 worse, goodput at c2 and c8, rank-minp, 12 v 12; the guard's stratified rank sums, 10 000 seeded relabellings, seed 4141 - MONTE CARLO)")
        for p in res["pools"]:
            out(f"  {p.cell}: pool {len(p.ref_rows)} S1 + {len(p.test_rows)} S2; relabelled S2 rows per seed " + " ".join(f"s{s}:{p.k[s]}" for s in p.seeds))
        for k, pv in res["p_true"].items():
            out(f"    {k[0]} {k[1]:<10} S={res['S_true'][k]:6.1f} / {res['S_null_mean'][k]:6.2f}  p={pv:.4f}")
        out(f"  min-p (true) {res['T_true']:.5f} at {res['argmin'][0]}; p = {res['p']:.4f}; worst-ranked rows (DNF / crash): {', '.join(f'{c} {a} s{sd} r{rp} {w}' for c, a, sd, rp, w in res['bad_rows']) or 'none'}")
        try:
            out("  realised d80 from the S1 rows' half-splits (q95 of |x| over the half-per-seed splits; the guard's resolution reading): "
                + "; ".join(f"{r['cell']} {r['clause']} q95={fm(r['q95'], 4)} (n={r['n_base']})" for r in pb.resolution(rows, arm="S1", cells=CELLS_A, clause_set={"floor": (), "inorder": ()}) if r["clause"] == "goodput"))
        except Exception as e:
            out(f"  realised d80: not computed ({type(e).__name__}: {e})")
        return f"AS {'PASS' if res['p'] > 0.05 else 'FAIL'} (p={res['p']:.4f}; PASS iff p > 0.05)"
    vas = verdict_of("AS", short_as, run_as)
    out(vas)
    out("")

    def run_a1():
        names = ("goodput", "cpu_cli/GB", "cpu_srv/GB")
        ex = rank_minp_exact(rows, "B0", "S1", names)
        mc = rank_minp_mc(rows, "B0", "S1", names)
        out("CLAUSE A1 (S1 against B0, one-sided S1 worse, goodput + cpu_cli/GB + cpu_srv/GB at c2 and c8, rank-minp over the six, 6 v 12)")
        if ex:
            out(f"  null: EXACT. The cells are relabelled independently, so each cell's stratified null (per-seed counts fixed, {' x '.join(str(d['n']) for d in ex['per'])} relabellings) is enumerated and the joint min-p null "
                f"({ex['n_total']} joint labellings) is their product (1 - prod(1 - F_c)); the unstratified C(18,6) = 18 564 count per cell is not used because the guard's rank sums are within seed")
            for d in ex["per"]:
                out(f"    {d['cell']}: true min clause-p {d['t_true']:.5f}, F_c {d['F']:.5f}; clause p " + " ".join(f"{k}={v:.4f}" for k, v in d["p_true"].items()))
            out(f"  exact p = {ex['p']:.5f}; the guard's seeded Monte Carlo (10 000, seed 4141) reads p = {mc['p']:.4f} (reported, cross-check)")
            p = ex["p"]
        else:
            out(f"  null: MONTE CARLO (the guard's 10 000 seeded relabellings, seed 4141; a cell's exact enumeration exceeds {EXACT_CAP})")
            p = mc["p"]
        out(f"  worst-ranked rows: {', '.join(f'{c} {a} s{sd} r{rp} {w}' for c, a, sd, rp, w in mc['bad_rows']) or 'none'}")
        return f"A1 {'PASS' if p > 0.05 else 'FAIL'} (p={p:.5f}; PASS iff p > 0.05)"
    va1 = verdict_of("A1", short_a1, run_a1)
    out(va1)
    out("")
    reported_a(rows, out)
    out("")
    if n06:
        out(f"S06-FAIL rows: {n06} (the merge is held: any S06-FAIL or AS FAIL means no merge)")
    out(vas)
    out(va1)
    out(f"S06-FAIL-ROWS {n06}")
    out("BELOW-FLOOR-HOLD " + ("YES (" + ", ".join(c for c in (C2, C8) if bf[c]["flag"]) + ")" if any(bf[c]["flag"] for c in bf) else "no"))
    out("VERDICT-67A " + vas.split(" (")[0] + "; " + va1.split(" (")[0] + f"; S06-FAIL rows {n06}")
    return {"AS": vas, "A1": va1, "s06": n06, "hold": any(bf[c]["flag"] for c in bf)}


# ── stage R1 ──
def score_r1(paths, out, plan_id="67r"):
    rows_all, tokens = rows_of(paths)
    rows0, info = apply_repl(rows_all, plan_id)
    aborted = sorted({t for t in tokens if t in pp.ABORT_TOKENS})
    out(f"PLAN 67 STAGE R1 (registration 4.3): c2-100 Realtime rho = 1, 100 MB, S1 12 / S2 12 / B0 6; slots {info['n_slots']}; ABORT-TOKENS {' '.join(aborted) or 'none'}")
    for r in rows_all:
        if r["status"] != "LIVE":
            out(f"  NONLIVE {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} rc={r.get('rc')} {' '.join(r.get('problems') or [])}")
    out(f"REPLACED SLOTS: {len(info['replaced'])} of at most {info['cap']} (voided {len(info['void_slots'])})")
    arm = lambda a: [r for r in rows0 if r["arm"] == a]      # noqa: E731
    s1, s2, b0 = arm("S1"), arm("S2"), arm("B0")
    done = lambda r: r["status"] in ("LIVE", "S06-FAIL") and not r.get("dnf")      # noqa: E731
    # A1-R
    gauge_bad = [f"S1 s{r['seed']} r{r['rep']}" for r in s1 if not r["w67"].get("g_four")] + [f"B0 s{r['seed']} r{r['rep']} carries prints" for r in b0 if r["w67"].get("g_any")]
    b0_all = bool(b0) and all(done(r) for r in b0)
    s1_miss = [f"S1 s{r['seed']} r{r['rep']}" for r in s1 if not done(r)] if b0_all else []
    a1r_ok = not gauge_bad and not s1_miss
    out("CLAUSE A1-R: every S1 row carries the four gauge prints and no B0 row does; every S1 row completes wherever every B0 row completes")
    out(f"  gauge-print failures: {', '.join(gauge_bad) or 'none'}; every B0 row completes: {b0_all}; S1 rows that did not complete: {', '.join(s1_miss) or 'none'}")
    out(f"  reported only: goodput S1 {fm(lc.med(m([r for r in s1 if done(r)], 'mbps')), 1)} vs B0 {fm(lc.med(m([r for r in b0 if done(r)], 'mbps')), 1)}; "
        f"cpu_cli/GB S1 {fm(lc.med([r['cpu_cli'] / (r['bytes'] / 1e9) for r in s1 if r.get('cpu_cli') and r.get('bytes')]), 2)} vs B0 {fm(lc.med([r['cpu_cli'] / (r['bytes'] / 1e9) for r in b0 if r.get('cpu_cli') and r.get('bytes')]), 2)}")
    # R1
    zero = [r for r in s1 if (r["w67"].get("taper") or 0) == 0]
    blind = bool(s1) and 2 * len(zero) > len(s1)
    s06 = [r for r in rows0 if r["status"] == "S06-FAIL"]
    s1_all = bool(s1) and all(done(r) for r in s1)
    s2_hold = [r for r in s2 if r["w67"].get("pre_ok") and r["w67"].get("s06") == []]
    s2_dead = [f"S2 s{r['seed']} r{r['rep']}" for r in s2_hold if not done(r)] if s1_all else []
    r1_ok = not s2_dead and not s06
    out("CLAUSE R1: every S2 row whose witnesses hold completes inside its timeout wherever every S1 row completes (plan 64 item 5); no S06-FAIL row")
    out(f"  S1 taper = 0 on {len(zero)} of {len(s1)} rows (R1-BLIND iff more than half: {blind}); every S1 row completes: {s1_all}; S2 rows that did not complete: {', '.join(s2_dead) or 'none'}; S06-FAIL rows: {len(s06)}")
    for r in s06:
        out(f"    S06-FAIL S2 s{r['seed']} r{r['rep']}: {'; '.join(x for x in r['problems'] if x.startswith('s06:'))}")
    out(f"  reported: goodput med S1 {fm(lc.med(m(s1, 'mbps')), 1)} S2 {fm(lc.med(m(s2, 'mbps')), 1)} B0 {fm(lc.med(m(b0, 'mbps')), 1)}; in-order delay is under the 30 s tick floor (marked, not read)")
    short = [f"{a} {len([r for r in rows0 if r['arm'] == a and r['status'] in ('LIVE', 'S06-FAIL') + CRASH])}" for a in ("S1", "S2") if len([r for r in rows0 if r["arm"] == a]) < 1]
    pre = ("UNSCOREABLE (abort " + ",".join(aborted) + ")") if aborted else ("UNSCOREABLE (no S1 or S2 row)" if short else ("NO VERDICT (more than %d replaced slots)" % info["cap"] if info["too_many"] else None))
    a1 = pre and "A1-R " + pre or f"A1-R {'PASS' if a1r_ok else 'FAIL'}"
    r1 = pre and "R1 " + pre or ("R1 FAIL" if not r1_ok else ("R1-BLIND" if blind else "R1 PASS"))
    out(a1)
    out(r1)
    out(f"VERDICT-67R {a1}; {r1}")
    return {"A1R": a1, "R1": r1}


# ── stage R2 ──
def score_r2(paths, out, plan_id="67p"):
    rows_all, tokens = rows_of(paths)
    rows0, info = apply_repl(rows_all, plan_id)
    aborted = sorted({t for t in tokens if t in pp.ABORT_TOKENS})
    out(f"PLAN 67 STAGE R2 (registration 4.4): c2 paced evicting rows P1 / P2 and the test-seams twins P1T / P2T; slots {info['n_slots']}; ABORT-TOKENS {' '.join(aborted) or 'none'}")
    for r in rows_all:
        if r["status"] != "LIVE":
            out(f"  NONLIVE {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} {' '.join(r.get('problems') or [])}")
    out(f"REPLACED SLOTS: {len(info['replaced'])} of at most {info['cap']}")
    out("TWIN ROWS (the [ACCT] reading: final=1 else ACCT-LAST, never mixed without the label): arm s rep label U O_strand O_max oldest_unchanged_max inert_sweeps holes_given_up identity")
    tw = {"P1T": [], "P2T": []}
    for r in rows0:
        if r["arm"] in tw and r["status"] == "LIVE" and r["w67"].get("acct"):
            tw[r["arm"]].append(r)
        if r["arm"] in tw:
            w = r["w67"]
            a, rx = w.get("acct") or {}, w.get("acct_rx") or {}
            out(f"  {r['arm']} s{r['seed']} r{r['rep']} {w.get('acct_label') or '-'} U={w.get('acct_U')} O_strand={a.get('O_strand')} O_max={a.get('O_max')} oldest_unchanged_max={a.get('oldest_unchanged_max')} "
                f"inert_sweeps={a.get('inert_sweeps')} holes_given_up={rx.get('holes_given_up')} identity(S_sent=A+H+E+O_serv+O_strand)={w.get('acct_identity')} status={r['status']}")
    labels = {r["w67"].get("acct_label") for v in tw.values() for r in v}
    if "ACCT-LAST" in labels:
        out("  NOTE: ACCT-LAST rows (a killed process runs no Drop): the reading is the last periodic line, labelled; " + ("MIXED with final=1 rows: read the labelled sets apart" if "final" in labels else "no final=1 row"))
    n1, n2 = len(tw["P1T"]), len(tw["P2T"])
    out(f"LIVE TWIN ROWS: P1T {n1}, P2T {n2} (minimum {plan67.MIN_TWIN} per seat)")
    hold = None
    if aborted:
        verdict = f"R2-HOLD UNSCOREABLE (abort {','.join(aborted)}); guard 2 NOT ELIGIBLE"
    elif n1 < plan67.MIN_TWIN or n2 < plan67.MIN_TWIN or info["too_many"]:
        verdict = "R2-HOLD UNSCOREABLE; guard 2 NOT ELIGIBLE (UNSCOREABLE is never read as 'did not fire')"
    else:
        res = {}
        for nm, get in (("U", lambda r: r["w67"]["acct_U"]), ("oldest_unchanged_max", lambda r: r["w67"]["acct"]["oldest_unchanged_max"])):
            ref, test = [get(r) for r in tw["P1T"]], [get(r) for r in tw["P2T"]]
            if any(x is None for x in ref + test):
                res[nm] = None
                continue
            p, n, pmin = exact_mw_p(ref, test)
            res[nm] = p
            out(f"  R2-HOLD ({'i' if nm == 'U' else 'ii'}) {nm}: exact one-sided Mann-Whitney rank sum, P2T worse, ALL {n} relabellings, midranks for ties: p = {p:.5f} (smallest attainable p {pmin:.5f}; "
                f"it resolves only a shift that puts most P2T rows above most P1T rows; a bare median or count comparison is NOT used)")
        hold = any(p is not None and p <= 0.05 for p in res.values())
        verdict = "R2-HOLD FIRED (holds the S06 merge until read)" if hold else "R2-HOLD CLEAR (not fired at this resolution)"
    out("")
    out("REPORTED (ADR-0079 Amendment 1): shed_slot (P2 = 0 by construction), shed_nack, shed_denied, shed_total, delivered fraction (A / S_sent of the twins' [ACCT])")
    tot = {}
    for arm in ("P1", "P2", "P1T", "P2T"):
        rs = [r for r in rows0 if r["arm"] == arm and r["status"] == "LIVE"]
        tot[arm] = rs
        out(f"  {arm:<3} n={len(rs)} shed_slot {fm(lc.med(m(rs, 'shed_slot')), 0)} shed_nack {fm(lc.med(m(rs, 'shed_nack')), 0)} shed_denied {fm(lc.med(m(rs, 'shed_denied')), 0)} shed_total {fm(lc.med(m(rs, 'shed_total')), 0)} "
            f"delivered {fm(lc.med(m(rs, 'acct_delivered')), 5)} taper {fm(lc.med(m(rs, 'taper')), 0)} pslot {fm(lc.med(m(rs, 'pslot')), 0)} rr_med {fm(lc.med(m(rs, 'rr_med')), 3)}")
    for a, b in (("P1", "P2"), ("P1T", "P2T")):
        per = {}
        for r in tot[b]:
            o = [x for x in tot[a] if str(x["seed"]) == str(r["seed"]) and x["w67"].get("shed_total") is not None]
            if o and r["w67"].get("shed_total") is not None:
                per.setdefault(str(r["seed"]), []).append(r["w67"]["shed_total"] - lc.med([x["w67"]["shed_total"] for x in o]))
        allv = [x for v in per.values() for x in v]
        out(f"  shed_total({b}) - shed_total({a}): med {fm(lc.med(allv), 1)}; per-seed range " + (", ".join(f"s{s}: [{min(v):.0f}, {max(v):.0f}]" for s, v in sorted(per.items())) or "n/a"))
    out(verdict)
    out(f"VERDICT-67P {verdict}")
    return {"verdict": verdict, "hold": hold}


# ── pool block and dump twins ──
def score_pool(paths, out, plan_id="67o"):
    rows_all, tokens = rows_of(paths)
    rows0, info = apply_repl(rows_all, plan_id)
    out(f"PLAN 67 POOL BLOCK (registration 4.2; reported only): c8-100 Auto, RWM_STORE_PATH_POOL 170 / 256 / 640, S1 and S2, n = 3, seed 42; slots {info['n_slots']}")
    for r in rows_all:
        if r["status"] != "LIVE":
            out(f"  NONLIVE {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} {' '.join(r.get('problems') or [])}")
    out("per (seat, pool): n, in-order delay mean ms, hole-close decode(rep) / resend(orig) p50 p90 ms, completion times s (per row; DNF = timeout), max and p90 over rows, receiver duplicates ([C8CONV-R] dup sum / [RFA] dup_src)")
    for seat in ("S1", "S2"):
        for pool in plan67.POOLS:
            rs = [r for r in rows0 if r["arm"] == f"{seat}P{pool}" and r["status"] == "LIVE"]
            if not rs:
                continue
            comp = [(r.get("wall_s") if not r.get("dnf") else float(plan67.TIMEOUT_A)) for r in rs]
            comp = [x for x in comp if x is not None]
            out(f"  {seat} pool {pool}: n={len(rs)} in-order {fm(lc.med(m(rs, 'inorder_mean')), 1)}  rep p50/p90 {fm(lc.med(m(rs, 'rep_p50_ms')), 2)}/{fm(lc.med(m(rs, 'rep_p90_ms')), 2)}  orig p50/p90 {fm(lc.med(m(rs, 'orig_p50_ms')), 2)}/{fm(lc.med(m(rs, 'orig_p90_ms')), 2)}  "
                f"completion {[round(x, 1) for x in comp]} max {fm(max(comp) if comp else None, 1)} p90 {fm(lc.q(comp, 0.9), 1)} dnf {sum(1 for r in rs if r.get('dnf'))}  "
                f"dup {fm(lc.med(m(rs, 'c8conv_dup')), 0)} / dup_src {fm(lc.med(m(rs, 'dup_src')), 0)}")
    out("VERDICT-67O REPORTED-ONLY (decides nothing here; lever B's revisit trigger)")


def score_dump(paths, out, plan_id="67d"):
    rows_all, _ = rows_of(paths)
    rows0, info = apply_repl(rows_all, plan_id)
    out(f"PLAN 67 DUMP TWINS (registration 4.2; RWM_SUCC_DUMP=1): rows {info['n_slots']}; [SUCCDUMP] record counts per row")
    for r in rows0:
        out(f"  {r['cell']} {r['arm']} s{r['seed']} r{r['rep']} {r['status']} records={r['w67'].get('succdump_records')} rep p50/p90 {fm(r['w67'].get('rep_p50_ms'), 2)}/{fm(r['w67'].get('rep_p90_ms'), 2)} orig p50/p90 {fm(r['w67'].get('orig_p50_ms'), 2)}/{fm(r['w67'].get('orig_p90_ms'), 2)}")
    out("VERDICT-67D REPORTED-ONLY")


def score(plan_id, paths, out=print):
    fn = {"67a": score_a, "67r": score_r1, "67p": score_r2, "67o": score_pool, "67d": score_dump}[plan_id]
    return fn(paths, out, plan_id) if fn is not score_a else score_a(paths, out, plan_id)


# ── smokes and block 1 ──
def smoke(plan_id, path, out=print):
    rows, _ = rows_of([path])
    got = {r["arm"]: r for r in rows}
    bad = []

    def crit(name, fails):
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)
    want = [a for _, _, a, _, _ in plan67.plan(plan_id, True)] if plan_id in ("67a", "67r", "67p") else sorted({r["arm"] for r in rows})
    out(f"SMOKE67 {plan_id} rows={len(rows)}/{len(want)} (the criteria below; a rehearsal row is never re-run)")
    crit("every smoke arm ran", [f"{a}: no row" for a in want if a not in got])
    for a, r in got.items():
        w = r.get("w67") or {}
        out(f"  ROW {a}: status={r['status']} rc={r.get('rc')} dnf={r.get('dnf')} mbps={r.get('mbps')} taper={w.get('taper')} pslot={w.get('pslot')} rr_med={w.get('rr_med')} gauges={w.get('g_four')} pre_fail={w.get('pre_fail')}")
    crit("every smoke row is LIVE with its precondition witnesses (a failed S2 outcome witness is a finding)", [f"{a}: {r['status']} {r.get('problems')}" for a, r in got.items() if r["status"] != "LIVE" or not (r.get("w67") or {}).get("pre_ok")])
    crit("each row ran its seat's binary (no bin-sha problem)", [f"{a}: {x}" for a, r in got.items() for x in r.get("problems") or [] if x.startswith("bin-sha")])
    if plan_id == "67a":
        ok = [a for a, r in got.items() if (r.get("w67") or {}).get("succ_ok")]
        out(f"{'PASS' if len(ok) == len(got) else 'INFO'} [SUCC] check: the final line carries both rep_p50_us and orig_p50_us without RWM_SUCC_DUMP on {len(ok)}/{len(got)} rows" + ("" if len(ok) == len(got) else "  DUMP-TWINS-NEEDED (the dump twins run: written into the second amendment)"))
    if plan_id == "67r":
        crit("window_reliable: true on both ends of every R1 smoke row", [f"{a}: cli={(r.get('w67') or {}).get('wr_cli')} srv={(r.get('w67') or {}).get('wr_srv')}" for a, r in got.items() if not ((r.get('w67') or {}).get("wr_cli") is True and (r.get('w67') or {}).get("wr_srv") is True)])
        crit("S2: pslot > 0 with taper = 0; S1 carries the four gauge prints", ([f"S2: pslot={(got['S2']['w67']).get('pslot')} taper={(got['S2']['w67']).get('taper')}" for _ in [0] if "S2" in got and not ((got['S2']['w67'].get('pslot') or 0) > 0 and (got['S2']['w67'].get('taper') or 0) == 0)])
             + (["S1: no four gauge prints"] if "S1" in got and not got["S1"]["w67"].get("g_four") else []))
    if plan_id == "67p":
        w = lambda a: (got.get(a) or {}).get("w67") or {}      # noqa: E731
        crit("P1 taper > 0; P2 pslot > 0 with the [SPAN] median rr > 0", ([f"P1 taper={w('P1').get('taper')}"] if "P1" in got and not (w("P1").get("taper") or 0) > 0 else [])
             + ([f"P2 pslot={w('P2').get('pslot')} rr_med={w('P2').get('rr_med')}"] if "P2" in got and not ((w("P2").get("pslot") or 0) > 0 and (w("P2").get("rr_med") or 0) > 0) else []))
        crit("every pacer row prints the pacer echo", [f"{a}: no pace echo" for a in got if not w(a).get("pace_echo")])
        crit("each twin row shows [ACCT] on the client and [ACCT-RX] on the server in the battery's KEPT log", [f"{a}: acct={bool(w(a).get('acct'))} acct_rx={bool(w(a).get('acct_rx'))}" for a in ("P1T", "P2T") if a in got and not (w(a).get("acct") and w(a).get("acct_rx"))])
    if bad:
        out("SMOKE-FAIL " + "; ".join(bad))
        return 1
    out("SMOKE-PASS")
    return 0


def block1(plan_id, path, out=print):
    rows, _ = rows_of([path])
    first = {}
    for r in rows:
        first.setdefault(r["arm"], r)
    arms = []
    for _, _, a, _, _ in plan67.plan(plan_id):
        if a not in arms:
            arms.append(a)
    bad = []
    for a in arms:
        r = first.get(a)
        if r is None:
            bad.append(f"{a}: no row")
        elif r["status"] != "LIVE" or not (r.get("w67") or {}).get("pre_ok"):
            bad.append(f"{a}: first row {r['status']} pre_fail={(r.get('w67') or {}).get('pre_fail')} {r.get('problems')}")
    for b in bad:
        out("BLOCK1-FAIL " + b)
    out("BLOCK1 " + ("OK" if not bad else "FAILED"))
    return 0 if not bad else 1


# ── stage T ──
_REF = re.compile(r"^T67REF seat=(\S+) cmd=(\S+) key=(\S+) rc=(-?\d+) cfg=(\S+) err=(.*)$")
_SEATL = re.compile(r"^T67SEAT seat=(\S+) hint=(\S+)")
_TREP = re.compile(r"^TUNREP label=(\S+) rep=(\d+) rc=(-?\d+) (\{.*\})")
_TPIPE = re.compile(r"^TUNPIPE cli=(\S+) srv=(\S+)")
_TCFG = re.compile(r"^TUNCFG cli=(\S+) srv=(\S+)")
TUN_KEYS = ("window_out_of_order", "window_coded_only", "window_generation_coding", "window_systematic_repair")
TUN_TEXT = "{key} is a perf/native-object mode; the tunnel always delivers in order: remove it from the config file"
_CFG_FIELDS = ("ooo", "coded", "gen", "sysrep")


def tun_parse(path):
    inv, refs, cur, done = {}, [], None, False
    for ln in lc.read(path):
        m_ = _SEATL.match(ln)
        if m_:
            cur = (m_.group(1), m_.group(2))
            inv[cur] = {"pipe": None, "cfg": None, "reps": {}}
            continue
        m_ = _TPIPE.match(ln)
        if m_ and cur:
            inv[cur]["pipe"] = (m_.group(1), m_.group(2))
        m_ = _TCFG.match(ln)
        if m_ and cur:
            inv[cur]["cfg"] = (m_.group(1), m_.group(2))
        m_ = _TREP.match(ln)
        if m_ and cur:
            try:
                o = json.loads(m_.group(4))
            except ValueError:
                o = {"nodata": True}
            ok = not o.get("nodata") and (o.get("mbps") is not None or o.get("seconds") is not None)
            inv[cur]["reps"][int(m_.group(2))] = {"ok": ok, "mbps": lc.fnum(o.get("mbps")), "seconds": lc.fnum(o.get("seconds")), "rc": int(m_.group(3))}
        m_ = _REF.match(ln)
        if m_:
            refs.append({"seat": m_.group(1), "cmd": m_.group(2), "key": m_.group(3), "rc": int(m_.group(4)), "cfg": m_.group(5), "err": m_.group(6).strip()})
        if ln.startswith("T67DONE"):
            done = True
    return inv, refs, done


def _cfg_of(s):
    d = {}
    for part in (s or "").split(","):
        k, _, v = part.partition(":")
        d[k] = v
    return d


def score_t(path, out=print):
    inv, refs, done = tun_parse(path)
    ref = {(r["seat"], r["cmd"], r["key"]): r for r in refs}
    out("STAGE T (registration 3.3): the tunnel itself, c2, hints auto and realtime, B0 and S1")
    # seat check
    seat_abort = [f"B0 run {r['key']}: stderr carries the guard text" for r in refs if r["seat"] == "B0" and r["cmd"] == "run" and "is a perf/native-object mode" in r["err"]]
    out("B0 CHECK COMMANDS (printed, never scored): " + (", ".join(f"{k}: rc={ref[('B0', 'check', k)]['rc']} cfg={ref[('B0', 'check', k)]['cfg']}" for k in TUN_KEYS if ('B0', 'check', k) in ref) or "none"))
    # (1) refusals
    c1 = []
    for cmd in ("check", "run"):
        for k in TUN_KEYS:
            r = ref.get(("S1", cmd, k))
            if r is None:
                c1.append(f"S1 {cmd} {k}: refusal line missing")
                continue
            err = r["err"][len("Error: "):] if r["err"].startswith("Error: ") else r["err"]
            if r["rc"] == 0:
                c1.append(f"S1 {cmd} {k}: exit 0")
            elif err != TUN_TEXT.format(key=k):
                c1.append(f"S1 {cmd} {k}: stderr {r['err']!r}")
    out(f"(1) all 8 S1 refusal commands exit non-zero with stderr exactly `<key> is a perf/native-object mode; ...` (an optional leading `Error: ` stripped): {'ok' if not c1 else '; '.join(c1)}")
    # (2) B0 run reads true
    c2 = []
    for k in TUN_KEYS:
        r = ref.get(("B0", "run", k))
        if r is None:
            c2.append(f"B0 run {k}: line missing")
        elif r["cfg"] != "true":
            c2.append(f"B0 run {k}: cfg={r['cfg']}")
    out(f"(2) on each of the 4 B0 `run` commands the key reads true in `resolved configuration`: {'ok' if not c2 else '; '.join(c2) + ' -> the file never reached the tunnel: T VOID'}")
    # (3) completion
    c3 = []
    for hint in ("auto", "realtime"):
        b, s = inv.get(("B0", hint)), inv.get(("S1", hint))
        if not b or not s:
            c3.append(f"{hint}: invocation missing (B0 {bool(b)}, S1 {bool(s)})")
            continue
        for rep, br in sorted(b["reps"].items()):
            if br["ok"] and not (s["reps"].get(rep) or {}).get("ok"):
                c3.append(f"{hint} rep {rep}: B0 completes, S1 does not")
    out(f"(3) every S1 transfer completes wherever the B0 transfer of the same (hint, rep) completes: {'ok' if not c3 else '; '.join(c3)}")
    # (4) pipe and cfg
    c4 = []
    for hint in ("auto", "realtime"):
        s = inv.get(("S1", hint))
        if not s:
            c4.append(f"{hint}: S1 invocation missing")
            continue
        for side, i in (("cli", 0), ("srv", 1)):
            pipe = (s["pipe"] or ("-", "-"))[i]
            if pipe.split("/")[-1] != hint:
                c4.append(f"{hint} {side}: TUNPIPE hint {pipe}")
            cfg = _cfg_of((s["cfg"] or ("-", "-"))[i])
            vals = [cfg.get(f) for f in _CFG_FIELDS]
            if any(v != "false" for v in vals):
                c4.append(f"{hint} {side}: TUNCFG {vals}")
    out(f"(4) both ends of every S1 invocation print TUNPIPE with the given hint and a TUNCFG whose four fields read false: {'ok' if not c4 else '; '.join(c4)}")
    out("REPORTED ONLY (never scored): per-hint goodput and completion time, S1 against B0 (n = 5, no A/A)")
    for hint in ("auto", "realtime"):
        for seat in ("B0", "S1"):
            i = inv.get((seat, hint))
            if i:
                rs = [x for x in i["reps"].values() if x["ok"]]
                out(f"  {seat} {hint}: n={len(rs)} goodput med {fm(lc.med([x['mbps'] for x in rs if x['mbps'] is not None]), 2)} completion med {fm(lc.med([x['seconds'] for x in rs if x['seconds'] is not None]), 2)} s")
    if seat_abort:
        res = "T ABORT-SEAT (B0 is not main's: " + "; ".join(seat_abort) + ")"
    elif not done:
        res = "T FAIL (no T67DONE)"
    elif c2:
        res = "T VOID (the file never reached the tunnel on B0)"
    elif c1 or c3 or c4:
        res = "T FAIL"
    else:
        res = "T PASS"
    out(res)
    out("VERDICT-67T " + res)
    return res


def smoke_t(path, out=print):
    inv, refs, done = tun_parse(path)
    bad = []

    def crit(name, fails):
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)
    crit("one invocation per seat at auto", [f"{s}: no invocation" for s in ("B0", "S1") if (s, "auto") not in inv])
    cfgbad = []
    for k, i in inv.items():
        for side in (0, 1):
            vals = [_cfg_of((i["cfg"] or ("-", "-"))[side]).get(f) for f in _CFG_FIELDS]
            if any(v in (None, "-") for v in vals):
                cfgbad.append(f"{k[0]} {k[1]} {'cli' if side == 0 else 'srv'}: TUNCFG {vals}")
    crit("every endpoint log has a `resolved configuration` line (TUNCFG fields not `-`; a missing line is a harness or log-level fault)", cfgbad)
    want = {(s, c, k) for s in ("S1", "B0") for c in ("check", "run") for k in TUN_KEYS}
    have = {(r["seat"], r["cmd"], r["key"]) for r in refs}
    crit("all 16 T67REF lines present", [f"missing {x}" for x in sorted(want - have)])
    crit("the refusal commands' `resolved configuration` line is present on B0 (cfg not `-`)", [f"B0 run {r['key']}: cfg=-" for r in refs if r["seat"] == "B0" and r["cmd"] == "run" and r["cfg"] == "-"])
    crit("T67DONE", [] if done else ["no T67DONE"])
    if bad:
        out("SMOKE-FAIL " + "; ".join(bad))
        return 1
    out("SMOKE-PASS")
    return 0


# ── stage G (plan 41d with the seats rebound) ──
def _g_seat(which, arm):
    return plan67.G_SEATS[which][0 if arm == "CAND" else 1]


def g_row(row, arm, cli, srv, drv, sha, sha_cand, sha_base, which):
    """Stage G's side of one row (LD_WITNESS67=g1|g2, plan 41d): fills row["w67g"], returns the problems that void the row (the seat / sha comparison and the PRECONDITION witnesses of 3.1 / 4.1)."""
    import pool54_score
    p = pool54_score.sha_problems(arm, sha, sha_cand, sha_base)
    seat = _g_seat(which, arm)
    w = readings(cli, srv, samples=False)
    w.update({"which": which, "seat": seat, "sha_checked": pool54_score.sha_checked(arm, sha, sha_cand, sha_base)})
    fail = []
    if which == "g1":
        three = bool(w.get("g_scw") and w.get("g_plx") and w.get("g_rlaw"))
        if seat == "S1" and not three:
            fail.append("g1-no-gauge:" + ",".join(k for k, g in (("scw", "g_scw"), ("plx", "g_plx"), ("rlaw", "g_rlaw")) if not w.get(g)))
        if seat == "B0" and (w.get("g_scw") or w.get("g_plx") or w.get("g_rlaw")):
            fail.append("g1-gauge-prints-on-B0")
    else:
        if seat == "S2" and w.get("pslot") is None:
            fail.append("g2-no-pslot-on-S2")
        if seat == "S1" and w.get("pslot") is not None:
            fail.append("g2-pslot-on-S1")
    if (w.get("rr_max") or 0) > 0:
        fail.append(f"w-route-rr={w['rr_max']}>0 on a Bulk row")
    w["fail"] = fail
    row["w67g"] = w
    return p + ([] if row.get("dnf") else ["w67g:" + x for x in fail])


def g_status(row):
    """The S06-FAIL precedence on a finished stage G row (the F2-FAIL analogue): an S2 row (g2 CAND) with taper > 0 or plost > 0 keeps its 41d label in status_41d; never replaced."""
    w = row.get("w67g") or {}
    if w.get("seat") != "S2" or w.get("which") != "g2" or row.get("arm") != "CAND":
        return
    st = row["status"]
    if st in INFRA or any(x.startswith("bin-sha") for x in row.get("problems") or []) or st == "WITNESS-FAIL" or w.get("pslot") is None:
        return
    why = [f"{k}={w[k]}" for k in ("taper", "plost") if (w.get(k) or 0) > 0]
    if why:
        row["status_41d"], row["status"], row["s06_why"] = st, "S06-FAIL", why


def view41d_g(row):
    if row.get("status") != "S06-FAIL":
        return row
    st = row.get("status_41d") or "VOID-RC"
    return dict(row, status="VOID-RC" if st in pp.REPLACEABLE else st)


def rows_41d_view_g(paths):
    rows, tokens = pp.rows_of(paths)
    return [view41d_g(r) for r in rows], tokens


def score67g(paths, out=print, which="g1"):
    """score41d's output byte for byte on the 41d view, then the G witness table, then `GATE-67G1|2 PASS|FAIL`. PASS iff GATE-41d PASS and no S06-FAIL row and no unreplaced witness failure."""
    res = pp._score_guard(paths, out, "v2", rows_fn=rows_41d_view_g)
    rows_all, tokens = pp.rows_of(paths)
    rows, _ = pp.apply_replacements(rows_all)
    out("")
    out(f"G-WITNESS67 ({which}; registration {'3.1' if which == 'g1' else '4.1'}: row witnesses outside the min-p family; the verdict above is score41d's)")
    nw = 0
    for cell, _ in pp.GUARD_CELLS:
        rc_ = [r for r in rows if r["cell"] == cell and r["arm"] == "CAND"]
        rb_ = [r for r in rows if r["cell"] == cell and r["arm"] == "BASE"]
        if not rc_:
            continue
        ws = [r.get("w67g") or {} for r in rc_ + rb_]
        out(f"  {cell:<8} CAND({plan67.G_SEATS[which][0]}) rows {len(rc_)} BASE({plan67.G_SEATS[which][1]}) rows {len(rb_)}  witness failures {sum(1 for w in ws if w.get('fail'))}  "
            f"rr>0 rows {sum(1 for w in ws if (w.get('rr_max') or 0) > 0)}  S06-FAIL {sum(1 for r in rc_ if r['status'] == 'S06-FAIL')}")
    s06 = [r for r in rows if r["status"] == "S06-FAIL"]
    dnf06 = []
    for cell, _ in pp.GUARD_CELLS:
        base = [r for r in rows if r["cell"] == cell and r["arm"] == "BASE" and r["status"] in ("LIVE",)]
        if which == "g2" and base and all(not r.get("dnf") for r in base):
            dnf06 += [r for r in rows if r["cell"] == cell and r["arm"] == "CAND" and r.get("dnf") and r["status"] == "LIVE"]
    for r in s06:
        out(f"  S06-FAIL {r['cell']} CAND s{r['seed']} rep{r['rep']} (would be {r.get('status_41d')}): {'; '.join(r.get('s06_why') or [])}")
    for r in dnf06:
        out(f"  S06-FAIL {r['cell']} CAND s{r['seed']} rep{r['rep']}: DNF where every S1 (BASE) row of the cell completes")
    for r in rows:
        w = r.get("w67g") or {}
        if w.get("fail") and r["status"] != "LIVE" or (w.get("fail") and not r.get("dnf") and r["status"] == "WITNESS-FAIL"):
            nw += 1
            out(f"  WITNESS-FAIL {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']} not replaced ({r['status']}): {'; '.join(w['fail'])}")
    for r in rows_all:
        w = r.get("w67g") or {}
        if (w.get("rr_max") or 0) > 0:
            out(f"  W-ROUTE {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']}: rr max {w['rr_max']} on a Bulk row (reported with the row)")
    aborted = sorted({t for t in tokens if t in pp.ABORT_TOKENS})
    why = []
    if aborted:
        why.append("UNSCOREABLE (abort " + ",".join(aborted) + ")")
    if res["gate"] != "PASS":
        why.append("GATE-41d " + ("NO VERDICT, re-run" if res["gate"] == "NO VERDICT" else res["gate"]))
    if s06 or dnf06:
        why.append(f"{len(s06) + len(dnf06)} S06-FAIL row(s)")
    if nw:
        why.append(f"{nw} unreplaced witness failure(s)")
    name = "GATE-67G" + which[1:]
    out(f"{name} PASS" if not why else f"{name} FAIL (" + "; ".join(why) + ")")
    return res, len(s06) + len(dnf06), nw


def smoke67g(path, which="g1", out=print):
    import io
    from contextlib import redirect_stdout
    rows, _ = pp.rows_of([path])
    got = {(r["cell"], r["arm"]): r for r in rows}
    bad = []

    def crit(name, fails):
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)
    want = [(c, a) for c, _, a, _, _ in pp.SMOKE41D]
    out(f"SMOKE67G {which} rows={len(rows)}/{len(want)} (plan 41d's smoke rows; the values are not read, only the criteria below)")
    crit("every smoke row ran", [f"{c} {a}: no row" for c, a in want if (c, a) not in got])
    rs = [got[k] for k in want if k in got]
    crit("every smoke row is LIVE (no void, no S06-FAIL)", [f"{r['cell']} {r['arm']}: {r['status']} {r.get('problems')}" for r in rs if r["status"] != "LIVE"])
    buf = io.StringIO()
    with redirect_stdout(buf):
        rc41 = pp.smoke(path, plan41c=True, plan41d=True)
    lines = [x for x in buf.getvalue().splitlines() if x]
    for x in lines:
        if not x.startswith(("SMOKE-PASS", "SMOKE-FAIL")):
            out("  " + x)
    crit("plan 41d's own smoke criteria", [] if rc41 == 0 else [x for x in lines if x.startswith("SMOKE-FAIL")] or ["plan 41d's smoke failed"])
    crit("the seat witnesses hold on every smoke row (" + ("scw / plx / rlaw on S1, none on B0" if which == "g1" else "pslot on S2, none on S1") + "; rr = 0)", [f"{r['cell']} {r['arm']}: {'; '.join((r.get('w67g') or {}).get('fail') or ['no w67g'])}" for r in rs if not (r.get("w67g") is not None and not r["w67g"].get("fail"))])
    crit("bin_sha matches the row's seat", [f"{r['cell']} {r['arm']}: {x}" for r in rs for x in r.get("problems") or [] if x.startswith("bin-sha")])
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
    if cmd == "row":
        if a and a[0].startswith("67"):      # the plan may be the first argument or come from LD_PLAN (the battery's `row <cell> <arm> ...` shape)
            plan_id, a = a[0], a[1:]
        else:
            plan_id = os.environ.get("LD_PLAN", "")
        cell, arm, seed, rep, rc, wall, drv, cli, srv = a[:9]
        cot = a[9] if len(a) > 9 else 0
        sha = a[10] if len(a) > 10 else None
        os.environ.setdefault("LD_PLAN", plan_id)
        try:
            r = row_of(plan_id, cell, arm, seed, rep, rc, wall, lc.read(drv), lc.read(cli), lc.read(srv), cot, sha)
        except Exception as e:      # never raise: a parser that dies deletes the rows the abort accounting is made of
            r = error_row(plan_id, cell, arm, seed, rep, rc, e)
        print("P67ROW " + json.dumps(r, sort_keys=True, default=str))
        return 0
    if cmd == "score":
        score(a[0], a[1:])
        return 0
    if cmd == "smoke":
        return smoke(a[0], a[1])
    if cmd == "block1":
        return block1(a[0], a[1])
    if cmd == "tun":
        score_t(a[0])
        return 0
    if cmd == "smokeT":
        return smoke_t(a[0])
    if cmd == "scoreg":
        score67g(a[1:], print, a[0])
        return 0
    if cmd == "smokeg":
        return smoke67g(a[1], a[0])
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
