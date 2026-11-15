#!/usr/bin/env python3
"""Offline exercise of `threadq1_parse.py` (docs/status.md §13) on SYNTHETIC
lines and rows.

    python3 test_threadq1_parse.py        # exit 0 iff every check passes

NO ENGINE, NO VM. The `[IOWN]` / `[LAG] io` / `[THR] io` lines are
transcribed from `src/runtime_obs.rs`'s renderers (its unit test
`the_iown_line_carries_deltas_and_the_asleep_gauge` pins the same tokens);
the `[TOPO]` line from `transport/quic.rs`. The scorer is checked on
hand-built rows whose verdict is known in advance (absolute checks).
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import threadq1_parse as tp  # noqa: E402

CHECKS = 0
FAILS = []


def check(cond, msg):
    global CHECKS
    CHECKS += 1
    if not cond:
        FAILS.append(msg)
        print("FAIL", msg)


TS = "\x1b[2m2026-10-06T10:00:00.000000Z\x1b[0m \x1b[32m INFO\x1b[0m raptorpath::runtime_obs: "


def iown(path, rt, asleep, on, off, win="run=1", phase="xfer", tx_dg=640, tx_b=10, rx_dg=600, rx_b=30):
    return (TS + f"[IOWN] phase={phase} side=client {win} path={path} rt={rt} polls=100 drains=10 "
            f"tx_batches={tx_b} tx_dg={tx_dg} ctrl_batches=1 ctrl_dg=2 rx_batches={rx_b} rx_dg={rx_dg} "
            f"rx_capped=3 send_err=0 too_large_staged=0 orphaned=0 qcalls=900 q_secs=40 q_wall_us=5000 "
            f"q_asleep_us=100 asleep_frac={asleep} drv_on={on} drv_off={off} wall_s=2.000")


cli = [
    iown(0, "rp-io-0", "0.0010", 50, 0),
    iown(1, "rp-io-1", "0.0030", 40, 10),
    iown(0, "rp-io-0", "0.9000", 1, 1, win="run=0"),        # another window: ignored
    iown(0, "rp-io-0", "0.5000", 1, 1, win="", phase="run"),  # process-end: ignored
    TS + "[LAG] io phase=xfer side=client run=1 rt=rp-io-0 tick_ms=10 n=200 p50_us=900 p99_us=1500 max_us=2000 dropped=0",
    TS + "[LAG] io phase=xfer side=client run=1 rt=rp-io-1 tick_ms=10 n=200 p50_us=900 p99_us=2500 max_us=3000 dropped=0",
    TS + "[THR] io phase=xfer side=client run=1 rt=rp-io-0 busy_s=0.5 busy_frac=0.250 park=300 unpark=310 wall_s=2.000",
    TS + "[THR] io phase=xfer side=client run=1 rt=rp-io-1 busy_s=0.5 busy_frac=0.250 park=100 unpark=110 wall_s=2.000",
]
c = tp.io_columns(cli, "cli", "xfer", "run=1")
check(c["cli_owners"] == 2, f"two owners in the window ({c['cli_owners']})")
check(abs(c["cli_asleep_max"] - 0.003) < 1e-12 and abs(c["cli_asleep_sum"] - 0.004) < 1e-12,
      f"asleep max/sum {c['cli_asleep_max']}/{c['cli_asleep_sum']}")
check(c["cli_drv_on"] == 90 and c["cli_drv_off"] == 10 and abs(c["cli_drv_off_frac"] - 0.1) < 1e-12,
      f"routing sums {c['cli_drv_on']}/{c['cli_drv_off']}/{c['cli_drv_off_frac']}")
check(c["cli_tx_per_batch"] == 64.0 and c["cli_rx_per_batch"] == 20.0, "datagrams per batch")
check(c["cli_iolag_p99_us"] == 2500.0, "[LAG] io p99: max over I/O runtimes")
check(c["cli_io_park_per_s"] == 200.0 and c["cli_io_runtimes"] == 2, "[THR] io parks/s summed")
empty = tp.io_columns([], "cli")
check(set(empty) == set(c) and all(v is None for v in empty.values()),
      "io_columns: the row shape does not depend on the capture")

# ── the min-max rule ─────────────────────────────────────────────────────
check(tp.reading([90, 91, 92], [100, 101, 102], 0.05, True)[0] == "WORSE", "disjoint beyond band -> WORSE")
check(tp.reading([90, 91, 103], [100, 101, 102], 0.05, True)[0] == "TREND-WORSE", "overlap -> TREND-WORSE")
check(tp.reading([99, 100, 101], [100, 101, 102], 0.05, True)[0] == "WITHIN", "inside -> WITHIN")
check(tp.reading([], [1], 0.05, True)[0] == "WORSE", "arm without a value -> WORSE")
check(abs(tp.spread_rel([80, 100, 120]) - 0.20) < 1e-12, "tolerance: half-range / median")


def row(arm, mbps, cell="c1s-400", lag_c=1000, feed=1.0, rep=1, seed="42", asleep=None, off=None, on=None):
    return {"cell": cell, "arm": arm, "seed": seed, "rep": rep, "status": "LIVE", "dnf": False,
            "mbps": mbps, "cpu_cli": 5.8, "cpu_srv": 8.2, "rtp_floor_p0": 2400, "rtp_floor_p1": 2400,
            "cli_lag_p99_us": lag_c, "srv_lag_p99_us": 1000, "feed_ratio_p0": feed, "feed_ratio_p1": feed,
            "problems": [], "bin": tp.ARM_BIN[arm], "bin_sha": None,
            "cli_asleep_max": asleep, "cli_drv_off": off, "srv_drv_off": 0 if off is not None else None,
            "cli_drv_on": on, "srv_drv_on": on, "cli_drv_off_frac": (off / (on + off)) if (on and off is not None) else None}


def cell_rows(cell, arm_mbps):
    out = []
    for arm, m in arm_mbps.items():
        out += [row(arm, m + i, cell=cell, rep=i) for i in range(6)]
    return out


rows = cell_rows("c1s-400", {"MAIN": 500, "IOS": 501, "IOO": 400, "D9": 500})
lines = []
check(tp.cell_verdict(rows, rows, "c1s-400", "IOS", [], lines.append) == "PASS", "IOS same as MAIN -> PASS")
check(tp.cell_verdict(rows, rows, "c1s-400", "IOO", [], lines.append) == "FAIL", "IOO -20 % disjoint -> FAIL")
few = [r for r in rows if r["arm"] != "IOS"] + [r for r in rows if r["arm"] == "IOS"][:2]
check(tp.cell_verdict(few, few, "c1s-400", "IOS", [], lines.append) == "UNSCOREABLE", "2 live IOS rows -> UNSCOREABLE")
fed = [r for r in rows if r["arm"] != "IOS"] + [row("IOS", 501 + i, feed=1.5, rep=i) for i in range(6)]
check(tp.cell_verdict(fed, fed, "c1s-400", "IOS", [], lines.append) == "FAIL", "feed 1.5 x MAIN -> FAIL")

# ── the outcome, in precedence order ─────────────────────────────────────
P, F = "PASS-EVERYWHERE", "FAIL-AT-c1s-400"
check(tp.outcome({"IOS": P, "IOO": P}, []).startswith("DELIVERED (SHIP-SHARED"), "both pass -> shared ships")
check(tp.outcome({"IOS": P, "IOO": F}, []).startswith("DELIVERED (SHIP-SHARED"), "only shared -> shared")
check(tp.outcome({"IOS": F, "IOO": P}, []).startswith("DELIVERED (SHIP-OWN"), "only own -> own")
check(tp.outcome({"IOS": F, "IOO": F}, []).startswith("REFUTED-WITH-RECORD"), "neither -> refuted")
check(tp.outcome({"IOS": F, "IOO": "UNSCOREABLE-AT-c2-100"}, []).startswith("UNSCOREABLE-AT"),
      "one fails, one unscoreable -> UNSCOREABLE-AT")
check(tp.outcome({"IOS": P, "IOO": P}, ["ABORT-LOCK"]).startswith("UNSCOREABLE (abort"), "abort first")
check(tp.arm_verdict({c: "PASS" for c in tp.CELLS}) == P, "all cells PASS -> PASS-EVERYWHERE")
check(tp.arm_verdict(dict({c: "PASS" for c in tp.CELLS}, **{"c2-100": "FAIL", "c8-100": "UNSCOREABLE"}))
      == "FAIL-AT-c2-100", "a FAIL outranks an UNSCOREABLE")

# ── the mechanism predictions ────────────────────────────────────────────
live = []
for cell in tp.CELLS:
    live += [row("IOO", 500, cell=cell, rep=i, asleep=0.001, off=0, on=100) for i in range(3)]
    live += [row("IOS", 500, cell=cell, rep=i, asleep=0.05, off=30, on=70) for i in range(3)]
m = tp.mechanism(live, lambda s: None)
check(m == {"routing_own": "MET", "routing_shared_fails": "MET",
            "lockwait_own_zero": "MET", "lockwait_shared_nonzero": "MET"}, f"predictions met {m}")
live[0]["cli_drv_off"] = 1
m = tp.mechanism(live, lambda s: None)
check(m["routing_own"] == "MISSED", "one own row with drv_off > 0 misses the routing prediction")

print("test_threadq1_parse: %d checks, %d failed" % (CHECKS, len(FAILS)))
sys.exit(1 if FAILS else 0)
