#!/usr/bin/env python3
"""Tests for status 62's harness (plans 62r0 / 62a, c3 at jitter 0: tools/l1/plan62.py, the place38_parse hook, the battery / run_all / perf_rwm_c.sh / lib.sh wiring). The registration is
docs/derivations/s62-registration.md (section 8); the scorer's own tests are test_s62.py and the end-to-end launch paths (every NO-GO, ABORT-NETEM, the tools-test reuse HIT / MISS) run in test_stubrun.py.

  1. the tables: 7 / 44 / 1 rows, the Latin square, seed alternation, the dump rows, ABORT-PILOT without LD_KNEE62, the re-probe's slots;
  2. the per-arm run spec: bytes (W 12 25 MB, W 24 30 MB, 150 s; the formula, 120 MB at K / P1 / the control), env (RWM_SLAW first, the pool knob, RWM_RTT_DUMP=1 on a D arm);
  3. the row witnesses (the dump gate included) and the replacement rule;
  4. the shell side: plan accepted, the P62 branch, NO build (LD_BIN_PREBUILT required), the registered bin_sha, ABORT-CONTROL62 / ABORT-REHEARSAL62, the budget, the tools-test reuse, the live-netem witness, the CRLF guard lists;
  5. lib.sh's c3j0 scenario and the cell's registration in the row parser;
  6. the engine-diff witness: the engine crates and ref60-manifest.sha256 equal 1ecee59d's (needs git and the commit: skipped in an archive).
Run: python3 test_harness62.py"""
import json
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_WITNESS54", "LD_SHA", "LD_SHA_BASE", "LD_BASE_SHA", "LD_ROW_BYTES", "LD_NOGO", "LD_KNEE62", "LD_CTL62", "LD_REHEARSE", "LD_REHEARSAL62"):
    os.environ.pop(_k, None)
import harness_paths as hp  # noqa: E402
import l1common as lc  # noqa: E402
import place38_parse as pp  # noqa: E402
import plan62  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


def rd(name):
    return open(os.path.join(HERE, name), encoding="utf-8", errors="replace").read()


runall, battery, perf = rd("loopdiag_run_all.sh"), rd("loopdiag_battery.sh"), rd("perf_rwm_c.sh")
C = plan62.C3J0

# ═══ 1. tables ═══
R = plan62.plan("62r0")
os.environ["LD_KNEE62"] = "97.4"
A = plan62.plan("62a")
check("62a is 44 slots (7 arms x 6 replicates + 2 dump rows), 62r0 is 7, the 62a smoke 1", (len(A), len(R), len(plan62.plan("62a", smoke=True))) == (44, 7, 1))
check("62a: every slot is c3j0-25; the scored arms are G12, G24, K1 / K2 / K3 = G88 / G104 / G116, P1 = G144 and the control G512, six replicates of each, no duplicate slot",
      {r[0] for r in A} == {C} and all(sum(1 for r in A if r[2] == "G%d" % w) == 6 for w in (12, 24, 88, 104, 116, 144, 512)) and len({(r[0], r[2], r[3], r[4]) for r in A}) == 44)
check("62a: the arms rotate by replicate (a cyclic Latin square: replicate k starts at arm k mod 7) and the seeds alternate 42 / 7", [r[2] for r in A[:3]] == ["G12", "G24", "G88"] and A[7][2] == "G24" and A[14][2] == "G88"
      and [A[7 * k][3] for k in range(6)] == ["42", "7", "42", "7", "42", "7"] and [A[7 * k][4] for k in range(6)] == [1, 2, 3, 4, 5, 6])
check("62a: the two dump rows follow the grid: D12 (seed 42, rep 7) and D at K2 = D104 (seed 7, rep 8); they are kind D", A[42:] == [(C, "def", "D12", "42", 7), (C, "def", "D104", "7", 8)] and plan62.arm_parse("D104") == ("D", 104))
check("the rehearsal is R1 G512, R2 / R3 G12 (seeds 42 and 7), R4 / R5 G80 / G128 (the probes), R6 G512 (seed 7), D1 D12; every slot unique", [r[2] for r in R] == ["G512", "G12", "G12", "G80", "G128", "G512", "D12"]
      and [r[3] for r in R[1:3]] == ["42", "7"] and len({(r[0], r[2], r[3], r[4]) for r in R}) == 7)
check("the re-probe's slots are the same seed / rep as R4 / R5 at the new probes (112 / 160 or 64 / 96), distinct from the rehearsal's slots", plan62.reprobe_slots((64, 96)) == [(C, "def", "G64", "42", 1), (C, "def", "G96", "42", 1)]
      and plan62.reprobe_slots((112, 160))[0][2] == "G112" and not ({(c, a, s, r) for c, _, a, s, r in plan62.reprobe_slots((64, 96))} & {(c, a, s, r) for c, _, a, s, r in R}))
del os.environ["LD_KNEE62"]
try:
    plan62.plan("62a")
    raised = False
except ValueError:
    raised = True
