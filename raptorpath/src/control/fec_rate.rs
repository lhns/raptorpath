//! FEC rate controller: computes optimal repair symbol count.
//!
//! Uses a principled budget architecture (ADR-0050):
//!
//! 1. **Predictive upper bound**: BOCD posterior quantile at fixed 95%
//!    confidence provides the loss rate estimate WITH built-in
//!    estimation-uncertainty margin. No separate PI controller needed.
//!
//! 2. **Protocol hint → tail quantile**: The protocol hint (Realtime/Bulk/Auto)
//!    maps to `target_tail_loss`, which sets z_δ in the r* margin (paper
//!    Section 8.4), controlling the FEC/NACK balance. Tighter tail = more
//!    proactive FEC = less NACK latency. No magic additive offsets.
//!
//!    The two margins cover distinct variance sources and do not stack on
//!    the same quantity: the BOCD quantile covers uncertainty about the
//!    TRUE loss rate (estimation), while z_δ covers channel stochasticity
//!    GIVEN that rate (window-tail variance).
//!
//! 3. **Budget allocation**: Total repair budget is split between proactive FEC
//!    and NACK-based reactive repair, coordinated to avoid double-spending.
//!
//! 4. **Spare capacity gate**: Repair rate is clamped to spare link capacity
//!    (cwnd - in_flight) to ensure FEC never causes congestion.

use super::estimator::LossEstimator;
use crate::fec::FecBackend;
use serde::{Deserialize, Serialize};

/// Protocol hint controls the latency/tail-reliability tradeoff.
///
/// The system targets 100% reliability — everything gets through via FEC or NACK.
/// FEC is proactive (zero added latency), NACK is reactive (costs one RTT).
/// The protocol hint controls WHERE on the latency/reliability curve we sit
/// by adjusting `target_tail_loss`:
///
/// - **Realtime**: 100× tighter tail → very aggressive FEC → minimal NACK latency
/// - **Bulk**: 100× looser tail → less FEC → rely on NACK → saves bandwidth
/// - **Auto**: unchanged target → balanced
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ProtocolHint {
    /// Real-time traffic (VoIP, gaming): tighter tail loss → more proactive FEC
    Realtime,
    /// Bulk transfer: looser tail loss → rely on NACK for residual
    Bulk,
    /// Auto-detect based on packet patterns
    Auto,
}

impl std::str::FromStr for ProtocolHint {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "realtime" | "rt" => Ok(Self::Realtime),
            "bulk" => Ok(Self::Bulk),
            "auto" => Ok(Self::Auto),
            _ => Err(anyhow::anyhow!("unknown protocol hint: {s}")),
        }
    }
}

impl ProtocolHint {
    /// The hint's tail-loss-target scale ζ: `effective_tail_loss =
    /// target_tail_loss × ζ` (the FEC/NACK balance knob, see
    /// `FecRateController::new_with_toggles`). ζ is the hint's ONE declared
    /// price ratio — Realtime prices a late symbol 100× dearer than Auto,
    /// Bulk 100× cheaper — and everything else hint-coupled should derive
    /// from it rather than adding independent magic constants. The Copa δ
    /// mapping (scheduler, paper §12.4) consumes it as the latency price:
    /// δ(hint) = δ_auto / ζ(hint).
    pub fn tail_loss_scale(self) -> f64 {
        match self {
            ProtocolHint::Realtime => 0.01, // 100× tighter tail
            ProtocolHint::Bulk => 100.0,    // 100× looser tail
            ProtocolHint::Auto => 1.0,
        }
    }
}

/// FEC rate controller.
pub struct FecRateController {
    /// Effective target tail loss probability (after hint adjustment)
    target_tail_loss: f64,
    /// Maximum FEC overhead as fraction of source symbols
    max_overhead: f64,
    /// Codec decode overhead factor (raw, before P(decoder_invoked) weighting)
    rq_overhead: f64,
    /// Protocol hint (stored for diagnostics)
    hint: ProtocolHint,
    /// Symbol size in bytes (needed to compute T = RTT × throughput / symbol_size)
    symbol_size: u16,
    /// P5: cap the repair rate at the p99(r) saturation point (paper
    /// Section 14.21). Past r_sat, extra repairs displace source symbols
    /// and stretch the recovery window faster than the shrinking FEC-miss
    /// cost pays back — more FEC hurts the tail.
    saturation_cap_enabled: bool,
    /// P4a/P6: Bulk maps the tail target to the completion-exposure glide
    /// δ_eff = ε̂ + (0.05 − ε̂)·χ (paper Section 14.26): mid-stream (χ = 0)
    /// δ_eff = ε̂ and r* = 0 identically — pure ARQ, volume parity with
    /// retransmission transports (paper Sections 5.3, 12.5) — and near a
    /// KNOWN end of stream χ → 1 ramps r to the 14.25 tail budget.
    /// Public setter exists for ablation.
    bulk_pure_arq: bool,
    /// P6: completion exposure χ ∈ [0, 1] (paper Section 14.26). The
    /// production tunnel is an ENDLESS stream — there is no known T_rem,
    /// so χ stays 0 and Bulk's steady state is pure ARQ; the existing
    /// production tail behavior is unchanged. Future work: feed this from
    /// an application-known transfer size, or an idle-onset heuristic
    /// (send-queue drained = provisional end of stream). Drivers that DO
    /// know T_rem (the L0 gate, the wasm sim) set it per tick via
    /// `set_completion_exposure` with `raptorpath_math::completion_exposure`.
    completion_exposure: f64,
    /// #46: window-mass burst-tail provisioning (paper Section 8.4.1).
    /// The GE geometric burst law under-provisions r* by 2-4x on real
    /// bursty traces (Section 2.5, MEASURED); when enabled, the rate
    /// includes the `r_star_mass` quantile term fed by the receiver's own
    /// multi-scale window loss-mass statistics (GilbertElliottEstimator
    /// mass_stats). Env gate RWM_RSTAR_TAIL (default ON — this is a
    /// correctness fix to the reliability contract; =0 restores the
    /// legacy GE-only provisioning for A/B).
    tail_provision: bool,
    /// P10a: inner-feedback weight in [0, 1] (paper Section 14.28). The
    /// Bulk glide's mid-stream r* = 0 prices VOLUME; when the payload is
    /// itself a latency-sensitive control loop (TCP inside the tunnel),
    /// each unrepaired loss stalls the inner flow's in-order delivery
    /// ~min(1.5×SRTT_outer, RTO_inner) and that stall feeds back into the
    /// inner send rate (L1 C2: ~20 events per 1.8 MB transfer). Weight 1
    /// enables the `inner_feedback_floor` mid-stream repair floor — the
    /// smallest r whose residual stall fraction sits within delivery-jitter
    /// noise; weight 0 (default) is the old pure-glide behavior, kept for
    /// FILE-TRANSFER payload semantics (the L0 gate driver, bench_suite):
    /// there the transfer is the payload and mid-stream ARQ recovery is
    /// genuinely free. The production tunnel ALSO defaults to 0 (config
    /// `inner_feedback_weight`): the L1 C2/C3 ablation measured the floor
    /// active (FEC volume 2.5% -> 4.7%) but completion-neutral at C2 and
    /// 28% regressive at C3 — post-14.27/P9b the inner flow absorbs the
    /// residual stalls, and floor repairs displace source symbols inside
    /// the same inner-limited loop (paper 14.28, L1 verification).
    inner_feedback: f64,
}

impl FecRateController {
    /// Create a new FecRateController with default feature toggles (all enabled).
    pub fn new(target_tail_loss: f64, max_overhead: f64, hint: ProtocolHint, backend: FecBackend, symbol_size: u16) -> Self {
        Self::new_with_toggles(target_tail_loss, max_overhead, hint, backend, true, symbol_size)
    }

    /// Create a new FecRateController with explicit feature toggles.
    ///
    /// `enable_pi_feedback` is accepted for API compatibility but ignored —
    /// BOCD posterior quantile replaces the PI controller entirely.
    ///
    /// The protocol hint adjusts `target_tail_loss` to control the
    /// FEC/NACK balance (latency vs bandwidth tradeoff):
    /// - Realtime: 100× tighter (more FEC, less NACK latency)
    /// - Bulk: 100× looser (less FEC, rely on NACK)
    /// - Auto: unchanged
    pub fn new_with_toggles(
        target_tail_loss: f64,
        max_overhead: f64,
        hint: ProtocolHint,
        backend: FecBackend,
        _enable_pi_feedback: bool,
        symbol_size: u16,
    ) -> Self {
        let codec_overhead = match backend {
            FecBackend::RaptorQ => 0.01,
            FecBackend::ReedSolomon => 0.0,
            FecBackend::Rlc => 0.004,
        };

        // Protocol hint maps to target_tail_loss, not an additive offset.
        // This is the only principled knob: tighter tail = more proactive FEC.
        let effective_tail_loss = target_tail_loss * hint.tail_loss_scale();
        let effective_tail_loss = effective_tail_loss.clamp(1e-9, 0.1);

        Self {
            target_tail_loss: effective_tail_loss,
            max_overhead,
            rq_overhead: codec_overhead,
            hint,
            symbol_size,
            saturation_cap_enabled: true,
            bulk_pure_arq: true,
            completion_exposure: 0.0,
            tail_provision: crate::config::env_flag("RWM_RSTAR_TAIL", true),
            inner_feedback: 0.0,
        }
    }

