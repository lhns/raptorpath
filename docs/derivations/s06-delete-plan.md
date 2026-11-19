# S06 delete: implementation plan (the taper copy branch)

Status: PLAN for a Sonnet worker. Decided by the owner on 2026-11-19 (ADR-0079; status "§51 block 51 Result, Addendum 3"). Do not relitigate it.
- **Base:** `github/main` at `d1e952cf`. Line numbers below are at that commit; every anchor also quotes text, so match on the text.
- **Paper:** §3.2 is already amended on `s06-delete-docs` (proactive slots are coded; retransmission is reactive only). The engine change makes the paper true.
- **Not yours:** `docs/status.md`, `docs/derivations/law-ledger.md`/`.yaml` and the roadmap. The coordinator updates them with the merge.

## 0. Prerequisite: `c8-gauges` lands first

The shed-conservation test (§3, T3) needs the split shed counter `shed_slot=` / `shed_nack=`, which branch `c8-gauges` adds. That branch is not on `github` yet. It also edits the same proactive-slot block (`ptgt=` / `pmax=`).
- **§1 (the deletion) does not start until `c8-gauges` is on main.** Base the branch on main after that merge. Do not guess its code, and do not re-implement the split counter here.
- **Before that, only T4 may land** (model-only, green on main; §3). T1 and T2 may be written, but their red runs are recorded on the post-`c8-gauges` main, since that branch edits the block they exercise.
- If `c8-gauges` has not merged when you start, land T4 and stop with a note.
- `ptgt=` / `pmax=` count copies of one seq. After this change they read 0 forever. Keep them printing (see §2), and say so in the report.

## 1. The engine change (`raptorpath/src/net/emit_source.rs`)

### 1.1 Delete the copy branch

Inside `while st.repair_debt >= 1.0 && st.encoder.window_size() > 0 {` (:742), the block `let (correction_sym, correction_kind) = { ... };` (:749–854) becomes coded-only:
- **Remove** the comment `// P_lost-based correction symbol decision:` … `// Otherwise, generate a new repair symbol (FEC).` (:745–748).
- **Remove** `let now = now_us();` and the `super::p_lost_inputs(&*ctx.scheduler)` call (:750–752).
- **Remove** `use_retransmit`, `retransmit_seq`, the `oldest = st.retransmit_buffer.first()` read, the `p_lost` evaluation, the `rand::random::<f64>() < p` draw, and the whole slot-site shed block inside it, from `// δ-honest shed: a candidate older than D(δ)` to its closing braces (:754–799).
- **Remove** the `taper_copy.fetch_add` block (:801–806) and the `st.mpd_plost_retx += 1` block (:807–811).
- **Remove** the `kind` selection (:812–817) and the `if use_retransmit { st.sent_store.get(&retransmit_seq) … }` arm of `sym` (:818–826).
- **Keep, unchanged:** the trailing-span arm `else if let Some((a_star, delta)) = span_params { … }` and the leading-window `generate_repair()` fallback. Both become the whole `sym` expression: `let sym = if let Some((a_star, delta)) = span_params { … } else { st.encoder.generate_repair() };`
- **The block returns** `(sym, crate::monitor::stats::CorrectionKind::Coded)`.
- **Everything after the block is unchanged:** placement, `send_symbol`, `charge_in_flight`, `symbols_sent` and `record_correction(correction_kind, sent)`.

### 1.2 Shed accounting stays consistent

- The shed law keeps one call site: `sender_phases.rs:846–869` (`if pol.shed_on { if st.shed_seqs.contains(&seq) { continue; } … shed_allowed(…) … }`). Do not touch it.
- Its inputs `st.shed_deadline_us_live` and `st.shed_budget_frac` are refreshed in the span block (`emit_source.rs:630–640`, `if pol.shed_on { st.shed_deadline_us_live = shed_deadline_us(…); st.shed_budget_frac = …residual_loss_after_fec(…) }`). That is outside the deleted block, so the NACK site keeps reading live values. Keep it.
- `shed_total`, `shed_denied` and `shed_seqs` are now written only by the NACK site. The `[SPAN] … shed=a/b` gauge is unchanged.
- After `c8-gauges`: `shed_slot` has no producer left. Keep the field and its print (it reads 0), and keep `shed_nack`. The identity `shed_total == shed_slot + shed_nack` becomes `shed_total == shed_nack` (T3 asserts it).
- Remove `shed_allowed` from the `use super::{ … }` list (:41). It has no other use in this file.