check("62a without LD_KNEE62 raises (ABORT-PILOT); plan62.py plan 62a exits 3 with ABORT-PILOT", raised and plan62.main(["plan", "62a"]) == 3)
check("plan62.py count / plan / pilot print the tables and the PILOT62 line (the registered format: W= u_lo= u_hi= arms= case=)", plan62.main(["count", "62r0"]) == 0 and plan62.main(["pilot", "0.78", "1.0"]) == 0)

# ═══ 2. the per-arm run spec ═══
os.environ["LD_KNEE62"] = "97.4"
spec = {a: (plan62.arm_bytes(C, a), plan62.arm_timeout(C, a), plan62.arm_hint(a), plan62.arm_env(C, a)) for a in ("G12", "G24", "G80", "G88", "G104", "G116", "G128", "G144", "G512", "D12", "D104")}
check("bytes: W 12 25 MB, W 24 30 MB, 120 MB at the control and from K1 (W 88) up, the probe below K1 (W 80) 100 MB by 61a's formula", spec["G12"][0] == 25_000_000 and spec["G24"][0] == 30_000_000
      and all(spec[a][0] == 120_000_000 for a in ("G88", "G104", "G116", "G128", "G144", "G512", "D104")) and spec["G80"][0] == 100_000_000 and spec["D12"][0] == 25_000_000, {k: v[0] for k, v in spec.items()})
check("timeouts: 150 s on the floor arms (and the dump row at W 12), 120 s on every other arm; hint bulk everywhere", all(spec[a][1] == 150 for a in ("G12", "G24", "D12")) and all(spec[a][1] == 120 for a in spec if a not in ("G12", "G24", "D12"))
      and all(v[2] == "bulk" for v in spec.values()))
check("env: RWM_SLAW=1 first, RWM_RTOBS=1, the pool knob RWM_STORE_PATH_POOL = W, and RWM_RTT_DUMP=1 on a D arm only", spec["G12"][3] == ["RWM_SLAW=1", "RWM_RTOBS=1", "RWM_STORE_PATH_POOL=12"]
      and spec["D104"][3] == ["RWM_SLAW=1", "RWM_RTOBS=1", "RWM_STORE_PATH_POOL=104", "RWM_RTT_DUMP=1"] and all("RWM_RTT_DUMP=1" not in v[3] for a, v in spec.items() if a[0] == "G"))
check("armspec refuses an unknown (cell, arm): the c3 cell, a C arm, an arm the pilot cannot place, a D control", plan62.main(["armspec", "c3-25", "G12"]) == 1 and plan62.main(["armspec", C, "C12"]) == 1 and plan62.main(["armspec", C, "G13"]) == 1
      and plan62.main(["armspec", C, "D512"]) == 1 and plan62.main(["armspec", C, "G12"]) == 0 and plan62.main(["armspec", C, "D12"]) == 0)
del os.environ["LD_KNEE62"]

# ═══ 3. witnesses and the replacement rule ═══
G = "[GATES] RWM_DIAG=1 RWM_THREE_TERM=0 RWM_STORE_PATH_POOL=%s RWM_SLAW=%s RWM_RTT_DUMP=%s"
POOLINFO = "path-scaled outstanding pool ACTIVE (RWM_STORE_PATHS) pool_per_path=%s"
SHA = "a" * 64


def logs(slaw_c="1", slaw_s="1", pool="12", diag="1", dump_c="0", dump_s="0"):
    return ([G.replace("RWM_DIAG=1", "RWM_DIAG=" + diag) % (pool, slaw_c, dump_c), POOLINFO % pool], [G % (pool, slaw_s, dump_s), POOLINFO % pool])


c, s = logs()
check("a G row echoing RWM_SLAW=1, RWM_DIAG=1, RWM_THREE_TERM=0, RWM_RTT_DUMP=0 and its pool on both ends has no problem", plan62.row_problems(C, "G12", c, s, SHA, SHA) == [], plan62.row_problems(C, "G12", c, s, SHA, SHA))
check("RWM_SLAW echoed 0 on either end is a gate-slaw problem (VOID); a wrong pool echo, RWM_DIAG not 1 and a foreign binary are problems; an arm no plan carries is refused",
      any(x.startswith("gate-slaw-cli=0") for x in plan62.row_problems(C, "G12", *logs(slaw_c="0"), SHA, SHA)) and any(x.startswith("gate-slaw-srv=0") for x in plan62.row_problems(C, "G12", *logs(slaw_s="0"), SHA, SHA))
      and any(x.startswith("gate-pool") for x in plan62.row_problems(C, "G12", *logs(pool="2048"), SHA, SHA)) and any(x.startswith("gate-diag") for x in plan62.row_problems(C, "G12", *logs(diag="0"), SHA, SHA))
      and any(x.startswith("bin-sha") for x in plan62.row_problems(C, "G12", *logs(), SHA, "b" * 64)) and plan62.row_problems(C, "G13", *logs(), SHA, SHA) == ["unknown-arm-G13-at-c3j0-25"])
