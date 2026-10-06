#!/usr/bin/env python3
"""Parser and scorer for THE D9 ATTRIBUTION BATTERY (docs/status.md, "15. D9
attribution and the c8 lag re-check -- pre-registration"): `main` 88476a4
(MAIN, D9 shipped: the perf body is a task on a runtime worker) against NOD9
(88476a4 with only the D9 hunk of 95c750a reverted: the perf body back on the
`block_on` main thread), one session, interleaved. Derived from the V-Q2 parser
(`threadq2_parse.py`); the Q2-era witnesses now apply to BOTH arms (both
engines are Q2-era), and the D9 execution witness is two-sided:

  MAIN   client `[THR]` main thread (`comm=raptorpath`) < MAIN_THR_MAX core
         (D9 puts the generator on a worker: 0.000 on every Q1/Q2 row);
  NOD9   client main thread >= NOD9_THR_MIN core (the generator runs there).

    threadd9_parse.py row   <cell> <arm> <seed> <rep> <rc> <wall_s> <drv_out> <cli_log> <srv_log> [cotenant] [bin_sha]
    threadd9_parse.py check <ledger>
    threadd9_parse.py smoke <ledger>
    threadd9_parse.py cost  <ledger>
    threadd9_parse.py score <ledger>...

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

# ── THE DESIGN ───────────────────────────────────────────────────────────
ARMS = {
    "MAIN": ("window", "Rlc", "bulk", "ACTIVE"),
    "NOD9": ("window", "Rlc", "bulk", "ACTIVE"),
}
sp.ARM_SPEC.update(ARMS)
ARM_BIN = {"MAIN": "main", "NOD9": "nod9"}
SCORED = ["NOD9"]
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
REL_FLOOR = 0.05          # RTprop floor / [LAG] p99: max(5 %, MAIN half-range / median) (§11-§13)
DNF_THRESHOLD = 0.20      # §5
MIN_LIVE = 3              # live rows per (cell, arm)
WITNESS_FAIL_LIMIT = 2    # failed rows per (arm, cell) that void the cell
FEED_BAND = 1.3           # arm's plc/truth within [1/1.3, 1.3] x MAIN's, per leg (§11-§13)
MAIN_THR_MAX = 0.002      # D9 witness: MAIN's client main-thread cores below this (D9 reads 0.000)
NOD9_THR_MIN = 0.002      # D9 witness: NOD9's client main-thread cores at or above this
LAG_C8_REF = (("Section 14 MAIN", 1358), ("Section 14 Q2", 2202))  # report-only cross-session references (us)
ABORT_FIRST_FIVE = sp.ABORT_FIRST_FIVE


def io_columns(lines, prefix, phase="xfer", window="run=1"):
    """Flat columns from one window's `[IOWN]` lines (all None when absent,
    so the row shape never depends on the capture). `ack_tok` = how many of
    the window's owner lines carry the sender-lane token."""
    def k(s):
        return "%s_%s" % (prefix, s)

    own = [ln for ln in lines or [] if "[IOWN] " in ln and _in_window(ln, phase, window)]
    cols = {k("owners"): len(own) if own else None,
            k("ack_tok"): sum(1 for ln in own if " ack_batches=" in ln) if own else None}

    def s(key):
        v = [lc.inum(lc.field(ln, key)) for ln in own]
        v = [x for x in v if x is not None]
        return sum(v) if v else None

    fr = [lc.fnum(lc.field(ln, "asleep_frac")) for ln in own]
    fr = [x for x in fr if x is not None]
    cols[k("asleep_max")] = max(fr) if fr else None
    for key in ("tx_dg", "tx_batches", "ctrl_dg", "ctrl_batches", "rx_dg", "rx_batches",
                "rx_capped", "ack_dg", "ack_batches", "send_err", "too_large_staged",
                "orphaned", "polls", "drains", "drv_on", "drv_off"):
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
    legs = sp.N_LEGS.get(cell, 1)
    for side in ("cli", "srv"):
        if not row[f"{side}_owners"]:
            p.append(f"no-iown-{side}")
    # Both arms are Q2-era engines: the sender lane, the cmd wake and no
    # [TOPO] / RWM_IO_RT arm are required on both.
    for side in ("cli", "srv"):
        if row[f"{side}_owners"] and row[f"{side}_ack_tok"] != row[f"{side}_owners"]:
            p.append(f"no-sender-lane-token-{side}")
    if not row["cli_ack_dg"]:
        p.append("acks-not-routed-to-sender")
    if w is not None and "cmd" not in w:
        p.append("wake-cmd-token-missing")
    if row["topo_n_cli"] or row["topo_n_srv"]:
        p.append(f"topo-present={row['topo_n_cli']}/{row['topo_n_srv']}")
    if row["iort_gate_cli"] is not None or row["iort_gate_srv"] is not None:
        p.append(f"iort-gate-present={row['iort_gate_cli']}/{row['iort_gate_srv']}")
    # The D9 execution witness, two-sided, on the client's main thread.
    mt = row.get("cli_main_cores")
    if mt is None:
        p.append("d9-main-thread-unread")
    elif arm == "MAIN" and mt >= MAIN_THR_MAX:
        p.append(f"d9-witness-MAIN-main-thread={mt}")
    elif arm == "NOD9" and mt < NOD9_THR_MIN:
        p.append(f"d9-witness-NOD9-main-thread={mt}")
    if row["rtobs_gate_cli"] != "1" or row["rtobs_gate_srv"] != "1":
        p.append(f"rtobs-gate={row['rtobs_gate_cli']}/{row['rtobs_gate_srv']}")
    for side in ("cli", "srv"):
        if not row[f"{side}_n_workers"] or not row[f"{side}_lag_n"]:
            p.append(f"no-thr-lag-{side}")
    row["problems"].extend(p)
    if p and row["status"] == "LIVE":
        row["status"] = "WITNESS-FAIL"
    return row


