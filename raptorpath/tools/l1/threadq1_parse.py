#!/usr/bin/env python3
"""Parser and scorer for THE V-Q1 BATTERY (docs/status.md, "13. Threading
Q1 — pre-registration"): the per-path I/O owner in its two placements
(IOS = `RWM_IO_RT=shared`, IOO = `RWM_IO_RT=own`; one binary, Q1) and the
D9 commit alone (D9), each against `main` 69fd846 (MAIN), one session,
interleaved. Derived from the V-P2a parser (archive/thread-p2a).

    threadq1_parse.py row   <cell> <arm> <seed> <rep> <rc> <wall_s> <drv_out> <cli_log> <srv_log> [cotenant] [bin_sha]
    threadq1_parse.py check <ledger>
    threadq1_parse.py smoke <ledger>
    threadq1_parse.py cost  <ledger>
    threadq1_parse.py score <ledger> [<ackd ledger>]

`row` reuses `stage3_parse.make_row` (statuses, goodput/completion/CPU,
`[TRUTH]`, `plc`, `busy`, the two-sided `[PIPE]`/`[GATES]`/RLC/cadence/
`RWM_POOL_ANCHOR=0` witnesses) and adds:

  emit     `[GATES] RWM_EMIT_BATCH=1` both endpoints, the "emission batching
           ACTIVE" echo on the client (every arm);
  q1       the execution witness, two-sided. IOS/IOO rows: `[GATES]
           RWM_IO_RT=<shared|own>` on BOTH endpoints, one `[TOPO]
           io_rt=<arm>` line per leg on BOTH endpoints naming `rt=main`
           (shared) or `rt=rp-io-<k>` (own), and the `[IOWN]` window lines
           of the measured object on both ends. MAIN/D9 rows: NO `[TOPO]
           io_rt` line and no `RWM_IO_RT` token on either endpoint;
  thr/lag  the main runtime's `[THR]`/`[LAG]` window of the measured object
           on both ends (client `run=1`, server `obj=1`; every arm);
  io       (IOS/IOO) per side: the owners' lock-wait gauge (`asleep_frac`,
           max and sum over owners), the routing probe (`drv_on`/`drv_off`
           summed), datagrams per batch on the owner hops, `rx_capped`,
           `send_err`, `too_large_staged`; the `[LAG] io` p99 (max over I/O
           runtimes) and the `[THR] io` parks per second (summed);
  wake/rtp/ctld/ackdiag as V-P2a.

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

# ── THE DESIGN ───────────────────────────────────────────────────────────
ARMS = {
    "MAIN": ("window", "Rlc", "bulk", "ACTIVE"),
    "D9": ("window", "Rlc", "bulk", "ACTIVE"),
    "IOS": ("window", "Rlc", "bulk", "ACTIVE"),
    "IOO": ("window", "Rlc", "bulk", "ACTIVE"),
}
sp.ARM_SPEC.update(ARMS)
ARM_BIN = {"MAIN": "main", "D9": "d9", "IOS": "q1", "IOO": "q1"}
IO_RT = {"IOS": "shared", "IOO": "own"}
SCORED = ["IOS", "IOO"]          # the placement arms: their verdicts decide
ATTRIBUTION = ["D9"]             # scored the same way, reported, never decides
CELLS = ["c1s-400", "c1d-400", "c2-100", "c8-100"]
SEEDS = ["42", "7"]
EMB_ECHO = "emission batching ACTIVE"

# §5's committed relative MDE (status.md §5 (a)): cell -> {gp, cpu}. `cpu`
# is the CPUCLI MDE, applied to CPUSRV as a declared transfer (as §11, §12).
S3_REL = {
    "c1s-400": {"gp": 0.049, "cpu": 0.024},
    "c1d-400": {"gp": 0.056, "cpu": 0.065},
    "c2-100": {"gp": 0.014, "cpu": 0.063},
    "c8-100": {"gp": 0.040, "cpu": 0.116},
}
REL_FLOOR = 0.05          # RTprop floor / [LAG] p99: max(5 %, MAIN half-range / median) (§11, §12)
DNF_THRESHOLD = 0.20      # §5
MIN_LIVE = 3              # live rows per (cell, arm)
WITNESS_FAIL_LIMIT = 2    # failed rows per (arm, cell) that void the cell
FEED_BAND = 1.3           # arm's plc/truth within [1/1.3, 1.3] x MAIN's, per leg (§11, §12)
LOCKWAIT_ZERO = 0.01      # "lock wait ~ 0": client max-over-owners asleep_frac median <= 1 % of wall
BAR_C1D_MBPS = 501.0      # FDT's c1d goodput (status §10), reported bar
ABORT_FIRST_FIVE = sp.ABORT_FIRST_FIVE

_CTLD = re.compile(r"\bp(\d+) tx=(\d+) rx=(\d+)")
_ACKD = re.compile(r"\[ACKDIAG\] p(\d+) .*?gap_us\[p50=(\d+) p90=(\d+) p99=(\d+) n=(\d+)\]")


def ctld_ratio(srv):
    """Σ tx / Σ rx over paths on the server's LAST `[CTLD]` line, or None."""
    for ln in reversed(srv or []):
        if "[CTLD]" not in ln:
            continue
        tx = rx = 0
        for m in _CTLD.finditer(ln):
            tx += int(m.group(2))
            rx += int(m.group(3))
        return (tx / rx) if rx else None
    return None


