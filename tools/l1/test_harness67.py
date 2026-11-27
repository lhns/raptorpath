#!/usr/bin/env python3
"""Tests for status 67's harness (plan 67: VM batch 1; plan67.py's seats and tables, the shell wiring in loopdiag_run_all.sh / loopdiag_p67.sh / loopdiag_battery.sh / tun67_battery.sh / tun_bulk.sh, the place38_parse hook).
The registration is docs/derivations/vm-batch-1-registration.md; the scorer's clauses are tested in test_s67.py, the launch paths run in test_stubrun.py (case67).

  1. the seats: identity strings, pins, trees, the S2 family
  2. the stage tables (counts, arms per cell, the registered interleave, seeds and reps, the S2 drop), the row specs, the block-1 size, the session stage lists and the switches
  3. the shell side (plan selectable, rehearsal list, the envelope's aborts, the tools-diff witness, the manifest coverage of tools/ci/flake_rule.py, the battery's per-seat binaries, the T driver's 16 commands)
Run: python3 test_harness67.py"""
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in [k for k in os.environ if k.startswith(("LD_",))]:
    os.environ.pop(_k, None)
import plan67  # noqa: E402

fails = 0
CHECKS = 0


def check(name, ok, detail=""):
    global fails, CHECKS
    CHECKS += 1
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


def rd(name):
    return open(os.path.join(HERE, name), encoding="utf-8", errors="replace", newline="").read()


def tables(**env):
    """plan67's tables under an environment (the switches are read at call time)."""
    old = {k: os.environ.get(k) for k in env}
    os.environ.update(env)
    try:
        return {p: plan67.plan(p) for p in ("67a", "67r", "67p", "67o", "67d")}, plan67.stages("67x")
    finally:
        for k, v in old.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v


# ═══ 1. the seats ═══
S = plan67.SEATS
check("seven seats: B0, S1, S2, the pacer seats P1 / P2 and their test-seams twins P1T / P2T", list(S) == list(plan67.SEAT_ORDER) == ["B0", "S1", "S2", "P1", "P2", "P1T", "P2T"])
check("the commit pins: NO seat has a default (a main merge moves the seat heads: every LD_PIN_<SEAT> is required environment, plan 51e's pattern); P1T / P2T read P1's and P2's pins",
      all(S[x]["pin"] is None for x in plan67.SEAT_ORDER) and all(S[x]["pin_env"] == "LD_PIN_" + x for x in ("B0", "S1", "S2", "P1", "P2")) and S["P1T"]["pin_env"] == "LD_PIN_P1" and S["P2T"]["pin_env"] == "LD_PIN_P2"
      and not hasattr(plan67, "REGISTERED_SHAS"))
check("S1 and S2 must carry `pfec=` (the ABORT-SEAT string check of registration 4.2 includes it) and `rlaw=`; B0 carries neither; every gauge seat carries the tunnel guard's refusal text and B0 does not",
      all("pfec=" in S[x]["need"] and "rlaw=" in S[x]["need"] and plan67.GUARD_MSG in S[x]["need"] for x in ("S1", "S2", "P1", "P2", "P1T", "P2T")) and "pfec=" in S["B0"]["forbid"] and "rlaw=" in S["B0"]["forbid"] and plan67.GUARD_MSG in S["B0"]["forbid"])
check("`pslot=` is S2's seat witness (4.1): S2, P2, P2T need it, S1 / B0 / P1 / P1T forbid it", all("pslot=" in S[x]["need"] for x in ("S2", "P2", "P2T")) and all("pslot=" in S[x]["forbid"] for x in ("B0", "S1", "P1", "P1T")))
check("the test-seams twins need `[ACCT-RX]` (compiled only under test-seams, registration 2.2) and no other seat may carry it; the pacer seats need the pacer's string",
      all("[ACCT-RX]" in S[x]["need"] for x in ("P1T", "P2T")) and all("[ACCT-RX]" in S[x]["forbid"] for x in ("B0", "S1", "S2", "P1")) and all("offered-load pacing ACTIVE" in S[x]["need"] for x in ("P1", "P2", "P1T", "P2T")))
check("no seat needs a string it also forbids", all(not (set(v["need"]) & set(v["forbid"])) for v in S.values()))
check("the twins build from the pacer seats' trees with --features test-seams; every other seat builds its own tree plain", S["P1T"]["tree"] == "P1" and S["P2T"]["tree"] == "P2" and S["P1T"]["features"] == "test-seams"
      and all(S[x]["features"] == "" for x in ("B0", "S1", "S2", "P1", "P2")))
check("seat_of maps every arm of every table to a seat (pool and dump arms to S1 / S2)",
      all(plan67.seat_of(a) in S for p in ("67a", "67r", "67p", "67o", "67d") for _, _, a, _, _ in plan67.plan(p)) and plan67.seat_of("S2P640") == "S2" and plan67.seat_of("S1D") == "S1"
      and plan67.seat_of("P1T") == "P1T" and plan67.seat_of("P1") == "P1" and plan67.seat_of("B0") == "B0" and plan67.seat_of("nonsense") is None)

# ═══ 2. the tables ═══
T, st = tables()
check("stage A: 60 rows, per cell S1 12, S2 12, B0 6 (registration 5.1), c2-100 and c8-100",
      len(T["67a"]) == 60 and all(sum(1 for r in T["67a"] if r[0] == c and r[2] == a) == n for c in ("c2-100", "c8-100") for a, n in (("S1", 12), ("S2", 12), ("B0", 6))))
