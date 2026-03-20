//! Shared test infrastructure for network simulation tests.
//!
//! Provides a deterministic SimChannel that models delay, jitter, and
//! bursty loss (Gilbert-Elliott) for component-level integration tests.

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use raptorpath::fec::{FecBackend, WireSymbol};
use raptorpath::scheduler::{Clock, MockClock};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Approximate per-symbol wire overhead (metadata, not payload).
const SYMBOL_SIZE_BYTES: usize = 25;

// ---------------------------------------------------------------------------
// Gilbert-Elliott channel model
// ---------------------------------------------------------------------------

/// Two-state Markov loss model: Good state (low loss) ↔ Bad state (high loss).
///
/// Steady-state math:
///   π_bad = p_gb / (p_gb + p_bg)
///   avg_loss = (1 - π_bad) × loss_good + π_bad × loss_bad
///   mean_burst_length = 1 / p_bg  (expected packets in Bad before returning to Good)
pub struct GilbertElliottChannel {
    pub p_gb: f64,
    pub p_bg: f64,
    pub loss_good: f64,
    pub loss_bad: f64,
    in_bad: bool,
}

impl GilbertElliottChannel {
    pub fn new(p_gb: f64, p_bg: f64, loss_good: f64, loss_bad: f64) -> Self {
        Self {
            p_gb,
            p_bg,
            loss_good,
            loss_bad,
            in_bad: false,
        }
    }

    /// Returns true if the packet should be dropped.
    pub fn should_drop(&mut self, rng: &mut ChaCha8Rng) -> bool {
        let loss_prob = if self.in_bad {
            self.loss_bad
        } else {
            self.loss_good
        };
        let drop = rng.gen::<f64>() < loss_prob;

        // State transition
        let transition: f64 = rng.gen();
        if self.in_bad {
            if transition < self.p_bg {
                self.in_bad = false;
            }
        } else if transition < self.p_gb {
            self.in_bad = true;
        }

        drop
    }

    pub fn is_in_bad_state(&self) -> bool {
        self.in_bad
    }

    /// Force the channel into the Bad state (for correlated fading).
    pub fn force_bad_state(&mut self) {
        self.in_bad = true;
    }
}

// ---------------------------------------------------------------------------
// SimPacket — in-flight packet with scheduled delivery time
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct SimPacket {
    pub delivery_time: Instant,
    pub symbol: WireSymbol,
    pub seq: u64,
}

impl PartialEq for SimPacket {
    fn eq(&self, other: &Self) -> bool {
        self.delivery_time == other.delivery_time
    }
}
impl Eq for SimPacket {}

impl PartialOrd for SimPacket {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SimPacket {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.delivery_time.cmp(&other.delivery_time)
    }
}

// ---------------------------------------------------------------------------
// SimChannel
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// LinkModel — bottleneck link with finite capacity and buffer
// ---------------------------------------------------------------------------

/// Models a bottleneck link with finite capacity and buffer for tail-drop.
pub struct LinkModel {
    capacity_bps: f64,     // link capacity in bytes/sec
    max_queue: usize,      // max queue depth in packets
    link_free_at: Instant,  // when current transmission completes
    queue_depth: usize,     // current queue depth
    pub tail_drops: u64,    // stats
}

impl LinkModel {
    pub fn new(capacity_bps: f64, max_queue: usize) -> Self {
        Self {
            capacity_bps,
            max_queue,
            link_free_at: Instant::now(),
            queue_depth: 0,
            tail_drops: 0,
        }
    }

    /// Enqueue a packet. Returns queue_delay + serialization time, or None for tail-drop.
    pub fn enqueue(&mut self, now: Instant, pkt_size: usize) -> Option<Duration> {
        if self.queue_depth >= self.max_queue {
            self.tail_drops += 1;
            return None;
        }

        let serialization_time =
            Duration::from_secs_f64(pkt_size as f64 / self.capacity_bps);

        // Queue delay: how long until the link is free
        let queue_delay = if self.link_free_at > now {
            self.link_free_at - now
        } else {
            Duration::ZERO
        };

        // Update link availability
        let start = if self.link_free_at > now {
            self.link_free_at
        } else {
            now
        };
        self.link_free_at = start + serialization_time;
        self.queue_depth += 1;

        Some(queue_delay + serialization_time)
    }

    /// Notify that a packet has been delivered (frees queue slot).
    pub fn dequeue(&mut self) {
        self.queue_depth = self.queue_depth.saturating_sub(1);
    }
}

// ---------------------------------------------------------------------------
// CorrelatedFading — forces both paths into bad state simultaneously
// ---------------------------------------------------------------------------

