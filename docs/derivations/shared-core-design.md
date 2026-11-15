# Shared decision core for the engine and the L0 model (design, no code)

Base: `github/main` at `248bdbac`. Every file:line refers to that commit and
is relative to the repository root. Design only: no engine or model change,
no VM, no cargo build. Purity and dependency claims are **found by grep and
reading**. Every engine file does `use super::*`, so a hidden dependency can
only be ruled out by compiling. Phase 0 below adds that compile gate.

Context:
- ADR-0076 phase 6 (decided: move the closed forms into `raptorpath-math`).
- W3.2 per-leg state (`github/perleg-design:raptorpath/docs/derivations/w3-per-leg-state.md`,
  lines at `fdad664e`, several stale here).
- The model's mode-switch audit (`github/wasm-modeswitch-audit:docs/derivations/wasm-modeswitch-audit.md`).
  Its site classes are reused below.
- The drift audit (`github/viz-drift:docs/derivations/visualizer-drift-2026-11.md`,
  `f45b8ad0`, same base, not yet on main) covers model-vs-engine value drift
  and is not repeated here. Its table ids are cited as "drift A–E".
- The law ledger (`docs/derivations/law-ledger.yaml`), whose row ids (F01,
  C04, R03, ...) are cited where a moved law has a verdict.

**Goal.** Level 1 shares the laws (phase 6). Level 2, this design, extracts
the engine's per-tick **decisions** into an I/O-free crate that compiles to
wasm32. The visualizer then becomes "shared laws + shared decisions + a
simulated link and clock". Level 3 (the whole engine in the browser) is
rejected: the model has to stay an idealised picture that the paper can be
compared against.

**Stop rule for level 2.** A decision is shared only when the model already
holds all of its inputs as idealised state: per-path rate, RTprop, SRTT, ε̂,
seq ages, outstanding count, dial values. The model never creates engine
artefacts just to feed a shared decision. That excludes wire reports,
return-leg reordering, snapshot versions, quinn stats and the CC's probe
state. A decision that would need one stays engine-only and the model keeps
its idealisation, with a bounding test (§5.4).

## Summary of findings (side results, reported, not fixed)

- **F1. `RWM_DELTA` makes the store law NaN.** This was found by reading and
  has not been executed.
  - `codel_setpoint_q` (`raptorpath/src/net/store_cap.rs:130–135`) takes its
    band ends from `delta_budget_b(Realtime)` and `delta_budget_b(Bulk)`.
  - Those go through `delta_price` (`:311–313`), which returns
    `gates::delta_override()` when it is set (`gates.rs:58`).
  - With `RWM_DELTA` set, `b_lo == b_hi`, so `t = 0/0 = NaN` and the gain is
    1 + NaN.
  - `pooled_store_cap` then computes `NaN.ceil() as usize = 0` and clamps it
    to `floor`. The cap pins at `STORE_CAP_FLOOR` = 10 symbols.
  - Cause: an env read hidden inside a law. The config seam in §3 removes it.
- **F2. `raptorpath_math::FecRateController` has no caller.**
  `raptorpath-math/src/fec_rate_controller.rs` has none in `raptorpath`,
  `raptorpath-wasm` or the visualizer (grep), although its header says
  "Same code path for simulation and production". It is dead code.
- **F3. The loss estimator is forked.** The engine runs
  `raptorpath/src/control/estimator.rs`, which is `Instant`-clocked and reads
  gates (`:250`, `:284`). The model runs
  `raptorpath-math/src/estimator.rs` ("tick-based version for wasm", `:3`).
  Only BOCD and GE are shared (`control/mod.rs:13`).
- **F4. The span forms have engine counterparts, inline.** ADR-0076 says none
  of the four has a *same-named* engine counterpart, which is true. The
  expressions do exist inline (§2, table B), and A\* differs from the model's
  by a `ceil`. D(δ) exists twice inside the engine: in f64 at
  `net/emit_source.rs:577` and in integer µs at `net/shed.rs:31–33`.
- **F5. The shipped single-path loss law is the "legacy" age gate.** With the
  shipped gates (`RWM_RECOV_MP_LAW` on, `RWM_RECOV_SP` off,
  `gates.rs:366–371`), `serve_gaps` behaves as follows:
  - N > 1 runs the RFC 9002 9/8 time threshold plus the packet threshold.
  - N = 1 falls to `legacy_age_ripe`, srtt/2 on the pooled clock
    (`net/sender_phases.rs:892–961`).

  That is a path-count-selected law (class P below), and it is not a deletion
  candidate. The ledger has it as C04, and drift D lists it against the
  page's "loss detection" row.
- **F6. A shared law already carries a step.** `p_fec_normal`'s cut
  (`raptorpath-math/src/lib.rs:886–888`; audit W22, ledger R03) reaches the
  engine's shed budget through `residual_loss_after_fec` (D11). Sharing a law
  also shares its defects, so D11 moves only together with the R03 verdict's
  bounding test.

## 1. Inventory of engine decision logic