check("the dump gate: a D arm must echo RWM_RTT_DUMP=1 on both ends (a D row without it is VOID, gate-rttdump), a G arm must echo 0 (a dump running under a scored row is VOID)",
      plan62.row_problems(C, "D12", *logs(dump_c="1", dump_s="1"), SHA, SHA) == [] and any(x.startswith("gate-rttdump-cli=0") for x in plan62.row_problems(C, "D12", *logs(dump_c="0", dump_s="1"), SHA, SHA))
      and any(x.startswith("gate-rttdump-srv=1") for x in plan62.row_problems(C, "G12", *logs(dump_s="1"), SHA, SHA)))
d = tempfile.mkdtemp()


def ledger(rows):
    p = os.path.join(d, "ld%d.log" % len(os.listdir(d)))
    with open(p, "w") as fh:
        for r in rows:
            fh.write("P38ROW " + json.dumps(r) + "\n")
    return p


def row(cell, arm, seed, rep, problems=(), dnf=False, status="LIVE"):
    return {"cell": cell, "arm": arm, "seed": seed, "rep": rep, "problems": list(problems), "dnf": dnf, "status": status}


check("replacement: a gate-slaw VOID slot is re-run once ('slaw-echo'); a second row for the slot stops it",
      plan62.replace_reason(ledger([row(C, "G12", "42", 1, ["gate-slaw-cli=0 (want 1)"], status="WITNESS-FAIL")]), C, "G12", "42", 1) == "slaw-echo"
      and plan62.replace_reason(ledger([row(C, "G12", "42", 1, ["gate-slaw-cli=0 (want 1)"], status="WITNESS-FAIL"), row(C, "G12", "42", 1)]), C, "G12", "42", 1) is None)
check("replacement: a pool-echo or dump-echo VOID ('pool-echo'), a floor-arm DNF ('floor-dnf'); a non-floor DNF, a LIVE row, a crash row (VOID-ONDISK) are NOT replaced; a wrong binary never is",
      plan62.replace_reason(ledger([row(C, "G24", "7", 2, ["gate-pool-cli=2048 (want 24)"], status="WITNESS-FAIL")]), C, "G24", "7", 2) == "pool-echo"
      and plan62.replace_reason(ledger([row(C, "D12", "42", 7, ["gate-rttdump-cli=0 (want 1)"], status="WITNESS-FAIL")]), C, "D12", "42", 7) == "pool-echo"
      and plan62.replace_reason(ledger([row(C, "G24", "42", 1, dnf=True)]), C, "G24", "42", 1) == "floor-dnf" and plan62.replace_reason(ledger([row(C, "G104", "42", 1, dnf=True)]), C, "G104", "42", 1) is None
      and plan62.replace_reason(ledger([row(C, "G12", "42", 1)]), C, "G12", "42", 1) is None and plan62.replace_reason(ledger([row(C, "G12", "42", 1, dnf=True, status="VOID-ONDISK")]), C, "G12", "42", 1) is None
      and plan62.replace_reason(ledger([row(C, "G12", "42", 1, ["bin-sha=aaa (want bbb for CAND)"], status="WITNESS-FAIL")]), C, "G12", "42", 1) is None)
os.environ["LD_PLAN"] = "62r0"
check("a probe-row DNF (W 80 / 128, and a re-probe's) re-runs once in the REHEARSAL only (LD_PLAN=62r0)", plan62.replace_reason(ledger([row(C, "G80", "42", 1, dnf=True)]), C, "G80", "42", 1) == "pilot-dnf"
      and plan62.replace_reason(ledger([row(C, "G64", "42", 1, dnf=True)]), C, "G64", "42", 1) == "pilot-dnf")
os.environ["LD_PLAN"] = "62a"
check("... and not in the cell (LD_PLAN=62a)", plan62.replace_reason(ledger([row(C, "G80", "42", 1, dnf=True)]), C, "G80", "42", 1) is None)
del os.environ["LD_PLAN"]
os.environ["LD_KNEE62"] = "97.4"
rows_ok = [row(c_, a_, sd, rp) for c_, _, a_, sd, rp in A]
p_ok = ledger(rows_ok)
check("plan62.py check: rc 1 without LD-BATTERY-DONE, rc 0 with it and a row for every slot (a replaced slot's second row is not an extra slot; an unexpected slot is)",
      plan62.check("62a", p_ok) == 1 and (open(p_ok, "a").write("LD-BATTERY-DONE tag=ld blocks=1\n") or plan62.check("62a", p_ok) == 0) and plan62.check("62a", ledger(rows_ok + [row(C, "G999", "42", 1)] + [row(C, "G12", "42", 9)])) == 1)
