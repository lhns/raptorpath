#!/usr/bin/env python3
"""Tests for status 51's harness wiring (plans 51 and 51e: plan51.py / plan51e.py, the place38_parse hooks, the battery / run_all branches, the perf header). The scorers' own tests are test_s51.py and
test_s51e.py; the end-to-end launch paths run in test_stubrun.py. docs/derivations/s51-design.md section 6, s51-evict-design.md sections 9.2 and 10 / 11.

  1. the existing plans' lines are intact (test_harness60.py / test_s60.py must pass unchanged: the new branches are ADDED, never generalised);
  2. battery: plans accepted, P51 / P51E single-seat, the per-row armspec, the scrub (SCRUB_EXTRA, only for these two plans), the table-size asserts, replacement, per-block budget, binary preflights;
  3. run_all: gates (LD_BASE_SHA pin, LD_CELL, the registered bin_sha, the strict manifest, no suites), the dial pins, the budgets, the stage's scorer calls, the CRLF guards, LD_HARD / LD_CELL / LD_ONLY through `sudo -n env`;
  4. the perf header echoes the pacing rate (lib.sh's RWM_FORWARD belongs to the engine branch's commit A and is not touched here).
Run: python3 test_harness51.py"""
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_WITNESS54", "LD_SHA", "LD_SHA_BASE", "LD_BASE_SHA", "LD_ROW_BYTES", "LD_NOGO", "LD_CELL", "LD_ONLY"):
    os.environ.pop(_k, None)
import harness_paths as hp  # noqa: E402
import place38_parse as pp  # noqa: E402
import plan51  # noqa: E402
import plan51e  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


def rd(name):
    return open(os.path.join(HERE, name), encoding="utf-8", errors="replace").read()


runall, battery, perf = rd("loopdiag_run_all.sh"), rd("loopdiag_battery.sh"), rd("perf_rwm_c.sh")

# ═══ 1. the older plans' lines are intact (the s60 / harness60 tests read them) ═══
check("battery: the plan case still starts with 60a|60b|59|57a|57b|56 and the per-plan scrub line still reads '-u RWM_C_RELIABLE -u RWM_SLAW -u RWM_THREE_TERM' (the 60 tests' substrings)",
      'case "$PLAN" in 60a|60b|59|57a|57b|56' in battery and "-u RWM_C_RELIABLE -u RWM_SLAW -u RWM_THREE_TERM" in battery and 'read -r -a arm_env <<< "$p60_arm_env"' in battery)
check("run_all: the ABORT-REHEARSE case line is the registered one, byte for byte (plans 51 / 51e are rehearsable through a separate guard)",
      'case "$PLAN" in 62r0|61a) ;; 41c|41d|46|47|48|52|53|54b|56|57a|57b|59|60a|60b|65|67) ;; *) echo "ABORT-REHEARSE' in runall and 'if [ "$REHEARSE" = 1 ] && [ "$PLAN" != "51" ] && [ "$PLAN" != "51e" ] && [ "$PLAN" != "63" ]; then' in runall)

# ═══ 2. the battery ═══
check("battery: plans 51 / 51e accepted; both single-seat (P38 + PCAND, no P41); plan 51e's instrument env is RWM_RDIAG=1 + the arm's timeout (no RWM_RTOBS), plan 51's the guard env at 200 s",
      "|56|41d|51|51e|24|" in battery and "|46|48|52|53|54b|63|64|65) ;; *) echo \"ABORT-PLAN LD_PLAN=$PLAN" in battery and 'P51=0; [ "$PLAN" = "51" ] && { P51=1; P38=1; PCAND=1; }' in battery and 'P51E=0; [ "$PLAN" = "51e" ] && { P51E=1; P38=1; PCAND=1; }' in battery
      and '[ "$P51E" = 1 ] && INSTR_ENV=(RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=75)' in battery and '[ "$P51" = 1 ] && INSTR_ENV=(RWM_RTOBS=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=200)' in battery)
