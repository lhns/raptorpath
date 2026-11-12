//! Pacing of packet transmissions.

use crate::{Duration, Instant};

use tracing::warn;

/// A simple token-bucket pacer
///
/// The pacer's capacity is derived on a fraction of the congestion window
/// which can be sent in regular intervals
/// Once the bucket is empty, further transmission is blocked.
/// The bucket refills at a rate slightly faster
/// than one congestion window per RTT, as recommended in
/// <https://tools.ietf.org/html/draft-ietf-quic-recovery-34#section-7.7>
pub(super) struct Pacer {
    capacity: u64,
    last_window: u64,
    last_mtu: u16,
    /// The controller rate `capacity` was last derived from (`None`: from the window)
    last_rate: Option<u64>,
    tokens: u64,
    prev: Instant,
}

impl Pacer {
    /// Obtains a new [`Pacer`].
    pub(super) fn new(smoothed_rtt: Duration, window: u64, mtu: u16, now: Instant) -> Self {
        let capacity = optimal_capacity(smoothed_rtt, window, mtu);
        Self {
            capacity,
            last_window: window,
            last_mtu: mtu,
            last_rate: None,
            tokens: capacity,
            prev: now,
        }
    }

    /// Record that a packet has been transmitted.
    pub(super) fn on_transmit(&mut self, packet_length: u16) {
        self.tokens = self.tokens.saturating_sub(packet_length.into())
    }

    /// Return how long we need to wait before sending `bytes_to_send`
    ///
    /// If we can send a packet right away, this returns `None`. Otherwise, returns `Some(d)`,
    /// where `d` is the time before this function should be called again.
    ///
    /// The 5/4 ratio used here comes from the suggestion that N = 1.25 in the draft IETF RFC for
    /// QUIC.
    ///
    /// `rate` is the controller's pacing rate in bytes per second. With `Some(r)`, `r > 0`, the
    /// bucket refills at exactly `r` and its capacity is `r` x 2 ms; otherwise the window-derived
    /// rate above applies unchanged.
    pub(super) fn delay(
        &mut self,
        smoothed_rtt: Duration,
        bytes_to_send: u64,
        mtu: u16,
        window: u64,
        rate: Option<u64>,
        now: Instant,
    ) -> Option<Instant> {
        debug_assert_ne!(
            window, 0,
            "zero-sized congestion control window is nonsense"
        );

        let rate = rate.filter(|&r| r > 0);

        if window != self.last_window || mtu != self.last_mtu || rate != self.last_rate {
            self.capacity = match rate {
                Some(rate) => rate_capacity(rate, mtu),
                None => optimal_capacity(smoothed_rtt, window, mtu),
            };

            // Clamp the tokens
            self.tokens = self.capacity.min(self.tokens);
            self.last_window = window;
            self.last_mtu = mtu;
            self.last_rate = rate;
        }

        // if we can already send a packet, there is no need for delay
        if self.tokens >= bytes_to_send {
            return None;
        }

        if let Some(rate) = rate {
            return self.delay_at_rate(rate, bytes_to_send, now);
        }

        // we disable pacing for extremely large windows
        if window > u64::from(u32::MAX) {
            return None;
        }

        let window = window as u32;

        let time_elapsed = now.checked_duration_since(self.prev).unwrap_or_else(|| {
            warn!("received a timestamp early than a previous recorded time, ignoring");
            Default::default()
        });

        if smoothed_rtt.as_nanos() == 0 {
            return None;
        }

        let elapsed_rtts = time_elapsed.as_secs_f64() / smoothed_rtt.as_secs_f64();
        let new_tokens = window as f64 * 1.25 * elapsed_rtts;
        self.tokens = self
            .tokens
            .saturating_add(new_tokens as _)
            .min(self.capacity);

        self.prev = now;

        // if we can already send a packet, there is no need for delay
        if self.tokens >= bytes_to_send {
            return None;
        }

        let unscaled_delay = smoothed_rtt
            .checked_mul((bytes_to_send.max(self.capacity) - self.tokens) as _)
            .unwrap_or(Duration::MAX)
            / window;

        // divisions come before multiplications to prevent overflow
        // this is the time at which the pacing window becomes empty
        Some(self.prev + (unscaled_delay / 5) * 4)
    }
}

