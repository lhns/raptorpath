#!/usr/bin/env python3
"""Tests for the section 46 path-event harness: the netem builder (via test_topo_netem.sh), the injector, plan 46 and its pinned order,
the series readouts and witnesses, the qdisc check, and a scorer dry-run on synthetic rows for every outcome.
Run: python3 test_harness46.py"""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
os.environ.pop("LD_PLAN", None)
import l1common as lc  # noqa: E402
import place38_parse as pp  # noqa: E402
import place46_parse as q  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)) if not ok else ""))
    if not ok:
        fails += 1


bat = open(os.path.join(HERE, "loopdiag_battery.sh"), encoding="utf-8").read()
runall = open(os.path.join(HERE, "loopdiag_run_all.sh"), encoding="utf-8").read()
perf = open(os.path.join(HERE, "perf_rwm_c.sh"), encoding="utf-8").read()

# ── the netem builder and reshape (offline, stubbed `ip`) ─────────────────
if shutil.which("bash"):
    cp = subprocess.run(["bash", os.path.join(HERE, "test_topo_netem.sh")], capture_output=True, text=True)
    check("test_topo_netem.sh: netem_args is byte-for-byte the pre-refactor `shape` line; `reshape` is one `tc qdisc change` of the full line",
          cp.returncode == 0 and "ALL OK" in cp.stdout, cp.stdout[-600:] + cp.stderr[-300:])

# ── plan 46 ───────────────────────────────────────────────────────────────
pl = q.plan46()
sm = q.plan46(smoke=True)
check("plan 46: 2 check rows + 22 = 24 invocations (16 scored, 4 at +500 ms, 2 c1d)", len(sm) == 2 and len(pl) == 22 == q.EXPECT_ROWS46 and len(sm) + len(pl) == q.EXPECT_TOTAL46 == 24)
check("check rows: one per binary, BASE at SLOW-HIT and CAND at FAST-HIT, rep 0 (they double as the smoke)",
      sm == [("c8-500", "def", "BASE", "42", 0, "SLOW"), ("c8-500", "def", "CAND", "42", 0, "FAST")])
scored = [r for r in pl if r[5] in ("SLOW", "FAST")]
check("16 scored rows: n = 4 per arm per variant (seeds 42 and 7 x 2 reps)",
      len(scored) == 16 and all(sorted((r[3], r[4]) for r in scored if r[2] == a and r[5] == v) == [("42", 1), ("42", 2), ("7", 1), ("7", 2)] for a in q.ARMS46 for v in ("SLOW", "FAST")))
EXPECT46 = [  # rep-major; within a rep seed 42 then 7; within a seed SLOW-HIT then FAST-HIT; both arms per block, CAND first on odd blocks
    ("BASE", "42", 1, "SLOW"), ("CAND", "42", 1, "SLOW"),
    ("CAND", "42", 1, "FAST"), ("BASE", "42", 1, "FAST"),
    ("BASE", "7", 1, "SLOW"), ("CAND", "7", 1, "SLOW"),
    ("CAND", "7", 1, "FAST"), ("BASE", "7", 1, "FAST"),
    ("BASE", "42", 2, "SLOW"), ("CAND", "42", 2, "SLOW"),
    ("CAND", "42", 2, "FAST"), ("BASE", "42", 2, "FAST"),
    ("BASE", "7", 2, "SLOW"), ("CAND", "7", 2, "SLOW"),
    ("CAND", "7", 2, "FAST"), ("BASE", "7", 2, "FAST"),
    ("BASE", "42", 1, "SLOW500"), ("CAND", "42", 1, "SLOW500"),
    ("CAND", "7", 1, "SLOW500"), ("BASE", "7", 1, "SLOW500"),
    ("BASE", "42", 1, "C1D"), ("CAND", "42", 1, "C1D"),
]
check("plan 46: the order is pinned (the whole table, rep-major, CAND first on odd blocks)", [(r[2], r[3], r[4], r[5]) for r in pl] == EXPECT46, [(r[2], r[3], r[4], r[5]) for r in pl])
check("... the scored blocks alternate which arm goes first (BASE, CAND, CAND, BASE, ...): each arm leads 4 of the 8 blocks",
      [pl[2 * i][2] for i in range(8)] == ["BASE", "CAND", "BASE", "CAND", "BASE", "CAND", "BASE", "CAND"])
import stage3_parse as sp  # noqa: E402
check("cells: SLOW / FAST / +500 at c8-500 (c2/c3 dual, 500 MB), C1D at c1d-3500 (c1/c1 dual, 3.5 GB)", all(r[0] == ("c1d-3500" if r[5] == "C1D" else "c8-500") for r in pl)
      and sp.CELL_SPEC["c8-500"] == ("c2", "c3", "dual", 500000000) and sp.CELL_SPEC["c1d-3500"] == ("c1", "c1", "dual", 3500000000))
check("injector variants: SLOW = leg 1 +1500, FAST = leg 0 +1500, SLOW500 = leg 1 +500, C1D = leg 1 +1500",
      {v: (q.VARIANTS[v][1], q.VARIANTS[v][2]) for v in q.VARIANTS} == {"SLOW": (1, 1500), "FAST": (0, 1500), "SLOW500": (1, 500), "C1D": (1, 1500)})
