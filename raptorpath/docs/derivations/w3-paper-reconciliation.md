# W3.7: the paper against the engine (reconciliation record)

Plan items W3.7 and W3.5 (rate-law questions). No cargo and no VM were used.
Tests were found by grep (`fn <name>`, then the body was read for what it
asserts). None was run. The proposed tests below are specifications only.

**Refs.** The engine and paper are `github/main` at `fdad664e`. The fixed
`bbr_rs` is `github/bbr48cand` at `e5d7ffbd`; main still carries the
pre-§48 port. Line numbers are on main unless marked `cand`.

**The rule applied (CLAUDE.md).** Every documented model-vs-engine divergence
must have a test that bounds it, meaning a test that asserts its absolute
value or an exact identity. An ordinal test does not count, and neither does
a test that pins only the form of a function when the divergence lies in its
input.

Verdicts:
- **still true:** the divergence exists as the paper describes it.
- **fixed:** the paper records it as resolved, and the code agrees.
- **stale:** the paper text is wrong against the code.
- **new:** a divergence that is not in the §11.3 table.

## 1. The §11.3 table, row by row

| # | paper § / item | claim (engine column) | engine file:line | verdict | bounding test |
|---|---|---|---|---|---|
| 1 | §11.3 span deadline D, A* | D = min(b·RTprop, 2·RTprop) is fed `est.rtt()` (EWMA), not RTprop | `net/emit_source.rs:567-573` | still true | **MISSING.** `net/tests.rs:368 shed_deadline_is_the_span_law_d` pins only the form b·x capped at 2x, not the input |
| 2 | §11.3 receiver shed hold | hard-coded SRTT/2 | `net/shed.rs:58-62` | still true | `net/tests.rs:383 shed_recv_hold_delta_dial_and_legacy_fallback` (40 ms at SRTT 80 ms, exact) |
| 3 | §11.3 Copa price | δ = 0.5 constant plus a 1.08/1.125/1.25 queue-multiplier table unless the wire signal is on | `scheduler/copa.rs:609-615` (δ), `:314-318` (table) | still true | Partial. Table: `scheduler/tests.rs:580 test_hint_plumbed_to_paths` (exact). δ = 0.5 off-wire at every hint: **MISSING** |
| 4 | §11.3 store cap at one path | no path-count branch; `RWM_STORE_PATHS=0` is the control | `net/store_cap.rs:76-92`, `gates.rs:334` | fixed | `net/tests.rs:6086 one_path_refresh_routes_into_the_pooled_law_at_every_preset`, `:6135 control_arm_is_main_s_single_path_chain_exactly` |
| 4b | same row, the wasm clause | "the wasm model scales its pool per path with no guard" | `raptorpath-wasm/src/lib.rs:352` (`STORE_PER_PATH = 512`) | stale (understated): the model's pool is a constant 512 per path, not the (1+q(δ))·BtlBw·RTprop law | **MISSING** (no test bounds model pool against engine cap) |
| 5 | §11.3 pool path set | `channel_paths` = `live_paths()` | `net/channel_set.rs:26-35` | fixed | `scheduler/tests.rs:27 saturated_path_is_live_but_not_active`; `net/tests.rs:1511 recovery_clocks_keep_cwnd_saturated_paths` |
| 6 | §11.3 recovery-plane path set | live set for every pool and worst-path reader | `net/channel_set.rs:63-72` | fixed | as row 5, plus `net/tests.rs:5100 saturated_channel_keeps_the_p_lost_inputs` |
| 7 | §11.3 store headroom H | 1+q(δ) at every N; (gain−1)·RTprop only in the `RWM_STORE_PATHS=0` control and the default-off frontier arm | `net/store_cap.rs:162`; `scheduler/place.rs:438,464` (paper says `:409`) | still true (line drift) | `tests/formula_agreement.rs:699 the_delta_cap_substitutes_one_factor_and_reduces_to_candidate_d` |
| 8 | §11.3 r* to the generation encoder | constant floor 0.15 / 0.20 | `net/sender_policy.rs:357-359`, read `net/emit_source.rs:687` | still true | **MISSING** (only the `RWM_GEN_R=unset` echo is pinned) |
| 9 | §11.3 rate-law feedback leg (DoF ledger) | not implemented: open-loop `TaperBudget` plus `ceil(retx·ε̂)` margin plus `nacked·r` debt | `net/sender_phases.rs:1125`, `:1175-1185` | still true | **MISSING.** Bounded only by battery counters (`preempt_src`), which are measurements, not tests. The status §39 owed tests (`dof_ledger_without_feedback_is_the_taper_budget` …) do not exist |
| 10 | §11.3 pacing | the pacer debits source only | (bench model of `emit_source`) | still true | `tests/store_cap_sf_bench.rs:3205 pacer_debit_bounds_only_the_source_arm_not_the_wire` (exact identities; not `#[ignore]`) |
| 11 | §11.3 P_lost inputs | RTTVAR = 0.1·SRTT; worst path from `live_paths()` | `net/channel_set.rs:69` | still true | **MISSING** for the 0.1 constant. `saturated_channel_keeps_the_p_lost_inputs` pins routing, not the stand-in |
| 12 | §11.3 GE estimator input | per-batch counts, losses fed before receives | `control/estimator.rs:319-331`; callers `net/control_msg.rs:339,637` | still true | **MISSING** |
| 13 | §11.3 per-path loss feed | sender's own ack deltas, reorder credit carried | tracker `net/mod.rs:3698-3704`; credit `control/estimator.rs` | still true | `net/tests.rs:930 test_path_batch_tracker_with_gap`, `:961 s10_tracker_true_loss_plus_reorder_reads_exactly_the_true_loss`, `scheduler/tests.rs:2209 s10_ack_merge_counter_delta_carries_the_late_arrival_credit`, `control/estimator.rs:799 loss_credit_law_anchor_points` |
| 14 | §11.3 BOCD quantile | run-length-weighted average of quantiles, not the mixture quantile | `raptorpath-math/src/changepoint.rs:203-230` | still true | **MISSING.** `tests/control_loop.rs:248 test_predictive_loss_tracks_bocd` is ordinal |
| 15 | §11.3 δ_exit | not implemented | — | still true | n/a (an absent law has nothing to bound) |
| 16 | §11.3 visualizer Bulk tail target | wasm 0.05 at Bulk versus engine t_tail 10⁻³; `if bulkness > 0` guard | `raptorpath-wasm/src/lib.rs:883`, `:1676`, `:1689` | still true | `raptorpath-wasm/src/lib.rs:2229 test_sim_engine_tail_anchor_divergence_is_bounded` (bound log₁₀50, exact at Bulk). For the guard's continuity: `:2134 test_continuum_one_law_across_the_dial` |

