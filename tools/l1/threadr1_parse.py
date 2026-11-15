#!/usr/bin/env python3
"""Parser and scorer for THE V-R1 BATTERY (docs/status.md, "17. Receiver
per-datagram cost (R1) — pre-registration"): the receiver's four cheap
per-datagram fixes (pooled inbound Vecs, no per-message Vec, a ring-bitmap
received set, a ring `[LAT]` store) against `main` ce60079 (MAIN), one
session, interleaved. Derived from the V-Q2 parser (`threadq2_parse.py`).

    threadr1_parse.py row   <cell> <arm> <seed> <rep> <rc> <wall_s> <drv_out> <cli_log> <srv_log> [cotenant] [bin_sha]
    threadr1_parse.py check <ledger>
    threadr1_parse.py smoke <ledger>
    threadr1_parse.py cost  <ledger>
    threadr1_parse.py score <ledger>

`row` reuses `stage3_parse.make_row` and adds (every arm):

  emit     `[GATES] RWM_EMIT_BATCH=1` both endpoints, the "emission batching
           ACTIVE" echo on the client;
  gates    `[GATES] RWM_RTOBS=1` and `RWM_RDIAG=1` on both endpoints;
  rdiag    the in-transfer `[RDIAG]` busy median / max and q_avg per side
           (`msgs/s >= 1000`; the server's is the c1d prediction's datum);
  q2       the Q2 mechanism both arms already carry (ce60079 includes it):
           `[IOWN]` window lines with `ack_batches=`, the client's
           `ack_dg > 0`, the `wake[` token with `cmd=`, no `[TOPO]` / IO_RT;
  r1       the execution witness, two-sided. R1 rows: every `[IOWN]` window
           line on both ends carries `ibp_hit=` and the server's owners
           reused pooled inbound Vecs (`ibp_hit > 0`: the receiver returned
           them, the owner took them back). MAIN rows: no `ibp_hit=` token;
  thr/lag  the main runtime's `[THR]`/`[LAG]` window of the measured object
           on both ends (client `run=1`, server `obj=1`);
  io       per side: the owners' datagrams per batch on each hop,
           `rx_capped`, the lock-wait gauge, the pool's hits and misses.

Scoring constants are below, one definition each, restated in the
pre-registration.
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
from threadq1_parse import ackdiag_gaps, ctld_ratio, topo_lines, _in_window  # noqa: E402
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
    "R1": ("window", "Rlc", "bulk", "ACTIVE"),
}
sp.ARM_SPEC.update(ARMS)
ARM_BIN = {"MAIN": "main", "R1": "r1"}
SCORED = ["R1"]
CELLS = ["c1s-400", "c1d-400", "c2-100", "c8-100"]
SEEDS = ["42", "7"]
EMB_ECHO = "emission batching ACTIVE"

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
RDIAG_BUSY_MAX = 90.0     # the c1d prediction: the server receiver's [RDIAG] busy median stays below this
ABORT_FIRST_FIVE = sp.ABORT_FIRST_FIVE


def io_columns(lines, prefix, phase="xfer", window="run=1"):
    """Flat columns from one window's `[IOWN]` lines (all None when absent,
    so the row shape never depends on the capture). `ack_tok` = how many of
    the window's owner lines carry the Q2 sender-lane token, `ibp_tok` the R1
    pool token."""
    def k(s):
        return "%s_%s" % (prefix, s)

    own = [ln for ln in lines or [] if "[IOWN] " in ln and _in_window(ln, phase, window)]
    cols = {k("owners"): len(own) if own else None,
            k("ack_tok"): sum(1 for ln in own if " ack_batches=" in ln) if own else None,
            k("ibp_tok"): sum(1 for ln in own if " ibp_hit=" in ln) if own else None}

    def s(key):
        v = [lc.inum(lc.field(ln, key)) for ln in own]
        v = [x for x in v if x is not None]
        return sum(v) if v else None

    fr = [lc.fnum(lc.field(ln, "asleep_frac")) for ln in own]
    fr = [x for x in fr if x is not None]
    cols[k("asleep_max")] = max(fr) if fr else None
    for key in ("tx_dg", "tx_batches", "ctrl_dg", "ctrl_batches", "rx_dg", "rx_batches",
                "rx_capped", "ack_dg", "ack_batches", "send_err", "too_large_staged",
                "orphaned", "polls", "drains", "drv_on", "drv_off", "ibp_hit", "ibp_miss"):
        cols[k(key)] = s(key)
    for n, d, out in (("tx_dg", "tx_batches", "tx_per_batch"), ("rx_dg", "rx_batches", "rx_per_batch"),
                      ("ack_dg", "ack_batches", "ack_per_batch")):
        dv, bv = cols[k(n)], cols[k(d)]
        cols[k(out)] = (dv / bv) if (bv and dv is not None) else None
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
    w = wake_of(cli)
    row["wake"] = w
    for i, v in rtp_floor(cli).items():
        row["rtp_floor_p%d" % i] = v
    row.update(lc.thr_columns(cli, "cli", "xfer", "run=1"))
    row.update(lc.thr_columns(srv, "srv", "xfer", "obj=1"))
    row.update(io_columns(cli, "cli", "xfer", "run=1"))
    row.update(io_columns(srv, "srv", "xfer", "obj=1"))
    t1, t2 = lc.thr(cli, "xfer", "run=1"), lc.thr(srv, "xfer", "obj=1")
    row["cli_threads"] = [(x["comm"], x["cores"]) for x in ((t1 or {}).get("threads") or [])]
    row["srv_threads"] = [(x["comm"], x["cores"]) for x in ((t2 or {}).get("threads") or [])]
    row["acks_per_dgram"] = ctld_ratio(srv)
    for key in ("ack", "cmd", "paused", "pace", "tun", "timer_acked"):
        row["wake_" + key] = (w or {}).get(key)
    for i, g in ackdiag_gaps(cli).items():
        for kk, v in g.items():
            row["ackgap_%s_p%d" % (kk, i)] = v
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
    # The Q2 mechanism is in both trees (ce60079 contains it): both arms.
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
    # R1's own two-sided execution witness.
    if arm == "R1":
        for side in ("cli", "srv"):
            if row[f"{side}_owners"] and row[f"{side}_ibp_tok"] != row[f"{side}_owners"]:
                p.append(f"no-pool-token-{side}")
        if not row["srv_ibp_hit"]:
            p.append("pool-never-hit-srv")
    else:
        for side in ("cli", "srv"):
            if row[f"{side}_ibp_tok"]:
                p.append(f"pool-token-on-MAIN-{side}")
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
    row["problems"].extend(p)
    if p and row["status"] == "LIVE":
        row["status"] = "WITNESS-FAIL"
    return row


# ── LEDGER ───────────────────────────────────────────────────────────────
_HDR_SHA = re.compile(r"^=== binary (MAIN|R1) \S+ sha256 ([0-9a-f]{64})")


def rows_of(paths):
    rows, tokens, shas = [], [], {}
    for path in paths:
        for ln in lc.read(path):
            m = _HDR_SHA.match(ln)
            if m:
                shas[m.group(1).lower()] = m.group(2)
            if ln.startswith("TR1ROW "):
                try:
                    rows.append(json.loads(ln[len("TR1ROW "):]))
                except ValueError:
                    tokens.append("MALFORMED-TR1ROW")
            for t in ABORT_FIRST_FIVE:
                if ln.startswith(t):
                    tokens.append(t)
            if ln.startswith("TRUNCATED-AT-REP-BOUNDARY"):
                tokens.append("TRUNCATED-AT-REP-BOUNDARY")
    return bin_check(rows, shas), tokens, shas


fmt = sp.fmt


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
        key = f"{side}_lag_p99_us"
        rv = vals(rs, key)
        c[f"lag_p99_{side}"] = reading(vals(xs, key), rv, spread_rel(rv), False, arm)
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


def reported(live, cell, out):
    for a in ARMS:
        rs = sel(live, cell, a)
        legs = range(sp.N_LEGS[cell])
        out(f"  REPORT {cell} {a} acks/dgram={mmm(vals(rs, 'acks_per_dgram'), 3)} "
            f"busy={mmm(vals(rs, 'busy_med'), 1)} "
            f"rdiag_busy cli={mmm(vals(rs, 'rdiag_cli_busy_med'), 1)} srv={mmm(vals(rs, 'rdiag_srv_busy_med'), 1)} "
            f"rdiag_qavg srv={mmm(vals(rs, 'rdiag_srv_q_med'), 1)} "
            f"pool srv hit={mmm(vals(rs, 'srv_ibp_hit'), 0)} miss={mmm(vals(rs, 'srv_ibp_miss'), 0)} "
            + " ".join(f"gso_p{i}={mmm(vals(rs, 'truth_gso_p%d' % i), 2)}" for i in legs))
        out(f"  REPORT {cell} {a} wake ack={mmm(vals(rs, 'wake_ack'), 0)} cmd={mmm(vals(rs, 'wake_cmd'), 0)} "
            f"tun={mmm(vals(rs, 'wake_tun'), 0)} paused={mmm(vals(rs, 'wake_paused'), 0)} "
            f"timer_acked={mmm(vals(rs, 'wake_timer_acked'), 0)}")
        for side in ("cli", "srv"):
            out(f"  REPORT {cell} {a} {side} cores={mmm(vals(rs, side + '_cores'), 3)} "
                f"main_thr={mmm(vals(rs, side + '_main_cores'), 3)} "
                f"thr_r1={mmm(vals(rs, side + '_thr_r1'), 3)} "
                f"parks/s={mmm(vals(rs, side + '_park_per_s'), 0)} "
                f"lag p99={mmm(vals(rs, side + '_lag_p99_us'), 0)} "
                f"lockwait max={mmm(vals(rs, side + '_asleep_max'), 4)} "
                f"tx/batch={mmm(vals(rs, side + '_tx_per_batch'), 1)} rx/batch={mmm(vals(rs, side + '_rx_per_batch'), 1)} "
                f"ack/batch={mmm(vals(rs, side + '_ack_per_batch'), 1)} ack_dg={mmm(vals(rs, side + '_ack_dg'), 0)} "
                f"rx_capped={mmm(vals(rs, side + '_rx_capped'), 0)} send_err={mmm(vals(rs, side + '_send_err'), 0)}")
        for r in rs:
            out(f"    THREADS {cell} {a} s{r['seed']} r{r['rep']} cli="
                + ",".join(f"{cm}:{x:.3f}" for cm, x in sorted(r.get('cli_threads') or [], key=lambda t: -(t[1] or 0))[:4])
                + " srv="
                + ",".join(f"{cm}:{x:.3f}" for cm, x in sorted(r.get('srv_threads') or [], key=lambda t: -(t[1] or 0))[:4]))


def predictions(live, readings, out):
    """The pre-registered predictions (reported, named; they do not decide
    the outcome). `readings[(cell, arm)]` = the cell's clause readings.
    MET = the stated reading; PARTIAL = its TREND form; else MISSED.
      c1d_goodput_better   goodput BETTER and the server receiver's [RDIAG]
                           busy median < 90 %;
      cpusrv_better        CPUSRV per byte BETTER at c1s and c1d;
      c1s_goodput_within   c1s goodput WITHIN or better (the N = 1 store cap
                           is the constant 1024, §16 finding 3: the receiver
                           is not its ceiling);
      c2_c8_within         goodput and both CPU per byte WITHIN or better at
                           c2 and c8."""
    res = {}

    def rd(cell, key):
        return (readings.get((cell, "R1")) or {}).get(key)

    g = rd("c1d-400", "goodput")
    bs = lc.med(vals(sel(live, "c1d-400", "R1"), "rdiag_srv_busy_med"))
    bm = lc.med(vals(sel(live, "c1d-400", "MAIN"), "rdiag_srv_busy_med"))
    busy_ok = bs is not None and bs < RDIAG_BUSY_MAX
    res["c1d_goodput_better"] = (
        ("MET" if (g == "BETTER" and busy_ok) else "PARTIAL" if (g in ("BETTER", "TREND-BETTER") or busy_ok) else "MISSED")
        + f" (goodput {g}; server [RDIAG] busy median MAIN={fmt(bm, 1)} R1={fmt(bs, 1)}, threshold < {RDIAG_BUSY_MAX:.0f})")
    cs = [rd(c, "cpu_srv/GB") for c in ("c1s-400", "c1d-400")]
    res["cpusrv_better"] = ("MET" if all(x == "BETTER" for x in cs)
                            else "PARTIAL" if all(x in ("BETTER", "TREND-BETTER") for x in cs) else "MISSED") \
        + f" (c1s {cs[0]}, c1d {cs[1]})"
    g1 = rd("c1s-400", "goodput")
    res["c1s_goodput_within"] = ("MET" if g1 in ("WITHIN", "BETTER", "TREND-BETTER") else "MISSED") + f" (c1s goodput {g1})"
    ok, det = True, []
    for c in ("c2-100", "c8-100"):
        for key in ("goodput", "cpu_cli/GB", "cpu_srv/GB"):
            x = rd(c, key)
            det.append(f"{c.split('-')[0]} {key} {x}")
            ok = ok and x in ("WITHIN", "BETTER", "TREND-BETTER")
    res["c2_c8_within"] = ("MET" if ok else "MISSED") + " (" + "; ".join(det) + ")"
    for k, v in res.items():
        out(f"PREDICTION {k}: {v}")
    return res


def ackd_report(paths, out):
    rows, _, _ = rows_of(paths)
    live = [r for r in rows if r["status"] == "LIVE"]
    out("ACK-INTERARRIVAL (RWM_ACKDIAG block, reported only; client [ACKDIAG] gap_us per path)")
    for cell in CELLS:
        for a in ARMS:
            for i in range(sp.N_LEGS[cell]):
                rs = sel(live, cell, a)
                out(f"  ACKGAP {cell} {a} p{i} p50={mmm(vals(rs, 'ackgap_p50_p%d' % i), 0)} "
                    f"p90={mmm(vals(rs, 'ackgap_p90_p%d' % i), 0)} p99={mmm(vals(rs, 'ackgap_p99_p%d' % i), 0)} "
                    f"acks={mmm(vals(rs, 'ackgap_n_p%d' % i), 0)}")
    out(f"  ACKD rows={len(rows)} live={len(live)}")


def arm_verdict(verdicts):
    fail = [c for c in CELLS if verdicts[c] == "FAIL"]
    unsc = [c for c in CELLS if verdicts[c] == "UNSCOREABLE"]
    if fail:
        return "FAIL-AT-" + ",".join(fail)
    if unsc:
        return "UNSCOREABLE-AT-" + ",".join(unsc)
    return "PASS-EVERYWHERE"


def outcome(arm_v, aborted):
    """The one pre-registered outcome, in precedence order."""
    if aborted:
        return "UNSCOREABLE (abort: " + ",".join(aborted) + ")"
    v = arm_v["R1"]
    if v == "PASS-EVERYWHERE":
        return "DELIVERED (R1 passes at every cell: ships)"
    if v.startswith("FAIL"):
        return "REFUTED-WITH-RECORD (R1 " + v + ": nothing ships)"
    return "UNSCOREABLE-AT (R1 " + v + ")"


def score(paths, out=print, ackd=None):
    rows, tokens, shas = rows_of(paths)
    live = [r for r in rows if r["status"] == "LIVE"]
    aborted = [t for t in ABORT_FIRST_FIVE if t in tokens]
    out(f"BINARIES main={shas.get('main', '-')} r1={shas.get('r1', '-')}")
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
    out("PER-REP (rule 4): cell arm seed rep mbps cpu_cli cpu_srv busy rtp_floor feed lag_p99 cli/srv wake ack/cmd/timer_acked")
    for r in sorted(rows, key=lambda r: (CELLS.index(r["cell"]) if r["cell"] in CELLS else 9,
                                         r["arm"], r["seed"], r["rep"])):
        legs = range(sp.N_LEGS.get(r["cell"], 1))
        out(f"  {r['cell']} {r['arm']} s{r['seed']} r{r['rep']} {r['status']} mbps={fmt(r.get('mbps'), 1)} "
            f"cli={fmt(r.get('cpu_cli'), 2)} srv={fmt(r.get('cpu_srv'), 2)} busy={fmt(r.get('busy_med'), 1)} "
            f"rtp={'/'.join(str(r.get('rtp_floor_p%d' % i, '-')) for i in legs)} "
            f"feed={'/'.join(fmt(r.get('feed_ratio_p%d' % i), 3) for i in legs)} "
            f"lag99={fmt(r.get('cli_lag_p99_us'), 0)}/{fmt(r.get('srv_lag_p99_us'), 0)} "
            f"wake={r.get('wake_ack')}/{r.get('wake_cmd')}/{r.get('wake_timer_acked')}")
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
    predictions(live, readings, out)
    arm_v = {a: arm_verdict(cellv[a]) for a in SCORED}
    for a in SCORED:
        out(f"ARM {a} " + " ".join(f"{c}:{cellv[a][c]}" for c in CELLS) + f" => {arm_v[a]}")
    v = outcome(arm_v, aborted)
    out("VERDICT " + v)
    if ackd:
        out("")
        ackd_report(ackd, out)
    return v


def check(path):
    rows, tokens, _ = rows_of([path])
    if not rows or "MALFORMED-TR1ROW" in tokens:
        print(f"CHECK-FAIL {path} rows={len(rows)} tokens={tokens}")
        return 1
    print(f"CHECK-OK {path} rows={len(rows)} live={sum(1 for r in rows if r['status'] == 'LIVE')}")
    return 0


def smoke(path):
    """rc 0 iff every smoke row is LIVE with every gauge the scorer reads and
    both arms appear (the R1 witnesses are part of LIVE)."""
    rows, _, _ = rows_of([path])
    bad = []
    for r in rows:
        miss = []
        if r["status"] != "LIVE":
            miss.append(r["status"])
        for k in ("cpu_cli", "cpu_srv", "busy_med", "mbps", "cli_lag_p99_us", "srv_lag_p99_us",
                  "cli_owners", "srv_owners", "wake_ack"):
            if r.get(k) is None:
                miss.append(f"no-{k}")
        if not r.get("cli_ack_dg"):
            miss.append("no-cli_ack_dg")
        if r.get("rdiag_srv_busy_med") is None:
            miss.append("no-rdiag_srv_busy_med")
        if r["arm"] == "R1" and not r.get("srv_ibp_hit"):
            miss.append("no-srv_ibp_hit")
        for i in range(sp.N_LEGS[r["cell"]]):
            for k in ("truth_loss_p%d" % i, "plc_p%d" % i, "rtp_floor_p%d" % i):
                if r.get(k) is None:
                    miss.append(f"no-{k}")
        print(f"SMOKE-ROW {r['cell']} {r['arm']} bin={r['bin']} rc={r['rc']} status={r['status']} "
              f"mbps={r.get('mbps')} cli={r.get('cpu_cli')} srv={r.get('cpu_srv')} "
              f"topo={r.get('topo_n_cli')}/{r.get('topo_n_srv')} ack_dg={r.get('cli_ack_dg')} "
              f"wake={r.get('wake_ack')}/{r.get('wake_cmd')} lag99={r.get('cli_lag_p99_us')}/"
              f"{r.get('srv_lag_p99_us')} {' '.join(r['problems'] + miss)}")
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
        print("TR1ROW " + json.dumps(row, sort_keys=True))
        return 0
    if cmd == "check":
        return check(a[0])
    if cmd == "smoke":
        return smoke(a[0])
    if cmd == "cost":
        return sp.cost(a[0])
    if cmd == "score":
        score([a[0]], ackd=a[1:] or None)
        return 0
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
