"""The launch paths of the candidate plans that test_stubrun.py EXECUTES end to end (loopdiag_run_all.sh -> loopdiag_battery.sh -> the row parser and
scorer of the plan, against stubbed cargo / sudo / measurement driver). Plan -> path names; the test runs every entry and fails when an entry
was not executed or an executed path is not listed, so this table cannot drift from what is tested.

  rehearse               LD_REHEARSE=1, LD_BASE_SHA set: builds + smoke + smoke-check, then LD-ALL-REHEARSAL-DONE (no suites, no battery)
  full                   LD_BASE_SHA set, suites run: the whole session to LD-ALL-DONE (plan 47: guard then path-event test)
  prior-tests            LD_PRIOR_TESTS = a green suite run of byte-identical raptorpath/src+tests (rehearsal): reused, no cargo test
  prior-tests-mismatch   the same with one byte of raptorpath/src changed: ABORT-TESTS, nothing measured
  reuse-hit              plans 56, 57a, 57b, 59 only: LD_REF_MANIFEST = the sandbox tree's own manifest of raptorpath/src + raptorpath/tests -> the engine suites are SKIPPED (no cargo test), the tools tests
                         (python and bash) still run, the full session completes
  reuse-miss             plans 56, 57a, 57b, 59 only: the same with one byte of the reference manifest changed -> the full engine suites run (cargo test x4) as plan 54 did
  historical-base        LD_BASE_SHA unset (BASE = main 4926822): rehearsal passes, the row parser sees an empty LD_BASE_SHA (plans 41c / 46 / 47 only:
                         plans 48, 52, 53, 54 and 54b require LD_BASE_SHA; plan 52's BASE must carry the RWM_PLACE_WDIV echo)
Plan 41d (the merge guard v2, status 41d) is plan 48's shape: suites, then the one guard stage (c2 / c8 rows at 600 MB, score41d); LD_BASE_SHA is required. Plan 52's extra path
guard-v2 is its rehearsal with LD_GUARD_SET=v2 (the guard stage swapped for plan 41d's).
Plan 52 (the c8 recovery levers) is plan 47's shape: suites, the guard (41c), then the reported block (rep52: smoke = the three RWM_PLACE_WDIV arms, then 30 rows).
Plan 54 (the snapshot-order law's guard) is plan 41d with the three liveness witnesses (LD_WITNESS54=1); LD_BASE_SHA is required. Plan 54b (the store-budget block) is plan 53's shape: suites, then ONE reported stage rep54b (smoke = one row per arm with the rehearsal criteria, then 36 rows).
Plan 56 (the store-pool sweep, reported only) is ONE seat: suites_stage56 (the reference-manifest reuse above instead of LD_PRIOR_TESTS), then ONE reported stage rep56 (smoke = 4 rows with the
rehearsal criteria, then 70 slots); it has no LD_BASE_SHA, no BASE tree and no prior-tests paths. Its "full" path runs against the committed reference manifest, which the stub's fake tree
does not match (a miss: the suites run).
Plans 57a (the emission-burst block at c1s) and 57b (the c1d pool sweep) are plan 56's shape: ONE seat, suites_stage56 (the same reference manifest), then ONE reported stage (rep57a: smoke = the
B256-512 row, which must read a realised mean burst > 64, then 63 slots; rep57b: smoke = pools 192 and 2048, then 30 slots); no LD_BASE_SHA, no prior-tests paths. reuse-hit uses a manifest of
the sandbox's own tree; reuse-miss and full (the committed manifest does not match the stub's fake tree) run the full engine suites.
Plan 59 (the evicting-seat m-bar readout, reported only) is plan 57b's shape: ONE seat, suites_stage56 (the reference manifest), then ONE reported stage rep59 (smoke = one c2-100 EVICT row, then 5 slots);
no LD_BASE_SHA, no prior-tests paths. The stub driver replaces perf_rwm_c.sh, so its RWM_C_RELIABLE switch is executed by test_mbar59.py instead.
Plans 60a / 60b (the store law's gauge session, status 60) are plan 59's single-seat shape with a STRICTER suite rule: engine suites are reused ONLY by a reference-manifest HIT (60-pre's ref60-manifest.sha256); a MISS stops the launch
(ABORT-SUITES, nothing run, a rehearsal included), so "full" does not exist for them (it would be reuse-hit) and the paths are:
  rehearse               LD_REHEARSE=1 on a HIT: 60a runs the 10 registered rehearsal rows (both smokes + 6 floor-pool rows) and the go/no-go; 60b its 2-row smoke; no suites, no battery
  reuse-hit              the manifest of the sandbox's own tree and the registered bin_sha: the whole session to LD-ALL-DONE (60a: 96 slots; 60b: 45 slots incl. the 5 evicting rows), tools tests run, engine suites skipped
  reuse-miss             one byte of the manifest changed: ABORT-SUITES plan 60 suite-reuse MISS, rc 5, no row measured
  bin-sha-mismatch       LD_BIN_SHA_EXPECT differs from the built binary: ABORT-BIN-SHA, rc 5, no row measured
  slaw-echo-void         (60a) the first scored slot echoes RWM_SLAW wrongly on the client end: VOID (gate-slaw), re-run once, the replacement LIVE
  nogo-rehearse          (60a) the stub starves c8 leg 0: the rehearsal's go/no-go reads NO-GO for c8 (a printed decision, the rehearsal still passes)
  rsign-abort            (60b) 60a's score (LD_SCORE60A) carries R-SIGN-c2 STOP: ABORT-RSIGN, rc 3, nothing run (60b's non-rehearsal launch reads the gate before any row)
  prebuilt               (60b) LD_BIN_PREBUILT = 60a's binary: no build, the sha still checked against LD_BIN_SHA_EXPECT
Plan 51e (status 51 block E: the evicting seat with the spare-clamp gauge, paced; docs/derivations/s51-evict-design.md) is plan 60's single-seat shape with its STRICT suite rule (a reference-manifest MISS aborts, a rehearsal included) and a registered
bin_sha with no default (E-pre's, LD_BIN_SHA_EXPECT); one reported stage rep51e (smoke = one PM row; the rehearsal = PL, PM, U with the section 10 go/no-go), 12 slots, and the paths:
  rehearse               LD_REHEARSE=1 on a HIT: the 3 registered rehearsal rows and the go/no-go (a)-(h); no suites, no battery
  reuse-hit              the whole session to LD-ALL-DONE (12 slots), tools tests run, engine suites skipped, the score with the known absolute gauge readings
  reuse-miss             one byte of the manifest changed: ABORT-SUITES plan 51e suite-reuse MISS, rc 5, nothing measured
  bin-sha-mismatch       LD_BIN_SHA_EXPECT differs from the built binary: ABORT-BIN-SHA, rc 5, nothing measured
  pace-void              the first scored slot (PL) prints no pacing echo: VOID (pace-echo-missing), re-run once, the replacement LIVE
  spc-void               the first scored slot has fec n != an: VOID (spc-an-ne-fecn), re-run once, the replacement LIVE
  calibrate              (rehearsal) PL A* = 12: the calibration rule fires and prints the new rate, lambda and bytes; the rehearsal still passes
  recheck                LD_REHEARSE=1 LD_ONLY=PL: the PL-only recheck row, (h) gating
Plan 51 (status 51 block 1: the off-Bulk baseline and the delta sweep; docs/derivations/s51-design.md) is main at launch: LD_BASE_SHA pins the commit, no engine suites ever, the dial pins run on the built tree (ABORT-DIALPINS), the rehearsal RECORDS the
bin_sha and a battery launch must equal it (LD_BIN_SHA_EXPECT), one cell per launch (LD_CELL), a block is started only with rows x 215 s left; one reported stage rep51 (smoke = H-B, H-R, B+, N-A, the rehearsal runs it), 76 slots, and the paths:
  rehearse               LD_REHEARSE=1: the pins, the bin_sha recorded, the 4 smoke rows with the not-overdriven witness; no battery
  full                   the whole session to LD-ALL-DONE (76 slots, six blocks), score51 (T1 / T2, A/A, W-ROUTE, the delta table)
  bin-sha-mismatch       LD_BIN_SHA_EXPECT differs from the built binary: ABORT-BIN-SHA, rc 5, nothing measured
  delta-echo-void        the first scored slot echoes RWM_DELTA wrongly on the client end: VOID (gate-delta), re-run once
  short-void             the first scored slot is a 10 s row: VOID-SHORT, re-run once
  dialpins-fail          a dial pin FAILS: ABORT-DIALPINS, rc 5, nothing measured
Plan 63 (status 63, F2's first VM contact; plan63.py) is a TWO-seat reported-only plan on the single-seat gate shape: CONTROL (the e-tree binary, pinned to the registered E bin_sha, LD_BIN_CONTROL or the BASE tree's build)
then CAND (LD_BIN_SHA_CAND, REQUIRED on a launch; the rehearsal records it) per row, 10 slots, no engine suites, rows run once. The F2 counters are readings there, not a VOID. Paths:
  rehearse               LD_REHEARSE=1 without LD_BIN_SHA_CAND (it records it): both seats' binaries checked, the one CAND PL s42 row, REHEARSAL-PASS
  full                   the whole session to LD-ALL-DONE: smoke row + 10 slots, both pins checked, the report's labels against the draft's predictions
  cand-sha-missing       a launch without LD_BIN_SHA_CAND: ABORT-BIN-SHA, rc 3, nothing run
  cand-sha-mismatch      LD_BIN_SHA_CAND differs from the built CAND: ABORT-BIN-SHA plan 63 CAND, rc 5, no row measured
  control-sha-mismatch   the CONTROL binary differs from its pin: ABORT-BIN-SHA plan 63 CONTROL, rc 5, no row measured
  cand-fail              (STUB_BAD63=candfail) the CAND's paced rows fail closed: the report labels them FAIL and prints BLOCKS-E-PRE; no row is voided
Plan 53 (stale gap reports under lever B) has no guard: suites, then ONE reported stage rep53 (smoke = one row of each arm per block with the rehearsal criteria, then 60 rows).
Plans 24..43b, 41, 41b are earlier sessions and are not covered here."""

