use raptorpath_math as math;
use wasm_bindgen::prelude::*;

mod warm_tail;
pub mod knobs;
pub mod consumer;
pub mod link;
pub mod budget;

// =========================================================================
// Three-knob laws (proposal; docs/derivations/visualizer-three-knobs-plan.md
// §5.1): thin forwarding wrappers over `knobs.rs`, which holds every law.
// Prefixed `knob_` (`protected` is a reserved word in JavaScript).
// =========================================================================

#[wasm_bindgen]
pub fn knob_law_revision() -> String {
    knobs::KNOB_LAW_REVISION.to_string()
}

#[wasm_bindgen]
pub fn knob_intake_expired(lifetime_s: f64, age: f64) -> bool {
    knobs::intake_expired(lifetime_s, age)
}

#[wasm_bindgen]
pub fn knob_resend_admissible(lifetime_s: f64, age: f64, x: f64) -> bool {
    knobs::resend_admissible(lifetime_s, age, x)
}

#[wasm_bindgen]
pub fn knob_within_lifetime(lifetime_s: f64, age: f64) -> bool {
    knobs::within_lifetime(lifetime_s, age)
}

#[wasm_bindgen]
pub fn knob_dequeue_expired(lifetime_s: f64, age: f64, m: f64) -> bool {
    knobs::dequeue_expired(lifetime_s, age, m)
}

#[wasm_bindgen]
pub fn knob_protected(lifetime_s: f64, now: f64, last_send: f64, d: f64, h: f64, o_hat: f64) -> bool {
    knobs::protected(lifetime_s, now, last_send, d, h, o_hat)
}

#[wasm_bindgen]
pub fn knob_trigger_delay(resent: bool, c: f64, srtt_path: f64, p_reach: f64) -> f64 {
    knobs::trigger_delay(resent, c, srtt_path, p_reach)
}

#[wasm_bindgen]
pub fn knob_one_way_fwd(srtt: f64, rtprop: f64) -> f64 {
    knobs::one_way_fwd(srtt, rtprop)
}

#[wasm_bindgen]
pub fn knob_release_at(t_detect: f64, hold_s: f64, t_floor: f64, h_f: f64) -> f64 {
    knobs::release_at(t_detect, hold_s, t_floor, h_f)
}

#[wasm_bindgen]
pub fn knob_ack_timeout(srtt: f64, rttvar: f64, g: f64, ack_delay: f64) -> f64 {
    knobs::ack_timeout(srtt, rttvar, g, ack_delay)
}

// =========================================================================
// Pure math functions — thin wasm-bindgen wrappers
// =========================================================================

#[wasm_bindgen]
pub fn normal_survival(z: f64) -> f64 {
    math::normal_survival(z)
}

#[wasm_bindgen]
pub fn p_lost(age_secs: f64, epsilon: f64, srtt_secs: f64, rttvar_secs: f64) -> f64 {
    math::p_lost(age_secs, epsilon, srtt_secs, rttvar_secs)
}

#[wasm_bindgen]
pub fn burst_variance_factor(p: f64, q: f64) -> f64 {
    math::burst_variance_factor(p, q)
}

#[wasm_bindgen]
pub fn compute_r_star(epsilon: f64, sigma2: f64, window_size: f64) -> f64 {
    math::compute_r_star(epsilon, sigma2, window_size)
}

/// Continuous r* (§4.2): quantile at 1 - delta/eps; glides to 0.
#[wasm_bindgen]
pub fn compute_r_star_continuous(epsilon: f64, sigma2: f64, window_size: f64, delta: f64) -> f64 {
    math::compute_r_star_with_z(epsilon, sigma2, window_size, math::z_for_tail_target(delta, epsilon))
}

#[wasm_bindgen]
pub fn z_for_tail_target(delta: f64, epsilon: f64) -> f64 {
    math::z_for_tail_target(delta, epsilon)
}

#[wasm_bindgen]
pub fn r_saturation(epsilon: f64, sigma2: f64, window: f64, srtt: f64, t_sym: f64) -> f64 {
    math::r_saturation(epsilon, sigma2, window, srtt, t_sym)
}

/// Derived encoder window W* (§4.8): balances overhead (1/sqrt(W)
/// margin), recovery latency (W/send_rate within budget), and burst
/// absorbency. Clamped to [16, 512]. See `math::derive_window`.
#[wasm_bindgen]
pub fn derive_window(
    delta: f64, epsilon: f64, sigma2: f64, srtt: f64, send_rate: f64, latency_budget: f64,
) -> f64 {
    math::derive_window(delta, epsilon, sigma2, srtt, send_rate, latency_budget)
}

/// Soft saturation cap (§4.4): kink-free approach to r_sat.
#[wasm_bindgen]
pub fn soft_saturate(rate: f64, r_sat: f64) -> f64 {
    math::soft_saturate(rate, r_sat)
}

/// Saturation pressure in [0,1] (§4.4): a continuous indicator of how hard
/// the soft cap binds. `rate_requested` is the uncapped controller output.
#[wasm_bindgen]
pub fn saturation_pressure(rate_requested: f64, r_sat: f64) -> f64 {
    math::saturation_pressure(rate_requested, r_sat)
}

#[wasm_bindgen]
pub fn p_fec_exact(p_gb: f64, q_bg: f64, r: f64, window_size: usize) -> f64 {
    math::p_fec_exact(p_gb, q_bg, r, window_size)
}

#[wasm_bindgen]
pub fn compute_r_star_exact(p_gb: f64, q_bg: f64, window_size: usize, delta: f64) -> f64 {
    math::compute_r_star_exact(p_gb, q_bg, window_size, delta)
}

/// The warm-started tail solve (what the sim runs), exposed so the page's
/// tests can check it against `compute_r_star_exact` to the bit for any hint.
#[wasm_bindgen]
pub fn compute_r_star_warm(p_gb: f64, q_bg: f64, window_size: usize, delta: f64, hint_k: Option<usize>) -> f64 {
    warm_tail::warm_tail_solve(p_gb, q_bg, window_size, delta, hint_k).0
}

#[wasm_bindgen]
pub fn taper_density(t: f64, amplitude: f64, decay: f64) -> f64 {
    amplitude * decay.powf(t)
}

#[wasm_bindgen]
pub fn p_fec_normal(r: f64, epsilon: f64, window_size: f64, sigma2_burst: f64) -> f64 {
    math::p_fec_normal(r, epsilon, window_size, sigma2_burst)
}

#[wasm_bindgen]
pub fn compute_delta(epsilon: f64, r: f64, rho: f64, window_size: f64, sigma2_burst: f64) -> f64 {
    math::compute_delta(epsilon, r, rho, window_size, sigma2_burst)
}

#[wasm_bindgen]
pub fn find_t_cut(epsilon: f64, q: f64, r: f64, window_size: f64, sigma2_burst: f64, target_rho: f64) -> f64 {
    math::find_t_cut(epsilon, q, r, window_size, sigma2_burst, target_rho)
}

#[wasm_bindgen]
pub fn b_max(q: f64) -> u64 {
    math::b_max(q)
}

/// The shared production rate formula (raptorpath-math::controller_rate) —
/// the exact code path the real transport runs. Flat-args wrapper for JS.
/// `completion_exposure` is χ in [0,1] (§4.6); pass 0 for mid-stream /
/// unknown T_rem.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn controller_rate(
    p_upper: f64, sigma2: f64, mean_burst: f64, window: f64,
    t_symbols: f64, srtt: f64, t_sym: f64, codec_overhead: f64,
    tail_target: f64, bulk_late_is_fine: bool, completion_exposure: f64,
    saturation_cap: bool, max_overhead: f64,
) -> f64 {
    math::controller_rate(&math::RateInputs {
        p_upper, sigma2, mean_burst, window, t_symbols, srtt, t_sym,
        codec_overhead, tail_target, bulk_late_is_fine, completion_exposure,
        // Burst-tail provisioning (§4.3): the flat JS wrapper carries no
        // measured window-mass statistics; the tail term is inert without
        // them (identical to production cold start).
        mass: math::MassStats::default(),
        tail_provision: true,
        // §4.4: the visualizer models a raw transfer (file-transfer
        // semantics — no latency-feedback payload inside), so the
        // inner-feedback repair floor stays off (production defaults to 0
        // too).
        inner_feedback: 0.0,
        saturation_cap, max_overhead,
    })
}

/// Completion-exposure kernel chi(T_rem) (§4.6): the probability a
/// loss NOW can no longer hide behind ongoing sends.
#[wasm_bindgen]
pub fn completion_exposure(t_rem_secs: f64, srtt_secs: f64, rttvar_secs: f64) -> f64 {
    math::completion_exposure(t_rem_secs, srtt_secs, rttvar_secs)
}

// =========================================================================
// The unified span machine — the (δ, ρ, r) span law, ported verbatim from
// the paper (§5.3 sender span law, §5.4 horizon coefficient, §5.6 δ-honest
// shedding, §4.1 δ(hint) = 0.5/ζ; ADR-0064). These are the law formulas the
// visualizer's centerpiece animates; they are pure functions of measured
// anchors and carry no mode bit — Realtime and Bulk are the two ends of one
// continuum.
//
// Convention: `delta_price` is the hint's declared latency price
// δ(hint) = 0.5/ζ (§4.1): Realtime 50, Auto 0.5, Bulk 0.005. It is not
// the (δ, ρ, r) triangle's tail target (`tail target δ` below maps one to
// the other along the same continuum).
// =========================================================================

/// §4.1: δ(hint) = 0.5/ζ ⇔ ζ = 0.5/δ. ζ is the hint's tail-loss scale
/// {0.01 Realtime, 1 Auto, 100 Bulk} ⇒ δ ∈ {50, 0.5, 0.005}. Involution.
///
/// Re-export of `raptorpath-math::zeta_of_delta`, which the engine reads
/// too: the dial has one implementation for the L0 model and the engine.
#[wasm_bindgen]
pub fn zeta_of_delta(delta_price: f64) -> f64 {
    math::zeta_of_delta(delta_price)
}

/// §5.4: the horizon coefficient b of the span law's own deadline,
/// b = ½/1/2 at Realtime/Auto/Bulk, log-linear between them:
/// b(δ) = 2^(−log₁₀(2δ)/2), clamped to [½, 2] — exact at all three presets
/// (δ = 50 → ½, 0.5 → 1, 0.005 → 2).
///
/// Re-export of `raptorpath-math::span_horizon_b`, the same function the
/// engine reads through `net::delta_budget_b_of`: the engine and the
/// visualizer share one b.
#[wasm_bindgen]
pub fn span_horizon_b(delta_price: f64) -> f64 {
    math::span_horizon_b(delta_price)
}

/// §5.3/§5.6: the recovery deadline D(δ) = min(b(δ)·RTprop, 2·RTprop)
/// — past D, recovery belongs to ARQ/deficit anyway; a repair or retransmit
/// fired at age > D lands after the receiver's own δ-horizon give-up.
#[wasm_bindgen]
pub fn span_deadline_d(delta_price: f64, rtprop: f64) -> f64 {
    math::span_deadline_d(delta_price, rtprop)
}

/// §5.3: the coding quantum / span width A* = clamp(rate·D, 1, W) —
/// the recovery budget in symbols N_δ = rate·D, clamped into the window.
#[wasm_bindgen]
pub fn span_width_a_star(rate: f64, d: f64, w: f64) -> f64 {
    math::span_width_a_star(rate, d, w)
}

/// §5.3: pipeline depth M* = ceil(rate·2·RTprop / A*_q) + 1 (A*_q =
/// A* quantized to the retained grid), clamped to [2, 32] — the declared
/// cold-start floor / memory ceiling (GEN_PIPE_MAX_GENS). Bulk's derived
/// depth is the large-δ limit.
#[wasm_bindgen]
pub fn pipeline_depth_m_star(rate: f64, rtprop: f64, a_star_q: f64) -> f64 {
    math::pipeline_depth_m_star(rate, rtprop, a_star_q)
}

/// §5.3 / ADR-0064: the trailing offset Δ = clamp(⌈rate·J⌉, 1, 64)
/// (J = jitter anchor): the span must end Δ behind the send frontier so
/// every member has landed when the repair does — a repair is solvable at
/// arrival iff it trails the frontier by Δ (a leading-window emission is
/// not). Floor 1 = the FIFO-per-path minimum.
#[wasm_bindgen]
pub fn trailing_offset_delta(rate: f64, jitter: f64) -> f64 {
    math::trailing_offset_delta(rate, jitter)
}

/// §5.6: the residual R after in-window FEC at the live operating point:
///   R = ε̂ · (1 − P_fec(r_live, ε̂, A*, σ²_burst)).
/// It is one input of the give-up budget min(1 − ρ, R) (`giveup_budget`);
/// ρ is the declared dial, not derived from R. A hole is sheddable iff its
/// age exceeds D(δ) and cumulative shed stays within the budget;
/// past-budget candidates are served — ρ wins over δ.
#[wasm_bindgen]
pub fn shed_budget_residual(eps_hat: f64, r_live: f64, a_star: f64, sigma2_burst: f64) -> f64 {
    let e = eps_hat.clamp(0.0, 1.0);
    e * (1.0 - math::p_fec_normal(r_live.max(0.0), e, a_star.max(1.0), sigma2_burst))
}

/// The give-up budget, as a fraction of src_total (option (ii), docs/derivations/
/// giveup-law-model.md): budget = min(1 − ρ, R). ρ is the declared contract
/// dial, R the residual after in-window FEC (`shed_budget_residual`). At
/// ρ = 1 the budget is 0 at every δ: RETAIN is the ρ = 1 point of the dial,
/// reached with no branch. Both the sim and the span cartoon call this.
#[wasm_bindgen]
pub fn giveup_budget(rho: f64, residual: f64) -> f64 {
    (1.0 - rho).max(0.0).min(residual.max(0.0))
}

/// The visualizer's δ-continuum → r*-margin tail-target mapping: a
/// monotone log-log interpolation through three hint anchor points
/// (δ_price, tail target): Realtime (50, 1e-7), Auto (0.5, 1e-5),
/// Bulk (0.005, 0.05 — the late-is-fine class, BULK_TAIL_BUDGET of §4.5;
/// tail ≥ ε ⇒ the §4.2 continuous glide takes r* to 0 = pure ARQ, the
/// Bulk limit reached with no mode bit). §4.1 fixes only the presets;
/// the interpolation between them is the visualizer's documented choice,
/// and its Bulk anchor differs from the engine's t_tail (§11.3; bounded by
/// `test_sim_engine_tail_anchor_divergence_is_bounded`).
/// Bulkness β(δ) ∈ [0, 1] (§4.5): the log-position of the δ price between
/// the Auto (β = 0) and Bulk (β = 1) anchors. This is the continuum's only
/// shaping parameter: the sim's effective tail target cross-fades
/// (log-blend) from the interpolated anchor into §4.5's late-is-fine
/// form (target tracks p̂ with the §4.6 χ-glide) as β → 1 — one continuous
/// law, no hint branch, exact at all three presets.
///
/// Re-export of `raptorpath-math::bulkness_of_delta`, which the engine's
/// rate law reads as the mixing weight of
/// `r(β) = (1−β)·r_anchor + β·r_late-is-fine` (`control/fec_rate.rs`).
#[wasm_bindgen]
pub fn bulkness_of_delta(delta_price: f64) -> f64 {
    math::bulkness_of_delta(delta_price)
}

#[wasm_bindgen]
pub fn sim_tail_target_of_delta(delta_price: f64) -> f64 {
    math::sim_tail_target_of_delta(delta_price)
}

// =========================================================================
// Three-variable solvers — kept for the triangle explainer panel
// =========================================================================

#[wasm_bindgen]
pub fn solve_r_from_delta_rho(epsilon: f64, q: f64, window_size: f64, sigma2_burst: f64, delta: f64, rho: f64) -> Vec<f64> {
    let res = math::solve_r_from_delta_rho(epsilon, q, window_size, sigma2_burst, delta, rho);
    vec![res.r, res.delta, res.rho, res.t_cut, res.buffer_max]
}

#[wasm_bindgen]
pub fn solve_delta_from_r_rho(epsilon: f64, q: f64, window_size: f64, sigma2_burst: f64, r: f64, rho: f64) -> Vec<f64> {
    let res = math::solve_delta_from_r_rho(epsilon, q, window_size, sigma2_burst, r, rho);
    vec![res.r, res.delta, res.rho, res.t_cut, res.buffer_max]
}

#[wasm_bindgen]
pub fn solve_rho_from_r_delta(epsilon: f64, q: f64, window_size: f64, sigma2_burst: f64, r: f64, delta: f64) -> Vec<f64> {
    let res = math::solve_rho_from_r_delta(epsilon, q, window_size, sigma2_burst, r, delta);
    vec![res.r, res.delta, res.rho, res.t_cut, res.buffer_max]
}

// =========================================================================
// Simulation engine — runs the real algorithm:
//   - rate from the shared production formula (math::controller_rate):
//     continuous z_{delta/eps} margin, hint tail targets, Bulk pure-ARQ,
//     burst B/T term, saturation cap (§4.2 / §4.4 / §4.5)
//   - correction preemption over new source
//   - per-slot repair/retransmit mix by P_lost (§3.2, §3.4)
//   - honest feedback: the estimator learns outcomes one RTT after send,
//     fed with the true loss pattern (record_counts + record_symbol)
//   - completion-tail FEC: for Bulk, the continuous completion-exposure
//     ramp (§4.6 — the sim knows T_rem, so chi is fed per tick); for the
//     other hints, a source-position completion ramp over the final window
//     (§3.3 end-of-stream truncation; budget from the exact §4.7 DP), of
//     which the one-shot end-of-stream burst is the limiting case
//
// Multipath (§5.5, Reliable Windowed Multipath — L0 prototype):
// the Simulation carries N independent GE channels (own calibrated state
// array, own seed, own capacity, own RTT/feedback delay, own estimator)
// POURING INTO ONE shared reliable window (one RlcEncoder/RlcDecoder).
// Source symbols are striped across paths proportional to the ESTIMATED
// per-path goodput g_i = capacity_i * (1 - eps_hat_i) (estimated, not true —
// the sender cannot see the channel; smooth weighted round-robin);
// repairs/retransmits go preferentially to the highest-goodput path with a
// spare slot (a simplification of the §5.7 placement law), and a symbol lost on path i may
// be repaired via ANY path — the in-order frontier advances when the window
// prefix decodes from any combination of arrivals (§5.5: frontier
// rate -> sum of g_i). Controller inputs (p_upper, sigma2, SRTT, ...) are
// capacity-weighted aggregates of the per-path estimators: the window sees
// a mixture channel. N = 1 reduces exactly to the single-path simulation
// (bit-identical: same seed derivation, same RNG stream, same slot order).
//
// Laws modeled:
//   - per-path recovery clocks (wall #8, §7.1): RFC 9002 loss detection
//     generalized per path — a hole's retransmit clock is its OWN path's
//     RTT from send (no speculative pre-report firing); retransmits inherit
//     the clock of the path they ride; the phantom_held/phantom_avoided
//     counters show the spurious candidates a global clock would have fired
//   - a constant per-path outstanding pool (wall #7): the retention store is
//     STORE_PER_PATH × n_paths, an underived model constant (law ledger M01),
//     NOT the engine's §6.1 cap; a full pool stalls new source (the
//     Little's-law wall, visible when cap·RTT is cranked past it)
//   - SACK-clocked store release (wall #9, §6.3): slots free on the
//     selective ack (or on recovery of a loss), NOT on the cumulative
//     frontier — the frontier-clocked occupancy is kept as a live
//     counterfactual readout
//   - the δ continuum (§5.3/§5.6/§4.1): the span-law formulas
//     (A*, M*, D, Δ, shed budget) are exported above as pure functions;
//     the sim itself is driven along the continuum via the "continuum" hint
//     (custom_delta = the δ price; sim_tail_target_of_delta(δ) and the β
//     rate mix) — no mode bit anywhere
//
// Simplifications vs the L0 gate (documented): fixed wire capacity
// (slots/tick) instead of a queue+Copa model, no jitter (so no encoder lag
// needed — each path's channel is FIFO), delivery one-way latency uses the
// capacity-weighted mean SRTT, forward propagation is instantaneous (the
// one-way delay is added in the latency accounting, so cross-path reorder
// gaps exist only as losses).
// =========================================================================

const BASE_TAIL_TARGET: f64 = 1e-5;
const CODEC_OVERHEAD_RLC: f64 = 0.004;
const MAX_OVERHEAD: f64 = 0.5;
const TICK_SECS: f64 = 0.001;
/// Retention-store pool per path (wall #7): a CONSTANT, not the engine's
/// §6.1 law. 512/path is an underived model constant (law ledger M01): no
/// BDP or δ input. The engine's cap is clamp(law, floor, max(n_live·2048,
/// floor)), and 2048/path is a clamp measured pinned (ledger F01: the law's
/// value sat at it for 0.74–0.98 of c8 ticks), not a knee. Here the pool is
/// slack at the standard cells (cap·RTT ≤ ~400; stall fraction 0.000 at the
/// c8 preset) and binds only when the BDP is cranked past it (1 path, RTT
/// ≥ 200 at cap 8). The derived store law is unshipped and NOT modeled.
const STORE_PER_PATH: u32 = 512;
/// The reference deadline τ for comparing T = ∞ runs with finite-T ones
/// (three-knobs §13.6; plan Q-V3 default), seconds.
const REF_DEADLINE_S: f64 = 0.4;

/// The class of one handed-over unit (addendum 1 A1), decided in the model
/// by this precedence: expired at the intake (no seq); undelivered (passed by
/// the floor or given up, never delivered); out of order (delivered after a
/// higher-numbered unit); late (delivered, delay from hand-over > T); on
/// time; pending (not yet resolved). The page draws these codes and nothing
/// else.
pub const CLASS_PENDING: u8 = 0;
pub const CLASS_ON_TIME: u8 = 1;
pub const CLASS_LATE: u8 = 2;
pub const CLASS_OUT_OF_ORDER: u8 = 3;
pub const CLASS_UNDELIVERED: u8 = 4;
pub const CLASS_EXPIRED: u8 = 5;
/// Decoded at the receiver but still held at the consumer when the run
/// ended (behind a hole whose floor the receiver never got); never mixed with
/// `undelivered`. Reachable only with a forward delay.
pub const CLASS_HELD: u8 = 6;
const N_CLASSES: usize = 7;
/// A unit without a seq (not sent yet, or expired at the intake).
const NO_SEQ: u32 = u32::MAX;

/// What a datagram on the forward leg carries (addendum 1 A2).
enum DgKind {
    /// A source symbol's first copy; it holds a store slot until its SACK.
    Source,
    /// A resend of a lost source symbol.
    Resend,
    /// A repair with its coded data.
    Repair { start: u64, count: u16, index: u32, data: Vec<u8> },
    /// A floor-repeat datagram (no payload, no feedback entry).
    Floor,
}

/// One datagram in flight on a path's forward leg (addendum 1 A2). Its loss
/// outcome and its arrival time are decided at send; `deliver` does every
/// receiver-side effect at the arrival, keyed by seq.
struct Datagram {
    kind: DgKind,
    seq: u64,
    send_tick: u32,
    path: usize,
    /// F_ann at the send (the header carrier, read by the receiver at arrival).
    floor: u32,
    /// Lost on the wire: it still rides the leg as a ghost so its feedback
    /// entry is due at the arrival it would have had.
    lost: bool,
    arrive_s: f64,
}

/// Per-path state (§5.5): an independent GE channel with its own
/// capacity, RTT (feedback delay), estimator feed and traffic counters.
struct PathState {
    eps: f64,
    q: f64,
    capacity: u32, // wire symbols per tick on THIS path
    rtt_ticks: u32,
    srtt_secs: f64,
    rttvar_secs: f64,
    /// Pre-generated per-WIRE-SYMBOL channel states: true = lost.
    channel_states: Vec<bool>,
    wire_idx: usize,
    estimator: math::LossEstimator,
    /// Wire outcomes awaiting THIS path's ACK round-trip:
    /// (due tick, send tick, arrived, holds_store_slot). The due tick is the arrival
    /// (or would-be arrival) plus the reverse leg (addendum 1 A2). The third
    /// flag marks SOURCE sends, whose retention-store slot is released when
    /// this entry pops with arrived = true — the SACK-clocked release (§6.3):
    /// the slot frees on the selective ack, NOT on the cumulative frontier.
    feedback_queue: std::collections::VecDeque<(u32, u32, bool, bool)>,
    /// The forward leg (addendum 1 A2): datagrams in flight, FIFO.
    leg: std::collections::VecDeque<Datagram>,
    /// The reverse leg in ticks: feedback is due this long after an arrival,
    /// the first whole tick count ≥ rtt − d (addendum 1 A2).
    reverse_ticks: u32,
    /// The forward leg's service time, queue limit (both s) and propagation
    /// delay d (s, within rtt/2), and the instant its server is free.
    link_sv: f64,
    link_q: f64,
    link_d: f64,
    link_free: f64,
    /// Data datagrams the leg's queue dropped (no channel state, no loss
    /// draw), those of them sent inside the outage window, the server's busy
    /// time (s), floor-repeat datagrams sent, lost, dropped and arrived (kept
    /// apart: they are in no other counter), the one-way delay of every
    /// arrived data datagram (arrival − send + rtt/2 − d, ms) and the highest
    /// seq that arrived on this path.
    qdrop: u32,
    qdrop_in: u32,
    link_busy: f64,
    floor_sent: u32,
    floor_lost: u32,
    floor_dropped: u32,
    floor_arrived: u32,
    owd_ms: Vec<f64>,
    max_seq_arrived: Option<u64>,
    /// The RTT the sender believes (addendum 1 A3): RFC 6298 (α = 1/8,
    /// β = 1/4) in ticks, seeded at the base (srtt = rtt_ticks, rttvar =
    /// rtt_ticks/8), sampled at each feedback pop of an arrived source symbol
    /// (never resent: Karn) as pop tick − send tick. Without a queue every
    /// sample is rtt_ticks and srtt stays bit-equal to it.
    srtt_meas: f64,
    rttvar_meas: f64,
    rtt_samples: u32,
    /// Test-only: extra forward delay added to every arrival on this path,
    /// to drive the duplicate and late-fill rules (addendum 1 A2).
    #[cfg(test)]
    test_extra_s: f64,
    last_flush_tick: u32,
    pending_sent: u32,
    pending_ok: u32,
    // Per-path traffic counters.
    sent: u32,
    arrived: u32,
    src: u32,
    fec: u32,
    arq: u32,
    lost: u32,
    /// Smooth weighted-round-robin credit for goodput-proportional striping.
    stripe_credit: f64,
    /// Outage window [start, start + len) in seconds (three-knobs plan §4.1,
    /// Q-V7): every datagram sent inside it is lost. [∞, ∞) is empty.
    outage_start_s: f64,
    outage_len_s: f64,
}

impl PathState {
    /// ESTIMATED goodput g_i = capacity_i * (1 - eps_hat_i), the striping
    /// weight. Estimated, not true: the sender only has its RTT-delayed
    /// estimator (documented honesty choice).
    fn goodput_est(&self) -> f64 {
        self.capacity as f64 * (1.0 - self.estimator.loss_rate().clamp(0.0, 0.99))
    }
    /// TRUE goodput capacity_i * (1 - eps_i) — for the truth-side readouts
    /// and the aggregation-factor denominator only, never for decisions.
    fn goodput_true(&self) -> f64 {
        self.capacity as f64 * (1.0 - self.eps)
    }
    /// The retry timer, the estimator's flush period and `protected`'s
    /// cooldown, in ticks (addendum 1 A3): the believed srtt, rounded.
    fn retry_delay(&self) -> u32 {
        self.srtt_meas.round() as u32
    }
    /// The believed srtt in seconds: the configured base plus the measured
    /// excess, so exactly `srtt_secs` while srtt_meas = rtt_ticks (also at
    /// the rtt = 1 ms clamp, where rtt_ticks = 2).
    fn srtt_belief_s(&self) -> f64 {
        self.srtt_secs + (self.srtt_meas - self.rtt_ticks as f64) * TICK_SECS
    }
    /// One RFC 6298 sample, in ticks.
    fn rtt_sample(&mut self, r: f64) {
        self.rttvar_meas = 0.75 * self.rttvar_meas + 0.25 * (self.srtt_meas - r).abs();
        self.srtt_meas = 0.875 * self.srtt_meas + 0.125 * r;
        self.rtt_samples += 1;
    }
}