check("stage R1: 30 rows at c2-100, S1 12, S2 12, B0 6", len(T["67r"]) == 30 and {r[0] for r in T["67r"]} == {"c2-100"} and all(sum(1 for r in T["67r"] if r[2] == a) == n for a, n in (("S1", 12), ("S2", 12), ("B0", 6))))
check("stage R2: 24 rows at c2-100, P1 6, P2 6, P1T 6, P2T 6", len(T["67p"]) == 24 and all(sum(1 for r in T["67p"] if r[2] == a) == 6 for a in ("P1", "P2", "P1T", "P2T")))
check("the pool block: 18 rows at c8-100, S1 and S2 x pools 170 / 256 / 640 x n = 3, seed 42", len(T["67o"]) == 18 and {r[0] for r in T["67o"]} == {"c8-100"} and {r[3] for r in T["67o"]} == {"42"}
      and {r[2] for r in T["67o"]} == {"%sP%d" % (s, p) for s in ("S1", "S2") for p in (170, 256, 640)} and all(sum(1 for r in T["67o"] if r[2] == a) == 3 for a in {r[2] for r in T["67o"]}))
check("the dump twins: 8 rows (S1 and S2, seeds 42 and 7, c2 and c8)", len(T["67d"]) == 8 and {r[2] for r in T["67d"]} == {"S1D", "S2D"})
check("every slot of every table is unique (cell, arm, seed, rep)", all(len({(r[0], r[2], r[3], r[4]) for r in rows}) == len(rows) for rows in T.values()))
check("the registered block (S1, S2, S1, S2, B0), seeds alternating 42 / 7 block by block: the first five rows of stage R1", [r[2] for r in T["67r"][:5]] == ["S1", "S2", "S1", "S2", "B0"] and [r[3] for r in T["67r"][:5]] == ["42"] * 5
      and [r[3] for r in T["67r"][5:10]] == ["7"] * 5 and [r[4] for r in T["67r"][:5]] == [1, 1, 2, 2, 1])
check("stage A alternates its cells inside each block index (c2 block, c8 block)", [r[0] for r in T["67a"][:10]] == ["c2-100"] * 5 + ["c8-100"] * 5)
check("the smokes: A one c8 row per seat, R1 one row per seat, R2 the four pacer arms", [r[2] for r in plan67.plan("67a", True)] == ["S1", "S2", "B0"] and {r[0] for r in plan67.plan("67a", True)} == {"c8-100"}
      and [r[2] for r in plan67.plan("67r", True)] == ["S1", "S2", "B0"] and [r[2] for r in plan67.plan("67p", True)] == ["P1", "P2", "P1T", "P2T"])
sp = {(p, c, a): plan67.armspec(p, c, a) for p in ("67a", "67r", "67p", "67o", "67d") for c, _, a, _, _ in plan67.plan(p)}
check("A: Auto at rho = 1 (no RWM_C_RELIABLE), 600 MB, 200 s; R1: Realtime, 100 MB, 150 s, rho = 1", sp[("67a", "c2-100", "S1")] == (600000000, 200, "auto", []) and sp[("67r", "c2-100", "B0")] == (100000000, 150, "realtime", []))
check("R2 and the twins: block E's PM shape (plan51e: Realtime, evicting RWM_C_RELIABLE=0, RWM_PERF_RATE_MBIT=35, 262.5 MB, 75 s)",
      all(sp[("67p", "c2-100", a)] == (262500000, 75, "realtime", ["RWM_SLAW=1", "RWM_C_RELIABLE=0", "RWM_PERF_RATE_MBIT=35"]) for a in ("P1", "P2", "P1T", "P2T")))
check("the pool block sets RWM_STORE_PATH_POOL per arm, the dump twins RWM_SUCC_DUMP=1; both at Auto 600 MB 200 s", sp[("67o", "c8-100", "S2P256")] == (600000000, 200, "auto", ["RWM_STORE_PATH_POOL=256"]) and sp[("67d", "c2-100", "S1D")] == (600000000, 200, "auto", ["RWM_SUCC_DUMP=1"]))
check("an unknown slot has no spec", plan67.armspec("67a", "c1s-400", "S1") is None and plan67.armspec("67a", "c2-100", "P1") is None and plan67.armspec("67o", "c2-100", "S1P170") is None)
check("block-1 sizes: A 10 rows (5 per cell), R1 5, R2 4, the pool block 6, the dump twins none", [plan67.block1_n(p) for p in ("67a", "67r", "67p", "67o", "67d")] == [10, 5, 4, 6, 0])
Td, std = tables(LD_DROP_S2="1")
check("LD_DROP_S2=1 removes every S2 / P2 / P2T row and nothing else (A 36 rows, R1 18, R2 12, pool 9, dump 4) and stage G2", [len(Td[p]) for p in ("67a", "67r", "67p", "67o", "67d")] == [36, 18, 12, 9, 4]
      and not any(r[2].startswith(("S2", "P2")) for rows in Td.values() for r in rows) and plan67.stages("67g") == ["g1", "g2"] and plan67.SESSION["67g"] == ("g1", "g2"))
os.environ["LD_DROP_S2"] = "1"
check("... and the guard-2 stage g2 leaves the session list", plan67.stages("67g") == ["g1"] and plan67.stages("67") == ["g1", "a", "r1", "t", "r2"])
os.environ.pop("LD_DROP_S2")
check("the session stage lists: 67 (rehearsal) g1 g2 a r1 t r2; 67g g1 g2; 67x a r1 t r2 pool (the dump twins only with LD_DUMP67=1)", plan67.stages("67") == ["g1", "g2", "a", "r1", "t", "r2"] and plan67.stages("67g") == ["g1", "g2"] and st == ["a", "r1", "t", "r2", "pool"])
_, st1 = tables(LD_DUMP67="1")
_, st2 = tables(LD_DUMP67="1", LD_CUT67="dump")
_, st3 = tables(LD_G2_FAIL67="1", LD_DROP_S2="1")
check("LD_DUMP67=1 adds the dump twins before the pool block, LD_CUT67=dump (the one registered cut) removes them; LD_G2_FAIL67=1 drops R2, the pool block and the dump twins and keeps A, R1 and T (registration 5.2)",
      st1 == ["a", "r1", "t", "r2", "dump", "pool"] and st2 == ["a", "r1", "t", "r2", "pool"] and st3 == ["a", "r1", "t"])
