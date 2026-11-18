# §65: the Realtime in-order delay re-check after the framing fix — pre-registration draft

Draft for the coordinator to register as its own status section (called §65 here). No §65 row has run. Base text: github main `e696fc77`, the status entry "The Realtime delay lean after the framing fix, read post hoc" (`s64-lean` `6d55024c`).

**For the owner, in plain words.**
- §64's Realtime stage showed the fixed build delivering in order a little later than the old one: 9 of 11 slot pairs, read post hoc, on 12 s rows too short to resolve that readout. This session asks the question properly: one readout, registered before any row, on rows long enough to resolve it.
- **It runs the same two binaries §64 ran, already on the VM.** Nothing is built and no engine suite runs.
- **It costs about 40 min of VM time, plus a rehearsal of about 15 min,** with host cargo paused.
- **The merge stands whatever this reads.** A REGRESSION label starts a registered bisect between F2's commits 7 and 8. It does not revert anything.

## 1. Seats

| seat | binary | source | where |
|---|---|---|---|
| **RB** (BASE) | `0ec952583a99a3f3f08e56debbdcc9c3c2ddf41516f14220421964fe931ee39b` | BASE64, main before F2 (§64 stage G's BASE) | VM `~/r64/run/bins64/base-raptorpath` |
| **RC** (CAND) | `8104fa027c9dea559c9ffdb0d5630117e70ffcd97a092e73700441446771be5b` | CAND64 = BASE64 + `f2-impl` `ab9feec4` (F2 commit 8), the binary §64 guarded and main merged | VM `~/r64/run/bins64/cand-raptorpath` |

- Both shas are §64's `BINSHA.txt` (`docs/l1-raw/s64/s64-session.tgz`, `session/BINSHA.txt`). The launch installs them as prebuilt binaries and sha-checks each against these pins before any row; a mismatch is `ABORT-BUILD`.
- The seat witness of §64 stands: CAND's binary holds `[SYMSZ]` and `[UNFRAMABLE]`, BASE's neither (`ABORT-SEAT`).
- **No build and no engine suites.** These exact binaries passed §64's suite record (the reference-manifest HIT at `ab9feec4`) and its guard; the sha pin is the identity witness. The session writes `SUITES-NOT-RUN plan 65: binaries sha-pinned to §64's BINSHA.txt`.
- **Scope, stated:** RC is the F2 merge commit's binary, not a fresh build of today's main. Main has gained work since. §58 Amendment 3 records that `raptorpath-core` does not touch the engine binary, but no other identity check is claimed here. The result speaks for the F2 change, which is what a bisect needs.

## 2. Cell and rows

- **c2-100, Realtime hint, ρ = 1, 600 MB per row, `RWM_PERF_TIMEOUT_S=200`.** Instrument env as §64 stage R: `RWM_GEN=0 RWM_DIAG=1 RWM_RTOBS=1 RWM_RDIAG=1`.
- **Table:** plan 64's stage R table unchanged (`plan64.plan()`): arms RB and RC, 12 + 12 rows, seeds 42 and 7 × 6 replicates, plan 41c's interleave. The two rows of a slot run back to back.
- **ρ = 1 witnessed on both ends:** `window_reliable: true` in each end's `perf configuration` line; otherwise `VOID-RHO` (§64 §2.2's witness, unchanged).
- **Why 600 MB:**
  - §64's c2 Realtime rows ran at 68.2–68.7 Mbit/s (medians), so 600 MB is about 70 s of transfer.
  - That is well above guard v2's 30 s tick floor (§41d), and it matches stage G's c2 row size.
  - The 260 MB minimum (30 s at 68 Mbit/s) leaves no margin for a slower row.
- **Why 200 s:** it is plan 41d's timeout for 600 MB c2/c8 rows, 2.8 × the expected 71 s. Below about 24 Mbit/s a row is a DNF.
- **Not yet observed:** no committed row has run c2 Realtime at ρ = 1 and 600 MB. Plan 59's 600 MB Realtime rows evicted (ρ < 1). So the rehearsal's two rows are the go/no-go for size and timeout (§5).
- **Row void rules:**
  - plan 64's non-F2 voids stand: `VOID-COTENANT`, `VOID-ONDISK`, `CONTAMINATED`, `WITNESS-FAIL`, `VOID-RHO`, `VOID-RC`, `NO_DATA`;
  - **new:** a completed row with under 30 s of `[DIAG]` ticks is `VOID-SHORT` (§41d's rule, ported to stage R).

## 3. What is scored

**One readout: the whole-run in-order delay mean** (`threadp1_parse.inorder_delay_stats`, §41d's definition, in ms). Its domain is reliable delivery, and ρ = 1 is witnessed on every scored row.

**3.1 Slots and redraws (a slot holds one row per arm; at most one redraw per slot).**
- **A dead BASE warm-up** (`BASE-F2-DEAD`: the RB client prints `warm-up object never acked`, the old F2 defect, about 1 in 12 at ρ = 1) is **redrawn once, immediately, in the same slot,** at most 3 slots.
  - **Adjacency limit:** in odd groups RC runs first, so a redrawn RB row runs at least 215 s (the dead row's full timeout) after its RC partner, not back to back.
  - If the redraw also dies, or the cap is spent, the slot's pair is **dropped**.
  - A dead BASE row never enters the test. In particular it is never counted as RB's worst rank, so it can never count as CAND's win.
  - The dead count is reported.
- **A non-F2 void on either arm** (§2) is redrawn once in the same slot, at most 2 slots (§64's cap). If it voids again, the pair is dropped.
- **An RC row failing the F2 reading** (§64 §2.2 items 1–4) is `RT-FAIL`: an **F2-FINDING**, reported first and sent to the owner. It is never redrawn, and its pair is not scored.
- **A DNF** (a completed warm-up and no transfer inside 200 s) is the worst rank on its own side, guard practice (§47):
  - an RC DNF gives d = +L, an RB DNF gives d = −L, with L = 1 + max |d| over the finite pairs;
  - both DNF gives d = 0.

**3.2 Resolution, from the RB rows alone, before any RC value is read.**
- The q95 of |x| over the half-per-seed splits of the completed RB rows (`place41b_score.resolution`, the RESOLUTION lines' construction: 400 splits at 6 + 6), x = (med(half) − med(rest)) / med(rest).
- It is printed beside TOL. It is a **can-this-design-see-TOL gate,** not the test's noise: it compares unpaired 6-row medians, which is noisier than the paired estimator, so the gate is conservative.

**3.3 The margin: TOL = 5 %, fixed.**
- **Provenance:** `l1ab.REL_FLOOR`, the guard's tolerance floor for delay-type clauses (§11–§13; `spread_rel` = max(5 %, half-range / median)). It is the tolerance §64 stage G printed for c2's in-order clause.
- **Not data-derived.** `spread_rel`'s data branch would widen with BASE's noise, which would make NO-REGRESSION easier exactly when BASE is noisy, so it is not used.
- **Context:**
  - it is below the lean under re-check (+12.6 %, §64 stage R);
  - guard v2's A/A d80 for c2 in-order delay on 600 MB Bulk rows is 5–8 % (`guard-clause-dry.md` §11; §41d quotes 8 %). In the guard, d80 is the power-side number and the §21 tolerance (this 5 %) is the pass-rule side.
- **Reachability, stated.** No Realtime A/A ledger exists.
  - §64's RB half-split q95 was 19.3 % on 12 s rows. If row noise falls as √T, that is about 8 % on 70 s rows, above TOL.
  - On Bulk c2 at 600 MB it was 3.0 % (§64 stage G).
  - So the q95 gate's value on these rows is somewhere around 3–8 %, and NO-REGRESSION may be out of reach by construction.
- **The q95 gate binds before U does** (normal approximation, row log-sd σ, pair correlation ρ_p):
  - the 6-v-6 median split gives q95 ≈ 1.96 · √(π/6) · σ ≈ 1.42 σ;
  - the paired HL bound's half-width is about 1.645 · √(π/3) · √(2(1 − ρ_p)) · σ / √12 ≈ 0.69 σ √(1 − ρ_p).
  - So q95 ≈ 2 × the half-width or more. With h ≈ 0, NO-REGRESSION needs σ ≲ 3.5 % through the gate, against σ ≲ 7 % through U alone.
- **Two questions for the coordinator before this registers** (the draft and the reference scorer implement the first option of each):
  - **Q1, the margin:** TOL = 5 % (the §21 tolerance floor, as drafted), or 8 % (the c2 in-order A/A d80, the margin `rt64_score`'s owner flag uses)?
  - **Q2, the q95 gate:** keep it as a condition of NO-REGRESSION (guard practice: "q95 > tol: the guard cannot see this clause below q95"; drafted), or report it only and let the exact paired bound U decide?
    - U alone keeps the property that more noise never helps, because U widens with noise.
    - **My recommendation is Q2 = report-only.** With the gate on, the label most likely turns on BASE's unpaired noise rather than on the paired comparison the session exists for.
  - Either change is a constant or one condition in `s65_score.py`, plus a re-run of the path exercise and a re-pin.

**3.4 The test (exact, no RNG).**
- Per usable pair (slot): d = ln(RC / RB) of the in-order mean.
- **p:** the exact one-sided Wilcoxon signed-rank p, P₀(W⁺ ≥ W⁺_obs), over the 2ⁿ sign flips of the observed midranks of |d|, zeros dropped.
- **h:** the Hodges–Lehmann shift, the median of the n(n+1)/2 Walsh averages.
- **U:** its exact one-sided 95 % upper bound, W₍N−c₎, with c the largest value where P₀(W⁺ ≤ c) ≤ 0.05 for n untied ranks: c = 17, 13 and 10 at n = 12, 11 and 10.
- Relative values: h_rel = e^h − 1, U_rel = e^U − 1.

**3.5 Labels (mutually exclusive, first match wins).**

| # | label | condition | what it means for the merged fix | next |
|---|---|---|---|---|
| 1 | **UNRESOLVED-N** | fewer than 10 usable pairs | the session did not produce enough pairs | to the coordinator; a re-run needs its own amendment |
| 2 | **REGRESSION** | p ≤ 0.05 **and** h_rel > TOL | on Realtime c2 at ρ = 1, the merged F2 binary delivers in order later than the pre-fix binary, by more than the guard's tolerance | the merge stands. A registered bisect between F2 commits 7 and 8 (§6) |
| 3 | **NO-REGRESSION** | U_rel ≤ TOL **and** RB q95 ≤ TOL | the merged fix raises the whole-run in-order delay mean by at most 5 % (95 % one-sided), and BASE's own floor is under 5 % | §64's lean closes as not reproduced at this size. Nothing to bisect |
| 4 | **UNRESOLVED** | anything else | the session cannot tell, for one of three printed reasons: a resolved shift at or below TOL with its bound above TOL; a bound above TOL; or q95 > TOL | to the owner with the printed reason; no automatic re-run |

- **An F2-FINDING** (any RC `RT-FAIL`) is printed before the label and outranks it as news: it is a finding against the merged fix, diagnosed first, whatever the delay label reads.
- **A noisier BASE never helps.** Both the bound and the q95 gate get harder to pass as noise grows; synthetic case C5 shows this (§4).

**3.6 Reported only (never a label).** Per readout: RC worse in k of n pairs, the exact signed-rank p, and the RC/RB medians.
- **Readouts:** goodput, `cpu_cli/GB`, `cpu_srv/GB`, `lag_E_cli`, `lag_E_srv`, `rtp_floor_p0`.
  - `rtp_floor_p0` is a single start-up sample on 12 s rows (`s64-lean` read 2b). On 70 s rows the 10 s window rolls, so it is a different quantity there, and it is reported, not compared with §64.
- **The head in-order mean** (new row field `inorder_mean_head`: the mean over in-order ticks with t ≤ 11.5 s, the length of §64's rows).
  - **Why:** a whole-run mean on 70 s rows dilutes an effect confined to the first 12 s about sixfold. NO-REGRESSION is therefore a whole-run statement only.
  - **`HEAD-FLAG65`** (non-gating) is raised when the head readout's signed-rank p ≤ 0.05 and its HL shift exceeds TOL. It goes to the owner with the label.
  - `s64-lean` read 2b found §64's gap on both halves of its 12 s rows, so this is a check, not an expectation.
- **Counts:** dead BASE rows (`BASE-F2-DEAD65`: redrawn alive, dead twice, not redrawn), redraws, dropped pairs.
- **Limit, stated:** the RB rows that enter are, by construction, rows whose warm-up survived the F2 defect. Pairing removes drift between slots, not that selection. Its direction is unknown, and the dead count is printed beside the label.

## 4. The reference scorer and its path exercise (done, before registration; labelled)

- **Scorer:** `tools/l1/s65_score.py` (this branch), sha256 `3ebdfc8e24b97a3b…`. Pure and env-free, constants as above. It is the registered statistic: the harness worker wires it in unchanged (§7).
- **Path exercise:** `docs/derivations/s65-registration/s65_path_exercise.py` (sha256 `7668a5735726e2c5…`) and its output `s65_path_exercise.out` (`a14f9456cca5f2f9…`). Two runs were byte-identical; the exercise ends `PATH EXERCISE PASS`.
  ```
  python -I -X utf8 docs/derivations/s65-registration/s65_path_exercise.py tools/l1 docs/l1-raw/s64/s64-session.tgz
  ```
- **A. §64's committed stage R rows, tick floor passed as 0** (path exercise, not a reading). It reproduces `s64-lean`:
  - RB q95 0.1930 over 200 splits;
  - in-order RC worse in 9 of 11;
  - `lag_E_cli` 8/11, `lag_E_srv` 7/11, `rtp_floor_p0` 9/11;
  - the dead RB s42 r4 drops its pair.

  Signed-rank p = 0.021, HL shift +23.8 %, U = +43.3 %, so the code path prints REGRESSION.
  - **This is not a §65 result.** These are 12 s rows below the registered floor, and the readout was chosen after seeing them.
  - As a by-product, the paired signed-rank p on these pairs is 0.021 for in-order delay, 0.034 for `lag_E_cli` and 0.027 for `rtp_floor_p0`. These are post hoc too: one of seven readouts, chosen after the rows were seen. Their q95 of 19 % > TOL means NO-REGRESSION was unreachable on them.
- **B. The same rows under the registered 30 s floor:** all 23 completed rows are VOID-SHORT, giving UNRESOLVED-N.
- **C. Synthetic ledgers, one per rule:**

  | case | label |
  |---|---|
  | quiet null | NO-REGRESSION |
  | +15 % | REGRESSION, with HEAD-FLAG65 raised |
  | a resolved +3 % bounded under TOL | NO-REGRESSION |
  | ±20 % pair noise | UNRESOLVED |
  | identical pairs but a ±20 % BASE | UNRESOLVED: q95 gate |
  | three slots dead twice | 9 pairs, UNRESOLVED-N, the dead pairs not counted |
  | a dead RB redrawn alive | slot scored |
  | an RC `RT-FAIL` | F2-FINDING, pair dropped |
  | an RC DNF | d = +L |

## 5. Rehearsal (`LD_PLAN=65 LD_REHEARSE=1`) must show

| check | NO-GO consequence |
|---|---|
| both prebuilt binaries match their pins; the seat witness strings | ABORT-BUILD / ABORT-SEAT: the bins64 copies are not §64's; stop |
| every tools test passes on the VM; `TOOLS-TESTS-OK` and `TOOLS-MANIFEST.sha256` written | ABORT-TESTS: fixed on main, re-rehearsed |
| **RC s42 at 600 MB:** completes inside 200 s, RT-OK, passes items 1–4, `window_reliable: true` on both ends, ≥ 30 s of ticks, `inorder_mean` and `inorder_mean_head` computed | a missing witness or field is a harness fault: fixed, re-rehearsed. A DNF or a short row means the size or timeout is wrong: amend §2 before launch. A failed item is an F2-FINDING: no launch |
| **RB s42 at 600 MB:** LIVE with `window_reliable: true` on both ends and ≥ 30 s of ticks, or `BASE-F2-DEAD` | any other label is a harness fault. A dead RB leaves BASE's 600 MB completion unobserved: accepted, and reported in the amendment |

A rehearsal row is never re-run. Its rows are not scored.

## 6. After the label

- **NO-REGRESSION:** the status records §64's lean as not reproduced on long rows (whole-run, at TOL = 5 %), with the HEAD-FLAG65 line beside it.
- **REGRESSION:** a registered bisect.
  - F2 commit 7 (`626fdf9a`) is docs-only, so its engine is commit 6's. Commit 8 (`ab9feec4`: the allocation-free unframe and the `offer` rewrite in `net/mod.rs` / `receiver.rs`; the coordinator's reading in `f2-guard-registration.md` §1.1) is the only delivery-path change after commit 6.
  - The next registration builds commit 7's binary (BASE64 + `f2-impl` up to `626fdf9a`) and runs it as the base against commit 8's binary (RC here) in this design.
    - Commit 8 worse than commit 7: the cause is `offer`.
    - The two equal: the cause is in commits 1–6, the framing itself, and the bisect continues there.
  - It needs one build, so it has its own cost line.
- **UNRESOLVED or UNRESOLVED-N:** the printed reason goes to the owner. A further session needs its own registration; for example, more pairs if q95 > TOL.
- **F2-FINDING:** diagnosed before anything else, as §64 §2.4 rules for `RT-FAIL`.

## 7. Harness change (Sonnet worker; tools only, on main, which has harness64)

The worker starts from `s65-design` (it carries `s65_score.py`) and changes nothing in `s65_score.py`. If a change is unavoidable, the path exercise must still reproduce `s65_path_exercise.out` byte for byte, and the sha pin above is amended.

1. **`plan64.py`:**
   - **Per-plan sizes:** `R_SPECS = {"64": (100000000, 150), "65": (600000000, 200)}`, with `BYTES_R, TIMEOUT_R = R_SPECS["64"]` kept (`test_harness64.py` line 153 pins them), and a CLI `plan64.py rspec <plan>` that prints `BYTES TIMEOUT`.
   - **`rt_row`:** also writes `row["inorder_mean_head"]`, the mean in ms of `threadp1_parse.inorder_delay(cli)` over ticks with t ≤ 11.5 s (None when there is none). It is written for both plans and read by plan 65 only.
   - **`rt_status`, under `LD_PLAN=65` only:** a LIVE or RT-OK row that is not a DNF and has `tick_span_s` < 30 becomes `VOID-SHORT`, with a problem line.
   - **`replace`** takes the plan as a sixth argument (default `64`, the current behaviour). Under `65` it returns rc 0 and prints the reason for a one-row slot whose row is:
     - in `R_REPLACEABLE` or `VOID-SHORT`, with fewer than 2 such slots already redrawn; or
     - `BASE-F2-DEAD`, with fewer than 3 dead slots already redrawn (a separate counter).

     A slot holding two rows is never redrawn again.
   - **`BUDGET_S`:** `"65": 7500` and `"65r": 1800` (§8's formulas).
   - **`smoke`** gains plan 65's rehearsal criteria (§5): the ≥ 30 s ticks and the head field.
2. **`loopdiag_battery.sh`:**
   - **Plan list:** line 96 accepts `65`, and `P64=1` is set for plans 64 and 65 (add `P65`).
   - **Row size:** the `P64` line in `run_one` takes the bytes and timeout from `plan64.py rspec "$PLAN"` (bytes, rowbytes, `tmo_env=(RWM_PERF_TIMEOUT_S=…)`). `tmo_env` follows `INSTR_ENV` in the row env, so plan 64 still runs 100 MB at 150 s.
   - **Run path:** `if [ "$PLAN" = "64" ]` at the run block becomes 64 or 65. `run_slot64` passes `$PLAN` to `replace`, and the redraw runs immediately after the voided row.
   - **Copy list:** `s65_score.py` joins the file-copy list (line 84).
3. **`place38_parse.py:937`:** `LD_PLAN in ("64", "65")`.
4. **`loopdiag_parse.py`:** `smoke65r` → `plan64.smoke(path, plan="65")`, `check65r` → `plan64.check`, `score65r` → `s65_score.score`.
5. **`loopdiag_run_all.sh`, a stage-R-only session for `LD_PLAN=65`:**
   - **Binaries:** `LD_BIN_PREBUILT_BASE`, `LD_BIN_PREBUILT_CAND`, `LD_BIN_SHA_BASE` and `LD_BIN_SHA_CAND` are all required (`ABORT-BUILD` otherwise), and the binaries are installed as plan 64's prebuilt path does.
   - **No cargo:** no build, no suites stage, no `SRC` or `BASE_SRC` tree checks; it writes the `SUITES-NOT-RUN` line (§1).
   - **Witness and tests:** the seat witness strings as plan 64. Tools tests, or the `LD_PRIOR_TOOLS` reuse as plan 64.
   - **Stage:** `stage rt65 65` only (`smk=smoke65r; scr=score65r; chk=check65r`). A rehearsal stops after the smoke.
   - **Budget guard (here in `run_all`, beside plan 64's `W64` block):** refuse with less than `plan64.py budget 65` (or `65r` for a rehearsal) seconds left before `LD_HARD`, as plan 64 does (`ABORT-BUDGET`, nothing run).
   - **Hard limit and summary:** `LD_HARD` is the launch + 2.5 h, or the rehearsal + 1 h. `$RUN/score.txt` carries the `VERDICT65`, `F2-FINDING65` and `HEAD-FLAG65` lines.
   - **Copy list:** `s65_score.py` joins the copy list (line 121).
6. **`harness_paths.py`:** stub paths for plan 65: rehearse, full, base-dead-redrawn, base-dead-twice, short-row-void, rc-rtfail, rho-one-end.
7. **Tests:**
   - `test_harness64.py` keeps every plan 64 pin and adds plan 65's specs and budgets (line 393's formula style).
   - A new `test_s65_score.py` turns the path exercise's synthetic cases C1–C9 into unit tests.
   - `test_stubrun.py` gets a case65.
8. **Done when:**
   - every python tools test passes on the host, and the stub runs for 64 and 65 pass;
     - on this branch as committed, `test_harness64.py` (76/0), `test_harness_paths.py`, `test_harness60.py`, `test_pool54.py`, `test_pool57.py`, `test_binid.py` and `test_lagdiag2_digest.py` pass on the host with `s65_score.py` added;
     - `test_stubrun.py` (bash stub runs) was not run here;
   - `rt64_score.py` on the committed §64 `rt64/ld.log` prints `rt64/score.txt` byte for byte (checked on this branch: `loopdiag_parse.py score64r` output, line endings normalised, `cmp` equal);
   - the path exercise still prints `PATH EXERCISE PASS` and its `.out` is unchanged.
   - **Report:** the sha, and any rule the worker had to settle.

## 8. Cost

- **Typical:** about 40 min of VM.
  - 24 rows × about 75 s (70 s of transfer at 600 MB plus §64's 3 s per-row overhead; §64 ran 23 live rows in 345 s);
  - one dead BASE row at the full 215 s plus its 75 s redraw;
  - the smoke (about 4 min) and the tools tests (8 min, or reused).
- **Rehearsal:** about 15 min (tools tests plus two 600 MB rows).
- **Worst case (budget):** 31 rows at the full timeout, plus tools tests, plus scoring.
  - Launch: (2 smoke + 24 + 2 void redraws + 3 dead redraws) × (200 + 15) s + 480 s + 300 s = 7445 s, about 2.1 h. It is budgeted at 7500 s; `LD_HARD` = launch + 2.5 h.
  - Rehearsal: 480 + 2 × 215 + 300 = 1210 s, budgeted at 1800 s; `LD_HARD` = rehearsal + 1 h.
- **Host cargo paused** for the session (the cotenant rule).
- **Order:** after the harness change merges and the rehearsal passes; a pin amendment records the rehearsal before launch.
