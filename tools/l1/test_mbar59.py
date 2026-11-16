#!/usr/bin/env python3
"""Tests for the status 59 harness (the evicting-seat m-bar readout at c2-100; REPORTED ONLY, no build): tools/l1/mbar59_score.py, the place38_parse parts, the driver / battery / run_all wiring.

Layers:
  1. the plan table (5 slots, seeds 42 / 7 / 42 / 7 / 42), the smoke row, the arm registration, the shell side (hint, arm env, scrub, size, budget, stage, HARNESS_PATHS) and the driver's
     RWM_C_RELIABLE switch EXECUTED on the real text of perf_rwm_c.sh (the stub driver of test_stubrun.py replaces that script, so only this test runs the flag logic);
  2. the readouts on a synthetic client log with KNOWN answers: release batch d(cum)/d(wake ack), per loop d(cum)/sum(wait n) with a NON-CONSTANT per-period n (wait n resets every tick, so
     last minus first would be wrong), the per-tick range, cod/src, the [SPAN] medians (an early line and a None line must not move them), the shed counts, the LATE cut and the warm-up cut;
  3. the evicting-seat liveness witnesses, the replacement rule, the smoke criteria and a whole report.
Run: python3 test_mbar59.py (the VM runs it under `tst parsers`). The end-to-end path through loopdiag_run_all.sh (plan 59) runs in test_stubrun.py."""
import json
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_WITNESS54", "LD_SHA", "LD_SHA_BASE", "LD_BASE_SHA", "LD_ROW_BYTES"):
    os.environ.pop(_k, None)
import place38_parse as pp  # noqa: E402
import mbar59_score as m59  # noqa: E402
import harness_paths as hp  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


def near(a, b, tol=1e-9):
    return a is not None and abs(a - b) <= tol


def rd(name):
    return open(os.path.join(HERE, name), encoding="utf-8", errors="replace").read()


runall, battery, driver = rd("loopdiag_run_all.sh"), rd("loopdiag_battery.sh"), rd("perf_rwm_c.sh")

# ═══ 1. plan table, registration, wiring ═══
T = pp.plan59()
check("the table is 5 slots of c2-100 / EVICT, seeds 42 / 7 / 42 / 7 / 42, reps 1..5", T == [("c2-100", "def", "EVICT", s, i + 1) for i, s in enumerate(("42", "7", "42", "7", "42"))], T)
check("the smoke is ONE c2-100 EVICT row (seed 42, rep 1)", pp.plan59(smoke=True) == [("c2-100", "def", "EVICT", "42", 1)])
check("the arm is registered as window / Rlc / REALTIME / ACTIVE and rides the candidate binary family", pp.sp.ARM_SPEC["EVICT"] == ("window", "Rlc", "realtime", "ACTIVE") and pp.AFAM["EVICT"] == "CAND")
check("c2-100 is a single-path cell (no c3 cell exists: c8 = c2 + c3 is dual)", pp.sp.N_LEGS["c2-100"] == 1 and "c3-100" not in pp.sp.CELL_SPEC and pp.BYTES59 == {"c2-100": 600000000})
check("HARNESS_PATHS lists plan 59 with rehearse / full / reuse-hit / reuse-miss", hp.HARNESS_PATHS.get("59") == ("rehearse", "full", "reuse-hit", "reuse-miss"), hp.HARNESS_PATHS.get("59"))
check("battery: plan 59 is an accepted plan, P59 is single-seat (P38 + PCAND, no P41), HINT is realtime only under P59",
      "59|57a|57b|56|41d" in battery and 'P59=0; [ "$PLAN" = "59" ] && { P59=1; P38=1; PCAND=1; }' in battery and 'HINT=bulk; [ "$P59" = 1 ] && HINT=realtime' in battery)
