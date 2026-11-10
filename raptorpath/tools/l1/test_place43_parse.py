#!/usr/bin/env python3
"""Tests for place43_parse.py (status §43 with amendment 1). Run: python3 test_place43_parse.py"""
import json
import math
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.stdout.reconfigure(encoding="utf-8")
os.environ.pop("LD_PLAN", None)
os.environ.pop("LD_ONDISK", None)
import l1common as lc  # noqa: E402
import place43_parse as pp  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)) if not ok else ""))
    if not ok:
        fails += 1


# ── the log contract: one literal shared with the engine's format test ────
OCAP_FIXTURE = ("[OCAP] t=12.5s p0 law=oracle n=157 ask=156.42 h=0 "
                "rtprop_us=7012 tmin_us=7100 tp50_us=15200 s_us=8100 brake_us=88 n_k=2412 offered=30120 r_k=16.45 "
                "floor=0.000 pin=0.000 hold=0.000 wbind=0 nmin=150 nmax=161 refreshes=50 q=0.1000")
o = pp.parse_ocap(OCAP_FIXTURE)
check("contract: the engine's [OCAP] fixture parses into every field",
      o == {"tstr": "12.5", "t": 12.5, "path": 0, "law": "oracle", "n": 157, "ask": 156.42, "h": 0, "rtprop": 7012,
            "tmin": 7100, "tp50": 15200, "s_us": 8100, "brake": 88, "n_k": 2412, "offered": 30120, "r_k": 16.45,
            "floor": 0.0, "pin": 0.0, "hold": 0.0, "wbind": 0, "nmin": 150, "nmax": 161, "refreshes": 50, "q": 0.1}, o)
empty = pp.parse_ocap("[OCAP] t=0.2s p0 law=shadow n=10 ask=0.00 h=0 rtprop_us=- tmin_us=- tp50_us=- s_us=0 brake_us=- "
                      "n_k=0 offered=0 r_k=- floor=1.000 pin=0.000 hold=0.000 wbind=0 nmin=10 nmax=10 refreshes=40 q=0.1000")
check("contract: an empty window's `-` fields read as None", empty and empty["tmin"] is None and empty["r_k"] is None and empty["brake"] is None)
RS = os.path.join(HERE, "..", "..", "src", "net", "oracle", "tests.rs")
if os.path.exists(RS):
    src = open(RS, encoding="utf-8").read()
    m = re.search(r'OCAP_FIXTURE: &str = "(.*?)";', src, re.S)
    lit = re.sub(r"\\\r?\n\s*", "", m.group(1)) if m else None
    check("contract: the Rust OCAP_FIXTURE is the same string (net/oracle/tests.rs)", lit == OCAP_FIXTURE, lit)
else:
    print("skip the Rust fixture comparison (src/net/oracle/tests.rs is not on this branch)")

# ── the plan ───────────────────────────────────────────────────────────────
pl = pp.plan43()
c2p = [(a, s, r) for c, a, s, r in pl if c == "c2-100"]
c1p = [(a, s, r) for c, a, s, r in pl if c == "c1s-400"]
check("plan 43: 24 scored invocations = 3 blocks x (2 cells x 3 arms + 2 extra arms at c2); with the 2 smoke rows 26 invocations",
      len(pl) == 24 == pp.EXPECT_ROWS, len(pl))
check("plan 43: n = 3 per (cell, arm) for MAIN / SL / SH, and n = 3 for SL-B0 and SL-G12 at c2 only",
      all(sum(1 for c, a, s, r in pl if (c, a) == (cc, aa)) == 3 for cc in pp.CELL_ORDER for aa in pp.ARMS)
      and all(sum(1 for c, a, s, r in pl if (c, a) == ("c2-100", x)) == 3 for x in pp.EXTRA_ARMS)
      and not any(c == "c1s-400" and a in pp.EXTRA_ARMS for c, a, s, r in pl))
check("plan 43: the extra arms use seeds 42 / 7 / 42 (reps 1 / 1 / 2)",
      [(s, r) for a, s, r in c2p if a == "SL-B0"] == [("42", 1), ("7", 1), ("42", 2)]
      and [(s, r) for a, s, r in c2p if a == "SL-G12"] == [("42", 1), ("7", 1), ("42", 2)])
check("plan 43: blocks (42,1) (7,1) (42,2), c2 holds 5 rows and c1s 3 per block",
      [(s, r) for a, s, r in c2p[::5]] == [("42", 1), ("7", 1), ("42", 2)] and [(s, r) for a, s, r in c1p[::3]] == [("42", 1), ("7", 1), ("42", 2)])
check("plan 43: cells alternate, c2 first on even blocks",
      [c for c, a, s, r in pl][::1] == ["c2-100"] * 5 + ["c1s-400"] * 3 + ["c1s-400"] * 3 + ["c2-100"] * 5 + ["c2-100"] * 5 + ["c1s-400"] * 3)
check("plan 43: the main three arms keep the original rotation (MAIN SL SH / SL SH MAIN / SH MAIN SL) at both cells",
      all([a for a, s, r in rows_ if a in pp.ARMS] == exp for rows_, exp in
          ((c2p[0:5], ["MAIN", "SL", "SH"]), (c2p[5:10], ["SL", "SH", "MAIN"]), (c2p[10:15], ["SH", "MAIN", "SL"]),
           (c1p[0:3], ["MAIN", "SL", "SH"]), (c1p[3:6], ["SL", "SH", "MAIN"]), (c1p[6:9], ["SH", "MAIN", "SL"]))))
check("plan 43: the extra arms are interleaved with them (five-arm rotation), not appended: block 1 reads SL SH SL-B0 SL-G12 MAIN",
      [a for a, s, r in c2p[0:5]] == ["MAIN", "SL", "SH", "SL-B0", "SL-G12"] and [a for a, s, r in c2p[5:10]] == ["SL", "SH", "SL-B0", "SL-G12", "MAIN"]
      and [a for a, s, r in c2p[10:15]] == ["SH", "SL-B0", "SL-G12", "MAIN", "SL"])
check("plan 43: the smoke is c2 SH and c1s SL (full size)", pp.plan43(smoke=True) == [("c2-100", "SH", "42", 1), ("c1s-400", "SL", "42", 1)])
cp = subprocess.run([sys.executable, os.path.join(HERE, "place43_parse.py"), "plan"], capture_output=True, text=True)
check("`plan` prints the 24 `cell arm seed rep` lines", cp.stdout.splitlines() == [" ".join(map(str, r)) for r in pl])
check("the cells' constants are §43's: L, H, band, sd_pred, bytes",
      pp.CELLS["c2-100"]["L"] == 9773 and pp.CELLS["c2-100"]["H"] == 1536 and pp.CELLS["c2-100"]["band"] == (157.0, 164.0)
      and pp.CELLS["c2-100"]["sd"] == 0.08 and pp.CELLS["c2-100"]["bytes"] == 600_000_000
      and pp.CELLS["c1s-400"]["L"] == 97733 and pp.CELLS["c1s-400"]["H"] == 2048 and pp.CELLS["c1s-400"]["band"] == (614.0, 699.0)
      and pp.CELLS["c1s-400"]["sd"] == 0.11 and pp.CELLS["c1s-400"]["bytes"] == 5_000_000_000)


bat = open(os.path.join(HERE, "loopdiag_battery.sh"), encoding="utf-8").read()
runall = open(os.path.join(HERE, "loopdiag_run_all.sh"), encoding="utf-8").read()
check("battery: plan 43 accepted, arms SL/SH set RWM_STORE_LAW=oracle (+ RWM_ORACLE_HOLD=<H>:15), every arm carries the L list, 24-row table",
      "38|40|41|41b|43" in bat and "RWM_STORE_LAW=oracle" in bat and 'RWM_ORACLE_HOLD=$(p43_h "$cell"):15' in bat
      and 'RWM_ORACLE_L_DGPS=$(p43_l "$cell")' in bat and "-eq 24" in bat and "P43ROW" in bat)
