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
Plan 64 (status 64, F2's merge guard; docs/derivations/f2-guard-registration.md) is plan 41d's stage G with the F2 row witnesses (LD_WITNESS64=1; LD_PLAN=64 is rewritten to 41d, as plan 54 is) followed by stage R (Realtime c2-100 at
rho = 1, arms RB / RC, $RUN/rt64); LD_BASE_SHA is required. Its suite reuse is the reference manifest over the WIDER src_manifest() set (a MISS aborts, a rehearsal included), its binaries LD_BIN_PREBUILT_BASE / _CAND (sha-checked
against LD_BIN_SHA_BASE / _CAND) and its tools tests LD_PRIOR_TOOLS (the rehearsal's run dir, on a byte-identical tools/l1). No prior-tests paths. Every path is a session (fast mode: stage G 16 rows + 3 smoke, stage R 4 rows + 2 smoke):
  rehearse                 LD_REHEARSE=1 on a HIT: both builds (shas recorded), the seat witness, stage G's 3 smoke rows and stage R's RB / RC rehearsal rows with their criteria, the tools tests (TOOLS-TESTS-OK + TOOLS-MANIFEST.sha256), framing_ns x 10 with its label; no battery
  full                     the launch shape: a rehearsal's run dir feeds LD_BIN_PREBUILT_BASE / _CAND (sha-checked against its BINSHA.txt) and LD_PRIOR_TOOLS: no build, no cargo test, suite-reuse HIT, TOOLS-TESTS-REUSED, both stages to LD-ALL-DONE, GATE-64 PASS, RT-PASS
  cand-bulk-unframable     (STUB_BAD64) one CAND row prints [UNFRAMABLE]: F2-FAIL, never replaced, not counted toward NO VERDICT, GATE-64 FAIL with the 41d p still > 0.05
  cand-bulk-dnf-unframable the same on a CAND row that is a DNF: F2-FAIL, not DNF
  cand-bulk-symsz-missing  one CAND row has no [SYMSZ] on the server: F2-FAIL (a missing witness on an otherwise LIVE row)
  cand-bulk-ctld-send-only one CAND row's [CTLD] is on the sending end only (the receiving server has none): F2-FAIL
  base-has-symsz           the BASE binary carries [SYMSZ]: ABORT-SEAT, rc 5, no row measured
  completed-row-live       a completed row is LIVE (rc 0) on each seat under the real perf script's F2 lines run under `set -euo pipefail` (the section 63 VOID-RC defect), stage G and stage R
  base-rt-dead             an RB row's warm-up never acks (the driver exits 1): BASE-F2-DEAD (not replaced, no NO VERDICT, not VOID-RC), RT-PASS stands
  base-rt-dead-rc0         the same with the driver exiting 0 and no summary line: the row is NOT drawn again (one attempt: TRIES=1 at stage R), BASE-F2-DEAD
  rc-nodata-replaced       an RC row ends rc 0 with no run / summary line (NO_DATA, warm-up acked): a non-F2 void, replaced ONCE (not drawn again by the bring-up retry), the replacement RT-OK, RT-PASS
  rt-rho-one-end           an RC row's server prints window_reliable: false: VOID-RHO, replaced once
  cand-rt-dnf-base-completes  an RC row is a DNF while every live RB row completes: RT-FAIL 5
  cand-rt-dnf-base-dnf        the same DNF while an RB row is a DNF too: item 5 REPORTED, RT-PASS
  cand-rt-ctld-send-only   an RC row's [CTLD] is on the sending end only: RT-FAIL 2
  cand-rt-symsz-1200       an RC row's [SYMSZ] reads 1200: RT-FAIL 4
  manifest-miss            one byte of raptorpath/src differs from the reference manifest: ABORT-SUITES plan 64 suite-reuse MISS, rc 5, nothing run
  manifest-miss-cargo-toml-only  only raptorpath/Cargo.toml differs (outside the narrower engine manifest): ABORT-SUITES
  rc-void-twice            an RC slot voids (VOID-RHO) on its row and on its replacement: RC-VOID, RT-INCOMPLETE, not RT-PASS
  prebuilt-sha-mismatch    LD_BIN_SHA_CAND differs from the prebuilt CAND binary: ABORT-BUILD, rc 5, no row measured
  tools-manifest-differs   LD_PRIOR_TOOLS names a rehearsal whose tools/l1 manifest differs from the launch's: the tools tests run (no TOOLS-TESTS-REUSED)
Plan 65 (status 65, the exact replication of the Realtime in-order delay lean) is a STAGE-R-ONLY launch on the two prebuilt binaries of status 64 (LD_BIN_PREBUILT_BASE / _CAND, LD_BIN_SHA_BASE / _CAND): no build, no cargo, no suites,
no LD_BASE_SHA; the registered scorer s65_score.py by sha (LD_S65_SCORE_SHA); the tools tests reused through LD_PRIOR_TOOLS. Fast mode: 6 pairs (12 rows), floor 5. Paths:
  rehearse                     LD_REHEARSE=1: binaries installed and pinned, the seat witness, the scorer sha, the tools tests (TOOLS-TESTS-OK + TOOLS-MANIFEST.sha256), the process list twice, the 2-row smoke (RB s42 r1, RC s42 r1), no battery
  rehearse-rb-dead-once        RB s42 r1 dies: the conditional third row (RB s7 r1) runs, is LIVE, the rehearsal passes (a dead BASE row is the old defect at its known rate)
  rehearse-rb-dead-twice       RB s42 r1 and RB s7 r1 both die: SMOKE-FAIL (two BASE-F2-DEAD rows: NO-GO, to the coordinator), rc 5, NO-GO65 on record, nothing launched
  full                         the launch on the rehearsal's run dir (LD_PRIOR_TOOLS): TOOLS-TESTS-REUSED, no cargo at all, the stage to LD-ALL-DONE, NOT-REPLICATED on identical seats, the summary carries VERDICT65 / TEST65 / F2-FINDING65
  replicated                   the CAND rows deliver in order later (STUB_SHIFT65): REPLICATED
  unresolved-n                 two RB slots die on both draws: the dead pairs are not counted, fewer usable pairs than the floor: UNRESOLVED-N
  base-dead-redrawn            the first RB scored row dies and is redrawn once in its slot (BASE-F2-DEAD is replaceable at plan 65): the slot is scored
  base-dead-twice              the first RB scored slot dies on the redraw too: the pair is dropped, the label stands on the rest
  rc-rtfail                    an RC row fails the F2 reading (item 2): F2-FINDING65, never redrawn, its pair dropped
  rho-one-end                  an RC row's server prints window_reliable: false: VOID-RHO, redrawn once, scored
  score-sha-mismatch           LD_S65_SCORE_SHA differs from tools/l1/s65_score.py: ABORT-TOOLS, rc 3, nothing run
  prebuilt-missing             LD_BIN_PREBUILT_BASE unset: ABORT-BUILD, rc 3, nothing run
  prebuilt-sha-mismatch        LD_BIN_SHA_CAND differs from the prebuilt CAND binary: ABORT-BUILD, rc 5, no row measured
Plans 61a / 61b (the c3 sweep, status 61) are plan 60's shape with their own tables (plan61.py) and scorer (s61_score.py): the same strict manifest rule and registered bin_sha, ONE stage rep61a / rep61b. Paths:
  rehearse               (61a) LD_REHEARSE=1 on a HIT: the 6 rehearsal rows R1-R6 (control, floor x2, pilot x2, fallback control), the go/no-go and PILOT61; no suites, no battery
  reuse-hit              the whole session to LD-ALL-DONE (61a: smoke + 66 slots from LD_KNEE61; 61b: smoke + 30 slots), tools tests run, engine suites skipped
  reuse-miss             one byte of the manifest changed: ABORT-SUITES plan 61 suite-reuse MISS, rc 5, no row measured
  bin-sha-mismatch       LD_BIN_SHA_EXPECT differs from the built binary: ABORT-BIN-SHA, rc 5, no row measured
  slaw-echo-void         (61a) the first scored slot echoes RWM_SLAW wrongly on the client end: VOID (gate-slaw), re-run once, the replacement LIVE
  pilot-missing          (61a) a non-rehearsal launch without LD_KNEE61: ABORT-PILOT, rc 5, no row measured
  control-missing        (61a) a launch (the rehearsal included) without LD_CTL61=512: ABORT-CONTROL61, rc 5, no row measured (status 61 Amendment 2: the 61a control is W 512)
  prebuilt               (61b) LD_BIN_PREBUILT = 61a's binary: no build, the sha still checked against LD_BIN_SHA_EXPECT
Plans 62r0 / 62a (c3 at jitter 0, status 62) are plan 61's single-seat shape with their own tables (plan62.py), scorer (s62_score.py), the same strict manifest rule and registered bin_sha, and NO build: LD_BIN_PREBUILT (the staged
binary, its sha checked) is required. The rehearsal IS plan 62r0 (its rows are its smoke; LD_REHEARSE is implied); 62a reads the rehearsal's GO and PILOT62 W= from LD_REHEARSAL62 (the rehearsal's run dir) and reuses its python and bash
tools-test result when tools/l1 is byte-identical (a recorded manifest). One stage rep62r0 / rep62a. Paths:
  62r0 rehearse          the 7 rows R1-R6 + D1 (control x2, floor pair, probes W 80 / 128, the dump row), no battery, no engine suites; `LABEL62 GO-c3j0 GO` and PILOT62; the tools tests ran and their manifest is recorded
  62r0 reprobe           the first probes read both-above: the rehearsal re-runs R4 / R5 ONCE at W 64 / 96 (section 3.3) and goes
  62r0 nogo-floor        a floor row DNFs (re-run once, DNF again): `LABEL62 GO-c3j0 NO-GO-FLOOR`
  62r0 nogo-gate         the floor minimum falls with the sample count: NO-GO-GATE
  62r0 nogo-pilot        both the first and the re-probe pair read both-above: NO-GO-PILOT
  62r0 nogo-control      the controls' loss truth above 4.76 % + 1 pp: NO-GO-CONTROL
  62r0 nogo-short        a row with fewer than 100 native LATE windows: NO-GO-SHORT
  62r0 abort-netem-first the FIRST row's pre read of the live qdisc carries a jitter term: ABORT-NETEM, the launch stops after that row
  62r0 control-missing   LD_CTL62 unset: ABORT-CONTROL62 (both launches), rc 5, no row measured
  62r0 prebuilt-required no LD_BIN_PREBUILT: ABORT-BUILD (plan 62 never builds), no cargo call
  62r0 budget / 62a budget   a launch at the REGISTERED LD_HARD offset (launch + 1.0 h / + 2.5 h) passes the budget guard, one 300 s under the guard's worst case is refused (ABORT-BUDGET)
  62r0 reuse-miss / bin-sha-mismatch   as plan 61 (ABORT-SUITES, ABORT-BIN-SHA), rc 5, no row measured
  62a full               the whole session (smoke + 7 arms x 6 replicates + 2 dump rows from the rehearsal's PILOT62 W=); tools-test reuse HIT (skipped, TOOLS-TESTS-REUSED), scored
  62a tools-reuse-miss   one byte of tools/l1 differs from the rehearsal's: the reuse MISSes, the tools tests run, the launch completes
  62a rehearsal-missing / rehearsal-nogo / knee-mismatch   no LD_REHEARSAL62, a rehearsal without `LABEL62 GO-c3j0 GO`, LD_KNEE62 != the rehearsal's PILOT62 W=: ABORT-REHEARSAL62, no row measured
  62a pilot-missing      LD_KNEE62 unset: ABORT-PILOT, rc 5, no row measured
  62a abort-netem-last   the LAST row's post read (the ACK leg) carries a jitter term: ABORT-NETEM, every row ran, nothing scored (the scorer refuses too)
  62a slaw-echo-void     the first scored slot echoes RWM_SLAW wrongly on the client end: VOID (gate-slaw), re-run once, the replacement LIVE
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
    "65": ("rehearse", "full") + ('rehearse-rb-dead-once', 'rehearse-rb-dead-twice', 'replicated', 'unresolved-n', 'base-dead-redrawn', 'base-dead-twice', 'rc-rtfail', 'rho-one-end', 'score-sha-mismatch', 'prebuilt-missing', 'prebuilt-sha-mismatch'),
    "64": ("rehearse", "full", "cand-bulk-unframable", "cand-bulk-dnf-unframable", "cand-bulk-symsz-missing", "cand-bulk-ctld-send-only", "base-has-symsz", "completed-row-live", "base-rt-dead", "base-rt-dead-rc0", "rc-nodata-replaced", "rt-rho-one-end",
           "cand-rt-dnf-base-completes", "cand-rt-dnf-base-dnf", "cand-rt-ctld-send-only", "cand-rt-symsz-1200", "manifest-miss", "manifest-miss-cargo-toml-only", "rc-void-twice", "prebuilt-sha-mismatch", "tools-manifest-differs"),
    "61a": ("rehearse", "reuse-hit", "reuse-miss", "bin-sha-mismatch", "slaw-echo-void", "pilot-missing", "control-missing"),
    "61b": ("reuse-hit", "reuse-miss", "bin-sha-mismatch", "prebuilt"),
    "62r0": ("rehearse", "reprobe", "nogo-floor", "nogo-gate", "nogo-pilot", "nogo-control", "nogo-short", "abort-netem-first", "control-missing", "prebuilt-required", "reuse-miss", "bin-sha-mismatch", "budget"),
    "62a": ("full", "tools-reuse-miss", "rehearsal-missing", "rehearsal-nogo", "knee-mismatch", "pilot-missing", "abort-netem-last", "slaw-echo-void", "budget"),
}