check("battery: the driver gets $HINT (never a hardcoded bulk) and the header echo reads it", 'bash perf_rwm_c.sh "$ca" "$cb" "$HINT"' in battery and "hint=$HINT spec=" in battery and " bulk \"$bytes\"" not in battery)
check("battery: arm EVICT = RWM_C_RELIABLE=0 ONLY (RWM_UNIFIED / RWM_UNIFIED_SHED default on in gates.rs, so the arm sets nothing for them), any other arm refused under plan 59",
      "EVICT) arm_env=(RWM_C_RELIABLE=0) ;;" in battery and 'env_flag("RWM_UNIFIED", true)' in open(os.path.join(HERE, "..", "..", "raptorpath", "src", "gates.rs"), encoding="utf-8").read()
      and 'env_flag("RWM_UNIFIED_SHED", true)' in open(os.path.join(HERE, "..", "..", "raptorpath", "src", "gates.rs"), encoding="utf-8").read() and "UNKNOWN-ARM $arm (plan 59: EVICT)" in battery)
check("battery: the env scrub withholds RWM_UNIFIED, RWM_UNIFIED_SHED and RWM_C_RELIABLE (an ambient value reaches no row), the instrument env is RWM_RDIAG=1 with a 150 s timeout",
      "-u RWM_UNIFIED -u RWM_UNIFIED_SHED -u RWM_C_RELIABLE" in battery and "[ \"$P59\" = 1 ] && INSTR_ENV=(RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=150)" in battery and m59.PERF_TIMEOUT_S == 150)
check("battery: the size is p59_bytes (600 MB at c2-100 only) = BYTES59, the binary must carry the shed gate, the table sizes are 5 / 1, the replacement is replace59",
      'p59_bytes() { case "$1" in c2-100) echo 600000000 ;;' in battery and 'preflight_binary "$BIN" "RWM_UNIFIED_SHED" "unified overload shedding ACTIVE"' in battery
      and '-eq 5 ] || { echo "ABORT-PLAN59 table' in battery and '-eq 1 ] || { echo "ABORT-PLAN59 smoke' in battery and "place38_parse.py replace59" in battery)
check("run_all: plan 59 is selectable, rehearsable, single-seat (rides P57's gates), has its stage rep59 with smoke59 / score59 / check59 and its own tests",
      "  59) P_SMOKE=smoke59; P_SCORE=score59; P_CHECK=check59" in runall and "57a|57b|59|60a|60b) ;; *) echo \"ABORT-REHEARSE" in runall and 'P59=0; [ "$PLAN" = "59" ] && { P59=1; P46=1; P57=1; }' in runall
      and "59) smk=smoke59; scr=score59; chk=check59 ;;" in runall and "stage rep59 59" in runall and "./test_mbar59.py" in runall)
check("run_all: the budget guard for plan 59 is 7 200 s (the worst case with a reference-manifest MISS) and names the plan; LD_BASE_SHA is not required",
      'P57_NEED=7200; P57_TAG="59"' in runall and "ABORT-BUDGET plan $P57_TAG" in runall and ': "${LD_BASE_SHA:?' in runall and '"$PLAN" = "59"' not in runall.split(': "${LD_BASE_SHA:?')[0].split("if [ \"$P41D\" = 1 ]")[-1])
for f in ("mbar59_score.py", "test_mbar59.py"):
    check(f"the CRLF guard lists {f} in run_all and battery", f in runall.split("crlf_guard", 1)[1].split("ROOT=")[0] and f in battery.split("crlf_guard", 1)[1].split("BIN=")[0])


def drv_default(gen, reliable=None):
    """Run the REAL flag construction of perf_rwm_c.sh (the RWM_GEN gating through the RWM_C_RELIABLE block, with lib.sh sourced) -> (rc, WR_FLAG, GEN_FLAG)."""
    i = driver.index('GEN_GATE="${RWM_GEN:-1}"')
    j = driver.index("esac\n", driver.index('RELIABLE="${RWM_C_RELIABLE:-1}"')) + 5
    env = {"PATH": os.environ.get("PATH", ""), "SYSTEMROOT": os.environ.get("SYSTEMROOT", "")}
    if gen is not None:
        env["RWM_GEN"] = gen
    if reliable is not None:
        env["RWM_C_RELIABLE"] = reliable
    script = 'set -uo pipefail; source ./lib.sh; PLACE_T=""; TENV=""\n' + driver[i:j] + '\necho "WR=[$WR_FLAG] GEN=[$GEN_FLAG]"'
    r = subprocess.run(["bash", "-c", script], capture_output=True, text=True, env=env, cwd=HERE)
    w = re.search(r"WR=\[(.*)\] GEN=\[(.*)\]", r.stdout)
    return (r.returncode,) + (w.groups() if w else (None, None))


