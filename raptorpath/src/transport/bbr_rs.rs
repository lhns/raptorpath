//! Burst-robust BBR substrate congestion controller (`RWM_QUIC_CC=bbr_rs`).
//!
//! ADR-0054/0061. An experiment arm, not the default.
//!
//! This is quinn-proto 0.11.14's `congestion::bbr` ported into the engine
//! tree (quinn is Apache-2.0/MIT dual-licensed; the port keeps the upstream
//! structure, gains, mode machine, recovery window, ProbeRTT, and
//! ack-aggregation cwnd term verbatim, so a measurement attributes the
//! changed mechanisms only), with the bandwidth estimator replaced.
//!
//! Why: upstream `bw_estimation.rs` samples both of its rates over
//! adjacent-event gaps — `ack_rate` = this ack
//! event's bytes / gap to the previous ack event, `send_rate` = the last
//! two send events' delta (and `u64::MAX` when they share a timestamp,
//! which quinn's own ≥10-packet pacer bursts and GSO batches guarantee).
//! A token-bucket bottleneck drains its bucket at line rate, delivering
//! ~11-packet clusters; the resulting ack clusters make `ack_rate` read
//! the line rate, `min(send, ack)` admits it (the send side is vacuous
//! inside a burst), and the 10-round windowed-max filter latches a
//! bottleneck estimate ~10x the true link — after which pacing_rate and
//! cwnd (both derived from the estimate) never bind, and the controller
//! sustains overshoot into a shallow buffer indefinitely.
//!
//! The fix is the same one the engine's own delivery anchor received in
//! ADR-0061 (`CopaState::rs_on_delivered`, scheduler — the
//! Cardwell/Cheng draft-cheng-iccrg-delivery-rate-estimation sampler,
//! law-tested by `rate_sample_anchor_reads_true_btlbw_under_aggregation_
//! and_queue`): per-flight rate samples
//!     rate = delta_delivered / max(send_elapsed, ack_elapsed)
//! taken against a snapshot of the delivery state at the acked packet's
//! send time, with samples spanning < RTprop rejected (an interval below
//! one propagation RTT cannot estimate the bottleneck — it is the
//! ack-aggregation artefact itself) and app-limited samples admitted
//! raise-only. The `Controller` trait hands `on_ack` the acked packet's
//! send `Instant`, which keys the snapshot map (packets of one GSO batch
//! share the timestamp and therefore the snapshot — same send state by
//! construction).

//!
//! Window law (status §48, paper §8.4 "The substrate's window law, fixed").
//! Three defects of the port are fixed, each against v1 (`tcp_bbr.c`):
//! - Own ack clock: every ack-side time is a_e, read once per ACK event
//!   (`BbrRsConfig::clock`; quinn's runtime clock in production), because
//!   quinn's `now` is a per-poll stamp that can precede a datagram's arrival.
//!   m̂ is the 10 s windowed minimum of r_e = a_e - max time_sent, not quinn's
//!   lifetime `rtt.min()`; the sampler's interval guard is max(m̂, 1 ms).
//! - Startup grows only while W < T(2.885) + e (the port compared a gain
//!   with a byte count); Drain's window and exit test are both T(1);
//!   ProbeBw is min(T(2) + A, W + a).
//! - ProbeRtt is entered when the min filter expires (and its stamp restarts
//!   on exit), not on a 10 s timer.

use std::any::Any;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::{Duration, Instant};

use quinn_proto::RttEstimator;
use quinn_proto::congestion::{Controller, ControllerFactory, ControllerMetrics};

use rand::{Rng, SeedableRng};

/// quinn-proto's `BASE_DATAGRAM_SIZE` (private upstream; copied).
const BASE_DATAGRAM_SIZE: u64 = 1200;

// ---------------------------------------------------------------------------
// Windowed max filter — verbatim port of quinn-proto bbr/min_max.rs
// (Kathleen Nichols' algorithm; round-count windowed, window = 10 rounds).
// ---------------------------------------------------------------------------

#[derive(Copy, Clone, Debug)]
struct MinMaxSample {
    /// round number, not a timestamp
    time: u64,
    value: u64,
}

