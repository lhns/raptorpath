"""The launch paths of the candidate plans that test_stubrun.py EXECUTES end to end (loopdiag_run_all.sh -> loopdiag_battery.sh -> the row parser and
scorer of the plan, against stubbed cargo / sudo / measurement driver). Plan -> path names; the test runs every entry and fails when an entry
was not executed or an executed path is not listed, so this table cannot drift from what is tested.

  rehearse               LD_REHEARSE=1, LD_BASE_SHA set: builds + smoke + smoke-check, then LD-ALL-REHEARSAL-DONE (no suites, no battery)
  full                   LD_BASE_SHA set, suites run: the whole session to LD-ALL-DONE (plan 47: guard then path-event test)
  prior-tests            LD_PRIOR_TESTS = a green suite run of byte-identical raptorpath/src+tests (rehearsal): reused, no cargo test
  prior-tests-mismatch   the same with one byte of raptorpath/src changed: ABORT-TESTS, nothing measured
  reuse-hit              plans 56, 57a, 57b only: LD_REF_MANIFEST = the sandbox tree's own manifest of raptorpath/src + raptorpath/tests -> the engine suites are SKIPPED (no cargo test), the tools tests
                         (python and bash) still run, the full session completes
  reuse-miss             plans 56, 57a, 57b only: the same with one byte of the reference manifest changed -> the full engine suites run (cargo test x4) as plan 54 did
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
}