def drv_switch(reliable, gen_flag):
    """Run the REAL RWM_C_RELIABLE text of perf_rwm_c.sh with the given inputs -> (rc, WR_FLAG)."""
    m = re.search(r'^RELIABLE="\$\{RWM_C_RELIABLE:-1\}"\ncase "\$RELIABLE" in\n.*?\nesac\n', driver, re.S | re.M)
    if not m:
        return None, None
    env = dict(os.environ)
    env.pop("RWM_C_RELIABLE", None)
    if reliable is not None:
        env["RWM_C_RELIABLE"] = reliable
    bash = "bash"
    r = subprocess.run([bash, "-c", f'WR_FLAG="--window-reliable"; GEN_FLAG="{gen_flag}"\n{m.group(0)}\necho "WR=[$WR_FLAG]"'], capture_output=True, text=True, env=env)
    w = re.search(r"WR=\[(.*)\]", r.stdout)
    return r.returncode, (w.group(1) if w else None)


try:
    check("driver: unset / =1 keeps --window-reliable (every other plan unchanged)", drv_switch(None, "") == (0, "--window-reliable") and drv_switch("1", "--window-generation-coding") == (0, "--window-reliable"), drv_switch(None, ""))
    check("driver: RWM_C_RELIABLE=0 empties the flag with RWM_GEN=0", drv_switch("0", "") == (0, ""), drv_switch("0", ""))
    check("driver: =0 with generation coding on is refused (rc 2), as is any other value", drv_switch("0", "--window-generation-coding")[0] == 2 and drv_switch("2", "")[0] == 2)
except FileNotFoundError:
    print("SKIP the driver switch execution: no bash on PATH")
try:
    dflt = {g: drv_default(g) for g in (None, "0", "1")}
    check("driver DEFAULT branch (RWM_C_RELIABLE unset): --window-reliable is still passed for RWM_GEN unset, 0 and 1 (generation flag on for unset/1, off for 0)",
          all(v[0] == 0 and v[1] == "--window-reliable" for v in dflt.values()) and dflt[None][2] == "--window-generation-coding" and dflt["1"][2] == "--window-generation-coding" and dflt["0"][2] == "", dflt)
    check("driver: RWM_C_RELIABLE=0 with RWM_GEN=0 (the only legal pairing) drops the flag in the full flag construction; with RWM_GEN unset/1 it is refused (rc 2)",
          drv_default("0", "0") == (0, "", "") and drv_default(None, "0")[0] == 2 and drv_default("1", "0")[0] == 2, (drv_default("0", "0"), drv_default(None, "0")))
except FileNotFoundError:
    print("SKIP the driver default-branch execution: no bash on PATH")
check("driver: the header echo ends with reliable=$RELIABLE and the new variable is not an engine gate (not in lib.sh's forward list)", 'start=$(date +%T) reliable=$RELIABLE"' in driver and "RWM_C_RELIABLE" not in rd("lib.sh"))

# ═══ 2. readouts on a synthetic log with known answers ═══
SRC_UNTIL = 48            # source until tick 48 (t = 12 s); ticks 49.. carry src = 0


def inc_ack(k):
    return 200 if k % 2 == 0 else 280


def n_loop(k):
    return 80 if k % 2 == 0 else 120