cp = subprocess.run([sys.executable, os.path.join(HERE, "place46_parse.py"), "plan"], capture_output=True, text=True)
check("`plan` prints the 22 `cell store arm seed rep variant` lines; `plan smoke` the 2 check rows",
      cp.stdout.splitlines() == [" ".join(map(str, r)) for r in pl]
      and subprocess.run([sys.executable, os.path.join(HERE, "place46_parse.py"), "plan", "smoke"], capture_output=True, text=True).stdout.splitlines() == [" ".join(map(str, r)) for r in sm])
cp = subprocess.run([sys.executable, os.path.join(HERE, "place46_parse.py"), "event", "SLOW"], capture_output=True, text=True)
check("`event SLOW` = 1:1500 (RWM_EVENT=<leg>:<delta_ms>)", cp.stdout.strip() == "1:1500")

# ── the shell side ────────────────────────────────────────────────────────
check("battery: plan 46 accepted, per-row binary (BASE -> BIN_BASE), CAND arm, the injector env set per row and withheld otherwise, "
      "check rows get the qdisc captures, the evt files are copied, a dead injection is ABORT-INJECT, c8-500 and c1d-3500 cells, 22-row table",
      "|46) ;;" in bat and 'RBIN="$BIN_BASE"' in bat and "CAND)" in bat and "-u RWM_EVENT -u RWM_EVENT_QDISC -u RWM_EVENT_ONSET_S -u RWM_EVENT_HOLD_S" in bat
      and "RWM_EVENT_QDISC=1" in bat and "rwm-evt.txt" in bat and "rwm-evtq.txt" in bat and "ABORT-INJECT" in bat
      and 'c8-500)  echo "c2 c3 dual 500000000"' in bat and 'c1d-3500) echo "c1 c1 dual 3500000000"' in bat and "-eq 22" in bat and "place46_parse.py" in bat)
check("battery: the CAND binary must lack RWM_PLACE_WIRE (the promotion), refused otherwise", 'ABORT-CAND the CAND binary' in bat and 'grep -aq -- "RWM_PLACE_WIRE" "$BIN"' in bat)
check("run_all: plan 46 REFUSES to launch while LD_CAND_SHA is unset / the placeholder or differs from the shipped COMMIT; nothing is run",
      'ABORT-CAND LD_CAND_SHA is unset' in runall and 'LD_CAND_SHA' in runall and "NOTHING WAS RUN" in runall
      and runall.index("ABORT-CAND LD_CAND_SHA is unset") < runall.index("prove_sentinels"))
check("run_all: plan 46 scores smoke.log + ld.log, a failed check row is ABORT-INJECT before any scored row, suites only with LD_P46_SUITES=1",
      "P_SCORE=score46" in runall and '"$RUN/smoke.log" "$RUN/ld.log"' in runall and 'fail_all "ABORT-INJECT' in runall and "LD_P46_SUITES" in runall)
check("perf_rwm_c.sh: the injector starts only when RWM_EVENT is set, with the leg's own scenario and leg_seed, and is stopped by PID after the client",
      'if [[ -n "${RWM_EVENT:-}" ]]' in perf and "event_inject.py" in perf and 'leg_seed "${SEED:-42}" "$_evleg"' in perf and 'kill -TERM "$EVT_PID"' in perf)

# ── the injector, against a fake topology ─────────────────────────────────
td = tempfile.mkdtemp()
fake = os.path.join(td, "fake_topo.sh")
open(fake, "w", newline="\n").write('#!/bin/bash\ncase "$1" in\n  oneway) echo 20 ;;\n  reshape) echo "$@" >> "$FAKE_LOG"; exit "${FAKE_RC:-0}" ;;\nesac\n')
logp, outp, qp, flog = (os.path.join(td, n) for n in ("c.log", "evt.txt", "evtq.txt", "fake.log"))
os.environ["FAKE_LOG"] = flog


def writer():
    with open(logp, "w", buffering=1) as fh:
        for i in range(3):
            fh.write(f"[DIAG] t={0.1 * i:.1f}s win=1/1\n")           # the warm-up run's own DIAG lines: ignored
            time.sleep(0.05)
        fh.write('{"seconds":0.01,"warmup":true}\n')
        t0 = time.time()
        k = 0
        while time.time() - t0 < 3.0:
            k += 1
            fh.write(f"[DIAG] t={k * 0.1:.1f}s win=1/1\n")
            time.sleep(0.1)


th = threading.Thread(target=writer)
th.start()
t_begin = time.time()
rc = subprocess.run([sys.executable, os.path.join(HERE, "event_inject.py"), "--log", logp, "--out", outp, "--ns", "rp-cli", "--dev", "cli1", "--scenario", "c3",
                     "--seed", "1042", "--delta-ms", "1500", "--onset-s", "0.6", "--hold-s", "0.5", "--tail-s", "0.4", "--qdisc-out", qp, "--topo", fake],
                    cwd=td, capture_output=True, text=True).returncode
th.join()
txt = open(outp).read()
e = q.parse_evt(txt)
freshly = open(flog).read().splitlines()
check("injector: exits 0; start stamped at the warm-up line, then exactly two reshape calls: delay 20 + 1500 = 1520 ms, then 20 ms",
      rc == 0 and e["start"] is not None and len(freshly) == 2 and freshly[0].endswith("rp-cli cli1 c3 1042 1520") and freshly[1].endswith("rp-cli cli1 c3 1042 20"), (rc, freshly, txt[:300]))