/// Models correlated fading events that affect multiple paths simultaneously.
pub struct CorrelatedFading {
    correlation_prob: f64,    // per-tick probability of starting a correlated burst
    burst_duration: Duration, // how long both paths stay in bad
    in_burst: bool,
    burst_end: Option<Instant>,
    rng: ChaCha8Rng,
}

impl CorrelatedFading {
    pub fn new(correlation_prob: f64, burst_duration: Duration, seed: u64) -> Self {
        Self {
            correlation_prob,
            burst_duration,
            in_burst: false,
            burst_end: None,
            rng: ChaCha8Rng::seed_from_u64(seed),
        }
    }

    /// Step the correlation model. Returns true if both paths should be in bad state.
    pub fn step(&mut self, now: Instant) -> bool {
        // Check if current burst has ended
        if let Some(end) = self.burst_end {
            if now >= end {
                self.in_burst = false;
                self.burst_end = None;
            }
        }

        // Maybe start a new burst
        if !self.in_burst && self.rng.gen::<f64>() < self.correlation_prob {
            self.in_burst = true;
            self.burst_end = Some(now + self.burst_duration);
        }

        self.in_burst
    }
}

/// Deterministic network channel simulator with delay, jitter, and GE loss.
pub struct SimChannel {
    clock: Arc<MockClock>,
    rng: ChaCha8Rng,
    in_flight: BinaryHeap<Reverse<SimPacket>>,
    base_delay: Duration,
    jitter_ms: u64,
    ge: GilbertElliottChannel,
    next_seq: u64,
    link: Option<LinkModel>,
}

impl SimChannel {
    pub fn new(
        clock: Arc<MockClock>,
        seed: u64,
        base_delay: Duration,
        jitter_ms: u64,
        ge: GilbertElliottChannel,
    ) -> Self {
        Self {
            clock,
            rng: ChaCha8Rng::seed_from_u64(seed),
            in_flight: BinaryHeap::new(),
            base_delay,
            jitter_ms,
            ge,
            next_seq: 0,
            link: None,
        }
    }

    /// Datacenter preset: 1ms delay, 0 jitter, ~0.1% uniform loss.
    /// GE: p_gb=0.00, p_bg=1.00 → π_bad=0%, avg_loss=0.1% (pure Good state, uniform)
    pub fn datacenter(clock: Arc<MockClock>, seed: u64) -> Self {
        Self::new(
            clock,
            seed,
            Duration::from_millis(1),
            0,
            GilbertElliottChannel::new(0.0, 1.0, 0.001, 0.0),
        )
    }

    /// WiFi preset: 5ms delay, 3ms jitter, bursty loss (~2.6% avg).
    /// GE: p_gb=0.03, p_bg=0.50 → π_bad=5.7%, avg_loss=0.943×0.01+0.057×0.30=2.6%
    /// Mean burst length = 1/p_bg = 2.0 packets
    pub fn wifi(clock: Arc<MockClock>, seed: u64) -> Self {
        Self::new(
            clock,
            seed,
            Duration::from_millis(5),
            3,
            GilbertElliottChannel::new(0.03, 0.5, 0.01, 0.3),
        )
    }

    /// LTE preset: 20ms delay, 5ms jitter, bursty loss (~3.7% avg).
    /// GE: p_gb=0.02, p_bg=0.25 → π_bad=7.4%, avg_loss=0.926×0.005+0.074×0.40=3.4%
    /// Mean burst length = 1/p_bg = 4.0 packets
    pub fn lte(clock: Arc<MockClock>, seed: u64) -> Self {
        Self::new(
            clock,
            seed,
            Duration::from_millis(20),
            5,
            GilbertElliottChannel::new(0.02, 0.25, 0.005, 0.4),
        )
    }

    /// Satellite preset: 100ms delay, 10ms jitter, bursty loss (~8% avg).
    /// GE: p_gb=0.05, p_bg=0.40 → π_bad=11.1%, avg_loss=0.889×0.04+0.111×0.50=9.1%
    /// Mean burst length = 1/p_bg = 2.5 packets
    pub fn satellite(clock: Arc<MockClock>, seed: u64) -> Self {
        Self::new(
            clock,
            seed,
            Duration::from_millis(100),
            10,
            GilbertElliottChannel::new(0.05, 0.4, 0.04, 0.5),
        )
    }

    /// Add a bottleneck link model with finite capacity and buffer.
    pub fn with_link(mut self, capacity_bps: f64, max_queue: usize) -> Self {
        self.link = Some(LinkModel::new(capacity_bps, max_queue));
        self
    }