check("the budgets: 67 under launch + 2.25 h, 67g under + 3 h, 67x under + 4.5 h (the registered LD_HARD offsets)", plan67.BUDGET_S["67"] < 2.25 * 3600 and plan67.BUDGET_S["67g"] < 3 * 3600 and plan67.BUDGET_S["67x"] < 4.5 * 3600
      and plan67.BUDGET_S["67"] < plan67.BUDGET_S["67g"] < plan67.BUDGET_S["67x"])
check("the replacement caps: 4 slots in A and the other stages, 2 in R1 (plan 64's stage R rule)", plan67.replace_cap("67a") == 4 and plan67.replace_cap("67r") == 2 and plan67.replace_cap("67o") == 4)
check("registered constants: MIN_LIVE_A 10, MIN_TWIN 5, 6 blocks, 3 twin reps, pools 170 / 256 / 640 (the test_stubrun fast mode patches these lines; each must exist exactly once)",
      plan67.MIN_LIVE_A == 10 and plan67.MIN_TWIN == 5 and plan67.N_BLOCKS == 6 and plan67.N_TWIN_REPS == 3 and plan67.POOLS == (170, 256, 640) and plan67.N_POOL_REPS == 3
      and all(rd("plan67.py").count(x) == 1 for x in ("N_BLOCKS = 6  ", "N_TWIN_REPS = 3  ", "N_POOL_REPS = 3  ", "MIN_LIVE_A = 10  ", "MIN_LIVE_B0 = 4  ", "MIN_TWIN = 5  ")))

check("the replaceable statuses: VOID-RC (a crash) is not replaceable, NO_DATA keeps its one retry", "VOID-RC" not in plan67.REPLACEABLE and "NO_DATA" in plan67.REPLACEABLE)


# the CLI the shell reads
def cli(*a, env=None):
    e = dict(os.environ)
    e.update(env or {})
    r = subprocess.run([sys.executable, os.path.join(HERE, "plan67.py")] + list(a), capture_output=True, text=True, env=e)
    return r.returncode, r.stdout.strip()


rc, o = cli("armspec", "67p", "c2-100", "P2T")
check("plan67.py armspec prints `BYTES TIMEOUT HINT ENV...` and rc 1 with UNKNOWN-ARM on an unknown slot", rc == 0 and o == "262500000 75 realtime RWM_SLAW=1 RWM_C_RELIABLE=0 RWM_PERF_RATE_MBIT=35" and cli("armspec", "67a", "c2-100", "P1")[0] == 1)
rc, o = cli("strings", "S2")
check("plan67.py strings prints `need` / `forbid` lines (pfec= among S2's needs)", rc == 0 and "need pfec=" in o.splitlines() and "need pslot=" in o.splitlines() and "forbid [ACCT-RX]" in o.splitlines())
check("plan67.py pin prints the env name alone for every seat (no default)", all(cli("pin", s)[1] == "LD_PIN_" + s[:2] for s in ("B0", "S1", "S2", "P1", "P2")) and cli("pin", "P1T")[1] == "LD_PIN_P1")
check("plan67.py count / plan / block1n / budget / stages agree with the module", cli("count", "67a")[1] == "60" and cli("count", "67a", "smoke")[1] == "3" and cli("block1n", "67o")[1] == "6" and cli("budget", "67x")[1] == str(plan67.BUDGET_S["67x"])
      and cli("stages", "67x", env={"LD_DUMP67": "1"})[1] == "a r1 t r2 dump pool")

# ═══ 2b. the test seams (#128 / #134): plan67.REGISTERED, `plan67.py seams` ═══
check("REGISTERED holds the registered design (6 blocks, 3 twin reps, 3 pool reps, minima 10 / 4 / 5, 96 G rows) and the module's constants equal it", plan67.REGISTERED == {"N_BLOCKS": 6, "N_TWIN_REPS": 3, "N_POOL_REPS": 3, "MIN_LIVE_A": 10, "MIN_LIVE_B0": 4, "MIN_TWIN": 5, "G_ROWS67": 96}
      and plan67.seams(96) == [] and plan67.seams() == [])
_old = plan67.N_BLOCKS, plan67.MIN_TWIN
plan67.N_BLOCKS, plan67.MIN_TWIN = 2, 2
check("seams names a shrunk constant (the fast-mode edit) with its registered value", plan67.seams(96) == ["N_BLOCKS=2(registered 6)", "MIN_TWIN=2(registered 5)"], plan67.seams(96))
plan67.N_BLOCKS, plan67.MIN_TWIN = _old
check("seams names a shrunk G_ROWS67 (the shell passes it in)", plan67.seams(16) == ["G_ROWS67=16(registered 96)"])
for _k in plan67.SEAM_ENV:
    os.environ[_k] = "1"
check("seams names the environment seams: LD_HARD_MIN_S, R67_T1_S, LD_ROW_BYTES, LD_BLOCK_ROW_S, session 0's R67_N, R67_BUDGET_S, R67_SIM and the stub path seams LD_DEVSHM, LD_PROCFS, LD_LOCK_VM, LD_LOCK_RP, LD_TMPD",
      plan67.seams(96) == list(plan67.SEAM_ENV) and plan67.SEAM_ENV == ("LD_HARD_MIN_S", "R67_T1_S", "LD_ROW_BYTES", "LD_BLOCK_ROW_S", "R67_N", "R67_BUDGET_S", "R67_SIM", "LD_DEVSHM", "LD_PROCFS", "LD_LOCK_VM", "LD_LOCK_RP", "LD_TMPD"))
for _k in plan67.SEAM_ENV:
    os.environ.pop(_k)
for _k in ("R67_N", "R67_BUDGET_S", "R67_SIM"):
    check(f"`plan67.py seams` prints {_k} when it is set (so run_all aborts ABORT-SEAM without LD_TEST_SEAMS=1)", cli("seams", "96", env={_k: "3"})[1] == _k and plan67.seams(96) == [], cli("seams", "96", env={_k: "3"}))
