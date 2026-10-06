//! Loss rate estimation using Bayesian EWMA + BOCD.
//!
//! Combines:
//! - Beta-Binomial conjugate prior for principled uncertainty quantification
//! - EWMA for fast adaptation to changing conditions
//! - Burst detection for non-iid loss patterns
//! - BOCD (Bayesian Online Changepoint Detection) for regime-aware prediction
//! - Separate TX/RX loss tracking for asymmetric path estimation: TX is the
//!   OUTGOING direction (fed by this endpoint's sender from its own ack
//!   deltas), RX the INCOMING one (fed by this endpoint's receiver from its
//!   `PathBatchTracker`). One `Scheduler` per endpoint holds both.

use super::changepoint::BayesianChangepoint;
use super::gilbert_elliott::GilbertElliottEstimator;
use raptorpath_math::normal_quantile;
use std::time::{Duration, Instant};

/// The carried late-arrival credit: one `(expected, received)` count pair
/// in, one fed pair out, with
///
/// ```text
///   raw  = expected − received                  (signed; < 0 on a late arrival)
///   fed  = max(0, raw − held)                   (the loss this pair feeds)
///   held' = held − (raw − fed)                  (≥ 0: excess received is kept)
///   out  = (expected, expected − fed)
/// ```
///
/// so the fed loss is never negative, the fed `received` never exceeds
/// `expected`, and Σfed = Σraw + held: cumulative fed loss equals cumulative
/// `(expected − received)` whenever the credit has drained, and exceeds it
/// by at most the credit still held (bounded by the reorder depth). It
/// replaces the clamp `received.min(expected)`, which discarded the excess
/// and so fed every reordered symbol as a loss.
#[derive(Debug, Default, Clone, Copy)]
pub struct LossCredit {
    held: u64,
}

impl LossCredit {
    /// Apply the law to one pair. See the type doc.
    pub fn apply(&mut self, expected: u64, received: u64) -> (u64, u64) {
        if received >= expected {
            self.held += received - expected;
            return (expected, expected);
        }
        let raw = expected - received;
        let used = raw.min(self.held);
        self.held -= used;
        (expected, expected - (raw - used))
    }

    /// Credit currently held (received in excess of expected, not yet
    /// matched against a later loss).
    pub fn held(&self) -> u64 {
        self.held
    }
}

/// The incoming direction's estimator (threading Q2: the RX half). Owned
/// by the receiver task per path; fed by every arrived batch
/// ([`Self::record_rx_batch`], feed D) and every data arrival
/// ([`Self::record_arrival`]). Its two outputs — the incoming loss EWMA and
/// the RFC 3550 jitter — are published per path (`PathStats`) for the
/// sender's mirror ([`LossEstimator::set_rx_mirror`]) and the PathReport.
/// The math is the one `LossEstimator` always ran on these fields, moved.
#[derive(Debug, Clone)]
pub struct RxEstimator {
    /// EWMA smoothing factor — the same 0.1 the TX loss EWMA uses.
    alpha: f64,
    /// EWMA of RX loss rate, fed by [`Self::record_rx_batch`].
    rx_ewma_loss: f64,
    /// The late-arrival credit of the RX feed (its own; never the TX one).
    rx_credit: LossCredit,
    /// Interarrival jitter (RTCP-style, RFC 3550 A.8)
    jitter: f64,
    /// Last packet arrival timestamp for jitter calculation
    last_arrival_us: Option<u64>,
    /// Last packet send timestamp for jitter calculation
    last_send_ts_us: Option<u64>,
}

impl Default for RxEstimator {
    fn default() -> Self {
        Self::new()
    }
}

impl RxEstimator {
    pub fn new() -> Self {
        Self {
            alpha: 0.1,
            // RX path: weak prior
            rx_ewma_loss: 0.0,
            rx_credit: LossCredit::default(),
            jitter: 0.0,
            last_arrival_us: None,
            last_send_ts_us: None,
        }
    }

    /// One arrived batch's `(expected, received)` on the incoming direction
    /// (see [`LossEstimator::record_rx_batch`]).
    pub fn record_rx_batch(&mut self, expected: u32, received: u32) {
        let (e, r) = self.rx_credit.apply(expected as u64, received as u64);
        if e == 0 {
            return;
        }
        let batch_loss = (e - r) as f64 / e as f64;
        self.rx_ewma_loss = self.alpha * batch_loss + (1.0 - self.alpha) * self.rx_ewma_loss;
    }