impl Default for MinMaxSample {
    fn default() -> Self {
        Self { time: 0, value: 0 }
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct MinMax {
    /// round count, not a timestamp
    window: u64,
    samples: [MinMaxSample; 3],
}

impl Default for MinMax {
    fn default() -> Self {
        Self {
            window: 10,
            samples: [Default::default(); 3],
        }
    }
}

impl MinMax {
    pub(crate) fn get(&self) -> u64 {
        self.samples[0].value
    }

    fn fill(&mut self, sample: MinMaxSample) {
        self.samples.fill(sample);
    }

    pub(crate) fn reset(&mut self) {
        self.fill(Default::default())
    }

    pub(crate) fn update_max(&mut self, current_round: u64, measurement: u64) {
        let sample = MinMaxSample {
            time: current_round,
            value: measurement,
        };

        if self.samples[0].value == 0  /* uninitialised */
            || /* found new max? */ sample.value >= self.samples[0].value
            || /* nothing left in window? */ sample.time - self.samples[2].time > self.window
        {
            self.fill(sample); /* forget earlier samples */
            return;
        }

        if sample.value >= self.samples[1].value {
            self.samples[2] = sample;
            self.samples[1] = sample;
        } else if sample.value >= self.samples[2].value {
            self.samples[2] = sample;
        }

        self.subwin_update(sample);
    }

    fn subwin_update(&mut self, sample: MinMaxSample) {
        let dt = sample.time - self.samples[0].time;
        if dt > self.window {
            self.samples[0] = self.samples[1];
            self.samples[1] = self.samples[2];
            self.samples[2] = sample;
            if sample.time - self.samples[0].time > self.window {
                self.samples[0] = self.samples[1];
                self.samples[1] = self.samples[2];
                self.samples[2] = sample;
            }
        } else if self.samples[1].time == self.samples[0].time && dt > self.window / 4 {
            self.samples[2] = sample;
            self.samples[1] = sample;
        } else if self.samples[2].time == self.samples[1].time && dt > self.window / 2 {
            self.samples[2] = sample;
        }
    }
}

// ---------------------------------------------------------------------------
// The changed mechanism: interval-guarded per-flight delivery-rate sampler
// (replaces upstream bw_estimation.rs; the rs_* design of ADR-0061).
// ---------------------------------------------------------------------------

/// Snapshot of the delivery state at a packet's send time (BBR
/// `SendPacket`): everything needed to form a per-flight rate sample when
/// that packet is acked.
#[derive(Clone, Copy, Debug)]
struct SendRecord {
    /// `C.delivered` at send time (bytes acked so far).
    delivered: u64,
    /// `C.delivered_time` at send time.
    delivered_time: Instant,
    /// `C.first_sent_time` at send time (start of the in-flight send burst).
    first_sent_time: Instant,
}

/// Bound on retained send records — the engine's `RS_MAX_TRACKED` constant
/// (scheduler/mod.rs), reused: records for packets that are lost (never
/// acked) are pruned by the next ack at a later send time anyway; this
/// bounds the pathological no-ack case.
const RS_MAX_TRACKED: usize = 8192;

/// The engine rs_* sampler's absolute interval floor (1 ms) — used until a
/// real RTprop sample exists (scheduler/mod.rs `rs_on_delivered`).
const RS_MIN_INTERVAL: Duration = Duration::from_millis(1);

#[derive(Clone, Debug, Default)]
pub(crate) struct RsBandwidthEstimation {
    /// Total bytes acked (BBR `C.delivered`).
    delivered: u64,
    /// When `delivered` last advanced (BBR `C.delivered_time`).
    delivered_time: Option<Instant>,
    /// Send time of the packet that started the current in-flight send
    /// burst (BBR `C.first_sent_time`); advances to each acked packet's
    /// send time.
    first_sent_time: Option<Instant>,
    /// Outstanding send-state snapshots keyed by packet send `Instant`
    /// (one record per transmit timestamp; a GSO batch shares one).
    sent: BTreeMap<Instant, SendRecord>,
    max_filter: MinMax,
    acked_at_last_window: u64,
    // DIAG counters (test/observability only — never gate control).
    samples_generated: u64,
    samples_rejected_interval: u64,
    samples_rejected_app_limited: u64,
}

impl RsBandwidthEstimation {
    pub(crate) fn on_sent(&mut self, now: Instant, _bytes: u64) {
        // Pipe (re)start: nothing tracked in flight -> the send burst and
        // the delivery clock restart here (BBR: if packets_in_flight == 0,
        // C.first_sent_time = C.delivered_time = P.time_sent).
        if self.sent.is_empty() {
            self.first_sent_time = Some(now);
            self.delivered_time = Some(now);
        }
        let first_sent_time = self.first_sent_time.unwrap_or(now);
        let delivered_time = self.delivered_time.unwrap_or(now);
        // First packet of a same-instant batch wins (shared send state).
        self.sent.entry(now).or_insert(SendRecord {
            delivered: self.delivered,
            delivered_time,
            first_sent_time,
        });
        while self.sent.len() > RS_MAX_TRACKED {
            let Some(&k) = self.sent.keys().next() else {
                break;
            };
            self.sent.remove(&k);
        }
    }

    /// One acked packet (BBR `UpdateRateSample` + `GenerateRateSample`).
    /// `sent` is the acked packet's transmit time (the snapshot key);
    /// `min_interval` is the RTprop guard (caller: `rtt.min()`, floored at
    /// `RS_MIN_INTERVAL`).
    pub(crate) fn on_ack(
        &mut self,
        now: Instant,
        sent: Instant,
        bytes: u64,
        round: u64,
        app_limited: bool,
        min_interval: Duration,
    ) {
        self.delivered += bytes;
        self.delivered_time = Some(now);

        // Snapshot for this packet: exact send instant, or the nearest
        // earlier one (older shared/pruned records still carry valid —
        // strictly older — send state, making the sample span LONGER,
        // never shorter: safe for a max filter).
        let Some((&key, rec)) = self.sent.range(..=sent).next_back() else {
            return;
        };
        let rec = *rec;
        // Advance the burst-window start (BBR: C.first_sent_time =
        // P.sent_time) and drop records strictly older than this packet's
        // (acked or presumed lost — a later ack proves the wire moved on).
        self.first_sent_time = Some(sent);
        self.sent = self.sent.split_off(&key);

        // send_elapsed spans the send spacing from the burst start to this
        // packet; ack_elapsed spans the same deliveries in wall time. The
        // max() is what makes the sample ack-aggregation robust: a batched
        // ack collapses ack_elapsed, but send_elapsed preserves the true
        // spacing (draft-cheng-iccrg-delivery-rate-estimation).
        let send_elapsed = sent.saturating_duration_since(rec.first_sent_time);
        let ack_elapsed = now.saturating_duration_since(rec.delivered_time);
        let interval = send_elapsed.max(ack_elapsed);

        // Interval guard (the load-bearing piece vs upstream): a sample
        // spanning less than one propagation RTT is the aggregation
        // artefact itself — a cluster of queued packets acked over a tiny
        // window reads many multiples of the true link.
        let min_interval = min_interval.max(RS_MIN_INTERVAL);
        if interval < min_interval {
            self.samples_rejected_interval += 1;
            return;
        }
        let delivered_delta = self.delivered.saturating_sub(rec.delivered);
        if delivered_delta == 0 {
            return;
        }
        let Some(rate) = Self::bw_from_delta(delivered_delta, interval) else {
            return;
        };
        // App-limited samples underestimate the pipe (it was starved, not
        // full): admit them only when they raise the filter (BBR
        // §app-limited; the engine rs_* semantics).
        if app_limited && rate <= self.max_filter.get() {
            self.samples_rejected_app_limited += 1;
            return;
        }
        self.samples_generated += 1;
        self.max_filter.update_max(round, rate);
    }

    pub(crate) fn bytes_acked_this_window(&self) -> u64 {
        self.delivered - self.acked_at_last_window
    }

    pub(crate) fn end_acks(&mut self, _current_round: u64, _app_limited: bool) {
        self.acked_at_last_window = self.delivered;
    }

    /// Estimated bottleneck bandwidth, bytes/second.
    pub(crate) fn get_estimate(&self) -> u64 {
        self.max_filter.get()
    }

    pub(crate) const fn bw_from_delta(bytes: u64, delta: Duration) -> Option<u64> {
        let window_duration_ns = delta.as_nanos();
        if window_duration_ns == 0 {
            return None;
        }
        let b_ns = bytes as u128 * 1_000_000_000;
        let bytes_per_second = b_ns / window_duration_ns;
        if bytes_per_second > u64::MAX as u128 {
            return Some(u64::MAX);
        }
        Some(bytes_per_second as u64)
    }

    /// DIAG snapshot: (samples_generated, rejected_interval,
    /// rejected_app_limited, tracked_records). Observation only.
    #[cfg(test)]
    pub(crate) fn rs_diag(&self) -> (u64, u64, u64, usize) {
        (
            self.samples_generated,
            self.samples_rejected_interval,
            self.samples_rejected_app_limited,
            self.sent.len(),
        )
    }
}

// ---------------------------------------------------------------------------
// The controller — verbatim port of quinn-proto bbr/mod.rs `Bbr` except the
// `max_bandwidth` field type and its two call sites (on_sent / on_ack).
// ---------------------------------------------------------------------------

/// Burst-robust BBR congestion controller (see module docs).
#[derive(Debug, Clone)]
pub struct BbrRs {
    config: Arc<BbrRsConfig>,
    current_mtu: u64,
    max_bandwidth: RsBandwidthEstimation,
    acked_bytes: u64,
    mode: Mode,
    loss_state: LossState,
    recovery_state: RecoveryState,
    recovery_window: u64,
    is_at_full_bandwidth: bool,
    pacing_gain: f32,
    high_gain: f32,
    drain_gain: f32,
    cwnd_gain: f32,
    high_cwnd_gain: f32,
    last_cycle_start: Option<Instant>,
    current_cycle_offset: u8,
    init_cwnd: u64,
    min_cwnd: u64,
    prev_in_flight_count: u64,
    exit_probe_rtt_at: Option<Instant>,
    /// m̂: the 10 s windowed minimum of the ack-event loop time r_e (ZERO
    /// until the first event).
    min_rtt: Duration,
    /// t_m: when m̂ was last refreshed (`None` until the first event).
    min_rtt_stamp: Option<Instant>,
    /// The ACK event in progress (first `ack()` .. `on_end_acks`).
    event: Option<AckEvent>,
    /// Smoothed r_e (gain 1/8), observation only: the `[QCC]` `qsr_us`.
    smoothed_loop: Duration,
    /// quinn's own `rtt.min()` as last seen by `on_ack`: observation only
    /// (the `[QCC]` gauge reads it beside `min_rtt`), never read by the law.
    quinn_min_rtt: Duration,
    exiting_quiescence: bool,
    pacing_rate: u64,
    max_acked_packet_number: u64,
    max_sent_packet_number: u64,
    end_recovery_at_packet_number: u64,
    cwnd: u64,
    current_round_trip_end_packet_number: u64,
    round_count: u64,
    bw_at_last_round: u64,
    round_wo_bw_gain: u64,
    ack_aggregation: AckAggregationState,
    random_number_generator: rand::rngs::StdRng,
}

impl BbrRs {
    /// Construct a state using the given `config` and current time `now`
    pub fn new(config: Arc<BbrRsConfig>, current_mtu: u16) -> Self {
        let initial_window = config.initial_window;
        Self {
            config,
            current_mtu: current_mtu as u64,
            max_bandwidth: RsBandwidthEstimation::default(),
            acked_bytes: 0,
            mode: Mode::Startup,
            loss_state: Default::default(),
            recovery_state: RecoveryState::NotInRecovery,
            recovery_window: 0,
            is_at_full_bandwidth: false,
            pacing_gain: K_DEFAULT_HIGH_GAIN,
            high_gain: K_DEFAULT_HIGH_GAIN,
            drain_gain: 1.0 / K_DEFAULT_HIGH_GAIN,
            cwnd_gain: K_DEFAULT_HIGH_GAIN,
            high_cwnd_gain: K_DEFAULT_HIGH_GAIN,
            last_cycle_start: None,
            current_cycle_offset: 0,
            init_cwnd: initial_window,
            min_cwnd: calculate_min_window(current_mtu as u64),
            prev_in_flight_count: 0,
            exit_probe_rtt_at: None,
            min_rtt: Default::default(),
            min_rtt_stamp: None,
            event: None,
            smoothed_loop: Default::default(),
            quinn_min_rtt: Default::default(),
            exiting_quiescence: false,
            pacing_rate: 0,
            max_acked_packet_number: 0,
            max_sent_packet_number: 0,
            end_recovery_at_packet_number: 0,
            cwnd: initial_window,
            current_round_trip_end_packet_number: 0,
            round_count: 0,
            bw_at_last_round: 0,
            round_wo_bw_gain: 0,
            ack_aggregation: AckAggregationState::default(),
            // rand 0.8 API (upstream uses 0.9's from_os_rng): OS-seeded.
            random_number_generator: rand::rngs::StdRng::from_entropy(),
        }
    }

    /// One acked packet. The `Controller::on_ack` body, split out because
    /// `RttEstimator` has no public constructor: unit tests drive this with
    /// `quinn_min` (quinn's `rtt.min()`) directly.
    ///
    /// Every ack-side time is the event clock a_e, read once at the event's
    /// first callback (paper §8.4): quinn's `now` is a per-poll stamp that can
    /// precede the datagram's arrival. `_now` is that stamp, unused.
    pub(crate) fn ack(
        &mut self,
        _now: Instant,
        sent: Instant,
        bytes: u64,
        app_limited: bool,
        quinn_min: Duration,
    ) {
        let a_e = match &mut self.event {
            Some(e) => {
                e.max_sent = e.max_sent.max(sent);
                e.a_e
            }
            None => {
                let a_e = self.config.clk();
                self.event = Some(AckEvent { a_e, max_sent: sent });
                a_e
            }
        };
        // Per-flight sample; the interval guard is max(m̂, 1 ms) (the sampler
        // floors at RS_MIN_INTERVAL).
        self.max_bandwidth
            .on_ack(a_e, sent, bytes, self.round_count, app_limited, self.min_rtt);
        self.acked_bytes += bytes;
        self.quinn_min_rtt = quinn_min;
    }

    /// The `[QCC]` gauge: the controller's own model, read off a clone.
    pub(crate) fn qcc(&self) -> Qcc {
        Qcc {
            mode: match self.mode {
                Mode::Startup => 'S',
                Mode::Drain => 'D',
                Mode::ProbeBw => 'B',
                Mode::ProbeRtt => 'R',
            },
            rt_us: self.min_rtt.as_micros() as u64,
            sr_us: self.smoothed_loop.as_micros() as u64,
            bw: self.max_bandwidth.get_estimate(),
            quinn_min_us: self.quinn_min_rtt.as_micros() as u64,
        }
    }

    /// Outstanding send-state snapshots (what `congestion_state()` clones).
    #[cfg(test)]
    pub(crate) fn tracked_records(&self) -> usize {
        self.max_bandwidth.sent.len()
    }

    /// Test seam: plant the min-RTT model `d`, fresh as of the controller's
    /// clock. Pre-fix this sets `min_rtt` and the ProbeRtt stamp; post-fix it
    /// seeds the windowed filter. Tests using it are identical on both trees.
    #[cfg(test)]
    pub(crate) fn plant_min_rtt(&mut self, d: Duration) {
        self.min_rtt = d;
        self.min_rtt_stamp = Some(self.config.clk());
    }

    fn enter_startup_mode(&mut self) {
        self.mode = Mode::Startup;
        self.pacing_gain = self.high_gain;
        self.cwnd_gain = self.high_cwnd_gain;
    }

    fn enter_probe_bandwidth_mode(&mut self, now: Instant) {
        self.mode = Mode::ProbeBw;
        self.cwnd_gain = K_DERIVED_HIGH_CWNDGAIN;
        self.last_cycle_start = Some(now);
        // Pick a random offset for the gain cycle out of {0, 2..7} range. 1 is
        // excluded because in that case increased gain and decreased gain would not
        // follow each other.
        let mut rand_index = self
            .random_number_generator
            .gen_range(0..K_PACING_GAIN.len() as u8 - 1);
        if rand_index >= 1 {
            rand_index += 1;
        }
        self.current_cycle_offset = rand_index;
        self.pacing_gain = K_PACING_GAIN[rand_index as usize];
    }

    fn update_recovery_state(&mut self, is_round_start: bool) {
        // Exit recovery when there are no losses for a round.
        if self.loss_state.has_losses() {
            self.end_recovery_at_packet_number = self.max_sent_packet_number;
        }
        match self.recovery_state {
            // Enter conservation on the first loss.
            RecoveryState::NotInRecovery if self.loss_state.has_losses() => {
                self.recovery_state = RecoveryState::Conservation;
                // This will cause the |recovery_window| to be set to the
                // correct value in CalculateRecoveryWindow().
                self.recovery_window = 0;
                // Since the conservation phase is meant to be lasting for a whole
                // round, extend the current round as if it were started right now.
                self.current_round_trip_end_packet_number = self.max_sent_packet_number;
            }
            RecoveryState::Growth | RecoveryState::Conservation => {
                if self.recovery_state == RecoveryState::Conservation && is_round_start {
                    self.recovery_state = RecoveryState::Growth;
                }
                // Exit recovery if appropriate.
                if !self.loss_state.has_losses()
                    && self.max_acked_packet_number > self.end_recovery_at_packet_number
                {
                    self.recovery_state = RecoveryState::NotInRecovery;
                }
            }
            _ => {}
        }
    }

    fn update_gain_cycle_phase(&mut self, now: Instant, in_flight: u64) {
        // In most cases, the cycle is advanced after an RTT passes.
        let mut should_advance_gain_cycling = self
            .last_cycle_start
            .map(|last_cycle_start| now.duration_since(last_cycle_start) > self.min_rtt)
            .unwrap_or(false);
        // If the pacing gain is above 1.0, the connection is trying to probe the
        // bandwidth by increasing the number of bytes in flight to at least
        // pacing_gain * BDP.  Make sure that it actually reaches the target, as
        // long as there are no losses suggesting that the buffers are not able to
        // hold that much.
        if self.pacing_gain > 1.0
            && !self.loss_state.has_losses()
            && self.prev_in_flight_count < self.get_target_cwnd(self.pacing_gain)
        {
            should_advance_gain_cycling = false;
        }

        // If pacing gain is below 1.0, the connection is trying to drain the extra
        // queue which could have been incurred by probing prior to it.  If the
        // number of bytes in flight falls down to the estimated BDP value earlier,
        // conclude that the queue has been successfully drained and exit this cycle
        // early.
        if self.pacing_gain < 1.0 && in_flight <= self.get_target_cwnd(1.0) {
            should_advance_gain_cycling = true;
        }

        if should_advance_gain_cycling {
            self.current_cycle_offset = (self.current_cycle_offset + 1) % K_PACING_GAIN.len() as u8;
            self.last_cycle_start = Some(now);
            // Stay in low gain mode until the target BDP is hit.  Low gain mode
            // will be exited immediately when the target BDP is achieved.
            if DRAIN_TO_TARGET
                && self.pacing_gain < 1.0
                && (K_PACING_GAIN[self.current_cycle_offset as usize] - 1.0).abs() < f32::EPSILON
                && in_flight > self.get_target_cwnd(1.0)
            {
                return;
            }
            self.pacing_gain = K_PACING_GAIN[self.current_cycle_offset as usize];
        }
    }

    fn maybe_exit_startup_or_drain(&mut self, now: Instant, in_flight: u64) {
        if self.mode == Mode::Startup && self.is_at_full_bandwidth {
            self.mode = Mode::Drain;
            self.pacing_gain = self.drain_gain;
            self.cwnd_gain = K_DRAIN_CWND_GAIN;
        }
        if self.mode == Mode::Drain && in_flight <= self.get_target_cwnd(1.0) {
            self.enter_probe_bandwidth_mode(now);
        }
    }

    fn maybe_enter_or_exit_probe_rtt(
        &mut self,
        a_e: Instant,
        is_round_start: bool,
        bytes_in_flight: u64,
        min_rtt_expired: bool,
    ) {
        if min_rtt_expired && !self.exiting_quiescence && self.mode != Mode::ProbeRtt {
            self.mode = Mode::ProbeRtt;
            self.pacing_gain = 1.0;
            // Do not decide on the time to exit ProbeRtt until the
            // |bytes_in_flight| is at the target small value.
            self.exit_probe_rtt_at = None;
        }

        if self.mode == Mode::ProbeRtt {
            match self.exit_probe_rtt_at {
                None => {
                    // If the window has reached the appropriate size, schedule exiting
                    // ProbeRtt.  The CWND during ProbeRtt is
                    // kMinimumCongestionWindow, but we allow an extra packet since QUIC
                    // checks CWND before sending a packet.
                    if bytes_in_flight < self.get_probe_rtt_cwnd() + self.current_mtu {
                        const K_PROBE_RTT_TIME: Duration = Duration::from_millis(200);
                        self.exit_probe_rtt_at = Some(a_e + K_PROBE_RTT_TIME);
                    }
                }
                Some(exit_time) if is_round_start && a_e >= exit_time => {
                    // v1: the filter stamp restarts on exit.
                    self.min_rtt_stamp = Some(a_e);
                    if !self.is_at_full_bandwidth {
                        self.enter_startup_mode();
                    } else {
                        self.enter_probe_bandwidth_mode(a_e);
                    }
                }
                Some(_) => {}
            }
        }

        self.exiting_quiescence = false;
    }

    fn get_target_cwnd(&self, gain: f32) -> u64 {
        let bw = self.max_bandwidth.get_estimate();
        let bdp = self.min_rtt.as_micros() as u64 * bw;
        let bdpf = bdp as f64;
        let cwnd = ((gain as f64 * bdpf) / 1_000_000f64) as u64;
        // BDP estimate will be zero if no bandwidth samples are available yet.
        if cwnd == 0 {
            return self.init_cwnd;
        }
        cwnd.max(self.min_cwnd)
    }

    fn get_probe_rtt_cwnd(&self) -> u64 {
        const K_MODERATE_PROBE_RTT_MULTIPLIER: f32 = 0.75;
        if PROBE_RTT_BASED_ON_BDP {
            return self.get_target_cwnd(K_MODERATE_PROBE_RTT_MULTIPLIER);
        }
        self.min_cwnd
    }

    fn calculate_pacing_rate(&mut self) {
        let bw = self.max_bandwidth.get_estimate();
        if bw == 0 {
            return;
        }
        let target_rate = (bw as f64 * self.pacing_gain as f64) as u64;
        if self.is_at_full_bandwidth {
            self.pacing_rate = target_rate;
            return;
        }

        // Pace at the rate of initial_window / RTT as soon as RTT measurements are
        // available.
        if self.pacing_rate == 0 && self.min_rtt.as_nanos() != 0 {
            self.pacing_rate =
                RsBandwidthEstimation::bw_from_delta(self.init_cwnd, self.min_rtt).unwrap();
            return;
        }

        // Do not decrease the pacing rate during startup.
        if self.pacing_rate < target_rate {
            self.pacing_rate = target_rate;
        }
    }

    fn calculate_cwnd(&mut self, bytes_acked: u64, excess_acked: u64) {
        match self.mode {
            Mode::ProbeRtt => return,
            // Drain: W = T(1), and the exit test is on the same T(1), the only
            // window form whose exit is reachable under a backlog.
            Mode::Drain => self.cwnd = self.get_target_cwnd(self.cwnd_gain),
            _ if self.is_at_full_bandwidth => {
                // ProbeBw: W <- min(T(g_c) + A, W + a), A the 10-round max ack height.
                let target_window =
                    self.get_target_cwnd(self.cwnd_gain) + self.ack_aggregation.max_ack_height.get();
                self.cwnd = target_window.min(self.cwnd + bytes_acked);
            }
            _ => {
                // Startup (v1 bbr_set_cwnd): grow by the acked bytes only while
                // the window is below the model target (plus the most recent
                // excess acked) or the initial window has not been delivered.
                // Never decreases.
                let target_window = self.get_target_cwnd(self.cwnd_gain) + excess_acked;
                if self.cwnd < target_window || self.acked_bytes < self.init_cwnd {
                    self.cwnd += bytes_acked;
                }
            }
        }

        // Enforce the limits on the congestion window.
        if self.cwnd < self.min_cwnd {
            self.cwnd = self.min_cwnd;
        }
    }

    fn calculate_recovery_window(&mut self, bytes_acked: u64, bytes_lost: u64, in_flight: u64) {
        if !self.recovery_state.in_recovery() {
            return;
        }
        // Set up the initial recovery window.
        if self.recovery_window == 0 {
            self.recovery_window = self.min_cwnd.max(in_flight + bytes_acked);
            return;
        }

        // Remove losses from the recovery window, while accounting for a potential
        // integer underflow.
        if self.recovery_window >= bytes_lost {
            self.recovery_window -= bytes_lost;
        } else {
            // k_max_segment_size = current_mtu
            self.recovery_window = self.current_mtu;
        }
        // In CONSERVATION mode, just subtracting losses is sufficient.  In GROWTH,
        // release additional |bytes_acked| to achieve a slow-start-like behavior.
        if self.recovery_state == RecoveryState::Growth {
            self.recovery_window += bytes_acked;
        }

        // Sanity checks.  Ensure that we always allow to send at least an MSS or
        // |bytes_acked| in response, whichever is larger.
        self.recovery_window = self
            .recovery_window
            .max(in_flight + bytes_acked)
            .max(self.min_cwnd);
        // Second changed mechanism: floor the recovery window at the rate
        // model's own 1×BDP target once a bandwidth estimate exists.
        // Upstream's quiche-style recovery window requires a loss-free
        // round to exit recovery; with ~7% per-packet GE loss a loss-free
        // round has probability ≈ 0, so the conservation clamp becomes the
        // permanent window and ratchets the controller to ~0.5×BDP —
        // loss-limited Reno-class behavior on a rate-model controller.
        // Linux BBRv1 holds cwnd at the model target through channel loss
        // (loss is not a model signal in v1); this floor restores exactly
        // that semantic: conservation may hold the window at the measured
        // pipe, never clamp it below (paper §8.1 at the substrate: channel
        // loss is the recovery plane's job; congestion
        // evidence is the model's falling bw estimate). When no bandwidth
        // estimate exists yet, get_target_cwnd returns init_cwnd — the
        // floor then keeps startup loss from wedging the handshake window,
        // matching upstream's init_cwnd fallback semantics.
        self.recovery_window = self.recovery_window.max(self.get_target_cwnd(1.0));
    }

    /// <https://datatracker.ietf.org/doc/html/draft-cardwell-iccrg-bbr-congestion-control#section-4.3.2.2>
    fn check_if_full_bw_reached(&mut self, app_limited: bool) {
        if app_limited {
            return;
        }
        let target = (self.bw_at_last_round as f64 * K_STARTUP_GROWTH_TARGET as f64) as u64;
        let bw = self.max_bandwidth.get_estimate();
        if bw >= target {
            self.bw_at_last_round = bw;
            self.round_wo_bw_gain = 0;
            self.ack_aggregation.max_ack_height.reset();
            return;
        }

        self.round_wo_bw_gain += 1;
        if self.round_wo_bw_gain >= K_ROUND_TRIPS_WITHOUT_GROWTH_BEFORE_EXITING_STARTUP as u64
            || (self.recovery_state.in_recovery())
        {
            self.is_at_full_bandwidth = true;
        }
    }
}

impl Controller for BbrRs {
    fn on_sent(&mut self, now: Instant, bytes: u64, last_packet_number: u64) {
        self.max_sent_packet_number = last_packet_number;
        self.max_bandwidth.on_sent(now, bytes);
    }

    fn on_ack(
        &mut self,
        now: Instant,
        sent: Instant,
        bytes: u64,
        app_limited: bool,
        rtt: &RttEstimator,
    ) {
        self.ack(now, sent, bytes, app_limited, rtt.min());
    }

    fn on_end_acks(
        &mut self,
        _now: Instant,
        in_flight: u64,
        app_limited: bool,
        largest_packet_num_acked: Option<u64>,
    ) {
        let event = self.event.take();
        let a_e = event.map_or_else(|| self.config.clk(), |e| e.a_e);

        // v1 bbr_update_min_rtt on the event clock: r_e = a_e - max s_k.
        let min_rtt_expired = self
            .min_rtt_stamp
            .is_some_and(|t| a_e.saturating_duration_since(t) > MIN_RTT_WINDOW);
        if let Some(e) = event {
            let r_e = a_e.saturating_duration_since(e.max_sent);
            self.smoothed_loop = if self.smoothed_loop.is_zero() {
                r_e
            } else {
                // 7/8 old + 1/8 new
                (self.smoothed_loop * 7 + r_e) / 8
            };
            if self.min_rtt_stamp.is_none() || r_e <= self.min_rtt || min_rtt_expired {
                self.min_rtt = r_e;
                self.min_rtt_stamp = Some(a_e);
            }
        }

        let bytes_acked = self.max_bandwidth.bytes_acked_this_window();
        let excess_acked = self.ack_aggregation.update_ack_aggregation_bytes(
            bytes_acked,
            a_e,
            self.round_count,
            self.max_bandwidth.get_estimate(),
        );
        self.max_bandwidth.end_acks(self.round_count, app_limited);
        if let Some(largest_acked_packet) = largest_packet_num_acked {
            self.max_acked_packet_number = largest_acked_packet;
        }

        let mut is_round_start = false;
        if bytes_acked > 0 {
            is_round_start =
                self.max_acked_packet_number > self.current_round_trip_end_packet_number;
            if is_round_start {
                self.current_round_trip_end_packet_number = self.max_sent_packet_number;
                self.round_count += 1;
            }
        }

        self.update_recovery_state(is_round_start);

        if self.mode == Mode::ProbeBw {
            self.update_gain_cycle_phase(a_e, in_flight);
        }

        if is_round_start && !self.is_at_full_bandwidth {
            self.check_if_full_bw_reached(app_limited);
        }

        self.maybe_exit_startup_or_drain(a_e, in_flight);

        self.maybe_enter_or_exit_probe_rtt(a_e, is_round_start, in_flight, min_rtt_expired);

        // After the model is updated, recalculate the pacing rate and congestion window.
        self.calculate_pacing_rate();
        self.calculate_cwnd(bytes_acked, excess_acked);
        self.calculate_recovery_window(bytes_acked, self.loss_state.lost_bytes, in_flight);

        self.prev_in_flight_count = in_flight;
        self.loss_state.reset();
    }

    fn on_congestion_event(
        &mut self,
        _now: Instant,
        _sent: Instant,
        _is_persistent_congestion: bool,
        lost_bytes: u64,
    ) {
        self.loss_state.lost_bytes += lost_bytes;
    }

    fn on_mtu_update(&mut self, new_mtu: u16) {
        self.current_mtu = new_mtu as u64;
        self.min_cwnd = calculate_min_window(self.current_mtu);
        self.init_cwnd = self.config.initial_window.max(self.min_cwnd);
        self.cwnd = self.cwnd.max(self.min_cwnd);
    }

    fn window(&self) -> u64 {
        if self.mode == Mode::ProbeRtt {
            return self.get_probe_rtt_cwnd();
        } else if self.recovery_state.in_recovery() && self.mode != Mode::Startup {
            return self.cwnd.min(self.recovery_window);
        }
        self.cwnd
    }

    fn metrics(&self) -> ControllerMetrics {
        let mut m = ControllerMetrics::default();
        m.congestion_window = self.window();
        m.ssthresh = None;
        m.pacing_rate = Some(self.pacing_rate * 8);
        m
    }

    fn clone_box(&self) -> Box<dyn Controller> {
        Box::new(self.clone())
    }

    fn initial_window(&self) -> u64 {
        self.config.initial_window
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

/// The controller's own model, for the `[QCC]` gauge line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Qcc {
    /// `S`tartup, `D`rain, `B` (ProbeBw), `R` (ProbeRtt).
    pub mode: char,
    /// m̂ (µs): the controller's min-RTT model.
    pub rt_us: u64,
    /// Smoothed ack-event loop time r_e (µs).
    pub sr_us: u64,
    /// b̂ (bytes/s): the windowed-max delivery rate.
    pub bw: u64,
    /// quinn's own `rtt.min()` (µs), beside m̂.
    pub quinn_min_us: u64,
}

/// The `[QCC]` read of a connection's controller (`congestion_state()`'s
/// clone): `None` when the connection runs another controller.
pub(crate) fn qcc_of(c: Box<dyn Controller>) -> Option<Qcc> {
    c.into_any().downcast::<BbrRs>().ok().map(|b| b.qcc())
}

impl Qcc {
    /// One path's `[QCC]` block: `p<id>:qm=<S|D|B|R> qrt_us=<m̂> qsr_us=<smoothed
    /// r_e> qbw=<b̂ B/s> qmin_us=<quinn's rtt.min()> qcns=<last>/<reads>/<mean>`,
    /// the last field the host cost (ns) of the `congestion_state()` read.
    pub(crate) fn token(&self, path: u32, last_ns: u64, reads: u64, mean_ns: u64) -> String {
        format!(
            " p{path}:qm={} qrt_us={} qsr_us={} qbw={} qmin_us={} qcns={last_ns}/{reads}/{mean_ns}",
            self.mode, self.rt_us, self.sr_us, self.bw, self.quinn_min_us
        )
    }
}

/// The ack-side clock: quinn's runtime clock in production, injectable for
/// host tests.
#[derive(Clone)]
pub(crate) struct Clock(Arc<dyn Fn() -> Instant + Send + Sync>);

impl Debug for Clock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Clock")
    }
}

/// Configuration for the [`BbrRs`] congestion controller (mirrors upstream
/// `BbrConfig`: `initial_window` is the only knob, same default; `clock` is
/// the test seam, inert before the §48 fix).
#[derive(Debug, Clone)]
pub struct BbrRsConfig {
    initial_window: u64,
    /// `None`: `tokio::time::Instant::now().into_std()`, quinn's own runtime
    /// clock (quinn `runtime/tokio.rs`), so it shares a time base with the
    /// send stamps under paused test time.
    clock: Option<Clock>,
}

impl Default for BbrRsConfig {
    fn default() -> Self {
        Self {
            initial_window: K_MAX_INITIAL_CONGESTION_WINDOW * BASE_DATAGRAM_SIZE,
            clock: None,
        }
    }
}

impl BbrRsConfig {
    /// A config whose ack clock is `f` (host tests).
    #[cfg(test)]
    pub(crate) fn with_clock(f: impl Fn() -> Instant + Send + Sync + 'static) -> Self {
        Self { clock: Some(Clock(Arc::new(f))), ..Self::default() }
    }

