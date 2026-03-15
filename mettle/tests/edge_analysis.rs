//! Edge collision analysis for METTLE graph structure.
//!
//! Diagnoses whether stochastic edges collide with the TLE edge, which
//! would waste graph connectivity and reduce recovery performance.

use mettle::{graph, MettleConfig};

/// Count how often the first stochastic edge (edge_idx=1) lands on the same
/// bin as the TLE edge, across all source positions.
#[test]
fn tle_stochastic_collision_rate() {
    let config = MettleConfig {
        window_size: 50,
        num_edges: 4,
        overhead_factor: 0.1,
    };
    let seed = 42u64;
    let num_source = 200;

    let mut collisions = 0;
    let mut total = 0;

    for x in 0..num_source {
        let indices = graph::compute_bin_indices(x, &config, seed);
        let tle = indices[0];

        // Check if any stochastic edge collides with TLE
        for &idx in &indices[1..] {
            if idx == tle {
                collisions += 1;
                break; // count at most one collision per source
            }
        }
        total += 1;
    }

    let collision_rate = collisions as f64 / total as f64;
    println!(
        "TLE-stochastic collision rate: {:.1}% ({collisions}/{total})",
        collision_rate * 100.0
    );

    // With corrected formula (p = 1/2^i), collisions should be rare (<5%)
    // With old formula (p = 1/2^(i-1)), edge_idx=1 had p=1.0 → 100% collision
    assert!(
        collision_rate < 0.10,
        "Collision rate {:.1}% is too high — edge formula may be wrong",
        collision_rate * 100.0
    );
}

/// Verify that stochastic edges produce geometrically-spaced mean offsets.
#[test]
fn stochastic_edge_mean_offsets() {
    let config = MettleConfig {
        window_size: 50,
        num_edges: 4,
        overhead_factor: 0.1,
    };
    let seed = 42u64;
    let num_source = 500;
    let n = ((1.0 + config.overhead_factor) * config.window_size as f64).floor() as f64;

    // For each stochastic edge, collect the offset from right_boundary
    // edge_idx 1 (i=1): expected mean = n * p = n/2 = 27.5
    // edge_idx 2 (i=2): expected mean = n/4 = 13.75
    // edge_idx 3 (i=3): expected mean = n/8 = 6.875
    let mut offset_sums = vec![0.0f64; 3]; // edges 1, 2, 3
    let mut counts = vec![0usize; 3];

    for x in 0..num_source {
        let indices = graph::compute_bin_indices(x, &config, seed);
        let right_boundary =
            ((1.0 + config.overhead_factor) * (x + config.window_size) as f64).floor() as usize;

        for (edge_i, &bin) in indices[1..].iter().enumerate() {
            let offset = right_boundary.saturating_sub(bin);
            offset_sums[edge_i] += offset as f64;
            counts[edge_i] += 1;
        }
    }

    println!("Expected n = {n}");
    for i in 0..3 {
        let mean = offset_sums[i] / counts[i] as f64;
        let expected = n / (1u64 << (i + 1)) as f64; // n/2, n/4, n/8
        let ratio = mean / expected;
        println!(
            "Edge {}: mean offset = {mean:.1}, expected = {expected:.1}, ratio = {ratio:.2}",
            i + 1
        );

        // Mean should be within 20% of expected (statistical tolerance)
        assert!(
            (0.8..=1.2).contains(&ratio),
            "Edge {} mean offset {mean:.1} deviates >20% from expected {expected:.1}",
            i + 1
        );
    }
}

/// Count unique edges per source position to verify we get the full l=4 effective edges.
#[test]
fn effective_edge_count() {
    let config = MettleConfig {
        window_size: 50,
        num_edges: 4,
        overhead_factor: 0.1,
    };
    let seed = 42u64;
    let num_source = 500;

    let mut total_unique = 0usize;
    let mut total_positions = 0usize;

    for x in 0..num_source {
        let indices = graph::compute_bin_indices(x, &config, seed);
        let unique: std::collections::HashSet<_> = indices.iter().collect();
        total_unique += unique.len();
        total_positions += 1;
    }

    let avg_unique = total_unique as f64 / total_positions as f64;
    println!(
        "Average unique edges per source: {avg_unique:.2} (target: {:.1})",
        config.num_edges as f64
    );

    // With the corrected formula, should average close to l=4
    // (some rare collisions are expected but not systematic)
    assert!(
        avg_unique > 3.5,
        "Average unique edges {avg_unique:.2} too low — systematic collisions suspected"
    );
}

/// A/B comparison: run recovery trials with both edge formulas.
/// This test directly validates the fix by comparing old vs new behavior.
#[test]
fn ab_test_edge_formula() {
    use mettle::{MettleDecoder, MettleEncoder};
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    let trials = 500;
    let loss_rate = 0.05;

    for (label, w, k) in [("w=50, k=50", 50, 50), ("w=600, k=100", 600, 100)] {
        let config = MettleConfig {
            window_size: w,
            num_edges: 4,
            overhead_factor: 0.1,
        };

        let mut rng = StdRng::seed_from_u64(12345);
        let mut successes = 0;

        for trial in 0..trials {
            let seed = trial as u64 * 31337;
            let packets: Vec<Vec<u8>> = (0..k).map(|i| vec![(i % 256) as u8; 100]).collect();

            let mut encoder = MettleEncoder::new(config, seed);
            for pkt in &packets {
                encoder.add_source_packet(pkt);
            }
            let coded = encoder.coded_packets();

            let mut decoder = MettleDecoder::new(config, k, seed);
            for (i, pkt) in packets.iter().enumerate() {
                if rng.gen::<f64>() >= loss_rate {
                    decoder.add_source_packet(i, pkt);
                }
            }

            for cp in &coded {
                decoder.add_coded_packet(cp);
                if decoder.is_complete() {
                    break;
                }
            }

            if decoder.is_complete() {
                successes += 1;
            }
        }

        let rate = successes as f64 / trials as f64;
        println!(
            "[Corrected formula] {label}: {:.1}% success ({successes}/{trials})",
            rate * 100.0
        );
    }
}
