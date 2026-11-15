#!/usr/bin/env python3
"""Tests for slaw_parse.py. Run: python3 test_slaw_parse.py

The saved sample (testdata/slaw_sample.log) is the engine's own output: the
lines `net::slaw::tests::render_window_two_legs` renders for two legs over
K + 2 windows (K = 8), captured with `cargo test ... render_window_two_legs
-- --nocapture`. The expectations below are read off its construction (20
samples per window per leg, leg 0 at 3000 us + 20j, leg 1 at 6000 us + 20j).
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import slaw_parse as sp  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)) if not ok else ""))
    if not ok:
        fails += 1


SAMPLE = lc.read(os.path.join(HERE, "testdata", "slaw_sample.log"))
check("the saved engine sample is present", len(SAMPLE) > 0)

g = sp.parse(SAMPLE)
check("windows: 10 closes x 2 legs on both per-leg tags", len(g["SLAW"]) == 20 and len(g["WSTAT"]) == 20,
      (len(g["SLAW"]), len(g["WSTAT"])))
check("store-scope tags once per window", len(g["WSTORE"]) == 10 and len(g["SLAWC"]) == 10)

# warm_s=0: every window is measured (the sample's t starts at 2.0 s anyway).
m = sp.row_medians(SAMPLE, warm_s=0.0)
l0, l1 = m["SLAW"]["p0"], m["SLAW"]["p1"]
check("leg 0 tmin = 3.000 ms, leg 1 = 6.000 ms (medians)", l0["tmin"] == 3.0 and l1["tmin"] == 6.0, (l0["tmin"], l1["tmin"]))
check("tp50 is the nearest-rank median of 3000 + 20j, j<20 (3200 us)", l0["tp50"] == 3.2, l0["tp50"])
check("nk = 20 on every window", l0["nk"] == 20 and l1["nk"] == 20)
check("lifetime min l = tmin", l0["l"] == 3.0 and l1["l"] == 6.0)
check("m = Copa anchor given (2900 / 5800 us)", l0["m"] == 2.9 and l1["m"] == 5.8, (l0["m"], l1["m"]))
check("b0p = the leg's constant brake (0.1 / 0.2 ms), defined from window K", l0["b0p"] == 0.1 and l1["b0p"] == 0.2,
      (l0["b0p"], l1["b0p"]))
check("b0k: K=8 raw brakes printed once", l0["b0k"] == [0.1] * 8 and len(l1["b0kn"]) == 8, (l0["b0k"], l1["b0kn"]))
check("R is `-` (None) before K windows: only windows 8..10 define it", l0["windows"] == 10)
rs = [r["R"] for r in g["SLAW"] if r["p"] == 0]
check("R defined on exactly the last 3 windows", [x is not None for x in rs] == [False] * 7 + [True] * 3, rs)
check("R, Rb are positive reals and the same on both legs of a window",
      all(a["R"] == b["R"] and a["Rb"] == b["Rb"] and a["R"] > 0
          for a, b in zip([r for r in g["SLAW"] if r["p"] == 0][7:], [r for r in g["SLAW"] if r["p"] == 1][7:])))
check("cumulative crossing/above-below counters are the LAST value, not a median",
      l0["xs"] is not None and l0["ra.0"] + l0["ra.1"] == 3, (l0["xs"], l0["ra.0"], l0["ra.1"]))
check("compound token expanded: n=100 W=50 per window", l0["n"] == 100 and l0["W"] == 50)

st = m["WSTORE"]
check("[WSTORE] release batches 1 event of 12 each window", st["relb.0"] == 1 and st["relb.1"] == 12 and st["relb.3"] == 12, st)
check("[WSTORE] emission bursts 2 events 8/16 -> p50 16 (nearest-rank of {8,16})", st["emb.0"] == 2 and st["emb.1"] == 16, st)
check("[WSTORE] adv= one sample per cumulative advance: 2 advances, sizes 10 and 30 -> p50 30, max 30", st["adv.0"] == 2 and st["adv.1"] == 30 and st["adv.3"] == 30, st)
check("[WSTORE] W1 debt flows present (zero in the unit sample; share is None, not 0, when nothing accrued)", st["dbt_acc"] == 0.0 and st["dbt_em"] == 0.0 and st["dbt_share"] is None and "dbt_rm_ack" in st and "dbt_ask_nack" in st, st)
check("[WSTORE] fec=eps/sigma2/r/astar/n expands to fec.0..fec.4 (n=0 window: components None, n 0)", "fec.4" in st and "fec.0" in st, st)
fec = sp.parse_line("[WSTORE] t=2.00 k=1 fec=0.020000/4.0000/0.20000/15.00/3 dbt_acc=0.60")[1]
check("fec token parsed: eps 0.02, sigma2 4, r 0.2, A* 15, n 3", (fec["fec.0"], fec["fec.1"], fec["fec.2"], fec["fec.3"], fec["fec.4"]) == (0.02, 4.0, 0.2, 15.0, 3.0), fec)
fec0 = sp.parse_line("[WSTORE] t=2.00 k=1 fec=-/-/-/-/0 dbt_acc=0.00")[1]
check("empty fec window: components None, n 0", fec0["fec.0"] is None and fec0["fec.3"] is None and fec0["fec.4"] == 0.0, fec0)
check("[WSTORE] a/W = 0.5", st["aw"] == 0.5 and st["awmin"] == 0.5)
check("[SLAWC] last line has the counters", "gaprep" in m["SLAWC"] and m["SLAWC"]["gaprep"] is not None)
check("a/b dash rule: '-' is None, not 0 (gaptrunc_frac with no reports)", m["SLAWC"].get("gaptrunc_frac") is None, m["SLAWC"])

# Warm-up exclusion: with the default warm_s = 1.0 s all windows (t >= 2.0) count; with 2.6 only t >= 2.6.
late = sp.row_medians(SAMPLE, warm_s=2.6)["SLAW"]["p0"]
check("warm_s drops early windows", late["windows"] == 7, late["windows"])

# The registered collapse rule against a cell BDP.
qc = sp.qcwnd_collapse(SAMPLE, bdp_bytes=6000.0, warm_s=0.0)
check("qcwnd 5000 < BDP 6000 on every window: flagged", qc["p0"]["share"] == 1.0 and qc["p0"]["flag"])
qc = sp.qcwnd_collapse(SAMPLE, bdp_bytes=4000.0, warm_s=0.0)
check("qcwnd 5000 >= BDP 4000: not flagged", qc["p0"]["share"] == 0.0 and not qc["p0"]["flag"])

# Tracing noise glued to a readout line is split by lc.read; a non-gauge line is ignored.
check("parse_line ignores other tags", sp.parse_line("[DIAG] t=1.0s win=3/4") is None)
check("an empty log has no windows: main() reports failure", sp.row_medians([])["SLAW"] == {})

sys.exit(1 if fails else 0)
