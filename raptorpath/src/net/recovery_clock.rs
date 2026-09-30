//! The quantile-native window law (paper §7.4) and the `[QCLK]` gauge.

use super::*;

// ── The quantile-native window law (paper §7.4) ─────────────────────────
//
// `W_q(a) = X_(N(a)−K+1)`, `N(a) = max(⌈K/a⌉, 2K)`, `K = 10`: the order
// statistic the hold-down clock (`RWM_HOLDDOWN_Q`) evaluates on the
// hole-resolution stream at `a = 1 − q`.

/// The order-statistic exceedance count `K` of the quantile-native window
/// law — cited, not fitted.
///
/// To read the `1 − α` quantile from a window, the window must hold enough
/// samples above it that the reading rests on real order statistics; the
/// classical requirement is `N·min(α, 1−α) ≥ 10` (the same requirement
/// `SIGMA_CAND_WINDOW` cites). `K = 10` takes it as an equality.
///
/// Fixing the exceedance count rather than the window is the design. The
/// realized tail level of `X_(N−K+1)` is exactly `Beta(K, N−K+1)` — the
/// probability-integral transform, valid for any continuous `F` — with
/// relative sampling SD `≈ 1/√K`, so precision is the same at every α, where
/// a fixed window would make the tail's precision far worse than the head's.
/// Raising `K` narrows every CI as `1/√K` and lengthens every window as `K`.
pub const QNATIVE_EXCEEDANCE_K: usize = 10;

/// Declared resource bound, stated outside the law: the deepest window the
/// quantile-native ring will hold. 8192 × 4 B = 32 KiB per path.
///
/// It is `≥ N(0.002) = 5 000`. It binds below `α ≈ 1.2e-3`; at a contract
/// tail of `α = 1e-5` the law declares itself unavailable and the evaluation
/// falls through, which the `law_n` bind-fraction gauge makes visible
/// instead of extrapolating.
pub const QNATIVE_WINDOW_MAX: usize = 8192;

/// The window law: `N(α) = max(⌈K/α⌉, 2K)`.
///
/// * `⌈K/α⌉` is the exceedance clause: the window must carry `K` samples
///   above the quantile for the reading to be an order statistic.
/// * `2K` is the symmetric clause `N·(1−α) ≥ K` made explicit. It is implied
///   for `α ≤ ½` (`N(0.40) = 25`), stated so a floor that rarely binds is not
///   mistaken for a tuned one.
///
/// Returns `None` when the law asks for more than [`QNATIVE_WINDOW_MAX`] —
/// the α at which the direct route is unavailable, a property of α and the
/// declared bound and never a mode.
pub fn qnative_window_n(alpha: f64) -> Option<usize> {
    if !alpha.is_finite() || alpha <= 0.0 || alpha > 1.0 {
        return None;
    }
    let exceedance = (QNATIVE_EXCEEDANCE_K as f64 / alpha).ceil();
    if !exceedance.is_finite() || exceedance > QNATIVE_WINDOW_MAX as f64 {
        return None;
    }
    let n = (exceedance as usize).max(2 * QNATIVE_EXCEEDANCE_K);
    (n <= QNATIVE_WINDOW_MAX).then_some(n)
}

/// The quantile-native recovery round (µs): `W_q(α) = X_(N−K+1)`. One index
/// into a sorted window; no arithmetic beyond the index.
///
/// `window` is the most recent `N(α)` raw samples in arrival order — the
/// caller supplies exactly `N(α)` of them or nothing, so this function cannot
/// silently read a shorter window at a different level. A short window is a
/// different law's output, which is why the caller falls through rather than
/// truncating.
///
/// Floored at the timer granularity for the same information-availability
/// reason the derived recovery round is.
pub fn qnative_recovery_round_us(window: &[u32], alpha: f64) -> Option<u64> {
    let n = qnative_window_n(alpha)?;
    if window.len() < n {
        return None;
    }
    // Only the freshest `n` count — a longer slice would read a different
    // level of a longer window.
    let mut s: Vec<u32> = window[window.len() - n..].to_vec();
    // `X_(n−K+1)` with 1-based order statistics ⇒ index `n − K` 0-based:
    // exactly `K − 1` samples lie strictly above it and it is the K-th from
    // the top. Chosen over `X_(n−K)` so `E[τ] = K/(n+1) ≈ α` rather than
    // `≈ 1.1·α`.
    let idx = n.saturating_sub(QNATIVE_EXCEEDANCE_K).min(n - 1);
    // Selection, not a sort: a declared cost bound on the sender. A full
    // sort is `O(N log N)` ≈ 12× the work at `N(0.002) = 5 000`;
    // `select_nth_unstable` is `O(N)` average and returns the same order
    // statistic.
    let (_, nth, _) = s.select_nth_unstable(idx);
    Some((*nth as u64).max(TIMER_GRANULARITY_US))
}