    fn clk(&self) -> Instant {
        match &self.clock {
            Some(c) => (c.0)(),
            None => tokio::time::Instant::now().into_std(),
        }
    }
}

impl ControllerFactory for BbrRsConfig {
    fn build(self: Arc<Self>, _now: Instant, current_mtu: u16) -> Box<dyn Controller> {
        Box::new(BbrRs::new(self, current_mtu))
    }
}

/// One ACK event: a_e = the event clock (read once, at its first callback);
/// `max_sent` = the latest send stamp of a packet newly acked in it.
#[derive(Debug, Clone, Copy)]
struct AckEvent {
    a_e: Instant,
    max_sent: Instant,
}

#[derive(Debug, Default, Copy, Clone)]
struct AckAggregationState {
    max_ack_height: MinMax,
    aggregation_epoch_start_time: Option<Instant>,
    aggregation_epoch_bytes: u64,
}

impl AckAggregationState {
    fn update_ack_aggregation_bytes(
        &mut self,
        newly_acked_bytes: u64,
        now: Instant,
        round: u64,
        max_bandwidth: u64,
    ) -> u64 {
        // Compute how many bytes are expected to be delivered, assuming max
        // bandwidth is correct.
        let expected_bytes_acked = max_bandwidth
            * now
                .saturating_duration_since(self.aggregation_epoch_start_time.unwrap_or(now))
                .as_micros() as u64
            / 1_000_000;

        // Reset the current aggregation epoch as soon as the ack arrival rate is
        // less than or equal to the max bandwidth.
        if self.aggregation_epoch_bytes <= expected_bytes_acked {
            // Reset to start measuring a new aggregation epoch.
            self.aggregation_epoch_bytes = newly_acked_bytes;
            self.aggregation_epoch_start_time = Some(now);
            return 0;
        }

        // Compute how many extra bytes were delivered vs max bandwidth.
        // Include the bytes most recently acknowledged to account for stretch acks.
        self.aggregation_epoch_bytes += newly_acked_bytes;
        let diff = self.aggregation_epoch_bytes - expected_bytes_acked;
        self.max_ack_height.update_max(round, diff);
        diff
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Mode {
    // Startup phase of the connection.
    Startup,
    // After achieving the highest possible bandwidth during the startup, lower
    // the pacing rate in order to drain the queue.
    Drain,
    // Cruising mode.
    ProbeBw,
    // Temporarily slow down sending in order to empty the buffer and measure
    // the real minimum RTT.
    ProbeRtt,
}

// Indicates how the congestion control limits the amount of bytes in flight.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum RecoveryState {
    // Do not limit.
    NotInRecovery,
    // Allow an extra outstanding byte for each byte acknowledged.
    Conservation,
    // Allow two extra outstanding bytes for each byte acknowledged (slow
    // start).
    Growth,
}

impl RecoveryState {
    fn in_recovery(&self) -> bool {
        !matches!(self, Self::NotInRecovery)
    }
}

#[derive(Debug, Clone, Default)]
struct LossState {
    lost_bytes: u64,
}

impl LossState {
    fn reset(&mut self) {
        self.lost_bytes = 0;
    }