def ackdiag_gaps(cli):
    out = {}
    for ln in cli or []:
        m = _ACKD.search(ln)
        if m:
            out[int(m.group(1))] = {"p50": int(m.group(2)), "p90": int(m.group(3)),
                                    "p99": int(m.group(4)), "n": int(m.group(5))}
    return out


def topo_lines(lines):
    return [ln for ln in lines or [] if "[TOPO] io_rt=" in ln]


def _in_window(ln, phase, window):
    if lc.field(ln, "phase") != phase:
        return False
    k, v = window.split("=")
    return lc.field(ln, k) == v


def io_columns(lines, prefix, phase="xfer", window="run=1"):
    """Flat columns from one window's `[IOWN]`, `[LAG] io`, `[THR] io` lines
    (all None when absent, so the row shape never depends on the capture)."""
    def k(s):
        return "%s_%s" % (prefix, s)

    own = [ln for ln in lines or [] if "[IOWN] " in ln and _in_window(ln, phase, window)]
    lag = [ln for ln in lines or [] if "[LAG] io " in ln and _in_window(ln, phase, window)]
    thr = [ln for ln in lines or [] if "[THR] io " in ln and _in_window(ln, phase, window)]
    cols = {k("owners"): len(own) if own else None}

    def s(key):
        v = [lc.inum(lc.field(ln, key)) for ln in own]
        v = [x for x in v if x is not None]
        return sum(v) if v else None

    fr = [lc.fnum(lc.field(ln, "asleep_frac")) for ln in own]
    fr = [x for x in fr if x is not None]
    cols[k("asleep_max")] = max(fr) if fr else None
    cols[k("asleep_sum")] = sum(fr) if fr else None
    for key in ("drv_on", "drv_off", "tx_dg", "tx_batches", "ctrl_dg", "ctrl_batches",
                "rx_dg", "rx_batches", "rx_capped", "send_err", "too_large_staged",
                "orphaned", "polls", "drains"):
        cols[k(key)] = s(key)
    on, off = cols[k("drv_on")], cols[k("drv_off")]
    cols[k("drv_off_frac")] = (off / (on + off)) if (on is not None and off is not None and on + off) else None
    tb, td = cols[k("tx_batches")], cols[k("tx_dg")]
    cols[k("tx_per_batch")] = (td / tb) if (tb and td is not None) else None
    rb, rd = cols[k("rx_batches")], cols[k("rx_dg")]
    cols[k("rx_per_batch")] = (rd / rb) if (rb and rd is not None) else None
    p99 = [lc.fnum(lc.field(ln, "p99_us")) for ln in lag]
    p99 = [x for x in p99 if x is not None]
    cols[k("iolag_p99_us")] = max(p99) if p99 else None
    parks, wall = [], None
    for ln in thr:
        p = lc.inum(lc.field(ln, "park"))
        if p is not None:
            parks.append(p)
        wall = lc.fnum(lc.field(ln, "wall_s")) or wall
    cols[k("io_park_per_s")] = (sum(parks) / wall) if (parks and wall) else None
    cols[k("io_runtimes")] = len(thr) if thr else None
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
    row["topo_rt_cli"] = sorted({lc.field(ln, "rt") for ln in tc})
    row["topo_rt_srv"] = sorted({lc.field(ln, "rt") for ln in ts})
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
    if arm in IO_RT:
        want = IO_RT[arm]
        if row["iort_gate_cli"] != want or row["iort_gate_srv"] != want:
            p.append(f"iort-gate={row['iort_gate_cli']}/{row['iort_gate_srv']}")
        if row["topo_n_cli"] != legs or row["topo_n_srv"] != legs:
            p.append(f"topo-count={row['topo_n_cli']}/{row['topo_n_srv']}!={legs}")
        for side in ("cli", "srv"):
            rts = row[f"topo_rt_{side}"]
            ok = rts == ["main"] if want == "shared" else (rts and all((r or "").startswith("rp-io-") for r in rts))
            if not ok:
                p.append(f"topo-rt-{side}={','.join(str(r) for r in rts)}")
            if not row[f"{side}_owners"]:
                p.append(f"no-iown-{side}")
            if any(ln for ln in (cli if side == "cli" else srv) if f"[TOPO] io_rt={'own' if want == 'shared' else 'shared'}" in ln):
                p.append(f"topo-other-arm-{side}")
        if row["rtobs_gate_cli"] != "1" or row["rtobs_gate_srv"] != "1":
            p.append(f"rtobs-gate={row['rtobs_gate_cli']}/{row['rtobs_gate_srv']}")
    else:
        if row["topo_n_cli"] or row["topo_n_srv"]:
            p.append(f"topo-on-{arm}={row['topo_n_cli']}/{row['topo_n_srv']}")
        if row["iort_gate_cli"] is not None or row["iort_gate_srv"] is not None:
            p.append(f"iort-gate-on-{arm}={row['iort_gate_cli']}/{row['iort_gate_srv']}")
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
_HDR_SHA = re.compile(r"^=== binary (MAIN|D9|Q1) \S+ sha256 ([0-9a-f]{64})")


