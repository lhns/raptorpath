//! The window sender's RESOLVE-ONCE policy: the derived constants that fix
//! `run_window_sender`'s behaviour for the lifetime of a tunnel.
//!
//! History (net seam pass 2, 2026-08-09): `run_window_sender` opened with
//! ~1,300 lines of setup that turned the `RuntimeGates` env surface, the
//! protocol hint and the four pipeline booleans (`reliable`, `coded_only`,
//! `generation`, `systematic`) into ~56 locals which are then NEVER
//! reassigned. Because they were locals, every function extracted out of the
//! sender had to take them as parameters — the seam map's third blocker.
//! They are now the fields of [`SenderPolicy`], resolved once by
//! [`SenderPolicy::resolve`], in the `RuntimeGates::resolve()` shape
//! (`src/gates.rs`).
//!
//! BEHAVIOUR CONTRACT: this is a change of WHERE a value is computed, not of
//! what it is. Every field's initializer is the ORIGINAL expression, moved
//! VERBATIM with its comment, in the ORIGINAL relative order, with the
//! original clamp / `unwrap_or` chain and the original gate composition. The
//! immutability that licenses the move is STRUCTURAL, not a reading: all 56
//! were `let` WITHOUT `mut`, so Rust already guaranteed they could not be
//! reassigned. `RuntimeGates` remains the sole env reader (nothing here calls
//! `std::env`), and the mode-dependent defaults `gates.rs` deliberately left
//! at the use site (`RWM_GEN_R`, `RWM_REACT_CAP`, `RWM_INFL_BDP`) are
//! resolved here against the SAME mode inputs they were resolved against
//! inline. Every expression is PURE — reads of `gates` fields, the six
//! caller inputs, and each other — so computing them together at the top of
//! the sender instead of spread through its setup cannot change what any of
//! them holds.
//!
//! The mechanism-liveness `info!` echoes stay in `run_window_sender`: they
//! are startup SIDE EFFECTS, not policy, and hoisting them would reorder the
//! log against the `WindowStart` broadcast and the stall-witness spawn. The
//! one non-pure member of the old block, the span-law trace's t0
//! `span_diag_start_us = now_us()`, is therefore NOT resolved here — the
//! sender rebinds `pol` with it at the exact point in setup the stamp was
//! always taken.
//!
//! NOT covered here (deliberately) — everything `run_window_sender`
//! REASSIGNS after setup, which is why it stays a local: the pacing token
//! buckets and their refresh stamps (`gen_tokens`, `src_tokens`,
//! `cc_rate_cached`, `cc_rate_ceiling`, …), the derived-depth and
//! dynamic-cap caches (`gen_pipe_m`, `gen_pipe_store_cap`, `dyn_store_cap`,
//! `dyn_infl_cap`, `wd_*`, `pa_*`, `ps_*`), the per-path account caps
//! (`percap_caps` / `percap_bounds` / `percap_rr` / `percap_k`, refreshed at
//! the dyn-cap cadence), `emit_batch_live` (RE-SCOPED every loop iteration
//! on the live-path count — the one member of the `RWM_EMIT_BATCH` family
//! that is NOT policy), the DIAG counter set, and the two DIAG t0 stamps.
//! The mutable EMISSION state lives in
//! [`SenderState`](super::emit_source::SenderState).

use super::{GEN_PIPE_MAX_GENS, MAX_WINDOW_SIZE, RELIABLE_STORE_MAX, shed_armed};
use crate::control::fec_rate::ProtocolHint;
use crate::gates::RuntimeGates;

/// Everything `run_window_sender` decides once and then only reads.
///
/// Grouped as the sender itself is: the caller's pipeline selection, the
/// generation stack, CC/pacing, the retention / flow-control laws, the
/// per-path account family, the unified span/shed laws, the recovery plane,
/// emission batching, and the instruments. The full rationale for each
/// value — the measurements, the ADRs, the removed experiments — lives on
/// its expression inside [`SenderPolicy::resolve`], where it was moved from.
#[derive(Debug, Clone)]
pub(crate) struct SenderPolicy {
    // ── The caller's pipeline selection ──────────────────────────────────
    /// Symbol payload size in bytes.
    pub symbol_size: u16,
    /// The (δ, ρ, r) named point the tunnel was opened at.
    pub protocol_hint: ProtocolHint,
    /// RWM Phase A: RETAIN-UNTIL-ACKED retention at the ARQ layer.
    pub reliable: bool,
    /// Generation-based coding (§16.3): fixed generations, per-seq ARQ OFF.
    pub generation: bool,
    /// Systematic + deficit-repair (§16.3 oracle): a submode of `generation`.
    pub systematic: bool,

    // ── The generation stack ─────────────────────────────────────────────
    /// Coded wire symbols (`coded_only || generation`).
    pub coded_wire: bool,
    /// `RWM_GEN` generation width in source symbols.
    pub gen_size: usize,
    /// `RWM_PIPELINE` generations concurrently in flight.
    pub pipeline: usize,
    /// `RWM_GEN_PIPE` ∧ generation: the derived-depth M* substrate stack.
    pub gen_pipe: bool,
    /// `RWM_MSTAR_ANCHOR` ∧ generation: the M* anchor-pair repair.
    pub mstar_anchor: bool,
    /// Generation-coding proactive overhead r (`RWM_GEN_R`).
    pub gen_repair_floor: f64,
    /// `RWM_GEN_RATE` coded pacing ceiling (symbols/s).
    pub gen_rate: f64,
    /// Bootstrap pacing floor before the ack-rate estimator has a sample.
    pub gen_rate_floor: f64,
    /// `RWM_GEN_INFLIGHT` in-flight coded allowance W.
    pub gen_inflight_window: f64,
    /// `RWM_OOO_RETAIN` ∧ generation: out-of-order retention decouple.
    pub ooo_retain: bool,
    /// `RWM_CODED_SRC`: coded emission clocked on the source send.
    pub coded_src_clock: bool,
    /// `RWM_NO_REACTIVE`: disable the deficit-driven reactive plane.
    pub no_reactive: bool,
    /// `RWM_XPATH_REPAIR` ∧ generation: repair to the max-spare path.
    pub xpath_repair: bool,
    /// `RWM_PROACTIVE_PACER` ∧ systematic: present-at-stall proactive repair.
    pub proactive_pacer: bool,

    // ── CC / pacing ──────────────────────────────────────────────────────
    /// `RWM_CC_PACE`: CC-rate pacing of the systematic source (Fix 1).
    pub cc_pace: bool,
    /// `RWM_CC_PACE_HR` headroom multiplier on the paced source rate.
    pub cc_pace_headroom: f64,
    /// `RWM_REACT_CAP` spacing scale (Fix 2); `0` = the legacy exempt arm.
    pub react_cap_cfg: f64,
    /// `react_cap_cfg > 0.0` — the bounded-reactive gate itself.
    pub react_cap_on: bool,
    /// `RWM_INFL_CAP` static in-flight cap (0 = off).
    pub infl_cap: u64,
    /// `RWM_INFL_BDP` gain on the BDP-derived in-flight cap.
    pub infl_bdp_gain: f64,
    /// `infl_bdp_gain > 0.0` — the dynamic in-flight cap gate.
    pub infl_bdp_on: bool,
    /// Per-path in-flight fullness (the #64 fix); rides `gen_pipe`.
    pub infl_percap: bool,

    // ── Retention / flow control ─────────────────────────────────────────
    /// Coding-window / retention width (§16.5 W_mp; `RWM_WINDOW`).
    pub win_cap: usize,
    /// Backpressure ceiling for the retention store (`RWM_STORE`).
    pub store_max: usize,
    /// The plain-reliable delay-based dynamic window cap is active.
    pub plain_dyn_cap: bool,
    /// `RWM_STORE_GAIN`: window = gain × BDP.
    pub store_bdp_gain: f64,
    /// `RWM_STORE_BOOT`: cap before the BtlBw anchor warms.
    pub store_boot_cap: usize,
    /// Floor so a transiently-tiny BDP estimate cannot strangle the pipe.
    pub store_cap_floor: usize,
    /// `RWM_STORE_PATHS` (task #84): path-scaled outstanding pool.
    pub store_paths_on: bool,
    /// `RWM_STORE_PATH_POOL`: per-live-path pool knee.
    pub store_path_pool: usize,
    /// `RWM_STORE_CAPW`: capacity-weighted outstanding pool.
    pub capw_on: bool,
    /// `RWM_POOL_ANCHOR`: pool-anchor honest dual-store law.
    pub pool_anchor_on: bool,
    /// `RWM_STORE_CAP_UNIFIED` (goal-gate "Store-Cap Triplication"): the
    /// plain dyn-store-cap phase's path set is `live_paths()` rather than
    /// the saturation-filtered `active_paths()`. Scoped to the plain
    /// dynamic cap — Copa-sole already reads `live_paths()`.
    pub store_cap_unified: bool,
    /// `RWM_THREE_TERM` (goal-gate "Three-Term Law"): the plain dynamic
    /// store cap is the composed three-term law
    /// (`net::three_term_store_cap`). Scoped to the plain dynamic cap, like
    /// `store_cap_unified`. Default OFF: the shipped tree is bit-identical.
    pub three_term_on: bool,
    /// The δ dial's deadline budget b(δ) at this tunnel's named point
    /// (`net::delta_budget_b`) — a NUMBER on a dial, resolved once, read by
    /// the three-term law's stall term. Not a mode selector: the law is
    /// continuous and monotone in it.
    pub delta_b: f64,
    /// The retention dial ρ the three-term law is evaluated at.
    ///
    /// This is a VALUE of the (δ, ρ, r) triangle's ρ axis, not a mode
    /// selector, and it is a constant here by SCOPE rather than by a
    /// branch: the plain dynamic cap exists only on the RETAIN-UNTIL-ACKED
    /// path (`plain_dyn_cap ⇒ reliable`), whose declared retention contract
    /// IS ρ = 1. `net::contract_stall_s` is continuous over ρ ∈ [0, 1] with
    /// both of its terms always computed, and is unit-tested at 21 points.
    pub contract_rho: f64,
    /// `RWM_STORE_SACK_RELEASE`: SACK-clocked store release.
    pub store_sack_release_on: bool,
    /// `RWM_PLACE_SLACK`: frontier-slack placement cost.
    pub place_slack_on: bool,
    /// `RWM_WIN_DECOUPLE`: window/inflight decoupling at N = 1.
    pub win_decouple_on: bool,

