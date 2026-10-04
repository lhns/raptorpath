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
//!    §4.2), controlling the FEC/NACK balance. Tighter tail = more
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
    /// `FecRateController::new_with_toggles`). ζ is the hint's one declared
    /// price ratio — Realtime prices a late symbol 100× dearer than Auto,
    /// Bulk 100× cheaper — and everything else hint-coupled derives from it.
    /// The Copa δ mapping (scheduler, paper §4.1) consumes it as the latency
    /// price: δ(hint) = δ_auto / ζ(hint).
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
    /// Bulkness β = `bulkness_of_delta(delta_price(hint))` ∈ [0, 1] — the
    /// log-position of this contract's latency price between the Auto anchor
    /// (β = 0) and the Bulk anchor (β = 1), resolved once at construction.
    /// It weights the rate mix (paper §4.5)
    ///
    /// ```text
    ///     r(β) = (1 − β)·r_anchor + β·r_late-is-fine
    /// ```
    ///
    /// with both terms always computed, so no hint equality selects a law.
    /// β is exactly 0 at Realtime and Auto and 1 at Bulk, so the presets are
    /// byte-identical to the single-term rates (pinned by
    /// `the_rate_mix_is_byte_identical_at_the_presets`), and a δ between
    /// them (`RWM_DELTA`) yields a rate between them instead of a step.
    bulkness: f64,
    /// Symbol size in bytes (needed to compute T = RTT × throughput / symbol_size)
    symbol_size: u16,
    /// P5: cap the repair rate at the p99(r) saturation point (paper
    /// §4.4). Past r_sat, extra repairs displace source symbols
    /// and stretch the recovery window faster than the shrinking FEC-miss
    /// cost pays back — more FEC hurts the tail.
    saturation_cap_enabled: bool,
    /// P4a/P6: Bulk maps the tail target to the completion-exposure glide
    /// δ_eff = ε̂ + (0.05 − ε̂)·χ (paper §4.6): mid-stream (χ = 0)
    /// δ_eff = ε̂ and r* = 0 identically — pure ARQ, volume parity with
    /// retransmission transports (paper §4.5) — and near a known end of
    /// stream χ → 1 ramps r to the tail budget. Public setter exists for
    /// ablation.
    bulk_pure_arq: bool,
    /// P6: completion exposure χ ∈ [0, 1] (paper §4.6). The production
    /// tunnel is an endless stream — there is no known T_rem, so χ stays 0
    /// and Bulk's steady state is pure ARQ. Drivers that do know T_rem (the L0 gate, the wasm sim) set it per tick via
    /// `set_completion_exposure` with `raptorpath_math::completion_exposure`.
    completion_exposure: f64,
    /// #46: window-mass burst-tail provisioning (paper §4.3, ADR-0063).
    /// The GE geometric burst law under-provisions r* by 2-4x on real
    /// bursty traces (paper §2.5); when enabled, the rate includes the
    /// `r_star_mass` quantile term fed by the receiver's own multi-scale
    /// window loss-mass statistics (GilbertElliottEstimator mass_stats).
    /// Env gate `RWM_RSTAR_TAIL` (default ON; `=0` is GE-only provisioning).
    tail_provision: bool,
    /// P10a: inner-feedback weight in [0, 1] (paper §4.4). The Bulk glide's
    /// mid-stream r* = 0 prices volume; when the payload is itself a
    /// latency-sensitive control loop (TCP inside the tunnel), each
    /// unrepaired loss stalls the inner flow's in-order delivery
    /// ~min(1.5×SRTT_outer, RTO_inner) and that stall feeds back into the
    /// inner send rate. Weight 1 enables the `inner_feedback_floor`
    /// mid-stream repair floor — the smallest r whose residual stall
    /// fraction sits within delivery-jitter noise. Weight 0 (the default,
    /// including the production tunnel via config `inner_feedback_weight`)
    /// is the pure glide: the inner flow absorbs the residual stalls, and
    /// floor repairs would displace source symbols inside the same
    /// inner-limited loop.
    inner_feedback: f64,
}

/// Everything one evaluation of the rate mix reads (see
/// [`FecRateController::rate_snapshot`]): the shared `RateInputs` and the
/// contract's mixing weight β. Plain data (`Copy`), so it outlives the locks
/// it was taken under.
#[derive(Debug, Clone, Copy)]
pub struct RateSnapshot {
    inputs: raptorpath_math::RateInputs,
    beta: f64,
}

