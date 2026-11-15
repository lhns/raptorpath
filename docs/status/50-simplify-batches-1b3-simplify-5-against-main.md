## 50. Simplify batches 1b–3 (`simplify-5`) against main — the guard session — pre-registration (before any VM contact)

**What is tested.** On top of §49's merged batch 1, `simplify-5` stacks three commit series, each arm in its own commit with an `archive/RWM_*` tag:
- **`simplify-3`:** `RWM_PLACE_HOL` archived; 16 dead `RWM_PLAIN_RS=1` test-env lines removed.
- **`simplify-4` (Group C):** `RWM_HOLDDOWN_Q`, `RWM_REFRESH_FLOOR_US`, `RWM_DERIVED_SWEEP` removed.
- **`simplify-5`:** `RWM_PLACE_T_DERIVED` archived (user ruling: archive the code, keep the formula open); paper corrections (`RWM_PLACE_WDIV_DERIVED` never ran; the SRC_BP re-test clause restored; units of T and w_div); a sensitivity bounding test for T = 0.15 over T ∈ [0.03, 0.68] and ref ×0.5–×2.
- Every deleted gate resolved off on the default path. **The expected behaviour change is none.** `git diff BASE CAND -- raptorpath/src raptorpath/tests` = 36 files, +343 / −2 921. **The guard asks whether the deletions are free.**

**Design: §49's exactly.**
- **Plan:** `LD_PLAN=41c`, `LD_P46_SUITES=1`. Suites on CAND's tree first (REAL failure ⇒ `ABORT-TESTS`), then the §41c guard.
- **Statistic:** `rank-minp`, N_PERM 10 000, seed 4141, one-sided, PASS iff p > 0.05.
- **Clauses and design:** 26 clauses; BASE n = 12 and CAND n = 12 at c1s-400, c1d-400, c2-100 and c8-100 default (96 rows plus the 2-row smoke).
- **Rules:** §41c's DNF and replacement rules; an incomplete design reads NO VERDICT. No §46 stage.

**Readouts.** The verdict block, the §21 readings, the tail line, QCWND-BELOW-BDP, and on the host the resolution table.
- **Reported:** the `[ETA]` sender line loses `t_eff`/`t_cold`/`t_n` (parsers read absent as None). The `[HOLD]` line loses the hold-down arm's fields; `evals` and the class fields stay.
- **Known:** the §47 quinn collapse may appear in either arm's c1 rows (§49 had one in CAND, §48b three in BASE). Reported, never scored.

**Prediction.** PASS.

**Pre-flight (VM, fresh target):**
- **`simplify-5` `07afc9b7`:** `cargo check --all-targets` 0 errors; lib gates 14/0, scheduler 88/0, net 275/0, transport 32/0; integration `place_arms`, `sigma_diag`, `sigma_candidates`, `tlag`, `holeclass`, `recovery_bench`, `eta_reachability` all pass.
- **After merging main (`3ac3a209`, the bounding tests):** the full lib suite is re-checked before pinning (several bounds pin source text the simplify series edits).

**Decision rule.**
- **PASS:** `simplify-5` merges to main (`--no-ff`), with status §1's on-list and the `[GATES]` pin already updated in the branch.
- **FAIL:** no merge; the failing clause, its size and the resolution table go to the user; bisect by arm commit.
- **NO VERDICT:** re-run. **ABORT-TESTS:** fixed on the branch and re-run.

