#!/usr/bin/env python3
"""Offline exercise of `p0_parse.py` (docs/status.md §9) on SYNTHETIC lines.

    python3 test_p0_parse.py        # exit 0 iff every check passes

NO ENGINE, NO VM. `[THR]`/`[LAG]` lines are transcribed from src/runtime_obs.rs's
format strings (pinned there by `the_thr_lines_carry_deltas_and_cores_over_the_window`),
`[RDIAG]` from net/receiver.rs, the rest from test_stage3_parse.py's
fixtures. Verdicts are checked on hand-built ledgers whose outcome is known
in advance (absolute checks of every constant, not ordinal ones).
"""
import io
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import p0_parse as pp  # noqa: E402

CHECKS = 0
FAILS = []


def check(cond, msg):
    global CHECKS
    CHECKS += 1
    if not cond:
        FAILS.append(msg)
        print("FAIL", msg)


TS = "\x1b[2m2026-10-05T10:00:00.000000Z\x1b[0m \x1b[32m INFO\x1b[0m raptorpath::net: "


def gates(rdiag=1, emb=1):
    return TS + f"[GATES] RWM_UNIFIED=1 RWM_POOL_ANCHOR=0 RWM_EMIT_BATCH={emb} RWM_RDIAG={rdiag} RWM_GEN=384"


WINL = TS + "reliable window mode (retain until acked): auto-selecting RLC windowed backend"
CADL = TS + "estimator heavy-math cadence ACTIVE (RWM_EST_CADENCE: BOCD update at 10 ms)"
PIPE = TS + "[PIPE] pipeline=window backend=Rlc hint=bulk"


def diag(busy):
    return (f"[DIAG] t=1.0s win=1/2 paused=0% good=1.0Mbit cum=1000/0/900 cwnd=1 "
            f"wait[tun=0% paused=0% n=10 us=100 busy={busy}% busy_us=5] p0:infl=0/sinfl=0/bdp1(cap0) "
            f"sout=1 est=Y pl=0.0000 qlp=1/2 | ANCHOR sent=0 gen=0 fill=1 plu=0.0010 plc=0.0010 rce=1")


def rd(busy, msgs=50000, q=3):
    return f"[RDIAG] busy={busy}% msgs={msgs}/s q_avg={q} q_max={q * 4} cap=4096"


def thr(side, win, top, main=0.15, wall=6.0, lag_p99=1500):
    """One transfer window as runtime_obs renders it: 2 workers, 3 threads."""
    return [
        f"[THR] rt phase=xfer side={side} {win} worker=0 busy_s={top * wall:.3f} busy_frac={top:.3f} park=100 unpark=90 wall_s={wall:.3f}",
        f"[THR] rt phase=xfer side={side} {win} worker=1 busy_s=0.600 busy_frac={0.6 / wall:.3f} park=300 unpark=280 wall_s={wall:.3f}",
        f"[THR] os phase=xfer side={side} {win} tid=100 comm=raptorpath cpu_s={main * wall:.3f} cores={main:.3f} wall_s={wall:.3f}",
        f"[THR] os phase=xfer side={side} {win} tid=101 comm=rp-w-0 cpu_s={top * wall:.3f} cores={top:.3f} wall_s={wall:.3f}",
        f"[THR] os phase=xfer side={side} {win} tid=102 comm=rp-w-1 cpu_s=0.600 cores={0.6 / wall:.3f} wall_s={wall:.3f}",
        f"[THR] sum phase=xfer side={side} {win} workers=2 busy_s=1.0 threads=3 cpu_s=1.0 cores={top + main + 0.1:.3f} wall_s={wall:.3f}",
        f"[LAG] phase=xfer side={side} {win} tick_ms=10 n=600 p50_us=700 p99_us={lag_p99} max_us=9000 dropped=0",
    ]