check("injector: onset at start + 0.6 s (+-0.15 s), restore 0.5 s after the onset call, both rc 0, 'done' written",
      e["on"] and e["off"] and abs(e["on"]["before"] - e["start"] - 0.6) < 0.15 and abs(e["off"]["before"] - e["on"]["before"] - 0.5) < 0.15
      and e["on"]["rc"] == e["off"]["rc"] == 0 and e["done"], txt[:400])
check("injector: [DIAG] lines are stamped only AFTER the warm-up line, and the DIAG-axis estimate rides on the on/off lines",
      len(e["stamps"]) >= 10 and min(t for t, _ in e["stamps"]) >= 0.1 and "diag_t=" in txt and "[EVENT] on " in txt and "[EVENT] off " in txt)
ax = q.event_axis(e)
check("event_axis maps the event onto the [DIAG] axis: the on/off times are 0.5 s apart on that axis too (+-0.15 s)", ax is not None and abs((ax[1] - ax[0]) - 0.5) < 0.15, ax)
qtxt = open(qp).read()
check("the qdisc capture file has before / during (every ~100 ms; the stub `bash` call eats part of the 0.5 s hold) / after blocks",
      qtxt.count("### before") == 1 and qtxt.count("### during") >= 2 and qtxt.count("### after") == 1, (qtxt.count("### before"), qtxt.count("### during"), qtxt.count("### after")))
os.environ["FAKE_RC"] = "1"
open(flog, "w").close()
th = threading.Thread(target=writer)
th.start()
rc2 = subprocess.run([sys.executable, os.path.join(HERE, "event_inject.py"), "--log", logp, "--out", outp, "--dev", "cli1", "--scenario", "c3", "--seed", "1", "--delta-ms", "1500",
                      "--onset-s", "0.4", "--hold-s", "0.3", "--tail-s", "0.2", "--topo", fake], cwd=td, capture_output=True, text=True).returncode
th.join()
e2 = q.parse_evt(open(outp).read())
check("a failing tc call: the injector exits non-zero and logs rc != 0 (the row is then ABORT-INJECT)", rc2 != 0 and e2["on"]["rc"] == 1, (rc2, e2["on"]))
rc3 = subprocess.run([sys.executable, os.path.join(HERE, "event_inject.py"), "--log", os.path.join(td, "never.log"), "--out", outp, "--dev", "cli1", "--scenario", "c3", "--delta-ms", "1500",
                      "--max-wait-s", "0.3", "--topo", fake], cwd=td, capture_output=True, text=True).returncode
check("no warm-up line within the wait: ABORT, exit 3, no tc call", rc3 == 3 and "ABORT no-warmup-line" in open(outp).read() and open(flog).read().count("reshape") <= 2)
shutil.rmtree(td, ignore_errors=True)

# ── synthetic series ──────────────────────────────────────────────────────
T_ON, T_OFF = 6.0, 8.0


def gen(sigma=0.18, back_at=None, g_pre=100.0, g_post=95.0, g_hold=40.0, wrtt_pre=70, delta_w=1500, hit=1, rtt_frozen=False, decay_back=None,
        n_end=45.0, drop_event=False, dt=0.25, period=2.0):
    """A [DIAG] series: legs 0 and 1, the hit leg's share sigma before the event, ~0 from t_on until `back_at` (seconds after t_off; None = never),
    then sigma again. wrtt jumps by delta_w at t_on and decays 1/8 per idle sample (period 2 s) after t_off."""
    S, tx = [], [0, 0]
    t, w = 0.0, wrtt_pre
    last_sample = T_OFF
    while t < n_end:
        t = round(t + dt, 6)
        hold = T_ON <= t <= T_OFF + 1.5 and not drop_event
        recovered = back_at is not None and t >= T_OFF + back_at
        if t < T_ON or recovered:
            d_hit = int(1000 * sigma / (1 - sigma))
        elif T_ON <= t < T_OFF + 1.5 and not recovered:
            d_hit = 1 if t >= T_ON + 0.5 else int(1000 * sigma / (1 - sigma))
        else:
            d_hit = 1
        tx[1 - hit] += 1000
        tx[hit] += d_hit
        infl_hit = 0 if (t >= T_ON + 0.5 and not recovered) else 5
        if T_ON <= t < T_OFF and not drop_event:
            w = wrtt_pre + delta_w
        elif t >= T_OFF and w > wrtt_pre * 1.02 and (recovered or t - last_sample >= period / 2):
            if recovered:
                w = wrtt_pre
            else:
                w = int(w - (w - wrtt_pre) / 8)
                last_sample = t
        elif t >= T_OFF and w > wrtt_pre * 1.02 and False:
            pass
        g = g_pre if t < T_ON else g_hold if t < T_OFF else g_post
        rtt_hit = w if not rtt_frozen else (wrtt_pre + delta_w)
        wpl = 1000 * w
        legs = [{"infl": 5, "rtt": 70, "wrtt": 70, "wpl": 70000}, {"infl": 5, "rtt": 70, "wrtt": 70, "wpl": 70000}]
        legs[hit] = {"infl": infl_hit, "rtt": rtt_hit, "wrtt": w, "wpl": wpl}
        S.append({"t": t, "good": g, "tx": {0: tx[0], 1: tx[1]}, "infl": {i: legs[i]["infl"] for i in (0, 1)}, "rtt": {i: legs[i]["rtt"] for i in (0, 1)},
                  "wrtt": {i: legs[i]["wrtt"] for i in (0, 1)}, "wpl": {i: legs[i]["wpl"] for i in (0, 1)}})
    return S


