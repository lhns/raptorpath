#!/usr/bin/env python3
"""Parser and scorer for THE V-SK BATTERY (incremental SACK marking, status
§26 finding 1): the sender's SACK drain no longer re-marks every retained seq
above the stalled frontier per report (`SackMemo`), against `main` 92934a7
(MAIN), one session, interleaved. Derived from the V-R2 parser
(`threadr2_parse.py`), the §21 pass rule unchanged except:

  * the `[LAG]` clauses (client and server) score E = p99_us - min_us of the
    `[LAG] phase=xfer` line (raw p99 is still reported);
  * `lag_E_spread_cli` (c8-100 only): the client E max/min ratio over reps,
    NEW WORSE iff NEW's ratio > MAIN's x 1.5, BETTER iff < MAIN's / 1.5.

    threadsk_parse.py row   <cell> <arm> <seed> <rep> <rc> <wall_s> <drv_out> <cli_log> <srv_log> [cotenant] [bin_sha]
    threadsk_parse.py check <ledger>
    threadsk_parse.py smoke <ledger>
    threadsk_parse.py cost  <ledger>
    threadsk_parse.py score <ledger>

Row witness (two-sided, rule 1): the NEW client log carries a `[SACKI]` line
with reports > 0 (the incremental marker ran); MAIN logs carry no `[SACKI]`
on either end. A violation is WITNESS-FAIL.
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import stage3_parse as sp  # noqa: E402
from threadp1_parse import rtp_floor, wake_of  # noqa: E402
from threadq1_parse import ctld_ratio, topo_lines, _in_window  # noqa: E402
from l1ab import (  # noqa: E402
    bin_check,
    sel,
    vals,
    per_byte,
    reading,
    spread_rel,
    witness_fails,
    blockers,
    mmm,
    REL_FLOOR,
    MIN_LIVE,
    WITNESS_FAIL_LIMIT,
    rdiag,
    RDIAG_MIN_MSGS,
    _RDIAG,
)

# ── THE DESIGN ───────────────────────────────────────────────────────────
ARMS = {
    "MAIN": ("window", "Rlc", "bulk", "ACTIVE"),
    "NEW": ("window", "Rlc", "bulk", "ACTIVE"),
}
sp.ARM_SPEC.update(ARMS)
ARM_BIN = {"MAIN": "main", "NEW": "new"}
SCORED = ["NEW"]
CELLS = ["c1s-400", "c1d-400", "c2-100", "c8-100"]
SEEDS = ["42", "7"]
EMB_ECHO = "emission batching ACTIVE"
SPREAD_CELL = "c8-100"
SPREAD_FACTOR = 1.5       # lag_E_spread_cli: WORSE iff NEW ratio > MAIN ratio x this, BETTER iff < MAIN / this

# §5's committed relative MDE (status.md §5 (a)): cell -> {gp, cpu}. `cpu`
# is the CPUCLI MDE, applied to CPUSRV as a declared transfer (§11-§13).
S3_REL = {
    "c1s-400": {"gp": 0.049, "cpu": 0.024},
    "c1d-400": {"gp": 0.056, "cpu": 0.065},
    "c2-100": {"gp": 0.014, "cpu": 0.063},
    "c8-100": {"gp": 0.040, "cpu": 0.116},
}
DNF_THRESHOLD = 0.20      # §5
FEED_BAND = 1.3           # arm's plc/truth within [1/1.3, 1.3] x MAIN's, per leg (§11-§13)
ABORT_FIRST_FIVE = sp.ABORT_FIRST_FIVE


_SACKI = re.compile(r"\[SACKI\] reports=(\d+) ranges=(\d+) visited=(\d+) newly=(\d+)")


def sacki(lines):
    """The last (cumulative) `[SACKI]` line -> {reports, ranges, visited, newly}, or None."""
    last = None
    for ln in lines or []:
        m = _SACKI.search(ln)
        if m:
            last = {k: int(v) for k, v in zip(("reports", "ranges", "visited", "newly"), m.groups())}
    return last


def lag_E(lines, window):
    """(p99_us, min_us, E = p99 - min) of the measured window's main-runtime
    `[LAG] phase=xfer` line; None where unread."""
    for ln in (lc._thr_windows(lines, "xfer").get(window) or []):
        if "[LAG] " in ln and "[LAG] io " not in ln:
            p99, mn = lc.fnum(lc.field(ln, "p99_us")), lc.fnum(lc.field(ln, "min_us"))
            return p99, mn, (p99 - mn if p99 is not None and mn is not None else None)
    return None, None, None


def io_columns(lines, prefix, phase="xfer", window="run=1"):
    """Flat columns from one window's `[IOWN]` lines (all None when absent,
    so the row shape never depends on the capture). `ack_tok` = how many of
    the window's owner lines carry the Q2 sender-lane token."""
    def k(s):
        return "%s_%s" % (prefix, s)

    own = [ln for ln in lines or [] if "[IOWN] " in ln and _in_window(ln, phase, window)]
    cols = {k("owners"): len(own) if own else None,
            k("ack_tok"): sum(1 for ln in own if " ack_batches=" in ln) if own else None}

    def s(key):
        v = [lc.inum(lc.field(ln, key)) for ln in own]
        v = [x for x in v if x is not None]
        return sum(v) if v else None

    for key in ("tx_dg", "tx_batches", "rx_dg", "rx_batches", "rx_capped", "ack_dg", "ack_batches", "send_err"):
        cols[k(key)] = s(key)
    return cols