def truth(i):
    return (f"    [TRUTH] leg={i} dev=cli{i} egress_dgrams=300000 egress_skbs=33000 gso=9.00 "
            f"netem_sent_dgrams=300000 netem_dropped_skbs=0 backlog=0 lost=0 loss=0.000000 "
            f"rcvbuf_drops=0 rcvbuf_scope=netns")


def logs(arm, cell, mbps=510.0, cpu=5.7, srv_busy=70, srv_top=0.80, thr_on=None, rdiag=1, os_lines=True):
    legs = 2 if cell == "c1d-400" else 1
    mode = "dual" if legs == 2 else "single"
    secs = 400e6 * 8 / mbps / 1e6
    drv = [f"--- RWM-C perf pipeline=window mode={mode} hint=bulk A=c1 B=c1 ooo=0 extra='' T=default (400000000 x 1) start=10:00:00",
           f"    CPU: CPUSRV=8.20s CPUCLI={cpu:.2f}s (srv=decoder cli=sender; whole-invocation incl warmup)"] \
        + [truth(i) for i in range(legs)]
    base = [gates(rdiag), PIPE, WINL, CADL]
    cli = base + [diag(30), diag(40), rd(20)]
    srv = base + [rd(srv_busy - 5), rd(srv_busy), rd(srv_busy + 5), rd(5, msgs=200)]
    thr_on = (arm == "P0") if thr_on is None else thr_on
    if thr_on:
        tc, ts = thr("client", "run=1", 0.45), thr("server", "obj=1", srv_top)
        if not os_lines:
            tc = [x for x in tc if "[THR] os " not in x]
        cli += tc
        srv += thr("server", "obj=0", 0.9, wall=0.002) + ts + [
            "[THR] rt phase=run side=server worker=0 busy_s=5 busy_frac=0.3 park=1 unpark=1 wall_s=20.0",
            "[THR] sum phase=run side=server workers=2 busy_s=6 threads=3 cpu_s=8 cores=0.4 wall_s=20.0",
            "[LAG] phase=run side=server tick_ms=10 n=2000 p50_us=600 p99_us=1500 max_us=9000 dropped=0 final=1"]
    cli += [json.dumps({"proto": "rp-native", "hint": "bulk", "bytes": 400000000, "run": 1,
                        "seconds": round(secs, 4), "mbps": mbps}),
            json.dumps({"summary": True, "dnf": 0})]
    return drv, cli, srv


# ── row: witnesses and gauges ────────────────────────────────────────────
d, c, s = logs("P0", "c1s-400")
r = pp.make_row("c1s-400", "P0", 42, 1, 0, 20, d, c, s, 0, "abc")
check(r["status"] == "LIVE", f"P0 live: {r['status']} {r['problems']}")
check(r["rdiag_srv_busy_med"] == 70.0 and r["rdiag_srv_busy_max"] == 75.0 and r["rdiag_srv_n"] == 3,
      f"server rdiag over in-transfer lines only (the msgs=200 line excluded): {r['rdiag_srv_busy_med']} {r['rdiag_srv_n']}")
check(r["rdiag_cli_busy_med"] == 20.0, "client rdiag read")
check(r["srv_thr_r1"] == 0.8 and r["srv_top_comm"] == "rp-w-0" and r["srv_main_cores"] == 0.15,
      f"server window obj=1 (not the warm-up obj=0): {r['srv_thr_r1']} {r['srv_top_comm']}")
check(r["srv_wall_s"] == 6.0 and r["cli_wall_s"] == 6.0, "window walls")
check(r["srv_run_lines"] is True, "server phase=run lines seen")
check(r["srv_lag_p99_us"] == 1500.0 and r["cli_lag_n"] == 600, "lag columns")
check(abs(r["cpu_us_per_dgram"] - 1e6 * 5.7 / 300000) < 1e-9, "cpu per egress datagram")
check(("rp-w-0", 0.8) in [tuple(x) for x in r["srv_threads"]], "per-rep thread table kept")
d, c, s = logs("MAIN", "c1s-400")
r = pp.make_row("c1s-400", "MAIN", 42, 1, 0, 20, d, c, s)
check(r["status"] == "LIVE" and r["srv_thr_r1"] is None, f"MAIN live without [THR]: {r['problems']}")
d, c, s = logs("MAIN", "c1s-400", thr_on=True)
check(pp.make_row("c1s-400", "MAIN", 42, 1, 0, 20, d, c, s)["status"] == "WITNESS-FAIL",
      "MAIN carrying [THR] lines fails (two-sided: wrong binary)")
