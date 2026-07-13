use raptorpath_math as math;
use wasm_bindgen::prelude::*;

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

/// Continuous r* (paper 8.4): quantile at 1 - delta/eps; glides to 0.
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

/// Derived encoder window W* (paper 8.8): balances overhead (1/sqrt(W)
/// margin), recovery latency (W/send_rate within budget), and burst
/// absorbency. Clamped to [16, 512]. See `math::derive_window`.
#[wasm_bindgen]
pub fn derive_window(
    delta: f64, epsilon: f64, sigma2: f64, srtt: f64, send_rate: f64, latency_budget: f64,
) -> f64 {
    math::derive_window(delta, epsilon, sigma2, srtt, send_rate, latency_budget)
}

/// Soft saturation cap (paper 14.21.1): kink-free approach to r_sat.
#[wasm_bindgen]
pub fn soft_saturate(rate: f64, r_sat: f64) -> f64 {
    math::soft_saturate(rate, r_sat)
}

/// Saturation pressure in [0,1] (paper 14.21.1): the continuous indicator
/// that supersedes the binary CAP BINDING badge. `rate_requested` is the
/// UNCAPPED controller output.
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

/// The SHARED production rate formula (raptorpath-math::controller_rate) —
/// the exact code path the real transport runs. Flat-args wrapper for JS.
/// `completion_exposure` is the P6 chi in [0,1] (paper 14.26); pass 0 for
/// mid-stream / unknown T_rem.
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
        // #46 (paper 8.4.1): the flat JS wrapper carries no measured
        // window-mass statistics; the tail term is inert without them
        // (identical to production cold start).
        mass: math::MassStats::default(),
        tail_provision: true,
        // Paper 14.28: the visualizer models a raw transfer (file-transfer
        // semantics — no latency-feedback payload inside), so the
        // inner-feedback repair floor stays off (as does production by
        // default after the negative L1 C2/C3 ablation).
        inner_feedback: 0.0,
        saturation_cap, max_overhead,
    })
}

/// Completion-exposure kernel chi(T_rem) (paper 14.26): the probability a
/// loss NOW can no longer hide behind ongoing sends.
#[wasm_bindgen]
pub fn completion_exposure(t_rem_secs: f64, srtt_secs: f64, rttvar_secs: f64) -> f64 {
    math::completion_exposure(t_rem_secs, srtt_secs, rttvar_secs)
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
// Simulation engine — runs the REAL improved algorithm:
//   - rate from the SHARED production formula (math::controller_rate):
//     continuous z_{delta/eps} margin, hint tail targets, Bulk pure-ARQ,
//     burst B/T term, saturation cap (paper 8.4 / 12.5 / 14.21)
//   - correction preemption over new source (paper C.1)
//   - per-slot repair/retransmit mix by P_lost (paper 5.4)
//   - honest feedback: the estimator learns outcomes one RTT after send,
//     fed with the TRUE loss pattern (record_counts + record_symbol)
//   - completion-tail FEC: for Bulk, the continuous completion-exposure
//     ramp (paper 14.26 — the sim KNOWS T_rem, so chi is fed per tick);
//     for the other hints, the one-shot end-of-stream burst (paper 14.25,
//     exact 8.7 DP), which the chi ramp subsumes as its limiting case
//
// MULTIPATH (paper Section 16, Reliable Windowed Multipath — L0 prototype):
// the Simulation carries N independent GE channels (own calibrated state
// array, own seed, own capacity, own RTT/feedback delay, own estimator)
// POURING INTO ONE shared reliable window (one RlcEncoder/RlcDecoder).
// Source symbols are striped across paths proportional to the ESTIMATED
// per-path goodput g_i = capacity_i * (1 - eps_hat_i) (estimated, not true —
// the sender cannot see the channel; smooth weighted round-robin);
// repairs/retransmits go preferentially to the highest-goodput path with a
// spare slot (the Section 13.8 preference), and a symbol lost on path i may
// be repaired via ANY path — the in-order frontier advances when the window
// prefix decodes from any combination of arrivals (Section 16.3: frontier
// rate -> sum of g_i). Controller inputs (p_upper, sigma2, SRTT, ...) are
// capacity-weighted aggregates of the per-path estimators: the window sees
// a mixture channel. N = 1 reduces exactly to the single-path simulation
// (bit-identical: same seed derivation, same RNG stream, same slot order).
//
// Simplifications vs the L0 gate (documented): fixed wire capacity
// (slots/tick) instead of a queue+Copa model, no jitter (so no encoder lag
// needed — each path's channel is FIFO), delivery one-way latency uses the
// capacity-weighted mean SRTT.
// =========================================================================

const BASE_TAIL_TARGET: f64 = 1e-5;
const CODEC_OVERHEAD_RLC: f64 = 0.004;
const MAX_OVERHEAD: f64 = 0.5;
const TICK_SECS: f64 = 0.001;

/// Per-path state (paper Section 16): an independent GE channel with its own
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
    /// Wire outcomes awaiting THIS path's ACK round-trip: (send_tick, arrived).
    feedback_queue: std::collections::VecDeque<(u32, bool)>,
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
}