    /// See [`LossEstimator::update_rx_loss`] (test-only feed).
    pub fn update_rx_loss(&mut self, nacks_sent: u32, acks_received: u32) {
        if nacks_sent == 0 {
            return;
        }
        let lost = nacks_sent.saturating_sub(acks_received);
        let batch_loss = lost as f64 / nacks_sent as f64;

        // EWMA update
        self.rx_ewma_loss = self.alpha * batch_loss + (1.0 - self.alpha) * self.rx_ewma_loss;
    }

    /// RX path loss rate (point estimate): the incoming direction's EWMA.
    pub fn rx_loss_rate(&self) -> f64 {
        self.rx_ewma_loss
    }

    /// `(1 - ε_rx)²` (see [`LossEstimator::nack_effectiveness`]).
    pub fn nack_effectiveness(&self) -> f64 {
        let rx_loss = self.rx_ewma_loss;
        (1.0 - rx_loss).powi(2)
    }

    /// Record arrival for jitter calculation (RFC 3550 A.8).
    pub fn record_arrival(&mut self, send_ts_us: u64, arrival_us: u64) {
        if let (Some(last_send), Some(last_arrival)) = (self.last_send_ts_us, self.last_arrival_us) {
            // D(i,j) = (Rj - Ri) - (Sj - Si)
            let transit_diff = (arrival_us as i64 - last_arrival as i64)
                - (send_ts_us as i64 - last_send as i64);
            let d = transit_diff.unsigned_abs() as f64;
            // J(i) = J(i-1) + (|D(i,j)| - J(i-1)) / 16
            self.jitter += (d - self.jitter) / 16.0;
        }
        self.last_send_ts_us = Some(send_ts_us);
        self.last_arrival_us = Some(arrival_us);
    }

    /// Current jitter estimate in microseconds (RFC 3550 style).
    pub fn jitter_us(&self) -> f64 {
        self.jitter
    }
}

/// Per-path loss estimator.
#[derive(Debug)]
pub struct LossEstimator {
    // --- TX path loss (forward direction) ---
    /// EWMA of TX loss rate
    tx_ewma_loss: f64,
    /// EWMA smoothing factor (higher = more responsive)
    alpha: f64,
    /// Beta distribution parameters (Bayesian prior) for TX path
    beta_a: f64, // successes (received)
    beta_b: f64, // failures (lost)
    /// Decay factor for Beta params to forget old data
    beta_decay: f64,

    // --- The incoming direction (threading Q2) ---
    /// The RX-direction estimator: the incoming loss EWMA and the RFC 3550
    /// arrival jitter. On a production endpoint this copy is the TX half's
    /// MIRROR of the receiver's own [`RxEstimator`] (written by
    /// [`Self::set_rx_mirror`] from the receiver's published atomics, so the
    /// sender's readers — `nack_effectiveness`, the jitter fallback of
    /// `rtt_jitter_us`, the PathReport build — read the receiver's values
    /// without sharing its state); a unit test that feeds it directly
    /// through the delegating methods below sees the same math.
    rx: RxEstimator,

    /// RTT estimation (EWMA)
    ewma_rtt: Duration,
    rtt_alpha: f64,
    /// `RWM_MSTAR_ANCHOR`: seed the RTT EWMA from the first measured sample
    /// instead of blending real samples into the 50-ms DEFAULT_SRTT-class
    /// constructor seed (hygiene rule 1, ADR-0061: an anchor is seeded from
    /// measurements; a seed surviving warm-up leaves the M* floor stale).
    rtt_seed_from_sample: bool,
    /// True once a real RTT sample has been recorded (seed consumed).
    rtt_seeded: bool,
    /// True once any real RTT sample has been recorded, independent of the
    /// hygiene seeding gate — i.e. whether `ewma_rtt` is a measurement at all
    /// or still the 50-ms DEFAULT_SRTT-class constructor constant. Read by
    /// `rtt_measured()`; see the hygiene rule-1 note on `rtt_seed_from_sample`.
    rtt_sampled: bool,

    /// Throughput estimation (bytes/sec EWMA)
    ewma_throughput: f64,

    /// Burst loss detection
    consecutive_losses: u32,
    burst_threshold: u32,
    in_burst: bool,

    /// Gilbert-Elliott HMM for bursty loss estimation
    ge: GilbertElliottEstimator,

    /// BOCD for regime-aware prediction
    bocd: BayesianChangepoint,

