//! Burst-robust BBR substrate congestion controller (`RWM_QUIC_CC=bbr_rs`).
//!
//! Goal-gate "Ship The Wins 2: shal8 anchor" (2026-08-07), ADR-0054/0061.
//!
//! This is quinn-proto 0.11.14's `congestion::bbr` ported into the engine
//! tree (quinn is Apache-2.0/MIT dual-licensed; the port keeps the upstream
//! structure, gains, mode machine, recovery window, ProbeRTT, and
//! ack-aggregation cwnd term VERBATIM — one mechanism changes, so a battery
//! attributes one mechanism), with the bandwidth estimator replaced.
//!
//! WHY (the named defect, measured at the shal8 8-packet-buffer cell —
//! goal-gate "Adversarial Cells (B1)"): upstream `bw_estimation.rs` samples
//! BOTH of its rates over ADJACENT-EVENT gaps — `ack_rate` = this ack
//! event's bytes / gap to the previous ack event, `send_rate` = the last
//! two send events' delta (and `u64::MAX` when they share a timestamp,
//! which quinn's own ≥10-packet pacer bursts and GSO batches guarantee).
//! A token-bucket bottleneck drains its bucket at line rate, delivering
//! ~11-packet clusters; the resulting ack clusters make `ack_rate` read
//! the LINE rate, `min(send, ack)` admits it (the send side is vacuous
//! inside a burst), and the 10-round windowed-MAX filter LATCHES a
//! bottleneck estimate ~10x the true link — after which pacing_rate and
//! cwnd (both derived from the estimate) never bind, and the controller
//! sustains overshoot into the shallow buffer forever (measured: 9.8/10.0
//! Mbit of a 100 Mbit link at 7.3% sustained drops, vs 75-79 for a
//! delay-law controller on the same cell).
//!
//! THE FIX is the same one the engine's own delivery anchor received in
//! ADR-0061 (`CopaState::rs_on_delivered`, scheduler/mod.rs — the
//! Cardwell/Cheng draft-cheng-iccrg-delivery-rate-estimation sampler,
//! law-tested by `rate_sample_anchor_reads_true_btlbw_under_aggregation_
//! and_queue`): per-flight rate samples
//!     rate = delta_delivered / max(send_elapsed, ack_elapsed)
//! taken against a snapshot of the delivery state at the acked packet's
//! SEND time, with samples spanning < RTprop rejected (an interval below
//! one propagation RTT cannot estimate the bottleneck — it is the
//! ack-aggregation artefact itself) and app-limited samples admitted
//! raise-only. The `Controller` trait hands `on_ack` the acked packet's
//! send `Instant`, which keys the snapshot map (packets of one GSO batch
//! share the timestamp and therefore the snapshot — same send state by
//! construction).

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
// THE CHANGED MECHANISM: interval-guarded per-flight delivery-rate sampler
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
        // full): admit them only when they RAISE the filter (BBR
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
    probe_rtt_last_started_at: Option<Instant>,
    min_rtt: Duration,
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
            probe_rtt_last_started_at: None,
            min_rtt: Default::default(),
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
            self.cwnd_gain = self.high_cwnd_gain;
        }
        if self.mode == Mode::Drain && in_flight <= self.get_target_cwnd(1.0) {
            self.enter_probe_bandwidth_mode(now);
        }
    }

    fn is_min_rtt_expired(&self, now: Instant, app_limited: bool) -> bool {
        !app_limited
            && self
                .probe_rtt_last_started_at
                .map(|last| now.saturating_duration_since(last) > Duration::from_secs(10))
                .unwrap_or(true)
    }

    fn maybe_enter_or_exit_probe_rtt(
        &mut self,
        now: Instant,
        is_round_start: bool,
        bytes_in_flight: u64,
        app_limited: bool,
    ) {
        let min_rtt_expired = self.is_min_rtt_expired(now, app_limited);
        if min_rtt_expired && !self.exiting_quiescence && self.mode != Mode::ProbeRtt {
            self.mode = Mode::ProbeRtt;
            self.pacing_gain = 1.0;
            // Do not decide on the time to exit ProbeRtt until the
            // |bytes_in_flight| is at the target small value.
            self.exit_probe_rtt_at = None;
            self.probe_rtt_last_started_at = Some(now);
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
                        self.exit_probe_rtt_at = Some(now + K_PROBE_RTT_TIME);
                    }
                }
                Some(exit_time) if is_round_start && now >= exit_time => {
                    if !self.is_at_full_bandwidth {
                        self.enter_startup_mode();
                    } else {
                        self.enter_probe_bandwidth_mode(now);
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
        if self.mode == Mode::ProbeRtt {
            return;
        }
        let mut target_window = self.get_target_cwnd(self.cwnd_gain);
        if self.is_at_full_bandwidth {
            // Add the max recently measured ack aggregation to CWND.
            target_window += self.ack_aggregation.max_ack_height.get();
        } else {
            // Add the most recent excess acked.  Because CWND never decreases in
            // STARTUP, this will automatically create a very localized max filter.
            target_window += excess_acked;
        }
        // Instead of immediately setting the target CWND as the new one, BBR grows
        // the CWND towards |target_window| by only increasing it |bytes_acked| at a
        // time.
        if self.is_at_full_bandwidth {
            self.cwnd = target_window.min(self.cwnd + bytes_acked);
        } else if (self.cwnd_gain < target_window as f32) || (self.acked_bytes < self.init_cwnd) {
            // If the connection is not yet out of startup phase, do not decrease
            // the window.
            self.cwnd += bytes_acked;
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
        // THE CHANGED CALL: per-flight sample with the RTprop interval guard
        // (rtt.min() — quinn's packet-timed floor; RS_MIN_INTERVAL until
        // real samples exist / for sub-ms LAN paths).
        self.max_bandwidth
            .on_ack(now, sent, bytes, self.round_count, app_limited, rtt.min());
        self.acked_bytes += bytes;
        if self.is_min_rtt_expired(now, app_limited) || self.min_rtt > rtt.min() {
            self.min_rtt = rtt.min();
        }
    }

    fn on_end_acks(
        &mut self,
        now: Instant,
        in_flight: u64,
        app_limited: bool,
        largest_packet_num_acked: Option<u64>,
    ) {
        let bytes_acked = self.max_bandwidth.bytes_acked_this_window();
        let excess_acked = self.ack_aggregation.update_ack_aggregation_bytes(
            bytes_acked,
            now,
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
            self.update_gain_cycle_phase(now, in_flight);
        }

        if is_round_start && !self.is_at_full_bandwidth {
            self.check_if_full_bw_reached(app_limited);
        }

        self.maybe_exit_startup_or_drain(now, in_flight);

        self.maybe_enter_or_exit_probe_rtt(now, is_round_start, in_flight, app_limited);

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

/// Configuration for the [`BbrRs`] congestion controller (mirrors upstream
/// `BbrConfig`: `initial_window` is the only knob, same default).
#[derive(Debug, Clone)]
pub struct BbrRsConfig {
    initial_window: u64,
}

impl BbrRsConfig {
    /// Default limit on the amount of outstanding data in bytes.
    #[allow(dead_code)]
    pub fn initial_window(&mut self, value: u64) -> &mut Self {
        self.initial_window = value;
        self
    }
}

impl Default for BbrRsConfig {
    fn default() -> Self {
        Self {
            initial_window: K_MAX_INITIAL_CONGESTION_WINDOW * BASE_DATAGRAM_SIZE,
        }
    }
}

impl ControllerFactory for BbrRsConfig {
    fn build(self: Arc<Self>, _now: Instant, current_mtu: u16) -> Box<dyn Controller> {
        Box::new(BbrRs::new(self, current_mtu))
    }
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

    /// The law test (mirrors scheduler/mod.rs
    /// `rate_sample_anchor_reads_true_btlbw_under_aggregation_and_queue`,
    /// here at the substrate controller): a token-bucket bottleneck that
    /// delivers line-rate ACK CLUSTERS at the true-link average must read
    /// ≈ the true link, NOT the cluster rate — the upstream adjacent-event
    /// estimator's measured ×10 latch is the defect under test.
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
        // The law: within [0.5x, 2x] of the true link, and NEVER the
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

    /// App-limited samples may only RAISE the filter, never depress it —
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
        // App-limited dribble at 1/10th the rate: filter must NOT drop.
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

    /// The controller wires the estimator into cwnd: under clustered acks
    /// the window must settle in the BDP class, not the ×10 class.
    #[test]
    fn controller_window_tracks_honest_bdp_under_clusters() {
        let cfg = Arc::new(BbrRsConfig::default());
        let mut bbr = BbrRs::new(cfg, 1200);
        // Drive the estimator directly (the controller's cwnd law reads
        // get_target_cwnd = gain × bw × min_rtt): plant an honest bw and a
        // floor, then check the target-cwnd arithmetic stays ~2×BDP class.
        bbr.min_rtt = 10 * MS;
        bbr.max_bandwidth.max_filter.update_max(1, 12_000_000); // 12 MB/s
        let target = bbr.get_target_cwnd(2.0);
        let bdp = 12_000_000u64 / 100; // bw × 10 ms = 120 kB
        assert!(
            target >= 2 * bdp * 9 / 10 && target <= 2 * bdp * 11 / 10,
            "target cwnd {target} must be ~2×BDP ({bdp})"
        );
    }
}
