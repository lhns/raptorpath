# W3.7: the paper against the engine (reconciliation record)

Plan items W3.7 and W3.5 (rate-law questions). No cargo and no VM were used.

**How tests were checked.** Each test was found by grep (`fn <name>`). The
body of every test named as a bound in §1 and §2 was then read for two
things: does it call engine code, and does it assert an absolute value? No
test was run. The proposed tests in §6 are specifications only.

**Refs.** The engine and paper are `github/main` at `fdad664e`. The fixed
`bbr_rs` is `github/bbr48cand` at `e5d7ffbd`; main still carries the pre-§48
port. Line numbers are on main unless marked `cand`.

**Rule (CLAUDE.md).** Every documented model-vs-engine divergence needs a test
that bounds it, meaning one that asserts its absolute value or an exact
identity. These do not count:
- an ordinal test;
- a test that pins only a function's form when the divergence lies in the
  function's input;
- a test that runs a replica of the engine. This is marked "partial".

**Verdicts.**
- **still true:** the paper describes the code correctly.
- **stale:** the paper text is wrong against the code.
- **fixed:** the paper records the item as resolved, and the code agrees.

The "in §11.3" column says whether the paper's divergence table lists the
item.

## 1. The §11.3 table, row by row

| # | paper § / item | in §11.3 | claim | engine file:line | verdict | bounding test |
|---|---|---|---|---|---|---|
| 1 | span deadline D, A* | yes | D = min(b·RTprop, 2·RTprop) is fed `est.rtt()` (EWMA), not RTprop | `net/emit_source.rs:567-573` | still true | **MISSING.** `net/tests.rs:368 shed_deadline_is_the_span_law_d` pins the form b·x capped at 2x, not the input |
| 2 | receiver shed hold | yes | hard-coded SRTT/2 | `net/shed.rs:58-62` | still true | `net/tests.rs:383 shed_recv_hold_delta_dial_and_legacy_fallback` (40 ms at SRTT 80 ms, `assert_eq!`) |
| 3 | Copa price | yes | δ = 0.5 constant plus a 1.08/1.125/1.25 queue-multiplier table unless the wire signal is on | `scheduler/copa.rs:609-615` (δ), `:314-318` (table) | still true | Table: `scheduler/tests.rs:580 test_hint_plumbed_to_paths` (exact). δ = 0.5 off-wire at every hint: **MISSING** |
| 4 | store cap at one path | yes | no path-count branch; `RWM_STORE_PATHS=0` is the control | `net/store_cap.rs:76-92`, `gates.rs:334` | fixed | `net/tests.rs:6086 one_path_refresh_routes_into_the_pooled_law_at_every_preset` (cap == ceil((1+q)·bdp) through the sender refresh), `:6135 control_arm_is_main_s_single_path_chain_exactly` |
| 4b | same row, wasm clause | yes | "the wasm model scales its pool per path with no guard" | `raptorpath-wasm/src/lib.rs:352` (`STORE_PER_PATH = 512`) | stale (understated): the model pool is a constant 512 per path, not the §6.1 law | **MISSING** |
| 5 | pool path set | yes | `channel_paths` = `live_paths()` | `net/channel_set.rs:26-35` | fixed | `scheduler/tests.rs:27 saturated_path_is_live_but_not_active`, `net/tests.rs:1511 recovery_clocks_keep_cwnd_saturated_paths` (exact id lists) |
| 6 | recovery-plane path set | yes | live set for every pool and worst-path reader | `net/channel_set.rs:63-72` | fixed | as row 5, plus `net/tests.rs:5100 saturated_channel_keeps_the_p_lost_inputs` |
| 7 | store headroom H | yes | 1+q(δ) at every N; (gain−1)·RTprop only in the `RWM_STORE_PATHS=0` control and the default-off `RWM_PLACE_HOL` arm | `net/store_cap.rs:162`; `scheduler/place.rs:438,464` (paper says `:409`) | still true (line drift) | `tests/formula_agreement.rs:699 the_delta_cap_substitutes_one_factor_and_reduces_to_candidate_d` (multiplier = 1+q(b) to 1e-12, engine fn) |
| 8 | r* to the generation encoder | yes | constant floor 0.15 / 0.20 | `net/sender_policy.rs:357-359`, read at `net/emit_source.rs:687` | still true | **MISSING** (only the `RWM_GEN_R=unset` echo is pinned) |
| 9 | rate-law feedback leg (DoF ledger) | yes | not implemented: open-loop `TaperBudget` plus `ceil(retx·ε̂)` margin plus `nacked·r` debt | `net/sender_phases.rs:1125`, `:1175-1185` | still true | **MISSING.** Only battery counters (`preempt_src`) bound it. The owed tests from status §39 do not exist |
| 10 | pacing | yes | the pacer debits source only, and is off by default (`cc_pace` = `copa_wire`) | debit `net/emit_source.rs:425`; gate `net/mod.rs:2804`; default `gates.rs:368` | still true | Partial. `tests/store_cap_sf_bench.rs:3205 pacer_debit_bounds_only_the_source_arm_not_the_wire` asserts exact identities, but on the bench's own replica of the debit (`:2600`), not on `emit_source`. Its message cites `emit_source.rs:493-497`, which is now `:425` |
| 11 | P_lost inputs | yes | RTTVAR = 0.1·SRTT; worst path from `live_paths()` | `net/channel_set.rs:69` | still true | **MISSING** for the 0.1 constant (`saturated_channel_keeps_the_p_lost_inputs` pins routing only) |
| 12 | GE estimator input | yes | per-batch counts, losses fed before receives | `control/estimator.rs:319-331`; callers `net/control_msg.rs:339,637` | still true | **MISSING** |
| 13 | per-path loss feed | yes | sender's own ack deltas, reorder credit carried | tracker `net/mod.rs:3698-3704`; credit in `control/estimator.rs` | still true | `net/tests.rs:930 test_path_batch_tracker_with_gap`, `:961 s10_tracker_true_loss_plus_reorder_reads_exactly_the_true_loss`, `scheduler/tests.rs:2209 s10_ack_merge_counter_delta_carries_the_late_arrival_credit` (engine `PathState`, exact fed loss), `control/estimator.rs:799 loss_credit_law_anchor_points` |
| 14 | BOCD quantile | yes | run-length-weighted average of quantiles, not the mixture quantile | `raptorpath-math/src/changepoint.rs:203-230` | still true | **MISSING** (`tests/control_loop.rs:248 test_predictive_loss_tracks_bocd` is ordinal) |
| 15 | δ_exit | yes | not implemented | — | still true | n/a (an absent law) |
| 16 | visualizer Bulk tail target | yes | wasm 0.05 at Bulk against engine t_tail 10⁻³ | `raptorpath-wasm/src/lib.rs:225-250` | still true | `raptorpath-wasm/src/lib.rs:2229 test_sim_engine_tail_anchor_divergence_is_bounded` (bound log₁₀50, exact at Bulk) |
| 16b | same row, the `if bulkness > 0` guard | yes | the rate mix is guarded | `raptorpath-wasm/src/lib.rs:883`, `:1676`, `:1689` | still true | Partial. `test_continuum_one_law_across_the_dial` (`:2134`) and the `test_visualizer.mjs` gate check sim behaviour at β = 0 exactly (δ = 50) and across the **Bulk** preset. Nothing checks sim behaviour across the β → 0⁺ edge at the Auto preset (δ = 0.5 against 0.49), which is where the guard flips branch: **MISSING** |