# ── ROW ──────────────────────────────────────────────────────────────────
def make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant=0, bin_sha=None):
    row = sp.make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant)
    row["bin_sha"] = bin_sha
    row["bin"] = ARM_BIN[arm]
    row["emb_gate_cli"] = lc.gate(cli, "RWM_EMIT_BATCH")
    row["emb_gate_srv"] = lc.gate(srv, "RWM_EMIT_BATCH")
    row["emb_echo_cli"] = any(EMB_ECHO in ln for ln in cli)
    row["iort_gate_cli"] = lc.gate_tok(cli, "RWM_IO_RT")
    row["iort_gate_srv"] = lc.gate_tok(srv, "RWM_IO_RT")
    tc, ts = topo_lines(cli), topo_lines(srv)
    row["topo_n_cli"], row["topo_n_srv"] = len(tc), len(ts)
    row["rtobs_gate_cli"] = lc.gate_tok(cli, "RWM_RTOBS")
    row["rtobs_gate_srv"] = lc.gate_tok(srv, "RWM_RTOBS")
    for side, lines in (("cli", cli), ("srv", srv)):
        row[f"rdiag_gate_{side}"] = lc.gate_tok(lines, "RWM_RDIAG")
        b, bmax, q, n = rdiag(lines)
        row[f"rdiag_{side}_busy_med"], row[f"rdiag_{side}_busy_max"] = b, bmax
        row[f"rdiag_{side}_q_med"], row[f"rdiag_{side}_n"] = q, n
        si = sacki(lines)
        for key in ("reports", "ranges", "visited", "newly"):
            row[f"{side}_sacki_{key}"] = si[key] if si else None
    w = wake_of(cli)
    row["wake"] = w
    for i, v in rtp_floor(cli).items():
        row["rtp_floor_p%d" % i] = v
    row.update(lc.thr_columns(cli, "cli", "xfer", "run=1"))
    row.update(lc.thr_columns(srv, "srv", "xfer", "obj=1"))
    for side, lines, win in (("cli", cli, "run=1"), ("srv", srv, "obj=1")):
        _, mn, e = lag_E(lines, win)
        row[f"{side}_lag_min_us"], row[f"{side}_lag_E"] = mn, e
    row.update(io_columns(cli, "cli", "xfer", "run=1"))
    row.update(io_columns(srv, "srv", "xfer", "obj=1"))
    t1, t2 = lc.thr(cli, "xfer", "run=1"), lc.thr(srv, "xfer", "obj=1")
    row["cli_threads"] = [(x["comm"], x["cores"]) for x in ((t1 or {}).get("threads") or [])]
    row["srv_threads"] = [(x["comm"], x["cores"]) for x in ((t2 or {}).get("threads") or [])]
    row["acks_per_dgram"] = ctld_ratio(srv)
    for key in ("ack", "cmd", "paused", "pace", "tun", "timer_acked"):
        row["wake_" + key] = (w or {}).get(key)
    p = []
    if row["emb_gate_cli"] != 1 or row["emb_gate_srv"] != 1:
        p.append(f"emit-gate={row['emb_gate_cli']}/{row['emb_gate_srv']}")
    if not row["emb_echo_cli"]:
        p.append("emit-echo-missing")
    if w is None:
        p.append("wake-token-missing")
    for side in ("cli", "srv"):
        if not row[f"{side}_owners"]:
            p.append(f"no-iown-{side}")
    for side in ("cli", "srv"):
        if row[f"{side}_owners"] and row[f"{side}_ack_tok"] != row[f"{side}_owners"]:
            p.append(f"no-sender-lane-token-{side}")
    if not row["cli_ack_dg"]:
        p.append("acks-not-routed-to-sender")
    if w is not None and "cmd" not in w:
        p.append("wake-cmd-token-missing")
    if row["topo_n_cli"] or row["topo_n_srv"]:
        p.append(f"topo-line={row['topo_n_cli']}/{row['topo_n_srv']}")
    if row["iort_gate_cli"] is not None or row["iort_gate_srv"] is not None:
        p.append(f"iort-gate-on={row['iort_gate_cli']}/{row['iort_gate_srv']}")
    # The fix's two-sided execution witness ([SACKI]).
    if arm == "NEW":
        if not row["cli_sacki_reports"]:
            p.append("sacki-missing-or-zero-cli")
    else:
        for side in ("cli", "srv"):
            if row[f"{side}_sacki_reports"] is not None:
                p.append(f"sacki-on-MAIN-{side}")
    for side in ("cli", "srv"):
        if row[f"rdiag_gate_{side}"] != "1":
            p.append(f"rdiag-gate-{side}={row[f'rdiag_gate_{side}']}")
    if row["rdiag_srv_n"] == 0:
        p.append("no-in-transfer-rdiag-srv")
    if row["rtobs_gate_cli"] != "1" or row["rtobs_gate_srv"] != "1":
        p.append(f"rtobs-gate={row['rtobs_gate_cli']}/{row['rtobs_gate_srv']}")
    for side in ("cli", "srv"):
        if not row[f"{side}_n_workers"] or not row[f"{side}_lag_n"]:
            p.append(f"no-thr-lag-{side}")
        elif row[f"{side}_lag_E"] is None:
            p.append(f"no-lag-min-{side}")
    row["problems"].extend(p)
    if p and row["status"] == "LIVE":
        row["status"] = "WITNESS-FAIL"
    return row