    /// WiFi with congestion: 10 Mbps, 20-pkt buffer.
    pub fn wifi_congested(clock: Arc<MockClock>, seed: u64) -> Self {
        Self::wifi(clock, seed).with_link(10_000_000.0 / 8.0, 20) // 10 Mbps
    }

    /// LTE with congestion: 2 Mbps, 10-pkt buffer.
    pub fn lte_congested(clock: Arc<MockClock>, seed: u64) -> Self {
        Self::lte(clock, seed).with_link(2_000_000.0 / 8.0, 10) // 2 Mbps
    }

    /// Send a symbol through the channel. Returns true if not dropped.
    pub fn send(&mut self, symbol: WireSymbol) -> bool {
        let seq = self.next_seq;
        self.next_seq += 1;

        if self.ge.should_drop(&mut self.rng) {
            return false;
        }

        let now = self.clock.now();

        // Link model: check for tail-drop and add queuing delay
        let queue_delay = if let Some(ref mut link) = self.link {
            match link.enqueue(now, SYMBOL_SIZE_BYTES + symbol.data.len()) {
                Some(delay) => delay,
                None => return false, // tail-dropped
            }
        } else {
            Duration::ZERO
        };

        let jitter = if self.jitter_ms > 0 {
            Duration::from_millis(self.rng.gen_range(0..=self.jitter_ms))
        } else {
            Duration::ZERO
        };

        let delivery_time = now + self.base_delay + jitter + queue_delay;

        self.in_flight.push(Reverse(SimPacket {
            delivery_time,
            symbol,
            seq,
        }));

        true
    }

    /// Deliver all packets whose delivery_time <= clock.now().
    pub fn deliver(&mut self) -> Vec<SimPacket> {
        let now = self.clock.now();
        let mut delivered = Vec::new();

        while let Some(Reverse(pkt)) = self.in_flight.peek() {
            if pkt.delivery_time <= now {
                delivered.push(self.in_flight.pop().unwrap().0);
                if let Some(ref mut link) = self.link {
                    link.dequeue();
                }
            } else {
                break;
            }
        }

        delivered
    }

    /// Number of packets still in flight.
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.len()
    }

    /// Mutable reference to the GE channel (for correlated fading).
    pub fn ge_mut(&mut self) -> &mut GilbertElliottChannel {
        &mut self.ge
    }

    /// Tail-drop count from the link model (0 if no link model).
    pub fn tail_drop_count(&self) -> u64 {
        self.link.as_ref().map_or(0, |l| l.tail_drops)
    }

    /// Base delay of this channel.
    pub fn base_delay(&self) -> Duration {
        self.base_delay
    }
}

// ---------------------------------------------------------------------------
// ReliableSimChannel — retransmission-based reliable delivery
// ---------------------------------------------------------------------------

/// Reliable channel that retransmits on loss (models QUIC/TCP behavior).
/// Packets always arrive eventually, but with added delay for retransmissions.
/// Optionally includes a LinkModel for congestion/tail-drop simulation.
pub struct ReliableSimChannel {
    clock: Arc<MockClock>,
    rng: ChaCha8Rng,
    in_flight: BinaryHeap<Reverse<SimPacket>>,
    base_delay: Duration,
    jitter_ms: u64,
    ge: GilbertElliottChannel,
    retransmit_delay: Duration,
    max_retries: u32,
    next_seq: u64,
    total_transmissions: u64,
    total_unique: u64,
    link: Option<LinkModel>,
    tail_drops: u64,
}

impl ReliableSimChannel {
    pub fn new(
        clock: Arc<MockClock>,
        seed: u64,
        base_delay: Duration,
        jitter_ms: u64,
        ge: GilbertElliottChannel,
        retransmit_delay: Duration,
        max_retries: u32,
    ) -> Self {
        Self {
            clock,
            rng: ChaCha8Rng::seed_from_u64(seed),
            in_flight: BinaryHeap::new(),
            base_delay,
            jitter_ms,
            ge,
            retransmit_delay,
            max_retries,
            next_seq: 0,
            total_transmissions: 0,
            total_unique: 0,
            link: None,
            tail_drops: 0,
        }
    }

    /// Datacenter preset: 1ms delay, 0 jitter, ~0.1% loss, 2ms retransmit.
    pub fn datacenter(clock: Arc<MockClock>, seed: u64) -> Self {
        Self::new(
            clock,
            seed,
            Duration::from_millis(1),
            0,
            GilbertElliottChannel::new(0.0, 1.0, 0.001, 0.0),
            Duration::from_millis(2),
            5,
        )
    }