def diag_line(k):
    t = 0.25 * k
    kk = min(k, SRC_UNTIL)
    ack = sum(inc_ack(j) for j in range(1, kk + 1))
    src = 2000 if k <= SRC_UNTIL else 0
    return (f"[DIAG] t={t:.1f}s win=800/1000 paused=0% good={80.0 + k:.1f}Mbit ackrate_ewma=0sym/s eff_pace=0sym/s src={src}sym/s cod={320 if src else 0}sym/s cum={250 * kk}/{40 * kk}/{ack} "
            f"sidle=0ms/0/mx0ms cwnd=1 infl=1 np=1 np_act=1 rtt=5.0ms bdp100=165sym sweeps=0 retx=0 gapdrop=0 nbud=0 "
            f"p0:infl=1/sinfl=0/bdp1(cap0) sout=0 khr=1.00/kraw=- btlbw=1 sr=0/g0d0 est=Y pl=0.0150 cmp=- rtt=5/wrtt=3/rtp2ms rtp_us=1 "
            + (f"shed={3 * kk}/{kk} bud=0.0500 D=40ms " if SHED else "")
            + f"wait[tun=1% paused=0% pace=0% gen=0% nack=0% defc=0% tail=0% flush=0% ack=82% cmd=0% n={n_loop(k)} us=250000 busy=45% busy_us=112500] "
              f"wake[tun=1 paused=0 pace=0 gen=0 nack=0 defc=0 tail=0 flush=0 ack={20 * k} cmd=1 timer_acked=0]")


def span_line(j, early=False, none=False):
    sh = f" shed={3 * j}/{j} bud=0.0500 D=40ms" if SHED else ""
    a, d = ("None", "None") if none else ("Some(12)", "Some(0.5)")
    rr = 0.9 if early else 0.1 + 0.001 * j
    return f"[SPAN] t={0.5 * j:.1f}s ack={240 * j} win=[{240 * j},{240 * j + 64}] wsize=64 a_star={a} delta={d} owed=0.10 rr={rr:.3f} debt=0.30 retx_buf=5{sh} | shim enq=1 ge=0 tail=0 ok=1 err=0 q=0"


SHED = True


def cli_log(shed=True, n_ticks=60):
    global SHED
    SHED = shed
    L = ['[DIAG] t=9.9s win=1/1 src=5000sym/s cod=9sym/s cum=999999/9/999999 wait[tun=1% ack=1% n=9999999 us=1 busy=1% busy_us=1] wake[tun=1 ack=999999]',   # before the marker: ignored
         '{"seconds":0.01,"warmup":true}']
    for k in range(1, n_ticks + 1):
        L.append(diag_line(k))
        if k % 2 == 0:
            j = k // 2
            L.append(span_line(j, early=(0.5 * j < 5.0), none=(j == 12)))
    return L