# ── LEDGER ───────────────────────────────────────────────────────────────
_HDR_SHA = re.compile(r"^=== binary (MAIN|NEW) \S+ sha256 ([0-9a-f]{64})")


def rows_of(paths):
    rows, tokens, shas = [], [], {}
    for path in paths:
        for ln in lc.read(path):
            m = _HDR_SHA.match(ln)
            if m:
                shas[m.group(1).lower()] = m.group(2)
            if ln.startswith("TSKROW "):
                try:
                    rows.append(json.loads(ln[len("TSKROW "):]))
                except ValueError:
                    tokens.append("MALFORMED-TSKROW")
            for t in ABORT_FIRST_FIVE:
                if ln.startswith(t):
                    tokens.append(t)
            if ln.startswith("TRUNCATED-AT-REP-BOUNDARY"):
                tokens.append("TRUNCATED-AT-REP-BOUNDARY")
    return bin_check(rows, shas), tokens, shas


fmt = sp.fmt


def spread_ratio(v):
    """max/min over reps; None when unreadable (fewer than 2 values or min <= 0)."""
    if len(v) < 2 or min(v) <= 0:
        return None
    return max(v) / min(v)


def spread_reading(xv, rv, name="ARM"):
    """`lag_E_spread_cli`: single numbers per arm, no ranges."""
    xr, rr = spread_ratio(xv), spread_ratio(rv)
    if rr is None:
        return "VACUOUS", "MAIN spread unreadable"
    if xr is None:
        return "VACUOUS", f"{name} spread unreadable"
    det = f"{name} max/min={xr:.3f} MAIN max/min={rr:.3f} (x{SPREAD_FACTOR} band [{rr / SPREAD_FACTOR:.3f},{rr * SPREAD_FACTOR:.3f}])"
    if xr > rr * SPREAD_FACTOR:
        return "WORSE", det
    if xr < rr / SPREAD_FACTOR:
        return "BETTER", det
    return "WITHIN", det


