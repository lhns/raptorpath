//! Hash-based Tanner graph edge generation for METTLE.
//!
//! Each source packet at position `x` is "thrown" into `l` bins. The bin indices
//! are computed via:
//!
//! - **Edge 1 (TLE — Touch-less Leading Edge)**: deterministic at `floor((1+c) * x)`.
//!   Since consecutive TLE bins have distance `1+c > 1`, no two source packets share
//!   the same TLE bin. This guarantees a starting point for peeling.
//!
//! - **Edges 2..l**: stochastic placement using a hash-seeded RNG. The i-th edge lands
//!   at distance `η_i` from the right boundary of the window, where `η_i` is drawn from
//!   `Binomial((1+c)*w, 1/2^i)`. This gives geometrically-spaced spatial coupling:
//!   i=1 → mean offset n/2, i=2 → n/4, i=3 → n/8, placing edges progressively closer
//!   to the source packet's position.

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use crate::MettleConfig;

/// Compute the bin indices for source packet at position `x`.
///
/// Returns `l` bin indices. The first is always the TLE edge, the rest are stochastic.
/// The total number of bins is approximately `(1+c) * (num_source + w)`.
pub fn compute_bin_indices(x: usize, config: &MettleConfig, seed: u64) -> Vec<usize> {
    let c = config.overhead_factor;
    let w = config.window_size;
    let l = config.num_edges;

    let mut indices = Vec::with_capacity(l);

    // Edge 1: TLE — deterministic
    indices.push(tle_bin(x, c));

    // Edges 2..l: stochastic with binomial placement
    if l > 1 {
        // Seed the RNG with a hash of (seed, x) for reproducibility
        // splitmix64-style combiner for better avalanche than linear hash
        let mut h = seed;
        h = h.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        h ^= x as u64;
        h = h.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let mut rng = SmallRng::seed_from_u64(h);

        let right_boundary = ((1.0 + c) * (x + w) as f64).floor() as usize;

        for i in 1..l {
            let bin = binomial_bin(right_boundary, i, w, c, &mut rng);
            indices.push(bin);
        }
    }

    indices
}

/// Total number of bins needed for `num_source` source packets.
pub fn total_bins(num_source: usize, config: &MettleConfig) -> usize {
    let c = config.overhead_factor;
    let w = config.window_size;
    // Bins span from 0 to (1+c)*(num_source - 1 + w), rounded up with margin
    ((1.0 + c) * (num_source + w) as f64).ceil() as usize + 1
}

/// TLE bin: deterministic edge at floor((1+c) * x).
fn tle_bin(x: usize, c: f64) -> usize {
    ((1.0 + c) * x as f64).floor() as usize
}

