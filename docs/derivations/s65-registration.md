# §65: the Realtime in-order delay re-check after the framing fix — pre-registration draft

Draft for the coordinator to register as its own status section (called §65 here). No §65 row has run.

- **Base text:** github main `e696fc77`, the status entry "The Realtime delay lean after the framing fix, read post hoc" (`s64-lean` `6d55024c`).
- **Round 2** takes the coordinator's rulings after Fable's review:
  - TOL = 5 %;
  - the half-split q95 is reported only;
  - two scored windows (head = replication, rest = persistence);
  - the pair count fixed from power;
  - the path exercise kept out of the result claims;
  - run order and early loss reported.

**For the owner, in plain words.**
- **The question.** §64's Realtime stage showed the fixed build delivering in order later than the old one: 9 of 11 slot pairs, about +25 %, read post hoc on 12 s rows. This session asks two questions, registered before any row, on 70 s rows:
  - does the effect come back in the same first 11.5 s (the replication)?
  - does it last for the rest of the transfer (persistence)?
- **The binaries.** It runs the same two binaries §64 ran, already on the VM. Nothing is built and no engine suite runs.
- **The cost.** About 80 min of VM for 24 pairs (48 rows), plus a rehearsal of about 15 min, with host cargo paused. The count comes from a power computation on §64's rows (§4). It is fixed now and never extended after rows are read.
- **The merge stands whatever this reads.** "Bisect" means a registered build of F2's commit 7 against commit 8. "Start-up cost" means the fix costs delivery delay only in the first seconds of a Realtime transfer, and the owner decides whether that matters.

## 1. Seats