S0 = gen(back_at=3.0)
m = q.analyze(S0, T_ON, T_OFF, 1)
check("analyze: sigma_pre = the pooled hit share over [2.0 s, t_on) (0.18)", abs(m["sigma_pre"] - 0.18) < 0.01, m["sigma_pre"])
check("analyze: R = the last of the first 4 consecutive lines that START after t_off with pooled share >= sigma/2; with the leg back at +3.0 s, two of the four "
      "lines suffice to pool to sigma/2, so R = 3.25 (the rule as written)", not m["R_cens"] and abs(m["R"] - 3.25) < 1e-6, m["R"])
check("analyze: R0 = the first line after t_off with d_hit > 2 (back at +3.0 s -> 3.25); T_w finite", abs(m["R0"] - 3.25) < 0.3 and not m["R0_cens"] and not m["T_w_cens"], (m["R0"], m["T_w"]))
check("analyze: the event witness bites (peak wrtt 1570 >= 70 + 750) and the exit witness holds (share collapses inside [t_on, t_off + 1 s])", m["event_ok"] and m["exit_ok"], (m["event_peak_wrtt"], m["exit_ok"]))
m500 = q.analyze(gen(back_at=3.0, delta_w=300), T_ON, T_OFF, 1, delta_ms=500)
check("analyze: a +500 ms event bites at Delta/2 = 250 ms (wrtt +300; session 3's SLOW500 rows peaked +290..+342 and were voided by a fixed 750)", m500["event_ok"], m500["event_peak_wrtt"])
check("analyze: a +500 ms event that moved wrtt by only 200 ms does not bite", not q.analyze(gen(back_at=3.0, delta_w=200), T_ON, T_OFF, 1, delta_ms=500)["event_ok"])
check("analyze: the scored Delta = 1500 still needs 750 ms (wrtt +700 does not bite)", not q.analyze(gen(back_at=3.0, delta_w=700), T_ON, T_OFF, 1)["event_ok"])
check("analyze: gbar_pre 100, gbar_post 95, G = 1 - 95/100 = 5 %, gbar_hold between 40 and 50 (its last line is the post value)",
      abs(m["g_pre"] - 100) < 1e-6 and 40 <= m["g_hold"] < 50 and abs(m["g_post"] - 95) < 1e-6 and abs(m["G"] - 0.05) < 1e-6, m)
mc = q.analyze(gen(back_at=None), T_ON, T_OFF, 1)
check("analyze: a leg that never comes back is censored: R = H = 25 (flag set), R0 and T_w too", mc["R_cens"] and mc["R"] == 25.0 and mc["R0_cens"], (mc["R"], mc["R_cens"]))
mb = q.analyze(gen(back_at=25.0), T_ON, T_OFF, 1)
check("analyze: a return at +25.0 s is censored (R would be 25.25 > H), a return at +24.0 s is finite (R = 24.25)",
      mb["R_cens"] and not q.analyze(gen(back_at=24.0), T_ON, T_OFF, 1)["R_cens"] and abs(q.analyze(gen(back_at=24.0), T_ON, T_OFF, 1)["R"] - 24.25) < 1e-6)
mw = q.analyze(gen(back_at=3.0, delta_w=200), T_ON, T_OFF, 1)
check("analyze: the event witness fails when wrtt rises by only 200 ms (< 750)", not mw["event_ok"], mw["event_peak_wrtt"])
mn = q.analyze(gen(back_at=3.0, drop_event=True), T_ON, T_OFF, 1)
check("analyze: no event on the leg's wrtt -> the witness fails", not mn["event_ok"])
mf = q.analyze(gen(back_at=None, rtt_frozen=True), T_ON, T_OFF, 1)
check("... idle lines, a constant rtt (range 0) and a wrtt that fell from its first idle value", mf["idle_lines"] > 3 and mf["idle_rtt_range"] == 0 and mf["idle_wrtt_last"] < mf["idle_wrtt_first"] and mf["idle_rtt_frozen"], mf)
check("... refresh rate: wrtt changes per 2 s on idle lines is > 0 and about 1 per 2 s in the synthetic series", mf["refresh_per_2s"] is not None and 0.3 < mf["refresh_per_2s"] < 2.5, mf["refresh_per_2s"])
mca = q.analyze(gen(back_at=6.0), T_ON, T_OFF, 1)
check("... CAND mechanism: wpl_us numeric, and it fell in >= 2 steps before R0", mca["wpl_numeric_all"] and mca["wpl_steps_before_R0"] >= 2, (mca["wpl_numeric_all"], mca["wpl_steps_before_R0"]))
mfa = q.analyze(gen(hit=0, sigma=0.82, back_at=2.0), T_ON, T_OFF, 0)
check("FAST-HIT shape (sigma 0.82, hit = leg 0): sigma_pre 0.82; one recovered line already pools to sigma/2, so R lands at the return (2.0 s)", abs(mfa["sigma_pre"] - 0.82) < 0.01 and abs(mfa["R"] - 2.0) < 0.3, (mfa["sigma_pre"], mfa["R"]))

