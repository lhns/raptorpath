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

// ---------------------------------------------------------------------------
// Gilbert-Elliott channel model
// ---------------------------------------------------------------------------

/// Two-state Markov loss model: Good state (low loss) ↔ Bad state (high loss).
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

/// Deterministic network channel simulator with delay, jitter, and GE loss.
pub struct SimChannel {
    clock: Arc<MockClock>,
    rng: ChaCha8Rng,
    in_flight: BinaryHeap<Reverse<SimPacket>>,
    base_delay: Duration,
    jitter_ms: u64,
    ge: GilbertElliottChannel,
    next_seq: u64,
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
        }
    }

    /// Datacenter preset: 1ms delay, 0 jitter, ~0.1% uniform loss.
    pub fn datacenter(clock: Arc<MockClock>, seed: u64) -> Self {
        Self::new(
            clock,
            seed,
            Duration::from_millis(1),
            0,
            GilbertElliottChannel::new(0.0, 1.0, 0.001, 0.0),
        )
    }

    /// WiFi preset: 5ms delay, 3ms jitter, bursty loss (~2.5% avg).
    pub fn wifi(clock: Arc<MockClock>, seed: u64) -> Self {
        Self::new(
            clock,
            seed,
            Duration::from_millis(5),
            3,
            GilbertElliottChannel::new(0.03, 0.5, 0.01, 0.3),
        )
    }

    /// LTE preset: 20ms delay, 5ms jitter, bursty loss (~3.5% avg).
    pub fn lte(clock: Arc<MockClock>, seed: u64) -> Self {
        Self::new(
            clock,
            seed,
            Duration::from_millis(20),
            5,
            GilbertElliottChannel::new(0.02, 0.25, 0.005, 0.4),
        )
    }

    /// Send a symbol through the channel. Returns true if not dropped.
    pub fn send(&mut self, symbol: WireSymbol) -> bool {
        let seq = self.next_seq;
        self.next_seq += 1;

        if self.ge.should_drop(&mut self.rng) {
            return false;
        }

        let jitter = if self.jitter_ms > 0 {
            Duration::from_millis(self.rng.gen_range(0..=self.jitter_ms))
        } else {
            Duration::ZERO
        };

        let now = self.clock.now();
        let delivery_time = now + self.base_delay + jitter;

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
}

// ---------------------------------------------------------------------------
// ReliableSimChannel — retransmission-based reliable delivery
// ---------------------------------------------------------------------------

/// Reliable channel that retransmits on loss (models QUIC/TCP behavior).
/// Packets always arrive eventually, but with added delay for retransmissions.
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

    /// Send a symbol through the reliable channel. Always delivers eventually.
    /// Returns the number of transmissions needed (1 = no retransmit).
    pub fn send(&mut self, symbol: WireSymbol) -> u32 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.total_unique += 1;

        let mut attempts = 0u32;
        let mut extra_delay = Duration::ZERO;

        loop {
            attempts += 1;
            self.total_transmissions += 1;

            if !self.ge.should_drop(&mut self.rng) || attempts > self.max_retries {
                // Packet gets through (or forced delivery after max retries)
                break;
            }
            extra_delay += self.retransmit_delay;
        }

        let jitter = if self.jitter_ms > 0 {
            Duration::from_millis(self.rng.gen_range(0..=self.jitter_ms))
        } else {
            Duration::ZERO
        };

        let now = self.clock.now();
        let delivery_time = now + self.base_delay + jitter + extra_delay;

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
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a single WireSymbol for testing.
pub fn make_wire_symbol(id: u32, repair: bool) -> WireSymbol {
    WireSymbol {
        block_id: 0,
        payload_id: id,
        is_repair: repair,
        data: vec![0u8; 64],
        backend: FecBackend::Rlc,
    }
}

/// Create a batch of source WireSymbols.
pub fn make_source_batch(count: u32) -> Vec<WireSymbol> {
    (0..count).map(|i| make_wire_symbol(i, false)).collect()
}

/// Create a batch of repair WireSymbols.
pub fn make_repair_batch(count: u32) -> Vec<WireSymbol> {
    (0..count).map(|i| make_wire_symbol(i, true)).collect()
}