### 1.3 Dead code the deletion leaves (separate commit, same branch)

| item | call | why |
|---|---|---|
| `p_lost_inputs` (`net/channel_set.rs:63–73`) | **remove** | its only caller was the branch |
| `saturated_channel_keeps_the_p_lost_inputs` (`net/tests.rs:4876–4884`) | **remove** | pins the inputs of a deleted decision |
| `p_lost_rttvar_is_a_tenth_of_srtt` (`net/tests.rs:6076–6092`) | **remove** | same |
| `chi_and_p_lost_rttvar_stand_ins_differ_by_the_declared_ratio` (`net/tests.rs:6094–6117`) | **keep the χ half**: the two `completion_exposure` assertions and the `sender_phases.rs` source-text pin. **Drop** the `channel_set.rs` `(srtt,srtt*0.1,p.estimator.loss_rate())` pin. Rename it to `chi_rttvar_stand_in_is_an_eighth_of_srtt` | the 0.1 stand-in no longer exists |
| the ε slot of `retransmit_buffer: SeqRing<(u64, f64, u32)>` (:89) and its `worst_eps_estimator` read per source symbol (:498–505) | **remove**, making it `SeqRing<(u64, u32)>` | write-only after the deletion. Destructure sites to update: `net/mod.rs:2789`; `sender_phases.rs:826, 851, 888, 956`; `net/tests.rs:3487, 3492` |
| the comment `// Add to retransmit buffer for P_lost-based retransmit decisions.` and "so the P_lost retransmit branch never fires" (:493–497) | **reword**: the buffer feeds NACK, shed and the tail sweep | stale |
| `net/mod.rs:1043` "the `p_lost` taper and the end-of-stream tail sweep" | **reword**: "the end-of-stream tail sweep" | stale |
| `monitor/stats.rs:347–356` (`taper_copy` doc) | **reword**: "always 0 since ADR-0079; kept so `[DIAG] taper=` parses" | stale |
| `diag.rs:168` and `emit_source.rs:181–182` (`mpd_plost_retx` docs) | **reword** the same way | stale |

`raptorpath::control::fec_rate::p_lost` (the re-export, `control/fec_rate.rs:737–738`) **stays**: `tests/gate_suite.rs`, `tests/ablation_bench.rs` and `tests/common/gate_harness.rs` import it. No engine decision reads it after this change.

The re-reads in §1.3 change no behaviour; the ε read is a per-symbol cost removed, so run the host ns bench (§5).

## 2. Every reader of the copy counters

The fields **keep printing, at 0**, so every parser stays stable. No field is removed.

| reader | what it reads | change |
|---|---|---|
| `monitor/stats.rs:356` `FecStats::taper_copy` | the ungated copy count | keep the field, never incremented; doc as above |
| `emit_source.rs:182` `SenderState::mpd_plost_retx` | the DIAG-gated count | keep the field, never incremented |
| `net/diag.rs:879–894` `mpr[… plost={} taper={} …]` | both | unchanged format; both print 0 |
| `raptorpath/tests/holeclass_reachability.rs` clause 6 (:20–23, :182–196) | `taper=` == `plost=` | keep the equality and the "every `mpr[` line has `taper=`" check; **add** `assert_eq!(taper, 0)`; reword the clause doc to "the taper copy is deleted (ADR-0079): both read 0" |
| `tools/l1/s51_score.py:44, 118–164` (`_TAPER`, `r_consumed = (cod + taper) / src`) | consumed r | unchanged; on CAND `taper` = 0, so `r_consumed` = cod / src, which is still the consumed r |
| `tools/l1/holeaudit_report.py:125, 164` (`WASTE SPLIT: taper_copy=`) | waste split | unchanged; reads 0 |
| `tools/l1/holeaudit_battery.sh:45` | comment | unchanged |
| `tools/l1/stage3_parse.py:50, 192, 509` (`coded_share = cod / (src + cod)`) and `s51_score.py`'s `coded_share` | coded share | unchanged; on CAND it rises, because `cod=` now carries the former copy slots. That is the effect, not a parser change |
| `tools/l1/stubrun_gen.py:860`, `test_s51.py:253–263`, `test_snap53.py:40–44` | synthetic fixture lines | unchanged: they are inputs, not output pins |
| `[DIAG] retx=` and `total_copy_symbols` (`diag.rs:445`) | all source copies | taper copies leave both. `CorrectionKind::SourceCopy` keeps its other producers (`sender_phases.rs:1096`, `net/mod.rs:3478`), so the variant and the `stats.rs:512` unit test stay |
| `ptgt=` / `pmax=` (from `c8-gauges`) | copies of one seq | keep printing, at 0 |