def cell_verdict(rows, live, cell, arm, aborted, out, store=None):
    """PASS / FAIL / UNSCOREABLE of `arm` against MAIN at `cell` (the plan's
    pass rule: no clause WORSE, the feed unmoved, no DNF excess)."""
    xs, rs = sel(live, cell, arm), sel(live, cell, "MAIN")
    why = blockers(rows, live, cell, arm, aborted)
    rel = S3_REL[cell]
    c = {}
    c["goodput"] = reading(vals(xs, "mbps"), vals(rs, "mbps"), rel["gp"], True, arm)
    c["cpu_cli/GB"] = reading(per_byte(xs, "cpu_cli"), per_byte(rs, "cpu_cli"), rel["cpu"], False, arm)
    c["cpu_srv/GB"] = reading(per_byte(xs, "cpu_srv"), per_byte(rs, "cpu_srv"), rel["cpu"], False, arm)
    for side in ("cli", "srv"):
        key = f"{side}_lag_E"
        rv = vals(rs, key)
        c[f"lag_E_{side}"] = reading(vals(xs, key), rv, spread_rel(rv), False, arm)
    if cell == SPREAD_CELL:
        c["lag_E_spread_cli"] = spread_reading(vals(xs, "cli_lag_E"), vals(rs, "cli_lag_E"), arm)
    feed_bad, feed_unread = [], []
    for i in range(sp.N_LEGS[cell]):
        key = "rtp_floor_p%d" % i
        rv = vals(rs, key)
        c[f"rtp_floor_p{i}"] = reading(vals(xs, key), rv, spread_rel(rv), False, arm)
        fk = "feed_ratio_p%d" % i
        fx, fr = lc.med(vals(xs, fk)), lc.med(vals(rs, fk))
        if fx is None or fr is None:
            feed_unread.append(i)
            out(f"  FEED {cell} {arm} p{i} {arm}={fmt(fx, 3)} MAIN={fmt(fr, 3)} -> UNREAD")
        else:
            ok = fr / FEED_BAND <= fx <= fr * FEED_BAND
            if not ok:
                feed_bad.append(i)
            out(f"  FEED {cell} {arm} p{i} plc/truth {arm}={fx:.3f} MAIN={fr:.3f} band=[{fr / FEED_BAND:.3f},"
                f"{fr * FEED_BAND:.3f}] -> {'SAME' if ok else 'FEED-MOVED'}")
    dr = (sum(1 for r in rs if r["dnf"]) / len(rs)) if rs else None
    dx = (sum(1 for r in xs if r["dnf"]) / len(xs)) if xs else None
    dnf_bad = dr is not None and dx is not None and dx - dr > DNF_THRESHOLD
    if store is not None:
        store[(cell, arm)] = {key: rd for key, (rd, _) in c.items()}
    for key, (rd, det) in c.items():
        out(f"  CLAUSE {cell} {arm} {key}: {rd} {det}")
    worse = [key for key, (rd, _) in c.items() if rd == "WORSE"]
    worse += [f"feed_p{i}" for i in feed_bad] + (["dnf"] if dnf_bad else [])
    if why or feed_unread:
        v = "UNSCOREABLE"
    elif worse:
        v = "FAIL"
    else:
        v = "PASS"
    trend = [f"{key}:{rd}" for key, (rd, _) in c.items() if rd.startswith("TREND") or rd == "BETTER"]
    out(f"  CELL {cell} {arm} n={len(xs)} MAIN n={len(rs)} dnf MAIN={fmt(dr, 2)} {arm}={fmt(dx, 2)} "
        f"worse={'+'.join(worse) or '-'} notes={','.join(trend) or '-'}"
        + (f" UNSCOREABLE({'; '.join(why + [f'feed-unread-p{i}' for i in feed_unread])})"
           if (why or feed_unread) else "")
        + f" => {v}")
    return v


def per_report(rows, key):
    """key/reports per row for the client's cumulative [SACKI] counters."""
    return [r[f"cli_sacki_{key}"] / r["cli_sacki_reports"] for r in rows
            if r.get(f"cli_sacki_{key}") is not None and r.get("cli_sacki_reports")]