## 2. Divergences outside the §11.3 table

This section is not exhaustive. It covers the brief's items plus greps of
the paper for `not implemented|stands in|hard-code|not shipped`. No crate-wide
divergence hunt was run.

| # | paper § | in §11.3 | claim | engine file:line | verdict | bounding test |
|---|---|---|---|---|---|---|
| N1 | §4.6 prose | no | χ ≡ 0 on the tunnel; only the perf client under `RWM_COMPLETION_EXPOSURE` sets it | `config.rs:363` (`completion_feed: None`); `net/sender_phases.rs:1315` | still true | `tests/chi_reachability.rs:117 without_the_arm_chi_is_never_evaluated_and_the_gauge_says_so` (n = 0 and max = 0, through a loopback transfer); `:148 with_chi_zero_the_bulk_rate_is_exactly_the_corner_it_always_was` (`controller_rate == 0.0`) |
| N1b | §4.5 "r* = 0 identically" at Bulk | no | the controller-level test is weaker than the claim | `control/fec_rate/tests.rs:425` | still true (claim); test weak | `test_bulk_pure_arq_zero_steady_state_rate` asserts `< 0.01` through `FecRateController`. The exact `== 0.0` exists only one layer down (N1) |
| N2 | §4.6 against §3.2 | no | one RTTVAR, two stand-ins: χ uses 0.125·SRTT, P_lost uses 0.1·SRTT | `net/sender_phases.rs:1325`; `net/channel_set.rs:69` | still true (undocumented) | **MISSING** |
| N3 | §4.8 | no | W* uses per-hint latency budgets (Bulk 2, Auto 1, Realtime ½ RTT) | `control/fec_rate.rs:546-553` passes `latency_budget = 0.0`, so `raptorpath-math/src/lib.rs:1011-1017` uses 1·SRTT at every δ. The wasm does the same (`lib.rs:1711-1718`, `:2037`) | stale | **MISSING** |
| N3b | §4.8 worked table | no | targets 10⁻² / 10⁻⁴ / 10⁻⁶ | the shipped t_tail presets are 10⁻³ / 10⁻⁵ / 10⁻⁷ (`config.rs:337`, paper §4.1). W* is fed `predictive_loss_upper(0.95)`, not ε̂ (`control/fec_rate.rs:543`) | stale (the illustrative inputs are not labelled) | **MISSING** (the table is unpinned) |
| N4 | §4.8 | partly ("the sender caps W at 200") | W* sizes the encoder window | `net/sender_phases.rs:1383-1387`: in the non-generation branch, `advance(ack − W*.clamp(16, win_cap))` sets the window's trailing edge to W* symbols behind the cumulative ack. `win_cap = MAX_WINDOW_SIZE = 200` on the systematic seat (`net/sender_policy.rs:451-473`, `net/mod.rs:203`) | still true | **MISSING**. The paper should say how W* acts: through the trailing edge behind the ack. The table's 512, 429 and 214 are clipped to 200 on the default seat |
| N5 | §8.4 law line `fec-arq-model.md:3425` | no | min filter `if r_e < m̂` | cand `src/transport/bbr_rs.rs:839`: `r_e <= self.min_rtt` | stale | cand `bbr_rs.rs:1515 tied_min_rtt_samples_keep_the_filter_fresh_under_le_and_would_fire_probe_rtt_under_lt` asserts no ProbeRtt under `≤`, exactly, through the controller. Its strict-`<` half is arithmetic inside the test and does not run the controller. On main: n/a (no fixed `bbr_rs`) |
| N6 | §8.4 tests list `:3570-3580` | no | eight named tests: 3 unit, 3 `cc_sim`, 2 upstream `quinn_bbr_*` pins | all eight exist only on cand (`bbr_rs.rs`; `transport/cc_sim.rs:635`, `:665` for the pins) | stale on main | — |
| N7 | §5.7 `:1803`, §9.10 `:4032` | no | `place_costs_match_the_pinned_table` | renamed to `place_costs_match_the_pinned_wired_table` (`scheduler/tests.rs:3028`, commit `4265e3db`) | stale | the renamed test |
| N8 | `raptorpath-math/src/lib.rs:976` doc comment | no | "the Realtime hint's budget" | no caller passes a non-zero budget | stale (code comment) | — |
| N9 | §6.1 `:2464` | no | `RWM_STORE_LAW=oracle` (and `RWM_ORACLE_L_DGPS`) described as a default-off arm | neither name exists in main `src/` | stale on main (diagnostic branch only) | — |