    fn has_losses(&self) -> bool {
        self.lost_bytes != 0
    }
}

fn calculate_min_window(current_mtu: u64) -> u64 {
    4 * current_mtu
}

// The gain used for the STARTUP, equal to 2/ln(2).
const K_DEFAULT_HIGH_GAIN: f32 = 2.885;
// The newly derived CWND gain for STARTUP, 2.
const K_DERIVED_HIGH_CWNDGAIN: f32 = 2.0;
// Drain's window gain: T(1) (v1 drains with a pacing gain quinn ignores).
const K_DRAIN_CWND_GAIN: f32 = 1.0;
// v1 `bbr_min_rtt_win_sec`.
const MIN_RTT_WINDOW: Duration = Duration::from_secs(10);
// The cycle of gains used during the ProbeBw stage.
const K_PACING_GAIN: [f32; 8] = [1.25, 0.75, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];

const K_STARTUP_GROWTH_TARGET: f32 = 1.25;
const K_ROUND_TRIPS_WITHOUT_GROWTH_BEFORE_EXITING_STARTUP: u8 = 3;

// Do not allow initial congestion window to be greater than 200 packets.
const K_MAX_INITIAL_CONGESTION_WINDOW: u64 = 200;

const PROBE_RTT_BASED_ON_BDP: bool = true;
const DRAIN_TO_TARGET: bool = true;

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    /// The law test (mirrors the scheduler's
    /// `rate_sample_anchor_reads_true_btlbw_under_aggregation_and_queue`,
    /// here at the substrate controller): a token-bucket bottleneck that
    /// delivers line-rate ack clusters at the true-link average must read
    /// ≈ the true link, not the cluster rate — the upstream adjacent-event
    /// estimator's ×10 latch is the defect under test.
    #[test]
    fn estimator_reads_true_link_under_token_bucket_ack_clusters() {
        let t0 = Instant::now();
        let mut est = RsBandwidthEstimation::default();
        let rtprop = 10 * MS; // shal8-class propagation floor
        let pkt: u64 = 1200;
        // True link: 100 mbit ≈ 12.5 kB/ms ≈ ~10.4 pkts/ms... model
        // 10 pkts per ms-tick delivered in ONE burst every 10 packets'
        // worth of time: sends paced ~1 pkt / 0.1 ms (sender overshoot is
        // irrelevant to the estimator law), acks arrive as 10-packet
        // clusters with intra-cluster spacing of 1 µs (line rate), one
        // cluster per ms (the token-bucket release cadence).
        let true_rate_bps: f64 = 12_000_000.0 / 8.0 * 8.0; // 12 MB/s in bytes/s ≈ 96 mbit
        // Send phase: 2000 packets, 0.1 ms apart (≈ true link input rate).
        let mut send_t = t0;
        for _ in 0..2000 {
            est.on_sent(send_t, pkt);
            send_t += Duration::from_micros(100);
        }
        // Ack phase: clusters of 10, cluster start lags send by ~RTprop,
        // intra-cluster 1 µs (the aggregation artefact), inter-cluster 1 ms.
        let mut round = 0u64;
        let mut acked = 0u64;
        let mut cluster_t = t0 + rtprop;
        let mut sent_t = t0;
        while acked < 2000 {
            let mut ack_t = cluster_t;
            for _ in 0..10 {
                if acked >= 2000 {
                    break;
                }
                est.on_ack(ack_t, sent_t, pkt, round, false, rtprop);
                ack_t += Duration::from_micros(1);
                sent_t += Duration::from_micros(100);
                acked += 1;
            }
            cluster_t += MS;
            if acked % 200 == 0 {
                round += 1; // ~a round per 200 pkts; irrelevant to the law
            }
        }
        let read = est.get_estimate() as f64;
        assert!(
            read > 0.0,
            "estimator must establish under clustered acks (generated={:?})",
            est.rs_diag()
        );
        // The law: within [0.5x, 2x] of the true link, and never the
        // ×10-class over-read the adjacent-event estimator latches.
        assert!(
            read > 0.5 * true_rate_bps && read < 2.0 * true_rate_bps,
            "burst-robust estimator must read ~the true link under ack \
             clustering: read {read:.0} B/s vs true {true_rate_bps:.0} B/s \
             (diag {:?})",
            est.rs_diag()
        );
    }