#[wasm_bindgen]
pub struct Simulation {
    paths: Vec<PathState>,
    /// Capacity-weighted true burst variance factor (aggregate).
    sigma2_true: f64,
    tail_target: f64,
    /// Continuum bulkness β = bulkness_of_delta(δ) ∈ [0, 1]: the rate is
    /// the mix (1−β)·r_anchor + β·r_late-is-fine (`rate_now`), and the
    /// `δ_eff` readout log-blends the tail the same way (`continuum_tail`).
    /// One law, continuous in δ, no mode bit.
    bulkness: f64,
    fixed_r: Option<f64>,
    /// Test-only ablation: revert the Bulk end to the earlier mapping
    /// (delta_eff = min(0.1, p_hat) + one-shot tail burst), to isolate the
    /// completion-exposure change. Compiled out of the shipped wasm.
    #[cfg(test)]
    legacy_bulk_delta: bool,
    /// Test-only ablation (§4.6): revert to the one-shot end-of-stream burst
    /// (disabling the continuous completion ramp), to isolate the
    /// taper-truncation remedy. Compiled out of the shipped wasm.
    #[cfg(test)]
    legacy_tail_burst: bool,
    /// Reliability target (rho): the give-up budget is min(1 − ρ, R)
    /// (§3.5, §5.6) — the third triangle corner.
    rho: f64,
    given_up: u32,
    /// The δ price (the dial), kept for the give-up deadline D(δ).
    dprice: f64,
    /// Test log of every give-up: (age at give-up in ticks, had a
    /// retransmit fired, given_up count after, budget·src_total then).
    #[cfg(test)]
    giveup_log: Vec<(u32, bool, u32, f64)>,
    /// Seqs the receiver has pruned: late data for them is
    /// discarded, not delivered.
    given_up_seqs: std::collections::BTreeSet<u64>,
    /// Send tick per source seq (for delivery-latency measurement).
    send_tick: Vec<u32>,
    /// Delivery latency (ms) per delivered source symbol, in delivery order:
    /// (recovery_tick - send_tick) + one-way propagation.
    delivered_lat_ms: Vec<f64>,
    /// Delivery latency (ms) indexed BY SOURCE SEQ (NaN until delivered).
    /// Used to compare last-window vs mid-stream tail latency (§3.3: the
    /// end-of-stream reliability cliff must vanish).
    lat_by_seq: Vec<f64>,
    /// RFC 3550-style smoothed jitter over successive delivery latencies:
    /// J += (|D| - J) / 16.
    jitter_ms: f64,
    prev_lat_ms: f64,

    w: u32, // encoder window (shared across paths — ONE reliable window)

    tick: u32,
    source_done: bool,
    tail_flushed: bool,
    finished: bool,
    num_source: u32,

    encoder: math::RlcEncoder,
    decoder: math::RlcDecoder,

    symbols: Vec<Symbol>,
    /// Source payloads by seq — retransmission source, independent of the
    /// encoder window (a retransmit long after eviction must still work).
    source_store: Vec<Vec<u8>>,
    next_seq: u32,

    // Rate state
    rate: f64,
    debt: f64,
    /// Last completion-exposure chi (§4.6): the Stieltjes metering of
    /// the completion-tail budget accrues B_tail x (chi - prev_chi).
    prev_completion_chi: f64,
    /// One-entry memo of the tail-rate solve, keyed on the exact bits of
    /// (pg, qg, W, tail_target): the aggregate GE estimate only moves per
    /// tick, so the several source symbols of a tick share one solve.
    /// Pure: the same inputs always give the same r_tail.
    r_tail_cache: Option<([u64; 4], f64)>,
    /// Crossing repair count k* of the last tail solve: the warm start of
    /// the next one (None at start).
    prev_kstar: Option<usize>,
    /// Test-only: total distinct fail(k) DP evaluations over all solves.
    #[cfg(test)]
    r_tail_dp_runs: usize,
    #[cfg(test)]
    r_tail_log: Vec<[u64; 4]>,
    #[cfg(test)]
    r_tail_solves: u32,
    #[cfg(test)]
    verify_r_tail_cache: bool,

    // Counters
    total_src: u32, total_fec: u32, total_arq: u32, total_lost: u32,
    cum_sent: u32, cum_arrived: u32, cum_decoded: u32,
    lost_pending: u32,
    last_src: u32, last_fec: u32, last_arq: u32, last_lost: u32,

    // --- Retention-store model (walls #7/#9; §6.1, §6.3) ---
    /// Slots held by source symbols: taken at send, released on SACK
    /// (arrival ack pops one path-RTT later) or on recovery/give-up of a
    /// lost symbol. This is the SACK-clocked law (§6.3, the shipped
    /// default): SACKed-but-not-cumulative symbols do not hold slots.
    store_occupancy: u32,
    /// Source slots skipped because the path-scaled pool was full
    /// (STORE_PER_PATH × n_paths) — the wall-#7 Little's-law stall,
    /// visible when cap·RTT is cranked past the pool.
    pool_stalls: u32,
    /// Receiver's contiguous delivered/given-up prefix (next undelivered
    /// source seq) — the cumulative frontier.
    frontier: u32,
    /// The frontier as the SENDER knows it (one aggregate RTT late):
    /// what a frontier-clocked store (wall #9) would release against.
    /// occupancy_frontier = next_seq − frontier_acked is the
    /// counterfactual the §6.3 gauge compares against.
    frontier_acked: u32,
    frontier_hist: std::collections::VecDeque<(u32, u32)>,

    // --- Per-path recovery clocks (wall #8; §7.1, RFC 9002 per path) ---
    /// Holes currently held by their OWN path's RTT clock although the
    /// aggregate (global) clock has already expired — the retransmits a
    /// global-clock recovery plane would have fired spuriously.
    phantom_held: u32,
    /// Cumulative count of such crossings (one per hole per retx cycle):
    /// phantom retransmits avoided by the per-path law.
    phantom_avoided: u32,

    rng_state: u64,

    // --- Three knobs (proposal; docs/derivations/visualizer-three-knobs-plan.md
    // §3.1, §7). Every field below is inert at T = ∞, H = ∞, λ = ∞ by IEEE
    // arithmetic; the laws live in `knobs.rs`. ---
    /// Lifetime T, seconds (∞ = never give up).
    lifetime_s: f64,
    /// Order hold H, seconds (0 = any order, ∞ = strict).
    hold_s: f64,
    /// Application hand-over rate λ, units per second (∞ = the object:
    /// everything handed over at t = 0). Unit u is handed over at u / λ.
    source_rate: f64,
    /// Next unit to leave the intake (sent or expired). Units expired at
    /// the intake never get a seq, so this runs ahead of `next_seq`.
    u_next: u32,
    /// Units dropped at the intake by L1 (age ≥ T) and by the dequeue rule
    /// (§13.2: age + R ≥ T while a younger unit waits).
    intake_expired: u32,
    intake_expired_deq: u32,
    /// Per-seq sender state (indexed by seq; `symbols` is pruned): the
    /// hand-over time (s), the last send tick of any copy, the path of the
    /// last copy, and whether s was ever resent.
    h_seq: Vec<f64>,
    last_send_seq: Vec<u32>,
    path_seq: Vec<usize>,
    resent_seq: Vec<bool>,
    /// `acked_seq[s]`: the sender knows the receiver holds s or has passed
    /// it. Set at exactly five points (plan §7, sender-state rules): (1) the
    /// ack of an arrived source after its own path's RTT; (2) a resend's
    /// arrival; (3) a decode recovery; (4) a ρ give-up; (5) the receiver's
    /// floor pass of s plus the RTT of the path of s's last copy.
    acked_seq: Vec<bool>,
    /// The sender's ack frontier A_known: the first s with !acked_seq[s].
    a_known: u32,
    /// The floor F_ann (§17.1/§15.3): the lowest seq the sender still means
    /// to deliver, a running maximum.
    f_ann: u32,
    /// The floor the receiver holds (carried free on every arrival; a floor
    /// repeat datagram on the sweep clock while F_ann > A_known).
    f_rx: u32,
    /// Lost holes below F_ann, resolved at the sender by the commitment and
    /// not (yet) decoded. A later decode moves the unit to `cum_decoded`.
    committed_seq: Vec<bool>,
    /// Per seq: ever committed (never cleared; `committed_seq` clears at a
    /// later decode). The unit class reads it: a passed unit stays
    /// `undelivered` until the consumer delivers it.
    passed_seq: Vec<bool>,
    passed_undelivered: u32,
    /// A committed hole keeps its store slot until feedback (set-point 3 or
    /// 5) releases it: the brake (§15.4).
    slot_owed: Vec<bool>,
    /// Set-point 1 and 5 clocks, per path: (due tick, seq).
    ack_due: Vec<std::collections::VecDeque<(u32, u32)>>,
    floor_ack_due: Vec<std::collections::VecDeque<(u32, u32)>>,
    last_sweep_tick: u32,
    /// Readouts: copies staged below the floor (must be 0), candidates the
    /// re-check before a copy skipped, payload datagrams with age ≥ T (must
    /// be 0), floor-repeat datagrams, arrivals that raised the receiver's
    /// floor, and repairs whose window the L1 age edge cut.
    commit_viol: u32,
    skip_below_floor: u32,
    l1_viol: u32,
    floors: u32,
    floor_carried: u32,
    span_cut_pending: bool,
    span_cut_repairs: u32,
    /// The receiver's consumer stage under the hold H (L6; output only) and
    /// the seqs the decoder released this tick, its input.
    consumer: consumer::Consumer,
    decoded_this_tick: Vec<u64>,
    /// The unit index of each seq (units expired at the intake have none).
    seq_unit: Vec<u32>,
    /// Per unit: its seq (NO_SEQ until sent; forever for an intake expiry),
    /// the class last handed out by `take_unit_events`, and the units whose
    /// class may have changed since then (addendum 1 A1).
    unit_seq: Vec<u32>,
    unit_emitted: Vec<u8>,
    unit_dirty: Vec<u32>,
    /// Copies that reached a receiver already holding their seq (addendum 1
    /// A2): counted, with no decode work, slot release or class change.
    dup: u32,
    /// The lead the sender takes by learning a resend's arrival at the
    /// arrival tick rather than one reverse leg later (addendum 1 A2 gauge):
    /// count, sum and maximum, in ticks.
    lead_n: u32,
    lead_sum: u64,
    lead_max: u32,
    /// Arrivals below a seq that already arrived on another path (A4).
    cross_reorder: u32,
    /// Ticks the receiver-side drain ran after the sender finished.
    drain_ticks: u32,
    /// Each path's backlog (ms of queued work) at the end of every tick,
    /// tick-major, since the last `take_queue_samples` (A4; the page's strip).
    queue_pending: Vec<f64>,
    /// Test-only: sends that left a backlog above the queue limit plus one
    /// service time (must stay 0).
    #[cfg(test)]
    backlog_over: u32,
    /// Resends that arrived, and those of them later than T (L2's outcome).
    resends_arrived: u32,
    l2_late: u32,
    /// Test-only: every resend at its send (seq, path, send s, arrival s or
    /// NaN if lost or dropped, ô then, hand-over s, ô of its path then).
    #[cfg(test)]
    resend_log: Vec<(u64, usize, f64, f64, f64, f64, f64)>,
    /// The resend budget per 5-tick window (addendum 1 A5; ∞ = unlimited),
    /// the resends each window carried, the distinct seqs held back at least
    /// once, and the refusals (a seq refused at several slots counts each).
    resend_limit: f64,
    budget_used: Vec<u32>,
    braked_seq: Vec<bool>,
    braked_units: u32,
    braked_attempts: u32,
    /// Test-only: every delivered datagram (send tick, deliver tick, path,
    /// kind 0 source / 1 resend / 2 repair / 3 floor, lost), each resend's
    /// lead (seq, path, ticks), every decoder feed in order (seq or window
    /// start, u16::MAX for a source or the window count, repair index,
    /// data), and sends that found the leg's server busy past the send.
    #[cfg(test)]
    leg_log: Vec<(u32, u32, usize, u8, bool, u64, f64)>,
    #[cfg(test)]
    lead_log: Vec<(u64, usize, u32)>,
    #[cfg(test)]
    feed_log: Vec<(u64, u16, u32, Vec<u8>)>,
    #[cfg(test)]
    free_after_now: u32,
    /// Test-only: the decode tick of every delivered seq.
    #[cfg(test)]
    decode_log: Vec<(u64, u32)>,
    /// Test-only: commitments (tick, seq) and committed-slot releases
    /// (tick, seq, 3 = decode | 5 = floor pass + RTT).
    #[cfg(test)]
    commit_log: Vec<(u32, u64)>,
    #[cfg(test)]
    slot_release_log: Vec<(u32, u64, u8)>,
    /// Test-only staging log of every payload-bearing datagram: (tick,
    /// kind 0 = source / 1 = resend / 2 = repair-window member, seq, age s).
    #[cfg(test)]
    payload_log: Vec<(u32, u8, u64, f64)>,
}

struct Symbol {
    tick: u32,
    seq: u64,
    lost: bool,
    recovered: bool,
    last_retx_tick: i64,
    /// The tick its arrival's ack lands (arrival + reverse leg); u32::MAX
    /// until it arrives. Drives only the pruning of arrived symbols.
    ack_at: u32,
    /// Path the symbol was LAST sent on (original send, then updated per
    /// retransmit): its RTT gates the retry timer and the SACK ack, its
    /// estimator feeds the P_lost retransmit decision.
    path: usize,
}

/// The float guard of an arrival's dueness (s): far below a tick, far above
/// the rounding of a sum of seconds.
const DUE_GUARD_S: f64 = 1e-9;

/// The reverse leg in whole ticks: the first k with k·TICK ≥ rtt − d, so
/// exactly `rtt_ticks` at d = 0 (no division by TICK, which can be an ulp off).
fn reverse_ticks(rtt_ticks: u32, d: f64) -> u32 {
    let target = rtt_ticks as f64 * TICK_SECS - d;
    let mut k = rtt_ticks;
    while k > 0 && (k - 1) as f64 * TICK_SECS >= target {
        k -= 1;
    }
    k
}

fn xorshift64(state: &mut u64) -> f64 {
    let mut x = *state;
    x ^= x << 13; x ^= x >> 7; x ^= x << 17;
    *state = x;
    (x as f64) / (u64::MAX as f64)
}

/// Pre-generate channel states using the GE model (h_B = 1: Bad = lost),
/// then calibrate to the exact target epsilon for reproducible visuals.
fn generate_channel_states(p: f64, q: f64, target_eps: f64, n: usize, seed: u64) -> Vec<bool> {
    let mut rng = seed;
    let mut good = true;
    let mut states: Vec<bool> = (0..n).map(|_| {
        if good {
            if xorshift64(&mut rng) < p { good = false; }
        } else if xorshift64(&mut rng) < q {
            good = true;
        }
        !good
    }).collect();

    let actual = states.iter().filter(|&&l| l).count();
    let target = (target_eps * n as f64).round() as usize;
    if actual < target {
        let mut cands: Vec<(usize, u32)> = (0..n).filter(|&i| !states[i]).map(|i| {
            let adj = (i > 0 && states[i - 1]) as u32 + (i + 1 < n && states[i + 1]) as u32;
            (i, adj * 1000 + (xorshift64(&mut rng) * 999.0) as u32)
        }).collect();
        cands.sort_by(|a, b| b.1.cmp(&a.1));
        for &(i, _) in cands.iter().take(target - actual) { states[i] = true; }
    } else if actual > target {
        let mut cands: Vec<(usize, u32)> = (0..n).filter(|&i| states[i]).map(|i| {
            let adj = (i > 0 && states[i - 1]) as u32 + (i + 1 < n && states[i + 1]) as u32;
            (i, adj * 1000 + (xorshift64(&mut rng) * 999.0) as u32)
        }).collect();
        cands.sort_by(|a, b| a.1.cmp(&b.1));
        for &(i, _) in cands.iter().take(actual - target) { states[i] = false; }
    }
    states
}

impl Simulation {
    fn rng(&mut self) -> f64 {
        xorshift64(&mut self.rng_state)
    }

    /// Consume the next wire-slot channel outcome on path `pi` for a datagram
    /// departing at `at` (s; the outage is tested there, addendum 1 A2).
    /// true = lost.
    fn wire_lost(&mut self, pi: usize, at: f64) -> bool {
        let now = at;
        let p = &mut self.paths[pi];
        let lost = p.channel_states.get(p.wire_idx).copied().unwrap_or(false);
        p.wire_idx += 1;
        // The outage still consumes the channel state, so the pattern after
        // it stays aligned with the run without one.
        let in_outage = now >= p.outage_start_s && now < p.outage_start_s + p.outage_len_s;
        lost || in_outage
    }

    /// The current tick in seconds (the knobs are seconds; ticks convert
    /// to seconds, never the knobs to ticks).
    fn now_s(&self) -> f64 {
        self.tick as f64 * TICK_SECS
    }

    /// Hand-over time of unit u: u / λ (0 for every unit at λ = ∞).
    fn handover_s(&self, u: u32) -> f64 {
        u as f64 / self.source_rate
    }

    /// The ack timeout R (§13; RFC 6298 with g = ack delay = one tick, as
    /// in the toy's tick), from the measured srtt and, for now, the FIXED
    /// rttvar. Addendum 1 A3 has R read the measured rttvar; that switch is
    /// held for the coordinator: the measured rttvar decays to 0 without a
    /// queue, R falls from 76 to 52 ms at RTT 50 ms, the §13.2 dequeue stops
    /// firing near T = 0.4 and the T-continuity gate fails on its binding
    /// cell. `ack_timeout_meas_s` is the A3 form, reported.
    fn ack_timeout_s(&self) -> f64 {
        knobs::ack_timeout(self.agg_srtt(), self.agg_rttvar(), TICK_SECS, TICK_SECS)
    }
    /// R as addendum 1 A3 states it (measured srtt and measured rttvar): a
    /// readout only while the switch is held.
    fn ack_timeout_meas_s(&self) -> f64 {
        knobs::ack_timeout(self.agg_srtt(), self.agg_rttvar_meas(), TICK_SECS, TICK_SECS)
    }

    /// ô, the one-way term of L2 and `protected` (§17.1: srtt − RTprop/2):
    /// the measured aggregate srtt minus the configured aggregate RTprop/2,
    /// the resend path pinned as the capacity-weighted aggregate (addendum 1
    /// A3). Without a queue srtt = RTprop and ô is the RTT/2 the delay
    /// accounting adds. Read only by L2 and `protected`.
    fn one_way_s(&self) -> f64 {
        knobs::one_way_fwd(self.agg_srtt(), self.agg_srtt_base())
    }

    /// `protected(s)` (§17.1) with the model's bindings: the cooldown c and
    /// the never-resent trigger are both the retry timer, the RTT of the
    /// path of s's last copy (the model's oracle loss notice arrives then);
    /// no data sweep reaches s (P_reach = ∞).
    fn seq_protected(&self, i: usize, now: f64, o_hat: f64) -> bool {
        let c = self.paths[self.path_seq[i]].retry_delay() as f64 * TICK_SECS;
        let d = knobs::trigger_delay(self.resent_seq[i], c, c, f64::INFINITY);
        let last = self.last_send_seq[i] as f64 * TICK_SECS;
        knobs::protected(self.lifetime_s, now, last, d, self.h_seq[i], o_hat)
    }

    /// Recompute A_known and the floor F_ann (plan §7): scan up from
    /// max(F_ann, A_known) past every acked or unprotected seq, stop at the
    /// first unacked protected seq or at next_seq. At T = ∞ every seq is
    /// protected, so F_ann = A_known and nothing below it is unresolved.
    fn update_floor(&mut self) {
        let n = self.next_seq;
        while self.a_known < n && self.acked_seq[self.a_known as usize] {
            self.a_known += 1;
        }
        let now = self.now_s();
        let o_hat = self.one_way_s();
        let mut s = self.f_ann.max(self.a_known);
        while s < n {
            let i = s as usize;
            if !self.acked_seq[i] && self.seq_protected(i, now, o_hat) {
                break;
            }
            s += 1;
        }
        if s > self.f_ann {
            self.f_ann = s;
            self.commit_below_floor();
        }
    }

    /// The commitment (plan §7): a lost, unrecovered hole below F_ann is
    /// never resent; it is resolved at the sender (out of `lost_pending`,
    /// into `passed_undelivered`). Its store slot stays held until feedback.
    fn commit_below_floor(&mut self) {
        let f = self.f_ann as u64;
        for sym in &mut self.symbols {
            if sym.lost && !sym.recovered && sym.seq < f {
                sym.recovered = true;
                self.lost_pending = self.lost_pending.saturating_sub(1);
                self.passed_undelivered += 1;
                self.committed_seq[sym.seq as usize] = true;
                self.passed_seq[sym.seq as usize] = true;
                self.slot_owed[sym.seq as usize] = true;
                self.unit_dirty.push(self.seq_unit[sym.seq as usize]);
                #[cfg(test)]
                self.commit_log.push((self.tick, sym.seq));
            }
        }
    }

    /// Mark s acked; a committed hole's store slot is released here, by
    /// feedback only (cause 3 = decode, 5 = floor pass + RTT).
    fn ack_seq(&mut self, s: usize, _cause: u8) {
        self.acked_seq[s] = true;
        if self.slot_owed[s] {
            self.slot_owed[s] = false;
            self.store_occupancy = self.store_occupancy.saturating_sub(1);
            #[cfg(test)]
            self.slot_release_log.push((self.tick, s as u64, _cause));
        }
    }

    /// The receiver learns floor f. Every undelivered, unacked seq it now
    /// passes is acked one RTT (of its last copy's path) later: set-point 5.
    fn receive_floor(&mut self, f: u32) {
        if f <= self.f_rx {
            return;
        }
        for s in self.f_rx..f {
            let i = s as usize;
            if !self.acked_seq[i] && self.lat_by_seq[i].is_nan() {
                let p = self.path_seq[i];
                self.floor_ack_due[p].push_back((self.tick + self.paths[p].reverse_ticks, s));
            }
        }
        self.f_rx = f;
    }

    /// Header carrier (Q-V6 default): every arriving datagram carries the
    /// F_ann stamped at its send.
    fn carry_floor(&mut self, f: u32) {
        if f > self.f_rx {
            self.floor_carried += 1;
            self.receive_floor(f);
        }
    }

    /// The re-check immediately before a copy is staged (§17.1): a candidate
    /// below the current floor is skipped and counted.
    fn floor_recheck(&mut self, i: usize) -> bool {
        if self.symbols[i].seq < self.f_ann as u64 {
            self.skip_below_floor += 1;
            return false;
        }
        true
    }

    /// The resend budget (addendum 1 A5), at the point of sending: a refusal
    /// leaves the candidate as a failed P_lost draw does (no `last_retx`
    /// change; the next slot that picks it tries again).
    fn budget_admits(&mut self, i: usize) -> bool {
        let w = budget::window(self.tick);
        if self.budget_used.len() <= w {
            self.budget_used.resize(w + 1, 0);
        }
        if budget::admits(self.budget_used[w], self.resend_limit) {
            self.budget_used[w] += 1;
            return true;
        }
        self.braked_attempts += 1;
        let s = self.symbols[i].seq as usize;
        if self.braked_seq.len() <= s {
            self.braked_seq.resize(s + 1, false);
        }
        if !self.braked_seq[s] {
            self.braked_seq[s] = true;
            self.braked_units += 1;
        }
        false
    }

    /// The L1 age edge of the repair window (plan §7 P3): no window member
    /// with a ≥ T. Ages fall with seq, so the edge is a window start. True
    /// when it cut a member.
    fn apply_age_edge(&mut self) -> bool {
        let next = self.encoder.next_seq();
        let start = next - self.encoder.window_size() as u64;
        let now = self.now_s();
        let mut s = start;
        while s < next && knobs::intake_expired(self.lifetime_s, now - self.h_seq[s as usize]) {
            s += 1;
        }
        if s > start {
            self.encoder.advance(s);
        }
        s > start
    }

    /// Count a payload datagram for seq s that left with age ≥ T (must stay 0).
    fn check_l1(&mut self, s: usize, _kind: u8) {
        let age = self.now_s() - self.h_seq[s];
        if knobs::intake_expired(self.lifetime_s, age) {
            self.l1_viol += 1;
        }
        #[cfg(test)]
        self.payload_log.push((self.tick, _kind, s as u64, age));
    }

    /// Per unit: consumer delivery time − hand-over + the one-way accounting
    /// term ô (s); ∞ for a unit not delivered (expired, passed, or pending).
    fn unit_delays(&self) -> Vec<f64> {
        let mut d = vec![f64::INFINITY; self.num_source as usize];
        let o = self.acc_s();
        for (s, &u) in self.seq_unit.iter().enumerate() {
            let t = self.consumer.delivered_at(s);
            if !t.is_nan() {
                d[u as usize] = t - self.h_seq[s] + o;
            }
        }
        d
    }

    /// The share of all units delivered within `deadline` of hand-over (L3).
    fn share_within(&self, deadline: f64) -> f64 {
        // Delivered units only: an undelivered unit's delay is ∞, and
        // within_lifetime(∞, ∞) is true, so the delay alone cannot say it.
        let o = self.acc_s();
        let n = (0..self.seq_unit.len())
            .filter(|&s| {
                let t = self.consumer.delivered_at(s);
                !t.is_nan() && knobs::within_lifetime(deadline, t - self.h_seq[s] + o)
            })
            .count();
        n as f64 / self.num_source as f64
    }

    /// The class of unit u and its delay from hand-over (s; ∞ when not
    /// delivered), by A1's precedence. Late is L3's readout `within_lifetime`.
    fn unit_class(&self, u: usize) -> (u8, f64) {
        let s = self.unit_seq[u];
        if s == NO_SEQ {
            let expired = (u as u32) < self.u_next;
            return ([CLASS_PENDING, CLASS_EXPIRED][expired as usize], f64::INFINITY);
        }
        let s = s as usize;
        let t = self.consumer.delivered_at(s);
        if t.is_nan() {
            if self.finished && self.receiver_holds(s as u64) {
                return (CLASS_HELD, f64::INFINITY);
            }
            let passed = self.passed_seq[s] || self.given_up_seqs.contains(&(s as u64));
            return ([CLASS_PENDING, CLASS_UNDELIVERED][passed as usize], f64::INFINITY);
        }
        let delay = t - self.h_seq[s] + self.acc_s();
        let class = if self.consumer.out_of_order(s) {
            CLASS_OUT_OF_ORDER
        } else if knobs::within_lifetime(self.lifetime_s, delay) {
            CLASS_ON_TIME
        } else {
            CLASS_LATE
        };
        (class, delay)
    }

    /// The intake (L1, then the §13.2 dequeue rule, oldest first): drops
    /// the head while it has outlived T, or while it can no longer arrive
    /// in time and a younger unit waits. True when the head unit is handed
    /// over and may be sent now. Ends the source when the intake drains.
    fn intake_head_ready(&mut self) -> bool {
        let now = self.now_s();
        let r = self.ack_timeout_s();
        loop {
            if self.u_next >= self.num_source {
                self.source_done = true;
                return false;
            }
            let h = self.handover_s(self.u_next);
            if h > now {
                return false; // not handed over yet
            }
            let age = now - h;
            if knobs::intake_expired(self.lifetime_s, age) {
                self.intake_expired += 1;
                self.unit_dirty.push(self.u_next);
                self.u_next += 1;
                continue;
            }
            let younger_waiting =
                self.u_next + 1 < self.num_source && self.handover_s(self.u_next + 1) <= now;
            if younger_waiting && knobs::dequeue_expired(self.lifetime_s, age, r) {
                self.intake_expired_deq += 1;
                self.unit_dirty.push(self.u_next);
                self.u_next += 1;
                continue;
            }
            return true;
        }
    }

