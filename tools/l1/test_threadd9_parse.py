#!/usr/bin/env python3
"""Offline exercise of `threadd9_parse.py` (docs/status.md §15) on SYNTHETIC
lines and rows.

    python3 test_threadd9_parse.py        # exit 0 iff every check passes

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
import threadd9_parse as tp  # noqa: E402

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

# ── the witnesses: Q2-era on both arms, the D9 main-thread witness two-sided ──
WAKE_Q2 = TS + "[DIAG] t=1 wake[tun=5 paused=3 pace=0 gen=0 nack=0 defc=0 tail=0 flush=0 ack=40 cmd=1 timer_acked=0]"
WAKE_OLD = TS + "[DIAG] t=1 wake[tun=5 paused=3 pace=0 gen=0 nack=0 defc=0 tail=0 flush=0 ack=40 timer_acked=0]"

for arm in ("MAIN", "NOD9"):
    r = tp.make_row("c1s-400", arm, "42", 1, 0, 10, [], [iown(0), WAKE_OLD], [])
    check("acks-not-routed-to-sender" not in r["problems"] and "wake-cmd-token-missing" in r["problems"],
          f"{arm}: a pre-Q2 wake token fails the Q2-era witness {r['problems']}")
    r = tp.make_row("c1s-400", arm, "42", 1, 0, 10, [], [iown(0, ack_dg=0), WAKE_Q2], [])
    check("acks-not-routed-to-sender" in r["problems"], f"{arm}: unrouted acks fail {r['problems']}")
    r = tp.make_row("c1s-400", arm, "42", 1, 0, 10, [], [iown(0), WAKE_Q2], [])
    check("d9-main-thread-unread" in r["problems"] or any(p.startswith("d9-witness") for p in r["problems"]),
          f"{arm}: the D9 witness reads the main thread {r['problems']}")

check(0.0 < tp.NOD9_THR_MIN and tp.MAIN_THR_MAX == tp.NOD9_THR_MIN,
      "MAIN's ceiling and NOD9's floor are one threshold, so no row satisfies both arms")

# ── the min-max rule ─────────────────────────────────────────────────────
check(tp.reading([90, 91, 92], [100, 101, 102], 0.05, True)[0] == "WORSE", "disjoint beyond band -> WORSE")
check(tp.reading([90, 91, 103], [100, 101, 102], 0.05, True)[0] == "TREND-WORSE", "overlap -> TREND-WORSE")
check(tp.reading([99, 100, 101], [100, 101, 102], 0.05, True)[0] == "WITHIN", "inside -> WITHIN")
check(tp.reading([110, 111, 112], [100, 101, 102], 0.05, True)[0] == "BETTER", "disjoint above -> BETTER")
check(tp.reading([110, 111, 100], [100, 101, 102], 0.05, True)[0] == "TREND-BETTER", "overlap above -> TREND-BETTER")
check(tp.reading([], [1], 0.05, True)[0] == "WORSE", "arm without a value -> WORSE")
check(abs(tp.spread_rel([80, 100, 120]) - 0.20) < 1e-12, "tolerance: half-range / median")


def row(arm, mbps, cell="c1s-400", lag_c=1000, feed=1.0, rep=1, seed="42", cpu_cli=5.8, busy=50.0):
    return {"cell": cell, "arm": arm, "seed": seed, "rep": rep, "status": "LIVE", "dnf": False,
            "mbps": mbps, "cpu_cli": cpu_cli, "cpu_srv": 8.2, "rtp_floor_p0": 2400, "rtp_floor_p1": 2400,
            "cli_lag_p99_us": lag_c, "srv_lag_p99_us": 1000, "feed_ratio_p0": feed, "feed_ratio_p1": feed,
            "problems": [], "bin": tp.ARM_BIN[arm], "bin_sha": None, "busy_med": busy,
            "cli_main_cores": 0.0 if arm == "MAIN" else 0.15}


def cell_rows(cell, arm_mbps):
    out = []
    for arm, mb in arm_mbps.items():
        out += [row(arm, mb + i, cell=cell, rep=i) for i in range(6)]
    return out


L = []
rows = cell_rows("c1s-400", {"MAIN": 500, "NOD9": 501})
check(tp.cell_verdict(rows, rows, "c1s-400", "NOD9", [], L.append) == "PASS", "NOD9 same as MAIN -> PASS")
check(tp.better_clauses(None) == [], "SAME -> no BETTER clause")
rows = cell_rows("c1s-400", {"MAIN": 500, "NOD9": 400})
check(tp.cell_verdict(rows, rows, "c1s-400", "NOD9", [], L.append) == "FAIL", "NOD9 -20 % disjoint -> FAIL")
rows = cell_rows("c1s-400", {"MAIN": 500, "NOD9": 600})
check(tp.cell_verdict(rows, rows, "c1s-400", "NOD9", [], L.append) == "PASS", "NOD9 +20 % -> PASS")
check(tp.better_clauses(None) == ["c1s-400:goodput"], f"+20 % disjoint -> BETTER goodput {tp.better_clauses(None)}")
few = [r for r in rows if r["arm"] != "NOD9"] + [r for r in rows if r["arm"] == "NOD9"][:2]
check(tp.cell_verdict(few, few, "c1s-400", "NOD9", [], L.append) == "UNSCOREABLE", "2 live NOD9 rows -> UNSCOREABLE")
base = cell_rows("c1s-400", {"MAIN": 500})
fed = base + [row("NOD9", 501 + i, feed=1.5, rep=i) for i in range(6)]
check(tp.cell_verdict(fed, fed, "c1s-400", "NOD9", [], L.append) == "FAIL", "feed 1.5 x MAIN -> FAIL")
lag = base + [row("NOD9", 501 + i, lag_c=5000 + i, rep=i) for i in range(6)]
check(tp.cell_verdict(lag, lag, "c1s-400", "NOD9", [], L.append) == "FAIL", "[LAG] p99 x5 disjoint -> FAIL")
tb = base + [row("NOD9", 500 + 4 * i, rep=i) for i in range(6)]
check(tp.cell_verdict(tb, tb, "c1s-400", "NOD9", [], L.append) == "PASS" and tp.better_clauses(None) == [],
      f"overlapping improvement is TREND-BETTER, not BETTER {tp.READINGS[('c1s-400', 'NOD9')]}")

# ── the decision, in precedence order ────────────────────────────────────
P = {"NOD9": "PASS-EVERYWHERE"}
check(tp.outcome(P, [], ["c1s-400:goodput"]).startswith("REVERT-D9"), "PASS + BETTER -> REVERT-D9")
check(tp.outcome(P, [], []).startswith("KEEP-D9 (SAME"), "PASS, nothing BETTER -> KEEP-D9 SAME")
check(tp.outcome({"NOD9": "FAIL-AT-c1s-400"}, [], []).startswith("KEEP-D9 (MAIN better"), "FAIL -> KEEP-D9, MAIN better")
check(tp.outcome({"NOD9": "FAIL-AT-c1s-400"}, [], ["c2-100:cpu_cli/GB"]).startswith("KEEP-D9 (MIXED"),
      "FAIL + BETTER elsewhere -> KEEP-D9 MIXED (the revert rule needs PASS-EVERYWHERE)")
check(tp.outcome({"NOD9": "UNSCOREABLE-AT-c8-100"}, [], ["c1s-400:goodput"]).startswith("UNSCOREABLE-AT"),
      "unscoreable cell -> UNSCOREABLE-AT")
check(tp.outcome(P, ["ABORT-LOCK"], ["c1s-400:goodput"]).startswith("UNSCOREABLE (abort"), "abort first")
check(tp.arm_verdict({c: "PASS" for c in tp.CELLS}) == "PASS-EVERYWHERE", "all cells PASS -> PASS-EVERYWHERE")
check(tp.arm_verdict(dict({c: "PASS" for c in tp.CELLS}, **{"c2-100": "FAIL", "c8-100": "UNSCOREABLE"}))
      == "FAIL-AT-c2-100", "a FAIL outranks an UNSCOREABLE")

# ── the c8 lag readout ───────────────────────────────────────────────────
live = [row("MAIN", 90, cell="c8-100", rep=i, lag_c=1500 + 10 * i) for i in range(12)]
live += [row("NOD9", 90, cell="c8-100", rep=i, lag_c=1600 + 10 * i) for i in range(12)]
lines = []
tp.readouts(live, lines.append)
check(any(l.startswith("C8LAG MAIN") and "n=12" in l and "1358" in l and "2202" in l for l in lines),
      f"C8LAG reports MAIN at n=12 against both references {lines}")

print("test_threadd9_parse: %d checks, %d failed" % (CHECKS, len(FAILS)))
sys.exit(1 if FAILS else 0)
