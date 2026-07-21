//! Anchor hygiene (branch `feat/anchor-hygiene`): the shared sampling-layer
//! discipline for every measured anchor the transport derives control values
//! from (A*/M* span law, BtlBw/BDP, RTT floors).
//!
//! The principle (paper "Anchor Hygiene" section; goal-gate ledger):
//! an anchor is trustworthy only if
//!   1. it is SEEDED from measured sends/samples (a windowed-max of real
//!      samples — never a static default surviving warm-up),
//!   2. its samples EXCLUDE scheduler clock gaps (a sample whose interval
//!      spans a process stall measures the stall, not the link; detect it and
//!      DISCARD it, don't average it in), and
//!   3. its floors/backstops EXPIRE (a floor that outlives its min-window is
//!      a constant wearing a floor's clothes).
//!
//! This module factors the clock-gap detector ONCE (`is_clock_gap` + the
//! process-clock `StallWitness`) and provides the windowed-max send-rate
//! anchor (`SendRateAnchor`) that replaces the cold 2-s-interval α=0.125 EWMA
//! behind the unified span law's A* (the defect measured in goal-gate
//! "Unified Decoder" COLLAPSE ATTRIBUTION: A* pinned at 1 for ~10 s, then
//! flood-poisoned 1→38 by the post-stall release burst).
//!
//! Everything here is pure/injectable-time state — no env reads, no globals —
//! so each consumer gates it behind its own `RWM_*` flag and the shipped
//! default stays byte-identical.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// A sample interval this many times the expected/typical interval is a
/// clock gap (process stall, timer starvation), not a measurement.
const GAP_FACTOR: f64 = 8.0;
/// Absolute floor for the gap threshold: sub-250 ms hiccups are jitter-class
/// and stay in the sample stream (the estimators' own robustness handles
/// them); ≥ 250 ms beyond the expected cadence is stall-class.
const GAP_ABS_FLOOR_S: f64 = 0.25;
/// Post-gap quarantine is the gap length itself (the release flood drains
/// the backlog the stall built, so it is at most gap-long), capped at 2 s.
const QUARANTINE_CAP_S: f64 = 2.0;

/// THE gap predicate, factored once (hygiene rule 2). `expected_s` is the
/// consumer's notion of a typical inter-sample interval (the fixed tick
/// period for the stall witness, the bucket duration for the send-rate
/// anchor).
pub fn is_clock_gap(interval_s: f64, expected_s: f64) -> bool {
    interval_s > (GAP_FACTOR * expected_s).max(GAP_ABS_FLOOR_S)
}

/// PROCESS-clock stall witness — the correct detector for the post-stall
/// estimator poisoning (fix 4 / `RWM_CLOCK_GAP`).
///
/// WHY NOT the ack-arrival clock: at high-RTT lossy cells, ack silences of
/// 0.5–3 s are NORMAL protocol behavior (in-order frontier waves, deficit
/// rounds at 2·RTprop) — an arrival-clock detector reads every recovery
/// quiet period as a "stall" and quarantines exactly the ack wave that
/// carries the true delivered rate (MEASURED on the r200 gen L0 rung:
/// gapd 9/5578 with both median and p90 cadence statistics — a discard
/// storm during healthy transfer). A whole-process scheduler stall — the
/// COLLAPSE ATTRIBUTION trigger — freezes the tokio timer wheel itself, so
/// the witness is a fixed-cadence timer tick: a tick interval ≫ the tick
/// period is a PROCESS stall (nothing else can delay a timer that far);
/// samples processed during the stall's release flood (one quarantine =
/// min(gap, 2 s)) are the poisoned ones (echo RTTs that measured the stall,
/// ack-interval Δt collapsed by the flood) and are discarded at the feed
/// sites. Ack silences with a live process never trip it.
///
/// Lock-free (atomics): ticked by a dedicated 50-ms interval task, consulted
/// on the ack-processing paths.
#[derive(Debug)]
pub struct StallWitness {
    /// Monotonic epoch for the no-arg (`*_now`) convenience methods.
    epoch: Instant,
    /// Last tick, µs on the caller's monotonic epoch.
    last_tick_us: std::sync::atomic::AtomicU64,
    /// Quarantine end, µs on the same epoch (0 = none).
    quarantine_until_us: std::sync::atomic::AtomicU64,
    /// Stalls detected (DIAG).
    gaps: std::sync::atomic::AtomicU64,
    /// Samples reported discarded via `note_discard` (DIAG).
    discarded: std::sync::atomic::AtomicU64,
}

/// The witness's expected tick period (seconds) — the `on_tick` cadence the
/// spawner must honor.
pub const STALL_TICK_S: f64 = 0.05;

