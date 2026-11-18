# §65: the exact replication of §64's Realtime in-order delay lean — pre-registration draft

Draft for the coordinator to register as its own status section (called §65 here). No §65 row has run.

- **Base text:** github main `e696fc77`, the status entry "The Realtime delay lean after the framing fix, read post hoc" (`s64-lean` `6d55024c`).
- **Round 3** takes Fable's last round, through the coordinator. It registers the exact replication: §64's stage R configuration unchanged, 24 pairs, one scored readout, three labels.
- **Round 2's 600 MB head/rest design is withdrawn.** It moves, with a bisect seat, into a follow-up that runs only if this session replicates (§7).

**For the owner, in plain words.**
- **The question.** §64's Realtime stage showed the fixed build delivering in order later than the old one: 9 of 11 slot pairs, about +24 %, read post hoc. This session asks one question, registered before any row: does that come back when the same configuration runs again, with twice the pairs?
- **What it runs.** The same two binaries as §64, already on the VM, with nothing built. The same rows: Realtime, 100 MB, full reliability.
- **Cost.** About 25 min of VM plus a rehearsal of about 15 min, with host cargo and tests paused under the host lock.
- **Outcomes.** The merge stands whatever this reads.
  - **REPLICATED** starts the follow-up: long rows, plus a build of F2's commit 7 to bisect against commit 8.
  - **NOT-REPLICATED** closes the question at the size seen, printing the largest cost still possible.
  - **UNRESOLVED-N** means too few usable pairs.

## 1. Seats

