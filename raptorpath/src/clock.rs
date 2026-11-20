//! SIM SPIKE (throwaway, branch `sim-spike`): the engine's one clock and
//! one RNG. Every std `Instant::now()` / `.elapsed()` / `rand::random` read
//! in engine code goes through here, so a simulation can put them on tokio's
//! paused clock and a seeded generator. Without the `sim` feature this is
//! exactly `std::time::Instant::now()` and `rand::random()`.

use std::time::Instant;

#[cfg(feature = "sim")]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};

/// Set by the simulation harness: reads follow tokio's (paused) clock.
#[cfg(feature = "sim")]
pub static SIM_CLOCK: AtomicBool = AtomicBool::new(false);
/// Set by the simulation harness: `random_f64` is the seeded SplitMix64.
#[cfg(feature = "sim")]
pub static SIM_RNG: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "sim")]
pub static SIM_RNG_STATE: AtomicU64 = AtomicU64::new(0);

#[inline]
pub fn now() -> Instant {
    #[cfg(feature = "sim")]
    if SIM_CLOCK.load(Relaxed) {
        return tokio::time::Instant::now().into_std();
    }
    Instant::now()
}

/// `.elapsed()` on the engine clock (std `Instant::elapsed` reads the std
/// clock directly, so it would not follow the paused clock).
pub trait SimElapsed {
    fn sim_elapsed(&self) -> std::time::Duration;
}

impl SimElapsed for Instant {
    #[inline]
    fn sim_elapsed(&self) -> std::time::Duration {
        now().saturating_duration_since(*self)
    }
}

impl SimElapsed for tokio::time::Instant {
    #[inline]
    fn sim_elapsed(&self) -> std::time::Duration {
        self.elapsed()
    }
}

/// The wall-clock base (µs since the epoch). Pinned under the sim.
pub fn wall_us() -> u64 {
    #[cfg(feature = "sim")]
    if SIM_CLOCK.load(Relaxed) {
        return 1_700_000_000_000_000;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

/// Uniform [0, 1).
pub fn random_f64() -> f64 {
    #[cfg(feature = "sim")]
    if SIM_RNG.load(Relaxed) {
        let s = SIM_RNG_STATE.fetch_add(0x9E37_79B9_7F4A_7C15, Relaxed).wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        return (z >> 11) as f64 / (1u64 << 53) as f64;
    }
    rand::random::<f64>()
}

/// Per-path maps whose iteration order drives a decision or a spawn order:
/// ordered under the sim (std's `RandomState` order differs per process).
#[cfg(feature = "sim")]
pub type PathMap<K, V> = std::collections::BTreeMap<K, V>;
#[cfg(not(feature = "sim"))]
pub type PathMap<K, V> = std::collections::HashMap<K, V>;