d, c, s = logs("P0", "c1s-400", thr_on=False)
check(pp.make_row("c1s-400", "P0", 42, 1, 0, 20, d, c, s)["status"] == "WITNESS-FAIL",
      "P0 without [THR] fails (the instrument did not execute)")
d, c, s = logs("P0", "c1s-400", os_lines=False)
r = pp.make_row("c1s-400", "P0", 42, 1, 0, 20, d, c, s)
check(r["status"] == "WITNESS-FAIL" and "no-thr-os-rpw-cli" in r["problems"],
      "P0 without per-thread os lines fails (thread_name_fn witness)")
d, c, s = logs("P0", "c1d-400", rdiag=0)
r = pp.make_row("c1d-400", "P0", 42, 1, 0, 20, d, c, s)
check(r["status"] == "WITNESS-FAIL" and any(p.startswith("rdiag-gate") for p in r["problems"]),
      "RWM_RDIAG=0 on a row fails the gate witness")
d, c, s = logs("P0", "c1d-400")
s = [x for x in s if "[RDIAG]" not in x]
r = pp.make_row("c1d-400", "P0", 42, 1, 0, 20, d, c, s)
check(r["status"] == "WITNESS-FAIL" and "no-in-transfer-rdiag-srv" in r["problems"],
      "no in-transfer [RDIAG] on the server fails")
check(pp.make_row("c1d-400", "P0", 42, 1, 3, 20, *logs("P0", "c1d-400"))["status"] == "VOID-RC", "rc void")


# ── ledgers ──────────────────────────────────────────────────────────────
def ledger(spec, extra=""):
    """spec: list of (cell, arm, seed, rep, kwargs)."""
    out = []
    for cell, arm, seed, rep, kw in spec:
        d, c, s = logs(arm, cell, **kw)
        out.append("P0ROW " + json.dumps(pp.make_row(cell, arm, seed, rep, 0, 20, d, c, s), sort_keys=True))
    f = tempfile.NamedTemporaryFile("w", suffix=".log", delete=False, encoding="utf-8")
    f.write(extra + "\n".join(out) + "\n")
    f.close()
    return f.name


def full(p0kw=None, mainkw=None, n=3):
    spec = []
    seeds = ["42", "7", "42"][:n]
    for i, seed in enumerate(seeds):
        for cell in pp.CELLS:
            spec.append((cell, "P0", seed, i + 1, dict(p0kw or {})))
            spec.append((cell, "MAIN", seed, i + 1, dict(mainkw or {})))
    return spec


def run_score(path):
    buf = io.StringIO()
    pp.score(path, out=lambda s: buf.write(s + "\n"))
    return buf.getvalue()