No reader of `plost=` / `taper=` exists in `raptorpath-wasm`, `raptorpath-visualizer` or `raptorpath-core`.

## 3. Red-first tests

Each test is committed first and **run red on main**. Record the red output (counts) in the commit message. A zero-copy test that is green on main proves nothing: `holeclass_reachability` reads 0 copies over c3 loopback, because ε̂ at send is near 0 there.

**Preconditions for every engine test, each with a witness:**
- Warm ε̂ with seeded i.i.d. loss of about 2.5 %, using L0 netem on the client: `RWM_L0_NETEM=custom:<rate>;<ow_ms>;<jit_ms>;<ge_p>;<ge_q>` plus `RWM_L0_SEED` (the `CLEAN_RATE` / `C3` pattern in `tests/common/loopback.rs:422–455`).
- Run at Auto, so that rr > 0.
- Use `RWM_DIAG=1`.
- **Witness:** proactive slots > 0 and ε̂ > 0 on the CAND run, read from the log, not assumed.

**A slot counter is needed.** `cod=` and `record_correction(Coded)` also count the reactive leading-window repairs (`sender_phases.rs:1156`), so "coded == slots" cannot be read from them.
- Add an ungated `FecStats::proactive_slots: AtomicU64`, incremented once per iteration of the `while st.repair_debt >= 1.0 …` loop.
- Print it as ` pslot=<n>` right after ` taper=<n>` inside `mpr[…]`. The parsers are key-based (checked: no positional reader of `mpr[` fields in `tools/` or `raptorpath/tests`).
- Add it in the red-first commit, so main's red run can read it.

**T1. `no_taper_copy_with_slots_on_either_seat`** (new `raptorpath/tests/s06_no_copy.rs`, host loopback, N = 1 and N = 2).
- Seats:
  - reliable: `Transfer` / `lossy_run` (always passes `--window-reliable`);
  - evicting: Realtime without `--window-reliable`. Either add a `reliable: bool` to `Transfer` (default true), or use `in_process_loopback(hint, false, …)` (`loopback.rs:715`).
- Dial points: the nine preset triples {0.98, 1, 1.02}·δ_p at Bulk (0.005), Auto (0.5) and Realtime (50), set with `RWM_DELTA=<v>` on the Auto carrier on both ends, as the §51 sweep did (`tools/l1/plan51.py` `ARM_TABLE`: BM 0.0049, BP 0.0051, AM 0.49, AP 0.51, RM 49, RP 51), plus the three named hints. Read `[GATES] RWM_DELTA=` on both ends as the wiring witness.
- Points where `pslot=` is 0 (the Bulk triple: rr = 0) assert `taper=` == 0 only. That is not a step: the share is undefined at zero slots.
- Run the nine points on the reliable seat at N = 1. Run the seat and N = 2 variants at Auto (0.5) only, to bound the runtime.
- **Assert, absolute:** `taper=` == 0 and `plost=` == 0 on the last `[DIAG]` at every point, and `pslot=` > 0 at every point off the Bulk triple.
- **Red on main:** `taper=` > 0 at Auto on the reliable seat. Record the value.

**T2. `stalled_blocker_recovered_by_nack`** (same file, reliable seat, N = 1).
- Drop k's original and its first resend, with a test seam under `test-seams`, as `TEST_DROP_WINDOW_START` does (`transport/io_owner.rs:61`).
- **Assert:**
  - NACK(k) ≥ 1 within the time threshold plus one hole-refresh period;
  - NACK(k) ≤ ⌈T / SRTT_min⌉ + 1 over the stall T;
  - `taper=` == 0.
- Read NACK(k) from a per-seq trace under the same seam. Do not infer it from `retx=`.
- **Red on main:** copies of k > 0, i.e. `taper=` grows during the stall.

**T3. `shed_set_conserved_without_the_copy_branch`** (evicting seat, paced, seeded loss, small D(δ) at Realtime; needs `c8-gauges`).
- **Assert on CAND, absolute:**
  - `shed_slot=` == 0;
  - `shed_nack=` > 0 (witness);
  - `shed_total == shed_nack`.