    /// `RWM_EST_CADENCE`: run the BOCD heavy update at its design cadence
    /// instead of per message. The detector's own constructor documents
    /// "regime changes every ~100 batches (200 s at 2 s intervals)" — a
    /// batch-cadence model; per message, the window wire calls its
    /// O(MAX_RUN_LENGTH) exp/ln update ~22k×/s per side (~22–26 % of a core
    /// at the single-path throughput wall). With the gate ON, clean
    /// observations accumulate and flush every `EST_HEAVY_CADENCE`; any call
    /// that carries a loss flushes immediately (zero staleness on losses).
    /// EWMA/Beta/burst/GE stay per-call. ON is the default (Stage 3 (d));
    /// `RWM_EST_CADENCE=0` = per-call BOCD.
    est_cadence: bool,
    /// Accumulated (received, lost) counts awaiting the next BOCD flush.
    bocd_acc_received: u64,
    bocd_acc_lost: u64,
    /// Instant of the last BOCD flush (cadence clock).
    bocd_last_flush: Instant,

    /// Bookkeeping
    total_sent: u64,
    total_received: u64,
    last_update: Instant,
}

/// `RWM_EST_CADENCE` heartbeat: clean evidence flushes to the BOCD at this
/// cadence (10 ms ≪ the 100 ms recovery round; ~100 updates/s ≈ the
/// detector's design regime). Not a tuning knob.
const EST_HEAVY_CADENCE: Duration = Duration::from_millis(10);

/// Whether `RWM_EST_CADENCE` is on, read from the process gate resolution.
///
/// Default ON (Stage 3 (d) `FLIP-RECOMMENDED`, status.md §5: not worse at
/// any cell, dual c1 +41 %, sender CPU −12 to −32 %, fed loss vs truth
/// unchanged). History: e84ef1c's flip was the COMPOSED form — the cadence
/// with `RWM_POOL_ANCHOR` riding it — and it failed its symmetric-dual (c7)
/// clause because the send-side pool anchor became the binding cap (send-side
/// anchors cannot ratchet above the cap-limited carried rate). The pool anchor
/// no longer follows this gate; the shipped flip is the cadence alone, the
/// form Stage 3 measured. Emission batching (`RWM_EMIT_BATCH`) composes on top
/// and ships ON as well (status §8 re-run; measured with the cadence ON).
pub(crate) fn est_cadence_active() -> bool {
    crate::gates::get().est_cadence
}

/// The resolve-time read behind [`est_cadence_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_est_cadence() -> bool {
    let on = crate::config::env_flag("RWM_EST_CADENCE", true);
    // Echoed both ways (measurement-discipline rule 15c): with the default ON,
    // the `=0` control arm must still witness that the knob reached the binary.
    if on {
        tracing::info!(
            "estimator heavy-math cadence ACTIVE (RWM_EST_CADENCE: BOCD update at 10 ms/loss-event cadence, accumulated counts)"
        );
    } else {
        tracing::info!(
            "estimator heavy-math cadence OFF (RWM_EST_CADENCE=0: per-call BOCD update)"
        );
    }
    on
}

impl LossEstimator {
    pub fn new() -> Self {
        Self {
            tx_ewma_loss: 0.0,
            alpha: 0.1, // ~10-sample half-life
            // Weak prior: Beta(1,1) = uniform
            beta_a: 1.0,
            beta_b: 1.0,
            beta_decay: 0.995, // slowly forget old observations
            rx: RxEstimator::new(),
            ewma_rtt: Duration::from_millis(50),
            rtt_alpha: 0.125, // standard TCP EWMA
            // Default ON.
            rtt_seed_from_sample: crate::gates::get().mstar_anchor,
            rtt_seeded: false,
            rtt_sampled: false,
            ewma_throughput: 0.0,
            consecutive_losses: 0,
            burst_threshold: 3,
            in_burst: false,
            ge: GilbertElliottEstimator::new(),
            bocd: BayesianChangepoint::default_fec(),
            est_cadence: est_cadence_active(),
            bocd_acc_received: 0,
            bocd_acc_lost: 0,
            bocd_last_flush: Instant::now(),
            total_sent: 0,
            total_received: 0,
            last_update: Instant::now(),
        }
    }

    /// Record that `received` out of `sent` symbols arrived in a batch.
    ///
    /// The Gilbert-Elliott estimator is fed a LUMPED approximation (`lost`
    /// Bad symbols followed by `received` Good ones), which overestimates
    /// burstiness — the conservative direction. Callers that know the true
    /// per-symbol pattern (SACK gaps) should use `record_counts` +
    /// `record_symbol` instead for an unbiased burst estimate.
    pub fn record_batch(&mut self, sent: u32, received: u32) {
        self.record_batch_at(sent, received, Instant::now());
    }