## 3. §11.2 open-constants register

Values and symbols come from a lookup pass. Rows marked `*` were checked by
hand here: `RWM_STORE_GAIN`, `w_div` = 1.0 (`scheduler/mod.rs:93`),
`STORE_CAP_FLOOR` = max(8, 10) = 10 (`net/sender_policy.rs:79`), and the
`derive_window` bounds 16 / 512 (`raptorpath-math/src/lib.rs:947-949`).

These are declared constants, not model-vs-engine divergences, so the
bounding-test rule does not apply to them. The register itself is their
record, and its "deciding measurement" column says what would settle each
one.

| constant | verdict | note |
|---|---|---|
| refresh clamp, legacy age gate, 9/8 and 3, `GAP_ACK_MIN_INTERVAL`, `NACK_RETX_COOLDOWN_FLOOR_US`, κ, `PLACE_TEMPERATURE`, `w_div`*, dead-leg clamp and 50 ms seed, near-tie band, `BULK_TAIL_BUDGET`, χ constants, W* α and bounds* and 200, reactive margin, debt reduction, knee 2048, `queue_target_mult`, receiver hold, anchor constants, `RWM_EMIT_BURST`, taper-cache 50 ms, `LAG_TICK`, `IO_CHANNEL_DEPTH`, `VIEW_REFRESH`, `INBOUND_BATCH_MAX`, `CONTROL_SEND_TIMEOUT`, `SENDER_IN_BATCHES`, `SENDER_CMD_DEPTH`, M* clamp, rate cadence | still true | values match; the cited lines, where given, still point at them |
| `RWM_STORE_GAIN`* | stale, now edited | named readers included the deleted honest-cap and pool-anchor arms; `gates.rs:361` → `:338`; `place.rs:409` → `:438`/`:464` |
| `RELIABLE_STORE_MAX` | still true (line drift) | `sender_policy.rs:529` → `:517` |
| Δ cap 64 | still true (line drift) | `emit_source.rs:591` → `:577` |
| store boot 128 / `STORE_BOOT_DERIVED`* | still true (line drift) | `gates.rs:364` → `:341` |
| repair floors `RWM_GEN_R` (two rows) | still true (line drift) | `gates.rs:165` → `:156`/`:354`; `sender_policy.rs:369` → `:357-359`; `emit_source.rs:701` → `:687` |
| `SRTT/2` heal classifier | not checked | offline audit; no code site |