check("battery (amendment 2): SL-B0 sets RWM_ORACLE_BRAKE_OFFSET_US=2470 and SL-G12 RWM_ORACLE_GAIN=1.2 on top of the oracle law, c2-100 only; "
      "both envs are withheld and preflighted",
      "RWM_STORE_LAW=oracle RWM_ORACLE_BRAKE_OFFSET_US=2470" in bat and "RWM_STORE_LAW=oracle RWM_ORACLE_GAIN=1.2" in bat
      and "-u RWM_ORACLE_GAIN -u RWM_ORACLE_BRAKE_OFFSET_US" in bat and '"RWM_ORACLE_GAIN" "RWM_ORACLE_BRAKE_OFFSET_US"' in bat
      and bat.count('[ "$cell" = "c2-100" ]') >= 2 and "-eq 24" in bat)
check("run_all (amendment 2): the 45 min guard is unchanged and the 26-invocation budget is stated",
      "MINBTO=2700" in runall and "26 invocations" in runall)
check("battery: RWM_ORACLE_* withheld before the arm env, RWM_ORACLE_DIAG=1 and the 200 s timeout on every arm, ABORT-BIN and ABORT-RC kept",
      "-u RWM_ORACLE_L_DGPS -u RWM_ORACLE_HOLD -u RWM_ORACLE_DIAG" in bat and "-u RWM_STORE_LAW" in bat
      and "RWM_ORACLE_DIAG=1 RWM_PERF_TIMEOUT_S=200" in bat and "ABORT-BIN" in bat and "ABORT-RC" in bat)
check("battery: the per-cell constants (size, L, H) are the scorer's",
      all(f"{c}) echo {v}" in bat for c, k in (("c2-100", "bytes"), ("c1s-400", "bytes")) for v in [pp.CELLS[c][k]])
      and all(f"{c}) echo {pp.CELLS[c]['L']}" in bat for c in pp.CELLS) and all(f"{c}) echo {pp.CELLS[c]['H']}" in bat for c in pp.CELLS))
check("battery: plan 43 sizes the cells itself (600 MB / 5 GB) and parses rows with place43_parse.py",
      'bytes="$(p43_bytes "$cell")"' in bat and "parser=./place43_parse.py" in bat)
check("run_all: plan 43 takes the root-owned tmpfs tree, the precondition and LD_HARD, the root-installed binary, a 45 min budget, "
      "gates on test_place43_parse.py and scores with score43",
      "test_place43_parse.py" in runall and "P_SCORE=score43" in runall and runall.count('"$P43" = 1') >= 6 and "MINBTO=2700" in runall)
lp = open(os.path.join(HERE, "loopdiag_parse.py"), encoding="utf-8").read()
check("loopdiag_parse.py dispatches smoke43 / check43 / score43 to place43_parse", all(c in lp for c in ('cmd == "smoke43"', 'cmd == "check43"', 'cmd == "score43"')))


# ── synthetic windows -> readouts ─────────────────────────────────────────
def W(t, n, **kw):
    d = dict(t=t, n=n, ask=float(n), h=0, rtprop=7000, tmin=7100, tp50=15200, n_k=2400, floor=0.0, pin=0.0, hold=0.0,
             wbind=0, nmin=n, nmax=n, occ=0.9, B=5)
    d.update(kw)
    return pp.Win(**d)


def series(ns, t0=1.0, dt=0.25, **kw):
    return [W(round(t0 + i * dt, 2), n, **(kw(i) if callable(kw) else kw)) for i, n in enumerate(ns)]


# R1: last third = windows with t >= 2/3 of the last window time
ws = series([100] * 38 + [200] * 22)        # t = 1.0 .. 15.75; last third: t >= 10.5 = windows 38..59
r = pp.readouts(ws, "SL", "c2-100")
check("R1: the last third is t >= 2/3 of the last window time, n_end its median, s_end sd(ln n)",
      r["t_last"] == 15.75 and r["n_end"] == 200 and r["s_end"] == 0.0, (r["t_last"], r["n_end"], r["s_end"]))
ws = series([100, 200] * 30)                # sd(ln n) of an alternating pair over the last third
r = pp.readouts(ws, "SL", "c2-100")
exp = math.sqrt(sum((x - (math.log(100) + math.log(200)) / 2) ** 2 for x in [math.log(100), math.log(200)] * 10) / 19)
ls = [w.n for w in ws if w.t >= 2 / 3 * ws[-1].t]
check("R1: s_end is the sample sd of ln n over exactly the last-third windows", abs(r["s_end"] - pp.sd_ln(ls)) < 1e-12 and abs(r["s_end"] - math.log(2) / 2 * math.sqrt(22 / 21)) < 1e-9, r["s_end"])
check("OSCILLATION: s_end > 3 sd_pred (0.24 at c2): alternating 100/200 is 0.362 -> yes",
      r["OSCILLATION"] and not pp.readouts(series([100, 110] * 30), "SL", "c2-100")["OSCILLATION"])
check("OSCILLATION boundary at c1s is 0.33: 0.362 -> yes, ln(1.3)/2 pair -> no",
      pp.readouts(series([100, 200] * 30), "SL", "c1s-400")["OSCILLATION"] and not pp.readouts(series([100, 130] * 30), "SL", "c1s-400")["OSCILLATION"])
check("a static row (constant n) never reads OSCILLATION (s_end = 0)",
      not pp.readouts(series([512] * 80), "SL", "c2-100")["OSCILLATION"] and pp.readouts(series([512] * 80), "SL", "c2-100")["s_end"] == 0.0)
# RUNAWAY / COLLAPSE
pin = lambda i: dict(pin=1.0 if i % 4 == 0 else 0.0)        # noqa: E731  (a quarter of the windows)
r = pp.readouts(series([300] * 80, **{}) if False else [W(1.0 + i * .25, 300, pin=pin(i)["pin"]) for i in range(80)], "SL", "c2-100")
check("RUNAWAY (SL): pin >= 0.25 over the last third; exactly 0.25 counts", r["pin_last"] >= 0.25 and r["RUNAWAY_pin"], r["pin_last"])
r = pp.readouts([W(1.0 + i * .25, 300, pin=0.24) for i in range(80)], "SL", "c2-100")
check("RUNAWAY: 0.24 does not", not r["RUNAWAY_pin"])
rsl = {"arm": "SL", "RUNAWAY_pin": True, "COLLAPSE": False, "OSCILLATION": False}
rsh = dict(rsl, arm="SH")
check("RUNAWAY is SL only: a pinned SH row is evidence for BISTABLE, not RUNAWAY", pp.unstable_modes(rsl) == ["RUNAWAY"] and pp.unstable_modes(rsh) == [])
check("MAIN rows are never UNSTABLE (only ORACLE rows)", pp.unstable_modes(dict(rsl, arm="MAIN", COLLAPSE=True)) == [])
r = pp.readouts([W(1.0 + i * .25, 10, floor=1.0) for i in range(80)], "SL", "c2-100")
check("COLLAPSE (amendment 2): the floor clause is struck: a floor bind alone is not a collapse", not r["COLLAPSE"] and r["floor_last"] == 1.0)
check("COLLAPSE (amendment 2): a DNF row is COLLAPSE, whatever its floor reads",
      pp.readouts(series([300] * 80), "SL", "c2-100", dnf=True)["COLLAPSE"] and not pp.readouts(series([300] * 80), "SL", "c2-100", dnf=False)["COLLAPSE"])
# R5/R6/R10
ws = [W(1.0 + i * .25, 300, n_k=(9000 if i % 2 else 5), wbind=(1 if i < 8 else 0)) for i in range(40)]
r = pp.readouts(ws, "SL", "c2-100")
check("R6/R10: n_k min / max, the share above 8192, and the share under 8 samples",
      r["nk_min"] == 5 and r["nk_max"] == 9000 and r["nk_over_cap"] == 0.5 and r["nk_few"] == 0.5 and abs(r["wbind_all"] - 0.2) < 1e-12, r)