def reported(live, cell, out):
    for a in ARMS:
        rs = sel(live, cell, a)
        out(f"  REPORT {cell} {a} sacki/report visited={mmm(per_report(rs, 'visited'), 2)} "
            f"newly={mmm(per_report(rs, 'newly'), 2)} ranges={mmm(per_report(rs, 'ranges'), 2)} "
            f"cpu_cli/GB={mmm(per_byte(rs, 'cpu_cli'), 3)} cpu_srv/GB={mmm(per_byte(rs, 'cpu_srv'), 3)} "
            f"acks/dgram={mmm(vals(rs, 'acks_per_dgram'), 3)} busy={mmm(vals(rs, 'busy_med'), 1)}")
        out(f"  REPORT {cell} {a} wake ack={mmm(vals(rs, 'wake_ack'), 0)} cmd={mmm(vals(rs, 'wake_cmd'), 0)} "
            f"tun={mmm(vals(rs, 'wake_tun'), 0)} paused={mmm(vals(rs, 'wake_paused'), 0)} "
            f"timer_acked={mmm(vals(rs, 'wake_timer_acked'), 0)}")
        for side in ("cli", "srv"):
            out(f"  REPORT {cell} {a} {side} cores={mmm(vals(rs, side + '_cores'), 3)} "
                f"parks/s={mmm(vals(rs, side + '_park_per_s'), 0)} "
                f"lag p99={mmm(vals(rs, side + '_lag_p99_us'), 0)} min={mmm(vals(rs, side + '_lag_min_us'), 0)} "
                f"E={mmm(vals(rs, side + '_lag_E'), 0)} "
                f"ack_dg={mmm(vals(rs, side + '_ack_dg'), 0)} rx_capped={mmm(vals(rs, side + '_rx_capped'), 0)}")
        for r in rs:
            out(f"    THREADS {cell} {a} s{r['seed']} r{r['rep']} cli="
                + ",".join(f"{cm}:{x:.3f}" for cm, x in sorted(r.get('cli_threads') or [], key=lambda t: -(t[1] or 0))[:4])
                + " srv="
                + ",".join(f"{cm}:{x:.3f}" for cm, x in sorted(r.get('srv_threads') or [], key=lambda t: -(t[1] or 0))[:4]))


def arm_verdict(verdicts):
    fail = [c for c in CELLS if verdicts[c] == "FAIL"]
    unsc = [c for c in CELLS if verdicts[c] == "UNSCOREABLE"]
    if fail:
        return "FAIL-AT-" + ",".join(fail)
    if unsc:
        return "UNSCOREABLE-AT-" + ",".join(unsc)
    return "PASS-EVERYWHERE"


def outcome(arm_v, aborted):
    """The one pre-registered outcome, in precedence order: SHIP iff
    PASS-EVERYWHERE, otherwise REFUTED-WITH-RECORD."""
    if aborted:
        return "UNSCOREABLE (abort: " + ",".join(aborted) + ")"
    v = arm_v["NEW"]
    if v == "PASS-EVERYWHERE":
        return "SHIP (NEW passes at every cell)"
    if v.startswith("FAIL"):
        return "REFUTED-WITH-RECORD (NEW " + v + ": nothing ships)"
    return "REFUTED-WITH-RECORD (NEW " + v + ": unscoreable, nothing ships)"


def score(paths, out=print):
    rows, tokens, shas = rows_of(paths)
    live = [r for r in rows if r["status"] == "LIVE"]
    aborted = [t for t in ABORT_FIRST_FIVE if t in tokens]
    out(f"BINARIES main={shas.get('main', '-')} new={shas.get('new', '-')}")
    out("STATUS-TABLE (every invocation row)")
    for st in ("LIVE", "NO_DATA", "VOID-RC", "VOID-COTENANT", "CONTAMINATED", "WITNESS-FAIL"):
        out(f"  {st:<13} {sum(1 for r in rows if r['status'] == st)}")
    for r in rows:
        if r["status"] != "LIVE":
            out(f"  NONLIVE {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} "
                f"rc={r['rc']} {' '.join(r['problems'])}")
    out(f"  ABORT-TOKENS {' '.join(aborted) or 'none'}; truncated="
        f"{int('TRUNCATED-AT-REP-BOUNDARY' in tokens)}")
    out("")
    out("PER-REP (rule 4): cell arm seed rep mbps cpu_cli cpu_srv busy rtp_floor feed lag_E cli/srv (p99 cli/srv) sacki visited/report")
    for r in sorted(rows, key=lambda r: (CELLS.index(r["cell"]) if r["cell"] in CELLS else 9,
                                         r["arm"], r["seed"], r["rep"])):
        legs = range(sp.N_LEGS.get(r["cell"], 1))
        vr = (r["cli_sacki_visited"] / r["cli_sacki_reports"]
              if r.get("cli_sacki_reports") and r.get("cli_sacki_visited") is not None else None)
        out(f"  {r['cell']} {r['arm']} s{r['seed']} r{r['rep']} {r['status']} mbps={fmt(r.get('mbps'), 1)} "
            f"cli={fmt(r.get('cpu_cli'), 2)} srv={fmt(r.get('cpu_srv'), 2)} busy={fmt(r.get('busy_med'), 1)} "
            f"rtp={'/'.join(str(r.get('rtp_floor_p%d' % i, '-')) for i in legs)} "
            f"feed={'/'.join(fmt(r.get('feed_ratio_p%d' % i), 3) for i in legs)} "
            f"E={fmt(r.get('cli_lag_E'), 0)}/{fmt(r.get('srv_lag_E'), 0)} "
            f"(p99 {fmt(r.get('cli_lag_p99_us'), 0)}/{fmt(r.get('srv_lag_p99_us'), 0)}) "
            f"sacki_v/r={fmt(vr, 2)}")
    out("")
    out("PER-SEED MEDIANS (goodput, cpu_cli)")
    for cell in CELLS:
        for a in ARMS:
            out(f"  {cell} {a} " + " ".join(
                f"s{s}: gp={fmt(lc.med(vals(sel(live, cell, a, s), 'mbps')), 1)} "
                f"cli={fmt(lc.med(vals(sel(live, cell, a, s), 'cpu_cli')), 2)}" for s in SEEDS))
    out("")
    cellv = {a: {} for a in SCORED}
    readings = {}
    for cell in CELLS:
        out(f"[{cell}]")
        for a in SCORED:
            cellv[a][cell] = cell_verdict(rows, live, cell, a, aborted, out, readings)
        reported(live, cell, out)
    out("")
    arm_v = {a: arm_verdict(cellv[a]) for a in SCORED}
    for a in SCORED:
        out(f"ARM {a} " + " ".join(f"{c}:{cellv[a][c]}" for c in CELLS) + f" => {arm_v[a]}")
    v = outcome(arm_v, aborted)
    out("VERDICT " + v)
    return v