impl Pacer {
    /// The controller-rate arm of [`Pacer::delay`]: refill at `rate` bytes/s, no window/RTT needed
    fn delay_at_rate(&mut self, rate: u64, bytes_to_send: u64, now: Instant) -> Option<Instant> {
        let time_elapsed = now.checked_duration_since(self.prev).unwrap_or_else(|| {
            warn!("received a timestamp early than a previous recorded time, ignoring");
            Default::default()
        });

        let new_tokens = rate as f64 * time_elapsed.as_secs_f64();
        self.tokens = self
            .tokens
            .saturating_add(new_tokens as _)
            .min(self.capacity);

        self.prev = now;

        if self.tokens >= bytes_to_send {
            return None;
        }

        let deficit = bytes_to_send.max(self.capacity) - self.tokens;
        let nanos = (deficit as u128 * 1_000_000_000 / rate as u128).min(u64::MAX as u128) as u64;
        Some(self.prev + Duration::from_nanos(nanos))
    }
}

/// Calculates a pacer capacity for a controller-supplied rate: `rate` x the burst interval,
/// within the same packet-count clamp as [`optimal_capacity`]
fn rate_capacity(rate: u64, mtu: u16) -> u64 {
    let capacity = (rate as u128 * BURST_INTERVAL_NANOS / 1_000_000_000).min(u64::MAX as u128) as u64;
    capacity.clamp(MIN_BURST_SIZE * mtu as u64, MAX_BURST_SIZE * mtu as u64)
}

/// Calculates a pacer capacity for a certain window and RTT
///
/// The goal is to emit a burst (of size `capacity`) in timer intervals
/// which compromise between
/// - ideally distributing datagrams over time
/// - constantly waking up the connection to produce additional datagrams
///
/// Too short burst intervals means we will never meet them since the timer
/// accuracy in user-space is not high enough. If we miss the interval by more
/// than 25%, we will lose that part of the congestion window since no additional
/// tokens for the extra-elapsed time can be stored.
///
/// Too long burst intervals make pacing less effective.
fn optimal_capacity(smoothed_rtt: Duration, window: u64, mtu: u16) -> u64 {
    let rtt = smoothed_rtt.as_nanos().max(1);

    let capacity = ((window as u128 * BURST_INTERVAL_NANOS) / rtt) as u64;

    // Small bursts are less efficient (no GSO), could increase latency and don't effectively
    // use the channel's buffer capacity. Large bursts might block the connection on sending.
    capacity.clamp(MIN_BURST_SIZE * mtu as u64, MAX_BURST_SIZE * mtu as u64)
}

/// The burst interval
///
/// The capacity will we refilled in 4/5 of that time.
/// 2ms is chosen here since framework timers might have 1ms precision.
/// If kernel-level pacing is supported later a higher time here might be
/// more applicable.
const BURST_INTERVAL_NANOS: u128 = 2_000_000; // 2ms

/// Allows some usage of GSO, and doesn't slow down the handshake.
const MIN_BURST_SIZE: u64 = 10;