check("the fast-mode shrink table (test_stubrun.py FAST_PATCHES) names the constants this file registers, each anchor once in plan67.py / loopdiag_p67.sh",
      all(rd("plan67.py").count(x) == 1 for x in ("N_BLOCKS = 6  ", "N_TWIN_REPS = 3  ", "N_POOL_REPS = 3  ", "MIN_LIVE_A = 10  ", "MIN_LIVE_B0 = 4  ", "MIN_TWIN = 5  ")) and rd("loopdiag_p67.sh").count("G_ROWS67=96     #") == 1
      and all(x in rd("test_stubrun.py") for x in ('("plan67.py", "N_BLOCKS = 6  "', '("plan67.py", "MIN_LIVE_A = 10  "', '("loopdiag_p67.sh", "G_ROWS67=96     #"')))

# ═══ 3. the shell side ═══
runall, p67, bat, tun = rd("loopdiag_run_all.sh"), rd("loopdiag_p67.sh"), rd("loopdiag_battery.sh"), rd("tun67_battery.sh")
check("run_all: plans 67pre / 67 / 67g / 67x are selectable (P67), rehearsable only as 67 (67pre is not in the REHEARSE list), and the envelope is sourced after the helpers are defined and never falls through",
      'P67=0; case "$PLAN" in 67|67g|67x|67pre) P67=1 ;; esac' in runall and re.search(r"^  67\|67g\|67x\|67pre\) P_SMOKE=", runall, re.M) is not None and '|59|60a|60b|65|67) ;; *) echo "ABORT-REHEARSE' in runall and "ABORT-PLAN LD_PLAN=$PLAN (67pre|67|67g|67x|" in runall
      and '[ "$P67" != 1 ] || source "$HERE/loopdiag_p67.sh"' in runall and runall.index("framing_bench() {") < runall.index('source "$HERE/loopdiag_p67.sh"') and p67.rstrip().endswith('fail_all "plan $PLAN: stages not run to the end: ${P67BAD[*]}${P67CUT[*]:+; cut at LD_HARD: ${P67CUT[*]}}"'))
check("run_all: the budget guard (plan67.py budget), no CAND commit / LD_BASE_SHA requirement for plan 67 (its seats have their own pins), the tools gates incl. test_s67 / test_harness67, and the python and bash tools tests of run_suites",
      'P67_NEED="$(python3 ./plan67.py budget "$PLAN")"' in runall and 'ABORT-BUDGET plan $PLAN: ${P67_LEFT}s before LD_HARD' in runall and '[ "$P65" != 1 ] && [ "$P67" != 1 ]; then\n  CAND_SHA=' in runall
      and "test_s67.py test_harness67.py; do" in runall and '|| [ "$P65" = 1 ] || [ "$P67" = 1 ] || { [ "$P46" = 1 ]' in runall and '[ "$P65" = 1 ] || [ "$P67" = 1 ]; } && { tst bashtests' in runall)
check("run_all: the build block is skipped for plan 67 (the seats are built by loopdiag_p67.sh)", 'if [ "$P67" = 1 ]; then\n  era "LD-ALL plan $PLAN: the seats are built / installed by loopdiag_p67.sh' in runall)
check("the crlf guards name every new file (run_all, battery)", all(f in runall and f in bat for f in ("plan67.py", "s67_score.py", "s67_fixtures.py", "test_s67.py", "test_harness67.py", "tun67_battery.sh", "loopdiag_p67.sh")))
for f in ("plan67.py", "s67_score.py", "s67_fixtures.py", "test_s67.py", "tun67_battery.sh", "loopdiag_p67.sh", "ref_manifest67.sh", "repeat67_run.sh", "binid67_run.sh"):
    check(f"{f} exists", os.path.exists(os.path.join(HERE, f)))
check("p67: every launch refuses to build (ABORT-BUILD, LD_BIN_PREBUILT_<SEAT> + LD_BIN_SHA_<SEAT> required), the rehearsal builds each seat in its own fresh target dir, the twins with --features test-seams (plan67.SEATS features)",
      "launches never build" in p67 and 'needs $_pin' in p67 and '${_feat:+--features $_feat}' in p67 and 'target-$_s' in p67 and "is not fresh" in p67)
check("p67: ABORT-SEAT on a commit pin (COMMIT against LD_PIN_<SEAT>; B0 / P1 / P2 have none registered), on the identity strings (need / forbid), on the tools-diff witness; ABORT-BUILD on a binary sha against LD_BIN_SHA_<SEAT> and on two identical seats",
      p67.count("ABORT-SEAT") >= 6 and "is unset (every seat's commit pin is required, none has a default" in p67 and 'plan67.py strings "$_s"' in p67 and "tools-diff witness" in p67 and "ABORT-BUILD plan 67 seat $_s: built / installed" in p67 and "are the same binary" in p67)
check("p67: the pacer seats' tools trees may differ from the harness's by the one lib.sh forward line (cmp after removing it) and by nothing else; every other seat's tools tree is byte-identical",
      "sed 's/ RWM_PERF_RATE_MBIT//' \"$_td/lib.sh\" | cmp -s - \"$HERE/lib.sh\"" in p67 and '[ "$_dl" = "./lib.sh " ]' in p67 and '[ "$_tt" = "$P67HT" ]' in p67)
check("p67: the suite-reuse manifest covers tools/ci/flake_rule.py (src_manifest67), through this function only: run_all's src_manifest() is unchanged",
      "tools/ci/flake_rule.py -type f" in p67 and "src_manifest67() {" in p67 and "ABORT-SUITES plan 67 suite-reuse MISS" in p67 and "flake_rule.py" not in re.search(r"src_manifest\(\) \{.*?\n\}", runall, re.S).group(0))