    /// [`Self::record_batch`] on an explicit clock: `now` drives the
    /// `RWM_EST_CADENCE` heartbeat. The engine passes the wall clock (via
    /// `record_batch`); a `MockClock` sim passes its own `now`, or the
    /// heartbeat would fire on wall time and two identical sim runs would
    /// flush the BOCD at different points.
    pub fn record_batch_at(&mut self, sent: u32, received: u32, now: Instant) {
        let lost = sent.saturating_sub(received);
        self.record_counts_at(sent, received, now);

        // Feed Gilbert-Elliott HMM: approximate as `lost` Bad symbols
        // followed by `received` Good symbols within this batch
        for _ in 0..lost {
            self.ge.record_symbol(false);
        }
        for _ in 0..received {
            self.ge.record_symbol(true);
        }
    }

    /// Count-only update (EWMA + Beta + BOCD + burst flag) without the
    /// lumped Gilbert-Elliott approximation. Pair with per-symbol
    /// `record_symbol` calls carrying the actual arrival pattern.
    pub fn record_counts(&mut self, sent: u32, received: u32) {
        self.record_counts_at(sent, received, Instant::now());
    }

    /// [`Self::record_counts`] on an explicit clock (see
    /// [`Self::record_batch_at`]).
    pub fn record_counts_at(&mut self, sent: u32, received: u32, now: Instant) {
        let lost = sent.saturating_sub(received);
        let batch_loss = if sent > 0 {
            lost as f64 / sent as f64
        } else {
            0.0
        };

        // EWMA update
        self.tx_ewma_loss = self.alpha * batch_loss + (1.0 - self.alpha) * self.tx_ewma_loss;

        // Beta-Binomial update with decay
        self.beta_a *= self.beta_decay;
        self.beta_b *= self.beta_decay;
        self.beta_a += received as f64;
        self.beta_b += lost as f64;

        // BOCD update — per-call when the cadence gate is OFF; accumulated +
        // flushed on loss / 10 ms heartbeat when ON (the per-message
        // O(MAX_RUN_LENGTH) update costs 22–26 % of a core at the wall).
        if self.est_cadence {
            self.bocd_acc_received += received as u64;
            self.bocd_acc_lost += lost as u64;
            if lost > 0
                || now.saturating_duration_since(self.bocd_last_flush) >= EST_HEAVY_CADENCE
            {
                self.flush_bocd(now);
            }
        } else {
            self.bocd.update(received, lost);
        }

        // Burst detection
        if lost > 0 {
            self.consecutive_losses += lost;
            if self.consecutive_losses >= self.burst_threshold {
                self.in_burst = true;
            }
        } else {
            self.consecutive_losses = 0;
            self.in_burst = false;
        }

        self.total_sent += sent as u64;
        self.total_received += received as u64;
        self.last_update = now;
    }

    /// Flush the accumulated counts into the BOCD (cadence gate). The
    /// posterior sees the same evidence as the per-call path, batched —
    /// exactly the batch-cadence observation model `default_fec()` was
    /// designed for.
    fn flush_bocd(&mut self, now: Instant) {
        if self.bocd_acc_received > 0 || self.bocd_acc_lost > 0 {
            self.bocd.update(
                self.bocd_acc_received.min(u32::MAX as u64) as u32,
                self.bocd_acc_lost.min(u32::MAX as u64) as u32,
            );
            self.bocd_acc_received = 0;
            self.bocd_acc_lost = 0;
        }
        self.bocd_last_flush = now;
    }

    /// Start the `RWM_EST_CADENCE` heartbeat at `now` (construction starts it
    /// on the wall clock). For a `MockClock` sim feeding
    /// [`Self::record_batch_at`]: without it the first heartbeat lands at a
    /// wall-dependent offset. The engine never calls it.
    #[doc(hidden)]
    pub fn start_cadence_clock_at(&mut self, now: Instant) {
        self.bocd_last_flush = now;
    }

    /// Record one wire-symbol outcome (true = received) into the
    /// Gilbert-Elliott estimator, preserving the true loss interleaving —
    /// e.g., reconstructed from SACK gap patterns (paper §2.6).
    pub fn record_symbol(&mut self, received: bool) {
        self.ge.record_symbol(received);
    }