    // --- Capacity-weighted aggregates over paths (§5.5: the shared
    // window sees a mixture channel; each path contributes in proportion to
    // its share of the wire). For N = 1 every aggregate reduces EXACTLY to
    // the single path's value (weight = 1.0, x * 1.0 == x in IEEE754).
    fn total_capacity(&self) -> u32 {
        self.paths.iter().map(|p| p.capacity).sum()
    }
    fn agg<F: Fn(&PathState) -> f64>(&self, f: F) -> f64 {
        let tot: f64 = self.paths.iter().map(|p| p.capacity as f64).sum();
        self.paths.iter().map(|p| p.capacity as f64 / tot * f(p)).sum()
    }
    fn agg_eps(&self) -> f64 {
        self.agg(|p| p.eps)
    }
    /// The believed srtt (s), capacity-weighted (addendum 1 A3).
    fn agg_srtt(&self) -> f64 {
        self.agg(|p| p.srtt_belief_s())
    }
    /// The configured base RTT, RTprop (s), capacity-weighted.
    fn agg_srtt_base(&self) -> f64 {
        self.agg(|p| p.srtt_secs)
    }
    /// The fixed rttvar main's laws read (P_lost, completion): rtt/8, a
    /// declared model constant (addendum 1 A3; Q-V10 bounds it).
    fn agg_rttvar(&self) -> f64 {
        self.agg(|p| p.rttvar_secs)
    }
    /// The measured RFC rttvar (s), which R reads.
    fn agg_rttvar_meas(&self) -> f64 {
        self.agg(|p| p.rttvar_meas * TICK_SECS)
    }
    /// The believed srtt in ticks, rounded (the aggregate clocks).
    fn agg_rtt_ticks(&self) -> u32 {
        self.agg(|p| p.srtt_meas).round() as u32
    }
    /// The delay accounting's one physical term (addendum 1 A2): the part of
    /// the one-way delay the forward leg does not model, rtt/2 − d per path,
    /// from the configured values; never the belief (the arrival tick
    /// already holds any queueing).
    fn acc_s(&self) -> f64 {
        self.agg(|p| p.srtt_secs / 2.0 - p.link_d)
    }
    fn agg_loss_est(&self) -> f64 {
        self.agg(|p| p.estimator.loss_rate())
    }
    fn agg_p_upper(&self) -> f64 {
        self.agg(|p| p.estimator.predictive_loss_upper(0.95))
    }

    /// Highest-estimated-goodput path with a spare slot this tick: repairs
    /// and retransmits prefer the best path (a simplification of the §5.7
    /// placement law — corrections ride the path most likely to deliver
    /// them).
    fn best_correction_path(&self, free: &[u32]) -> usize {
        let mut best = 0usize;
        let mut bg = f64::NEG_INFINITY;
        for (i, p) in self.paths.iter().enumerate() {
            if free[i] == 0 {
                continue;
            }
            let g = p.goodput_est();
            if g > bg {
                bg = g;
                best = i;
            }
        }
        best
    }

    /// Goodput-proportional striping for SOURCE symbols: smooth weighted
    /// round-robin over the paths with spare slots, weights g_i / sum g
    /// (estimated goodput). N = 1 degenerates to "always path 0" with zero
    /// credit drift.
    fn next_source_path(&mut self, free: &[u32]) -> usize {
        let g: Vec<f64> = self
            .paths
            .iter()
            .enumerate()
            .map(|(i, p)| if free[i] > 0 { p.goodput_est().max(1e-9) } else { 0.0 })
            .collect();
        let sum: f64 = g.iter().sum();
        let mut best = 0usize;
        let mut best_c = f64::NEG_INFINITY;
        for i in 0..self.paths.len() {
            if free[i] == 0 {
                continue;
            }
            self.paths[i].stripe_credit += g[i] / sum;
            if self.paths[i].stripe_credit > best_c {
                best_c = self.paths[i].stripe_credit;
                best = i;
            }
        }
        self.paths[best].stripe_credit -= 1.0;
        best
    }

    /// Completion exposure chi (§4.6): the sim knows the transfer
    /// length, so T_rem = remaining source symbols / send rate. The send
    /// rate is approximated by the wire capacity (Bulk mid-stream is
    /// nearly all source). Computed for EVERY hint — mid-stream it is 0, and
    /// over the final ~1.5 SRTT it ramps to 1 (Phi_bar of the RTT tail).
    fn completion_chi_raw(&self) -> f64 {
        let remaining = self.num_source.saturating_sub(self.u_next) as f64;
        let t_rem_secs = remaining / self.total_capacity() as f64 * TICK_SECS;
        math::completion_exposure(t_rem_secs, self.agg_srtt(), self.agg_rttvar())
    }

    /// Completion-tail debt increment for non-Bulk hints (the
    /// taper-truncation completion term, §3.3 / §4.6). Near a known
    /// end-of-stream the final window's symbols never receive their future
    /// repairs — the taper integral is truncated (§3.3) — so their
    /// late-window coverage is cut and tail losses fall to serial ARQ.
    ///
    /// The remedy delivers the same budget as the one-shot burst,
    /// B_tail = r_tail x W repairs (r_tail = the exact-DP rate meeting
    /// delta_hint on the final window, §4.7), but metered out
    /// continuously as a Stieltjes measure over a completion kernel chi_trunc:
    /// each source symbol accrues B_tail x d(chi_trunc). Since chi_trunc rises
    /// monotonically 0 -> 1 as the window empties, the total accrued is
    /// exactly B_tail — one window's worth, released continuously instead of
    /// dumped at a single instant.
    ///
    /// The kernel is over SOURCE POSITION, not wall time: the taper
    /// truncation is a source-position phenomenon (only the last W symbols
    /// lose future coverage — a symbol at distance j < W from the end misses
    /// repairs from W - j future positions), whereas Bulk's completion
    /// exposure chi(T_rem) is a wall-time economics kernel. Concentrating the
    /// budget on exactly the truncated region avoids diluting the final
    /// window (which a wide wall-time spread would do) while still ramping
    /// smoothly (no kink):
    ///
    ///   chi_trunc(remaining) = Phi_bar( (remaining - W/2) / (W/4) )
    ///
    /// Mid-stream (remaining >> W) chi_trunc = 0 (no completion FEC, exactly
    /// as chi = 0); over the final window it rises to ~1. As W/4 -> 0 the
    /// kernel becomes a step at remaining = W/2 and the whole budget releases
    /// at once: the one-shot burst is the limiting case.
    ///
    /// The Bulk end uses its delta glide instead: the (1 − β) weight below
    /// is 0 there. Fixed-r and the test-only ablations keep the one-shot
    /// burst (return 0).
    fn completion_debt_increment(&mut self) -> f64 {
        #[cfg(test)]
        {
            if self.legacy_tail_burst {
                return 0.0;
            }
        }
        if self.fixed_r.is_some() {
            return 0.0;
        }
        let remaining = self.num_source.saturating_sub(self.u_next) as f64;
        let w = self.w as f64;
        let chi_trunc = math::normal_survival((remaining - 0.5 * w) / (0.25 * w));
        let dchi = (chi_trunc - self.prev_completion_chi).max(0.0);
        self.prev_completion_chi = chi_trunc;
        if dchi <= 0.0 {
            return 0.0;
        }
        let (pg, qg) = self.agg_ge_params();
        let r_tail = self.r_tail_exact(pg, qg);
        // On the continuum the two completion mechanisms cross-fade
        // continuously: the source-position ramp (the FEC side's completion
        // law) scales by (1 − β) as the §4.6 χ-glide — which lives inside
        // the blended tail target — takes over toward the Bulk end. At
        // β = 1 the glide alone covers the tail (no double-pay); at β = 0
        // the ramp is whole.
        r_tail * w * dchi * (1.0 - self.bulkness)
    }

    /// `compute_r_star_exact(pg, qg, W, tail_target)` (solved warm-started
    /// from the previous k*, bit-identical), memoised on the exact
    /// input bits (no tolerance, no β-keyed shortcut: every call that
    /// differs in any input bit solves afresh).
    fn r_tail_exact(&mut self, pg: f64, qg: f64) -> f64 {
        let key = [pg.to_bits(), qg.to_bits(), self.w as u64, self.tail_target.to_bits()];
        #[cfg(test)]
        self.r_tail_log.push(key);
        if let Some((k, v)) = self.r_tail_cache {
            if k == key {
                #[cfg(test)]
                if self.verify_r_tail_cache {
                    let fresh = math::compute_r_star_exact(pg, qg, self.w as usize, self.tail_target);
                    assert_eq!(fresh.to_bits(), v.to_bits());
                }
                return v;
            }
        }
        #[cfg(test)]
        {
            self.r_tail_solves += 1;
        }
        let (v, kstar, _probes) =
            warm_tail::warm_tail_solve(pg, qg, self.w as usize, self.tail_target, self.prev_kstar);
        #[cfg(test)]
        {
            self.r_tail_dp_runs += _probes;
        }
        self.prev_kstar = kstar.or(self.prev_kstar);
        self.r_tail_cache = Some((key, v));
        v
    }

    /// The continuum's effective tail target — ONE law, no branch:
    /// log-blend of the anchor tail with §4.5's late-is-fine form
    /// p̂ + (BULK_TAIL_BUDGET − p̂)·χ, weighted by β. β = 0 ⇒ the anchor
    /// exactly (the Auto/Realtime side); β = 1 ⇒ tracks p̂ exactly
    /// (r = 0 mid-stream at ANY ε, the χ-glide covering the stream
    /// tail). Continuous in δ everywhere, exact at all three presets.
    fn continuum_tail(&self, p_upper: f64) -> f64 {
        if self.bulkness <= 0.0 {
            return self.tail_target;
        }
        let chi = self.completion_chi_raw();
        let p = p_upper.clamp(1e-9, 0.99);
        // NOTE the upper clamp: the blend must be allowed to track p̂ all
        // the way up (cold start p̂₉₅ ≈ 0.975) — clamping it below p̂
        // would pin r at max-overhead exactly when late-is-fine says 0.
        let bulk_form = (p + (math::BULK_TAIL_BUDGET - p) * chi).clamp(1e-9, 0.99);
        10f64
            .powf(
                (1.0 - self.bulkness) * self.tail_target.log10()
                    + self.bulkness * bulk_form.log10(),
            )
            .clamp(1e-9, 0.99)
    }

    /// Capacity-weighted GE (p_gb, p_bg) across paths, each path falling
    /// back to its TRUE parameters while its estimator is cold (exactly the
    /// single-path fallback rule, applied per path).
    fn agg_ge_params(&self) -> (f64, f64) {
        let pg = self.agg(|p| {
            let ge = p.estimator.ge_estimator();
            if ge.is_valid() && ge.p_gb() > 0.0 && ge.p_bg() > 0.0 {
                ge.p_gb()
            } else {
                p.eps * p.q / (1.0 - p.eps).max(1e-6)
            }
        });
        let qg = self.agg(|p| {
            let ge = p.estimator.ge_estimator();
            if ge.is_valid() && ge.p_gb() > 0.0 && ge.p_bg() > 0.0 {
                ge.p_bg()
            } else {
                p.q
            }
        });
        (pg, qg)
    }

    /// Build the shared production `RateInputs` from the live estimator
    /// state (identical to the real transport's FecRateController path),
    /// with EXPLICIT semantics (tail target, bulk flag, completion χ) — the
    /// plumbing for the mix's two always-computed law terms.
    /// `saturation_cap` is a parameter so the pressure accessor can request
    /// the UNCAPPED rate.
    fn rate_inputs_for(
        &self,
        saturation_cap: bool,
        tail_target: f64,
        bulk_late_is_fine: bool,
        completion_exposure: f64,
    ) -> math::RateInputs {
        // Capacity-weighted aggregates over the per-path estimators: the
        // shared window rides a mixture of the paths. Each path contributes
        // its estimate when its GE estimator is valid, the same cold-start
        // fallback (sigma2 = 1, mean_burst = 1, no t_symbols contribution)
        // otherwise. N = 1 reduces exactly to the single-path inputs.
        let sigma2 = self.agg(|p| {
            let ge = p.estimator.ge_estimator();
            if ge.is_valid() {
                math::burst_variance_factor(ge.p_gb(), ge.p_bg())
            } else {
                1.0
            }
        });
        let mean_burst = self.agg(|p| {
            let ge = p.estimator.ge_estimator();
            if ge.is_valid() { ge.mean_burst_length() } else { 1.0 }
        });
        let t_symbols_raw: f64 = self
            .paths
            .iter()
            .map(|p| {
                if p.estimator.ge_estimator().is_valid() {
                    p.srtt_meas * p.capacity as f64
                } else {
                    0.0
                }
            })
            .sum();
        let t_symbols = if t_symbols_raw > 0.0 { t_symbols_raw.max(1.0) } else { 0.0 };
        let p_upper = self.agg_p_upper();
        math::RateInputs {
            p_upper,
            sigma2,
            mean_burst,
            // Burst-tail provisioning (§4.3): the sim aggregates several per-path
            // estimators and a capacity-weighted mixture of mass QUANTILES
            // is not defined (unlike the moment aggregates above), so the
            // sim does not feed the window-mass tail term yet — it stays
            // inert, matching production cold start. Single-path parity
            // with the production term is covered by the math-crate tests.
            mass: math::MassStats::default(),
            tail_provision: true,
            window: self.encoder.window_size().max(1) as f64,
            t_symbols,
            srtt: self.agg_srtt(),
            t_sym: TICK_SECS / self.total_capacity() as f64,
            codec_overhead: CODEC_OVERHEAD_RLC,
            tail_target,
            bulk_late_is_fine,
            completion_exposure,
            // §4.4: the sim's payload is the transfer (file-transfer
            // semantics) — its delivery latency does not feed back into its
            // own throughput, so the inner-feedback repair floor stays off
            // and mid-stream Bulk remains pure ARQ (production defaults to
            // 0 too).
            inner_feedback: 0.0,
            saturation_cap,
            max_overhead: MAX_OVERHEAD,
        }
    }

    /// Rate from the shared production formula — identical code to the
    /// real transport's FecRateController (via raptorpath-math).
    fn controller_rate_now(&self) -> f64 {
        if let Some(r) = self.fixed_r {
            return r;
        }
        self.rate_now(true)
    }

    /// The rate law, one path at every dial position: a convex mix of the
    /// two laws' outputs (§4.5), both terms computed every tick, no branch
    /// (not even at β = 0), no mode bit:  r(β) = (1−β)·r_anchor + β·r_late.
    /// β = 0 is exactly the anchor term; β = 1 exactly the late-is-fine
    /// term (χ-gated, r = 0 mid-stream at any ε — including the cold start,
    /// which the anchor side pays and the mix therefore fades out
    /// continuously toward Bulk). A tail-domain blend would step at the
    /// preset instead: r*(δ) is near-binary in δ against the cold-start ε̂.
    fn rate_now(&self, saturation_cap: bool) -> f64 {
        #[cfg(test)]
        {
            if self.legacy_bulk_delta {
                // Test-only ablation (see the field): the earlier Bulk mapping.
                let tail = (0.1f64).min(self.agg_p_upper());
                return math::controller_rate(&self.rate_inputs_for(
                    saturation_cap,
                    tail,
                    false,
                    0.0,
                ));
            }
        }
        let anchor = math::controller_rate(&self.rate_inputs_for(
            saturation_cap,
            self.tail_target,
            false,
            0.0,
        ));
        let late = math::controller_rate(&self.rate_inputs_for(
            saturation_cap,
            (BASE_TAIL_TARGET * 100.0).clamp(1e-9, 0.1),
            true,
            self.completion_chi_raw(),
        ));
        (1.0 - self.bulkness) * anchor + self.bulkness * late
    }

    /// Send one repair symbol on path `pi`. Returns whether it survived.
    fn send_repair(&mut self, pi: usize) -> bool {
        let repair = self.encoder.generate_repair();
        for m in repair.window_start..repair.window_start + repair.window_count as u64 {
            self.check_l1(m as usize, 2);
        }
        if self.span_cut_pending {
            self.span_cut_pending = false;
            self.span_cut_repairs += 1;
        }
        let kind = DgKind::Repair {
            start: repair.window_start, count: repair.window_count,
            index: repair.repair_index, data: repair.coded_data,
        };
        let lost = self.send_datagram(kind, 0, pi);
        self.total_fec += 1;
        self.cum_sent += 1;
        {
            let p = &mut self.paths[pi];
            p.sent += 1;
            p.fec += 1;
        }
        self.deliver_due();
        !lost
    }

    /// Record delivery latencies for decoder outputs (excluding pruned
    /// seqs). Latency = time from send to decode plus one-way propagation
    /// (arrived symbols decode at their send tick, so their latency is the
    /// one-way delay; recovered symbols add the recovery wait).
    fn record_delivery(&mut self, seqs: &[u64]) {
        let one_way = self.acc_s() * 1000.0; // ms: the physical accounting term (RTT/2 − d)
        for q in seqs {
            if self.given_up_seqs.contains(q) {
                continue;
            }
            let st = self.send_tick.get(*q as usize).copied().unwrap_or(self.tick);
            let lat = (self.tick.saturating_sub(st)) as f64 + one_way;
            if self.prev_lat_ms >= 0.0 {
                let d = (lat - self.prev_lat_ms).abs();
                self.jitter_ms += (d - self.jitter_ms) / 16.0;
            }
            self.prev_lat_ms = lat;
            self.delivered_lat_ms.push(lat);
            self.decoded_this_tick.push(*q);
            #[cfg(test)]
            self.decode_log.push((*q, self.tick));
            if let Some(slot) = self.lat_by_seq.get_mut(*q as usize) {
                if slot.is_nan() {
                    *slot = lat;
                }
            }
        }
    }

    /// Put a datagram on its path's forward leg (addendum 1 A2). Its loss
    /// outcome is drawn here, at the send, in send order. The default leg
    /// delivers at the send instant.
    fn send_datagram(&mut self, kind: DgKind, seq: u64, pi: usize) -> bool {
        let t = self.now_s();
        #[cfg(test)]
        {
            if self.paths[pi].link_free > t {
                self.free_after_now += 1;
            }
        }
        let floor = matches!(kind, DgKind::Floor);
        let p = &mut self.paths[pi];
        let leg = link::leg_send(&mut p.link_free, t, p.link_sv, p.link_q, p.link_d);
        let (lost, arrive_s) = match leg {
            Some((depart, arrive)) => {
                self.paths[pi].link_busy += self.paths[pi].link_sv;
                #[cfg(test)]
                {
                    let p = &self.paths[pi];
                    if p.link_free - t > p.link_q + p.link_sv {
                        self.backlog_over += 1;
                    }
                }
                let lost = self.wire_lost(pi, depart);
                let p = &mut self.paths[pi];
                if floor {
                    p.floor_sent += 1;
                    p.floor_lost += lost as u32;
                } else {
                    p.lost += lost as u32;
                }
                (lost, arrive)
            }
            None => {
                // Dropped at the queue: no channel state, no loss draw; it
                // rides the leg as a ghost to its would-be arrival.
                let p = &mut self.paths[pi];
                let in_outage = t >= p.outage_start_s && t < p.outage_start_s + p.outage_len_s;
                if floor {
                    p.floor_sent += 1;
                    p.floor_dropped += 1;
                } else {
                    p.qdrop += 1;
                    p.qdrop_in += in_outage as u32;
                }
                (true, link::would_arrive(p.link_free, t, p.link_sv, p.link_d))
            }
        };
        #[cfg(test)]
        let arrive_s = arrive_s + self.paths[pi].test_extra_s;
        #[cfg(test)]
        if matches!(kind, DgKind::Resend) {
            let o = self.one_way_s();
            let p = &self.paths[pi];
            let o_p = p.srtt_belief_s() - p.srtt_secs / 2.0;
            let arr = [arrive_s, f64::NAN][lost as usize];
            self.resend_log.push((seq, pi, t, arr, o, self.h_seq[seq as usize], o_p));
        }
        let dg = Datagram { kind, seq, send_tick: self.tick, path: pi, floor: self.f_ann, lost, arrive_s };
        self.paths[pi].leg.push_back(dg);
        lost
    }

    /// Deliver every datagram whose arrival is due, across paths by arrival
    /// time, ties by path index, then send order (each leg is FIFO). Due =
    /// arrival at or before this tick's instant, with a 1 ns guard so that an
    /// arrival on the tick grid (t + d summed in float seconds can land an ulp
    /// after it) is processed on its own tick. The default leg's arrival is
    /// the send instant itself.
    fn deliver_due(&mut self) {
        let now = self.now_s() + DUE_GUARD_S;
        loop {
            let mut best: Option<(usize, f64)> = None;
            for (pi, p) in self.paths.iter().enumerate() {
                if let Some(dg) = p.leg.front() {
                    if dg.arrive_s <= now && best.is_none_or(|(_, a)| dg.arrive_s < a) {
                        best = Some((pi, dg.arrive_s));
                    }
                }
            }
            let Some((pi, _)) = best else { break };
            let dg = self.paths[pi].leg.pop_front().expect("front");
            self.deliver(dg);
        }
    }

    /// The receiver-side drain after the sender finishes (the offline
    /// simulation's end of run: it stops once the receiver holds no hole, plus
    /// a grace): while the consumer still holds decoded units, the forward leg
    /// delivers, the acks of set-points 1 and 5 land, the floor repeat runs on
    /// its sweep clock and the consumer steps. The sender sends no data and
    /// decides nothing; its finish tick (`get_tick`) and every sender counter
    /// stay as they were. With the default leg the consumer holds nothing at
    /// the finish, so the drain does not run (inert). Bounded by the tick cap.
    /// Units still held when it ends are classed `held`.
    fn drain(&mut self) {
        let finish_tick = self.tick;
        while self.consumer.held() > 0 && self.tick <= 20_000 {
            let now = self.now_s();
            self.deliver_due();
            for pi in 0..self.paths.len() {
                while let Some(&(due, s)) = self.ack_due[pi].front() {
                    if due > self.tick {
                        break;
                    }
                    self.ack_due[pi].pop_front();
                    self.acked_seq[s as usize] = true;
                }
                while let Some(&(due, s)) = self.floor_ack_due[pi].front() {
                    if due > self.tick {
                        break;
                    }
                    self.floor_ack_due[pi].pop_front();
                    self.ack_seq(s as usize, 5);
                }
            }
            while self.a_known < self.next_seq && self.acked_seq[self.a_known as usize] {
                self.a_known += 1;
            }
            let sweep_ticks = ((2.0 * self.agg_srtt()).clamp(0.025, 0.1) / TICK_SECS).round() as u32;
            if self.tick.saturating_sub(self.last_sweep_tick) >= sweep_ticks && self.f_ann > self.a_known {
                self.last_sweep_tick = self.tick;
                let free: Vec<u32> = self.paths.iter().map(|p| p.capacity).collect();
                let dest = self.best_correction_path(&free);
                self.floors += 1;
                self.send_datagram(DgKind::Floor, 0, dest);
                self.deliver_due();
            }
            let decoded = std::mem::take(&mut self.decoded_this_tick);
            self.consumer.step(now, &decoded, self.f_rx);
            for s in std::mem::take(&mut self.consumer.delivered_now) {
                self.unit_dirty.push(self.seq_unit[s]);
            }
            self.tick += 1;
            self.drain_ticks += 1;
        }
        // The class of every unit still held now reads `held`.
        for s in 0..self.next_seq as usize {
            if self.receiver_holds(s as u64) && self.consumer.delivered_at(s).is_nan() {
                self.unit_dirty.push(self.seq_unit[s]);
            }
        }
        self.tick = finish_tick;
    }

    /// Datagrams in flight on every leg.
    fn in_flight(&self) -> usize {
        self.paths.iter().map(|p| p.leg.len()).sum()
    }

    /// The receiver holds seq s (decoded, given-up seqs excepted).
    fn receiver_holds(&self, s: u64) -> bool {
        !self.lat_by_seq[s as usize].is_nan()
    }

    /// Every receiver-side effect of one datagram, at its arrival (addendum 1
    /// A2), keyed by seq. Transport effects (the feedback entry, the arrival
    /// counters, the header floor, an original's set-point-1 ack) always
    /// happen; a copy for a seq the receiver already holds is a duplicate and
    /// does no decode work, no slot release and no class change.
    fn deliver(&mut self, dg: Datagram) {
        let pi = dg.path;
        let due = self.tick + self.paths[pi].reverse_ticks;
        #[cfg(test)]
        self.leg_log.push((dg.send_tick, self.tick, pi, match dg.kind {
            DgKind::Source => 0, DgKind::Resend => 1, DgKind::Repair { .. } => 2, DgKind::Floor => 3,
        }, dg.lost, dg.seq, dg.arrive_s));
        let is_source = matches!(dg.kind, DgKind::Source);
        if !matches!(dg.kind, DgKind::Floor) {
            self.paths[pi].feedback_queue.push_back((due, dg.send_tick, !dg.lost, is_source));
        }
        if dg.lost {
            return;
        }
        let seq = dg.seq;
        {
            let p = &mut self.paths[pi];
            if !matches!(dg.kind, DgKind::Floor) {
                let owd = dg.arrive_s - dg.send_tick as f64 * TICK_SECS + p.srtt_secs / 2.0 - p.link_d;
                p.owd_ms.push(owd * 1000.0);
            }
        }
        if matches!(dg.kind, DgKind::Source | DgKind::Resend) {
            // Cross-path reordering (addendum 1 A4): a seq arriving below one
            // that already arrived on another path.
            let behind = self.paths.iter().enumerate()
                .any(|(j, q)| j != pi && q.max_seq_arrived.is_some_and(|m| m > seq));
            self.cross_reorder += behind as u32;
            let p = &mut self.paths[pi];
            p.max_seq_arrived = Some(p.max_seq_arrived.map_or(seq, |m| m.max(seq)));
        }
        match dg.kind {
            DgKind::Floor => {
                self.paths[pi].floor_arrived += 1;
                self.receive_floor(dg.floor);
            }
            DgKind::Source => {
                self.paths[pi].arrived += 1;
                self.cum_arrived += 1;
                self.carry_floor(dg.floor);
                // Set-point 1: the ack lands one reverse leg after the arrival.
                self.ack_due[pi].push_back((due, seq as u32));
                if let Some(sym) = self.symbols.iter_mut().rev().find(|x| x.seq == seq) {
                    sym.ack_at = due;
                }
                if self.receiver_holds(seq) {
                    self.dup += 1;
                    return;
                }
                let data = self.source_store[seq as usize].clone();
                #[cfg(test)]
                self.feed_log.push((seq, u16::MAX, 0, data.clone()));
                let rec = self.decoder.feed_source(seq, &data);
                // Cascade outputs may resolve OTHER lost symbols.
                self.mark_recovered(&rec);
                self.record_delivery(&rec);
                self.cum_decoded += rec
                    .iter()
                    .filter(|q| !self.given_up_seqs.contains(q))
                    .count() as u32;
            }
            DgKind::Resend => {
                self.paths[pi].arrived += 1;
                self.cum_arrived += 1;
                self.carry_floor(dg.floor);
                // The sender learns this arrival now, at the arrival tick,
                // one reverse leg before its feedback could land (coordinator
                // default, addendum 1 A2): the lead it takes.
                let lead = due - self.tick;
                // L2's outcome (addendum 1 A3): an admitted resend that
                // arrives later than T after hand-over.
                self.resends_arrived += 1;
                let age_at_arrival = dg.arrive_s - self.h_seq[seq as usize] + self.acc_s();
                self.l2_late += !knobs::within_lifetime(self.lifetime_s, age_at_arrival) as u32;
                self.lead_n += 1;
                self.lead_sum += lead as u64;
                self.lead_max = self.lead_max.max(lead);
                #[cfg(test)]
                self.lead_log.push((seq, pi, lead));
                if self.receiver_holds(seq) {
                    self.dup += 1;
                    return;
                }
                // The sender's bookkeeping below runs only for a hole still
                // open at its arrival (always so when the copy arrives at its
                // send); a hole the floor committed meanwhile is a late fill,
                // resolved by `mark_recovered` alone.
                let open = self.symbols.iter().position(|x| x.seq == seq && x.lost && !x.recovered);
                let data = self.source_store[seq as usize].clone();
                #[cfg(test)]
                self.feed_log.push((seq, u16::MAX, 0, data.clone()));
                let rec = self.decoder.feed_source(seq, &data);
                // Cascade outputs may resolve OTHER lost symbols;
                // pruned (given-up) seqs are discarded, not counted.
                self.mark_recovered(&rec);
                self.record_delivery(&rec);
                self.cum_decoded += rec
                    .iter()
                    .filter(|q| !self.given_up_seqs.contains(q))
                    .count() as u32;
                if let Some(i) = open {
                    // Recovery releases the lost original's store slot —
                    // exactly once: mark_recovered above may already have
                    // resolved this seq (the fed source is in the
                    // decoder's output) and released it there.
                    if !self.symbols[i].recovered {
                        self.store_occupancy = self.store_occupancy.saturating_sub(1);
                    }
                    self.symbols[i].recovered = true;
                    // NOTE (kept bit for bit): this decrements again after
                    // mark_recovered already did for the same seq.
                    self.lost_pending = self.lost_pending.saturating_sub(1);
                    self.acked_seq[seq as usize] = true; // set-point 2
                }
            }
            DgKind::Repair { start, count, index, data } => {
                self.paths[pi].arrived += 1;
                self.cum_arrived += 1;
                self.carry_floor(dg.floor);
                #[cfg(test)]
                self.feed_log.push((start, count, index, data.clone()));
                let recovered = self.decoder.feed_repair(start, count, index, &data);
                self.mark_recovered(&recovered);
                self.record_delivery(&recovered);
                self.cum_decoded += recovered
                    .iter()
                    .filter(|q| !self.given_up_seqs.contains(q))
                    .count() as u32;
            }
        }
    }

