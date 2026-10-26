#!/usr/bin/env python3
"""Tests for storelaw_parse.py. Run: python3 test_storelaw_parse.py

The witness test replays one saved §18 Realtime log (docs/l1-raw/store-law-n1):
the row was voided by the inherited `window-line-missing-on-window` check,
which a named hint never satisfies (the engine prints "auto-selecting RLC
windowed backend" only when the hint is auto). A named hint is accepted from
`[PIPE] pipeline=window hint=<h>` plus `[DCAP] q=`.
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import storelaw_parse as sl  # noqa: E402

LEDGER = os.path.join(HERE, "..", "..", "docs", "l1-raw", "store-law-n1", "diag-sl")
BASE = os.path.join(LEDGER, "c1s-400-LAW-realtime-s42-r1-a1-")
fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)) if not ok else ""))
    if not ok:
        fails += 1


def row_of(base, arm, cell="c1s-400"):
    return sl.make_row(cell, arm, "42", "1", 0, 10,
                       lc.read(base + "drv.out"), lc.read(base + "c.log"), lc.read(base + "s.log"), 0, "x")


r = row_of(BASE, "LAW-realtime")
check("saved realtime log: pipeline echo is window/realtime",
      any("[PIPE] pipeline=window" in ln and "hint=realtime" in ln for ln in lc.read(BASE + "c.log")))
check("saved realtime log: [DCAP] q=0.05 reached the seat", r.get("dcap_q") == 0.05, r.get("dcap_q"))
check("a named hint is not voided by the missing auto-selection line",
      "window-line-missing-on-window" not in r["problems"], r["problems"])
check("saved realtime log is LIVE", r["status"] == "LIVE", (r["status"], r["problems"]))

# The witness must still bite: a named hint with no [DCAP] q= (mechanism not shown) stays void.
cli = [ln for ln in lc.read(BASE + "c.log") if "[DCAP]" not in ln]
r2 = sl.make_row("c1s-400", "LAW-realtime", "42", "1", 0, 10, lc.read(BASE + "drv.out"), cli,
                 lc.read(BASE + "s.log"), 0, "x")
check("named hint without [DCAP] q= stays WITNESS-FAIL", r2["status"] == "WITNESS-FAIL", r2["status"])

# An auto-hint row is unchanged: the line is still required there.
AUTO = os.path.join(LEDGER, "c1s-400-LAW-auto-s42-r2-a1-")
r3 = row_of(AUTO, "LAW-auto")
check("auto row unchanged (LIVE as scored in §18)", r3["status"] == "LIVE", (r3["status"], r3["problems"]))

print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