del os.environ["LD_KNEE62"]
rr = ledger([row(c_, a_, sd, rp) for c_, _, a_, sd, rp in R + plan62.reprobe_slots((64, 96))])
open(rr, "a").write("LD-BATTERY-DONE tag=smoke blocks=0\n")
check("plan62.py check 62r0 accepts a re-probe's two slots (and only those)", plan62.check("62r0", rr) == 0)
check("budget: the registered LD_HARD offsets are 3 600 s (62r0, launch + 1.0 h) / 9 000 s (62a, launch + 2.5 h); the guard's worst case (plan62.BUDGET_S: 2 400 / 7 800 s) is STRICTLY under each (the guard reads the time left after the lock wait: a guard at the offset refuses every registered launch), above the registration's own 44 x 150 s = 6 600 s, and run_all carries it", plan62.LD_HARD_S == {"62r0": 3600, "62a": 9000} and plan62.BUDGET_S == {"62r0": 2400, "62a": 7800}
      and all(plan62.BUDGET_S[k] < plan62.LD_HARD_S[k] for k in plan62.BUDGET_S) and plan62.BUDGET_S["62a"] > 44 * 150 and 'P57_NEED=7800; [ "$PLAN" = "62r0" ] && P57_NEED=2400' in runall)

# ═══ 4. shell side ═══
check("battery: plans 62r0 / 62a accepted (P62), single-seat (P38 + PCAND, no P41), the instrument env adds RWM_NETEM_WITNESS=1, the cell c3j0-25 has its cell_spec, [NETEM] is kept in the ledger",
      'P62=0; case "$PLAN" in 62r0|62a) P62=1 ;; esac' in battery and '{ [ "$P61" = 1 ] || [ "$P62" = 1 ]; } && { P38=1; PCAND=1; }' in battery
      and 'INSTR_ENV=(RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=120 RWM_NETEM_WITNESS=1)' in battery and 'c3j0-25) echo "c3j0 c3j0 single 25000000"' in battery and "\\[NETEM\\]" in battery)
check("battery: every row's bytes / timeout / hint / env come from plan62.py armspec (one table), the cell must be c3j0-25, an unknown arm is refused, the env scrub withholds RWM_RTT_DUMP, RWM_RTT_DUMP_MAX and RWM_NETEM_WITNESS",
      'python3 ./plan62.py armspec "$cell" "$arm"' in battery and 'read -r bytes p62_tmo HINT p60_arm_env' in battery and 'tmo_env=(RWM_PERF_TIMEOUT_S="$p62_tmo")' in battery and "UNKNOWN-ARM $arm at $cell (plan 62)" in battery
      and "-u RWM_RTT_DUMP -u RWM_RTT_DUMP_MAX -u RWM_NETEM_WITNESS" in battery and 'if [ "$P60" = 1 ] || [ "$P61" = 1 ] || [ "$P62" = 1 ]; then\n    read -r -a arm_env <<< "$p60_arm_env"' in battery)
check("battery: the table-size asserts (44; the rehearsal 7; the smoke 1), ABORT-PILOT without LD_KNEE62, the replacement call (REPLACE62), the one re-probe asked of s62_score.py reprobe (REPROBE62), the binary must carry RWM_RTT_DUMP",
      "    want=44\n" in battery and '-eq 7 ]' in battery and "ABORT-PILOT" in battery and "python3 ./plan62.py replace" in battery and "REPLACE62" in battery and 'python3 ./s62_score.py reprobe "$OUT"' in battery and "REPROBE62 probes" in battery
      and 'preflight_binary "$BIN" "RWM_RTT_DUMP"' in battery)
check("battery: after EVERY row the live-netem witness is checked (plan62.py netem on the row's driver output) and a bad read stops the launch with ABORT-NETEM (exit 5), after the ledger row is parsed",
      '[ "$P62" = 1 ] && ! python3 ./plan62.py netem "$TMPD/ld-drv.out" >> "$OUT" 2>&1' in battery and 'ABORT-NETEM $name s$seed rep=$rep' in battery and battery.index("ABORT-NETEM $name") > battery.index('python3 "$parser" row "$cell"'))
check("perf_rwm_c.sh: the [NETEM] witness is printed for the data leg and its ACK leg right after the topology is up (pre) and before the teardown (post), only under RWM_NETEM_WITNESS=1, every substitution ending `|| true` (the script runs under set -e)",
      "netem_witness pre" in perf and "netem_witness post" in perf and 'if [ "${RWM_NETEM_WITNESS:-0}" = 1 ]; then netem_witness pre; fi' in perf and perf.index("netem_witness pre") < perf.index("netem_witness post")
      and "tc qdisc show dev" in perf and perf.count("tr '\\n' ' ' || true)") >= 2)
check("perf_rwm_c.sh carries harness63's fixed F2 lines: every substitution of the F2 block ends `|| true` (a bare grep -c under set -e killed the script on plan 63's first rows)",
      (lambda f2: len(f2) == 3 and all(ln.endswith('|| true)"') for ln in f2) and '--- F2 undeliv=${_F2_UND:-0} unframable=${_F2_UNF:-0}' in perf)([ln for ln in perf.splitlines() if ln.startswith("_F2_")]))
check("run_all: plans 62r0 / 62a are selectable, 62r0 implies the rehearsal (REHEARSE=1; 62a refuses LD_REHEARSE), both ride plan 60's gates (P60: bin_sha, strict manifest; P57: budget, /dev/shm; P46)",
      "  62r0|62a) P_SMOKE=" in runall and '[ "$PLAN" = "62r0" ] && REHEARSE=1' in runall and 'case "$PLAN" in 62r0|61a) ;; 41c|' in runall and 'P62=0; { [ "$PLAN" = "62r0" ] || [ "$PLAN" = "62a" ]; } && { P62=1; P60=1; P46=1; P57=1; }' in runall)