    /// WiFi preset: 5ms delay, 3ms jitter, bursty loss, 10ms retransmit.
    pub fn wifi(clock: Arc<MockClock>, seed: u64) -> Self {
        Self::new(
            clock,
            seed,
            Duration::from_millis(5),
            3,
            GilbertElliottChannel::new(0.03, 0.5, 0.01, 0.3),
            Duration::from_millis(10),
            5,
        )
    }

    /// LTE preset: 20ms delay, 5ms jitter, bursty loss, 40ms retransmit.
    pub fn lte(clock: Arc<MockClock>, seed: u64) -> Self {
        Self::new(
            clock,
            seed,
            Duration::from_millis(20),
            5,
            GilbertElliottChannel::new(0.02, 0.25, 0.005, 0.4),
            Duration::from_millis(40),
            5,
        )
    }

    /// Satellite preset: 100ms delay, 10ms jitter, 8% loss, 200ms retransmit.
    pub fn satellite(clock: Arc<MockClock>, seed: u64) -> Self {
        Self::new(
            clock,
            seed,
            Duration::from_millis(100),
            10,
            GilbertElliottChannel::new(0.05, 0.4, 0.04, 0.5),
            Duration::from_millis(200),
            8,
        )
    }

    /// Add a bottleneck link model with finite capacity and buffer.
    pub fn with_link(mut self, capacity_bps: f64, max_queue: usize) -> Self {
        self.link = Some(LinkModel::new(capacity_bps, max_queue));
        self
    }

    /// WiFi with congestion: 10 Mbps, 20-pkt buffer.
    pub fn wifi_congested(clock: Arc<MockClock>, seed: u64) -> Self {
        Self::wifi(clock, seed).with_link(10_000_000.0 / 8.0, 20)
    }

    /// LTE with congestion: 2 Mbps, 10-pkt buffer.
    pub fn lte_congested(clock: Arc<MockClock>, seed: u64) -> Self {
        Self::lte(clock, seed).with_link(2_000_000.0 / 8.0, 10)
    }

    /// Send a symbol through the reliable channel. Always delivers eventually.
    /// Returns the number of transmissions needed (1 = no retransmit).
    pub fn send(&mut self, symbol: WireSymbol) -> u32 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.total_unique += 1;

        let mut attempts = 0u32;
        let mut extra_delay = Duration::ZERO;
        let mut link_delay = Duration::ZERO;
        let pkt_size = SYMBOL_SIZE_BYTES + symbol.data.len();

        loop {
            attempts += 1;
            self.total_transmissions += 1;

            // Stage 1: GE loss check
            if self.ge.should_drop(&mut self.rng) {
                if attempts > self.max_retries {
                    break; // forced delivery
                }
                extra_delay += self.retransmit_delay;
                continue;
            }

            // Stage 2: Link model tail-drop check
            if let Some(ref mut link) = self.link {
                match link.enqueue(self.clock.now(), pkt_size) {
                    Some(qd) => {
                        link_delay = qd;
                        break; // enqueued
                    }
                    None => {
                        self.tail_drops += 1;
                        if attempts > self.max_retries {
                            break; // forced delivery
                        }
                        extra_delay += self.retransmit_delay;
                        continue;
                    }
                }
            } else {
                break; // no link model, packet through
            }
        }

        let jitter = if self.jitter_ms > 0 {
            Duration::from_millis(self.rng.gen_range(0..=self.jitter_ms))
        } else {
            Duration::ZERO
        };

        let now = self.clock.now();
        let delivery_time = now + self.base_delay + jitter + extra_delay + link_delay;

        self.in_flight.push(Reverse(SimPacket {
            delivery_time,
            symbol,
            seq,
        }));