def rows_of(paths):
    rows, tokens, shas = [], [], {}
    for path in paths:
        for ln in lc.read(path):
            m = _HDR_SHA.match(ln)
            if m:
                shas[m.group(1).lower()] = m.group(2)
            if ln.startswith("TQ1ROW "):
                try:
                    rows.append(json.loads(ln[len("TQ1ROW "):]))
                except ValueError:
                    tokens.append("MALFORMED-TQ1ROW")
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
        for side in ("cli", "srv"):
            out(f"  REPORT {cell} {a} {side} cores={mmm(vals(rs, side + '_cores'), 3)} "
                f"main_thr={mmm(vals(rs, side + '_main_cores'), 3)} "
                f"thr_r1={mmm(vals(rs, side + '_thr_r1'), 3)} "
                f"parks/s main={mmm(vals(rs, side + '_park_per_s'), 0)} io={mmm(vals(rs, side + '_io_park_per_s'), 0)} "
                f"lag p99 main={mmm(vals(rs, side + '_lag_p99_us'), 0)} io={mmm(vals(rs, side + '_iolag_p99_us'), 0)}")
            if a in IO_RT:
                out(f"  REPORT {cell} {a} {side} lockwait asleep_frac max={mmm(vals(rs, side + '_asleep_max'), 4)} "
                    f"sum={mmm(vals(rs, side + '_asleep_sum'), 4)} "
                    f"drv_off_frac={mmm(vals(rs, side + '_drv_off_frac'), 4)} "
                    f"tx/batch={mmm(vals(rs, side + '_tx_per_batch'), 1)} rx/batch={mmm(vals(rs, side + '_rx_per_batch'), 1)} "
                    f"rx_capped={mmm(vals(rs, side + '_rx_capped'), 0)} send_err={mmm(vals(rs, side + '_send_err'), 0)} "
                    f"too_large_staged={mmm(vals(rs, side + '_too_large_staged'), 0)}")
        for r in rs:
            out(f"    THREADS {cell} {a} s{r['seed']} r{r['rep']} cli="
                + ",".join(f"{cm}:{x:.3f}" for cm, x in sorted(r.get('cli_threads') or [], key=lambda t: -(t[1] or 0))[:4])
                + " srv="
                + ",".join(f"{cm}:{x:.3f}" for cm, x in sorted(r.get('srv_threads') or [], key=lambda t: -(t[1] or 0))[:4]))