## 2. Divergences outside the §11.3 table

| # | paper § | claim | engine file:line | verdict | bounding test |
|---|---|---|---|---|---|
| N1 | §4.6 (prose only) | χ ≡ 0 on the tunnel; only the perf client under `RWM_COMPLETION_EXPOSURE` sets it | `config.rs:363` (`completion_feed: None`); `net/sender_phases.rs:1315` | new to §11.3 (true) | `tests/chi_reachability.rs:117 without_the_arm_chi_is_never_evaluated_and_the_gauge_says_so` (n = 0, max = 0, exact); `:148 with_chi_zero_the_bulk_rate_is_exactly_the_corner_it_always_was` (`controller_rate == 0.0`) |
| N1b | §4.5 | "r* = 0 identically" at Bulk | `control/fec_rate/tests.rs:425` | test weaker than claim | `test_bulk_pure_arq_zero_steady_state_rate` asserts `< 0.01` through `FecRateController`, not `== 0.0`. The exact form exists only one layer down (N1) |
| N2 | §4.6 vs §3.2 | one RTTVAR, two stand-ins: χ uses 0.125·SRTT, P_lost uses 0.1·SRTT | `net/sender_phases.rs:1325`; `net/channel_set.rs:69` | new | **MISSING** |
| N3 | §4.8 | W* uses per-hint latency budgets (Bulk 2, Auto 1, Realtime ½ RTT) | `control/fec_rate.rs:546-553` passes `latency_budget = 0.0`, so `raptorpath-math/src/lib.rs:1008-1014` uses 1·SRTT at every δ. The wasm does the same (`lib.rs:1711-1718`, `:2037`) | stale (paper) or engine gap; user decision | **MISSING** |
| N3b | §4.8 table | targets 10⁻² / 10⁻⁴ / 10⁻⁶ and ε | the shipped t_tail presets are 10⁻³ / 10⁻⁵ / 10⁻⁷ (`config.rs:337`, §4.1). W* is fed `predictive_loss_upper(0.95)`, not ε̂ (`fec_rate.rs:543`) | stale (illustrative inputs not labelled) | **MISSING** (the worked table is unpinned) |
| N4 | §4.8 | W* is the encoder window | W* acts only as `keep_behind = W*.clamp(16, win_cap)` (`net/sender_phases.rs:1384-1386`); `win_cap = MAX_WINDOW_SIZE = 200` on the systematic seat (`net/sender_policy.rs:451-473`, `net/mod.rs:203`) | still true but understated: the 512 / 429 / 214 / 200 table values above 200 are unreachable on the default seat, and W* is a retention depth behind the ack, not a coding-window choice | **MISSING** |
| N5 | §8.4 law line (`fec-arq-model.md:3425`) | min filter `if r_e < m̂` | cand `src/transport/bbr_rs.rs:839`: `r_e <= self.min_rtt` | stale (paper) | cand `bbr_rs.rs:1515 tied_min_rtt_samples_keep_the_filter_fresh_under_le_and_would_fire_probe_rtt_under_lt`. It asserts no ProbeRtt under `≤` exactly. Its strict-`<` half is arithmetic inside the test and does not run the controller. On main: n/a (no fixed `bbr_rs`) |
| N6 | §8.4 tests list (`:3570-3580`) | seven named tests | all seven exist only on cand (`bbr_rs.rs`, `transport/cc_sim.rs`) | stale on main (the section says "candidate, not shipped", but the tests are cited as if present) | — |
| N7 | §5.7 (`:1803`), §9.10 (`:4032`) | `place_costs_match_the_pinned_table` | renamed to `place_costs_match_the_pinned_wired_table` (`scheduler/tests.rs:3028`, `4265e3db`) | stale (test name) | the renamed test |
| N8 | `raptorpath-math/src/lib.rs:976` doc | "the Realtime hint's budget" | no caller passes a non-zero budget | stale (code comment) | — |