    // ── Per-path outstanding accounting (task #86, ADR-0058) ─────────────
    /// `RWM_STORE_PERCAP`: per-path accounts.
    pub percap_on: bool,
    /// `RWM_PERCAP_GUARD`: the delay-aware redirect guard.
    pub percap_guard_on: bool,
    /// `RWM_STORE_BORROW` (§16.22): bounded account borrowing.
    pub percap_borrow_on: bool,
    /// `RWM_HONEST_CAP` (+ `RWM_PLAIN_RS`): honest floor-clock caps.
    pub honest_cap_on: bool,
    /// `percap_on || (diag_on && plain_dyn_cap)` — under `RWM_DIAG` the
    /// account maps are maintained as a GAUGE even when the law is off.
    /// Behavior-inert by construction: every percap DECISION site keys on
    /// `percap_caps` NON-EMPTY (the "law engaged" signal).
    pub percap_track: bool,

    // ── The unified span / shed laws (§16.20.3, ADR-0064) ────────────────
    /// `RWM_UNIFIED`: trailing solvable-span proactive-repair placement.
    pub unified_span: bool,
    /// `RWM_ASTAR_ANCHOR`: the windowed-max send-rate A* anchor.
    pub astar_anchor_on: bool,
    /// `RWM_UNIFIED_SHED`: δ-honest shedding, EVICT path only.
    pub shed_on: bool,
    /// `RWM_TAPER_R` (#85): budget-conserving taper emission.
    pub taper_r_budget: bool,
    /// `RWM_MIN_R` (§16.5): experimental per-symbol repair-rate FLOOR.
    pub repair_rate_floor: f64,

    // ── The recovery plane ───────────────────────────────────────────────
    /// `RWM_RECOV_MP`: multipath recovery suppression.
    pub recov_mp: bool,
    /// `RWM_RECOV_MP_LAW`: the per-flight hole law under that umbrella.
    pub recov_mp_law: bool,
    /// `RWM_RECOV_SP`: single-path hole-law suppression.
    pub recov_sp: bool,
    /// `RWM_RECOV_MP_LIVE`: recovery clocks on `live_paths()`.
    pub recov_mp_live: bool,
    /// `RWM_PATIENCE_DERIVED`: the derived recovery-patience floor.
    pub patience_derived: bool,
    /// `RWM_SIDLE_DERIVED` ∧ diag: the second, derived stall gauge.
    pub sidle_derived: bool,

    // ── Emission ─────────────────────────────────────────────────────────
    /// `RWM_EMIT_BATCH` CONFIGURED (the per-iteration live scoping on the
    /// live-path count is `emit_batch_live`, a local — see the module doc).
    pub emit_batch_on: bool,
    /// `RWM_EMIT_BURST` pacer-quantum burst size (symbols).
    pub emit_burst: usize,
    /// Realtime packing: accumulate small packets into packed symbols.
    pub use_packing: bool,

    // ── Instruments ──────────────────────────────────────────────────────
    /// `RWM_DIAG` master gate.
    pub diag_on: bool,
    /// diag/unified-collapse: the span-law trace's own t0. NOT resolved by
    /// [`SenderPolicy::resolve`] (a wall-clock read, not a policy): the
    /// sender rebinds `pol` with it at the point in setup it was always
    /// sampled, so the stamp does not drift.
    pub span_diag_start_us: u64,
}

