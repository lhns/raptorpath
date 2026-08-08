//! Injectable clock for testability.
//!
//! Production code uses `WallClock` (wraps `Instant::now()`).
//! Tests use `MockClock` to advance time instantly, eliminating
//! `thread::sleep` calls and timing-dependent flakiness.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Trait for obtaining the current time.
pub trait Clock: Send + Sync + std::fmt::Debug {
    fn now(&self) -> Instant;
}

/// Real wall-clock time.
#[derive(Debug)]
pub struct WallClock;

impl Clock for WallClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Mock clock for tests — time only advances when `advance()` is called.
// Constructed by the scheduler's own unit tests AND by the integration-test
// harness (`tests/common/mod.rs`, gate_suite, bench_suite). Integration tests
// are separate crates, so the lib's dead_code lint cannot see those uses.
#[allow(dead_code)]
#[derive(Debug)]
pub struct MockClock {
    current: Mutex<Instant>,
}

impl MockClock {
    pub fn new() -> Self {
        Self {
            current: Mutex::new(Instant::now()),
        }
    }

    /// Advance the clock by the given duration.
    pub fn advance(&self, d: Duration) {
        let mut t = self.current.lock().unwrap();
        *t += d;
    }
}

impl Clock for MockClock {
    fn now(&self) -> Instant {
        *self.current.lock().unwrap()
    }
}