check("run_all: NO build in either launch: LD_BIN_PREBUILT is required (ABORT-BUILD, nothing run, before any cargo call) and its sha is checked against the registered bin_sha (e410d640...) before any row",
      '[ "$P62" != 1 ] || [ -n "${LD_BIN_PREBUILT:-}" ] || fail_all "ABORT-BUILD plan $PLAN never builds' in runall and runall.index("never builds") < runall.index('( cd "$SRC" && CARGO_TARGET_DIR="$TARGET"')
      and 'if [ "$P60" = 1 ] && [ "$SHA" != "$LD_BIN_SHA_EXPECT" ]; then' in runall and "e410d64053ab0a143e813aded2e4023176b41edf09d287e795252736b0e5f25d" in runall)
check("run_all: ABORT-CONTROL62 (LD_CTL62 != 512, both launches) comes after the bin_sha check; 62a then needs LD_KNEE62 (ABORT-PILOT) and the rehearsal's GO and PILOT62 W= equal to it (ABORT-REHEARSAL62), all before any row",
      'fail_all "ABORT-CONTROL62 plan $PLAN' in runall and runall.index('fail_all "ABORT-CONTROL62') > runall.index('fail_all "ABORT-BIN-SHA plan $PLAN') and runall.index('fail_all "ABORT-PILOT plan 62a') > runall.index('fail_all "ABORT-CONTROL62')
      and runall.index('fail_all "ABORT-REHEARSAL62') > runall.index('fail_all "ABORT-PILOT plan 62a') and "grep -aqx 'LABEL62 GO-c3j0 GO'" in runall and '[ "$R62W" = "${LD_KNEE62:-}" ]' in runall)
check("run_all: the tools-test reuse: tools_manifest (every file of tools/l1 sha256, no __pycache__), recorded beside TOOLS-TESTS-OK at the rehearsal, compared at 62a only (HIT skips the python and bash tools tests; a MISS RUNS them, not an abort); "
      "EVERY python and bash tools test is skipped on a HIT: the pre-build gates (parser-test.log) and the suites stage; the manifest is taken before ANY test runs",
      "tools_manifest() {" in runall and "tools_reuse_ok() {" in runall and '[ "$PLAN" != "62a" ]' in runall and "tools-tests reuse HIT" in runall and "tools-tests reuse MISS" in runall and "TOOLS-TESTS-REUSED" in runall
      and 'cp "$RUN/TOOLS-MANIFEST.now" "$SD/TOOLS-MANIFEST.sha256"' in runall and 'tools_manifest > "$RUN/TOOLS-MANIFEST.now"' in runall and runall.index('tools_manifest > "$RUN/TOOLS-MANIFEST.now"') < runall.index('python3 ./test_loopdiag_parse.py > "$RUN/parser-test.log"')
      and 'if [ "$TOOLS_REUSE" = 1 ]; then\n  echo "TOOLS-TESTS-REUSED' in runall and 'if [ "$P62" = 1 ] && [ "$TOOLS_REUSE" = 1 ]; then' in runall and runall.index("TOOLS_REUSE=0") < runall.index('python3 ./test_loopdiag_parse.py > "$RUN/parser-test.log"')
      and "! -path '*/__pycache__/*'" in runall)
check("run_all: the tools-test gate runs test_s62.py and test_harness62.py; the stage calls the 62 scorer for the smoke (the rehearsal's rows + go/no-go), the score and the slot check; LD_KNEE62 / LD_CTL62 cross `sudo -n env`; LD_REHEARSAL62 does not (run_all only)",
      "for t in test_s62.py test_harness62.py" in runall and 's62_score.py smoke62 "$splan"' in runall and 's62_score.py score62 "$splan"' in runall and 'plan62.py check "$splan"' in runall and 'LD_KNEE62="${LD_KNEE62:-}" LD_CTL62="${LD_CTL62:-}"' in runall
      and "62r0) stage rep62r0 62r0" in runall and "62a) stage rep62a 62a" in runall and "REHEARSAL62 " in runall and "LD_REHEARSAL62" not in runall.split("local SENV=(")[1].split("\n")[0])
check("run_all and the battery guard the new files against CRLF", all(f in runall for f in ("plan62.py", "s62_score.py", "test_s62.py", "test_harness62.py")) and all(f in battery for f in ("plan62.py", "s62_score.py", "test_s62.py", "test_harness62.py")))
check("HARNESS_PATHS lists the 62r0 NO-GO paths (floor, gate, pilot, control, short), the re-probe, ABORT-NETEM (first / last row) and the tools-reuse HIT / MISS (62a full / tools-reuse-miss)",
      {"nogo-floor", "nogo-gate", "nogo-pilot", "nogo-control", "nogo-short", "reprobe", "abort-netem-first"} <= set(hp.HARNESS_PATHS["62r0"]) and {"full", "tools-reuse-miss", "abort-netem-last", "rehearsal-nogo", "knee-mismatch"} <= set(hp.HARNESS_PATHS["62a"]))