def check(path):
    rows, tokens, _ = rows_of([path])
    if not rows or "MALFORMED-TSKROW" in tokens:
        print(f"CHECK-FAIL {path} rows={len(rows)} tokens={tokens}")
        return 1
    print(f"CHECK-OK {path} rows={len(rows)} live={sum(1 for r in rows if r['status'] == 'LIVE')}")
    return 0


def smoke(path):
    """rc 0 iff every smoke row is LIVE with every gauge the scorer reads and
    both arms appear (the [SACKI] witnesses are part of LIVE)."""
    rows, _, _ = rows_of([path])
    bad = []
    for r in rows:
        miss = []
        if r["status"] != "LIVE":
            miss.append(r["status"])
        for k in ("cpu_cli", "cpu_srv", "busy_med", "mbps", "cli_lag_E", "srv_lag_E",
                  "cli_owners", "srv_owners", "wake_ack"):
            if r.get(k) is None:
                miss.append(f"no-{k}")
        if not r.get("cli_ack_dg"):
            miss.append("no-cli_ack_dg")
        if r.get("rdiag_srv_busy_med") is None:
            miss.append("no-rdiag_srv_busy_med")
        if r["arm"] == "NEW" and not r.get("cli_sacki_reports"):
            miss.append("no-cli_sacki_reports")
        for i in range(sp.N_LEGS[r["cell"]]):
            for k in ("truth_loss_p%d" % i, "plc_p%d" % i, "rtp_floor_p%d" % i):
                if r.get(k) is None:
                    miss.append(f"no-{k}")
        print(f"SMOKE-ROW {r['cell']} {r['arm']} bin={r['bin']} rc={r['rc']} status={r['status']} "
              f"mbps={r.get('mbps')} cli={r.get('cpu_cli')} srv={r.get('cpu_srv')} "
              f"sacki={r.get('cli_sacki_reports')}/{r.get('cli_sacki_visited')}/{r.get('cli_sacki_newly')} "
              f"E={r.get('cli_lag_E')}/{r.get('srv_lag_E')} {' '.join(r['problems'] + miss)}")
        if miss:
            bad.append(r)
    arms = {r["arm"] for r in rows}
    if not rows or bad or arms != set(ARMS):
        print(f"ABORT-SMOKE rows={len(rows)} bad={len(bad)} arms={sorted(arms)}")
        return 1
    print(f"SMOKE-PASS rows={len(rows)}")
    return 0


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd == "row":
        cell, arm, seed, rep, rc, wall, drv, cli, srv = a[:9]
        cot = a[9] if len(a) > 9 else 0
        bsha = a[10] if len(a) > 10 else None
        row = make_row(cell, arm, seed, rep, rc, wall, lc.read(drv), lc.read(cli), lc.read(srv), cot, bsha)
        print("TSKROW " + json.dumps(row, sort_keys=True))
        return 0
    if cmd == "check":
        return check(a[0])
    if cmd == "smoke":
        return smoke(a[0])
    if cmd == "cost":
        return sp.cost(a[0])
    if cmd == "score":
        score([a[0]])
        return 0
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