HARNESS_PATHS = {
    "41c": ("rehearse", "full", "prior-tests", "prior-tests-mismatch", "historical-base"),
    "46": ("rehearse", "full", "prior-tests", "prior-tests-mismatch", "historical-base"),
    "47": ("rehearse", "full", "prior-tests", "prior-tests-mismatch", "historical-base"),
    "48": ("rehearse", "full", "prior-tests", "prior-tests-mismatch"),
    "52": ("rehearse", "full", "prior-tests", "prior-tests-mismatch", "guard-v2"),
    "41d": ("rehearse", "full", "prior-tests", "prior-tests-mismatch"),
    "53": ("rehearse", "full", "prior-tests", "prior-tests-mismatch"),
    "54": ("rehearse", "full", "prior-tests", "prior-tests-mismatch"),
    "54b": ("rehearse", "full", "prior-tests", "prior-tests-mismatch"),
    "56": ("rehearse", "full", "reuse-hit", "reuse-miss"),
    "57a": ("rehearse", "full", "reuse-hit", "reuse-miss"),
    "57b": ("rehearse", "full", "reuse-hit", "reuse-miss"),
    "59": ("rehearse", "full", "reuse-hit", "reuse-miss"),
    "60a": ("rehearse", "reuse-hit", "reuse-miss", "bin-sha-mismatch", "slaw-echo-void", "nogo-rehearse"),
    "60b": ("rehearse", "reuse-hit", "reuse-miss", "bin-sha-mismatch", "prebuilt", "rsign-abort"),
    "51e": ("rehearse", "reuse-hit", "reuse-miss", "bin-sha-mismatch", "pace-void", "spc-void", "calibrate", "recheck"),
    "51": ("rehearse", "full", "bin-sha-mismatch", "delta-echo-void", "short-void", "dialpins-fail"),
    "63": ("rehearse", "full", "cand-sha-missing", "cand-sha-mismatch", "control-sha-mismatch", "cand-fail"),
}
