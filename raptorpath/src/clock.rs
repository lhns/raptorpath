//! The engine's one clock and one RNG (ADR-0082, test-detection plan S8).
//!
//! Every engine `Instant::now()`, std `Instant::elapsed()`, `rand::random`,
//! OS-seeded `StdRng` and quinn endpoint RNG goes through here, so the
//! deterministic simulation (`--features sim`, `crate::sim`) can put them on
//! tokio's paused clock and one seeded generator.
//!
//! Without `sim`, every function is `#[inline(always)]` and its body is
//! literally the expression it replaced: the release binary must not change
//! (release-binary identity is the merge gate). Call sites were rewritten in
//! place, on the same line, so no existing line moves and no panic location
//! in `.rodata` changes.

use std::time::{Duration, Instant};

/// The engine clock. Under `sim` it is tokio's clock, which is the std clock
/// unless the current runtime's time is paused.
#[inline(always)]
pub fn now() -> Instant {
    #[cfg(not(feature = "sim"))]
    {
        Instant::now()
    }
    #[cfg(feature = "sim")]
    {
        tokio::time::Instant::now().into_std()
    }
}

/// `.elapsed()` on the engine clock: std `Instant::elapsed` reads the std
/// clock directly, so it would not follow the paused clock. Imported as
/// `use crate::clock::ClockElapsed as _;` at the end of each using file.
pub trait ClockElapsed {
    fn clock_elapsed(&self) -> Duration;
}

impl ClockElapsed for Instant {
    #[inline(always)]
    fn clock_elapsed(&self) -> Duration {
        #[cfg(not(feature = "sim"))]
        {
            self.elapsed()
        }
        #[cfg(feature = "sim")]
        {
            now().saturating_duration_since(*self)
        }
    }
}

/// The wall-clock base (µs since the epoch) that `net::now_us` adds to the
/// engine clock. Pinned while a simulation is active, so traces carry the
/// same absolute stamps on every run.
#[inline(always)]
pub fn wall_us(real: u64) -> u64 {
    #[cfg(feature = "sim")]
    if sim_active() {
        return SIM_WALL_BASE_US;
    }
    real
}

/// Uniform [0, 1): `rand::random::<f64>()`, or the seeded stream under an
/// active simulation.
#[inline(always)]
pub fn random_f64() -> f64 {
    #[cfg(feature = "sim")]
    if sim_active() {
        return (next_u64() >> 11) as f64 / (1u64 << 53) as f64;
    }
    rand::random::<f64>()
}

/// An OS-seeded `StdRng`, or one seeded from the stream under an active
/// simulation.
#[inline(always)]
pub fn std_rng() -> rand::rngs::StdRng {
    #[cfg(feature = "sim")]
    if sim_active() {
        return <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(next_u64());
    }
    <rand::rngs::StdRng as rand::SeedableRng>::from_entropy()
}

/// The quinn endpoint config for one bound path. Under an active simulation
/// it also swaps the runtime for the in-memory network's and seeds quinn's
/// RNG (connection IDs, packet-number skips).
#[inline(always)]
pub fn endpoint_config(
    runtime: &mut std::sync::Arc<dyn quinn::Runtime>,
    role: &str,
    path_id: crate::scheduler::PathId,
) -> quinn::EndpointConfig {
    #[cfg(not(feature = "sim"))]
    {
        let _ = (runtime, role, path_id);
        quinn::EndpointConfig::default()
    }
    #[cfg(feature = "sim")]
    {
        crate::sim::endpoint_config(runtime, role, path_id)
    }
}

/// Spawn order over per-path ids taken from a hash map: unchanged without
/// `sim`, sorted under it (hash order differs per process).
#[inline(always)]
pub fn path_order<T: Ord>(ids: Vec<T>) -> Vec<T> {
    #[cfg(not(feature = "sim"))]
    {
        ids
    }
    #[cfg(feature = "sim")]
    {
        let mut ids = ids;
        ids.sort_unstable();
        ids
    }
}

/// Per-path maps whose iteration order drives a decision: ordered under
/// `sim` (std's `RandomState` order differs per process), unchanged without.
#[cfg(feature = "sim")]
pub type PathMap<K, V> = std::collections::BTreeMap<K, V>;
#[cfg(not(feature = "sim"))]
pub type PathMap<K, V> = std::collections::HashMap<K, V>;

#[cfg(feature = "sim")]
const SIM_WALL_BASE_US: u64 = 1_700_000_000_000_000;
#[cfg(feature = "sim")]
static SIM_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(feature = "sim")]
static SIM_RNG: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(feature = "sim")]
#[inline]
fn sim_active() -> bool {
    SIM_ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Seed the engine RNG and pin the wall base. Called once per process by
/// `sim::SimNet::install` (the statics are process-global: one scenario
/// per process).
#[cfg(feature = "sim")]
pub fn activate_sim(seed: u64) {
    use std::sync::atomic::Ordering::Relaxed;
    SIM_RNG.store(seed.wrapping_mul(0x2545_F491_4F6C_DD1D), Relaxed);
    SIM_ACTIVE.store(true, Relaxed);
}

/// SplitMix64 over a shared atomic state.
#[cfg(feature = "sim")]
fn next_u64() -> u64 {
    const G: u64 = 0x9E37_79B9_7F4A_7C15;
    let s = SIM_RNG.fetch_add(G, std::sync::atomic::Ordering::Relaxed).wrapping_add(G);
    crate::sim::splitmix_mix(s)
}