check("ref_manifest67.sh lists the same sets as src_manifest67 (Cargo files, the engine crates, vendor/, flake_rule.py)", all(x in rd("ref_manifest67.sh") for x in ("raptorpath/src/", "raptorpath/tests/", "gf256/src/", "vendor/", "tools/ci/flake_rule")))
check("p67: the S2 family aborts name LD_DROP_S2=1 (guard 1 launches alone, registration 5.2); LD_G2_FAIL67 implies LD_DROP_S2; LD_CUT67 accepts only `dump`",
      "relaunch with LD_DROP_S2=1" in p67 and '[ "$LD_G2_FAIL67" != 1 ] || LD_DROP_S2=1' in p67 and "ABORT-CUT plan 67" in p67 and ",,|,dump,)" in p67)
check("p67: the block-1 stop (battery rc 6) leaves FAILED-<stage> and the NEXT stage still runs; a G1 verdict does not stop G2 (every stage returns to the session loop)",
      'if [ "$brc" = "6" ]; then touch "$dir/FAILED-$name"' in p67 and 'for _st in "${P67STAGES[@]}"; do\n  stage67 "$_st"; _rc=$?\n  case "$_rc" in 0) ;; 7|8) P67CUT+=("$_st(rc=$_rc)") ;; *) P67BAD+=' in p67 and "fail_all" not in p67.split('if [ "$brc" = "6" ]')[1].split("return 6")[0])
check("p67: stage G is plan 41d with LD_WITNESS67 = g1 (S1 against B0) / g2 (S2 against S1) and BASE's commit as LD_BASE_SHA; the rehearsal runs only smokes (no battery), a launch only the battery",
      'g1) cand=S1; base=B0; w=g1' in p67 and 'g2) cand=S2; base=S1; w=g2' in p67 and 'LD_PLAN=41d LD_WITNESS67="$w"' in p67 and 'LD_BASE_SHA="${P67PIN[$base]}"' in p67 and 'if [ "$REHEARSE" = 1 ]; then\n    era "LD-ALL stage $name smoke' in p67)
check("p67: the rehearsal writes AMENDMENT2.txt (the shas, the dump-twin decision from DUMP-TWINS-NEEDED, the suite-reuse and tools manifests) and ends LD-ALL-REHEARSAL-DONE; plan 67 without LD_REHEARSE=1 is ABORT-REHEARSE",
      "AMENDMENT2.txt" in p67 and "DUMP-TWINS-NEEDED" in p67 and "LD-ALL-REHEARSAL-DONE" in p67 and 'ABORT-REHEARSE plan 67 is the rehearsal' in p67)
check("p67: every seat's variables ride SENV explicitly (sudo resets the environment): LD_BIN_<SEAT>, LD_SHA_<SEAT>, LD_TOOLS_<SEAT> (pacer seats), LD_SEATS67, LD_DROP_S2, LD_DUMP67, LD_WITNESS67",
      all(x in p67 for x in ('"LD_BIN_$s=${P67BIN[$s]}"', '"LD_SHA_$s=${P67SHA[$s]}"', '"LD_TOOLS_$s=${P67TOOLS[$s]}"', 'LD_SEATS67="$seats"', 'LD_DROP_S2="$LD_DROP_S2"', 'LD_DUMP67="$LD_DUMP67"', 'LD_WITNESS67="$w"')))
check("battery: plans 67a / 67r / 67p / 67o / 67d are valid, ride the plan-64 flags (P38, PCAND, TRIES=1), scrub the pacer / delta env, run the arm's SEAT binary and a pacer seat's own perf_rwm_c.sh, parse with s67_score.py and keep the twins' account lines (ACCT-KEPT)",
      'P67=0; case "$PLAN" in 67a|67r|67p|67o|67d) P67=1 ;; esac' in bat and '[ "$P67" = 1 ] && { TRIES=1; P38=1; PCAND=1; }' in bat and '|| [ "$P67" = 1 ]; } && SCRUB_EXTRA=(-u RWM_DELTA' in bat
      and 'RBIN="${!p67_bv:-}"; RWANT="${!p67_sv:-}"' in bat and '[ -z "${!p67_tv:-}" ] || PERFDIR="${!p67_tv}"' in bat and "parser=./s67_score.py" in bat and "ACCT-KEPT side=cli" in bat and "ACCT-KEPT side=srv" in bat and 'cd "$PERFDIR"' in bat)
check("battery: every seat's file, sha and identity strings are checked before any row (ABORT-SHA / ABORT-SEAT), the block-1 rule stops a stage with exit 6, a precondition void is replaced once by plan67.py replace",
      'plan67.py strings "$_seat"' in bat and "ABORT-SEAT plan 67 seat $_seat" in bat and 'exit 6' in bat and 'python3 ./plan67.py replace "$PLAN" "$OUT"' in bat and 'python3 ./s67_score.py block1 "$PLAN" "$OUT"' in bat)
check("battery: LD_WITNESS67 is defined for plan 41d with g1 | g2 only (ABORT-PLAN otherwise)", 'W67="${LD_WITNESS67:-}"' in bat and "ABORT-PLAN LD_WITNESS67=$W67" in bat)
pp = rd("place38_parse.py")
check("place38_parse: stage G's hook (LD_PLAN=41d with LD_WITNESS67) calls s67_score.g_row and, at the end of make_row, s67_score.g_status", 'os.environ.get("LD_WITNESS67") in ("g1", "g2")' in pp and "s67_score.g_row(row, arm, cli, srv, drv, sha," in pp and "s67_score.g_status(row)" in pp)
check("run_all: a plan-67 session aborts ABORT-SEAM (rc 3, before the locks) on any active test seam unless LD_TEST_SEAMS=1; the check is plan67.py seams with G_ROWS67 read from loopdiag_p67.sh",
      'if [ "$P67" = 1 ] && [ "${LD_TEST_SEAMS:-}" != 1 ]; then' in runall and 'python3 ./plan67.py seams "$(sed -n' in runall and "ABORT-SEAM plan $PLAN: test seam(s) active without LD_TEST_SEAMS=1: $P67_SEAMS" in runall
      and runall.index("ABORT-SEAM plan $PLAN") < runall.index("take_lock"))