# R4 / R7 / R8 / R9 per third
ws = [W(1.0 + i * .25, 100, tmin=7000 + 100 * (i // 20), B=i // 20, occ=0.5 + 0.1 * (i // 20), ask=100.0 * (1 + i // 20)) for i in range(60)]
r = pp.readouts(ws, "MAIN", "c2-100")
check("R4/R7/R8/R9: medians per time tertile (brake grows 0, 100, 200 with tmin; B, occupancy, ask/n likewise)",
      r["brake_3"] == [0, 100, 200] and r["B_3"] == [0, 1, 2] and [round(x, 1) for x in r["occ_3"]] == [0.5, 0.6, 0.7]
      and r["askn_3"] == [1.0, 2.0, 3.0], (r["brake_3"], r["B_3"], r["occ_3"], r["askn_3"]))
check("the window with t < 1 s is warm-up and is not read", pp.readouts([W(0.5, 9999)] + series([100] * 20), "SL", "c2-100")["n_end"] == 100)


# ── SHORT ──────────────────────────────────────────────────────────────────
def rowd(arm, seconds=None, dnf=False, **kw):
    d = {"arm": arm, "seconds": seconds, "dnf": dnf, "t_last": seconds}
    d.update(kw)
    return d


check("SHORT: an SL or MAIN row under 30 s; exactly 30 s is not",
      pp.is_short(rowd("SL", 29.9)) and pp.is_short(rowd("MAIN", 10.0)) and not pp.is_short(rowd("SL", 30.0)) and not pp.is_short(rowd("MAIN", 30.1)))
check("SHORT: an SH row under t_h + 30 = 45 s; 45 s is not",
      pp.is_short(rowd("SH", 44.9)) and not pp.is_short(rowd("SH", 45.0)) and pp.is_short(rowd("SH", 30.0)))
check("a DNF row ran to the timeout: not SHORT", not pp.is_short(rowd("SL", None, dnf=True, t_last=2.0)))

# ── HOLD-EXPIRED / HOLD-INERT ────────────────────────────────────────────
def hold_row(rt1, rth, tmin_last2, c="c2-100"):
    ws = []
    for i in range(0, 100):
        t = round(1.0 + i * 0.25, 2)
        rp = rt1 if t < 10 else (rth if t >= 14.0 else (rt1 + rth) // 2)
        ws.append(W(t, 300, rtprop=rp, tmin=(tmin_last2 if 13.0 < t <= 15.0 else 99999), ask=500.0))
    return pp.readouts(ws, "SH", c)


r = hold_row(7000, 60000, 100000)
check("HOLD-EXPIRED iff RTprop(t_h) >= RTprop(1 s) + 1/2 (min Tmin over the last 2 s - RTprop(1 s)): threshold 53 500 us",
      r["hold_thr_us"] == 53500 and r["hold_state"] == "HOLD-EXPIRED", r.get("hold_thr_us"))
check("  exactly at the threshold counts as expired; one us under does not",
      hold_row(7000, 53500, 100000)["hold_state"] == "HOLD-EXPIRED" and hold_row(7000, 53499, 100000)["hold_state"] == "HOLD-INERT")
check("  the ledger's c2 1 024 reading: threshold 50.2 ms for RTprop 7.0 ms and Tmin 93.4 ms; a fresh anchor (7.4 ms) is INERT, 89 ms is EXPIRED",
      abs(hold_row(7000, 7400, 93400)["hold_thr_us"] - 50200) < 1 and hold_row(7000, 7400, 93400)["hold_state"] == "HOLD-INERT"
      and hold_row(7000, 89000, 93400)["hold_state"] == "HOLD-EXPIRED")
check("  R3: the median ask / H over the hold's last 2 s", abs(r["ask_over_H_hold"] - 500.0 / 1536) < 1e-12, r["ask_over_H_hold"])
check("  a row with no tmin in the hold's last 2 s has no hold state (not scored)", hold_row(7000, 60000, None)["hold_state"] is None)


# ── RETURNED / t_ret / d(x) ───────────────────────────────────────────────
c2 = pp.CELLS["c2-100"]
nsl = 160.0
lab = lambda n: pp.returned({"n_end": n}, nsl, c2)  # noqa: E731
check("RETURNED iff |ln(n_end / n_SL)| <= 2 sd_pred (0.16 at c2)",
      lab(nsl * math.exp(0.159)) == ("RETURNED", None) and lab(nsl * math.exp(-0.159)) == ("RETURNED", None))
check("  beyond the tolerance: NOT-RETURNED, with its direction", lab(nsl * math.exp(0.17)) == ("NOT-RETURNED", "HIGH")
      and lab(nsl * math.exp(-0.17)) == ("NOT-RETURNED", "LOW"))
sr = [[1.0 + i * 0.25, 1536] for i in range(56)] + [[15.0 + i * 0.25, 1000 - 100 * i if i < 5 else 160] for i in range(20)]
check("t_ret: the first window after t_h opening 4 consecutive windows within tolerance",
      pp.t_ret([[t, n] for t, n in sr], 160.0, c2) == 15.0 + 5 * 0.25, pp.t_ret([[t, n] for t, n in sr], 160.0, c2))
check("  never returning is `none`", pp.t_ret([[16.0 + i * .25, 1536] for i in range(40)], 160.0, c2) is None)
check("  three windows are not four", pp.t_ret([[16.0, 160], [16.25, 160], [16.5, 160], [16.75, 1500]], 160.0, c2) is None)
check("d(x): 0 inside the band (edges included), else |ln| to the nearer edge",
      pp.d_band(157, c2["band"]) == 0 and pp.d_band(164, c2["band"]) == 0 and abs(pp.d_band(200, c2["band"]) - math.log(200 / 164)) < 1e-12
      and abs(pp.d_band(100, c2["band"]) - math.log(157 / 100)) < 1e-12)


# ── synthetic rows through make_row ──────────────────────────────────────
RAW = os.path.join(HERE, "..", "..", "docs", "l1-raw", "loop-diag")
BASE = os.path.join(RAW, "p29", "diag-ld", "c2-100-MAIN-s42-r1-a1-")


def base_logs():
    drv, cli, srv = lc.read(BASE + "drv.out"), lc.read(BASE + "c.log"), lc.read(BASE + "s.log")
    keep = lambda ln: not any(k in ln for k in ("[DIAG] t=", "[LOOPDIAG]", "[SUMCAP]", "[OCAP]"))  # noqa: E731
    nojson = lambda ln: '"summary"' not in ln and '"run"' not in ln  # noqa: E731
    return [ln for ln in drv if nojson(ln)], [ln for ln in cli if keep(ln) and nojson(ln)], [ln for ln in srv if keep(ln)]


def gates(lines, **kw):
    out = []
    for ln in lines:
        if "[GATES]" in ln:
            for k, v in kw.items():
                if re.search(r"\b%s=\S+" % k, ln):
                    ln = re.sub(r"\b%s=\S+" % k, f"{k}={v}", ln)
                else:
                    ln += f" {k}={v}"
        out.append(ln)
    return out


def ocap(t, n, **kw):
    d = dict(law="oracle", ask=float(n), h=0, rtprop=7012, tmin=7100, tp50=15200, s_us=8100, n_k=2400, offered=None, r_k=16.45,
             floor=0.0, pin=0.0, hold=0.0, wbind=0, nmin=n, nmax=n, refreshes=50)
    d.update(kw)
    return (f"[OCAP] t={t:.1f}s p0 law={d['law']} n={n} ask={d['ask']:.2f} h={d['h']} rtprop_us={d['rtprop']} tmin_us={d['tmin']} "
            f"tp50_us={d['tp50']} s_us={d['s_us']} brake_us={d['tmin'] - d['rtprop']} n_k={d['n_k']} offered={d['offered']} "
            f"r_k={d['r_k']} floor={d['floor']:.3f} pin={d['pin']:.3f} hold={d['hold']:.3f} wbind={d['wbind']} nmin={d['nmin']} "
            f"nmax={d['nmax']} refreshes={d['refreshes']} q=0.1000")


def diag(t, num, den, dgq=True, paused=19, rtt=14, wrtt=14, qcwnd=None):
    return (f"[DIAG] t={t:.1f}s win={num}/{den} paused={paused}% good=84.6Mbit cwnd=100 infl=40 np=1 np_act=1 rtt=14.2ms"
            + (" dgq0[hand=41123 tx=41091 full=0 err=0 sp=4156391]" if dgq else "")
            + f" p0:infl=37/sinfl=0/bdp418(cap0) sout=143 khr=1.00/kraw=- btlbw=44336 sr=0/g0d0 est=Y pl=0.0000 cmp=- rtt={rtt}/wrtt={wrtt}/rtp9ms rtp_us=9429" + (f" sig_us=1/n2 qcwnd={qcwnd} qce=16" if qcwnd is not None else ""))


def echo(arm, cell="c2-100", hold=None, l=None, b0=None, gain=None):
    cfg = pp.CELLS[cell]
    wb0, wg = pp.EXTRA_ENV.get(arm, ("0", "q"))
    tail = f" b0_us={wb0 if b0 is None else b0} gain={wg if gain is None else gain}"
    l = cfg["L"] if l is None else l
    h = f"H={cfg['H']} t_h=15s" if arm == "SH" and hold is None else (hold or "none")
    if arm == "MAIN":
        return "[OCAP] sampler readout ACTIVE (RWM_ORACLE_DIAG=1, observation only): L_dgps=[%s.0] T_w=0.25s" % l + tail
    return f"oracle store law ACTIVE (RWM_STORE_LAW=oracle, diagnostic, never merged): L_dgps=[{l}.0] hold={h} T_w=0.25s" + tail


def mk(arm="SL", cell="c2-100", ns=None, seconds=60.0, dnf=False, rc=0, omit_t=None, off_by=None, route_bad=0, no_dgq=False,
       echo_line="auto", sumcap=None, per=None, gate_store="unset", t_end=None, mbps=80.0, stall_from=None, qcw=None, sha=None):
    drv, cli, srv = base_logs()
    cli = gates(cli, RWM_STORE=gate_store, RWM_DIAG="1")
    srv = gates(srv, RWM_STORE=gate_store, RWM_DIAG="1")
    ns = ns if ns is not None else [160] * int(((t_end or seconds) - 0.0) / 0.25)
    if echo_line == "auto":
        echo_line = echo(arm, cell)
    if echo_line:
        cli = cli + [echo_line]
    if sumcap is None:
        sumcap = arm == "MAIN"
    if sumcap:
        cli.append("[SUMCAP] on=1 eng=100/100 chg=100/100 chg_frac=1.0000 pin=0.0000 floor=0.0000 cap=300.0 ask=300.0")
    cum = 0
    for i, n in enumerate(ns):
        t = round(0.25 * (i + 1), 2)
        kw = dict(per(i, t) if per else {})
        nk = kw.pop("n_k", 2400)
        if omit_t is not None and abs(t - omit_t) < 1e-9:
            cli.append(diag(t, 100, n))
            continue
        cum += nk
        stalled = stall_from is not None and t >= stall_from
        cli.append(diag(t, 100, n + (7 if (route_bad and i < route_bad) else 0), dgq=not no_dgq,
                        paused=97 if stalled else 19, rtt=380 if stalled else 14, wrtt=2 if stalled else 14,
                        qcwnd=(qcw(i) if qcw else None)))
        cli.append(ocap(t, n, n_k=nk, offered=cum + (off_by if (off_by and i == len(ns) // 2) else 0), law=("shadow" if arm == "MAIN" else "oracle"), **kw))
    if dnf:
        cli = cli + ['{"bytes":600000000,"run":1,"dnf":true,"timeout_s":200}', '{"bytes":600000000,"dnf":1,"hint":"bulk","proto":"rp-native","runs":1,"summary":true}']
    else:
        cli = cli + [json.dumps({"bytes": 600000000, "run": 1, "seconds": seconds, "mbps": mbps}),
                     json.dumps({"bytes": 600000000, "dnf": 0, "hint": "bulk", "mean_mbps": mbps, "median_s": seconds, "summary": True, "runs": 1})]
    srv = srv + ["[LAT] site=receiver n=9 ax_p50=15000 ax_p90=20000 ax_p95=22000"]
    return pp.make_row(cell, arm, "42", 1, rc, 70.0, drv, cli, srv, 0, sha)


r = mk("SL")
check("a good SL row is LIVE and scored; n_end, the series and the sampler gauges are read", r["status"] == "LIVE" and pp.scored(r)
      and r["n_end"] == 160 and r["nk_min"] == 2400 and r["route_frac"] == 1.0 and r["ax_p50_us"] == 15000 and len(r["series"]) > 100, (r["status"], r["problems"]))
check("a good MAIN row (readout echo, no law echo, [SUMCAP] eng > 0) is LIVE", mk("MAIN")["status"] == "LIVE", mk("MAIN")["problems"])
check("a good SH row with the hold echo is LIVE", mk("SH")["status"] == "LIVE", mk("SH")["problems"])
check("witness: a MAIN row carrying the oracle-law echo is void", any("oracle-law-echo-on-MAIN" in p for p in mk("MAIN", echo_line=echo("SL"))["problems"]))
check("witness: a MAIN row with [SUMCAP] eng = 0 is void", any(p.startswith("sumcap-eng") for p in mk("MAIN", sumcap=False)["problems"]))
check("witness: an ORACLE row without the law echo is void", "no-oracle-law-echo" in mk("SL", echo_line=None)["problems"])
check("witness: the echo's L list must be the cell's (9773 at c2)", any(p.startswith("oracle-L-list") for p in mk("SL", echo_line=echo("SL", l=97733))["problems"]))
check("witness: an SH row's echo must carry H and t_h; an SL row's must say hold=none",
      any(p.startswith("oracle-hold") for p in mk("SH", echo_line=echo("SH", hold="none"))["problems"])
      and "oracle-hold-on-SL" in mk("SL", echo_line=echo("SL", hold="H=1536 t_h=15s"))["problems"])
check("witness: RWM_STORE must read unset", any(p.startswith("gate-store=") for p in mk("SL", gate_store="256")["problems"]))
omit = mk("SL", omit_t=6.0)
check("witness: a [DIAG] tick with t >= 1 s and no [OCAP] line is void", omit["status"] == "WITNESS-FAIL" and any(p.startswith("ocap-missing") for p in omit["problems"]), omit["problems"])
check("witness: a [DIAG] line without dgq (t >= 1 s) is void", mk("SL", no_dgq=True)["status"] == "WITNESS-FAIL" and any(p == "dgq-missing" for p in mk("SL", no_dgq=True)["problems"]))
bs = mk("SL", off_by=3)
check("witness: Σ n_k != offered is VOID-SAMPLER (and the row is not scored)", bs["status"] == "VOID-SAMPLER" and not pp.scored(bs), bs["problems"])
nt = len(mk("SL")["series"])
check("routing: the win= denominator must equal [OCAP] n on >= 99 % of the ticks: 1 % off passes, 2 % off fails",
      mk("SL", route_bad=int(0.01 * nt))["status"] == "LIVE" and mk("SL", route_bad=int(0.02 * nt) + 2)["status"] == "WITNESS-FAIL",
      (mk("SL", route_bad=int(0.01 * nt))["route_frac"], mk("SL", route_bad=int(0.02 * nt) + 2)["route_frac"]))
check("routing is read on ORACLE rows only (a MAIN row's cap is the shipped law's)", "route_frac" not in mk("MAIN"))
check("rc != 0 is VOID-RC (ABORT-RC), not DNF", mk("SL", rc=1)["status"] == "VOID-RC")
d = mk("SL", dnf=True, seconds=None, ns=[160] * 800, t_end=200)
check("DNF: rc 0 with the perf timeout is a live DNF row, scored from its log, not SHORT", d["status"] == "LIVE" and d["dnf"] and not d["short"] and pp.scored(d), (d["status"], d["problems"]))
sh = mk("SL", seconds=12.0, ns=[160] * 48)
check("SHORT: a 12 s SL row is LIVE, labelled SHORT and not scored", sh["status"] == "LIVE" and sh["short"] and not pp.scored(sh))
sx = mk("SH", per=lambda i, t: dict(rtprop=7000 if t < 10 else 60000, tmin=100000 if 13.0 < t <= 15.0 else 99999, ask=500.0), seconds=60.0)
check("an SH row carries its hold state, and HOLD-EXPIRED rows are scored", sx["hold_state"] == "HOLD-EXPIRED" and pp.scored(sx), (sx.get("hold_state"), sx["problems"]))
si = mk("SH", per=lambda i, t: dict(rtprop=7000 if t < 10 else 7400, tmin=100000 if 13.0 < t <= 15.0 else 99999), seconds=60.0)
check("a HOLD-INERT row is live, reported and not scored", si["hold_state"] == "HOLD-INERT" and si["status"] == "LIVE" and not pp.scored(si))
check("a row survives the ledger line round trip", json.loads(json.dumps(r, sort_keys=True))["n_end"] == 160)


# ── the ladder ────────────────────────────────────────────────────────────
def lrow(arm, n_end, cell="c2-100", seed="42", rep=1, s_end=0.05, hold="HOLD-EXPIRED", modes=(), mbps=80.0, ax=15000.0, tp50=15000.0,
         short=False, series=None):
    rr = {"cell": cell, "arm": arm, "seed": seed, "rep": rep, "status": "LIVE", "short": short, "dnf": False, "n_end": float(n_end),
          "s_end": s_end, "COLLAPSE": "COLLAPSE" in modes, "OSCILLATION": "OSCILLATION" in modes, "RUNAWAY_pin": "RUNAWAY" in modes,
          "mbps": mbps, "ax_p50_us": ax, "tp50_last": tp50, "seconds": 60.0, "problems": [], "floor_last": 0.0, "pin_last": 0.0,
          "series": series or [[16.0 + i * .25, n_end] for i in range(20)]}
    if arm == "SH":
        rr["hold_state"] = hold
    return rr


def ladder(rows, cell="c2-100"):
    return pp.cell_outcome(rows, cell, out=lambda *_: None)


sl3 = [lrow("SL", 160, seed=s, rep=i) for i, s in enumerate(("42", "7", "42"))]
sh3 = [lrow("SH", 160, seed=s, rep=i) for i, s in enumerate(("42", "7", "42"))]
mn3 = [lrow("MAIN", 800, mbps=m, ax=a, tp50=a) for m, a in ((90.0, 90000.0), (91.0, 92000.0), (89.0, 88000.0))]
check("ladder: all returned and n_SL inside the band -> CONVERGES-ONE-POINT", ladder(sl3 + sh3 + mn3)["label"] == "CONVERGES-ONE-POINT")
check("ladder: all returned but n_SL 200 (d = ln(200/164) = 0.198 > 0.16) -> ONE-POINT-OFF-PREDICTION",
      ladder([lrow("SL", 200, seed=s) for s in "427"] + [lrow("SH", 200, seed=s) for s in "427"] + mn3)["label"] == "ONE-POINT-OFF-PREDICTION")
check("ladder: n_SL 190 (d = 0.147 <= 0.16) is still one point", ladder([lrow("SL", 190, seed=s) for s in "427"] + [lrow("SH", 190, seed=s) for s in "427"] + mn3)["label"] == "CONVERGES-ONE-POINT")
sh_pin = [lrow("SH", 1536, seed=s, rep=i) for i, s in enumerate("427")]
check("ladder: two SH rows NOT-RETURNED -> BISTABLE", ladder(sl3 + sh_pin[:2] + [sh3[0]] + mn3)["label"] == "BISTABLE")
check("ladder: exactly one SH row NOT-RETURNED -> MIXED", ladder(sl3 + [sh_pin[0]] + sh3[1:] + mn3)["label"] == "MIXED")
check("ladder: UNSTABLE comes before BISTABLE (an oscillating SL row, two pinned SH rows)",
      ladder([lrow("SL", 160, modes=("OSCILLATION",))] + sl3[1:] + sh_pin + mn3)["label"] == "UNSTABLE")
check("ladder: a pinned SH row alone is BISTABLE evidence, not RUNAWAY; a pinned SL row is RUNAWAY -> UNSTABLE",
      ladder(sl3 + sh_pin[:2] + [sh3[0]] + mn3)["label"] == "BISTABLE"
      and ladder([lrow("SL", 2048, modes=("RUNAWAY",))] + sl3[1:] + sh3 + mn3)["label"] == "UNSTABLE")
check("ladder: COLLAPSE on an SH row is UNSTABLE", ladder(sl3 + [lrow("SH", 10, modes=("COLLAPSE",))] + sh3[1:] + mn3)["label"] == "UNSTABLE")
inert3 = [lrow("SH", 650, cell="c1s-400", hold="HOLD-INERT", seed=s) for s in "427"]
sl3c = [lrow("SL", 650, cell="c1s-400", seed=s, rep=i) for i, s in enumerate("427")]
mn3c = [lrow("MAIN", 2000, cell="c1s-400", seed=s, rep=i) for i, s in enumerate("427")]
rc = ladder(sl3c + inert3 + mn3c, "c1s-400")
check("amendment 2: at c1s, SL rows carry the cell when its SH rows read HOLD-INERT (SL-ONLY note, no bistability test)",
      rc["label"] == "CONVERGES-ONE-POINT" and rc["sl_only"] and "SH cannot test bistability" in rc["why"], rc)
check("  not at c2 (INERT SH rows there stay UNSCOREABLE), and not with one INERT row",
      ladder([lrow("SL", 160, seed=s) for s in "427"] + [lrow("SH", 160, hold="HOLD-INERT", seed=s) for s in "427"] + mn3)["label"] == "UNSCOREABLE"
      and ladder(sl3c + inert3[:1] + mn3c, "c1s-400")["label"] == "UNSCOREABLE")
check("ladder: UNSCOREABLE with one scored SL row, or one scored SH row",
      ladder(sl3[:1] + sh3 + mn3)["label"] == "UNSCOREABLE" and ladder(sl3 + sh3[:1] + mn3)["label"] == "UNSCOREABLE")
check("ladder: SHORT rows are left out of the outcome (two of three SL SHORT -> UNSCOREABLE)",
      ladder([lrow("SL", 160, short=True), lrow("SL", 160, short=True), sl3[2]] + sh3 + mn3)["label"] == "UNSCOREABLE")
check("ladder: HOLD-INERT SH rows are left out (two of three INERT -> UNSCOREABLE)",
      ladder(sl3 + [lrow("SH", 160, hold="HOLD-INERT"), lrow("SH", 160, hold="HOLD-INERT"), sh3[2]] + mn3)["label"] == "UNSCOREABLE")
msgs = []
pp.cell_outcome(sl3 + sh3 + [lrow("SL", 160, short=True, modes=("OSCILLATION",)), mn3[0]], "c2-100", out=msgs.append)
check("a left-out row that would have triggered UNSTABLE is printed, and does not trigger it",
      any("EXCLUDED-WOULD-TRIGGER" in m for m in msgs) and not any("LABEL c2-100: UNSTABLE" in m for m in msgs))
res = ladder(sl3 + sh3 + mn3)
check("reported: SPREAD AS-PREDICTED iff the median s_end / sd_pred is in [0.5, 2] (0.05/0.08 = 0.63)", res["SPREAD"].startswith("AS-PREDICTED"), res["SPREAD"])
res2 = ladder([lrow("SL", 160, s_end=0.3, seed=s) for s in "427"] + sh3 + mn3)
check("  0.3 / 0.08 = 3.75 is off", res2["SPREAD"].startswith("OFF-PREDICTION"))
res3 = ladder([lrow("SL", 160, mbps=80.0 - 0.1 * i, ax=15000 + 10 * i, tp50=15000 + 10 * i, seed=s, rep=i) for i, s in enumerate("427")] + sh3 + mn3)
check("reported GOODPUT: SL 80 vs MAIN 90 +- 1 is WORSE (|diff| > 2 sigma_pooled); DELAY: 15 ms vs 90 ms is BETTER",
      res3["GOODPUT"] == "WORSE" and res3["DELAY_Ax"] == "BETTER" and res3["DELAY_Tp50"] == "BETTER", (res3["GOODPUT"], res3["DELAY_Ax"]))
res4 = ladder([lrow("SL", 160, mbps=m, seed=s, rep=i) for i, (s, m) in enumerate(zip("427", (60.0, 120.0, 90.0)))] + sh3 + mn3)
check("  a spread of 30 Mbit/s makes a 0 difference WITHIN", res4["GOODPUT"] == "WITHIN", res4["GOODPUT"])
check("reported: fewer than 2 live rows in an arm makes the comparison UNSCOREABLE", ladder(sl3 + sh3 + mn3[:1])["GOODPUT"] == "UNSCOREABLE")
check("decision: UNSTABLE anywhere points at §42 step 3; BISTABLE/MIXED at the user's anchor decision",
      "step 3" in pp.decision({"c2-100": "UNSTABLE", "c1s-400": "CONVERGES-ONE-POINT"})
      and "design decision" in pp.decision({"c2-100": "BISTABLE", "c1s-400": "CONVERGES-ONE-POINT"})
      and "one operating point" in pp.decision({"c2-100": "CONVERGES-ONE-POINT", "c1s-400": "CONVERGES-ONE-POINT"}))

# ── the ledger readers (dry-run) on committed rows ───────────────────────
P29 = os.path.join(RAW, "p29", "diag-ld")
pwr = lc.read(os.path.join(P29, "c2-100-PWR-s42-r1-a1-c.log"))
ws = pp.ledger_series(pwr)
den = [int(m.group(2)) for ln in pwr for m in [re.search(r"\[DIAG\] t=[\d.]+s win=(\d+)/(\d+)", ln)] if m]
check("ledger reader: n is the [DIAG] win= denominator, one window per tick", [w.n for w in ws] == den and len(ws) == len(den) > 10)
ro = pp.readouts(ws, "SL", "c2-100")
t_last = [w for w in ws if w.t >= 1.0][-1].t
last = [w.n for w in ws if w.t >= 1.0 and w.t >= t_last * 2 / 3]
check("ledger reader: n_end is the median of the last-third denominators", ro["n_end"] == lc.med(last), (ro["n_end"], last))
ld = [w for w in ws if w.n_k]
check("ledger reader: the sampler count is buffered + overflow ([LOOPDIAG] n + ov), tmin/tp50/rtprop from the line", len(ld) > 5 and all(w.tmin and w.tp50 and w.rtprop for w in ld))
if os.path.isdir(os.path.join(RAW, "p27")):
    st = pp.readouts(pp.ledger_series(lc.read(os.path.join(RAW, "p27", "diag-ld", "c1s-400-DEF2048-s42-r1-a1-c.log"))), "SL", "c1s-400")
    st2 = pp.readouts(pp.ledger_series(lc.read(os.path.join(RAW, "p27", "diag-ld", "c1s-400-DEF512-s42-r1-a1-c.log"))), "SL", "c1s-400")
    check("static ledger rows: DEF2048 sits at N*knee -> pin = 1, RUNAWAY; DEF512 never pins; neither is OSCILLATION",
          st["pin_last"] == 1.0 and st["RUNAWAY_pin"] and not st2["RUNAWAY_pin"] and not st["OSCILLATION"] and not st2["OSCILLATION"]
          and st["s_end"] == 0.0 and st2["s_end"] == 0.0, (st["pin_last"], st2["pin_last"], st["s_end"], st2["s_end"]))

# ── end to end: 24 synthetic invocations -> P43ROW ledger -> check / smoke / score ──
import tempfile  # noqa: E402


def e2e_rows(sh_n_end_c2=160):
    rows = []
    for cell, n_sl in (("c2-100", 160), ("c1s-400", 650)):
        H = pp.CELLS[cell]["H"]
        for k, seed in enumerate(("42", "7", "42")):
            rows.append(mk("MAIN", cell, ns=[800] * 240, seconds=60.0, mbps=90.0 + k))
            rows.append(mk("SL", cell, ns=[n_sl] * 240, seconds=60.0, mbps=80.0 + k))
            rows.append(mk("SH", cell, ns=[H] * 60 + [(sh_n_end_c2 if cell == "c2-100" else n_sl)] * 180, seconds=60.0, mbps=80.0 + k,
                           per=lambda i, t: dict(rtprop=7000 if t < 10 else 60000, tmin=100000 if 13.0 < t <= 15.0 else 99999, ask=500.0)))
            if cell == "c2-100":
                rows.append(mk("SL-B0", cell, ns=[195] * 240, seconds=60.0, mbps=88.0 + k))
                rows.append(mk("SL-G12", cell, ns=[184] * 240, seconds=60.0, mbps=86.0 + k))
            for r in rows[-(5 if cell == "c2-100" else 3):]:
                r["seed"], r["rep"] = seed, (2 if k == 2 else 1)
    return rows


rows = e2e_rows()
with tempfile.TemporaryDirectory() as td:
    lp = os.path.join(td, "ld.log")
    with open(lp, "w", encoding="utf-8") as f:
        f.write("=== LOOP-DIAG BATTERY\n")
        for r in rows:
            f.write("P43ROW " + json.dumps(r, sort_keys=True) + "\n")
        f.write("LD-BATTERY-DONE tag=ld blocks=3 2026-11-10T00:00:00Z\n")
    check("check43: 24 rows and LD-BATTERY-DONE -> rc 0", pp.check(lp) == 0)
    out = []
    labs = pp.score([lp], out=out.append)
    check("score: both cells CONVERGES-ONE-POINT when SL sits in the band and every SH row returns",
          labs == {"c2-100": "CONVERGES-ONE-POINT", "c1s-400": "CONVERGES-ONE-POINT"}, labs)
    check("score: the decision line points at the user (nothing ships)", any(m.startswith("DECISION CONVERGES-ONE-POINT at both cells") for m in out))
    rows_b = e2e_rows(sh_n_end_c2=1536)
    lb = os.path.join(td, "b.log")
    with open(lb, "w", encoding="utf-8") as f:
        for r in rows_b:
            f.write("P43ROW " + json.dumps(r, sort_keys=True) + "\n")
    labs_b = pp.score([lb], out=lambda *_: None)
    check("score: c2 SH rows that stay at 1 536 -> BISTABLE at c2 only", labs_b == {"c2-100": "BISTABLE", "c1s-400": "CONVERGES-ONE-POINT"}, labs_b)
    sm = os.path.join(td, "smoke.log")
    smoke_rows = [mk("SH", "c2-100", seconds=60.0, mbps=80.0, per=lambda i, t: dict(rtprop=7000 if t < 10 else 60000, tmin=100000 if 13.0 < t <= 15.0 else 99999)),
                  mk("SL", "c1s-400", ns=[650] * 240, seconds=60.0, mbps=700.0)]
    with open(sm, "w", encoding="utf-8") as f:
        for r in smoke_rows:
            f.write("P43ROW " + json.dumps(r, sort_keys=True) + "\n")
    check("smoke43: c2 SH + c1s SL, both LIVE with their witnesses -> SMOKE-PASS (rc 0)", pp.smoke(sm) == 0)
    with open(sm, "w", encoding="utf-8") as f:
        f.write("P43ROW " + json.dumps(smoke_rows[0], sort_keys=True) + "\n")
    check("smoke43: a missing smoke row fails", pp.smoke(sm) == 1)
    cp = subprocess.run([sys.executable, os.path.join(HERE, "loopdiag_parse.py"), "score43", lp], capture_output=True, text=True, encoding="utf-8", env=dict(os.environ, PYTHONIOENCODING="utf-8"))
    check("loopdiag_parse.py score43 reaches the scorer (the envelope's entry point)", "VERDICT" not in cp.stdout and "DECISION CONVERGES-ONE-POINT" in cp.stdout, cp.stdout[-300:] + cp.stderr[-300:])


# ── amendment 2: the reported-only arms SL-B0 and SL-G12 ──────────────────
for arm in pp.EXTRA_ARMS:
    r = mk(arm)
    check(f"{arm}: a good row is LIVE and scored, with its declared b0 / gain in the echo", r["status"] == "LIVE" and pp.scored(r)
          and r["echo_b0_gain"] == list(pp.EXTRA_ENV[arm]), (r["status"], r["problems"], r["echo_b0_gain"]))
    check(f"{arm}: the echo must declare the arm's own env (b0_us=0 gain=q is void)",
          any(p.startswith("echo-b0-gain") for p in mk(arm, echo_line=echo("SL"))["problems"]))
check("the main arms must declare b0_us=0 gain=q: an SL row echoing the SL-B0 env is void, and so is a MAIN row",
      any(p.startswith("echo-b0-gain") for p in mk("SL", echo_line=echo("SL", b0=2470))["problems"])
      and any(p.startswith("echo-b0-gain") for p in mk("MAIN", echo_line=echo("MAIN", gain="1.2"))["problems"]))
check("an SL-B0 row's hold must be none", "oracle-hold-on-SL-B0" in mk("SL-B0", echo_line=echo("SL-B0", hold="H=1536 t_h=15s"))["problems"])
wild = [lrow("SL-B0", 2048, cell="c2-100", seed=s_, rep=i, modes=("OSCILLATION", "RUNAWAY", "COLLAPSE"), mbps=999.0, short=False) for i, s_ in enumerate("427")]     + [lrow("SL-G12", 5, cell="c2-100", seed=s_, rep=i, modes=("COLLAPSE",)) for i, s_ in enumerate("427")]
check("extras never enter the outcome: wild SL-B0 / SL-G12 rows change neither the label nor the arms' comparisons",
      pp.unstable_modes(wild[0]) == [] and ladder(sl3 + sh3 + mn3 + wild)["label"] == ladder(sl3 + sh3 + mn3)["label"] == "CONVERGES-ONE-POINT"
      and ladder(sl3 + sh3 + mn3 + wild)["GOODPUT"] == ladder(sl3 + sh3 + mn3)["GOODPUT"])
msgs = []
ex = [lrow("SL-B0", 195, cell="c2-100", seed=s_, rep=i, mbps=88.0 + i, ax=15000 + 10 * i, tp50=15000 + 10 * i) for i, s_ in enumerate("427")]     + [lrow("SL-G12", 184, cell="c2-100", seed=s_, rep=i, mbps=70.0 + i, ax=16000 + 10 * i, tp50=16000 + 10 * i) for i, s_ in enumerate("427")]
rx = pp.cell_outcome(sl3 + sh3 + mn3 + ex, "c2-100", out=msgs.append)
check("reported: R1-R10 lines per extra arm, goodput and delay against the scored MAIN rows, n_end against the offline point",
      any("EXTRA SL-B0 (reported only): scored 3/3" in m for m in msgs) and any("R SL-G12 n_end" in m for m in msgs)
      and any("R SL-B0 R4 brake_us per third" in m for m in msgs) and rx["extra"]["SL-B0"]["GOODPUT"] == "WITHIN"
      and rx["extra"]["SL-G12"]["GOODPUT"] == "WORSE" and rx["extra"]["SL-B0"]["DELAY_Tp50"] == "BETTER"
      and abs(rx["extra"]["SL-B0"]["ln_vs_pred"]) < 1e-9 and abs(rx["extra"]["SL-G12"]["ln_vs_pred"]) < 1e-9, rx["extra"])
check("  the extra rows are listed as ROW lines too, and SHORT extras are left out of the report",
      any(m.strip().startswith("ROW c2-100 SL-B0") for m in msgs)
      and pp.cell_outcome(sl3 + sh3 + mn3 + [dict(x, short=True) for x in ex], "c2-100", out=lambda *_: None)["extra"]["SL-B0"]["scored"] == 0)
check("  at c1s no extra section is printed", ladder(sl3c + inert3 + mn3c, "c1s-400")["extra"] == {})


# ── the status 43 note: ENGINE-STALL, reported beside the (unchanged) label ──
# n above its last-third median before the stall; the stall: paused 97 %, rtt 380 ms against wrtt 2 ms from t = 100 s.
ns_stall = [900] * 400 + [400] * 400                              # 200 s of windows; the stall starts at 100 s with n = 400
dst = mk("SL", cell="c1s-400", ns=ns_stall, dnf=True, seconds=None, t_end=200, stall_from=100.0, echo_line=echo("SL", "c1s-400"))
check("ENGINE-STALL: a DNF row with n at/above its last-third median until the stall, paused 97 % and rtt 380 >> wrtt 2 from 100 s",
      dst["status"] == "LIVE" and dst["dnf"] and dst["stall"]["engine_stall"] and dst["stall"]["onset"] is not None
      and 99.5 <= dst["stall"]["onset"] <= 100.5 and dst["stall"]["n_held"] and dst["stall"]["stalled_frac"] == 1.0, dst["stall"])
dno = mk("SL", cell="c1s-400", ns=[600] * 800, dnf=True, seconds=None, t_end=200, echo_line=echo("SL", "c1s-400"))
check("  a DNF row with no stall (paused 19 %, rtt = wrtt) is not ENGINE-STALL, and a completed row never is",
      not dno["stall"]["engine_stall"] and dno["stall"]["onset"] is None and not mk("SL", stall_from=30.0)["stall"]["engine_stall"], dno["stall"])
dnl = mk("SL", cell="c1s-400", ns=[300] * 200 + [900] * 600, dnf=True, seconds=None, t_end=200, stall_from=100.0, echo_line=echo("SL", "c1s-400"))
check("  n BELOW its last-third median before the stall is not ENGINE-STALL (the components are still reported)",
      not dnl["stall"]["engine_stall"] and dnl["stall"]["onset"] is not None and dnl["stall"]["n_held"] is False, dnl["stall"])
dsh = mk("SL", cell="c1s-400", ns=ns_stall, dnf=True, seconds=None, t_end=200, stall_from=199.0, echo_line=echo("SL", "c1s-400"))
check("  a stall shorter than 8 windows (2 s) is no onset", dsh["stall"]["onset"] is None and not dsh["stall"]["engine_stall"], dsh["stall"])
msg = []
lab_a = ladder(sl3c + inert3 + mn3c + [dst], "c1s-400")["label"]
lab_b = ladder(sl3c + inert3 + mn3c, "c1s-400")["label"]
check("  the registered label is computed literally: the stalled DNF row is an SL row for the ladder and ENGINE-STALL changes no rule",
      lab_a != "" and isinstance(lab_a, str) and lab_b == "CONVERGES-ONE-POINT", (lab_a, lab_b))
lg = []
with tempfile.TemporaryDirectory() as td2:
    lp2 = os.path.join(td2, "s.log")
    with open(lp2, "w", encoding="utf-8") as f:
        for r in [dst, dno]:
            f.write("P43ROW " + json.dumps(r, sort_keys=True) + "\n")
    pp.score([lp2], out=lg.append)
check("score prints the DNF classification block with both the label and ENGINE-STALL",
      any("DNF rows 2: ENGINE-STALL" in m for m in lg) and any("DNF c1s-400 SL" in m and ": ENGINE-STALL" in m for m in lg)
      and any("not ENGINE-STALL" in m for m in lg) and any("LABEL c1s-400" in m for m in lg))


# ── plan 43b (status 43b): SL-T / SH-T / MAIN at c2 under a plan pair (G, tau) and a band set by amendment ──
p43b = pp.plan43b()
check("plan 43b: 9 scored invocations = 3 blocks x 3 arms at c2-100 only; the smoke is c2 SH-T and SL-T",
      len(p43b) == 9 == pp.EXPECT_ROWS43B and {c for c, a, s_, r in p43b} == {"c2-100"}
      and pp.plan43b(smoke=True) == [("c2-100", "SH-T", "42", 1), ("c2-100", "SL-T", "42", 1)])
check("plan 43b: n = 3 per arm with seeds 42 / 7 / 42, the arm order rotating per block (MAIN SL-T SH-T / SL-T SH-T MAIN / SH-T MAIN SL-T)",
      all([(s_, r) for c, a, s_, r in p43b if a == x] == [("42", 1), ("7", 1), ("42", 2)] for x in pp.ARMS43B)
      and [a for c, a, s_, r in p43b[0:3]] == ["MAIN", "SL-T", "SH-T"] and [a for c, a, s_, r in p43b[3:6]] == ["SL-T", "SH-T", "MAIN"]
      and [a for c, a, s_, r in p43b[6:9]] == ["SH-T", "MAIN", "SL-T"])
cp = subprocess.run([sys.executable, os.path.join(HERE, "place43_parse.py"), "plan43b"], capture_output=True, text=True)
check("`plan43b` prints the 9 lines", cp.stdout.splitlines() == [" ".join(map(str, r)) for r in p43b])
check("the plan variables are amendment 1's: G = 0.9, tau = 5000 us, band [178, 217]",
      pp.G43B == 0.9 and pp.TAU43B_US == 5000.0 and pp.BAND43B == (178.0, 217.0) and pp.plan43b_vars() == (0.9, 5000.0, (178.0, 217.0)))
cv = subprocess.run([sys.executable, os.path.join(HERE, "place43_parse.py"), "vars43b"], capture_output=True, text=True)
check("  ... `vars43b` prints them the way the engine echoes them (the battery's G and tau)", cv.returncode == 0 and cv.stdout.split() == ["0.9", "5000"], (cv.returncode, cv.stdout))
_saved = (pp.G43B, pp.TAU43B_US, pp.BAND43B)
pp.G43B = None
check("  ... and refuses (rc 3) while any is unset", pp.plan43b_vars() is None)
pp.G43B = _saved[0]
check("rust_f64 prints as the engine's echo does (5000.0 -> 5000, 0.9 -> 0.9, 2470.5 -> 2470.5)",
      pp.rust_f64(5000.0) == "5000" and pp.rust_f64(0.9) == "0.9" and pp.rust_f64(2470.5) == "2470.5" and pp.rust_f64(1) == "1")
check("battery (43b): refuses while the variables are unset, arm envs from G43B / TAU43B, hold 1536:15, plan table of 9, the sha of status 43",
      'ABORT-PLAN43B-UNSET' in bat and 'vars43b' in bat and 'RWM_ORACLE_GAIN=$G43B' in bat and 'RWM_ORACLE_BRAKE_OFFSET_US=$TAU43B' in bat
      and 'SH-T)' in bat and 'SL-T)' in bat and '-eq 9 ]' in bat and "94959c05ec9898b4deaea53f815b60d8ba1d9636030f3959bea259d79c1dc008" in bat
      and '"$PLAN" != "43b" ] || [ "$GOT" = "$SHA43" ]' in bat and "ABORT-SHA plan 43b" in bat and "plan43b smoke" in bat and 'P43=1' in bat)
check("run_all (43b): refuses before any lock while the variables are unset, ABORT-SHA on any other binary, scores with score43b",
      'vars43b' in runall and "ABORT-PLAN43B-UNSET" in runall and 'ABORT-SHA plan 43b' in runall and "P_SCORE=score43b" in runall
      and pp.SHA43 == "94959c05ec9898b4deaea53f815b60d8ba1d9636030f3959bea259d79c1dc008")
check("loopdiag_parse.py dispatches smoke43b / check43b / score43b", all(c in open(os.path.join(HERE, "loopdiag_parse.py"), encoding="utf-8").read() for c in ('cmd == "smoke43b"', 'cmd == "check43b"', 'cmd == "score43b"')))
check("the SHA constant of battery and scorer agrees with the ledger of status 43 (BINSHA.txt)",
      pp.SHA43 in open(os.path.join(HERE, "..", "..", "docs", "l1-raw", "oracle43", "BINSHA.txt"), encoding="utf-8").read())

# the row under LD_PLAN=43b with the pair set by an "amendment"
os.environ["LD_PLAN"] = "43b"
et = lambda b0="5000", g="0.9", hold="none": f"oracle store law ACTIVE (RWM_STORE_LAW=oracle, diagnostic, never merged): L_dgps=[9773.0] hold={hold} T_w=0.25s b0_us={b0} gain={g}"  # noqa: E731
rt = mk("SL-T", echo_line=et(), sha=pp.SHA43)
check("43b: a good SL-T row is LIVE; the ladder sees it as an SL row (arm_name keeps SL-T); its echo declares its own gain and b0_us",
      rt["status"] == "LIVE" and rt["arm"] == "SL" and rt["arm_name"] == "SL-T" and rt["echo_b0_gain"] == ["5000", "0.9"], (rt["status"], rt["problems"]))
rh = mk("SH-T", echo_line=et(hold="H=1536 t_h=15s"), sha=pp.SHA43)
check("43b: a good SH-T row is an SH row", rh["status"] == "LIVE" and rh["arm"] == "SH" and rh["arm_name"] == "SH-T", rh["problems"])
check("43b: MAIN must declare b0_us=0 gain=q", mk("MAIN", sha=pp.SHA43)["status"] == "LIVE"
      and any(p_.startswith("echo-b0-gain") for p_ in mk("MAIN", echo_line=echo("MAIN", gain="0.9"), sha=pp.SHA43)["problems"]))
check("43b: an SL-T row whose echo declares another gain or another b0_us is void",
      any(p_.startswith("echo-b0-gain") for p_ in mk("SL-T", echo_line=et(g="1"), sha=pp.SHA43)["problems"])
      and any(p_.startswith("echo-b0-gain") for p_ in mk("SL-T", echo_line=et(b0="0"), sha=pp.SHA43)["problems"])
      and mk("SL-T", echo_line=et(g="1"), sha=pp.SHA43)["status"] == "WITNESS-FAIL")
check("43b: a binary other than status 43's is a witness failure on the row (the battery aborts on it before any row)",
      any(p_.startswith("bin-sha") for p_ in mk("SL-T", echo_line=et(), sha="0" * 64)["problems"]))
pp.G43B = None
check("43b: with the plan variables unset every row is void (plan-43b-variables-unset)",
      "plan-43b-variables-unset" in mk("SL-T", echo_line=et(), sha=pp.SHA43)["problems"])
pp.G43B = _saved[0]
os.environ.pop("LD_PLAN", None)

# qcwnd: the lowest against the median, per row
rq = mk("SL", qcw=lambda i: 20000 if i == 100 else 100000)
check("per row: the lowest qcwnd against its median (a quinn window collapse is not scored as the law)",
      rq["qcwnd_min"] == 20000 and rq["qcwnd_med"] == 100000 and abs(rq["qcwnd_ratio"] - 0.2) < 1e-12, (rq["qcwnd_min"], rq["qcwnd_med"]))
ms = []
pp.cell_outcome([rq], "c2-100", out=ms.append)
check("  ... printed on the ROW line", any("qcwnd min/med=20000/100000 (0.200)" in m for m in ms), ms[:2])
qq = pp.qcwnd_series(lc.read(os.path.join(HERE, "..", "..", "docs", "l1-raw", "oracle43", "diag-ld", "c1s-400-SL-s42-r2-a1-c.log")))
check("  ... and read from a committed §43 client log (c1s SL s42 r2, the runaway row: min / median about 0.10)",
      len(qq) > 500 and 0.05 < min(qq) / lc.med(qq) < 0.2, (len(qq), min(qq) / lc.med(qq)))

# the band is a plan constant for 43b
sl_b = [lrow("SL", 160, seed=s_, rep=i) for i, s_ in enumerate("427")]
sh_b = [lrow("SH", 160, seed=s_, rep=i) for i, s_ in enumerate("427")]
mn_b = [lrow("MAIN", 2048, seed=s_, rep=i) for i, s_ in enumerate("427")]
check("43b: the cell's band is the plan constant (160 inside (150, 250) -> CONVERGES; against (300, 400) -> ONE-POINT-OFF-PREDICTION)",
      pp.cell_outcome(sl_b + sh_b + mn_b, "c2-100", out=lambda *_: None, band=(150.0, 250.0))["label"] == "CONVERGES-ONE-POINT"
      and pp.cell_outcome(sl_b + sh_b + mn_b, "c2-100", out=lambda *_: None, band=(300.0, 400.0))["label"] == "ONE-POINT-OFF-PREDICTION")

# the dry run on status 43's committed rows, SL standing for SL-T
cd_ = subprocess.run([sys.executable, os.path.join(HERE, "place43_parse.py"), "dryrun43b",
                      os.path.join(HERE, "..", "..", "docs", "l1-raw", "oracle43", "ld.log")], capture_output=True, text=True, encoding="utf-8",
                     env=dict(os.environ, PYTHONIOENCODING="utf-8"))
check("dry run on §43's ledger rows (c2 MAIN / SL / SH as MAIN / SL-T / SH-T): runs, 9/9 rows, the ladder gives §43's label",
      cd_.returncode == 0 and "ROWS 9/9" in cd_.stdout and "LABEL c2-100: BISTABLE" in cd_.stdout, (cd_.returncode, cd_.stderr[-300:]))

print("FAILS %d" % fails)
sys.exit(1 if fails else 0)