impl SenderPolicy {
    /// Resolve the sender's whole derived policy once, from the engine's
    /// `RuntimeGates` and the caller's pipeline selection.
    ///
    /// The body below is the original setup of `run_window_sender`, moved
    /// verbatim: same expressions, same order, same comments. `gates` keeps
    /// its name so not one of those expressions had to change.
    #[allow(clippy::too_many_arguments)]
    pub fn resolve(
        gates: &RuntimeGates,
        symbol_size: u16,
        protocol_hint: ProtocolHint,
        reliable: bool,
        coded_only: bool,
        generation: bool,
        systematic: bool,
    ) -> Self {
        // Generation coding emits coded wire symbols exactly like coded-only; the
        // difference is the coding UNIT (a stable generation vs the moving window)
        // and that per-seq ARQ is disabled below.
        let coded_wire = coded_only || generation;
        let gen_size: usize = gates.gen_size;
        let pipeline: usize = gates.pipeline;
        // Generation-coding proactive overhead r (coded per generation beyond K_G):
        // the encoder provisions each generation to ceil(len·(1+r)) coded before it
        // is only coded for recovery. Covers loss + the MDS margin. RWM_GEN_R env.
        // Systematic-repair provisions only the loss-FEC overhead r (the K base DoF
        // ride the wire as source), so its natural default is smaller than
        // coded-only's (which must also fund the K base). r ≳ 1.5·ε keeps windowed
        // repair ahead of loss (the oracle's provisioning floor; r < ε → DNF). At C8
        // ε_slow ≈ 4.8 %, so 0.15 clears both paths with margin. RWM_GEN_R overrides.
        // The DAPS chain (RWM_DAPS, _BDP, _PACE, RWM_PACE_ALL, RWM_RATE_SAMPLE,
        // RWM_PER_PATH_EST, RWM_DAPS_DEPTH) was REMOVED 2026-07-27 per the
        // DEPRECATION REGISTER (ADR-0065/0066): the original 2026-07-12 arc was
        // voided by the Methodology Audit (generation-inert era), and the live
        // re-ask ("Gen-ON Stack Ablation" 2026-07-13, generation actually ON)
        // measured the stack itself as the sym-C7 collapse (rate-sample −22%,
        // depth −17…−30%). Every surviving idea was re-derived better elsewhere:
        // per-path BDP cap + derived depth → RWM_GEN_PIPE's M* law (ADR-0064),
        // honest per-path anchors → ADR-0061 (the send-interval sampler the
        // CopaFeed/RWM_PLAIN_RS machinery keeps IS that fix — shared, retained),
        // per-path admission → the percap family (ADR-0058).
        // The RWM_FMTCP(+_WIN) decode-on-total composite was REMOVED 2026-07-27
        // per the DEPRECATION REGISTER: re-tested on the FULL clean substrate
        // ("C8-Aware Pool Law" battery, piggybacked arm) → CONFIRMED-REFUTED
        // (c7 18.30/18.98, c8 14.30/15.03 Mbit/s = ×0.11–0.20 of the same-session
        // default stack, both seeds, ≫σ) — the 2026-07-08 pathology was never
        // wall-tainted. Its forced sub-levers (RWM_REASM_BDP, RWM_OOO_RETAIN,
        // RWM_XPATH_REPAIR) survive as independent gates; the per-path in-flight
        // cap + derived win backstop it pioneered live on under gen_pipe/M*.
        // feat/gen-substrate-ceiling (RWM_GEN_PIPE, DEFAULT OFF ⇒ same-binary A/B;
        // shipped non-generation default byte-identical — every use is generation-
        // gated). The JOB-1 diagnosis: the L1 per-path ~10 Mbit/s generation
        // ceiling is the SUBSTRATE — quinn's loss-reactive Cubic window under the
        // datagram path (per connection = per path), COLLAPSED further by bare
        // generation mode's own standing queue (uncapped in-flight → RTT inflated
        // 3–5× → Cubic throughput ∝ 1/RTT). The L0 netem-shim bench (which
        // reproduces RTT/rate/GE-loss but hides them from quinn) measures the app
        // machine at 34 Mbit/s on the same c2 parameters — the wall is NOT the
        // app pipeline. This gate composes the app-side remedies so the substrate
        // sees a queue-lean, BDP-covering pipeline:
        //   1. per-path BDP in-flight cap (infl_bdp 1.5, percap) — queue ≈ 0,
        //      RTT ≈ RTprop (the mechanism behind DAPS's accidental +44% single);
        //   2. DERIVED pipeline depth M* (gen_pipe_depth above, #61's A*) —
        //      generations in flight cover BDP + one deficit round, recomputed
        //      from measured rate/SRTT (no fixed M);
        //   3. coded-emission budget clocked on the SENT frontier (the stalled
        //      cumulative ack must not freeze emission for the still-recovering
        //      oldest generation while M* fresh generations have budget);
        //   4. pace anchored to the windowed-MAX delivered rate (§16.15: the
        //      decode-clocked samples are mostly-low; the legacy decaying EWMA
        //      under-reads between generation decodes and throttles emission);
        //   5. once-per-SRTT deficit action (react_cap 1.0 — the known-good
        //      bounded reactive from the FMTCP-era arm).
        // The substrate CC itself is A/B-able independently via RWM_QUIC_CC (bbr)
        // in transport/quic.rs.
        // §16.20 (d): under RWM_UNIFIED the derived-depth law (M* =
        // ceil(rate·2·RTprop/G)+1, the large-δ limit of A*) is the DEFAULT for
        // generation mode; RWM_GEN_PIPE=0 still reproduces the fixed legacy
        // pipeline as the same-binary A/B arm.
        let gen_pipe = gates.gen_pipe && generation;
        // feat/anchor-hygiene (`RWM_MSTAR_ANCHOR`): the M* anchor-pair repair —
        // (a) the peer-report 50-ms pseudo-sample no longer pins the RTprop floor
        // (PathReport arm; hygiene rules 1+3), (b) the windowed-MAX delivered-rate
        // filter seeds from 500-ms buckets instead of 2-s ones (rule 1: the
        // anchor is live within ~1 bucket of the first acks). (Historic rule (c),
        // the derived (M*+2)·G replacement of the STATIC FMTCP win backstop, was
        // removed with the FMTCP composite 2026-07-27 — the derived-depth idea
        // ships as gen_pipe's M* law itself.)
        // DEFAULT ON (2026-07-21, "Consolidation" battery: plain subset inert
        // within sigma at every bulk cell on both seeds, tail crown unregressed;
        // the generation-gated knee evidence is 16.21's).
        let mstar_anchor = gates.mstar_anchor && generation;
        // feat/source-backpressure (RWM_SRC_BP) — REMOVED 2026-07-27 per the
        // DEPRECATION REGISTER: deferring the source into per-path pacing budgets
        // stalls the generation-fill pipeline (the source read IS the pipeline
        // clock — C8 −53% both seeds, "Source Backpressure" 2026-07-12; era
        // audit-classified UNCERTAIN). The mechanism space (per-path admission of
        // source) was re-asked BY the percap account family on live code with
        // gauges and lost for a named structural reason (ADR-0058); any future
        // gen-mode re-ask rides that family, not this code.
        // Generation-coding proactive overhead r.
        let gen_repair_floor: f64 = gates
            .gen_r
            .unwrap_or(if systematic { 0.15 } else { 0.20 })
            .clamp(0.0, 2.0);
        let gen_rate: f64 = gates.gen_rate;
        // Bootstrap pacing floor (symbols/sec): the rate used before the ack-rate
        // estimator has a sample (primes the first generation). Kept modest so the
        // startup burst can't overrun a bandwidth-limited link's datagram intake;
        // once the ack rate is known the pacing clocks to delivered goodput × 1.5.
        let gen_rate_floor: f64 = gates.gen_rate_floor;
        // ── Fix 1 (transport-substrate): CC-RATE PACING of the SYSTEMATIC SOURCE ──
        // PRIMARY high-RTT lever. The systematic source rides the DROPPABLE QUIC-
        // datagram path driven only by TUN-read intake, gated by a BDP-scaled
        // WINDOW (store_max / infl_cap) but NOT by a RATE. At high RTT the window is
        // BDP-sized, so the source is spent as one big BURST that netem/QUIC drops
        // faster than the receiver decodes — per-generation loss then exceeds the
        // ceil(len·r) proactive budget and the proactive-recovery fraction
        // COLLAPSES (0.95→0.23), forcing reactive round-trips (goal-gate "Proactive
        // FEC vs ARQ"). This paces the source at the measured LINK rate, smoothed
        // over the RTT with a SMALL burst, so no BDP-sized burst ever hits the wire.
        //
        // Rate signal: the delivered-goodput EWMA (`gen_rate_ewma`) is the achieved
        // BtlBw in generation mode — the true CC anchor. The Copa `cwnd` is NOT
        // usable here: window-mode WindowAcks do not drive `record_delivery`, so
        // cwnd is pinned at INITIAL_CWND and cwnd/SRTT would strangle the pipe. The
        // ack-clocked delivered-goodput EWMA already tracks the link and is what the
        // coded bucket uses; the source now shares it. A small headroom lets the
        // rate ramp without the 1.5× overshoot that itself overruns the datagram
        // path. Env-gated (RWM_CC_PACE) so the A/B baseline is byte-identical.
        //
        // feat/copa-wire-signal: DEFAULT ON under the wire-clocked Copa signal.
        // Copa's model assumes a PACED wire (the paper paces at 2·cwnd/RTT; our
        // §12.5 token bucket does the same for the block path), but under
        // RWM_QUIC_CC=passthrough quinn's own pacer derives from the engine
        // window — at Copa's Bulk operating point (cwnd ≈ BDP + 1/δ ≈ 5×BDP at
        // c2) that pacer never binds, the send process degrades to pure
        // ack-clocking, and each GE loss burst's recovery micro-stall idles the
        // bottleneck (MEASURED at the L1 c2 smoke: 55.7 → 67 Mbit/s from this
        // default alone, store no longer pinned at the cap, wire queue p50
        // 3–5 ms). RWM_CC_PACE=0 still forces it off (the #80 A/B arms are
        // reproduced by RWM_COPA_WIRE=0, under which this default is false).
        let cc_pace = gates.cc_pace;
        let cc_pace_headroom: f64 = gates.cc_pace_headroom;
        // ── Fix 2 (transport-substrate): BOUNDED REACTIVE under congestion control ─
        // The deficit-driven recovery loop was EXEMPT from the in-flight congestion
        // cap and re-emitted the reported residual on EVERY deficit report. At high
        // RTT the reports are ~RTT stale, so it re-sends the deficit faster than an
        // updated report can shrink it, its own recovery symbols overrun the pipe
        // and drop, the stale deficit persists, and it re-floods — MEASURED
        // recovery_coded 60 k–252 k symbols for a ~5 k-symbol object (up to 120×),
        // which DNFs at RTT200. Two bounds close the loop:
        //   (a) PER-GENERATION RTT SPACING. After emitting recovery for a
        //       generation, do NOT emit for it again for ~1 SRTT — long enough for
        //       those symbols to arrive and the receiver's NEXT deficit report to
        //       reflect them. This is the "send the deficit, wait ~RTT, re-evaluate"
        //       the design intended but never TIMED, so a stale periodic re-report
        //       could no longer trigger an immediate re-flood.
        //   (b) NON-EXEMPT from the in-flight cap. Reactive now also stops at
        //       `cwnd_full` (RWM_INFL_CAP) like proactive — it may not push the pipe
        //       past the congestion cap. The in-flight budget expires on the RTT
        //       timescale, so the frontier is still funded within a bounded delay
        //       (no permanent deadlock), it just cannot BURST past the cap.
        // Enabled by RWM_REACT_CAP (any value; the value optionally scales the
        // spacing — <1 = fraction of SRTT, >=1 = absolute µs). Unset = OFF (legacy
        // exempt behaviour), so Fix 1 measures alone and Fix 2 stacks on top.
        // gen_pipe defaults to once-per-RTT deficit coalescing (1.0·SRTT): "ONE
        // deficit feedback per RTT" — the #59/#60 lesson that a sub-RTT re-flood
        // of the fungible top-up defeats aggregation. RWM_REACT_CAP still overrides.
        let react_cap_cfg: f64 = gates
            .react_cap
            .unwrap_or(if gen_pipe { 1.0 } else { 0.0 })
            .max(0.0);
        let react_cap_on = react_cap_cfg > 0.0;
        // In-flight coded allowance W (coded symbols the pipe may hold ahead of the
        // decode frontier). MUST be ≥ pipeline·gen_size: coded symbols are striped
        // round-robin across the M active generations, so to let the FIRST
        // generation accumulate its K_G (and thereby decode → advance the ack that
        // grows the target) each of the M active generations needs ~gen_size coded
        // in flight at once. Below M·G the first generation never reaches K_G, ack
        // stays 0, and the target never grows — a startup deadlock. Default
        // (M+1)·gen_size (matches the source-retention store_max) plus decode/loss
        // slack. RWM_GEN_INFLIGHT overrides.
        let gen_inflight_window: f64 = gates
            .gen_inflight
            .unwrap_or((2 * pipeline * gen_size) as f64);
        // RWM Phase C (paper §16.5, the BANDWIDTH knob r): experimental
        // per-symbol repair-rate FLOOR. The Bulk χ glide drives r*→0 mid-stream
        // (§14.26), leaving the window systematic (not rateless-fungible), so a
        // heterogeneous slow path's source symbols are fixed positions the fast
        // path cannot decode around (the measured Phase B C8 wall). Raising r
        // makes the pooled window fungible so completion → K/Σg. Env-gated
        // (RWM_MIN_R, repairs per source symbol, e.g. 0.18 ≈ the slow path's
        // symbol share at C8); 0 = production default (unchanged glide). Test
        // instrument for the raise-r arm, not a shipped control law.
        let repair_rate_floor: f64 = gates.min_r;
        // ── Fix 3 (transport-substrate): OUT-OF-ORDER RETENTION DECOUPLE ──────────
        // Defect #3: generation backpressure caps the send frontier at ~store_max =
        // a few generations ahead of the CUMULATIVE (in-order) decode ack, so ONE
        // hole stalls the whole pipeline even under out-of-order delivery — throughput
        // ∝ generations/RTT = window/RTT, reproducing ARQ's serialization. This
        // raises the retention/backpressure window to `ooo_gens` generations so the
        // sender keeps sending (and proactively coding, via the send-frontier-tracking
        // `set_code_base` below) MANY generations past a stalled in-order frontier;
        // the stalled generation is recovered by the bounded reactive tail (Fix 2)
        // while everything above it completes out of order. Retention still drops on
        // the in-order ack (advance(ack+1)) so RELIABILITY IS UNCHANGED — the sources
        // of every not-yet-in-order-acked generation stay retained for reactive
        // recovery; memory is bounded by `ooo_gens·G`. Env RWM_OOO_RETAIN (value =
        // generation count, default 16; unset = OFF, byte-identical legacy).
        let ooo_retain = gates.ooo_retain && generation;
        let ooo_gens: usize = gates.ooo_gens;
        // Fungible frontier window sizing (§16.5, the FOURTH bound W_mp). A hole
        // at the frontier is raced by coded symbols that combine over the CURRENT
        // window; sustained Σg aggregation needs the window to span the cross-path
        // recovery horizon, W_mp ≳ Σg·(RTT_max+t_slack) ≈ 600 symbols at C8 — 3×
        // the systematic pipeline's MAX_WINDOW_SIZE=200, which §16.5 states would
        // "starve RWM at C8 by construction". Coded-only therefore widens the
        // coding window to W_mp (default 640, RWM_WINDOW override for the sweep);
        // the oracle (oracle_c8_fungible_wmp_window) confirms W≥384 reaches the
        // ×1.19 ceiling while W=200 does not. Systematic modes keep 200.
        let win_cap: usize = if generation {
            // Generation mode retains the whole in-flight pipeline: M generations
            // of G symbols (plus one for the currently-filling head). This is the
            // stable-anchor analogue of W_mp — every not-yet-decoded generation
            // stays retained (and keeps getting coded symbols) until it decodes.
            // Fix 3: RWM_OOO_RETAIN widens this to `ooo_gens` generations so the
            // send frontier can run far past a stalled in-order frontier.
            // gen_pipe: retention ceiling = the M* hard cap (the DYNAMIC intake
            // cap `gen_pipe_store_cap` below is what actually bounds the queue).
            let gens = if gen_pipe {
                GEN_PIPE_MAX_GENS + 1
            } else if ooo_retain {
                ooo_gens + 1
            } else {
                pipeline + 1
            };
            (gen_size * gens).clamp(MAX_WINDOW_SIZE, 1 << 20)
        } else if coded_only {
            gates
                .window_override
                .unwrap_or(640)
                .clamp(MAX_WINDOW_SIZE, 4096)
        } else {
            MAX_WINDOW_SIZE
        };
        // Fungible-frontier retention bound = the coding window itself. This is
        // the §16.5 W_mp bound doing double duty: the backpressure cap must keep
        // the SEND frontier within ONE window of the cumulative ack, so every
        // not-yet-decoded seq stays INSIDE the current coding window and is raced
        // by ongoing coded symbols (fungible in-window refill) rather than aging
        // out and forcing a congestion-throttled targeted ARQ. At the systematic
        // RELIABLE_STORE_MAX=1024 > W the frontier runs ~1024 ahead while the
        // window covers only the last 640, so a lost DOF at the ack ages out to
        // slow ARQ (MEASURED ~4.7 Mbit/s, 80% idle); lifting the cap entirely
        // decouples them and DNFs. Sizing the store to W_mp is what makes the
        // window rateless-fungible in practice. W_mp also comfortably exceeds the
        // BDP (~190 sym at C8), so both paths stay saturated. RWM_STORE overrides.
        let store_max: usize = if generation {
            // Backpressure at the pipeline bound: the send frontier may run at most
            // ~M generations ahead of the cumulative-decode frontier, so exactly M
            // generations are in flight. TUN reads pause here (flow control), never
            // dropping data. Generation mode uses the encoder's retained size as the
            // backpressure signal (no sent_store), so this matches win_cap.
            //
            // Transport-ceiling fix (MEASURED at L1): win_cap = G·(M+1) as the
            // BACKPRESSURE point is 14× the BDP at C2, so the unacked pipeline is a
            // multi-hundred-ms standing queue (RTT inflated to 0.5–1.3 s). That
            // bufferbloat does NOT cap single-path throughput (it is window-
            // INDEPENDENT — a per-symbol processing limit) but it (a) produces
            // catastrophic slow-run outliers (single-path 50 MB×6 stdev 24.8 s at
            // G·(M+1)) and (b) SERIALIZES dual-path aggregation: the fast path
            // stalls on the bloated in-order-frontier cross-path feedback, so
            // symmetric C7 falls BELOW single (×0.65, anti-aggregation).
            //
            // The send frontier needs only TWO generations outstanding to pipeline
            // — one filling head + one sealed-and-recovering — not M+1. Backpressure
            // at 2·G (retention stays at win_cap = G·(M+1) for decode headroom)
            // decouples the standing queue from the retention horizon. MEASURED
            // (G=480, 50 MB×6): single 11.2→15.6 Mbit (stdev 24.8→0.7 s), symmetric
            // C7 9.8→22.3 (×1.43 aggregation), heterogeneous C8 9.45→14.55 — all
            // up, tighter, 0 DNF. RWM_STORE overrides for the sweep.
            // Fix 3: under OOO retention the backpressure window is the wide
            // ooo_gens·G, so the send frontier decouples from the stalled in-order
            // frontier. Otherwise the tight 2·G standing-queue bound.
            // gen_pipe: the static cap is the M* ceiling; the DYNAMIC per-loop cap
            // (`gen_pipe_store_cap` = M*·G) is what gates intake each iteration.
            let default_store = if gen_pipe {
                GEN_PIPE_MAX_GENS * gen_size
            } else if ooo_retain {
                ooo_gens * gen_size
            } else {
                2 * gen_size
            };
            gates
                .store_override
                .unwrap_or(default_store)
                .clamp(gen_size, win_cap)
        } else if coded_only {
            gates.store_override.unwrap_or(win_cap).clamp(win_cap, 1 << 20)
        } else {
            // Plain-reliable (systematic-free, non-generation) MEMORY ceiling for
            // the retention store. RWM_STORE forces a STATIC window (disables the
            // dynamic BDP cap below) for the sweep; the shipped default keeps the
            // large retention ceiling and lets the delay-based `plain_dyn_cap`
            // bound the *outstanding* window instead.
            gates.store_override.unwrap_or(RELIABLE_STORE_MAX)
        };
        // Delay-based send-window cap for the plain-reliable path (paper §12).
        // The fixed RELIABLE_STORE_MAX (1024) is ≈12× the BDP at C2, so the
        // unacked store builds a multi-hundred-ms standing queue (MEASURED RTT
        // 0.41–0.52 s vs 10 ms base). On a CLEAN link that only adds latency, but
        // under loss every hole must traverse that bloated queue to recover, the
        // cumulative-ack (and thus the ack-clocked pacing) freezes for a full
        // bufferbloat-RTT, and single-path throughput COLLAPSES (MEASURED 75→14
        // Mbit at C2). The remedy is to bound the OUTSTANDING window to a
        // BDP-scaled cap so the queue — and hence recovery latency — stays ~1 RTT.
        // BtlBw×RTprop is bufferbloat-robust (windowed-max rate × min-RTT floor),
        // so it tracks the true pipe even while the live RTT is inflated. Active
        // only for the plain-reliable path and only when RWM_STORE is NOT forcing
        // a static window; generation/coded-only keep their own structural caps.
        let plain_dyn_cap =
            reliable && !generation && !coded_only && !gates.store_env_set;
        // Window = gain × BDP. ≥2 keeps the pipe full (≈1 BDP) while leaving ≈1
        // BDP of headroom to keep sending fresh data during a one-RTT recovery
        // round; 2.5 adds jitter/burst slack. RWM_STORE_GAIN overrides.
        let store_bdp_gain: f64 = gates.store_gain;
        // Cap before the BtlBw anchor warms (a few RTTs). Tight so the startup
        // burst can't pre-bloat the queue and inflate the min-RTT floor (which
        // would then inflate the anchor itself); the anchor takes over once
        // samples land. ~1.5× a 100 Mbit / 10 ms BDP.
        let store_boot_cap: usize = gates.store_boot;
        // Floor so a transiently-tiny BDP estimate can't strangle the pipe.
        let store_cap_floor: usize = 64;
        // ── Path-scaled outstanding pool (task #84, env RWM_STORE_PATHS) ──────
        // MEASURED at L1 (2026-07-14, host-passthrough E5-2650v3): the plain-
        // reliable OUTSTANDING ceiling is a per-TRANSFER constant
        // (RELIABLE_STORE_MAX = 1024, which the 2×Σanchor dynamic cap latches at
        // on fast paths because the legacy ack-interval anchor over-reads), so a
        // multipath sender is store-starved: the DIAG shows win=1024/1024 pegged
        // while both paths idle (infl=0 spikes). Same-binary static-store sweep,
        // C7 plain+BBR: 1024→103 Mbit, 2048→122.7, 4096→141.3, 8192→143.7
        // (saturated); C8: 4096→71.5, 8192→31.8 (slow-path bufferbloat collapse);
        // singles: sc2 2048→81.6 / 4096→75.6 / 8192→43.0 (collapse), sc3
        // degrades monotonically with a static pool (the dynamic cap binds at
        // ~684 there and is the right law). The knee is 2048 PER LIVE PATH.
        // Under RWM_STORE_PATHS=1 and N = live_paths ≥ 2 the dynamic-cap value
        // scales ×N and its clamp ceiling becomes N × 2048 (RWM_STORE_PATH_POOL
        // overrides); N = 1 keeps the legacy law bit-exactly, so singles are
        // unaffected even with the flag ON. Default OFF: shipped byte-identical.
        // The engine sink is NOT the binder here: single-path c1 sinks 187.7
        // Mbit/s through the same receiver task, and pinning the C7 receiver to
        // one core costs only −8% at the default store.
        // DEFAULT ON (2026-07-21, "Consolidation" LOO battery: removal from the
        // composed stack re-opens the c7 collapse class (86-97 Mbit runs, both
        // seeds) and drops the mean; no cell regressed >>sigma. The c8 sub-sigma
        // cost vs the legacy pool under SACK-release is the register's WATCHED
        // follow-up — see goal-gate "Consolidation".)
        let store_paths_on = gates.store_paths;
        let store_path_pool: usize = gates.store_path_pool;
        // ── Capacity-weighted outstanding pool (env RWM_STORE_CAPW) ──────────
        // The ADR-0058 "c8 WATCH" follow-up (goal-gate "C8-Aware Pool Law"):
        // pool = Σ_i honest per-path cap over LIVE paths (capw_store_cap) — each
        // path earns unacked-frontier depth for its OWN pipe + recovery round,
        // summed as ONE shared pool (borrowing stays free, only the sizing law
        // changes vs RWM_STORE_PATHS' count-scaled clamp). Engaged N ≥ 2 with
        // every live anchor warm; until then the configured pooled law
        // (path-scaled / legacy) is the warm-up fallback. Default OFF: shipped
        // byte-identical; the battery arm composes RWM_PLAIN_RS=1 so the anchor
        // terms read ≈1× truth (the legacy over-read clamps this law to the
        // N×knee ceiling ≡ path-scaled — documented at capw_store_cap).
        let capw_on = gates.store_capw && plain_dyn_cap;
        // ── Pool-anchor honest dual-store law (env RWM_POOL_ANCHOR) ──────────
        // Goal-gate "Ship The Wins 1" (the §16.35 c7 blocker's named successor):
        // at N ≥ 2 live paths the pooled-store cap's RATE input is the per-path
        // hygiene-grade SEND-interval anchor (SendRateAnchor fed at
        // charge_in_flight — burst-immune by construction: Δt spans the SEND
        // interval, so the est-cadence ack clock's tighter ack bursts cannot
        // inflate it; clock-gap buckets discarded) instead of the legacy
        // ack-interval windowed-max (measured over-read ×4.6–7.4, a further
        // ×3.4–3.7 under RWM_EST_CADENCE). Law: pool = clamp(Σ_i
        // honest_store_cap(sr_i·RTprop_i, sr_i, K_i, gain), floor, N·knee) —
        // the capw shape (ONE shared pool, borrowing free), engaged only with
        // ALL live send-anchors warm; until then the configured path-scaled law
        // runs verbatim. The Copa cwnd feed (record_delivery/on_ack) and every
        // N = 1 law are bit-exactly untouched — no CopaFeed machinery runs at
        // duals (the measured −22…−27 c7 RS-composition price stays
        // unreachable; no src_inflight is charged — the §16.34 falsification-5
        // lesson). Default rides the est-cadence resolution (OFF unset; ON
        // under the est opt-in — the composed default flip was measured and
        // REVERTED on its pre-set c7 clause, 2026-08-07).
        let pool_anchor_on = gates.pool_anchor && plain_dyn_cap;
        // ── The store-cap path set (env RWM_STORE_CAP_UNIFIED) ───────────────
        // Goal-gate "Store-Cap Triplication" (pre-registered 2026-08-09): the
        // dyn-cap phase's Σ-anchor base and honest per-path cap sum move off
        // `active_paths()` (the cwnd-saturation data-scheduling filter) onto
        // `live_paths()` — the set `n_live` is already counted from, and the
        // set every OTHER honest-cap consumer in this phase already reads.
        // Default OFF: the shipped tree is bit-identical.
        let store_cap_unified = gates.store_cap_unified && plain_dyn_cap;
        // ── The composed THREE-TERM limit (env RWM_THREE_TERM) ────────────
        // Goal-gate "Three-Term Law" (pre-registered 2026-08-10), paper
        // §16.43 + §16.44: the outstanding-data limit is Σ per-path network
        // window + Σ per-path emission slack + ONE resequencing span, each
        // Little's law over a measured signal, no fitted coefficient. The
        // span term is identically zero at a single path BY ARITHMETIC
        // (max RTprop = min RTprop), which is what retires the
        // `active_paths()`/`live_paths()` topology branch without an
        // `if N == 1`. Scoped to the plain dynamic cap: under Copa
        // ownership cwnd IS the operating point and that law is untouched.
        // Default OFF: the shipped tree is bit-identical.
        let three_term_on = gates.three_term && plain_dyn_cap;
        // b(δ) at this tunnel's named point on the dial, ONCE.
        let delta_b = crate::net::delta_budget_b(protocol_hint);
        // ρ, the retention dial's declared value in this scope (see the
        // field doc): the plain dynamic cap is RETAIN-UNTIL-ACKED, so the
        // contract's ρ is 1 here — a scope, not a branch.
        let contract_rho: f64 = 1.0;
        // ── SACK-clocked store release (env RWM_STORE_SACK_RELEASE) ──────────
        // Goal-gate "SACK-Clocked Store Release" (pre-registered 2026-07-21):
        // the retention store releases slots only on the cumulative frontier,
        // so SACKed-but-not-cumulative symbols hold slots a full frontier round
        // — at c7 the store recycles at frontier latency, not path rate. Under
        // this law a SACKed seq is UNCOUNTED from the flow-control outstanding
        // (the slot returns to the pool / per-path account, the window opens)
        // while sent_store + retransmit_buffer + nack_retx_at + source_path_map
        // are kept UNTOUCHED until the cumulative frontier passes it — release
        // a STORE SLOT, never recoverability (the RWM_SACK_PRUNE lesson; see
        // sack_release_mark). DEFAULT ON (2026-07-21, the pre-registered
        // battery earned the flip: c7 0.96–1.05×Σ both seeds, sc2 +3–4 at
        // N=1, dual-c1 +20–22 composed, no regression; goal-gate
        // "SACK-Clocked Store Release"); RWM_STORE_SACK_RELEASE=0 is the
        // legacy frontier-only-release opt-out arm, under which the released
        // set stays empty and the gate arithmetic is exactly the legacy
        // store_len.
        let store_sack_release_on =
            reliable && !generation && !coded_only && gates.store_sack_release;
        // ── Frontier-slack placement (env RWM_PLACE_SLACK) ───────────────────
        // Goal-gate "C8 Slow-Path Conversion" (pre-registered 2026-08-06): the
        // §16.3 placement cost's load term becomes max(0, Ê_i − S)/ref with
        // S = clamp(span/R_ack, 0, 250 ms) — span = sent_edge − cum_ack,
        // R_ack = EWMA of the cumulative-ack advance rate (delivery truth,
        // immune to the plain anchor's over-read). S = 0 until R_ack warms and
        // whenever N < 2 (shipped cost bit-exact — the law is a strict
        // continuous generalization; see Scheduler::set_place_slack /
        // place_costs). Plain reliable window only. Default OFF.
        let place_slack_on = gates.place_slack && reliable && !generation;
        // ── Per-path outstanding accounting (task #86, env RWM_STORE_PERCAP) ──
        // The #84 residual: the PATH-SCALED pool is still ONE pool — it cannot
        // fit a c2-deep and a c3-shallow path simultaneously (C8 stuck at
        // 0.79–0.80 of Σ; raising the shared cap to 8192 collapsed the slow
        // path to 31.8 Mbit/s). Here each path gets its OWN account sized to
        // ITS pipe (percap_store_cap: gain·rate_i·echoRTT_i, clamped to
        // [floor, pool]); a symbol placed on path i draws path i's account and
        // is released on the ack that removes it from the retention store
        // (SACK/OOO or cumulative). Admission pauses only when NO live path
        // has account headroom (percap_store_full — the infl_percap_full
        // pattern), and the plain-reliable placement redirects a cap-full pick
        // to the path with headroom (percap_place_path). Engaged only for
        // N ≥ 2 live paths — N = 1 keeps the legacy pooled law bit-exactly.
        // Default OFF: shipped byte-identical. Supersedes RWM_STORE_PATHS'
        // pooled GATE when both are set (the warm-up share still inherits from
        // whichever pooled law is configured, so STORE_PATHS composes as the
        // warm-up baseline rather than conflicting).
        let percap_on = gates.store_percap && plain_dyn_cap;
        // Roadmap item 1 (the #86 c8 follow-up): the delay-aware redirect guard.
        // Default ON whenever percap is on (RWM_PERCAP_GUARD=0 restores the
        // unguarded redirect — the measured c8-regression control arm). The
        // shipped default is untouched: percap itself is default OFF.
        let percap_guard_on = percap_on && gates.percap_guard;
        // Bounded account borrowing (feat/store-borrowing, paper §16.22): a
        // pick landing on a cap-full account may FLY on that pipe while being
        // CHARGED to a sibling account, bounded by
        //   lend_i→j ≤ max(0, cap_i − out_i − rate_i·T_return(j)),
        //   T_return(j) = fly_j/rate_j + RTprop_j (floor clock)
        // — lend only headroom the lender cannot use within the loan's return
        // latency. Requires the percap stack (accounts, guard, honest caps
        // under RWM_PLAIN_RS). Default OFF: shipped byte-identical; the
        // no-borrow percap arm is the same-binary control.
        let percap_borrow_on = percap_on && gates.store_borrow;
        // ── Honest floor-clock store caps (feat/percap-honest-cap) ────────────
        // GUARD-RESULTS residual (i): with the redirect channel closed, the c8
        // parking flowed through the softmax's OWN picks under the knee-clamped
        // slow cap — the legacy plain anchor over-reads ×4.6–7.4 ("Anchor
        // Hygiene" battery (b)) so cap_slow latched at the 2048 knee and the
        // derived differentiation never engaged. With the honest send-interval
        // sampler (RWM_PLAIN_RS) the anchor reads ≈1× truth, and the cap law
        // is re-derived on it: cap_i = anchor_i·(K_i + gain − 1) +
        // rate_i·(gain−1)·R — residence on the measured unloaded drain clock
        // plus runway on the RECOVERY engine's clock (R = the 100-ms hole-
        // refresh/tail-sweep cadence bound), see `honest_store_cap`. Applies
        // to the per-account
        // percap caps AND the N=1/anchor-sum pooled cap (the sc2 −20% fix: the
        // over-read was accidentally load-bearing there; K supplies that
        // headroom explicitly and honestly). Engaged only where the honest
        // sampler is live (plain in-order, no Copa CC ownership — the Σcwnd
        // and per-path cwnd laws are already honest and stay untouched).
        // RWM_HONEST_CAP=0 = the floor-law control arm (reproduces the −20%);
        // both gates default-OFF paths keep the shipped tree byte-identical
        // (RWM_PLAIN_RS itself is default OFF).
        let honest_cap_on = plain_dyn_cap && gates.plain_rs && gates.honest_cap;
        // ── Window/inflight decoupling (env RWM_WIN_DECOUPLE) ────────────────
        // Goal-gate "Window Decoupling + MTU Scaling" part 1 (pre-registered
        // 2026-08-06 + diagnosis amendment): at N = 1 the admission gate moves
        // from the un-SACKed total vs the anchor-sum latch to the live HEAD
        // SPAN (last_sent − SACK/cum frontier — recovery-stalled holes
        // excluded) vs the stall-metered allowance `win_decouple_allow`; the
        // un-SACKed total keeps a retention backstop `win_decouple_cap_ret`
        // (memory clamp 4096). Under Copa-sole the residence term is
        // gain·Σcwnd (the 1024 ceiling truncation — the B1 jitter-cell dwell
        // binder — is released). N ≥ 2 and warm-up keep the configured laws
        // bit-exactly. Default OFF: shipped byte-identical.
        let win_decouple_on = gates.win_decouple && plain_dyn_cap;
        // ── #85 budget-conserving taper (RWM_TAPER_R, default OFF) ────────────
        // MEASURED (goal-gate "r* Bursty-Loss Provisioning", L1 2026-07-13): the
        // legacy taper accrual below sums to Σ τ(t) = r symbols PER ACK CYCLE
        // (taper_offset resets on cumulative-ack advancement), so the emitted
        // plain-mode proactive overhead is ~r/cycle-length — nearly independent
        // of r's computed magnitude. Legacy r*=0.206 and corrected r*=0.255 both
        // emitted cod/src ≈ 0.03–0.10 at c3-realtime: the whole r* control loop
        // (incl. the §8.4.1 burst-tail correction) was INERT at the wire. With
        // the flag ON, `TaperBudget` makes emission consume r as computed: a
        // per-window budget (emitted ≈ r × source per coding window), the taper
        // shape kept as a re-timing (repair still concentrated at the frontier),
        // paced ≤ 1 repair per source send and spare-capped (existing anchors,
        // no new constants). OFF ⇒ byte-identical legacy emission (A/B arm).
        // L0 VERDICT (2026-07-18, goal-gate "Taper Emission Fix"): the budget
        // law is LIVE at the wire (cod/src 0.03-0.05 → 0.21-0.34 on the
        // c3heavy 2x2) but delivered reliability DEGRADES at realtime and the
        // r* arms stay tied — the emitted repair codes over the LEADING sliding
        // window (in-flight entanglement, the RWM_MIN_R defect class above), so
        // it is recovery-inert within realtime's reorder horizon; quantity was
        // not the only binder. Default stays OFF; flipping it is gated on the
        // solvable-span emission follow-up, not on L1 alone.
        // §16.20 (c): under RWM_UNIFIED the quantity law is the default (the #85
        // fix composes with the trailing solvable-span placement below, which
        // removes the leading-window entanglement that kept it OFF); RWM_TAPER_R=0
        // still reproduces the legacy accrual as the same-binary A/B arm.
        let taper_r_budget = gates.taper_r;
        // §16.20 (c): trailing solvable-span placement for plain-mode proactive
        // repair — span width A* = clamp(rate·D, 1, W) with D = b(hint)·RTprop
        // (§8.8 budgets: Realtime ½, Auto 1, Bulk 2 RTT — capped at 2·RTprop, the
        // deficit-round limit) and trailing offset Δ = ceil(rate·jitter) ≥ 1, so
        // every covered member has LANDED when the repair does (solvable at
        // arrival — the #85 leading-window entanglement removed structurally).
        let unified_span = gates.unified;
        // feat/anchor-hygiene (`RWM_ASTAR_ANCHOR`): the A* rate anchor repaired.
        // Legacy A* reads `est.throughput()` — a 2-s-interval α=0.125 EWMA of the
        // report-tick send rate — which (i) pins A* = 1 for ~10 s of every stream
        // (realtime FEC inert: ru/rf ≈ 9%) and (ii) is flood-poisonable (A* 1→38
        // off the post-stall release burst) — goal-gate COLLAPSE ATTRIBUTION,
        // defect designs A+B. The repair: a windowed-max send-rate anchor
        // (SendRateAnchor) fed by the sender's OWN send events — live within ~1
        // RTT (hygiene rule 1), with gap-spanning/flood buckets DISCARDED
        // (rule 2). Gate off ⇒ the EWMA path byte-identical.
        // goal-gate "Unified Shedding": DEFAULT ON under the unified machine —
        // the span law ships with its repaired anchor (fix A gates the flip
        // battery; without it the realtime spans pin at width 1, ru/rf ≈ 9%).
        // `RWM_ASTAR_ANCHOR=0` / `RWM_ANCHOR_HYGIENE=0` still opt out for A/B.
        let astar_anchor_on = unified_span && gates.astar_anchor;
        // ── δ-honest overload shedding (fix C, goal-gate "Unified Shedding") ──
        // Part of the unified machine's REALTIME semantics: armed only on the
        // EVICT path (`!reliable` — the ρ = 1 RETAIN contract is excluded by
        // construction) under RWM_UNIFIED; `RWM_UNIFIED_SHED=0` reproduces the
        // serializing arm for A/B. A hole whose retransmit can no longer meet
        // the δ deadline D = b(hint)·RTprop (the span law's own D) is DROPPED
        // from the ARQ set instead of serializing the stream behind it — but
        // only while cumulative shed stays within the DERIVED 1−ρ budget
        // (`residual_loss_after_fec`: ε̂·(1−P_fec) at the live (r, A*, σ²)
        // operating point). Budget spent ⇒ serialize (ρ wins over δ).
        let shed_on = shed_armed(gates.unified, reliable, gates.unified_shed);
        // ── Removed proactive-repair experiments (DEPRECATION REGISTER) ───────
        // RWM_FRONTIER* ("Proactive Frontier", 2026-07-07: repair anchored at the
        // ½-RTT-stale ack frontier loses the race to its own ARQ — rf=718 emitted,
        // ru=4 useful) and RWM_INLINE_REPAIR ("Repair In-Flight", 2026-07-08:
        // stall-starved + cross-grid stranding — every inline config wedged or
        // crawled) were both refuted on GEOMETRY, not substrate, and REMOVED
        // 2026-07-27. Their goal (repair present at stall) is achieved by
        // RWM_PROACTIVE_PACER below, whose own measured null resolved into the
        // structural presence⊥throughput identity; the unified TRAILING span law
        // (§16.20.3) is the derived realization of the frontier intent. The FDIAG
        // diagnosis instrument (RWM_FDIAG, receiver loop) is retained.
        // ── Proactive-repair pacer (RWM_PROACTIVE_PACER) — present-at-stall ───────
        // A DEDICATED proactive-repair emission on the GENERATION grid, decoupled
        // from BOTH source availability and the ack-clock `target`. For each
        // in-flight generation (still FILLING or recently sealed) it emits
        // proactive repair over the retained contiguous PREFIX at the full
        // generation width (`generate_repair_filling` → same (anchor, G) matrix, no
        // cross-grid stranding), paced by the shared CC token bucket. Fixes BOTH
        // refutations of the interspersed inline repair (goal-gate "Repair
        // In-Flight"): (1) NOT stall-starved — it runs in the main loop every
        // iteration incl. tx_paused wakeups, so repair flows under backpressure when
        // the frontier most needs it; (2) NOT cross-grid stranded — it codes the
        // generation grid, so a buffered filling equation combines directly with the
        // reactive generation deficit. The covering equation reaches the receiver
        // EARLY (around when the hole is sent, not a generation-span later at seal),
        // so it is PRESENT when the frontier detects the hole → proactive decode, no
        // round-trip. Supersedes the sealed batched proactive path when on; the
        // reactive deficit (RWM_REACT_CAP + RWM_REPAIR_WAIT) stays the bounded
        // fallback for holes the proactive repair still misses. Systematic only;
        // shipped path untouched.
        let proactive_pacer = systematic && gates.proactive_pacer;
        // ── Cross-path repair placement (RWM_XPATH_REPAIR) — the C8 realization ────
        // Route proactive (and deficit) REPAIR to the max-spare-capacity path (the
        // underutilized path — the slow path once the fast path is source-saturated)
        // instead of the marginal-cost softmax (which biases repair toward the fast
        // path, so it competes with systematic source — the single-path
        // presence⊥throughput tension). With this on, a fast-path loss is covered by
        // repair already in flight on the SLOW path, WITHOUT displacing fast-path
        // source: presence is bought from the spare path's capacity. Symmetric paths
        // (C7) have equal spare, so `place_repair_spare_path` splits the near-tie set
        // uniformly (no hard-argmax concentration → no C7 regression). Generation/
        // systematic only; shipped path untouched. Default-OFF.
        let xpath_repair = generation && gates.xpath_repair;
        // Symbol packer: accumulate small packets into packed symbols for Realtime mode
        let use_packing = protocol_hint == ProtocolHint::Realtime;
        // RWM_DIAG (transport-ceiling diagnosis) master gate. Carried into the
        // emission step as `SenderPolicy::diag_on` (the GLIFE fill tracking).
        let diag_on = gates.diag;
        // Per-path store-attribution GAUGE (goal-gate "C8-Aware Pool Law"
        // diagnosis instrument, ADR-0052 class — no behavior): under RWM_DIAG the
        // percap account maps are maintained even when the percap LAW is off, so
        // the DIAG `sout=` field shows each path's share of the POOLED
        // outstanding (which path is holding the unacked-frontier span — the c8
        // pool-arm diagnosis gauge). Behavior-inert by construction: every percap
        // decision site keys on `percap_caps` NON-EMPTY (the "law engaged"
        // signal), and caps are only computed under percap_on — with the law off
        // the maps feed the DIAG print alone.
        let percap_track = percap_on || (diag_on && plain_dyn_cap);
        // ── feat/recovery-suppression: multipath recovery suppression ─────────
        // (`RWM_RECOV_MP`, DEFAULT ON 2026-07-21 — the "Consolidation" LOO
        // battery: removal costs −12.3/−13.9 Mbit >>sigma at c7 on both seeds
        // (retx 18k vs 5.4k) with the dual-c1 retx flood (13-30k) re-appearing;
        // neutral within sigma everywhere else. `=0` is the legacy global-clock
        // opt-out arm. Plain window reliable mode only — generation mode has no
        // per-seq ARQ to suppress).
        // Sub-gate for trace attribution: _LAW (per-flight hole law, default ON
        // under the umbrella). The _SERIAL per-path batch-namespace arm was
        // REMOVED 2026-07-27 (register: refuted on the clean substrate, ×2.4
        // sender CPU — see the module-header design note (2)).
        let recov_mp = gates.recov_mp && reliable && !generation;
        let recov_mp_law = recov_mp && gates.recov_mp_law;
        // ── diag/lossy-residual: SINGLE-path hole-law suppression ─────────────
        // (`RWM_RECOV_SP`, default OFF — the A/B arm; goal-gate "Lossy-Single
        // Residual"). The 2026-07-27 diagnosis measured the N=1 reactive plane
        // firing ×4.4–5.7 the realized loss (sc2-100M: fired 3313, y=2659 younger
        // than the law's own threshold, vs ~580 netem drops; sc3: 2556 vs ~510)
        // — the "single-path gaps are FIFO-real" premise of `mp_hole_ripe`'s
        // N=1 bypass is REFUTED on a jittery substrate (netem delay jitter
        // reorders tens of packets deep; the receiver's gap reports name
        // merely-late seqs, and re-fires chase flights still queued behind the
        // store-cap standing queue). The law: at N=1 a gap seq with a LIVE
        // flight (original or retransmit) fires only once the flight is
        // ≥ 9/8×max(smoothed clocks) old (RFC 9002 §6.1.2, same
        // `mp_time_threshold_us`); TIME channel only — the §6.1.1 packet
        // channel is excluded at N=1 (reorder depth ≫ kPacketThreshold).
        // Suppression-only: the receiver's hole-refresh re-advertises until the
        // flight ripens, so real holes still recover.
        let recov_sp = gates.recov_sp && reliable && !generation;
        // ── feat/c8-conversion: recovery clocks on LIVE paths ────────────────
        // (`RWM_RECOV_MP_LIVE`, default OFF — the A/B arm; goal-gate "C8
        // Slow-Path Conversion"). The hole law's N + per-path clock snapshot
        // read `active_paths()` — the saturation-filtered set (`available() >
        // 0`) whose cwnd-full-path trap collapses the law to the N=1 bypass
        // (legacy age gate on a cross-path clock) mid-transfer; the same
        // filter trap already documented at the Copa-sole store law and
        // `capw_store_cap`. Diagnosis signature (2026-08-06): c8-pbs 412–749
        // of ~1.2–1.5k retransmits fired YOUNG vs their own flight-path law
        // threshold. Under this gate the snapshot uses `live_paths()`.
        let recov_mp_live = gates.recov_mp_live && recov_mp_law;
        // Goal-gate "Unlock The Default 2: derived patience" — the two gates.
        // `patience_derived` is BEHAVIOURAL (the recovery-patience floor);
        // `sidle_derived` is DIAG-only (the second, derived stall gauge printed
        // beside the unchanged legacy one). Both default OFF.
        let patience_derived = gates.patience_derived;
        let sidle_derived = gates.sidle_derived && diag_on;
        // ── Emission batching (goal-gate "Emission Batching", RWM_EMIT_BATCH,
        // DEFAULT OFF — same-binary A/B) ──────────────────────────────────────
        // The §16.23 sender-emission service wall (~19.5–20k sym/s ≈ 190 Mbit)
        // is per-SYMBOL loop cost, profiled 2026-07-27 on the c1 cell: taper/
        // span control math (compute_repair_rate + predictive_loss_upper +
        // exp/log ≈ 15–17%/core, recomputed per symbol), plus a full select!
        // iteration (tail-deadline scan, SACK drain, pacing refresh) and the
        // waker churn of one-datagram-per-wakeup handoff to quinn (syscall
        // density is NOT the wall — quinn-udp GSO already batches ~7.6
        // segments/sendmsg on this path). Under the gate the sender:
        //   1. drains TUN intake in pacer-quantum bursts (≤ emit_burst symbols
        //      per loop iteration, ~64 KB — inside the flow-control store
        //      headroom and the cc_pace token bucket, checked per symbol), so
        //      loop-iteration overhead amortizes and quinn's endpoint driver
        //      sees a multi-datagram queue (deeper GSO transmits);
        //   2. refreshes the derived taper/span math once per burst instead of
        //      per symbol (the A* send-rate anchor is still FED per symbol —
        //      only the derived-rate recomputation is amortized).
        // OFF ⇒ per-symbol recompute, bit-identical shipped path. Plain
        // window-reliable mode only (generation/coded emission has its own
        // paced block; realtime packing keeps its per-packet latency path).
        // SINGLE-LIVE-PATH ONLY (measured, 2026-07-27 battery rep 1): the
        // emission service wall is a c1-class single-path binder (§16.23);
        // dual cells are wire/recovery-bound and bursting there AMPLIFIES the
        // wall-#8 striping-gap loss misread (global batch serials + longer
        // same-path arrival runs → per-path pl read up to 0.74 at a 2.6%-loss
        // cell, tail-recovery stretch: c7 167→115, c8 87→52). With N ≥ 2 live
        // paths the emission path stays bit-identical (`emit_batch_live`
        // re-checked per loop iteration — path flaps re-scope within one
        // burst).
        // Realtime (packed) mode is excluded outright: its per-packet latency
        // path must never trade a wakeup for a burst, and its symbol rate is
        // orders below the wall. The taper cache additionally carries a 50 ms
        // staleness bound so a low-rate bulk-hint tunnel (e.g. the tail-matrix
        // message workload riding the bulk tunnel at 50 msg/s) never runs the
        // span/shed law on second-old anchors.
        let emit_batch_on = gates.emit_batch && reliable && !coded_wire && !use_packing;
        let emit_burst: usize = gates.emit_burst;
        // RWM_DIAG (transport-ceiling diagnosis): once per ~250 ms emit one line
        // isolating the binding single-connection constraint — window occupancy vs
        // store_max, tx_paused duty cycle, cumulative-ack goodput (Mbit/s), the
        // ack-clocked pacing rate vs the link, cwnd/in_flight vs BDP, and the
        // source/coded send rates. Gated on the RWM_DIAG env so the hot path is
        // untouched when off. (`diag_on` itself is resolved above.)
        // Transport-ceiling fix (generation mode): bound the in-flight (unacked)
        // symbols to ~BDP instead of the fixed store_max = G·(M+1). The oversized
        // store_max is decoupled from the pipe (14× BDP at C2), so unpaced source
        // emission builds a multi-hundred-ms standing queue (MEASURED RTT inflated
        // to 0.5–1.3 s), which turns every hole into a ~1 s recovery stall. Cap
        // total in-flight at a BDP-scaled bound so the queue — and thus the
        // recovery-stall latency — stays small. 0 = off (legacy store-only
        // backpressure). The deficit-recovery emission is EXEMPT (it must always be
        // able to fund a frontier hole, else a full-window pipe deadlocks).
        let infl_cap: u64 = gates.infl_cap;
        // PART 1.2 (receiver-tail): BDP-DERIVED in-flight cap. A fixed RWM_INFL_CAP
        // must be hand-tuned per RTT; instead bound total in-flight to
        // gain × Σ copa_bdp_anchor (BtlBw×RTprop, bufferbloat-robust) recomputed
        // live, so the standing queue — and thus the RECOVERY-ROUND RTT — stays
        // ~gain·BDP at ANY RTT. It gates BOTH proactive emission AND (Fix-2
        // non-exempt) reactive/deficit recovery via `cwnd_full`, so the parallel
        // tail flush cannot re-bloat the queue. Env RWM_INFL_BDP=gain (e.g. 2.0);
        // 0/unset = off (legacy static RWM_INFL_CAP / store-only backpressure).
        // gen_pipe remedy 1: the per-path BDP in-flight cap ON (gain 1.5 — the
        // FMTCP-era oracle PART 5c finding: the bare aggregate BDP starves the
        // recovery headroom; ~1.5× over the windowed-max — hence under-estimating —
        // anchor gives the emergent ~1.3× BDP operating point) so the standing
        // queue — and the RTT the SUBSTRATE CC sees — stays ≈ RTprop.
        let infl_bdp_gain: f64 = gates
            .infl_bdp
            .unwrap_or(if gen_pipe { 1.5 } else { 0.0 })
            .max(0.0);
        let infl_bdp_on = infl_bdp_gain > 0.0;
        // The #64 fix (FMTCP-era, retained under gen_pipe): enforce the in-flight
        // cap PER PATH (path i outstanding ≤ gain·BtlBw_i·RTprop_i) rather than as
        // one fungible global Σ budget. The sender is TUN-paused only when EVERY
        // active path is at its own cap, so the fast path keeps pulling fresh
        // source while the slow path is full.
        let infl_percap = gen_pipe;
        // Transport-ceiling fix (generation mode): clock the coded-emission budget
        // to the SENT source frontier instead of the ACKED frontier. The
        // ack-clocked `target = ack·(1+r) + W` DEADLOCKS a small generation: once
        // the proactive budget W is spent, coded stops until the ack advances — but
        // the ack is stalled precisely because the frontier generation is missing
        // the coded it needs to decode (MEASURED: G=96 wedges with in_flight=0,
        // src=0, cod=0). Sourcing the budget from the sent frontier lets the
        // encoder's own per-generation ceil(K_g·(1+r)) cap + the M-generation
        // retention bound govern coded emission (both already bound the datagram
        // buffer), so proactive coverage always completes and small generations —
        // which keep the store near BDP and avoid the bufferbloat stall — work.
        let coded_src_clock = gates.coded_src;
        // PURE-PROACTIVE demonstrator (proactive-FEC-vs-ARQ crossover, directive #4):
        // when set, DISABLE the deficit-driven reactive recovery loop entirely. All
        // recovery then comes from the UPFRONT proactive per-generation budget
        // (ceil(len·r)) — no NACK/deficit round-trips, and (crucially) no
        // recovery-emission path that is EXEMPT from the in-flight congestion cap, so
        // every emitted symbol (systematic source + proactive coded) is bounded by
        // RWM_INFL_CAP and cannot overrun the droppable datagram path. This isolates
        // the clean question: with enough upfront repair (high r) that holes decode
        // on arrival, does proactive FEC beat ARQ at high RTT? Requires r sized to
        // cover the per-generation loss tail — a generation that loses more than its
        // budget never decodes (the object DNFs), which is itself the honest result.
        let no_reactive = gates.no_reactive;

        Self {
            symbol_size,
            protocol_hint,
            reliable,
            generation,
            systematic,
            coded_wire,
            gen_size,
            pipeline,
            gen_pipe,
            mstar_anchor,
            gen_repair_floor,
            gen_rate,
            gen_rate_floor,
            gen_inflight_window,
            ooo_retain,
            coded_src_clock,
            no_reactive,
            xpath_repair,
            proactive_pacer,
            cc_pace,
            cc_pace_headroom,
            react_cap_cfg,
            react_cap_on,
            infl_cap,
            infl_bdp_gain,
            infl_bdp_on,
            infl_percap,
            win_cap,
            store_max,
            plain_dyn_cap,
            store_bdp_gain,
            store_boot_cap,
            store_cap_floor,
            store_paths_on,
            store_path_pool,
            capw_on,
            pool_anchor_on,
            store_cap_unified,
            three_term_on,
            delta_b,
            contract_rho,
            store_sack_release_on,
            place_slack_on,
            win_decouple_on,
            percap_on,
            percap_guard_on,
            percap_borrow_on,
            honest_cap_on,
            percap_track,
            unified_span,
            astar_anchor_on,
            shed_on,
            taper_r_budget,
            repair_rate_floor,
            recov_mp,
            recov_mp_law,
            recov_sp,
            recov_mp_live,
            patience_derived,
            sidle_derived,
            emit_batch_on,
            emit_burst,
            use_packing,
            diag_on,
            // Sampled by the sender at its original point in setup (see the
            // module doc); rebound there via a struct-update on `pol`.
            span_diag_start_us: 0,
        }
    }
}