m = m59.row_metrics(cli_log(), [], [], 600000000, "c2-100", 123.4)
# LATE = t >= 5 s with src > 0 -> ticks 20..48 (29 ticks); first a = tick 20, last b = tick 48
ACK_D = sum(inc_ack(k) for k in range(21, 49))
WAKE_D = 20 * 28
N_SUM = sum(n_loop(k) for k in range(21, 49))
check("LATE ticks: t >= 5 s with src > 0 (29), the stale line before the warm-up marker is ignored", m["late_n"] == 29, m["late_n"])
check("cum deltas over LATE: src +7000, cod +1120, ack +6720; wake ack +560", (m["cum_src_d"], m["cum_cod_d"], m["cum_ack_d"], m["wake_ack_d"]) == (7000, 1120, ACK_D, WAKE_D) and ACK_D == 6720, (m["cum_src_d"], m["cum_ack_d"], m["wake_ack_d"]))
check("loop iterations = the SUM of the per-period wait n over the ticks after the first LATE tick (2 800), not last minus first (which would read 40)", m["loop_n"] == N_SUM == 2800, m["loop_n"])
check("release batch d(cum)/d(wake ack) (bounds nothing): 12.5 on the src field, 12.0 on the ack field", near(m["rel_src"], 12.5) and near(m["rel_ack"], 12.0), (m["rel_src"], m["rel_ack"]))
check("mbar_lo = d(cum)/sum(wait n), the LOWER bound on sources per advance: 2.5 on the src field, 2.4 on the ack field", near(m["loop_src"], 2.5) and near(m["loop_ack"], 2.4) and near(m["mbar_lo"], 2.4) and near(m["mbar_lo_src"], 2.5), (m["loop_src"], m["loop_ack"]))
check("W1 bracket: upper = (cod/src)/rr = 0.16/0.12; lower = max(0, 1 - 1/(0.12 x 2.4)) = 0 (vacuous at m-bar_lo 2.4: negative, clamped)", near(m["w1_hi"], 0.16 / 0.12) and m["w1_lo"] == 0.0, (m["w1_lo"], m["w1_hi"]))
m10 = m59.row_metrics([re.sub(r" n=\d+ ", " n=10 ", ln) for ln in cli_log()], [], [])
check("W1 lower bound with a per-loop count of 10 (mbar_lo = 6720/280 = 24): 1 - 1/(0.12 x 24) = 0.65278", near(m10["mbar_lo"], 24.0) and near(m10["w1_lo"], 1 - 1 / (0.12 * 24.0), 1e-9), (m10["mbar_lo"], m10["w1_lo"]))
check("eps_hat = the median per-path pl= over LATE (0.015); no FDIAG on this log -> None", near(m["eps_hat"], 0.015) and m["fdiag_rf"] is None, m["eps_hat"])
mf = m59.row_metrics(cli_log(), ["[FDIAG] frontier=1 seen=2 | rf=7 ru=3 x"], [])
check("FDIAG rf / ru are read from the server log when RWM_FDIAG=1 printed them", (mf["fdiag_rf"], mf["fdiag_ru"]) == (7, 3))
check("per-tick range of the ack release batch: median 12, min 10, max 14; of the ack per-loop value: median 2.41667, min 2.33333, max 2.5",
      near(m["rel_ack_med"], 12.0) and near(m["rel_ack_min"], 10.0) and near(m["rel_ack_max"], 14.0)
      and near(m["loop_ack_med"], (7 / 3 + 2.5) / 2, 1e-9) and near(m["loop_ack_min"], 7 / 3) and near(m["loop_ack_max"], 2.5),
      (m["rel_ack_med"], m["rel_ack_min"], m["rel_ack_max"], m["loop_ack_med"], m["loop_ack_min"], m["loop_ack_max"]))
check("cod/src: 0.16 on the DIAG rates (320 / 2000 over the 29 LATE ticks, src > 1000 sym/s) and 0.16 cumulative (+1120 / +7000)", near(m["cod_src_rate"], 0.16) and m["cod_src_n"] == 29 and near(m["cod_src_cum"], 0.16),
      (m["cod_src_rate"], m["cod_src_n"], m["cod_src_cum"]))
check("the sweep points not excluded by mbar_lo = 2.4 are {4, 16, 64, 256}", m["grid_above"] == [4, 16, 64, 256], m["grid_above"])
check("[SPAN] medians over t >= 5 s only (the 0.9 early lines are cut, the None line is skipped): rr 0.12 (median of 0.110..0.130 less the None line's neighbours), debt 0.3, a_star 12, delta 0.5, bud 0.05",
      near(m["rr"], 0.12, 1e-9) and near(m["debt"], 0.3) and near(m["a_star"], 12.0) and near(m["delta"], 0.5) and near(m["bud"], 0.05), (m["rr"], m["debt"], m["a_star"], m["delta"], m["bud"]))
check("the shed counts are the LAST cumulative [SPAN] value (90 shed, 30 budget-refused) and the DIAG shed token reads the last tick's (144)", (m["shed_total"], m["shed_denied"], m["shed_diag_total"]) == (90, 30, 144), (m["shed_total"], m["shed_denied"], m["shed_diag_total"]))
check("goodput: the [DIAG] good= median over LATE (ticks 20..48: 80 + 34) and the summary value passed in (a DNF passes None)", near(m["goodput_diag"], 80.0 + 34) and m["goodput_summary"] == 123.4, m["goodput_diag"])
check("the armed seat reads true on the span and DIAG gauges; no readout is missing", m["shed_armed_span"] and m["shed_armed_diag"] and m59.missing({"t59": m}) == [], m59.missing({"t59": m}))
# the cod/src rate form with a real source rate
L2 = [ln.replace("src=2000sym/s cod=320sym/s", "src=1000sym/s cod=300sym/s") for ln in cli_log()]
m2 = m59.row_metrics(L2, [], [], None)
check("cod/src reads only ticks with src ABOVE 1000 sym/s (strict): at exactly 1000 sym/s the rate form is None and the cumulative form is unaffected", m2["cod_src_rate"] is None and m2["cod_src_n"] == 0 and near(m2["cod_src_cum"], 0.16), (m2["cod_src_rate"], m2["cod_src_n"]))
mu = m59.row_metrics(cli_log(shed=False), [], [], None)
check("an unarmed log (no shed= anywhere): shed readouts None, armed flags false, and the smoke would name shed_total / bud", not mu["shed_armed_span"] and not mu["shed_armed_diag"] and mu["shed_total"] is None
      and {"bud", "shed_total"} <= set(m59.missing({"t59": mu})), m59.missing({"t59": mu}))