## 4. Rate law (W3.5): what the record supports

- **χ.** The engine matches the paper's §4.6 status. The tunnel never sets χ,
  so at Bulk the late-is-fine term is exactly 0, and that is bounded (N1). It
  is missing from §11.3.
- **W\* budget.** The paper's ½ / 1 / 2 RTT budgets are exactly b(δ) at the
  presets. That is `delta_budget_b` (`net/store_cap.rs:285-296`), the
  continuous `span_horizon_b`, pinned by
  `net/tests.rs:3194 delta_budget_b_is_the_dial_not_a_mode`. The engine and
  the visualizer both pass 0, which means 1 RTT at every δ, so they agree
  with each other and not with the paper. Passing `b(δ)·SRTT` would be one
  continuous formula with no hint branch, so it is safe under the
  NO-MODE-SWITCH invariant.
- **r ≈ 0 on every measured cell.**
  - Status §37 rates the rate law "inert at Bulk (r* = 0, coded share ≤ 0.9 %)".
  - Status §44 item 5 and the §45 plan (stage 3, item 5 "Waiting") both put
    one off-Bulk which-term-wins row first. §43 has no rate-law note.
  - No such row exists. §9.10 rates the composed rate "partial".
  - This is an evidence gap, not a divergence.
- **`RWM_RSTAR_TAIL`.** The paper (`:772`) says default on. `gates.rs:421`
  has `env_flag("RWM_RSTAR_TAIL", true)`, and status §1 also says on.
  Consistent; the disagreement noted in §44 is closed.

## 5. Stale deleted-gate text (edited in commit `833b7e4c`)

1. `fec-arq-model.md:1858`, §5.7. "the path pool, the honest cap and the
   per-path `sout`" became "the path pool and the per-path `sout`".
   `RWM_HONEST_CAP` was removed in simplification batch 1.
2. `:4089`, §10, the `RWM_COMPOSED_CAP` row. Its status was "off" and is now
   "removed (simplification batch 1; row below)". The arm is deleted, and
   `:4126` records the removal.
3. `:4177`, §11.2, the `RWM_STORE_GAIN` row. "by the default-off honest and
   pool-anchor arms" is dropped. Both arms are deleted, and no code reads
   `store_gain` for them.