## 3. Rate law (W3.5): what the record supports

- **χ.** The engine matches the paper's §4.6 status. The tunnel never sets χ,
  so at Bulk the late-is-fine term is exactly 0, and that is bounded (N1). It
  is absent from §11.3 and should be added there.
- **W\* budget.** The paper's per-hint budgets ½ / 1 / 2 RTT are exactly
  b(δ) at the presets (`net/store_cap.rs:285-296`: `delta_budget_b`, the
  continuous `span_horizon_b`). The engine and the visualizer both pass 0,
  which means 1 RTT at every δ. They therefore agree with each other and not
  with the paper. Passing `b(δ)·SRTT` would be one continuous formula, with
  no hint branch (NO-MODE-SWITCH safe).
- **r ≈ 0 on every measured cell.** Status §37 records r* = 0 at Bulk with a
  coded share of ≤ 0.9 %, and status §44 item 5 says the same. No off-Bulk
  which-term-wins row exists. §9.10 rates the composed rate "partial", which
  is consistent with this. This is an evidence gap, not a divergence, and it
  is owed before any rate-law change.
- **`RWM_RSTAR_TAIL`.** The paper (`:772`) says default on. The code
  (`control/fec_rate.rs:128`) and status §1 say on. Consistent; the §44
  disagreement is closed.

