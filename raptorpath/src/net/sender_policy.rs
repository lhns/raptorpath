//! The window sender's resolve-once policy: the derived constants that fix
//! `run_window_sender`'s behaviour for the lifetime of a tunnel.
//!
//! Every field of [`SenderPolicy`] is resolved once by
//! [`SenderPolicy::resolve`], in the `RuntimeGates::resolve()` shape
//! (`src/gates.rs`), and never reassigned. Every expression is pure — reads of
//! `gates` fields, the caller inputs, and each other. `RuntimeGates` remains
//! the sole env reader (nothing here calls `std::env`); the mode-dependent
//! defaults `gates.rs` leaves at the use site (`RWM_GEN_R`, `RWM_REACT_CAP`,
//! `RWM_INFL_BDP`) are resolved here against the pipeline booleans.
//!
//! The mechanism-liveness `info!` echoes stay in `run_window_sender`: they are
//! startup side effects, and hoisting them would reorder the log against the
//! `WindowStart` broadcast and the stall-witness spawn. For the same reason the
//! span-law trace's t0 (`span_diag_start_us = now_us()`) is not resolved here:
//! the sender rebinds `pol` with it at its point in setup.
//!
//! Not covered here — everything `run_window_sender` reassigns after setup:
//! the pacing token buckets and their refresh stamps (`gen_tokens`,
//! `src_tokens`, `cc_rate_cached`, `cc_rate_ceiling`, …), the derived-depth and
//! dynamic-cap caches (`gen_pipe_m`, `gen_pipe_store_cap`, `dyn_store_cap`,
//! `dyn_infl_cap`, `pa_*`), the per-path echo-ratio state (`percap_k`),
//! `emit_batch_live` (re-scoped every loop iteration on the live-path count),
//! the DIAG counter set, and the two DIAG t0 stamps. The mutable emission
//! state lives in [`SenderState`](super::emit_source::SenderState).

use super::{GEN_PIPE_MAX_GENS, MAX_WINDOW_SIZE, RELIABLE_STORE_MAX, shed_armed};
use crate::control::fec_rate::ProtocolHint;
use crate::gates::RuntimeGates;
use crate::scheduler::ANCHOR_MIN_SAMPLES;

/// Symbols that must be outstanding to buy one delivered-rate sample, at the
/// shipped merged-ack cadence.
///
/// `PathState::on_ack` calls `record_delivery` once per received ack datagram
/// and that call is the only site that pushes a `BwSample`, so "samples per
/// round" is "acks per round". With `RWM_ACK_MERGE` (default on) the receiver
/// emits one control datagram per data message (`[CTLD]` ≈ 1.0), so this
/// factor is 1.
///
/// Written as a named factor because it is the term that would move if the
/// ack cadence changed (a stretch-ack scheme acking every m-th symbol makes it
/// m).
pub const MERGED_ACK_SYMBOLS_PER_SAMPLE: usize = 1;

/// RFC 6928 — "Increasing TCP's Initial Window" — raises the permitted initial
/// window to 10 segments. Cited, not fitted: the standardised size of the
/// first flight a transport may put on an unmeasured path.
pub const RFC6928_INITIAL_WINDOW: usize = 10;

/// The bootstrap floor of the store-cap chain, derived from its job (paper
/// §6.1).
///
/// The floor's job is that a transiently-tiny BDP estimate must not strangle
/// the pipe. That is two independent lower bounds on the same quantity:
///
/// 1. **Keep the estimators warm.** The BtlBw anchor every pooled cap law
///    consumes does not exist until [`ANCHOR_MIN_SAMPLES`] delivered-rate
///    samples are in its window (`PathState::bdp_anchor` /
///    `effective_btlbw` both return `None` below it). A floor that funds fewer
///    symbols than that per round cannot buy the samples that let the law take
///    over — the floor would be self-sustaining. At the merged-ack cadence
///    that is `ANCHOR_MIN_SAMPLES · MERGED_ACK_SYMBOLS_PER_SAMPLE` = 8 · 1 = 8.
/// 2. **Never open below the standard initial burst.** [`RFC6928_INITIAL_WINDOW`]
///    = 10.
///
/// ```text
///   STORE_CAP_FLOOR = max( ANCHOR_MIN_SAMPLES · MERGED_ACK_SYMBOLS_PER_SAMPLE,
///                          RFC6928_INITIAL_WINDOW )
///                   = max( 8 · 1, 10 ) = 10
/// ```
///
/// `max`, not `+`: the two clauses are independent lower bounds on one
/// quantity, so their conjunction is the larger of them.
///
/// The floor only binds where the chain's unclamped ask is under it (on the
/// shipped chain, `Σ(max_bw·min_rtt)` under 16 symbols at N = 2), so in
/// practice it moves only the degenerate end (loopback). Bounded by
/// `derived_floor_is_the_max_of_its_two_clauses_and_only_moves_the_degenerate_end`.
pub const STORE_CAP_FLOOR: usize = {
    let warm = ANCHOR_MIN_SAMPLES * MERGED_ACK_SYMBOLS_PER_SAMPLE;
    if warm > RFC6928_INITIAL_WINDOW { warm } else { RFC6928_INITIAL_WINDOW }
};

