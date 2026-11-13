#!/usr/bin/env python3
"""Tests for the dry-run guard-clause candidates in threadp1_parse.py (docs/derivations/guard-clause-dry.md): (A) rtp_floor_source_phase,
(B) inorder_delay / inorder_delay_stats, and (C) rtp_floor left unchanged.  Fixture lines carry only the tokens the parsers read.
Run: python3 test_guard_clause_dry.py"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import threadp1_parse as t1  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1200]) if not ok else ""))
    if not ok:
        fails += 1


def near(a, b, tol=1e-9):
    return a is not None and abs(a - b) <= tol


def diag(t, src, cum, rtp=()):
    """One client [DIAG] line: src= rate (sym/s), cum=sent/coded/ack, p<i> blocks with rtp_us (0 = no sample yet)."""
    s, c, a = cum
    legs = " ".join(f"p{i}:infl=1/sinfl=0 rtt=9/wrtt=9/rtp1ms rtp_us={v} sig_us=1/n1" for i, v in enumerate(rtp))
    return f"[DIAG] t={t:.1f}s win=1/1 src={src}sym/s cod=0sym/s cum={s}/{c}/{a} cwnd=1 infl=1 {legs}"


# start-up sample at t=0.3, a rise, the source stops at t=2.0, the drained tail at t=3.0 reads low again
LOG = [
    "noise line",
    diag(0.3, 100, (10, 0, 0), (900, 0)),
    diag(0.6, 100, (20, 0, 5), (900, 5000)),
    diag(1.0, 100, (30, 0, 10), (1500, 4000)),
    diag(1.5, 100, (40, 0, 20), (1400, 4500)),
    diag(2.0, 0, (50, 0, 30), (1300, 4200)),
    diag(3.0, 0, (50, 0, 50), (700, 4100)),
]

# (C) unchanged: the plain minimum of non-zero rtp_us over every tick
check("C rtp_floor: minimum non-zero per leg over every [DIAG] tick (a 0 is not a sample)", t1.rtp_floor(LOG) == {0: 700, 1: 4000}, t1.rtp_floor(LOG))

# (A)
a = t1.rtp_floor_source_phase(LOG)
check("A: ticks with t < 1.0 and ticks with src=0 are excluded: leg 0 -> 1400 (t=1.5), not 900 (start-up) nor 700 (drained tail)", a == {0: 1400, 1: 4000}, a)
d = t1.rtp_floor_source_phase(LOG, detail=True)
check("A detail: (min, t of its first occurrence, value at the first in-window tick) = (1400, 1.5, 1500) for leg 0",
      d[0] == (1400, 1.5, 1500) and d[1] == (4000, 1.0, 4000), d)
check("A: t_min is a parameter (t >= 0.0 admits the start-up tick)", t1.rtp_floor_source_phase(LOG, t_min=0.0)[0] == 900)
check("A: a leg with no in-window source-phase sample is ABSENT, never a substitute",
      t1.rtp_floor_source_phase([diag(0.3, 10, (1, 0, 0), (500,)), diag(2.0, 0, (1, 0, 1), (400,))]) == {})
check("A: rtp_us=0 ticks are skipped (leg 1 has no sample at t=0.3)", t1.rtp_floor_source_phase([diag(1.2, 10, (1, 0, 0), (0, 800))]) == {1: 800})
check("A: a line without src= or t= is ignored, and non-DIAG lines too",
      t1.rtp_floor_source_phase(["[DIAG] t=2.0s rtp_us=5", "p0:infl=1 rtp_us=7"]) == {})

# (B): sent curve 10/tick-pair; ack lags
# ticks t: 1,2,3,4 ; sent: 100,200,300,400 ; ack: 0,50,150,300
LB = [diag(1.0, 5, (100, 0, 0)), diag(2.0, 5, (200, 0, 50)), diag(3.0, 5, (300, 0, 150)), diag(4.0, 5, (400, 0, 300))]
dl = dict(t1.inorder_delay(LB))
# ack 0 is skipped; ack 50 first sent at t = 1 + (50-... bisect: SRC[0]=100 >= 50 -> i=0 -> T[0]=1.0 -> delay 2.0-1.0 = 1.0
# ack 150: between (100,t=1) and (200,t=2) -> 1.5 -> delay 3.0-1.5 = 1.5 ; ack 300: SRC[2]=300 -> i=2: (200,t=2)->(300,t=3) -> 3.0 -> delay 1.0
check("B delay(t) = t - t_src(ack(t)) by linear interpolation of the sent curve (ack 0 skipped; ack below the first sent point -> first tick)",
      set(dl) == {2.0, 3.0, 4.0} and near(dl[2.0], 1.0) and near(dl[3.0], 1.5) and near(dl[4.0], 1.0), dl)
st = t1.inorder_delay_stats(LB)
check("B stats: mean 7/6 of (1.0, 1.5, 1.0) and p95 = sorted[int(0.95*(n-1)+0.5)] = 1.5, n = 3",
      st["n"] == 3 and near(st["mean"], 3.5 / 3) and near(st["p95"], 1.5), st)
check("B: an ack beyond the sent curve is skipped, not clamped", [x for x in t1.inorder_delay([diag(1.0, 5, (100, 0, 0)), diag(2.0, 5, (200, 0, 250))])] == [])
check("B: no usable tick -> None (an undefined row, never 0)", t1.inorder_delay_stats(["noise", diag(1.0, 5, (100, 0, 0))]) is None)
# drained tail: the source stops at t=3, the receiver catches up late
LT = LB[:3] + [diag(4.0, 0, (300, 0, 150)), diag(5.0, 0, (300, 0, 250))]
check("B source_only drops ticks with src=0 (the drained tail) from the series",
      [t for t, _ in t1.inorder_delay(LT, source_only=True)] == [2.0, 3.0] and [t for t, _ in t1.inorder_delay(LT)] == [2.0, 3.0, 4.0, 5.0])
check("B whole run sees the tail: delay at t=5 (ack 250 sent at t=2.5) is 2.5 s", near(dict(t1.inorder_delay(LT))[5.0], 2.5))

print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