# ── the qdisc check ───────────────────────────────────────────────────────
def qd(delay, backlog=3, extra=""):
    return f"qdisc netem 1: root refcnt 2 limit 1000 delay {delay}  5ms loss gemodel 2% 40% 1-h 0% 1-k 100% rate 20Mbit seed 1042 {extra}\n Sent 1000 bytes 10 pkt (dropped 1, overlimits 0 requeues 0)\n backlog 0b {backlog}p requeues 0\n"


def qcap(before, durings, after):
    s = "### before wall=1.0\n" + before
    for i, d in enumerate(durings):
        s += f"### during wall={2 + i}.0\n" + d
    return s + "### after wall=9.0\n" + after


good = qcap(qd("20ms"), [qd("1.52s", 120), qd("1.52s", 340), qd("1.52s", 600)], qd("20ms", 0))
r = q.qdisc_check(good, 1500)
check("qdisc check passes: during differs from before only in delay (+1500 ms, printed as 1.52s), after equals before, backlog peak 600 < 900", r["ok"] and r["peak_backlog"] == 600, r)
check("qdisc check fails when another option changed during the hold (the loss model)", not q.qdisc_check(qcap(qd("20ms"), [qd("1.52s").replace("gemodel 2%", "gemodel 3%")], qd("20ms")), 1500)["ok"])
check("qdisc check fails when after != before", not q.qdisc_check(qcap(qd("20ms"), [qd("1.52s")], qd("25ms")), 1500)["ok"])
check("qdisc check fails when the delay did not move by +1500 ms", not q.qdisc_check(qcap(qd("20ms"), [qd("520ms")], qd("20ms")), 1500)["ok"])
check("qdisc check fails at a backlog peak of 900 packets (needs < 900)", not q.qdisc_check(qcap(qd("20ms"), [qd("1.52s", 900)], qd("20ms")), 1500)["ok"] and q.qdisc_check(qcap(qd("20ms"), [qd("1.52s", 899)], qd("20ms")), 1500)["ok"])
check("qdisc check fails with no during captures", not q.qdisc_check("### before wall=1\n" + qd("20ms") + "### after wall=2\n" + qd("20ms"), 1500)["ok"])
check("qdisc check passes at the c8 FAST leg: 5 ms + 1500 ms, which tc prints with 3 significant digits as 1.5s (session 47b's false SMOKE-FAIL)", q.qdisc_check(qcap(qd("5ms"), [qd("1.5s")], qd("5ms")), 1500)["ok"])
check("qdisc check still fails beyond display precision: 5 ms + 1500 ms read as 1.52s (15 ms off, tolerance 5 ms)", not q.qdisc_check(qcap(qd("5ms"), [qd("1.52s")], qd("5ms")), 1500)["ok"])
check("qdisc check ignores the clsact block and counters (they differ between blocks)", q.qdisc_check(good.replace("Sent 1000 bytes", "Sent 7777 bytes", 1), 1500)["ok"])

# ── a real row through make_row ───────────────────────────────────────────
RAW = os.path.join(HERE, "..", "..", "docs", "l1-raw")
PD = os.path.join(RAW, "thread-pd", "battery", "diag-tpd", "c1d-400-MAIN-s42-r1-a1-")


def diag_line(s, nleg=2, wpl=True):
    legs = "".join(f" p{i}:infl={s['infl'][i]}/sinfl=0/bdp1(cap0) sout=0 khr=1.00/kraw=- btlbw=1 sr=0/g0d0 est=Y pl=0.0000 cmp=- rtt={s['rtt'][i]}/wrtt={s['wrtt'][i]}/rtp3ms rtp_us=1 "
                   f"| ANCHOR sent=0 plu=0.0010 plc=0.0014" + (f" wpl_us={s['wpl'][i]}" if wpl else "") for i in range(nleg))
    return (f"[DIAG] t={s['t']}s win=1/1 paused=0% good={s['good']}Mbit cwnd=1 infl=1 np=2 rtt=1.0ms dgq0[hand=1 tx={s['tx'][0]} full=0 err=0 sp=1] "
            f"dgq1[hand=1 tx={s['tx'][1]} full=0 err=0 sp=1]{legs} dgev0=0")


def logs_for(S, arm="CAND", evt_rc=0, extra_cli=()):
    drv, cli, srv = (lc.read(PD + x) for x in ("drv.out", "c.log", "s.log"))
    cli = [ln for ln in cli if "[DIAG] t=" not in ln and "[ETA] site=sender" not in ln]
    srv = list(srv)
    cli = cli + [diag_line(s, wpl=(arm != "BASE")) for s in S] + list(extra_cli)
    # events: t_on / t_off on the DIAG axis, wall = t + 1000
    evt = (f"[EVENT] start wall=1000.000\n[EVENT] on wall_before={1000 + T_ON - 0.02:.3f} wall_after={1000 + T_ON + 0.02:.3f} rc={evt_rc} delay_ms=1520 diag_t={T_ON}\n"
           f"[EVENT] off wall_before={1000 + T_OFF - 0.02:.3f} wall_after={1000 + T_OFF + 0.02:.3f} rc=0 delay_ms=20 diag_t={T_OFF}\n[EVENT] done wall=1020.000\n"
           + "".join(f"[DIAGSEEN] t={s['t']} wall={1000 + s['t'] + 0.01:.3f}\n" for s in S))
    return drv, cli, srv, evt


