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
use raptorpath_math::p_fec_normal;
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
    /// BULKNESS β = `bulkness_of_delta(delta_price(hint))` ∈ [0, 1] — the
    /// log-position of this contract's latency price between the Auto anchor
    /// (β = 0) and the Bulk anchor (β = 1), resolved ONCE at construction
    /// (paper §16.81/§16.82).
    ///
    /// **THIS FIELD IS THE REPAIR OF A MODE SWITCH.** `compute_repair_rate`
    /// used to set `bulk_late_is_fine = hint == Bulk && bulk_pure_arq`, a hint
    /// EQUALITY that swapped the entire `δ_eff` law inside
    /// `raptorpath_math::controller_rate` — the largest NO-MODE-SWITCH
    /// violation left in the engine, and the mechanism by which the r leg sat
    /// at the corner `r* = 0` on every scored battery. The rate is now the mix
    ///
    /// ```text
    ///     r(β) = (1 − β)·r_anchor + β·r_late-is-fine
    /// ```
    ///
    /// with BOTH terms always computed. β = 0 at Realtime AND Auto (the
    /// clamp) and 1 at Bulk (x/x), all three exactly, so the shipped presets
    /// are BYTE-IDENTICAL — pinned by `the_rate_mix_is_byte_identical_at_the_
    /// presets` — while a δ between them (`RWM_DELTA`) now yields a rate
    /// between them instead of a step.
    bulkness: f64,
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

        // The CONTRACT'S δ maps to target_tail_loss, not an additive offset.
        // This is the only principled knob: tighter tail = more proactive FEC.
        //
        // Read through the DIAL since 2026-09-08 (§16.81):
        // `ζ(δ(hint))` in place of the enum's own `hint.tail_loss_scale()`.
        // Bit-identical at all three presets — δ = 0.5/ζ and ζ = 0.5/δ are
        // exact f64 inverses at ζ ∈ {0.01, 1, 100}, pinned by
        // `zeta_of_the_hints_delta_is_the_hints_own_scale` — and it means a
        // δ set BETWEEN the presets (`RWM_DELTA`) moves the tail target
        // continuously with b and β instead of leaving it pinned to a preset.
        let delta = crate::net::delta_price(hint);
        let effective_tail_loss = target_tail_loss * raptorpath_math::zeta_of_delta(delta);
        let effective_tail_loss = effective_tail_loss.clamp(1e-9, 0.1);

        Self {
            target_tail_loss: effective_tail_loss,
            max_overhead,
            rq_overhead: codec_overhead,
            hint,
            bulkness: raptorpath_math::bulkness_of_delta(delta),
            symbol_size,
            saturation_cap_enabled: true,
            bulk_pure_arq: true,
            completion_exposure: 0.0,
            tail_provision: crate::gates::get().rstar_tail,
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
    ///
    /// **RE-READ AS `β := 0`** (§16.81). The flag no longer selects
    /// a law; it zeroes the mixing weight, which is the SAME arithmetic it
    /// always performed — the ablation arm was never anything but "evaluate
    /// the anchor term", and at Realtime/Auto (β = 0 already) it was, and
    /// remains, exactly inert. The existing ablation assertions
    /// (`test_bulk_pure_arq_zero_steady_state_rate`, `gate_suite`'s
    /// `ablation_p4a_bulk_pure_arq`) hold unchanged under this reading and
    /// were not edited.
    pub fn set_bulk_pure_arq(&mut self, enabled: bool) {
        self.bulk_pure_arq = enabled;
    }

    /// The effective mixing weight β of `r(β) = (1−β)·r_anchor + β·r_bulk`:
    /// this contract's bulkness, or 0 when the P4a ablation zeroes it.
    fn effective_bulkness(&self) -> f64 {
        if self.bulk_pure_arq { self.bulkness } else { 0.0 }
    }

    /// P6: set the completion exposure χ ∈ [0, 1] (paper Section 14.26)
    /// for callers that know the remaining send time T_rem — compute it
    /// with `raptorpath_math::completion_exposure(t_rem, srtt, rttvar)`.
    /// The production tunnel never calls this (endless stream ⇒ χ = 0).
    // Test-only consumer: tests/gate_suite.rs drives the χ arm through it.
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
        // ── THE RATE MIX, r(β) = (1−β)·r_anchor + β·r_late-is-fine ────────
        // §16.81/§16.82. ONE `RateInputs` is built, and the ONE
        // shared `controller_rate` is evaluated TWICE — once with the
        // late-is-fine δ_eff law off (the anchor term) and once with it on
        // (the Bulk term) — and the two are blended by the contract's own
        // bulkness. Both terms are ALWAYS computed: there is no `hint ==`
        // anywhere on this path, and no δ threshold selects a law.
        //
        // COST, disclosed: two `controller_rate` evaluations per rate
        // computation instead of one. The function is a few dozen flops plus
        // one `normal_quantile`; the rate site is per-ack under a lock that
        // already holds the estimator, and the L0 gate's own timing has never
        // resolved it. Not conditionalised — a `if β == 0 { … }` fast path
        // would be the mode switch again, wearing a performance costume.
        let mut inputs = raptorpath_math::RateInputs {
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
            // Rebound below for each of the two terms of the mix; the value
            // here is the anchor term's.
            bulk_late_is_fine: false,
            // P6 (paper 14.26): 0.0 unless a T_rem-aware caller set it —
            // the production tunnel is an endless stream, so mid-stream
            // semantics (δ_eff = ε̂, r* = 0) apply permanently.
            completion_exposure: self.completion_exposure,
            // P10a (paper 14.28): mid-stream repair floor for payloads
            // whose latency feeds back (TCP-in-tunnel). 0.0 default.
            inner_feedback: self.inner_feedback,
            saturation_cap: self.saturation_cap_enabled,
            max_overhead: self.max_overhead,
        };
        // The ANCHOR term: δ_eff = the contract's own tail target.
        inputs.bulk_late_is_fine = false;
        let r_anchor = raptorpath_math::controller_rate(&inputs);
        // The LATE-IS-FINE term: δ_eff = the §14.26 completion-exposure glide.
        inputs.bulk_late_is_fine = true;
        let r_bulk = raptorpath_math::controller_rate(&inputs);
        // The mix. EXACT at the presets: β = 0 gives `1·r_anchor + 0·r_bulk`
        // and β = 1 gives `0·r_anchor + 1·r_bulk`, both bit-exact for finite
        // terms (`controller_rate` clamps to [0, max_overhead], so they are).
        let beta = self.effective_bulkness();
        (1.0 - beta) * r_anchor + beta * r_bulk
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

    /// Get diagnostics for monitoring.
    pub fn diagnostics(&self) -> FecDiagnostics {
        FecDiagnostics {
            actual_failure_rate: 0.0,
            pi_correction: 0.0,
        }
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
    // Test-only reader: the taper geometric-sum law tests below.
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

/// P_lost(t) (paper §3.4), the shared math crate's single definition —
/// re-exported so `control::fec_rate::p_lost` keeps its path.
pub use raptorpath_math::p_lost;

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

    // Test-only consumers: the BudgetAllocator conservation tests below.
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
}

#[cfg(test)]
mod tests;