check("p67: LD_BLOCK_ROW_S and LD_ROW_BYTES reach a battery only under LD_TEST_SEAMS=1; the commit pin of every seat is required (no default): ABORT-SEAT `<LD_PIN_SEAT> is unset`",
      '[ "${LD_TEST_SEAMS:-}" != 1 ] || SENV+=(LD_BLOCK_ROW_S="${LD_BLOCK_ROW_S:-}" LD_ROW_BYTES="${LD_ROW_BYTES:-}")' in p67 and 'LD_WITNESS64=0 LD_BLOCK_ROW_S=' not in p67
      and '_pv="${!_penv:-}"' in p67 and "is unset (every seat's commit pin is required, none has a default" in p67 and "_pdef" not in p67 and "(S1, S2: the registered default" not in p67)
check("p67: the rehearsal smoke runs under timeout min(1800 s, the time left to LD_HARD); a timeout (rc 124) is ABORT-SMOKE with the reason",
      'smkto=$(( LD_HARD - $(date +%s) )); [ "$smkto" -le 1800 ] || smkto=1800' in p67 and 'timeout "$smkto" sudo -n env "${SENV[@]}" LD_TAG=smoke LD_SMOKE=1 bash ./$runner' in p67
      and 'if [ "$sbrc" = "124" ]; then touch "$dir/ABORT-SMOKE"; era "ABORT-SMOKE stage $name: the smoke timed out after ${smkto}s' in p67)
check("p67: a battery (or a G replacement run) that exits rc 124 gets `LD-BATTERY-TRUNCATED at LD_HARD <epoch>` appended to its ledger (ld.log / repl.log) and a TRUNCATED marker, for every stage (one code path)",
      p67.count("LD-BATTERY-TRUNCATED at LD_HARD") == 2 and 'if [ "$brc" = "124" ]; then' in p67 and '>> "$dir/ld.log"; touch "$dir/TRUNCATED"' in p67 and '>> "$dir/repl.log"; touch "$dir/TRUNCATED"' in p67
      and p67.index('if [ "$brc" = "124" ]; then') < p67.index('if [ "$brc" = "6" ]'))
check("p67: a cut stage returns 7 (TRUNCATED) and a stage not started for lack of time 8; a session whose only not-ok stages are 7 / 8 ends TRUNCATED-ALL (exit 7, never DONE-ALL, never FAILED-ALL); any other not-ok stage is FAILED-ALL",
      '[ -e "$dir/TRUNCATED" ] && return 7' in p67 and 'touch "$dir/NOT-STARTED"; return 8; fi' in p67 and "7|8) P67CUT+=" in p67 and 'touch "$RUN/TRUNCATED-ALL"; persist; echo "LD-ALL-TRUNCATED"; exit $P67_EXIT_NONPASS; fi' in p67 and "P67_EXIT_NONPASS=7 " in p67
      and p67.index('touch "$RUN/TRUNCATED-ALL"') < p67.index('fail_all "plan $PLAN: stages not run to the end'))
check("p67: a session cut at LD_HARD lists the non-pass verdict lines of the cut stages (s67_score.py nonpass) in the `LD-ALL TRUNCATED` era line and in $RUN/score.txt; the exit code and the TRUNCATED-ALL vs FAILED-ALL choice are unchanged",
      "nonpass67()" in p67 and 'python3 ./s67_score.py nonpass "$_d/score.txt"' in p67 and 'non-pass verdict lines of the cut stages: ${P67NP:-none}' in p67 and "NONPASS-IN-CUT-STAGE" in p67
      and p67.index("nonpass67 | sed 's/^/NONPASS-IN-CUT-STAGE /'") < p67.index('} > "$RUN/score.txt"') and p67.index("${P67NP:-none}") < p67.index('touch "$RUN/TRUNCATED-ALL"'))
check("p67: an R2 smoke failure in the rehearsal is R2 VOID, not FAILED-ALL (registration 5.2 / 4.4): the R2-VOID marker beside the kept ABORT-SMOKE, the other stages' smokes run, AMENDMENT2.txt says `R2 VOID`, the session ends LD-ALL-REHEARSAL-R2-VOID with the sentinel REHEARSAL-R2-VOID and the one non-pass exit; every other stage's smoke failure stays fail_all",
      '[ "$_st" = "r2" ] && [ "$_rc" = "1" ]' in p67 and "R2-VOID" in p67 and 'echo "R2 VOID (rehearsal smoke failed: guard 2 waits for an R2 re-run)"' in p67
      and 'echo "LD-ALL-REHEARSAL-R2-VOID"; exit $P67_EXIT_NONPASS' in p67 and 'touch "$RUN/REHEARSAL-R2-VOID"' in p67 and 'fail_all "rehearsal: stage $_st smoke did not pass ($_st)"' in p67
      and p67.index('"$_st" = "r2"') < p67.index('fail_all "rehearsal: stage $_st smoke did not pass') and p67.count("LD-ALL-REHEARSAL-DONE") >= 1
      and 'SENTINELS="DONE-ALL FAILED-ALL TRUNCATED-ALL REHEARSAL-R2-VOID ' in open(os.path.join(HERE, "loopdiag_run_all.sh")).read())
check("run_all: TRUNCATED-ALL is a sentinel (cleared and proven like DONE-ALL / FAILED-ALL)", 'SENTINELS="DONE-ALL FAILED-ALL TRUNCATED-ALL ' in open(os.path.join(HERE, "loopdiag_run_all.sh")).read())
check("p67: stage T is ok only without a TUN-BRINGUP-FAIL line in its ledger (ABORT-BRINGUP-T)", "! grep -aq '^TUN-BRINGUP-FAIL' \"$dir/ld.log\" && ok=1" in p67)
check("battery: plan 41d with LD_WITNESS67 applies the block-1 rule after G_BLOCK1_N rows (s67_score.py block1g, exit 6), not on the smoke and not on the replacement run",
      'b1g="$(python3 -c \'import s67_score; print(s67_score.G_BLOCK1_N)\')"' in bat and 'python3 ./s67_score.py block1g "$W67" "$OUT"' in bat and '[ -n "${LD_REPL_FILE:-}" ] || b1g=' in bat and '[ "$1" = "1" ] || [ -n "${LD_REPL_FILE:-}" ] || b1g' in bat)