Blocker key:
- **I**: a clock read or a stored `Instant`: `Instant::now()`,
  `.elapsed()` (`control/anchor.rs:94`), or the scheduler's injectable
  `Clock::now()` (`scheduler/clock.rs:11`, read at `copa.rs:616`, `:748`,
  `:820`, `:852`, `:943`, `:1264`, `:1436`, `path.rs:127`, `:497`, `:1119`,
  `:1186`, `scheduler/mod.rs:382`, `:413`, `:443`).
  - *Unverified here:* `Instant` compiles on `wasm32-unknown-unknown`, and
    `now()` panics at run time (std's unsupported-time backend). This is
    std's documented behaviour; step 0.1's wasm32 check does not catch it,
    and the deny-list does.
- **E**: env or gate read at call time (`gates::get()` is a process
  `OnceLock`, `gates.rs:86`).
- **T**: tokio. **Q**: a quinn/transport handle. **A**: atomics or `static`
  gauges. **R**: `rand::random()`. **O**: `info!` or a gauge side effect
  inside the decision.

Class key (from the wasm audit, plus P):
- **A**: a continuous law, or a gate that acts as a factor inside one
  expression.
- **B**: a hint, δ or ρ threshold that selects a code path (the forbidden
  kind).
- **C**: a predecessor or ablation arm selected by an env gate.
- **P**: a path-count predicate that selects a law.

| # | decision | site | pure today? | reads | wasm blockers | class | fate |
|---|---|---|---|---|---|---|---|
| D1 | pooled store cap law | `net/store_cap.rs:76–96` (`pooled_store_cap`), `:177–200` (unclamped), `:162` (gain) | yes, by signature | values only | none in the fn; **E** via `codel_setpoint_q` → `delta_price` (F1) | A (sum_cap, delta_cap are factors); ledger F01: measured pinned at its N·2048 clamp | **move** the engine side (step 2.3); the model adopts it only after F01's verdict (5.1) |
| D2 | q(δ), b(δ) | `store_cap.rs:130`, `:294–320` | no: `delta_price` reads `gates::delta_override` | the hint, env | **E** | A (b is a `raptorpath_math` form already) | move with δ as an input (2.3), F1 fixed as its own step (2.4) |
| D3 | store-cap selection chain | `net/sender_phases.rs:60–236` (`refresh_store_cap`) | no | `Scheduler` (live paths, b̂·m̂, cwnd), `pol`, `now_us()` | **O** (`[SF]`, `[3T]`, `SumCapGauge`, `DeltaCapGauge`), time | C: `copa_feed.is_some()` picks Σcwnd (`:85`), the `three_term_on` arm (`:179`) and the `store_paths=0` chain (`:209`) | extract the shipped chain only (3.2); Σcwnd and three-term stay in the engine (both are slated to leave, w3 §4 B5 and Group B) |
| D4 | flow-control gate | `net/mod.rs:2388–2397` (`tx_paused = reliable && (store_len >= cap \|\| cwnd_full)`) | inline expression | the loop locals | none | **B** (`reliable` is a ρ = 1 bit) | stays until the ρ-keyed split is resolved (Stage 3.4 debt) |
| D5 | snapshot order | `net/snapver.rs:39–77` (`SnapOrder`, `snapver_judges`, `snapver_coalesce_keeps`), `:120` (`judged_gaps`) | yes | `AboveReport` (`net/sack.rs:327`) | none; `take_gap_batch` (`:87`) takes a **T** `mpsc::Receiver` | A | move the law; the dequeue stays in the engine. **Engine-only** under the stop rule (the model has no out-of-order reports) |
| D6 | time threshold | `net/recovery_laws.rs:118` (`mp_time_threshold_split`), `:225` (`mp_hole_ripe`), `:249` (`time_threshold_ripe`) | yes (µs `u64` in, already a seam) | values | none | A; `mp_hole_ripe` carries a **P** bypass at N ≤ 1 (`:229`) | move `mp_time_threshold_split` and `time_threshold_ripe` (2.1); `mp_hole_ripe` and its N bypass stay in the engine until owner question 1 |
| D7 | packet threshold | `recovery_laws.rs:148`, `:166`, `:177` (`mp_ingest_evidence`), `:206` (`mp_packet_threshold_fires`) | yes | `SeqRing<u32>` (`net/seq_ring.rs:29`), `HashMap` | none | A | move with `SeqRing` (2.1) |
| D8 | age gate, pooled clocks, cooldown, tail sweep, hole refresh | `recovery_laws.rs:266`, `:277`, `:308`, `:317`, `:323`, `:331`, `:340`; constants `net/mod.rs:241–251` | yes | values | none | A; the age gate is the shipped N = 1 law (F5) | move (2.1) |
| D9 | per-seq gap verdict | `sender_phases.rs:804–961` inside `serve_gaps` (`:614`) | no | `SenderState` maps, the scheduler's clocks, `stats.fec.total_source_symbols` (**A**; one writer, `emit_source.rs:425`, same task, so a per-report snapshot is bit-identical) | **T** (`nack_rx`), **Q** (`transport.send_symbols` `:1051`; its result feeds only `record_correction`, no decision), **O** (`mpd_*`, `mp_thr_of` bumps `mpd_pf_*` `:726–730`), **E** (`charge_recovery_active` `:1066`) | A + **P** (`mp_n_paths > 1` `:892`, `recov_sp && n ≤ 1` `:931`) + C (`recov_sp`) + **B** (`if reliable` clamp `:795`, path choice `:974`) | extract the per-channel verdicts (cooldown, shed, time threshold, packet threshold, age gate) as core functions returning a cause (3.1). The channel **selection** (the N split, the `recov_sp` arm, the `reliable` arms) and the I/O stay in the engine wrapper until owner question 1 |
| D10 | NACK repair margin | `sender_phases.rs:1106–1112` (`ceil(retx · max ε̂)`) | inline | the scheduler | none | A | extract (3.1) |
| D11 | δ-honest shed | `net/shed.rs:31` (`shed_deadline_us`), `:39` (`shed_allowed`); `control/fec_rate.rs:784` (`residual_loss_after_fec`) | yes | values | none | A, but it inherits R03's step (F6) | move the engine side (2.2: the same code, so the same defect); the model adopts it only after R03's verdict (5.2) |
| D12 | span parameters A\*, D, Δ | `net/emit_source.rs:562–600` inline | no (inline in the emit block) | `est.rtt()` (misnamed `rtprop`, w3 R5), the A\* anchor (**I**, `control/anchor.rs`) | **I** through the anchor | A | engine calls the phase-6 forms (step 1.2) |
| D13 | generation pipeline depth M\* | `net/mod.rs:475–482` (`gen_pipe_depth`) | yes, private | values | none | A | engine calls the phase-6 form (1.2) |
| D14 | rate law snapshot | `control/fec_rate.rs:451–528` (`rate_snapshot`), the mix in `RateSnapshot::rate` | the evaluation is shared already (`raptorpath_math::controller_rate`); the snapshot reads a `LossEstimator` | estimator state, `effective_bulkness` (`:379`) | the estimator is **I**, **E** | A (r(β) mix, both terms always computed) | already level 1; it becomes a level-2 decision once the estimator is shared (4.2) |
| D15 | placement softmax | `scheduler/place.rs:85–100` (`place_symbol`), `:154` (`place_probs`), `:273` (`place_e`), `:288` (`place_costs`) | no | `Scheduler` paths, `place_temperature()` (**E**, `:22`) | **R** (`:90`, `:146`), **E** | A; `place_repair_spare_path` (`:119`) is C (`RWM_XPATH_REPAIR`, Group A) | extract with `u: f64` and `T` as inputs (3.3) |
| D16 | ρ < 1 path picks, Realtime duplicate | `net/mod.rs:1382–1393`; `emit_source.rs:466` (`protocol_hint == Realtime`) | — | — | — | **B** | never moved as is (Stage 3.4 debt, w3 §6) |
| D17 | per-path estimators | Copa `scheduler/copa.rs` (19 `Instant` sites; `queue_target_mult` `:314` is a hint table), `LossEstimator` `control/estimator.rs` (`Instant::now()` at `:296`, `:299`, `:311`, `:337`), `SendRateAnchor` `control/anchor.rs` (atomics, `Instant`) | stateful | samples and the clock | **I**, **E**, **A** | Copa's queue target is B (hint table) | `LegState` is born seam-clean in the core (4.1, W3.2); the Copa controller is not moved (W3.2 B5 removes it) |
| D21 | proactive repair budget | `control/fec_rate.rs:678–735` (`TaperBudget::accrue`: grant = min(owed, max(desire, rate), spare, 1), owed ≤ max(r·W, 1)); its seat `net/emit_source.rs:529–560` (flat rate = min(r, spare), `spare_capacity` `scheduler/mod.rs:568`) | `accrue` is pure state + values; the seat is not | rate, taper (q, decay) from the estimator, W, spare (Copa cwnd, w3 B2) | none in `accrue`; the seat reads **I** through the estimator | A; `RWM_TAPER_R=0` density arm (`:550`) is C | move `TaperBudget` and `TaperFunction` (2.2); `spare` stays an input (W3.2 B2 retargets it) |
| D22 | repair-rate cadence, debt reduction | `net/emit_source.rs:878` (`cadenced_repair_rate`, `RepairRateCache` `fec_rate.rs:211`); debt reduction `sender_phases.rs:1172`, `:1328` | the cache is keyed on `now_us` | the worst-ε̂ path, `now_us` | **I** (`Instant::now()` `:895` for the eval-ns gauge, **O**) | A | move the cache (2.2); the ns gauge stays outside |
| D23 | cumulative-ack advance | `sender_phases.rs:1225` (`on_ack_advance`): pruning, χ publication, W\* target via `raptorpath_math::derive_window` | bookkeeping plus shared laws | `SenderState` maps | **O**, **E** | the `if generation` pruning (`:1338`) is C (the generation pipeline) | not moved: bookkeeping, not a decision. W\* and χ are already level 1 |
| D24 | generation emission | `sender_phases.rs:273` (`emit_generation_coded`) | no | generation maps, transport | **Q**, **A** | C (the generation pipeline, `RWM_GEN_PIPE`, paper §5.8) | out of scope: the model runs the window pipeline only. M\* (D13) is the one generation law the model reads |
| D18 | gates and env | `gates.rs:86` (`get`), `RuntimeGates::resolve`; `net/sender_policy.rs:297` (`SenderPolicy::resolve`, already resolve-once) | — | env | **E** | — | stays in the engine; it builds `CoreConfig` (§3) |
| D19 | `[HOLD]` | `net/holddown.rs` | — | — | — | the gauge only ("observation only", `:1`); no decision | stays in the engine |
| D20 | `[QCLK]` quantile clock | `net/recovery_clock.rs:47`, `:70` | yes | values | none | A (a gauge law, not on the default path) | optional, with D8 |

Not inventoried here: receiver-side decisions such as the hole refresh
(D8's `hole_nack_refresh` is its law) and the receiver hold
`shed_recv_hold` (`net/shed.rs:58`), which is two laws behind one signature
(ledger S05, drift D). The same classes and the same stop rule apply when
they are taken up.

## 2. What the model re-implements or approximates

Grep of `raptorpath-wasm/src/lib.rs`. These are only the rows the core
boundary touches. Value-level drift belongs to `viz-drift`, and its tables
are cited as drift A–E.

**A. Already shared.** `raptorpath-wasm` calls `raptorpath-math`:
`controller_rate`, `span_horizon_b`, `zeta_of_delta`, `bulkness_of_delta`,
`p_fec_normal`, `completion_exposure`, BOCD and GE. `math::LossEstimator` and
`math::RlcEncoder/Decoder` are forks, not shared code (F3).

**B. Local closed forms, the phase-6 set, each against its engine expression.**

| model (`raptorpath-wasm/src/lib.rs`) | engine | shape agrees? |
|---|---|---|
| `span_deadline_d` `:179` = `min(b·RTprop, 2·RTprop)` | `emit_source.rs:577` (same f64 expression); `shed.rs:32` (`min(b,2)·RTprop` truncated to µs, then `min 2·RTprop`) | yes; the µs truncation is an engine-internal second copy |
| `span_width_a_star` `:186` = `clamp(rate·D, 1, W)` | `emit_source.rs:578` = `clamp(ceil(rate·D), 1, W)` | **no**: `ceil` |
| `pipeline_depth_m_star` `:195` = `clamp(ceil(rate·2·RTprop/A_q)+1, 2, 32)` | `net/mod.rs:475` (the same, plus a cold guard that returns 2) | yes |
| `trailing_offset_delta` `:205` = `clamp(ceil(rate·J), 1, 64)` | `emit_source.rs:580` | yes |
| `shed_budget_residual` `:217` = `ε·(1−P_fec)`, ε clamped to [0, 1] | `fec_rate.rs:784`: returns 0 for ε ≤ 0 or ε ≥ 1, then clamps | differs at ε = 1 |
| `sim_tail_target_of_delta` `:247` | none (the model's own interpolation; bounded by `test_sim_engine_tail_anchor_divergence_is_bounded` `:2229`) | model-only |

**C. Decisions the model approximates (level-2 candidates).**

| model site | engine decision | relation |
|---|---|---|
| `STORE_PER_PATH = 512` constant (`:352`), gate at `:1303` | D1 at the knee `RWM_STORE_PATH_POOL` = 2048 (`gates.rs:323`), law (1 + q(δ))·Σ b̂·m̂ | the divergence is bounded (model side only) by `wasm_pool_is_constant_per_path_whatever_the_dial_and_bdp` (78a898a5), ledger M01. The stop rule allows sharing it, because the model has per-path capacity and RTT, so b̂·m̂ is computable. But the engine law is measured pinned (F01, drift C), and drift C's item 3 says not to model the unshipped replacement (`store-law.md` rev 6), so adoption waits on that verdict |
| per-path RTT hole clock (`:1454`, "wall #8"), P_lost-gated retransmit with RNG (`:1205–1229`) | D6–D9: 9/8 time threshold at N > 1, srtt/2 at N = 1 (F5); packet threshold; cooldown | the model uses its own clock at every N and a P_lost coin; the engine uses neither. The time threshold is shareable. The packet threshold needs per-path delivered seqs, which the model has in its feedback queues. The coin is an idealisation (owner question 3) |
| goodput-proportional WRR (`:628`), best-goodput correction path (`:608`) | D15 placement softmax | the model's own law; sharing needs `place_e`'s inputs (s_i, ŵ, I_i, the drain term), all model state. Owner question 4 |
| T_cut age eviction at ρ < 1 (`:1401–1420`) | D11 shed at D(δ) within the 1 − ρ residual | different laws for the same job. Shareable (inputs: age, D, ε̂, r, A\*) |
| `math::LossEstimator` (tick) | `control/estimator.rs` (`Instant`) | fork (F3); unify in 4.2 and 5.3 |
| repair debt: `debt = min(debt + r + completion, 8)` (`:1358`), with `completion_debt_increment` (`:705`) and a repair when `debt ≥ 1` (`:1289`) | D21 `TaperBudget::accrue` | different laws for the same job: the cap 8 against max(r·W, 1), and no spare or taper shape in the model. Shareable (inputs r, q, W, capacity). The completion term is the phase-6 candidate in ADR-0076 |
| per-slot priority in `step()` (`:1186–1300`): retransmit, then repair, then source | the engine loop's phase order (`net/mod.rs:1420` `run_window_sender`: store cap, gaps, emission) | **model-only**: the loop is the engine's I/O schedule, which the stop rule excludes |

## 3. Proposed crate boundary

**Name: `raptorpath-core`.** It is the sans-IO decision core, in the
`quinn-proto` sense. It depends on `raptorpath-math` only; `gf256` comes in
through math. `raptorpath` and `raptorpath-wasm` both depend on it.

Why a new crate rather than `raptorpath-math`:
- Phase 6 admits only branch-free formulas into math (ADR-0076, Decisions 3).
- Decisions carry state structs (`SnapOrder`, evidence maps, `LegState`) and
  verdict enums.
- Keeping the two apart keeps math's "only closed forms" rule checkable.

**What moves.** Every row marked move or extract in §1: D1, D2, the shipped
arm of D3, D5's law, D6–D8 minus `mp_hole_ripe`, D9's per-channel verdicts,
D10, D11, D15's law, D21, D22's cache, and D17 as `LegState` (with W3.2). The engine re-exports them under their current paths
(`pub use raptorpath_core::recovery::*;` in `net`), so
`raptorpath::net::*` callers such as `tests/recovery_bench.rs`,
`tests/formula_agreement.rs` and `net::tests::law_shape` compile unchanged.

**Seams.**
- **Clock.**
  - Laws that take µs `u64` today (D6–D8, D11) keep `now_us: u64`.
  - Stateful estimators become generic over a time type:

    ```text
    trait Time: Copy + Ord + Add<Duration, Output = Self> + Sub<Self, Output = Duration>
    ```

    `std::time::Instant` satisfies it, so the engine instantiates
    `LegState<Instant>` and the code it compiles is the same expression. The
    model implements `SimTime(u64 ns)`.
  - No `now()` exists in the core. Constructors take `now: T`, which
    replaces `estimator.rs:296` and `:299`. `record_batch` without `_at`
    (`:311`, `:337`) stays an engine wrapper.
  - The existing `Clock` trait (`scheduler/clock.rs:11`, `WallClock` and
    `MockClock`) stays in the engine. `Scheduler` and `PathState` read it
    once per entry, as they do today, and pass `now` into the core. The core
    never holds a `Clock`. The model's tick is its clock.
  - Converting to µs instead would not be bit-identical: the ns `Duration`
    arithmetic in Copa, the estimator and the anchor would round.
- **Config.**
  - `CoreConfig` is a plain struct: the dial values δ and ρ as numbers, the
    factor-valued gates (`sum_cap`, `delta_cap`, `store_bdp_gain`), and the
    knee, floor and temperature. Gates that select a code path, such as
    `recov_mp_law` and `recov_sp`, are not in it.
  - The engine builds it inside `SenderPolicy::resolve`
    (`sender_policy.rs:297`), the existing resolve-once seat.
  - `CoreConfig::shipped()` is the single source of defaults. An engine test
    asserts that `RuntimeGates::resolve` with an empty env maps onto
    `shipped()`, so the model and the engine cannot hold different defaults:
    512 against 2048 becomes a compile-visible choice.
  - Code-path arms (class C) are not in `CoreConfig`. They stay engine-side
    wrappers that call the core's shipped function or their own.
- **State.** The caller owns every state struct (`SnapOrder`, the evidence
  map, `LegState`, `StoreCapState`'s cache). The core has no statics,
  atomics or `OnceLock`.
- **RNG.** Draws are inputs (`place_symbol(.., u: f64)`). The engine passes
  `rand::random()` at the same call, in the same order, so the draw stream is
  unchanged. The model passes its seeded xorshift.
- **Observation.** Decisions return what the gauges need, following the
  existing `mp_time_threshold_split -> (thr, floor_won)` pattern: verdicts
  carry a cause (`Verdict::{Suppressed(Cooldown|Law|Age), Shed, Retransmit
  {fast}}`). Counters, `info!` and the `[SNAPV]`, `[HOLD]`, `[SUMCAP]` and
  `[DCAP]` gauges stay in the engine and are fed from the verdict. A pure
  twin such as `mp_thr_pure` (`sender_phases.rs:740`) is then not needed.
- **std, not `no_std`.** `wasm32-unknown-unknown` has std (alloc,
  collections, `Duration`, f64 math). `no_std` would add `alloc` and `libm`
  plumbing for no gain. Forbidden in the core: `Instant::now`, `.elapsed()`,
  `SystemTime`, `std::env`, `std::thread`, `tokio`, `quinn`, `tracing`,
  `rand`, statics and atomics.
- **Enforcement** (Phase 0), three gates:
  - `cargo check -p raptorpath-core --target wasm32-unknown-unknown` in
    `build_visualizer.sh`.
  - A source deny-list test in the crate that walks `raptorpath-core/src`
    for the forbidden tokens and asserts it visited every file, so it cannot
    pass vacuously (ADR-0076 phase 5.1).
  - The same no-mode-switch scan the mjs gates apply: no `ProtocolHint` type
    in the core at all, so a hint can only arrive as a δ number.

**How the engine calls it.** The call sites are unchanged apart from the
argument plumbing: values out of `Scheduler` and `SenderState` in, a verdict
out, and the engine executes the verdict (send, stats, gauges). `serve_gaps`
keeps its I/O loop and its channel selection, and calls the core's
per-channel verdicts per seq.

**How wasm calls it.**
- `Simulation` owns `CoreConfig::shipped()` with its dials applied, plus the
  core state structs.
- It feeds them from its idealised link state, with `SimTime` from the tick.
- It executes verdicts on the simulated link.
- No core type crosses the `wasm_bindgen` boundary; the getters stay
  numeric.

## 4. Phased order

Each step is one small refactor and one commit.
- **E** steps change engine bytes with zero intended behaviour change.
- **M** steps are deliberate model changes. Each carries
  `GOLDEN_CAPTURE=1`, the mjs gates and `test_continuum_one_law_across_the_dial`,
  and says so in the commit.
- No step is both.

**Proof ladder for E steps** (cheapest first, stopping at the first that
settles it):
1. **Release `.text` hash identical** before and after (status §22
   Amendment 5's method). Release is `lto = true` with `codegen-units = 1`,
   so a pure cross-crate move often keeps the bytes, although function
   layout may still move them. `.text` does not cover constants in the data
   sections (`.rodata`), so the check hashes those too.
   - ADR-0076 requires the §41c guard and the ns bench for **any**
     engine-byte change. Whether byte identity may waive the guard is
     owner question 9. Until the owner rules, every batch below runs its
     guard.
2. **Pins green.** These pins exist today:
   - `recovery_bench_fixtures_pin_the_plane` and
     `the_shipped_recovery_clocks_are_pinned_absolutely`
     (`tests/recovery_bench.rs:341`, `:538`);
   - `tests/formula_agreement.rs`;
   - `net::tests::law_shape`;
   - `place_costs_match_the_pinned_wired_table` (`scheduler/tests.rs:2787`);
   - `hotpath_pin`;
   - the snapver tests.

   New: `decision_trace_pin`, the `hotpath_pin` pattern. It runs a scripted
   input sequence through each decision about to move and records a digest
   of every output. It is captured on the untouched tree in step 0.3, and
   W3.2's A0 `leg_state_trace_pin` is its estimator half.
3. **Host ns bench** for the hot paths: `place_costs_ns`
   (`scheduler/place_alloc_tests.rs:78`); a new per-seq channel-verdict ns
   bench; the estimator per-sample cost.
4. **§41c guard** (12 + 12 rows at c1s, c1d, c2 and c8). As in W3.2
   Phase A, one guard covers a batch of steps whose pins all held,
   expected WITHIN. A step whose pin moved gets its own guard.

| step | kind | what | proof | guard | depends on |
|---|---|---|---|---|---|
| 0.1 | infra | empty `raptorpath-core` crate, the wasm32 check, the deny-list test; add it to `src_manifest` (`tools/l1/loopdiag_run_all.sh:435–440`), `tools/l1/test_harness_paths.py:77` and `tools/l1/test_stubrun.py:179`, `:248`, `:270` | builds | none (no engine bytes) | — |
| 0.2 | infra | make the source-walk tests non-vacuous and extend them to the core (ADR-0076 phase 5.1; the walks at this base: `raptorpath/src/gates.rs:526`, `readout.rs:68`, `transport/quic.rs:1296`, `net/emit_burst.rs:271`, `net/tests.rs:5075`, `:6193`) | tests | none (`cfg(test)`) | — |
| 0.3 | test | capture `decision_trace_pin` on the untouched tree | — | none | — |
| 0.4 | doc | the owner accepts the §1 class column (the audit precondition) | — | — | — |
| 1.1 | M | ADR-0076 phase 6 as approved: the span forms move into math **unchanged** (A\* unrounded), and the model calls them | goldens re-captured (expected unchanged; a pure move) | none | phase 6 |
| 1.2 | E | the engine calls the math forms at `emit_source.rs:576–580`, `net/mod.rs:475` and `shed.rs:31`. Each engine seat keeps its own rounding: A\* = `(math::span_width_a_star(r, D, W).ceil() as u64).clamp(1, W)`, which equals today's `clamp(ceil(r·D), 1, W)`. Both give 1 for NaN (`NaN as u64 = 0`, then clamped). The seat keeps today's `Ord::clamp` panic at W = 0. The µs D stays a named µs wrapper | ladder 1–3 | batch α | 1.1 |
| 2.1 | E | move D6–D8 except `mp_hole_ripe` (+ `SeqRing`, the recovery constants) into `core::recovery`; the engine re-exports | ladder 1–2 (recovery_bench fixtures) | batch α | 0.x |
| 2.2 | E | move D11 (`shed_deadline_us`, `shed_allowed`, `residual_loss_after_fec`), D21 (`TaperBudget`, `TaperFunction`) and D22's cache | ladder 1–3 | batch α | 1.2 |
| 2.3 | E | move D1, D2 with δ as a number: `codel_setpoint_q(b, b_lo, b_hi)`. The engine still passes `delta_budget_b(Realtime)` and `delta_budget_b(Bulk)`, override included, so the values are identical, the F1 NaN included | ladder 1–2 (law_shape, formula_agreement) | batch α | 0.x |
| 2.4 | E, **behaviour** | the F1 fix: the engine passes `b_lo`, `b_hi` from the hint **table** (`span_horizon_b(50)`, `span_horizon_b(0.005)`); red-first test with `RWM_DELTA` set. The default-cell values are unchanged by construction (law_shape stays green); the bytes change | its test + law_shape | batch α's guard | 2.3 |
| 2.5 | E | move D5's law (`SnapOrder`, `snapver_*`, `judged_gaps`, `AboveReport`); `take_gap_batch` stays | ladder 1–2 | batch α | 0.x |
| — | VM | **guard α**: one §41c run for 1.2–2.5 (waived only if owner question 9 allows it and ladder 1 held for every step) | | **α** | |
| 3.1 | E | extract the per-channel verdicts + `nack_margin` from `serve_gaps` **inside the engine** first, gauges fed from the verdicts, then move them to the core. The channel selection (N split, `recov_sp`, `reliable`) stays in `serve_gaps` | pins + new verdict ns bench | batch β | 2.1, 2.5 |
| 3.2 | E | extract the shipped store-cap chain from `refresh_store_cap` (the gauges stay outside), then move | pins (law_shape, `store_cap_bench`) | batch β | 2.3 |
| 3.3 | E | placement: `place_costs`, `place_probs` and the sampler with `u` and `T` as arguments, over a slice of per-path inputs instead of `&Scheduler`, then move | `place_costs_match_the_pinned_wired_table`, `place_costs_ns` | batch β | 4.1 for the s_i, ŵ inputs, or a temporary input struct |
| — | VM | **guard β** | | **β** | |
| 4.1 | E | `LegState<T: Time>` born in the core: W3.2 A1–A2 land there directly instead of in `scheduler/`, so the estimators move once | W3.2's `leg_state_trace_pin` | W3.2's Phase-A guard (one, expected WITHIN) | W3.2 A0 |
| 4.2 | E | `LossEstimator<T>` into the core; the engine instantiates `Instant`. The math tick fork stays until 5.3 | trace pin, estimator ns bench | batch γ (guard γ) | 4.1 |
| 5.1 | M | the model's store gate calls the core's store law, whichever one F01's verdict ships, over its b̂·m̂ (replacing `STORE_PER_PATH`); the 78a898a5 bound test becomes an agreement test | goldens, mjs | none | 2.3, the F01 verdict |
| 5.2 | M | the model's hole ripeness calls the core's channel verdicts on its per-path clocks; T_cut is replaced by D11; the model's debt by D21 | goldens, mjs; the continuity gates through ρ | none | 2.1, 2.2, the R03 verdict, owner questions 1, 3 and 5 |
| 5.3 | M | the model runs the shared `LossEstimator<SimTime>`; the math tick fork is deleted | goldens | none | 4.2 |
| 5.4 | M | placement softmax in the model (only if the owner says yes) | goldens, mjs | none | 3.3, owner question 4 |

Dependencies:
- W3.2 Phase A must not land `LegState` with `Option<Instant>` fields in
  `scheduler/` and then move them a second time. Coordinate before W3.2 A2
  (owner question 2).
- Phase 6 (1.1) gates 1.2 and 2.2.
- ADR-0076 phase 5 (the `net/` split) touches the same files. Do 2.x before
  it, or after it, but not interleaved.

## 5. Risks

1. **Hot-path cost.**
   - The release build has LTO with one codegen unit, so cross-crate
     inlining is available. Generic `T` code is monomorphised in the caller.
   - The risk is the test profile (opt 2, no LTO), where non-generic leaf
     laws need `#[inline]` to match. That affects test wall time, not the
     shipped binary.
   - Hot sites: `place_costs`, once per symbol; the estimator, once per
     sample; the gap verdict, once per reported seq. Each has an ns bench
     before and after its step (§4 ladder 3).
2. **A mode switch hidden in shared code.**
   - Only class-A code moves. B sites (D4, D9's `reliable` arms, D16,
     Copa's queue target) never move as they are. C arms stay in
     engine-side wrappers. P sites (`mp_hole_ripe`'s N ≤ 1 bypass, D9's N > 1 split,
     F5) wait for the owner (question 1).
   - A P branch moved into the core becomes something the model reaches at
     N = 1. It would then carry, at the path count, the same step the
     invariant forbids at a preset.
   - Enforcement in the core:
     - no `ProtocolHint` type;
     - a law-continuity test for every function that takes δ or ρ: absolute
       values at the presets and ±2 % nudges, the
       `delta_budget_b_is_the_dial_not_a_mode` pattern;
     - an absolute test at N = 1 and N = 2 for every function that takes N.
3. **Golden re-captures.** Every M step re-captures the six
   `test_multipath_n1_identical_golden` rows (`raptorpath-wasm/src/lib.rs:1835`)
   and re-runs both mjs gates. An E step never touches `raptorpath-wasm`, so
   the goldens there are a free control: a golden that moves on an E step is
   a defect.
4. **What stays model-only on purpose**, each with its bounding test:

   | idealisation | why it stays | bounding test |
   |---|---|---|
   | the link: GE channel, per-tick capacity, fixed RTT ticks | it is the truth side, never a decision | `test_channel_generation_loss_rate` (`:1984`) |
   | in-order per-path feedback (no return-leg reordering, so no snapshot versions) | stop rule; D5 stays engine-only | new: assert the model's gap view is newest by construction (no stale feedback ever applied) |
   | the P_lost coin on retransmits | idealised loss belief (owner question 3) | new: its retransmit count against D6–D8's on the same seeded trace, bounded |
   | the tail-target interpolation | the model's own map | `test_sim_engine_tail_anchor_divergence_is_bounded` (`:2229`) |
   | the constant pool, until 5.1 | — | `wasm_pool_is_constant_per_path_whatever_the_dial_and_bdp` |
   | goodput WRR, until 5.4 or forever | owner question 4 | new: the per-path share against the softmax's at the c2/c8 analog cells |

   The value-level drift rows themselves belong to `viz-drift`.
5. **Drift moves into the wrappers.** The engine wrapper around a core
   verdict still holds the I/O and the C arms. The `decision_trace_pin`
   digests the wrapper's outputs as well as the core's, so a change in
   either is red.

## 6. Proposed amendment to CLAUDE.md's scope rule (text only; the owner decides)

This replaces the first bullet of "Scope rules". It supersedes ADR-0076's
drafted bullet, which covers phase 6 only.

> - The visualizer (`raptorpath-visualizer/` + `raptorpath-wasm/`) is a
>   self-contained L0 model: never change engine *behaviour* for a
>   visualizer feature. Two things may still cross the boundary, each in its
>   own commit:
>   (a) a closed-form law the model evaluates moves to `raptorpath-math`
>   with its paper derivation (ADR-0076 phase 6);
>   (b) a per-tick decision the model takes moves to `raptorpath-core`
>   (`docs/derivations/shared-core-design.md`), I/O-free and wasm32-clean.
>   An engine-side step of either kind is a zero-behaviour-change refactor,
>   proven by the decision pins, the host ns bench and a §41c guard. A
>   model-side step is a
>   deliberate model change with a `GOLDEN_CAPTURE=1` re-capture.
>   Only class-A code moves (`shared-core-design.md` §1): no hint type, no
>   δ/ρ- or path-count-selected branch, and no env read enters either
>   crate. Simulation plumbing (link, RNG, stepping, hint selection) stays
>   in `raptorpath-wasm`.

Add to "Enforcement":

> - `raptorpath-core`: `cargo check --target wasm32-unknown-unknown` and
>   the source deny-list test run in `build_visualizer.sh`; a failure fails
>   the build. Every core function taking δ or ρ carries the preset
>   ±2 % continuity test.

## 7. Open questions for the owner

1. **Path-count laws (class P).** The shipped recovery runs srtt/2 at N = 1
   and the 9/8 time plus packet threshold at N > 1 (F5).
   - (a) Should one expression at every N, as ADR-0072 requires of the store
     law, become a precondition for moving D6–D9?
   - (b) Or may the core carry the N split as it is, so the model inherits
     it?
2. **W3.2 coordination.** Should `LegState` be born in `raptorpath-core` and
   generic over `T` (recommended), or land in `scheduler/` first?
3. **The P_lost coin.** Does the model's coin stay as an idealisation, or
   does the model run the shared time and packet thresholds (step 5.2)?
4. **Placement.** Should the model adopt the softmax (5.4), or keep goodput
   WRR as a declared idealisation?
5. **T_cut versus shed.** At ρ < 1 the model gives up at T_cut and the
   engine sheds at D(δ) within 1 − ρ. Should the model take the engine's
   law? This is the same call the drift audit's item 5 already has pending
   (W14 together with V5/V6), so one ruling covers both.
6. **F1.** Fix it as step 2.4, or before this work?
7. **F2, dead code.** Delete `raptorpath_math::FecRateController`
   (ADR-0073 route)?
8. **ADR.** The crate boundary is a hard-to-reverse decision. Should an ADR
   (next free number) record it once this design is accepted?
9. **Byte identity as a proof.** ADR-0076 requires the §41c guard and the
   ns bench for every engine-byte change. May an identical release `.text`
   and `.rodata` hash waive the guard for a pure move? This design assumes
   it may not until the owner rules (§4 ladder 1).
10. **The shed residual at ε = 1.** The model gives ε·(1 − P_fec) and the
    engine gives 0 (§2 B). Which form does the shared function take? Phase 6
    moves the model's form unchanged, and 1.2 keeps the engine's at its
    seat until this is ruled.