check("battery: the env scrub of section 4 (RWM_DELTA RWM_COPA_DELTA RWM_COPA_WIRE RWM_COMPLETION_EXPOSURE RWM_MIN_R RWM_CC_PACE and the pacing knob) is SCRUB_EXTRA, set for plans 51 / 51e ONLY, and rides before the arm env",
      "SCRUB_EXTRA=(-u RWM_DELTA -u RWM_COPA_DELTA -u RWM_COPA_WIRE -u RWM_COMPLETION_EXPOSURE -u RWM_MIN_R -u RWM_CC_PACE -u RWM_PERF_RATE_MBIT)" in battery and '{ [ "$P51" = 1 ] || [ "$P51E" = 1 ] || [ "$P63" = 1 ] || [ "$P67" = 1 ]; } && SCRUB_EXTRA=' in battery
      and battery.index('"${SCRUB_EXTRA[@]}" "${arm_env[@]}"') > battery.index("-u RWM_C_RELIABLE -u RWM_SLAW -u RWM_THREE_TERM") and battery.count("SCRUB_EXTRA=(-u") == 1)
check("battery: every row's bytes / timeout / hint / env come from the one table (plan51.py / plan51e.py armspec), an unknown arm is refused, HINT is per row",
      "p51_py=plan51.py" in battery and '[ "$P51E" = 1 ] && p51_py=plan51e.py' in battery and 'python3 ./$p51_py armspec "$cell" "$arm"' in battery and 'read -r bytes p51_tmo HINT p51_arm_env' in battery
      and 'tmo_env=(RWM_PERF_TIMEOUT_S="$p51_tmo")' in battery and 'read -r -a arm_env <<< "$p51_arm_env"' in battery and "UNKNOWN-ARM $arm at $cell (plan $PLAN)" in battery)
check("battery: the table-size asserts (51e: 12 / 7 with PL dropped / smoke 1 / rehearsal 3; 51: 76 / smoke 4), the replacement calls and the per-block budget (LD_BLOCK_ROW_S, default 215 s) with ABORT-BUDGET and exit 6",
      'want=12; [ "$(python3 ./plan51e.py dropped)" = 1 ] && want=7' in battery and "want=3" in battery and "    want=76\n" in battery and '-eq 4 ]' in battery and "python3 ./plan51e.py replace" in battery and "python3 ./plan51.py replace" in battery
      and "${LD_BLOCK_ROW_S:-215}" in battery and "ABORT-BUDGET block $rep" in battery and "exit 6" in battery)
check("battery: LD_ONLY (the PL-only recheck) filters the 51e rehearsal in plan51e.py, and the battery's smoke count assert is waived under it", "[ -n \"${LD_ONLY:-}\" ] || [ \"${#tbl[@]}\" -eq \"$want\" ]" in battery)
check("battery: the 51e binary must carry the shed gate, RWM_SLAW, the [SLAW] window line, the spc= field and the pacing echo; the 51 binary RWM_DELTA and both batching echoes",
      all(x in battery for x in ('preflight_binary "$BIN" "RWM_SLAW" "RWM_UNIFIED_SHED" "unified overload shedding ACTIVE" "RWM_PERF_RATE_MBIT"', 'grep -aqF -- "spc=" "$BIN"', 'grep -aqF -- "offered-load pacing ACTIVE" "$BIN"',
                                         'preflight_binary "$BIN" "RWM_DELTA" "emission batching ACTIVE" "emission batching out of scope"')))