    /// The receiver's own feed: one arrived batch's `(expected, received)`
    /// from `PathBatchTracker` — loss on the INCOMING direction. Feeds the RX
    /// EWMA only; the TX estimator (EWMA, Beta, BOCD, burst flag, GE, totals),
    /// which this endpoint's sender reads for the OUTGOING direction, is
    /// untouched. A late arrival's `(0, received)` is carried as credit
    /// ([`LossCredit`]); a pair with nothing expected after the credit is no
    /// trial and leaves the EWMA as it is.
    pub fn record_rx_batch(&mut self, expected: u32, received: u32) {
        self.rx.record_rx_batch(expected, received)
    }

    /// Update the RX (reverse path) loss estimate from request/echo counts.
    /// Only tests call it: the wire has no NACK-echo message.
    ///
    /// `nacks_sent`: number of NACKs the receiver sent in this period
    /// `acks_received`: number of those echoed back by the sender
    pub fn update_rx_loss(&mut self, nacks_sent: u32, acks_received: u32) {
        self.rx.update_rx_loss(nacks_sent, acks_received)
    }

    /// RX path loss rate (point estimate): the incoming direction's EWMA.
    pub fn rx_loss_rate(&self) -> f64 {
        self.rx.rx_loss_rate()
    }

    /// NACK effectiveness: probability that a NACK round-trip succeeds.
    /// = (1 - ε_rx)² where ε_rx is the RX path loss rate.
    /// The NACK must survive the reverse path AND the repair must survive the forward path.
    pub fn nack_effectiveness(&self) -> f64 {
        self.rx.nack_effectiveness()
    }

    /// Threading Q2: overwrite the RX-direction values this (TX-half)
    /// estimator reads with the receiver's published ones — the incoming
    /// loss EWMA and the arrival jitter, both µs/fraction as the receiver's
    /// [`RxEstimator`] holds them. The arrival-pair memory stays the
    /// receiver's (a mirror never feeds `record_arrival`).
    pub fn set_rx_mirror(&mut self, rx_loss: f64, jitter_us: f64) {
        self.rx.rx_ewma_loss = rx_loss;
        self.rx.jitter = jitter_us;
    }

    /// Record an RTT measurement.
    pub fn record_rtt(&mut self, rtt: Duration) {
        // Observation only (no behaviour rides on this assignment): from here
        // on `ewma_rtt` contains a measurement, so `rtt_measured()` may
        // report it. Set before the hygiene early-return so both arms record.
        self.rtt_sampled = true;
        // Hygiene rule 1: the first measured sample replaces the
        // constructor seed outright (no blend with the 50-ms constant).
        if self.rtt_seed_from_sample && !self.rtt_seeded {
            self.rtt_seeded = true;
            self.ewma_rtt = rtt;
            return;
        }
        let rtt_secs = rtt.as_secs_f64();
        let old_secs = self.ewma_rtt.as_secs_f64();
        let new_secs = self.rtt_alpha * rtt_secs + (1.0 - self.rtt_alpha) * old_secs;
        self.ewma_rtt = Duration::from_secs_f64(new_secs);
    }

    /// Test hook: force the seed-from-sample gate
    /// without the process-global env (parallel unit tests must not race
    /// env vars).
    #[cfg(test)]
    pub(crate) fn force_anchor_hygiene(&mut self, seed_from_sample: bool) {
        self.rtt_seed_from_sample = seed_from_sample;
    }

    /// Record throughput measurement.
    pub fn record_throughput(&mut self, bytes_per_sec: f64) {
        self.ewma_throughput =
            self.rtt_alpha * bytes_per_sec + (1.0 - self.rtt_alpha) * self.ewma_throughput;
    }

    /// Current TX loss rate estimate (point estimate, EWMA).
    pub fn loss_rate(&self) -> f64 {
        self.tx_ewma_loss
    }

    /// Upper bound of TX loss rate at given confidence level.
    /// Uses the Beta posterior: quantile at (1 - confidence).
    /// This is what we use for computing FEC rate — we want to be conservative.
    pub fn loss_rate_upper(&self, confidence: f64) -> f64 {
        beta_quantile(self.beta_b, self.beta_a, confidence)
    }

    /// Predictive upper bound from BOCD posterior.
    ///
    /// This integrates over run-length uncertainty, producing a tighter
    /// bound than the Beta posterior when in steady state, and a wider
    /// bound during regime changes. This is the margin — no additional
    /// safety factor needed.
    pub fn predictive_loss_upper(&self, confidence: f64) -> f64 {
        if self.bocd.updates() < 5 {
            // Not enough data for BOCD — fall back to Beta upper bound
            return self.loss_rate_upper(confidence);
        }
        self.bocd.predictive_quantile(confidence)
    }