me = m59.row_metrics([], [], [], None)
check("an empty log reads None everywhere and never raises", me["rel_ack"] is None and me["late_n"] == 0 and len(m59.missing({"t59": me})) == len(m59.LIVE_TOKENS) + len(m59.SPAN_TOKENS))
# a DNF-style log: ticks stop early (t < 5 s only) -> no LATE window
ms = m59.row_metrics(cli_log(n_ticks=16), [], [], None)
check("a row that dies before 5 s has no LATE window (late_n 0, every [DIAG] delta None)", ms["late_n"] == 0 and ms["rel_ack"] is None and ms["loop_n"] is None, (ms["late_n"], ms["rel_ack"]))

# ═══ 3. witnesses, replacement, smoke, report ═══
G = "[GATES] RWM_UNIFIED=1 RWM_UNIFIED_SHED=1 RWM_TAPER_R=1"
RT = "INFO raptorpath::net: " + m59.REALTIME_ECHO
good_c = [G, RT, "INFO raptorpath::net: " + pp.SHED_ECHO59 + "RWM_UNIFIED_SHED: ...)", span_line(12)]
good_s = [G, RT, "INFO raptorpath::net: unified overload shedding ACTIVE at receiver (...)"]
SHA = "c" * 64
check("a fully armed evicting seat has no problem", pp.evict_problems59(good_c, good_s, SHA, SHA) == [], pp.evict_problems59(good_c, good_s, SHA, SHA))


def probs(c=None, s=None):
    return pp.evict_problems59(good_c if c is None else c, good_s if s is None else s, SHA, SHA)


check("no sender shed echo -> evict-sender-shed-echo-missing; the receiver's echo is reported, not required", "evict-sender-shed-echo-missing" in probs(c=[x for x in good_c if "shedding ACTIVE" not in x])
      and probs(s=[x for x in good_s if "at receiver" not in x]) == [])
check("a retain-until-acked echo on either end -> evict-reliable-echo-present", "evict-reliable-echo-present" in probs(c=good_c + ["reliable window mode (retain until acked): auto-selecting RLC windowed backend"])
      and "evict-reliable-echo-present" in probs(s=good_s + ["reliable window mode (retain until acked): auto-selecting RLC windowed backend"]))
check("no shed= on any [SPAN] line (the law is not armed even when the echo is) -> evict-no-shed-gauge-on-span", "evict-no-shed-gauge-on-span" in probs(c=[x for x in good_c if "[SPAN]" not in x]) and
      "evict-no-shed-gauge-on-span" in probs(c=[x if "[SPAN]" not in x else re.sub(r" shed=\S+", "", x) for x in good_c]))
check("RWM_UNIFIED / RWM_UNIFIED_SHED must read 1 on both ends (the shed gate alone is echoed on a reliable row too, so it never stands in for the liveness)",
      any(x.startswith("gate-unified-RWM_UNIFIED_SHED-srv=0") for x in probs(s=[G.replace("SHED=1", "SHED=0"), RT]))
      and any(x.startswith("gate-unified-RWM_UNIFIED-cli=None") for x in probs(c=["INFO plain"] + good_c[1:])))
check("no Realtime span-machine echo on an end -> no-realtime-echo-<side>; a wrong binary -> bin-sha", "no-realtime-echo-srv" in probs(s=[G]) and any(x.startswith("bin-sha=") for x in pp.evict_problems59(good_c, good_s, "d" * 64, SHA))
      and any(x.startswith("bin-sha-uncheckable") for x in pp.evict_problems59(good_c, good_s, SHA, None)))