    fn mark_recovered(&mut self, seqs: &[u64]) {
        for rseq in seqs {
            for sym in &mut self.symbols {
                if sym.seq == *rseq && sym.lost && !sym.recovered {
                    sym.recovered = true;
                    self.lost_pending = self.lost_pending.saturating_sub(1);
                    // A lost symbol's store slot is released on recovery
                    // (its SACK never came — the slot was held for exactly
                    // the recovery span; §6.3 semantics).
                    self.store_occupancy = self.store_occupancy.saturating_sub(1);
                    self.acked_seq[*rseq as usize] = true; // set-point 3
                }
            }
            // A committed hole decoded later: out of passed_undelivered here,
            // into cum_decoded at the caller (the same decoder output).
            let i = *rseq as usize;
            if self.committed_seq.get(i).copied().unwrap_or(false) {
                self.committed_seq[i] = false;
                self.passed_undelivered -= 1;
                self.ack_seq(i, 3);
            }
        }
    }
}

#[wasm_bindgen]
impl Simulation {
    /// hint: "continuum" (custom_delta = the δ price; the dial's one path) |
    /// "bulk" | "auto" | "realtime" (NAMED POINTS on that dial: δ = 0.005 /
    /// 0.5 / 50, built by the same code) | "fixed" (uses fixed_r as r).
    /// custom_rho (every hint) sets the retention contract: holes older than
    /// D(δ) are given up within the budget min(1 − ρ, R)·src_total (0 at
    /// ρ = 1); reliability becomes the emergent third variable.
    #[wasm_bindgen(constructor)]
    pub fn new(
        eps: f64, q: f64, rtt_ms: u32, w: u32, hint: String,
        fixed_r: Option<f64>, custom_delta: Option<f64>, custom_rho: Option<f64>,
    ) -> Self {
        Self::multipath(
            vec![eps], vec![q], vec![rtt_ms], vec![4], w, hint,
            fixed_r, custom_delta, custom_rho,
        )
    }

    /// Multipath constructor (§5.5, RWM at L0): N independent GE
    /// channels (per-path eps / q / RTT / capacity in slots-per-tick) pouring
    /// into ONE shared reliable window. `Simulation::new(e,q,rtt,w,...)` is
    /// exactly `multipath([e],[q],[rtt],[4],w,...)` — the single-path
    /// simulation is the N = 1 special case, bit for bit (same per-path seed
    /// derivation, same RNG stream, same slot order).
    #[allow(clippy::too_many_arguments)]
    pub fn multipath(
        eps: Vec<f64>, q: Vec<f64>, rtt_ms: Vec<u32>, capacity: Vec<u32>,
        w: u32, hint: String,
        fixed_r: Option<f64>, custom_delta: Option<f64>, custom_rho: Option<f64>,
    ) -> Self {
        let n = eps.len().max(1).min(8);
        // The three knobs at their inert values (plan §3.1): T = H = λ = ∞,
        // 2 000 units, an empty outage window on every path.
        let inf = f64::INFINITY;
        Self::multipath_knobs(
            eps, q, rtt_ms, capacity, w, hint, fixed_r, custom_delta, custom_rho,
            inf, inf, inf, 2000, vec![inf; n], vec![0.0; n],
        )
        .expect("the inert knobs are in the domain")
    }

    /// The three knobs with the default forward leg (no bottleneck, no
    /// propagation split) and no resend budget: `multipath_link` at
    /// service 0, queue ∞, propagation 0 per path and limit ∞.
    #[allow(clippy::too_many_arguments)]
    pub fn multipath_knobs(
        eps: Vec<f64>, q: Vec<f64>, rtt_ms: Vec<u32>, capacity: Vec<u32>,
        w: u32, hint: String,
        fixed_r: Option<f64>, custom_delta: Option<f64>, custom_rho: Option<f64>,
        lifetime_s: f64, hold_s: f64, source_rate: f64, num_source: u32,
        outage_start_s: Vec<f64>, outage_len_s: Vec<f64>,
    ) -> Result<Simulation, String> {
        let n = eps.len().max(1).min(8);
        let inf = f64::INFINITY;
        Self::multipath_link(
            eps, q, rtt_ms, capacity, w, hint, fixed_r, custom_delta, custom_rho,
            lifetime_s, hold_s, source_rate, num_source, outage_start_s, outage_len_s,
            vec![0.0; n], vec![inf; n], vec![0.0; n], inf,
        )
    }

    /// The one constructor (three-knobs plan §3.1, addendum 1 A2). `lifetime_s`
    /// = T ∈ (0, ∞], `hold_s` = H ∈ [0, ∞], `source_rate` = λ in units/s (∞ =
    /// the object), `num_source` units, a per-path outage window [start,
    /// start + len) in seconds; per path the forward leg's service time and
    /// queue limit (s) and propagation delay (s, ≤ rtt/2); and the resend
    /// budget per 5-tick window. `multipath_knobs`, `multipath` and `new`
    /// delegate here with the inert values; at those values the run is main's
    /// model bit for bit.
    #[allow(clippy::too_many_arguments)]
    pub fn multipath_link(
        eps: Vec<f64>, q: Vec<f64>, rtt_ms: Vec<u32>, capacity: Vec<u32>,
        w: u32, hint: String,
        fixed_r: Option<f64>, custom_delta: Option<f64>, custom_rho: Option<f64>,
        lifetime_s: f64, hold_s: f64, source_rate: f64, num_source: u32,
        outage_start_s: Vec<f64>, outage_len_s: Vec<f64>,
        service_s: Vec<f64>, queue_s: Vec<f64>, prop_s: Vec<f64>, resend_limit: f64,
    ) -> Result<Simulation, String> {
        knobs::validate_knobs(lifetime_s, hold_s)?;
        if !(source_rate > 0.0) {
            return Err(format!("source rate must be in (0, inf], got {source_rate}"));
        }
        if !(resend_limit >= 0.0) {
            return Err(format!("resend limit must be in [0, inf], got {resend_limit}"));
        }
        let n = eps.len().max(1).min(8);
        assert!(
            q.len() >= n && rtt_ms.len() >= n && capacity.len() >= n
                && outage_start_s.len() >= n && outage_len_s.len() >= n
                && service_s.len() >= n && queue_s.len() >= n && prop_s.len() >= n,
            "per-path parameter arrays must have equal length"
        );
        for i in 0..n {
            link::validate_link(service_s[i], queue_s[i], prop_s[i], rtt_ms[i] as f64 / 1000.0)?;
        }

        // Hint -> δ price, then ONE construction path for every hint.
        //
        // The named hints are an ALIAS TABLE onto the δ dial (§4.1: Bulk
        // 0.005, Auto 0.5, Realtime 50), not machines: each goes through the
        // same tail map t(δ) and bulkness β(δ) as "continuum", whose
        // `custom_delta` carries the δ PRICE. The rate is the (1−β) mix in
        // `rate_now`. No hint branch exists downstream of construction: a
        // threshold-keyed hint flip at a preset would be a hidden mode
        // switch. ("fixed" and unknown strings take the Auto point; "fixed"
        // then overrides r, see below.)
        let dp = match hint.as_str() {
            "bulk" => 0.005,
            "realtime" => 50.0,
            "continuum" => custom_delta.unwrap_or(0.5),
            _ => 0.5, // "auto", "fixed"
        };
        let tail_target = sim_tail_target_of_delta(dp).clamp(1e-9, 0.1);
        let bulkness = bulkness_of_delta(dp);
        let fixed_r = if hint == "fixed" { Some(fixed_r.unwrap_or(0.1)) } else { None };
        // The ρ dial composes with the δ price — the triangle's two dials
        // are independent (§1.2), so EVERY hint honours custom_rho: a
        // Bulk-priced transfer may declare ρ < 1 (§3.5 give-up), and
        // so may Realtime. Gating ρ on the hint would be a hidden mode
        // switch. None defaults to 1.0 (the shipped engine hints carry
        // ρ = 1).
        let rho = custom_rho.unwrap_or(1.0).clamp(0.9, 1.0);

        let mut paths = Vec::with_capacity(n);
        for i in 0..n {
            let (e, qq) = (eps[i], q[i]);
            let rtt = rtt_ms[i];
            let cap = capacity[i].clamp(1, 16);
            let p = if e < 1.0 { e * qq / (1.0 - e) } else { qq };
            // Path 0 keeps exactly the single-path seed (i * salt == 0);
            // sibling paths get decorrelated streams even with identical
            // channel parameters (the symmetric 2-path case).
            let seed = e.to_bits() ^ qq.to_bits().rotate_left(32)
                ^ (rtt as u64).wrapping_mul(0x517cc1b727220a95)
                ^ (i as u64).wrapping_mul(0x9e3779b97f4a7c15);
            let num_wire = num_source as usize * 4; // generous margin incl. corrections
            paths.push(PathState {
                eps: e,
                q: qq,
                capacity: cap,
                rtt_ticks: rtt.max(2),
                srtt_secs: rtt as f64 / 1000.0,
                rttvar_secs: rtt as f64 / 8000.0,
                channel_states: generate_channel_states(p, qq, e, num_wire, seed),
                wire_idx: 0,
                estimator: math::LossEstimator::new(),
                feedback_queue: std::collections::VecDeque::new(),
                leg: std::collections::VecDeque::new(),
                reverse_ticks: reverse_ticks(rtt.max(2), prop_s[i]),
                link_sv: service_s[i],
                link_q: queue_s[i],
                link_d: prop_s[i],
                link_free: 0.0,
                qdrop: 0,
                qdrop_in: 0,
                link_busy: 0.0,
                floor_sent: 0,
                floor_lost: 0,
                floor_dropped: 0,
                floor_arrived: 0,
                owd_ms: Vec::new(),
                max_seq_arrived: None,
                srtt_meas: rtt.max(2) as f64,
                rttvar_meas: rtt.max(2) as f64 / 8.0,
                rtt_samples: 0,
                #[cfg(test)]
                test_extra_s: 0.0,
                last_flush_tick: 0,
                pending_sent: 0,
                pending_ok: 0,
                sent: 0,
                arrived: 0,
                src: 0,
                fec: 0,
                arq: 0,
                lost: 0,
                stripe_credit: 0.0,
                outage_start_s: outage_start_s[i],
                outage_len_s: outage_len_s[i],
            });
        }
        let tot_cap: f64 = paths.iter().map(|p| p.capacity as f64).sum();
        let sigma2_true = paths
            .iter()
            .map(|pp| {
                let p = if pp.eps < 1.0 { pp.eps * pp.q / (1.0 - pp.eps) } else { pp.q };
                pp.capacity as f64 / tot_cap * math::burst_variance_factor(p, pp.q)
            })
            .sum();

        Ok(Self {
            paths,
            sigma2_true,
            tail_target, bulkness, fixed_r,
            #[cfg(test)]
            legacy_bulk_delta: false,
            #[cfg(test)]
            legacy_tail_burst: false,
            rho,
            given_up: 0,
            dprice: dp,
            #[cfg(test)]
            giveup_log: Vec::new(),
            given_up_seqs: std::collections::BTreeSet::new(),
            send_tick: Vec::new(),
            delivered_lat_ms: Vec::new(),
            lat_by_seq: vec![f64::NAN; num_source as usize],
            jitter_ms: 0.0,
            prev_lat_ms: -1.0,
            w,
            tick: 0,
            source_done: false,
            tail_flushed: false,
            finished: false,
            num_source,
            encoder: math::RlcEncoder::new(8),
            decoder: math::RlcDecoder::new(8),
            symbols: Vec::new(),
            source_store: Vec::new(),
            next_seq: 0,
            rate: 0.0,
            debt: 0.0,
            prev_completion_chi: 0.0,
            r_tail_cache: None,
            prev_kstar: None,
            #[cfg(test)]
            r_tail_dp_runs: 0,
            #[cfg(test)]
            r_tail_log: Vec::new(),
            #[cfg(test)]
            r_tail_solves: 0,
            #[cfg(test)]
            verify_r_tail_cache: false,
            total_src: 0, total_fec: 0, total_arq: 0, total_lost: 0,
            cum_sent: 0, cum_arrived: 0, cum_decoded: 0,
            lost_pending: 0,
            last_src: 0, last_fec: 0, last_arq: 0, last_lost: 0,
            store_occupancy: 0,
            pool_stalls: 0,
            frontier: 0,
            frontier_acked: 0,
            frontier_hist: std::collections::VecDeque::new(),
            phantom_held: 0,
            phantom_avoided: 0,
            rng_state: 0xdeadbeef12345678,
            lifetime_s,
            hold_s,
            source_rate,
            u_next: 0,
            intake_expired: 0,
            intake_expired_deq: 0,
            h_seq: Vec::new(),
            last_send_seq: Vec::new(),
            path_seq: Vec::new(),
            resent_seq: Vec::new(),
            acked_seq: Vec::new(),
            a_known: 0,
            f_ann: 0,
            f_rx: 0,
            committed_seq: Vec::new(),
            passed_seq: Vec::new(),
            passed_undelivered: 0,
            slot_owed: Vec::new(),
            ack_due: vec![std::collections::VecDeque::new(); n],
            floor_ack_due: vec![std::collections::VecDeque::new(); n],
            last_sweep_tick: 0,
            commit_viol: 0,
            skip_below_floor: 0,
            l1_viol: 0,
            floors: 0,
            floor_carried: 0,
            span_cut_pending: false,
            span_cut_repairs: 0,
            consumer: consumer::Consumer::new(hold_s),
            decoded_this_tick: Vec::new(),
            seq_unit: Vec::new(),
            unit_seq: vec![NO_SEQ; num_source as usize],
            unit_emitted: vec![CLASS_PENDING; num_source as usize],
            unit_dirty: Vec::new(),
            dup: 0,
            lead_n: 0,
            lead_sum: 0,
            lead_max: 0,
            cross_reorder: 0,
            drain_ticks: 0,
            queue_pending: Vec::new(),
            #[cfg(test)]
            backlog_over: 0,
            resends_arrived: 0,
            l2_late: 0,
            #[cfg(test)]
            resend_log: Vec::new(),
            resend_limit,
            budget_used: Vec::new(),
            braked_seq: Vec::new(),
            braked_units: 0,
            braked_attempts: 0,
            #[cfg(test)]
            leg_log: Vec::new(),
            #[cfg(test)]
            lead_log: Vec::new(),
            #[cfg(test)]
            feed_log: Vec::new(),
            #[cfg(test)]
            free_after_now: 0,
            #[cfg(test)]
            decode_log: Vec::new(),
            #[cfg(test)]
            commit_log: Vec::new(),
            #[cfg(test)]
            slot_release_log: Vec::new(),
            #[cfg(test)]
            payload_log: Vec::new(),
        })
    }

    pub fn step(&mut self) {
        if self.finished {
            return;
        }
        let mut src_n = 0u32;
        let mut fec_n = 0u32;
        let mut arq_n = 0u32;
        let mut lost_n = 0u32;

        // --- Feedback path: outcomes become known one RTT after send, PER
        // PATH (each path has its own ACK delay and its own estimator). Fed
        // with the true per-symbol pattern (§2.6); flushed once per
        // that path's RTT — each estimator honestly lags its own path.
        // Arrivals due by now (addendum 1 A2), before the feedback pop.
        self.deliver_due();
        let mut sack_released = 0u32;
        for pi in 0..self.paths.len() {
            let tick = self.tick;
            let p = &mut self.paths[pi];
            while let Some(&(due, sent, ok, src_slot)) = p.feedback_queue.front() {
                if tick < due {
                    break;
                }
                p.feedback_queue.pop_front();
                p.estimator.record_symbol(ok);
                p.pending_sent += 1;
                if ok && src_slot {
                    // An arrived original (a source symbol is never resent
                    // while its original arrived: Karn holds by construction).
                    p.rtt_sample((tick - sent) as f64);
                }
                if ok {
                    p.pending_ok += 1;
                    // SACK-clocked store release (§6.3, wall #9): the
                    // slot frees the moment THIS path's selective ack
                    // lands — not when the cumulative frontier passes it.
                    if src_slot {
                        sack_released += 1;
                    }
                }
            }
            if tick.saturating_sub(p.last_flush_tick) >= p.retry_delay() && p.pending_sent > 0 {
                p.estimator.record_counts(p.pending_sent, p.pending_ok, tick as u64);
                p.pending_sent = 0;
                p.pending_ok = 0;
                p.last_flush_tick = tick;
            }
        }
        self.store_occupancy = self.store_occupancy.saturating_sub(sack_released);

        // --- Rate from the shared production formula ---
        self.rate = self.controller_rate_now();

        // --- Wire slots ---
        // Per-slot priority (§3.4): (1) a P_lost-confirmed
        // retransmit — ARQ is driven by loss confidence, INDEPENDENT of the
        // FEC budget (Bulk's r ~ 0 must not delay recovery to end of
        // stream); (2) a repair when the taper debt says one is due;
        // (3) new source. Retransmits scan ALL outstanding candidates so a
        // single symbol waiting out its retry timer cannot head-of-line
        // stall the drain.
        //
        // Multipath (§5.5): the tick's slot budget is the union of all
        // paths' capacities. Each action then picks its carrying path:
        // corrections (repairs AND retransmits — a symbol lost on path i may
        // be resent on any path j) ride the highest-goodput path with a
        // spare slot (cf. the §5.7 placement law); source symbols are
        // striped across spare-slot paths proportional to estimated goodput
        // g_i (smooth WRR). With N = 1 every choice is path 0 and the loop
        // is exactly the single-path loop.
        let mut free: Vec<u32> = self.paths.iter().map(|p| p.capacity).collect();
        // Floor repeat (plan §7 P3; toy `floor_repeat`): on the sweep clock
        // clamp(2·srtt, 25, 100 ms), while F_ann > A_known, one datagram
        // carrying F_ann takes one slot and one loss draw. Never at T = ∞
        // (F_ann = A_known there).
        let sweep_ticks = ((2.0 * self.agg_srtt()).clamp(0.025, 0.1) / TICK_SECS).round() as u32;
        if self.tick.saturating_sub(self.last_sweep_tick) >= sweep_ticks {
            self.last_sweep_tick = self.tick;
            self.update_floor();
            if self.f_ann > self.a_known {
                let dest = self.best_correction_path(&free);
                free[dest] -= 1;
                self.floors += 1;
                self.send_datagram(DgKind::Floor, 0, dest);
                self.deliver_due();
            }
        }
        let total_slots: u32 = free.iter().sum();
        for _ in 0..total_slots {
            // F_ann is current at every slot: recomputed after every send
            // (and once per tick below).
            self.update_floor();
            // (1) P_lost-gated retransmit across all candidates. Retry timer
            // and loss belief come from the path the symbol was LAST sent on
            // (that is where its feedback lives).
            let mut did_retx = false;
            let mut cand: Option<usize> = None;
            // Three knobs: no candidate below the floor (the commitment), and
            // L2 — a resend only if it can arrive before T (§2.2, ô of §17.1).
            let (now_s, o_hat) = (self.now_s(), self.one_way_s());
            for (i, sym) in self.symbols.iter().enumerate() {
                if sym.lost
                    && !sym.recovered
                    && (self.tick as i64 - sym.last_retx_tick)
                        >= self.paths[sym.path].retry_delay() as i64
                    && sym.seq >= self.f_ann as u64
                    && knobs::resend_admissible(
                        self.lifetime_s, now_s - self.h_seq[sym.seq as usize], o_hat,
                    )
                {
                    cand = Some(i);
                    break;
                }
            }
            if let Some(i) = cand {
                let sp = self.symbols[i].path;
                let age_ticks = self.tick.saturating_sub(self.symbols[i].tick);
                let pl = math::p_lost(
                    age_ticks as f64 * TICK_SECS,
                    self.paths[sp].estimator.loss_rate().clamp(1e-4, 0.99),
                    self.paths[sp].srtt_belief_s(),
                    self.paths[sp].rttvar_secs,
                );
                if self.rng() < pl && self.floor_recheck(i) && self.budget_admits(i) {
                    // Cross-path recovery: resend the EXACT stored bytes on
                    // the best currently-available path, not necessarily the
                    // one that lost them.
                    let dest = self.best_correction_path(&free);
                    let seq = self.symbols[i].seq;
                    self.symbols[i].last_retx_tick = self.tick as i64;
                    self.symbols[i].path = dest;
                    self.last_send_seq[seq as usize] = self.tick;
                    self.path_seq[seq as usize] = dest;
                    self.resent_seq[seq as usize] = true;
                    if seq < self.f_ann as u64 {
                        self.commit_viol += 1; // a copy below the floor (must stay 0)
                    }
                    self.check_l1(seq as usize, 1);
                    // A retransmit holds no NEW store slot: the lost
                    // original's slot is still held (until recovery).
                    let lost = self.send_datagram(DgKind::Resend, seq, dest);
                    free[dest] -= 1;
                    self.total_arq += 1;
                    arq_n += 1;
                    self.cum_sent += 1;
                    {
                        let p = &mut self.paths[dest];
                        p.sent += 1;
                        p.arq += 1;
                    }
                    if lost {
                        lost_n += 1;
                    }
                    self.deliver_due();
                    did_retx = true;
                }
            }
            if did_retx {
                continue;
            }

            // (2) Repair when the taper debt says one is due — on the best
            // available path (repairs are path-agnostic: ANY path's arrival
            // advances the shared window decode).
            // The L1 age edge (P3), before the arm's window test so an emptied
            // window cannot take the repair arm.
            if self.apply_age_edge() {
                self.span_cut_pending = true;
            }
            if self.debt >= 1.0 && self.encoder.window_size() > 0 {
                let dest = self.best_correction_path(&free);
                self.debt -= 1.0;
                let ok = self.send_repair(dest);
                free[dest] -= 1;
                fec_n += 1;
                if !ok {
                    lost_n += 1;
                }
            } else if !self.source_done && self.intake_head_ready() {
                // Path-scaled outstanding pool (wall #7, §6.1): a new
                // source symbol needs a retention-store slot; the pool is
                // STORE_PER_PATH × n_paths. When cap·RTT outgrows it the
                // slot idles — the Little's-law stall, made visible.
                if self.store_occupancy >= STORE_PER_PATH * self.paths.len() as u32 {
                    self.pool_stalls += 1;
                    continue;
                }
                // --- Source slot: striped proportional to estimated goodput ---
                let dest = self.next_source_path(&free);
                let data = vec![self.next_seq as u8; 8];
                let seq = self.encoder.add_source(&data);
                self.source_store.push(data.clone());
                self.send_tick.push(self.tick);
                self.h_seq.push(self.handover_s(self.u_next));
                self.last_send_seq.push(self.tick);
                self.path_seq.push(dest);
                self.resent_seq.push(false);
                self.acked_seq.push(false);
                self.committed_seq.push(false);
                self.passed_seq.push(false);
                self.slot_owed.push(false);
                self.seq_unit.push(self.u_next);
                self.unit_seq[self.u_next as usize] = seq as u32;
                self.check_l1(seq as usize, 0);
                // Slide the encoder window (paper W).
                if self.encoder.window_size() > self.w as usize {
                    let oldest = self.encoder.next_seq().saturating_sub(self.w as u64);
                    self.encoder.advance(oldest);
                }
                let lost = self.send_datagram(DgKind::Source, seq, dest);
                free[dest] -= 1;
                self.store_occupancy += 1; // slot taken at send (§6.3)
                {
                    let p = &mut self.paths[dest];
                    p.sent += 1;
                    p.src += 1;
                }
                self.symbols.push(Symbol {
                    tick: self.tick,
                    seq,
                    lost,
                    recovered: false,
                    // Per-path recovery clock (wall #8, §7.1 / RFC 9002
                    // generalized per path): a hole may not fire before its
                    // own path's RTT clock has run from the send — the loss
                    // report cannot exist earlier. Seeding the retx clock at
                    // the send tick makes first eligibility send + own-RTT
                    // (immediate P_lost-gated firing would produce phantom
                    // retransmits).
                    last_retx_tick: self.tick as i64,
                    ack_at: u32::MAX,
                    path: dest,
                });
                self.total_src += 1;
                src_n += 1;
                self.cum_sent += 1;
                // Steady-state debt accrual: r per SOURCE symbol — the
                // aggregate correction rate is taper-shape-invariant
                // (§3.3). Plus the completion-tail term (§4.6):
                // near a known end-of-stream, B_tail x dchi extra debt refills
                // the truncated taper integral, metering one window's worth of
                // repairs across the exposed span (non-Bulk hints; 0
                // mid-stream). Continuous replacement for the one-shot burst.
                let completion = self.completion_debt_increment();
                self.debt = (self.debt + self.rate + completion).min(8.0);
                if lost {
                    lost_n += 1;
                    self.total_lost += 1;
                    self.lost_pending += 1;
                }
                self.deliver_due();
                self.next_seq += 1;
                self.u_next += 1;
                if self.u_next >= self.num_source {
                    self.source_done = true;
                    // One-shot completion-tail burst (the limiting case of
                    // §4.6's ramp): kept only for the test-only ablation arms
                    // (legacy Bulk delta, legacy tail burst) used for A/B
                    // comparison. The live law is the continuous chi-driven
                    // completion term — the Bulk end via its delta glide
                    // (§4.6), the rest via the `completion_debt_increment`
                    // folded into the debt accrual above (§3.3), cross-faded
                    // by β. Firing the burst on top would double-pay the
                    // tail budget.
                    #[cfg(test)]
                    let legacy_burst = self.legacy_bulk_delta || self.legacy_tail_burst;
                    #[cfg(not(test))]
                    let legacy_burst = false;
                    if !self.tail_flushed && legacy_burst {
                        self.tail_flushed = true;
                        let (pg, qg) = self.agg_ge_params();
                        let r_tail = math::compute_r_star_exact(pg, qg, self.w as usize, 0.05);
                        let n_tail = (r_tail * self.w as f64).ceil().min(24.0);
                        self.debt += n_tail;
                    }
                }
            }
        }

        // Give-up law (§3.5, §5.6; docs/derivations/giveup-law-model.md):
        // a lost, unrecovered hole is given up iff
        //   age > D(δ)  ∧  given_up + 1 <= min(1 − ρ, R) · src_total,
        // oldest first. One formula, no ρ guard: at ρ = 1 the budget is 0
        // and nothing is given up (RETAIN is the ρ = 1 point of the dial).
        // R is the residual after in-window FEC over this sim's own window W.
        {
            let cap = self.get_giveup_budget() * self.total_src as f64;
            let deadline = self.get_giveup_deadline();
            let now = self.tick;
            for sym in &mut self.symbols {
                if (self.given_up + 1) as f64 > cap {
                    break;
                }
                if sym.lost && !sym.recovered && now.saturating_sub(sym.tick) as f64 > deadline {
                    sym.recovered = true; // given up
                    self.given_up += 1;
                    #[cfg(test)]
                    self.giveup_log.push((
                        now - sym.tick,
                        sym.last_retx_tick != sym.tick as i64,
                        self.given_up,
                        cap,
                    ));
                    self.given_up_seqs.insert(sym.seq);
                    self.unit_dirty.push(self.seq_unit[sym.seq as usize]);
                    self.acked_seq[sym.seq as usize] = true; // set-point 4
                    self.lost_pending = self.lost_pending.saturating_sub(1);
                    // Give-up resolves the slot too (§6.3 semantics).
                    self.store_occupancy = self.store_occupancy.saturating_sub(1);
                }
            }
        }

        // ACK arrived-but-unmarked symbols when their arrival's ack lands
        // (SACK view; arrival + reverse leg, addendum 1 A2).
        let tick = self.tick;
        for sym in &mut self.symbols {
            if !sym.lost && !sym.recovered && tick >= sym.ack_at {
                sym.recovered = true;
            }
        }

        // Three knobs, set-points 1 and 5 (per seq: `symbols` is pruned
        // before most acks land), then the once-per-tick floor.
        for pi in 0..self.paths.len() {
            while let Some(&(due, s)) = self.ack_due[pi].front() {
                if due > tick {
                    break;
                }
                self.ack_due[pi].pop_front();
                self.acked_seq[s as usize] = true;
            }
            while let Some(&(due, s)) = self.floor_ack_due[pi].front() {
                if due > tick {
                    break;
                }
                self.floor_ack_due[pi].pop_front();
                self.ack_seq(s as usize, 5);
            }
        }
        self.update_floor();
        // The consumer stage (L6), output only: this tick's decodes, then
        // the floor the receiver holds.
        let decoded = std::mem::take(&mut self.decoded_this_tick);
        self.consumer.step(self.now_s(), &decoded, self.f_rx);
        for s in std::mem::take(&mut self.consumer.delivered_now) {
            self.unit_dirty.push(self.seq_unit[s]);
        }

        // Finish conditions (rho = 100%: retransmit until everything is in;
        // rho < 100%: given-up symbols count as resolved, not delivered).
        // Every unit is in exactly one class: decoded, given up, expired at
        // the intake, or passed by the floor undelivered (0 at T = ∞).
        let resolved = self.cum_decoded + self.given_up + self.intake_expired
            + self.intake_expired_deq + self.passed_undelivered;
        // Nothing may still be in flight (addendum 1 A2; always so with the
        // default leg, which delivers at the send).
        let drained = self.in_flight() == 0;
        if self.source_done && resolved >= self.num_source && drained {
            self.finished = true;
        }
        if self.source_done
            && self.lost_pending == 0
            && !self.symbols.iter().any(|s| s.lost && !s.recovered)
            && drained
        {
            self.finished = true;
        }
        if self.tick > 20_000 {
            self.finished = true;
        }

        // --- Per-path recovery clocks (wall #8, §7.1): RFC 9002 loss
        // detection generalized per path. A hole's retransmit clock is its
        // own path's RTT; a global clock (the aggregate RTT) would already
        // have fired
        // for slow-path holes the moment the aggregate clock expired. Count
        // the holes currently HELD by their own clock past the global one
        // (a striping gap not firing while the other path's clock runs),
        // and the cumulative phantom retransmits thereby avoided.
        {
            let global_clock = self.agg_rtt_ticks();
            let mut held = 0u32;
            let mut crossings = 0u32;
            for sym in &self.symbols {
                if sym.lost && !sym.recovered {
                    let own = self.paths[sym.path].retry_delay();
                    if own > global_clock {
                        let base = if sym.last_retx_tick >= 0 {
                            sym.last_retx_tick as u32
                        } else {
                            sym.tick
                        };
                        let age = self.tick.saturating_sub(base);
                        if age >= global_clock && age < own {
                            held += 1;
                            if age == global_clock {
                                crossings += 1; // once per hole per retx cycle
                            }
                        }
                    }
                }
            }
            self.phantom_held = held;
            self.phantom_avoided += crossings;
        }

        // --- Cumulative delivery frontier + its RTT-delayed acked view.
        // frontier = next undelivered source seq; frontier_acked = the
        // frontier as the sender knows it (one aggregate-RTT-old), i.e.
        // what a frontier-clocked store (wall #9) would release against.
        // next_seq − frontier_acked is the counterfactual occupancy the
        // SACK-clocked gauge (§6.3) is compared with.
        while (self.frontier as usize) < self.lat_by_seq.len()
            && (!self.lat_by_seq[self.frontier as usize].is_nan()
                || self.given_up_seqs.contains(&(self.frontier as u64))
                || self.committed_seq.get(self.frontier as usize).copied().unwrap_or(false))
        {
            self.frontier += 1;
        }
        self.frontier_hist.push_back((self.tick, self.frontier));
        if let Some(ack_cut) = self.tick.checked_sub(self.agg_rtt_ticks()) {
            while self.frontier_hist.len() > 1 && self.frontier_hist[1].0 <= ack_cut {
                self.frontier_hist.pop_front();
            }
            if let Some(&(t, f)) = self.frontier_hist.front() {
                if t <= ack_cut {
                    self.frontier_acked = f;
                }
            }
        }

        // Prune resolved symbols (keep lost unrecovered for ARQ).
        self.symbols
            .retain(|s| (s.lost && !s.recovered) || tick.saturating_sub(s.tick) < 10);

        self.last_src = src_n;
        self.last_fec = fec_n;
        self.last_arq = arq_n;
        self.last_lost = lost_n;
        let now = self.now_s();
        for p in &self.paths {
            self.queue_pending.push((p.link_free - now).max(0.0) * 1000.0);
        }
        self.tick += 1;
        if self.finished {
            self.drain();
        }
    }