    pub fn rtt(&self) -> Duration {
        self.ewma_rtt
    }

    /// The RTT estimate only if it is a measurement — `None` while
    /// `ewma_rtt` is still the 50-ms DEFAULT_SRTT-class constructor seed.
    ///
    /// `rtt()` cannot distinguish the two, which is exactly the hygiene
    /// rule-1 hazard: a consumer that reads a seed as if it were an anchor
    /// prices an unmeasured path with a constant. This accessor lets a
    /// consumer ask instead (see `PathState::srtt_measured`).
    pub fn rtt_measured(&self) -> Option<Duration> {
        self.rtt_sampled.then_some(self.ewma_rtt)
    }

    pub fn throughput(&self) -> f64 {
        self.ewma_throughput
    }

    /// Record arrival for jitter calculation (RFC 3550 A.8).
    /// `send_ts_us`: sender's timestamp in microseconds.
    /// `arrival_us`: local arrival time in microseconds.
    pub fn record_arrival(&mut self, send_ts_us: u64, arrival_us: u64) {
        self.rx.record_arrival(send_ts_us, arrival_us)
    }

    /// Current jitter estimate in microseconds (RFC 3550 style).
    pub fn jitter_us(&self) -> f64 {
        self.rx.jitter_us()
    }

    /// Cumulative fed loss `1 - sum(received) / sum(sent)` over every
    /// `record_counts` call (0.0 before the first): the feed's long-run
    /// mean, read only by the `[DIAG]` `plc=` gauge.
    pub fn cumulative_loss(&self) -> f64 {
        if self.total_sent == 0 {
            0.0
        } else {
            1.0 - self.total_received as f64 / self.total_sent as f64
        }
    }

    /// Point estimate of loss rate from Beta posterior mean.
    pub fn loss_rate_mean(&self) -> f64 {
        self.beta_b / (self.beta_a + self.beta_b)
    }

    pub fn ge_estimator(&self) -> &GilbertElliottEstimator {
        &self.ge
    }

    pub fn is_in_burst(&self) -> bool {
        self.in_burst
    }
}

impl Default for LossEstimator {
    fn default() -> Self {
        Self::new()
    }
}

/// Approximate Beta distribution quantile using the normal approximation.
/// For Beta(a, b), mean = a/(a+b), var = ab/((a+b)^2(a+b+1))
/// Returns the `p`-th quantile.
fn beta_quantile(a: f64, b: f64, p: f64) -> f64 {
    let mean = a / (a + b);
    let var = (a * b) / ((a + b).powi(2) * (a + b + 1.0));
    let std = var.sqrt();

    // Normal approximation: quantile ≈ mean + z_p * std
    let z = normal_quantile(p);
    (mean + z * std).clamp(0.0, 1.0)
}


impl LossEstimator {
    /// Test-only constructor with the heavy-math cadence forced on
    /// (the env gate is process-global; law tests need both arms).
    #[cfg(test)]
    pub fn new_with_cadence_for_test() -> Self {
        let mut e = Self::new();
        e.est_cadence = true;
        e
    }

    /// Test-only constructor with the heavy-math cadence forced off — the
    /// `RWM_EST_CADENCE=0` default arm (per-call BOCD), env-independent for
    /// the law tests.
    #[cfg(test)]
    pub fn new_per_call_for_test() -> Self {
        let mut e = Self::new();
        e.est_cadence = false;
        e
    }