check("place38_parse: plans 51 / 51e arms are registered (window / Rlc / the arm's hint / ACTIVE cadence) and ride the candidate seat; the LD_PLAN=51 / 51e hooks call the witnesses and the readouts",
      all(pp.sp.ARM_SPEC[a][:3] == ("window", "Rlc", plan51.arm_hint(a)) and pp.AFAM[a] == "CAND" for a in plan51.ARMS) and all(pp.sp.ARM_SPEC[a][:3] == ("window", "Rlc", "realtime") for a in plan51e.ARMS)
      and 'os.environ.get("LD_PLAN") == "51":' in rd("place38_parse.py") and 'os.environ.get("LD_PLAN") == "51e":' in rd("place38_parse.py") and "plan51e.row_problems(" in rd("place38_parse.py") and "plan51.row_problems(" in rd("place38_parse.py"))
check("the batching echo is its own witness field (batch-echo-*): the 4th ARM_SPEC field stays the estimator-cadence echo ('ACTIVE'), as plan 60's arms", all(pp.sp.ARM_SPEC[a][3] == "ACTIVE" for a in plan51.ARMS + plan51e.ARMS))

# ═══ 3. run_all ═══
check("run_all: plans 51 / 51e selectable and rehearsable; both ride the single-seat gates (P46 + P57)",
      "  51) P_SMOKE=" in runall and "  51e) P_SMOKE=" in runall and 'P51E=0; [ "$PLAN" = "51e" ] && { P51E=1; P46=1; P57=1; }' in runall and 'P51=0; [ "$PLAN" = "51" ] && { P51=1; P46=1; P57=1; }' in runall)
check("run_all 51e: E-pre's bin_sha has NO default and is required (ABORT-BIN-SHA before any lock), the reference manifest defaults to tools/l1/ref51e-manifest.sha256 and is required, a MISS is ABORT-SUITES (a rehearsal included)",
      'LD_REF_MANIFEST="${LD_REF_MANIFEST:-$HERE/ref51e-manifest.sha256}"' in runall and "ABORT-SUITES no reference manifest at $LD_REF_MANIFEST (plan 51e" in runall and "ABORT-BIN-SHA plan 51e needs LD_BIN_SHA_EXPECT" in runall
      and 'fail_all "ABORT-BIN-SHA plan 51e: built $SHA != registered $LD_BIN_SHA_EXPECT; NOTHING WAS RUN"' in runall and '[ "$P51E" != 1 ] || fail_all "ABORT-SUITES plan $PLAN suite-reuse MISS' in runall)
check("run_all 51: LD_BASE_SHA (main at launch) is required and doubles as the candidate pin, a different LD_CAND_SHA is refused, LD_CELL is c2-100 | c8-100 (required on a battery launch), the bin_sha is RECORDED by the rehearsal and REQUIRED to equal the built binary on a battery launch, LD_PRIOR_TESTS / LD_REF_MANIFEST are refused",
      all(x in runall for x in ("ABORT-BASE plan 51 needs LD_BASE_SHA", "ABORT-CAND plan 51 is single-seat", 'LD_CAND_SHA="$LD_BASE_SHA"', "ABORT-CELL plan 51: LD_CELL=", "ABORT-CELL plan 51 needs LD_CELL", "ABORT-BIN-SHA plan 51 needs LD_BIN_SHA_EXPECT",
                                "ABORT-TESTS plan 51 runs no engine suites", "LD-ALL plan 51 bin_sha RECORDED (rehearsal)", 'fail_all "ABORT-BIN-SHA plan 51: built $SHA != registered $LD_BIN_SHA_EXPECT; NOTHING WAS RUN"')))
check("run_all 51: no engine suites ever (suites_stage56 returns early with ENGINE_SUITES=0), the tools tests run; the dial pins run on the built tree after the bin_sha check and stop the launch if one fails or is not proven executed",
      'if [ "$P51" = 1 ]; then      # plan 51: main at launch, no engine suites ever' in runall and "dialpins() {" in runall and "[ \"$P51\" != 1 ] || dialpins" in runall
      and all(x in runall for x in ("delta_budget_b_is_the_dial_not_a_mode", "scheduling_weights_are_the_dial_not_a_mode", "control::fec_rate", "test_continuum_one_law_across_the_dial", "ABORT-DIALPINS plan 51", "dial pin NOT EXECUTED"))
      and runall.index("[ \"$P51\" != 1 ] || dialpins") < runall.index("# ── plan 27: the test suites"))
