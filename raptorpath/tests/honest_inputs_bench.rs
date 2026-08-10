//! goal-gate "Honest Inputs" — component validation (MEASUREMENT DISCIPLINE
//! 14) for the TWO dishonest measured inputs the three-term arc left
//! standing, at the pub-API level (no transport, no tokio, no VM):
//!
//!   (K) `EchoRatioMin` under jitter: the shipped K feed observes the
//!       SMOOTHED srtt at the 5 ms refresh clock, and the minimum of a
//!       smoothed series sits near the distribution's MEAN, not its floor —
//!       the measured jit25 inversion (`[3T]` window ×1.34/1.38 its
//!       pre-registered value, the OPPOSITE of the pre-registered "min
//!       reads the low end"). This bench generates a deterministic jittered
//!       echo series in the jit25 shape (40 ms base ± 25 ms uniform),
//!       reproduces the bias in the shipped feeding, and shows the
//!       `RWM_HONEST_K` feeding (RAW ratio at the sample clock, same
//!       tracker/window/clamp/guard) reads the floor.
//!
//!   (engine wiring) `PathState::k_raw()` — proves the mechanism executes
//!       at the real feed site (`record_rtt_sample`), two-sided: with
//!       `RWM_HONEST_K` unset it must be `None` (the OFF-value property
//!       through the pub API); under `RWM_HONEST_K=1` (a separate
//!       invocation — the gate resolves once per process) it must read the
//!       jittered stream's floor.
//!
//! The RATE-ANCHOR fix's cost curve (`RWM_HONEST_ANCHOR` — the c1 −35%
//! mechanism) is measured by the in-crate bench
//! `scheduler::tests::bw_filter_cost_is_quadratic_legacy_and_linear_fixed`
//! (both arms in ONE process via the test hooks):
//!
//! ```text
//!   cargo test --release -p raptorpath --lib -- --ignored --nocapture bw_filter_cost
//! ```
//!
//! and its value-equivalence is unit-pinned by
//! `bw_mono_front_equals_full_window_fold` (runs in every `cargo test`).
//!
//! Run this file's tests (they are deterministic and fast — they run in the
//! normal suite; the ON-arm wiring check needs its own invocation):
//!
//! ```text
//!   cargo test -p raptorpath --test honest_inputs_bench -- --nocapture
//!   RWM_HONEST_K=1 cargo test -p raptorpath --test honest_inputs_bench -- --nocapture
//! ```

use std::time::Duration;

use raptorpath::net::{EchoRatioMin, PERCAP_K_HALF_WINDOW_US};
use raptorpath::scheduler::{MockClock, PathState};
use std::sync::Arc;

/// Deterministic uniform [0,1) stream (LCG — no rand dependency).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f64) / (u32::MAX as f64)
    }
}

/// The jit25 delay shape: base 40 ms round trip, ±25 ms uniform jitter,
/// floored at netem's negative-delay clamp class.
fn jittered_rtt(u: &mut Lcg) -> Duration {
    let jitter_ms = 50.0 * u.next() - 25.0;
    Duration::from_secs_f64((0.040 + jitter_ms / 1e3).max(0.000_07))
}

/// (K) The bias, REPRODUCED then REMOVED, on the pure estimator: both
/// estimators are the SAME `EchoRatioMin` (same window, same ≥ 1 clamp,
/// same seed-identity guard); the ONLY difference is the fed series —
/// smoothed-at-refresh (shipped) vs raw-at-sample (`RWM_HONEST_K`). That
/// isolation is the fix's zero-constant claim in executable form.
#[test]
fn k_jitter_bias_reproduced_and_removed() {
    let mut u = Lcg(42);
    let mut smoothed_fed = EchoRatioMin::new(PERCAP_K_HALF_WINDOW_US);
    let mut raw_fed = EchoRatioMin::new(PERCAP_K_HALF_WINDOW_US);
    let mut srtt: Option<f64> = None; // RFC 6298 EWMA α = 1/8 (the engine's)
    let mut rtprop: Option<f64> = None; // min over RAW samples (the engine's)
    let mut now_us: u64 = 0;
    // 12 s at a 5 ms ack cadence — past one full two-half-bucket window, so
    // the reported min excludes the seeding transient exactly as the
    // engine's rolling window does.
    for _ in 0..2400 {
        now_us += 5_000;
        let raw = jittered_rtt(&mut u).as_secs_f64();
        srtt = Some(match srtt {
            Some(s) => s * 0.875 + raw * 0.125,
            None => raw,
        });
        rtprop = Some(rtprop.map_or(raw, |m: f64| m.min(raw)));
        let rtp = Duration::from_secs_f64(rtprop.unwrap());
        // Shipped feed: the SMOOTHED series at the refresh clock.
        smoothed_fed.observe_srtt_over_rtprop(
            Duration::from_secs_f64(srtt.unwrap()),
            Some(rtp),
            now_us,
        );
        // RWM_HONEST_K feed: the RAW sample at the sample clock.
        raw_fed.observe_srtt_over_rtprop(Duration::from_secs_f64(raw), Some(rtp), now_us);
    }
    let k_smoothed = smoothed_fed.k();
    let k_raw = raw_fed.k();
    // The three-term window term is LINEAR in K, so K's inflation IS the
    // jit25 limit inflation class.
    println!(
        "[HONEST-K-BENCH] jit25 shape (40ms ± 25ms): K_smoothed = {k_smoothed:.3} \
         (the shipped read; measured battery inflation ×1.34/1.38), \
         K_raw = {k_raw:.3} (the RWM_HONEST_K read; floor = 1.0)"
    );
    assert!(
        k_smoothed > 1.2,
        "the smoothed-series min must read HIGH under ±25 ms jitter \
         (the jit25 inversion reproduced): {k_smoothed}"
    );
    assert!(
        k_raw < 1.05,
        "the raw-series min must read the distribution floor: {k_raw}"
    );
    assert!(
        k_smoothed / k_raw > 1.2,
        "the bias is the smoothing's and the raw feed removes it: \
         {k_smoothed} vs {k_raw}"
    );
}