    /// Test/diag: BOCD updates processed (the cadence mechanism gauge).
    pub fn bocd_updates(&self) -> u64 {
        self.bocd.updates()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RWM_EST_CADENCE default: ships ON — the form Stage 3 (d) measured
    /// (cadence on, pool anchor off). Relies on the test env not exporting
    /// RWM_* overrides, like every engine-default test in this crate.
    #[test]
    fn test_est_cadence_default_on() {
        let est = LossEstimator::new();
        assert!(
            est.est_cadence,
            "RWM_EST_CADENCE ships default ON (Stage 3 (d) FLIP-RECOMMENDED; the              earlier composed flip that failed its c7 clause carried the pool anchor)"
        );
    }

    /// RWM_EST_CADENCE law: with the gate OFF (`=0`, the default arm), every
    /// record_batch performs a per-call BOCD update.
    #[test]
    fn test_est_cadence_off_is_per_call() {
        let mut est = LossEstimator::new_per_call_for_test();
        assert!(!est.est_cadence);
        for _ in 0..7 {
            est.record_batch(1, 1);
        }
        assert_eq!(est.bocd_updates(), 7);
    }

    /// RWM_EST_CADENCE law: clean observations within the 10 ms heartbeat
    /// accumulate (no heavy update); a loss-bearing call flushes
    /// immediately — every informative observation reaches the posterior
    /// on the per-call clock.
    #[test]
    fn test_est_cadence_accumulates_clean_flushes_loss() {
        let mut est = LossEstimator::new_with_cadence_for_test();
        for _ in 0..50 {
            est.record_batch(1, 1); // clean
        }
        assert_eq!(
            est.bocd_updates(),
            0,
            "clean sub-cadence evidence must accumulate"
        );
        est.record_batch(2, 1); // one loss → immediate flush with the backlog
        assert_eq!(est.bocd_updates(), 1, "a loss flushes immediately");
        // The flush carried the whole backlog: 52 sent / 51 received.
        std::thread::sleep(std::time::Duration::from_millis(12));
        est.record_batch(1, 1); // heartbeat elapsed → flush
        assert_eq!(est.bocd_updates(), 2, "the 10 ms heartbeat flushes");
    }

    /// RWM_EST_CADENCE law on an injected clock: the heartbeat reads the
    /// `now` it is fed, never the wall — clean evidence 9.999 ms after the
    /// clock start stays held, at exactly 10 ms it flushes. This is what
    /// keeps a `MockClock` sim deterministic.
    #[test]
    fn test_est_cadence_heartbeat_reads_the_fed_clock() {
        let mut est = LossEstimator::new_with_cadence_for_test();
        let t0 = Instant::now() + Duration::from_secs(3600);
        est.start_cadence_clock_at(t0);
        est.record_batch_at(10, 10, t0 + Duration::from_micros(9_999));
        assert_eq!(est.bocd_updates(), 0, "sub-heartbeat clean evidence must be held");
        est.record_batch_at(10, 10, t0 + EST_HEAVY_CADENCE);
        assert_eq!(est.bocd_updates(), 1, "the fed clock's 10 ms heartbeat flushes");
        est.record_batch_at(10, 10, t0 + EST_HEAVY_CADENCE + Duration::from_millis(5));
        assert_eq!(est.bocd_updates(), 1, "the heartbeat restarts at the flush");
    }

    /// RWM_EST_CADENCE law: the cadenced posterior lands in the same class
    /// as the per-call posterior for a steady lossy stream — the consumers
    /// (predictive_loss_upper → r*) read equal-class values.
    #[test]
    fn test_est_cadence_posterior_equal_class() {
        let mut per_call = LossEstimator::new_per_call_for_test();
        let mut cadenced = LossEstimator::new_with_cadence_for_test();
        // ~5% loss in per-message batches (1 symbol per batch, 1 loss / 20).
        for i in 0..400 {
            let received = if i % 20 == 0 { 0 } else { 1 };
            per_call.record_batch(1, received);
            cadenced.record_batch(1, received);
        }
        let a = per_call.predictive_loss_upper(0.95);
        let b = cadenced.predictive_loss_upper(0.95);
        assert!(
            (a - b).abs() < 0.05,
            "cadenced posterior must stay in the per-call class: {a} vs {b}"
        );
        // And the cheap per-call estimates are bit-identical by construction.
        assert!((per_call.loss_rate() - cadenced.loss_rate()).abs() < 1e-12);
    }

    #[test]
    fn test_loss_estimator_basic() {
        let mut est = LossEstimator::new();

        // Simulate 10% loss
        for _ in 0..100 {
            est.record_batch(100, 90);
        }

        let loss = est.loss_rate();
        assert!((loss - 0.1).abs() < 0.02, "Expected ~10% loss, got {loss}");
    }

    #[test]
    fn test_loss_upper_bound() {
        let mut est = LossEstimator::new();
        for _ in 0..50 {
            est.record_batch(100, 90);
        }

        let upper = est.loss_rate_upper(0.95);
        assert!(upper > est.loss_rate(), "Upper bound should exceed point estimate");
        assert!(upper < 0.3, "Upper bound should be reasonable: {upper}");
    }

    #[test]
    fn test_burst_detection() {
        let mut est = LossEstimator::new();
        est.record_batch(10, 7); // 3 losses
        assert!(est.is_in_burst());
        est.record_batch(10, 10); // no loss
        assert!(!est.is_in_burst());
    }

    #[test]
    fn test_predictive_loss_upper() {
        let mut est = LossEstimator::new();
        for _ in 0..100 {
            est.record_batch(100, 90);
        }

        let pred_upper = est.predictive_loss_upper(0.95);
        assert!(pred_upper > 0.08, "Predictive upper should be above ~10%: {pred_upper}");
        assert!(pred_upper < 0.25, "Predictive upper should be reasonable: {pred_upper}");
    }

    // `RWM_MSTAR_ANCHOR`, hygiene rule 1: the RTT EWMA seeds from the first
    // measured sample; the 50-ms constructor constant never blends into
    // measurements. Control arm: the unseeded law blends
    // 0.875·50 ms + 0.125·sample — the constant leaks for ~20 samples.
    #[test]
    fn rtt_seeds_from_first_measured_sample_under_hygiene() {
        let mut est = LossEstimator::new();
        est.force_anchor_hygiene(true);
        est.record_rtt(Duration::from_millis(200));
        assert_eq!(
            est.rtt(),
            Duration::from_millis(200),
            "first measured sample IS the estimate — no 50-ms blend"
        );
        // Subsequent samples EWMA-blend off the measured seed.
        est.record_rtt(Duration::from_millis(100));
        let expected = 0.875 * 0.200 + 0.125 * 0.100;
        assert!((est.rtt().as_secs_f64() - expected).abs() < 1e-9);

        // Control: the unseeded path blends the constructor seed.
        let mut legacy = LossEstimator::new();
        legacy.force_anchor_hygiene(false);
        legacy.record_rtt(Duration::from_millis(200));
        let blended = 0.875 * 0.050 + 0.125 * 0.200;
        assert!(
            (legacy.rtt().as_secs_f64() - blended).abs() < 1e-9,
            "legacy control keeps the 50-ms seed blend (the defect, preserved \
             for the A/B): got {:?}",
            legacy.rtt()
        );
    }

    /// The carried credit's law at its anchor points: a late arrival
    /// (received > expected) feeds nothing and is held; the next loss is
    /// paid from it; the fed pair never has received > expected; and the
    /// cumulative fed loss equals Σexpected − Σreceived + held.
    #[test]
    fn loss_credit_law_anchor_points() {
        let mut c = LossCredit::default();
        assert_eq!(c.apply(10, 8), (10, 8), "no credit: raw loss fed");
        assert_eq!(c.apply(0, 2), (0, 0), "late arrivals: no trial, 2 held");
        assert_eq!(c.held(), 2);
        assert_eq!(c.apply(10, 9), (10, 10), "1 raw loss paid from the credit");
        assert_eq!(c.apply(10, 7), (10, 8), "3 raw loss, 1 credit left: 2 fed");
        assert_eq!(c.held(), 0);
        // Σe = 30, Σr = 26; fed 2 + 0 + 0 + 2 = 4 = 30 − 26 + 0 held.
    }

    /// The receiver feed reaches the RX EWMA only, with its own credit.
    #[test]
    fn record_rx_batch_feeds_rx_only_and_carries_its_credit() {
        let mut est = LossEstimator::new();
        est.record_rx_batch(1, 1);
        est.record_rx_batch(2, 1); // seq 8 over missing 7
        let after_gap = est.rx_loss_rate();
        assert!(after_gap > 0.0);
        est.record_rx_batch(0, 1); // 7 arrives late: no trial, held
        assert_eq!(est.rx_loss_rate(), after_gap);
        est.record_rx_batch(2, 1); // a real one-batch gap, paid by the credit
        assert!(est.rx_loss_rate() < after_gap, "the credited pair is a clean trial");
        assert_eq!(est.loss_rate(), 0.0);
        assert_eq!(est.cumulative_loss(), 0.0);
    }

    #[test]
    fn test_rx_loss_tracking() {
        let mut est = LossEstimator::new();

        // Simulate 20% RX path loss
        for _ in 0..50 {
            est.update_rx_loss(10, 8);
        }

        let rx_loss = est.rx_loss_rate();
        assert!((rx_loss - 0.2).abs() < 0.05, "Expected ~20% RX loss, got {rx_loss}");

        let effectiveness = est.nack_effectiveness();
        // (1 - 0.2)^2 = 0.64
        assert!((effectiveness - 0.64).abs() < 0.1, "Expected ~0.64 effectiveness, got {effectiveness}");
    }

    #[test]
    fn test_nack_effectiveness_no_loss() {
        let est = LossEstimator::new();
        let eff = est.nack_effectiveness();
        assert!((eff - 1.0).abs() < 0.01, "No RX loss should give ~1.0 effectiveness: {eff}");
    }
}