#[wasm_bindgen]
pub struct Simulation {
    paths: Vec<PathState>,
    /// Capacity-weighted true burst variance factor (aggregate).
    sigma2_true: f64,
    hint_bulk: bool,
    tail_target: f64,
    fixed_r: Option<f64>,
    /// Ablation-only: revert Bulk to the pre-P6 mapping (delta_eff =
    /// min(0.1, p_hat) + one-shot tail burst). Never exposed to JS; set
    /// directly by native tests to isolate the completion-exposure change.
    legacy_bulk_delta: bool,
    /// Ablation-only (paper 14.29): revert NON-Bulk hints to the pre-14.29
    /// one-shot end-of-stream burst (disabling the continuous completion
    /// ramp), to isolate the taper-truncation fix. Never exposed to JS.
    legacy_tail_burst: bool,
    /// Reliability target (rho). Below 1.0, losses older than T_cut are
    /// given up (paper 6.1 age eviction) — the third triangle corner.
    rho: f64,
    given_up: u32,
    /// Seqs the receiver has pruned (paper 6.2): late data for them is
    /// discarded, not delivered.
    given_up_seqs: std::collections::BTreeSet<u64>,
    /// Send tick per source seq (for delivery-latency measurement).
    send_tick: Vec<u32>,
    /// Delivery latency (ms) per delivered source symbol, in delivery order:
    /// (recovery_tick - send_tick) + one-way propagation.
    delivered_lat_ms: Vec<f64>,
    /// Delivery latency (ms) indexed BY SOURCE SEQ (NaN until delivered).
    /// Used to compare last-window vs mid-stream tail latency (paper 14.29:
    /// the end-of-stream reliability cliff must vanish).
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
    /// Last completion-exposure chi (paper 14.29): the Stieltjes metering of
    /// the completion-tail budget accrues B_tail x (chi - prev_chi).
    prev_completion_chi: f64,

    // Counters
    total_src: u32, total_fec: u32, total_arq: u32, total_lost: u32,
    cum_sent: u32, cum_arrived: u32, cum_decoded: u32,
    lost_pending: u32,
    last_src: u32, last_fec: u32, last_arq: u32, last_lost: u32,

    rng_state: u64,
}

struct Symbol {
    tick: u32,
    seq: u64,
    lost: bool,
    recovered: bool,
    last_retx_tick: i64,
    /// Path the symbol was LAST sent on (original send, then updated per
    /// retransmit): its RTT gates the retry timer and the SACK ack, its
    /// estimator feeds the P_lost retransmit decision.
    path: usize,
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

    /// Consume the next wire-slot channel outcome on path `pi`. true = lost.
    fn wire_lost(&mut self, pi: usize) -> bool {
        let p = &mut self.paths[pi];
        let lost = p.channel_states.get(p.wire_idx).copied().unwrap_or(false);
        p.wire_idx += 1;
        lost
    }

    // --- Capacity-weighted aggregates over paths (paper 16.3: the shared
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
    fn agg_q(&self) -> f64 {
        self.agg(|p| p.q)
    }
    fn agg_srtt(&self) -> f64 {
        self.agg(|p| p.srtt_secs)
    }
    fn agg_rttvar(&self) -> f64 {
        self.agg(|p| p.rttvar_secs)
    }
    fn agg_rtt_ticks(&self) -> u32 {
        self.agg(|p| p.rtt_ticks as f64).round() as u32
    }
    fn agg_loss_est(&self) -> f64 {
        self.agg(|p| p.estimator.loss_rate())
    }
    fn agg_p_upper(&self) -> f64 {
        self.agg(|p| p.estimator.predictive_loss_upper(0.95))
    }

    /// Highest-estimated-goodput path with a spare slot this tick: repairs
    /// and retransmits prefer the best path (the Section 13.8 preference —
    /// corrections ride the path most likely to deliver them).
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