    // --- Accessors (superset of the old API; UI-compatible names) ---
    pub fn is_finished(&self) -> bool { self.finished }
    /// Last consumed wire slot's channel state on path 0 (legacy name).
    pub fn channel_is_good(&self) -> bool {
        self.path_channel_is_good(0)
    }
    /// Last consumed wire slot's channel state on path `i`.
    pub fn path_channel_is_good(&self, i: usize) -> bool {
        let p = match self.paths.get(i) { Some(p) => p, None => return true };
        !p.channel_states.get(p.wire_idx.saturating_sub(1)).copied().unwrap_or(false)
    }

    // --- Multipath accessors (§5.5 — RWM at L0) ---
    pub fn get_num_paths(&self) -> usize { self.paths.len() }
    pub fn get_path_capacity(&self, i: usize) -> u32 {
        self.paths.get(i).map_or(0, |p| p.capacity)
    }
    pub fn get_path_rtt_ms(&self, i: usize) -> u32 {
        self.paths.get(i).map_or(0, |p| p.rtt_ticks)
    }
    /// The RTT path i's sender believes (addendum 1 A3): measured srtt and
    /// rttvar, ms; and the number of samples taken.
    pub fn get_path_srtt_meas_ms(&self, i: usize) -> f64 {
        self.paths.get(i).map_or(f64::NAN, |p| p.srtt_meas * TICK_SECS * 1000.0)
    }
    pub fn get_path_rttvar_meas_ms(&self, i: usize) -> f64 {
        self.paths.get(i).map_or(f64::NAN, |p| p.rttvar_meas * TICK_SECS * 1000.0)
    }
    pub fn get_path_rtt_samples(&self, i: usize) -> u32 {
        self.paths.get(i).map_or(0, |p| p.rtt_samples)
    }
    /// Estimated per-path loss eps_hat_i (the striping input).
    pub fn get_path_eps_hat(&self, i: usize) -> f64 {
        self.paths.get(i).map_or(0.0, |p| p.estimator.loss_rate())
    }
    /// ESTIMATED per-path goodput g_i = capacity_i * (1 - eps_hat_i)
    /// (symbols/tick) — the live striping weight.
    pub fn get_path_goodput_est(&self, i: usize) -> f64 {
        self.paths.get(i).map_or(0.0, |p| p.goodput_est())
    }
    /// TRUE per-path goodput capacity_i * (1 - eps_i) (symbols/tick).
    pub fn get_path_goodput_true(&self, i: usize) -> f64 {
        self.paths.get(i).map_or(0.0, |p| p.goodput_true())
    }
    pub fn get_path_sent(&self, i: usize) -> u32 {
        self.paths.get(i).map_or(0, |p| p.sent)
    }
    pub fn get_path_arrived(&self, i: usize) -> u32 {
        self.paths.get(i).map_or(0, |p| p.arrived)
    }
    pub fn get_path_src(&self, i: usize) -> u32 {
        self.paths.get(i).map_or(0, |p| p.src)
    }
    pub fn get_path_fec(&self, i: usize) -> u32 {
        self.paths.get(i).map_or(0, |p| p.fec)
    }
    pub fn get_path_arq(&self, i: usize) -> u32 {
        self.paths.get(i).map_or(0, |p| p.arq)
    }
    pub fn get_path_lost(&self, i: usize) -> u32 {
        self.paths.get(i).map_or(0, |p| p.lost)
    }
    pub fn get_total_capacity(&self) -> u32 { self.total_capacity() }

    // --- Retention store + recovery clocks (walls #7/#8/#9) ---
    /// Path-scaled pool capacity STORE_PER_PATH × n_paths (wall #7,
    /// §6.1: the pool scales with paths; a per-transfer constant is a
    /// Little's-law wall under multipath).
    pub fn get_pool_cap(&self) -> u32 {
        STORE_PER_PATH * self.paths.len() as u32
    }
    /// Live SACK-clocked store occupancy (§6.3, the shipped law): slots
    /// held by un-SACKed sends plus unrecovered losses only.
    pub fn get_store_occupancy(&self) -> u32 { self.store_occupancy }
    /// Counterfactual frontier-clocked occupancy (wall #9): everything
    /// above the sender-known cumulative frontier —
    /// next_seq − frontier_acked. Always ≥ the SACK-clocked occupancy.
    pub fn get_store_occupancy_frontier(&self) -> u32 {
        self.next_seq.saturating_sub(self.frontier_acked)
    }
    /// Source slots stalled on a full pool (the wall-#7 signature).
    pub fn get_pool_stalls(&self) -> u32 { self.pool_stalls }
    /// Receiver's contiguous delivered/given-up prefix.
    pub fn get_frontier(&self) -> u32 { self.frontier }
    /// Holes currently held by their own path's RTT clock past the global
    /// (aggregate) clock — §7.1's suppressed spurious candidates, live.
    pub fn get_phantom_held(&self) -> u32 { self.phantom_held }
    /// Cumulative phantom retransmits avoided by per-path clocks (§7.1).
    pub fn get_phantom_avoided(&self) -> u32 { self.phantom_avoided }
    /// Measured AGGREGATE delivery goodput so far: decoded source symbols
    /// per tick. After completion this is the completion goodput
    /// num_source / completion_ticks.
    pub fn get_agg_goodput(&self) -> f64 {
        if self.tick == 0 { return 0.0; }
        self.cum_decoded as f64 / self.tick as f64
    }
    /// The BEST single path's TRUE goodput max_i capacity_i * (1 - eps_i)
    /// (symbols/tick): the §5.5 resequencing ceiling for every
    /// per-path-affine in-order transport on this path set.
    pub fn get_best_single_goodput(&self) -> f64 {
        self.paths.iter().map(|p| p.goodput_true()).fold(0.0, f64::max)
    }
    /// The §5.5 readout: measured aggregate goodput over the best
    /// single path's true goodput. > 1 means the shared window is
    /// delivering in-order faster than ANY single path could — the
    /// order-statistic aggregation (frontier rate -> sum g_i) made visible.
    /// (Includes ramp-up and drain, so it is an honest lower bound on the
    /// steady-state frontier-rate ratio.)
    pub fn get_aggregation_factor(&self) -> f64 {
        let best = self.get_best_single_goodput();
        if best <= 0.0 { return 0.0; }
        self.get_agg_goodput() / best
    }
    pub fn get_tick(&self) -> u32 { self.tick }
    pub fn get_src(&self) -> u32 { self.last_src }
    pub fn get_fec(&self) -> u32 { self.last_fec }
    pub fn get_arq(&self) -> u32 { self.last_arq }
    pub fn get_lost(&self) -> u32 { self.last_lost }
    pub fn get_cum_sent(&self) -> u32 { self.cum_sent }
    pub fn get_cum_arrived(&self) -> u32 { self.cum_arrived }
    pub fn get_cum_decoded(&self) -> u32 { self.cum_decoded }
    pub fn get_total_src(&self) -> u32 { self.total_src }
    pub fn get_total_fec(&self) -> u32 { self.total_fec }
    pub fn get_total_arq(&self) -> u32 { self.total_arq }
    pub fn get_total_lost(&self) -> u32 { self.total_lost }
    /// Live controller rate (the shared production formula's output).
    pub fn get_r_star(&self) -> f64 { self.rate }
    /// Reference: the closed-form continuous r* at the TRUE channel params
    /// (capacity-weighted aggregates for N > 1).
    pub fn get_r_star_auto(&self) -> f64 {
        let eps = self.agg_eps();
        math::compute_r_star_with_z(
            eps, self.sigma2_true, self.w as f64,
            math::z_for_tail_target(self.tail_target, eps),
        )
    }
    pub fn get_sigma2(&self) -> f64 { self.sigma2_true }
    pub fn get_retx_buf_size(&self) -> u32 { self.lost_pending }
    pub fn get_num_source(&self) -> u32 { self.num_source }
    /// Capacity-weighted aggregate loss estimate across paths.
    pub fn get_estimated_loss(&self) -> f64 { self.agg_loss_est() }
    /// Conservative loss estimate the controller actually uses (BOCD 95%,
    /// capacity-weighted across paths).
    pub fn get_p_upper(&self) -> f64 { self.agg_p_upper() }
    /// Estimated burst variance factor (capacity-weighted across the live
    /// per-path GE estimators; cold paths contribute 1.0).
    pub fn get_sigma2_est(&self) -> f64 {
        self.agg(|p| {
            let ge = p.estimator.ge_estimator();
            if ge.is_valid() {
                math::burst_variance_factor(ge.p_gb(), ge.p_bg())
            } else {
                1.0
            }
        })
    }
    /// Effective tail target readout (display only; the executed law is the
    /// rate-domain mix): the log-blend of the anchor tail with §4.5's
    /// late-is-fine glide p̂ + (0.05 − p̂)·χ, weighted by β.
    pub fn get_delta_eff(&self) -> f64 {
        self.continuum_tail(self.get_p_upper())
    }
    /// Live completion exposure chi (§4.6): 0 mid-stream, ramps to 1
    /// over the final ~1.5 SRTT of the transfer. This is the raw χ the
    /// late-is-fine term of the mix reads at every dial position (weighted
    /// by β inside the rate); the source-position completion term (§3.3) is
    /// a separate metering.
    pub fn get_completion_exposure(&self) -> f64 {
        self.completion_chi_raw()
    }
    /// Saturation cap for the current estimator state (§4.4).
    pub fn get_r_sat(&self) -> f64 {
        math::r_saturation(
            self.get_p_upper(),
            self.get_sigma2_est(),
            self.encoder.window_size().max(1) as f64,
            self.agg_srtt(),
            TICK_SECS / self.total_capacity() as f64,
        )
    }
    /// Derived encoder window W* (§4.8) for the current estimator state
    /// and hint tail target — the window the controller WOULD choose. The UI
    /// shows this next to the user's slider W so the tradeoff is visible:
    /// larger W thins the r* overhead margin (1/sqrt(W)) but stretches
    /// recovery latency (W/send_rate). send_rate = capacity / tick.
    pub fn get_derived_w(&self) -> f64 {
        math::derive_window(
            self.tail_target,
            self.get_p_upper().max(1e-6),
            self.get_sigma2_est(),
            self.agg_srtt(),
            self.total_capacity() as f64 / TICK_SECS, // source symbols per second
            0.0, // budget = 0 => align to ~1 RTT
        )
    }

    /// Saturation pressure in [0, 1] (§4.4): a continuous indicator of
    /// how hard the soft cap binds. 0 = far below
    /// the cap (more FEC still helps), 0.5 = exactly at r_sat, ->1 = the cap
    /// is holding r near r_sat. Computed from the UNCAPPED request (what the
    /// hint asked for before the soft cap) vs r_sat.
    pub fn get_saturation_pressure(&self) -> f64 {
        if self.fixed_r.is_some() {
            return 0.0;
        }
        let uncapped = self.rate_now(false);
        math::saturation_pressure(uncapped, self.get_r_sat())
    }
    /// Symbols permanently given up (rho < 1.0 age eviction).
    pub fn get_given_up(&self) -> u32 { self.given_up }
    /// Latency (ms) of the most recently delivered source symbol.
    pub fn get_lat_last(&self) -> f64 {
        self.delivered_lat_ms.last().copied().unwrap_or(0.0)
    }
    /// Mean delivery latency (ms) over all delivered source symbols.
    pub fn get_lat_avg(&self) -> f64 {
        if self.delivered_lat_ms.is_empty() { return 0.0; }
        self.delivered_lat_ms.iter().sum::<f64>() / self.delivered_lat_ms.len() as f64
    }
    /// Delivery latency percentile (ms), pct in [0, 1].
    pub fn get_lat_percentile(&self, pct: f64) -> f64 {
        if self.delivered_lat_ms.is_empty() { return 0.0; }
        let mut v = self.delivered_lat_ms.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = ((v.len() as f64 - 1.0) * pct.clamp(0.0, 1.0)).round() as usize;
        v[idx.min(v.len() - 1)]
    }
    /// RFC 3550-style smoothed delivery jitter (ms).
    pub fn get_jitter(&self) -> f64 { self.jitter_ms }
    /// Achieved reliability so far: delivered / sent source symbols.
    pub fn get_reliability(&self) -> f64 {
        if self.total_src > 0 {
            self.cum_decoded as f64 / self.total_src as f64
        } else {
            1.0
        }
    }
    /// Configured reliability target rho.
    pub fn get_rho(&self) -> f64 { self.rho }
    // --- Three knobs (proposal; plan §4.3). Readouts only, none fed back. ---
    /// The declared lifetime T, seconds (routing witness for the page gate).
    pub fn get_lifetime_s(&self) -> f64 { self.lifetime_s }
    /// The declared order hold H, seconds.
    pub fn get_hold_s(&self) -> f64 { self.hold_s }
    /// The application hand-over rate λ, units/s.
    pub fn get_source_rate(&self) -> f64 { self.source_rate }
    /// Units dropped at the intake by L1 (age ≥ T).
    pub fn get_intake_expired(&self) -> u32 { self.intake_expired }
    /// Units dropped at the dequeue (§13.2: age + R ≥ T, a younger unit waits).
    pub fn get_intake_expired_deq(&self) -> u32 { self.intake_expired_deq }
    /// Units that have left the intake (sent or expired).
    pub fn get_units_out(&self) -> u32 { self.u_next }
    /// The floor F_ann, the sender's ack frontier A_known, and the floor the
    /// receiver holds.
    pub fn get_f_ann(&self) -> u32 { self.f_ann }
    pub fn get_a_known(&self) -> u32 { self.a_known }
    pub fn get_f_rx(&self) -> u32 { self.f_rx }
    /// Lost units passed by the floor and never decoded.
    pub fn get_passed_undelivered(&self) -> u32 { self.passed_undelivered }
    /// Copies staged below the floor and payload datagrams with age ≥ T:
    /// both must read 0 (the absolute tests read the same counters).
    pub fn get_commit_viol(&self) -> u32 { self.commit_viol }
    pub fn get_l1_viol(&self) -> u32 { self.l1_viol }
    /// Candidates the re-check before a copy skipped (§17.1).
    pub fn get_skip_below_floor(&self) -> u32 { self.skip_below_floor }
    /// Floor-repeat datagrams, and arrivals that raised the receiver's floor.
    pub fn get_floors(&self) -> u32 { self.floors }
    pub fn get_floor_carried(&self) -> u32 { self.floor_carried }
    /// Bind gauge of the L1 age edge: the fraction of repairs whose window
    /// it cut (§2.4 of the derivation).
    pub fn get_span_cut_by_t(&self) -> f64 {
        self.span_cut_repairs as f64 / self.total_fec.max(1) as f64
    }
    /// Delivered units over all units (intake expiries in the denominator):
    /// "delivered at all", the reported ρ.
    pub fn get_share_any(&self) -> f64 {
        let n = (0..self.seq_unit.len())
            .filter(|&s| !self.consumer.delivered_at(s).is_nan())
            .count();
        n as f64 / self.num_source as f64
    }
    /// Units delivered within T of hand-over (L3), over all units.
    pub fn get_share_t(&self) -> f64 {
        self.share_within(self.lifetime_s)
    }
    /// The same at the reference deadline τ = 0.4 s (§13.6; Q-V3).
    pub fn get_share_by_ref(&self) -> f64 {
        self.share_within(REF_DEADLINE_S)
    }
    /// Delay from hand-over at the consumer (s), over all units: an
    /// undelivered unit's delay is ∞, so a quantile past the delivered
    /// share reads ∞.
    pub fn get_delay_quantile(&self, p: f64) -> f64 {
        consumer::quantile(&self.unit_delays(), p)
    }
    /// Units delivered after a higher seq was delivered (the toy's `ooo_loss`).
    pub fn get_ooo(&self) -> u32 { self.consumer.ooo }
    /// Out-of-order deliveries over delivered units (0 when none delivered).
    pub fn get_ooo_share(&self) -> f64 {
        let delivered = (0..self.seq_unit.len())
            .filter(|&s| !self.consumer.delivered_at(s).is_nan())
            .count();
        self.consumer.ooo as f64 / delivered.max(1) as f64
    }
    /// Per-unit out-of-order marker for drawing (the toy's definition: the
    /// unit reached the consumer after a higher-numbered one), indexed by
    /// unit; units with no seq (intake expiries) read false.
    pub fn get_ooo_units(&self) -> Vec<u8> {
        let mut v = vec![0u8; self.num_source as usize];
        for (s, &u) in self.seq_unit.iter().enumerate() {
            v[u as usize] = self.consumer.out_of_order(s) as u8;
        }
        v
    }
    /// Per-unit consumer delivery time minus hand-over plus ô (s), ∞ when
    /// not delivered: the drawing's companion to `get_ooo_units`.
    pub fn get_unit_delays(&self) -> Vec<f64> { self.unit_delays() }
    /// Per-unit delivered marker (1 = the consumer delivered it), so the
    /// page classifies units without testing for ∞.
    pub fn get_delivered_units(&self) -> Vec<u8> {
        let mut v = vec![0u8; self.num_source as usize];
        for (s, &u) in self.seq_unit.iter().enumerate() {
            v[u as usize] = (!self.consumer.delivered_at(s).is_nan()) as u8;
        }
        v
    }
    /// The class of every unit (CLASS_* codes), the full array for tests.
    pub fn get_unit_classes(&self) -> Vec<u8> {
        (0..self.num_source as usize).map(|u| self.unit_class(u).0).collect()
    }
    /// Units per class, indexed by the CLASS_* code.
    pub fn get_class_counts(&self) -> Vec<u32> {
        let mut n = vec![0u32; N_CLASSES];
        for u in 0..self.num_source as usize {
            n[self.unit_class(u).0 as usize] += 1;
        }
        n
    }
    /// Units both out of order and late: classed out of order (A1's
    /// precedence), counted here so nothing is hidden.
    pub fn get_ooo_late(&self) -> u32 {
        (0..self.num_source as usize)
            .filter(|&u| {
                let (c, d) = self.unit_class(u);
                c == CLASS_OUT_OF_ORDER && !knobs::within_lifetime(self.lifetime_s, d)
            })
            .count() as u32
    }
    /// Delivered units with delay from hand-over > T (out of order or not),
    /// over delivered units (0 when none delivered).
    pub fn get_late_share(&self) -> f64 {
        let (mut late, mut delivered) = (0u32, 0u32);
        for u in 0..self.num_source as usize {
            let (c, d) = self.unit_class(u);
            if c == CLASS_ON_TIME || c == CLASS_LATE || c == CLASS_OUT_OF_ORDER {
                delivered += 1;
                late += !knobs::within_lifetime(self.lifetime_s, d) as u32;
            }
        }
        late as f64 / delivered.max(1) as f64
    }
    /// Holes the consumer released and that were filled later (delivered out
    /// of order at once, O3 = yes).
    pub fn get_late_fills(&self) -> u32 { self.consumer.late_fills }
    /// Copies that reached a receiver already holding their seq (addendum 1 A2).
    pub fn get_dup(&self) -> u32 { self.dup }
    /// The forward leg of path i (addendum 1 A2; routing witnesses): service
    /// time, queue limit and propagation delay, seconds.
    pub fn get_link_service_s(&self, i: usize) -> f64 { self.paths.get(i).map_or(f64::NAN, |p| p.link_sv) }
    pub fn get_link_queue_s(&self, i: usize) -> f64 { self.paths.get(i).map_or(f64::NAN, |p| p.link_q) }
    pub fn get_link_prop_s(&self, i: usize) -> f64 { self.paths.get(i).map_or(f64::NAN, |p| p.link_d) }
    /// Data datagrams path i's queue dropped, those sent inside the outage
    /// window and outside it (addendum 1 A4).
    pub fn get_path_qdrop(&self, i: usize) -> u32 { self.paths.get(i).map_or(0, |p| p.qdrop) }
    pub fn get_path_qdrop_in(&self, i: usize) -> u32 { self.paths.get(i).map_or(0, |p| p.qdrop_in) }
    pub fn get_path_qdrop_out(&self, i: usize) -> u32 { self.paths.get(i).map_or(0, |p| p.qdrop - p.qdrop_in) }
    /// Path i's one-way delay of arrived data datagrams (arrival − send +
    /// rtt/2 − d, ms): the last value and a quantile (NaN before any).
    pub fn get_path_owd_last_ms(&self, i: usize) -> f64 {
        self.paths.get(i).and_then(|p| p.owd_ms.last().copied()).unwrap_or(f64::NAN)
    }
    pub fn get_path_owd_quantile_ms(&self, i: usize, q: f64) -> f64 {
        self.paths.get(i).map_or(f64::NAN, |p| consumer::quantile(&p.owd_ms, q))
    }
    /// Path i's link use: the server's busy time over the elapsed time.
    pub fn get_path_link_use(&self, i: usize) -> f64 {
        self.paths.get(i).map_or(0.0, |p| p.link_busy / (self.tick.max(1) as f64 * TICK_SECS))
    }
    /// Arrivals below a seq that already arrived on another path.
    pub fn get_cross_path_reorder(&self) -> u32 { self.cross_reorder }
    /// Ticks the receiver-side drain ran after the sender finished (0 when
    /// the consumer held nothing at the finish), and units the consumer still
    /// holds (the `held` class at the end).
    pub fn get_drain_ticks(&self) -> u32 { self.drain_ticks }
    pub fn get_consumer_held(&self) -> u32 { self.consumer.held() as u32 }
    /// Each path's backlog (ms) at the end of every tick since the last call,
    /// tick-major (`get_num_paths` values per tick): the page appends them.
    pub fn take_queue_samples(&mut self) -> Vec<f64> { std::mem::take(&mut self.queue_pending) }
    /// Datagrams in flight on every forward leg now.
    pub fn get_in_flight(&self) -> u32 { self.in_flight() as u32 }
    /// The resend budget per 5 ms window (∞ = unlimited).
    pub fn get_resend_limit(&self) -> f64 { self.resend_limit }
    /// Resends per 5 ms window, from tick 0 (addendum 1 A5; the page's bars).
    pub fn get_budget_used(&self) -> Vec<u32> { self.budget_used.clone() }
    /// Distinct seqs the budget held back at least once (the headline), and
    /// the refusals (every slot that picked a held-back seq again).
    pub fn get_resend_braked_units(&self) -> u32 { self.braked_units }
    pub fn get_resend_braked_attempts(&self) -> u32 { self.braked_attempts }
    /// The knowledge lead (addendum 1 A2 gauge): the reverse-leg delay the
    /// sender skips by learning a resend's arrival at the arrival tick, mean
    /// and maximum over resends, ms (0 before any resend arrived).
    pub fn get_knowledge_lead_mean_ms(&self) -> f64 {
        self.lead_sum as f64 / self.lead_n.max(1) as f64 * TICK_SECS * 1000.0
    }
    pub fn get_knowledge_lead_max_ms(&self) -> f64 { self.lead_max as f64 * TICK_SECS * 1000.0 }
    /// ô (s), the one-way term L2 and `protected` read, and the delay
    /// accounting's physical term (s) every delay readout adds (addendum 1 A3).
    pub fn get_o_hat_s(&self) -> f64 { self.one_way_s() }
    /// The ack timeout R the model runs (s), and R with the measured rttvar
    /// (addendum 1 A3's form, held; see `ack_timeout_s`).
    pub fn get_ack_timeout_s(&self) -> f64 { self.ack_timeout_s() }
    pub fn get_ack_timeout_meas_s(&self) -> f64 { self.ack_timeout_meas_s() }
    pub fn get_acc_s(&self) -> f64 { self.acc_s() }
    /// L2's outcome: the share of arrived resends that arrived later than T
    /// after hand-over (0 before any arrived).
    pub fn get_l2_late_share(&self) -> f64 {
        self.l2_late as f64 / self.resends_arrived.max(1) as f64
    }
    /// The units whose class changed since the last call, as flat quadruples
    /// `[unit, hand-over s, delay s (∞ when not delivered), class]`. A unit
    /// appears when it is delivered, expires at the intake, or is passed or
    /// given up; an undelivered unit filled later appears once more. The page
    /// appends these and derives nothing.
    pub fn take_unit_events(&mut self) -> Vec<f64> {
        let dirty = std::mem::take(&mut self.unit_dirty);
        let mut out = Vec::new();
        for u in dirty {
            let u = u as usize;
            let (c, d) = self.unit_class(u);
            if c != self.unit_emitted[u] {
                self.unit_emitted[u] = c;
                out.extend([u as f64, self.handover_s(u as u32), d, c as f64]);
            }
        }
        out
    }
    /// L6's bind gauge: holes released by the hold term and by the floor term.
    pub fn get_rel_by_hold(&self) -> u32 { self.consumer.rel_by_hold }
    pub fn get_rel_by_floor(&self) -> u32 { self.consumer.rel_by_floor }
    pub fn get_released_holes(&self) -> u32 { self.consumer.released_holes() }
    /// Recovery time after the outage (§17.4 windows over hand-over time,
    /// on time = within τ), seconds after the outage end; NaN = not within
    /// the run (also when the run is too short to read).
    pub fn get_recovery_time(&self) -> f64 {
        let delays = self.unit_delays();
        let h: Vec<f64> = (0..self.num_source).map(|u| self.handover_s(u)).collect();
        let on: Vec<bool> =
            delays.iter().map(|&d| knobs::within_lifetime(REF_DEADLINE_S, d)).collect();
        let p = &self.paths[0];
        consumer::recovery_time(&h, &on, p.outage_start_s, p.outage_start_s + p.outage_len_s)
    }
    /// The longest consumer stall that overlaps the outage, minus the
    /// outage length (§13.5), seconds; NaN when no stall overlaps it.
    pub fn get_strict_stall_beyond_outage(&self) -> f64 {
        let p = &self.paths[0];
        let (os, oe) = (p.outage_start_s, p.outage_start_s + p.outage_len_s);
        self.consumer
            .stall_episodes(self.now_s())
            .iter()
            .filter(|(t0, len)| *t0 < oe && t0 + len > os)
            .map(|(_, len)| len - p.outage_len_s)
            .fold(f64::NAN, f64::max)
    }
    /// The law revision the model implements.
    pub fn get_knob_law_revision(&self) -> String { knobs::KNOB_LAW_REVISION.to_string() }
    /// The residual R after in-window FEC at the live operating point, over
    /// the sim's own window W (`shed_budget_residual`; provisional until the
    /// sliding per-loss P_fec lands, W22).
    pub fn get_giveup_residual(&self) -> f64 {
        shed_budget_residual(
            self.agg_loss_est().clamp(0.0, 0.99), self.rate,
            self.encoder.window_size().max(1) as f64, self.get_sigma2_est(),
        )
    }
    /// The give-up budget min(1 − ρ, R), a fraction of src_total.
    pub fn get_giveup_budget(&self) -> f64 {
        giveup_budget(self.rho, self.get_giveup_residual())
    }
    /// The give-up deadline D(δ) = min(b(δ)·RTT, 2·RTT), in ticks.
    pub fn get_giveup_deadline(&self) -> f64 {
        span_deadline_d(self.dprice, self.agg_rtt_ticks() as f64)
    }
    /// The channel's information-theoretic overhead floor: with loss rate
    /// eps, EVERY reliable scheme must send at least 1/(1-eps) symbols per
    /// delivered symbol -- a floor of eps/(1-eps) extra, no matter how
    /// clever. As a percentage of source symbols.
    pub fn get_overhead_floor(&self) -> f64 {
        let eps = self.agg_eps();
        if eps < 1.0 {
            eps / (1.0 - eps) * 100.0
        } else {
            0.0
        }
    }
    /// The REALIZED channel floor for this specific run: over the wire slots
    /// actually consumed, the loss rate was (sent - arrived)/sent, which
    /// differs from the nominal eps because a finite run samples only a
    /// prefix of the (expectation-calibrated) channel. The nominal floor is
    /// unbeatable only in expectation; this one is what THIS channel forced.
    /// As a percentage of delivered symbols: lost/arrived = eps_real/(1-eps_real).
    pub fn get_overhead_floor_realized(&self) -> f64 {
        if self.cum_arrived > 0 {
            (self.cum_sent.saturating_sub(self.cum_arrived)) as f64 / self.cum_arrived as f64
                * 100.0
        } else {
            0.0
        }
    }
    /// Excess overhead: what we actually spent BEYOND the channel floor THIS
    /// run forced -- the honest inefficiency measure (0% = information-
    /// theoretically perfect; the floor itself is the channel's fault, not
    /// the protocol's). Measured against the REALIZED floor so it is a true
    /// invariant: overhead (all sends) >= realized floor always, because
    /// decoding N source symbols needs >= N arrived symbols, giving
    /// overhead - floor = sent*(arrived - N)/(N*arrived) >= 0.
    pub fn get_excess_overhead(&self) -> f64 {
        (self.get_overhead() - self.get_overhead_floor_realized()).max(0.0)
    }
    /// Total overhead over ALL sends (steady stream AND post-stream drain):
    /// (FEC + ARQ) / source symbols, in percent. Counting only the steady
    /// phase would undercount, because corrections
    /// fired during the drain -- the tail-FEC burst and the final ARQ
    /// retransmits -- are real bandwidth and can push realized loss recovery
    /// below the floor's whole-transfer semantics.
    pub fn get_overhead(&self) -> f64 {
        if self.total_src > 0 {
            (self.total_fec + self.total_arq) as f64 / self.total_src as f64 * 100.0
        } else {
            0.0
        }
    }
    pub fn get_recovery(&self) -> f64 {
        if self.total_src > 0 {
            self.cum_decoded as f64 / self.total_src as f64 * 100.0
        } else {
            100.0
        }
    }
}