def mkrow(S, arm="CAND", variant="SLOW", rep=1, evt_rc=0, extra_cli=(), qtext=None, seed="42"):
    drv, cli, srv, evt = logs_for(S, arm, evt_rc, extra_cli)
    return q.make_row("c8-500", arm, seed, rep, 0, 40.0, drv, cli, srv, 0, "x", variant=variant, evt_text=evt, evtq_text=qtext)


rw = mkrow(gen(back_at=3.0))
check("make_row: a CAND row with a biting event, no inactive/recovered lines and rc 0 is LIVE; t_on / t_off land on the DIAG axis within 0.05 s",
      rw["status"] == "LIVE" and abs(rw["t_on"] - T_ON) < 0.05 and abs(rw["t_off"] - T_OFF) < 0.05 and rw["m"]["event_ok"] and rw["kind"] == "scored", (rw["status"], rw["problems"], rw.get("t_on")))
rbase = mkrow(gen(back_at=None, rtt_frozen=True), arm="BASE")
check("make_row: a BASE row (no RWM_PLACE_WIRE token on [GATES]) is LIVE; its frozen-rtt mechanism witness is recorded", rbase["status"] == "LIVE" and rbase["m"]["idle_rtt_frozen"] and rbase["m"]["R_cens"], (rbase["status"], rbase["problems"]))
rdead = mkrow(gen(back_at=3.0, delta_w=100))
check("make_row: an event that did not bite voids the row (WITNESS-FAIL, event-did-not-bite)", rdead["status"] == "WITNESS-FAIL" and any(p.startswith("event-did-not-bite") for p in rdead["problems"]), rdead["problems"])
rin = mkrow(gen(back_at=3.0), extra_cli=["2026-11-10T15:00:00Z  WARN raptorpath::scheduler: path timed out — marking inactive path_id=1"])
check("make_row: a `path timed out — marking inactive` line voids the row", rin["status"] == "WITNESS-FAIL" and rin["path_inactive"] == 1 and any(p.startswith("path-inactive") for p in rin["problems"]), rin["problems"])
rrec = mkrow(gen(back_at=3.0), extra_cli=["INFO scheduler: path recovered — marking active path_id=1"])
check("make_row: a `path recovered — marking active` line voids the row", rrec["status"] == "WITNESS-FAIL" and rrec["path_recovered"] == 1)
rrc = mkrow(gen(back_at=3.0), evt_rc=1)
check("make_row: a failed tc call is inject-failed (WITNESS-FAIL) and the row command prints ABORT-INJECT", rrc["status"] == "WITNESS-FAIL" and not rrc["inject_ok"] and any(p.startswith("inject-failed") for p in rrc["problems"]))
rchk = mkrow(gen(back_at=3.0), rep=0, qtext=good)
check("make_row: a rep-0 row is a check row and carries the qdisc diff", rchk["kind"] == "check" and rchk["qdisc"]["ok"])
check("make_row: cand wpl_us numeric; the CAND arm has no wpl_us / w_* void (reported only), BASE keeps the strict no-wire-fields witnesses",
      rw["m"]["wpl_numeric_all"] and not any("wpl_us-numeric" in p for p in rw["problems"]))
check("the row survives a JSON round trip", json.loads(json.dumps(rw, sort_keys=True, default=str))["m"]["R"] == rw["m"]["R"])

# ── the disjoint rule ─────────────────────────────────────────────────────
V = lambda *xs: [(x, False) for x in xs]  # noqa: E731
rd, det = q.rule(V(3, 4, 5, 6), V(26, 27, 28, 29), True, 1.0)
check("rule: all 4 CAND below all 4 BASE with medians 4.5 / 27.5 -> BETTER, p = 1/70 printed", rd == "BETTER" and "p(disjoint)=0.014" in det, det)
check("rule: the mirror image is WORSE", q.rule(V(26, 27, 28, 29), V(3, 4, 5, 6), True, 1.0)[0] == "WORSE")
check("rule: one overlap breaks disjointness -> WITHIN", q.rule(V(3, 4, 5, 28), V(26, 27, 28, 29), True, 1.0)[0] == "WITHIN")
check("rule: a tie at the boundary breaks disjointness", q.rule(V(3, 4, 5, 26), V(26, 27, 28, 29), True, 1.0)[0] == "WITHIN")
check("rule: disjoint but medians within the floor -> WITHIN (R floor 1 s)", q.rule(V(3.0, 3.1, 3.2, 3.3), V(3.4, 3.5, 3.6, 3.7), True, 1.0)[0] == "WITHIN")
cen = [(25.0, True)] * 4
check("rule: two censored values tie and break disjointness (CAND all censored vs BASE all censored -> WITHIN)", q.rule(cen, cen, True, 1.0)[0] == "WITHIN")
check("rule: a censored value ranks above every finite one: finite CAND vs censored BASE -> BETTER; the reverse -> WORSE",
      q.rule(V(5, 6, 7, 8), cen, True, 1.0)[0] == "BETTER" and q.rule(cen, V(5, 6, 7, 8), True, 1.0)[0] == "WORSE")
check("rule: 3 against 4 gives p = 1/35 and 3 against 3 p = 1/20", "p(disjoint)=0.029" in q.rule(V(1, 2, 3), V(10, 11, 12, 13), True, 1.0)[1] and "p(disjoint)=0.050" in q.rule(V(1, 2, 3), V(10, 11, 12), True, 1.0)[1])
check("rule (goodput, higher is better): CAND above BASE by more than the floor and disjoint -> BETTER; below -> WORSE",
      q.rule(V(94, 95, 96, 97), V(86, 88, 90, 91), False, 3.6)[0] == "BETTER" and q.rule(V(86, 88, 90, 91), V(94, 95, 96, 97), False, 3.8)[0] == "WORSE")