def mechanism(live, out):
    """The pre-registered mechanism predictions (reported, named; they do not
    decide the outcome): routing — under IOO every live row reads
    drv_off = 0 on both ends; under IOS the median client drv_off_frac > 0.
    Lock wait — IOO's client asleep_max median <= LOCKWAIT_ZERO at every
    cell; IOS's > LOCKWAIT_ZERO at some cell."""
    res = {}
    ioo = [r for r in live if r["arm"] == "IOO"]
    off = [(r.get("cli_drv_off") or 0) + (r.get("srv_drv_off") or 0) for r in ioo]
    on = [(r.get("cli_drv_on") or 0) + (r.get("srv_drv_on") or 0) for r in ioo]
    res["routing_own"] = "MET" if ioo and max(off) == 0 and min(on) > 0 else "MISSED"
    ios_frac = vals([r for r in live if r["arm"] == "IOS"], "cli_drv_off_frac")
    res["routing_shared_fails"] = "MET" if ios_frac and lc.med(ios_frac) > 0 else "MISSED"
    own_lw = {c: lc.med(vals(sel(live, c, "IOO"), "cli_asleep_max")) for c in CELLS}
    sh_lw = {c: lc.med(vals(sel(live, c, "IOS"), "cli_asleep_max")) for c in CELLS}
    res["lockwait_own_zero"] = ("MET" if all(v is not None and v <= LOCKWAIT_ZERO for v in own_lw.values())
                                else "MISSED")
    res["lockwait_shared_nonzero"] = ("MET" if any(v is not None and v > LOCKWAIT_ZERO for v in sh_lw.values())
                                      else "MISSED")
    for k, v in res.items():
        out(f"MECHANISM {k}: {v}")
    out("MECHANISM lockwait medians (client asleep_max) own=" + " ".join(f"{c}:{fmt(own_lw[c], 4)}" for c in CELLS)
        + " shared=" + " ".join(f"{c}:{fmt(sh_lw[c], 4)}" for c in CELLS))
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
    if arm_v["IOS"] == "PASS-EVERYWHERE":
        return "DELIVERED (SHIP-SHARED: shared passes everywhere; the simpler arm ships, own is deleted)"
    if arm_v["IOO"] == "PASS-EVERYWHERE":
        return "DELIVERED (SHIP-OWN: own passes everywhere, shared does not; shared is deleted)"
    if all(v.startswith("FAIL") for v in arm_v.values()):
        return "REFUTED-WITH-RECORD (NEITHER-PLACEMENT-PASSES: IOS " + arm_v["IOS"] + "; IOO " + arm_v["IOO"] + ")"
    return "UNSCOREABLE-AT (IOS " + arm_v["IOS"] + "; IOO " + arm_v["IOO"] + ")"