#[cfg(test)]
mod warm_scan;

#[cfg(test)]
mod knob_sim_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// N = 1 identical-behavior regression: golden fingerprints pin the
    /// current model era. Both the legacy constructor and the multipath
    /// constructor with one path must reproduce them exactly — a refactor
    /// may not perturb single-path behavior by one tick or one symbol.
    /// The retransmit clock is the per-path RFC 9002 law (§7.1: first
    /// eligibility = send + own-path RTT). To re-capture after a deliberate
    /// model change: `GOLDEN_CAPTURE=1 cargo test -p raptorpath-wasm
    /// test_multipath_n1_identical_golden -- --nocapture`.
    #[test]
    fn test_multipath_n1_identical_golden() {
        // (hint, eps, q, rtt) -> (ticks, src, fec, arq, lost, decoded, sent, arrived)
        let goldens: [(&str, f64, f64, u32, [u32; 8]); 6] = [
            ("auto",     0.05, 0.5, 50, [667, 2000, 668,  0,   101, 2000, 2668, 2532]),
            ("auto",     0.10, 0.3, 80, [747, 2000, 985,  0,   185, 2000, 2985, 2705]),
            ("bulk",     0.05, 0.5, 50, [569, 2000, 0,    116, 113, 2000, 2116, 2000]),
            ("bulk",     0.10, 0.3, 80, [647, 2000, 85,   188, 198, 2000, 2273, 2059]),
            ("realtime", 0.05, 0.5, 50, [733, 2000, 710,  3,   101, 2000, 2713, 2571]),
            ("realtime", 0.10, 0.3, 80, [754, 2000, 1015, 0,   184, 2000, 3015, 2734]),
        ];
        for (hint, eps, q, rtt, g) in goldens {
            let mut arms = [
                Simulation::new(eps, q, rtt, 64, hint.into(), None, None, None),
                Simulation::multipath(
                    vec![eps], vec![q], vec![rtt], vec![4], 64, hint.into(),
                    None, None, None,
                ),
            ];
            for (ai, s) in arms.iter_mut().enumerate() {
                while !s.is_finished() && s.get_tick() < 20_000 { s.step(); }
                let got = [
                    s.get_tick(), s.get_total_src(), s.get_total_fec(), s.get_total_arq(),
                    s.get_total_lost(), s.get_cum_decoded(), s.get_cum_sent(),
                    s.get_cum_arrived(),
                ];
                println!("golden {hint} eps={eps} q={q} rtt={rtt} arm{ai}: {got:?}");
                if std::env::var("GOLDEN_CAPTURE").is_err() { assert_eq!(
                    got, g,
                    "N=1 regression (arm {ai}) {hint} eps={eps} q={q} rtt={rtt}: \
                     [ticks,src,fec,arq,lost,decoded,sent,arrived] diverged from \
                     the pre-multipath engine"
                ); }
            }
        }
    }

    /// ADR-0079 / S06, bounding test for the documented model-vs-engine
    /// divergence (paper §11.3, "reactive retransmit trigger in the wasm
    /// model"). The model's P_lost retransmit is its whole REACTIVE plane
    /// (NACK and refresh merged): its candidates are oracle-lost symbols
    /// (`lost && !recovered`) cooled per symbol by the `rtt_ticks` of the
    /// path they were last sent on. It is not a copy branch: the engine
    /// deleted its copy of a still-in-flight seq; the model never had one.
    /// Bound, stated absolutely: at most one retransmit of a symbol per
    /// `rtt_ticks` of the path the PREVIOUS send went out on (a resend sets
    /// `sym.path = dest`, so the next eligibility is read against that
    /// path), and none of a recovered symbol. Green on main by construction:
    /// a bounding test, not a red-first one.
    #[test]
    fn model_retransmit_is_reactive_and_bounded() {
        use std::collections::HashMap;
        // (label, eps, q, rtt_ms, capacity) per path.
        let scenarios: [(&str, &str, Vec<f64>, Vec<f64>, Vec<u32>, Vec<u32>); 3] = [
            ("N=1 bulk", "bulk", vec![0.10], vec![0.3], vec![80], vec![4]),
            ("N=2 bulk", "bulk", vec![0.05, 0.10], vec![0.5, 0.3], vec![30, 80], vec![4, 4]),
            ("N=2 bulk, skewed", "bulk", vec![0.10, 0.10], vec![0.3, 0.3], vec![40, 120], vec![4, 4]),
        ];
        for (label, hint, eps, q, rtt, cap) in scenarios {
            let mut s = Simulation::multipath(
                eps, q, rtt, cap, 64, hint.into(), None, None, None,
            );
            let mut observed = 0u64;
            while !s.is_finished() && s.get_tick() < 20_000 {
                // seq -> (last_retx_tick, path, recovered) before the tick.
                let before: HashMap<u64, (i64, usize, bool)> = s
                    .symbols
                    .iter()
                    .map(|x| (x.seq, (x.last_retx_tick, x.path, x.recovered)))
                    .collect();
                s.step();
                for x in &s.symbols {
                    let Some(&(prev_tick, prev_path, was_recovered)) = before.get(&x.seq)
                    else {
                        continue;
                    };
                    if x.last_retx_tick == prev_tick {
                        continue;
                    }
                    observed += 1;
                    assert!(
                        !was_recovered,
                        "{label}: seq {} retransmitted after it was recovered",
                        x.seq
                    );
                    let gap = x.last_retx_tick - prev_tick;
                    let rtt = s.paths[prev_path].rtt_ticks as i64;
                    assert!(
                        gap >= rtt,
                        "{label}: seq {} retransmitted {gap} ticks after its previous send                          on path {prev_path} (rtt_ticks {rtt})",
                        x.seq
                    );
                }
            }
            println!(
                "model_retransmit_is_reactive_and_bounded {label}: total_arq={} observed={observed}",
                s.get_total_arq()
            );
            // Witness: the mechanism under test executed.
            assert!(s.get_total_arq() > 0, "{label}: no retransmit happened");
            assert!(observed > 0, "{label}: no retransmit was observed");
            assert!(observed <= s.get_total_arq() as u64, "{label}: observed more than sent");
        }
    }

    /// The tail-rate memo is pure: every cache hit equals a fresh solve to
    /// the bit, and solves = runs of identical consecutive keys (a solve
    /// happens at least once per key change, never per source symbol), and
    /// the DP runs per solve are bounded (every solve is also checked
    /// bit-for-bit against the cold solver by `warm_tail_solve`).
    #[test]
    fn test_r_tail_cache_is_pure_and_bounds_solves() {
        for (w, d) in [(64u32, 0.005), (64, 0.5), (64, 50.0), (25, 0.5)] {
            let mut s = Simulation::multipath(
                vec![0.05], vec![0.5], vec![50], vec![4], w, "continuum".into(),
                None, Some(d), Some(1.0),
            );
            s.verify_r_tail_cache = true;
            while !s.is_finished() && s.get_tick() < 20_000 { s.step(); }
            let calls = s.r_tail_log.len();
            let runs = 1 + s.r_tail_log.windows(2).filter(|p| p[0] != p[1]).count();
            assert!(calls > 0, "W={w} d={d}: tail never solved");
            assert_eq!(s.r_tail_solves as usize, runs, "W={w} d={d}");
            assert!(s.r_tail_solves as usize <= calls);
            // DP work: a warm solve gallops from the previous k* then bisects,
            // so at most 2*ceil(log2(2W+1)) + 3 distinct fail(k) evaluations;
            // a memo hit costs none.
            let per_solve = 2 * ((2 * w as usize + 1).next_power_of_two().trailing_zeros() as usize) + 3;
            assert!(
                s.r_tail_dp_runs <= s.r_tail_solves as usize * per_solve,
                "W={w} d={d}: {} DP runs over {} solves exceeds {per_solve}/solve",
                s.r_tail_dp_runs, s.r_tail_solves
            );
            println!(
                "W={w} d={d}: {calls} calls -> {} solves, {} DP runs ({:.2}/solve, {:.2}/call; bound {per_solve}/solve)",
                s.r_tail_solves, s.r_tail_dp_runs,
                s.r_tail_dp_runs as f64 / s.r_tail_solves as f64, s.r_tail_dp_runs as f64 / calls as f64
            );
        }
    }

    /// §5.5 at L0: two symmetric paths through one shared
    /// window must complete ~2x faster than one path (minus the drain tail,
    /// which is RTT-bound, not capacity-bound).
    #[test]
    fn test_multipath_symmetric_aggregation() {
        for hint in ["bulk", "auto"] {
            let mut single = Simulation::new(0.05, 0.5, 50, 64, hint.into(), None, None, None);
            let mut dual = Simulation::multipath(
                vec![0.05, 0.05], vec![0.5, 0.5], vec![50, 50], vec![4, 4],
                64, hint.into(), None, None, None,
            );
            run_to_end(&mut single);
            run_to_end(&mut dual);
            assert_eq!(single.get_cum_decoded(), single.get_num_source());
            assert_eq!(dual.get_cum_decoded(), dual.get_num_source());
            // rho = 1 contract: full retention, NOTHING is ever given up.
            assert_eq!(dual.get_given_up(), 0, "[{hint}] rho=1 must never give up");
            let factor = single.get_tick() as f64 / dual.get_tick() as f64;
            println!(
                "symmetric 2-path [{hint}]: single {} ticks, dual {} ticks -> speedup x{:.2} \
                 (agg factor accessor: x{:.2})",
                single.get_tick(), dual.get_tick(), factor, dual.get_aggregation_factor()
            );
            // Gate 1.6–2.1: the drain tail is RTT-bound, and under the
            // per-path RFC 9002 retransmit clock (§7.1) a drain straggler
            // costs a full own-RTT detection round — the dual run pays it
            // on a shorter total, so the ratio sits somewhat below 2.
            assert!(
                factor > 1.6 && factor <= 2.1,
                "[{hint}] symmetric aggregation x{factor:.2} outside ~1.6-2x"
            );
            // Striping is goodput-proportional: symmetric paths carry ~equal
            // source shares.
            let s0 = dual.get_path_src(0) as f64;
            let s1 = dual.get_path_src(1) as f64;
            assert!(
                (s0 / (s0 + s1) - 0.5).abs() < 0.05,
                "[{hint}] symmetric striping skewed: {s0} vs {s1}"
            );
        }
    }

    /// The reliability contract is CONFIGURABLE via rho on multipath too
    /// (§3.5): rho < 1 gives up losses older than D(δ) within min(1-rho, R) (counted,
    /// receiver-pruned), rho = 1 retains until acked — same triangle
    /// semantics as single-path, now across N paths.
    #[test]
    fn test_multipath_custom_rho_semantics() {
        // rho < 1: bounded retention — give-ups counted, reliability >= target.
        let mut lossy = Simulation::multipath(
            vec![0.10, 0.10], vec![0.3, 0.3], vec![80, 80], vec![4, 4],
            64, "continuum".into(), None, Some(0.005), Some(0.95),
        );
        run_to_end(&mut lossy);
        assert_eq!(
            lossy.get_cum_decoded() + lossy.get_given_up(),
            lossy.get_num_source(),
            "every source symbol is either delivered or explicitly given up"
        );
        assert!(
            lossy.get_reliability() >= 0.90,
            "reliability {}",
            lossy.get_reliability()
        );
        // rho = 1 (same channels): ack-only retention — NEVER gives up.
        let mut full = Simulation::multipath(
            vec![0.10, 0.10], vec![0.3, 0.3], vec![80, 80], vec![4, 4],
            64, "continuum".into(), None, Some(0.005), Some(1.0),
        );
        run_to_end(&mut full);
        assert_eq!(full.get_given_up(), 0, "rho=1 must never give up");
        assert_eq!(full.get_cum_decoded(), full.get_num_source());
    }

    /// §5.5 heterogeneous-cell claim at L0 (c8-like heterogeneity, scaled
    /// to sim units: 100 Mbit/10 ms/2.6% -> cap 5, and 20 Mbit/40 ms/4.8%
    /// -> cap 1): aggregate goodput of the shared window over both paths
    /// must be strictly greater than the fast path alone — the §5.5 resequencing
    /// ceiling of every per-path-affine in-order transport. The expected
    /// factor is ~(g_A+g_B)/g_A ~ 1.20 at this heterogeneity.
    #[test]
    fn test_multipath_heterogeneous_beats_fast_path_alone() {
        for hint in ["bulk", "auto"] {
            let mut fast_alone = Simulation::multipath(
                vec![0.026], vec![0.5], vec![10], vec![5],
                64, hint.into(), None, None, None,
            );
            let mut rwm = Simulation::multipath(
                vec![0.026, 0.048], vec![0.5, 0.5], vec![10, 40], vec![5, 1],
                64, hint.into(), None, None, None,
            );
            run_to_end(&mut fast_alone);
            run_to_end(&mut rwm);
            assert_eq!(fast_alone.get_cum_decoded(), fast_alone.get_num_source());
            assert_eq!(rwm.get_cum_decoded(), rwm.get_num_source());
            let factor = fast_alone.get_tick() as f64 / rwm.get_tick() as f64;
            println!(
                "heterogeneous C8-like [{hint}]: fast-alone {} ticks, RWM {} ticks -> x{:.3} \
                 (P1 pass line: > 1.0; sum-goodput asymptote ~1.20) | slow-path share: \
                 src {}/{} | agg-factor accessor x{:.2}",
                fast_alone.get_tick(), rwm.get_tick(), factor,
                rwm.get_path_src(1), rwm.get_total_src(), rwm.get_aggregation_factor()
            );
            assert!(
                factor > 1.0,
                "[{hint}] P1 FAILED at L0: adding the slow path made completion SLOWER \
                 (x{factor:.3}) — the RWM aggregation claim does not survive \
                 the sim; investigate before building production code"
            );
        }
    }

    #[test]
    fn test_channel_generation_loss_rate() {
        let states = generate_channel_states(0.0556, 0.5, 0.10, 4000, 42);
        let losses = states.iter().filter(|&&l| l).count();
        let rate = losses as f64 / states.len() as f64;
        assert!((rate - 0.10).abs() < 0.01, "Expected ~10% loss, got {:.1}%", rate * 100.0);
    }

    #[test]
    fn test_simulation_runs_to_completion_auto() {
        let mut sim = Simulation::new(0.05, 0.5, 50, 64, "auto".into(), None, None, None);
        while !sim.is_finished() && sim.get_tick() < 20_000 {
            sim.step();
        }
        assert!(sim.is_finished());
        assert_eq!(sim.get_cum_decoded(), sim.get_num_source(),
            "all source symbols must be delivered");
        // Auto at 5% loss should carry meaningful FEC
        assert!(sim.get_total_fec() > 0, "Auto must send repairs");
    }

    // §4.8 quality check: derived W vs a fixed W = 64 across the three
    // reference channels. Runs the SAME simulator both ways and reports
    // overhead (FEC/source) and p99 delivery latency. Prints a table with
    // `--nocapture`; asserts the derived window never loses on overhead where
    // the 1/sqrt(W) margin is fattest (Satellite) while staying complete.
    #[test]
    fn test_derived_window_quality_vs_fixed64() {
        struct Ch { name: &'static str, eps: f64, q: f64, rtt: u32 }
        let channels = [
            Ch { name: "DC",        eps: 0.001, q: 0.5, rtt: 5 },
            Ch { name: "WiFi",      eps: 0.025, q: 0.5, rtt: 13 },
            Ch { name: "Satellite", eps: 0.09,  q: 0.3, rtt: 210 },
        ];
        // Hint "auto" => tail_target = BASE_TAIL_TARGET. send_rate = capacity/tick.
        let delta = BASE_TAIL_TARGET;
        let send_rate = 4.0 / TICK_SECS;

        let run = |eps: f64, q: f64, rtt: u32, w: u32| -> (f64, f64, bool) {
            let mut sim = Simulation::new(eps, q, rtt, w, "auto".into(), None, None, None);
            while !sim.is_finished() && sim.get_tick() < 200_000 { sim.step(); }
            let overhead = sim.get_total_fec() as f64 / sim.get_total_src().max(1) as f64;
            let p99 = sim.get_lat_percentile(0.99);
            let complete = sim.get_cum_decoded() == sim.get_num_source();
            (overhead, p99, complete)
        };

        println!("\npaper 8.8 quality: derived W vs fixed W=64 (hint=auto, delta={delta:.0e})");
        println!("  {:<10} {:>4} {:>6} | {:>8} {:>8} | {:>8} {:>8}",
            "channel", "W64", "W*", "oh(64)", "oh(W*)", "p99(64)", "p99(W*)");
        for ch in &channels {
            let p = ch.eps * ch.q / (1.0 - ch.eps);
            let sigma2 = math::burst_variance_factor(p, ch.q);
            let srtt = ch.rtt as f64 / 1000.0;
            let wstar = math::derive_window(delta, ch.eps, sigma2, srtt, send_rate, 0.0).round() as u32;
            let (oh64, p99_64, c64) = run(ch.eps, ch.q, ch.rtt, 64);
            let (ohw, p99_w, cw) = run(ch.eps, ch.q, ch.rtt, wstar);
            println!("  {:<10} {:>4} {:>6} | {:>7.1}% {:>7.1}% | {:>6.1}ms {:>6.1}ms",
                ch.name, 64, wstar, oh64 * 100.0, ohw * 100.0, p99_64, p99_w);
            assert!(c64 && cw, "{}: both runs must deliver all source", ch.name);
            if ch.name == "Satellite" {
                // High-eps channel: the derived (larger) window must not cost
                // MORE overhead than fixed 64 — the 1/sqrt(W) margin is fattest here.
                assert!(ohw <= oh64 + 0.005,
                    "Satellite derived-W overhead {ohw:.3} should not exceed fixed-64 {oh64:.3}");
            }
        }
    }

    #[test]
    fn test_bulk_is_mostly_arq() {
        let mut sim_bulk = Simulation::new(0.05, 0.5, 50, 64, "bulk".into(), None, None, None);
        let mut sim_auto = Simulation::new(0.05, 0.5, 50, 64, "auto".into(), None, None, None);
        while !sim_bulk.is_finished() && sim_bulk.get_tick() < 20_000 { sim_bulk.step(); }
        while !sim_auto.is_finished() && sim_auto.get_tick() < 20_000 { sim_auto.step(); }
        assert!(sim_bulk.is_finished() && sim_auto.is_finished());
        // Bulk pure-ARQ: steady-state overhead well below Auto's
        assert!(
            sim_bulk.get_overhead() < sim_auto.get_overhead(),
            "bulk {}% should be below auto {}%",
            sim_bulk.get_overhead(), sim_auto.get_overhead()
        );
        assert_eq!(sim_bulk.get_cum_decoded(), sim_bulk.get_num_source());
    }

    #[test]
    fn test_bulk_completes_faster_than_realtime() {
        // Bulk sends ~no FEC (wire budget goes to source) and recovers via
        // P_lost-driven ARQ in parallel with the stream + tail FEC — its
        // completion must beat Realtime's (which pays r ~ 20%+ of the wire
        // for corrections).
        let mut bulk = Simulation::new(0.05, 0.5, 50, 64, "bulk".into(), None, None, None);
        let mut rt = Simulation::new(0.05, 0.5, 50, 64, "realtime".into(), None, None, None);
        while !bulk.is_finished() && bulk.get_tick() < 20_000 { bulk.step(); }
        while !rt.is_finished() && rt.get_tick() < 20_000 { rt.step(); }
        assert!(bulk.is_finished() && rt.is_finished());
        assert_eq!(bulk.get_cum_decoded(), bulk.get_num_source());
        assert!(
            bulk.get_tick() < rt.get_tick(),
            "bulk ({} ticks) must complete before realtime ({} ticks)",
            bulk.get_tick(), rt.get_tick()
        );
    }

    #[test]
    fn test_latency_metrics_show_the_trade() {
        // Realtime buys a tighter latency tail with FEC; Bulk gives the
        // wire to source and pays the tail via ARQ waits. Both should have
        // avg latency >= one-way propagation.
        let mut rt = Simulation::new(0.05, 0.5, 50, 64, "realtime".into(), None, None, None);
        let mut bulk = Simulation::new(0.05, 0.5, 50, 64, "bulk".into(), None, None, None);
        while !rt.is_finished() && rt.get_tick() < 20_000 { rt.step(); }
        while !bulk.is_finished() && bulk.get_tick() < 20_000 { bulk.step(); }
        let one_way = 25.0;
        assert!(rt.get_lat_avg() >= one_way && bulk.get_lat_avg() >= one_way);
        let rt_p99 = rt.get_lat_percentile(0.99);
        let bulk_p99 = bulk.get_lat_percentile(0.99);
        assert!(
            rt_p99 < bulk_p99,
            "realtime p99 latency ({rt_p99:.1}ms) must beat bulk ({bulk_p99:.1}ms)"
        );
        assert!(rt.get_jitter() >= 0.0 && rt.get_jitter().is_finite());
        assert_eq!(
            rt.delivered_lat_ms.len(),
            rt.get_num_source() as usize,
            "every delivered symbol has a latency sample"
        );
    }

    #[test]
    fn test_excess_overhead_metric() {
        // Floor at eps=0.05: 5/95 = 5.26%. Bulk (mostly ARQ) should sit
        // close to the floor; excess = overhead - floor >= 0 always.
        let mut sim = Simulation::new(0.05, 0.5, 50, 64, "bulk".into(), None, None, None);
        while !sim.is_finished() && sim.get_tick() < 20_000 { sim.step(); }
        let floor = sim.get_overhead_floor();
        assert!((floor - 5.263).abs() < 0.01, "floor {floor}");
        assert!(sim.get_excess_overhead() >= 0.0);
        assert!(
            sim.get_excess_overhead() <= sim.get_overhead(),
            "excess cannot exceed total"
        );
    }

    /// The "continuum" hint: one law across the whole dial, no mode bit.
    /// The Bulk end (β = 1) reproduces late-is-fine (r = 0 mid-stream at
    /// any ε); the Realtime end (β = 0) is bit-identical to the "realtime"
    /// alias of the same dial point; ρ composes at every position; and the law is
    /// continuous across the Bulk preset (no behavior step at δ = 0.005
    /// vs δ just above it).
    #[test]
    fn test_continuum_one_law_across_the_dial() {
        // Bulk end: pure ARQ in the settled mid-stream at ε = 5% AND 10%
        // (the tracks-p̂ form emerges from the blend, not from a flag).
        // The settled window [100, 300) excludes the estimator warm-up —
        // the SHIPPED bulk reference arm itself shows a brief warm-up
        // spike at some cells, so the parity bar is the reference, not an
        // idealized flat zero.
        for eps in [0.05, 0.10] {
            let mut s = Simulation::new(
                eps, 0.5, 50, 64, "continuum".into(), None, Some(0.005), None,
            );
            let mut reference = Simulation::new(
                eps, 0.5, 50, 64, "bulk".into(), None, None, None,
            );
            let (mut mid_max, mut ref_max): (f64, f64) = (0.0, 0.0);
            for i in 0..300 {
                s.step();
                reference.step();
                if i >= 100 {
                    mid_max = mid_max.max(s.get_r_star());
                    ref_max = ref_max.max(reference.get_r_star());
                }
            }
            // Parity with the reference arm (the tiny residual is the χ
            // horizon brushing the settled window on the toy transfer).
            assert!(
                mid_max <= ref_max + 1e-4,
                "continuum bulk end settled r {mid_max} must match the bulk arm {ref_max} (eps={eps})"
            );
            assert!(mid_max < 1e-3, "settled bulk-end r must be ~0: {mid_max}");
            run_to_end(&mut s);
            assert_eq!(s.get_cum_decoded(), s.get_num_source());
        }
        // Realtime end (β = 0): the named hint is the same point of the dial
        // (alias table; the full accessor-level check is
        // `test_named_hints_are_points_on_the_continuum`).
        let mut rt = Simulation::new(
            0.05, 0.5, 50, 64, "continuum".into(), None, Some(50.0), None,
        );
        let mut cu = Simulation::new(0.05, 0.5, 50, 64, "realtime".into(), None, None, None);
        run_to_end(&mut rt);
        run_to_end(&mut cu);
        assert_eq!(rt.get_tick(), cu.get_tick(), "β = 0 must equal the realtime alias");
        assert_eq!(rt.get_total_fec(), cu.get_total_fec());
        // Continuity across the Bulk preset: a 5% dial nudge off the
        // preset must not step the early behavior (β 1.0 → ~0.989).
        let mut at = Simulation::new(0.05, 0.5, 50, 64, "continuum".into(), None, Some(0.005), None);
        let mut off = Simulation::new(0.05, 0.5, 50, 64, "continuum".into(), None, Some(0.00525), None);
        for _ in 0..300 {
            at.step();
            off.step();
        }
        let (fec_at, fec_off) = (at.get_total_fec(), off.get_total_fec());
        // The off-preset point mixes Δβ ≈ 1.1% of the anchor law back in,
        // so its early FEC must scale with Δβ (a few symbols), not step to
        // the anchor arm's full cold-start emission (the step a tail-domain
        // blend would produce).
        assert!(
            (fec_at as i64 - fec_off as i64).abs() <= 10,
            "behavior step across the Bulk preset: fec {fec_at} vs {fec_off}"
        );
        // ρ composes at the bulk end of the continuum.
        let mut br = Simulation::new(
            0.10, 0.3, 80, 64, "continuum".into(), None, Some(0.005), Some(0.95),
        );
        run_to_end(&mut br);
        assert_eq!(br.get_cum_decoded() + br.get_given_up(), br.get_num_source());
        assert!(br.get_reliability() >= 0.90);

        // Three knobs (plan §5.4 item 7): at the three δ presets the knobs
        // constructor at T = H = λ = ∞ is the dial's point exactly ...
        for d in [0.005, 0.5, 50.0] {
            let mut a = Simulation::new(0.05, 0.5, 50, 64, "continuum".into(), None, Some(d), None);
            let mut b = Simulation::multipath_knobs(
                vec![0.05], vec![0.5], vec![50], vec![4], 64, "continuum".into(), None, Some(d),
                None, f64::INFINITY, f64::INFINITY, f64::INFINITY, 2000,
                vec![f64::INFINITY], vec![0.0],
            )
            .unwrap();
            run_to_end(&mut a);
            run_to_end(&mut b);
            assert_eq!(
                (a.get_tick(), a.get_total_fec(), a.get_total_arq(), a.get_cum_decoded()),
                (b.get_tick(), b.get_total_fec(), b.get_total_arq(), b.get_cum_decoded()),
                "T = ∞ identity at δ = {d}"
            );
        }
        // ... and the Realtime-call point (T = 0.4, δ = 50, H = 0) has no
        // behaviour step under a ±2 % T nudge (the derived bound of item 5).
        let call = crate::knob_sim_tests::Knobs {
            h: 0.0, load: 0.5, ..crate::knob_sim_tests::OBJECT
        };
        crate::knob_sim_tests::t_nudge_within_bound(
            &[crate::knob_sim_tests::Cell { eps: 0.05, q: 0.5, rtt: 50, cap: 4 }],
            "continuum", Some(50.0), call, 0.4,
        );
    }

    /// F1/F2 (docs/derivations/wasm-modeswitch-audit.md): the classic hint
    /// names are an ALIAS TABLE onto the one continuum constructor (bulk →
    /// δ 0.005, auto → 0.5, realtime → 50), not machines. For every alias and
    /// every golden cell, every public accessor the UI or the gates read
    /// equals `continuum@δ(hint)`'s; and EVERY hint honours the ρ dial (the
    /// old hint-gated ρ dropped it for auto/realtime/fixed).
    #[test]
    fn test_named_hints_are_points_on_the_continuum() {
        fn fp(s: &mut Simulation) -> Vec<f64> {
            while !s.is_finished() && s.get_tick() < 20_000 { s.step(); }
            vec![
                s.get_tick() as f64, s.get_total_src() as f64, s.get_total_fec() as f64,
                s.get_total_arq() as f64, s.get_total_lost() as f64,
                s.get_cum_decoded() as f64, s.get_cum_sent() as f64,
                s.get_cum_arrived() as f64, s.get_given_up() as f64,
                s.get_rho(), s.get_r_star_auto(), s.get_derived_w(),
                s.get_delta_eff(), s.get_completion_exposure(),
            ]
        }
        for (hint, dp) in [("auto", 0.5), ("bulk", 0.005), ("realtime", 50.0)] {
            for (eps, q, rtt) in [(0.05, 0.5, 50u32), (0.10, 0.3, 80)] {
                let mut named = Simulation::new(eps, q, rtt, 64, hint.into(), None, None, None);
                let mut dial =
                    Simulation::new(eps, q, rtt, 64, "continuum".into(), None, Some(dp), None);
                assert_eq!(fp(&mut named), fp(&mut dial), "{hint} eps={eps} q={q} rtt={rtt}");
            }
            // F2: harsh cell with ρ = 0.95 — the declared ρ reaches every
            // alias and the run is the continuum's, give-ups included.
            let mut named =
                Simulation::new(0.20, 0.1, 100, 64, hint.into(), None, None, Some(0.95));
            let mut dial =
                Simulation::new(0.20, 0.1, 100, 64, "continuum".into(), None, Some(dp), Some(0.95));
            assert_eq!(named.get_rho(), 0.95, "{hint}: ρ dial honoured");
            assert_eq!(fp(&mut named), fp(&mut dial), "{hint} at rho=0.95");
        }
        // The nameless path composes ρ too ("fixed" keeps its own r override).
        let fixed = Simulation::new(0.05, 0.5, 50, 64, "fixed".into(), Some(0.1), None, Some(0.95));
        assert_eq!(fixed.get_rho(), 0.95);
    }

    /// W11 (audit Coverage): `rate_now` is the mix with no branch on β --
    /// §4.5: "There is no `if β == 0` fast path". A source check, because the
    /// value step of such a guard is zero by identity and no ordinal test
    /// can see it.
    #[test]
    fn test_rate_now_has_no_beta_branch() {
        let src = include_str!("lib.rs").replace("\r\n", "\n");
        let start = src.find("fn rate_now(").expect("rate_now");
        let body = &src[start..];
        let body = &body[..body.find("
    }
").expect("end of rate_now")];
        for needle in ["bulkness >", "bulkness <", "bulkness ==", "bulkness !=", "hint"] {
            assert!(!body.contains(needle), "rate_now branches on `{needle}`");
        }
    }

    /// W22 (docs/derivations/wasm-modeswitch-audit.md, ledger R03): the
    /// documented code-vs-paper divergence of `raptorpath_math::p_fec_normal`,
    /// BOUNDED (CLAUDE.md: a documented divergence carries a test that bounds
    /// it). The paper's law (§4.2) is continuous,
    ///   P_paper(r) = Φ( √W·(r(1−ε) − ε) / √(ε(1−ε)(r + σ²)) ),
    /// exactly ½ at the floor r = ε/(1−ε). The code adds a truncation, so it
    /// is 0 below the floor and steps to ≈ ½ at it. This pins that shape:
    ///   - below the floor: |P_code − P_paper| = P_paper (code is 0), and
    ///     P_paper < ½, so the gap is < ½;
    ///   - above the floor: code = paper;
    ///   - at the floor: code ≤ ½ (and paper = ½).
    /// It bounds the divergence; it does not fix it (the fix is an
    /// engine-crate change and the owner's call).
    #[test]
    fn test_p_fec_normal_vs_paper_phi_divergence_is_bounded() {
        // The paper's Φ, evaluated independently of `p_fec_normal`'s early
        // returns: Φ(z) = 1 − Q(z).
        let paper = |r: f64, eps: f64, w: f64, s2: f64| -> f64 {
            let z = w.sqrt() * (r * (1.0 - eps) - eps) / (eps * (1.0 - eps) * (r + s2)).sqrt();
            1.0 - math::normal_survival(z)
        };
        let mut worst_gap_below = 0.0f64;
        let mut jump_above = 0.0f64;
        for eps in [0.025, 0.05, 0.10] {
            let floor = eps / (1.0 - eps);
            for w in [5.0, 20.0, 64.0, 200.0] {
                for s2 in [1.0, 2.6, 3.76] {
                    let code = |r: f64| math::p_fec_normal(r, eps, w, s2);
                    // Below the floor: the code is 0, so the gap IS the paper's value.
                    for f in [0.01, 0.25, 0.5, 0.9, 0.99, 0.999] {
                        let r = floor * f;
                        let (c, p) = (code(r), paper(r, eps, w, s2));
                        assert_eq!(c, 0.0, "code is 0 below the floor (eps {eps} W {w} σ² {s2} r {r})");
                        assert!(p < 0.5, "paper < ½ below the floor: {p}");
                        assert!(
                            ((c - p).abs() - p).abs() < 1e-12,
                            "|P_code − P_paper| = P_paper below the floor (eps {eps} W {w} σ² {s2} f {f})"
                        );
                        worst_gap_below = worst_gap_below.max((c - p).abs());
                    }
                    // Above the floor: the code IS the paper's law.
                    for f in [1.001, 1.01, 1.5, 2.0, 5.0, 20.0] {
                        let r = floor * f;
                        let (c, p) = (code(r), paper(r, eps, w, s2));
                        assert!(
                            (c - p).abs() < 1e-12,
                            "code = paper above the floor (eps {eps} W {w} σ² {s2} f {f}): {c} vs {p}"
                        );
                    }
                    // At the floor: the paper says exactly ½; the code is 0 or ½.
                    let (c, p) = (code(floor), paper(floor, eps, w, s2));
                    assert!((p - 0.5).abs() < 1e-9, "paper is ½ at the floor: {p}");
                    assert!(c <= 0.5 + 1e-9, "code ≤ ½ at the floor: {c}");
                    // The step across the floor (the divergence's size): ≈ ½.
                    jump_above = jump_above.max(code(floor * 1.001) - code(floor * 0.999));
                }
            }
        }
        // The divergence is bounded by ½ everywhere and attains ≈ ½ at the floor.
        assert!(worst_gap_below < 0.5, "gap below the floor < ½: {worst_gap_below}");
        assert!(
            (0.45..=0.51).contains(&jump_above),
            "the code's step across the floor is ≈ ½ (documented, not fixed): {jump_above}"
        );
    }

    /// The sim/engine tail-anchor divergence, bounded (CLAUDE.md: every
    /// documented model-vs-engine divergence carries a test that bounds it,
    /// not prose that describes it). §11.3.
    ///
    /// The engine's effective tail target is `BASE·ζ(δ)` for `BASE = 1e-5`
    /// at every point of the dial (§4.1), which is what
    /// `contract_alpha`/`FecRateController` compute. This sim uses its own
    /// log-log interpolation through the anchor (0.005, 0.05) — §4.5's
    /// late-is-fine budget — where the engine's ζ-map says 1e-3. That is a
    /// deliberate between-preset divergence of the L0 model, bounded here
    /// rather than unified: unifying it would move the golden fingerprints,
    /// which is a separate, deliberate model change (CLAUDE.md scope rule).
    ///
    /// The bound is tight and stated three ways, so the divergence cannot
    /// grow silently:
    ///   1. Identical (exactly 0) on the whole Realtime–Auto half:
    ///      both are slope −1 in log₁₀δ and agree at both anchors.
    ///   2. ≤ log₁₀(50) over the dial between its named points.
    ///   3. Attained at the Bulk anchor, where it is log₁₀(0.05/1e-3).
    ///
    /// log₁₀(50) is the smallest true bound: the anchors differ by exactly
    /// 50× at Bulk, so a tighter bound such as log₁₀(50)/2 is false there
    /// by construction.
    #[test]
    fn test_sim_engine_tail_anchor_divergence_is_bounded() {
        /// The engine's `target_tail_loss` default (`config.rs`,
        /// `target_tail_loss.unwrap_or(1e-5)`).
        const BASE: f64 = 1e-5;
        let engine = |d: f64| BASE * zeta_of_delta(d);
        let bound = 50.0f64.log10();

        // The dial between its named points, log-uniform.
        let (lo, hi) = (0.005f64, 50.0f64);
        let mut worst = 0.0f64;
        let mut worst_at = lo;
        for i in 0..=400 {
            let d = lo * (hi / lo).powf(i as f64 / 400.0);
            let gap = (sim_tail_target_of_delta(d).log10() - engine(d).log10()).abs();
            assert!(
                gap <= bound + 1e-12,
                "δ={d}: sim/engine tail anchors diverge by {gap} decades, past the \
                 log10(50) = {bound} anchor gap this divergence is documented as"
            );
            if gap > worst {
                worst = gap;
                worst_at = d;
            }
            // Clause 1: EXACT agreement on the Realtime–Auto half.
            if d >= 0.5 {
                assert!(
                    gap < 1e-12,
                    "δ={d}: the sim and the engine must be the SAME law above Auto \
                     (both slope −1 in log δ through the same two anchors): {gap}"
                );
            }
        }
        // Clause 3: the worst case is the Bulk anchor and it is the anchor
        // gap itself — an assertion on the MECHANISM, not a spread.
        assert!(
            (worst - bound).abs() < 1e-9 && (worst_at - 0.005).abs() < 1e-4,
            "the divergence must be maximal AT the Bulk anchor and equal to the \
             0.05-vs-1e-3 anchor gap: worst={worst} at δ={worst_at}, bound={bound}"
        );
        assert!(
            (sim_tail_target_of_delta(0.005) - 0.05).abs() < 1e-15
                && (engine(0.005) - 1e-3).abs() < 1e-15,
            "the two anchors themselves moved; re-derive the bound before editing it"
        );
    }

    /// The three dial functions are re-exports of `raptorpath-math`, not
    /// re-derivations: they must reproduce the exact f64 of the sim's own
    /// closed forms below. Bit-exact, because an ulp here moves a golden
    /// fingerprint.
    #[test]
    fn test_dial_reexports_are_bit_identical_to_the_shipped_sim_forms() {
        for i in 0..=600 {
            // Log-uniform over four decades either side of the dial.
            let d = 0.0005f64 * (500.0f64 / 0.0005).powf(i as f64 / 600.0);
            assert_eq!(zeta_of_delta(d), 0.5 / d.max(1e-12), "ζ moved at δ={d}");
            assert_eq!(
                span_horizon_b(d),
                (2.0f64).powf(-((2.0 * d.max(1e-12)).log10() / 2.0)).clamp(0.5, 2.0),
                "b moved at δ={d}"
            );
            let x = d.max(1e-12).log10();
            let (a, b) = (0.5f64.log10(), 0.005f64.log10());
            assert_eq!(
                bulkness_of_delta(d),
                ((a - x) / (a - b)).clamp(0.0, 1.0),
                "β moved at δ={d}"
            );
        }
        // The presets, absolutely and bit-exactly.
        assert_eq!(span_horizon_b(50.0), 0.5);
        assert_eq!(span_horizon_b(0.5), 1.0);
        assert_eq!(span_horizon_b(0.005), 2.0);
        assert_eq!(bulkness_of_delta(50.0), 0.0);
        assert_eq!(bulkness_of_delta(0.5), 0.0);
        assert_eq!(bulkness_of_delta(0.005), 1.0);
        assert_eq!(zeta_of_delta(50.0), 0.01);
        assert_eq!(zeta_of_delta(0.5), 1.0);
        assert_eq!(zeta_of_delta(0.005), 100.0);
    }

    /// The ρ dial composes with the Bulk price (no hidden mode switch
    /// keyed on ρ): 'bulk' + ρ < 1 keeps the late-is-fine controller
    /// (r = 0 mid-stream) and the §3.5 give-up semantics.
    #[test]
    fn test_bulk_composes_with_rho() {
        let mut sim = Simulation::new(
            0.10, 0.3, 80, 64, "bulk".into(), None, None, Some(0.95),
        );
        // Settled-window parity with the ρ = 1 reference arm (the shipped
        // bulk arm itself has a warm-up spike and a tiny χ-horizon
        // residual at this cell — the bar is parity, not idealization).
        let mut reference =
            Simulation::new(0.10, 0.3, 80, 64, "bulk".into(), None, None, None);
        let (mut mid_r_max, mut ref_r_max): (f64, f64) = (0.0, 0.0);
        for i in 0..300 {
            sim.step();
            reference.step();
            if i >= 100 {
                mid_r_max = mid_r_max.max(sim.get_r_star());
                ref_r_max = ref_r_max.max(reference.get_r_star());
            }
        }
        assert!(
            mid_r_max <= ref_r_max + 1e-3,
            "bulk + rho<1 settled r {mid_r_max} must match the rho=1 arm {ref_r_max}"
        );
        while !sim.is_finished() && sim.get_tick() < 20_000 { sim.step(); }
        assert_eq!(
            sim.get_cum_decoded() + sim.get_given_up(),
            sim.get_num_source(),
            "every symbol delivered or explicitly given up"
        );
        assert!(sim.get_reliability() >= 0.90, "reliability {}", sim.get_reliability());
        assert!((sim.get_rho() - 0.95).abs() < 1e-12, "rho dial honored");
        // Budget min(1−ρ, R) admits >= 1 symbol here, so give-up happens.
        assert!(sim.get_giveup_budget() * sim.get_total_src() as f64 >= 1.0);
        assert!(sim.get_given_up() > 0, "budget admits a symbol: ρ<1 gives up");
        println!("MOVED given_up={} budget*src={:.2}", sim.get_given_up(),
            sim.get_giveup_budget() * sim.get_total_src() as f64);
        // ρ = 1 (None) remains byte-identical bulk (the golden contract).
        let mut plain = Simulation::new(0.10, 0.3, 80, 64, "bulk".into(), None, None, None);
        run_to_end(&mut plain);
        assert_eq!(plain.get_given_up(), 0);
    }

    #[test]
    fn test_custom_rho_gives_up_late_losses() {
        // Custom triangle mode: rho < 1 -> budgeted give-up; some symbols are
        // given up instead of retransmitted forever.
        let mut sim = Simulation::new(
            0.10, 0.3, 80, 64, "continuum".into(), None, Some(0.005), Some(0.95),
        );
        while !sim.is_finished() && sim.get_tick() < 20_000 { sim.step(); }
        assert!(sim.is_finished());
        assert!(sim.get_reliability() >= 0.90, "reliability {}", sim.get_reliability());
        assert_eq!(
            sim.get_cum_decoded() + sim.get_given_up(),
            sim.get_num_source()
        );
        // The law's budget min(1−ρ, R) must admit >= 1 symbol on this cell,
        // and then something IS given up (age > D(δ) within the budget).
        assert!(sim.get_giveup_budget() * sim.get_total_src() as f64 >= 1.0);
        assert!(sim.get_given_up() > 0, "budget admits a symbol: ρ<1 gives up");
        println!("MOVED given_up={} budget*src={:.2}", sim.get_given_up(),
            sim.get_giveup_budget() * sim.get_total_src() as f64);
    }

    #[test]
    fn test_realtime_more_fec_than_bulk() {
        let mut rt = Simulation::new(0.05, 0.5, 50, 64, "realtime".into(), None, None, None);
        let mut bulk = Simulation::new(0.05, 0.5, 50, 64, "bulk".into(), None, None, None);
        while !rt.is_finished() && rt.get_tick() < 20_000 { rt.step(); }
        while !bulk.is_finished() && bulk.get_tick() < 20_000 { bulk.step(); }
        assert!(rt.get_total_fec() > bulk.get_total_fec(),
            "realtime fec {} should exceed bulk fec {}",
            rt.get_total_fec(), bulk.get_total_fec());
    }

    #[test]
    fn test_fixed_r_mode() {
        let mut sim = Simulation::new(0.10, 0.5, 20, 64, "fixed".into(), Some(0.2), None, None);
        while !sim.is_finished() && sim.get_tick() < 20_000 { sim.step(); }
        assert!(sim.is_finished());
        let overhead = sim.get_overhead();
        assert!(overhead > 10.0 && overhead < 45.0,
            "fixed r=0.2 should give ~20-30% overhead incl. retx, got {overhead:.1}%");
    }

    fn run_to_end(sim: &mut Simulation) {
        while !sim.is_finished() && sim.get_tick() < 20_000 {
            sim.step();
        }
        assert!(sim.is_finished());
    }

    #[test]
    fn test_bulk_beats_fixed_001() {
        // Bulk vs a fixed r = 0.01 floor (§4.6). A min(0.1, p_hat) Bulk
        // mapping pins the cold-start rate at max_overhead (M1) and leaks
        // permanent FEC (M2). With the completion-exposure glide, Bulk must
        // match fixed(0.01) on completion (within 5%) and beat it on
        // overhead at the representative cell.
        let mut bulk = Simulation::new(0.05, 0.5, 50, 64, "bulk".into(), None, None, None);
        let mut fixed = Simulation::new(0.05, 0.5, 50, 64, "fixed".into(), Some(0.01), None, None);
        run_to_end(&mut bulk);
        run_to_end(&mut fixed);
        assert_eq!(bulk.get_cum_decoded(), bulk.get_num_source());
        println!(
            "eps=0.05 rtt=50: bulk {} ticks / {:.2}% overhead vs fixed(0.01) {} ticks / {:.2}%",
            bulk.get_tick(), bulk.get_overhead(), fixed.get_tick(), fixed.get_overhead()
        );
        assert!(
            (bulk.get_tick() as f64) <= fixed.get_tick() as f64 * 1.05,
            "bulk completion {} ticks must be within 5% of fixed(0.01) {} ticks",
            bulk.get_tick(), fixed.get_tick()
        );
        assert!(
            bulk.get_overhead() < fixed.get_overhead() + 1.0,
            "bulk overhead {:.2}% must be < fixed(0.01) {:.2}% + 1pt",
            bulk.get_overhead(), fixed.get_overhead()
        );

        // M2 cell (eps = 0.10 >= the min(0.1, p_hat) clamp, where that
        // mapping pays permanent FEC ~ the IT floor on top of ARQ): the chi
        // glide must keep excess overhead under 2%.
        let mut bulk10 = Simulation::new(0.10, 0.5, 50, 64, "bulk".into(), None, None, None);
        run_to_end(&mut bulk10);
        assert_eq!(bulk10.get_cum_decoded(), bulk10.get_num_source());
        assert!(
            bulk10.get_excess_overhead() < 2.0,
            "bulk excess overhead at eps=0.10 must be < 2%: {:.2}% (total {:.2}%, floor {:.2}%)",
            bulk10.get_excess_overhead(), bulk10.get_overhead(), bulk10.get_overhead_floor()
        );
    }

    #[test]
    fn test_bulk_chi_ramps_at_stream_tail() {
        // chi must be exactly 0 mid-stream (r* = 0 identically, even during
        // the estimator cold start) and ramp toward 1 by end of stream.
        let mut sim = Simulation::new(0.05, 0.5, 50, 64, "bulk".into(), None, None, None);
        // Cold start: first ticks have p_upper ~ 0.975 but chi = 0 -> r = 0.
        sim.step();
        assert_eq!(sim.get_completion_exposure(), 0.0);
        assert!(
            sim.get_p_upper() > 0.5,
            "cold-start upper quantile is pessimistic: {}",
            sim.get_p_upper()
        );
        assert_eq!(sim.get_r_star(), 0.0, "M1: cold-start Bulk rate must be 0");
        // Mid-stream: still 0.
        for _ in 0..200 { sim.step(); }
        assert_eq!(sim.get_completion_exposure(), 0.0);
        // Run until the source is done: chi must have ramped up.
        while !sim.source_done && sim.get_tick() < 20_000 { sim.step(); }
        assert!(
            sim.get_completion_exposure() > 0.95,
            "chi at end of stream: {}",
            sim.get_completion_exposure()
        );
        run_to_end(&mut sim);
        assert_eq!(sim.get_cum_decoded(), sim.get_num_source());
    }

    #[test]
    fn test_ablation_p6_completion_exposure() {
        // Old mapping (delta_eff = min(0.1, p_hat) + one-shot tail burst)
        // vs the chi glide, same seeds (the channel seed derives from
        // eps/q/rtt only). Run with --nocapture to record the deltas.
        //
        // The rtt=150 cell is the documented horizon caveat (§4.6):
        // a 2000-symbol transfer (~0.5 s) fits entirely inside the chi
        // exposure horizon (~5.5 x SRTT at 150 ms), so chi > 0 from the
        // first tick and the cold-start estimator still pins the early
        // rate in BOTH arms — the glide's r* = 0 guarantee needs a genuine
        // mid-stream phase. Gate: improvement where mid-stream exists
        // (rtt=50 cells), no-regression elsewhere.
        for (eps, q, rtt) in [(0.05, 0.5, 50u32), (0.10, 0.5, 50u32), (0.05, 0.5, 150u32)] {
            let mut new = Simulation::new(eps, q, rtt, 64, "bulk".into(), None, None, None);
            let mut old = Simulation::new(eps, q, rtt, 64, "bulk".into(), None, None, None);
            old.legacy_bulk_delta = true;
            run_to_end(&mut new);
            run_to_end(&mut old);
            assert_eq!(new.get_cum_decoded(), new.get_num_source());
            assert_eq!(old.get_cum_decoded(), old.get_num_source());
            println!(
                "eps={eps} rtt={rtt}: completion {} -> {} ticks, overhead {:.2}% -> {:.2}% (excess {:.2}% -> {:.2}%)",
                old.get_tick(), new.get_tick(),
                old.get_overhead(), new.get_overhead(),
                old.get_excess_overhead(), new.get_excess_overhead()
            );
            // The glide must not regress completion anywhere...
            assert!(
                (new.get_tick() as f64) <= old.get_tick() as f64 * 1.05,
                "eps={eps} rtt={rtt}: completion regressed {} -> {}",
                old.get_tick(), new.get_tick()
            );
            // ...and must cut overhead wherever a mid-stream (chi = 0)
            // phase exists; at rtt=150 (horizon caveat above) the gate is
            // no-regression within noise.
            if rtt <= 50 {
                assert!(
                    new.get_overhead() < old.get_overhead(),
                    "eps={eps} rtt={rtt}: overhead must improve: {:.2}% -> {:.2}%",
                    old.get_overhead(), new.get_overhead()
                );
            } else {
                assert!(
                    new.get_overhead() <= old.get_overhead() + 1.0,
                    "eps={eps} rtt={rtt}: overhead regressed: {:.2}% -> {:.2}%",
                    old.get_overhead(), new.get_overhead()
                );
            }
        }
    }

    #[test]
    fn test_fixed_r_sets_realized_rate() {
        // "Fixed r" pins the CONTROLLER (correction) rate: the realized FEC
        // rate tracks the target within noise. Overhead and excess-overhead
        // are DIFFERENT metrics -- overhead adds the ARQ that recovers the
        // channel's losses (the floor); excess = overhead - realized floor,
        // which comes out near r only because at small r the FEC barely
        // displaces ARQ. This test asserts the realized FEC rate == target r,
        // NOT that overhead == r (the naming trap the UI must not imply).
        for &fr in &[0.01f64, 0.05, 0.10] {
            for &eps in &[0.02f64, 0.05, 0.10] {
                for &rtt in &[20u32, 50, 100] {
                    let mut sim = Simulation::new(
                        eps, 0.5, rtt, 64, "fixed".into(), Some(fr), None, None,
                    );
                    run_to_end(&mut sim);
                    let fec_rate = sim.total_fec as f64 / sim.total_src as f64;
                    // Realized FEC rate is r plus a small, bounded tail-flush
                    // constant (the end-of-stream burst, <= 24 repairs over
                    // 2000 source ~ +0.012); never a systematic multiple.
                    assert!(
                        fec_rate >= fr - 0.005 && fec_rate <= fr + 0.02,
                        "fixed r={fr} eps={eps} rtt={rtt}: realized FEC rate \
                         {fec_rate:.4} must track target (not diverge)"
                    );
                }
            }
        }
    }

    #[test]
    fn test_overhead_never_below_realized_floor() {
        // Overhead never drops below the channel floor. Two ways a metric
        // could show it doing so: (a) measuring overhead over the steady
        // phase only drops the drain-phase corrections; (b) the nominal
        // floor uses eps, but a finite run samples only a prefix of the
        // expectation-calibrated channel, so realized loss can sit below
        // eps. Hence overhead counts all sends, and the invariant is stated
        // against the realized floor -- which is exact: decoding N source
        // symbols requires >= N arrived symbols.
        for hint in ["bulk", "auto", "realtime", "fixed"] {
            for &eps in &[0.02f64, 0.05, 0.10, 0.15] {
                for &q in &[0.3f64, 0.5] {
                    for &rtt in &[20u32, 50, 100] {
                        let fr = if hint == "fixed" { Some(0.03) } else { None };
                        let mut sim = Simulation::new(eps, q, rtt, 64, hint.into(), fr, None, None);
                        run_to_end(&mut sim);
                        // rho = 1: every source symbol delivered.
                        assert_eq!(sim.get_cum_decoded(), sim.get_num_source());
                        let oh = sim.get_overhead();
                        let floor_real = sim.get_overhead_floor_realized();
                        assert!(
                            oh + 1e-9 >= floor_real,
                            "hint={hint} eps={eps} q={q} rtt={rtt}: overhead \
                             {oh:.4}% dropped below realized floor {floor_real:.4}%"
                        );
                        assert!(sim.get_excess_overhead() >= 0.0);
                    }
                }
            }
        }
    }

    #[test]
    fn test_estimator_converges_in_sim() {
        let mut sim = Simulation::new(0.10, 0.5, 20, 64, "auto".into(), None, None, None);
        for _ in 0..1500 {
            if sim.is_finished() { break; }
            sim.step();
        }
        let est = sim.get_estimated_loss();
        assert!((est - 0.10).abs() < 0.05,
            "estimator should converge to ~10%: got {est:.3}");
    }

    /// p99 of the delivered latencies for source seqs in [lo, hi).
    fn p99_range(sim: &Simulation, lo: u32, hi: u32) -> f64 {
        let mut v: Vec<f64> = (lo..hi)
            .filter_map(|s| sim.lat_by_seq.get(s as usize).copied())
            .filter(|x| x.is_finite())
            .collect();
        assert!(!v.is_empty(), "no delivered latencies in [{lo},{hi})");
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = ((v.len() as f64 - 1.0) * 0.99).round() as usize;
        v[idx.min(v.len() - 1)]
    }

    fn mean_range(sim: &Simulation, lo: u32, hi: u32) -> f64 {
        let v: Vec<f64> = (lo..hi)
            .filter_map(|s| sim.lat_by_seq.get(s as usize).copied())
            .filter(|x| x.is_finite())
            .collect();
        v.iter().sum::<f64>() / v.len() as f64
    }

    #[test]
    fn test_no_end_of_stream_cliff_auto_realtime() {
        // §3.3 / §4.6: the taper integral is truncated for the final window's
        // symbols (no future source symbols -> no future repair coverage), so
        // WITHOUT a completion term the last-window symbols suffer a latency
        // cliff (serial ARQ). The continuous chi-driven completion ramp
        // (replacing the one-shot burst) must bring last-window recovery back
        // in line with mid-stream — for BOTH the tight (realtime) and the
        // balanced (auto) hints, whose steady rate does NOT already saturate
        // the tail.
        for hint in ["auto", "realtime"] {
            let mut sim = Simulation::new(0.05, 0.5, 50, 64, hint.into(), None, None, None);
            let w = 64u32;
            let n = sim.get_num_source();
            while !sim.is_finished() && sim.get_tick() < 20_000 {
                sim.step();
            }
            assert_eq!(sim.get_cum_decoded(), n, "{hint} must deliver all");
            // Mid-stream band [W, N-2W) vs last-window band [N-W, N).
            let mid_p99 = p99_range(&sim, w, n - 2 * w);
            let tail_p99 = p99_range(&sim, n - w, n);
            let mid_mean = mean_range(&sim, w, n - 2 * w);
            let tail_mean = mean_range(&sim, n - w, n);
            println!(
                "{hint}: mid p99={mid_p99:.1}ms tail p99={tail_p99:.1}ms | mid mean={mid_mean:.1} tail mean={tail_mean:.1}"
            );
            // No cliff: the last window's tail must stay within ONE honest
            // ARQ round of the mid-stream tail. Under the per-path RFC 9002
            // retransmit clock (§7.1) a residual straggler costs a full
            // own-RTT detection round — the cliff this test guards against
            // is serial multi-round ARQ (several
            // chained RTTs), which the completion ramp must prevent.
            assert!(
                tail_p99 <= mid_p99 + 50.0 + 10.0,
                "{hint} end-of-stream cliff: tail p99 {tail_p99:.1}ms vs mid {mid_p99:.1}ms \
                 (> mid + one ARQ round)"
            );
        }
    }

    /// §4.1 δ(hint) = 0.5/ζ: the three preset anchors, exactly.
    #[test]
    fn test_span_law_zeta_mapping() {
        assert!((zeta_of_delta(50.0) - 0.01).abs() < 1e-12, "Realtime ζ");
        assert!((zeta_of_delta(0.5) - 1.0).abs() < 1e-12, "Auto ζ");
        assert!((zeta_of_delta(0.005) - 100.0).abs() < 1e-9, "Bulk ζ");
    }

    /// §5.3/§5.4 D(δ) = min(b·RTprop, 2·RTprop), b = ½/1/2 at the presets.
    #[test]
    fn test_span_law_deadline() {
        assert!((span_horizon_b(50.0) - 0.5).abs() < 1e-12);
        assert!((span_horizon_b(0.5) - 1.0).abs() < 1e-12);
        assert!((span_horizon_b(0.005) - 2.0).abs() < 1e-9);
        // clamps outside the preset range
        assert_eq!(span_horizon_b(1e6), 0.5);
        assert_eq!(span_horizon_b(1e-9), 2.0);
        // D at RTprop = 100 ms: 50 / 100 / 200 ms
        assert!((span_deadline_d(50.0, 0.1) - 0.05).abs() < 1e-12);
        assert!((span_deadline_d(0.5, 0.1) - 0.1).abs() < 1e-12);
        assert!((span_deadline_d(0.005, 0.1) - 0.2).abs() < 1e-9);
    }

    /// §5.3 A* = clamp(rate·D, 1, W): a voice-like example
    /// (200 sym/s × 20 ms = 4), the W clamp, and the floor.
    #[test]
    fn test_span_law_a_star() {
        assert!((span_width_a_star(200.0, 0.02, 512.0) - 4.0).abs() < 1e-12);
        assert!((span_width_a_star(1000.0, 0.2, 64.0) - 64.0).abs() < 1e-12, "W clamp");
        assert!((span_width_a_star(10.0, 0.002, 512.0) - 1.0).abs() < 1e-12, "floor 1");
    }

    /// §5.3 M* = ceil(rate·2·RTprop/A*_q)+1, clamp [2, 32]
    /// (declared constants, §5.3).
    #[test]
    fn test_span_law_m_star() {
        // bulk-like: 5000 sym/s, RTprop 100 ms, grid quantum 128:
        // ceil(1000/128)+1 = 9
        assert!((pipeline_depth_m_star(5000.0, 0.1, 128.0) - 9.0).abs() < 1e-12);
        // realtime-like: BDP ≤ A* ⇒ the floor of 2 (depth term inert)
        assert!((pipeline_depth_m_star(200.0, 0.01, 4.0) - 2.0).abs() < 1e-12);
        // memory ceiling clamp
        assert!((pipeline_depth_m_star(1e6, 0.1, 1.0) - 32.0).abs() < 1e-12);
    }

    /// §5.3/ADR-0064 Δ = clamp(⌈rate·J⌉, 1, 64).
    #[test]
    fn test_span_law_trailing_offset() {
        assert!((trailing_offset_delta(200.0, 0.005) - 1.0).abs() < 1e-12);
        assert!((trailing_offset_delta(5000.0, 0.003) - 15.0).abs() < 1e-12);
        assert!((trailing_offset_delta(1e6, 1.0) - 64.0).abs() < 1e-12, "cap 64");
        assert!((trailing_offset_delta(10.0, 0.0) - 1.0).abs() < 1e-12, "FIFO floor");
    }

    /// §5.6 shed budget 1−ρ = ε̂·(1−P_fec): bounds and monotonicity in r.
    #[test]
    fn test_span_law_shed_budget() {
        assert_eq!(shed_budget_residual(0.0, 0.1, 64.0, 2.0), 0.0);
        let b_lo = shed_budget_residual(0.05, 0.0, 64.0, 2.9);
        let b_hi = shed_budget_residual(0.05, 0.30, 64.0, 2.9);
        assert!(b_lo <= 0.05 + 1e-12 && b_lo >= 0.0);
        assert!(b_hi < b_lo, "more FEC ⇒ smaller conceded residual");
        assert!(b_hi < 0.01, "realtime-class r concedes ~nothing: {b_hi}");
    }

    /// The δ-continuum tail-target interpolation hits the three §4.1
    /// preset anchors and is monotone (smaller δ price ⇒ looser tail).
    #[test]
    fn test_sim_tail_target_of_delta_anchors() {
        assert!((sim_tail_target_of_delta(50.0) - 1e-7).abs() / 1e-7 < 1e-9);
        assert!((sim_tail_target_of_delta(0.5) - 1e-5).abs() / 1e-5 < 1e-9);
        assert!((sim_tail_target_of_delta(0.005) - 0.05).abs() / 0.05 < 1e-9);
        let mut prev = sim_tail_target_of_delta(100.0);
        for i in 1..50 {
            let d = 100.0 * (1e-4f64 / 100.0).powf(i as f64 / 50.0);
            let t = sim_tail_target_of_delta(d);
            assert!(t >= prev, "tail target must be monotone along the continuum");
            prev = t;
        }
    }

    /// Walls #7/#9 model: SACK-clocked occupancy stays within the
    /// path-scaled pool, is ≤ the frontier-clocked counterfactual, and the
    /// pool BINDS (stalls) when cap·RTT is cranked past it — while the
    /// transfer still completes.
    #[test]
    fn test_store_pool_sack_release() {
        // Standard cell: slack pool, SACK ≤ frontier counterfactual.
        let mut sim = Simulation::new(0.05, 0.5, 50, 64, "auto".into(), None, None, None);
        let mut saw_occupancy = false;
        while !sim.is_finished() && sim.get_tick() < 20_000 {
            sim.step();
            assert!(sim.get_store_occupancy() <= sim.get_pool_cap());
            assert!(
                sim.get_store_occupancy() <= sim.get_store_occupancy_frontier(),
                "SACK release can only hold LESS than frontier release (tick {})",
                sim.get_tick()
            );
            if sim.get_store_occupancy() > 100 {
                saw_occupancy = true;
            }
        }
        assert!(saw_occupancy, "the store gauge must be live mid-stream");
        assert_eq!(sim.get_pool_cap(), STORE_PER_PATH);
        assert_eq!(sim.get_pool_stalls(), 0, "standard cell must not stall");
        assert_eq!(sim.get_cum_decoded(), sim.get_num_source());

        // High-BDP cell (cap 8 × RTT 200 = 1600 ≫ 512): the wall binds.
        let mut big = Simulation::multipath(
            vec![0.02], vec![0.5], vec![200], vec![8], 64, "auto".into(), None, None, None,
        );
        while !big.is_finished() && big.get_tick() < 20_000 {
            big.step();
        }
        assert!(big.get_pool_stalls() > 0, "cap·RTT ≫ pool must stall (wall #7)");
        assert_eq!(big.get_cum_decoded(), big.get_num_source());
        // Two paths double the pool (the path-scaled law).
        let dual = Simulation::multipath(
            vec![0.02, 0.02], vec![0.5, 0.5], vec![200, 200], vec![8, 8],
            64, "auto".into(), None, None, None,
        );
        assert_eq!(dual.get_pool_cap(), 2 * STORE_PER_PATH);
    }

    /// Paper §6.1 / reconciliation 4b (divergence, bounded): the model's pool
    /// is the CONSTANT 512 per path (law ledger M01: an underived constant).
    /// The engine's cap is `clamp(law, floor, max(n_live·2048, floor))`
    /// (`net/store_cap.rs`), and at c8 the law's value sits at that clamp for
    /// 0.74–0.98 of ticks (ledger F01, measured pinned) — a constant 2048 per
    /// path in effect, not a cap that moves with the BDP; 2048 is a clamp,
    /// not a knee (§56: c2 flat down to a total store W ≈ 192–256, c8 to
    /// ≈ 512, c1s loses at every cut). The model's pool does not bind at its
    /// c8 preset (stall fraction 0.000) while the engine's does. This test
    /// pins the MODEL side only: exactly 512 x n_paths at every delta, hint,
    /// RTT and capacity, before and after stepping, until the model changes
    /// deliberately (that moves the golden fingerprints). The derived store
    /// law is unshipped and is not modeled.
    #[test]
    fn wasm_pool_is_constant_per_path_whatever_the_dial_and_bdp() {
        assert_eq!(STORE_PER_PATH, 512, "the model's per-path pool constant");
        for n in 1usize..=3 {
            for (rtt, cap) in [(10u32, 2u32), (50, 4), (200, 8)] {
                for delta in [Some(0.005), Some(0.5), Some(50.0), None] {
                    let hint = if delta.is_some() { "continuum" } else { "auto" };
                    let mut sim = Simulation::multipath(
                        vec![0.02; n], vec![0.5; n], vec![rtt; n], vec![cap; n],
                        64, hint.into(), None, delta, None,
                    );
                    let want = 512 * n as u32;
                    assert_eq!(
                        sim.get_pool_cap(), want,
                        "pool = 512 x {n} paths at rtt {rtt} cap {cap} delta {delta:?}                          (the model pool is a constant; this pins the divergence from the engine's §6.1 cap)"
                    );
                    for _ in 0..50 {
                        sim.step();
                    }
                    assert_eq!(sim.get_pool_cap(), want, "pool does not move as the sim runs");
                }
            }
        }
    }

    /// Wall #8 model: on a heterogeneous 2-path cell, slow-path holes are
    /// HELD by their own path's RTT clock past the aggregate clock — the
    /// phantom retransmits a global-clock plane would have fired.
    #[test]
    fn test_per_path_clock_phantom_counter() {
        // Bulk (pure ARQ): holes survive to their own-path clock, so the
        // slow path's holes are visibly HELD past the aggregate clock.
        // (FEC-heavy hints heal holes within a few ticks — before the
        // aggregate clock even expires — so the counter honestly reads ~0.)
        let mut rwm = Simulation::multipath(
            vec![0.026, 0.048], vec![0.5, 0.5], vec![10, 40], vec![5, 1],
            64, "bulk".into(), None, None, None,
        );
        while !rwm.is_finished() && rwm.get_tick() < 20_000 {
            rwm.step();
        }
        assert!(
            rwm.get_phantom_avoided() > 0,
            "heterogeneous RTTs must produce held slow-path holes"
        );
        // Homogeneous paths: own clock == aggregate clock ⇒ no phantoms.
        let mut homo = Simulation::multipath(
            vec![0.05, 0.05], vec![0.5, 0.5], vec![50, 50], vec![4, 4],
            64, "auto".into(), None, None, None,
        );
        while !homo.is_finished() && homo.get_tick() < 20_000 {
            homo.step();
        }
        assert_eq!(homo.get_phantom_avoided(), 0);
    }

    #[test]
    fn test_ablation_completion_ramp_vs_burst() {
        // §4.6: the continuous chi-driven completion ramp replaces the
        // one-shot end-of-stream burst for non-Bulk hints. At high
        // RTT the in-flight span (symbols per RTT) exceeds W, so the burst
        // (which only covers the final window W) leaves late-stream losses
        // OUTSIDE the final window but inside the serial-recovery horizon
        // exposed; the ramp covers the whole exposed span. Same seeds per
        // cell (derived from eps/q/rtt only). Gate: the ramp must not regress
        // the last-window tail vs the burst, and must not blow up overhead.
        for (eps, rtt) in [(0.05, 50u32), (0.08, 150u32)] {
            for hint in ["auto", "realtime"] {
                let w = 64u32;
                let mut ramp = Simulation::new(eps, 0.5, rtt, w, hint.into(), None, None, None);
                let mut burst = Simulation::new(eps, 0.5, rtt, w, hint.into(), None, None, None);
                burst.legacy_tail_burst = true;
                run_to_end(&mut ramp);
                run_to_end(&mut burst);
                let n = ramp.get_num_source();
                assert_eq!(ramp.get_cum_decoded(), n);
                assert_eq!(burst.get_cum_decoded(), n);
                let ramp_tail = p99_range(&ramp, n - w, n);
                let burst_tail = p99_range(&burst, n - w, n);
                println!(
                    "eps={eps} rtt={rtt} {hint}: last-window p99 burst={burst_tail:.1}ms ramp={ramp_tail:.1}ms | overhead burst={:.2}% ramp={:.2}%",
                    burst.get_overhead(), ramp.get_overhead()
                );
                // The ramp meters the same budget continuously, so a couple
                // of stragglers can land in ARQ where the one-shot burst
                // heals instantly — and under the per-path RFC 9002 clock
                // (§7.1) one ARQ round costs a full own-RTT. Gate: within
                // one ARQ round of the burst arm (the claim is "no serial
                // multi-round cliff", not
                // "beats the burst at p99 of a 64-sample window").
                assert!(
                    ramp_tail <= burst_tail + rtt as f64 + 5.0,
                    "eps={eps} rtt={rtt} {hint}: ramp tail p99 {ramp_tail:.1} must stay within \
                     one ARQ round of burst {burst_tail:.1}"
                );
                assert!(
                    ramp.get_overhead() <= burst.get_overhead() + 6.0,
                    "eps={eps} rtt={rtt} {hint}: ramp overhead {:.2}% vs burst {:.2}%",
                    ramp.get_overhead(), burst.get_overhead()
                );
            }
        }
    }
}

/// The give-up law, option (ii) (docs/derivations/giveup-law-model.md):
/// give up a hole iff age > D(δ) and given_up + 1 <= min(1 − ρ, R)·src_total.
/// ρ = 1 is the retain point of the dial: budget 0, with no branch.
#[cfg(test)]
mod giveup_law_tests {
    use super::*;

    /// The cartoon's R at a cell and a δ price (what `spanLaw()` computes in
    /// the page: A* over the derived window, r* = (1−β)·r*(tail(δ))).
    fn cartoon_residual(eps: f64, q: f64, rtt: f64, dp: f64, w: f64) -> f64 {
        let p = eps * q / (1.0 - eps);
        let s2 = burst_variance_factor(p, q);
        let rate = 4.0 * (1.0 - eps);
        let d = span_deadline_d(dp, rtt.max(2.0));
        let a_star = span_width_a_star(rate, d, w);
        let r_star = (1.0 - bulkness_of_delta(dp))
            * compute_r_star_continuous(eps, s2, w, sim_tail_target_of_delta(dp));
        shed_budget_residual(eps, r_star, a_star, s2)
    }

    const PRESETS: [(&str, f64); 3] = [("bulk", 0.005), ("auto", 0.5), ("realtime", 50.0)];
    const CELLS: [(f64, f64, f64); 5] = [
        (0.05, 0.5, 50.0), (0.10, 0.3, 80.0), (0.20, 0.1, 100.0),
        (0.30, 0.1, 100.0), (0.20, 0.5, 50.0),
    ];

    /// Test 1: anchors.
    #[test]
    fn giveup_budget_anchors() {
        for r in [0.0, 1e-9, 1e-4, 0.01, 0.05, 0.1, 0.3] {
            assert_eq!(giveup_budget(1.0, r), 0.0, "budget(ρ=1) = 0 at R={r}");
            for k in 0..=100 {
                let rho = 0.9 + 0.001 * k as f64;
                let b = giveup_budget(rho, r);
                if 1.0 - rho >= r {
                    assert_eq!(b, r, "1−ρ ≥ R ⇒ budget = R exactly (ρ={rho}, R={r})");
                } else {
                    assert_eq!(b, 1.0 - rho, "1−ρ < R ⇒ budget = 1−ρ exactly (ρ={rho}, R={r})");
                }
            }
        }
        // At every δ preset, through the sim's own readout: ρ = 1 ⇒ 0.
        for (name, dp) in PRESETS {
            let mut s = Simulation::new(
                0.10, 0.3, 80, 64, "continuum".into(), None, Some(dp), Some(1.0));
            for _ in 0..300 { s.step(); }
            assert_eq!(s.get_giveup_budget(), 0.0, "{name}: budget(ρ=1) = 0");
            assert!(s.get_giveup_residual() >= 0.0);
        }
    }

    /// Test 2: monotone non-increasing in ρ (including ρ → 1) and continuous
    /// under ±2 % nudges of the dial, at R from the cartoon's law at every
    /// preset and cell.
    #[test]
    fn giveup_budget_monotone_and_continuous_in_rho() {
        for (name, dp) in PRESETS {
            for (eps, q, rtt) in CELLS {
                let r = cartoon_residual(eps, q, rtt, dp, 64.0);
                let mut prev = f64::INFINITY;
                let mut rhos: Vec<f64> = (0..=100).map(|k| 0.9 + 0.001 * k as f64).collect();
                rhos.extend([1.0 - 1e-6, 1.0 - 1e-9, 1.0]);
                rhos.sort_by(|a, b| a.partial_cmp(b).unwrap());
                for rho in rhos {
                    let b = giveup_budget(rho, r);
                    assert!(b <= prev + 1e-15, "{name} {eps}: budget rose in ρ at {rho}");
                    prev = b;
                }
                for rho in [0.9, 0.95, 0.99, 0.999] {
                    // ±2 % nudge of the contract's slack (1−ρ).
                    let lo = giveup_budget(1.0 - (1.0 - rho) * 0.98, r);
                    let hi = giveup_budget(1.0 - (1.0 - rho) * 1.02, r);
                    assert!(
                        (hi - lo).abs() <= 0.04 * (1.0 - rho) + 1e-15,
                        "{name} {eps} ρ={rho}: ±2% nudge stepped the budget: {lo} {hi}"
                    );
                }
            }
        }
    }

    /// Test 3: law-form bound at equal R. The engine's two seats are
    /// R_seat(ρ) = R (evicting, ρ < 1) and 0 (retain, ρ = 1); the model
    /// agrees at ρ = 1 and wherever 1−ρ ≥ R, and is tighter by at most
    /// R − (1−ρ) otherwise. (The real agreement test waits for D11 in
    /// raptorpath-core; this crate cannot call the engine's residual.)
    #[test]
    fn giveup_budget_vs_engine_seats_at_equal_r_is_bounded() {
        for r in [0.0, 1e-5, 1e-3, 0.02, 0.05, 0.2] {
            for k in 0..=100 {
                let rho = 0.9 + 0.001 * k as f64;
                let seat = if rho >= 1.0 { 0.0 } else { r };
                let gap = (giveup_budget(rho, r) - seat).abs();
                if rho >= 1.0 || 1.0 - rho >= r {
                    assert!(gap < 1e-15, "ρ={rho} R={r}: model must equal the seat, gap {gap}");
                } else {
                    assert!(gap <= r - (1.0 - rho) + 1e-15, "ρ={rho} R={r}: gap {gap}");
                }
            }
        }
    }

    /// Tests 4 + 8: the reduced form (give up after D(δ)) against the general
    /// form (give up after T_cut = RTT·max(0, ln x / ln ε̂), x = (1−ρ)/R),
    /// over the slider grid ρ = 0.900 .. 0.999 (100 positions) × presets ×
    /// cells. REPORTS where T_cut > D; asserts only the analytic bound
    /// (T_cut − D)⁺/RTT <= ln(1e-3/ε̂)/ln ε̂ (the supremum at x = 1e-3/ε̂).
    /// Also reports the bind fraction of each side of the min (FORMULA-FIRST
    /// clamp gauge): the share of positions where 1−ρ < R.
    #[test]
    fn giveup_reduced_vs_general_tcut_bound_and_bind_gauge() {
        for (name, dp) in PRESETS {
            for (eps, q, rtt) in CELLS {
                let r = cartoon_residual(eps, q, rtt, dp, 64.0);
                let d = span_deadline_d(dp, rtt);
                let bound = (1e-3 / eps).ln() / eps.ln();
                let (mut over, mut worst, mut rho_binds) = (0, 0.0f64, 0);
                for k in 0..100 {
                    let rho = 0.9 + 0.001 * k as f64;
                    let x = (1.0 - rho) / r.max(1e-300);
                    let t_cut = rtt * (x.ln() / eps.ln()).max(0.0);
                    let excess = ((t_cut - d) / rtt).max(0.0);
                    if excess > 0.0 { over += 1; }
                    worst = worst.max(excess);
                    if 1.0 - rho < r { rho_binds += 1; }
                }
                assert!(worst <= bound + 1e-9, "{name} ε={eps}: excess {worst} > bound {bound}");
                println!(
                    "GAUGE {name:8} cell ε={eps} q={q} rtt={rtt}: R={r:.3e} \
                     1-rho binds {rho_binds}/100, R binds {}/100 | T_cut>D at {over}/100, \
                     worst (T_cut-D)+/RTT = {worst:.3} (bound {bound:.3})",
                    100 - rho_binds
                );
            }
        }
    }

    /// Test 5: the sim's rule change at Auto and Realtime, on cells where
    /// budget·src_total >= 1: given_up > 0 at ρ < 1, 0 at ρ = 1; every
    /// give-up is older than D(δ) and within the budget at that moment.
    /// Reports how many were given up before their first retransmit.
    #[test]
    fn sim_gives_up_by_the_law_at_auto_and_realtime() {
        for (name, dp) in [("auto", 0.5), ("realtime", 50.0)] {
            for (eps, q, rtt) in [(0.20, 0.1, 100u32), (0.30, 0.1, 100)] {
                let run = |rho: f64| {
                    let mut s = Simulation::new(
                        eps, q, rtt, 64, "continuum".into(), None, Some(dp), Some(rho));
                    let mut maxcap = 0.0f64;
                    while !s.is_finished() && s.get_tick() < 20_000 {
                        s.step();
                        maxcap = maxcap.max(s.get_giveup_budget() * s.get_total_src() as f64);
                    }
                    assert!(s.is_finished());
                    (s, maxcap)
                };
                let (s, maxcap) = run(0.95);
                assert!(maxcap >= 1.0, "{name} ε={eps}: cell must admit a symbol (cap {maxcap})");
                assert!(s.get_given_up() > 0, "{name} ε={eps}: ρ<1 must give up");
                let d = s.get_giveup_deadline();
                let mut before_retx = 0;
                for &(age, retxd, count, cap) in &s.giveup_log {
                    assert!(age as f64 > d, "{name}: gave up at age {age} <= D {d}");
                    assert!(count as f64 <= cap + 1e-9, "{name}: count {count} over budget {cap}");
                    if !retxd { before_retx += 1; }
                }
                assert_eq!(s.giveup_log.len() as u32, s.get_given_up());
                println!(
                    "GIVEUP {name} eps={eps} q={q} rtt={rtt}: given_up={} (max cap {maxcap:.2}), \
                     before first retransmit {before_retx}, D={d}",
                    s.get_given_up()
                );
                let (s1, _) = run(1.0);
                assert_eq!(s1.get_given_up(), 0, "{name} ε={eps}: ρ = 1 must never give up");
            }
        }
    }
}