    /// Sub-RTprop samples are the aggregation artefact and must be
    /// rejected — an ack cluster acked over µs windows may not move the
    /// filter at all.
    #[test]
    fn estimator_rejects_sub_rtprop_intervals() {
        let t0 = Instant::now();
        let mut est = RsBandwidthEstimation::default();
        let rtprop = 10 * MS;
        let pkt: u64 = 1200;
        // One instantaneous burst of sends, acked 1 µs apart 100 µs later:
        // every sample spans ≪ RTprop.
        for i in 0..20 {
            est.on_sent(t0 + Duration::from_micros(i), pkt);
        }
        let mut ack_t = t0 + Duration::from_micros(100);
        for i in 0..20 {
            est.on_ack(ack_t, t0 + Duration::from_micros(i), pkt, 0, false, rtprop);
            ack_t += Duration::from_micros(1);
        }
        assert_eq!(
            est.get_estimate(),
            0,
            "all sub-RTprop samples must be rejected (diag {:?})",
            est.rs_diag()
        );
        let (generated, rej_interval, _, _) = est.rs_diag();
        assert_eq!(generated, 0);
        assert!(rej_interval > 0, "the interval guard must have fired");
    }

    /// App-limited samples may only raise the filter, never depress it —
    /// and a low app-limited sample is not admitted.
    #[test]
    fn app_limited_samples_are_raise_only() {
        let t0 = Instant::now();
        let mut est = RsBandwidthEstimation::default();
        let rtprop = 10 * MS;
        let pkt: u64 = 1200;
        // Establish a healthy estimate: 100 pkts sent 0.1 ms apart, acked
        // in-flow RTprop later at the same spacing (interval ≈ RTprop+).
        let mut send_t = t0;
        for _ in 0..300 {
            est.on_sent(send_t, pkt);
            send_t += Duration::from_micros(100);
        }
        let mut sent_t = t0;
        let mut ack_t = t0 + rtprop;
        for _ in 0..300 {
            est.on_ack(ack_t, sent_t, pkt, 1, false, rtprop);
            sent_t += Duration::from_micros(100);
            ack_t += Duration::from_micros(100);
        }
        let established = est.get_estimate();
        assert!(established > 0);
        // App-limited dribble at 1/10th the rate: filter must not drop.
        let mut send_t = ack_t + MS;
        for _ in 0..50 {
            est.on_sent(send_t, pkt);
            send_t += MS;
        }
        let mut sent_t = ack_t + MS;
        let mut ack_t2 = ack_t + MS + rtprop;
        for _ in 0..50 {
            est.on_ack(ack_t2, sent_t, pkt, 2, true, rtprop);
            sent_t += MS;
            ack_t2 += MS;
        }
        assert_eq!(
            est.get_estimate(),
            established,
            "an app-limited dribble must not depress the max filter"
        );
    }