impl RateSnapshot {
    /// The rate mix `r(β) = (1−β)·r_anchor + β·r_late-is-fine` (paper §4.5).
    pub fn rate(&self) -> f64 {
        let mut inputs = self.inputs;
        // The anchor term: δ_eff = the contract's own tail target.
        inputs.bulk_late_is_fine = false;
        let r_anchor = raptorpath_math::controller_rate(&inputs);
        // The late-is-fine term: δ_eff = the completion-exposure glide (paper §4.6).
        inputs.bulk_late_is_fine = true;
        let r_bulk = raptorpath_math::controller_rate(&inputs);
        // The mix, exact at the presets: β = 0 gives `1·r_anchor + 0·r_bulk`
        // and β = 1 gives `0·r_anchor + 1·r_bulk`, both bit-exact for finite
        // terms (`controller_rate` clamps to [0, max_overhead], so they are).
        // Both terms are always evaluated: no β = 0 fast path (a mode switch).
        let beta = self.beta;
        (1.0 - beta) * r_anchor + beta * r_bulk
    }
}

/// Upper bound (µs) on the repair-rate cadence: the sender's derived
/// quantities refresh on a ~5 ms tick (store cap, pipeline depth, in-flight
/// cap), and the rate joins them. Provenance: a RESOURCE bound stated
/// outside the rate law, not a law parameter — one evaluation costs
/// ~0.1 ms (VM, W = 200, the Stage-1b′ microbenchmark), so ≤ 1 per 5 ms
/// caps the rate at ~2 % of a core. It changes no formula; it bounds how
/// stale the formula's output may be.
pub const RATE_CADENCE_MAX_US: u64 = 5_000;

/// The window sender's repair-rate cadence (Stage 1c, a DISCLOSED numerical
/// change): the rate mix is re-evaluated at most once per cadence period
/// instead of once per emitted source symbol, and every window-path reader
/// (source emission, `serve_gaps`, the cumulative-ack advance) reads the one
/// cached value.
///
/// Why: one evaluation costs ~0.1 ms (the window-mass solve of the anchor
/// term, which the mix always evaluates — no β = 0 skip), and the sender
/// emits ~9 300 source symbols/s at c2: per-symbol evaluation is a full core.
///
/// The period is `min(RATE_CADENCE_MAX_US, SRTT/4)` of the path it reads —
/// continuous in the measured SRTT, no hint or δ key. The estimator itself
/// moves on the ack clock (≥ SRTT granularity per loss event), so a rate at
/// most SRTT/4 old lags it by at most a quarter of its own update period.
///
/// AGE IS THE ONLY INVALIDATION. The encoder window W and the worst-ε path
/// are INPUTS sampled at the evaluation instant, exactly like the estimator
/// state — never cache keys. W is `encoder.window_size()`, the live FILL of
/// the sliding window, which moves on every emitted symbol and every ack
/// advance; the worst-ε pick flips on noise when the per-path ε̂ are
/// near-tied (wire v9). Keying on either re-evaluated at its change rate —
/// at dual c1 the unfixed v9 sender ran ~12 500 evaluations/s, 99.7 % of
/// them W-change misses (S9) — so the cadence never bound. Staleness is
/// exactly bounded: the returned rate is bit-identical to the fresh rate
/// (that instant's estimator, W and worst path) at an instant less than one
/// period earlier (pinned by `rate_cadence_*` tests, including a W that
/// moves every ack and a flipping worst path).
#[derive(Debug, Clone, Default)]
pub struct RepairRateCache {
    /// (computed-at µs, window, path, period µs, rate). W and path are
    /// recorded for the `rce=` gauge only; `hit` does not read them.
    entry: Option<(u64, usize, u32, u64, f64)>,
    /// Evaluations performed (the mechanism gauge; tests assert it runs).
    pub evaluations: u64,
    /// `[DIAG] rce=` gauges. `miss_cold`/`miss_age`: why an evaluation ran
    /// (no entry yet / the entry aged out — the only two causes).
    /// `miss_window`/`miss_path`: evaluations whose W / worst path differed
    /// from the previous evaluation's (input movement, not a cause; the
    /// names keep the token's field order).
    pub miss_cold: u64,
    pub miss_window: u64,
    pub miss_path: u64,
    pub miss_age: u64,
    /// Cumulative ns spent in the rate evaluation itself (callers that
    /// time it report through [`Self::add_eval_ns`]).
    pub eval_ns: u64,
}

impl RepairRateCache {
    /// The cadence period for a path with smoothed RTT `srtt`.
    pub fn period_us(srtt: std::time::Duration) -> u64 {
        RATE_CADENCE_MAX_US.min(srtt.as_micros() as u64 / 4)
    }

    /// The cached rate if it is younger than its period at `now_us`, else
    /// `None`. Age only: W and the path are inputs, not keys.
    pub fn hit(&self, now_us: u64) -> Option<f64> {
        match self.entry {
            Some((at, _, _, period, rate)) if now_us.saturating_sub(at) < period => Some(rate),
            _ => None,
        }
    }