| seat | binary | source | where |
|---|---|---|---|
| **RB** (BASE) | `0ec952583a99a3f3f08e56debbdcc9c3c2ddf41516f14220421964fe931ee39b` | BASE64, main before F2 (§64 stage G's BASE) | VM `~/r64/run/bins64/base-raptorpath` |
| **RC** (CAND) | `8104fa027c9dea559c9ffdb0d5630117e70ffcd97a092e73700441446771be5b` | CAND64 = BASE64 + `f2-impl` `ab9feec4` (F2 commit 8), the binary §64 guarded and main merged | VM `~/r64/run/bins64/cand-raptorpath` |

- **Pins.** Both shas are §64's `BINSHA.txt` (`docs/l1-raw/s64/s64-session.tgz`, `session/BINSHA.txt`). Each is sha-checked before any row; a mismatch is `ABORT-BUILD`.
- **Seat witness.** CAND's binary holds `[SYMSZ]` and `[UNFRAMABLE]`, BASE's neither (`ABORT-SEAT`).
- **No build and no engine suites.** These binaries passed §64's suite record and guard; the sha pin is the identity witness (`SUITES-NOT-RUN plan 65: binaries sha-pinned to §64's BINSHA.txt`).
- **Scope, stated.** RC is the F2 merge commit's binary, not a fresh build of today's main.

## 2. Cell and rows: §64's stage R, unchanged

- **The cell.** c2-100, Realtime hint, ρ = 1, **100 MB rows, `RWM_PERF_TIMEOUT_S=150`.** Instrument env `RWM_GEN=0 RWM_DIAG=1 RWM_RTOBS=1 RWM_RDIAG=1`. These are `plan64.BYTES_R` / `TIMEOUT_R` and the battery's plan 64 env, unchanged.
- **ρ = 1 witnessed on both ends:** `window_reliable: true` in each end's `perf configuration` line; otherwise `VOID-RHO` (§64's witness).
- **The table: 24 slots, 48 rows** (`s65_score.plan()`). Seeds 42 and 7 × 12 replicates, plan 41c's interleave.
  - The two rows of a slot run back to back. Seed 42's pair runs RB first, seed 7's RC first.
  - At 12 slots the table is `plan64.plan()` exactly, so §64's 24 rows are this table's first half in layout. They are not reused: every §65 row is new.
- **Why rows under the guard's 30 s tick floor are scored here.**
  - §41d's floor belongs to the guard's unpaired 12 + 12 rank comparison of whole-run means. There, row length is what tightens each row's mean enough to resolve a few per cent.
  - Here the comparison is paired by slot: the two rows run back to back under one seed. The pair count supplies the power, and the power table (§4) gives 0.96 at the seen size on exactly these rows.
  - A replication must also repeat the configuration that produced the observation. Row length is changed only in the follow-up (§7).
- **Void rules:** plan 64's, unchanged. There is no `VOID-SHORT`.

## 3. What is scored

### 3.1 Slots and the dead BASE redraw

- **The redraw rule is plan 64's replacement rule, with one entry added and a larger cap** (a small change to `plan64.replace`, §8):
  - `BASE-F2-DEAD` joins the replaceable set;
  - the cap goes from 2 to 4 slots;
  - everything else is plan 64's: once per slot, immediately, never for an F2 failure on RC.
- **A dead BASE warm-up** (about 1 in 12; about 2 expected in 24) is redrawn once in the same slot.
  - If the redraw also dies, or the cap is spent, the pair is dropped.
  - A dead BASE row never enters the test, so it can never count as CAND's win.
  - In RC-first slots, the redraw runs at least 165 s after its partner (the dead row's full timeout).
- **An RC row failing the F2 reading** (§64 §2.2 items 1–4) is `RT-FAIL`: an **F2-FINDING**, printed first and sent to the owner. It is never redrawn, and its pair is not scored.
- **A DNF** is the worst rank on its own side (§47):
  - an RC DNF gives d = +L, an RB DNF gives d = −L, with L = 1 + max |d| over the finite pairs;
  - both DNF gives d = 0.

### 3.2 The readout, the test and the margin

- **Scored, one readout:** the whole-row in-order delay mean (the row's `inorder_mean`, §41d's definition), paired by slot.
  - **Prediction, registered: RC worse.** The test is one-sided.
- **Per usable pair:** d = ln(RC / RB).
  - **p:** the exact one-sided Wilcoxon signed-rank p, P₀(W⁺ ≥ W⁺_obs), over all sign flips of the observed midranks of |d| (computed by exact counting), zeros dropped.
  - **h:** the Hodges–Lehmann shift.
  - **U:** its exact one-sided 95 % upper bound W₍N−c₎ (c = 91 at 24 pairs).
- **TOL = 5 %, fixed.** This is the tolerance the merge guard applies to c2's in-order clause: §64 stage G printed `tol=0.0500` (`l1ab.REL_FLOOR`, §11–§13).
  - Round 1's 8 % is c2's in-order A/A d80 (§41d; `guard-clause-dry.md` §11), not `rt64_score`'s CPU-per-GB flag.
  - The off-Bulk design's 20 % (`s51-design.md`) is a different design's d80 on Bulk rows.
- **The RB half-split q95 is reported only.**

### 3.3 Labels (first match wins)

| label | condition | what it means for the merged fix | next |
|---|---|---|---|
| **UNRESOLVED-N** | fewer than **18** usable pairs (the power table: 18 pairs replicate the seen size with probability 0.90) | too few pairs to read | to the coordinator; any re-run needs its own registration |
| **REPLICATED** | p ≤ 0.05 **and** h_rel > 5 % | §64's lean is real: on Realtime c2 at ρ = 1 the merged binary delivers in order later than the pre-fix one, by more than the guard's tolerance | the follow-up (§7); the merge stands |
| **NOT-REPLICATED** | otherwise | the lean does not come back at the size seen (power 0.96 at +23.8 %). **U is printed as the largest cost still possible** (95 % one-sided) | none by default; the question closes |

- **F2-FINDING** (any RC `RT-FAIL`) is printed before the label and outranks it.
- **On a noisy ledger,** NOT-REPLICATED prints a large U (stub C4: ±50 % noise, U = +9.5 %), so the closing statement always carries its own uncertainty.

### 3.4 Reported only (per pair, per row)

- **The shift per run order:** seed 42's pairs (RB first) against seed 7's (RC first).
- **Early loss per row, both seats:** quinn's lost and sent packets on leg 0 at the client's last `[DIAG]` tick with t ≤ 2 s (`qlp=` in `[DIAG]`; `s65_score.row_fields`).
  - **What it can see:** the first 2 s of transfer loss per seat.
  - **What it cannot see:** the warm-up. On a live row the warm-up (about 0.05 s) ends before the first tick, and one lost `WindowStart` is invisible among about 27 000 packets.
  - A dead row's `[DIAG]` clock is the warm-up phase, so the dead rows are listed apart, not compared.
- **Other readouts:** goodput, CPU/GB, `lag_E_cli`, `lag_E_srv`, `rtp_floor_p0` (paired count worse, signed-rank p, HL).
  - `rtp_floor_p0` is a single start-up sample on these rows (`s64-lean` read 2b).
- **Counts:** `BASE-F2-DEAD65` (redrawn alive, dead twice, not redrawn), redraws, dropped pairs.
- **Limit, stated:** pairing removes drift between slots, not the survivor selection of the RB rows.

## 4. Pair count and pair floor from power (design computation, done before any §65 row)

- **Source:** `docs/derivations/s65-registration/s65_power.py` and its `.out`.
- **Model:** §64's 11 stage R pairs, the configuration this session repeats. d = ln(RC / RB) of the whole-row in-order mean: sd 0.268, HL +23.8 %.
- **Method:** the pairs are centred on their HL and resampled with replacement; 4000 seeded draws per cell.

| pairs | P(REPLICATED) at +23.8 % (seen) | P(REPLICATED) at +11.9 % (half) | no shift: mean U | no shift: P(REPLICATED), false |
|---|---|---|---|---|
| 12 | 0.748 | 0.357 | +12.6 % | 0.057 |
| 18 | 0.898 | 0.484 | +9.8 % | 0.046 |
| **24** | **0.958** | **0.602** | +8.6 % | 0.058 |

- **Count: 24 pairs (48 rows), fixed, never extended after rows are read.** It is the only count at ≥ 0.9 at the seen size, and it costs about 6 min more than 18.
- **Pair floor: 18.** Below 18 usable pairs, power at the seen size falls under 0.90, so a NOT-REPLICATED could no longer close the question. Round 1's floor of 10 is replaced.
- **Under no shift, U averages +8.6 %.** A NOT-REPLICATED therefore typically says "no effect of +24 %, and not more than about +9 %". It does not say "no effect".
- **Caveat on the model.** It is §64's 11 pairs, which are themselves the observation under test. Resampling 11 values understates the tails.

## 5. The reference scorer and its path exercise (done, before registration; labelled)

- **Scorer:** `tools/l1/s65_score.py` (this branch), sha256 `41d9d58e27f42eb92862e243c25900d547ea7ffd6b64a6d00ddcb039fa844a21`. It is the registered statistic, pure and env-free; the launch checks its sha (§8).
- **Path exercise:** `s65_path_exercise.py` (`f22c6f412db2ea8c…`) and `s65_path_exercise.out` (`43b2160cfe895e69…`). Two runs were byte-identical; it ends `PATH EXERCISE PASS`.
- **A. §64's stage R rows,** at the 12-slot table with the floor scaled to 9 and the early-loss field read from the rows' logs. It reproduces `s64-lean`:
  - RB q95 0.1930 over 200 splits;
  - in-order 9/11;
  - `lag_E_cli` 8/11, `lag_E_srv` 7/11, `rtp_floor_p0` 9/11;
  - the dead RB s42 r4 drops its pair.

  It prints p 0.021, HL +23.8 %, U +43.3 %, and the label REPLICATED.

  **This is not evidence and is kept out of every result claim.** It is the third test on the same 11 pairs, after the sign test and the rank p, and the readout was chosen after seeing them. It is the prediction (RC worse, about +24 %) that §65 tests on 48 new rows.
- **C. Synthetic 24-pair ledgers:**

  | case | label |
  |---|---|
  | quiet null | NOT-REPLICATED, U printed |
  | +25 % | REPLICATED |
  | a resolved +3 % | NOT-REPLICATED: HL under TOL |
  | **stub:** pairs as noisy as §64's (±50 %), no shift | NOT-REPLICATED, U = +9.5 % |
  | seven slots dead twice (17 pairs) | UNRESOLVED-N, the dead pairs not counted |
  | a dead RB redrawn alive | slot scored |
  | an RC `RT-FAIL` | F2-FINDING, pair dropped |
  | an RC DNF | d = +L |

**Read now on §64's 11 pairs (POST HOC, reported; from section A):**
- **By run order:**
  - seed 42 (RB first): HL +26.6 %, RC worse in 5 of 5;
  - seed 7 (RC first): HL +21.4 %, 4 of 6.
  - On these rows the lean does not depend on which seat runs first.
- **Early loss by 2 s:**
  - live RB rows 598–754 lost of about 27 000 sent (median 685);
  - RC rows 595–727 (median 663);
  - seed 7 runs higher than seed 42 on both seats.
  - The dead RB s42 r4 sent 22 packets by 2 s, 4 of them lost. Its clock is the warm-up, so it is not comparable. That is the only heavy early loss in the ledger, which fits selection acting on the first packets; the live rows' readout cannot show that selection (§3.4).

## 6. Rehearsal (`LD_PLAN=65 LD_REHEARSE=1`) must show

Rows are never re-run.

| check | NO-GO consequence |
|---|---|
| both prebuilt binaries match their pins; the seat witness strings; `s65_score.py` matches its registered sha | ABORT-BUILD / ABORT-SEAT / ABORT-TOOLS: stop |
| every tools test passes on the VM; `TOOLS-TESTS-OK` and `TOOLS-MANIFEST.sha256` written | ABORT-TESTS: fixed on main, re-rehearsed |
| **RC s42 r1 is LIVE (RT-OK):** completes inside 150 s, rc 0, items 1–4 pass, `window_reliable: true` on both ends, readouts and `qlost_2s` computed | a missing field or witness is a harness fault: fixed, re-rehearsed. A failed item is an F2-FINDING: no launch |
| **RB is LIVE on one row:** RB s42 r1 LIVE with `window_reliable: true` on both ends and its readouts computed | see below |

**What a dead BASE rehearsal row means.**
- If RB s42 r1 is `BASE-F2-DEAD`, that is the old defect at its known rate, not a harness fault.
- It does not satisfy the LIVE assertion, and it is not re-run.
  - The rehearsal runs **one more RB row on a different slot** (RB s7 r1, the smoke table's conditional third row), which must be LIVE.
  - If that one dies too, the rehearsal is **NO-GO**, and the two dead rows go to the coordinator.
- **Why two dead rows stop the launch:** two deaths in two draws has probability about 1 in 144 at 1 in 12. It would mean the death rate is far above the one the redraw cap and the 18-pair floor were sized for.

## 7. After the label

- **REPLICATED:** a follow-up registration that combines two things.
  - **Long rows:** 600 MB with a rest window, to ask whether the lean persists past start-up. This is round 2's design.
  - **A bisect seat:** F2 commit 7 (`626fdf9a`) is docs-only, so its engine is commit 6's. Its binary runs against commit 8's (`ab9feec4`, the unframe and `offer` rewrite).
  - It needs one build. The merge stands meanwhile.
- **NOT-REPLICATED:** the status records §64's lean as not reproduced at +24 % (power 0.96), with U as the largest cost still possible. No further session by default.
- **UNRESOLVED-N:** to the coordinator. Any further session needs its own registration.
- **F2-FINDING:** diagnosed before anything else.

## 8. Harness brief (the idle guard-harness worker; tools only, on main, which has harness64)

The worker starts from `s65-design` and does not change `s65_score.py`.

1. **`plan64.py`:**
   - **Tables:** `plan(smoke=False, plan_id="64")`.
     - Plan `"65"` returns the 48 rows of `s65_score.plan()`, as `(CELL, "def", arm, seed, int(rep))`.
     - Its smoke table is RB s42 r1, RC s42 r1, and the conditional RB s7 r1 (§6).
     - Plan 64's tables are unchanged, and `count` follows.
   - **`rt_row`:** adds `row.update(s65_score.row_fields(cli))` (the early-loss fields; harmless on plan 64 rows).
   - **`replace <ledger> <cell> <arm> <seed> <rep> [plan]`** (default `64`, unchanged). Under `65`, the replaceable set is `R_REPLACEABLE + ("BASE-F2-DEAD",)` and the cap is `s65_score.REDRAW_CAP` (4) slots. Nothing else changes.
   - **`BUDGET_S`:** `"65": 6700`, `"65r": 1800` (§9).
   - **`smoke(path, plan="65")`:** asserts §6's rows. **The LIVE assertion applies on each seat:** RC RT-OK, and RB LIVE on s42 r1 or on the conditional s7 r1. Two dead RB rows read `SMOKE-FAIL (two BASE-F2-DEAD rows: NO-GO, to the coordinator)`.
2. **`loopdiag_battery.sh`:**
   - **Plan list:** line 96 accepts `65`; `P64=1` for plans 64 and 65, plus a `P65` flag. Sizes, timeout and env are plan 64's (no change).
   - **Table:** the `plan64` function reads `plan64.py plan [smoke] <plan>` and asserts `plan64.py count <plan>` rows (24 / 48).
   - **Smoke:** runs the conditional RB s7 r1 only when RB s42 r1's row is `BASE-F2-DEAD`.
   - **Run path:** the run block's `if [ "$PLAN" = "64" ]` becomes 64 or 65. `run_slot64` passes `$PLAN` to `replace`.
   - **Copy list:** `s65_score.py` joins the file-copy list (line 84).
3. **`place38_parse.py:937`:** `LD_PLAN in ("64", "65")`.
4. **`loopdiag_parse.py`:** `smoke65r` → `plan64.smoke(path, plan="65")`, `check65r` → `plan64.check(path, plan="65")`, `score65r` → `s65_score.score`.
5. **`loopdiag_run_all.sh`, a stage-R-only launch for `LD_PLAN=65`:**
   - **Binaries:** the two pinned binaries from `~/r64/run/bins64`, through `LD_BIN_PREBUILT_BASE` / `LD_BIN_PREBUILT_CAND`. `LD_BIN_SHA_BASE` and `LD_BIN_SHA_CAND` are required, and both are sha-checked before any row (`ABORT-BUILD`).
   - **No cargo:** no build, no suites stage, no `SRC` / `BASE_SRC` checks; the `SUITES-NOT-RUN` line (§1).
   - **Scorer by sha:** the launch computes the sha256 of `tools/l1/s65_score.py` and requires it to equal `LD_S65_`41d9d58e27f42eb92862e243c25900d547ea7ffd6b64a6d00ddcb039fa844a21``, the registered pin in §5 (`ABORT-TOOLS`, nothing run).
   - **Seat witness:** the strings, as plan 64.
   - **Tools tests:** reused through `LD_PRIOR_TOOLS=<the rehearsal's run dir>` when `tools/l1` is byte-identical (`TOOLS-MANIFEST.sha256`, §64's rule; `TOOLS-TESTS-REUSED`); otherwise run.
   - **Host lock:** the session runs only while the coordinator holds the host lock, with host cargo and host tests paused.
     - The process list is checked before the rehearsal and again before the launch, and recorded in the era log (§64 recorded only the first).
     - The VM's own locks are taken as for every plan (`take_lock LOCK_VM`, `LOCK_RP`).
   - **Budget guard,** beside plan 64's `W64` block: refuse with less than `plan64.py budget 65` (or `65r`) seconds before `LD_HARD` (`ABORT-BUDGET`).
   - **Stage:** `stage rt65 65` only (`smk=smoke65r; scr=score65r; chk=check65r`). A rehearsal stops after the smoke.
   - **Hard limit:** `LD_HARD` = launch + 2.5 h, or rehearsal + 1 h.
   - **Summary:** `$RUN/score.txt` carries `VERDICT65`, `TEST65` and `F2-FINDING65`.
   - **Copy list:** `s65_score.py` joins the copy list (line 121).