check("run_all: the budget guard needs 2 730 s (51e battery) / 1 500 s (51e rehearsal) / 17 400 s (51) / 1 200 s (51 rehearsal), equal to plan51e.BUDGET_S and plan51.BUDGET_S",
      'P57_NEED=2730; [ "$REHEARSE" = 1 ] && P57_NEED=1500; P57_TAG="51e"' in runall and 'P57_NEED=17400; [ "$REHEARSE" = 1 ] && P57_NEED=1200; P57_TAG="51"' in runall and plan51e.BUDGET_S == {"51e": 2730, "51er": 1500} and plan51.BUDGET_S == {"51": 17400, "51r": 1200}
      and 'P57_NEED=18360; [ "$PLAN" = "60b" ] && P57_NEED=13860' in runall)
check("run_all: the tests gate runs test_mbar59 / test_s60 / test_s51e / test_harness51 (51e) and test_place38_parse / test_tail52 / test_s51 / test_harness51 (51), and the 60 gates are untouched",
      "for t in test_mbar59.py test_s60.py test_s51e.py test_harness51.py; do" in runall and "for t in test_place38_parse.py test_tail52.py test_s51.py test_harness51.py; do" in runall and "for t in test_s60.py test_harness60.py; do" in runall)
check("run_all: the stage calls the 51 / 51e scorers for the smoke (smoke | rehearsal | recheck), the score and the slot check; LD_HARD / LD_CELL / LD_ONLY / LD_BLOCK_ROW_S ride `sudo -n env` (it resets the environment)",
      all(x in runall for x in ('s51e_score.py smoke51e "$P51SMOKE"', 's51_score.py smoke51 "$P51SMOKE"', "P51SMOKE=rehearsal", "P51SMOKE=recheck", 's51e_score.py score51e "$dir/ld.log"', 's51_score.py score51 "$dir/ld.log"', 'plan51e.py check "$splan"', 'plan51.py check "$splan"',
                                'stage rep51 51; rrc=$?', 'stage rep51e 51e; rrc=$?'))
      and 'LD_HARD="${LD_HARD:-}" LD_CELL="${LD_CELL:-}" LD_ONLY="${LD_ONLY:-}" LD_BLOCK_ROW_S="${LD_BLOCK_ROW_S:-}")' in runall)
check("run_all and the battery guard every new file against CRLF", all(x in runall and x in battery for x in ("plan51e.py", "s51e_score.py", "plan51.py", "s51_score.py", "test_s51e.py", "test_s51.py", "test_harness51.py")))
check("HARNESS_PATHS lists 51e (the strict-manifest paths + the pacing / gauge VOIDs, the calibration and the PL recheck) and 51 (rehearse, full, bin_sha, the two replacements, the pin failure) and no prior-tests path for either",
      {"rehearse", "reuse-hit", "reuse-miss", "bin-sha-mismatch", "pace-void", "spc-void", "calibrate", "recheck"} == set(hp.HARNESS_PATHS["51e"]) and {"rehearse", "full", "bin-sha-mismatch", "delta-echo-void", "short-void", "dialpins-fail"} == set(hp.HARNESS_PATHS["51"]))

# ═══ 4. the perf driver and lib.sh ═══
check("perf_rwm_c.sh: the `--- RWM-C perf` header echoes rate=${RWM_PERF_RATE_MBIT:-unset} (before start=, so the registered 'start=$(date +%T) reliable=$RELIABLE\"' tail of test_mbar59 holds)",
      "rate=${RWM_PERF_RATE_MBIT:-unset} start=$(date +%T) reliable=$RELIABLE\"" in perf)
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