    /// Recovery-window floor law: sustained per-round loss must not clamp
    /// the recovery window below the rate model's 1×BDP target — a
    /// loss-free round has probability ~0 at 7% per-packet loss, so
    /// without the floor the conservation clamp becomes the permanent
    /// window (~0.5×BDP).
    #[test]
    fn sustained_loss_cannot_clamp_recovery_below_the_pipe() {
        let cfg = Arc::new(BbrRsConfig::default());
        let mut bbr = BbrRs::new(cfg, 1200);
        let t0 = Instant::now();
        // Established model: RTprop 10 ms, bw 12 MB/s (~100 mbit).
        // m̂ planted through the filter, fresh now: keeps ProbeRtt out of the
        // law under test.
        bbr.plant_min_rtt(10 * MS);
        bbr.max_bandwidth.max_filter.update_max(1, 12_000_000);
        bbr.is_at_full_bandwidth = true;
        bbr.mode = Mode::ProbeBw;
        bbr.cwnd = bbr.get_target_cwnd(2.0);
        let bdp = bbr.get_target_cwnd(1.0); // = bw x RTprop = 120 kB
        // Drive rounds that each carry loss (GE + tail-drop cell class):
        // a link-consistent flight (100 x 1200 B per 10 ms = 12 MB/s, the
        // planted bw) is sent, acked, one loss recorded, batch closed —
        // recovery never sees a loss-free round.
        let mut now = t0;
        let mut pn: u64 = 0;
        for _round in 0..50 {
            now += 10 * MS;
            for _ in 0..100 {
                pn += 1;
                bbr.on_sent(now, 1200, pn);
            }
            bbr.on_congestion_event(now, now, false, 1200);
            // acks driven at the estimator directly — the Controller-level
            // on_ack needs quinn's RttEstimator, whose constructor is
            // private; the recovery law under test lives in
            // on_end_acks/calculate_recovery_window.
            for _ in 0..100 {
                bbr.max_bandwidth
                    .on_ack(now, now - 10 * MS, 1200, bbr.round_count, false, 10 * MS);
            }
            bbr.on_end_acks(now, bdp, false, Some(pn));
            assert!(
                bbr.recovery_state.in_recovery(),
                "test must exercise the recovery-resident regime"
            );
            assert_eq!(bbr.mode, Mode::ProbeBw, "mode drift would void the law");
        }
        assert!(
            bbr.window() >= bdp,
            "recovery-resident window {} clamped below the 1xBDP target {}",
            bbr.window(),
            bdp
        );
    }