/// How many `W` samples [`QuantileClockGauge`] retains. Bounded because the
/// clock is evaluated on every recovery-timer tick and a rep can produce tens
/// of thousands; 4096 is enough for a p95 that is stable to well inside the
/// spread this gauge exists to report.
pub const QCLK_SAMPLE_CAP: usize = 4096;

/// The `[QCLK]` gauge — the realized recovery clock, as a distribution.
///
/// The tail-sweep timeout and the receiver's hole-refresh cadence are what
/// the engine will wait, per evaluation; this records them as a distribution
/// (mean, p05/p50/p95, min/max) beside the mean srtt and the mean measured σ
/// that fed the evaluations. It is the one per-run readout of the shipped
/// clamp's realized cadence (the `RWM_REFRESH_FLOOR_US` reachability reads
/// it).
///
/// Observation only: no gate of its own, no control flow, no wire byte.
pub(crate) struct QuantileClockGauge {
    site: &'static str,
    evals: u64,
    w_sum: f64,
    w_min: u64,
    w_max: u64,
    srtt_sum: f64,
    sigma_sum: f64,
    sigma_n: u64,
    /// Uniformly decimated `W` samples, µs. Deterministic — no RNG, so two
    /// runs of one binary on one log produce the same quantiles.
    samples: Vec<u32>,
    /// Keep every `stride`-th evaluation; doubles each time the store fills,
    /// halving what is held. Uniform over the whole run rather than biased to
    /// its warm-up, which is where a reservoir-free prefix would sit.
    stride: u64,
    seen: u64,
}

impl QuantileClockGauge {
    pub(crate) fn new(site: &'static str) -> Self {
        Self {
            site,
            evals: 0,
            w_sum: 0.0,
            w_min: u64::MAX,
            w_max: 0,
            srtt_sum: 0.0,
            sigma_sum: 0.0,
            sigma_n: 0,
            samples: Vec::new(),
            stride: 1,
            seen: 0,
        }
    }

    /// One recovery-clock evaluation. `w_us` is the cadence the engine will
    /// actually use, computed by the caller through the same function the
    /// engine uses — never recomputed here, so this can never report a clock
    /// the engine did not run.
    pub(crate) fn record(&mut self, w_us: u64, srtt_us: u64, sigma_us: Option<u64>) {
        self.evals += 1;
        if let Some(sg) = sigma_us {
            self.sigma_sum += sg as f64;
            self.sigma_n += 1;
        }
        self.w_sum += w_us as f64;
        self.w_min = self.w_min.min(w_us);
        self.w_max = self.w_max.max(w_us);
        self.srtt_sum += srtt_us as f64;
        if self.seen % self.stride == 0 {
            self.samples.push(w_us.min(u32::MAX as u64) as u32);
            if self.samples.len() >= QCLK_SAMPLE_CAP {
                let mut i = 0;
                self.samples.retain(|_| {
                    i += 1;
                    i % 2 == 1
                });
                self.stride = self.stride.saturating_mul(2);
            }
        }
        self.seen += 1;
    }

    /// How many recovery-clock evaluations this gauge has seen. Zero means
    /// the evaluation site was never reached — the reachability question, not
    /// a value.
    pub(crate) fn evals(&self) -> u64 {
        self.evals
    }

    fn quantile(sorted: &[u32], q: f64) -> u64 {
        crate::monitor::quantile::nearest_rank(sorted, q) as u64
    }

    /// The `[QCLK]` line this gauge would emit right now.
    pub(crate) fn line(&self) -> String {
        let mut s = self.samples.clone();
        s.sort_unstable();
        let mean = |sum: f64, n: u64| if n == 0 { 0.0 } else { sum / n as f64 };
        format!(
            "[QCLK] site={} evals={} kept={} \
             w_us_mean={:.1} w_us_p05={} w_us_p50={} w_us_p95={} \
             w_us_min={} w_us_max={} srtt_us_mean={:.1} sigma_us_mean={:.1}/n{} \
             fa_class={:.4}",
            self.site,
            self.evals,
            s.len(),
            mean(self.w_sum, self.evals),
            Self::quantile(&s, 0.05),
            Self::quantile(&s, 0.50),
            Self::quantile(&s, 0.95),
            if self.w_min == u64::MAX { 0 } else { self.w_min },
            self.w_max,
            mean(self.srtt_sum, self.evals),
            mean(self.sigma_sum, self.sigma_n),
            self.sigma_n,
            // The sacrificial trailing constant: stderr has two writers and a
            // `tracing` write can land inside a gauge line's last field, so
            // the last field is a constant the parser can lose.
            RACK_SPURIOUS_BUDGET,
        )
    }
}

impl Drop for QuantileClockGauge {
    fn drop(&mut self) {
        // A run that never evaluated a recovery clock stays silent — the same
        // rule `[RACK]` uses, so an absent line can only be read as an
        // unreached evaluation site and never as an unset gate.
        if self.evals > 0 {
            crate::readout!("{}", self.line());
        }
    }
}