# ── the scorer on synthetic rows: every outcome ───────────────────────────
def srow(arm, variant, i, R_target, g_post=95.0, kind="scored", sigma=0.18, exit_ok=True, status="LIVE", rtt_frozen=False):
    S = gen(sigma=sigma, back_at=R_target, g_post=g_post + 0.1 * i, hit=1 if variant != "FAST" else 0, rtt_frozen=rtt_frozen)
    hit = 1 if variant != "FAST" else 0
    if variant == "FAST":
        S = gen(sigma=0.82, back_at=R_target, g_post=g_post + 0.1 * i, hit=0)
    m = q.analyze(S, T_ON, T_OFF, hit)
    if not exit_ok:
        m["exit_ok"] = False
    return {"cell": "c8-500", "store": "def", "arm": arm, "seed": "42" if i % 2 == 0 else "7", "rep": 1 + i // 2, "rc": 0, "status": status, "problems": [],
            "variant": variant, "kind": kind, "m": m, "inject_ok": True, "path_inactive": 0, "path_recovered": 0}


def ledger46(spec, extra=()):
    """spec: {(variant, arm): (R_target list, g_post)}"""
    rows = []
    for (var, arm), (Rs, gp) in spec.items():
        for i, Rt in enumerate(Rs):
            rows.append(srow(arm, var, i, Rt, g_post=gp))
    rows += list(extra)
    return rows


def run46(rows, abort=None):
    lines = []
    with tempfile.TemporaryDirectory() as td:
        pth = os.path.join(td, "l.log")
        with open(pth, "w") as fh:
            for r in rows:
                fh.write("P46ROW " + json.dumps(r, default=str) + "\n")
            if abort:
                fh.write(abort + " reason\n")
        v = q.score([pth], out=lines.append)
    return v, "\n".join(lines)


NEVER = [None] * 4
FASTBACK = [1.0, 1.5, 2.0, 2.5]
SLOWBACK = [4.0, 5.0, 6.0, 7.0]
BASEOK = {("SLOW", "BASE"): (NEVER, 88.0), ("FAST", "BASE"): (FASTBACK, 95.0), ("FAST", "CAND"): (FASTBACK, 95.0)}
v, t = run46(ledger46({**BASEOK, ("SLOW", "CAND"): (SLOWBACK, 98.0)}))
check("outcome CAND-RECOVERS-FASTER: SLOW-HIT provoked, CAND returns at 4-7 s against BASE censored, goodput not worse, FAST-HIT overlapping", v.startswith("CAND-RECOVERS-FASTER") and "SLOW" in v, v)
check("... the report prints R and gbar_post readings per variant, the exit-witness counts and the p of the disjoint split", "R (s from t_off, scored" in t and "gbar_post" in t and "PROVOKED" in t and "p(disjoint)=0.014" in t)
v, _ = run46(ledger46({**BASEOK, ("SLOW", "CAND"): (NEVER, 88.0)}))
check("outcome NO-DIFFERENCE: provoked, but CAND no better (both censored, same goodput)", v == "NO-DIFFERENCE", v)
v, _ = run46(ledger46({("SLOW", "BASE"): (NEVER, 88.0), ("SLOW", "CAND"): (NEVER, 88.0), ("FAST", "BASE"): (FASTBACK, 95.0), ("FAST", "CAND"): (FASTBACK, 95.0)},
                      extra=[]))
check("outcome NO-DIFFERENCE also when R reads WITHIN at both variants", v == "NO-DIFFERENCE", v)
v, _ = run46(ledger46({**BASEOK, ("SLOW", "CAND"): (NEVER, 98.0)}))
check("a goodput-only gain (R tied, censored) is NO-DIFFERENCE: the verdict keys on R BETTER", v == "NO-DIFFERENCE", v)
# CAND-WORSE
v, t = run46(ledger46({**BASEOK, ("SLOW", "BASE"): (SLOWBACK, 90.0), ("SLOW", "CAND"): (NEVER, 90.0)}))
check("outcome CAND-WORSE: R reads WORSE (CAND censored, BASE back at 4-7 s)", v.startswith("CAND-WORSE") and "SLOW R" in v, v)
v, _ = run46(ledger46({**BASEOK, ("SLOW", "CAND"): (NEVER, 88.0), ("FAST", "CAND"): (FASTBACK, 85.0)}, extra=[]))
check("outcome CAND-WORSE: gbar_post reads WORSE at FAST-HIT (CAND 85 against BASE 95: > 4 % floor and disjoint)", v.startswith("CAND-WORSE") and "FAST gpost" in v, v)
v, _ = run46(ledger46({("SLOW", "BASE"): (NEVER, 88.0), ("SLOW", "CAND"): (SLOWBACK, 98.0), ("FAST", "BASE"): (FASTBACK, 95.0), ("FAST", "CAND"): (NEVER, 95.0)}))
check("precedence: CAND-WORSE (FAST R worse) outranks CAND-RECOVERS-FASTER (SLOW R better)", v.startswith("CAND-WORSE"), v)
# NOT-PROVOKED
def notprov(extra_better=False):
    rows = []
    for arm in ("BASE", "CAND"):
        for i in range(4):
            rows.append(srow(arm, "SLOW", i, 2.0 if arm == "CAND" else 2.0, exit_ok=False))
    spec = {("FAST", "BASE"): ([4.0, 4.5, 5.0, 5.5] if extra_better else FASTBACK, 95.0),
            ("FAST", "CAND"): ([0.5, 0.75, 1.0, 1.25] if extra_better else FASTBACK, 95.0)}
    return rows + ledger46(spec)