    /// The controller wires the estimator into cwnd: under clustered acks
    /// the window must settle in the BDP class, not the ×10 class.
    #[test]
    fn controller_window_tracks_honest_bdp_under_clusters() {
        let cfg = Arc::new(BbrRsConfig::default());
        let mut bbr = BbrRs::new(cfg, 1200);
        // Drive the estimator directly (the controller's cwnd law reads
        // get_target_cwnd = gain × bw × min_rtt): plant an honest bw and a
        // floor, then check the target-cwnd arithmetic stays ~2×BDP class.
        bbr.plant_min_rtt(10 * MS);
        bbr.max_bandwidth.max_filter.update_max(1, 12_000_000); // 12 MB/s
        let target = bbr.get_target_cwnd(2.0);
        let bdp = 12_000_000u64 / 100; // bw × 10 ms = 120 kB
        assert!(
            target >= 2 * bdp * 9 / 10 && target <= 2 * bdp * 11 / 10,
            "target cwnd {target} must be ~2×BDP ({bdp})"
        );
    }

    // ── §48 window-law tests (identical on the pre-fix and post-fix trees) ─

    use std::sync::atomic::{AtomicU64, Ordering};

    /// A controller on an injectable ack clock, driven one ACK event at a
    /// time. `RttEstimator` cannot be built here, so it drives `ack()`.
    struct H {
        bbr: BbrRs,
        base: Instant,
        clock: Arc<AtomicU64>,
        pn: u64,
    }

