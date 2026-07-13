//! Pure math functions for the raptorpath FEC/ARQ model.
//!
//! No IO, no async, no platform dependencies. Used by both the main
//! raptorpath crate and the raptorpath-wasm crate.

pub mod changepoint;
pub mod gilbert_elliott;
pub mod estimator;
pub mod fec_rate_controller;
pub mod rlc;

pub use changepoint::BayesianChangepoint;
pub use gilbert_elliott::GilbertElliottEstimator;
pub use estimator::LossEstimator;
pub use fec_rate_controller::FecRateController;
pub use rlc::{RlcEncoder, RlcDecoder};

/// Triangle mode: which variable to compute from the other two.
/// See paper Section 1.4, 8.6.
#[derive(Debug, Clone)]
pub enum TriangleMode {
    /// Fix delta (tail latency) + rho (reliability) → compute r (bandwidth).
    ComputeR { delta: f64, rho: f64 },
    /// Fix r (bandwidth) + rho (reliability) → compute delta (tail latency).
    ComputeDelta { r: f64, rho: f64 },
    /// Fix r (bandwidth) + delta (tail latency) → compute rho (reliability).
    ComputeRho { r: f64, delta: f64 },
}

/// Standard normal survival function: P(Z > z) = 1 - Phi(z).
/// Abramowitz & Stegun rational approximation.
pub fn normal_survival(z: f64) -> f64 {
    if z > 8.0 { return 0.0; }
    if z < -8.0 { return 1.0; }
    let x = z / std::f64::consts::SQRT_2;
    let ax = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * ax);
    let poly = t * (0.254829592
        + t * (-0.284496736
            + t * (1.421413741
                + t * (-1.453152027
                    + t * 1.061405429))));
    let erfc_ax = poly * (-ax * ax).exp();
    if x >= 0.0 { 0.5 * erfc_ax } else { 1.0 - 0.5 * erfc_ax }
}

/// Standard normal quantile (inverse CDF). Abramowitz & Stegun rational approximation.
pub fn normal_quantile(p: f64) -> f64 {
    if p <= 0.0 { return f64::NEG_INFINITY; }
    if p >= 1.0 { return f64::INFINITY; }
    if (p - 0.5).abs() < 1e-12 { return 0.0; }
    let (sign, q) = if p < 0.5 { (-1.0, p) } else { (1.0, 1.0 - p) };
    let t = (-2.0 * q.ln()).sqrt();
    let num = 2.515517 + 0.802853 * t + 0.010328 * t * t;
    let den = 1.0 + 1.432788 * t + 0.189269 * t * t + 0.001308 * t * t * t;
    sign * (t - num / den)
}

/// Compute P_lost(t): probability a symbol was lost given no ACK after time t.
///
/// P_lost(t) = epsilon / [epsilon + (1-epsilon) * P(RTT > t)]
///
/// See paper Section 3.4.
pub fn p_lost(age_secs: f64, epsilon: f64, srtt_secs: f64, rttvar_secs: f64) -> f64 {
    if epsilon >= 1.0 { return 1.0; }
    if epsilon <= 0.0 { return 0.0; }
    let rttvar = rttvar_secs.max(0.001);
    let z = (age_secs - srtt_secs) / rttvar;
    let p_rtt_exceeds = normal_survival(z);
    let denom = epsilon + (1.0 - epsilon) * p_rtt_exceeds;
    if denom < 1e-300 { return 1.0; }
    (epsilon / denom).clamp(0.0, 1.0)
}

/// Burst variance inflation factor sigma2_burst for the GE channel.
///
/// sigma2_burst = 1 + 2(1-p-q)/(p+q)
///
/// where p = P(Good->Bad), q = P(Bad->Good).
/// Returns 1.0 (iid) when parameters are degenerate.
/// See paper Section 8.3.
pub fn burst_variance_factor(p: f64, q: f64) -> f64 {
    // q = 0 (or p = 0) is the estimator's NO-DATA sentinel (no observed
    // Bad-state transitions on very clean channels), not a measurement of
    // infinite bursts — treating it as one makes sigma2 ~ 2/p explode and
    // over-provisions the cleanest links. No data => iid default.
    // See paper Section 8.3.
    if p <= 0.0 || q <= 0.0 { return 1.0; }
    let sum = p + q;
    if sum < 1e-10 { return 1.0; }
    let factor = 1.0 + 2.0 * (1.0 - p - q) / sum;
    factor.max(1.0)
}

/// Compute the optimal correction rate r* with sigma2_burst margin.
///
/// r* = epsilon/(1-epsilon) + z * sqrt(epsilon * sigma2 / (W * (1-epsilon)))
///
/// where z = 2.33 (99th percentile). See paper Section 8.4.
pub fn compute_r_star(epsilon: f64, sigma2: f64, window_size: f64) -> f64 {
    compute_r_star_with_z(epsilon, sigma2, window_size, 2.33)
}

/// Compute r* with a custom z quantile value.
///
/// Canonical (continuous) choice: z = normal_quantile(1 - delta/epsilon),
/// so the margin shrinks continuously as the channel improves relative to
/// the target and r* decreases to 0 (paper Section 8.4). Negative z (delta
/// close to epsilon) is allowed; the result is clamped at the physical
/// floor 0.
pub fn compute_r_star_with_z(epsilon: f64, sigma2: f64, window_size: f64, z_delta: f64) -> f64 {
    if epsilon <= 0.0 || epsilon >= 1.0 { return 0.0; }
    let base = epsilon / (1.0 - epsilon);
    let margin = if window_size > 0.0 {
        z_delta * (epsilon * sigma2 / (window_size * (1.0 - epsilon))).sqrt()
    } else {
        0.0
    };
    (base + margin).max(0.0)
}

/// Bulk completion-tail budget delta_tail (paper Sections 14.25/14.26):
/// the residual probability of one serial ARQ round at end-of-stream.
pub const BULK_TAIL_BUDGET: f64 = 0.05;

// =========================================================================
// Burst-tail provisioning (paper Section 8.4.1): r* against heavy-tailed
// burst-length distributions. The GE geometric burst law under-provisions
// r* by 2-4x on real traces (paper Section 2.5, MEASURED); the corrected
// solver provisions against the burst-length QUANTILE the (delta, rho)
// contract implies, under a discrete-Weibull burst-tail model whose k = 1
// special case IS the geometric (GE) law — so on a GE channel the
// correction reproduces the Section 8.7 exact r*, and on heavy tails it
// provisions for the fade bursts that actually kill windows.
// =========================================================================

/// Lower solver bound on the discrete-Weibull shape k (numerical range of
/// the moment fit, not a behavioral constant): k = 0.2 corresponds to a
/// second-moment ratio E[X^2]/E[X]^2 of ~252 — far beyond any measured
/// trace. Heavier demands clamp here.
pub const BURST_TAIL_K_MIN: f64 = 0.2;
/// Upper solver bound on k: k = 8 is essentially deterministic burst
/// lengths (ratio ~1.08); lighter-than-that demands clamp here.
pub const BURST_TAIL_K_MAX: f64 = 8.0;
/// Search ceiling of the tail solver (same convention as
/// `compute_r_star_exact`): a burst quantile no in-window rate can cover
/// returns this ceiling and the caller's max_overhead clamp governs.
pub const R_STAR_TAIL_CEILING: f64 = 2.0;

