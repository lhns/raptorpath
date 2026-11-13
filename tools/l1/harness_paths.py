"""The launch paths of the candidate plans that test_stubrun.py EXECUTES end to end (loopdiag_run_all.sh -> loopdiag_battery.sh -> the row parser and
scorer of the plan, against stubbed cargo / sudo / measurement driver). Plan -> path names; the test runs every entry and fails when an entry
was not executed or an executed path is not listed, so this table cannot drift from what is tested.

  rehearse               LD_REHEARSE=1, LD_BASE_SHA set: builds + smoke + smoke-check, then LD-ALL-REHEARSAL-DONE (no suites, no battery)
  full                   LD_BASE_SHA set, suites run: the whole session to LD-ALL-DONE (plan 47: guard then path-event test)
  prior-tests            LD_PRIOR_TESTS = a green suite run of byte-identical raptorpath/src+tests (rehearsal): reused, no cargo test
  prior-tests-mismatch   the same with one byte of raptorpath/src changed: ABORT-TESTS, nothing measured
  historical-base        LD_BASE_SHA unset (BASE = main 4926822): rehearsal passes, the row parser sees an empty LD_BASE_SHA (plans 41c / 46 / 47 only:
                         plans 48 and 52 require LD_BASE_SHA; plan 52's BASE must carry the RWM_PLACE_WDIV echo)
Plan 41d (the merge guard v2, status 41d) is plan 48's shape: suites, then the one guard stage (c2 / c8 rows at 600 MB, score41d); LD_BASE_SHA is required. Plan 52's extra path
guard-v2 is its rehearsal with LD_GUARD_SET=v2 (the guard stage swapped for plan 41d's).
Plan 52 (the c8 recovery levers) is plan 47's shape: suites, the guard (41c), then the reported block (rep52: smoke = the three RWM_PLACE_WDIV arms, then 30 rows).
Plans 24..43b, 41, 41b are earlier sessions and are not covered here."""

HARNESS_PATHS = {
    "41c": ("rehearse", "full", "prior-tests", "prior-tests-mismatch", "historical-base"),
    "46": ("rehearse", "full", "prior-tests", "prior-tests-mismatch", "historical-base"),
    "47": ("rehearse", "full", "prior-tests", "prior-tests-mismatch", "historical-base"),
    "48": ("rehearse", "full", "prior-tests", "prior-tests-mismatch"),
    "52": ("rehearse", "full", "prior-tests", "prior-tests-mismatch", "guard-v2"),
    "41d": ("rehearse", "full", "prior-tests", "prior-tests-mismatch"),
}

# The adversarial cells (adv_cells.sh, ADR-0068 W5.3): scenario name (what perf_rwm_c.sh routes to adv_cells.sh instead of topo_dual.sh) ->
# the loopdiag_battery.sh cell name that selects it. These are CELLS, not plans: they ride the existing battery rows (run_one ->
# perf_rwm_c.sh), so no loopdiag_run_all.sh path is added above. The cell code path (perf_rwm_c.sh's topology routing) is NOT executed by
# test_stubrun.py, whose driver stub replaces perf_rwm_c.sh; test_adv_cells.py executes that routing against a stub `ip`.
# Asserted equal to lib.sh ADV_CELLS, loopdiag_battery.sh cell_spec, loopdiag_parse.ADV_CELL_SPEC and adv_cells_validate.CELLS.
ADV_CELLS = {
    "c2ctl": "c2ctl-100",
    "jit0": "jit0-100",
    "jit5": "jit5-100",
    "jit15": "jit15-100",
    "jit25": "jit25-100",
    "shal8": "shal8-50",
    "pol100": "pol100-25",
}