    impl H {
        fn new() -> Self {
            let base = Instant::now();
            let clock = Arc::new(AtomicU64::new(0));
            let c = clock.clone();
            let cfg = Arc::new(BbrRsConfig::with_clock(move || {
                base + Duration::from_nanos(c.load(Ordering::Relaxed))
            }));
            Self { bbr: BbrRs::new(cfg, 1200), base, clock, pn: 0 }
        }

        fn at(&self, t: Duration) -> Instant {
            self.base + t
        }

        /// One ACK event at time `t` (>= `rtt`): `n` 1200-B packets sent at
        /// `t - rtt` and acked together. The clock reads `t`, quinn's stamp
        /// is also `t`, quinn's `rtt.min()` is `rtt`.
        fn event(&mut self, t: Duration, rtt: Duration, n: u64, app_limited: bool, in_flight: u64) {
            self.clock.store(t.as_nanos() as u64, Ordering::Relaxed);
            let sent = self.at(t - rtt);
            for _ in 0..n {
                self.pn += 1;
                self.bbr.on_sent(sent, 1200, self.pn);
            }
            for _ in 0..n {
                self.bbr.ack(self.at(t), sent, 1200, app_limited, rtt);
            }
            self.bbr.on_end_acks(self.at(t), in_flight, app_limited, Some(self.pn));
        }
    }

    /// Status §48 test 6. v1 grows the Startup window only while it is below
    /// the model target. The port compared a gain with a byte count (always
    /// true), so with the store keeping the flow app-limited (Startup never
    /// ends) the window was the sum of the acked bytes.
    #[test]
    fn startup_window_is_bounded_by_the_model_target() {
        let mut h = H::new();
        h.bbr.plant_min_rtt(10 * MS);
        h.bbr.max_bandwidth.max_filter.update_max(0, 12_000_000);
        let target = h.bbr.get_target_cwnd(K_DEFAULT_HIGH_GAIN); // 2.885 x 120 kB
        assert!((340_000..=350_000).contains(&target), "plant: target {target}");
        let acked_per_event = 100 * 1200;
        let mut max_e = 0;
        for i in 0..50u32 {
            // app-limited: the full-bandwidth check is skipped, Startup stays
            h.event(10 * MS * (i + 2), 10 * MS, 100, true, 0);
            max_e = max_e.max(h.bbr.ack_aggregation.max_ack_height.get());
        }
        assert_eq!(h.bbr.mode, Mode::Startup, "the regime under test is app-limited Startup");
        let w = h.bbr.window();
        assert!(
            w <= target + max_e + acked_per_event,
            "Startup window {w} exceeds T(2.885) {target} + max excess {max_e} + one event {acked_per_event}"
        );
    }

    /// Status §48 test 7. With in_flight held at the window (a backlog), the
    /// exit test F <= T(1) must be reachable: Drain's window is T(1).
    #[test]
    fn drain_exits_under_a_backlog() {
        let mut h = H::new();
        h.bbr.plant_min_rtt(10 * MS);
        h.bbr.max_bandwidth.max_filter.update_max(0, 12_000_000);
        let (mut full_at, mut bw_at) = (None, None);
        for i in 0..60u32 {
            if h.bbr.is_at_full_bandwidth {
                // a nonzero ack-aggregation term: Drain's window and exit test
                // must both leave it out (T(1), not T(1) + A)
                let round = h.bbr.round_count;
                h.bbr.ack_aggregation.max_ack_height.update_max(round, 50_000);
            }
            let backlog = h.bbr.window(); // in flight held at W
            h.event(10 * MS * (i + 2), 10 * MS, 100, false, backlog);
            assert_ne!(h.bbr.mode, Mode::ProbeRtt, "a ProbeRtt in between voids the test (event {i})");
            if full_at.is_none() && h.bbr.is_at_full_bandwidth {
                full_at = Some(i);
            }
            if full_at.is_some() && bw_at.is_none() && h.bbr.mode == Mode::ProbeBw {
                bw_at = Some(i);
            }
        }
        let f = full_at.expect("full bandwidth never reached: the scenario is broken");
        let b = bw_at.unwrap_or_else(|| {
            panic!(
                "Drain (entered at event {f}) never exited to ProbeBw under a backlog; mode {:?}, W {}, T(1) {}",
                h.bbr.mode,
                h.bbr.window(),
                h.bbr.get_target_cwnd(1.0)
            )
        });
        // events are one m̂ (10 ms) apart
        assert!(b - f <= 3, "Drain took {} m̂ (events {f} -> {b})", b - f);
    }

    /// Status §48 test 8. v1 enters ProbeRtt when the min filter has expired
    /// (no sample at or below m̂ for 10 s), not on a timer.
    #[test]
    fn probe_rtt_fires_only_on_an_expired_min_filter() {
        let mut h = H::new();
        let s = Duration::from_secs;
        let mut entries: Vec<Duration> = Vec::new();
        let mut in_probe_rtt = false;
        let mut run = |h: &mut H, t: Duration, rtt: Duration| {
            h.event(t, rtt, 10, false, 0);
            let now = h.bbr.mode == Mode::ProbeRtt;
            if now && !in_probe_rtt {
                entries.push(t);
            }
            in_probe_rtt = now;
            entries.len()
        };
        // a new minimum every second for 31 s (r_e falls 1 us per second, so
        // the test holds under both `<` and `<=`), from the very first
        // non-app-limited event
        for sec in 1..=31u32 {
            let n = run(&mut h, s(sec as u64), 10 * MS - Duration::from_micros(sec as u64));
            assert_eq!(n, 0, "ProbeRtt at t={sec}s while r_e refreshes the filter every second");
        }
        // refreshes stop (r_e above m̂): exactly one ProbeRtt, when the filter expires
        for sec in 32..=45 {
            run(&mut h, s(sec), 15 * MS);
        }
        assert_eq!(entries.len(), 1, "ProbeRtt entries {entries:?}");
        assert!(
            entries[0] > s(41) && entries[0] <= s(42),
            "ProbeRtt at {:?}, expected at t_m + 10 s = 41 s (first event past it: 42 s)",
            entries[0]
        );
    }
}