**§50 amendment 1 (before VM contact, advisor review).**
- **Trees:** BASE = main `318a881212acef7441fd1e086ef4aeb1db0e05b2` (`raptorpath/src` and `raptorpath/tests` identical to `3ac3a209`, which carries the bounding tests). CAND = `simplify-5` `d5bd282af072eeff128d8bc0919b458d335eade7` (main merged in). BASE-vs-CAND over `raptorpath/src raptorpath/tests raptorpath-math raptorpath-wasm Cargo.*`: 36 files, +343 / −2 921, the deletions alone.
- **Merged-tree pre-flight (VM, fresh target):** `cargo check --all-targets` 0 errors; **full lib suite 586 passed / 0 failed / 9 ignored.**
- **Parser chain:** the post-simplify `[ETA] site=sender` line (no `t_*`/`hol_*`) parses through `place38_parse.eta_last` (cold_r, cold_ge, ref_fl, w_cold, w_n, w_calls, per-leg e_p50/e_p90; w_mv None). None of the removed `[ETA]`/`[HOLD]` fields are read anywhere in plan 41c's chain (grep).
- **The one live-path refactor** (`1b00ede5`: `patience_floor_us` deleted; sender and receiver call `hole_nack_refresh` / `tail_sweep_timeout_us` directly): `the_shipped_recovery_clocks_are_pinned_absolutely` pins the shipped values (tail sweep 2·srtt clamped to [25, 100] ms incl. `TAIL_SWEEP_MIN_US` = 25 000 at c1's 2 ms srtt; the hole-refresh table at the same srtt points as the deleted test). The guard cannot resolve recovery at c8 (goodput d80 8–12 %), so this pin is the evidence for "unchanged".
- **Correction:** "collapse rows are reported, never scored" is wrong. §41c ranks every LIVE row by its own values ("slow rows, reported only" means *listed*, not excluded). Registered reading: no row is dropped or relabelled; on a FAIL the user gets each c1 row's QCWND-below-BDP share beside the failing clause.

### 50. Result — `GATE-41c PASS`, p = 0.1716; merged

Session (`LD_PLAN=41c`, `LD_P46_SUITES=1`), CAND `simplify-5` `d5bd282a`, BASE main `318a8812`, launched 2026-11-12T17:32:35Z, `LD-ALL-DONE` 18:43:54Z. Ledgers: `docs/l1-raw/simplify50/` (scrubbed).
- **Suites (CAND's tree):** release 1119 passed / 0 failed, debug-witness 978 / 0, doc ok, wasm 36, every `tools/l1` test ok. BASE's binary hashes to `6bcb46e8…`, identical to §48b's BASE (only test modules changed since).
- **Guard:** 96/96 LIVE, 0 replaced slots, no DNF or crash rows, no slow rows, ABORT-TOKENS none. **min-p (true) 0.0089 at c1s `lag_E_cli` (x = +15.05 %); p = 0.1716 → PASS.**
- **Worse-side clauses as printed:** c1s `lag_E_cli` +15.05 % (p 0.0089), c1s `rtp_floor_p0` +2.21 % (p 0.013), c2 `cpu_srv/GB` +1.25 % (p 0.14), c1d `lag_E_cli` +8.2 % (p 0.35); goodput within ±0.9 % in every cell. The c1s client-lag excess sits on 4–5 CAND rows (`lagC` 1 574–2 371 against BASE 855–1 273), not across all pairs; contention was low (io avg10 ≈ 0 on most rows). **Carried to §52 as a watch item**, reported only.
- **Tail:** no row below 0.5 × its cell median; lowest CAND row c8 s7 r3 at 0.878 ×.
- **Resolution (`resolution41c`, seed 0):** goodput d80 c1s 8 %, c1d 8 %, c2 2 %; CPU/GB d80 c1s 2–3 %, c1d 2.4–5 %, c2 5 %.
- **Merge checks:** the merged tree's engine bytes (`raptorpath/src`, `tests`, math, wasm, `gf256`, `Cargo.*`) are identical to CAND's; `lib.sh` identical; all 30 `tools/l1` tests (with and without `LD_BASE_SHA`), `test_topo_netem` and the fast stub run pass on the merged tree. All five new `archive/RWM_*` tags are on github.
- **Debt carried:** the kept hole-class `[HOLD]` gauge (`evals` + class fields; delete or keep: a later ruling); `qnative_recovery_round_us` may now have no non-test user; stale comments named by the workers (`recovery_clock.rs:8`, `docs/research/literature-crosscheck.md`).

**Build-cost measurement (ADR-0076 Q4, reported only; VM, main `36c54292`, fresh target, `cargo build -p raptorpath --release --tests --timings`):** wall **746 s**, 351 units, Σ unit time 5 999 s. **The 86 integration-test targets take 5 228 s (87 %)**; the `raptorpath` bin 173 s, the lib 56 s, the largest dependency build script (aws-lc-sys) 74 s. Each test target links separately under `[profile.release] lto = true, codegen-units = 1`. So the release suite's build is dominated by per-test-binary LTO links, not by the engine. Candidate levers for ADR-0076 phase 5 (each its own decision, measured again): a test-build profile without LTO for the suites (changes what the suites compile, not the shipped binary; needs a stated equivalence argument), and consolidating integration tests into fewer binaries (minding the process-global hazards ADR-0076 lists: 3 global allocators, 17 `set_var` callers against 16 `OnceLock` statics, 2 signal senders). Report: `docs/l1-raw/build-timings/cargo-timing-release-tests.html`.