**Sweep.** Every `RWM_*` name in the paper was compared with the names in
non-test `raptorpath/src/**/*.rs`.
- Every name found only in the paper sits in a §10 row marked removed, with
  two exceptions: `RWM_STORE_LAW` and `RWM_ORACLE_L_DGPS` (N9).
- Of the nine batch-1 gates, the paper outside §10 names none as live after
  the three edits above.

## 6. Proposed paper edits (not made)

1. **§8.4 `:3425`.** Change `if r_e < m̂ or expired` to `if r_e ≤ m̂ or
   expired`, and cite the tie test: ties restamp t_m, so a recurring minimum
   does not force ProbeRtt. Do not cite "v1" as the authority for either
   sign; the Linux `bbr_update_min_rtt` comparison was not checked against a
   pinned kernel source here.
2. **§8.4 tests list.** Mark the eight tests "on the §48 candidate branch",
   or land them with the fix.
3. **§11.3.**
   - Add rows N1 (χ ≡ 0 on the tunnel), N2 (two RTTVAR stand-ins), N3/N4 (W\*
     budget and how W\* acts).
   - Reword 4b: "the model's pool is a constant 512 per path; it does not
     implement the §6.1 law".
   - Row 10: the bound is a bench replica.
4. **§4.8.**
   - Either state that the engine runs budget = 1 RTT at every δ, or change
     the engine (question 1).
   - Label the table's targets as illustrative, or recompute the table at the
     shipped t_tail presets.
   - State that W\* acts as the window's trailing edge behind the cumulative
     ack, clipped to 200 on the systematic seat.
5. **§5.7 `:1803` and §9.10 `:4032`.** Rename the test to
   `place_costs_match_the_pinned_wired_table`.
6. **§6.1 `:2464`.** Say that the oracle arm lives on its diagnostic branch,
   not on main.
7. **Line-reference drift since `168f161`.** This list comes from the lookup
   pass. The `place.rs` and `sender_phases.rs:1314` items and the log echo
   were spot-checked by hand. The symbols and values all match.
   - §11.2: see §3 above.
   - §11.1 q13:
     - `emit_source.rs:475` → `:461`
     - `sender_policy.rs:746` → `:686`
     - `emit_batch_on` `:805` → `:745`. It now also requires `gates.emit_batch` and `!coded_wire`.
     - `control_msg.rs:404` → `:396`
     - `emit_source.rs:829` → `:815`
     - `net/mod.rs:2517` → `:2400`
     - `sender_policy.rs:542` → `:530`
     - `sender_policy.rs:667` → `:636`
     - `sender_policy.rs:760`/`:773` → `:700`/`:713`
     - `sender_phases.rs:984` → `:803`
     - `sender_phases.rs:1161`/`:1173`/`:1314` → `:980`/`:992`/`:1131`
     - `sender_phases.rs:1597` → `:1413`
     - the log echo `net/mod.rs:651` → `:604-617`

   Prefer symbol names over line numbers in the paper; the numbers will drift
   again.

**Code nits (reported, not changed).**
- `control/estimator.rs:243` describes `RWM_POOL_ANCHOR` in the present tense.
- `tests/chi_reachability.rs` `ARM` still sets the deleted `RWM_PLAIN_RS=1`,
  which is inert.
- The message in `pacer_debit_…` cites `emit_source.rs:493-497`; the debit is
  now at `:425`.

## 7. Proposed bounding tests (specifications)

