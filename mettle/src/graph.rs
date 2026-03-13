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
//!   `Binomial((1+c)*w, 1/2^(i-1))`. This places later edges progressively closer to
//!   the source packet's position, creating the spatial coupling that enables peeling.

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
        let mut rng = SmallRng::seed_from_u64(seed.wrapping_mul(2654435761).wrapping_add(x as u64));

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
/// approximation of `Binomial(n, p)` with `n = (1+c)*w` and `p = 1/2^(edge_idx-1)`.
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
    let p = 1.0 / (1u64 << (edge_idx - 1)) as f64; // 1/2^(i-1) for edge i (1-indexed from 2nd)

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
}