/// ln Gamma(x) for x > 0 (Lanczos approximation, g = 7, n = 9).
/// Absolute error < 1e-10 over the range used here (x in [1, 12]).
pub fn ln_gamma(x: f64) -> f64 {
    // Coefficients for g = 7, n = 9 (Godfrey/Lanczos).
    const COEF: [f64; 9] = [
        0.99999999999980993,
        676.5203681218851,
        -1259.1392167224028,
        771.32342877765313,
        -176.61502916214059,
        12.507343278686905,
        -0.13857109526572012,
        9.9843695780195716e-6,
        1.5056327351493116e-7,
    ];
    if x < 0.5 {
        // Reflection: Gamma(x) Gamma(1-x) = pi / sin(pi x)
        let pi = std::f64::consts::PI;
        return (pi / (pi * x).sin()).ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut a = COEF[0];
    let t = x + 7.5;
    for (i, &c) in COEF.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

/// Discrete-Weibull burst-length survival: S(t) = P(B > t) = theta^(t^k).
///
/// k = 1 is exactly geometric with persistence theta = 1 - q (the GE burst
/// law, Section 2.3); k < 1 is a stretched-exponential heavy tail. t is
/// real-valued (the survival extends continuously between integers).
pub fn burst_tail_survival(theta: f64, k: f64, t: f64) -> f64 {
    if t <= 0.0 { return 1.0; }
    if theta <= 0.0 { return 0.0; }
    if theta >= 1.0 { return 1.0; }
    (t.powf(k) * theta.ln()).exp()
}

/// Fit the discrete-Weibull burst-tail (theta, k) from the first two
/// moments of the observed burst lengths: m1 = E[B], m2 = E[B^2].
///
/// Both moments are online-estimable from the receiver's own loss runs
/// with the SAME decayed-counter pattern as the GE transition counts —
/// no new parameter enters the (delta, rho, r) contract.
///
/// Method (paper Section 8.4.1): midpoint-correct the discrete variable
/// (X = B - 1/2 on (0, inf)), then moment-match the continuous Weibull
/// exp(-c x^k):
///
///   ratio = E[X^2]/E[X]^2 = Gamma(1+2/k) / Gamma(1+1/k)^2   (solve k)
///   c     = (Gamma(1+1/k) / E[X])^k,   theta = exp(-c)
///
/// The ratio is strictly decreasing in k, so a binary search suffices.
/// Geometric bursts (GE) have ratio ~2 and fit k ~ 1; heavy-tailed real
/// traces fit k < 1. Degenerate inputs (m2 <= m1, m1 < 1) fall back to
/// the geometric fit k = 1, theta = 1 - 1/m1.
pub fn fit_burst_tail(m1: f64, m2: f64) -> (f64, f64) {
    let m1v = if m1.is_finite() { m1.max(1.0) } else { 1.0 };
    let geometric = ((1.0 - 1.0 / m1v).clamp(1e-12, 1.0 - 1e-12), 1.0);
    if !(m2.is_finite()) || m2 <= m1v || m1v <= 1.0 {
        return geometric;
    }
    // Midpoint shift: B in {1,2,...} <-> X = B - 1/2 in (0, inf).
    let mu = m1v - 0.5;
    let x2 = m2 - m1v + 0.25; // E[(B-1/2)^2]
    if x2 <= mu * mu {
        return geometric; // sub-deterministic: no tail to fit
    }
    let ratio = x2 / (mu * mu);
    let ratio_of_k =
        |k: f64| (ln_gamma(1.0 + 2.0 / k) - 2.0 * ln_gamma(1.0 + 1.0 / k)).exp();
    // ratio(k) is strictly decreasing; clamp into the solvable band.
    let ratio = ratio.clamp(ratio_of_k(BURST_TAIL_K_MAX), ratio_of_k(BURST_TAIL_K_MIN));
    let (mut lo, mut hi) = (BURST_TAIL_K_MIN, BURST_TAIL_K_MAX);
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if ratio_of_k(mid) > ratio {
            lo = mid; // fitted tail heavier than mid => k below mid
        } else {
            hi = mid;
        }
    }
    let k = 0.5 * (lo + hi);
    let c = ((ln_gamma(1.0 + 1.0 / k).exp()) / mu).powf(k);
    let theta = (-c).exp().clamp(1e-300, 1.0 - 1e-12);
    (theta, k)
}

/// Number of block scales the window-mass estimator tracks: the tail of
/// the sliding m-block loss mass is measured for m = 1..=MASS_SCALES.
/// With the default block scale w0 = 64 this covers window spans up to
/// 8 x 64 = 512 wire slots — the derive_window WINDOW_MAX.
pub const MASS_SCALES: usize = 8;

/// Multi-scale window loss-mass statistics (paper Section 8.4.1): for
/// each m in 1..=MASS_SCALES, the tail of J_m = losses in m consecutive
/// blocks of `block_scale` wire slots (sliding at block granularity),
/// measured by the receiver with the same decayed counters as the GE
/// transition counts:
///
///   nz[m-1] = P(J_m >= 1)         fraction of m-spans with any loss
///   m1[m-1] = E[J_m | J_m >= 1]   conditional mean mass
///   m2[m-1] = E[J_m^2 | J_m >= 1] conditional second moment
///
/// Scales with no valid data carry nz = 0 (the solver skips them).
#[derive(Debug, Clone, Copy, Default)]
pub struct MassStats {
    /// Block scale w0 in wire slots (0.0 = no mass data).
    pub block_scale: f64,
    pub nz: [f64; MASS_SCALES],
    pub m1: [f64; MASS_SCALES],
    pub m2: [f64; MASS_SCALES],
}

impl MassStats {
    /// True when the statistic is usable (block scale known and the
    /// single-block scale has observed loss mass).
    pub fn is_valid(&self) -> bool {
        self.block_scale >= 1.0
            && self.nz[0] > 0.0
            && self.nz[0].is_finite()
            && self.m1[0] > 0.0
    }

    /// The mean loss level the mass statistics embody:
    /// eps_mass = P(K_1 >= 1) x E[K_1 | K_1 >= 1] / w0. Used for the
    /// level rescale in `r_star_mass` (see there). 0.0 when invalid.
    pub fn eps_mass(&self) -> f64 {
        if !self.is_valid() {
            return 0.0;
        }
        (self.nz[0] * self.m1[0] / self.block_scale).clamp(0.0, 1.0)
    }

    /// Fitted tail of J_m: P(J_m > x) ~ nz_m x S(x; theta_m, k_m) with
    /// the discrete-Weibull tail fit from the conditional moments.
    fn tail(&self, idx: usize, x: f64) -> f64 {
        let nz = self.nz[idx].clamp(0.0, 1.0);
        if nz <= 0.0 {
            return 0.0; // no data at this scale: skip (documented)
        }
        let (theta, k) = fit_burst_tail(self.m1[idx], self.m2[idx]);
        nz * burst_tail_survival(theta, k, x)
    }
}

/// Window-mass quantile r* (paper Section 8.4.1): the least rate whose
/// repair count covers the measured window LOSS-MASS quantile the tail
/// target implies.
///
/// THE EXACT FAILURE CRITERION. A coding window of W source symbols and
/// R = rW repairs spans N = W + R wire slots. If K of those N slots are
/// lost — x of them repairs — the window fails iff source losses exceed
/// surviving repairs: K - x > R - x, i.e. **K > R, independent of x**.
/// So the window-failure probability is EXACTLY the upper tail of the
/// window loss mass K_N, and the right statistic to provision against is
/// the receiver's own measured window-mass tail — not the single-burst
/// length law (a window is killed just as dead by two clustered 20-loss
/// bursts as by one 40-loss burst, and real traces cluster far beyond
/// GE: the Section 2.5 "long memory" miss).
///
/// LAW. P(K_N > R) is read from the measured multi-scale mass tails at
/// the scale matching N: with x = N / w0 in [1, MASS_SCALES],
///
///   F(r) = (1-f) T_lo(R) + f T_hi(R),   f = frac(x)
///
/// interpolating the fitted tails T_m(R) = P(J_m > R) of the two block
/// scales bracketing x. Linear-in-probability interpolation dominates
/// the log-linear reading (AM-GM), i.e. it takes the conservative side
/// of the interpolation uncertainty; by stationarity T_ceil(x) is an
/// upper bound on the true window tail, so F is sandwiched between the
/// tight and the safe reading. Beyond the largest tracked scale the
/// window is chunked and a union bound applies:
/// F = c x T_max(R / c), c = ceil(x / MASS_SCALES). r* is the least r
/// on [0, R_STAR_TAIL_CEILING] with F(r) <= delta_wf (= delta/eps, the
/// same per-window mapping as Section 8.4).
///
/// Properties:
/// - Measured mass tail at the window's own scale: heavy burst tails,
///   burst CLUSTERING, and loss/repair correlation are all inside K, and
///   there is no union-bound slack at the scales that matter — on a GE
///   channel r* tracks the Section 8.7 exact r* (validated: no material
///   over-provisioning where GE is adequate).
/// - Continuous in delta_wf and in the measured moments; 0 when the
///   measured tail already meets the target at r = 0 (pure ARQ), and
///   R_STAR_TAIL_CEILING when even the ceiling cannot (the contract is
///   infeasible in-window; the caller's max_overhead clamp governs and
///   the miss is declared, not hidden).
/// - This is an ESTIMATE of the window-failure tail (block-aligned
///   measurement + Weibull-fit tail extension), validated against replay
///   in rstar_tail_validation.rs — not a distribution-free bound.
///
/// LEVEL RESCALING (`level_scale`). The mass moments carry a LONG memory
/// (they estimate rare tails), so after a loss-REGIME change they lag the
/// fast BOCD level estimate. The caller passes
/// level_scale = eps_now / eps_mass, where eps_mass = nz_1 x m1_1 / w0 is
/// the mean loss level the mass statistics themselves embody; the tail is
/// then read as P(K > R) = T(R / level_scale) — the level-equivariant
/// family assumption (a regime LEVEL shift rescales the whole mass
/// distribution; the SHAPE keeps its long memory). With eps_now the BOCD
/// posterior upper quantile this also inherits the architecture's
/// estimation-uncertainty margin. level_scale -> 0 (channel now clean)
/// sends the term to 0 continuously; 1.0 = no rescale (stationary case,
/// e.g. the oracle-side full-trace fits).
pub fn r_star_mass(stats: &MassStats, window: f64, delta_wf: f64, level_scale: f64) -> f64 {
    let valid = stats.is_valid()
        && window.is_finite()
        && window >= 1.0
        && delta_wf.is_finite()
        && delta_wf > 0.0
        && level_scale.is_finite()
        && level_scale > 0.0;
    if !valid {
        return 0.0;
    }
    if delta_wf >= 1.0 {
        return 0.0; // contract met by pure ARQ: no within-window requirement
    }
    let w0 = stats.block_scale;
    let bound = |r: f64| -> f64 {
        let n = window * (1.0 + r);
        let x = n / w0;
        let rr = r * window / level_scale; // repair count R, level-rescaled
        if x <= 1.0 {
            return stats.tail(0, rr);
        }
        let max_m = MASS_SCALES as f64;
        if x >= max_m {
            // chunk + union bound beyond the tracked scales
            let c = (x / max_m).ceil();
            return (c * stats.tail(MASS_SCALES - 1, rr / c)).min(1.0);
        }
        let lo = x.floor() as usize - 1;
        let hi = (lo + 1).min(MASS_SCALES - 1);
        let f = x - x.floor();
        let (t_lo, t_hi) = (stats.tail(lo, rr), stats.tail(hi, rr));
        (1.0 - f) * t_lo + f * t_hi
    };
    if bound(0.0) <= delta_wf {
        return 0.0;
    }
    if bound(R_STAR_TAIL_CEILING) > delta_wf {
        return R_STAR_TAIL_CEILING; // infeasible in-window at the ceiling
    }
    // The bound is not perfectly monotone in r everywhere (the span grows
    // with r before the survival decay bites), so find the LAST downward
    // crossing on a grid (same 0.005 resolution as `r_saturation`), then
    // bisect inside the bracket for continuity.
    let step = 0.005;
    let n_steps = (R_STAR_TAIL_CEILING / step) as usize;
    let mut hi_idx = n_steps;
    for i in (0..n_steps).rev() {
        if bound(i as f64 * step) > delta_wf {
            hi_idx = i + 1;
            break;
        }
        hi_idx = i;
    }
    if hi_idx == 0 {
        return 0.0;
    }
    let (mut lo, mut hi) = ((hi_idx - 1) as f64 * step, hi_idx as f64 * step);
    for _ in 0..30 {
        let mid = 0.5 * (lo + hi);
        if bound(mid) > delta_wf {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}

/// Completion-exposure kernel chi(T_rem) (paper Section 14.26).
///
/// chi is the probability that a loss suffered NOW can no longer hide
/// behind ongoing sends: its ~1.5 x SRTT ARQ recovery would outlive the
/// remaining send time T_rem and become serial completion time. Reuses
/// the Section 3.4/5.4 P_lost machinery (normal RTT tail):
///
///   chi(T_rem) = Phi_bar((T_rem - 1.5 x SRTT) / sigma_arq)
///   sigma_arq  = max(4 x RTTVAR, SRTT / 4)     (floor avoids div-by-0)
///
/// Mid-stream (T_rem >> SRTT) chi = 0; over the final ~1.5 SRTT it rises
/// smoothly to 1. Unknown T_rem (endless tunnel stream) must be passed
/// as infinity (or the caller keeps chi = 0): both yield 0 — pure ARQ.
pub fn completion_exposure(t_rem_secs: f64, srtt_secs: f64, rttvar_secs: f64) -> f64 {
    if !(srtt_secs > 0.0) || t_rem_secs.is_nan() || t_rem_secs == f64::INFINITY {
        return 0.0;
    }
    let t_rem = t_rem_secs.max(0.0);
    let sigma_arq = (4.0 * rttvar_secs.max(0.0)).max(srtt_secs / 4.0);
    normal_survival((t_rem - 1.5 * srtt_secs) / sigma_arq)
}

/// Mid-stream repair floor for inner-feedback flows (paper Section 14.28).
///
/// Bulk's completion-exposure glide (Section 14.26) sets r* = 0 mid-stream
/// on the grounds that ARQ recovery runs in parallel with ongoing sends and
/// costs no completion time. That argument prices VOLUME, not delivery
/// latency: when the payload is itself a latency-sensitive control loop
/// (TCP inside the tunnel), every unrepaired loss stalls the inner flow's
/// in-order delivery for one outer ARQ round, and — IF the stall exceeds
/// the inner loss-detection tolerance — the inner congestion controller
/// feeds it back into its send rate. The P9b analysis attributed the
/// residual L1 C2 gap to ~20 such events per 1.8 MB transfer.
///
/// HONEST STATUS (paper 14.28, L1 verification): the premise was REFUTED
/// post-14.27 — with block-mode ARQ (~1.5 RTT recovery) and in-order
/// delivery in place, the inner TCP absorbs the residual stalls (its RTO
/// floor is 200 ms >> the 20-60 ms stalls). The floor, measured ACTIVE
/// (+2.2% FEC volume at C2), was completion-neutral at C2 and 28%
/// REGRESSIVE at C3 (floor repairs displace source symbols in the same
/// inner-limited closed loop). Production therefore defaults the weight
/// to 0; the derivation and mechanism are kept for payloads whose inner
/// loop is genuinely stall-brittle — measure before enabling.
///
/// Derivation (Section 14.28). Per unrepaired loss event the inner flow
/// stalls for
///
///   L_stall = min(1.5 x SRTT_outer, RTO_inner),
///            RTO_inner >= max(RTO_MIN = 200 ms, SRTT_inner)
///
/// Loss events (GE burst onsets) arrive at rate eps x q_hat per wire slot
/// (q_hat = 1/mean_burst), and a proactive repair stream at rate r repairs
/// an m-loss event within the stall horizon T_arq = L_stall / t_sym slots
/// with probability C(r) = p_fec_recovery_marginalized(T_arq, r, q_hat,
/// eps) (the Section 14.14 race, run against the ARQ horizon). The
/// expected fraction of wall time the inner flow spends stalled is
///
///   S(r) = eps x q_hat x T_arq x (1 - C(r))
///
/// and the floor is the smallest r whose residual stall is at or below the
/// delivery-jitter scale the inner flow already absorbs:
///
///   S(r_min) <= theta,   theta = sigma_j / L_stall,   sigma_j = SRTT/4
///
/// (sigma_j is the Section 14.26 sigma_arq evaluated at its SRTT/4 floor —
/// the sender cannot observe the inner flow's tolerance, and the outer
/// RTTVAR estimate is a heuristic in production, so the floor uses the
/// deterministic branch). Everything is continuous: S is continuous and
/// nonincreasing in r, so r_min is continuous in every input, and when
/// S(0) <= theta already (clean channel, short stall horizon) the floor is
/// 0 with no cutoff. Unknown t_sym (no throughput estimate) disables the
/// floor — same sentinel convention as the burst B/T term.
///
/// Returns r_min in [0, r_cap]; the caller weights it by the
/// inner-feedback weight and takes max() against the base rate.
pub fn inner_feedback_floor(p: f64, mean_burst: f64, srtt: f64, t_sym: f64, r_cap: f64) -> f64 {
    let valid = p.is_finite()
        && p > 0.0
        && p < 1.0
        && srtt.is_finite()
        && srtt > 0.0
        && t_sym.is_finite()
        && t_sym > 0.0
        && r_cap.is_finite()
        && r_cap > 0.0;
    if !valid {
        return 0.0;
    }
    // GE burst-onset probability per slot is ~eps * q_hat (p_GB weighted by
    // time in Good); mean_burst = 1 (no GE data) degenerates to iid.
    let q_hat = (1.0 / mean_burst.max(1.0)).clamp(0.01, 1.0);
    // Inner stall per unrepaired event: one outer ARQ round, clamped at a
    // conservative lower bound for the inner RTO (Linux RTO_MIN = 200 ms;
    // RTO_inner >= SRTT_inner >= SRTT_outer).
    const TCP_RTO_MIN: f64 = 0.2;
    let l_stall = (1.5 * srtt).min(TCP_RTO_MIN.max(srtt));
    let t_arq = l_stall / t_sym; // stall horizon in wire slots
    let theta = (srtt / 4.0) / l_stall; // jitter-scale tolerance (dimensionless)
    // Residual stall fraction: events/slot x P(unrepaired) x stall slots.
    let stall = |r: f64| p * q_hat * t_arq * (1.0 - p_fec_recovery_marginalized(t_arq, r, q_hat, p));
    if stall(0.0) <= theta {
        return 0.0; // pure ARQ already within jitter noise — floor vanishes
    }
    if stall(r_cap) > theta {
        return r_cap; // even the cap cannot meet it; caller's clamp governs
    }
    let mut lo = 0.0_f64;
    let mut hi = r_cap;
    for _ in 0..50 {
        let mid = 0.5 * (lo + hi);
        if stall(mid) > theta {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}

/// Inputs to the shared production rate controller (see `controller_rate`).
/// All values are estimator-known; unknowns use their documented sentinels.
#[derive(Debug, Clone, Copy)]
pub struct RateInputs {
    /// Conservative loss estimate (BOCD posterior upper quantile at 95%).
    pub p_upper: f64,
    /// Burst variance factor sigma2_burst (1.0 when no GE data).
    pub sigma2: f64,
    /// GE mean burst length (1.0 when unknown).
    pub mean_burst: f64,
    /// Measured multi-scale window loss-mass statistics (paper Section
    /// 8.4.1). `MassStats::default()` (no data) leaves the tail term
    /// inert and the controller behaves exactly as pre-#46.
    pub mass: MassStats,
    /// Enable the window-mass quantile provisioning term (paper Section
    /// 8.4.1; production gates this on RWM_RSTAR_TAIL, default ON).
    pub tail_provision: bool,
    /// Encoder window W.
    pub window: f64,
    /// Symbols per RTT = rtt x throughput / symbol_size (0.0 = unknown;
    /// disables the burst B/T term).
    pub t_symbols: f64,
    /// Smoothed RTT in seconds (used by the saturation model).
    pub srtt: f64,
    /// Symbol serialization time = symbol_size / throughput in seconds
    /// (0.0 = unknown; disables the saturation cap).
    pub t_sym: f64,
    /// Raw codec decode overhead (weighted by P(decoder invoked) inside).
    pub codec_overhead: f64,
    /// Hint-adjusted tail-latency target delta.
    pub tail_target: f64,
    /// Bulk "late is fine" (P4a/P6, paper 14.26): the effective target
    /// becomes the completion-exposure glide
    /// delta_eff = p + (BULK_TAIL_BUDGET - p) x chi, so mid-stream
    /// (chi = 0) delta_eff = p and r* = 0 IDENTICALLY — independent of
    /// estimator uncertainty — and near completion (chi -> 1) delta_eff
    /// glides to the 14.25 tail budget.
    pub bulk_late_is_fine: bool,
    /// Completion exposure chi in [0, 1] (paper 14.26, see
    /// `completion_exposure`). 0 = mid-stream or T_rem unknown (endless
    /// tunnel stream); 1 = final ~1.5 SRTT of the transfer. Only read
    /// when `bulk_late_is_fine`.
    pub completion_exposure: f64,
    /// Inner-feedback weight in [0, 1] (paper 14.28): how strongly the
    /// payload's delivery latency feeds back into its own throughput.
    /// 1 = tunnel carrying TCP (an unrepaired loss stalls the inner
    /// control loop ~one ARQ round); 0 = file-transfer semantics (the L0
    /// gate, the wasm sim, true bulk objects), where mid-stream ARQ
    /// recovery is genuinely free and the old behavior is preserved
    /// exactly. The weight scales the `inner_feedback_floor` repair floor,
    /// so the rate is continuous in it.
    pub inner_feedback: f64,
    /// Cap the rate at the p99(r) saturation point (paper 14.21).
    pub saturation_cap: bool,
    /// Hard overhead ceiling.
    pub max_overhead: f64,
}

/// The production FEC rate formula (paper Sections 8.4, 9.2, 14.21; ADR-0050
/// architecture with the continuous z_{delta/eps} margin). This is the SINGLE
/// implementation shared by the production controller and the visualizer --
/// they cannot drift.
///
///   delta_eff = p + (0.05 - p) x chi             if bulk_late_is_fine
///                                                (paper 14.26: chi = 0
///                                                mid-stream -> r* = 0;
///                                                chi -> 1 -> tail budget)
///             = tail_target                      otherwise
///   z         = normal_quantile(1 - delta_eff/p)
///   random    = max(0, p/(1-p) + z*sqrt(p*sigma2/(W(1-p)))) [+ codec_eff]
///   burst     = (B / t_symbols) x (1 - delta_eff/p)+
///   rate      = clamp( soft_saturate(max(random, burst), r_sat), 0, max_overhead )
///               (soft_saturate = kink-free approach to r_sat, Section 14.21.1)
pub fn controller_rate(inp: &RateInputs) -> f64 {
    let p = inp.p_upper;
    if p < 1e-10 {
        return 0.0;
    }

    let delta_eff = if inp.bulk_late_is_fine {
        // Completion-exposure glide (paper 14.26): a convex combination of
        // "late is fine" (delta = p, exactly the channel: pure ARQ) and the
        // 14.25 completion-tail budget. chi = NaN is treated as unknown.
        let chi = if inp.completion_exposure.is_nan() {
            0.0
        } else {
            inp.completion_exposure.clamp(0.0, 1.0)
        };
        p + (BULK_TAIL_BUDGET - p) * chi
    } else {
        inp.tail_target
    };

    // Codec overhead weighted by P(decoder invoked) for systematic codecs.
    let effective_codec_overhead = if inp.codec_overhead > 0.0 && inp.window > 0.0 {
        let p_decoder_invoked = 1.0 - (1.0 - p).powi(inp.window as i32);
        inp.codec_overhead * p_decoder_invoked
    } else {
        0.0
    };

    // Continuous tail margin (paper Section 8.4).
    let z = z_for_tail_target(delta_eff, p);
    let core = compute_r_star_with_z(p, inp.sigma2, inp.window, z);
    let random_rate = if core > 0.0 {
        core + effective_codec_overhead
    } else {
        0.0
    };

    // Burst term B/T, scaled by the required FEC fraction (continuous).
    let required_fec_fraction = (1.0 - delta_eff / p).clamp(0.0, 1.0);
    let burst_rate = if inp.t_symbols > 0.0 {
        (inp.mean_burst.max(1.0) / inp.t_symbols) * required_fec_fraction
    } else {
        0.0
    };

    // Window-mass quantile term (paper Section 8.4.1): provisions the
    // repair count that covers the measured window loss-mass quantile the
    // tail target implies (heavy burst tails AND burst clustering — the
    // structures the GE margin misses on real traces, Section 2.5). Inert
    // (0) without measured mass data, so cold start behaves exactly as
    // pre-#46; delta_wf >= 1 (the Bulk chi = 0 identity delta_eff = p)
    // returns 0 inside r_star_mass, keeping r*(delta = p) = 0.
    let tail_rate = if inp.tail_provision && inp.mass.is_valid() {
        // Level rescale (see r_star_mass): read the long-memory mass tail
        // at the CURRENT BOCD loss level, so the term follows regime
        // changes at estimator speed (level-equivariant tail family).
        let eps_mass = inp.mass.eps_mass();
        if eps_mass > 0.0 {
            r_star_mass(&inp.mass, inp.window, delta_eff / p, p / eps_mass)
        } else {
            0.0
        }
    } else {
        0.0
    };

    let mut rate = random_rate.max(burst_rate).max(tail_rate);

    // Inner-feedback repair floor (paper Section 14.28): for payloads whose
    // delivery latency feeds back into their own throughput (TCP inside the
    // tunnel), the Bulk glide's mid-stream r* = 0 leaves every loss to stall
    // the INNER flow one ARQ round. The floor is the smallest r whose
    // residual stall fraction sits within delivery-jitter noise, weighted
    // by inner_feedback (0 = old behavior, exactly). Applied before the
    // saturation cap: 14.21's "more FEC hurts the tail" still overrides.
    let w = if inp.inner_feedback.is_nan() {
        0.0
    } else {
        inp.inner_feedback.clamp(0.0, 1.0)
    };
    if w > 0.0 {
        let floor = inner_feedback_floor(p, inp.mean_burst, inp.srtt, inp.t_sym, inp.max_overhead);
        rate = rate.max(w * floor);
    }

    // Saturation cap (paper Section 14.21): past r_sat more FEC hurts p99.
    // Soft, kink-free (Section 14.21.1): the rate eases ASYMPTOTICALLY toward
    // r_sat instead of hitting a wall, modeling the p99 curve's smooth
    // interior minimum. `soft_saturate` <= min(rate, r_sat) always, so the
    // cap never adds FEC, and -> the old hard min as SAT_SOFTNESS -> 0.
    if inp.saturation_cap && inp.t_sym > 0.0 && inp.srtt > 0.0 {
        rate = soft_saturate(rate, r_saturation(p, inp.sigma2, inp.window, inp.srtt, inp.t_sym));
    }

    rate.clamp(0.0, inp.max_overhead)
}

/// Continuous z for the r* margin: normal_quantile(1 - delta/epsilon).
///
/// Returns NEG_INFINITY when delta >= epsilon — the exact value of
/// Phi^-1(0): the tail target is met by pure ARQ, so r* evaluates to 0
/// through the max(0, ..) floor REGARDLESS of the size of the IT term
/// epsilon/(1-epsilon). (A finite clamp here breaks the Bulk chi = 0
/// identity r*(delta = p) = 0 at cold-start p close to 1, where no
/// finite z can cancel the IT term — paper 14.26.) Returns a large
/// positive value when delta << epsilon. See paper Section 8.4.
pub fn z_for_tail_target(delta: f64, epsilon: f64) -> f64 {
    if epsilon <= 0.0 { return f64::NEG_INFINITY; }
    let ratio = delta / epsilon;
    if ratio >= 1.0 { return f64::NEG_INFINITY; }
    normal_quantile(1.0 - ratio.max(1e-15))
}

/// Taper function: time-decaying correction density tau(t) = A * (1-q)^t.
///
/// See paper Section 4.
#[derive(Debug, Clone)]
pub struct TaperFunction {
    pub amplitude: f64,
    pub decay: f64,
    pub total_rate: f64,
    pub q: f64,
}

impl TaperFunction {
    /// Create from correction rate and GE parameter q.
    pub fn new(rate: f64, q: f64) -> Self {
        let q = q.clamp(0.01, 1.0);
        Self {
            amplitude: rate * q,
            decay: 1.0 - q,
            total_rate: rate,
            q,
        }
    }

    /// Correction density at time offset t.
    pub fn density(&self, t: f64) -> f64 {
        self.amplitude * self.decay.powf(t)
    }

    /// Probabilistic generation decision.
    pub fn should_generate(&self, t: f64, rng_value: f64) -> bool {
        let d = self.density(t);
        if d >= 1.0 { true } else { rng_value < d }
    }
}

/// P_fec using normal approximation (paper Section 8.1).
///
/// P_fec = Phi(sqrt(W) * (r(1-epsilon)-epsilon) / sqrt(epsilon*(1-epsilon)*(r+sigma2)))
pub fn p_fec_normal(r: f64, epsilon: f64, window_size: f64, sigma2_burst: f64) -> f64 {
    if window_size <= 0.0 || epsilon <= 0.0 || epsilon >= 1.0 || r <= 0.0 { return 0.0; }
    let numerator = r * (1.0 - epsilon) - epsilon;
    if numerator <= 0.0 { return 0.0; }
    let denominator = (epsilon * (1.0 - epsilon) * (r + sigma2_burst)).sqrt();
    if denominator < 1e-300 { return 1.0; }
    let z = window_size.sqrt() * numerator / denominator;
    1.0 - normal_survival(z)
}

/// Saturation point r_sat of the p99(r) tail model (paper Section 14.21).
///
/// The tail latency has decreasing and increasing components in r:
///
///   tail_fec(r) = (1 - P_fec(r)) x L_arq,       L_arq = 1.5 x SRTT
///                 (FEC-miss cost: the Section 8.2 normal P_fec; misses
///                 fall through to ARQ)                       [decreasing]
///   tail_rec(r) = B x t_sym x (1+r) / (r x (1-eps)), B = (sigma2+1)/2
///                 (wait for B surviving repairs: repairs occupy an
///                 r/(1+r) share of wire slots)               [decreasing]
///   tail_svc(r) = c x (1+r) x W x t_sym,        c = 0.5
///                 (dilution cost: corrections stretch the recovery
///                 window traversal at the diluted source rate) [increasing]
///
/// p99_model(r) = tail_fec + tail_rec + tail_svc has an interior minimum
/// r_sat; past it, more FEC HURTS the tail. The controller should emit
/// min(r_hint, r_sat). All inputs are estimator-known. The model is rough:
/// c is a constant and queueing is ignored beyond linear dilution — see
/// the paper for caveats.
///
/// Returns the argmin over r in [0.01, 1.0] (step 0.005). Degenerate
/// inputs return 1.0 (no cap).
pub fn r_saturation(epsilon: f64, sigma2: f64, window: f64, srtt: f64, t_sym: f64) -> f64 {
    let valid = epsilon.is_finite() && epsilon > 0.0 && epsilon < 1.0
        && sigma2.is_finite() && sigma2 > 0.0
        && window.is_finite() && window > 0.0
        && srtt.is_finite() && srtt > 0.0
        && t_sym.is_finite() && t_sym > 0.0;
    if !valid {
        return 1.0; // no cap
    }
    let l_arq = 1.5 * srtt;
    // Mean burst length implied by the GE variance factor (sigma2 = 2B - 1
    // when p << q, Section 8.3), so B is recoverable from estimator state.
    let b_hat = (sigma2 + 1.0) / 2.0;
    const C_DILUTION: f64 = 0.5;
    let mut best_r = 1.0;
    let mut best_cost = f64::INFINITY;
    for i in 0..=198u32 {
        let r = 0.01 + 0.005 * i as f64;
        let tail_fec = (1.0 - p_fec_normal(r, epsilon, window, sigma2)) * l_arq;
        let tail_rec = b_hat * t_sym * (1.0 + r) / (r * (1.0 - epsilon));
        let tail_svc = C_DILUTION * (1.0 + r) * window * t_sym;
        let cost = tail_fec + tail_rec + tail_svc;
        if cost < best_cost {
            best_cost = cost;
            best_r = r;
        }
    }
    best_r
}

/// Lower bound on the derived encoder window (`derive_window`).
pub const WINDOW_MIN: f64 = 16.0;
/// Upper bound on the derived encoder window (`derive_window`).
pub const WINDOW_MAX: f64 = 512.0;
/// Overhead knee fraction alpha (`derive_window`): the window is sized so the
/// residual variance margin of r* (Section 8.4) is within alpha of the
/// information-theoretic floor eps/(1-eps). Smaller alpha demands a larger
/// window (the margin must be pushed further below the floor).
pub const WINDOW_KNEE_ALPHA: f64 = 0.25;

/// Derive the encoder window size W* (paper Section 8.8).
///
/// W enters the model through THREE opposing channels, each giving a bound:
///
///   1. Overhead knee (upper target, favours large W). The r* margin
///      (Section 8.4) is `z * sqrt(eps*sigma2 / (W*(1-eps)))`, decaying as
///      W^-1/2 with slope d(r*)/dW ~ W^-3/2 — diminishing returns. Sizing
///      the window so the residual margin sits within a fraction alpha of the
///      IT floor eps/(1-eps) gives a closed form:
///
///        margin(W) <= alpha * eps/(1-eps)
///        => W_over = z^2 * sigma2 * (1-eps) / (eps * alpha^2)
///
///      z = z_for_tail_target(delta, eps). When delta >= eps there is no
///      margin to amortise (z <= 0) and W_over = 0: nothing pulls the window
///      up, so the burst floor / latency ceiling decide (this is the Bulk
///      regime — smallest window that still catches bursts).
///
///   2. Latency ceiling (upper bound, favours small W). A window loss waits
///      for a covering repair within the window horizon, traversed at the
///      source rate: recovery latency ~ W / send_rate = W * t_sym. Keeping it
///      within the latency budget (the Realtime hint's budget, else ~1 RTT so
///      FEC and ARQ horizons align, Section 14.5) bounds
///
///        W_lat = budget * send_rate.
///
///      With no throughput/RTT sample the ceiling is disabled (WINDOW_MAX).
///
///   3. Burst floor (lower bound, favours large-enough W). Ambient FEC at
///      r = eps/(1-eps) must accumulate B surviving repairs to absorb a mean
///      burst (Section 14.5): W_bur = B / (eps*(1-eps)), B = (sigma2+1)/2.
///      The floor never overrides the latency ceiling — if a burst cannot be
///      absorbed within budget, no window size fixes it and latency wins.
///
/// Combined: W* = clamp( W_over, min(W_bur, W_lat), W_lat ), then clamped to
/// [WINDOW_MIN, WINDOW_MAX]. Continuous (piecewise-linear via min/max/clamp)
/// and finite in every input by construction. The three regimes read out as:
/// loose delta -> burst-floor-bound (small W); tight delta -> latency-bound
/// (W rises to the ceiling); moderate delta/eps -> overhead-knee-bound.
///
/// Inputs: `delta` reliability tail target; `eps` loss rate; `sigma2` burst
/// variance factor (Section 8.3); `srtt` smoothed RTT (s); `send_rate` source
/// symbols per second; `latency_budget` seconds (<= 0 => fall back to srtt).
pub fn derive_window(
    delta: f64,
    eps: f64,
    sigma2: f64,
    srtt: f64,
    send_rate: f64,
    latency_budget: f64,
) -> f64 {
    let sigma2 = if sigma2.is_finite() && sigma2 >= 1.0 { sigma2 } else { 1.0 };

    // Latency budget: explicit Realtime budget, else ~1 RTT (Section 14.5).
    let budget = if latency_budget.is_finite() && latency_budget > 0.0 {
        latency_budget
    } else if srtt.is_finite() && srtt > 0.0 {
        srtt
    } else {
        0.0
    };
    let w_lat = if send_rate.is_finite() && send_rate > 0.0 && budget > 0.0 {
        budget * send_rate
    } else {
        WINDOW_MAX // no throughput/RTT sample => no latency ceiling
    };

    // Overhead knee and burst floor need a valid loss rate.
    let (w_over, w_bur) = if eps.is_finite() && eps > 0.0 && eps < 1.0 {
        let z = z_for_tail_target(delta, eps);
        let w_over = if z.is_finite() && z > 0.0 {
            z * z * sigma2 * (1.0 - eps) / (eps * WINDOW_KNEE_ALPHA * WINDOW_KNEE_ALPHA)
        } else {
            0.0 // no margin pressure (delta >= eps): let floor/ceiling decide
        };
        let b_hat = (sigma2 + 1.0) / 2.0;
        let w_bur = b_hat / (eps * (1.0 - eps));
        (w_over, w_bur)
    } else {
        (0.0, 0.0)
    };

    // Burst floor, but never above the latency ceiling (latency is the hard
    // constraint). clamp() needs lo <= hi, which min(., w_lat) guarantees.
    let lo = w_bur.min(w_lat);
    let w = w_over.clamp(lo, w_lat);
    w.clamp(WINDOW_MIN, WINDOW_MAX)
}

/// Soft-saturation smoothing scale as a fraction of r_sat (paper 14.21.1).
///
/// The hard cap `min(r, r_sat)` has a kink at r = r_sat. The soft cap
/// approaches r_sat asymptotically over a rate band of width ~SAT_SOFTNESS x
/// r_sat. Deliberately NARROW: the p99 model is trusted for the LOCATION of
/// r_sat, not the DEPTH of the penalty past it (Section 14.21 caveat: the
/// real cost is a queueing knee much steeper than the model's shallow
/// quadratic). A curvature-derived width (s = C'_svc/C'' at r_sat) would
/// track the model's OWN shallow curvature and over-soften the cap, letting
/// FEC drift into the measured +115 ms regime. A small fixed fraction keeps
/// the soft cap faithful to the gate-validated hard min while removing the
/// discontinuity. As SAT_SOFTNESS -> 0 the hard min is recovered exactly.
pub const SAT_SOFTNESS: f64 = 0.1;

/// Numerically stable softplus: ln(1 + e^x) = max(x,0) + ln1p(e^-|x|).
fn softplus(x: f64) -> f64 {
    x.max(0.0) + (-x.abs()).exp().ln_1p()
}

/// Soft saturation cap (paper Section 14.21.1): a continuous, kink-free
/// replacement for `min(rate, r_sat)`.
///
///   r_eff = r_sat - s x softplus((r_sat - rate) / s),   s = SAT_SOFTNESS x r_sat
///
/// Properties (all exact):
///   - rate << r_sat  -> r_eff -> rate           (unsaturated: request honored)
///   - rate  = r_sat  -> r_eff = r_sat - s x ln2 (smoothly just below)
///   - rate >> r_sat  -> r_eff -> r_sat          (asymptote, never crossed)
///   - r_eff <= min(rate, r_sat) always          (the cap never ADDS FEC)
///   - dr_eff/drate = 1 - sigmoid((rate - r_sat)/s) in (0,1): C-infinity,
///     monotone. The complement sigmoid IS the saturation pressure below.
///
/// The asymptotic approach models the p99 curve's smooth interior minimum
/// (Section 14.21): past r_sat the marginal p99 penalty grows continuously
/// (queue-delay-like), so the effective rate eases toward r_sat instead of
/// hitting a wall. Degenerate r_sat (<= 0) falls back to the hard min.
pub fn soft_saturate(rate: f64, r_sat: f64) -> f64 {
    let s = SAT_SOFTNESS * r_sat;
    if !(s > 0.0) {
        return rate.min(r_sat);
    }
    // softplus(x) > x by O(e^-x), so at rate = 0 the closed form dips a few
    // ×1e-6 below 0; floor at 0 (a rate is nonnegative). The excursion is far
    // below any operating rate, so this does not perturb the smooth interior.
    (r_sat - s * softplus((r_sat - rate) / s)).max(0.0)
}

/// Saturation pressure in [0, 1] (paper Section 14.21.1): the continuous
/// indicator that supersedes the binary "CAP BINDING" badge.
///
///   pressure = sigmoid((rate_requested - r_sat) / s),  s = SAT_SOFTNESS x r_sat
///
/// It is exactly 1 - dr_eff/drate of `soft_saturate`: the fraction of a
/// marginal repair-rate increment that the cap is currently absorbing.
///   - 0   : deep sub-saturation (more FEC still helps the tail)
///   - 0.5 : exactly at r_sat (marginal p99 benefit = marginal harm)
///   - ->1 : past r_sat (more FEC hurts the tail; the cap holds r near r_sat)
///
/// `rate_requested` is the UNCAPPED controller output (what the hint asked
/// for); pass the rate computed with the saturation cap disabled. Degenerate
/// r_sat returns 0 (no cap, no pressure).
pub fn saturation_pressure(rate_requested: f64, r_sat: f64) -> f64 {
    let s = SAT_SOFTNESS * r_sat;
    if !(s > 0.0) {
        return 0.0;
    }
    1.0 / (1.0 + (-(rate_requested - r_sat) / s).exp())
}

/// Exact P_fec over the GE channel via transfer-matrix dynamic programming.
///
/// Walks the two-state GE chain across the interleaved wire sequence of
/// W source symbols and R = round(r × W) repairs (slot i is a repair iff
/// ⌊(i+1)R/N⌋ > ⌊iR/N⌋, N = W + R), tracking the joint distribution of
/// channel state and running deficit D = (#source losses) − (#surviving
/// repairs). FEC succeeds iff D ≤ 0 at the end of the window — the same
/// criterion as `p_fec_normal` (Section 8.2), but on the exact joint
/// distribution: burst-correlated losses, burst-correlated repair
/// erasures, and the negative loss/repair correlation are all captured.
///
/// `p_gb` = P(Good→Bad), `q_bg` = P(Bad→Good); implied ε = p/(p+q).
/// O(W²) time, O(W) space. Codec overhead is NOT modeled here.
/// See paper Section 8.7.
pub fn p_fec_exact(p_gb: f64, q_bg: f64, r: f64, window_size: usize) -> f64 {
    if window_size == 0 || p_gb <= 0.0 { return 1.0; }
    let p = p_gb.min(1.0);
    let q = q_bg.clamp(1e-9, 1.0);
    let w = window_size;
    let repairs = (r.max(0.0) * w as f64).round() as usize;
    let n = w + repairs;
    // Deficit index: d ∈ [−repairs, w] stored at d + repairs ∈ [0, w+repairs].
    let dmax = w + repairs + 1;
    let off = repairs;
    let pi_b = p / (p + q);
    let mut f = vec![[0.0f64; 2]; dmax]; // [Good, Bad] per deficit
    f[off][0] = 1.0 - pi_b;
    f[off][1] = pi_b;
    let mut next = vec![[0.0f64; 2]; dmax];
    for i in 0..n {
        let is_repair = (i + 1) * repairs / n > i * repairs / n;
        for row in next.iter_mut() { *row = [0.0, 0.0]; }
        for d in 0..dmax {
            let [fg, fb] = f[d];
            if fg == 0.0 && fb == 0.0 { continue; }
            let to_good = fg * (1.0 - p) + fb * q;
            let to_bad = fg * p + fb * (1.0 - q);
            if is_repair {
                next[d.saturating_sub(1)][0] += to_good; // repair survives → deficit−1
                next[d][1] += to_bad;                    // repair lost → no help
            } else {
                next[d][0] += to_good;                   // source arrives
                next[(d + 1).min(dmax - 1)][1] += to_bad; // source lost → deficit+1
            }
        }
        std::mem::swap(&mut f, &mut next);
    }
    let p_fail: f64 = f[off + 1..].iter().map(|row| row[0] + row[1]).sum();
    (1.0 - p_fail).clamp(0.0, 1.0)
}

/// Exact minimum correction rate: smallest r with 1 − p_fec_exact ≤ delta.
///
/// Binary search on r; P_fail(r) is monotone nonincreasing up to the 1/W
/// rounding of the repair count, so r* is resolved in steps of 1/W.
/// Returns 2.0 (the search ceiling) if even 200% overhead cannot meet the
/// target. See paper Section 8.7.
pub fn compute_r_star_exact(p_gb: f64, q_bg: f64, window_size: usize, delta: f64) -> f64 {
    if window_size == 0 || p_gb <= 0.0 { return 0.0; }
    let fail = |r: f64| 1.0 - p_fec_exact(p_gb, q_bg, r, window_size);
    let mut hi = 2.0f64;
    if fail(hi) > delta { return hi; }
    let mut lo = 0.0f64;
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if fail(mid) > delta { lo = mid; } else { hi = mid; }
    }
    hi
}

/// Compute delta (tail latency) from r and rho.
///
/// delta = P(late delivery) / rho. See paper Section 6.3.
pub fn compute_delta(epsilon: f64, r: f64, rho: f64, window_size: f64, sigma2_burst: f64) -> f64 {
    if epsilon <= 0.0 || rho <= 0.0 { return 0.0; }
    let p_fec = p_fec_normal(r, epsilon, window_size, sigma2_burst);
    let fec_miss = epsilon * (1.0 - p_fec);
    if fec_miss < 1e-15 { return 0.0; }
    let p_arq = (1.0 - (1.0 - rho) / fec_miss).clamp(0.0, 1.0);
    fec_miss * p_arq / rho
}

/// P(symbol recovered within time T) using FEC + ARQ.
pub fn p_recovered_within(t: f64, epsilon: f64, q: f64, r: f64, window_size: f64, sigma2_burst: f64) -> f64 {
    if epsilon <= 0.0 { return 1.0; }
    let p_fec = p_fec_normal(r, epsilon, window_size, sigma2_burst);
    let decay = (1.0 - q).max(0.0);
    let corrections_by_t = r * (1.0 - decay.powf(t + 1.0));
    let r_needed = epsilon / (1.0 - epsilon);
    let p_arq = if r_needed > 0.0 { (corrections_by_t / r_needed).min(1.0) } else { 1.0 };
    1.0 - epsilon * (1.0 - p_fec) * (1.0 - p_arq)
}

/// Find T_cut from target reliability rho via binary search.
/// Returns Infinity at rho=1.0. See paper Section 8.6 (Mode 1, Step 1).
///
/// Search range is capped at 10×W: if the true T_cut exceeds that (rho
/// very close to 1 with marginal r), the capped value is returned.
pub fn find_t_cut(epsilon: f64, q: f64, r: f64, window_size: f64, sigma2_burst: f64, target_rho: f64) -> f64 {
    if target_rho >= 1.0 { return f64::INFINITY; }
    if target_rho <= 0.0 || epsilon <= 0.0 { return 0.0; }
    let mut lo: f64 = 0.0;
    let mut hi: f64 = window_size * 10.0;
    for _ in 0..100 {
        if hi - lo < 0.01 { break; }
        let mid = (lo + hi) / 2.0;
        if p_recovered_within(mid, epsilon, q, r, window_size, sigma2_burst) < target_rho {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}

/// Maximum burst length B_max at 99.99th percentile.
/// B_max = ceil(ln(0.0001) / ln(1-q)). See paper Section 9.3.
pub fn b_max(q: f64) -> u64 {
    if q <= 0.0 || q >= 1.0 { return 1; }
    let ln_persist = (1.0 - q).ln();
    if ln_persist >= 0.0 { return 1; }
    ((0.0001_f64).ln() / ln_persist).ceil() as u64
}

/// Buffer max from T_cut. See paper Section 9.3.
pub fn compute_buffer_max(epsilon: f64, q: f64, r: f64, t_cut: f64) -> f64 {
    if t_cut.is_infinite() {
        let bmax = b_max(q) as f64;
        let drain_rate = r * (1.0 - epsilon);
        if drain_rate > 0.0 { bmax / drain_rate } else { bmax }
    } else {
        t_cut
    }
}

/// Three-variable result.
#[derive(Debug, Clone)]
pub struct ThreeVarResult {
    pub r: f64,
    pub delta: f64,
    pub rho: f64,
    pub t_cut: f64,
    pub buffer_max: f64,
}

/// Mode 1: Given (delta, rho) -> compute r. See paper Section 8.6.
pub fn solve_r_from_delta_rho(epsilon: f64, q: f64, window_size: f64, sigma2_burst: f64, delta: f64, rho: f64) -> ThreeVarResult {
    let mut lo = epsilon / (1.0 - epsilon);
    let mut hi = 2.0;
    for _ in 0..100 {
        if hi - lo < 1e-6 { break; }
        let mid = (lo + hi) / 2.0;
        if compute_delta(epsilon, mid, rho, window_size, sigma2_burst) > delta { lo = mid; } else { hi = mid; }
    }
    let r = hi;
    let t_cut = find_t_cut(epsilon, q, r, window_size, sigma2_burst, rho);
    ThreeVarResult { r, delta, rho, t_cut, buffer_max: compute_buffer_max(epsilon, q, r, t_cut) }
}

/// Mode 2: Given (r, rho) -> compute delta. See paper Section 8.6.
pub fn solve_delta_from_r_rho(epsilon: f64, q: f64, window_size: f64, sigma2_burst: f64, r: f64, rho: f64) -> ThreeVarResult {
    let delta = compute_delta(epsilon, r, rho, window_size, sigma2_burst);
    let t_cut = find_t_cut(epsilon, q, r, window_size, sigma2_burst, rho);
    ThreeVarResult { r, delta, rho, t_cut, buffer_max: compute_buffer_max(epsilon, q, r, t_cut) }
}

/// Mode 3: Given (r, delta) -> compute rho. See paper Section 8.6.
pub fn solve_rho_from_r_delta(epsilon: f64, q: f64, window_size: f64, sigma2_burst: f64, r: f64, delta: f64) -> ThreeVarResult {
    let mut lo = 0.5;
    let mut hi = 1.0 - 1e-12;
    for _ in 0..100 {
        if hi - lo < 1e-6 { break; }
        let mid = (lo + hi) / 2.0;
        if compute_delta(epsilon, r, mid, window_size, sigma2_burst) > delta { hi = mid; } else { lo = mid; }
    }
    let rho = lo;
    let t_cut = find_t_cut(epsilon, q, r, window_size, sigma2_burst, rho);
    ThreeVarResult { r, delta, rho, t_cut, buffer_max: compute_buffer_max(epsilon, q, r, t_cut) }
}

// =========================================================================
// FEC Latency Distribution (Paper Section 14.3)
// =========================================================================

/// Poisson CDF: P(X ≥ m) where X ~ Poisson(lambda).
/// Uses the regularized incomplete gamma function via direct summation.
fn poisson_cdf_ge(m: u32, lambda: f64) -> f64 {
    if lambda <= 0.0 { return if m == 0 { 1.0 } else { 0.0 }; }
    // P(X ≥ m) = 1 - P(X < m) = 1 - Σ_{k=0}^{m-1} e^(-λ) λ^k / k!
    let mut sum = 0.0;
    let mut term = (-lambda).exp();
    for k in 0..m {
        sum += term;
        term *= lambda / (k + 1) as f64;
    }
    1.0 - sum
}

/// P(FEC recovers m losses by time T wire slots) using the Poisson model.
///
/// T is measured in wire slots after the loss. Every repair covers the
/// entire encoder window, so the useful-equation arrival rate is the
/// AGGREGATE correction rate — a fraction r/(1+r) of wire slots, each
/// surviving with probability (1-ε):
///
/// λ(T) = T × r/(1+r) × (1-ε)
///
/// P(t_fec ≤ T | m) = P(Poisson(λ(T)) ≥ m)
///
/// The taper shape (q) deliberately does NOT enter: in steady state the
/// aggregate correction rate per slot is shape-invariant (paper Section
/// 4.2); the parameter is kept for API stability.
///
/// See paper Section 14.3.
pub fn p_fec_recovery_by_time(t: f64, m: u32, r: f64, _q: f64, epsilon: f64) -> f64 {
    if m == 0 { return 1.0; }
    if epsilon <= 0.0 || r <= 0.0 { return if m == 0 { 1.0 } else { 0.0 }; }
    let lambda = t * r * (1.0 - epsilon) / (1.0 + r);
    poisson_cdf_ge(m, lambda)
}

/// Unconditional P(FEC recovers by T), marginalized over burst length.
///
/// P(t_fec ≤ T) = Σ_{m=1}^{B_99} (1-q)^{m-1} × q × Q(m, λ(T))
///
/// See paper Section 14.14.
pub fn p_fec_recovery_marginalized(t: f64, r: f64, q: f64, epsilon: f64) -> f64 {
    let q_clamped = q.clamp(0.01, 1.0);
    // B_99: 99th-percentile burst length, ceil(ln(0.01)/ln(1-q)).
    // (Not b_max(), which uses the 99.99th percentile for buffer sizing.)
    let b99 = ((0.01_f64.ln() / (1.0 - q_clamped).max(0.001).ln()).ceil() as u32).max(1);
    let mut total = 0.0;
    let mut burst_prob = q_clamped; // P(burst=1) = q
    for m in 1..=b99 {
        let p_recover = p_fec_recovery_by_time(t, m, r, q_clamped, epsilon);
        total += burst_prob * p_recover;
        burst_prob *= 1.0 - q_clamped; // P(burst=m+1) = (1-q)^m × q
    }
    total
}

/// P(symbol delivered by time T) combining FEC and ARQ.
///
/// P(delivered by T) = (1-ε) + ε × P_fec(T) + ε × (1-P_fec(T)) × I(T ≥ L_arq)
///
/// See paper Section 14.9.
pub fn p_delivered_by_time(t: f64, epsilon: f64, q: f64, r: f64, srtt: f64) -> f64 {
    if epsilon <= 0.0 { return 1.0; }
    let p_not_lost = 1.0 - epsilon;
    let p_fec = p_fec_recovery_marginalized(t, r, q, epsilon);
    let l_arq = 1.5 * srtt; // ARQ recovery time
    let p_arq = if t >= l_arq { 1.0 } else { 0.0 };
    p_not_lost + epsilon * p_fec + epsilon * (1.0 - p_fec) * p_arq
}

/// Sequence-aware P_lost: uses SACK evidence (k subsequent ACKs).
///
/// P_lost_seq(k) = 1 - reorder_rate^k
///
/// On a FIFO channel (reorder_rate=0): P_lost_seq(1) = 1.0.
/// See paper Section 14.22.
pub fn p_lost_seq(k: u32, reorder_rate: f64) -> f64 {
    if k == 0 { return 0.0; }
    1.0 - reorder_rate.powi(k as i32)
}

/// Combined P_lost from time AND sequence evidence.
pub fn p_lost_combined(age_secs: f64, epsilon: f64, srtt: f64, rttvar: f64,
                       subsequent_acks: u32, reorder_rate: f64) -> f64 {
    let p_time = p_lost(age_secs, epsilon, srtt, rttvar);
    let p_seq = p_lost_seq(subsequent_acks, reorder_rate);
    p_time.max(p_seq)
}

/// Compute correction deficit after a burst (Section 14.23).
pub fn burst_deficit(burst_length: u32, r: f64, epsilon: f64, time_in_window: f64) -> f64 {
    let pipeline = r * (1.0 - epsilon) * time_in_window / (1.0 + r);
    (burst_length as f64 - pipeline).max(0.0)
}

/// Compute boost parameters to recover deficit.
/// Returns (boosted_r, boost_duration_ticks).
pub fn boost_params(deficit: f64, r: f64, epsilon: f64) -> (f64, f64) {
    if deficit <= 0.0 { return (r, 0.0); }
    let duration = (deficit / (r * (1.0 - epsilon)).max(0.001)).max(1.0);
    let boost_r = r + deficit / duration;
    (boost_r, duration)
}

/// Solve for minimum r given a time budget and reliability target.
///
/// Binary search: find r such that P(delivered by T_budget) ≥ 1 - delta_target.
/// See paper Section 14.9.
pub fn solve_r_from_time_budget(
    epsilon: f64, q: f64, t_budget: f64, rho: f64, srtt: f64,
) -> f64 {
    let mut lo = epsilon / (1.0 - epsilon); // IT minimum
    let mut hi = 2.0;
    let target = rho; // P(delivered by T_budget) ≥ rho
    for _ in 0..100 {
        if hi - lo < 1e-6 { break; }
        let mid = (lo + hi) / 2.0;
        let p = p_delivered_by_time(t_budget, epsilon, q, mid, srtt);
        if p < target { lo = mid; } else { hi = mid; }
    }
    hi
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_survival_basic() {
        assert!((normal_survival(0.0) - 0.5).abs() < 0.01);
        assert!((normal_survival(2.0) - 0.0228).abs() < 0.005);
    }

    #[test]
    fn test_p_lost_monotone() {
        let p1 = p_lost(0.01, 0.025, 0.050, 0.005);
        let p2 = p_lost(0.05, 0.025, 0.050, 0.005);
        let p3 = p_lost(0.10, 0.025, 0.050, 0.005);
        assert!(p1 < p2 && p2 < p3);
    }

    #[test]
    fn test_burst_variance() {
        let s = burst_variance_factor(0.001, 0.5);
        assert!((s - 3.0).abs() < 0.1);
    }

    #[test]
    fn test_taper_density_decays() {
        let taper = TaperFunction::new(0.08, 0.5);
        assert!(taper.density(0.0) > taper.density(1.0));
        assert!(taper.density(1.0) > taper.density(2.0));
    }

    #[test]
    fn test_r_saturation_c4_interior_minimum() {
        // C4-Satellite numbers (paper Section 14.21 worked example):
        // eps ~ 9%, sigma2 ~ 5, W = 64, SRTT ~ 0.21s, throughput 2.5 MB/s,
        // symbol 1225 B -> t_sym = 4.9e-4 s. The saturation point must land
        // below Realtime's uncapped request (~0.49) — consistent with the
        // measured p99 reversal (412ms vs Auto's 297ms).
        let r_sat = r_saturation(0.09, 5.0, 64.0, 0.21, 1225.0 / 2.5e6);
        println!("r_sat(C4) = {r_sat}");
        assert!(
            (0.2..=0.45).contains(&r_sat),
            "C4 r_sat must be in [0.2, 0.45]: {r_sat}"
        );
    }

    #[test]
    fn test_r_saturation_c2_non_binding() {
        // C2-WiFi-like numbers: eps = 2.5%, sigma2 = 3, W = 64,
        // srtt = 13ms, t_sym = 0.1ms. Realtime requests ~0.2 here and the
        // measured tail does NOT revert (more FEC still helps at C2), so
        // the cap must sit ABOVE the request — non-binding.
        let r_sat = r_saturation(0.025, 3.0, 64.0, 0.013, 0.0001);
        println!("r_sat(C2) = {r_sat}");
        assert!(r_sat > 0.2, "C2 r_sat must be above the ~0.2 Realtime request: {r_sat}");
    }

    #[test]
    fn test_r_saturation_degenerate_inputs_no_cap() {
        assert_eq!(r_saturation(0.0, 5.0, 64.0, 0.21, 5e-4), 1.0);
        assert_eq!(r_saturation(1.0, 5.0, 64.0, 0.21, 5e-4), 1.0);
        assert_eq!(r_saturation(0.09, 0.0, 64.0, 0.21, 5e-4), 1.0);
        assert_eq!(r_saturation(0.09, 5.0, 0.0, 0.21, 5e-4), 1.0);
        assert_eq!(r_saturation(0.09, 5.0, 64.0, 0.0, 5e-4), 1.0);
        assert_eq!(r_saturation(0.09, 5.0, 64.0, 0.21, 0.0), 1.0);
        assert_eq!(r_saturation(f64::NAN, 5.0, 64.0, 0.21, 5e-4), 1.0);
    }

    #[test]
    fn test_soft_saturate_limits_and_shape() {
        let r_sat = 0.40;
        // Deep sub-saturation: r_eff -> rate (request honored).
        assert!((soft_saturate(0.05, r_sat) - 0.05).abs() < 1e-3);
        // At r_sat: smoothly just below (r_sat - s*ln2).
        let s = SAT_SOFTNESS * r_sat;
        assert!((soft_saturate(r_sat, r_sat) - (r_sat - s * 2f64.ln())).abs() < 1e-12);
        // Far above: asymptotes to r_sat (reaches it to machine precision),
        // never crossing it.
        let hi = soft_saturate(5.0, r_sat);
        assert!(hi <= r_sat && hi > r_sat - 1e-6, "asymptote to r_sat: {hi}");
        // r_eff <= min(rate, r_sat) everywhere on a fine sweep, and monotone.
        let mut prev = 0.0;
        for i in 0..=500 {
            let rate = i as f64 * 0.002; // 0 .. 1.0
            let e = soft_saturate(rate, r_sat);
            assert!(e <= rate.min(r_sat) + 1e-12, "cap must not add FEC at {rate}: {e}");
            assert!(e >= prev - 1e-12, "monotone nondecreasing in rate");
            prev = e;
        }
        // Continuity: no jump anywhere (bounded finite differences).
        let mut max_step = 0.0f64;
        let mut last = soft_saturate(0.0, r_sat);
        for i in 1..=1000 {
            let e = soft_saturate(i as f64 * 0.001, r_sat);
            max_step = max_step.max((e - last).abs());
            last = e;
        }
        assert!(max_step < 0.002, "kink-free: max step {max_step}");
        // Degenerate r_sat: falls back to hard min.
        assert_eq!(soft_saturate(0.3, 0.0), 0.0);
    }

    #[test]
    fn test_saturation_pressure_monotone_half_at_rsat() {
        let r_sat = 0.40;
        // 0.5 exactly at r_sat; -> 0 far below; -> 1 far above.
        assert!((saturation_pressure(r_sat, r_sat) - 0.5).abs() < 1e-12);
        assert!(saturation_pressure(0.05, r_sat) < 0.01);
        assert!(saturation_pressure(1.0, r_sat) > 0.99);
        // Monotone increasing in the requested rate, in [0,1].
        let mut prev = 0.0;
        for i in 0..=500 {
            let req = i as f64 * 0.003;
            let pi = saturation_pressure(req, r_sat);
            assert!((0.0..=1.0).contains(&pi));
            assert!(pi >= prev - 1e-12, "pressure monotone in request");
            prev = pi;
        }
        // Consistency with soft_saturate: pressure = 1 - dr_eff/drate.
        let h = 1e-6;
        let d = (soft_saturate(0.42 + h, r_sat) - soft_saturate(0.42 - h, r_sat)) / (2.0 * h);
        assert!((saturation_pressure(0.42, r_sat) - (1.0 - d)).abs() < 1e-4);
        // Degenerate r_sat: no pressure.
        assert_eq!(saturation_pressure(0.3, 0.0), 0.0);
    }

    #[test]
    fn test_controller_soft_cap_continuous_across_rsat() {
        // C4-like inputs (Section 14.21 worked example) with saturation on.
        // Sweep the tail target from loose to very tight: the emitted rate
        // must rise CONTINUOUSLY and approach r_sat without a kink where the
        // uncapped request crosses it (the old hard min had a corner there).
        let mk = |tail: f64| RateInputs {
            p_upper: 0.09,
            sigma2: 5.0,
            mean_burst: 3.0,
            mass: MassStats::default(),
            tail_provision: false,
            window: 64.0,
            t_symbols: 0.0,
            srtt: 0.21,
            t_sym: 1225.0 / 2.5e6,
            codec_overhead: 0.0,
            tail_target: tail,
            bulk_late_is_fine: false,
            completion_exposure: 0.0,
            inner_feedback: 0.0,
            saturation_cap: true,
            max_overhead: 0.6,
        };
        let r_sat = r_saturation(0.09, 5.0, 64.0, 0.21, 1225.0 / 2.5e6);
        // Geometric sweep of the tail target from ~eps (uncapped r = 0) down
        // to 9e-11 (very aggressive request, uncapped r well past r_sat), so
        // the INPUT moves smoothly and any kink is the function's own.
        let mut prev = controller_rate(&mk(0.09));
        let mut max_step = 0.0f64;
        for i in 1..=2000 {
            let tail = 0.09 * (1e-9f64).powf(i as f64 / 2000.0);
            let r = controller_rate(&mk(tail));
            assert!(r <= r_sat + 1e-9, "soft cap holds below r_sat: {r} vs {r_sat}");
            max_step = max_step.max((r - prev).abs());
            prev = r;
        }
        assert!(max_step < 0.01, "no kink across r_sat: max step {max_step}");
        // The tightest request presses hard against the cap (pressure high),
        // yet the emitted rate stays just under r_sat.
        let tight = controller_rate(&mk(1e-9));
        assert!(tight > 0.9 * r_sat && tight < r_sat);
    }

    fn bulk_inputs(p: f64, chi: f64) -> RateInputs {
        RateInputs {
            p_upper: p,
            sigma2: 1.0,
            mean_burst: 1.0,
            mass: MassStats::default(),
            tail_provision: false,
            window: 64.0,
            t_symbols: 0.0,
            srtt: 0.05,
            t_sym: 0.0,
            codec_overhead: 0.0,
            tail_target: 1e-5,
            bulk_late_is_fine: true,
            completion_exposure: chi,
            inner_feedback: 0.0,
            saturation_cap: false,
            max_overhead: 0.5,
        }
    }

    /// C2-tunnel operating point (paper 14.28): eps ~ 2.6% (GE 1.3%/50%,
    /// mean burst 2), SRTT ~ 13 ms, 100 Mbit with 1200 B symbols.
    fn tunnel_inputs(p: f64, w: f64) -> RateInputs {
        RateInputs {
            p_upper: p,
            sigma2: 2.9,
            mean_burst: 2.0,
            mass: MassStats::default(),
            tail_provision: false,
            window: 56.0,
            t_symbols: 0.0,
            srtt: 0.013,
            t_sym: 1200.0 * 8.0 / 100e6,
            codec_overhead: 0.0,
            tail_target: 1e-5,
            bulk_late_is_fine: true,
            completion_exposure: 0.0,
            inner_feedback: w,
            saturation_cap: false,
            max_overhead: 0.5,
        }
    }

    #[test]
    fn test_inner_feedback_floor_c2_band() {
        // At the L1 C2 operating point the floor must land in the sane
        // band the P6 fixed-floor ablation suggested (~0.01-0.04), and
        // stay there across the plausible p_upper range.
        for p in [0.026, 0.035, 0.045] {
            let r = controller_rate(&tunnel_inputs(p, 1.0));
            println!("r_floor(C2, p={p}) = {r}");
            assert!(
                (0.01..=0.05).contains(&r),
                "C2 floor out of band at p={p}: {r}"
            );
        }
        // Without the weight the Bulk glide keeps r* = 0 (old behavior).
        assert_eq!(controller_rate(&tunnel_inputs(0.026, 0.0)), 0.0);
    }

    #[test]
    fn test_inner_feedback_floor_continuous_in_weight_and_loss() {
        // Continuous & monotone in the weight.
        let mut prev = controller_rate(&tunnel_inputs(0.026, 0.0));
        assert_eq!(prev, 0.0);
        let mut max_step = 0.0f64;
        for i in 1..=100 {
            let w = i as f64 / 100.0;
            let r = controller_rate(&tunnel_inputs(0.026, w));
            assert!(r >= prev - 1e-12, "rate must be nondecreasing in w");
            max_step = max_step.max(r - prev);
            prev = r;
        }
        assert!(prev > 0.01);
        assert!(max_step < 0.005, "no jumps along the weight: {max_step}");

        // Continuous in eps, and vanishes (r -> 0) on clean channels with
        // no cutoff: the floor is 0 well before p reaches 0.
        let mut prev = 0.0f64;
        let mut max_step = 0.0f64;
        for i in 0..=400 {
            let p = i as f64 * 1e-4; // 0 .. 4%
            let r = controller_rate(&tunnel_inputs(p, 1.0));
            if i > 0 {
                max_step = max_step.max((r - prev).abs());
            }
            prev = r;
        }
        assert!(max_step < 0.005, "no jumps along eps: {max_step}");
        let clean = controller_rate(&tunnel_inputs(5e-4, 1.0));
        assert_eq!(clean, 0.0, "clean channel keeps pure ARQ: {clean}");
    }

    #[test]
    fn test_inner_feedback_floor_sentinels() {
        // No throughput estimate (t_sym = 0) disables the floor — same
        // convention as the burst B/T term.
        let mut inp = tunnel_inputs(0.026, 1.0);
        inp.t_sym = 0.0;
        assert_eq!(controller_rate(&inp), 0.0);
        // Degenerate srtt likewise.
        assert_eq!(inner_feedback_floor(0.026, 2.0, 0.0, 1e-4, 0.5), 0.0);
        assert_eq!(inner_feedback_floor(0.026, 2.0, f64::NAN, 1e-4, 0.5), 0.0);
        assert_eq!(inner_feedback_floor(0.0, 2.0, 0.013, 1e-4, 0.5), 0.0);
        // NaN weight is treated as 0 (unknown).
        let mut inp = tunnel_inputs(0.026, f64::NAN);
        inp.inner_feedback = f64::NAN;
        assert_eq!(controller_rate(&inp), 0.0);
        // The floor respects the cap.
        let r = inner_feedback_floor(0.4, 2.0, 0.013, 1e-4, 0.05);
        assert!(r <= 0.05 + 1e-12);
    }

    #[test]
    fn test_inner_feedback_floor_never_reduces_rate() {
        // max() semantics: where the base controller already exceeds the
        // floor (Realtime-tight target), the weight changes nothing.
        let mut tight = tunnel_inputs(0.026, 0.0);
        tight.bulk_late_is_fine = false;
        tight.tail_target = 1e-7;
        let base = controller_rate(&tight);
        let mut tight_w = tight;
        tight_w.inner_feedback = 1.0;
        let with_floor = controller_rate(&tight_w);
        assert!(with_floor >= base - 1e-12);
        assert!(
            base > 0.05,
            "tight target should already exceed the floor: {base}"
        );
        assert_eq!(with_floor, base, "floor must be inert below the base rate");
    }

    #[test]
    fn test_completion_exposure_kernel() {
        let srtt = 0.05;
        let rttvar = srtt / 8.0; // sigma_arq = 4*rttvar = srtt/2
        // Mid-stream: T_rem far beyond 1.5 SRTT + 8 sigma -> exactly 0.
        assert_eq!(completion_exposure(10.0, srtt, rttvar), 0.0);
        // At the ARQ horizon T_rem = 1.5 SRTT: chi = 1/2.
        let mid = completion_exposure(1.5 * srtt, srtt, rttvar);
        assert!((mid - 0.5).abs() < 0.01, "chi(1.5 SRTT) ~ 0.5: {mid}");
        // T_rem = 0 (source exhausted): chi ~ 1.
        assert!(completion_exposure(0.0, srtt, rttvar) > 0.99);
        // Monotone nonincreasing in T_rem.
        let mut prev = 1.0;
        for i in 0..200 {
            let chi = completion_exposure(i as f64 * 0.002, srtt, rttvar);
            assert!(chi <= prev + 1e-12);
            prev = chi;
        }
        // sigma_arq floor: rttvar = 0 must not divide by zero.
        let chi0 = completion_exposure(0.0, srtt, 0.0);
        assert!(chi0.is_finite() && chi0 > 0.99);
        // Unknown T_rem (endless tunnel) -> 0 (pure ARQ steady state).
        assert_eq!(completion_exposure(f64::INFINITY, srtt, rttvar), 0.0);
        assert_eq!(completion_exposure(f64::NAN, srtt, rttvar), 0.0);
    }

    #[test]
    fn test_bulk_chi_zero_rate_zero_even_at_cold_start() {
        // M1 (paper 14.26): mid-stream chi = 0 -> delta_eff = p -> r* = 0
        // IDENTICALLY, even at the estimator's Beta(1,1) cold-start upper
        // quantile p ~ 0.975 (the old min(0.1, p) clamp pinned r at
        // max_overhead here, wasting ~1/3 of the wire for 2-3 RTTs).
        assert_eq!(controller_rate(&bulk_inputs(0.975, 0.0)), 0.0);
        // M2 (paper 14.26): p above the old 0.1 clamp forever -> the old
        // mapping paid permanent FEC on top of ARQ; chi = 0 -> 0 now.
        assert_eq!(controller_rate(&bulk_inputs(0.12, 0.0)), 0.0);
        // And an ordinary steady state.
        assert_eq!(controller_rate(&bulk_inputs(0.05, 0.0)), 0.0);
    }

    #[test]
    fn test_bulk_chi_one_matches_tail_target_rate() {
        // chi = 1 -> delta_eff = BULK_TAIL_BUDGET regardless of p: the rate
        // equals the plain controller at tail_target = 0.05 (the 14.25 tail
        // budget) — the tail burst as the glide's limiting case.
        for p in [0.05, 0.10, 0.20] {
            let bulk = controller_rate(&bulk_inputs(p, 1.0));
            let mut plain = bulk_inputs(p, 0.0);
            plain.bulk_late_is_fine = false;
            plain.tail_target = BULK_TAIL_BUDGET;
            let reference = controller_rate(&plain);
            assert!(
                (bulk - reference).abs() < 1e-12,
                "chi=1 must equal the delta=0.05 tail rate at p={p}: {bulk} vs {reference}"
            );
        }
        // At p above the budget the tail rate is genuinely positive.
        assert!(controller_rate(&bulk_inputs(0.10, 1.0)) > 0.05);
    }

    #[test]
    fn test_bulk_chi_glide_is_continuous() {
        // Sweep chi 0 -> 1 at the M2 operating point: the rate must rise
        // from 0 to the tail rate with no jumps (continuity — the hard
        // project principle; the ramp REPLACES the one-shot tail burst).
        let p = 0.12;
        let mut prev = controller_rate(&bulk_inputs(p, 0.0));
        assert_eq!(prev, 0.0);
        let mut max_step = 0.0f64;
        for i in 1..=1000 {
            let chi = i as f64 / 1000.0;
            let r = controller_rate(&bulk_inputs(p, chi));
            assert!(r >= prev - 1e-12, "rate must be nondecreasing in chi");
            max_step = max_step.max(r - prev);
            prev = r;
        }
        assert!(prev > 0.1, "chi=1 tail rate must be positive: {prev}");
        assert!(
            max_step < 0.01,
            "no jumps along the chi glide: max step {max_step}"
        );
    }

    // --- window-mass tail provisioning (paper Section 8.4.1) ---------------

    /// Exact moments of a geometric burst law with exit probability q:
    /// E[B] = 1/q, E[B^2] = (2-q)/q^2.
    fn geom_moments(q: f64) -> (f64, f64) {
        (1.0 / q, (2.0 - q) / (q * q))
    }

    /// Simulate a GE chain and collect the multi-scale sliding block-mass
    /// statistics at block scale w0 — the estimator-side measurement.
    fn ge_mass_stats(p: f64, q: f64, w0: usize, n: usize, seed: u64) -> MassStats {
        use rand::prelude::*;
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        let mut bad = false;
        let mut stats = MassStats { block_scale: w0 as f64, ..Default::default() };
        let mut cnt = [0f64; MASS_SCALES];
        let mut ring: Vec<f64> = Vec::new();
        let mut k = 0f64;
        for i in 0..n {
            bad = if bad { rng.gen::<f64>() >= q } else { rng.gen::<f64>() < p };
            if bad {
                k += 1.0;
            }
            if (i + 1) % w0 == 0 {
                ring.push(k);
                k = 0.0;
                for m in 1..=MASS_SCALES {
                    if ring.len() >= m {
                        let j: f64 = ring[ring.len() - m..].iter().sum();
                        cnt[m - 1] += 1.0;
                        if j > 0.0 {
                            stats.nz[m - 1] += 1.0;
                            stats.m1[m - 1] += j;
                            stats.m2[m - 1] += j * j;
                        }
                    }
                }
            }
        }
        for m in 0..MASS_SCALES {
            if stats.nz[m] > 0.0 {
                stats.m1[m] /= stats.nz[m];
                stats.m2[m] /= stats.nz[m];
                stats.nz[m] /= cnt[m];
            }
        }
        stats
    }

    /// MassStats with the SAME (nz, m1, m2) at every scale — a mechanical
    /// fixture for controller-level tests (unphysical but well-defined).
    fn uniform_mass(w0: f64, nz: f64, m1: f64, m2: f64) -> MassStats {
        MassStats {
            block_scale: w0,
            nz: [nz; MASS_SCALES],
            m1: [m1; MASS_SCALES],
            m2: [m2; MASS_SCALES],
        }
    }

    #[test]
    fn test_ln_gamma_reference_values() {
        // Gamma(1) = Gamma(2) = 1, Gamma(3) = 2, Gamma(11) = 3628800,
        // Gamma(1.5) = sqrt(pi)/2.
        assert!(ln_gamma(1.0).abs() < 1e-10);
        assert!(ln_gamma(2.0).abs() < 1e-10);
        assert!((ln_gamma(3.0) - 2.0f64.ln()).abs() < 1e-10);
        assert!((ln_gamma(11.0) - 3628800.0f64.ln()).abs() < 1e-8);
        let g15 = (std::f64::consts::PI.sqrt() / 2.0).ln();
        assert!((ln_gamma(1.5) - g15).abs() < 1e-10);
    }

    #[test]
    fn test_fit_burst_tail_recovers_geometric() {
        // Feeding EXACT geometric moments must fit k ~ 1 and theta ~ 1-q:
        // the geometric law is the k = 1 special case, so a GE-like
        // measured tail is reproduced, not inflated.
        for &q in &[0.5, 0.4, 0.3] {
            let (m1, m2) = geom_moments(q);
            let (theta, k) = fit_burst_tail(m1, m2);
            assert!(
                (0.85..=1.25).contains(&k),
                "geometric moments must fit k ~ 1 at q={q}: k={k}"
            );
            assert!(
                (theta - (1.0 - q)).abs() < 0.08,
                "theta ~ 1-q at q={q}: theta={theta}"
            );
        }
    }

    #[test]
    fn test_fit_burst_tail_heavy_tail_fits_small_k() {
        // A heavy-tailed law (large E[X^2] relative to E[X]^2) must fit
        // k < 1 and a heavier survival than the geometric of the same
        // mean at large t.
        let (m1, m2) = (4.0, 400.0);
        let (theta, k) = fit_burst_tail(m1, m2);
        assert!(k < 0.7, "heavy moments must fit k < 0.7: k={k}");
        let s_fit = burst_tail_survival(theta, k, 50.0);
        let (tg, kg) = fit_burst_tail(m1, geom_moments(1.0 / m1).1);
        let s_geom = burst_tail_survival(tg, kg, 50.0);
        assert!(
            s_fit > 20.0 * s_geom,
            "fitted tail must be far heavier at t=50: {s_fit} vs geometric {s_geom}"
        );
    }

    #[test]
    fn test_fit_burst_tail_degenerate_inputs() {
        // Unknown / inconsistent moments fall back to the geometric law.
        let (_, k) = fit_burst_tail(2.0, 0.0);
        assert_eq!(k, 1.0);
        let (_, k) = fit_burst_tail(2.0, f64::NAN);
        assert_eq!(k, 1.0);
        let (theta, k) = fit_burst_tail(1.0, 1.0);
        assert_eq!(k, 1.0);
        assert!(theta <= 1e-10, "all-singleton masses: theta ~ 0, got {theta}");
        // m2 below the geometric spread (sub-geometric) still returns a
        // valid, lighter-tailed model (k > 1).
        let (theta, k) = fit_burst_tail(3.0, 9.5);
        assert!(theta > 0.0 && theta < 1.0 && k > 1.0, "theta={theta} k={k}");
    }

    #[test]
    fn test_r_star_mass_tracks_exact_ge() {
        // On a GE channel the measured mass tail is what the Section 8.7
        // exact DP predicts, so the mass-quantile r* must land in a band
        // around r*_exact on the Section 2.4 scenarios: no material
        // over-provisioning where GE is adequate (Weibull-fit slack and
        // scale interpolation are the honest error bars).
        let cases = [("WiFi", 0.013, 0.5), ("LTE", 0.02, 0.4), ("Sat", 0.03, 0.3)];
        for &(name, p, q) in &cases {
            let stats = ge_mass_stats(p, q, 50, 2_000_000, 42);
            for &tgt in &[0.05, 0.02, 0.01] {
                let r_exact = compute_r_star_exact(p, q, 50, tgt);
                let r_mass = r_star_mass(&stats, 50.0, tgt, 1.0);
                let ratio = r_mass / r_exact;
                println!(
                    "{name} tgt={tgt}: exact={r_exact:.3} mass={r_mass:.3} ratio={ratio:.2}"
                );
                assert!(
                    (0.6..=1.5).contains(&ratio),
                    "{name} tgt={tgt}: mass r* {r_mass:.3} vs exact {r_exact:.3} (ratio {ratio:.2})"
                );
            }
        }
    }

    #[test]
    fn test_r_star_mass_monotone_and_continuous_in_delta() {
        // Tighter delta_wf => higher quantile => more rate; the sweep must
        // be smooth (no jumps) and 0 at delta_wf >= 1 (pure ARQ meets the
        // contract).
        let stats = uniform_mass(64.0, 0.5, 3.0, 20.0);
        assert_eq!(r_star_mass(&stats, 64.0, 1.0, 1.0), 0.0);
        assert_eq!(r_star_mass(&stats, 64.0, 2.0, 1.0), 0.0);
        let mut prev = r_star_mass(&stats, 64.0, 0.999, 1.0);
        let mut max_step = 0.0f64;
        for i in 1..=800 {
            let tgt = 0.999 * (1e-6f64 / 0.999).powf(i as f64 / 800.0);
            let r = r_star_mass(&stats, 64.0, tgt, 1.0);
            assert!(r >= prev - 1e-9, "monotone as delta tightens: {prev} -> {r}");
            max_step = max_step.max(r - prev);
            prev = r;
        }
        assert!(max_step < 0.02, "no jumps along delta_wf: max step {max_step}");
    }

    #[test]
    fn test_r_star_mass_heavier_tail_more_rate() {
        // Same nonzero rate and mean mass, heavier tail (larger m2)
        // => more provisioning; monotone and material.
        let (w0, w, tgt) = (64.0, 64.0, 0.01);
        let mut prev = 0.0f64;
        for &m2 in &[30.0, 60.0, 120.0, 300.0] {
            let r = r_star_mass(&uniform_mass(w0, 0.6, 4.0, m2), w, tgt, 1.0);
            assert!(
                r >= prev - 1e-9,
                "heavier mass tail must not need less rate: m2={m2} r={r} prev={prev}"
            );
            prev = r;
        }
        let r_light = r_star_mass(&uniform_mass(w0, 0.6, 4.0, 30.0), w, tgt, 1.0);
        assert!(
            prev > 1.5 * r_light,
            "m2=300 must demand >1.5x the light-tail rate: {prev} vs {r_light}"
        );
    }

    #[test]
    fn test_r_star_mass_degenerate_guards() {
        let good = uniform_mass(64.0, 0.5, 3.0, 20.0);
        assert_eq!(r_star_mass(&MassStats::default(), 64.0, 0.01, 1.0), 0.0);
        let mut s = good;
        s.nz[0] = 0.0;
        assert_eq!(r_star_mass(&s, 64.0, 0.01, 1.0), 0.0);
        let mut s = good;
        s.block_scale = 0.0;
        assert_eq!(r_star_mass(&s, 64.0, 0.01, 1.0), 0.0);
        assert_eq!(r_star_mass(&good, 0.0, 0.01, 1.0), 0.0);
        assert_eq!(r_star_mass(&good, 64.0, 0.0, 1.0), 0.0);
        assert_eq!(r_star_mass(&good, 64.0, f64::NAN, 1.0), 0.0);
        // Uncoverable mass quantile saturates at the declared ceiling and
        // the caller's max_overhead clamp governs.
        let heavy = uniform_mass(64.0, 0.9, 30.0, 3000.0);
        let r = r_star_mass(&heavy, 64.0, 1e-6, 1.0);
        assert!(r >= R_STAR_TAIL_CEILING - 1e-9, "infeasible must hit ceiling: {r}");
    }

    #[test]
    fn test_r_star_mass_level_rescale() {
        // Level equivariance: halving the current loss level (scale 0.5)
        // must need materially less rate than scale 1, doubling more; and
        // scale -> 0 sends the term to 0 (clean-now channel), all
        // continuously and monotonically.
        let stats = uniform_mass(64.0, 0.6, 4.0, 60.0);
        let r_half = r_star_mass(&stats, 64.0, 0.01, 0.5);
        let r_one = r_star_mass(&stats, 64.0, 0.01, 1.0);
        let r_two = r_star_mass(&stats, 64.0, 0.01, 2.0);
        assert!(r_half < r_one && r_one < r_two, "{r_half} < {r_one} < {r_two}");
        assert!(r_half < 0.7 * r_one, "halved level must materially relax: {r_half} vs {r_one}");
        let mut prev = 0.0f64;
        let mut max_step = 0.0f64;
        for i in 1..=400 {
            let s = i as f64 / 100.0; // 0.01 .. 4.0
            let r = r_star_mass(&stats, 64.0, 0.01, s);
            assert!(r >= prev - 1e-9, "monotone in level scale");
            max_step = max_step.max(r - prev);
            prev = r;
        }
        assert!(max_step < 0.05, "no jumps along the level scale: {max_step}");
        // Degenerate scales are guarded.
        assert_eq!(r_star_mass(&stats, 64.0, 0.01, 0.0), 0.0);
        assert_eq!(r_star_mass(&stats, 64.0, 0.01, f64::NAN), 0.0);
        // eps_mass helper consistency.
        assert!((stats.eps_mass() - 0.6 * 4.0 / 64.0).abs() < 1e-12);
        assert_eq!(MassStats::default().eps_mass(), 0.0);
    }

    #[test]
    fn test_controller_tail_provision_identities() {
        // The Bulk chi = 0 identity r*(delta = p) = 0 must survive with
        // the tail term ON and mass data present; loose targets likewise.
        let mk = |tail_on: bool, tgt: f64| RateInputs {
            p_upper: 0.026,
            sigma2: 2.9,
            mean_burst: 2.0,
            mass: uniform_mass(64.0, 0.6, 2.5, 10.0),
            tail_provision: tail_on,
            window: 64.0,
            t_symbols: 0.0,
            srtt: 0.013,
            t_sym: 0.0,
            codec_overhead: 0.0,
            tail_target: tgt,
            bulk_late_is_fine: false,
            completion_exposure: 0.0,
            inner_feedback: 0.0,
            saturation_cap: false,
            max_overhead: 0.6,
        };
        let mut bulk = mk(true, 1e-3);
        bulk.bulk_late_is_fine = true;
        assert_eq!(controller_rate(&bulk), 0.0, "Bulk chi=0 identity must survive");
        // delta above eps: both 0 (delta_wf >= 1 short-circuits the term).
        assert_eq!(controller_rate(&mk(true, 0.05)), 0.0);
        assert_eq!(controller_rate(&mk(false, 0.05)), 0.0);
        // No mass data => term inert => exactly the old rate.
        let mut no_mass = mk(true, 1e-4);
        no_mass.mass = MassStats::default();
        let mut off = mk(false, 1e-4);
        off.mass = MassStats::default();
        assert_eq!(controller_rate(&no_mass), controller_rate(&off));
    }

    #[test]
    fn test_controller_tail_provision_heavy_mass_raises_rate() {
        // Same nonzero-block rate and mean mass, heavy measured mass tail:
        // the corrected controller must provision materially more than
        // the GE-only controller at a tight (Auto/Realtime-class) target.
        let mk = |m2: f64, tail_on: bool| RateInputs {
            p_upper: 0.09,
            sigma2: 5.0,
            mean_burst: 3.3,
            mass: uniform_mass(64.0, 0.9, 6.4, m2),
            tail_provision: tail_on,
            window: 64.0,
            t_symbols: 0.0,
            srtt: 0.21,
            t_sym: 0.0,
            codec_overhead: 0.0,
            tail_target: 1e-4,
            bulk_late_is_fine: false,
            completion_exposure: 0.0,
            inner_feedback: 0.0,
            saturation_cap: false,
            max_overhead: 1.5,
        };
        let m2_geom = 6.4f64 * 6.4 * 2.0; // geometric-like spread
        let r_old = controller_rate(&mk(m2_geom, false));
        let r_ge = controller_rate(&mk(m2_geom, true));
        let r_heavy = controller_rate(&mk(400.0, true));
        println!("old={r_old:.3} ge-mass={r_ge:.3} heavy-mass={r_heavy:.3}");
        assert!(
            r_heavy > 1.2 * r_ge,
            "heavy mass tail must raise r materially: {r_heavy} vs {r_ge}"
        );
        assert!(r_ge >= r_old - 1e-12, "tail term never lowers the rate");
    }

    #[test]
    fn test_controller_tail_continuous_in_moments() {
        // Sweep the measured second moment from geometric-like to heavy:
        // the emitted rate must rise continuously (no fit-boundary jumps).
        let mk = |m2: f64| RateInputs {
            p_upper: 0.05,
            sigma2: 3.8,
            mean_burst: 2.5,
            mass: uniform_mass(64.0, 0.7, 4.6, m2),
            tail_provision: true,
            window: 64.0,
            t_symbols: 0.0,
            srtt: 0.05,
            t_sym: 0.0,
            codec_overhead: 0.0,
            tail_target: 1e-4,
            bulk_late_is_fine: false,
            completion_exposure: 0.0,
            inner_feedback: 0.0,
            saturation_cap: false,
            max_overhead: 2.0,
        };
        let m2_lo = 4.6f64 * 4.6 * 1.5;
        let mut prev = controller_rate(&mk(m2_lo));
        let mut max_step = 0.0f64;
        for i in 1..=4000 {
            let m2 = m2_lo + (400.0 - m2_lo) * i as f64 / 4000.0;
            let r = controller_rate(&mk(m2));
            assert!(r >= prev - 1e-9, "rate nondecreasing in m2");
            max_step = max_step.max(r - prev);
            prev = r;
        }
        assert!(max_step < 0.005, "no jumps along m2: max step {max_step}");
    }

    #[test]
    fn test_three_var_consistency() {
        let eps = 0.10;
        let q = 0.3;
        let w = 20.0;
        let s2 = 3.0;
        let m1 = solve_r_from_delta_rho(eps, q, w, s2, 0.05, 0.999);
        let m2 = solve_delta_from_r_rho(eps, q, w, s2, m1.r, 0.999);
        assert!((m1.delta - m2.delta).abs() < 0.01);
    }

    // --- derive_window (paper Section 8.8) ---------------------------------

    // Reference channels (paper Sections 2.4 / 8.5 / 14.21). send_rate in
    // symbols/s = throughput / symbol_size; srtt in seconds.
    // WiFi:  eps=0.025, sigma2=3.0, srtt=13 ms,  send_rate=10_000 (t_sym=1e-4)
    // Sat:   eps=0.09,  sigma2=5.0, srtt=210 ms, send_rate=2_041  (t_sym=4.9e-4)

    #[test]
    fn test_derive_window_finite_and_bounded_everywhere() {
        // Sweep a wide grid including degenerate inputs; W* must stay in
        // [WINDOW_MIN, WINDOW_MAX] and be finite.
        for &delta in &[1e-9, 1e-6, 1e-4, 1e-2, 0.05, 0.2, 0.9] {
            for &eps in &[0.0, 1e-6, 0.001, 0.025, 0.09, 0.3, 0.5, 0.999, 1.0] {
                for &sigma2 in &[0.0, 1.0, 3.0, 5.0, 50.0] {
                    for &srtt in &[0.0, 0.001, 0.013, 0.21] {
                        for &send_rate in &[0.0, 100.0, 2_041.0, 10_000.0] {
                            for &budget in &[-1.0, 0.0, 0.01, 0.2] {
                                let w = derive_window(delta, eps, sigma2, srtt, send_rate, budget);
                                assert!(w.is_finite(), "W* not finite at eps={eps} delta={delta}");
                                assert!((WINDOW_MIN..=WINDOW_MAX).contains(&w),
                                    "W*={w} out of bounds at eps={eps} delta={delta} s2={sigma2}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_derive_window_nondecreasing_as_delta_tightens() {
        // Tighter delta => larger variance margin => a larger window is
        // wanted to amortise it (until the latency ceiling caps it). W* must
        // be non-decreasing as delta shrinks. Use a loose latency budget so
        // the overhead knee, not the ceiling, is the binding term.
        let (eps, sigma2, srtt, send_rate, budget) = (0.05, 3.0, 0.05, 5_000.0, 10.0);
        let mut prev = 0.0;
        for &delta in &[0.05, 1e-2, 1e-3, 1e-4, 1e-6, 1e-9] {
            let w = derive_window(delta, eps, sigma2, srtt, send_rate, budget);
            assert!(w >= prev - 1e-9, "W* dropped as delta tightened: {prev} -> {w} at delta={delta}");
            prev = w;
        }
    }

    #[test]
    fn test_derive_window_increases_with_burst_variance() {
        // Higher sigma2 (burstier channel) => the burst-absorbency floor
        // W_bur = (sigma2+1)/2 / (eps(1-eps)) grows => larger window. Use a
        // loose delta so the burst floor (not the unreachable knee) governs,
        // and a loose budget so the latency ceiling does not cap it.
        let (delta, eps, srtt, send_rate, budget) = (0.15, 0.05, 0.05, 5_000.0, 10.0);
        let mut prev = 0.0;
        for &sigma2 in &[1.0, 2.0, 3.0, 5.0, 8.0] {
            let w = derive_window(delta, eps, sigma2, srtt, send_rate, budget);
            assert!(w >= prev - 1e-9, "W* dropped as sigma2 grew: {prev} -> {w} at sigma2={sigma2}");
            prev = w;
        }
    }

    #[test]
    fn test_derive_window_latency_ceiling_binds_for_tight_delta() {
        // WiFi, Realtime-tight delta: the overhead knee is unreachable (the
        // margin dominates the small IT floor), so the window rides the
        // latency ceiling W_lat = budget * send_rate. With budget = srtt and
        // send_rate = 10_000, W_lat = 130 -> W* = 130.
        let w = derive_window(1e-6, 0.025, 3.0, 0.013, 10_000.0, 0.0);
        assert!((w - 130.0).abs() < 1.0, "expected latency-bound ~130, got {w}");
    }

    #[test]
    fn test_derive_window_tighter_budget_shrinks_window() {
        // A tighter (Realtime) latency budget must shrink the window vs an
        // Auto ~1 RTT budget, all else equal — the latency/overhead tradeoff.
        let auto = derive_window(1e-6, 0.025, 3.0, 0.013, 10_000.0, 0.013);
        let realtime = derive_window(1e-6, 0.025, 3.0, 0.013, 10_000.0, 0.005);
        assert!(realtime < auto, "tighter budget did not shrink W*: rt={realtime} auto={auto}");
        assert!(realtime >= WINDOW_MIN);
    }

    #[test]
    fn test_derive_window_loose_delta_is_burst_floor_bound() {
        // Bulk (delta >= eps): no margin pressure (z <= 0, W_over = 0), so the
        // window collapses to the burst-absorbency floor min(W_bur, W_lat),
        // i.e. a small window — NOT the latency ceiling. At eps=0.09,
        // sigma2=5, W_bur = 3/(0.09*0.91) = 36.6; W_lat huge (loose budget).
        let w = derive_window(0.2, 0.09, 5.0, 0.21, 5_000.0, 10.0);
        assert!((30.0..45.0).contains(&w), "expected burst-floor ~37, got {w}");
    }

    #[test]
    fn test_derive_window_no_throughput_disables_ceiling() {
        // No send_rate sample => latency ceiling disabled => the overhead knee
        // (unbounded here) pushes to WINDOW_MAX, not a degenerate small value.
        let w = derive_window(1e-6, 0.025, 3.0, 0.013, 0.0, 0.0);
        assert!(w >= WINDOW_MAX - 1e-9, "expected WINDOW_MAX with no ceiling, got {w}");
    }
}