impl StallWitness {
    pub fn new() -> Self {
        Self {
            epoch: Instant::now(),
            last_tick_us: std::sync::atomic::AtomicU64::new(0),
            quarantine_until_us: std::sync::atomic::AtomicU64::new(0),
            gaps: std::sync::atomic::AtomicU64::new(0),
            discarded: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn now_us(&self) -> u64 {
        self.epoch.elapsed().as_micros() as u64
    }

    /// Timer-task tick on the witness's own monotonic epoch.
    pub fn tick_now(&self) {
        self.on_tick(self.now_us());
    }

    /// Feed-site query on the witness's own monotonic epoch.
    pub fn quarantined_now(&self) -> bool {
        self.in_quarantine(self.now_us())
    }

    /// Timer-task tick at `now_us` (monotonic µs). Detects a process stall
    /// when the tick interval is a gap against the FIXED tick period.
    pub fn on_tick(&self, now_us: u64) {
        use std::sync::atomic::Ordering;
        let last = self.last_tick_us.swap(now_us, Ordering::Relaxed);
        if last == 0 {
            return;
        }
        let dt_s = now_us.saturating_sub(last) as f64 / 1e6;
        if is_clock_gap(dt_s, STALL_TICK_S) {
            self.gaps.fetch_add(1, Ordering::Relaxed);
            let q_s = dt_s.min(QUARANTINE_CAP_S);
            self.quarantine_until_us
                .fetch_max(now_us + (q_s * 1e6) as u64, Ordering::Relaxed);
        }
    }

    /// True while samples must be discarded (stall release flood).
    pub fn in_quarantine(&self, now_us: u64) -> bool {
        now_us < self.quarantine_until_us.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Record that a feed site discarded a sample (DIAG accounting).
    pub fn note_discard(&self) {
        self.discarded.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// (stalls detected, samples discarded) — DIAG gauges.
    pub fn stats(&self) -> (u64, u64) {
        use std::sync::atomic::Ordering;
        (self.gaps.load(Ordering::Relaxed), self.discarded.load(Ordering::Relaxed))
    }
}

impl Default for StallWitness {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-global stall witness (fix 4, `RWM_CLOCK_GAP`): a scheduler stall
/// is process-wide by definition, so ONE witness serves every engine and
/// estimator in the process (the "shared sampling layer" — the detector is
/// factored once, not scattered). None when the gate is off (shipped
/// default: zero cost, no task spawned, every feed site byte-identical).
/// The env gate is read once at first touch (process-global cache, the
/// `copa_wire_active` pattern).
pub fn stall_witness() -> Option<&'static StallWitness> {
    static W: std::sync::OnceLock<Option<StallWitness>> = std::sync::OnceLock::new();
    W.get_or_init(|| {
        // DEFAULT ON (2026-07-21, "Consolidation" battery: bulk cells inert
        // within sigma on both seeds, tail crown unregressed, the post-stall
        // poisoning fix wins at the realtime family — goal-gate).
        if crate::config::anchor_gate_default("RWM_CLOCK_GAP", true) {
            Some(StallWitness::new())
        } else {
            None
        }
    })
    .as_ref()
}

/// Windowed-max SEND-rate anchor (hygiene rules 1+2 for the span law's A*).
///
/// Sends are bucketed on the sender's own clock (bucket ≈ SRTT/2, clamped
/// [5 ms, 250 ms]); each closed bucket yields one rate sample
/// count/Δt, and the anchor is the MAX over samples in a ~8·SRTT window —
/// the same statistic family as BBR's BtlBw filter and §16.15/§16.17's
/// windowed-max delivered rate, applied to the send process the A* law
/// actually models (W = rate·D symbols of trailing span).
///
/// Seeding: the first bucket closes ~SRTT/2 after the first send, so the
/// anchor is live within ONE RTT of stream start (vs the 2-s α=0.125 EWMA's
/// ~10 s crawl — defect (i) of the collapse attribution).
///
/// Clock gaps: a bucket whose Δt is a gap (`is_clock_gap` against the
/// bucket duration) measured the stall — it is discarded and a quarantine
/// (gap length, capped) swallows the release-flood buckets behind it
/// (defect (ii): flood-poison A* 1→38). During quarantine the window does
/// NOT expire: the anchor holds the last known-good rate through the
/// disturbance instead of collapsing to "no sample".
#[derive(Debug)]
pub struct SendRateAnchor {
    bucket_start: Option<Instant>,
    bucket_count: u64,
    /// (bucket close instant, rate sym/s) — surviving samples.
    samples: VecDeque<(Instant, f64)>,
    quarantine_until: Option<Instant>,
    gaps: u64,
    discarded: u64,
}

fn bucket_dur_s(srtt: Duration) -> f64 {
    (srtt.as_secs_f64() / 2.0).clamp(0.005, 0.25)
}

fn window_s(srtt: Duration) -> f64 {
    (srtt.as_secs_f64() * 8.0).clamp(0.5, 10.0)
}

impl SendRateAnchor {
    pub fn new() -> Self {
        Self {
            bucket_start: None,
            bucket_count: 0,
            samples: VecDeque::new(),
            quarantine_until: None,
            gaps: 0,
            discarded: 0,
        }
    }

    fn in_quarantine(&self, now: Instant) -> bool {
        self.quarantine_until.is_some_and(|q| now < q)
    }

    /// Record `n` symbols sent at `now`. `srtt` sizes the bucket/window.
    pub fn on_send(&mut self, now: Instant, n: u64, srtt: Duration) {
        let dur = bucket_dur_s(srtt);
        let Some(start) = self.bucket_start else {
            self.bucket_start = Some(now);
            self.bucket_count = n;
            return;
        };
        let dt = now.saturating_duration_since(start).as_secs_f64();
        if dt < dur {
            self.bucket_count += n;
            return;
        }
        // Close the bucket at `now`; the closing send starts the next one.
        if is_clock_gap(dt, dur) {
            // The bucket spans a stall: discard it, quarantine the flood.
            self.gaps += 1;
            self.discarded += 1;
            self.quarantine_until =
                Some(now + Duration::from_secs_f64(dt.min(QUARANTINE_CAP_S)));
        } else if self.in_quarantine(now) {
            // Release-flood bucket: a backlog drained at burst rate measures
            // the queue, not the send process. Discard.
            self.discarded += 1;
        } else {
            self.quarantine_until = None;
            self.samples.push_back((now, self.bucket_count as f64 / dt));
            // Expire outside quarantine only (hold-through-disturbance).
            if let Some(cutoff) = now.checked_sub(Duration::from_secs_f64(window_s(srtt))) {
                while self.samples.front().is_some_and(|&(t, _)| t < cutoff) {
                    self.samples.pop_front();
                }
            }
        }
        self.bucket_start = Some(now);
        self.bucket_count = n;
    }

    /// The windowed-max send rate (symbols/s), or None before the first
    /// surviving bucket (the honest cold-start: the caller's clamp floor —
    /// A* ≥ 1 — is the only defensible value until a send interval has been
    /// MEASURED).
    pub fn rate(&self, now: Instant, srtt: Duration) -> Option<f64> {
        let iter = self.samples.iter();
        let max = if self.in_quarantine(now) {
            // Hold the pre-gap window through the disturbance.
            iter.map(|&(_, r)| r).fold(f64::NAN, f64::max)
        } else {
            let cutoff = now.checked_sub(Duration::from_secs_f64(window_s(srtt)));
            iter.filter(|&&(t, _)| cutoff.map_or(true, |c| t >= c))
                .map(|&(_, r)| r)
                .fold(f64::NAN, f64::max)
        };
        if max.is_nan() {
            None
        } else {
            Some(max)
        }
    }

    /// (gaps detected, buckets discarded) — DIAG gauges.
    pub fn stats(&self) -> (u64, u64) {
        (self.gaps, self.discarded)
    }
}

impl Default for SendRateAnchor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRTT: Duration = Duration::from_millis(60);

    /// Feed a steady stream at `rate` sym/s from `t` for `secs`, returning
    /// the instant after the last send.
    fn feed_steady(
        a: &mut SendRateAnchor,
        mut t: Instant,
        rate: f64,
        secs: f64,
        srtt: Duration,
    ) -> Instant {
        let step = Duration::from_secs_f64(1.0 / rate);
        let end = t + Duration::from_secs_f64(secs);
        while t < end {
            a.on_send(t, 1, srtt);
            t += step;
        }
        t
    }

    #[test]
    fn gap_predicate_thresholds() {
        // Typical ack cadence 5 ms: 40 ms is jitter (below the abs floor),
        // 300 ms is a gap.
        assert!(!is_clock_gap(0.040, 0.005));
        assert!(is_clock_gap(0.300, 0.005));
        // Slow cadence 2 s (report tick): 3 s is NOT a gap (< 8×median),
        // 20 s is.
        assert!(!is_clock_gap(3.0, 2.0));
        assert!(is_clock_gap(20.0, 2.0));
    }

    #[test]
    fn send_rate_anchor_seeds_from_first_measured_sends() {
        let t0 = Instant::now();
        let mut a = SendRateAnchor::new();
        // 150 sym/s (the c3-1200B realtime stream class). One SRTT of sends
        // must be enough for a live, truthful anchor — the A* ≈ rate·D law
        // needs no 10-s warm-up (defect (i)).
        let t = feed_steady(&mut a, t0, 150.0, 0.062, SRTT);
        let r = a.rate(t, SRTT).expect("anchor live within ~1 RTT");
        assert!(
            (r - 150.0).abs() / 150.0 < 0.35,
            "seeded rate ≈ truth, got {r}"
        );
        // And converges tight with a few more buckets.
        let t = feed_steady(&mut a, t, 150.0, 0.5, SRTT);
        let r = a.rate(t, SRTT).unwrap();
        assert!((r - 150.0).abs() / 150.0 < 0.15, "converged rate, got {r}");
    }

    #[test]
    fn send_rate_anchor_flood_poison_injection_does_not_move_the_max() {
        // THE poison test from the collapse attribution: steady stream, a
        // synthetic 1-s clock gap (process stall), then the whole backlog
        // released as an instantaneous flood. A* = clamp(rate·D, 1, W) must
        // not move: the anchor may not read the flood as link rate.
        let t0 = Instant::now();
        let mut a = SendRateAnchor::new();
        let t = feed_steady(&mut a, t0, 150.0, 2.0, SRTT);
        let baseline = a.rate(t, SRTT).unwrap();
        assert!((baseline - 150.0).abs() / 150.0 < 0.15);

        // 1-s stall: no sends. Then 150 backlogged sends flood out in ~15 ms
        // (loopback burst), then steady resumes.
        let mut tf = t + Duration::from_secs(1);
        for _ in 0..150 {
            a.on_send(tf, 1, SRTT);
            tf += Duration::from_micros(100);
        }
        // Through the flood and the quarantine, the anchor must hold the
        // pre-gap value — neither spike (poison) nor collapse (dark).
        let during = a.rate(tf, SRTT).expect("anchor holds through the gap");
        assert!(
            (during - baseline).abs() / baseline < 0.2,
            "1-s gap + flood moved the anchor: {baseline} -> {during}"
        );
        let (gaps, discarded) = a.stats();
        assert!(gaps >= 1, "the gap must be DETECTED");
        assert!(discarded >= 1);

        // Steady resumes: back to truth after the quarantine.
        let t_end = feed_steady(&mut a, tf, 150.0, 2.5, SRTT);
        let after = a.rate(t_end, SRTT).unwrap();
        assert!(
            (after - 150.0).abs() / 150.0 < 0.2,
            "post-quarantine rate re-measures truth, got {after}"
        );
    }

    #[test]
    fn send_rate_anchor_no_samples_is_none() {
        let a = SendRateAnchor::new();
        assert!(a.rate(Instant::now(), SRTT).is_none());
    }

    #[test]
    fn stall_witness_quarantines_process_stalls_not_ack_silences() {
        let w = StallWitness::new();
        let tick_us = (STALL_TICK_S * 1e6) as u64;
        let mut t: u64 = 1_000_000;
        // Steady 50-ms ticks: never quarantined — an ack silence (no samples
        // arriving) with a LIVE process leaves the witness untouched, so a
        // recovery quiet period can never be misread as a stall (the r200
        // discard-storm lesson).
        for _ in 0..100 {
            w.on_tick(t);
            assert!(!w.in_quarantine(t));
            t += tick_us;
        }
        // A 1-s process stall freezes the timer wheel: the next tick arrives
        // 1 s late and opens a quarantine covering the release flood…
        t += 1_000_000;
        w.on_tick(t);
        assert!(w.in_quarantine(t), "post-stall flood is quarantined");
        assert!(w.in_quarantine(t + 900_000), "quarantine ≈ the gap length");
        // …which EXPIRES (hygiene rule 3): min(gap, cap) later. The timer
        // keeps ticking THROUGH the quarantine (the process is live again),
        // so no cascade fires.
        let q_end = t + 1_000_000;
        while t < q_end + 500_000 {
            t += tick_us;
            w.on_tick(t);
        }
        assert!(!w.in_quarantine(t), "quarantine expired, ticks steady");
        let (gaps, _) = w.stats();
        assert_eq!(gaps, 1, "exactly the one stall detected — no cascade");
    }

    #[test]
    fn stall_witness_quarantine_is_capped() {
        let w = StallWitness::new();
        let mut t: u64 = 1_000_000;
        for _ in 0..10 {
            w.on_tick(t);
            t += 50_000;
        }
        // A 10-s stall quarantines only QUARANTINE_CAP_S, not 10 s.
        t += 10_000_000;
        w.on_tick(t);
        assert!(w.in_quarantine(t + 1_900_000));
        assert!(!w.in_quarantine(t + 2_100_000));
    }

}
