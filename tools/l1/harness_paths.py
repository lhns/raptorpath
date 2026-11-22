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
  rehearse               LD_REHEARSE=1 without LD_BIN_SHA_CAND (it records it): both seats' binaries checked, then CONTROL PL s42 and CAND PL s42 (both reported, never re-run), REHEARSAL-PASS
  full                   the whole session to LD-ALL-DONE: smoke row + 10 slots, both pins checked, the report's labels against the draft's predictions
  cand-sha-missing       a launch without LD_BIN_SHA_CAND: ABORT-BIN-SHA, rc 3, nothing run
  cand-sha-mismatch      LD_BIN_SHA_CAND differs from the built CAND: ABORT-BIN-SHA plan 63 CAND, rc 5, no row measured
  control-sha-mismatch   the CONTROL binary differs from its pin: ABORT-BIN-SHA plan 63 CONTROL, rc 5, no row measured
  cand-fail              (STUB_BAD63=candfail) the CAND's paced rows fail closed: the report labels them FAIL and prints BLOCKS-E-PRE; no row is voided
  cand-noctld            (STUB_BAD63=noctld) the CAND prints no [CTLD] line: a missing witness, labelled FAIL (never a pass on undeliv=0 by absence)
  cand-ctld-send-only    (STUB_BAD63=ctldsend) only the sending end (the client) prints [CTLD]: the receiving server's witness is missing, labelled FAIL
  cand-prebuilt          LD_BIN_PREBUILT set: ABORT-BUILD, rc 3, nothing run (the CAND is a plain release build from the launch archive, never a prebuilt one)