    /// Enable/disable the saturation cap (paper Section 14.21). Default: on.
    pub fn set_saturation_cap(&mut self, enabled: bool) {
        self.saturation_cap_enabled = enabled;
    }

    /// #46: enable/disable the window-mass burst-tail provisioning term
    /// (paper Section 8.4.1). Default follows RWM_RSTAR_TAIL (ON).
    /// Exposed for ablation.
    pub fn set_tail_provision(&mut self, enabled: bool) {
        self.tail_provision = enabled;
    }

    /// Enable/disable the Bulk pure-ARQ tail target (P4a, on by default).
    /// Exposed for ablation: with it off, Bulk falls back to the plain
    /// 100×-loosened `target_tail_loss`.
    pub fn set_bulk_pure_arq(&mut self, enabled: bool) {
        self.bulk_pure_arq = enabled;
    }

    /// P6: set the completion exposure χ ∈ [0, 1] (paper Section 14.26)
    /// for callers that know the remaining send time T_rem — compute it
    /// with `raptorpath_math::completion_exposure(t_rem, srtt, rttvar)`.
    /// The production tunnel never calls this (endless stream ⇒ χ = 0).
    pub fn set_completion_exposure(&mut self, chi: f64) {
        self.completion_exposure = chi.clamp(0.0, 1.0);
    }

    /// P10a: set the inner-feedback weight ∈ [0, 1] (paper Section 14.28).
    /// 1.0 = the payload's delivery latency feeds back into its own
    /// throughput (TCP-in-tunnel) — enables the mid-stream repair floor;
    /// 0.0 (default everywhere, including the production tunnel after the
    /// negative L1 C2/C3 ablation) = pure Bulk glide. Config knob:
    /// `inner_feedback_weight`.
    pub fn set_inner_feedback(&mut self, weight: f64) {
        self.inner_feedback = weight.clamp(0.0, 1.0);
    }

    /// Compute the number of repair symbols needed for `k` source symbols
    /// given the current loss estimate from `estimator`.
    ///
    /// `window_size`: current encoder window or block size (for codec overhead weighting).
    pub fn compute_repair_count(&self, k: u32, estimator: &LossEstimator, window_size: usize) -> u32 {
        let rate = self.compute_repair_rate(estimator, window_size);
        (k as f64 * rate).ceil() as u32
    }

    /// PI feedback update — no-op in new architecture.
    pub fn feedback_update(&mut self, _block_succeeded: bool) {}

    /// Window-mode PI feedback — no-op in new architecture.
    pub fn feedback_update_window(&mut self, _repairs_fed: u64, _repairs_useful: u64) {}

    /// Compute the repair rate for sliding-window mode: how many repair symbols
    /// to generate per source symbol. E.g., 0.1 = 1 repair per 10 source symbols.
    ///
    /// Uses the BOCD predictive quantile as the loss estimate and the paper's
    /// r* formula (Section 8.4) for the rate:
    ///
    ///   p    = BOCD posterior upper quantile at 95% (estimation margin)
    ///   z    = normal_quantile(1 - δ/p) — fluid: margin shrinks continuously
    ///          as the channel improves relative to the hint's tail target δ,
    ///          and the rate glides to 0 when pure ARQ meets it
    ///   rate = max( max(0, p/(1-p) + z·√(p·σ²_burst/(W·(1-p)))) + codec,
    ///               (B/T) × (1 - δ/p)⁺ )
    ///
    /// The two margins cover distinct variance sources: the BOCD quantile
    /// covers uncertainty about the TRUE loss rate (regime changes widen it
    /// automatically); z_δ covers window-tail loss variance GIVEN that rate.
    /// The protocol hint enters only through z_δ — feeding it into the
    /// estimation confidence as well would double-count the tail target.
    /// Codec overhead is weighted by P(decoder_invoked) for systematic codecs.
    ///
    /// When enabled (default), the result is capped at the p99 saturation
    /// point r_sat (paper Section 14.21): past it, extra repairs hurt the
    /// tail by displacing source symbols. See `set_saturation_cap`.
    ///
    /// `window_size`: current encoder window or block size.
    pub fn compute_repair_rate(&self, estimator: &LossEstimator, window_size: usize) -> f64 {
        // The formula itself lives in raptorpath-math::controller_rate — a
        // SINGLE shared implementation used by both this production
        // controller and the visualizer (raptorpath-wasm), so the two
        // cannot drift. This method only extracts the estimator state.
        let ge = estimator.ge_estimator();
        let (sigma2, mean_burst) = if ge.is_valid() {
            (
                raptorpath_math::burst_variance_factor(ge.p_gb(), ge.p_bg()),
                ge.mean_burst_length(),
            )
        } else {
            (1.0, 1.0)
        };
        let tput = estimator.throughput();
        let rtt_secs = estimator.rtt().as_secs_f64();
        // Burst B/T term requires valid GE data AND a throughput estimate.
        let t_symbols = if ge.is_valid() && tput > 0.0 {
            (rtt_secs * tput / self.symbol_size as f64).max(1.0)
        } else {
            0.0
        };
        // Saturation cap requires a throughput estimate for t_sym.
        let t_sym = if tput > 0.0 {
            self.symbol_size as f64 / tput
        } else {
            0.0
        };
        raptorpath_math::controller_rate(&raptorpath_math::RateInputs {
            p_upper: estimator.predictive_loss_upper(0.95),
            sigma2,
            mean_burst,
            // #46 (paper 8.4.1): the receiver's measured window loss-mass
            // tail; MassStats::default() until enough nonzero blocks are
            // observed, keeping cold start identical to pre-#46.
            mass: ge.mass_stats(),
            tail_provision: self.tail_provision,
            window: window_size as f64,
            t_symbols,
            srtt: rtt_secs,
            t_sym,
            codec_overhead: self.rq_overhead,
            tail_target: self.target_tail_loss,
            bulk_late_is_fine: self.hint == ProtocolHint::Bulk && self.bulk_pure_arq,
            // P6 (paper 14.26): 0.0 unless a T_rem-aware caller set it —
            // the production tunnel is an endless stream, so mid-stream
            // semantics (δ_eff = ε̂, r* = 0) apply permanently.
            completion_exposure: self.completion_exposure,
            // P10a (paper 14.28): mid-stream repair floor for payloads
            // whose latency feeds back (TCP-in-tunnel). 0.0 default.
            inner_feedback: self.inner_feedback,
            saturation_cap: self.saturation_cap_enabled,
            max_overhead: self.max_overhead,
        })
    }

    /// Derive the encoder window size W* from the current channel estimate
    /// (paper Section 8.8). Returns `None` when the estimator lacks the
    /// throughput/RTT sample the latency ceiling needs, so the caller can
    /// keep its default window. The formula lives in
    /// `raptorpath_math::derive_window` — the same code the visualizer reads.
    ///
    /// Balances overhead (larger W shrinks the r* margin as 1/sqrt(W)),
    /// recovery latency (W / send_rate must stay within ~1 RTT), and burst
    /// absorbency. The result is clamped to [16, 512] by the math layer; the
    /// window-mode sender additionally caps it at its own MAX_WINDOW_SIZE.
    pub fn derive_window(&self, estimator: &LossEstimator) -> Option<usize> {
        let tput = estimator.throughput();
        let rtt_secs = estimator.rtt().as_secs_f64();
        if !(tput > 0.0) || !(rtt_secs > 0.0) {
            return None; // no latency ceiling => keep the default window
        }
        let ge = estimator.ge_estimator();
        let sigma2 = if ge.is_valid() {
            raptorpath_math::burst_variance_factor(ge.p_gb(), ge.p_bg())
        } else {
            1.0
        };
        let eps = estimator.predictive_loss_upper(0.95).max(1e-6);
        let send_rate = tput / self.symbol_size as f64; // source symbols per second
        // latency_budget = 0 => the math layer aligns W to ~1 RTT (Section 14.5).
        let w = raptorpath_math::derive_window(
            self.target_tail_loss,
            eps,
            sigma2,
            rtt_secs,
            send_rate,
            0.0,
        );
        Some(w.round() as usize)
    }