/// The bootstrap cap, derived from its job — and equal to the floor (paper
/// §6.2).
///
/// Derived, not shipped: `RWM_STORE_BOOT` still defaults to 128. This constant
/// stands beside the shipped value so the gap is a fact in the source, and so
/// an arm that tests it has a named quantity to set.
///
/// Boot's job: cap outstanding before the BtlBw anchor warms, tight so the
/// startup burst cannot pre-bloat the queue and inflate the min-RTT floor
/// (which would inflate the anchor itself). As requirements:
///
/// 1. **It must buy the samples that end it**: at least
///    `ANCHOR_MIN_SAMPLES · MERGED_ACK_SYMBOLS_PER_SAMPLE` = 8 symbols, or it
///    cannot warm the anchor in one round trip.
/// 2. **It must not open below the standard initial burst**:
///    [`RFC6928_INITIAL_WINDOW`] = 10 — RFC 6928 sizes the first flight on an
///    unmeasured path, which is exactly the state before the anchor warms.
/// 3. **Tight, otherwise**: select the smallest value satisfying 1 and 2.
///
/// ```text
///   STORE_BOOT_DERIVED = max( ANCHOR_MIN_SAMPLES · MERGED_ACK_SYMBOLS_PER_SAMPLE,
///                             RFC6928_INITIAL_WINDOW )
///                      = max( 8 · 1, 10 ) = 10   ≡ STORE_CAP_FLOOR
/// ```
///
/// Boot and the floor are the same quantity — the outstanding bound when the
/// law has no measurement to offer — so this is a `const` equal to
/// [`STORE_CAP_FLOOR`] rather than a re-statement of it. 128 is a fit to one
/// cell's link budget (≈1.5 × a 100 Mbit / 10 ms BDP), never swept.
///
/// It did not ship because boot also bound mid-transfer when `active_paths()`
/// emptied and the pooled chain fell through to the boot branch; replacing
/// 128 with 10 there would have deepened that cliff. The store-cap Σ now
/// ranges over the channel's membership (`net::channel_paths`), so boot is
/// reachable only at genuine cold start, where the two values are
/// indistinguishable — not yet re-measured, so it still does not ship. Bounded by
/// `store_cap_bench.rs::derived_boot_is_the_floors_twin_and_is_inert_only_once_the_cliff_is_closed`.
pub const STORE_BOOT_DERIVED: usize = STORE_CAP_FLOOR;

/// Everything `run_window_sender` decides once and then only reads.
///
/// Grouped as the sender itself is: the caller's pipeline selection, the
/// generation stack, CC/pacing, the retention / flow-control laws, the
/// unified span/shed laws, the recovery plane, emission batching, and the
/// instruments. Each value's rationale is on its expression inside
/// [`SenderPolicy::resolve`].
#[derive(Debug, Clone)]
pub(crate) struct SenderPolicy {
    // ── The caller's pipeline selection ──────────────────────────────────
    /// Symbol payload size in bytes.
    pub symbol_size: u16,
    /// The (δ, ρ, r) named point the tunnel was opened at.
    pub protocol_hint: ProtocolHint,
    /// Retain-until-acked retention at the ARQ layer (ρ = 1).
    pub reliable: bool,
    /// Generation-based coding (paper §5.8): fixed generations, per-seq ARQ off.
    pub generation: bool,
    /// Systematic + deficit-repair: a submode of `generation`.
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
    /// `RWM_CC_PACE`: CC-rate pacing of the systematic source.
    pub cc_pace: bool,
    /// `RWM_CC_PACE_HR` headroom multiplier on the paced source rate.
    pub cc_pace_headroom: f64,
    /// `RWM_REACT_CAP` spacing scale; `0` = the unbounded (exempt) reactive arm.
    pub react_cap_cfg: f64,
    /// `react_cap_cfg > 0.0` — the bounded-reactive gate itself.
    pub react_cap_on: bool,
    /// `RWM_INFL_CAP` static in-flight cap (0 = off).
    pub infl_cap: u64,
    /// `RWM_INFL_BDP` gain on the BDP-derived in-flight cap.
    pub infl_bdp_gain: f64,
    /// `infl_bdp_gain > 0.0` — the dynamic in-flight cap gate.
    pub infl_bdp_on: bool,
    /// Per-path in-flight fullness; rides `gen_pipe`.
    pub infl_percap: bool,

    // ── Retention / flow control ─────────────────────────────────────────
    /// Coding-window / retention width (paper §5.5 W_mp; `RWM_WINDOW`).
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
    /// `RWM_STORE_PATHS`: path-scaled outstanding pool.
    pub store_paths_on: bool,
    /// `RWM_STORE_PATH_POOL`: per-live-path pool knee.
    pub store_path_pool: usize,
    /// `RWM_POOL_ANCHOR`: pool-anchor honest dual-store law.
    pub pool_anchor_on: bool,
    /// `RWM_THREE_TERM` (default off): the plain dynamic store cap is the
    /// composed three-term law (`net::three_term_store_cap`). Scoped to the
    /// plain dynamic cap.
    pub three_term_on: bool,
    /// `RWM_COMPOSED_CAP` (default off; paper §10): the composition arm. Implies
    /// [`Self::three_term_on`] (same pool law, same function) and additionally
    /// arms the late-stage per-path brake with the path's own cwnd as its cap,
    /// over `live_paths()` (with `active_paths()` the brake could never close).
    pub composed_cap: bool,
    /// `RWM_SUM_CAP` (default on; paper §6.1): the pooled law's count
    /// multiplier is removed from the value and kept in the ceiling
    /// (`net::pooled_store_cap`'s `sum_cap` argument). The Σ's path set is
    /// always the channel's membership (`net::channel_paths`). `=0` re-runs
    /// the quadratic.
    pub sum_cap: bool,
    /// `RWM_DELTA_CAP` (default on; paper §6.1): the pooled cap's value
    /// multiplier is `1 + q(δ)` — the CoDel-derived standing-queue setpoint
    /// (RFC 8289 §3.2) mapped continuously onto the δ dial — instead of
    /// `gain = 2.0`. Independent of [`Self::sum_cap`] (the count multiplier).
    /// With both on the pooled law is
    /// `cap = clamp((1 + q(δ))·Σᵢ bwᵢ·RTpropᵢ, floor, N·knee)`.
    pub delta_cap: bool,
    /// `RWM_LATE_BRAKE` (default off): the late-stage per-path cwnd brake,
    /// armed without the composed pool law — [`Self::composed_cap`]'s brake
    /// alone: same code path, same per-path cap (the path's own cwnd), same
    /// `live_paths()` set, no constant in the predicate. Off, `cwnd_full`
    /// stays false on the plain seat.
    pub late_brake: bool,
    /// The δ dial's deadline budget b(δ) at this tunnel's named point
    /// The δ dial's deadline budget b(δ) at this tunnel's named point
    /// (`net::delta_budget_b`) — a number on a dial, resolved once, read by
    /// the three-term law's stall term. The law is continuous and monotone
    /// in it.
    pub delta_b: f64,
    /// The retention dial ρ the three-term law is evaluated at.
    ///
    /// A value of the ρ axis, constant here by scope rather than by a branch:
    /// the plain dynamic cap exists only on the retain-until-acked path
    /// (`plain_dyn_cap ⇒ reliable`), whose declared retention contract is
    /// ρ = 1. `net::contract_stall_s` is continuous over ρ ∈ [0, 1].
    pub contract_rho: f64,
    /// `RWM_STORE_SACK_RELEASE`: SACK-clocked store release.
    pub store_sack_release_on: bool,