Plan 53 (stale gap reports under lever B) has no guard: suites, then ONE reported stage rep53 (smoke = one row of each arm per block with the rehearsal criteria, then 60 rows).
Plan 67 (status 67, VM batch 1: docs/derivations/vm-batch-1-registration.md) has SEVEN seats and four session tokens, each a plan of this table: 67pre is SESSION 0 (the pre-flight, no battery, no build), 67 is the REHEARSAL (LD_REHEARSE=1: seat builds, identity checks, tools tests, one smoke per stage,
no battery), 67g the Bulk guards (G1: S1 against B0, G2: S2 against S1; plan 41d twice), 67x the stages off Bulk and the tunnel (A, R1, T, R2 with the test-seams twins, the dump twins, the pool block). The launches run on the rehearsal's
binaries (LD_BIN_PREBUILT_<SEAT> + LD_BIN_SHA_<SEAT>) and tools result (LD_PRIOR_TOOLS). Plan 67 paths (rehearsal; every one a session, fast mode). Plan 67pre (session 0, loopdiag_p67pre.sh; R67_N = 3 on the stub; its paths first):
  full                   rc 0, LD-ALL-SESSION0-DONE: cargo check on S1 and S2, the gates reference on B0, the suites on S1 then S2 (each its own tree and fresh target, no tools tests), the repeats (REPEAT67 lines, locks inherited), the visualizer and
                         the golden test on S2 only, binid on B0 / S1 / S2 (BINID67 PASS), REF-MANIFEST-S1 / S2 + SESSION0.txt; no release build, no driver call, no ledger; the manifests fed into a plan-67 rehearsal give suite-reuse HIT on S1 and S2
  check-fail-s1          cargo check fails on S1: ABORT-PREFLIGHT seat S1, rc 5, nothing after it ran
  check-fail-s2          cargo check fails on S2: ABORT-PREFLIGHT seat S2 naming guard 2's stop and LD_DROP_S2=1
  gates-no-result        the gates reference on B0 has no `test result` line: ABORT-PREFLIGHT seat B0 (the reference did not run), nothing after it
  gates-mismatch         a gates failure on S1's suites that B0 does not have: ABORT-PREFLIGHT seat S1 (gates failure not equal on B0), S2's suites never run
  gates-equal-b0         the same gates failure on B0 and S1: GATES-B0-FINDING and GATES-FINDING-ON-MAIN seat=S1, the session completes
  gates-and-flake        both a gates failure absent on B0 and a non-quarantined FLAKE on S1: both lines printed, the abort is ABORT-PREFLIGHT
  flake-not-quarantined  a non-gates test fails once on S1's suite and passes its solo re-run, not on quarantine.txt: ABORT-TESTS seat S1
  repeat-one-failure     one failing run of S2's gating tests: ABORT-REPEAT seat S2 (guard 2's stop), S1 read zero failures; REF-MANIFEST-S1 is already in the persisted run (written when S1's own steps passed), REF-MANIFEST-S2 is not
  visualizer-fail        build_visualizer.sh fails on S2: ABORT-PREFLIGHT seat S2 (visualizer), no golden, no binid
  golden-fail            the native golden test fails on S2: ABORT-PREFLIGHT seat S2 (golden), no binid
  s2-dropped             LD_DROP_S2=1: no S2 step, binid on B0 and S1, REF-MANIFEST-S1 only, LD-ALL-SESSION0-DONE
  budget                 less than the registered worst case (17 040 s) before LD_HARD: ABORT-BUDGET, rc 3, nothing run
  budget-step            under 600 s left when a step would start: ABORT-BUDGET plan 67pre step check-S1, nothing ran
  lock-inherit-fail      (STUB_LOCK_PARENT, under LD_TEST_SEAMS=1) the step-0 child sees another parent pid and cannot inherit the locks: ABORT-PREFLIGHT plan 67pre step locks, nothing ran after it
  binid-test-mismatch    S1's math + wasm tests (BINTEST) fail a test that B0 passes: ABORT-PREFLIGHT binid: BINTEST S1 <names> not equal on B0 (guard 1's seat), the manifests of the passed steps are kept
  binid-test-equal-b0    the same test fails on B0 and on S1: BINTEST-FINDING-ON-MAIN on record, the session completes
  seam-r67-n             R67_N set without LD_TEST_SEAMS=1: ABORT-SEAM (a session-0 seam), rc 3, nothing ran
  rehearse-refused       LD_REHEARSE=1 with LD_PLAN=67pre: ABORT-REHEARSE (not in run_all's REHEARSE list), nothing run
Plan 67 (the rehearsal, session 1):
  rehearse               the 7 seat builds (the twins --features test-seams), ABORT-SEAT identity strings (S1 and S2 hold pfec=), the tools-diff witness, the suite-reuse HIT of S1 and S2 (manifest incl. tools/ci/flake_rule.py), the tools tests,
                         one smoke per stage (g1, g2, a, r1, t, r2), the twins' [ACCT] / [ACCT-RX] lines in the kept ledger, AMENDMENT2.txt; LD-ALL-REHEARSAL-DONE
  not-rehearsal          LD_PLAN=67 without LD_REHEARSE=1: ABORT-REHEARSE, nothing run
  seat-s1-no-pfec        S1's binary has no `pfec=`: ABORT-SEAT (the c8-gauges-pfec string is part of the identity)
  seat-b0-has-gauge      B0's binary carries `rlaw=`: ABORT-SEAT (B0 is main's)
  seat-commit-mismatch   S1's tree COMMIT is not the pin: ABORT-SEAT before any build
  pin-missing-b0         LD_PIN_B0 unset (every seat's pin is required, none has a default): ABORT-SEAT
  pin-missing-s1         LD_PIN_S1 unset: ABORT-SEAT (the S1 / S2 pins have no registered default either)
  s2-seat-abort          S2's binary has no `pslot=`: ABORT-SEAT naming guard 2's stop and LD_DROP_S2=1
  s2-dropped             the same binary under LD_DROP_S2=1: S2, P2 and P2T never built or looked at, guard 1 and the other stages rehearse
  tools-diff             a seat's tools tree differs from the harness's: ABORT-SEAT (tools-diff witness)
  manifest-miss          one byte of S1's engine source differs from session 0's manifest: ABORT-SUITES
  flake-rule-edit        one byte of S1's tools/ci/flake_rule.py differs: ABORT-SUITES (the manifest covers the rule)
  ref-missing            no reference manifest for S1: ABORT-SUITES
  rehearsal-r2-smoke-fail a twin smoke row has no [ACCT] line: ABORT-SMOKE of stage r2 makes R2 VOID (R2-VOID marker); the other smokes still run, AMENDMENT2.txt says `R2 VOID`, the session ends LD-ALL-REHEARSAL-R2-VOID (rc 7, not FAILED-ALL): guard 1 may launch
  dump-needed            the [SUCC] line lacks the close times: DUMP-TWINS-NEEDED, AMENDMENT2 says DUMP67=1, the rehearsal passes
  t-base-refuses         B0's `run` refuses with the guard's text: ABORT-SEAT in the T ledger, ABORT-SMOKE of stage t
  s2-taper-nonzero       S2's guard-2 smoke row prints taper > 0 (S06-FAIL): ABORT-SMOKE, a finding is never a pass
Plan 67g (the Bulk guards; a launch on the rehearsal's artifacts):
  full                   G1 then G2 to LD-ALL-DONE; no build, no cargo test, TOOLS-TESTS-REUSED, GATE-67G1 / GATE-67G2 PASS
  g1-fail                S1 is slower than B0: GATE-67G1 FAIL and G2 still runs (guard 2's base is S1)
  s1-no-gauge            an S1 row of G1 (past block 1) lacks the gauge prints: WITNESS-FAIL, replaced once
  s2-taper-nonzero       an S2 row of G2 (past block 1) prints taper > 0 with the mechanism preconditions holding (pslot > 0, [SPAN] median rr > 0): S06-FAIL, never replaced, GATE-67G2 FAIL, G1 unaffected
  g2-taper-replaced      an S2 row of G2 prints taper > 0 but no pslot= (a seat-witness failure, so not S06-FAIL): it is replaced once by a clean row, and GATE-67G2 still FAILs (G2-TAPER-NONZERO: a replacement never clears it)
  g-block1-fail          S1's first G1 row lacks the gauge prints: the block-1 rule (the first row of each of plan 41c's first 8 (cell, arm) pairs) stops G1 (exit 6, FAILED-g1), G2 still runs
  seam-without-marker    LD_HARD_MIN_S set without LD_TEST_SEAMS=1: ABORT-SEAM before the locks, nothing run
  prebuilt-sha-mismatch  LD_BIN_SHA_S1 differs from the prebuilt S1 binary: ABORT-BUILD
  prebuilt-missing       no prebuilt for S2: ABORT-BUILD (a launch never builds)
  budget                 less than the registered worst case before LD_HARD: ABORT-BUDGET
Plan 67x (the stages off Bulk, the tunnel and the pool block):
  full                   A, R1, T, R2, the pool block to LD-ALL-DONE; AS, A1, A1-R, R1, T PASS, R2-HOLD not fired
  dump-twins             LD_DUMP67=1: the dump twins run before the pool block
  s2-taper-nonzero       an S2 row of stage A prints taper > 0: S06-FAIL, not replaced, AS cannot pass
  s2-taper-cotenant      the same row is also a crash void (rc 1): still S06-FAIL, not replaced (a finding is never a void)
  s2-no-pslot            an S2 row of stage A has no pslot: VOID-WITNESS, replaced once
  s2-belowfloor-high     S2's below-floor count is far above S1's and fill_coded: a hold flag, rows LIVE, no FAIL
  a-under-min            S1 rows of a cell crash (VOID-RC: never replaced, not counted) and fall under the minimum of LIVE rows: UNSCOREABLE for AS and A1 (the old count of LIVE + crash rows would have scored)
  block1-fail            S1's first A row is void: FAILED-sa, the NEXT stage still runs
  r1-blind               S1 prints taper = 0 on most R1 rows: R1-BLIND, reported
  twin-no-final          the twins' sender line has no final=1: the last periodic [ACCT] line is read and labelled ACCT-LAST
  twin-under-min         P2T has fewer live rows than the minimum: R2-HOLD UNSCOREABLE, guard 2 not eligible
  twin-worse             P2T's unresolved blockers sit above P1T's: R2-HOLD fires
  r1-b0-gauge            R1's B0 rows carry gauge prints: LIVE, not replaced, A1-R FAIL
  r2-blind               P1 taper = 0 and P2 pslot = 0 on every scored R2 row: R2-BLIND, R2-HOLD UNSCOREABLE (R2 VOID)
  twin-missing-field     P2T's [ACCT] line lacks oldest_unchanged_max: R2-HOLD UNSCOREABLE, never CLEAR
  t-refusal-missing      an S1 command does not refuse: T FAIL
  t-refusal-and-void     an S1 refusal failure together with B0's key reading false: T FAIL (a finding before a void)
  t-bringup-fail         tun_bulk.sh's TUN-BRINGUP-FAIL for B0 at realtime: ABORT-BRINGUP-T seat=B0 hint=realtime, no T verdict, stage not ok
  truncated              the R1 battery returns timeout's rc 124 (a stub timeout): LD-BATTERY-TRUNCATED, TRUNCATED marker, A1-R / R1 TRUNCATED, stage not ok
  truncated-s06-fail     the A battery is cut (stub timeout rc 124) and an S2 row is S06-FAIL: the verdict line keeps `S06-FAIL rows 1` (never TRUNCATED), the stage returns the cut, the session ends TRUNCATED-ALL (rc 7); the era line and $RUN/score.txt list the non-pass line
  truncated-candidate-crash  the A battery is cut and S2 has crash rows in c8: AS UNSCOREABLE (CANDIDATE-CRASH S2 n=3 ...) stays on the verdict line and in the era line / $RUN/score.txt; TRUNCATED-ALL (rc 7)
  a-crash-partial        S2 in c8 has one crash and every other slot void (live + crashes under the minimum): AS UNSCOREABLE (CANDIDATE-CRASH S2 n=1 in c8-100: ... (live + crashes 1 < minimum 2: other voids also short))
  b0-run-key-false       B0's resolved configuration reads the key false: T VOID, not a pass
  g2-fail                LD_G2_FAIL67=1 (so LD_DROP_S2=1): the S2 arms, R2, the pool block and the dump twins are dropped; A1, A1-R and T still run; R1's ledger carries `LD-PLAN67 ... drop_s2=1` and the pulled ledger re-scored on the host with LD_DROP_S2 unset reads A1-R PASS (not INCOMPLETE)
  cut-pool               LD_CUT67=pool: ABORT-CUT (only the dump twins may be cut)
  budget                 less than the registered worst case before LD_HARD: ABORT-BUDGET
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
    "51e": ("rehearse", "reuse-hit", "reuse-miss", "bin-sha-mismatch", "pace-void", "spc-void", "calibrate", "recheck"),
    "51": ("rehearse", "full", "bin-sha-mismatch", "delta-echo-void", "short-void", "dialpins-fail"),
    "63": ("rehearse", "full", "cand-sha-missing", "cand-sha-mismatch", "control-sha-mismatch", "cand-fail", "cand-noctld", "cand-ctld-send-only", "cand-prebuilt"),
    "67pre": ("full", "check-fail-s1", "check-fail-s2", "gates-no-result", "gates-mismatch", "gates-equal-b0", "gates-and-flake", "flake-not-quarantined", "repeat-one-failure", "visualizer-fail", "golden-fail", "s2-dropped", "budget", "budget-step", "rehearse-refused", "lock-inherit-fail", "binid-test-mismatch", "binid-test-equal-b0", "seam-r67-n"),
    "67": ("rehearse", "not-rehearsal", "seat-s1-no-pfec", "seat-b0-has-gauge", "seat-commit-mismatch", "pin-missing-b0", "pin-missing-s1", "s2-seat-abort", "s2-dropped", "tools-diff", "manifest-miss", "flake-rule-edit", "ref-missing",
           "rehearsal-r2-smoke-fail", "dump-needed", "t-base-refuses", "s2-taper-nonzero"),
    "67g": ("full", "g1-fail", "s1-no-gauge", "s2-taper-nonzero", "g2-taper-replaced", "g-block1-fail", "seam-without-marker", "prebuilt-sha-mismatch", "prebuilt-missing", "budget"),
    "67x": ("full", "dump-twins", "s2-taper-nonzero", "s2-taper-cotenant", "s2-no-pslot", "s2-belowfloor-high", "a-under-min", "block1-fail", "r1-blind", "r1-b0-gauge", "r2-blind", "twin-no-final", "twin-under-min", "twin-worse", "twin-missing-field",
            "t-refusal-missing", "t-refusal-and-void", "t-bringup-fail", "b0-run-key-false", "g2-fail", "cut-pool", "truncated", "truncated-s06-fail", "truncated-candidate-crash", "a-crash-partial", "budget"),
}