txt = run_score(ledger(full()))
check("NO-CHANGE VERDICT: NO-CHANGE-HELD" in txt, "identical arms: NO-CHANGE-HELD")
check("D3 VERDICT (c1s-400): D3-REFUTED-WITH-RECORD" in txt, "busy 70 %, thread 0.80: D3 refuted")
check("c1s-400 MAIN identity vs §10 NEW 510.4 band (467.1, 568.5): IN-BAND" in txt, "control identity in band")
txt = run_score(ledger(full(p0kw={"srv_busy": 92})))
check("D3-CONFIRMED" in txt and "STOP RULE FIRED" in txt, "receiver busy 92 % confirms D3 and fires the stop rule")
txt = run_score(ledger(full(p0kw={"srv_top": 0.95})))
check("D3-CONFIRMED" in txt and "hottest server thread 0.950" in txt, "a 0.95-core server thread confirms D3")
txt = run_score(ledger(full(p0kw={"srv_top": 0.899, "srv_busy": 89.9})))
check("D3-REFUTED-WITH-RECORD" in txt, "just below both thresholds: refuted (thresholds are >=)")
txt = run_score(ledger(full(p0kw={"srv_top": 0.90})))
check("D3-CONFIRMED" in txt, "exactly 0.90 core confirms (>=)")
txt = run_score(ledger(full(n=2)))
check("D3 VERDICT (c1s-400): UNSCOREABLE" in txt and "NO-CHANGE VERDICT: UNSCOREABLE" in txt,
      "n = 2 per arm: both verdicts UNSCOREABLE")
# CPU +10 % at c1s with disjoint ranges: the no-change claim is refuted.
txt = run_score(ledger(full(p0kw={"cpu": 6.27})))
check("c1s-400 NO-CHANGE cpu: P0 vs MAIN 10.0 % (MDE 2.4 %) -> MOVED" in txt
      and "NO-CHANGE VERDICT: REFUTED-WITH-RECORD" in txt, "CPU moved with disjoint ranges: REFUTED")
# Overlapping ranges outside the MDE: underpowered, not refuted.
spec = full()
# full() orders (c1s P0, c1s MAIN, c1d P0, c1d MAIN) per block: c1s P0 rows
# are 0, 4, 8 and c1s MAIN rows 1, 5, 9.
spec[0] = ("c1s-400", "P0", "42", 1, {"cpu": 6.4})
spec[8] = ("c1s-400", "P0", "42", 3, {"cpu": 6.4})
spec[5] = ("c1s-400", "MAIN", "7", 2, {"cpu": 6.6})
txt = run_score(ledger(spec))
check("-> UNDERPOWERED" in txt and "NO-CHANGE VERDICT: GUARD-UNDERPOWERED" in txt,
      "outside the MDE with overlapping ranges: GUARD-UNDERPOWERED")
txt = run_score(ledger(full(mainkw={"mbps": 400.0}, p0kw={"mbps": 400.0})))
check("c1s-400 MAIN identity vs §10 NEW 510.4 band (467.1, 568.5): CONTROL-MOVED" in txt,
      "control out of the §10 band: CONTROL-MOVED (reported)")
txt = run_score(ledger(full(), extra="ABORT-SHA x\n"))
check("ABORT TOKENS: ['ABORT-SHA']" in txt, "abort tokens surface")
# smoke / check / cost
path = ledger([("c1s-400", "P0", "42", 1, {}), ("c1s-400", "MAIN", "42", 1, {}), ("c1d-400", "P0", "42", 1, {})],
              extra="RUNTIME c1s-400-P0 s42 rep=1 attempt=1 21s rc=0\nRUNTIME c1s-400-MAIN s42 rep=1 attempt=1 19s rc=0\n")
sys.stdout = io.StringIO()
rc_smoke, rc_check, rc_cost = pp.smoke(path), pp.check(path), pp.cost(path)
cap = sys.stdout.getvalue()
sys.stdout = sys.__stdout__
check(rc_smoke == 0 and "SMOKE-PASS rows=3" in cap, f"smoke passes: {cap}")
check(rc_check == 0 and "COST total=40 n=2" in cap, "check / cost")
path = ledger([("c1s-400", "P0", "42", 1, {"thr_on": False}), ("c1s-400", "MAIN", "42", 1, {})])
sys.stdout = io.StringIO()
rc_smoke = pp.smoke(path)
sys.stdout = sys.__stdout__
check(rc_smoke == 1, "a P0 smoke row without [THR] aborts the smoke")

print("test_p0_parse: %d checks, %d failed" % (CHECKS, len(FAILS)))
sys.exit(1 if FAILS else 0)