    /// Compute repair rate with spare capacity constraint.
    ///
    /// This is the primary method for the "never hurts" guarantee:
    /// repair_rate ≤ spare_capacity, ensuring FEC never causes congestion.
    pub fn compute_repair_rate_capped(
        &self,
        estimator: &LossEstimator,
        spare_capacity: f64,
        window_size: usize,
    ) -> f64 {
        let rate = self.compute_repair_rate(estimator, window_size);
        rate.min(spare_capacity.max(0.0))
    }

    /// Update the codec overhead for a new backend (used during runtime switching).
    pub fn update_backend(&mut self, backend: FecBackend) {
        self.rq_overhead = match backend {
            FecBackend::RaptorQ => 0.01,
            FecBackend::ReedSolomon => 0.0,
            FecBackend::Rlc => 0.004,
        };
    }

    /// Get diagnostics for monitoring.
    pub fn diagnostics(&self) -> FecDiagnostics {
        FecDiagnostics {
            actual_failure_rate: 0.0,
            pi_correction: 0.0,
            integral_error: 0.0,
            target_tail_loss: self.target_tail_loss,
        }
    }

    /// Get the maximum overhead setting.
    pub fn max_overhead(&self) -> f64 {
        self.max_overhead
    }

    /// Get the raw codec overhead (before P(decoder_invoked) weighting).
    pub fn codec_overhead(&self) -> f64 {
        self.rq_overhead
    }

    /// Get the effective target tail loss probability (after hint adjustment).
    pub fn target_tail_loss(&self) -> f64 {
        self.target_tail_loss
    }
}

/// Taper function: time-decaying correction density τ(t) = A × (1-q)^t.
///
/// Matches the GE burst survival function — more corrections where loss is
/// likely (right after a burst), fewer as time passes. The amplitude A is
/// derived from the optimal correction rate r* and the GE parameter q.
///
/// See paper Section 4 (The Taper Function).
#[derive(Debug, Clone)]
pub struct TaperFunction {
    /// Taper amplitude: peak correction density at t=0.
    /// A = r* × q, where r* is the optimal correction rate.
    pub amplitude: f64,
    /// Decay base: (1-q) where q = P(Bad→Good) from GE model.
    /// Each time step, the density multiplies by this factor.
    pub decay: f64,
    /// Total correction rate: A / q = r*.
    pub total_rate: f64,
    /// The GE parameter q (for reference).
    pub q: f64,
}

impl TaperFunction {
    /// Create a taper function from the estimator and rate controller.
    ///
    /// Uses the GE model's q parameter for the decay shape and the
    /// optimal correction rate r* for the amplitude.
    pub fn from_estimator(estimator: &LossEstimator, rate: f64) -> Self {
        let ge = estimator.ge_estimator();
        let q = if ge.is_valid() {
            ge.p_bg().clamp(0.01, 1.0) // q = P(Bad→Good)
        } else {
            0.5 // Default: mean burst length = 2
        };

        let amplitude = rate * q;
        let decay = 1.0 - q;

        Self {
            amplitude,
            decay,
            total_rate: rate,
            q,
        }
    }

    /// Correction density at time offset t (in symbol intervals).
    ///
    /// Returns τ(t) = A × (1-q)^t — the number of correction symbols
    /// to generate per source symbol at offset t.
    pub fn density(&self, t: f64) -> f64 {
        self.amplitude * self.decay.powf(t)
    }

    /// Whether to generate a correction symbol at this offset,
    /// using the taper density as a probability.
    ///
    /// For densities > 1.0 (high loss), always generate.
    /// For densities < 1.0, probabilistic based on density.
    pub fn should_generate(&self, t: f64, rng_value: f64) -> bool {
        let d = self.density(t);
        if d >= 1.0 {
            true
        } else {
            rng_value < d
        }
    }
}

/// #85: budget-conserving taper accrual for the plain-mode proactive-repair
/// emission (env `RWM_TAPER_R`, default OFF).
///
/// MEASURED bug (goal-gate "r* Bursty-Loss Provisioning", L1 2026-07-13):
/// the legacy accrual feeds the emission debt with the raw taper density
/// τ(t) = r·q·(1−q)^t and t resets on every cumulative-ack advance, so the
/// emitted proactive repair sums to Σ_t τ(t) = r symbols PER ACK CYCLE —
/// nearly independent of r's magnitude (an ack cycle at BDP is hundreds of
/// symbols; measured cod/src ≈ 0.03–0.10 for BOTH r* = 0.206 and 0.255).
/// The whole r* control loop, including the §8.4.1 burst-tail correction,
/// was therefore INERT on the plain-mode wire.
///
/// The budget law: emitted repair must track r × (source symbols) — the
/// wire consumes r AS COMPUTED, per coding window. Per source symbol the
/// computed rate is banked into `owed` (Σ grants ≤ Σ rates, conserved up
/// to the expiry cap below) and the grant handed to the emission debt is
///
/// ```text
/// grant = min( owed, max(desire, rate), spare, 1.0 )
/// ```
///
/// where desire = rate · shape(t mod W) re-times the spend with the SAME
/// GE-survival taper shape, renormalized to mean weight 1 over a W-span:
/// shape(t) = W·q·(1−q)^t / (1−(1−q)^W). The taper's intent — repair
/// concentrated right after the frontier advances, where a covering repair
/// recovers a hole without a round-trip — is preserved as a RE-TIMING;
/// the TOTAL is governed by the budget, not by the ack cadence. No new
/// constants: the floor at `rate` guarantees the budget drains at least
/// uniformly (the desire tail cannot strand it), `spare` is the same
/// link-headroom anchor the legacy path capped with, and the 1.0 cap paces
/// backlog at ≤ 1 repair per source send (the source clock is the emission
/// clock — no bursts). `owed` is capped at one coding window's budget,
/// max(r·W, 1): repair budget for source older than a window has expired
/// (the window has slid), so spare-starved budget cannot accumulate
/// unboundedly.
#[derive(Debug, Default)]
pub struct TaperBudget {
    /// Banked, not-yet-granted repair budget (in symbols).
    owed: f64,
}

impl TaperBudget {
    pub fn new() -> Self {
        Self { owed: 0.0 }
    }

    /// Un-granted budget currently banked (diagnostics/tests).
    pub fn owed(&self) -> f64 {
        self.owed
    }

    /// Per SOURCE symbol: bank the computed rate and return the grant to
    /// add to the emission debt this symbol.
    ///
    /// * `rate`   — computed per-source repair rate r (already spare-capped
    ///              upstream by `compute_repair_rate_capped`)
    /// * `offset` — source symbols since the last cumulative-ack advance
    ///              (the taper phase; the caller keeps resetting it — under
    ///              the budget law the reset re-times, it no longer sizes)
    /// * `taper`  — the GE taper shape (q, decay) for this estimator state
    /// * `span`   — the coding window W (shape renormalization span)
    /// * `spare`  — link spare capacity (legacy cap anchor)
    pub fn accrue(
        &mut self,
        rate: f64,
        offset: u64,
        taper: &TaperFunction,
        span: usize,
        spare: f64,
    ) -> f64 {
        let rate = rate.max(0.0);
        let span_u = span.max(1) as u64;
        let span_f = span_u as f64;
        // Bank this symbol's budget; expire beyond one window's worth.
        self.owed = (self.owed + rate).min((rate * span_f).max(1.0));
        // Span-normalized taper shape at the current phase (mean 1 over W).
        let t = (offset % span_u) as f64;
        let norm = 1.0 - taper.decay.powf(span_f);
        let shape = if norm > 1e-12 && taper.q > 0.0 {
            span_f * taper.q * taper.decay.powf(t) / norm
        } else {
            1.0
        };
        let desire = rate * shape;
        let grant = self
            .owed
            .min(desire.max(rate))
            .min(spare.max(0.0))
            .min(1.0);
        self.owed -= grant;
        grant
    }
}

/// Compute P_lost(t): probability a symbol was lost given no ACK after time t.
///
/// Uses Bayes' theorem with the channel loss rate as prior:
///   P_lost(t) = ε / [ε + (1-ε) × P(RTT > t)]
///
/// where P(RTT > t) is the normal survival function.
///
/// Returns a value in [ε, 1.0] that smoothly transitions from "probably fine"
/// (t << SRTT) to "certainly lost" (t >> SRTT).
///
/// See paper Section 3.4 (Recovery Latency and the P_lost(t) Model).
pub fn p_lost(age_secs: f64, epsilon: f64, srtt_secs: f64, rttvar_secs: f64) -> f64 {
    if epsilon >= 1.0 {
        return 1.0;
    }
    if epsilon <= 0.0 {
        return 0.0;
    }

    // P(RTT > t) using normal survival function
    let rttvar = rttvar_secs.max(0.001); // avoid division by zero
    let z = (age_secs - srtt_secs) / rttvar;
    // Phi(-z) = P(RTT > t) for normal distribution
    let p_rtt_exceeds = normal_survival(z);

    // Bayes: P(lost | no ACK at t) = ε / [ε + (1-ε) × P(RTT > t)]
    let denom = epsilon + (1.0 - epsilon) * p_rtt_exceeds;
    if denom < 1e-300 {
        return 1.0;
    }
    (epsilon / denom).clamp(0.0, 1.0)
}