check("plan 60 / 61's tables are untouched by the 62 additions (96 / 40 slots; 6 rehearsal rows)", len(__import__("plan60").plan("60a")) == 96 and len(__import__("plan60").plan("60b")) == 40 and len(__import__("plan61").plan("61r")) == 6)

# ═══ 4b. the REAL perf_rwm_c.sh lines (the stub driver replaces the script in test_stubrun, so they run here, under the script's own `set -euo pipefail`) ═══
import re  # noqa: E402
bash_ = shutil.which("bash")
if bash_:
    def run_bash(script):
        return subprocess.run([bash_, "-c", script], cwd=HERE, capture_output=True, text=True, timeout=60)

    m_fn = re.search(r"^netem_witness\(\) \{.*?^\}$", perf, re.S | re.M)
    GOOD_TC = "qdisc netem 8001: root refcnt 2 limit 1000 delay 20ms loss gemodel p 2% r 40% 1-h 100% 1-k 0% rate 20Mbit seed 42"
    prelude = ("set -euo pipefail\nNS_CLI=nscli; NS_SRV=nssrv; CLI_LEGS=(cli0); SRV_LEGS=(srv0)\n"
               "ip() { if [ \"${FAIL_TC:-0}\" = 1 ]; then echo 'Cannot open network namespace' >&2; return 1; fi; echo \"$TC_OUT\"; echo \" Sent 100 bytes 1 pkt (dropped 0, overlimits 0 requeues 0)\"; }\n")
    check("perf_rwm_c.sh: the netem_witness function is found in the script", m_fn is not None)
    if m_fn:
        env_ok = subprocess.run([bash_, "-c", prelude + m_fn.group(0) + "\nnetem_witness pre\nnetem_witness post\necho REACHED-END\n"], cwd=HERE, capture_output=True, text=True, timeout=60,
                                env={**os.environ, "TC_OUT": GOOD_TC})
        lines_ok = env_ok.stdout.splitlines()
        got, prob = lc.netem_witness(lines_ok)
        check("the REAL netem_witness under `set -euo pipefail` prints [NETEM] for pre and post on both legs (four lines, the multi-line tc text joined on one) and the script goes on (rc 0)",
              env_ok.returncode == 0 and "REACHED-END" in lines_ok and sum(1 for x in lines_ok if x.startswith("[NETEM] ")) == 4 and prob == [] and all(ok for ok, _ in got.values()), (env_ok.returncode, env_ok.stdout[:500], env_ok.stderr[:300]))
        env_jit = subprocess.run([bash_, "-c", prelude + m_fn.group(0) + "\nnetem_witness pre\nnetem_witness post\necho REACHED-END\n"], cwd=HERE, capture_output=True, text=True, timeout=60,
                                 env={**os.environ, "TC_OUT": GOOD_TC.replace("delay 20ms", "delay 20ms  5ms")})
        got, prob = lc.netem_witness(env_jit.stdout.splitlines())
        check("the same lines on the c3 line (a jitter term) are read by lc.netem_witness as a wrong read on every leg and tag", env_jit.returncode == 0 and len(prob) == 4 and all("jitter term 5ms" in x for x in prob), (prob, env_jit.stdout[:300]))
        env_fail = subprocess.run([bash_, "-c", prelude + m_fn.group(0) + "\nnetem_witness pre\nnetem_witness post\necho REACHED-END\n"], cwd=HERE, capture_output=True, text=True, timeout=60,
                                  env={**os.environ, "TC_OUT": GOOD_TC, "FAIL_TC": "1"})
        got, prob = lc.netem_witness(env_fail.stdout.splitlines())
        check("a failing `ip netns exec ... tc` (no namespace) does NOT kill the script under set -e (rc 0, the end is reached, four lines with an empty qdisc: every read is wrong), so the battery decides, never a silent rc 1 (plan 63's VOID-RC failure class)",
              env_fail.returncode == 0 and "REACHED-END" in env_fail.stdout and sum(1 for x in env_fail.stdout.splitlines() if x.startswith("[NETEM] ")) == 4 and len(prob) == 4, (env_fail.returncode, env_fail.stdout[:400], env_fail.stderr[:200]))
    f2 = [ln for ln in perf.splitlines() if ln.startswith("_F2_") or ln.startswith('echo "--- F2 undeliv=')]
    check("perf_rwm_c.sh: the F2 block is found (three substitutions and the printed line)", len(f2) == 4)
    if len(f2) == 4:
        tmp = tempfile.mkdtemp()
        blk = "\n".join(f2).replace("/tmp/rwm-s.log", tmp.replace("\\", "/") + "/s.log").replace("/tmp/rwm-c.log", tmp.replace("\\", "/") + "/c.log")
        for name, body in (("quiet", ("plain line\n", "another line\n")), ("busy", ("[CTLD] t=1 undeliv=3\n[CTLD] t=2 undeliv=7\n", "[UNFRAMABLE] x\n[UNFRAMABLE] y\n"))):
            open(os.path.join(tmp, "s.log"), "w").write(body[0])
            open(os.path.join(tmp, "c.log"), "w").write(body[1])
            r = run_bash("set -euo pipefail\n" + blk + "\necho REACHED-END\n")
            want = "--- F2 undeliv=0 unframable=0" if name == "quiet" else "--- F2 undeliv=7 unframable=2"
            check(f"the REAL F2 lines under `set -euo pipefail` on a log with {'no [CTLD] / [UNFRAMABLE] line (every grep exits 1)' if name == 'quiet' else '[CTLD] undeliv= and [UNFRAMABLE] lines'}: rc 0, the script goes on, `{want}`",
                  r.returncode == 0 and want in r.stdout and "REACHED-END" in r.stdout, (r.returncode, r.stdout[:300], r.stderr[:300]))
        shutil.rmtree(tmp, ignore_errors=True)