    /// `RWM_HONEST_CAP` (+ `RWM_PLAIN_RS`): honest floor-clock caps.
    pub honest_cap_on: bool,
    /// `diag_on && plain_dyn_cap` — under `RWM_DIAG` the per-path store
    /// attribution maps behind `[DIAG] sout=` are maintained. A gauge only.
    pub percap_track: bool,

    // ── The unified span / shed laws (paper §5.3, §5.6; ADR-0064) ────────
    /// `RWM_UNIFIED`: trailing solvable-span proactive-repair placement.
    pub unified_span: bool,
    /// `RWM_ASTAR_ANCHOR`: the windowed-max send-rate A* anchor.
    pub astar_anchor_on: bool,
    /// `RWM_UNIFIED_SHED`: δ-honest shedding, EVICT path only.
    pub shed_on: bool,
    /// `RWM_TAPER_R`: budget-conserving taper emission.
    pub taper_r_budget: bool,
    /// `RWM_MIN_R`: experimental per-symbol repair-rate floor.
    pub repair_rate_floor: f64,

    // ── The recovery plane ───────────────────────────────────────────────
    /// `RWM_RECOV_MP`: multipath recovery suppression.
    pub recov_mp: bool,
    /// `RWM_RECOV_MP_LAW`: the per-flight hole law under that umbrella.
    pub recov_mp_law: bool,
    /// `RWM_RECOV_SP`: single-path hole-law suppression.
    pub recov_sp: bool,
    /// `RWM_DERIVED_SWEEP` (default off): the tail-sweep / hole-refresh round
    /// on the derived law (2·SRTT floored by `patience_floor_us`, no ceiling)
    /// instead of `2·SRTT` clamped to [25, 100] ms.
    pub derived_sweep: bool,
    /// `RWM_HOLDDOWN_Q` as resolved by the gate — `None` by default, where the
    /// sender answers a reported hole immediately. A number, never a branch
    /// (paper §7.4).
    pub holddown_q: Option<f64>,
    /// `RWM_SIDLE_DERIVED` ∧ diag: the second, derived stall gauge.
    pub sidle_derived: bool,

    // ── Emission ─────────────────────────────────────────────────────────
    /// `RWM_EMIT_BATCH` as configured (the per-iteration scoping on the
    /// live-path count is `emit_batch_live`, a local — see the module doc).
    pub emit_batch_on: bool,
    /// `RWM_EMIT_BURST` pacer-quantum burst size (symbols).
    pub emit_burst: usize,
    /// Realtime packing: accumulate small packets into packed symbols.
    pub use_packing: bool,

    // ── Instruments ──────────────────────────────────────────────────────
    /// `RWM_DIAG` master gate.
    pub diag_on: bool,
    /// `RWM_ACKDIAG` master gate — the ack-cadence gauge (`net/ackdiag.rs`).
    /// Independent of `diag_on`: it prints its own `[ACKDIAG]` line on its own
    /// ~2 s cadence.
    pub ackdiag_on: bool,
    /// `RWM_WALLDIAG` master gate — the dead-wall onset/duration instrument
    /// (`net/walldiag.rs`). Independent of `diag_on` for the same reason
    /// `ackdiag_on` is: it prints ONE `[WALL]` line, at teardown.
    pub walldiag_on: bool,
    /// The span-law trace's t0. Not resolved by [`SenderPolicy::resolve`] (a
    /// wall-clock read, not a policy): the sender rebinds `pol` with it at its
    /// point in setup.
    pub span_diag_start_us: u64,
}