check("battery: a plan-67 ledger, stage G's included, carries its drop state (`LD-PLAN67 plan= [stage=] drop_s2=` header line, read by s67_score.ledger_drop_s2; the environment is only the scorer's fallback); no dump= field (nothing reads it)",
      'if [ "$P67" = 1 ] || [ -n "$W67" ]; then echo "LD-PLAN67 plan=$PLAN${W67:+ stage=$W67} drop_s2=${LD_DROP_S2:-0}" >> "$OUT"; fi' in bat and bat.index('echo "LD-PLAN67 plan=') < bat.index("# ── plan 41c ──"))
check("stage T: its ledger carries the LD-PLAN67 drop-state line and the envelope forwards LD_DROP_S2 to it", 'echo "LD-PLAN67 plan=t drop_s2=${LD_DROP_S2:-0}"' in open(os.path.join(HERE, "tun67_battery.sh")).read() and 'LD_HARD="${LD_HARD:-}" LD_DROP_S2="$LD_DROP_S2"); }' in open(os.path.join(HERE, "loopdiag_p67.sh")).read())
check("block1g: the CLI doc line, the battery comment and the code agree: the first row of EACH of the 8 (cell, arm) pairs", "the first row of each of the 8 (cell, arm) pairs of block 1" in rd("s67_score.py") and "each (cell, arm)'s first row must be LIVE" in bat
      and "for cell, arm in [(c, a) for c, _, a, _, _ in pp.plan41c()[:G_BLOCK1_N]]" in rd("s67_score.py"))
import s67_score  # noqa: E402
check("block 1 of G = plan 41c's first 8 rows: the smallest prefix that holds every (cell, arm) once", s67_score.G_BLOCK1_N == 8 and len({(c, a) for c, _, a, _, _ in __import__("place38_parse").plan41c()[:8]}) == 8
      and len({(c, a) for c, _, a, _, _ in __import__("place38_parse").plan41c()[:7]}) == 7)
check("tun67_battery: tun_bulk.sh's stdout (TUN-BRINGUP-FAIL included) goes to the ledger; the comment no longer says the scorer ignores it, and names ABORT-BRINGUP-T",
      'bash ./tun_bulk.sh c2 "$hint" 100000000 "$REPS" "$seat-$hint" >> "$OUT" 2>&1' in tun and "ABORT-BRINGUP-T" in tun and "it is\n# not an abort here" not in tun and "read by the scorer; it is" not in tun)

keys = ["window_out_of_order", "window_coded_only", "window_generation_coding", "window_systematic_repair"]
check("tun67_battery: 4 keys x (check, run) x (S1, B0) = 16 refusal commands, S1 first; each key's TOML sets only that key to true; the exact refusal text is in the scorer, B0's refusal is ABORT-SEAT",
      all(k in tun for k in keys) and 'for seat in S1 B0; do' in tun and 'for cmd in check run; do' in tun and "printf '%s = true\\n' \"$key\"" in tun and "T67REF seat=$seat cmd=$cmd key=$key rc=$rc cfg=${cfg:--} err=$err" in tun
      and 'ABORT-SEAT plan 67 stage T: B0' in tun and "perf/native-object mode" in tun)
check("tun67_battery: 5 transfers of 100 MB per (seat, hint) at c2, seed 42, hints auto and realtime, both ends load a TOML with no object-mode key at info level (TB_CONFIG / TB_RUST_LOG); the smoke is REPS=1 at auto",
      'REPS=5; HINTS=(auto realtime)' in tun and '[ "$SMOKE" = "1" ] && { REPS=1; HINTS=(auto); }' in tun and 'tun_bulk.sh c2 "$hint" 100000000 "$REPS"' in tun and "TB_CONFIG=" in tun and "TB_RUST_LOG=info" in tun and "SEED=42" in tun)
tb = rd("tun_bulk.sh")
check("tun_bulk.sh: the four object-mode fields of each endpoint's `resolved configuration` line are printed as TUNCFG (additive: TUNPIPE / TUNCAD / TUNREP are unchanged), the config / log level / tmp dir are opt-in seams",
      'echo "TUNCFG cli=$(cfg_of $TB_TMP/tb-c.log) srv=$(cfg_of $TB_TMP/tb-s.log)"' in tb and "${TB_CONFIG:+--config $TB_CONFIG}" in tb and "${TB_RUST_LOG:+RUST_LOG=$TB_RUST_LOG}" in tb and 'TB_TMP="${TB_TMPD:-/tmp}"' in tb)

rep, bid = rd("repeat67_run.sh"), rd("binid67_run.sh")
check("repeat67_run.sh: N = 30 default, S1 repeats `--lib -- config::`, S2 the two gating targets s06_no_copy and holeclass_reachability (every non-ignored test; s06_reactive is all #[ignore]), the sim tests run twice with an equal-digest check, any failure is ABORT-REPEAT for THAT seat, a budget cut is written (REPEAT-CUT), no quarantine",
      'N="${R67_N:-30}"' in rep and "-- config::" in rep and "--test s06_no_copy --test holeclass_reachability" in rep and "--test s06_reactive" not in rep and "SIM-DIGEST-DIFFERS" in rep and "ABORT-REPEAT seat=$seat" in rep and "REPEAT-CUT seat=" in rep and "flake_rule" not in rep)
check("binid67_run.sh: B0, S1 and S2 are packed with their COMMIT and built by binid_run.sh at one path; the shas must be pairwise different (BINID67 FAIL otherwise)",
      'for seat in "${SEATS[@]}"; do' in bid and 'set -- B0 S1 S2' in bid and 'bash ./binid_run.sh "${args[@]}"' in bid and "two seats built to the same binary" in bid and "BINID67 PASS" in bid)