## 4. Stale deleted-gate text (edited in this commit)

1. `fec-arq-model.md:1858`, §5.7. "the path pool, the honest cap and the
   per-path `sout`" became "the path pool and the per-path `sout`".
   `RWM_HONEST_CAP` was removed in simplification batch 1.
2. `:4089`, §10, the `RWM_COMPOSED_CAP` row. Its status was "off" and is now
   "removed (simplification batch 1; row below)". The arm is deleted and
   `:4126` records the removal.
3. `:4177`, §11.2, the `RWM_STORE_GAIN` row. "by the default-off honest and
   pool-anchor arms" is dropped; both arms are deleted, and no code reads
   `store_gain` for them.

A mechanical sweep compared every `RWM_*` name in the paper with the names in
`src/`, `tests/`, the math and wasm crates and `tools/`. Every name that
appears only in the paper sits in a §10 row marked removed. No other
deleted gate is named as live.

## 5. Proposed paper edits (not made)

1. §8.4 `:3425`: `if r_e < m̂ or expired` → `if r_e ≤ m̂ or expired`. Cite
   the tie test. Ties restamp t_m, so a recurring minimum does not force
   ProbeRtt. The Linux `bbr_update_min_rtt` comparison was not checked
   against a pinned kernel source here, so do not cite "v1" as the authority
   for either sign.
2. §8.4 tests list: mark the seven tests "on the §48 candidate branch".
   Alternatively, land them with the fix.
3. §11.3: add rows N1 (χ ≡ 0 on the tunnel), N2 (two RTTVAR stand-ins), N3
   and N4 (W\* budget and acting site). Reword 4b to "the model's pool is a
   constant 512 per path; it does not implement the §6.1 law".
4. §4.8: either state that the engine runs budget = 1 RTT at every δ, or
   change the engine (see the user questions). Label the table's targets as
   illustrative, or recompute them at the shipped t_tail presets. State that
   values above 200 are clipped on the systematic seat and that W\* sets
   retention behind the ack.
5. §5.7 `:1803` and §9.10 `:4032`: rename to
   `place_costs_match_the_pinned_wired_table`.
6. Line-reference drift since `168f161` (the symbols and values all match):
   - §11.2:
     - `gates.rs:361` → `:338` (store gain)
     - `gates.rs:364` → `:341` (store boot)
     - `gates.rs:165` → `:156`/`:354` (`RWM_GEN_R`)
     - `sender_policy.rs:369` → `:357-359`
     - `sender_policy.rs:529` → `:517`
     - `emit_source.rs:591` → `:577` (Δ cap)
     - `emit_source.rs:701` → `:687`
     - `place.rs:409` → `:438`/`:464` (also §11.3 row 7)
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

   Prefer symbol names over line numbers in the paper; the numbers will drift again.

Code nits, reported and not changed:
- `control/estimator.rs:243` still describes `RWM_POOL_ANCHOR` in the present tense.
- `tests/chi_reachability.rs` `ARM` still sets the deleted `RWM_PLAIN_RS=1`, which is inert.

## 6. Proposed bounding tests (specifications)

