#!/usr/bin/env python3
"""Offline exercise of `threadpd_parse.py` on SYNTHETIC lines and rows.

    python3 test_threadpd_parse.py        # exit 0 iff every check passes

NO ENGINE, NO VM. The `[IOWN]` / `wake[..]` / `[RDIAG]` lines are transcribed
from `src/runtime_obs.rs`, `net/diag.rs` and `net/receiver.rs`; the `[LAG]` line
from `l1common.py`'s documented shape (with `min_us`). The scorer is checked
on hand-built rows whose verdict is known in advance (absolute checks).
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import threadpd_parse as tp  # noqa: E402

CHECKS = 0
FAILS = []


def check(cond, msg):
    global CHECKS
    CHECKS += 1
    if not cond:
        FAILS.append(msg)
        print("FAIL", msg)


TS = "\x1b[2m2026-10-26T10:00:00.000000Z\x1b[0m \x1b[32m INFO\x1b[0m raptorpath::runtime_obs: "


def iown(path, win="run=1", phase="xfer", ack_dg=900, ack_b=60):
    return (TS + f"[IOWN] phase={phase} side=client {win} path={path} rt=main polls=100 drains=10 "
            f"tx_batches=10 tx_dg=640 ctrl_batches=1 ctrl_dg=2 rx_batches=30 rx_dg=600 "
            f"rx_capped=3 ack_batches={ack_b} ack_dg={ack_dg} send_err=0 too_large_staged=0 orphaned=0 "
            f"qcalls=900 q_secs=40 q_wall_us=5000 q_asleep_us=100 asleep_frac=0.0010 drv_on=5 drv_off=5 wall_s=2.000")


def lag(win, p99, mn, side="client"):
    return TS + f"[LAG] phase=xfer side={side} {win} tick_ms=10 n=200 p50_us=600 p99_us={p99} max_us=9000 min_us={mn}"


def thr(win, side="client"):
    return [TS + f"[THR] sum phase=xfer side={side} {win} workers=2 busy_s=1.0 threads=4 cpu_s=2.0 cores=1.5 wall_s=2.000"]


def rdiag(msgs=5000, prune_n=40, dec=True, recovered=300, below_floor=7, prune_max_us=120):
    """The receiver's [RDIAG] line (net/receiver.rs); `dec=False` is the old
    (MAIN) shape without the decoder witness."""
    base = TS + f"[RDIAG] busy=55% msgs={msgs}/s q_avg=3 q_max=9 cap=4096"
    if not dec:
        return base
    return (base + f" dec[rows=0 max_span=0 coeff_kb=0 recovered={recovered} seen={recovered} floor=1000 "
            f"below_floor={below_floor}] wcap=200 prune_n={prune_n} prune_max_us={prune_max_us}")


WAKE = TS + "[DIAG] t=1 wake[tun=5 paused=3 pace=0 gen=0 nack=0 defc=0 tail=0 flush=0 ack=40 cmd=1 timer_acked=0]"

# ── [RDIAG] decoder witness and E parsing ────────────────────────────────
d = tp.rdiag_dec([rdiag(prune_n=1, recovered=100), "x", rdiag(prune_n=40, recovered=500, below_floor=9, prune_max_us=300),
                  rdiag(prune_n=30, recovered=400, below_floor=12, prune_max_us=200)])
check(d == {"n": 3, "lines_pruned2": 2, "recovered_max": 500, "below_floor": 12, "prune_max_us": 300,
            "wcap": 200, "prune_n_max": 40}, f"rdiag_dec aggregates the in-transfer lines: {d}")
check(tp.rdiag_dec([rdiag(dec=False)]) is None, "old-shape [RDIAG] -> None")
check(tp.rdiag_dec([rdiag(msgs=10)]) is None, "an [RDIAG] line below RDIAG_MIN_MSGS is not in-transfer")
check(tp.any_dec([rdiag(msgs=10)]) and not tp.any_dec([rdiag(dec=False)]), "any_dec sees dec[ at any rate")
p99, mn, e = tp.lag_E(thr("run=1") + [lag("run=1", 1500, 300)], "run=1")
check((p99, mn, e) == (1500.0, 300.0, 1200.0), f"E = p99 - min ({p99},{mn},{e})")
check(tp.lag_max([lag("run=1", 1500, 300)], "run=1") == 9000.0, "lag max_us read from the xfer line")
check(tp.lag_E([lag("run=1", 1500, 300).replace(" min_us=300", "")], "run=1")[2] is None, "no min_us -> E None")
check(tp.lag_E([lag("run=2", 1500, 300)], "run=1") == (None, None, None), "other window ignored")

# ── the two-sided decoder witness ────────────────────────────────────────
def mkrow(arm, cli_extra, srv_extra, p99=1500, mn=300):
    cli = [iown(0), WAKE, lag("run=1", p99, mn)] + thr("run=1") + cli_extra
    srv = [iown(0, win="obj=1"), lag("obj=1", p99, mn, "server")] + thr("obj=1", "server") + srv_extra
    return tp.make_row("c1s-400", arm, "42", 1, 0, 10, [], cli, srv)


WIT = ("prune-per-drain-missing-srv", "dec-on-MAIN")
r = mkrow("NEW", [], [rdiag(prune_n=40, recovered=420, below_floor=5)])
check(not any(p in WIT for p in r["problems"]) and r["srv_prune_n_max"] == 40 and r["srv_recovered_max"] == 420
      and r["srv_below_floor"] == 5 and r["srv_wcap"] == 200 and r["srv_lag_max_us"] == 9000.0,
      f"NEW with prune_n >= 2 on the server: LIVE witness and readings ({r['problems']})")
r = mkrow("NEW", [], [rdiag(dec=False)])
check("prune-per-drain-missing-srv" in r["problems"] and r["srv_dec_n"] == 0, "NEW without dec[ -> not LIVE")
check(r["status"] != "LIVE", "NEW without dec[ -> status is not LIVE")
r = mkrow("NEW", [], [rdiag(prune_n=1)])
check("prune-per-drain-missing-srv" in r["problems"], "NEW with prune_n=1 (the 2 s tick) -> witness fail")
r = mkrow("NEW", [], [rdiag(prune_n=1), rdiag(prune_n=3)])
check("prune-per-drain-missing-srv" not in r["problems"], "NEW with one line at prune_n >= 2 is enough")
r = mkrow("MAIN", [], [rdiag(prune_n=0)])
check("dec-on-MAIN" in r["problems"] and r["status"] != "LIVE", "MAIN carrying dec[ on the server -> flagged")
r = mkrow("MAIN", [], [rdiag(dec=False)])
check(not any(p in WIT for p in r["problems"]), f"clean MAIN row has no decoder problem {r['problems']}")

# ── the min-max rule ─────────────────────────────────────────────────────
check(tp.reading([90, 91, 92], [100, 101, 102], 0.05, True)[0] == "WORSE", "disjoint beyond band -> WORSE")
check(tp.reading([90, 91, 103], [100, 101, 102], 0.05, True)[0] == "TREND-WORSE", "overlap -> TREND-WORSE")
check(tp.reading([99, 100, 101], [100, 101, 102], 0.05, True)[0] == "WITHIN", "inside -> WITHIN")
check(tp.reading([120, 121, 122], [100, 101, 102], 0.05, True)[0] == "BETTER", "disjoint above band -> BETTER")
check(tp.reading([], [1], 0.05, True)[0] == "WORSE", "arm without a value -> WORSE")


def row(arm, mbps, cell="c1s-400", lag_c=1000, lag_min=0, e_c=None, feed=1.0, rep=1, seed="42", cpu_cli=5.8, cpu_srv=8.2):
    e = (lag_c - lag_min) if e_c is None else e_c
    return {"cell": cell, "arm": arm, "seed": seed, "rep": rep, "status": "LIVE", "dnf": False,
            "mbps": mbps, "cpu_cli": cpu_cli, "cpu_srv": cpu_srv, "rtp_floor_p0": 2400, "rtp_floor_p1": 2400,
            "cli_lag_p99_us": lag_c, "srv_lag_p99_us": 1000, "cli_lag_min_us": lag_min, "srv_lag_min_us": 0,
            "cli_lag_E": e, "srv_lag_E": 1000, "feed_ratio_p0": feed, "feed_ratio_p1": feed,
            "problems": [], "bin": tp.ARM_BIN[arm], "bin_sha": None, "busy_med": 50.0}


def cell_rows(cell, arm_mbps, **kw):
    out = []
    for arm, mb in arm_mbps.items():
        out += [row(arm, mb + i, cell=cell, rep=i, **kw) for i in range(6)]
    return out


lines = []
rows = cell_rows("c1s-400", {"MAIN": 500, "NEW": 501})
check(tp.cell_verdict(rows, rows, "c1s-400", "NEW", [], lines.append) == "PASS", "NEW same as MAIN -> PASS")
rows = cell_rows("c1s-400", {"MAIN": 500, "NEW": 400})
check(tp.cell_verdict(rows, rows, "c1s-400", "NEW", [], lines.append) == "FAIL", "NEW -20 % disjoint -> FAIL")
few = [r for r in rows if r["arm"] != "NEW"] + [r for r in rows if r["arm"] == "NEW"][:2]
check(tp.cell_verdict(few, few, "c1s-400", "NEW", [], lines.append) == "UNSCOREABLE", "2 live NEW rows -> UNSCOREABLE")
base = cell_rows("c1s-400", {"MAIN": 500})
fed = base + [row("NEW", 501 + i, feed=1.5, rep=i) for i in range(6)]
check(tp.cell_verdict(fed, fed, "c1s-400", "NEW", [], lines.append) == "FAIL", "feed 1.5 x MAIN -> FAIL")
cpu = base + [row("NEW", 501 + i, cpu_srv=12.0 + i * 0.01, rep=i) for i in range(6)]
check(tp.cell_verdict(cpu, cpu, "c1s-400", "NEW", [], lines.append) == "FAIL", "CPUSRV/byte +45 % disjoint -> FAIL")

# ── reported batching-tail readings (not gating) ─────────────────────────
def rdq(q_avg, q_max):
    return rdiag().replace("q_avg=3 q_max=9", f"q_avg={q_avg} q_max={q_max}")


cv, qm = tp.rdiag_q([rdq(2, 5), rdq(4, 30), rdq(6, 12)])
check(abs(cv - ((8 / 3) ** 0.5) / 4) < 1e-9 and qm == 30, f"q_avg CV across lines and max q_max ({cv},{qm})")
check(tp.rdiag_q([rdq(2, 5)]) == (None, 5), "one line -> CV None")
r = mkrow("NEW", [], [rdq(2, 5), rdq(4, 30)])
check(r["rdiag_srv_q_cv"] is not None and r["rdiag_srv_qmax"] == 30, "row carries q_avg CV and q_max")
out = []
rows = [dict(row("NEW", 500 + i, rep=i), rdiag_srv_q_med=3.0, rdiag_srv_qmax=10, rdiag_srv_q_cv=0.2,
             acks_per_dgram=0.1 + i / 100, srv_recovered_max=300) for i in range(3)]
tp.reported(rows, "c1s-400", out.append)
check(any("batching tail" in x and "goodput max/min=1.004" in x and "acks/dgram min-max=0.100-0.120" in x for x in out),
      f"REPORT carries the batching-tail line: {out[:3]}")

# ── [LAG] scores E, not raw p99 ──────────────────────────────────────────
# p99 differs by 5x (the phase offset moved) but E is equal -> WITHIN, PASS.
mm = [row("MAIN", 500 + i, rep=i, lag_c=1000 + i, lag_min=0) for i in range(6)]
nn = [row("NEW", 500 + i, rep=i, lag_c=5000 + i, lag_min=4000) for i in range(6)]
st = {}
v = tp.cell_verdict(mm + nn, mm + nn, "c1s-400", "NEW", [], lines.append, st)
check(st[("c1s-400", "NEW")]["lag_E_cli"] == "WITHIN" and v == "PASS", f"p99 x5 but E equal -> WITHIN/PASS ({st})")
# E genuinely x5 -> WORSE -> FAIL
nn = [row("NEW", 500 + i, rep=i, lag_c=5000 + i, lag_min=0) for i in range(6)]
st = {}
v = tp.cell_verdict(mm + nn, mm + nn, "c1s-400", "NEW", [], lines.append, st)
check(st[("c1s-400", "NEW")]["lag_E_cli"] == "WORSE" and v == "FAIL", f"E x5 disjoint -> WORSE/FAIL ({st})")
check("lag_E_spread_cli" not in st[("c1s-400", "NEW")], "the spread clause is c8-100 only")

# ── lag_E_spread_cli branches (c8-100 only) ──────────────────────────────
check(abs(tp.spread_ratio([100, 200, 400]) - 4.0) < 1e-12, "spread ratio = max/min")
check(tp.spread_ratio([100]) is None and tp.spread_ratio([0, 5]) is None, "unreadable spread -> None")
# MAIN ratio 2.0; NEW 3.1 > 3.0 -> WORSE; 3.0 -> WITHIN; 1.3 < 1.333 -> BETTER; 1.4 -> WITHIN
check(tp.spread_reading([100, 310], [100, 200])[0] == "WORSE", "NEW ratio 3.1 > 2.0 x 1.5 -> WORSE")
check(tp.spread_reading([100, 300], [100, 200])[0] == "WITHIN", "NEW ratio 3.0 (= 2.0 x 1.5) -> WITHIN")
check(tp.spread_reading([100, 130], [100, 200])[0] == "BETTER", "NEW ratio 1.3 < 2.0 / 1.5 -> BETTER")
check(tp.spread_reading([100, 140], [100, 200])[0] == "WITHIN", "NEW ratio 1.4 -> WITHIN")
check(tp.spread_reading([100, 200], [100])[0] == "VACUOUS", "MAIN spread unreadable -> VACUOUS")


def c8(new_es):
    m = [row("MAIN", 90 + i, cell="c8-100", rep=i, e_c=e) for i, e in enumerate((100, 110, 120, 130, 140, 200))]
    n = [row("NEW", 90 + i, cell="c8-100", rep=i, e_c=e) for i, e in enumerate(new_es)]
    st = {}
    v = tp.cell_verdict(m + n, m + n, "c8-100", "NEW", [], lines.append, st)
    return v, st[("c8-100", "NEW")]


v, s = c8((100, 110, 120, 130, 140, 200))
check(s["lag_E_spread_cli"] == "WITHIN" and v == "PASS", f"equal spread -> WITHIN/PASS ({s})")
v, s = c8((100, 101, 102, 103, 104, 105))
check(s["lag_E_spread_cli"] == "BETTER", f"tight NEW spread -> BETTER ({s})")
v, s = c8((100, 110, 120, 130, 140, 400))
check(s["lag_E_spread_cli"] == "WORSE" and v == "FAIL", f"NEW spread x2 of MAIN's -> WORSE/FAIL ({s})")

# ── the outcome, in precedence order ─────────────────────────────────────
check(tp.outcome({"NEW": "PASS-EVERYWHERE"}, []).startswith("SHIP"), "passes everywhere -> SHIP")
check(tp.outcome({"NEW": "FAIL-AT-c1d-400"}, []).startswith("REFUTED-WITH-RECORD"), "a FAIL -> REFUTED-WITH-RECORD")
check(tp.outcome({"NEW": "UNSCOREABLE-AT-c2-100"}, []).startswith("REFUTED-WITH-RECORD"), "unscoreable -> REFUTED-WITH-RECORD")
check(tp.outcome({"NEW": "PASS-EVERYWHERE"}, ["ABORT-LOCK"]).startswith("UNSCOREABLE (abort"), "abort first")
check(tp.arm_verdict({c: "PASS" for c in tp.CELLS}) == "PASS-EVERYWHERE", "all cells PASS -> PASS-EVERYWHERE")
check(tp.arm_verdict(dict({c: "PASS" for c in tp.CELLS}, **{"c2-100": "FAIL", "c8-100": "UNSCOREABLE"}))
      == "FAIL-AT-c2-100", "a FAIL outranks an UNSCOREABLE")

print("test_threadpd_parse: %d checks, %d failed" % (CHECKS, len(FAILS)))
sys.exit(1 if FAILS else 0)