/// Standard normal survival function: P(Z > z) = 1 - Φ(z).
/// Uses the same rational approximation as normal_quantile.
fn normal_survival(z: f64) -> f64 {
    // For large positive z, survival is tiny
    if z > 8.0 {
        return 0.0;
    }
    if z < -8.0 {
        return 1.0;
    }

    // Use the error function approximation
    // Φ(z) ≈ 0.5 × (1 + erf(z/√2))
    // P(Z > z) = 1 - Φ(z) = 0.5 × erfc(z/√2)
    //
    // Abramowitz & Stegun approximation for erfc:
    let x = z / std::f64::consts::SQRT_2;
    let ax = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * ax);
    let poly = t * (0.254829592
        + t * (-0.284496736
            + t * (1.421413741
                + t * (-1.453152027
                    + t * 1.061405429))));
    let erfc_ax = poly * (-ax * ax).exp();

    if x >= 0.0 {
        0.5 * erfc_ax
    } else {
        1.0 - 0.5 * erfc_ax
    }
}

/// Burst variance inflation factor σ²_burst for the GE channel.
///
/// σ²_burst = 1 + 2(1-p-q)/(p+q)
///
/// where p = P(Good→Bad), q = P(Bad→Good) from the GE model.
/// This inflates the margin term in the r* formula to account for
/// correlated (bursty) losses. See paper Section 8.3.
///
/// Returns 1.0 (iid) when GE parameters are unavailable or degenerate.
pub fn burst_variance_factor(estimator: &LossEstimator) -> f64 {
    let ge = estimator.ge_estimator();
    if !ge.is_valid() {
        return 1.0;
    }
    let p = ge.p_gb(); // P(Good→Bad)
    let q = ge.p_bg(); // P(Bad→Good)
    // q = 0 / p = 0 are NO-DATA sentinels (decayed counters below 1 on very
    // clean channels), not measurements — no data means iid (σ² = 1).
    // Otherwise σ² ≈ 2/p̂ explodes and over-provisions the cleanest links.
    if p <= 0.0 || q <= 0.0 {
        return 1.0;
    }
    let sum = p + q;
    if sum < 1e-10 {
        return 1.0;
    }
    let factor = 1.0 + 2.0 * (1.0 - p - q) / sum;
    factor.max(1.0) // σ²_burst ≥ 1 (iid is the minimum)
}

/// P_fec using normal approximation (paper Section 8.1).
///
/// P_fec = Φ(√W × (r(1-ε)-ε) / √(ε(1-ε)(r+σ²_burst)))
fn p_fec_normal(r: f64, epsilon: f64, window_size: f64, sigma2_burst: f64) -> f64 {
    if window_size <= 0.0 || epsilon <= 0.0 || epsilon >= 1.0 || r <= 0.0 {
        return 0.0;
    }
    let numerator = r * (1.0 - epsilon) - epsilon;
    if numerator <= 0.0 {
        return 0.0; // r too low to overcome loss
    }
    let denominator = (epsilon * (1.0 - epsilon) * (r + sigma2_burst)).sqrt();
    if denominator < 1e-300 {
        return 1.0;
    }
    let z = window_size.sqrt() * numerator / denominator;
    1.0 - normal_survival(z)
}

/// The (δ, ρ, r) residual-loss allowance 1−ρ at the operating point (the
/// δ-honest overload-shedding budget, goal-gate "Unified Shedding"):
///
///   1−ρ = ε · (1 − P_fec(r, ε, W, σ²_burst))
///
/// — the loss fraction the design already concedes past in-window FEC at
/// the deadline (§6.3: P(lost) = ε·(1−P_fec)·(1−P_arq); at small δ,
/// recovery past D(δ) belongs to no one, so P_arq's contribution is priced
/// out and the residual IS the shed allowance). Every input is a measured
/// anchor or an already-derived parameter: ε̂ from the loss estimator,
/// r = the live consumed taper rate, W = the live solvable-span width A*,
/// σ²_burst from the GE estimator. No new constants. Returns a fraction in
/// [0, 1]; 0 when ε or r has no sample yet (cold start sheds nothing —
/// the conservative side of the ρ contract).
pub fn residual_loss_after_fec(epsilon: f64, r: f64, window_size: f64, sigma2_burst: f64) -> f64 {
    if !(epsilon > 0.0) || epsilon >= 1.0 {
        return 0.0;
    }
    let p_fec = p_fec_normal(r, epsilon, window_size, sigma2_burst);
    (epsilon * (1.0 - p_fec)).clamp(0.0, 1.0)
}

/// Joint FEC/NACK budget allocator.
///
/// Splits the total repair budget between proactive FEC and reactive NACK repair,
/// ensuring they don't compete for the same bandwidth.
///
/// Budget conservation: proactive + nack ≤ total_budget ≤ spare_capacity
pub struct BudgetAllocator {
    total_budget: f64,
    nack_expected: f64,
    proactive_budget: f64,
    nack_cap: f64,
}

impl BudgetAllocator {
    /// Compute budget allocation from current estimates.
    pub fn compute(
        p_upper: f64,
        codec_overhead: f64,
        nack_rate: f64,
        nack_effectiveness: f64,
    ) -> Self {
        let total_budget = if p_upper < 1e-10 {
            0.0
        } else {
            p_upper / (1.0 - p_upper) + codec_overhead
        };

        let nack_expected = (nack_rate * nack_effectiveness).min(total_budget);
        let proactive_budget = (total_budget - nack_expected).max(0.0);
        let nack_cap = (total_budget - proactive_budget).max(0.0);

        Self {
            total_budget,
            nack_expected,
            proactive_budget,
            nack_cap,
        }
    }

    pub fn proactive_rate(&self) -> f64 {
        self.proactive_budget
    }

    pub fn nack_cap(&self) -> f64 {
        self.nack_cap
    }

    pub fn total_budget(&self) -> f64 {
        self.total_budget
    }
}

#[derive(Debug, Clone)]
pub struct FecDiagnostics {
    pub actual_failure_rate: f64,
    pub pi_correction: f64,
    pub integral_error: f64,
    pub target_tail_loss: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::estimator::LossEstimator;

    const W: usize = 50; // typical window size for tests

    /// δ-honest shed budget (goal-gate "Unified Shedding"): the 1−ρ
    /// allowance is the DESIGN residual ε·(1−P_fec) — 0 with no loss or no
    /// FEC sample (cold start sheds nothing), ε itself when r cannot
    /// overcome the loss (P_fec = 0), monotone non-increasing in r, and in
    /// the streaming-machine ~1% class at the measured c3 operating point.
    #[test]
    fn residual_loss_after_fec_is_the_design_residual() {
        // Cold / degenerate inputs: budget 0.
        assert_eq!(residual_loss_after_fec(0.0, 0.3, 5.0, 3.0), 0.0);
        assert_eq!(residual_loss_after_fec(-1.0, 0.3, 5.0, 3.0), 0.0);
        // r too low to overcome loss: residual = ε (pure-loss allowance).
        let eps = 0.048;
        let all = residual_loss_after_fec(eps, 0.0, 5.0, 3.0);
        assert!((all - eps).abs() < 1e-12, "P_fec=0 ⇒ residual=ε, got {all}");
        // Monotone non-increasing in r.
        let mut prev = all;
        for r in [0.05, 0.1, 0.2, 0.34, 0.5, 1.0] {
            let v = residual_loss_after_fec(eps, r, 5.0, 3.76);
            assert!(v <= prev + 1e-12, "residual must fall as r rises");
            prev = v;
        }
        // The measured c3 operating point (ε≈4.8%, consumed r≈0.34, A*≈3–5,
        // GE σ²≈3.76): the residual sits in the ~1% class the streaming
        // machine sheds — well below ε, well above zero.
        let c3 = residual_loss_after_fec(eps, 0.34, 4.0, 3.76);
        assert!(c3 > 0.001 && c3 < eps, "c3-class residual out of class: {c3}");
    }