else:
    print("skip the real perf_rwm_c.sh lines: no bash here")
sem = tempfile.mkdtemp()


def netem_file(lines, summary=True):
    p_ = os.path.join(sem, "drv%d.out" % len(os.listdir(sem)))
    open(p_, "w").write("\n".join(lines + (['{"bytes":1,"summary":true}'] if summary else [])) + "\n")
    return p_


GOOD = "qdisc netem 8001: root refcnt 2 limit 1000 delay 20ms loss gemodel p 2% r 40% 1-h 100% 1-k 0% rate 20Mbit seed 42"
four = lambda srvpost=GOOD: [f"[NETEM] tag=pre leg=cli dev=cli0 qdisc: {GOOD}", f"[NETEM] tag=pre leg=srv dev=srv0 qdisc: {GOOD}", f"[NETEM] tag=post leg=cli dev=cli0 qdisc: {GOOD}", f"[NETEM] tag=post leg=srv dev=srv0 qdisc: {srvpost}"]  # noqa: E731
check("plan62.py netem: a clean row is rc 0; a wrong read (a jitter term on the ACK leg after the transfer) is rc 1; a MISSING read is rc 1 on a row that completed (a summary) but rc 0 on a row that did not (a bring-up failure is a void row, not a harness fault)",
      plan62.netem_check(netem_file(four())) == 0 and plan62.netem_check(netem_file(four(GOOD.replace("delay 20ms", "delay 20ms  5ms")))) == 1 and plan62.netem_check(netem_file(four()[:3])) == 1
      and plan62.netem_check(netem_file(four()[:3], summary=False)) == 0 and plan62.netem_check(netem_file(four(GOOD.replace("delay 20ms", "delay 20ms  5ms"))[:3] + [four(GOOD.replace("delay 20ms", "delay 20ms  5ms"))[3]], summary=False)) == 1)
shutil.rmtree(sem, ignore_errors=True)

# ═══ 4c. cell-keyed lookups: c3j0-25 must hit every table c3-25 hits, and never fall back silently ═══
import s60_score as _s6  # noqa: E402
import s61_score as _s1  # noqa: E402
import plan60 as _p60  # noqa: E402
import plan61 as _p61  # noqa: E402
check("LATE_T is registered for c3j0-25 in BOTH tables the row readers read with a silent default (plan60.LATE_T via s60_score.row_metrics, plan61.LATE_T via s61_score.row_extras), and plan62's own", _p60.LATE_T.get(C) == 15.0 and _p61.LATE_T.get(C) == 15.0 and plan62.LATE_T[C] == 15.0 and _s6.LATE_T is _p60.LATE_T)
_tk = ['{"warmup":true}'] + [f"[SLAW] t={t:.2f} k={k} p=0 n=12 W=12 aw=0.5 nk=60 tmin=41.0" for k, t in enumerate((5.0, 20.0, 30.0), 1)] + [f"[WSTAT] t={t:.2f} k={k} p=0 nk=60 tmin=41.0 p10=41.1 p50=42.0 p90=43.0" for k, t in enumerate((5.0, 20.0, 30.0), 1)]
_old = _p61.LATE_T[C]
_n_reg = len(_s1.row_extras(_tk, [], [], C)["win"]["t"])
_p61.LATE_T[C] = 25.0
_n_25 = len(_s1.row_extras(_tk, [], [], C)["win"]["t"])
_p61.LATE_T[C] = _old
check("the LATE cut is read BY the cell's key (15 s keeps the t = 20 and 30 windows, a test-set 25 s keeps one): the lookup is the registered entry, not the 15.0 default", (_n_reg, _n_25) == (2, 1), (_n_reg, _n_25))
check("every other cell-keyed default the c3j0-25 readers take is the SAME as c3-25's (neither is in LEGS / X_COLLAPSE / OMEGA: one leg, no collapse X, omega 0: plan 61 ran on those defaults), so no value differs silently",
      all(((t.get(C) is None) == (t.get("c3-25") is None)) for t in (_s6.LEGS, _s6.X_COLLAPSE, _s6.OMEGA)) and _s6.LEGS.get(C, (0,)) == _s6.LEGS.get("c3-25", (0,)))