check("only the liveness problems void (evict-*, gate-unified*): a wrong binary or a missing Realtime echo is a witness failure that is NOT replaced", pp.is_evict_void59({"problems": ["evict-no-shed-gauge-on-span"]})
      and pp.is_evict_void59({"problems": ["gate-unified-RWM_UNIFIED-cli=0 (want 1)"]}) and not pp.is_evict_void59({"problems": ["bin-sha=abc (want d)", "no-realtime-echo-cli"]}) and not pp.is_evict_void59({"problems": []}))


def row(seed, rep, status="LIVE", problems=(), dnf=False, t59="auto", rc=0, byt=600000000, wall=60.0):
    if t59 == "auto":
        t59 = dict(m)
    return {"cell": "c2-100", "arm": "EVICT", "seed": seed, "rep": rep, "store": "def", "status": status, "problems": list(problems), "dnf": dnf, "rc": rc, "bytes": byt, "wall_s": wall, "mbps": None if dnf else 88.0, "t59": t59}


def ledger(rows, extra=()):
    d = tempfile.mkdtemp(prefix="mbar59-")
    path = os.path.join(d, "ld.log")
    with open(path, "w") as fh:
        for r in rows:
            fh.write("P38ROW " + json.dumps(r) + "\n")
        for x in extra:
            fh.write(x + "\n")
    return path


def run_py(args):
    r = subprocess.run([sys.executable, os.path.join(HERE, "place38_parse.py")] + args, capture_output=True, text=True, cwd=HERE)
    return r.returncode, r.stdout.strip()


void1 = row("42", 1, "WITNESS-FAIL", ["evict-sender-shed-echo-missing"])
check("replace59: ONE VOID-by-liveness row in the slot -> prints evict-liveness, rc 0 (the battery re-runs it once)", run_py(["replace59", ledger([void1]), "c2-100", "EVICT", "42", "1"]) == (0, "evict-liveness"))
check("replace59: never twice (two rows in the slot), never a LIVE row, never a non-liveness failure, never a DNF crash", run_py(["replace59", ledger([void1, void1]), "c2-100", "EVICT", "42", "1"])[0] == 1
      and run_py(["replace59", ledger([row("42", 1)]), "c2-100", "EVICT", "42", "1"])[0] == 1
      and run_py(["replace59", ledger([row("42", 1, "WITNESS-FAIL", ["bin-sha=x (want y)"])]), "c2-100", "EVICT", "42", "1"])[0] == 1
      and run_py(["replace59", ledger([row("42", 1, "VOID-RC", [], rc=1)]), "c2-100", "EVICT", "42", "1"])[0] == 1)
full = [row(s, i + 1) for i, s in enumerate(("42", "7", "42", "7", "42"))]
check("check59: the 5 slots + LD-BATTERY-DONE -> rc 0; a replaced slot (two rows) still passes; a missing slot or an unfinished battery fails",
      run_py(["check59", ledger(full, ["LD-BATTERY-DONE"])])[0] == 0 and run_py(["check59", ledger([void1] + full, ["LD-BATTERY-DONE"])])[0] == 0
      and run_py(["check59", ledger(full[:4], ["LD-BATTERY-DONE"])])[0] == 1 and run_py(["check59", ledger(full)])[0] == 1)
check("plan59 / plan59 smoke print the table lines the battery reads", run_py(["plan59"])[1].splitlines() == ["%s %s %s %s %d" % x for x in T] and run_py(["plan59", "smoke"])[1] == "c2-100 def EVICT 42 1")


def smoke_of(rows):
    r = subprocess.run([sys.executable, os.path.join(HERE, "loopdiag_parse.py"), "smoke59", ledger(rows)], capture_output=True, text=True, cwd=HERE)
    return r.returncode, r.stdout


