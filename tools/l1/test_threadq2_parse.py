#!/usr/bin/env python3
"""Offline exercise of `threadq2_parse.py` (docs/status.md §14) on SYNTHETIC
lines and rows.

    python3 test_threadq2_parse.py        # exit 0 iff every check passes

NO ENGINE, NO VM. The `[IOWN]` line is transcribed from
`src/runtime_obs.rs`'s renderer (its unit test
`the_iown_line_carries_deltas_and_the_asleep_gauge` pins the same tokens,
the Q2 sender-lane `ack_batches=`/`ack_dg=` included); the `wake[..]` token
from `net/diag.rs`'s `wake_line` (`the_wake_token_counts_per_arm_
cumulatively`). The scorer is checked on hand-built rows whose verdict is
known in advance (absolute checks).
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import threadq2_parse as tp  # noqa: E402

CHECKS = 0
FAILS = []


def check(cond, msg):
    global CHECKS
    CHECKS += 1
    if not cond:
        FAILS.append(msg)
        print("FAIL", msg)


TS = "\x1b[2m2026-10-06T10:00:00.000000Z\x1b[0m \x1b[32m INFO\x1b[0m raptorpath::runtime_obs: "


def iown(path, q2=True, win="run=1", phase="xfer", tx_dg=640, tx_b=10, rx_dg=600, rx_b=30, ack_dg=900, ack_b=60):
    lane = f"ack_batches={ack_b} ack_dg={ack_dg} " if q2 else ""
    return (TS + f"[IOWN] phase={phase} side=client {win} path={path} rt=main polls=100 drains=10 "
            f"tx_batches={tx_b} tx_dg={tx_dg} ctrl_batches=1 ctrl_dg=2 rx_batches={rx_b} rx_dg={rx_dg} "
            f"rx_capped=3 {lane}send_err=0 too_large_staged=0 orphaned=0 qcalls=900 q_secs=40 q_wall_us=5000 "
            f"q_asleep_us=100 asleep_frac=0.0010 drv_on=5 drv_off=5 wall_s=2.000")


cli = [
    iown(0),
    iown(1, ack_dg=300, ack_b=40),
    iown(0, win="run=0", ack_dg=99999),          # another window: ignored
    iown(0, win="", phase="run", ack_dg=99999),  # process-end: ignored
]
c = tp.io_columns(cli, "cli", "xfer", "run=1")
check(c["cli_owners"] == 2 and c["cli_ack_tok"] == 2, f"two owners, both with the sender lane ({c})")
check(c["cli_ack_dg"] == 1200 and c["cli_ack_batches"] == 100 and c["cli_ack_per_batch"] == 12.0,
      f"sender-lane sums {c['cli_ack_dg']}/{c['cli_ack_batches']}/{c['cli_ack_per_batch']}")
check(c["cli_tx_per_batch"] == 64.0 and c["cli_rx_per_batch"] == 20.0, "datagrams per batch")
m = tp.io_columns([iown(0, q2=False)], "cli")
check(m["cli_ack_tok"] == 0 and m["cli_ack_dg"] is None, "a MAIN owner line has no sender lane")
empty = tp.io_columns([], "cli")
check(set(empty) == set(c) and all(v is None for v in empty.values()),
      "io_columns: the row shape does not depend on the capture")

# ── the two-sided execution witness ──────────────────────────────────────
WAKE_Q2 = TS + "[DIAG] t=1 wake[tun=5 paused=3 pace=0 gen=0 nack=0 defc=0 tail=0 flush=0 ack=40 cmd=1 timer_acked=0]"
WAKE_MAIN = TS + "[DIAG] t=1 wake[tun=5 paused=3 pace=0 gen=0 nack=0 defc=0 tail=0 flush=0 ack=40 timer_acked=0]"
r = tp.make_row("c1s-400", "Q2", "42", 1, 0, 10, [], [iown(0), WAKE_Q2], [])
check("no-iown-srv" in r["problems"] and "acks-not-routed-to-sender" not in r["problems"]
      and "wake-cmd-token-missing" not in r["problems"], f"Q2 witness reads the sender lane {r['problems']}")
r = tp.make_row("c1s-400", "Q2", "42", 1, 0, 10, [], [iown(0, ack_dg=0), WAKE_MAIN], [])
check("acks-not-routed-to-sender" in r["problems"] and "wake-cmd-token-missing" in r["problems"],
      f"Q2 without routed acks / cmd token fails its witness {r['problems']}")
r = tp.make_row("c1s-400", "MAIN", "42", 1, 0, 10, [], [iown(0), WAKE_Q2], [])
check("sender-lane-token-on-MAIN-cli" in r["problems"] and "wake-cmd-token-on-MAIN" in r["problems"]
      and any(p.startswith("topo-count-MAIN") for p in r["problems"]),
      f"MAIN carrying Q2 tokens fails its witness {r['problems']}")

# ── the min-max rule ─────────────────────────────────────────────────────
check(tp.reading([90, 91, 92], [100, 101, 102], 0.05, True)[0] == "WORSE", "disjoint beyond band -> WORSE")
check(tp.reading([90, 91, 103], [100, 101, 102], 0.05, True)[0] == "TREND-WORSE", "overlap -> TREND-WORSE")
check(tp.reading([99, 100, 101], [100, 101, 102], 0.05, True)[0] == "WITHIN", "inside -> WITHIN")
check(tp.reading([], [1], 0.05, True)[0] == "WORSE", "arm without a value -> WORSE")
check(abs(tp.spread_rel([80, 100, 120]) - 0.20) < 1e-12, "tolerance: half-range / median")


def row(arm, mbps, cell="c1s-400", lag_c=1000, feed=1.0, rep=1, seed="42", cpu_cli=5.8, busy=50.0):
    return {"cell": cell, "arm": arm, "seed": seed, "rep": rep, "status": "LIVE", "dnf": False,
            "mbps": mbps, "cpu_cli": cpu_cli, "cpu_srv": 8.2, "rtp_floor_p0": 2400, "rtp_floor_p1": 2400,
            "cli_lag_p99_us": lag_c, "srv_lag_p99_us": 1000, "feed_ratio_p0": feed, "feed_ratio_p1": feed,
            "problems": [], "bin": tp.ARM_BIN[arm], "bin_sha": None, "busy_med": busy}


def cell_rows(cell, arm_mbps):
    out = []
    for arm, mb in arm_mbps.items():
        out += [row(arm, mb + i, cell=cell, rep=i) for i in range(6)]
    return out


lines = []
rows = cell_rows("c1s-400", {"MAIN": 500, "Q2": 501})
check(tp.cell_verdict(rows, rows, "c1s-400", "Q2", [], lines.append) == "PASS", "Q2 same as MAIN -> PASS")
rows = cell_rows("c1s-400", {"MAIN": 500, "Q2": 400})
check(tp.cell_verdict(rows, rows, "c1s-400", "Q2", [], lines.append) == "FAIL", "Q2 -20 % disjoint -> FAIL")
few = [r for r in rows if r["arm"] != "Q2"] + [r for r in rows if r["arm"] == "Q2"][:2]
check(tp.cell_verdict(few, few, "c1s-400", "Q2", [], lines.append) == "UNSCOREABLE", "2 live Q2 rows -> UNSCOREABLE")
base = cell_rows("c1s-400", {"MAIN": 500})
fed = base + [row("Q2", 501 + i, feed=1.5, rep=i) for i in range(6)]
check(tp.cell_verdict(fed, fed, "c1s-400", "Q2", [], lines.append) == "FAIL", "feed 1.5 x MAIN -> FAIL")
lag = base + [row("Q2", 501 + i, lag_c=5000 + i, rep=i) for i in range(6)]
check(tp.cell_verdict(lag, lag, "c1s-400", "Q2", [], lines.append) == "FAIL", "[LAG] p99 x5 disjoint -> FAIL")

# ── the outcome, in precedence order ─────────────────────────────────────
check(tp.outcome({"Q2": "PASS-EVERYWHERE"}, []).startswith("DELIVERED"), "passes everywhere -> DELIVERED")
check(tp.outcome({"Q2": "FAIL-AT-c1d-400"}, []).startswith("REFUTED-WITH-RECORD"), "a FAIL -> REFUTED-WITH-RECORD")
check(tp.outcome({"Q2": "UNSCOREABLE-AT-c2-100"}, []).startswith("UNSCOREABLE-AT"), "unscoreable cell -> UNSCOREABLE-AT")
check(tp.outcome({"Q2": "PASS-EVERYWHERE"}, ["ABORT-LOCK"]).startswith("UNSCOREABLE (abort"), "abort first")
check(tp.arm_verdict({c: "PASS" for c in tp.CELLS}) == "PASS-EVERYWHERE", "all cells PASS -> PASS-EVERYWHERE")
check(tp.arm_verdict(dict({c: "PASS" for c in tp.CELLS}, **{"c2-100": "FAIL", "c8-100": "UNSCOREABLE"}))
      == "FAIL-AT-c2-100", "a FAIL outranks an UNSCOREABLE")

# ── the named c2 prediction ──────────────────────────────────────────────
live = [row("MAIN", 90, cell="c2-100", rep=i, cpu_cli=3.0) for i in range(6)]
live += [row("Q2", 90, cell="c2-100", rep=i, cpu_cli=2.8) for i in range(6)]
p = tp.predictions(live, lambda s: None)
check(p["c2_cpucli_recovers_p1"].startswith("MET"), f"-6.7 % meets the -5.3 % recovery {p}")
live = [r for r in live if r["arm"] == "MAIN"] + [row("Q2", 90, cell="c2-100", rep=i, cpu_cli=2.9) for i in range(6)]
p = tp.predictions(live, lambda s: None)
check(p["c2_cpucli_recovers_p1"].startswith("MISSED"), f"-3.3 % misses it {p}")

print("test_threadq2_parse: %d checks, %d failed" % (CHECKS, len(FAILS)))
sys.exit(1 if FAILS else 0)