    /// Completion exposure chi (paper 14.26): the sim KNOWS the transfer
    /// length, so T_rem = remaining source symbols / send rate. The send
    /// rate is approximated by the wire capacity (Bulk mid-stream is
    /// nearly all source). Computed for EVERY hint — mid-stream it is 0, and
    /// over the final ~1.5 SRTT it ramps to 1 (Phi_bar of the RTT tail).
    fn completion_chi_raw(&self) -> f64 {
        let remaining = self.num_source.saturating_sub(self.next_seq) as f64;
        let t_rem_secs = remaining / self.total_capacity() as f64 * TICK_SECS;
        math::completion_exposure(t_rem_secs, self.agg_srtt(), self.agg_rttvar())
    }

    /// Bulk's chi for the delta glide (paper 14.26): only Bulk maps chi into
    /// delta_eff. The legacy-ablation arm keeps chi = 0 (old one-shot burst).
    fn completion_chi(&self) -> f64 {
        if !self.hint_bulk || self.legacy_bulk_delta {
            return 0.0;
        }
        self.completion_chi_raw()
    }

    /// Completion-tail debt increment for NON-Bulk hints (paper Section
    /// 14.29: the taper-truncation completion term). Near a known
    /// end-of-stream the final window's symbols never receive their FUTURE
    /// repairs — the taper integral is truncated (Section 4.2 note) — so
    /// their late-window coverage is cut and tail losses fall to serial ARQ.
    ///
    /// The fix delivers the SAME budget as the 14.25 one-shot burst,
    /// B_tail = r_tail x W repairs (r_tail = the exact-DP rate meeting
    /// delta_hint on the final window, Section 8.7), but METERED OUT
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
    /// Bulk uses its delta glide instead (returns 0); fixed-r and the legacy
    /// ablation keep the one-shot burst (return 0).
    fn completion_debt_increment(&mut self) -> f64 {
        if self.hint_bulk || self.legacy_tail_burst || self.fixed_r.is_some() {
            return 0.0;
        }
        let remaining = self.num_source.saturating_sub(self.next_seq) as f64;
        let w = self.w as f64;
        let chi_trunc = math::normal_survival((remaining - 0.5 * w) / (0.25 * w));
        let dchi = (chi_trunc - self.prev_completion_chi).max(0.0);
        self.prev_completion_chi = chi_trunc;
        if dchi <= 0.0 {
            return 0.0;
        }
        let (pg, qg) = self.agg_ge_params();
        let r_tail = math::compute_r_star_exact(pg, qg, self.w as usize, self.tail_target);
        r_tail * w * dchi
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
    /// state (identical to the real transport's FecRateController path).
    /// `saturation_cap` is a parameter so the pressure accessor can request
    /// the UNCAPPED rate.
    fn rate_inputs(&self, saturation_cap: bool) -> math::RateInputs {
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
                    p.rtt_ticks as f64 * p.capacity as f64
                } else {
                    0.0
                }
            })
            .sum();
        let t_symbols = if t_symbols_raw > 0.0 { t_symbols_raw.max(1.0) } else { 0.0 };
        let p_upper = self.agg_p_upper();
        // Ablation arm: the pre-P6 Bulk mapping delta_eff = min(0.1, p_hat)
        // expressed through the plain tail_target path (equivalent by
        // construction; see test_ablation_p6_completion_exposure).
        let (tail_target, bulk_late_is_fine) = if self.legacy_bulk_delta && self.hint_bulk {
            ((0.1f64).min(p_upper), false)
        } else {
            (self.tail_target, self.hint_bulk)
        };
        math::RateInputs {
            p_upper,
            sigma2,
            mean_burst,
            // #46 (paper 8.4.1): the sim aggregates several per-path
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
            completion_exposure: self.completion_chi(),
            // Paper 14.28: the sim's payload IS the transfer (file-transfer
            // semantics) — its delivery latency does not feed back into its
            // own throughput, so the inner-feedback repair floor stays off
            // and mid-stream Bulk remains pure ARQ (production defaults to
            // 0 too after the negative L1 C2/C3 ablation).
            inner_feedback: 0.0,
            saturation_cap,
            max_overhead: MAX_OVERHEAD,
        }
    }

    /// Rate from the SHARED production formula — identical code to the
    /// real transport's FecRateController (via raptorpath-math).
    fn controller_rate_now(&self) -> f64 {
        if let Some(r) = self.fixed_r {
            return r;
        }
        math::controller_rate(&self.rate_inputs(true))
    }

    /// Send one repair symbol on path `pi`. Returns whether it survived.
    fn send_repair(&mut self, pi: usize) -> bool {
        let repair = self.encoder.generate_repair();
        let lost = self.wire_lost(pi);
        self.total_fec += 1;
        self.cum_sent += 1;
        let tick = self.tick;
        {
            let p = &mut self.paths[pi];
            p.sent += 1;
            p.fec += 1;
            p.feedback_queue.push_back((tick, !lost));
            if lost {
                p.lost += 1;
            } else {
                p.arrived += 1;
            }
        }
        if !lost {
            self.cum_arrived += 1;
            let recovered = self.decoder.feed_repair(
                repair.window_start, repair.window_count,
                repair.repair_index, &repair.coded_data,
            );
            self.mark_recovered(&recovered);
            self.record_delivery(&recovered);
            self.cum_decoded += recovered
                .iter()
                .filter(|q| !self.given_up_seqs.contains(q))
                .count() as u32;
        }
        !lost
    }

    /// Record delivery latencies for decoder outputs (excluding pruned
    /// seqs). Latency = time from send to decode plus one-way propagation
    /// (arrived symbols decode at their send tick, so their latency is the
    /// one-way delay; recovered symbols add the recovery wait).
    fn record_delivery(&mut self, seqs: &[u64]) {
        let one_way = self.agg_srtt() * 500.0; // ms: RTT/2 (capacity-weighted)
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
            if let Some(slot) = self.lat_by_seq.get_mut(*q as usize) {
                if slot.is_nan() {
                    *slot = lat;
                }
            }
        }
    }

    fn mark_recovered(&mut self, seqs: &[u64]) {
        for rseq in seqs {
            for sym in &mut self.symbols {
                if sym.seq == *rseq && sym.lost && !sym.recovered {
                    sym.recovered = true;
                    self.lost_pending = self.lost_pending.saturating_sub(1);
                }
            }
        }
    }
}