def bin_check(rows, shas):
    for r in rows:
        want = shas.get(r.get("bin"))
        if want and r.get("bin_sha") and r["bin_sha"] != want and r["status"] == "LIVE":
            r["status"] = "CONTAMINATED"
            r["problems"].append("bin-sha-mismatch")
    return rows


# ── LEDGER ───────────────────────────────────────────────────────────────
_HDR_SHA = re.compile(r"^=== binary (MAIN|NOD9) \S+ sha256 ([0-9a-f]{64})")


def rows_of(paths):
    rows, tokens, shas = [], [], {}
    for path in paths:
        for ln in lc.read(path):
            m = _HDR_SHA.match(ln)
            if m:
                shas[m.group(1).lower()] = m.group(2)
            if ln.startswith("TD9ROW "):
                try:
                    rows.append(json.loads(ln[len("TD9ROW "):]))
                except ValueError:
                    tokens.append("MALFORMED-TD9ROW")
            for t in ABORT_FIRST_FIVE:
                if ln.startswith(t):
                    tokens.append(t)
            if ln.startswith("TRUNCATED-AT-REP-BOUNDARY"):
                tokens.append("TRUNCATED-AT-REP-BOUNDARY")
    return bin_check(rows, shas), tokens, shas


fmt = sp.fmt


def sel(live, cell, arm, seed=None):
    return [r for r in live if r["cell"] == cell and r["arm"] == arm
            and (seed is None or r["seed"] == seed)]


def vals(rows, key):
    return [r[key] for r in rows if r["dnf"] is False and r.get(key) is not None]


def per_byte(rows, key):
    out = []
    for r in rows:
        if r["dnf"] is False and r.get(key) is not None:
            b = sp.CELL_SPEC[r["cell"]][3] if len(sp.CELL_SPEC[r["cell"]]) > 3 else None
            out.append(r[key] / (b / 1e9) if b else r[key])
    return out