def score(paths, out=print, ackd=None):
    rows, tokens, shas = rows_of(paths)
    live = [r for r in rows if r["status"] == "LIVE"]
    aborted = [t for t in ABORT_FIRST_FIVE if t in tokens]
    out(f"BINARIES main={shas.get('main', '-')} d9={shas.get('d9', '-')} q1={shas.get('q1', '-')}")
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
    out("PER-REP (rule 4): cell arm seed rep mbps cpu_cli cpu_srv busy rtp_floor feed lag_p99 cli/srv lockwait cli")
    for r in sorted(rows, key=lambda r: (CELLS.index(r["cell"]) if r["cell"] in CELLS else 9,
                                         r["arm"], r["seed"], r["rep"])):
        legs = range(sp.N_LEGS.get(r["cell"], 1))
        out(f"  {r['cell']} {r['arm']} s{r['seed']} r{r['rep']} {r['status']} mbps={fmt(r.get('mbps'), 1)} "
            f"cli={fmt(r.get('cpu_cli'), 2)} srv={fmt(r.get('cpu_srv'), 2)} busy={fmt(r.get('busy_med'), 1)} "
            f"rtp={'/'.join(str(r.get('rtp_floor_p%d' % i, '-')) for i in legs)} "
            f"feed={'/'.join(fmt(r.get('feed_ratio_p%d' % i), 3) for i in legs)} "
            f"lag99={fmt(r.get('cli_lag_p99_us'), 0)}/{fmt(r.get('srv_lag_p99_us'), 0)} "
            f"asleep={fmt(r.get('cli_asleep_max'), 4)} drv_off={fmt(r.get('cli_drv_off'), 0)}")
    out("")
    out("PER-SEED MEDIANS (goodput, cpu_cli)")
    for cell in CELLS:
        for a in ARMS:
            out(f"  {cell} {a} " + " ".join(
                f"s{s}: gp={fmt(lc.med(vals(sel(live, cell, a, s), 'mbps')), 1)} "
                f"cli={fmt(lc.med(vals(sel(live, cell, a, s), 'cpu_cli')), 2)}" for s in SEEDS))
    out("")
    cellv = {a: {} for a in SCORED + ATTRIBUTION}
    for cell in CELLS:
        out(f"[{cell}]")
        for a in SCORED + ATTRIBUTION:
            cellv[a][cell] = cell_verdict(rows, live, cell, a, aborted, out)
        reported(live, cell, out)
    out("")
    mechanism(live, out)
    arm_v = {a: arm_verdict(cellv[a]) for a in SCORED}
    for a in SCORED + ATTRIBUTION:
        out(f"ARM {a} " + " ".join(f"{c}:{cellv[a][c]}" for c in CELLS) + f" => {arm_verdict(cellv[a])}"
            + (" (attribution, reported)" if a in ATTRIBUTION else ""))
    for a in SCORED:
        g = lc.med(vals(sel(live, "c1d-400", a), "mbps"))
        out(f"BAR c1d {a} goodput={fmt(g, 1)} vs FDT {BAR_C1D_MBPS} -> "
            + ("-" if g is None else ("BAR-MET" if g >= BAR_C1D_MBPS else "BAR-MISSED")) + " (reported)")
    v = outcome(arm_v, aborted)
    out("VERDICT " + v)
    if ackd:
        out("")
        ackd_report(ackd, out)
    return v


def check(path):
    rows, tokens, _ = rows_of([path])
    if not rows or "MALFORMED-TQ1ROW" in tokens:
        print(f"CHECK-FAIL {path} rows={len(rows)} tokens={tokens}")
        return 1
    print(f"CHECK-OK {path} rows={len(rows)} live={sum(1 for r in rows if r['status'] == 'LIVE')}")
    return 0


def smoke(path):
    """rc 0 iff every smoke row is LIVE with every gauge the scorer reads and
    all four arms appear (the Q1 witnesses are part of LIVE)."""
    rows, _, _ = rows_of([path])
    bad = []
    for r in rows:
        miss = []
        if r["status"] != "LIVE":
            miss.append(r["status"])
        for k in ("cpu_cli", "cpu_srv", "busy_med", "mbps", "cli_lag_p99_us", "srv_lag_p99_us"):
            if r.get(k) is None:
                miss.append(f"no-{k}")
        if r["arm"] in IO_RT:
            for k in ("cli_drv_on", "srv_drv_on", "cli_owners", "srv_owners"):
                if r.get(k) is None:
                    miss.append(f"no-{k}")
        for i in range(sp.N_LEGS[r["cell"]]):
            for k in ("truth_loss_p%d" % i, "plc_p%d" % i, "rtp_floor_p%d" % i):
                if r.get(k) is None:
                    miss.append(f"no-{k}")
        print(f"SMOKE-ROW {r['cell']} {r['arm']} bin={r['bin']} rc={r['rc']} status={r['status']} "
              f"mbps={r.get('mbps')} cli={r.get('cpu_cli')} srv={r.get('cpu_srv')} "
              f"topo={r.get('topo_n_cli')}/{r.get('topo_n_srv')} lag99={r.get('cli_lag_p99_us')}/"
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
        print("TQ1ROW " + json.dumps(row, sort_keys=True))
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
