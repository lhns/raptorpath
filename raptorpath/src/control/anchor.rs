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
    /// (bucket close instant, symbol count, bucket Δt seconds) — surviving
    /// samples. Per-bucket rate = count/Δt (the `rate()` windowed-max);
    /// the gap-robust windowed mean (`mean_rate()`) is Σcount/ΣΔt.
    samples: VecDeque<(Instant, u64, f64)>,
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
            self.samples.push_back((now, self.bucket_count, dt));
            // Expire outside quarantine only (hold-through-disturbance).
            if let Some(cutoff) = now.checked_sub(Duration::from_secs_f64(window_s(srtt))) {
                while self.samples.front().is_some_and(|&(t, _, _)| t < cutoff) {
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
            iter.map(|&(_, c, dt)| c as f64 / dt).fold(f64::NAN, f64::max)
        } else {
            let cutoff = now.checked_sub(Duration::from_secs_f64(window_s(srtt)));
            iter.filter(|&&(t, _, _)| cutoff.map_or(true, |c| t >= c))
                .map(|&(_, c, dt)| c as f64 / dt)
                .fold(f64::NAN, f64::max)
        };
        if max.is_nan() {
            None
        } else {
            Some(max)
        }
    }

    /// The GAP-ROBUST RATCHETED MEAN send rate (symbols/s): the MAX of the
    /// two rolling half-window means Σ count / Σ Δt over the surviving
    /// buckets (clock-gap buckets were discarded on entry; during
    /// quarantine the window is anchored to the last surviving bucket —
    /// hold-through-disturbance, like `rate()`). None until the surviving
    /// buckets span at least a quarter window of measured send time (a
    /// shorter mean is burst-weighted — the honest cold-start is the
    /// caller's fallback law).
    ///
    /// Why this statistic (goal-gate "Ship The Wins 1" amendment, both
    /// smoke-named defects):
    /// - NOT the per-bucket windowed-MAX (`rate()`, the paced-A*
    ///   statistic): an ADMISSION-GATED sender legitimately bursts whole
    ///   buckets at emission speed on every store-refill (SACK-release
    ///   flood, boot-cap ramp) and the max latches the burst — the
    ///   measured sr=53k-vs-8.9k defect. A half-window mean (≫ one refill
    ///   cycle) is time-normalized: burst concentration cannot inflate it.
    /// - NOT the plain full-window mean: the cap this anchor feeds limits
    ///   the send process itself, so an un-ratcheted mean inherits the
    ///   anchor⇄cap circularity (cap dip → carried rate dip → mean dip —
    ///   the measured 3588→938 oscillation). The max-of-two-halves holds
    ///   the pre-dip half's rate for up to a half window — the same
    ///   escape BBR's BtlBw max-filter provides over its interval-mean
    ///   samples, on the rolling two-half-window pattern already
    ///   established by `EchoRatioMin`.
    pub fn mean_rate(&self, now: Instant, srtt: Duration) -> Option<f64> {
        let last = self.samples.back()?.0;
        let w = window_s(srtt);
        // Anchor the window to `now` normally; to the last surviving
        // bucket during quarantine (hold-through-disturbance).
        let end = if self.in_quarantine(now) { last } else { now };
        let mid = end.checked_sub(Duration::from_secs_f64(w / 2.0));
        let start = end.checked_sub(Duration::from_secs_f64(w));
        let (mut c_old, mut d_old, mut c_new, mut d_new) = (0u64, 0.0f64, 0u64, 0.0f64);
        for &(t, c, dt) in self.samples.iter() {
            if start.is_some_and(|s| t < s) {
                continue; // outside the window (deque expiry is lazy)
            }
            if mid.map_or(true, |m| t >= m) {
                c_new += c;
                d_new += dt;
            } else {
                c_old += c;
                d_old += dt;
            }
        }
        // Warm gate: a mean is a rate only once it averages over enough
        // measured send time (¼ window ≈ several refill cycles).
        if d_old + d_new < w / 4.0 {
            return None;
        }
        let r_old = if d_old > 0.0 { c_old as f64 / d_old } else { f64::NAN };
        let r_new = if d_new > 0.0 { c_new as f64 / d_new } else { f64::NAN };
        let r = r_old.max(r_new); // NaN-ignoring: max(NaN, x) = x
        if r.is_finite() { Some(r) } else { None }
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

// --- Delivery-clocked rate anchor (goal-gate "Ship The Wins 1b") -------------

/// Bound on the send-cursor log (entries). At ≈SRTT-scale ack cadence and
/// one entry per `charge_in_flight` call, a few thousand entries span many
/// windows; older entries can only serve samples already taken.
const SEND_LOG_MAX: usize = 8192;

/// DELIVERY-CLOCKED rate anchor — the BBR `GenerateRateSample` statistic as a
/// STANDALONE SHADOW estimator, on AGGREGATE per-path cursors.
///
/// WHY IT EXISTS (goal-gate "Ship The Wins 1" verdict, paper §16.36): the
/// N ≥ 2 pooled-store cap needs a rate input that is not self-referential.
/// The ack-interval windowed-max (`CopaState::record_delivery`) reads burst
/// peaks — ack bunching collapses Δt and the max latches it (×10-class
/// over-read, ×3.4–3.7 worse under the est-cadence ack clock). The
/// send-interval ratcheted mean ([`SendRateAnchor::mean_rate`]) reads the
/// CAP'S OWN SHADOW — a cap-limited sender cannot emit faster than the cap
/// allows, so the anchor can never ratchet above the operating point and the
/// pool pins the store at its own ceiling (measured: win = cap, sweeps 8–21).
///
/// A delivery clock escapes both, and it is the ONLY source here bounded by
/// delivered-packet PHYSICS rather than by the sender's own admission gate:
///   - it cannot over-read, because Δt is `max(send_elapsed, ack_elapsed)` —
///     a batched ack shrinks `ack_elapsed` but the sender's own send spacing
///     survives (Cardwell/Cheng, draft-cheng-iccrg-delivery-rate-estimation),
///     and samples spanning less than one RTprop are rejected outright;
///   - it CAN read above the current cap, because during any store-refill /
///     SACK-release burst the wire delivers at the BOTTLENECK rate, and the
///     max filter (≈10·RTprop) holds that measurement — exactly BBR's escape
///     from the cwnd⇄BtlBw circularity.
///
/// AGGREGATE, NOT PER-SEQ (the honest limitation, stated in the code):
/// the plain N ≥ 2 stack has no per-seq delivery attribution without the
/// `CopaFeed` (whose sampling-only mode carries the measured −22…−27 Mbit c7
/// composition price and the §16.34 `src_inflight` leak — both must stay
/// unreachable). So this sampler keys on CUMULATIVE COUNTS: a monotone send
/// cursor (`(instant, cum_sent)` log fed at every wire send) and a monotone
/// ACCOUNTED cursor advanced by delivered + LOST at each ack. Advancing by
/// losses is what keeps the two cursors aligned — a lost symbol left the wire
/// too; only deliveries enter the numerator. `send_elapsed` is then the send
/// instant at the new accounted cursor minus the send instant at the previous
/// SAMPLE's cursor: the same quantity BBR reads from its per-packet snapshot,
/// resolved through the FIFO send order instead of a seq key.
///
/// SUB-RTprop SAMPLES ARE REJECTED **AND ACCUMULATED**: unlike BBR (whose
/// per-packet snapshots make the next sample naturally wider), the cursor is
/// NOT reset on rejection, so the next ack's sample spans the whole interval
/// since the last ACCEPTED sample. A drain burst is therefore averaged over
/// ≥ one pipe, never latched.
///
/// Hygiene (ADR-0061, rule 2): an ack interval that is a clock gap measured
/// the stall — discard it and quarantine the release flood behind it; during
/// quarantine the window does NOT expire (hold-through-disturbance, as in
/// [`SendRateAnchor`]).
///
/// Pure/injectable-time state: no env reads, no globals. Its ONLY consumer is
/// the N ≥ 2 pool law (`PathState::pool_rate_anchor`); no cwnd/pacing
/// consumer can reach it.
pub struct DeliveryRateAnchor {
    /// Cumulative symbols handed to the wire on this path.
    sent_total: u64,
    /// (send instant, cumulative `sent_total` AFTER that send) — the FIFO
    /// send cursor the delivery cursor is resolved against.
    send_log: VecDeque<(Instant, u64)>,
    /// Cumulative delivered + lost (symbols ACCOUNTED FOR — the cursor that
    /// tracks `sent_total`).
    acct_total: u64,
    /// Cumulative DELIVERED symbols (the sample numerator's source).
    delivered_total: u64,
    /// `acct_total` / `delivered_total` / send instant / wall instant at the
    /// last ACCEPTED sample — the sample's left edge.
    last_acct: u64,
    last_delivered: u64,
    last_send_instant: Option<Instant>,
    last_sample_time: Option<Instant>,
    /// (sample instant, delivery rate sym/s) inside the max-filter window.
    samples: VecDeque<(Instant, f64)>,
    quarantine_until: Option<Instant>,
    gaps: u64,
    /// Samples discarded: clock gaps + quarantine flood.
    discarded: u64,
    /// Samples rejected for spanning < one RTprop (accumulated, not lost).
    rej_short: u64,
    /// Accepted samples (DIAG: proves the mechanism executed).
    accepted: u64,
}

/// Max-filter window for the delivery anchor: BBR's ≈10·RTprop BtlBw filter,
/// clamped [1 s, 10 s] — long enough to hold a burst measurement across a
/// cap-limited quiet stretch, short enough that a genuine rate change is not
/// pinned for the whole transfer. Identical to `CopaState::rs_on_delivered`'s
/// window (the same statistic, deliberately).
fn deliv_window_s(rtprop: Option<Duration>) -> f64 {
    rtprop
        .map(|r| (r.as_secs_f64() * 10.0).clamp(1.0, 10.0))
        .unwrap_or(10.0)
}

impl DeliveryRateAnchor {
    pub fn new() -> Self {
        Self {
            sent_total: 0,
            send_log: VecDeque::new(),
            acct_total: 0,
            delivered_total: 0,
            last_acct: 0,
            last_delivered: 0,
            last_send_instant: None,
            last_sample_time: None,
            samples: VecDeque::new(),
            quarantine_until: None,
            gaps: 0,
            discarded: 0,
            rej_short: 0,
            accepted: 0,
        }
    }

    fn in_quarantine(&self, now: Instant) -> bool {
        self.quarantine_until.is_some_and(|q| now < q)
    }

    /// Record `n` symbols handed to the wire at `now` (every wire send on this
    /// path: source, redundant, retransmit — the true send process).
    pub fn on_send(&mut self, now: Instant, n: u64) {
        if n == 0 {
            return;
        }
        self.sent_total += n;
        self.send_log.push_back((now, self.sent_total));
        while self.send_log.len() > SEND_LOG_MAX {
            self.send_log.pop_front();
        }
    }

    /// The send instant of the symbol at cumulative index `idx`: the first log
    /// entry whose cumulative count reaches it. None when `idx` is beyond the
    /// live send log (not yet sent) or has fallen off its front (evicted).
    fn send_instant_at(&self, idx: u64) -> Option<Instant> {
        if idx == 0 {
            return None;
        }
        // The log is sorted by cumulative count (monotone) — binary search.
        let v = self.send_log.as_slices();
        let find = |s: &[(Instant, u64)]| -> Option<Instant> {
            let p = s.partition_point(|&(_, c)| c < idx);
            s.get(p).map(|&(t, _)| t)
        };
        find(v.0).or_else(|| find(v.1))
    }

    /// One delivery event on this path: `delivered` symbols confirmed received
    /// and `lost` symbols confirmed gone (both advance the accounted cursor;
    /// only `delivered` enters the numerator). `rtprop` is the path's measured
    /// minimum RTT — the sample's minimum span and the filter window's scale.
    ///
    /// `srtt` sizes the clock-gap expectation for the ack clock (the ack
    /// cadence is SRTT-scale by construction: one ack per batch in flight).
    pub fn on_delivery(
        &mut self,
        now: Instant,
        delivered: u64,
        lost: u64,
        rtprop: Option<Duration>,
        srtt: Duration,
    ) {
        if delivered == 0 && lost == 0 {
            return;
        }
        self.acct_total += delivered + lost;
        self.delivered_total += delivered;

        let Some(prev_time) = self.last_sample_time else {
            // First event: establish the sample's left edge only.
            self.last_sample_time = Some(now);
            self.last_acct = self.acct_total;
            self.last_delivered = self.delivered_total;
            self.last_send_instant = self.send_instant_at(self.acct_total);
            return;
        };

        let ack_elapsed = now.saturating_duration_since(prev_time).as_secs_f64();
        // Hygiene rule 2: an ack interval that spans a process stall measured
        // the stall. Discard, quarantine the release flood, and RE-BASE the
        // left edge (the pre-gap span is not a measurement of this path).
        if is_clock_gap(ack_elapsed, srtt.as_secs_f64().max(1e-3)) {
            self.gaps += 1;
            self.discarded += 1;
            self.quarantine_until =
                Some(now + Duration::from_secs_f64(ack_elapsed.min(QUARANTINE_CAP_S)));
            self.rebase(now);
            return;
        }
        if self.in_quarantine(now) {
            // Release-flood event: a backlog drained at burst speed measures
            // the queue, not the path.
            self.discarded += 1;
            self.rebase(now);
            return;
        }

        let cur_send = self.send_instant_at(self.acct_total);
        // send_elapsed spans the SEND spacing of the symbols this sample
        // covers; it is what makes a batched ack un-inflatable.
        let send_elapsed = match (self.last_send_instant, cur_send) {
            (Some(a), Some(b)) => b.saturating_duration_since(a).as_secs_f64(),
            _ => 0.0,
        };
        let interval = send_elapsed.max(ack_elapsed);
        // Reject-and-ACCUMULATE: a sample spanning less than one RTprop cannot
        // estimate a bottleneck (the classic ack-aggregation artefact). Leave
        // the left edge in place so the NEXT event spans a wider interval.
        let min_interval = rtprop.map(|r| r.as_secs_f64()).unwrap_or(0.001).max(0.001);
        if interval < min_interval {
            self.rej_short += 1;
            return;
        }
        let d = self.delivered_total.saturating_sub(self.last_delivered);
        if d == 0 {
            // Nothing delivered in the span (pure loss): no rate sample, but
            // the span is spent — re-base so the next one starts clean.
            self.rebase(now);
            return;
        }
        let rate = d as f64 / interval;
        self.accepted += 1;
        self.samples.push_back((now, rate));
        let cutoff = now.checked_sub(Duration::from_secs_f64(deliv_window_s(rtprop)));
        while self
            .samples
            .front()
            .is_some_and(|&(t, _)| cutoff.is_some_and(|c| t < c))
        {
            self.samples.pop_front();
        }
        self.rebase(now);
    }

    /// Move the sample's left edge to the current cursors at `now`.
    fn rebase(&mut self, now: Instant) {
        self.last_sample_time = Some(now);
        self.last_acct = self.acct_total;
        self.last_delivered = self.delivered_total;
        self.last_send_instant = self.send_instant_at(self.acct_total);
    }

    /// The windowed-MAX delivery rate (symbols/s), or None before the first
    /// accepted sample. During quarantine the window does NOT expire — the
    /// last known-good rate holds through the disturbance.
    pub fn rate(&self, now: Instant, rtprop: Option<Duration>) -> Option<f64> {
        let iter = self.samples.iter();
        let max = if self.in_quarantine(now) {
            iter.map(|&(_, r)| r).fold(f64::NAN, f64::max)
        } else {
            let cutoff = now.checked_sub(Duration::from_secs_f64(deliv_window_s(rtprop)));
            iter.filter(|&&(t, _)| cutoff.is_none_or(|c| t >= c))
                .map(|&(_, r)| r)
                .fold(f64::NAN, f64::max)
        };
        if max.is_nan() || max <= 0.0 {
            None
        } else {
            Some(max)
        }
    }

    /// (accepted samples, short-rejected, gaps, discarded) — the DIAG gauges
    /// that prove the mechanism executed and how the guards fired.
    pub fn stats(&self) -> (u64, u64, u64, u64) {
        (self.accepted, self.rej_short, self.gaps, self.discarded)
    }
}

impl Default for DeliveryRateAnchor {
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
        assert!(a.mean_rate(Instant::now(), SRTT).is_none());
    }

    /// Goal-gate "Ship The Wins 1" amendment law: an ADMISSION-GATED sender
    /// legitimately bursts whole buckets at emission speed on every
    /// store-refill (SACK-release flood) — the windowed-MAX latches the
    /// burst (the measured sr=53k-vs-8.9k smoke defect), while the
    /// GAP-ROBUST WINDOWED MEAN reads the true carried rate. Both
    /// statistics from the SAME buckets: max for the paced A* consumer,
    /// mean for the pooled store-cap law.
    #[test]
    fn send_rate_anchor_mean_is_refill_burst_immune_where_the_max_latches() {
        let t0 = Instant::now();
        let mut a = SendRateAnchor::new();
        let mut t = feed_steady(&mut a, t0, 1000.0, 1.0, SRTT);
        // 5 cycles of [165 ms steady @1000/s + 35 ms refill burst @40k/s]:
        // carried truth = (165 + 1400) / 0.2 s ≈ 7 825 sym/s.
        for _ in 0..5 {
            t = feed_steady(&mut a, t, 1000.0, 0.165, SRTT);
            for _ in 0..1400 {
                a.on_send(t, 1, SRTT);
                t += Duration::from_micros(25);
            }
        }
        let max = a.rate(t, SRTT).expect("max live");
        let mean = a.mean_rate(t, SRTT).expect("mean live");
        let truth = (165.0 + 1400.0) / 0.2;
        assert!(
            max > 3.0 * truth,
            "the windowed-max latches the refill burst (the defect): max={max} truth={truth}"
        );
        // The ratcheted mean carries a bounded upward bias (max of two
        // half-window means, ≤ ~1.6× under this burst geometry — the
        // anti-circularity ratchet) but must stay in the carried-truth
        // CLASS where the max reads the 5× burst peak.
        assert!(
            mean > 0.6 * truth && mean < 2.0 * truth,
            "the ratcheted mean stays in the carried-truth class: mean={mean} truth={truth}"
        );
    }

    /// The anti-circularity ratchet law: a self-inflicted rate DIP (the
    /// cap→rate→mean feedback the smoke measured as the 3588→938
    /// oscillation) must not drag the anchor down within a half window —
    /// the previous half's mean holds the pre-dip rate.
    #[test]
    fn send_rate_anchor_mean_ratchets_through_a_self_dip() {
        let t0 = Instant::now();
        let mut a = SendRateAnchor::new();
        let mut t = feed_steady(&mut a, t0, 8000.0, 1.0, SRTT);
        let before = a.mean_rate(t, SRTT).expect("warm");
        assert!((before - 8000.0).abs() / 8000.0 < 0.15);
        // A cap-induced dip: rate collapses ×4 for ~one half window
        // (0.24 s at SRTT 60 ms).
        t = feed_steady(&mut a, t, 2000.0, 0.2, SRTT);
        let during = a.mean_rate(t, SRTT).expect("still warm");
        assert!(
            during > 0.75 * before,
            "the pre-dip half must hold the anchor up: before={before} during={during}"
        );
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

    // ----- DeliveryRateAnchor (goal-gate "Ship The Wins 1b" arm A) ---------

    const RTP: Duration = Duration::from_millis(20);
    const DSRTT: Duration = Duration::from_millis(60);

    /// Drive a steady send+deliver stream at `rate` sym/s for `secs`, acking
    /// every `ack_every` symbols one RTprop after they were sent.
    fn deliv_steady(
        a: &mut DeliveryRateAnchor,
        mut t: Instant,
        rate: f64,
        secs: f64,
        ack_every: u64,
    ) -> Instant {
        let step = Duration::from_secs_f64(ack_every as f64 / rate);
        let end = t + Duration::from_secs_f64(secs);
        while t < end {
            a.on_send(t, ack_every);
            a.on_delivery(t + RTP, ack_every, 0, Some(RTP), DSRTT);
            t += step;
        }
        t
    }

    /// LAW 1 (the honesty guard): a BATCHED ack cannot inflate the delivery
    /// anchor. This is the exact defect the legacy ack-interval statistic has
    /// — Δdelivered/Δt_ack with a collapsed Δt reads ×10-class (measured
    /// btlbw 339–500k vs ≈8–12k truth at c7). `max(send_elapsed, ack_elapsed)`
    /// plus the ≥ RTprop reject-and-accumulate guard must hold the read at
    /// the true rate.
    #[test]
    fn delivery_anchor_is_immune_to_ack_bunching() {
        let t0 = Instant::now();
        let mut a = DeliveryRateAnchor::new();
        let truth = 8_000.0; // sym/s — the c7 per-path truth class
        // Sends are paced at truth; deliveries arrive in ONE clump per 100 ms
        // (400 symbols acked at once — the ack-aggregation model).
        let mut t = t0;
        let mut sent_since = 0u64;
        let step = Duration::from_secs_f64(1.0 / truth);
        let end = t0 + Duration::from_secs_f64(3.0);
        let mut next_ack = t0 + Duration::from_millis(100);
        while t < end {
            a.on_send(t, 1);
            sent_since += 1;
            if t >= next_ack {
                // The whole clump lands in one instant: ack_elapsed for the
                // NEXT sample is ~0 across the clump's own symbols.
                a.on_delivery(t, sent_since, 0, Some(RTP), DSRTT);
                sent_since = 0;
                next_ack = t + Duration::from_millis(100);
            }
            t += step;
        }
        let r = a.rate(t, Some(RTP)).expect("anchor live");
        assert!(
            r < truth * 2.0,
            "batched acks must not inflate the delivery anchor: got {r} vs truth {truth}"
        );
        assert!(
            r > truth * 0.5,
            "and it must still READ the rate: got {r} vs truth {truth}"
        );
    }

    /// LAW 2 — **THE mechanism law of attempt 2** (goal-gate "Ship The Wins
    /// 1b"): the delivery clock can ratchet ABOVE the cap-limited MEAN rate,
    /// which is exactly what a send-interval mean structurally cannot do
    /// (paper §16.36: "a send-derived rate cannot ratchet above the
    /// cap-limited carried rate — no delivery physics").
    ///
    /// Model: an admission-gated sender whose long-run MEAN is 4 000 sym/s,
    /// emitted as store-refill BURSTS that the wire carries at its 16 000
    /// sym/s bottleneck (burst 40 ms on, 120 ms idle). The `SendRateAnchor`
    /// ratcheted mean must read the ≈4 000 mean (its documented, correct
    /// behaviour); the delivery anchor must read the ≈16 000 BOTTLENECK —
    /// and hold it across the idle stretch via the max filter.
    #[test]
    fn delivery_anchor_ratchets_above_the_cap_limited_mean_the_send_anchor_reads() {
        let t0 = Instant::now();
        let mut d = DeliveryRateAnchor::new();
        let mut s = SendRateAnchor::new();
        let bottleneck = 16_000.0;
        let burst_s = 0.040;
        let idle_s = 0.120;
        let per_burst = (bottleneck * burst_s) as u64; // 640 symbols
        let mut t = t0;
        for _ in 0..24 {
            // The burst is emitted AND delivered at the bottleneck rate: 8
            // sub-events across the 40 ms, one RTprop later on the wire.
            let sub = per_burst / 8;
            let sub_dt = Duration::from_secs_f64(burst_s / 8.0);
            for k in 0..8u32 {
                let ts = t + sub_dt * k;
                d.on_send(ts, sub);
                s.on_send(ts, sub, DSRTT);
                d.on_delivery(ts + RTP, sub, 0, Some(RTP), DSRTT);
            }
            t += Duration::from_secs_f64(burst_s + idle_s);
        }
        let mean =
            (per_burst * 24) as f64 / (24.0 * (burst_s + idle_s)); // ≈4 000
        let sr = s.mean_rate(t, DSRTT).expect("send anchor warm");
        let dr = d.rate(t, Some(RTP)).expect("delivery anchor live");
        assert!(
            sr < mean * 2.0,
            "the SEND anchor reads the cap-limited mean (attempt 1's binder): sr={sr} vs mean={mean}"
        );
        assert!(
            dr > sr * 2.0 && dr > bottleneck * 0.5,
            "THE claim: the DELIVERY clock ratchets to the bottleneck the send mean cannot see — dr={dr}, sr={sr}, bottleneck={bottleneck}"
        );
        // …and it is still physics-bounded: it may not exceed the wire.
        assert!(
            dr < bottleneck * 1.6,
            "delivery anchor must not over-read the bottleneck: dr={dr}"
        );
    }

    /// LAW 3 (hygiene, ADR-0061 rule 2): an ack interval spanning a process
    /// stall is DISCARDED and the release flood behind it quarantined — the
    /// anchor holds its pre-gap value instead of reading the flood as link
    /// rate (the `SendRateAnchor` flood-poison law, on the delivery clock).
    #[test]
    fn delivery_anchor_discards_the_stall_and_holds_through_the_flood() {
        let t0 = Instant::now();
        let mut a = DeliveryRateAnchor::new();
        let t = deliv_steady(&mut a, t0, 8_000.0, 1.0, 200);
        let before = a.rate(t, Some(RTP)).expect("anchor live");
        // A 1-s process stall, then the whole backlog delivered instantly.
        let t_gap = t + Duration::from_secs(1);
        a.on_send(t_gap, 8_000);
        a.on_delivery(t_gap, 8_000, 0, Some(RTP), DSRTT);
        // …and the flood behind it (three clumps in 20 ms).
        let mut tf = t_gap;
        for _ in 0..3 {
            tf += Duration::from_millis(7);
            a.on_send(tf, 4_000);
            a.on_delivery(tf, 4_000, 0, Some(RTP), DSRTT);
        }
        let after = a.rate(tf, Some(RTP)).expect("anchor holds through the gap");
        assert!(
            after <= before * 1.05,
            "flood must not move the anchor: before={before}, after={after}"
        );
        let (_, _, gaps, disc) = a.stats();
        assert_eq!(gaps, 1, "exactly one gap detected");
        assert!(disc >= 2, "the flood behind it discarded too, got {disc}");
    }

    /// LAW 4: LOSSES advance the accounted cursor but never the numerator —
    /// that alignment is what lets an aggregate (non-per-seq) cursor resolve
    /// send spacing. A path losing half its symbols must read ≈ half the
    /// send rate, not the send rate.
    #[test]
    fn delivery_anchor_counts_deliveries_and_accounts_losses() {
        let t0 = Instant::now();
        let mut a = DeliveryRateAnchor::new();
        let sent_rate = 8_000.0;
        let mut t = t0;
        let step = Duration::from_millis(25);
        let per = (sent_rate * 0.025) as u64; // 200 per event
        for _ in 0..80 {
            a.on_send(t, per);
            // Half delivered, half lost.
            a.on_delivery(t + RTP, per / 2, per / 2, Some(RTP), DSRTT);
            t += step;
        }
        let r = a.rate(t, Some(RTP)).expect("anchor live");
        assert!(
            r > sent_rate * 0.3 && r < sent_rate * 0.75,
            "delivered rate ≈ half the send rate under 50% loss, got {r}"
        );
    }

    /// LAW 5: a cold anchor has NO opinion (the honest cold start — the
    /// caller's fallback law is the only defensible value), and a single
    /// sub-RTprop event is rejected-and-ACCUMULATED rather than latched.
    #[test]
    fn delivery_anchor_cold_start_and_short_sample_accumulation() {
        let t0 = Instant::now();
        let mut a = DeliveryRateAnchor::new();
        assert!(a.rate(t0, Some(RTP)).is_none(), "cold anchor has no rate");
        a.on_send(t0, 100);
        a.on_delivery(t0, 100, 0, Some(RTP), DSRTT);
        assert!(a.rate(t0, Some(RTP)).is_none(), "first event only seeds");
        // Ten events 1 ms apart: each spans ≪ RTprop (20 ms) so each is
        // rejected; the cursor is NOT reset, so the eleventh — landing past
        // one RTprop — reads the whole accumulated span at the true rate.
        let mut t = t0;
        for _ in 0..10 {
            t += Duration::from_millis(1);
            a.on_send(t, 8);
            a.on_delivery(t, 8, 0, Some(RTP), DSRTT);
        }
        let (ok, short, _, _) = a.stats();
        assert_eq!(ok, 0, "no sample may be accepted below one RTprop");
        assert_eq!(short, 10, "each short event rejected AND accumulated");
        t += Duration::from_millis(15);
        a.on_send(t, 120);
        a.on_delivery(t, 120, 0, Some(RTP), DSRTT);
        let r = a.rate(t, Some(RTP)).expect("accumulated sample accepted");
        let (ok, _, _, _) = a.stats();
        assert_eq!(ok, 1, "exactly one accepted sample");
        // 8·10 + 120 = 200 symbols over 25 ms ⇒ 8 000 sym/s.
        assert!(
            (r - 8_000.0).abs() / 8_000.0 < 0.25,
            "the accumulated span reads the true rate, got {r}"
        );
    }
}