| test | asserts (absolute) | where |
|---|---|---|
| `span_deadline_reads_the_ewma_rtt_not_rtprop` | Build an estimator whose EWMA RTT ≠ min RTT. The (A*, D) computed by the span block equals `min(b·ewma, 2·ewma)` exactly, and differs from `min(b·rtprop, 2·rtprop)` by the measured ratio. This needs the block at `emit_source.rs:567-577` extracted into a pure fn. | `net/tests.rs` |
| `legacy_copa_delta_is_half_at_every_hint_off_wire` | With the wire gate off, `CopaState::new(clock, h).delta == 0.5` for h ∈ {Realtime, Auto, Bulk}. With it on, `== copa_delta_for_hint(h)`. | `scheduler/tests.rs` |
| `generation_repair_floor_is_the_constant_prior` | With `RWM_GEN_R` unset, `gen_repair_floor` is exactly 0.15 (systematic) and 0.20 (coded). The emitted rate is `max(r*, floor)` at r* = 0 and at r* > floor. | `net/tests.rs` |
| `dof_ledger_without_feedback_is_the_taper_budget` (owed, status §39) | The open-loop bank equals ⌊r·n⌋ ± 1 over n sends; the `ceil(retx·ε̂)` margin and the `nacked·r` debt are what the ledger would subsume. Pins the present open loop. | `control/` or `net/tests.rs` |
| `p_lost_rttvar_is_a_tenth_of_srtt` | `p_lost_inputs(sched).1 == 0.1 * srtt` exactly on a measured path. | `net/tests.rs` |
| `chi_and_p_lost_rttvar_stand_ins_differ_by_the_declared_ratio` | Pins 0.125 vs 0.1 (or fails once they are unified). | `net/tests.rs` |
| `ge_batch_feed_orders_losses_before_receives` | `record_batch(n, n−k)` leaves the GE chain in the state of k Bad then n−k Good symbols, exactly (compare with a per-symbol replay). | `control/estimator.rs` tests |
| `bocd_quantile_is_the_weighted_average_of_quantiles` | On a two-run-length posterior, `predictive_quantile` equals Σwᵢ·qᵢ/Σwᵢ exactly, and differs from the bisected mixture quantile by a stated, asserted bound. | `raptorpath-math/src/changepoint.rs` |
| `derived_window_runs_one_rtt_budget_at_every_delta` | `ctrl.derive_window(est)` equals `math::derive_window(t_tail, ε_u, σ², srtt, rate, srtt)` exactly at the three presets and at ±2 % nudges. This pins today's single budget until the user decides N3. | `control/fec_rate/tests.rs` |
| `derive_window_paper_table` | The §4.8 worked rows recomputed at their stated inputs (or at the shipped targets, if the table is retabulated), to ±1 symbol. | `raptorpath-math` |
| `keep_behind_is_w_star_clipped_to_the_seat_cap` | On the systematic seat, `keep_behind == W*.clamp(16, 200)`, including W* = 512 → 200. | `net/tests.rs` |
| `bulk_rate_is_exactly_zero_through_the_controller` | Tightens N1b: `FecRateController(Bulk).compute_repair_rate(est@5 %) == 0.0`, with χ unset as on the tunnel. | `control/fec_rate/tests.rs` |
| `wasm_pool_is_constant_per_path` | The model's pool = 512·n_paths exactly, whatever δ and BDP. Pins model-vs-engine 4b until the model adopts the law. | `raptorpath-wasm` |

## 7. Questions for the user (rate law)

1. **W\* budget.** Should the engine and the visualizer pass `b(δ)·SRTT` as
   the latency budget? That is continuous, already defined, and equals the
   paper's ½ / 1 / 2 RTT at the presets. Or should the paper be rewritten to
   the shipped one-RTT budget? The first is a model change for the wasm, so
   golden fingerprints would need a deliberate re-capture.
2. **What W\* is for.** On the default seat it only sets retention behind the
   ack and is clipped at 200. Is that the intended role? If so, §4.8's
   framing (the coding window, values up to 512) needs rewriting. If not, the
   200 cap is the open constant.
3. **χ on the tunnel.** Is χ ≡ 0 the intended tunnel contract, so that the
   Bulk end-of-object glide exists only for drivers that know T_rem? Or should
   the tunnel estimate T_rem, for example from an inner-flow FIN or idle?
4. **Off-Bulk evidence.** r ≈ 0 on every measured cell. Should the off-Bulk
   which-term-wins row (status §44 item 5) be registered before any further
   rate-law work, and at which δ and cell?
5. **One RTTVAR.** Should the 0.1·SRTT (P_lost) and 0.125·SRTT (χ) stand-ins
   be unified, or replaced by a measured RTTVAR from the per-leg state (W3.2)?