- **Assert, per seq:** every named hole older than D at a judged gap batch or a tail sweep is shed within that batch while the budget allows.
  - Record (seq, age, D, site, verdict) in a `test-seams` trace at the NACK site, and assert on absolute seqs.
- **Witness:** `shed_denied` increments at least once with the budget set to refuse. Use a seam that forces `shed_budget_frac = 0`. If none exists, add it under `test-seams`.
- **Red on main:** `shed_slot=` > 0. Record it. That count is the slot-time trigger ADR-0079 gives up.
- **If main reads `shed_slot=0`** on this setup, the slot site never shed first there, and T3 is not red-first. Lengthen the stall: suppress gap reports past the blocker (no arrivals above the frontier) for longer than D and shorter than the tail-sweep timeout, the one interval where only the slot site can shed (derivation §4.1.2). If it still reads 0, do not weaken the test: keep the CAND assertions, report main's 0 with the setup, and stop for the coordinator.

**T4. `raptorpath-wasm`: `model_retransmit_is_reactive_and_bounded`** (in `src/lib.rs`'s test module; private fields are reachable).
- Step a lossy scenario tick by tick at N = 1 and N = 2.
- For each symbol, record the ticks at which `last_retx_tick` changes.
- **Assert, absolute:**
  - consecutive retransmits of one symbol are at least `rtt_ticks` apart, read for the path the **earlier** resend went out on. A resend sets `sym.path = dest`, and the next eligibility is checked against that path, so at N = 2 the original path's clock would fail falsely when `dest` is the faster leg;
  - no symbol is retransmitted after `recovered` is true;
  - `total_arq > 0` (witness).
- This bounds the documented model–engine divergence (paper §11.3, "reactive retransmit trigger in the wasm model").
- It is green on main by construction. Say so in the commit: it is a bounding test, not a red-first one.

## 4. The wasm model: a bounding test, no golden re-capture

`raptorpath-wasm/src/lib.rs:1266–1300` is not a copy branch:
- It is slot priority (1) and never decrements `self.debt`. Only priority (2), the repair branch at :1351–1358, draws on the debt.
- So the model's proactive slots are already coded-only, and its P_lost retransmit is the model's whole **reactive** plane (NACK and refresh merged, oracle candidates `lost && !recovered`, one per path `rtt_ticks`).

Deleting it would delete the model's ARQ, not mirror the engine change. Re-keying it onto the engine's NACK ripeness would be a separate, deliberate model change, which ADR-0079 does not ask for. So:
- **Choice: a bounding test (T4).** No model code changes. `GOLDEN_CAPTURE` is not set, and `test_multipath_n1_identical_golden` must pass unchanged.
- This follows CLAUDE.md: every documented model–engine divergence carries a test that bounds it, and re-capture is only for a deliberate model change.

## 5. Checks that must pass (host, no VM)

- **δ continuity at the presets (±2 %),** unchanged and green:
  - `the_rate_mix_is_byte_identical_at_the_presets`, `the_bulkness_weight_is_exact_at_the_presets_and_the_ablation_zeroes_it` and `the_rate_does_not_step_across_the_bulk_preset_or_anywhere_on_the_dial` (`control/fec_rate/tests.rs:62, 141, 169`);
  - `n1_is_continuous_and_monotone_in_delta_through_every_preset` (`net/tests.rs:5778`);
  - `the_request_bar_is_continuous_and_monotone_through_the_three_named_points` (`net/late.rs:603`);
  - `raptorpath-math` `span_deadline_d_pinned_and_continuous`, `span_width_a_star_pinned_and_continuous` and `sim_tail_target_of_delta_pinned_and_continuous` (`span_forms.rs:102, 122, 184`), and `test_r_star_mass_monotone_and_continuous_in_delta` (`lib.rs:2086`);
  - `raptorpath-wasm` `test_continuum_one_law_across_the_dial`, `test_named_hints_are_points_on_the_continuum` and `giveup_budget_monotone_and_continuous_in_rho`;
  - `raptorpath-visualizer/test_visualizer.mjs` and `test_visualizer_ui.mjs` via `build_visualizer.sh`.
- **Continuity of the deleted law itself:** T1 runs the nine preset points, and copies are identically 0 at each, so there is no step across any preset.
- **Decision pins:** `decision_trace_pin` (`net/decision_trace_pin.rs`) is unchanged. D11 `shed_allowed` and D21 `TaperBudget` are pure functions this change does not touch. `hotpath_pin` is unchanged (pure layer).
- `cargo check --all-targets`; lib `net::` and `gates`; integration tests `holeclass_reachability`, `recovery_bench`, `sim_nack_test` and `place_arms_reachability`; the new `s06_no_copy`; `raptorpath-wasm` tests; `build_visualizer.sh`; `tools/l1` python tests.
- The host ns bench, for the per-symbol ε read removed in §1.3.

## 6. Merge gate (VM, for the coordinator to register)

This is a deliberate behaviour change, not a zero-behaviour refactor. The derivation (§10) notes that §41c's "no change" reading does not apply. The owner's brief still asks for a guard against main, so it runs as a no-regression guard and is read as one.

1. **VM pre-flight** (status §53's list), on fresh targets for BASE (main) and CAND:
   - `cargo check --all-targets`; lib `net::` and `gates`;
   - `holeclass_reachability`, `recovery_bench`, `sim_nack_test`, `place_arms_reachability` and `s06_no_copy`;
   - `build_visualizer.sh` on a fresh wasm build, and the native golden test without `GOLDEN_CAPTURE`;
   - the `bound-tests` wasm pool test.
2. **Stage G, the guard: §41d v2** (`LD_PLAN=41d`, rank-minp, 12 + 12 at c1s/c1d/c2/c8; c2 and c8 at 600 MB).
   - **Caveat: the guard rows run at `hint=bulk`** (`tools/l1/loopdiag_battery.sh:156`). Bulk sends no proactive slots (rr = 0; status Addendum 2's H-B has no copies), so both arms take the same path and Stage G is blind to this change. Read it as a check that the release binary did not regress Bulk, nothing more.
3. **Stage A, the readout: Auto c2-100 and c8-100,** BASE vs CAND, 6 + 6 per seed (42, 7), 600 MB, reported only.
   - **Witnesses per row:** BASE `taper=` > 0, CAND `taper=` == 0 and `pslot=` > 0. A row that fails its witness is VOID.
   - **Register:**
     - **c8 in-order delay** (§41d's definition: mean of `t − t_src(ack(t))` over ticks with ack > 0, ρ = 1 only), **with no prediction**. This is ADR-0079's unknown.
     - c2 in-order delay;
     - goodput, predicted unchanged by arithmetic (Addendum 2);
     - CPU/GB at both ends;
     - `[DIAG] retx=` per source;
     - `[RFA] fill_coded` per source (holes closed by decoding; c2's predicted band is 63–83 % of source losses, derivation §4.1.1; c8 is unknown within [6 %, 100 %]);
     - `below_floor` arrivals, predicted to fall from about 60–97k to H-B's under 1k;
     - `cod=` / `pslot=`.
   - The c8 delay reading goes to the owner. The plan takes no position on what a cost there means.
4. **Stage R, evicting seat: Realtime c2-100 paced** (block E's PM shape).
   - Register `shed_slot` (CAND 0), `shed_nack`, `shed_denied` and delivered fraction, reported only.
   - In-order delay is not defined at ρ < 1 (§41d item 3), so it is not read here.

## 7. Paper follow-ups in the engine merge commit

The paper was amended ahead of the code, so the merge commit makes these edits to `docs/fec-arq-model.md`:
- **§3.2:** "Once the deletion merges, no engine decision reads it" becomes "No engine decision reads it".
- **§5.6:** drop "Today it also runs inside the probabilistic retransmit branch … except between gap batches", and keep the NACK-site sentence.
- **§10**, the "Probabilistic taper copy" row: status "removed (ADR-0079)", with the deletion commit's hash.
- **§11.3:**
  - delete the "proactive slot content" interim row and the "P_lost inputs" row;
  - in "recovery-plane path set", drop "the taper's ε̂ at send";
  - in the wasm row, replace "Bounding test owed (T4 …)" with the test's name.
- **§9.1:** "reads 0 once ADR-0079's deletion merges" becomes "reads 0 since ADR-0079".
- **ADR-0079 Status** and the `docs/adr/README.md` row: "engine change pending" becomes the merge commit.
- Rebuild with `bash tools/build_paper.sh docs/fec-arq-model.md <scratch>.pdf` (pandoc and typst, no cargo), and run `python tools/roadmap/history_audit.py`, which parses §10.