def reading(xv, rv, rel, higher_is_better, name="ARM"):
    """The median clause with the min-max rule (arm against MAIN)."""
    if not rv:
        return "VACUOUS", "reference has no value"
    if not xv:
        return "WORSE", f"{name} has no value"
    xm, rm = lc.med(xv), lc.med(rv)
    lo, hi = rm * (1 - rel), rm * (1 + rel)
    det = (f"{name}={xm:.4g} [{min(xv):.4g}-{max(xv):.4g}] MAIN={rm:.4g} [{min(rv):.4g}-{max(rv):.4g}] "
           f"({100 * (xm / rm - 1):+.1f}%, rel={rel:.3f})")
    if higher_is_better:
        worse_med, better_med = xm < lo, xm > hi
        disjoint_worse, disjoint_better = max(xv) < min(rv), min(xv) > max(rv)
    else:
        worse_med, better_med = xm > hi, xm < lo
        disjoint_worse, disjoint_better = min(xv) > max(rv), max(xv) < min(rv)
    if worse_med:
        return ("WORSE" if disjoint_worse else "TREND-WORSE"), det
    if better_med:
        return ("BETTER" if disjoint_better else "TREND-BETTER"), det
    return "WITHIN", det


def spread_rel(rv):
    if not rv:
        return REL_FLOOR
    m = lc.med(rv)
    return max(REL_FLOOR, 0.5 * (max(rv) - min(rv)) / m if m else 0.0)


def witness_fails(rows, arm, cell):
    return sum(1 for r in rows if r["arm"] == arm and r["cell"] == cell
               and r["status"] in ("CONTAMINATED", "WITNESS-FAIL"))


def blockers(rows, live, cell, arm, aborted):
    why = []
    for a in ("MAIN", arm):
        nl = len(sel(live, cell, a))
        if nl < MIN_LIVE:
            why.append(f"{a} live={nl}<{MIN_LIVE}")
        wf = witness_fails(rows, a, cell)
        if wf >= WITNESS_FAIL_LIMIT:
            why.append(f"{a} witness-failed={wf}")
    if aborted:
        why.append("abort")
    return why


READINGS = {}   # (cell, arm) -> {clause: reading}, read by decision()


def cell_verdict(rows, live, cell, arm, aborted, out):
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
    READINGS[(cell, arm)] = {k: v[0] for k, v in c.items()}
    dr = (sum(1 for r in rs if r["dnf"]) / len(rs)) if rs else None
    dx = (sum(1 for r in xs if r["dnf"]) / len(xs)) if xs else None
    dnf_bad = dr is not None and dx is not None and dx - dr > DNF_THRESHOLD
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


def mmm(v, nd=2):
    m = lc.med(v)
    return "-" if m is None else f"{m:.{nd}f} [{min(v):.{nd}f}-{max(v):.{nd}f}] n={len(v)}"