6. **`harness_paths.py`:** stub paths for 65: rehearse, rehearse-rb-dead-once, rehearse-rb-dead-twice, full, base-dead-redrawn, base-dead-twice, rc-rtfail, rho-one-end, score-sha-mismatch.
7. **Tests:**
   - `test_harness64.py` keeps every plan 64 pin and adds plan 65's tables and budgets.
   - A new `test_s65_score.py` turns cases C1–C8 into unit tests and asserts `s65_score.plan(12)` equals `plan64.plan()`'s slots.
   - `test_stubrun.py` gets a case65.
8. **Done when:**
   - every python tools test passes on the host, and the stub runs for 64 and 65 pass;
   - `rt64_score.py` on the committed §64 `rt64/ld.log` still prints `rt64/score.txt` byte for byte (true on this branch);
   - the path exercise prints `PATH EXERCISE PASS` with its `.out` unchanged.
   - Checked on this branch with `s65_score.py` added: `test_harness64.py` (76/0), `test_harness_paths.py` and `test_harness60.py` pass on the host. `test_stubrun.py` was not run here.
   - **Report:** the sha, and any rule the worker had to settle.

## 9. Cost

- **Typical: about 25 min of VM.**
  - 48 rows × about 15 s (§64 ran 23 live rows in 345 s) is 12 min;
  - about 2 dead BASE rows at the full 152 s, plus their 15 s redraws, is about 6 min;
  - the smoke is about 3 min (§64's took 168 s);
  - the tools tests are reused from the rehearsal (8 min if not).
- **Rehearsal: about 15 min** (tools tests plus 2 or 3 rows).
- **Worst case (budget), plan 64's formula with RB rows at the full timeout:**
  - Launch: smoke 200 + 24 RC × 30 + 24 RB × 165 + 4 redraws × 165 + scoring 600 + tools 480 = 6620 s. Budgeted at 6700 s; `LD_HARD` = launch + 2.5 h.
  - Rehearsal: 480 + 3 × 165 + 300 = 1275 s. Budgeted at 1800 s; `LD_HARD` = rehearsal + 1 h.
- **Host cargo and tests paused** under the host lock for both.
- **Order:** the harness change merges, then the rehearsal, then a pin amendment, then the launch.