#[wasm_bindgen]
impl Simulation {
    /// hint: "bulk" | "auto" | "realtime" | "fixed" (uses fixed_r as r) |
    /// "custom" (uses custom_delta + custom_rho — the full triangle).
    /// rho < 1.0 enables T_cut age eviction (losses older than T_cut are
    /// given up; reliability becomes the emergent third variable).
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

    /// Multipath constructor (paper Section 16, RWM at L0): N independent GE
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
        assert!(
            q.len() >= n && rtt_ms.len() >= n && capacity.len() >= n,
            "per-path parameter arrays must have equal length"
        );

        // Hint -> tail target, mirroring the production constructor.
        let (tail_target, hint_bulk) = match hint.as_str() {
            "bulk" => ((BASE_TAIL_TARGET * 100.0).clamp(1e-9, 0.1), true),
            "realtime" => ((BASE_TAIL_TARGET * 0.01).clamp(1e-9, 0.1), false),
            "custom" => (custom_delta.unwrap_or(BASE_TAIL_TARGET).clamp(1e-9, 0.1), false),
            _ => (BASE_TAIL_TARGET, false), // auto / fixed
        };
        let fixed_r = if hint == "fixed" { Some(fixed_r.unwrap_or(0.1)) } else { None };
        let rho = if hint == "custom" {
            custom_rho.unwrap_or(1.0).clamp(0.9, 1.0)
        } else {
            1.0
        };