/// (K, jitterless control) Where the delay distribution is NARROW the two
/// feedings must agree — the fix may not disturb the cells that were
/// already reading honestly (the 5–9.9 k band's K, sc2-class). A standing
/// +4 ms wire queue on an 8 ms floor: both estimators read ≈ 1.5.
#[test]
fn k_agrees_with_the_smoothed_feed_where_there_is_no_jitter() {
    let mut smoothed_fed = EchoRatioMin::new(PERCAP_K_HALF_WINDOW_US);
    let mut raw_fed = EchoRatioMin::new(PERCAP_K_HALF_WINDOW_US);
    let mut srtt: Option<f64> = None;
    let mut now_us: u64 = 0;
    let rtp = Duration::from_millis(8);
    for i in 0..2400 {
        now_us += 5_000;
        // Constant 12 ms echo (8 ms floor + 4 ms standing queue), with the
        // floor itself seen once at start (the un-queued first flight).
        let raw = if i == 0 { 0.008_2 } else { 0.012 };
        srtt = Some(match srtt {
            Some(s) => s * 0.875 + raw * 0.125,
            None => raw,
        });
        smoothed_fed.observe_srtt_over_rtprop(
            Duration::from_secs_f64(srtt.unwrap()),
            Some(rtp),
            now_us,
        );
        raw_fed.observe_srtt_over_rtprop(Duration::from_secs_f64(raw), Some(rtp), now_us);
    }
    let ks = smoothed_fed.k();
    let kr = raw_fed.k();
    println!("[HONEST-K-BENCH] narrow distribution: K_smoothed = {ks:.3}, K_raw = {kr:.3}");
    assert!(
        (ks - kr).abs() / ks < 0.05,
        "no-jitter cells must be undisturbed: smoothed {ks} vs raw {kr}"
    );
}

/// (engine wiring) `PathState::k_raw()` at the REAL feed site, two-sided on
/// the process's own gate resolution (MEASUREMENT DISCIPLINE 1: prove the
/// mechanism under test executes — or provably does not, on the control).
#[test]
fn k_raw_engine_wiring_follows_the_gate() {
    let gate_on = std::env::var("RWM_HONEST_K").map_or(false, |v| {
        let v = v.trim();
        !(v.is_empty() || v == "0" || v.eq_ignore_ascii_case("false"))
    }) || std::env::var("RWM_ANCHOR_HYGIENE").map_or(false, |v| {
        let v = v.trim();
        !(v.is_empty() || v == "0" || v.eq_ignore_ascii_case("false"))
    });
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    let mut u = Lcg(7);
    for _ in 0..2400 {
        clock.advance(Duration::from_millis(5));
        path.record_rtt_sample(jittered_rtt(&mut u));
    }
    match path.k_raw() {
        None => {
            assert!(
                !gate_on,
                "gate resolved ON but k_raw() is None — the wiring is dead"
            );
            println!("[HONEST-K-BENCH] RWM_HONEST_K off: k_raw() = None (OFF-value property holds)");
        }
        Some(k) => {
            assert!(
                gate_on,
                "k_raw() is Some without the gate — the OFF arm is contaminated"
            );
            println!("[HONEST-K-BENCH] RWM_HONEST_K on: k_raw() = {k:.3}");
            assert!(
                k < 1.05,
                "the raw-fed K must read the jittered stream's floor: {k}"
            );
        }
    }
}