        attempts
    }

    /// Deliver all packets whose delivery_time <= clock.now().
    pub fn deliver(&mut self) -> Vec<SimPacket> {
        let now = self.clock.now();
        let mut delivered = Vec::new();

        while let Some(Reverse(pkt)) = self.in_flight.peek() {
            if pkt.delivery_time <= now {
                delivered.push(self.in_flight.pop().unwrap().0);
                if let Some(ref mut link) = self.link {
                    link.dequeue();
                }
            } else {
                break;
            }
        }

        delivered
    }

    /// Total transmission attempts (including retransmissions).
    pub fn total_transmissions(&self) -> u64 {
        self.total_transmissions
    }

    /// Total unique packets sent.
    pub fn total_unique(&self) -> u64 {
        self.total_unique
    }

    /// Base delay of this channel.
    pub fn base_delay(&self) -> Duration {
        self.base_delay
    }

    /// Number of packets still in flight.
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.len()
    }

    /// Tail-drop count from the link model (0 if no link model).
    pub fn tail_drop_count(&self) -> u64 {
        self.link.as_ref().map_or(0, |l| l.tail_drops) + self.tail_drops
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a single WireSymbol for testing with configurable data size.
pub fn make_wire_symbol_sized(id: u32, repair: bool, data_size: usize) -> WireSymbol {
    WireSymbol {
        block_id: 0,
        payload_id: id,
        is_repair: repair,
        data: vec![0u8; data_size],
        backend: FecBackend::Rlc,
    }
}

/// Create a single WireSymbol for testing (64B default).
pub fn make_wire_symbol(id: u32, repair: bool) -> WireSymbol {
    make_wire_symbol_sized(id, repair, 64)
}

/// Create a batch of source WireSymbols.
pub fn make_source_batch(count: u32) -> Vec<WireSymbol> {
    (0..count).map(|i| make_wire_symbol(i, false)).collect()
}

/// Create a batch of repair WireSymbols.
pub fn make_repair_batch(count: u32) -> Vec<WireSymbol> {
    (0..count).map(|i| make_wire_symbol(i, true)).collect()
}

// ---------------------------------------------------------------------------
// ge_for_target_loss — derive GE parameters from target avg loss + burst len
// ---------------------------------------------------------------------------

/// Create a GE channel with a given target average loss rate and mean burst length.
///
/// Uses loss_good=0.0, loss_bad=0.7 for realistic partial loss in bad state.
/// Derives p_gb and p_bg from:
///   mean_burst_len = 1/p_bg  →  p_bg = 1/mean_burst_len
///   avg_loss = π_bad × loss_bad  (since loss_good=0)
///   π_bad = p_gb/(p_gb+p_bg) = target_loss/loss_bad
///   p_gb = π_bad × p_bg / (1 - π_bad)
pub fn ge_for_target_loss(target_loss: f64, mean_burst_len: f64) -> GilbertElliottChannel {
    let loss_good = 0.0;
    let loss_bad = 0.7;
    let p_bg = 1.0 / mean_burst_len;
    let pi_bad = target_loss / loss_bad;
    let pi_bad = pi_bad.min(0.99); // clamp to avoid division by zero
    let p_gb = pi_bad * p_bg / (1.0 - pi_bad);
    GilbertElliottChannel::new(p_gb, p_bg, loss_good, loss_bad)
}

// ---------------------------------------------------------------------------
// UniformChannel — fixed-rate packet loss (no Gilbert-Elliott state)
// ---------------------------------------------------------------------------

/// Simple uniform-random packet loss channel for loss-sweep benchmarks.
pub struct UniformChannel {
    pub loss_rate: f64,
    rng: ChaCha8Rng,
}

impl UniformChannel {
    pub fn new(loss_rate: f64, seed: u64) -> Self {
        Self {
            loss_rate,
            rng: ChaCha8Rng::seed_from_u64(seed),
        }
    }

    /// Filter a slice, independently dropping each element with probability `loss_rate`.
    pub fn apply<T: Clone>(&mut self, symbols: &[T]) -> Vec<T> {
        symbols
            .iter()
            .filter(|_| self.rng.gen::<f64>() >= self.loss_rate)
            .cloned()
            .collect()
    }
}

// ---------------------------------------------------------------------------
// TrialStats — collects f64 samples and computes mean / stddev / 95% CI
// ---------------------------------------------------------------------------

pub struct TrialStats {
    samples: Vec<f64>,
}

impl TrialStats {
    pub fn new() -> Self {
        Self {
            samples: Vec::new(),
        }
    }

    pub fn push(&mut self, val: f64) {
        self.samples.push(val);
    }

    pub fn mean(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        self.samples.iter().sum::<f64>() / self.samples.len() as f64
    }

    pub fn stddev(&self) -> f64 {
        if self.samples.len() < 2 {
            return 0.0;
        }
        let m = self.mean();
        let var =
            self.samples.iter().map(|v| (v - m).powi(2)).sum::<f64>() / (self.samples.len() - 1) as f64;
        var.sqrt()
    }

    /// 95% confidence interval half-width: 1.96 * stddev / sqrt(n).
    pub fn ci95(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        1.96 * self.stddev() / (self.samples.len() as f64).sqrt()
    }

    /// Format as "mean +/- ci95" (both values as f64, 1 decimal).
    pub fn fmt_ci(&self) -> String {
        format!("{:.1} +/- {:.1}", self.mean(), self.ci95())
    }
}
