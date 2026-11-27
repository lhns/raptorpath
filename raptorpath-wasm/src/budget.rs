//! The resend budget (addendum 1 A5 of
//! docs/derivations/visualizer-three-knobs-plan.md; three-knobs §15.4): at
//! most `MAX_NACK_REPAIRS_PER_NACK = 10` resends per
//! `NACK_REPAIR_COOLDOWN_US = 5 ms` window (raptorpath/src/net/mod.rs:227,
//! :229), with the congestion multiplier held at 1 and the `nack_cap·sources`
//! term omitted, as in the offline simulation (toy2.py `send_copy`,
//! `resend_brake = "engine"`).
//!
//! Windows are 5 ticks aligned at tick 0, in integer arithmetic. The toy
//! windows by `int(now / 0.005)` in float seconds, which can round down at an
//! exact multiple of 5 ms; `tests/link_vectors.txt` records each such case and
//! the integer form is the model's. The limit is a value: ∞ (the default)
//! admits every resend.

/// The engine's limit, the named point of the page's budget dial.
pub const ENGINE_RESENDS_PER_WINDOW: f64 = 10.0;
/// The window length in ticks (5 ms at the model's 1 ms tick).
pub const WINDOW_TICKS: u32 = 5;

/// The budget window of tick `tick`.
pub fn window(tick: u32) -> usize {
    (tick / WINDOW_TICKS) as usize
}

/// Whether one more resend fits a window that already carried `used`: at
/// most `limit` per window, `used + 1 ≤ limit`. This is the engine's and the
/// toy's `used < 10` at an integer limit; at a fractional one (the page's dial)
/// it admits floor(limit), never more than the limit.
pub fn admits(used: u32, limit: f64) -> bool {
    (used as f64) + 1.0 <= limit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_and_admission() {
        assert_eq!((window(0), window(4), window(5), window(9), window(10)), (0, 0, 1, 1, 2));
        assert!(admits(9, ENGINE_RESENDS_PER_WINDOW) && !admits(10, ENGINE_RESENDS_PER_WINDOW));
        assert!(admits(u32::MAX, f64::INFINITY));
        assert!(!admits(0, 0.0));
        // ±2 % around the engine's point: the admitted count per window is
        // floor(limit), so 9.8 and 10.2 admit 9 and 10 (a count is a
        // staircase in the limit; the engine's 10 sits on a step edge).
        let n = |lim: f64| (0..100).take_while(|&u| admits(u, lim)).count();
        assert_eq!((n(9.8), n(10.0), n(10.2)), (9, 10, 10));
        assert_eq!((n(0.0), n(0.99), n(1.0), n(1.11)), (0, 0, 1, 1));
    }
}