    /// Record a fresh evaluation made at `now_us` with inputs (`window`, `path`).
    pub fn store(&mut self, now_us: u64, window: usize, path: u32, period_us: u64, rate: f64) {
        match self.entry {
            None => self.miss_cold += 1,
            Some((_, w, p, _, _)) => {
                self.miss_age += 1;
                self.miss_window += (w != window) as u64;
                self.miss_path += (p != path) as u64;
            }
        }
        self.entry = Some((now_us, window, path, period_us, rate));
        self.evaluations += 1;
    }

    /// Account `ns` of evaluation time (the `[DIAG] rce=` cost gauge).
    pub fn add_eval_ns(&mut self, ns: u64) {
        self.eval_ns = self.eval_ns.saturating_add(ns);
    }

    /// The `[DIAG]` token: `rce=<evals>/c<cold>/w<window>/p<path>/a<age>/us<eval µs>`.
    pub fn diag_token(&self) -> String {
        format!(
            " rce={}/c{}/w{}/p{}/a{}/us{}",
            self.evaluations,
            self.miss_cold,
            self.miss_window,
            self.miss_path,
            self.miss_age,
            self.eval_ns / 1_000
        )
    }

    /// The cached rate, or `eval()` (stored) when stale.
    pub fn get_or_eval(
        &mut self,
        now_us: u64,
        window: usize,
        path: u32,
        period_us: u64,
        eval: impl FnOnce() -> f64,
    ) -> f64 {
        if let Some(r) = self.hit(now_us) {
            return r;
        }
        let r = eval();
        self.store(now_us, window, path, period_us, r);
        r
    }
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
        // RLC's codec overhead. The block-only codecs (RaptorQ, RS) cannot
        // run since ADR-0069 (`net::pipeline_backend` rejects them) and
        // carry none.
        let codec_overhead = match backend {
            FecBackend::Rlc => 0.004,
            FecBackend::RaptorQ | FecBackend::ReedSolomon => 0.0,
        };

        // The contract's δ maps to target_tail_loss, not an additive offset:
        // tighter tail = more proactive FEC. Read through the dial as
        // `ζ(δ(hint))` (paper §4.1), bit-identical to `hint.tail_loss_scale()`
        // at the three presets (δ = 0.5/ζ and ζ = 0.5/δ are exact f64
        // inverses at ζ ∈ {0.01, 1, 100}, pinned by
        // `zeta_of_the_hints_delta_is_the_hints_own_scale`), so a δ between
        // the presets (`RWM_DELTA`) moves the tail target continuously.
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

    /// Enable/disable the saturation cap (paper §4.4). Default: on.
    pub fn set_saturation_cap(&mut self, enabled: bool) {
        self.saturation_cap_enabled = enabled;
    }

    /// #46: enable/disable the window-mass burst-tail provisioning term
    /// (paper §4.3). Default follows `RWM_RSTAR_TAIL` (ON).
    /// Exposed for ablation.
    pub fn set_tail_provision(&mut self, enabled: bool) {
        self.tail_provision = enabled;
    }

    /// Enable/disable the Bulk pure-ARQ tail target (P4a, on by default).
    /// Exposed for ablation: with it off, Bulk falls back to the plain
    /// 100×-loosened `target_tail_loss`.
    ///
    /// Off means `β := 0` (paper §4.5): the flag selects no law, it zeroes
    /// the mixing weight so only the anchor term is evaluated. At
    /// Realtime/Auto (β = 0 already) it is inert.
    pub fn set_bulk_pure_arq(&mut self, enabled: bool) {
        self.bulk_pure_arq = enabled;
    }

    /// The effective mixing weight β of `r(β) = (1−β)·r_anchor + β·r_bulk`:
    /// this contract's bulkness, or 0 when the P4a ablation zeroes it.
    fn effective_bulkness(&self) -> f64 {
        if self.bulk_pure_arq { self.bulkness } else { 0.0 }
    }

    /// P6: set the completion exposure χ ∈ [0, 1] (paper §4.6)
    /// for callers that know the remaining send time T_rem — compute it
    /// with `raptorpath_math::completion_exposure(t_rem, srtt, rttvar)`.
    /// The production tunnel never calls this (endless stream ⇒ χ = 0).
    // Test-only consumer: tests/gate_suite.rs drives the χ arm through it.
    pub fn set_completion_exposure(&mut self, chi: f64) {
        self.completion_exposure = chi.clamp(0.0, 1.0);
    }