rc, out = smoke_of([row("42", 1)])
check("smoke59: a good row passes every criterion (rc 0, SMOKE-PASS)", rc == 0 and "FAIL " not in out and "SMOKE-PASS" in out, out[-900:])
rc, out = smoke_of([row("42", 1, dnf=True)])
check("smoke59: a DNF row (an evicting seat may never complete the object) is NOT a failure", rc == 0 and "SMOKE-PASS" in out, out[-600:])
rc, out = smoke_of([row("42", 1, "WITNESS-FAIL", ["evict-sender-shed-echo-missing"])])
check("smoke59: an unarmed row fails the evicting-seat criterion", rc == 1 and re.search(r"^FAIL the evicting seat is armed", out, re.M) is not None, out[-700:])
rc, out = smoke_of([row("42", 1, t59=dict(mu))])
check("smoke59: a row whose [SPAN] shed / bud tokens did not print fails the readout criterion, naming them", rc == 1 and re.search(r"^FAIL every \[DIAG\] / \[SPAN\] readout computed.*shed_total", out, re.M) is not None, out[-700:])
rc, out = smoke_of([row("42", 1, rc=1, status="VOID-RC")])
rc, out = smoke_of([row("42", 1, t59=dict(m, rr=0.0))])
check("smoke59 go/no-go: a row whose median rr is <= 0 (the debt loop never runs) fails the smoke (-> ABORT-SMOKE in the stage), and the GO/NO-GO line prints rr and a_star medians",
      rc == 1 and re.search(r"^FAIL the debt loop runs", out, re.M) is not None, out[-600:])
rc, out = smoke_of([row("42", 1)])
check("smoke59: the GO/NO-GO line prints the rr median (0.12) and the a_star median (12.0)", "rr median 0.120" in out and "a_star median 12.0" in out, out[-500:])
rc, out = smoke_of([row("42", 1, rc=1, status="VOID-RC")])
check("smoke59: a crash (rc != 0) is a failure, not a DNF", rc == 1 and re.search(r"^FAIL every smoke row returned", out, re.M) is not None, out[-700:])
rc, out = smoke_of([])
check("smoke59: no row at all fails", rc == 1 and "no row" in out)

rows = [row(s, i + 1) for i, s in enumerate(("42", "7", "42", "7", "42"))]
rows[1] = row("7", 2, dnf=True)
rep = subprocess.run([sys.executable, os.path.join(HERE, "loopdiag_parse.py"), "score59", ledger([void1] + [row("42", 1)] + rows[1:], ["LD-BATTERY-DONE"])], capture_output=True, text=True, cwd=HERE).stdout
mv = re.search(r"^VERDICT59 REPORTED-ONLY slots=(\d+)/(\d+) live=(\d+) void=(\d+) replaced=(\d+) dnf=(\d+) mbar_lo_med=(\S+) w1_lo_med=(\S+) w1_hi_med=(\S+) release_ack_med=(\S+)", rep, re.M)
check("score59: one VERDICT59 line counts the slots (5/5), live rows (5), replaced (1), DNF (1) and carries mbar_lo 2.400, W1 0.000 .. 1.333, release 12.00; the replaced row is listed", mv is not None and mv.groups() == ("5", "5", "5", "0", "1", "1", "2.400", "0.000", "1.333", "12.00")
      and rep.count("  REPLACED ") == 1, rep[-900:])
check("score59: the readout table has every registered line (mbar_lo, W1 bracket, release batch, eps_hat, cod/src, [SPAN] rr / debt / a_star / delta / bud, shed, goodput) and the m-bar lower bound with the W1 bracket", all(x in rep for x in
      ("release batch d(cum src)/d(wake ack) [no bound]", "mbar_lo  d(cum ack)/sum(wait n)", "W1 lower", "W1 upper", "eps_hat", "cod/src (DIAG rates", "[SPAN] rr", "[SPAN] debt", "[SPAN] a_star", "[SPAN] delta", "[SPAN] bud", "shed total", "goodput Mbit/s"))
      and "m-bar >= 2.400" in rep and "not excluded by it: 4, 16, 64, 256" in rep and "0.000 <= W1 <= 1.333" in rep, rep[-1500:])
check("score59 states what it is: reported only, nothing scored", "REPORTED ONLY" in rep and "nothing here is scored" in rep)

# ═══ END ═══
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