impl SenderPolicy {
    /// Resolve the sender's whole derived policy once, from the engine's
    /// `RuntimeGates` and the caller's pipeline selection.
    ///
    /// `gates` keeps its name so each expression reads as a plain gate
    /// lookup.
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
        // difference is the coding unit (a stable generation vs the moving window)
        // and that per-seq ARQ is disabled below.
        let coded_wire = coded_only || generation;
        let gen_size: usize = gates.gen_size;
        let pipeline: usize = gates.pipeline;
        // Generation-coding proactive overhead r (coded per generation beyond
        // K_G): the encoder provisions each generation to ceil(len·(1+r)) coded
        // before it is only coded for recovery. Systematic-repair provisions only
        // the loss-FEC overhead (the K base DoF ride the wire as source), so its
        // default is smaller than coded-only's (which must also fund the K base).
        // r ≳ 1.5·ε keeps windowed repair ahead of loss (r < ε → DNF).
        // `RWM_GEN_R` overrides.
        //
        // `RWM_GEN_PIPE` (generation only; paper §5.3, §5.8) composes the
        // app-side remedies so the substrate CC sees a queue-lean, BDP-covering
        // pipeline:
        //   1. per-path BDP in-flight cap (infl_bdp 1.5, percap) — queue ≈ 0,
        //      RTT ≈ RTprop;
        //   2. derived pipeline depth M* = ceil(rate·2·RTprop/G)+1 — generations
        //      in flight cover BDP + one deficit round, from measured rate/SRTT;
        //   3. coded-emission budget clocked on the sent frontier (the stalled
        //      cumulative ack must not freeze emission for the still-recovering
        //      oldest generation while M* fresh generations have budget);
        //   4. pace anchored to the windowed-max delivered rate (decode-clocked
        //      samples are mostly low; a decaying EWMA under-reads between
        //      generation decodes);
        //   5. once-per-SRTT deficit action (react_cap 1.0).
        // `RWM_GEN_PIPE=0` reproduces the fixed pipeline depth.
        let gen_pipe = gates.gen_pipe && generation;
        // `RWM_MSTAR_ANCHOR` (ADR-0061): the M* anchor-pair repair — (a) the
        // peer-report 50-ms pseudo-sample no longer pins the RTprop floor, (b) the
        // windowed-max delivered-rate filter seeds from 500-ms buckets instead of
        // 2-s ones, so the anchor is live within ~1 bucket of the first acks.
        let mstar_anchor = gates.mstar_anchor && generation;
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
        // CC-rate pacing of the systematic source (`RWM_CC_PACE`). The systematic
        // source rides the droppable QUIC-datagram path driven only by TUN-read
        // intake, gated by a BDP-scaled window but not by a rate. At high RTT the
        // window is BDP-sized, so the source is spent as one burst that the path
        // drops faster than the receiver decodes — per-generation loss exceeds
        // the ceil(len·r) proactive budget and recovery goes reactive. This paces
        // the source at the measured link rate with a small burst.
        //
        // Rate signal: the delivered-goodput EWMA (`gen_rate_ewma`), the achieved
        // BtlBw in generation mode. The Copa `cwnd` is not usable here:
        // window-mode WindowAcks do not drive `record_delivery`, so cwnd sits at
        // INITIAL_CWND and cwnd/SRTT would strangle the pipe. A small headroom
        // lets the rate ramp without overrunning the datagram path.
        //
        // Default on under the wire-clocked Copa signal (ADR-0062): Copa's model
        // assumes a paced wire, but under `RWM_QUIC_CC=passthrough` quinn's pacer
        // derives from the engine window and never binds at Copa's Bulk
        // operating point, so the send process degrades to ack-clocking and each
        // loss burst's recovery micro-stall idles the bottleneck. `RWM_CC_PACE=0`
        // forces it off.
        let cc_pace = gates.cc_pace;
        let cc_pace_headroom: f64 = gates.cc_pace_headroom;
        // Bounded reactive under congestion control (`RWM_REACT_CAP`). An
        // unbounded deficit loop is exempt from the in-flight cap and re-emits
        // the reported residual on every deficit report; at high RTT the reports
        // are ~RTT stale, so it re-sends faster than an updated report can shrink
        // the deficit, its own symbols overrun the pipe and drop, and it
        // re-floods. Two bounds close the loop:
        //   (a) per-generation RTT spacing: after emitting recovery for a
        //       generation, wait ~1 SRTT before emitting for it again, so the
        //       receiver's next deficit report reflects those symbols;
        //   (b) non-exempt from the in-flight cap: reactive stops at `cwnd_full`
        //       like proactive. The in-flight budget expires on the RTT
        //       timescale, so the frontier is still funded within a bounded
        //       delay (no deadlock).
        // Any value enables it; the value scales the spacing (<1 = fraction of
        // SRTT, >=1 = absolute µs). gen_pipe defaults to one deficit feedback per
        // RTT (1.0·SRTT): a sub-RTT re-flood of the fungible top-up defeats
        // aggregation.
        let react_cap_cfg: f64 = gates
            .react_cap
            .unwrap_or(if gen_pipe { 1.0 } else { 0.0 })
            .max(0.0);
        let react_cap_on = react_cap_cfg > 0.0;
        // In-flight coded allowance W (coded symbols the pipe may hold ahead of
        // the decode frontier). Must be ≥ pipeline·gen_size: coded symbols are
        // striped round-robin across the M active generations, so for the first
        // generation to accumulate its K_G (and decode, advancing the ack that
        // grows the target) each needs ~gen_size coded in flight at once. Below
        // M·G the first generation never reaches K_G, ack stays 0, and the target
        // never grows — a startup deadlock. Default 2·M·gen_size.
        // `RWM_GEN_INFLIGHT` overrides.
        let gen_inflight_window: f64 = gates
            .gen_inflight
            .unwrap_or((2 * pipeline * gen_size) as f64);
        // `RWM_MIN_R`: experimental per-symbol repair-rate floor (repairs per
        // source symbol). The Bulk χ glide (paper §4.6) drives r* → 0
        // mid-stream, leaving the window systematic, so a heterogeneous slow
        // path's source symbols are fixed positions the fast path cannot decode
        // around. Raising r makes the pooled window fungible. 0 = the production
        // glide. A test instrument, not a shipped control law.
        let repair_rate_floor: f64 = gates.min_r;
        // Out-of-order retention decouple (`RWM_OOO_RETAIN`, generation only).
        // Generation backpressure caps the send frontier at ~store_max = a few
        // generations ahead of the cumulative (in-order) decode ack, so one hole
        // stalls the whole pipeline — throughput ∝ window/RTT, ARQ's
        // serialization. This raises the retention/backpressure window to
        // `ooo_gens` generations so the sender keeps sending (and proactively
        // coding, via `set_code_base`) past a stalled in-order frontier; the
        // stalled generation is recovered by the bounded reactive tail. Retention
        // still drops on the in-order ack, so reliability is unchanged; memory
        // is bounded by `ooo_gens·G`. The value is the generation count
        // (default 16).
        let ooo_retain = gates.ooo_retain && generation;
        let ooo_gens: usize = gates.ooo_gens;
        // Fungible frontier window sizing (paper §5.5, W_mp). A hole at the
        // frontier is raced by coded symbols that combine over the current
        // window; sustained Σg aggregation needs the window to span the
        // cross-path recovery horizon, W_mp ≳ Σg·(RTT_max+t_slack) ≈ 600 symbols
        // at the heterogeneous dual — 3× the systematic MAX_WINDOW_SIZE = 200.
        // Coded-only therefore widens the coding window to W_mp (default 640,
        // `RWM_WINDOW` overrides); the oracle (`oracle_c8_fungible_wmp_window`)
        // confirms W ≥ 384 reaches the ceiling while W = 200 does not.
        // Systematic modes keep 200.
        let win_cap: usize = if generation {
            // Generation mode retains the whole in-flight pipeline: M generations
            // of G symbols (plus one for the filling head) — every not-yet-decoded
            // generation stays retained and keeps getting coded symbols until it
            // decodes. `RWM_OOO_RETAIN` widens this to `ooo_gens` generations;
            // under gen_pipe the retention ceiling is the M* hard cap (the dynamic
            // intake cap `gen_pipe_store_cap` is what bounds the queue).
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
        // Fungible-frontier retention bound = the coding window itself (W_mp
        // doing double duty): the backpressure cap keeps the send frontier within
        // one window of the cumulative ack, so every not-yet-decoded seq stays
        // inside the current coding window and is raced by ongoing coded symbols
        // rather than aging out to a congestion-throttled targeted ARQ. With the
        // systematic RELIABLE_STORE_MAX = 1024 > W the frontier would run ~1024
        // ahead while the window covers only the last 640; lifting the cap
        // entirely decouples them and DNFs. `RWM_STORE` overrides.
        let store_max: usize = if generation {
            // Backpressure at the pipeline bound: the send frontier may run at
            // most ~M generations ahead of the cumulative-decode frontier. TUN
            // reads pause here (flow control), never dropping data. Generation
            // mode uses the encoder's retained size as the backpressure signal
            // (no sent_store).
            //
            // Backpressure at G·(M+1) is many BDPs, so the unacked pipeline
            // becomes a standing queue that produces slow-run outliers and
            // serializes dual-path aggregation (the fast path stalls on the
            // bloated in-order-frontier feedback). The send frontier needs only
            // two generations outstanding to pipeline — one filling head + one
            // sealed-and-recovering — so backpressure is at 2·G while retention
            // stays at win_cap = G·(M+1) for decode headroom. Under OOO retention
            // it is the wide ooo_gens·G; under gen_pipe the static cap is the M*
            // ceiling and the dynamic per-loop cap (`gen_pipe_store_cap` = M*·G)
            // gates intake each iteration.
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
            // Plain-reliable (non-generation) memory ceiling for the retention
            // store. `RWM_STORE` forces a static window (disables the dynamic BDP
            // cap below); by default the large retention ceiling stays and the
            // delay-based `plain_dyn_cap` bounds the outstanding window instead.
            gates.store_override.unwrap_or(RELIABLE_STORE_MAX)
        };
        // Delay-based send-window cap for the plain-reliable path (paper §6).
        // A fixed RELIABLE_STORE_MAX (1024) is many BDPs at a 10 ms path, so the
        // unacked store builds a standing queue. Under loss every hole must
        // traverse that queue to recover, the cumulative ack (and the ack-clocked
        // pacing) freezes for a bufferbloat RTT, and single-path throughput
        // collapses. Bounding the outstanding window to a BDP-scaled cap keeps
        // the queue — and recovery latency — near one RTT. BtlBw×RTprop is
        // bufferbloat-robust (windowed-max rate × min-RTT floor). Active only
        // for the plain-reliable path and only when `RWM_STORE` is not forcing
        // a static window; generation/coded-only keep their own structural caps.
        let plain_dyn_cap =
            reliable && !generation && !coded_only && !gates.store_env_set;
        // Window = gain × BDP, gain = 2.0 (paper §6.2: unprovenanced as a
        // derivation). Its published analogues:
        //  - RFC 6182 §5.3 recommends 2*BDP buffers: "One BDP allows
        //    supporting reordering of segments by the network. The other BDP
        //    allows the connection to continue during fast retransmit";
        //  - BBR's cwnd_gain = 2 "bounds in-flight data to a small multiple of
        //    the BDP, in order to handle common network and receiver
        //    pathologies, such as delayed, stretched, or aggregated ACKs"
        //    (draft-cardwell-iccrg-bbr-00 §4.2.3.2; draft-ietf-ccwg-bbr §2.5
        //    re-derives the same 2 as "the minimum gain value that allows the
        //    sending rate to double each round"). BBR handles recovery by
        //    packet conservation and prior_cwnd, never via cwnd_gain.
        // `RWM_STORE_GAIN` overrides.
        let store_bdp_gain: f64 = gates.store_gain;
        // Cap before the BtlBw anchor warms (a few RTTs). Tight so the startup
        // burst can't pre-bloat the queue and inflate the min-RTT floor (which
        // would then inflate the anchor itself); the anchor takes over once
        // samples land. See `STORE_BOOT_DERIVED` for the derived value.
        let store_boot_cap: usize = gates.store_boot;
        // Floor so a transiently-tiny BDP estimate can't strangle the pipe;
        // derived — see `STORE_CAP_FLOOR`.
        let store_cap_floor: usize = STORE_CAP_FLOOR;
        // Path-scaled outstanding pool (`RWM_STORE_PATHS`, default on; paper
        // §6.1). A per-transfer outstanding ceiling (RELIABLE_STORE_MAX) leaves a
        // multipath sender store-starved: the pool must fund Σ per-path
        // (BDP + one recovery round), which grows with the path count. With
        // N = live_paths ≥ 2 the dynamic-cap value scales with N and its ceiling
        // becomes N × 2048 (`RWM_STORE_PATH_POOL`, the measured per-live-path
        // knee); N = 1 keeps the single-path law bit-exactly.
        let store_paths_on = gates.store_paths;
        let store_path_pool: usize = gates.store_path_pool;
        // Pool-anchor honest dual-store law (`RWM_POOL_ANCHOR`). At N ≥ 2 live
        // paths the pooled-store cap's rate input is the per-path send-interval
        // anchor (`SendRateAnchor` fed at `charge_in_flight` — burst-immune by
        // construction: Δt spans the send interval, so ack bursts cannot inflate
        // it; clock-gap buckets discarded) instead of the ack-interval
        // windowed-max, which over-reads. Law: pool = clamp(Σ_i
        // honest_store_cap(sr_i·RTprop_i, sr_i, K_i, gain), floor, N·knee) —
        // the capw shape (one shared pool, borrowing free), engaged only with
        // all live send-anchors warm; until then the configured path-scaled law
        // runs. The Copa cwnd feed and every N = 1 law are untouched. The
        // default rides the `RWM_EST_CADENCE` resolution (off when unset).
        let pool_anchor_on = gates.pool_anchor && plain_dyn_cap;
        // The store-cap path set is not a dial: the dyn-cap phase's Σ-anchor
        // base and honest per-path cap sum range over the channel's
        // membership (`net::channel_paths`), the set `n_live` is counted from.
        // The composed three-term limit (`RWM_THREE_TERM`, default off; paper
        // §10): Σ per-path network window + Σ per-path emission slack + one
        // resequencing span, each Little's law over a measured signal. The span
        // term is identically zero at a single path by arithmetic (max RTprop =
        // min RTprop), so there is no `if N == 1`. Scoped to the plain dynamic
        // cap: under Copa ownership cwnd is the operating point.
        //
        // The composed cap (`RWM_COMPOSED_CAP`, default off): the same pool law
        // as `three_term_on` (`net::three_term_store_cap`), plus the unified live
        // set at the brake, plus the late-stage per-path brake on the path's own
        // cwnd. It introduces no law and no constant of its own; the three-term
        // arm stays exactly what it was.
        let composed_cap = gates.composed_cap && plain_dyn_cap;
        let three_term_on = (gates.three_term || composed_cap) && plain_dyn_cap;
        // The `×N` deletion (`RWM_SUM_CAP`, default on; paper §6.1). The pooled
        // law's count multiplier is removed from the value and kept in the
        // ceiling:
        //
        //   `=0` arm   cap = clamp( gain · N · Σᵢ(max_bwᵢ·min_rttᵢ), floor, N·knee )
        //   shipped    cap = clamp( gain     · Σᵢ(max_bwᵢ·min_rttᵢ), floor, N·knee )
        //
        // The quantity the pool must fund, Σ per-path (BDP + one recovery round
        // of runway) = `Σᵢ(gain·anchorᵢ) = gain·Σ`, is already linear in the
        // path count because the Σ is; the `=0` arm multiplies it by N a second
        // time. Exactly one factor changes: gain, knee, floor, Σ-set and
        // estimator are identical on both arms. Bit-identical at N = 1 by
        // construction (`n_live < 2` returns None before the multiplier is
        // read).
        // `RWM_SUM_CAP=0` re-runs the quadratic.
        let sum_cap = gates.sum_cap && plain_dyn_cap;
        // The δ-priced value multiplier (`RWM_DELTA_CAP`, default on; paper
        // §6.1). The pooled law's value multiplier is the CoDel-derived setpoint
        // band instead of `gain = 2.0`:
        //
        //   cap = clamp( (1 + q(δ)) · Σᵢ(bwᵢ·RTpropᵢ), floor, N·knee )
        //   q(δ) = 0.05 + 0.05·(clamp(b(δ), ½, 2) − ½)/(2 − ½)  ==  (b+1)/30
        //
        // RFC 8289 §3.2 derives 5–10 % of RTT from Kleinrock power
        // maximisation. One factor changes: the Σ, its path set, the estimator,
        // the ceiling and the floor are identical on both arms. Independent of
        // `sum_cap` (the count multiplier) — two axes of one law. Bit-identical at N = 1 by construction.
        // `RWM_DELTA_CAP=0` re-runs `gain = 2.0`.
        let delta_cap = gates.delta_cap && plain_dyn_cap;
        // The late-stage brake alone (`RWM_LATE_BRAKE`, default off). The
        // predicate, whole:
        //
        //   brake closes  ⟺  ∀ i ∈ live_paths() : in_flightᵢ ≥ cwndᵢ
        //
        // This is `composed_cap`'s brake without its pool law — the same code
        // path, the same per-path cap (the path's own cwnd: derived, never
        // configured, always warm), the same `live_paths()` set (with
        // `capᵢ = cwndᵢ` the predicate is exactly `available()ᵢ == 0`, so an
        // `active_paths()` brake would never close). No constant appears in the
        // predicate. Without it the brake arms only with `composed_cap` (which
        // forces the three-term pool law) or `RWM_INFL_CAP`'s global
        // `Σ in_flight ≥ n` against an operator constant.
        //
        // Not scoped to `plain_dyn_cap`: the brake is an emission-side
        // congestion test, not a cap law. Off, `cwnd_full` stays false on the
        // plain seat and the store cap remains the sole brake on outstanding.
        let late_brake = gates.late_brake;
        // b(δ) at this tunnel's named point on the dial, once.
        let delta_b = crate::net::delta_budget_b(protocol_hint);
        // ρ, the retention dial's declared value in this scope (see the field
        // doc): the plain dynamic cap is retain-until-acked, so the contract's
        // ρ is 1 here — a scope, not a branch.
        let contract_rho: f64 = 1.0;
        // SACK-clocked store release (`RWM_STORE_SACK_RELEASE`, default on;
        // paper §6.3, ADR-0060). Releasing slots only on the cumulative frontier
        // makes SACKed-but-not-cumulative symbols hold slots a full frontier
        // round, so the store recycles at frontier latency, not path rate. Under
        // this law a SACKed seq is uncounted from the flow-control outstanding
        // (the window opens) while sent_store + retransmit_buffer + nack_retx_at
        // + source_path_map are kept until the cumulative frontier passes it —
        // release a store slot, never recoverability (see `sack_release_mark`).
        // `=0` is the frontier-only-release arm, under which the released set
        // stays empty and the gate arithmetic is exactly `store_len`.
        let store_sack_release_on =
            reliable && !generation && !coded_only && gates.store_sack_release;
        // Honest floor-clock store caps (`RWM_HONEST_CAP`, with `RWM_PLAIN_RS`).
        // The ack-interval plain anchor over-reads, so a knee-clamped slow-path
        // cap never engages the derived per-path differentiation. With the
        // honest send-interval sampler (`RWM_PLAIN_RS`, default off) the anchor
        // reads ≈1× truth, and the cap law is re-derived on it:
        // cap_i = anchor_i·(K_i + gain − 1) + rate_i·(gain−1)·R — residence on
        // the measured unloaded drain clock plus runway on the recovery
        // engine's clock (R = the 100-ms hole-refresh/tail-sweep bound), see
        // `honest_store_cap`. K supplies explicitly the headroom the over-read
        // supplied by accident. Engaged only where the honest sampler is live
        // (plain in-order, no Copa CC ownership — the Σcwnd and per-path cwnd
        // laws are already honest). `RWM_HONEST_CAP=0` is the floor-law arm.
        let honest_cap_on = plain_dyn_cap && gates.plain_rs && gates.honest_cap;
        // Budget-conserving taper (`RWM_TAPER_R`, default = `RWM_UNIFIED`; paper
        // §3.3). A per-ack-cycle taper accrual sums to Σ τ(t) = r symbols per
        // ack cycle (taper_offset resets on cumulative-ack advancement), so the
        // emitted proactive overhead would be ~r/cycle-length, nearly
        // independent of r's computed magnitude — the r* control loop would be
        // inert at the wire. With the flag on, `TaperBudget` makes emission
        // consume r as computed: a per-window budget (emitted ≈ r × source per
        // coding window), the taper shape kept as a re-timing (repair still
        // concentrated at the frontier), paced ≤ 1 repair per source send and
        // spare-capped. It composes with the trailing solvable-span placement
        // below, which removes the leading-window entanglement that makes the
        // quantity alone recovery-inert. `RWM_TAPER_R=0` reproduces the
        // per-ack-cycle accrual.
        let taper_r_budget = gates.taper_r;
        // Trailing solvable-span placement for plain-mode proactive repair
        // (paper §5.3) — span width A* = clamp(rate·D, 1, W) with
        // D = b(δ)·RTprop (Realtime ½, Auto 1, Bulk 2 RTT — capped at 2·RTprop,
        // the deficit-round limit) and trailing offset Δ = ceil(rate·jitter) ≥ 1,
        // so every covered member has landed when the repair does (solvable at
        // arrival).
        let unified_span = gates.unified;
        // `RWM_ASTAR_ANCHOR` (ADR-0061, default on under the unified machine):
        // the A* rate anchor. An EWMA of the report-tick send rate (`est.
        // throughput()`) pins A* = 1 for the first seconds of a stream and is
        // flood-poisonable off a post-stall release burst. The repair: a
        // windowed-max send-rate anchor (`SendRateAnchor`) fed by the sender's
        // own send events — live within ~1 RTT, with gap-spanning/flood buckets
        // discarded. `RWM_ASTAR_ANCHOR=0` / `RWM_ANCHOR_HYGIENE=0` opt out.
        let astar_anchor_on = unified_span && gates.astar_anchor;
        // δ-honest overload shedding (paper §5.6, ADR-0064): armed only on the
        // EVICT path (`!reliable` — the ρ = 1 retain contract is excluded by
        // construction) under `RWM_UNIFIED`; `RWM_UNIFIED_SHED=0` is the
        // serializing arm. A hole whose retransmit can no longer meet the δ
        // deadline D = b(δ)·RTprop is dropped from the ARQ set instead of
        // serializing the stream behind it — but only while cumulative shed stays
        // within the derived 1−ρ budget (`residual_loss_after_fec`: ε̂·(1−P_fec)
        // at the live (r, A*, σ²) operating point). Budget spent ⇒ serialize
        // (ρ wins over δ).
        let shed_on = shed_armed(gates.unified, reliable, gates.unified_shed);
        // Proactive-repair pacer (`RWM_PROACTIVE_PACER`, systematic only) —
        // present-at-stall. A dedicated proactive-repair emission on the
        // generation grid, decoupled from both source availability and the
        // ack-clock `target`. For each in-flight generation (still filling or
        // recently sealed) it emits proactive repair over the retained contiguous
        // prefix at the full generation width (`generate_repair_filling` → same
        // (anchor, G) matrix, no cross-grid stranding), paced by the shared CC
        // token bucket. It runs every main-loop iteration including tx_paused
        // wakeups, so repair flows under backpressure, and it codes the
        // generation grid, so a buffered filling equation combines directly with
        // the reactive generation deficit. The covering equation reaches the
        // receiver around when the hole is sent, so it is present when the
        // frontier detects the hole → proactive decode, no round trip. Supersedes
        // the sealed batched proactive path when on; the reactive deficit
        // (`RWM_REACT_CAP` + `RWM_REPAIR_WAIT`) stays the bounded fallback.
        let proactive_pacer = systematic && gates.proactive_pacer;
        // Cross-path repair placement (`RWM_XPATH_REPAIR`, generation only,
        // default off). Route proactive (and deficit) repair to the
        // max-spare-capacity path instead of the marginal-cost softmax (which
        // biases repair toward the fast path, where it competes with systematic
        // source). A fast-path loss is then covered by repair already in flight
        // on the slow path, without displacing fast-path source. Symmetric paths
        // have equal spare, so `place_repair_spare_path` splits the near-tie set
        // uniformly (no hard-argmax concentration).
        let xpath_repair = generation && gates.xpath_repair;
        // Symbol packer: accumulate small packets into packed symbols for Realtime mode
        let use_packing = protocol_hint == ProtocolHint::Realtime;
        // `RWM_DIAG` master gate. Carried into the emission step as
        // `SenderPolicy::diag_on` (the GLIFE fill tracking).
        let diag_on = gates.diag;
        // Per-path store-attribution gauge (no behaviour): under `RWM_DIAG` the
        // account maps are maintained so the DIAG `sout=` field shows each
        // path's share of the pooled outstanding (which path is holding the
        // unacked-frontier span). The maps feed the DIAG print alone.
        let percap_track = diag_on && plain_dyn_cap;
        // Multipath recovery suppression (`RWM_RECOV_MP`, default on; paper §7.1,
        // ADR-0059). `=0` is the global-clock opt-out arm. Plain window reliable
        // mode only — generation mode has no per-seq ARQ to suppress.
        // `RWM_RECOV_MP_LAW` (per-flight hole law, default on under the
        // umbrella) is the sub-gate for trace attribution.
        let recov_mp = gates.recov_mp && reliable && !generation;
        let recov_mp_law = recov_mp && gates.recov_mp_law;
        // Single-path hole-law suppression (`RWM_RECOV_SP`, default off). The
        // N = 1 bypass of `mp_hole_ripe` assumes single-path gaps are FIFO-real;
        // on a jittery substrate delay jitter reorders tens of packets deep, so
        // the receiver's gap reports name merely-late seqs and re-fires chase
        // flights still queued behind the store-cap standing queue. The law: at
        // N = 1 a gap seq with a live flight (original or retransmit) fires only
        // once the flight is ≥ 9/8×max(smoothed clocks) old (RFC 9002 §6.1.2,
        // same `mp_time_threshold_us`); time channel only — the §6.1.1 packet
        // channel is excluded at N = 1 (reorder depth ≫ kPacketThreshold).
        // Suppression-only: the receiver's hole-refresh re-advertises until the
        // flight ripens, so real holes still recover.
        let recov_sp = gates.recov_sp && reliable && !generation;
        // `sidle_derived` is DIAG-only (the second, derived stall gauge printed
        // beside the unchanged one). Default off.
        let sidle_derived = gates.sidle_derived && diag_on;
        // Emission batching (`RWM_EMIT_BATCH`, default off). The sender-emission
        // service wall is per-symbol loop cost: taper/span control math
        // (compute_repair_rate + predictive_loss_upper + exp/log, recomputed per
        // symbol), plus a full select! iteration (tail-deadline scan, SACK
        // drain, pacing refresh) and the waker churn of one-datagram-per-wakeup
        // handoff to quinn (syscall density is not the wall — quinn-udp GSO
        // already batches). Under the gate the sender:
        //   1. drains TUN intake in pacer-quantum bursts (≤ emit_burst symbols
        //      per loop iteration, ~64 KB — inside the flow-control store
        //      headroom and the cc_pace token bucket, checked per symbol), so
        //      loop-iteration overhead amortizes and quinn's endpoint driver
        //      sees a multi-datagram queue;
        //   2. refreshes the derived taper/span math once per burst instead of
        //      per symbol (the A* send-rate anchor is still fed per symbol).
        // Plain window-reliable mode only (generation/coded emission has its own
        // paced block). Single live path only: dual cells are wire/recovery-
        // bound and bursting there lengthens same-path arrival runs, which
        // inflates the per-path loss misread; with N ≥ 2 live paths the emission
        // path is unchanged (`emit_batch_live` re-checked per loop iteration, so
        // path flaps re-scope within one burst). Realtime (packed) mode is
        // excluded outright: its per-packet latency path must never trade a
        // wakeup for a burst. The taper cache carries a 50 ms staleness bound so
        // a low-rate bulk-hint tunnel never runs the span/shed law on second-old
        // anchors.
        let emit_batch_on = gates.emit_batch && reliable && !coded_wire && !use_packing;
        let emit_burst: usize = gates.emit_burst;
        // Generation-mode in-flight cap (`RWM_INFL_CAP`): bound the unacked
        // symbols to ~BDP instead of store_max = G·(M+1), which is decoupled
        // from the pipe, so unpaced source emission would build a standing queue
        // that turns every hole into a long recovery stall. 0 = off
        // (store-only backpressure). The deficit-recovery emission is exempt (it
        // must always be able to fund a frontier hole, else a full-window pipe
        // deadlocks).
        let infl_cap: u64 = gates.infl_cap;
        // BDP-derived in-flight cap (`RWM_INFL_BDP` = gain): bound total
        // in-flight to gain × Σ copa_bdp_anchor (BtlBw×RTprop, bufferbloat-
        // robust), recomputed live, so the standing queue — and the
        // recovery-round RTT — stays ~gain·BDP at any RTT. It gates both
        // proactive emission and (non-exempt) reactive recovery via `cwnd_full`,
        // so the tail flush cannot re-bloat the queue. 0/unset = off. gen_pipe
        // defaults it to 1.5: the bare aggregate BDP starves the recovery
        // headroom, and ~1.5× over the (under-estimating) windowed-max anchor
        // keeps the queue — and the RTT the substrate CC sees — near RTprop.
        let infl_bdp_gain: f64 = gates
            .infl_bdp
            .unwrap_or(if gen_pipe { 1.5 } else { 0.0 })
            .max(0.0);
        let infl_bdp_on = infl_bdp_gain > 0.0;
        // Enforce the in-flight cap per path (path i outstanding ≤
        // gain·BtlBw_i·RTprop_i) rather than as one fungible Σ budget, under
        // gen_pipe. The sender is TUN-paused only when every active path is at
        // its own cap, so the fast path keeps pulling fresh source while the slow
        // path is full.
        let infl_percap = gen_pipe;
        // Generation mode: clock the coded-emission budget to the sent source
        // frontier instead of the acked frontier (`RWM_CODED_SRC`). The
        // ack-clocked `target = ack·(1+r) + W` deadlocks a small generation: once
        // the proactive budget W is spent, coded stops until the ack advances —
        // but the ack is stalled because the frontier generation is missing the
        // coded it needs to decode. Sourcing the budget from the sent frontier
        // lets the encoder's per-generation ceil(K_g·(1+r)) cap and the
        // M-generation retention bound govern coded emission.
        let coded_src_clock = gates.coded_src;
        // Pure-proactive demonstrator (`RWM_NO_REACTIVE`): disable the
        // deficit-driven reactive recovery loop entirely. All recovery then
        // comes from the upfront proactive per-generation budget (ceil(len·r)),
        // with no recovery-emission path exempt from the in-flight cap, so every
        // emitted symbol is bounded by `RWM_INFL_CAP`. It isolates whether, with
        // enough upfront repair, proactive FEC beats ARQ at high RTT; a
        // generation that loses more than its budget never decodes (the object
        // DNFs), which is itself the result.
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
            pool_anchor_on,
            three_term_on,
            composed_cap,
            sum_cap,
            delta_cap,
            late_brake,
            delta_b,
            contract_rho,
            store_sack_release_on,
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
            derived_sweep: gates.derived_sweep,
            holddown_q: gates.holddown_q,
            sidle_derived,
            emit_batch_on,
            emit_burst,
            use_packing,
            diag_on,
            ackdiag_on: gates.ackdiag,
            walldiag_on: gates.walldiag,
            // Sampled by the sender at its original point in setup (see the
            // module doc); rebound there via a struct-update on `pol`.
            span_diag_start_us: 0,
        }
    }
}