    /// P10a: set the inner-feedback weight ∈ [0, 1] (paper §4.4).
    /// 1.0 = the payload's delivery latency feeds back into its own
    /// throughput (TCP-in-tunnel) — enables the mid-stream repair floor;
    /// 0.0 (default everywhere) = pure Bulk glide. Config knob:
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
    /// r* formula (paper §4.2) for the rate:
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
    /// point r_sat (paper §4.4): past it, extra repairs hurt the
    /// tail by displacing source symbols. See `set_saturation_cap`.
    ///
    /// `window_size`: current encoder window or block size.
    pub fn compute_repair_rate(&self, estimator: &LossEstimator, window_size: usize) -> f64 {
        self.rate_snapshot(estimator, window_size).rate()
    }

    /// The inputs `compute_repair_rate` reads, snapshotted: the estimator
    /// extraction (a few µs: the BOCD quantile, the mass moments) split from
    /// the evaluation ([`RateSnapshot::rate`], ~0.1 ms with the window-mass
    /// solve), so a caller can take the snapshot under the controller and
    /// scheduler locks and evaluate after releasing them.
    /// `compute_repair_rate` is exactly `rate_snapshot(..).rate()`.
    pub fn rate_snapshot(&self, estimator: &LossEstimator, window_size: usize) -> RateSnapshot {
        // The formula itself lives in raptorpath-math::controller_rate — a
        // single shared implementation used by both this production
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
        // ── The rate mix, r(β) = (1−β)·r_anchor + β·r_late-is-fine ────────
        // Paper §4.5. One `RateInputs` is built here; `RateSnapshot::rate`
        // evaluates the shared `controller_rate` twice — once with the
        // late-is-fine δ_eff law off (the anchor term) and once with it on
        // (the Bulk term) — and blends the two by the contract's bulkness.
        // Both terms are always computed: no `hint ==` and no δ threshold
        // selects a law, and neither term is skipped at β ∈ {0, 1} (that
        // fast path would be a mode switch). The price is real: at Bulk the
        // anchor term's δ_eff is the tail target, below ε̂, so it runs the
        // full window-mass solve (~0.1 ms) — which is why the window sender
        // reads the rate on a cadence (`RepairRateCache`), not per symbol.
        let inputs = raptorpath_math::RateInputs {
            p_upper: estimator.predictive_loss_upper(0.95),
            sigma2,
            mean_burst,
            // #46 (paper §4.3): the receiver's measured window loss-mass
            // tail; MassStats::default() until enough nonzero blocks are
            // observed, so cold start provisions from the GE law alone.
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
            // P6 (paper §4.6): 0.0 unless a T_rem-aware caller set it —
            // the production tunnel is an endless stream, so mid-stream
            // semantics (δ_eff = ε̂, r* = 0) apply permanently.
            completion_exposure: self.completion_exposure,
            // P10a (paper §4.4): mid-stream repair floor for payloads
            // whose latency feeds back (TCP-in-tunnel). 0.0 default.
            inner_feedback: self.inner_feedback,
            saturation_cap: self.saturation_cap_enabled,
            max_overhead: self.max_overhead,
        };
        RateSnapshot { inputs, beta: self.effective_bulkness() }
    }

    /// Derive the encoder window size W* from the current channel estimate
    /// (paper §4.8). Returns `None` when the estimator lacks the
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
        // latency_budget = 0 => the math layer aligns W to ~1 RTT (paper §4.8).
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
/// See paper §3.3 (the taper).
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
/// emission (env `RWM_TAPER_R`, default = `RWM_UNIFIED`, ON).
///
/// Why: accruing the raw taper density τ(t) = r·q·(1−q)^t with t reset on
/// every cumulative-ack advance emits Σ_t τ(t) = r symbols per ack cycle —
/// nearly independent of r's magnitude (an ack cycle at BDP is hundreds of
/// symbols) — which leaves the r* loop, including the burst-tail term,
/// inert on the wire.
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
/// link-headroom anchor as the per-cycle accrual, and the 1.0 cap paces
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
    ///              the budget law the reset re-times, it does not size)
    /// * `taper`  — the GE taper shape (q, decay) for this estimator state
    /// * `span`   — the coding window W (shape renormalization span)
    /// * `spare`  — link spare capacity (cap anchor)
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

/// P_lost(t) (paper §3.2), the shared math crate's single definition —
/// re-exported so `control::fec_rate::p_lost` keeps its path.
pub use raptorpath_math::p_lost;

/// Burst variance inflation factor σ²_burst for the GE channel.
///
/// σ²_burst = 1 + 2(1-p-q)/(p+q)
///
/// where p = P(Good→Bad), q = P(Bad→Good) from the GE model.
/// This inflates the margin term in the r* formula to account for
/// correlated (bursty) losses. See paper §2.4.
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
/// δ-honest overload-shedding budget, paper §5.6):
///
///   1−ρ = ε · (1 − P_fec(r, ε, W, σ²_burst))
///
/// — the loss fraction the design already concedes past in-window FEC at
/// the deadline (paper §3.5: P(lost) = ε·(1−P_fec)·(1−P_arq); at small δ,
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