| seat | binary | source | where |
|---|---|---|---|
| **RB** (BASE) | `0ec952583a99a3f3f08e56debbdcc9c3c2ddf41516f14220421964fe931ee39b` | BASE64, main before F2 (§64 stage G's BASE) | VM `~/r64/run/bins64/base-raptorpath` |
| **RC** (CAND) | `8104fa027c9dea559c9ffdb0d5630117e70ffcd97a092e73700441446771be5b` | CAND64 = BASE64 + `f2-impl` `ab9feec4` (F2 commit 8), the binary §64 guarded and main merged | VM `~/r64/run/bins64/cand-raptorpath` |

- **Pins.** Both shas are §64's `BINSHA.txt` (`docs/l1-raw/s64/s64-session.tgz`, `session/BINSHA.txt`). The launch installs them as prebuilt binaries and sha-checks each against these pins before any row; a mismatch is `ABORT-BUILD`.
- **Seat witness.** §64's witness stands: CAND's binary holds `[SYMSZ]` and `[UNFRAMABLE]`, BASE's neither (`ABORT-SEAT`).
- **No build and no engine suites.** These exact binaries passed §64's suite record (the reference-manifest HIT at `ab9feec4`) and its guard. The sha pin is the identity witness, and the session writes `SUITES-NOT-RUN plan 65: binaries sha-pinned to §64's BINSHA.txt`.
- **Scope, stated.** RC is the F2 merge commit's binary, not a fresh build of today's main. That is what a bisect needs.

## 2. Cell and rows

- **The cell.** c2-100, Realtime hint, ρ = 1, 600 MB per row, `RWM_PERF_TIMEOUT_S=200`. Instrument env as §64 stage R: `RWM_GEN=0 RWM_DIAG=1 RWM_RTOBS=1 RWM_RDIAG=1`.
- **The table: 24 slots, 48 rows** (`s65_score.plan()`).
  - Seeds 42 and 7 × 12 replicates, plan 41c's interleave, so the two rows of a slot run back to back.
  - Seed 42's pair runs RB first, seed 7's pair runs RC first. At 12 slots this is `plan64.plan()` exactly.
- **ρ = 1 witnessed on both ends:** `window_reliable: true` in each end's `perf configuration` line; otherwise `VOID-RHO`.
- **Why 600 MB and 200 s:**
  - §64's c2 Realtime rows ran at about 68 Mbit/s, so 600 MB is about 70 s.
  - That clears guard v2's 30 s tick floor (§41d) with room for a rest window of about 58 s, and matches stage G's c2 row size.
  - 200 s is plan 41d's 600 MB timeout.
- **Not yet observed:** no committed row has run Realtime at ρ = 1 and 600 MB, so the rehearsal's rows are the go/no-go (§6).
- **Void rules.** Plan 64's non-F2 voids stand. **New:** a completed row with under 30 s of `[DIAG]` ticks is `VOID-SHORT`.

## 3. What is scored

### 3.1 Slots and redraws (one row per arm per slot; at most one redraw per slot)

- **A dead BASE warm-up** (`BASE-F2-DEAD`, about 1 in 12 at ρ = 1) is redrawn once, immediately, in the same slot, at most 5 slots (24 / 12 + 3; at about 2 expected deaths, Poisson(2) ≥ 6 is 1.7 %).
  - If the redraw also dies, or the cap is spent, the pair is **dropped**.
  - A dead BASE row never enters a test, so it can never count as CAND's win.
  - **Adjacency limit:** in RC-first slots (seed 7), a redrawn RB row runs at least 215 s after its partner.
- **A non-F2 void on either arm** is redrawn once in the same slot, at most 4 slots (24 / 6). If it voids again, the pair is dropped.
- **An RC row failing the F2 reading** (§64 §2.2 items 1–4) is `RT-FAIL`: an **F2-FINDING**, printed first and sent to the owner. It is never redrawn, and its pair is not scored.
- **A DNF** is the worst rank on its own side (§47):
  - an RC DNF gives d = +L, an RB DNF gives d = −L, with L = 1 + max |d| over the finite pairs;
  - both DNF gives d = 0.

### 3.2 The two scored readouts (one-sided; the prediction is registered: RC worse)

- **HEAD (the replication):** the in-order delay mean (§41d's definition) over ticks with t ≤ 11.5 s.
  - This is the window §64's rows covered: they ended at 11.5–11.8 s.
  - On §64's pairs it read +25.2 % (Hodges–Lehmann). That is the effect this replicates.
- **REST (persistence):** the same mean over ticks with t > 11.5 s, about 58 s of each row.
- **The whole-run mean is reported, not scored.**
- **Limit, stated: the head window has no end of transfer.** On §64's 12 s rows, the head contains the final drain (the ack catching up after the source stops). On 70 s rows it is the first 11.5 s of a flow that keeps going. So the head replicates the time window, not the whole row's shape. A §64 effect that lived in the end-of-run drain would not reappear in the head; it would show up in the reported whole-run mean.

### 3.3 The margin and the test

- **TOL = 5 %, fixed.** This is the tolerance the merge guard already applies to c2's in-order clause: §64 stage G printed `tol=0.0500` (`spread_rel`'s floor, `l1ab.REL_FLOOR`, §11–§13). The data branch of `spread_rel` is not used, because it widens with BASE's noise.
- **Where the 8 % of round 1 came from, corrected.**
  - It is c2's in-order-delay A/A d80 in §41d ("New clause resolution … c2 8 %, c8 20 %") and in `guard-clause-dry.md` §11 (600 MB, v2 column: 8 / 5 / 5 % over three seeds).
  - Round 1 also said `rt64_score`'s owner flag uses it for in-order delay. That was wrong: the flag's 8 % is c2's guard-size CPU-per-GB d80 (`rt64_score.D80`), and in-order delay is not in that flag.
  - The 20 % in the same tables is c8's in-order d80.
  - **The off-Bulk design's 20 %** (`s51-design.md`, "A/A noise and resolution") is c2's in-order d80 for that design: §48b's Bulk BASE rows, 6 v 6 unpaired, a family of 4 at threshold 0.05/3. That is the power of a different, smaller, unpaired and family-corrected design on the same Bulk rows, not a measured Realtime noise level, so it is not this design's margin. It does warn that the rest's power on Realtime rows is not known; §4 brackets it.
- **Per window, per usable pair:** d = ln(RC / RB).
  - **p:** the exact one-sided Wilcoxon signed-rank p, P₀(W⁺ ≥ W⁺_obs), over all sign flips of the observed midranks of |d| (computed by exact counting), zeros dropped.
  - **h:** the Hodges–Lehmann shift, the median of the Walsh averages.
  - **U:** its exact one-sided 95 % upper bound, W₍N−c₎, c the largest with P₀(W⁺ ≤ c) ≤ 0.05 for n untied ranks.
  - Relative values: h_rel = e^h − 1, U_rel = e^U − 1.
- **The RB half-split q95 is REPORTED ONLY,** per window (Q2 ruling): U decides.
  - It uses the RESOLUTION lines' construction, computed from RB first.
  - It is exact up to 2000 splits (§64's 200). Above that, a seeded sample of 2000 is used (24 rows have 853 776 splits).
- **Window labels:**
  - fewer than 20 usable pairs (5/6 of 24) → **UNRESOLVED-N**;
  - p ≤ 0.05 and h_rel > TOL → **REGRESSION**;
  - U_rel ≤ TOL → **NO-REGRESSION**;
  - else **UNRESOLVED**.
  - More noise makes U wider, so noise never helps NO-REGRESSION. Stub C5 checks this (§5).

### 3.4 The session label (first match wins)

**A window is called clean only when it reads NO-REGRESSION.** An UNRESOLVED or UNRESOLVED-N window is open, never read as "no effect".

| head | rest | label | what it means for the merged fix | next |
|---|---|---|---|---|
| REGRESSION | REGRESSION | **BISECT** | the §64 effect replicates and persists: the merged binary delivers in order later than the pre-fix one on Realtime c2 at ρ = 1 | registered bisect (§7) |
| REGRESSION | NO-REGRESSION | **START-UP-COST** | the effect replicates in the first 11.5 s, and the rest excludes more than 5 % | to the owner: a start-up delay cost of the fix; no bisect by default |
| REGRESSION | UNRESOLVED(-N) | **REPLICATED-PERSISTENCE-OPEN** | the effect replicates; whether it persists is not decided | to the owner |
| NO-REGRESSION | REGRESSION | **LATE-ONLY** | not the predicted pattern: a steady-state regression, with the start-up window excluded | to the owner; a bisect is the owner's call |
| UNRESOLVED(-N) | REGRESSION | **REST-REGRESSION** | a steady-state regression of the fix; the start-up window is open | to the owner; bisect recommended |
| NO-REGRESSION | NO-REGRESSION | **CLOSED** | neither window is worse by more than 5 % (95 % one-sided) | §64's lean closes |
| UNRESOLVED(-N) | NO-REGRESSION | **NOT-REPLICATED** | no steady-state regression; the head neither replicates nor is excluded (its U is printed) | §64's lean is recorded as not reproduced; nothing to bisect |
| anything else | | **UNRESOLVED** | the session cannot tell (reasons printed) | to the owner; no automatic re-run |

- **Multiplicity:**
  - BISECT needs both one-sided tests, so it carries no inflation.
  - START-UP-COST rests on the head alone. It is the registered replication of a stated prediction.
  - LATE-ONLY is the unpredicted case, with its 5 % false-positive rate stated.
- **F2-FINDING** (any RC `RT-FAIL`) is printed before the label and outranks it.

### 3.5 Reported only

- **The whole-run in-order mean,** with the same statistic.
- **The shift per run order:** seed 42's pairs (RB first) against seed 7's (RC first), HL per window.
- **Early loss per row, both seats:** quinn's lost and sent packets on leg 0 at the client's last `[DIAG]` tick with t ≤ 2 s (`qlp=` in `[DIAG]`, cumulative). The dead BASE rows are listed too.
  - **Why:** the live BASE rows are the ones whose `WindowStart` survived, so selection on early loss is the obvious bias to look at.
- **Other readouts:** goodput, CPU/GB, `lag_E_cli`, `lag_E_srv`, `rtp_floor_p0` (paired count worse, signed-rank p, HL).
- **Counts:** `BASE-F2-DEAD65` (redrawn alive, dead twice, not redrawn), redraws, dropped pairs.
- **Limit, stated:** pairing removes drift between slots, not the survivor selection of the RB rows. The early-loss readout is the check on it.

## 4. Pair count from power (design computation, done before any §65 row)

`docs/derivations/s65-registration/s65_power.py` and its `.out`.

**Noise models.** Each model's pairs are resampled with replacement after centring on their own HL, so each carries spread only. 4000 draws per cell, seeded.
- **HEAD:** §64's 11 stage R pairs, the head mean taken from the rows' own logs (sd of d 0.265, HL +25.2 %).
- **LONG:** §64 stage G's 12 c2 pairs at 600 MB, Bulk, paired by slot (sd of d 0.024, HL −2.4 %). This stands in for REST.
- **The rest's spread on Realtime rows is not known** (no Realtime 600 MB pair exists), so it is bracketed by two sensitivity rows:
  - **LONGx4:** LONG's residuals × 4, sd about 0.10;
  - **HEADasREST:** the rest as noisy as §64's 12 s head pairs, the pessimistic end.

| model | pairs | P(REGRESSION) at +25.2 % | P(REGRESSION) at +12.6 % | no shift: mean U | no shift: P(NO-REGRESSION) | no shift: P(REGRESSION) |
|---|---|---|---|---|---|---|
| HEAD | 12 | 0.748 | 0.357 | +12.2 % | 0.217 | 0.056 |
| HEAD | 18 | 0.898 | 0.484 | +9.4 % | 0.299 | 0.044 |
| HEAD | **24** | **0.959** | **0.603** | +8.2 % | 0.351 | 0.056 |
| LONG | 12 | 1.000 | 1.000 | +1.4 % | 1.000 | 0.000 |
| LONG | 18 | 1.000 | 1.000 | +1.1 % | 1.000 | 0.000 |
| LONG | 24 | 1.000 | 1.000 | +1.0 % | 1.000 | 0.000 |
| LONGx4 | 12 | 1.000 | 0.979 | +5.8 % | 0.362 | 0.037 |
| LONGx4 | 18 | 1.000 | 0.997 | +4.6 % | 0.510 | 0.034 |
| LONGx4 | 24 | 1.000 | 0.998 | +3.9 % | 0.621 | 0.025 |
| HEADasREST | 12 | 0.755 | 0.344 | +12.4 % | 0.199 | 0.050 |
| HEADasREST | 18 | 0.897 | 0.477 | +9.4 % | 0.295 | 0.051 |
| HEADasREST | 24 | 0.964 | 0.591 | +7.8 % | 0.389 | 0.050 |

- **Choice: 24 pairs (48 rows), fixed, never extended after rows are read.**
  - It is the only count that replicates the seen size with ≥ 0.9 probability (18 gives 0.898).
  - It gives the most power at half the size (0.60).
  - The step from 18 to 24 costs about 15 min of VM.
- **What the table also says, stated so that no label is a surprise:**
  - With no true shift, the head reaches NO-REGRESSION only about 1 time in 3 (mean U +8 %), because §64's head pairs are noisy.
  - So under the null the likely labels are NOT-REPLICATED (head unresolved, rest clean) or CLOSED, not CLOSED alone.
  - **The rest's power to detect is high across the bracket:** ≥ 0.96 at the seen size even at the pessimistic end. Its power to EXCLUDE is not: P(NO-REGRESSION) with no shift is 1.0 (LONG), 0.62 (LONGx4) or 0.39 (HEADasREST) at 24 pairs. So CLOSED and NOT-REPLICATED need a rest closer to Bulk's spread than to §64's head.
  - The head model comes from 12 s rows. A head without the drain (§3.2) may be less noisy.

## 5. The reference scorer and its path exercise (done, before registration; labelled)

- **Scorer:** `tools/l1/s65_score.py` (this branch), sha256 `8da41441c5862323…`. It is the registered statistic, pure and env-free, and the harness worker wires it in unchanged (§8).
- **Path exercise:** `s65_path_exercise.py` (`dba77e3283e44833…`) and `s65_path_exercise.out` (`28fc18740b45014f…`). Two runs were byte-identical, and it ends `PATH EXERCISE PASS`.
- **A. §64's stage R rows,** with the plan 65 fields computed from their own logs and the tick floor passed as 0. It reproduces `s64-lean`:
  - RB whole-run q95 0.1930 over 200 splits;
  - whole-run 9/11;
  - `lag_E_cli` 8/11, `lag_E_srv` 7/11, `rtp_floor_p0` 9/11;
  - the dead RB s42 r4 drops its pair.

  The head reads 9/11, p 0.021, HL +25.2 %, so the code prints `REPLICATED-PERSISTENCE-OPEN` (the rest has one pair, UNRESOLVED-N).

  **This is not evidence and is kept out of every result claim.** It is a third test on the same 11 pairs, after the sign test and the rank p, and the window was chosen after seeing them. It is the source of the prediction (+25 %, RC worse) that §65 tests on fresh rows, nothing more.
- **B. The same rows under the registered 30 s floor:** all 23 completed rows are VOID-SHORT, giving UNRESOLVED.
- **C. Synthetic 24-pair ledgers:**

  | case | label |
  |---|---|
  | quiet null | CLOSED |
  | +15 % in both windows | BISECT |
  | +15 % in the head only | START-UP-COST |
  | +15 % in the rest only | LATE-ONLY |
  | **stub:** pairs as noisy as §64's head (±50 %), no shift | UNRESOLVED; neither window reads NO-REGRESSION |
  | noisy head, quiet rest | NOT-REPLICATED |
  | +15 % head, a rest as noisy as §64's head | REPLICATED-PERSISTENCE-OPEN, never "not a steady-state one" |
  | a noisy head, +15 % rest | REST-REGRESSION, never "without the start-up one" |
  | a resolved +3 % | CLOSED |
  | five slots dead twice (19 pairs) | UNRESOLVED-N in both windows |
  | a dead RB redrawn alive | slot scored |
  | an RC `RT-FAIL` | F2-FINDING, pair dropped |
  | an RC DNF | d = +L |

  At ±20 % pair noise, 24 pairs already bound U under 5 %, so that is a legitimate NO-REGRESSION, not a stub failure.

**Read now on §64's 11 pairs (item 4 of the ruling; POST HOC, reported, from the path exercise's A section):**
- **By run order:**
  - seed 42 (RB first): head HL +26.6 %, RC worse in 5 of 5;
  - seed 7 (RC first): head HL +23.8 %, 4 of 6.
  - On these rows the lean does not depend on which seat runs first.
- **Early loss by t = 2 s (quinn lost / sent on leg 0):**
  - live RB rows 598–754 lost of about 27 000 sent (median 685);
  - RC rows 595–727 (median 663);
  - seed 7 runs higher than seed 42 on both seats (medians).
- **What this readout can and cannot see:**
  - It sees the transfer's first 2 s of loss per seat.
  - It cannot see the selection that matters: on a live row the warm-up (about 0.05 s) ends before the `[DIAG]` clock's first ticks, and one lost `WindowStart` is invisible among about 27 000 packets.
  - The dead RB s42 r4 is not comparable. Its `[DIAG]` clock is the warm-up phase (every tick `src=0`, `cum=1/0/1`, one object never acked), and its 4 lost of 22 sent by 2 s is the only heavy early loss in the ledger. That fits selection acting on the first packets, which the live rows' readout cannot show.
  - So the early-loss readout neither shows nor rules out survivor selection. It is kept as reported context.

## 6. Rehearsal (`LD_PLAN=65 LD_REHEARSE=1`) must show

| check | NO-GO consequence |
|---|---|
| both prebuilt binaries match their pins; the seat witness strings | ABORT-BUILD / ABORT-SEAT: stop |
| every tools test passes on the VM; `TOOLS-TESTS-OK` and `TOOLS-MANIFEST.sha256` written | ABORT-TESTS: fixed on main, re-rehearsed |
| **RC s42 r1 at 600 MB:** completes inside 200 s, RT-OK, items 1–4 pass, `window_reliable: true` on both ends, ≥ 30 s of ticks; `inorder_mean`, `inorder_mean_head`, `inorder_mean_rest`, `qlost_2s` computed | a missing field or witness is a harness fault. A DNF or a short row means §2's size or timeout is wrong: amend before launch. A failed item is an F2-FINDING: no launch |
| **RB s42 r1 at 600 MB:** LIVE with the same fields and witnesses, or `BASE-F2-DEAD` | any other label is a harness fault. A dead RB leaves BASE's 600 MB completion unobserved: accepted, and reported |

A rehearsal row is never re-run and never scored.

## 7. After the label

- **BISECT:**
  - F2 commit 7 (`626fdf9a`) is docs-only, so its engine is commit 6's. Commit 8 (`ab9feec4`: the allocation-free unframe and the `offer` rewrite in `net/mod.rs` / `receiver.rs`; the coordinator's reading in `f2-guard-registration.md` §1.1) is the only delivery-path change after commit 6.
  - The next registration builds commit 7's binary (BASE64 + `f2-impl` up to `626fdf9a`) and runs it as the base against this RC, with this design.
    - Commit 8 worse than commit 7: the cause is `offer`.
    - The two equal: the cause is in commits 1–6, and the bisect continues there.
  - It needs one build and has its own cost line.
- **START-UP-COST** or **LATE-ONLY:** to the owner with both windows' numbers and the run-order and early-loss lines.
- **CLOSED** or **NOT-REPLICATED:** the status records the outcome; nothing to bisect.
- **UNRESOLVED:** the printed reasons go to the owner. Any further session needs its own registration; the pair count is never extended within this one.
- **F2-FINDING:** diagnosed before anything else.

## 8. Harness change (Sonnet worker; tools only, on main, which has harness64)

The worker starts from `s65-design` and does not change `s65_score.py`. If a change is unavoidable, the path exercise must reproduce `s65_path_exercise.out` byte for byte, and the pins are amended.

1. **`plan64.py`:**
   - **Tables:** `plan(smoke=False, plan_id="64")`. Plan `"65"` returns `[(CELL, "def", a, sd, int(rp)) for a, sd, rp in s65_score.plan()]` (48 rows); plan 64's table is unchanged. `count` follows.
   - **Sizes:** `R_SPECS = {"64": (100000000, 150), "65": (600000000, 200)}`, with `BYTES_R, TIMEOUT_R = R_SPECS["64"]` kept (`test_harness64.py` line 153), and a CLI `plan64.py rspec <plan>` that prints `BYTES TIMEOUT`.
   - **`rt_row`:** `row.update(s65_score.row_fields(cli))`, which adds the head, rest and early-loss fields, for both plans.
   - **`rt_status`, under `LD_PLAN=65` only:** a LIVE or RT-OK non-DNF row with `tick_span_s` < 30 becomes `VOID-SHORT`.
   - **`replace <ledger> <cell> <arm> <seed> <rep> [plan]`** (default `64`, unchanged behaviour). Under `65`, a one-row slot is redrawn when its row is:
     - in `R_REPLACEABLE` or `VOID-SHORT`, while fewer than `s65_score.VOID_CAP` such slots are redrawn; or
     - `BASE-F2-DEAD`, while fewer than `s65_score.DEAD_CAP` dead slots are redrawn.
   - **`BUDGET_S`:** `"65": 13500` and `"65r": 1800` (§9).
   - **`smoke`:** gains §6's plan 65 criteria.
2. **`loopdiag_battery.sh`:**
   - **Plan list:** line 96 accepts `65`; `P64=1` for plans 64 and 65, plus a `P65` flag.
   - **Row size:** `run_one`'s `P64` line takes the bytes and timeout from `plan64.py rspec "$PLAN"`, with `tmo_env` after `INSTR_ENV`, so plan 64 still runs 100 MB at 150 s.
   - **Table:** the `plan64` function reads `plan64.py plan [smoke] <plan>` and asserts `plan64.py count <plan>` rows (24 for 64, 48 for 65).
   - **Run path:** the run block's `if [ "$PLAN" = "64" ]` becomes 64 or 65. `run_slot64` passes `$PLAN` to `replace`, and the redraw runs immediately.
   - **Copy list:** `s65_score.py` joins the file-copy list (line 84).
3. **`place38_parse.py:937`:** `LD_PLAN in ("64", "65")`.
4. **`loopdiag_parse.py`:** `smoke65r` → `plan64.smoke(path, plan="65")`, `check65r` → `plan64.check(path, plan="65")`, `score65r` → `s65_score.score`.
5. **`loopdiag_run_all.sh`, a stage-R-only session for `LD_PLAN=65`:**
   - **Binaries:** `LD_BIN_PREBUILT_BASE`, `LD_BIN_PREBUILT_CAND`, `LD_BIN_SHA_BASE` and `LD_BIN_SHA_CAND` are all required (`ABORT-BUILD`), installed as plan 64's prebuilt path does.
   - **No cargo:** no build, no suites stage, no `SRC` / `BASE_SRC` checks; the `SUITES-NOT-RUN` line (§1).
   - **Witness and tests:** the seat witness strings; tools tests, or `LD_PRIOR_TOOLS` reuse.
   - **Budget guard,** beside plan 64's `W64` block: refuse with less than `plan64.py budget 65` (or `65r`) seconds before `LD_HARD`.
   - **Stage:** `stage rt65 65` only (`smk=smoke65r; scr=score65r; chk=check65r`). A rehearsal stops after the smoke.
   - **Hard limit:** `LD_HARD` = launch + 4.5 h, or rehearsal + 1 h.
   - **Summary:** `$RUN/score.txt` carries the `VERDICT65`, `TEST65` and `F2-FINDING65` lines.
   - **Copy list:** `s65_score.py` joins the copy list (line 121).
6. **`harness_paths.py`:** stub paths for 65: rehearse, full, base-dead-redrawn, base-dead-twice, short-row-void, rc-rtfail, rho-one-end.
7. **Tests:**
   - `test_harness64.py` keeps every plan 64 pin and adds plan 65's table, specs and budgets.
   - A new `test_s65_score.py` turns cases C1–C11 into unit tests, and asserts `s65_score.plan(12)` equals `plan64.plan()`'s slots.
   - `test_stubrun.py` gets a case65.
8. **Done when:**
   - every python tools test passes on the host, and the stub runs for 64 and 65 pass;
   - `rt64_score.py` on the committed §64 `rt64/ld.log` still prints `rt64/score.txt` byte for byte. That is true on this branch: `loopdiag_parse.py score64r`, line endings normalised, `cmp` equal;
   - the path exercise prints `PATH EXERCISE PASS` with its `.out` unchanged.
   - Checked on this branch with `s65_score.py` added: `test_harness64.py` (76/0), `test_harness_paths.py`, `test_harness60.py`, `test_pool54.py`, `test_pool57.py`, `test_binid.py` and `test_lagdiag2_digest.py` pass. `test_stubrun.py` was not run here.
   - **Report:** the sha, and any rule the worker had to settle.

## 9. Cost

- **Typical: about 80 min of VM.**
  - 48 rows × about 75 s (70 s of transfer plus §64's 3 s overhead per row) is 60 min;
  - about 2 dead BASE rows at the full 215 s, plus their 75 s redraws, is about 10 min;
  - the smoke (about 4 min) and the tools tests (8 min, or reused).
- **Rehearsal: about 15 min.**
- **Worst case (budget):**
  - Launch: (2 smoke + 48 + 4 void redraws + 5 dead redraws) × 215 s + 480 s tools + 300 s scoring = 13 465 s, about 3.7 h. Budgeted at 13 500 s; `LD_HARD` = launch + 4.5 h.
  - Rehearsal: 480 + 2 × 215 + 300 = 1210 s, budgeted at 1800 s; `LD_HARD` = rehearsal + 1 h.
- **Host cargo paused** for the session.
- **Order:** the harness change merges, then the rehearsal, then a pin amendment, then the launch.