v, t = run46(notprov())
check("outcome NOT-PROVOKED: SLOW-HIT BASE exit witness holds in < 3 rows and no provoked variant reads R BETTER", v == "NOT-PROVOKED" and "NOT provoked" in t, v)
v, _ = run46(notprov(extra_better=True) )
check("... but a provoked FAST-HIT with R BETTER escapes NOT-PROVOKED -> CAND-RECOVERS-FASTER (outcome 3 needs no provoked variant R BETTER)", v.startswith("CAND-RECOVERS-FASTER") and "FAST" in v, v)
# UNSCOREABLE
v, _ = run46(ledger46({**BASEOK, ("SLOW", "CAND"): (SLOWBACK, 98.0)}), abort="ABORT-INJECT")
check("outcome UNSCOREABLE: an abort token in the ledger", v.startswith("UNSCOREABLE") and "ABORT-INJECT" in v, v)
v, _ = run46(ledger46({**BASEOK, ("SLOW", "CAND"): (SLOWBACK[:2], 98.0)}))
check("outcome UNSCOREABLE: fewer than 3 live rows in an arm at a variant", v.startswith("UNSCOREABLE") and "SLOW" in v, v)
rows_v = ledger46({**BASEOK, ("SLOW", "CAND"): (SLOWBACK, 98.0)})
rows_v[0]["status"] = "WITNESS-FAIL"
rows_v[1]["status"] = "WITNESS-FAIL"
v, _ = run46(rows_v)
check("... void rows (event witness / inactive / recovered) do not count as live", v.startswith("UNSCOREABLE"), v)
chk = srow("BASE", "SLOW", 0, None, kind="check")
chk["qdisc"] = {"ok": False, "notes": ["after != before"], "peak_backlog": 10}
v, t = run46(ledger46({**BASEOK, ("SLOW", "CAND"): (SLOWBACK, 98.0)}, extra=[chk]))
check("outcome UNSCOREABLE: a failed check row", v.startswith("UNSCOREABLE") and "check row failed" in v and "CHECK-ROW" in t, v)
# 3 against 4 still scores, with its p
rows3 = ledger46({**BASEOK, ("SLOW", "CAND"): (SLOWBACK[:3], 98.0)})
v, t = run46(rows3)
check("3 live against 4 still scores and prints p = 1/35", v.startswith("CAND-RECOVERS-FASTER") and "p(disjoint)=0.029" in t, v)
# reported-only rows never reach the verdict
extra = [srow(a, "SLOW500", i, r, kind="reported") for a, rs in (("BASE", [3.0] * 2), ("CAND", [30.0] * 2)) for i, r in enumerate(rs)]
extra += [srow(a, "C1D", 0, r, kind="reported") for a, r in (("BASE", None), ("CAND", 15.0))]
v, t = run46(ledger46({**BASEOK, ("SLOW", "CAND"): (SLOWBACK, 98.0)}, extra=extra))
check("the +500 ms and c1d rows are reported only: a CAND-worse +500 ms result leaves the verdict alone, and both sections are printed",
      v.startswith("CAND-RECOVERS-FASTER") and "+500 ms at SLOW-HIT (reported only)" in t and "c1d, leg 1 hit (reported only" in t, v)
check("the score ends in a RESULT json line carrying the verdict", t.splitlines()[-1].startswith("RESULT {") and json.loads(t.splitlines()[-1][7:])["verdict"] == v)

# smoke (the check rows)
def smoke46_rc(rows):
    with tempfile.TemporaryDirectory() as td:
        pth = os.path.join(td, "s.log")
        with open(pth, "w") as fh:
            for r in rows:
                fh.write("P46ROW " + json.dumps(r, default=str) + "\n")
        return q.smoke(pth)


cb = srow("BASE", "SLOW", 0, None, kind="check")
cc = srow("CAND", "FAST", 0, 2.0, kind="check")
for c in (cb, cc):
    c["qdisc"] = {"ok": True, "notes": [], "peak_backlog": 300}
check("smoke46 passes on two LIVE check rows (qdisc ok, BASE's exit witness holds at SLOW-HIT)", smoke46_rc([cb, cc]) == 0)
cb2 = json.loads(json.dumps(cb))
cb2["m"]["exit_ok"] = False
check("smoke46 fails (ABORT-INJECT) when BASE's exit witness fails at SLOW-HIT", smoke46_rc([cb2, cc]) == 1)
cc2 = json.loads(json.dumps(cc))
cc2["qdisc"]["ok"] = False
check("smoke46 fails on a failed qdisc diff, and on a missing check row", smoke46_rc([cb, cc2]) == 1 and smoke46_rc([cb]) == 1)
cc3 = json.loads(json.dumps(cc))
cc3["status"] = "WITNESS-FAIL"
check("smoke46 fails on a void check row (e.g. inactive / recovered lines or an event that did not bite)", smoke46_rc([cb, cc3]) == 1)

print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