def reported(live, cell, out):
    for a in ARMS:
        rs = sel(live, cell, a)
        legs = range(sp.N_LEGS[cell])
        out(f"  REPORT {cell} {a} acks/dgram={mmm(vals(rs, 'acks_per_dgram'), 3)} "
            f"busy={mmm(vals(rs, 'busy_med'), 1)} "
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


def readouts(live, out):
    """Reported, not decisive: the D9 execution readout (the client main
    thread's cores per arm) and the c8 client [LAG] p99 re-check at n = 12
    against the section-14 readings (cross-session, report-only; the
    within-session read is MAIN-vs-NOD9 only, the lag clause of the cell)."""
    res = {}
    for a in ARMS:
        for cell in CELLS:
            rs = sel(live, cell, a)
            res[f"main_thr_{cell}_{a}"] = mmm(vals(rs, "cli_main_cores"), 3)
            out(f"READOUT d9 client main-thread cores {cell} {a}: {res[f'main_thr_{cell}_{a}']}")
    for a in ARMS:
        v = vals(sel(live, "c8-100", a), "cli_lag_p99_us")
        res[f"c8lag_{a}"] = mmm(v, 0)
        out(f"C8LAG {a} client [LAG] p99 us, c8-100: {res[f'c8lag_{a}']}"
            + "".join(f"; {nm} reading {ref}" + (f" ({100 * (lc.med(v) / ref - 1):+.0f}% median vs it)" if v else "")
                      for nm, ref in LAG_C8_REF))
    out("C8LAG cross-session comparison is report-only; the within-session read is MAIN vs NOD9 (the lag clause)")
    return res


def arm_verdict(verdicts):
    fail = [c for c in CELLS if verdicts[c] == "FAIL"]
    unsc = [c for c in CELLS if verdicts[c] == "UNSCOREABLE"]
    if fail:
        return "FAIL-AT-" + ",".join(fail)
    if unsc:
        return "UNSCOREABLE-AT-" + ",".join(unsc)
    return "PASS-EVERYWHERE"


def better_clauses(cellv):
    """`cell:clause` for every goodput / CPUCLI-per-GB / CPUSRV-per-GB clause
    where NOD9 is BETTER (median beyond the section-5 MDE AND ranges disjoint;
    TREND-BETTER does not count)."""
    out = []
    for cell in CELLS:
        rd = READINGS.get((cell, "NOD9"), {})
        for k in ("goodput", "cpu_cli/GB", "cpu_srv/GB"):
            if rd.get(k) == "BETTER":
                out.append(f"{cell}:{k}")
    return out


def outcome(arm_v, aborted, better):
    """The one pre-registered decision, in precedence order."""
    if aborted:
        return "UNSCOREABLE (abort: " + ",".join(aborted) + ")"
    v = arm_v["NOD9"]
    if v.startswith("UNSCOREABLE"):
        return "UNSCOREABLE-AT (NOD9 " + v + "): D9 stays on main"
    if v.startswith("FAIL"):
        if better:
            return ("KEEP-D9 (MIXED: NOD9 " + v + " yet BETTER at " + ",".join(better)
                    + "; outside the revert rule, which needs PASS-EVERYWHERE)")
        return "KEEP-D9 (MAIN better: NOD9 " + v + ")"
    if better:
        return "REVERT-D9 (NOD9 PASS-EVERYWHERE and BETTER at " + ",".join(better) + ")"
    return "KEEP-D9 (SAME: NOD9 PASS-EVERYWHERE, nothing BETTER beyond the MDE)"


def score(paths, out=print):
    rows, tokens, shas = rows_of(paths)
    live = [r for r in rows if r["status"] == "LIVE"]
    aborted = [t for t in ABORT_FIRST_FIVE if t in tokens]
    out(f"BINARIES main={shas.get('main', '-')} nod9={shas.get('nod9', '-')}")
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
            f"wake={r.get('wake_ack')}/{r.get('wake_cmd')}/{r.get('wake_timer_acked')} "
            f"mainthr={fmt(r.get('cli_main_cores'), 3)}")
    out("")
    out("PER-SEED MEDIANS (goodput, cpu_cli)")
    for cell in CELLS:
        for a in ARMS:
            out(f"  {cell} {a} " + " ".join(
                f"s{s}: gp={fmt(lc.med(vals(sel(live, cell, a, s), 'mbps')), 1)} "
                f"cli={fmt(lc.med(vals(sel(live, cell, a, s), 'cpu_cli')), 2)}" for s in SEEDS))
    out("")
    cellv = {a: {} for a in SCORED}
    for cell in CELLS:
        out(f"[{cell}]")
        for a in SCORED:
            cellv[a][cell] = cell_verdict(rows, live, cell, a, aborted, out)
        reported(live, cell, out)
    out("")
    readouts(live, out)
    arm_v = {a: arm_verdict(cellv[a]) for a in SCORED}
    for a in SCORED:
        out(f"ARM {a} " + " ".join(f"{c}:{cellv[a][c]}" for c in CELLS) + f" => {arm_v[a]}")
    v = outcome(arm_v, aborted, better_clauses(cellv))
    out("VERDICT " + v)
    return v


def check(path):
    rows, tokens, _ = rows_of([path])
    if not rows or "MALFORMED-TD9ROW" in tokens:
        print(f"CHECK-FAIL {path} rows={len(rows)} tokens={tokens}")
        return 1
    print(f"CHECK-OK {path} rows={len(rows)} live={sum(1 for r in rows if r['status'] == 'LIVE')}")
    return 0


def smoke(path):
    """rc 0 iff every smoke row is LIVE with every gauge the scorer reads and
    both arms appear (the Q2-era and D9 witnesses are part of LIVE)."""
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
        print("TD9ROW " + json.dumps(row, sort_keys=True))
        return 0
    if cmd == "check":
        return check(a[0])
    if cmd == "smoke":
        return smoke(a[0])
    if cmd == "cost":
        return sp.cost(a[0])
    if cmd == "score":
        score(a)
        return 0
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