    #[test]
    fn test_zero_loss_no_repair() {
        let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto, FecBackend::RaptorQ, 1200);
        let mut est = LossEstimator::new();
        for _ in 0..50 {
            est.record_batch(100, 100);
        }
        let r = ctrl.compute_repair_count(100, &est, W);
        assert!(r < 5, "Expected minimal repair for zero loss, got {r}");
    }

    #[test]
    fn test_high_loss_more_repair() {
        let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto, FecBackend::RaptorQ, 1200);
        let mut est = LossEstimator::new();
        for _ in 0..100 {
            est.record_batch(100, 80); // 20% loss
        }
        let r = ctrl.compute_repair_count(100, &est, W);
        assert!(r >= 20, "Expected significant repair for 20% loss, got {r}");
        assert!(r <= 50, "Repair should be capped at max overhead, got {r}");
    }

    #[test]
    fn test_protocol_hint_realtime_more_aggressive() {
        let ctrl_rt = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::RaptorQ, 1200);
        let ctrl_bulk = FecRateController::new(1e-5, 0.5, ProtocolHint::Bulk, FecBackend::RaptorQ, 1200);

        let mut est = LossEstimator::new();
        for _ in 0..100 {
            est.record_batch(100, 90);
        }

        let r_rt = ctrl_rt.compute_repair_count(100, &est, W);
        let r_bulk = ctrl_bulk.compute_repair_count(100, &est, W);
        assert!(
            r_rt >= r_bulk,
            "Realtime ({r_rt}) should use >= repair than bulk ({r_bulk})"
        );
    }

    #[test]
    fn test_hint_controls_tail_loss_not_offset() {
        // Realtime with target_tail_loss=1e-5 should behave like Auto with 1e-7
        // (because Realtime applies 100× tighter = 1e-5 * 0.01 = 1e-7)
        let ctrl_rt = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::RaptorQ, 1200);
        let ctrl_auto_tight = FecRateController::new(1e-7, 0.5, ProtocolHint::Auto, FecBackend::RaptorQ, 1200);

        let mut est = LossEstimator::new();
        for _ in 0..100 {
            est.record_batch(100, 90);
        }

        let r_rt = ctrl_rt.compute_repair_rate(&est, W);
        let r_auto = ctrl_auto_tight.compute_repair_rate(&est, W);
        assert!(
            (r_rt - r_auto).abs() < 0.001,
            "Realtime(1e-5) should equal Auto(1e-7): rt={r_rt}, auto={r_auto}"
        );
    }

    #[test]
    fn test_continuous_rate_no_fec_when_target_met() {
        // Paper Section 8.4 continuity: the z_{δ/ε} margin lets the rate
        // decrease to 0 when pure ARQ meets the tail target — no cutoff
        // branch. Clean link (0.1% loss) under Bulk (δ = 1e-5 × 100 = 1e-3).
        let ctrl_bulk = FecRateController::new(1e-5, 0.5, ProtocolHint::Bulk, FecBackend::Rlc, 1200);
        let ctrl_auto = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto, FecBackend::Rlc, 1200);
        let ctrl_rt = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::Rlc, 1200);
        let mut est = LossEstimator::new();
        for _ in 0..100 {
            est.record_batch(1000, 999); // 0.1% loss
        }
        let r_bulk = ctrl_bulk.compute_repair_rate(&est, 50);
        let r_auto = ctrl_auto.compute_repair_rate(&est, 50);
        let r_rt = ctrl_rt.compute_repair_rate(&est, 50);
        // Bulk target (1e-3) ≈ channel loss → essentially no FEC
        assert!(r_bulk < 0.01, "Bulk at 0.1% loss should carry ~no FEC: {r_bulk}");
        // Tighter hints → continuously more FEC
        assert!(r_bulk <= r_auto && r_auto <= r_rt,
            "rate must be monotone in tail tightness: bulk={r_bulk}, auto={r_auto}, rt={r_rt}");
        assert!(r_rt > 0.0, "Realtime at 0.1% loss should still use FEC: {r_rt}");
    }

    #[test]
    fn test_saturation_cap_binds_with_throughput() {
        // C4-like estimator state: 5% loss, long RTT, known throughput.
        // Realtime's aggressive request must be capped at r_sat; without
        // the flag (or without a throughput estimate) it must not be.
        let mut est = LossEstimator::new();
        for _ in 0..100 {
            est.record_batch(100, 95); // 5% loss
            est.record_rtt(std::time::Duration::from_millis(210));
        }

        // No throughput estimate -> cap skipped even when enabled.
        let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::Rlc, 1200);
        let uncapped_no_tput = ctrl.compute_repair_rate(&est, 64);

        for _ in 0..100 {
            est.record_throughput(2_500_000.0);
        }
        let capped = ctrl.compute_repair_rate(&est, 64);

        let mut ctrl_off = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::Rlc, 1200);
        ctrl_off.set_saturation_cap(false);
        let uncapped = ctrl_off.compute_repair_rate(&est, 64);

        assert!(
            (uncapped - uncapped_no_tput).abs() < 1e-9,
            "cap must be inert without a throughput estimate: {uncapped_no_tput} vs {uncapped}"
        );
        assert!(
            capped < uncapped,
            "saturation cap must bind for an aggressive request: capped={capped}, uncapped={uncapped}"
        );
        // The capped rate must be the SOFT saturation of the uncapped request
        // (paper 14.21.1): it sits just below r_sat, approaching it
        // asymptotically rather than pinning to it exactly.
        let p = est.predictive_loss_upper(0.95);
        let ge = est.ge_estimator();
        let sigma2 = if ge.is_valid() {
            raptorpath_math::burst_variance_factor(ge.p_gb(), ge.p_bg())
        } else {
            1.0
        };
        let r_sat = raptorpath_math::r_saturation(
            p, sigma2, 64.0, est.rtt().as_secs_f64(), 1200.0 / est.throughput(),
        );
        let expected = raptorpath_math::soft_saturate(uncapped, r_sat);
        assert!(
            (capped - expected).abs() < 1e-9,
            "capped rate must equal soft_saturate(uncapped, r_sat): capped={capped}, expected={expected}"
        );
        assert!(
            capped < r_sat && capped > r_sat * (1.0 - raptorpath_math::SAT_SOFTNESS),
            "soft cap sits just below r_sat: capped={capped}, r_sat={r_sat}"
        );
    }

    #[test]
    fn test_bulk_pure_arq_zero_steady_state_rate() {
        // P4a/P6 (paper 14.26): Bulk's effective tail target is the
        // completion-exposure glide δ_eff = ε̂ + (0.05 − ε̂)·χ; the tunnel
        // never sets χ, so δ_eff = ε̂ ("late is fine") and even at 5% loss
        // the steady-state rate is 0 identically (pure ARQ, volume parity
        // with retransmission transports).
        let ctrl_bulk = FecRateController::new(1e-5, 0.5, ProtocolHint::Bulk, FecBackend::Rlc, 1200);
        let mut est = LossEstimator::new();
        for _ in 0..100 {
            est.record_batch(100, 95); // 5% loss
        }
        let r_bulk = ctrl_bulk.compute_repair_rate(&est, W);
        assert!(r_bulk < 0.01, "Bulk at 5% loss must be ~pure ARQ: {r_bulk}");

        // Ablation arm: with the flag off, Bulk falls back to the plain
        // 100×-loosened target and pays steady-state FEC again.
        let mut ctrl_off = FecRateController::new(1e-5, 0.5, ProtocolHint::Bulk, FecBackend::Rlc, 1200);
        ctrl_off.set_bulk_pure_arq(false);
        let r_off = ctrl_off.compute_repair_rate(&est, W);
        assert!(r_off > r_bulk, "flag off must restore steady-state FEC: on={r_bulk}, off={r_off}");

        // Realtime is untouched by the flag (Bulk-only mapping).
        let ctrl_rt_on = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::Rlc, 1200);
        let mut ctrl_rt_off = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::Rlc, 1200);
        ctrl_rt_off.set_bulk_pure_arq(false);
        let rt_on = ctrl_rt_on.compute_repair_rate(&est, W);
        let rt_off = ctrl_rt_off.compute_repair_rate(&est, W);
        assert_eq!(rt_on, rt_off, "Realtime must be unaffected: on={rt_on}, off={rt_off}");
        assert!(rt_on > 0.05, "Realtime at 5% loss still carries FEC: {rt_on}");
    }

    #[test]
    fn test_inner_feedback_floor_tunnel_bulk() {
        // P10a (paper 14.28): at the L1 C2 operating point (ε ≈ 2.6%,
        // SRTT ≈ 13 ms, 100 Mbit) the Bulk glide alone is pure ARQ
        // mid-stream, but with the inner-feedback weight set (TCP-in-tunnel
        // payload) the repair floor keeps a small proactive rate that
        // covers loss events within the inner flow's stall horizon.
        let mut est = LossEstimator::new();
        for _ in 0..200 {
            est.record_batch(1000, 974); // 2.6% loss (C2 GE average)
            est.record_rtt(std::time::Duration::from_millis(13));
            est.record_throughput(12_500_000.0); // 100 Mbit/s
        }

        let mut ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Bulk, FecBackend::Rlc, 1200);
        let base = ctrl.compute_repair_rate(&est, 56);
        assert!(base < 0.005, "weight 0 keeps the pure Bulk glide: {base}");

        ctrl.set_inner_feedback(1.0);
        let floored = ctrl.compute_repair_rate(&est, 56);
        assert!(
            (0.01..=0.06).contains(&floored),
            "C2 tunnel floor must sit in the sane band: {floored}"
        );

        // Continuous in the weight: half weight lands strictly between.
        ctrl.set_inner_feedback(0.5);
        let half = ctrl.compute_repair_rate(&est, 56);
        assert!(
            base < half && half < floored,
            "floor must scale continuously with the weight: {base} < {half} < {floored}"
        );

        // No throughput estimate -> floor inert (same sentinel convention
        // as the burst term): a fresh estimator has no t_sym.
        let mut est_no_tput = LossEstimator::new();
        for _ in 0..200 {
            est_no_tput.record_batch(1000, 974);
            est_no_tput.record_rtt(std::time::Duration::from_millis(13));
        }
        ctrl.set_inner_feedback(1.0);
        let no_tput = ctrl.compute_repair_rate(&est_no_tput, 56);
        assert!(no_tput < 0.005, "floor needs a throughput estimate: {no_tput}");
    }

    #[test]
    fn test_tail_provision_bursty_channel_raises_rate() {
        // #46 (paper 8.4.1): feed a HEAVY-CLUSTERED per-symbol loss
        // pattern (fade episodes of ~48 lost symbols every ~1500) so the
        // measured window-mass tail is far beyond what the GE margin
        // models. With the tail term ON (shipped default) the rate must
        // rise materially above the legacy GE-only rate; with it OFF
        // (RWM_RSTAR_TAIL=0 arm, via the setter) the legacy rate returns.
        let mut est = LossEstimator::new();
        for _ in 0..60 {
            // one fade episode + clean stretch, fed with true interleaving
            est.record_counts(1500, 1452);
            for _ in 0..48 {
                est.record_symbol(false);
            }
            for _ in 0..1452 {
                est.record_symbol(true);
            }
        }
        let ge = est.ge_estimator();
        assert!(
            ge.mass_stats().is_valid(),
            "mass statistics must be live after 60 fade episodes"
        );

        let mut ctrl = FecRateController::new(1e-4, 1.0, ProtocolHint::Auto, FecBackend::Rlc, 1200);
        ctrl.set_tail_provision(false);
        let legacy = ctrl.compute_repair_rate(&est, 64);
        ctrl.set_tail_provision(true);
        let corrected = ctrl.compute_repair_rate(&est, 64);
        println!("legacy={legacy:.3} corrected={corrected:.3}");
        assert!(
            corrected > 1.2 * legacy,
            "clustered fades must raise the corrected rate materially: {corrected} vs {legacy}"
        );

        // On a NON-bursty channel of the same average loss the two arms
        // stay close (no over-provisioning where GE is adequate): iid-fed
        // pattern (isolated losses).
        let mut est_iid = LossEstimator::new();
        for _ in 0..2000 {
            est_iid.record_counts(31, 30);
            for _ in 0..30 {
                est_iid.record_symbol(true);
            }
            est_iid.record_symbol(false);
        }
        let mut ctrl2 = FecRateController::new(1e-4, 1.0, ProtocolHint::Auto, FecBackend::Rlc, 1200);
        ctrl2.set_tail_provision(false);
        let legacy_iid = ctrl2.compute_repair_rate(&est_iid, 64);
        ctrl2.set_tail_provision(true);
        let corrected_iid = ctrl2.compute_repair_rate(&est_iid, 64);
        println!("iid: legacy={legacy_iid:.3} corrected={corrected_iid:.3}");
        assert!(
            corrected_iid <= 1.35 * legacy_iid,
            "near-iid channel must not be materially over-provisioned: {corrected_iid} vs {legacy_iid}"
        );
    }

    #[test]
    fn test_spare_capacity_capping() {
        let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto, FecBackend::RaptorQ, 1200);
        let mut est = LossEstimator::new();
        for _ in 0..100 {
            est.record_batch(100, 80);
        }

        let uncapped = ctrl.compute_repair_rate(&est, W);
        let capped = ctrl.compute_repair_rate_capped(&est, 0.05, W);
        assert!(uncapped > 0.05, "Uncapped rate should be > 5%: {uncapped}");
        assert!(capped <= 0.05, "Capped rate should be ≤ 5%: {capped}");
    }

    #[test]
    fn test_codec_overhead_weighted_by_decoder_invocation() {
        // Compare RaptorQ (1% codec overhead) vs ReedSolomon (0% overhead) at same window.
        // The difference isolates the codec overhead contribution.
        let ctrl_rq = FecRateController::new(1e-5, 1.0, ProtocolHint::Auto, FecBackend::RaptorQ, 1200);
        let ctrl_rs = FecRateController::new(1e-5, 1.0, ProtocolHint::Auto, FecBackend::ReedSolomon, 1200);

        let mut est = LossEstimator::new();
        // Low-moderate loss so rate doesn't hit max_overhead cap
        for _ in 0..100 {
            est.record_batch(100, 95); // 5% loss
        }

        // At same window, RaptorQ should have higher rate than RS due to codec overhead
        let rate_rq = ctrl_rq.compute_repair_rate(&est, 50);
        let rate_rs = ctrl_rs.compute_repair_rate(&est, 50);
        assert!(
            rate_rq > rate_rs,
            "RaptorQ should have higher rate than RS due to codec overhead: rq={rate_rq}, rs={rate_rs}"
        );

        // With zero window size, no codec overhead → RaptorQ ≈ RS
        let rate_rq_zero = ctrl_rq.compute_repair_rate(&est, 0);
        let rate_rs_zero = ctrl_rs.compute_repair_rate(&est, 0);
        assert!(
            (rate_rq_zero - rate_rs_zero).abs() < 0.01,
            "Zero window should have no codec overhead: rq={rate_rq_zero}, rs={rate_rs_zero}"
        );
    }

    #[test]
    fn test_budget_allocator_basic() {
        let budget = BudgetAllocator::compute(0.1, 0.01, 0.05, 0.8);
        assert!(budget.total_budget() > 0.1);
        assert!(budget.proactive_rate() > 0.0);
        assert!(budget.nack_cap() > 0.0);
        assert!(
            (budget.proactive_rate() + budget.nack_cap() - budget.total_budget()).abs() < 1e-10,
            "Budget should be conserved"
        );
    }

    #[test]
    fn test_budget_allocator_no_nack() {
        let budget = BudgetAllocator::compute(0.1, 0.01, 0.0, 1.0);
        assert!(
            (budget.proactive_rate() - budget.total_budget()).abs() < 1e-10,
            "Without NACK, all budget goes to proactive"
        );
    }

    #[test]
    fn test_budget_allocator_zero_loss() {
        let budget = BudgetAllocator::compute(0.0, 0.01, 0.05, 0.8);
        assert!(budget.total_budget() < 1e-10);
        assert!(budget.proactive_rate() < 1e-10);
    }

    // --- Taper function tests ---

    #[test]
    fn test_taper_density_decays() {
        let taper = TaperFunction {
            amplitude: 0.04,
            decay: 0.5, // q=0.5, mean burst = 2
            total_rate: 0.08,
            q: 0.5,
        };

        let d0 = taper.density(0.0);
        let d1 = taper.density(1.0);
        let d2 = taper.density(2.0);

        assert!((d0 - 0.04).abs() < 1e-10, "density(0) = amplitude");
        assert!((d1 - 0.02).abs() < 1e-10, "density(1) = A * 0.5");
        assert!((d2 - 0.01).abs() < 1e-10, "density(2) = A * 0.25");
        assert!(d0 > d1, "density decays");
        assert!(d1 > d2, "density decays monotonically");
    }

    #[test]
    fn test_taper_from_estimator() {
        let mut est = LossEstimator::new();
        for _ in 0..100 {
            est.record_batch(100, 90); // 10% loss
        }

        let rate = 0.12; // 12% correction rate
        let taper = TaperFunction::from_estimator(&est, rate);

        assert!(taper.amplitude > 0.0, "amplitude should be positive");
        assert!(taper.decay > 0.0 && taper.decay < 1.0, "decay in (0,1)");
        assert!((taper.total_rate - rate).abs() < 1e-10, "total rate preserved");
        assert!((taper.amplitude - rate * taper.q).abs() < 1e-10, "A = r * q");
    }

    #[test]
    fn test_taper_total_rate_geometric_sum() {
        // The geometric series sum of τ(t) for t=0..∞ should equal total_rate
        let taper = TaperFunction {
            amplitude: 0.06,
            decay: 0.7, // q=0.3
            total_rate: 0.06 / 0.3,
            q: 0.3,
        };

        // Sum first 1000 terms (approximates infinite sum)
        let sum: f64 = (0..1000).map(|t| taper.density(t as f64)).sum();
        assert!(
            (sum - taper.total_rate).abs() < 0.001,
            "geometric sum ≈ A/q = total_rate: sum={sum}, expected={}",
            taper.total_rate
        );
    }

    // --- #85 TaperBudget tests (RWM_TAPER_R budget law) ---

    /// #85 attribution probe (not a gate; `--ignored`): the controller-level
    /// r for the two RWM_RSTAR_TAIL arms on the L0 2x2 battery cell
    /// (heavy:20;20;5;0.6;0.55;0.5 — semi-Markov, Weibull k=0.5 theta=0.55
    /// bursts, onset 0.6% => eps ~3.6%), realtime hint, W=64, with the c3
    /// rate/RTT anchors so the saturation cap is live. Prints legacy vs
    /// corrected r — the number the emission path consumes per arm.
    #[test]
    #[ignore = "measurement probe for the #85 L0 cell, not a CI gate"]
    fn probe_rstar_arms_c3heavy() {
        let mut est = LossEstimator::new();
        // Deterministic semi-Markov replay of the c3heavy law (splitmix-ish
        // LCG for portability; the exact stream is irrelevant — the SHAPE
        // is the cell's).
        let mut state = 42u64;
        let mut rand = move || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((state >> 11) as f64) / ((1u64 << 53) as f64)
        };
        let (onset, theta, k) = (0.006f64, 0.55f64, 0.5f64);
        let mut sent = 0u64;
        let mut got = 0u64;
        let mut batch = Vec::with_capacity(64);
        let mut n = 0usize;
        while n < 200_000 {
            // Good sojourn
            let g = ((rand().max(1e-12).ln() / (1.0 - onset).ln()).ceil()).max(1.0) as usize;
            for _ in 0..g {
                batch.push(true);
                n += 1;
            }
            // Weibull bad sojourn
            let b = ((rand().max(1e-300).ln() / theta.ln()).powf(1.0 / k).ceil()).max(1.0)
                .min(10_000.0) as usize;
            for _ in 0..b {
                batch.push(false);
                n += 1;
            }
            // Feed in 64-symbol batches like the receiver's block cadence.
            while batch.len() >= 64 {
                let chunk: Vec<bool> = batch.drain(..64).collect();
                let ok = chunk.iter().filter(|&&x| x).count() as u32;
                sent += 64;
                got += ok as u64;
                est.record_counts(64, ok);
                for &x in &chunk {
                    est.record_symbol(x);
                }
            }
        }
        // c3 anchors: 20 mbit, 40 ms RTT, realtime symbol size 512.
        for _ in 0..100 {
            est.record_rtt(std::time::Duration::from_millis(40));
            est.record_throughput(2_500_000.0);
        }
        let eps = 1.0 - got as f64 / sent as f64;
        let mut ctrl =
            FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::Rlc, 512);
        ctrl.set_tail_provision(false);
        let r_legacy = ctrl.compute_repair_rate(&est, 64);
        ctrl.set_tail_provision(true);
        let r_corrected = ctrl.compute_repair_rate(&est, 64);
        println!(
            "c3heavy probe: eps={:.3} mass_valid={} r_legacy={r_legacy:.3} r_corrected={r_corrected:.3}",
            eps,
            est.ge_estimator().mass_stats().is_valid()
        );
    }

    /// Replicates the plain-mode emission loop's accounting: per source
    /// symbol accrue into the fractional debt, emit whole symbols, reset
    /// the taper offset at each ack (per `ack_every`; 0 = never = one
    /// endless cycle). Returns emitted repair symbols.
    ///
    /// `budget = true` runs the #85 TaperBudget law; `false` runs the
    /// legacy density accrual (τ at the offset, spare-capped) — the
    /// measured-inert arm, kept here as the executable statement of the
    /// bug this law fixes.
    fn simulate_emission(
        rate: f64,
        q: f64,
        span: usize,
        n_sources: u64,
        ack_every: u64,
        spare: f64,
        budget: bool,
    ) -> u64 {
        let taper = TaperFunction {
            amplitude: rate * q,
            decay: 1.0 - q,
            total_rate: rate,
            q,
        };
        let mut tb = TaperBudget::new();
        let mut debt = 0.0f64;
        let mut emitted = 0u64;
        let mut offset = 0u64;
        for i in 0..n_sources {
            let add = if budget {
                tb.accrue(rate, offset, &taper, span, spare)
            } else {
                taper.density(offset as f64).min(spare.max(0.0))
            };
            debt += add;
            offset += 1;
            while debt >= 1.0 {
                debt -= 1.0;
                emitted += 1;
            }
            // Cumulative-ack advancement resets the taper phase (the
            // net/mod.rs `taper_offset = 0` on window advancement).
            if ack_every > 0 && (i + 1) % ack_every == 0 {
                offset = 0;
            }
        }
        emitted
    }

    #[test]
    fn test_taper_budget_tracks_r_magnitude() {
        // The bug (#46 L1): with the legacy law, r = 0.05 and r = 0.25
        // emit the SAME repair (≈ r per ack cycle → cycle-count-sized, not
        // r-sized). The budget law must emit ~5x apart and ≈ r × source.
        let (q, span, n, ack_every) = (0.4, 64, 20_000u64, 200u64);
        let lo = simulate_emission(0.05, q, span, n, ack_every, f64::INFINITY, true);
        let hi = simulate_emission(0.25, q, span, n, ack_every, f64::INFINITY, true);
        // Budget law: emitted ≈ r × n within 15%.
        let (exp_lo, exp_hi) = (0.05 * n as f64, 0.25 * n as f64);
        assert!(
            (lo as f64) > 0.85 * exp_lo && (lo as f64) < 1.15 * exp_lo,
            "budget law must emit ~r x source at r=0.05: {lo} vs {exp_lo}"
        );
        assert!(
            (hi as f64) > 0.85 * exp_hi && (hi as f64) < 1.15 * exp_hi,
            "budget law must emit ~r x source at r=0.25: {hi} vs {exp_hi}"
        );
        let ratio = hi as f64 / lo.max(1) as f64;
        assert!(
            (4.0..=6.0).contains(&ratio),
            "5x the rate must emit ~5x the repair: {ratio:.2}x ({lo} vs {hi})"
        );

        // The legacy arm documents the pathology: BOTH rates emit ≈ r per
        // ack cycle (n/ack_every cycles), an order below the budget and
        // nearly invariant in r.
        let lo_legacy = simulate_emission(0.05, q, span, n, ack_every, f64::INFINITY, false);
        let hi_legacy = simulate_emission(0.25, q, span, n, ack_every, f64::INFINITY, false);
        let cycles = (n / ack_every) as f64;
        assert!(
            (hi_legacy as f64) < 1.5 * 0.25 * cycles + 2.0,
            "legacy emits ~r per ack cycle, not r per source: {hi_legacy} vs {} cycles",
            cycles
        );
        assert!(
            (hi as f64) > 10.0 * (hi_legacy.max(1) as f64),
            "the budget law must break the per-ack-cycle ceiling: budget={hi} legacy={hi_legacy}"
        );
    }

    #[test]
    fn test_taper_budget_ack_cadence_invariance() {
        // The budget must be governed by SOURCE COUNT, not ack cadence:
        // burst acks (reset every symbol — the old reset pathology's fast
        // edge), a c3-like cycle (hundreds of symbols), and sparse acks
        // (one endless cycle) must all emit ≈ r × source.
        let (r, q, span, n) = (0.23, 0.4, 64, 20_000u64);
        let expect = r * n as f64;
        for (name, ack_every) in [("burst(1)", 1u64), ("cycle(300)", 300), ("sparse(0)", 0)] {
            let e = simulate_emission(r, q, span, n, ack_every, f64::INFINITY, true);
            assert!(
                (e as f64) > 0.8 * expect && (e as f64) < 1.2 * expect,
                "budget law must emit ~r x source under {name} acks: {e} vs {expect:.0}"
            );
        }
        // Contrast: legacy under burst acks pins the phase at 0 → emits
        // A = r·q per symbol (under), and under sparse acks emits ~r TOTAL.
        let sparse_legacy = simulate_emission(r, q, span, n, 0, f64::INFINITY, false);
        assert!(
            sparse_legacy <= 1,
            "legacy sparse-ack cycle emits ~r total (the pathology): {sparse_legacy}"
        );
    }

    #[test]
    fn test_taper_budget_spare_cap_and_expiry() {
        // Zero spare ⇒ zero grants (the never-hurts anchor is respected)
        // and the banked budget must EXPIRE at one window's worth
        // (max(r·W, 1)) instead of accumulating unboundedly.
        let (r, q, span) = (0.25, 0.4, 64usize);
        let taper = TaperFunction {
            amplitude: r * q,
            decay: 1.0 - q,
            total_rate: r,
            q,
        };
        let mut tb = TaperBudget::new();
        for t in 0..10_000u64 {
            let g = tb.accrue(r, t, &taper, span, 0.0);
            assert_eq!(g, 0.0, "no spare ⇒ no grant");
        }
        let cap = (r * span as f64).max(1.0);
        assert!(
            tb.owed() <= cap + 1e-9,
            "starved budget must expire at one window's budget: owed={} cap={cap}",
            tb.owed()
        );

        // When spare returns, the backlog drains paced at <= 1 repair per
        // source send (the source clock), never a burst.
        let mut max_grant = 0.0f64;
        let mut drained = 0.0;
        for t in 0..64u64 {
            let g = tb.accrue(r, t, &taper, span, f64::INFINITY);
            assert!(g <= 1.0 + 1e-9, "grant must never exceed 1 per source send");
            max_grant = max_grant.max(g);
            drained += g;
        }
        assert!(
            drained > cap * 0.9,
            "backlog must drain once spare returns: drained={drained:.2} of cap={cap}"
        );
        assert!(max_grant > r, "frontier drain must front-load above the flat rate");
    }

    #[test]
    fn test_taper_budget_front_loads_at_frontier() {
        // The taper's INTENT survives: with banked budget, the grant right
        // after a frontier advance (offset 0) exceeds the mid-span grant —
        // repair is still concentrated where it recovers a hole without a
        // round-trip. (Total is budget-governed; only the timing is shaped.)
        let (r, q, span) = (0.10, 0.4, 64usize);
        let taper = TaperFunction {
            amplitude: r * q,
            decay: 1.0 - q,
            total_rate: r,
            q,
        };
        let mut tb = TaperBudget::new();
        // Bank some budget under zero spare.
        for t in 0..40u64 {
            tb.accrue(r, t, &taper, span, 0.0);
        }
        let g_frontier = tb.accrue(r, 0, &taper, span, f64::INFINITY);
        // Re-bank, then read a mid-span grant with the same backlog.
        let mut tb2 = TaperBudget::new();
        for t in 0..40u64 {
            tb2.accrue(r, t, &taper, span, 0.0);
        }
        let g_mid = tb2.accrue(r, 32, &taper, span, f64::INFINITY);
        assert!(
            g_frontier > g_mid,
            "frontier grant must exceed mid-span grant: {g_frontier} vs {g_mid}"
        );
        assert!(
            (g_mid - r).abs() < 1e-9,
            "mid-span drains at the uniform budget rate r: {g_mid}"
        );
    }

    // --- P_lost tests ---

    #[test]
    fn test_p_lost_at_zero() {
        // At t=0, P_lost ≈ ε (just the base loss rate)
        let p = p_lost(0.0, 0.025, 0.050, 0.005);
        assert!(
            (p - 0.025).abs() < 0.005,
            "P_lost(0) ≈ ε: got {p}"
        );
    }

    #[test]
    fn test_p_lost_at_srtt() {
        // At t = SRTT, P_lost should be elevated (ACK expected by now)
        let p = p_lost(0.050, 0.025, 0.050, 0.005);
        assert!(
            p > 0.025 * 1.5,
            "P_lost(SRTT) should be well above ε: got {p}"
        );
    }

    #[test]
    fn test_p_lost_at_large_t() {
        // At t >> SRTT, P_lost → 1.0
        let p = p_lost(0.200, 0.025, 0.050, 0.005);
        assert!(
            p > 0.95,
            "P_lost(4×SRTT) should be near 1.0: got {p}"
        );
    }

    #[test]
    fn test_p_lost_monotone() {
        // P_lost should increase with age
        let srtt = 0.050;
        let rttvar = 0.005;
        let eps = 0.025;

        let p1 = p_lost(0.01, eps, srtt, rttvar);
        let p2 = p_lost(0.03, eps, srtt, rttvar);
        let p3 = p_lost(0.05, eps, srtt, rttvar);
        let p4 = p_lost(0.10, eps, srtt, rttvar);

        assert!(p1 < p2, "P_lost should increase: {p1} < {p2}");
        assert!(p2 < p3, "P_lost should increase: {p2} < {p3}");
        assert!(p3 < p4, "P_lost should increase: {p3} < {p4}");
    }

    #[test]
    fn test_p_lost_high_epsilon() {
        // With high loss rate, P_lost(0) should be high
        let p = p_lost(0.0, 0.5, 0.050, 0.005);
        assert!(
            (p - 0.5).abs() < 0.01,
            "P_lost(0) with ε=0.5 should be ~0.5: got {p}"
        );
    }

    #[test]
    fn test_normal_survival_basic() {
        // P(Z > 0) = 0.5
        let s = normal_survival(0.0);
        assert!((s - 0.5).abs() < 0.01, "survival(0) ≈ 0.5: got {s}");

        // P(Z > 2) ≈ 0.0228
        let s2 = normal_survival(2.0);
        assert!((s2 - 0.0228).abs() < 0.005, "survival(2) ≈ 0.023: got {s2}");

        // P(Z > -2) ≈ 0.9772
        let sm2 = normal_survival(-2.0);
        assert!((sm2 - 0.9772).abs() < 0.005, "survival(-2) ≈ 0.977: got {sm2}");
    }

    // --- Burst variance tests (Phase 5) ---

    #[test]
    fn test_burst_variance_iid_channel() {
        // For iid channel (p+q ≈ 1), σ²_burst → 1
        // This is the p=0.5, q=0.5 case: 1 + 2*(1-1)/1 = 1
        // We can't easily set GE params on LossEstimator, so test the formula directly
        // via p_fec_normal which uses sigma2_burst parameter
        let p_fec_iid = p_fec_normal(0.15, 0.10, 50.0, 1.0);   // σ²=1 (iid)
        let p_fec_burst = p_fec_normal(0.15, 0.10, 50.0, 3.0);  // σ²=3 (bursty)

        assert!(
            p_fec_iid > p_fec_burst,
            "iid should have higher P_fec than bursty: iid={p_fec_iid}, burst={p_fec_burst}"
        );
    }

    #[test]
    fn test_burst_variance_scenarios() {
        // Paper Section 8.3 reference values:
        // DC: σ²≈3.0, WiFi: σ²≈2.9, LTE: σ²≈3.8, Satellite: σ²≈5.1
        // Test the formula: σ² = 1 + 2(1-p-q)/(p+q)

        // DC: p=0.001, q=0.5 → 1 + 2*(1-0.501)/0.501 ≈ 2.99
        let s_dc: f64 = 1.0 + 2.0 * (1.0 - 0.001 - 0.5) / (0.001 + 0.5);
        assert!((s_dc - 3.0).abs() < 0.1, "DC σ²≈3.0: got {s_dc}");

        // LTE: p=0.01, q=0.2 → 1 + 2*(1-0.21)/0.21 ≈ 8.5
        // (actual values depend on exact p,q — test formula is correct)
        let s_lte: f64 = 1.0 + 2.0 * (1.0 - 0.01 - 0.2) / (0.01 + 0.2);
        assert!(s_lte > 1.0, "LTE σ² > 1 (bursty): got {s_lte}");
        assert!(s_lte > s_dc, "LTE more bursty than DC");
    }

    #[test]
    fn test_burst_variance_no_ge_data() {
        let est = LossEstimator::new();
        let s = burst_variance_factor(&est);
        assert_eq!(s, 1.0, "No GE data → σ²=1.0 (iid fallback)");
    }

    // --- P_fec normal approximation tests ---

    #[test]
    fn test_p_fec_normal_basic() {
        // With r well above ε/(1-ε), P_fec should be high
        let p = p_fec_normal(0.20, 0.10, 50.0, 1.0);
        assert!(p > 0.9, "r=0.20, ε=0.10, W=50 should have high P_fec: {p}");

        // With r barely above ε/(1-ε), P_fec should be moderate
        let p2 = p_fec_normal(0.12, 0.10, 50.0, 1.0);
        assert!(p2 > 0.0 && p2 < p, "Marginal r should give lower P_fec: {p2}");

        // With r below ε/(1-ε), P_fec = 0
        let p3 = p_fec_normal(0.05, 0.10, 50.0, 1.0);
        assert!(p3 < 0.01, "r < ε/(1-ε) should give P_fec ≈ 0: {p3}");
    }

    #[test]
    fn test_p_fec_increases_with_window() {
        // Larger window → tighter concentration → higher P_fec
        let p_small = p_fec_normal(0.15, 0.10, 20.0, 1.0);
        let p_large = p_fec_normal(0.15, 0.10, 200.0, 1.0);
        assert!(
            p_large > p_small,
            "Larger window should increase P_fec: W=20: {p_small}, W=200: {p_large}"
        );
    }
}
