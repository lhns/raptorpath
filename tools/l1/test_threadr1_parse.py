#!/usr/bin/env python3
"""Offline exercise of `threadr1_parse.py` (docs/status.md §17) on SYNTHETIC
lines and rows.

    python3 test_threadr1_parse.py        # exit 0 iff every check passes

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
import threadr1_parse as tp  # noqa: E402

CHECKS = 0
FAILS = []


def check(cond, msg):
    global CHECKS
    CHECKS += 1
    if not cond:
        FAILS.append(msg)
        print("FAIL", msg)


TS = "\x1b[2m2026-10-26T10:00:00.000000Z\x1b[0m \x1b[32m INFO\x1b[0m raptorpath::runtime_obs: "


def iown(path, r1=True, win="run=1", phase="xfer", tx_dg=640, tx_b=10, rx_dg=600, rx_b=30, ack_dg=900,
         ack_b=60, hit=28, miss=2):
    pool = f" ibp_hit={hit} ibp_miss={miss} cmdq_n=0 cmdq_p50=- cmdq_p99=- cmdq_max=-" if r1 else ""
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
m = tp.io_columns([iown(0, r1=False)], "cli")
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

# ── the two-sided execution witness ──────────────────────────────────────
WAKE = TS + "[DIAG] t=1 wake[tun=5 paused=3 pace=0 gen=0 nack=0 defc=0 tail=0 flush=0 ack=40 cmd=1 timer_acked=0]"
r = tp.make_row("c1s-400", "R1", "42", 1, 0, 10, [], [iown(0), WAKE], [iown(0, win="obj=1", hit=0)])
check("pool-never-hit-srv" in r["problems"] and "no-pool-token-cli" not in r["problems"],
      f"R1 whose server pool never hit fails its witness {r['problems']}")
r = tp.make_row("c1s-400", "R1", "42", 1, 0, 10, [], [iown(0, r1=False), WAKE], [iown(0, r1=False, win="obj=1")])
check("no-pool-token-cli" in r["problems"] and "no-pool-token-srv" in r["problems"],
      f"an R1 row from a binary without the pool fails {r['problems']}")
r = tp.make_row("c1s-400", "MAIN", "42", 1, 0, 10, [], [iown(0), WAKE], [iown(0, win="obj=1")])
check("pool-token-on-MAIN-cli" in r["problems"] and "pool-token-on-MAIN-srv" in r["problems"],
      f"MAIN carrying the pool tokens fails its witness {r['problems']}")
r = tp.make_row("c1s-400", "MAIN", "42", 1, 0, 10, [], [iown(0, r1=False), WAKE], [iown(0, r1=False, win="obj=1")])
check("acks-not-routed-to-sender" not in r["problems"] and not any(p.startswith(("pool-token", "pool-never", "no-pool")) for p in r["problems"]),
      f"a clean MAIN row has no R1 or Q2 witness problem {r['problems']}")
r = tp.make_row("c1s-400", "MAIN", "42", 1, 0, 10, [], [iown(0, r1=False, ack_dg=0), WAKE], [])
check("acks-not-routed-to-sender" in r["problems"], "Q2's sender lane is a witness on both arms")

# ── the min-max rule ─────────────────────────────────────────────────────
check(tp.reading([90, 91, 92], [100, 101, 102], 0.05, True)[0] == "WORSE", "disjoint beyond band -> WORSE")
check(tp.reading([90, 91, 103], [100, 101, 102], 0.05, True)[0] == "TREND-WORSE", "overlap -> TREND-WORSE")
check(tp.reading([99, 100, 101], [100, 101, 102], 0.05, True)[0] == "WITHIN", "inside -> WITHIN")
check(tp.reading([120, 121, 122], [100, 101, 102], 0.05, True)[0] == "BETTER", "disjoint above band -> BETTER")
check(tp.reading([], [1], 0.05, True)[0] == "WORSE", "arm without a value -> WORSE")
check(abs(tp.spread_rel([80, 100, 120]) - 0.20) < 1e-12, "tolerance: half-range / median")


def row(arm, mbps, cell="c1s-400", lag_c=1000, feed=1.0, rep=1, seed="42", cpu_cli=5.8, cpu_srv=8.2,
        busy=50.0, rdiag_srv=95.0):
    return {"cell": cell, "arm": arm, "seed": seed, "rep": rep, "status": "LIVE", "dnf": False,
            "mbps": mbps, "cpu_cli": cpu_cli, "cpu_srv": cpu_srv, "rtp_floor_p0": 2400, "rtp_floor_p1": 2400,
            "cli_lag_p99_us": lag_c, "srv_lag_p99_us": 1000, "feed_ratio_p0": feed, "feed_ratio_p1": feed,
            "problems": [], "bin": tp.ARM_BIN[arm], "bin_sha": None, "busy_med": busy,
            "rdiag_srv_busy_med": rdiag_srv}


def cell_rows(cell, arm_mbps, **kw):
    out = []
    for arm, mb in arm_mbps.items():
        out += [row(arm, mb + i, cell=cell, rep=i, **kw) for i in range(6)]
    return out


lines = []
rows = cell_rows("c1s-400", {"MAIN": 500, "R1": 501})
check(tp.cell_verdict(rows, rows, "c1s-400", "R1", [], lines.append) == "PASS", "R1 same as MAIN -> PASS")
rows = cell_rows("c1s-400", {"MAIN": 500, "R1": 400})
check(tp.cell_verdict(rows, rows, "c1s-400", "R1", [], lines.append) == "FAIL", "R1 -20 % disjoint -> FAIL")
few = [r for r in rows if r["arm"] != "R1"] + [r for r in rows if r["arm"] == "R1"][:2]
check(tp.cell_verdict(few, few, "c1s-400", "R1", [], lines.append) == "UNSCOREABLE", "2 live R1 rows -> UNSCOREABLE")
base = cell_rows("c1s-400", {"MAIN": 500})
fed = base + [row("R1", 501 + i, feed=1.5, rep=i) for i in range(6)]
check(tp.cell_verdict(fed, fed, "c1s-400", "R1", [], lines.append) == "FAIL", "feed 1.5 x MAIN -> FAIL")
lag = base + [row("R1", 501 + i, lag_c=5000 + i, rep=i) for i in range(6)]
check(tp.cell_verdict(lag, lag, "c1s-400", "R1", [], lines.append) == "FAIL", "[LAG] p99 x5 disjoint -> FAIL")
cpu = base + [row("R1", 501 + i, cpu_srv=12.0 + i * 0.01, rep=i) for i in range(6)]
check(tp.cell_verdict(cpu, cpu, "c1s-400", "R1", [], lines.append) == "FAIL", "CPUSRV/byte +45 % disjoint -> FAIL")

# ── the outcome, in precedence order ─────────────────────────────────────
check(tp.outcome({"R1": "PASS-EVERYWHERE"}, []).startswith("DELIVERED"), "passes everywhere -> DELIVERED")
check(tp.outcome({"R1": "FAIL-AT-c1d-400"}, []).startswith("REFUTED-WITH-RECORD"), "a FAIL -> REFUTED-WITH-RECORD")
check(tp.outcome({"R1": "UNSCOREABLE-AT-c2-100"}, []).startswith("UNSCOREABLE-AT"), "unscoreable cell -> UNSCOREABLE-AT")
check(tp.outcome({"R1": "PASS-EVERYWHERE"}, ["ABORT-LOCK"]).startswith("UNSCOREABLE (abort"), "abort first")
check(tp.arm_verdict({c: "PASS" for c in tp.CELLS}) == "PASS-EVERYWHERE", "all cells PASS -> PASS-EVERYWHERE")
check(tp.arm_verdict(dict({c: "PASS" for c in tp.CELLS}, **{"c2-100": "FAIL", "c8-100": "UNSCOREABLE"}))
      == "FAIL-AT-c2-100", "a FAIL outranks an UNSCOREABLE")

# ── the named predictions, on a hand-built session ───────────────────────
live, readings = [], {}
for cell, (gm, gr, sm, sr) in {"c1s-400": (500, 501, 8.2, 7.0), "c1d-400": (400, 470, 11.0, 9.0),
                               "c2-100": (90, 90, 8.2, 8.2), "c8-100": (90, 90, 8.2, 8.2)}.items():
    rs = [row("MAIN", gm + i, cell=cell, rep=i, cpu_srv=sm + 0.01 * i, rdiag_srv=98.0) for i in range(6)]
    xs = [row("R1", gr + i, cell=cell, rep=i, cpu_srv=sr + 0.01 * i, rdiag_srv=82.0) for i in range(6)]
    live += rs + xs
    tp.cell_verdict(live, live, cell, "R1", [], lambda s: None, readings)
p = tp.predictions(live, readings, lambda s: None)
check(p["c1d_goodput_better"].startswith("MET"), f"c1d +17 % with receiver busy 82 -> MET {p}")
check(p["cpusrv_better"].startswith("MET"), f"CPUSRV -15 % / -18 % at c1s / c1d -> MET {p}")
check(p["c1s_goodput_within"].startswith("MET") and p["c2_c8_within"].startswith("MET"), f"within -> MET {p}")
# the receiver still saturated: only PARTIAL
live2 = [r for r in live if r["arm"] == "MAIN"] + [dict(r, rdiag_srv_busy_med=97.0) for r in live if r["arm"] == "R1"]
p = tp.predictions(live2, readings, lambda s: None)
check(p["c1d_goodput_better"].startswith("PARTIAL"), f"goodput better but busy 97 -> PARTIAL {p}")

print("test_threadr1_parse: %d checks, %d failed" % (CHECKS, len(FAILS)))
sys.exit(1 if FAILS else 0)