/// Creating 256 packets took 1ms in a benchmark, so larger bursts don't make sense.
const MAX_BURST_SIZE: u64 = 256;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_not_panic_on_bad_instant() {
        let old_instant = Instant::now();
        let new_instant = old_instant + Duration::from_micros(15);
        let rtt = Duration::from_micros(400);

        assert!(
            Pacer::new(rtt, 30000, 1500, new_instant)
                .delay(Duration::from_micros(0), 0, 1500, 1, None, old_instant)
                .is_none()
        );
        assert!(
            Pacer::new(rtt, 30000, 1500, new_instant)
                .delay(Duration::from_micros(0), 1600, 1500, 1, None, old_instant)
                .is_none()
        );
        assert!(
            Pacer::new(rtt, 30000, 1500, new_instant)
                .delay(Duration::from_micros(0), 1500, 1500, 3000, None, old_instant)
                .is_none()
        );
    }

    #[test]
    fn derives_initial_capacity() {
        let window = 2_000_000;
        let mtu = 1500;
        let rtt = Duration::from_millis(50);
        let now = Instant::now();

        let pacer = Pacer::new(rtt, window, mtu, now);
        assert_eq!(
            pacer.capacity,
            (window as u128 * BURST_INTERVAL_NANOS / rtt.as_nanos()) as u64
        );
        assert_eq!(pacer.tokens, pacer.capacity);

        let pacer = Pacer::new(Duration::from_millis(0), window, mtu, now);
        assert_eq!(pacer.capacity, MAX_BURST_SIZE * mtu as u64);
        assert_eq!(pacer.tokens, pacer.capacity);

        let pacer = Pacer::new(rtt, 1, mtu, now);
        assert_eq!(pacer.capacity, MIN_BURST_SIZE * mtu as u64);
        assert_eq!(pacer.tokens, pacer.capacity);
    }

    #[test]
    fn adjusts_capacity() {
        let window = 2_000_000;
        let mtu = 1500;
        let rtt = Duration::from_millis(50);
        let now = Instant::now();

        let mut pacer = Pacer::new(rtt, window, mtu, now);
        assert_eq!(
            pacer.capacity,
            (window as u128 * BURST_INTERVAL_NANOS / rtt.as_nanos()) as u64
        );
        assert_eq!(pacer.tokens, pacer.capacity);
        let initial_tokens = pacer.tokens;

        pacer.delay(rtt, mtu as u64, mtu, window * 2, None, now);
        assert_eq!(
            pacer.capacity,
            (2 * window as u128 * BURST_INTERVAL_NANOS / rtt.as_nanos()) as u64
        );
        assert_eq!(pacer.tokens, initial_tokens);

        pacer.delay(rtt, mtu as u64, mtu, window / 2, None, now);
        assert_eq!(
            pacer.capacity,
            (window as u128 / 2 * BURST_INTERVAL_NANOS / rtt.as_nanos()) as u64
        );
        assert_eq!(pacer.tokens, initial_tokens / 2);

        pacer.delay(rtt, mtu as u64, mtu * 2, window, None, now);
        assert_eq!(
            pacer.capacity,
            (window as u128 * BURST_INTERVAL_NANOS / rtt.as_nanos()) as u64
        );

        pacer.delay(rtt, mtu as u64, 20_000, window, None, now);
        assert_eq!(pacer.capacity, 20_000_u64 * MIN_BURST_SIZE);
    }

    #[test]
    fn computes_pause_correctly() {
        let window = 2_000_000u64;
        let mtu = 1000;
        let rtt = Duration::from_millis(50);
        let old_instant = Instant::now();

        let mut pacer = Pacer::new(rtt, window, mtu, old_instant);
        let packet_capacity = pacer.capacity / mtu as u64;

        for _ in 0..packet_capacity {
            assert_eq!(
                pacer.delay(rtt, mtu as u64, mtu, window, None, old_instant),
                None,
                "When capacity is available packets should be sent immediately"
            );

            pacer.on_transmit(mtu);
        }

        let pace_duration = Duration::from_nanos((BURST_INTERVAL_NANOS * 4 / 5) as u64);

        assert_eq!(
            pacer
                .delay(rtt, mtu as u64, mtu, window, None, old_instant)
                .expect("Send must be delayed")
                .duration_since(old_instant),
            pace_duration
        );

        // Refill half of the tokens
        assert_eq!(
            pacer.delay(
                rtt,
                mtu as u64,
                mtu,
                window,
                None,
                old_instant + pace_duration / 2
            ),
            None
        );
        assert_eq!(pacer.tokens, pacer.capacity / 2);

        for _ in 0..packet_capacity / 2 {
            assert_eq!(
                pacer.delay(rtt, mtu as u64, mtu, window, None, old_instant),
                None,
                "When capacity is available packets should be sent immediately"
            );

            pacer.on_transmit(mtu);
        }

        // Refill all capacity by waiting more than the expected duration
        assert_eq!(
            pacer.delay(
                rtt,
                mtu as u64,
                mtu,
                window,
                None,
                old_instant + pace_duration * 3 / 2
            ),
            None
        );
        assert_eq!(pacer.tokens, pacer.capacity);
    }

    /// A controller rate replaces the window-derived refill rate: absolute token and delay values.
    #[test]
    fn controller_rate_sets_refill_and_capacity() {
        let window = 2_000_000u64; // would give 1.25 * 2e6 / 0.05 = 50 MB/s if it were used
        let mtu = 1000u16;
        let rtt = Duration::from_millis(50);
        let rate = 1_000_000u64; // 1 MB/s -> 1000 B/ms
        let t0 = Instant::now();

        let mut pacer = Pacer::new(rtt, window, mtu, t0);
        // Rate capacity: 1e6 B/s * 2 ms = 2000 B, floored at 10 MTU
        assert_eq!(pacer.delay(rtt, mtu as u64, mtu, window, Some(rate), t0), None);
        assert_eq!(pacer.capacity, MIN_BURST_SIZE * mtu as u64);
        assert_eq!(pacer.tokens, pacer.capacity);

        // Drain the bucket; an empty bucket waits for a full capacity (10_000 B) at 1 MB/s
        pacer.tokens = 0;
        let wait = pacer
            .delay(rtt, mtu as u64, mtu, window, Some(rate), t0)
            .expect("send must be delayed")
            .duration_since(t0);
        assert_eq!(wait, Duration::from_millis(10));

        // Half a capacity refills in 5 ms, exactly
        assert_eq!(
            pacer.delay(rtt, mtu as u64, mtu, window, Some(rate), t0 + Duration::from_millis(5)),
            None
        );
        assert_eq!(pacer.tokens, 5_000);

        // A high rate lifts the capacity above the floor: 100 MB/s * 2 ms = 200 kB
        let high = 100_000_000u64;
        pacer.delay(rtt, mtu as u64, mtu, window, Some(high), t0 + Duration::from_millis(5));
        assert_eq!(pacer.capacity, 200_000);
    }

    /// The window-disable rule (windows above u32::MAX) and zero RTT do not apply to a rate,
    /// and `Some(0)` falls back to the window law.
    #[test]
    fn controller_rate_ignores_window_and_rtt() {
        let mtu = 1000u16;
        let rtt = Duration::from_millis(50);
        let t0 = Instant::now();
        let huge = u64::from(u32::MAX) + 1;

        let mut pacer = Pacer::new(rtt, huge, mtu, t0);
        pacer.tokens = 0;
        // Window law: pacing disabled for huge windows
        assert_eq!(pacer.delay(rtt, mtu as u64, mtu, huge, None, t0), None);
        assert_eq!(pacer.delay(rtt, mtu as u64, mtu, huge, Some(0), t0), None);
        // Rate law: paced, even with zero smoothed RTT
        assert!(pacer.delay(Duration::ZERO, mtu as u64, mtu, huge, Some(1_000_000), t0).is_some());
    }

    /// The stock controllers keep the window-derived pacing (no rate), so nothing changes by default.
    #[test]
    fn stock_controllers_supply_no_rate() {
        use crate::congestion::{BbrConfig, ControllerFactory, CubicConfig, NewRenoConfig};
        use std::sync::Arc;
        let now = Instant::now();
        assert_eq!(Arc::new(NewRenoConfig::default()).build(now, 1200).pacing_rate(), None);
        assert_eq!(Arc::new(CubicConfig::default()).build(now, 1200).pacing_rate(), None);
        assert_eq!(Arc::new(BbrConfig::default()).build(now, 1200).pacing_rate(), None);
    }
}