check("stage3_parse / place38_parse tables the row path indexes are registered for c3j0-25 (CELL_SPEC, N_LEGS, SCEN_LINK, bdp) and the cell's scenario exists in lib.sh's table",
      C in pp.sp.CELL_SPEC and C in pp.sp.N_LEGS and "c3j0" in pp.SCEN_LINK and pp.bdp_bytes(C) == pp.bdp_bytes("c3-25") and "c3j0)" in rd("lib.sh"))

# ═══ 5. lib.sh's scenario and the row parser's cell ═══
check("the row parser knows the cell: c3j0-25 is a single-leg cell of scenario c3j0 (BDP 100 000 B per leg = 20 Mbit x 40 ms, c3's), and every plan-62 arm is registered", pp.sp.CELL_SPEC[C] == ("c3j0", "c3j0", "single", 25000000) and pp.sp.N_LEGS[C] == 1
      and pp.bdp_bytes(C) == [100000] and all(a in pp.sp.ARM_SPEC for a in plan62.all_arms()) and C not in pp.sp.CELLS)
bash = shutil.which("bash")
if bash:
    def sh(cmd):
        return subprocess.run([bash, "-c", cmd], cwd=HERE, capture_output=True, text=True, timeout=60).stdout.strip()
    check("lib.sh: scenario c3j0 is `20mbit 20 0 2 40` (c3's rate, delay, GE loss and burst; jitter 0) and netem_args prints `delay 20ms rate 20mbit loss gemodel 2% 40% seed 42` with no jitter term (c3 keeps its `20ms 5ms`)",
          " ".join(sh("source ./lib.sh; scenario_params c3j0").split()) == "20mbit 20 0 2 40" and sh("source ./lib.sh; netem_args c3j0 42") == "delay 20ms rate 20mbit loss gemodel 2% 40% seed 42"
          and sh("source ./lib.sh; netem_args c3 42") == "delay 20ms 5ms rate 20mbit loss gemodel 2% 40% seed 42")
else:
    print("skip lib.sh scenario check: no bash here")
check("lib.sh keeps RWM_RTT_DUMP / RWM_RTT_DUMP_MAX in the forwarded engine env (the dump rows' knob reaches the binary)", "RWM_RTT_DUMP RWM_RTT_DUMP_MAX" in rd("lib.sh"))


# ═══ 6. the engine-diff witness ═══
def git(*a):
    try:
        return subprocess.run(["git", *a], cwd=os.path.join(HERE, "..", ".."), capture_output=True, text=True, timeout=60)
    except Exception:      # noqa: BLE001
        return None


BASE = "1ecee59d"
ENGINE = ("raptorpath", "raptorpath-math", "raptorpath-core", "raptorpath-wasm", "gf256", "Cargo.toml", "Cargo.lock")
g0 = git("rev-parse", "--git-dir")
g1 = git("cat-file", "-t", BASE) if g0 is not None and g0.returncode == 0 else None
# These witnesses were launch-time checks of a plan that has since run: the engine moved on, so on main they
# can only fail. They run when the plan's launcher sets HARNESS_FROZEN_BASE=1; every other check stays live.
if os.environ.get("HARNESS_FROZEN_BASE") != "1":
    print("skip frozen-base witnesses (set HARNESS_FROZEN_BASE=1 at the plan's launch)")
elif g1 is not None and g1.returncode == 0 and g1.stdout.strip() == "commit":
    dd = git("diff", "--stat", BASE, "--", *ENGINE)      # the work tree against BASE (committed and not)
    check(f"engine diff against {BASE} (raptorpath, raptorpath-math, raptorpath-core, raptorpath-wasm, gf256, Cargo.toml, Cargo.lock) is EMPTY", dd is not None and dd.returncode == 0 and dd.stdout.strip() == "", dd.stdout if dd else "git failed")
    dm = git("diff", "--stat", BASE, "--", "tools/l1/ref60-manifest.sha256")
    check(f"tools/l1/ref60-manifest.sha256 equals {BASE}'s (the strict suite-reuse reference is unchanged)", dm is not None and dm.returncode == 0 and dm.stdout.strip() == "", dm.stdout if dm else "git failed")
    dl = git("diff", "-U0", BASE, "--", "tools/l1/lib.sh")
    added = [ln for ln in (dl.stdout.splitlines() if dl else []) if ln[:1] in "+-" and not ln.startswith(("+++", "---"))]
    check(f"tools/l1/lib.sh differs from {BASE}'s by the c3j0 scenario only (its comment and its line)", dl is not None and dl.returncode == 0 and len(added) == 2 and all(("c3j0" in ln) for ln in added) and added[0].startswith("+") and added[1].startswith("+"), added)
else:
    print(f"skip engine-diff witness: no git work tree or no commit {BASE} here (an archive); run test_harness62.py in the worktree")
check("the manifest the launch reuses is plan 60's (187 files)", len(rd("ref60-manifest.sha256").splitlines()) == 187)
shutil.rmtree(d, ignore_errors=True)
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