check("the crlf guards name the new scripts", all(f in runall and f in bat for f in ("repeat67_run.sh", "binid67_run.sh", "lockcheck67.sh")) and os.path.exists(os.path.join(HERE, "lockcheck67.sh")))

# ═══ 4. session 0 (LD_PLAN=67pre; loopdiag_p67pre.sh; the launch paths run in test_stubrun.case67pre) ═══
pre = rd("loopdiag_p67pre.sh")
code = "\n".join(ln for ln in pre.splitlines() if not ln.lstrip().startswith("#"))      # the steps in code order, not the header's
lib = rd("lib_battery.sh")
check("plan67: 67pre has the registered budget (17 040 s = LD_HARD launch + 4.9 h less the 600 s slack) and no stage, and `67p` stays plan 67's R2 battery plan (so session 0 needs its own token)",
      plan67.BUDGET_S["67pre"] == 17040 == int(4.9 * 3600) - 600 and plan67.SESSION["67pre"] == () and plan67.stages("67pre") == [] and plan67.STAGE_PLAN["r2"] == "67p")
check("p67: 67pre needs the seats B0 S1 S2, sources the session-0 file right after the seat pins (before any build), and src_manifest67 is defined above that point (one definition for the record and for section 5)",
      "67pre) P67NEED=(B0 S1 S2)" in p67 and p67.index("src_manifest67() {") < p67.index('source "$HERE/loopdiag_p67pre.sh"') < p67.index("# ── 2. the binaries") and p67.count("src_manifest67() {") == 1 and "loopdiag_p67pre.sh" in runall.split("crlf_guard plan67.py")[1].split("\n")[0])
check("p67pre: the steps in the registered order (check, gates reference, suites, repeat, visualizer, golden, binid, record), every path exits, no build and no battery",
      [code.index(x) for x in ("lockcheck67.sh", "check --all-targets", "--lib gates", "run_suites", "repeat67_run.sh", "p0_manifest \"$_s\"", "build_visualizer.sh", "raptorpath-wasm golden", "p0_manifest S2", "binid67_run.sh", "BINTEST")]
      == sorted(code.index(x) for x in ("lockcheck67.sh", "check --all-targets", "--lib gates", "run_suites", "repeat67_run.sh", "p0_manifest \"$_s\"", "build_visualizer.sh", "raptorpath-wasm golden", "p0_manifest S2", "binid67_run.sh", "BINTEST"))
      and pre.rstrip().endswith("exit 0") and " build --release" not in pre and "sudo" not in pre and "loopdiag_battery" not in pre)
check("p67pre: the abort strings of registration 5.4 (ABORT-PREFLIGHT / ABORT-TESTS / ABORT-REPEAT / ABORT-BUDGET per step, GATES-FINDING-ON-MAIN, GATES-B0-FINDING), every step is bounded by timeout at LD_HARD and starts only with 600 s left",
      all(s in pre for s in ("ABORT-PREFLIGHT plan 67pre seat $_s: cargo check --all-targets failed", "ABORT-PREFLIGHT plan 67pre seat B0: the gates reference did not run", "gates failure not equal on B0", "ABORT-TESTS plan 67pre seat $_s:",
                             "(ADR-0080: REAL or non-quarantined FLAKE)", "ABORT-REPEAT plan 67pre seat $_s", "ABORT-PREFLIGHT plan 67pre binid:", "ABORT-BUDGET plan 67pre step $1", "GATES-FINDING-ON-MAIN seat=$_s", "GATES-B0-FINDING", "LD-ALL-SESSION0-DONE"))
      and "-ge 600" in pre and pre.count('timeout "$(p0_left)"') >= 6 and "SUITES_HARD" in pre and "SUITES_HARD" in runall)
check("p67pre: the children get run_all's locks (LB_LOCK_PARENT, the envelope's lock paths) and the repeat / binid test seams only under LD_TEST_SEAMS=1; the repeat's targets go to disk, not the tmpfs run dir",
      'LB_LOCK_PARENT="$$"' in pre and 'RWM_VM_LOCK="$LOCK_VM"' in pre and 'RWM_RP_LOCK="$LOCK_RP"' in pre and '[ "${LD_TEST_SEAMS:-}" = 1 ] || P0CHILD+=(-u R67_T1_S -u R67_N -u R67_BUDGET_S -u R67_SIM)' in pre
      and 'R67_TGT="$_tgt"' in pre and 'TGT="${R67_TGT:-$ROOT}"' in rd("repeat67_run.sh"))
check("run_suites is parameterised (SUITES_REPO / SUITES_TT, SD; SUITES_NO_TOOLS skips the tools tests) with the old defaults; src_manifest follows SUITES_REPO",
      'REPO="${SUITES_REPO:-$(cd "$HERE/../.." && pwd)}"' in runall and 'TT="${SUITES_TT:-$ROOT/target-tests}"' in runall and "SUITES_NO_TOOLS:-0" in runall and 'cd "${SUITES_REPO:-$(cd "$HERE/../.." && pwd)}"' in runall)
check("lib_battery.sh: take_lock inherits a lock held by the live LB_LOCK_PARENT (LOCK-INHERITED, returns before taking, so release_locks never releases it); every other lock is taken or refused as before",
      'LB_LOCK_PARENT' in lib and 'LOCK-INHERITED $p from $LB_LOCK_PARENT' in lib and 'kill -0 "$LB_LOCK_PARENT"' in lib and lib.index("LOCK-INHERITED") < lib.index("command -v flock >/dev/null") and "LOCKS_TAKEN=\"$LOCKS_TAKEN $p\"" in lib)

if __name__ == "__main__":
    print("test_harness67: %d checks, %d failed" % (CHECKS, fails))
    sys.exit(1 if fails else 0)
