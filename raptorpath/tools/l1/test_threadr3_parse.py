#!/usr/bin/env python3
"""Offline exercise of `threadr3_parse.py` (docs/status.md §22 R3 ship battery) on SYNTHETIC
lines and rows.

    python3 test_threadr3_parse.py        # exit 0 iff every check passes

NO ENGINE, NO VM. The `[IOWN]` line is transcribed from
`src/runtime_obs.rs`'s renderer (its unit test
`the_iown_line_carries_deltas_and_the_asleep_gauge` pins the same tokens,
the R1 `ibp_hit=`/`ibp_miss=` and `cmdq_*` ones included); `[RDIAG]` from
`net/receiver.rs`; the `wake[..]` token from `net/diag.rs`'s `wake_line`.
The scorer is checked on hand-built rows whose verdict is known in advance
(absolute checks).
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import threadr3_parse as tp  # noqa: E402

CHECKS = 0
FAILS = []


def check(cond, msg):
    global CHECKS
    CHECKS += 1
    if not cond:
        FAILS.append(msg)
        print("FAIL", msg)


TS = "\x1b[2m2026-10-26T10:00:00.000000Z\x1b[0m \x1b[32m INFO\x1b[0m raptorpath::runtime_obs: "


def iown(path, r3=True, win="run=1", phase="xfer", tx_dg=640, tx_b=10, rx_dg=600, rx_b=30, ack_dg=900,
         ack_b=60, hit=28, miss=2):
    pool = f" ibp_hit={hit} ibp_miss={miss} cmdq_n=0 cmdq_p50=- cmdq_p99=- cmdq_max=-" if r3 else ""
    return (TS + f"[IOWN] phase={phase} side=client {win} path={path} rt=main polls=100 drains=10 "
            f"tx_batches={tx_b} tx_dg={tx_dg} ctrl_batches=1 ctrl_dg=2 rx_batches={rx_b} rx_dg={rx_dg} "
            f"rx_capped=3 ack_batches={ack_b} ack_dg={ack_dg} send_err=0 too_large_staged=0 orphaned=0 "
            f"qcalls=900 q_secs=40 q_wall_us=5000 q_asleep_us=100 asleep_frac=0.0010 drv_on=5 drv_off=5 "
            f"wall_s=2.000 qk_snd=0/0/0/0/0/0/0/0 qk_rd=0/0/0/0/0/0/0/0 qk_pub=0/0/0/0/0/0/0/0 rx_full=0 "
            f"rx_full_us=0 ack_full=0 ack_full_us=0 fl_sends=0 fl_full=0 fl_full_us=0{pool}")


cli = [
    iown(0),
    iown(1, hit=10, miss=1),
    iown(0, win="run=0", hit=99999),          # another window: ignored
    iown(0, win="", phase="run", hit=99999),  # process-end: ignored
]
c = tp.io_columns(cli, "cli", "xfer", "run=1")
check(c["cli_owners"] == 2 and c["cli_ibp_tok"] == 2, f"two owners, both with the pool token ({c})")
check(c["cli_ibp_hit"] == 38 and c["cli_ibp_miss"] == 3, f"pool sums {c['cli_ibp_hit']}/{c['cli_ibp_miss']}")
check(c["cli_ack_dg"] == 1800 and c["cli_ack_tok"] == 2, "the Q2 sender lane is read on both arms")
m = tp.io_columns([iown(0, r3=False)], "cli")
check(m["cli_ibp_tok"] == 0 and m["cli_ibp_hit"] is None, "a MAIN owner line has no pool token")
empty = tp.io_columns([], "cli")
check(set(empty) == set(c) and all(v is None for v in empty.values()),
      "io_columns: the row shape does not depend on the capture")

# ── [RDIAG] ──────────────────────────────────────────────────────────────
rd = [f"[RDIAG] busy={b}% msgs={m}/s q_avg=3.5 q_max=40 cap=4096" for b, m in
      ((50, 100), (92, 30000), (88, 31000), (96, 29000), (10, 5))]
b, bmax, q, n = tp.rdiag(rd)
check(n == 3 and b == 92.0 and bmax == 96.0, f"only in-transfer RDIAG lines count ({b}, {bmax}, {n})")
check(tp.rdiag([])[3] == 0, "no RDIAG lines -> n = 0")

# ── the execution witnesses ──────────────────────────────────────────────
WAKE = TS + "[DIAG] t=1 wake[tun=5 paused=3 pace=0 gen=0 nack=0 defc=0 tail=0 flush=0 ack=40 cmd=1 timer_acked=0]"
GATES = TS + "[GATES] RWM_EMIT_BATCH=1 RWM_RTOBS=1 RWM_RDIAG=1"
ECHO = TS + "emission batching ACTIVE"


def rdiag_line(rec=None, msgs=30000, below=0):
    dec = "" if rec is None else f" dec[rows=0 max_span=0 coeff_kb=0 recovered={rec} seen=3 below_floor={below}]"
    return f"[RDIAG] busy=80% msgs={msgs}/s q_avg=3.5 q_max=40 cap=4096{dec}"


# Offline, the engine-side extractors (the driver summary, [THR]/[LAG]) have
# no capture: stub them so a row's status is decided by the R1/R3 witnesses
# and the gates alone.
tp.sp.make_row = lambda *a, **k: {"status": "LIVE", "problems": []}
tp.lc.thr_columns = lambda lines, side, phase, win: {f"{side}_n_workers": 4, f"{side}_lag_n": 10}


def full(arm, srv_rdiag, ibp=True, srv_hit=28):
    cli = [GATES, ECHO, WAKE, iown(0, r3=ibp)]
    srv = [GATES, iown(0, r3=ibp, win="obj=1", hit=srv_hit)] + srv_rdiag
    return tp.make_row("c1s-400", arm, "42", 1, 0, 10, [], cli, srv)


r = full("R3", [rdiag_line(400), rdiag_line(4496, below=7), rdiag_line(60, msgs=5)])
check(r["status"] == "LIVE" and r["problems"] == [], f"a good R3 row is LIVE {r['status']} {r['problems']}")
check(r["srv_dec_recovered_max"] == 4496 and r["srv_dec_below_floor"] == 7,
      f"R3 columns: max {r['srv_dec_recovered_max']} below_floor(last) {r['srv_dec_below_floor']}")
r = full("MAIN", [rdiag_line(), rdiag_line()])
check(r["status"] == "LIVE" and r["problems"] == [], f"a good MAIN row is LIVE {r['status']} {r['problems']}")
check(r["srv_dec_recovered_max"] is None, "MAIN row has no dec column")
r = full("R3", [rdiag_line(400), rdiag_line(5000)])
check(r["status"] == "WITNESS-FAIL" and any(x.startswith("dec-recovered-over-bound-srv") for x in r["problems"]),
      f"R3 with recovered=5000 -> WITNESS-FAIL {r['status']} {r['problems']}")
r = full("R3", [rdiag_line(400), rdiag_line()])
check(r["status"] == "WITNESS-FAIL" and any(x.startswith("no-dec-token-srv") for x in r["problems"]),
      f"R3 with an in-transfer [RDIAG] line lacking dec[ -> WITNESS-FAIL {r['problems']}")
r = full("R3", [rdiag_line()])
check(r["status"] == "WITNESS-FAIL", "R3 with no dec[ at all -> WITNESS-FAIL")
r = full("MAIN", [rdiag_line(100), rdiag_line()])
check(r["status"] == "WITNESS-FAIL" and "dec-token-on-MAIN-srv" in r["problems"],
      f"MAIN carrying dec[ -> WITNESS-FAIL {r['problems']}")
# R1's pool witness now applies to both arms
r = full("MAIN", [rdiag_line()], ibp=False)
check("no-pool-token-cli" in r["problems"] and "no-pool-token-srv" in r["problems"],
      f"MAIN without the pool token fails (MAIN contains R1) {r['problems']}")
check("pool-token-on-MAIN-cli" not in r["problems"], "the old pool-token-on-MAIN rule is gone")
r = full("R3", [rdiag_line(100)], srv_hit=0)
check("pool-never-hit-srv" in r["problems"], f"R3 whose server pool never hit fails {r['problems']}")
r = full("MAIN", [rdiag_line()], srv_hit=0)
check("pool-never-hit-srv" in r["problems"], f"MAIN whose server pool never hit fails {r['problems']}")
r = tp.make_row("c1s-400", "MAIN", "42", 1, 0, 10, [], [iown(0, ack_dg=0), WAKE], [])
check("acks-not-routed-to-sender" in r["problems"], "Q2's sender lane is a witness on both arms")
n, nd, mx, bl = tp.dec_witness([rdiag_line(10), rdiag_line(30, below=2), rdiag_line(99, msgs=1)])
check((n, nd, mx, bl) == (2, 2, 30, 2), f"dec_witness counts in-transfer lines only {(n, nd, mx, bl)}")
check(tp.DEC_RECOVERED_MAX == 4496, "bound is 2*200 + 4096")

# ── the min-max rule ─────────────────────────────────────────────────────
check(tp.reading([90, 91, 92], [100, 101, 102], 0.05, True)[0] == "WORSE", "disjoint beyond band -> WORSE")
check(tp.reading([90, 91, 103], [100, 101, 102], 0.05, True)[0] == "TREND-WORSE", "overlap -> TREND-WORSE")
check(tp.reading([99, 100, 101], [100, 101, 102], 0.05, True)[0] == "WITHIN", "inside -> WITHIN")
check(tp.reading([120, 121, 122], [100, 101, 102], 0.05, True)[0] == "BETTER", "disjoint above band -> BETTER")
check(tp.reading([], [1], 0.05, True)[0] == "WORSE", "arm without a value -> WORSE")
check(abs(tp.spread_rel([80, 100, 120]) - 0.20) < 1e-12, "tolerance: half-range / median")


def row(arm, mbps, cell="c1s-400", lag_c=1000, feed=1.0, rep=1, seed="42", cpu_cli=5.8, cpu_srv=8.2,
        busy=50.0, rdiag_srv=95.0, q=3.0):
    return {"cell": cell, "arm": arm, "seed": seed, "rep": rep, "status": "LIVE", "dnf": False,
            "mbps": mbps, "cpu_cli": cpu_cli, "cpu_srv": cpu_srv, "rtp_floor_p0": 2400, "rtp_floor_p1": 2400,
            "cli_lag_p99_us": lag_c, "srv_lag_p99_us": 1000, "feed_ratio_p0": feed, "feed_ratio_p1": feed,
            "problems": [], "bin": tp.ARM_BIN[arm], "bin_sha": None, "busy_med": busy,
            "rdiag_srv_busy_med": rdiag_srv, "rdiag_srv_q_med": q}


def cell_rows(cell, arm_mbps, **kw):
    out = []
    for arm, mb in arm_mbps.items():
        out += [row(arm, mb + i, cell=cell, rep=i, **kw) for i in range(6)]
    return out


lines = []
rows = cell_rows("c1s-400", {"MAIN": 500, "R3": 501})
check(tp.cell_verdict(rows, rows, "c1s-400", "R3", [], lines.append) == "PASS", "R3 same as MAIN -> PASS")
rows = cell_rows("c1s-400", {"MAIN": 500, "R3": 400})
check(tp.cell_verdict(rows, rows, "c1s-400", "R3", [], lines.append) == "FAIL", "R3 -20 % disjoint -> FAIL")
few = [r for r in rows if r["arm"] != "R3"] + [r for r in rows if r["arm"] == "R3"][:2]
check(tp.cell_verdict(few, few, "c1s-400", "R3", [], lines.append) == "UNSCOREABLE", "2 live R3 rows -> UNSCOREABLE")
base = cell_rows("c1s-400", {"MAIN": 500})
fed = base + [row("R3", 501 + i, feed=1.5, rep=i) for i in range(6)]
check(tp.cell_verdict(fed, fed, "c1s-400", "R3", [], lines.append) == "FAIL", "feed 1.5 x MAIN -> FAIL")
lag = base + [row("R3", 501 + i, lag_c=5000 + i, rep=i) for i in range(6)]
check(tp.cell_verdict(lag, lag, "c1s-400", "R3", [], lines.append) == "FAIL", "[LAG] p99 x5 disjoint -> FAIL")
cpu = base + [row("R3", 501 + i, cpu_srv=12.0 + i * 0.01, rep=i) for i in range(6)]
check(tp.cell_verdict(cpu, cpu, "c1s-400", "R3", [], lines.append) == "FAIL", "CPUSRV/byte +45 % disjoint -> FAIL")

# ── the outcome, in precedence order ─────────────────────────────────────
check(tp.outcome({"R3": "PASS-EVERYWHERE"}, []).startswith("DELIVERED"), "passes everywhere -> DELIVERED")
check(tp.outcome({"R3": "FAIL-AT-c1d-400"}, []).startswith("REFUTED-WITH-RECORD"), "a FAIL -> REFUTED-WITH-RECORD")
check(tp.outcome({"R3": "UNSCOREABLE-AT-c2-100"}, []).startswith("UNSCOREABLE-AT"), "unscoreable cell -> UNSCOREABLE-AT")
check(tp.outcome({"R3": "PASS-EVERYWHERE"}, ["ABORT-LOCK"]).startswith("UNSCOREABLE (abort"), "abort first")
check(tp.arm_verdict({c: "PASS" for c in tp.CELLS}) == "PASS-EVERYWHERE", "all cells PASS -> PASS-EVERYWHERE")
check(tp.arm_verdict(dict({c: "PASS" for c in tp.CELLS}, **{"c2-100": "FAIL", "c8-100": "UNSCOREABLE"}))
      == "FAIL-AT-c2-100", "a FAIL outranks an UNSCOREABLE")

# ── the named predictions, on a hand-built session ───────────────────────
live, readings = [], {}
for cell, (gm, gr, sm, sr) in {"c1s-400": (500, 501, 8.2, 7.0), "c1d-400": (400, 470, 11.0, 9.0),
                               "c2-100": (90, 90, 8.2, 8.2), "c8-100": (90, 90, 8.2, 8.2)}.items():
    rs = [row("MAIN", gm + i, cell=cell, rep=i, cpu_srv=sm + 0.01 * i, rdiag_srv=98.0) for i in range(6)]
    xs = [row("R3", gr + i, cell=cell, rep=i, cpu_srv=sr + 0.01 * i, rdiag_srv=82.0) for i in range(6)]
    live += rs + xs
    tp.cell_verdict(live, live, cell, "R3", [], lambda s: None, readings)
p = tp.predictions(live, readings, lambda s: None)
check(p["c1d_goodput_better"].startswith("MET"), f"c1d +17 % with receiver busy 82 -> MET {p}")
check(p["cpusrv_better"].startswith("MET"), f"CPUSRV -15 % / -18 % at c1s / c1d -> MET {p}")
check("c1s_goodput_within" not in p, "R2's c1s_goodput_within is dropped")
check(p["c2_c8_within"].startswith("MET"), f"within -> MET {p}")
check(set(p) == {"c1d_goodput_better", "cpusrv_better", "c2_c8_within", "c1d_backlog"}, f"prediction names {sorted(p)}")
# the receiver still saturated: only PARTIAL
live2 = [r for r in live if r["arm"] == "MAIN"] + [dict(r, rdiag_srv_busy_med=97.0) for r in live if r["arm"] == "R3"]
p = tp.predictions(live2, readings, lambda s: None)
check(p["c1d_goodput_better"].startswith("PARTIAL"), f"goodput better but busy 97 -> PARTIAL {p}")

# ── c1d_backlog (reported) ───────────────────────────────────────────────
lv = [row("MAIN", 400, cell="c1d-400", rep=i, q=50.0) for i in range(3)] + \
     [row("R3", 400, cell="c1d-400", rep=i, q=10.0) for i in range(3)]
check(tp.predictions(lv, {}, lambda s: None)["c1d_backlog"].startswith("LOWER"), "q_avg 10 vs 50 -> LOWER")
lv = [dict(r, rdiag_srv_q_med=(60.0 if r["arm"] == "R3" else 50.0)) for r in lv]
check(tp.predictions(lv, {}, lambda s: None)["c1d_backlog"].startswith("NOT-LOWER"), "q_avg 60 vs 50 -> NOT-LOWER")
check(tp.predictions([], {}, lambda s: None)["c1d_backlog"].startswith("UNREAD"), "no rows -> UNREAD")

print("test_threadr3_parse: %d checks, %d failed" % (CHECKS, len(FAILS)))
sys.exit(1 if FAILS else 0)