        let num_source = 2000u32;
        let mut paths = Vec::with_capacity(n);
        for i in 0..n {
            let (e, qq) = (eps[i], q[i]);
            let rtt = rtt_ms[i];
            let cap = capacity[i].clamp(1, 16);
            let p = if e < 1.0 { e * qq / (1.0 - e) } else { qq };
            // Path 0 keeps EXACTLY the historical seed (i * salt == 0);
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

        Self {
            paths,
            sigma2_true,
            hint_bulk, tail_target, fixed_r,
            legacy_bulk_delta: false,
            legacy_tail_burst: false,
            rho,
            given_up: 0,
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
            total_src: 0, total_fec: 0, total_arq: 0, total_lost: 0,
            cum_sent: 0, cum_arrived: 0, cum_decoded: 0,
            lost_pending: 0,
            last_src: 0, last_fec: 0, last_arq: 0, last_lost: 0,
            rng_state: 0xdeadbeef12345678,
        }
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
        // with the TRUE per-symbol pattern (paper 7.5); flushed once per
        // that path's RTT — each estimator honestly lags its own path.
        for pi in 0..self.paths.len() {
            let tick = self.tick;
            let p = &mut self.paths[pi];
            while let Some(&(t, ok)) = p.feedback_queue.front() {
                if tick.saturating_sub(t) < p.rtt_ticks {
                    break;
                }
                p.feedback_queue.pop_front();
                p.estimator.record_symbol(ok);
                p.pending_sent += 1;
                if ok {
                    p.pending_ok += 1;
                }
            }
            if tick.saturating_sub(p.last_flush_tick) >= p.rtt_ticks && p.pending_sent > 0 {
                p.estimator.record_counts(p.pending_sent, p.pending_ok, tick as u64);
                p.pending_sent = 0;
                p.pending_ok = 0;
                p.last_flush_tick = tick;
            }
        }

        // --- Rate from the shared production formula ---
        self.rate = self.controller_rate_now();

        // --- Wire slots ---
        // Per-slot priority (paper C.1 + 5.4): (1) a P_lost-confirmed
        // retransmit — ARQ is driven by loss confidence, INDEPENDENT of the
        // FEC budget (Bulk's r ~ 0 must not delay recovery to end of
        // stream); (2) a repair when the taper debt says one is due;
        // (3) new source. Retransmits scan ALL outstanding candidates so a
        // single symbol waiting out its retry timer cannot head-of-line
        // stall the drain.
        //
        // MULTIPATH (paper 16.3): the tick's slot budget is the union of all
        // paths' capacities. Each action then picks its carrying path:
        // corrections (repairs AND retransmits — a symbol lost on path i may
        // be resent on any path j) ride the highest-goodput path with a
        // spare slot (13.8 preference); source symbols are striped across
        // spare-slot paths proportional to ESTIMATED goodput g_i (smooth
        // WRR). With N = 1 every choice is path 0 and the loop is exactly
        // the historical single-path loop.
        let mut free: Vec<u32> = self.paths.iter().map(|p| p.capacity).collect();
        let total_slots: u32 = free.iter().sum();
        for _ in 0..total_slots {
            // (1) P_lost-gated retransmit across all candidates. Retry timer
            // and loss belief come from the path the symbol was LAST sent on
            // (that is where its feedback lives).
            let mut did_retx = false;
            let mut cand: Option<usize> = None;
            for (i, sym) in self.symbols.iter().enumerate() {
                if sym.lost
                    && !sym.recovered
                    && (self.tick as i64 - sym.last_retx_tick)
                        >= self.paths[sym.path].rtt_ticks as i64
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
                    self.paths[sp].srtt_secs,
                    self.paths[sp].rttvar_secs,
                );
                if self.rng() < pl {
                    // Cross-path recovery: resend the EXACT stored bytes on
                    // the best currently-available path, not necessarily the
                    // one that lost them.
                    let dest = self.best_correction_path(&free);
                    let seq = self.symbols[i].seq;
                    self.symbols[i].last_retx_tick = self.tick as i64;
                    self.symbols[i].path = dest;
                    let lost = self.wire_lost(dest);
                    free[dest] -= 1;
                    self.total_arq += 1;
                    arq_n += 1;
                    self.cum_sent += 1;
                    {
                        let p = &mut self.paths[dest];
                        p.sent += 1;
                        p.arq += 1;
                        p.feedback_queue.push_back((self.tick, !lost));
                        if lost {
                            p.lost += 1;
                        } else {
                            p.arrived += 1;
                        }
                    }
                    if lost {
                        lost_n += 1;
                    } else {
                        self.cum_arrived += 1;
                        let data = self.source_store[seq as usize].clone();
                        let rec = self.decoder.feed_source(seq, &data);
                        // Cascade outputs may resolve OTHER lost symbols;
                        // pruned (given-up) seqs are discarded, not counted.
                        self.mark_recovered(&rec);
                        self.record_delivery(&rec);
                        self.cum_decoded += rec
                            .iter()
                            .filter(|q| !self.given_up_seqs.contains(q))
                            .count() as u32;
                        self.symbols[i].recovered = true;
                        self.lost_pending = self.lost_pending.saturating_sub(1);
                    }
                    did_retx = true;
                }
            }
            if did_retx {
                continue;
            }

            // (2) Repair when the taper debt says one is due — on the best
            // available path (repairs are path-agnostic: ANY path's arrival
            // advances the shared window decode).
            if self.debt >= 1.0 && self.encoder.window_size() > 0 {
                let dest = self.best_correction_path(&free);
                self.debt -= 1.0;
                let ok = self.send_repair(dest);
                free[dest] -= 1;
                fec_n += 1;
                if !ok {
                    lost_n += 1;
                }
            } else if !self.source_done {
                // --- Source slot: striped proportional to estimated goodput ---
                let dest = self.next_source_path(&free);
                let data = vec![self.next_seq as u8; 8];
                let seq = self.encoder.add_source(&data);
                self.source_store.push(data.clone());
                self.send_tick.push(self.tick);
                // Slide the encoder window (paper W).
                if self.encoder.window_size() > self.w as usize {
                    let oldest = self.encoder.next_seq().saturating_sub(self.w as u64);
                    self.encoder.advance(oldest);
                }
                let lost = self.wire_lost(dest);
                free[dest] -= 1;
                {
                    let p = &mut self.paths[dest];
                    p.sent += 1;
                    p.src += 1;
                    p.feedback_queue.push_back((self.tick, !lost));
                    if lost {
                        p.lost += 1;
                    } else {
                        p.arrived += 1;
                    }
                }
                self.symbols.push(Symbol {
                    tick: self.tick,
                    seq,
                    lost,
                    recovered: false,
                    last_retx_tick: -1_000_000,
                    path: dest,
                });
                self.total_src += 1;
                src_n += 1;
                self.cum_sent += 1;
                // Steady-state debt accrual: r per SOURCE symbol — the
                // aggregate correction rate is taper-shape-invariant
                // (paper 4.2). Plus the completion-tail term (paper 14.29):
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
                } else {
                    self.cum_arrived += 1;
                    let rec = self.decoder.feed_source(seq, &data);
                    // Cascade outputs may resolve OTHER lost symbols.
                    self.mark_recovered(&rec);
                    self.record_delivery(&rec);
                    self.cum_decoded += rec
                        .iter()
                        .filter(|q| !self.given_up_seqs.contains(q))
                        .count() as u32;
                }
                self.next_seq += 1;
                if self.next_seq >= self.num_source {
                    self.source_done = true;
                    // Legacy one-shot completion-tail burst (paper 14.25):
                    // kept ONLY for the ablation arms (legacy Bulk delta, or
                    // legacy non-Bulk tail burst) that reproduce the pre-14.29
                    // behavior for A/B comparison. Every live hint now uses
                    // the continuous chi-driven completion term instead — Bulk
                    // via its delta glide (14.26), the others via the
                    // `completion_debt_increment` folded into the debt accrual
                    // above (14.29). Firing the burst on top of either would
                    // double-pay the tail budget.
                    let legacy_burst = (self.legacy_bulk_delta && self.hint_bulk)
                        || (self.legacy_tail_burst && !self.hint_bulk);
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

        // T_cut age eviction (paper 6.1): for rho < 1.0, losses older than
        // T_cut are given up — reliability bends instead of latency.
        if self.rho < 1.0 {
            let p_up = self.agg_loss_est().clamp(1e-4, 0.99);
            let sig = self.get_sigma2_est();
            let t_cut = math::find_t_cut(
                p_up, self.agg_q(), self.rate.max(1e-3),
                self.encoder.window_size().max(1) as f64, sig, self.rho,
            );
            if t_cut.is_finite() {
                let cut_ticks = (t_cut as u32).max(self.agg_rtt_ticks() * 2);
                let now = self.tick;
                for sym in &mut self.symbols {
                    if sym.lost && !sym.recovered && now.saturating_sub(sym.tick) > cut_ticks {
                        sym.recovered = true; // given up
                        self.given_up += 1;
                        self.given_up_seqs.insert(sym.seq);
                        self.lost_pending = self.lost_pending.saturating_sub(1);
                    }
                }
            }
        }

        // ACK arrived-but-unmarked symbols after one RTT of THEIR path
        // (SACK view; per-path ack delay).
        let tick = self.tick;
        {
            let paths = &self.paths;
            for sym in &mut self.symbols {
                if !sym.lost
                    && !sym.recovered
                    && tick.saturating_sub(sym.tick) >= paths[sym.path].rtt_ticks
                {
                    sym.recovered = true;
                }
            }
        }

        // Finish conditions (rho = 100%: retransmit until everything is in;
        // rho < 100%: given-up symbols count as resolved, not delivered).
        if self.source_done && self.cum_decoded + self.given_up >= self.num_source {
            self.finished = true;
        }
        if self.source_done
            && self.lost_pending == 0
            && !self.symbols.iter().any(|s| s.lost && !s.recovered)
        {
            self.finished = true;
        }
        if self.tick > 20_000 {
            self.finished = true;
        }

        // Prune resolved symbols (keep lost unrecovered for ARQ).
        self.symbols
            .retain(|s| (s.lost && !s.recovered) || tick.saturating_sub(s.tick) < 10);

        self.last_src = src_n;
        self.last_fec = fec_n;
        self.last_arq = arq_n;
        self.last_lost = lost_n;
        self.tick += 1;
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

    // --- Multipath accessors (paper Section 16 — RWM at L0) ---
    pub fn get_num_paths(&self) -> usize { self.paths.len() }
    pub fn get_path_capacity(&self, i: usize) -> u32 {
        self.paths.get(i).map_or(0, |p| p.capacity)
    }
    pub fn get_path_rtt_ms(&self, i: usize) -> u32 {
        self.paths.get(i).map_or(0, |p| p.rtt_ticks)
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
    /// Measured AGGREGATE delivery goodput so far: decoded source symbols
    /// per tick. After completion this is the completion goodput
    /// num_source / completion_ticks.
    pub fn get_agg_goodput(&self) -> f64 {
        if self.tick == 0 { return 0.0; }
        self.cum_decoded as f64 / self.tick as f64
    }
    /// The BEST single path's TRUE goodput max_i capacity_i * (1 - eps_i)
    /// (symbols/tick): the Section 16.2 resequencing ceiling for every
    /// per-path-affine in-order transport on this path set.
    pub fn get_best_single_goodput(&self) -> f64 {
        self.paths.iter().map(|p| p.goodput_true()).fold(0.0, f64::max)
    }
    /// THE Section 16 readout: measured aggregate goodput over the best
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
    /// Effective tail target after the hint mapping. Bulk (paper 14.26):
    /// the completion-exposure glide p̂ + (0.05 − p̂)·χ — equals p̂
    /// mid-stream (pure ARQ) and the 14.25 tail budget as χ → 1.
    pub fn get_delta_eff(&self) -> f64 {
        if self.hint_bulk {
            let p = self.get_p_upper();
            p + (math::BULK_TAIL_BUDGET - p) * self.completion_chi()
        } else {
            self.tail_target
        }
    }
    /// Live completion exposure chi (paper 14.26): 0 mid-stream, ramps to 1
    /// over the final ~1.5 SRTT of the transfer. This is Bulk's wall-time
    /// glide kernel (delta_eff); non-Bulk hints refill the truncated taper
    /// via the SOURCE-POSITION completion term (paper 14.29), a separate
    /// metering, so this display stays 0 for them.
    pub fn get_completion_exposure(&self) -> f64 {
        self.completion_chi()
    }
    /// Saturation cap for the current estimator state (paper 14.21).
    pub fn get_r_sat(&self) -> f64 {
        math::r_saturation(
            self.get_p_upper(),
            self.get_sigma2_est(),
            self.encoder.window_size().max(1) as f64,
            self.agg_srtt(),
            TICK_SECS / self.total_capacity() as f64,
        )
    }
    /// Derived encoder window W* (paper 8.8) for the current estimator state
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

    /// Saturation pressure in [0, 1] (paper 14.21.1): the continuous
    /// indicator that supersedes the binary CAP BINDING badge. 0 = far below
    /// the cap (more FEC still helps), 0.5 = exactly at r_sat, ->1 = the cap
    /// is holding r near r_sat. Computed from the UNCAPPED request (what the
    /// hint asked for before the soft cap) vs r_sat.
    pub fn get_saturation_pressure(&self) -> f64 {
        if self.fixed_r.is_some() {
            return 0.0;
        }
        let uncapped = math::controller_rate(&self.rate_inputs(false));
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
    /// phase (as an earlier version did) undercounts, because corrections
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
mod tests {
    use super::*;

    /// N = 1 identical-behavior regression: golden fingerprints captured
    /// from the PRE-multipath simulation (commit 4663c38, the last
    /// single-path engine). Both the legacy constructor and the multipath
    /// constructor with one path must reproduce them EXACTLY — the
    /// multipath refactor may not perturb single-path behavior by one
    /// tick or one symbol.
    #[test]
    fn test_multipath_n1_identical_golden() {
        // (hint, eps, q, rtt) -> (ticks, src, fec, arq, lost, decoded, sent, arrived)
        let goldens: [(&str, f64, f64, u32, [u32; 8]); 6] = [
            ("auto",     0.05, 0.5, 50, [672, 2000, 668,  12,  100, 2000, 2680, 2542]),
            ("auto",     0.10, 0.3, 80, [750, 2000, 984,  15,  183, 2000, 2999, 2719]),
            ("bulk",     0.05, 0.5, 50, [562, 2000, 0,    116, 114, 2000, 2116, 2000]),
            ("bulk",     0.10, 0.3, 80, [619, 2000, 85,   190, 197, 2000, 2275, 2061]),
            ("realtime", 0.05, 0.5, 50, [680, 2000, 710,  9,   98,  2000, 2719, 2577]),
            ("realtime", 0.10, 0.3, 80, [757, 2000, 1014, 14,  179, 2000, 3028, 2747]),
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
                assert_eq!(
                    got, g,
                    "N=1 regression (arm {ai}) {hint} eps={eps} q={q} rtt={rtt}: \
                     [ticks,src,fec,arq,lost,decoded,sent,arrived] diverged from \
                     the pre-multipath engine"
                );
            }
        }
    }

    /// Section 16 P2-analog at L0: two SYMMETRIC paths through ONE shared
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
            assert!(
                factor > 1.7 && factor <= 2.1,
                "[{hint}] symmetric aggregation x{factor:.2} outside ~1.8-2x"
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
    /// (paper 6.1/6.2): rho < 1 gives up losses older than T_cut (counted,
    /// receiver-pruned), rho = 1 retains until acked — same triangle
    /// semantics as single-path, now across N paths.
    #[test]
    fn test_multipath_custom_rho_semantics() {
        // rho < 1: bounded retention — give-ups counted, reliability >= target.
        let mut lossy = Simulation::multipath(
            vec![0.10, 0.10], vec![0.3, 0.3], vec![80, 80], vec![4, 4],
            64, "custom".into(), None, Some(0.05), Some(0.95),
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
            64, "custom".into(), None, Some(0.05), Some(1.0),
        );
        run_to_end(&mut full);
        assert_eq!(full.get_given_up(), 0, "rho=1 must never give up");
        assert_eq!(full.get_cum_decoded(), full.get_num_source());
    }

    /// Section 16.6 P1 at L0 (C8-like heterogeneity, scaled to sim units:
    /// 100 Mbit/10 ms/2.6% -> cap 5, and 20 Mbit/40 ms/4.8% -> cap 1):
    /// aggregate goodput of the shared window over BOTH paths must be
    /// STRICTLY greater than the fast path alone — the (16.2) resequencing
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

    // Paper 8.8 quality check: derived W vs a fixed W = 64 across the three
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
        // completion must BEAT Realtime's (which pays r ~ 20%+ of the wire
        // for corrections). Mirrors the L0 gate result (0.163s vs 0.187s).
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

    #[test]
    fn test_custom_rho_gives_up_late_losses() {
        // Custom triangle mode: rho < 1 -> T_cut eviction; some symbols are
        // given up instead of retransmitted forever.
        let mut sim = Simulation::new(
            0.10, 0.3, 80, 64, "custom".into(), None, Some(0.05), Some(0.95),
        );
        while !sim.is_finished() && sim.get_tick() < 20_000 { sim.step(); }
        assert!(sim.is_finished());
        assert!(sim.get_reliability() >= 0.90, "reliability {}", sim.get_reliability());
        assert_eq!(
            sim.get_cum_decoded() + sim.get_given_up(),
            sim.get_num_source()
        );
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
        // P6 acceptance (paper 14.26): the old min(0.1, p_hat) mapping lost
        // to a fixed r = 0.01 floor on completion in 20/24 grid cells
        // (median +5%) and on overhead in 24/24 (excess overhead 2-14% vs
        // ~0-1%) — the M1 cold-start pin at max_overhead plus the M2
        // permanent-FEC leak. With the completion-exposure glide, Bulk must
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

        // M2 cell (eps = 0.10 >= the old 0.1 clamp): the old mapping paid
        // permanent FEC ~ the IT floor on top of ARQ (excess overhead
        // 8-14%); the chi glide must keep excess overhead under 2%.
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
        // vs the P6 chi glide, same seeds (the channel seed derives from
        // eps/q/rtt only). Run with --nocapture to record the deltas.
        //
        // The rtt=150 cell is the documented horizon caveat (paper 14.26):
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
        // The reported user bug: "overhead below the channel floor
        // (theoretically impossible)". Two confirmed causes: (a) overhead
        // was measured over the steady phase only, dropping the drain-phase
        // corrections; (b) the nominal floor uses eps, but a finite run
        // samples only a prefix of the expectation-calibrated channel, so
        // realized loss can sit below eps. Fix: overhead counts ALL sends,
        // and the invariant is stated against the REALIZED floor -- which is
        // exact: decoding N source symbols requires >= N arrived symbols.
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
        // Paper 14.29: the taper integral is truncated for the final window's
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
            // No cliff: the last window's tail latency must not blow past the
            // mid-stream tail (allow a modest 1.6x band for the residual
            // serial-ARQ few at the very last symbols).
            assert!(
                tail_p99 <= mid_p99 * 1.6 + 5.0,
                "{hint} end-of-stream cliff: tail p99 {tail_p99:.1}ms vs mid {mid_p99:.1}ms"
            );
        }
    }

    #[test]
    fn test_ablation_completion_ramp_vs_burst() {
        // Paper 14.29: the continuous chi-driven completion ramp REPLACES the
        // pre-14.29 one-shot end-of-stream burst for non-Bulk hints. At high
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
                assert!(
                    ramp_tail <= burst_tail * 1.15 + 3.0,
                    "eps={eps} rtt={rtt} {hint}: ramp tail p99 {ramp_tail:.1} must not regress vs burst {burst_tail:.1}"
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