| test | asserts (absolute) | where |
|---|---|---|
| `span_deadline_reads_the_ewma_rtt_not_rtprop` | Build an estimator whose EWMA RTT ≠ min RTT. The span block's D equals `min(b·ewma, 2·ewma)` exactly. Needs `emit_source.rs:567-577` extracted into a pure fn. | `net/tests.rs` |
| `legacy_copa_delta_is_half_at_every_hint_off_wire` | Wire gate off: `CopaState::new(clock, h).delta == 0.5` for every h. Wire gate on: `== copa_delta_for_hint(h)`. | `scheduler/tests.rs` |
| `generation_repair_floor_is_the_constant_prior` | With `RWM_GEN_R` unset, `gen_repair_floor` is exactly 0.15 (systematic) and 0.20 (coded), and the emitted rate is `max(r*, floor)`. | `net/tests.rs` |
| `reactive_margin_and_debt_are_the_open_loop_forms` | Pins today's open loop exactly. `serve_gaps` sends `ceil(retx·ε̂)` margin repairs per report, and `repair_debt` falls by `nacked·r`, at fixed ε̂ and r. It fails when the ledger (§4.13) replaces either. | `net/tests.rs` |
| `p_lost_rttvar_is_a_tenth_of_srtt` | `p_lost_inputs(sched).1 == 0.1 * srtt` exactly on a measured path. | `net/tests.rs` |
| `chi_and_p_lost_rttvar_stand_ins_differ_by_the_declared_ratio` | Pins 0.125 against 0.1, or fails once they are unified. | `net/tests.rs` |
| `ge_batch_feed_orders_losses_before_receives` | `record_batch(n, n−k)` leaves the GE chain exactly where a per-symbol replay of k Bad then n−k Good leaves it. | `control/estimator.rs` tests |
| `bocd_quantile_is_the_weighted_average_of_quantiles` | On a two-run-length posterior, `predictive_quantile == Σwᵢqᵢ/Σwᵢ` exactly. Its gap to the bisected mixture quantile is asserted within a stated bound. | `raptorpath-math/src/changepoint.rs` |
| `derived_window_runs_one_rtt_budget_at_every_delta` | `ctrl.derive_window(est)` equals `math::derive_window(t_tail, ε_u, σ², srtt, rate, srtt)` exactly at the presets and at ±2 % nudges. Pins today's single budget until question 1 is decided. | `control/fec_rate/tests.rs` |
| `derive_window_paper_table` | The §4.8 worked rows recomputed at their stated inputs, or at the shipped targets if the table is retabulated, to ±1 symbol. | `raptorpath-math` |
| `keep_behind_is_w_star_clipped_to_the_seat_cap` | On the systematic seat, the trailing edge is `ack − W*.clamp(16, 200)`, including W* = 512 → 200. | `net/tests.rs` |
| `bulk_rate_is_exactly_zero_through_the_controller` | `FecRateController(Bulk).compute_repair_rate(est at 5 %) == 0.0` with χ unset, as on the tunnel. Tightens N1b. | `control/fec_rate/tests.rs` |
| `sim_behaviour_is_continuous_across_the_auto_preset` | Sim FEC totals at δ = 0.5 and δ = 0.49 differ by no more than the Bulk-preset tolerance. This crosses the `bulkness > 0` guard. | `raptorpath-wasm` and `test_visualizer.mjs` |
| `wasm_pool_is_constant_per_path` | The model pool is exactly 512·n_paths at any δ and BDP. Pins 4b until the model adopts the law. | `raptorpath-wasm` |

## 8. Questions for the user (rate law)

1. **W\* budget.** Should the engine and the visualizer pass `b(δ)·SRTT` as
   the latency budget? It is continuous, already defined, and equals the
   paper's ½ / 1 / 2 RTT at the presets. Or should the paper be rewritten to
   the shipped one-RTT budget? The first changes the wasm model, which needs
   a deliberate golden re-capture.
2. **The 200 cap.** On the default seat W\* sets the window's trailing edge
   and is clipped at 200. Is 200 still intended? It is below the multipath
   bound of §5.5 at the heterogeneous cell. If it is, §4.8's table should be
   restated within it.
3. **χ on the tunnel.** Is χ ≡ 0 the intended tunnel contract, so that the
   Bulk end-of-object glide exists only for drivers that know T_rem? Or
   should the tunnel estimate T_rem, for example from an inner-flow FIN or
   idle?
4. **Off-Bulk evidence.** r ≈ 0 on every measured cell. Should the off-Bulk
   which-term-wins row (status §44 item 5, §45) be registered before any
   further rate-law work, and at which δ and cell?
5. **One RTTVAR.** Should the 0.1·SRTT (P_lost) and 0.125·SRTT (χ) stand-ins
   be unified, or replaced by a measured RTTVAR from the per-leg state (W3.2)?