/// Stochastic bin placement for edge `edge_idx` (1-indexed, starting from the 2nd edge).
///
/// The landing position is: `right_boundary - η`, where `η` is drawn from an
/// approximation of `Binomial(n, p)` with `n = (1+c)*w` and `p = 1/2^edge_idx`.
///
/// Using `p = 1/2^i` (not `1/2^(i-1)`) gives geometrically-spaced spatial coupling:
/// - i=1: p=0.5   → mean offset = n/2 (halfway through window)
/// - i=2: p=0.25  → mean offset = n/4
/// - i=3: p=0.125 → mean offset = n/8
///
/// The previous formula `1/2^(i-1)` made the first stochastic edge (i=1) use p=1.0,
/// causing η = n deterministically, which placed the bin at exactly the TLE position.
/// After deduplication, this wasted 25% of the graph connectivity.
///
/// For computational simplicity, we sample from the binomial by summing Bernoulli trials
/// (exact for small n, which is the case at typical window sizes).
fn binomial_bin(
    right_boundary: usize,
    edge_idx: usize,
    w: usize,
    c: f64,
    rng: &mut SmallRng,
) -> usize {
    let n = ((1.0 + c) * w as f64).floor() as usize;
    let p = 1.0 / (1u64 << edge_idx) as f64; // 1/2^i for edge i (1-indexed from 2nd)

    // Sample from Binomial(n, p) by summing Bernoulli trials
    let mut eta = 0usize;
    for _ in 0..n {
        if rng.gen::<f64>() < p {
            eta += 1;
        }
    }

    // Bin position = right_boundary - eta, clamped to >= 0
    right_boundary.saturating_sub(eta)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> MettleConfig {
        MettleConfig {
            window_size: 50,
            num_edges: 4,
            overhead_factor: 0.1,
        }
    }

    #[test]
    fn tle_is_deterministic() {
        let config = test_config();
        let a = compute_bin_indices(10, &config, 42);
        let b = compute_bin_indices(10, &config, 42);
        assert_eq!(a, b);
    }

    #[test]
    fn tle_bins_dont_collide() {
        // For consecutive x, TLE bins should be distinct (distance > 1 when c > 0)
        let c = 0.1;
        for x in 0..100 {
            let bin_a = tle_bin(x, c);
            let bin_b = tle_bin(x + 1, c);
            assert!(bin_b > bin_a, "TLE bins must be strictly increasing: x={x}");
        }
    }

    #[test]
    fn correct_number_of_edges() {
        let config = test_config();
        let indices = compute_bin_indices(5, &config, 0);
        assert_eq!(indices.len(), config.num_edges);
    }

    #[test]
    fn tle_formula_correct() {
        // TLE bin for x=10, c=0.1 should be floor(1.1 * 10) = floor(11.0) = 11
        assert_eq!(tle_bin(10, 0.1), 11);
        assert_eq!(tle_bin(0, 0.1), 0);
        assert_eq!(tle_bin(100, 0.1), 110);
    }

    #[test]
    fn bins_within_bounds() {
        let config = test_config();
        let num_source = 100;
        let max_bin = total_bins(num_source, &config);

        for x in 0..num_source {
            let indices = compute_bin_indices(x, &config, 12345);
            for &bin in &indices {
                assert!(
                    bin < max_bin + config.window_size * 2,
                    "Bin {bin} out of bounds for x={x}, max={max_bin}"
                );
            }
        }
    }

    #[test]
    fn different_seeds_give_different_stochastic_edges() {
        let config = test_config();
        let a = compute_bin_indices(50, &config, 1);
        let b = compute_bin_indices(50, &config, 2);
        // TLE edge (index 0) should be the same
        assert_eq!(a[0], b[0]);
        // Stochastic edges should differ (with overwhelming probability)
        assert_ne!(a[1..], b[1..]);
    }

    #[test]
    fn total_bins_scales_with_source_count() {
        let config = test_config();
        let bins_100 = total_bins(100, &config);
        let bins_200 = total_bins(200, &config);
        assert!(bins_200 > bins_100);
        // Should be approximately (1+c) * (n + w)
        let expected_100 = (1.1_f64 * 150.0).ceil() as usize + 1;
        assert_eq!(bins_100, expected_100);
    }

    // === Regression tests for ADR-0028: edge probability off-by-one ===

    #[test]
    fn first_stochastic_edge_probability_not_one() {
        // Regression: the old formula p = 1/2^(edge_idx-1) made edge_idx=1 use p=1.0,
        // so binomial_bin always returned right_boundary - n. With p=0.5 there must be
        // variance across different RNG seeds.
        let config = test_config();
        let w = config.window_size;
        let c = config.overhead_factor;
        let right_boundary = ((1.0 + c) * (50 + w) as f64).floor() as usize;

        let mut results = Vec::with_capacity(100);
        for trial in 0u64..100 {
            let mut rng = SmallRng::seed_from_u64(trial);
            let bin = binomial_bin(right_boundary, 1, w, c, &mut rng);
            results.push(bin);
        }

        // If p were 1.0, every result would be identical (right_boundary - n).
        // With p=0.5, results must vary.
        let first = results[0];
        let all_same = results.iter().all(|&r| r == first);
        assert!(
            !all_same,
            "All 100 trials produced the same bin {first}; p is likely 1.0 (off-by-one bug)"
        );
    }

    #[test]
    fn stochastic_edges_never_systematically_collide_with_tle() {
        // Regression: with p=1.0 the first stochastic edge always landed at the TLE
        // position, causing ~100% collision rate. After the fix (p=0.5) sporadic
        // collisions are fine, but systematic collision indicates a regression.
        let config = test_config();
        let num_positions = 200;
        let mut collisions = 0;

        for x in 0..num_positions {
            let indices = compute_bin_indices(x, &config, 0xDEAD);
            let tle = indices[0];
            // Edge at index 1 is the first stochastic edge (edge_idx=1)
            if indices[1] == tle {
                collisions += 1;
            }
        }

        let collision_rate = collisions as f64 / num_positions as f64;
        assert!(
            collision_rate < 0.50,
            "First stochastic edge collides with TLE {:.0}% of the time \
             (expected < 50%, got {collisions}/{num_positions}). \
             Likely regression to p=1/2^(i-1).",
            collision_rate * 100.0
        );
        // Tighter sanity: should actually be very rare (< 5%)
        assert!(
            collision_rate < 0.05,
            "Collision rate {:.1}% is higher than expected < 5%",
            collision_rate * 100.0
        );
    }

    #[test]
    fn first_stochastic_edge_has_variance() {
        // With p=1.0 (the bug), std dev of eta was exactly 0.
        // With p=0.5, std dev should be sqrt(n*p*(1-p)) ≈ 3.7 for n=55.
        let config = test_config();
        let w = config.window_size;
        let c = config.overhead_factor;
        let right_boundary = ((1.0 + c) * (50 + w) as f64).floor() as usize;
        let n = ((1.0 + c) * w as f64).floor() as usize;

        let trials = 1000;
        let mut values = Vec::with_capacity(trials);
        for trial in 0u64..trials as u64 {
            let mut rng = SmallRng::seed_from_u64(trial.wrapping_mul(7919));
            let bin = binomial_bin(right_boundary, 1, w, c, &mut rng);
            // eta = right_boundary - bin
            values.push((right_boundary - bin) as f64);
        }

        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance =
            values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
        let std_dev = variance.sqrt();

        // Expected std dev ≈ sqrt(n * 0.5 * 0.5) ≈ sqrt(n)/2
        let expected_std = (n as f64 * 0.5 * 0.5).sqrt();

        assert!(
            std_dev > 0.0,
            "Standard deviation is 0 — binomial_bin is deterministic (p=1.0 bug)"
        );
        assert!(
            std_dev > expected_std * 0.5,
            "Standard deviation {std_dev:.2} is too low (expected ~{expected_std:.2})"
        );
    }

    #[test]
    fn edge_probabilities_decrease_geometrically() {
        // Edges 1,2,3 have p=1/2, 1/4, 1/8 so mean offsets (eta) should be n/2, n/4, n/8.
        // Assert mean_offset[0] > mean_offset[1] > mean_offset[2].
        let config = test_config();
        let w = config.window_size;
        let c = config.overhead_factor;
        let right_boundary = ((1.0 + c) * (50 + w) as f64).floor() as usize;

        let trials = 1000;
        let mut mean_offsets = [0.0f64; 3];

        for edge_idx in 1..=3usize {
            let mut total = 0u64;
            for trial in 0u64..trials as u64 {
                let mut rng = SmallRng::seed_from_u64(trial.wrapping_mul(6271));
                let bin = binomial_bin(right_boundary, edge_idx, w, c, &mut rng);
                total += (right_boundary - bin) as u64;
            }
            mean_offsets[edge_idx - 1] = total as f64 / trials as f64;
        }

        assert!(
            mean_offsets[0] > mean_offsets[1],
            "Edge 1 mean offset ({:.1}) should exceed edge 2 ({:.1})",
            mean_offsets[0],
            mean_offsets[1]
        );
        assert!(
            mean_offsets[1] > mean_offsets[2],
            "Edge 2 mean offset ({:.1}) should exceed edge 3 ({:.1})",
            mean_offsets[1],
            mean_offsets[2]
        );

        // Generous check: each mean should be roughly 2x the next (within 50% tolerance)
        let ratio_1_2 = mean_offsets[0] / mean_offsets[1];
        let ratio_2_3 = mean_offsets[1] / mean_offsets[2];
        assert!(
            ratio_1_2 > 1.3 && ratio_1_2 < 3.0,
            "Ratio edge1/edge2 = {ratio_1_2:.2}, expected ~2.0"
        );
        assert!(
            ratio_2_3 > 1.3 && ratio_2_3 < 3.0,
            "Ratio edge2/edge3 = {ratio_2_3:.2}, expected ~2.0"
        );
    }
}
