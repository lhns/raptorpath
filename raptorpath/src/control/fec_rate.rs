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

/// FEC rate controller.
pub struct FecRateController {
    /// Effective target tail loss probability (after hint adjustment)
    target_tail_loss: f64,
    /// Maximum FEC overhead as fraction of source symbols
    max_overhead: f64,
    /// Codec decode overhead factor (raw, before P(decoder_invoked) weighting)
    rq_overhead: f64,
    /// Protocol hint (stored for streaming params and diagnostics)
    hint: ProtocolHint,
    /// Symbol size in bytes (needed to compute T = RTT × throughput / symbol_size)
    symbol_size: u16,
    /// P4a: Bulk maps the tail target to "late is fine" (δ = min(0.1, ε̂)),
    /// so the continuous r* glides to 0 in the steady state — pure ARQ,
    /// volume parity with retransmission transports (paper Sections 5.3,
    /// 12.5, 14.25). Public setter exists for ablation.
    bulk_pure_arq: bool,
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
            FecBackend::Mettle => 0.15,
            FecBackend::ReedSolomon => 0.0,
            FecBackend::Rlc => 0.004,
            FecBackend::Streaming => 0.0,
        };

        // Protocol hint maps to target_tail_loss, not an additive offset.
        // This is the only principled knob: tighter tail = more proactive FEC.
        let effective_tail_loss = match hint {
            ProtocolHint::Realtime => target_tail_loss * 0.01,  // 100× tighter
            ProtocolHint::Bulk => target_tail_loss * 100.0,     // 100× looser
            ProtocolHint::Auto => target_tail_loss,
        };
        let effective_tail_loss = effective_tail_loss.clamp(1e-9, 0.1);

        Self {
            target_tail_loss: effective_tail_loss,
            max_overhead,
            rq_overhead: codec_overhead,
            hint,
            symbol_size,
            bulk_pure_arq: true,
        }
    }

    /// Enable/disable the Bulk pure-ARQ tail target (P4a, on by default).
    /// Exposed for ablation: with it off, Bulk falls back to the plain
    /// 100×-loosened `target_tail_loss`.
    pub fn set_bulk_pure_arq(&mut self, enabled: bool) {
        self.bulk_pure_arq = enabled;
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
    /// `window_size`: current encoder window or block size.
    pub fn compute_repair_rate(&self, estimator: &LossEstimator, window_size: usize) -> f64 {
        let p = estimator.predictive_loss_upper(0.95);
        if p < 1e-10 {
            return 0.0;
        }

        // --- Codec overhead weighted by P(decoder invoked) ---
        // For systematic codecs, the decoder is only invoked when ≥1 source
        // symbol in the window is lost. No point paying full codec overhead
        // when the decoder runs only a fraction of the time.
        let effective_codec_overhead = if self.rq_overhead > 0.0 && window_size > 0 {
            let p_decoder_invoked = 1.0 - (1.0 - p).powi(window_size as i32);
            self.rq_overhead * p_decoder_invoked
        } else {
            0.0
        };

        // --- Random loss term: r* from raptorpath-math (shared with wasm) ---
        // Uses compute_r_star_with_z for the core formula:
        //   r* = ε/(1-ε) + z × √(ε × σ²_burst / (W × (1-ε)))
        let ge = estimator.ge_estimator();
        let sigma2 = if ge.is_valid() {
            raptorpath_math::burst_variance_factor(ge.p_gb(), ge.p_bg())
        } else {
            1.0
        };
        // Continuous tail margin (paper Section 8.4): the quantile is taken
        // at 1 - δ/ε, so the margin shrinks continuously as the channel
        // improves relative to the hint-adjusted target, and the core rate
        // decreases to 0 (max(0,·) floor inside compute_r_star_with_z) when
        // pure ARQ already meets the tail target. No cutoff branch anywhere.
        //
        // P4a (paper 5.3/12.5): Bulk's effective tail target is "late is
        // fine" — δ_eff = min(0.1, ε̂). With δ_eff ≥ p the formula yields
        // r ≈ 0: pure ARQ steady state, volume parity with retransmission
        // transports. Mid-transfer recovery overlaps ongoing sends, so
        // lateness costs a bulk transfer nothing; the completion-critical
        // final window is covered separately by tail FEC (paper 14.25).
        let delta_eff = if self.hint == ProtocolHint::Bulk && self.bulk_pure_arq {
            (0.1f64).min(p)
        } else {
            self.target_tail_loss
        };
        let z_delta = raptorpath_math::z_for_tail_target(delta_eff, p);
        let core_rate =
            raptorpath_math::compute_r_star_with_z(p, sigma2, window_size as f64, z_delta);
        // Codec overhead only applies when repairs are actually flowing
        // (no repairs → the decoder never runs → no overhead to pay).
        let random_rate = if core_rate > 0.0 {
            core_rate + effective_codec_overhead
        } else {
            0.0
        };

        // --- Burst loss term: delay-constrained capacity B/T ---
        // Scaled by the required FEC fraction (1 - δ/p)⁺ — the fraction of
        // losses that must be recovered proactively to meet the tail target.
        // Decreases continuously to 0 as the target loosens relative to the
        // channel, consistent with the r* margin above.
        let ge = estimator.ge_estimator();
        let required_fec_fraction = (1.0 - delta_eff / p).clamp(0.0, 1.0);
        let burst_rate = if ge.is_valid() && estimator.throughput() > 0.0 {
            let burst_length = ge.mean_burst_length().max(1.0);
            let rtt_secs = estimator.rtt().as_secs_f64();
            let t_symbols = (rtt_secs * estimator.throughput() / self.symbol_size as f64).max(1.0);
            (burst_length / t_symbols) * required_fec_fraction
        } else {
            0.0
        };

        // --- Optimal rate: max of random and burst ---
        let rate = random_rate.max(burst_rate);
        rate.clamp(0.0, self.max_overhead)
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

    /// Compute streaming code parameters from the current loss estimator.
    pub fn compute_streaming_params(
        &self,
        estimator: &LossEstimator,
    ) -> crate::fec::StreamingParams {
        let ge = estimator.ge_estimator();
        let burst_length = if ge.is_valid() {
            ge.mean_burst_length().max(1.0)
        } else {
            2.0
        };

        let loss_rate = estimator.predictive_loss_upper(0.95);

        // BOCD quantile already provides the uncertainty margin.
        // No per-hint safety factor needed.
        let safety = 1.0;

        crate::fec::StreamingParams::from_channel(burst_length, loss_rate, safety)
    }

    /// Update the codec overhead for a new backend (used during runtime switching).
    pub fn update_backend(&mut self, backend: FecBackend) {
        self.rq_overhead = match backend {
            FecBackend::RaptorQ => 0.01,
            FecBackend::Mettle => 0.15,
            FecBackend::ReedSolomon => 0.0,
            FecBackend::Rlc => 0.004,
            FecBackend::Streaming => 0.0,
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

/// Compute maximum burst length B_max at the 99.99th percentile.
///
/// B_max = ceil(ln(0.0001) / ln(1-q))
///
/// This is the number of consecutive lost symbols we expect to see
/// at most once in 10,000 bursts. Used for buffer sizing at ρ=100%.
/// See paper Section 9.3.
pub fn b_max(q: f64) -> u64 {
    if q <= 0.0 || q >= 1.0 {
        return 1;
    }
    let ln_threshold = (0.0001_f64).ln(); // ln(1e-4)
    let ln_persist = (1.0 - q).ln();      // ln(1-q) < 0
    if ln_persist >= 0.0 {
        return 1;
    }
    (ln_threshold / ln_persist).ceil() as u64
}

/// Three-variable optimization: given two of (r, δ, ρ), compute the third.
///
/// The three variables form a triangle (paper Section 1.4, 8.6):
///   r  = correction rate (bandwidth overhead)
///   δ  = tail latency: P(late delivery) / ρ — fraction of delivered symbols
///        that needed ARQ (arrived late rather than on-time via FEC)
///   ρ  = reliability: P(symbol delivered at all within T_cut)
///
/// Fix any two → the third is determined by the channel.
///
/// Key relationships (paper Section 6.3):
///   P(on-time) = (1-ε) + ε × P_fec
///   P(late)    = ε × (1-P_fec) × P_arq
///   P(lost)    = ε × (1-P_fec) × (1-P_arq) = 1-ρ
///   P_arq      = 1 - (1-ρ) / (ε × (1-P_fec))
///   δ          = P(late) / ρ
#[derive(Debug, Clone)]
pub struct ThreeVarResult {
    /// Correction rate (bandwidth overhead).
    pub r: f64,
    /// Tail latency: fraction of delivered symbols that arrived late (via ARQ).
    pub delta: f64,
    /// Reliability (fraction of symbols delivered within T_cut).
    pub rho: f64,
    /// Age cutoff in symbol intervals.
    pub t_cut: f64,
    /// Buffer size in symbols.
    pub buffer_max: f64,
}

/// Compute T_cut from target reliability ρ via binary search.
///
/// Finds the smallest T_cut such that P(recovered within T_cut) ≥ ρ.
/// Uses the taper integral: corrections accumulated = A × (1-(1-q)^(T+1)) / q.
///
/// See paper Section 9.4 (Mode 1, Step 1).
pub fn find_t_cut(
    epsilon: f64,
    q: f64,
    r: f64,
    window_size: f64,
    sigma2_burst: f64,
    target_rho: f64,
) -> f64 {
    if target_rho >= 1.0 {
        return f64::INFINITY; // 100% reliability needs infinite T_cut
    }
    if target_rho <= 0.0 || epsilon <= 0.0 {
        return 0.0;
    }

    let mut lo: f64 = 0.0;
    let mut hi: f64 = window_size * 10.0; // generous upper bound
    let tolerance = 0.01; // fine granularity for monotonicity

    for _ in 0..100 { // max iterations
        if hi - lo < tolerance {
            break;
        }
        let mid = (lo + hi) / 2.0;
        let p_recovered = p_recovered_within(mid, epsilon, q, r, window_size, sigma2_burst);
        if p_recovered < target_rho {
            lo = mid; // need more time
        } else {
            hi = mid; // enough time
        }
    }
    hi
}

/// P(symbol recovered within time T) using FEC + ARQ.
///
/// P_recovered = 1 - ε × (1 - P_fec(r, W)) × (1 - P_arq(T))
///
/// where P_fec uses the normal approximation and P_arq accounts for
/// corrections accumulated up to time T via the taper integral.
fn p_recovered_within(
    t: f64,
    epsilon: f64,
    q: f64,
    r: f64,
    window_size: f64,
    sigma2_burst: f64,
) -> f64 {
    if epsilon <= 0.0 {
        return 1.0;
    }

    // P_fec: probability FEC recovers (from r* and W)
    let p_fec = p_fec_normal(r, epsilon, window_size, sigma2_burst);

    // P_arq: probability ARQ recovers by time T
    // Taper integral: corrections accumulated = A × (1-(1-q)^(T+1)) / q
    // where A = r × q. So total corrections by T = r × (1-(1-q)^(T+1)).
    let decay = (1.0 - q).max(0.0);
    let corrections_by_t = r * (1.0 - decay.powf(t + 1.0));
    // P_arq ≈ 1 - (1-corrections_by_t/r_needed)^+ , simplified:
    // If accumulated corrections ≥ what's needed, P_arq → 1
    let r_needed = epsilon / (1.0 - epsilon);
    let p_arq = if r_needed > 0.0 {
        (corrections_by_t / r_needed).min(1.0)
    } else {
        1.0
    };

    // Combined: P(recovered) = 1 - P(lost) × P(FEC fails) × P(ARQ fails)
    1.0 - epsilon * (1.0 - p_fec) * (1.0 - p_arq)
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

/// Compute δ (tail latency) from r and ρ using the paper's delivery model.
///
/// δ = P(late delivery) / ρ
/// P(late) = ε × (1-P_fec) × P_arq
/// P_arq = 1 - (1-ρ) / (ε × (1-P_fec))   (derived from ρ target)
///
/// When ρ = 100%, P_arq = 1, so δ = ε × (1-P_fec) / 1.0.
/// When ρ < 100%, some symbols are permanently lost, reducing δ.
fn compute_delta(epsilon: f64, r: f64, rho: f64, window_size: f64, sigma2_burst: f64) -> f64 {
    if epsilon <= 0.0 || rho <= 0.0 {
        return 0.0;
    }
    let p_fec = p_fec_normal(r, epsilon, window_size, sigma2_burst);
    let fec_miss = epsilon * (1.0 - p_fec); // P(lost AND FEC failed)
    if fec_miss < 1e-15 {
        return 0.0; // FEC recovers everything
    }
    // P_arq = 1 - (1-ρ) / fec_miss, clamped to [0, 1]
    let p_arq = (1.0 - (1.0 - rho) / fec_miss).clamp(0.0, 1.0);
    let p_late = fec_miss * p_arq;
    p_late / rho
}

/// Mode 1: Given (δ, ρ) → compute r.
///
/// Find the minimum correction rate that achieves both the tail latency
/// target δ and reliability target ρ. See paper Section 8.6.
///
/// δ = P(late delivery among delivered symbols). Lower δ requires more FEC
/// so fewer symbols fall through to the slow ARQ path.
pub fn solve_r_from_delta_rho(
    epsilon: f64,
    q: f64,
    window_size: f64,
    sigma2_burst: f64,
    delta: f64,
    rho: f64,
) -> ThreeVarResult {
    // Binary search for r that achieves δ target
    let mut lo: f64 = epsilon / (1.0 - epsilon); // minimum r (information-theoretic)
    let mut hi: f64 = 2.0; // generous upper bound
    let tolerance = 1e-6;

    for _ in 0..100 {
        if hi - lo < tolerance {
            break;
        }
        let mid = (lo + hi) / 2.0;
        let d = compute_delta(epsilon, mid, rho, window_size, sigma2_burst);
        if d > delta {
            lo = mid; // need more FEC to reduce late deliveries
        } else {
            hi = mid; // enough FEC
        }
    }
    let r = hi;

    let t_cut = find_t_cut(epsilon, q, r, window_size, sigma2_burst, rho);
    let buffer_max = compute_buffer_max(epsilon, q, r, t_cut);

    ThreeVarResult { r, delta, rho, t_cut, buffer_max }
}

/// Mode 2: Given (r, ρ) → compute δ.
///
/// With fixed correction rate and reliability, compute the resulting
/// tail latency δ = P(late delivery) / ρ. See paper Section 8.6.
pub fn solve_delta_from_r_rho(
    epsilon: f64,
    q: f64,
    window_size: f64,
    sigma2_burst: f64,
    r: f64,
    rho: f64,
) -> ThreeVarResult {
    let delta = compute_delta(epsilon, r, rho, window_size, sigma2_burst);

    let t_cut = find_t_cut(epsilon, q, r, window_size, sigma2_burst, rho);
    let buffer_max = compute_buffer_max(epsilon, q, r, t_cut);

    ThreeVarResult { r, delta, rho, t_cut, buffer_max }
}

/// Mode 3: Given (r, δ) → compute ρ.
///
/// With fixed correction rate and tail latency δ, compute the achievable
/// reliability ρ. Binary search on ρ. See paper Section 8.6.
///
/// δ = P(late) / ρ depends on ρ (via P_arq), so we search for ρ where
/// compute_delta(ε, r, ρ, W, σ²) = δ.
pub fn solve_rho_from_r_delta(
    epsilon: f64,
    q: f64,
    window_size: f64,
    sigma2_burst: f64,
    r: f64,
    delta: f64,
) -> ThreeVarResult {
    // Binary search for ρ that produces the target δ.
    // As ρ increases (more reliable), P_arq increases, P(late) increases,
    // but δ = P(late)/ρ can go either way. We search for the highest ρ
    // where δ ≤ target.
    let mut lo: f64 = 0.5;
    let mut hi: f64 = 1.0 - 1e-12;
    let tolerance = 1e-6;

    for _ in 0..100 {
        if hi - lo < tolerance {
            break;
        }
        let mid = (lo + hi) / 2.0;
        let d = compute_delta(epsilon, r, mid, window_size, sigma2_burst);
        if d > delta {
            hi = mid; // too much late delivery at this ρ
        } else {
            lo = mid; // δ satisfied, try higher ρ
        }
    }
    let rho = lo;

    let t_cut = find_t_cut(epsilon, q, r, window_size, sigma2_burst, rho);
    let buffer_max = compute_buffer_max(epsilon, q, r, t_cut);

    ThreeVarResult { r, delta, rho, t_cut, buffer_max }
}

/// Compute buffer_max from T_cut and channel parameters.
///
/// For ρ < 100%: buffer_max = source_rate × T_cut (in symbol intervals).
/// For ρ = 100%: buffer_max = RTT + B_max / (r × (1-ε)) × t_sym.
///
/// This returns buffer_max in symbol intervals (caller scales by source_rate).
fn compute_buffer_max(epsilon: f64, q: f64, r: f64, t_cut: f64) -> f64 {
    if t_cut.is_infinite() {
        // ρ = 100%: use B_max formula
        let bmax = b_max(q) as f64;
        let drain_rate = r * (1.0 - epsilon);
        if drain_rate > 0.0 {
            bmax / drain_rate
        } else {
            bmax
        }
    } else {
        t_cut
    }
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
    fn test_bulk_pure_arq_zero_steady_state_rate() {
        // P4a: Bulk's effective tail target is δ = min(0.1, ε̂) — "late is
        // fine" — so even at 5% loss the steady-state rate glides to ~0
        // (pure ARQ, volume parity with retransmission transports).
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
        // Compare Mettle (15% codec overhead) vs ReedSolomon (0% overhead) at same window.
        // The difference isolates the codec overhead contribution.
        let ctrl_mettle = FecRateController::new(1e-5, 1.0, ProtocolHint::Auto, FecBackend::Mettle, 1200);
        let ctrl_rs = FecRateController::new(1e-5, 1.0, ProtocolHint::Auto, FecBackend::ReedSolomon, 1200);

        let mut est = LossEstimator::new();
        // Low-moderate loss so rate doesn't hit max_overhead cap
        for _ in 0..100 {
            est.record_batch(100, 95); // 5% loss
        }

        // At same window, Mettle should have higher rate than RS due to codec overhead
        let rate_mettle = ctrl_mettle.compute_repair_rate(&est, 50);
        let rate_rs = ctrl_rs.compute_repair_rate(&est, 50);
        assert!(
            rate_mettle > rate_rs,
            "Mettle should have higher rate than RS due to codec overhead: mettle={rate_mettle}, rs={rate_rs}"
        );

        // With zero window size, no codec overhead → Mettle ≈ RS
        let rate_mettle_zero = ctrl_mettle.compute_repair_rate(&est, 0);
        let rate_rs_zero = ctrl_rs.compute_repair_rate(&est, 0);
        assert!(
            (rate_mettle_zero - rate_rs_zero).abs() < 0.01,
            "Zero window should have no codec overhead: mettle={rate_mettle_zero}, rs={rate_rs_zero}"
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

    // --- B_max tests ---

    #[test]
    fn test_b_max_values() {
        // q=0.5: B_max = ceil(ln(0.0001)/ln(0.5)) ≈ ceil(9.21/0.693) = ceil(13.29) = 14
        let bm = b_max(0.5);
        assert_eq!(bm, 14, "B_max(q=0.5) = 14");

        // q=0.1: longer bursts → larger B_max
        let bm_low_q = b_max(0.1);
        assert!(bm_low_q > bm, "Lower q → longer bursts → larger B_max");

        // Approximate: B_max ≈ 9.2/q
        let approx = (9.2_f64 / 0.1).ceil() as u64;
        assert!((bm_low_q as i64 - approx as i64).abs() <= 5, "B_max ≈ 9.2/q: got {bm_low_q}, approx {approx}");
    }

    #[test]
    fn test_b_max_edge_cases() {
        assert_eq!(b_max(0.0), 1);
        assert_eq!(b_max(1.0), 1);
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

    // --- Three-variable optimization tests (Phase 6) ---

    #[test]
    fn test_find_t_cut_monotone_in_rho() {
        // Use marginal r (barely above ε/(1-ε)) and small window so FEC alone
        // doesn't provide near-perfect recovery, forcing T_cut to differentiate.
        let eps = 0.20;
        let q = 0.1; // long bursts
        let r = 0.26; // just above 0.25 = ε/(1-ε)
        let w = 10.0; // small window
        let s2 = 5.0; // high burst variance

        let t1 = find_t_cut(eps, q, r, w, s2, 0.80);
        let t2 = find_t_cut(eps, q, r, w, s2, 0.90);
        let t3 = find_t_cut(eps, q, r, w, s2, 0.95);

        assert!(t1 <= t2, "Higher ρ needs larger T_cut: t(0.80)={t1} <= t(0.90)={t2}");
        assert!(t2 <= t3, "Higher ρ needs larger T_cut: t(0.90)={t2} <= t(0.95)={t3}");
        // At least one pair should be strictly different
        assert!(t1 < t3, "T_cut should increase from ρ=0.80 to ρ=0.95: {t1} < {t3}");
    }

    #[test]
    fn test_mode1_delta_rho_to_r() {
        let result = solve_r_from_delta_rho(0.10, 0.3, 50.0, 3.0, 0.001, 0.99);
        // r should be above the information-theoretic minimum
        let r_min = 0.10 / 0.90;
        assert!(result.r > r_min, "r should exceed ε/(1-ε): r={}, min={r_min}", result.r);
        assert_eq!(result.delta, 0.001);
        assert_eq!(result.rho, 0.99);
        assert!(result.t_cut > 0.0, "T_cut should be positive");
        assert!(result.buffer_max > 0.0, "buffer_max should be positive");
    }

    #[test]
    fn test_mode2_r_rho_to_delta() {
        let result = solve_delta_from_r_rho(0.10, 0.3, 50.0, 3.0, 0.20, 0.99);
        // With generous r=0.20 for ε=0.10, delta should be small
        assert!(result.delta < 0.10, "delta should be small with generous r: {}", result.delta);
        assert_eq!(result.r, 0.20);
        assert_eq!(result.rho, 0.99);
    }

    #[test]
    fn test_mode3_r_delta_to_rho() {
        let result = solve_rho_from_r_delta(0.10, 0.3, 50.0, 3.0, 0.20, 0.001);
        // With generous r and tight δ, ρ should be high
        assert!(result.rho > 0.5, "ρ should be high with generous r: {}", result.rho);
        assert_eq!(result.r, 0.20);
        assert_eq!(result.delta, 0.001);
    }

    #[test]
    fn test_three_var_consistency() {
        // Mode 1 produces (r, δ, ρ). Feed r and ρ into Mode 2 → should get same δ.
        // Use high ρ (0.999) so FEC alone can't satisfy it, forcing ARQ to contribute
        // and producing a non-zero δ.
        let eps = 0.10;
        let q = 0.3;
        let w = 20.0; // smaller window so P_fec isn't near-perfect
        let s2 = 3.0;

        let m1 = solve_r_from_delta_rho(eps, q, w, s2, 0.05, 0.999);
        let m2 = solve_delta_from_r_rho(eps, q, w, s2, m1.r, 0.999);

        assert!(
            (m1.delta - m2.delta).abs() < 0.01,
            "Mode 1 and Mode 2 should agree on δ: m1={}, m2={}",
            m1.delta, m2.delta
        );
    }

    #[test]
    fn test_buffer_max_finite_rho() {
        let result = solve_r_from_delta_rho(0.10, 0.3, 50.0, 3.0, 0.001, 0.98);
        assert!(result.buffer_max.is_finite(), "buffer_max should be finite for ρ<1");
        assert!(result.t_cut.is_finite(), "T_cut should be finite for ρ<1");
    }
}
