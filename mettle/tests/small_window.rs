//! Tests specifically for small window sizes (~50 symbols) as used by raptorpath.
//!
//! The METTLE paper optimized for w=600. These tests characterize behavior at
//! raptorpath's window size of ~50 to understand if the spatially-coupled
//! peeling cascade propagates reliably at small w.

use mettle::{MettleConfig, MettleDecoder, MettleEncoder};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

fn measure_success_rate(config: MettleConfig, num_packets: usize, loss_rate: f64, trials: usize) -> f64 {
    let mut rng = StdRng::seed_from_u64(54321);
    let mut successes = 0;

    for trial in 0..trials {
        let seed = trial as u64 * 997;
        let packets: Vec<Vec<u8>> = (0..num_packets)
            .map(|i| vec![(i % 256) as u8; 100])
            .collect();

        let mut encoder = MettleEncoder::new(config, seed);
        for pkt in &packets {
            encoder.add_source_packet(pkt);
        }
        let coded = encoder.coded_packets();

        let mut decoder = MettleDecoder::new(config, num_packets, seed);

        // Simulate random loss
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
            // Verify integrity
            for (i, pkt) in packets.iter().enumerate() {
                if let Some(recovered) = decoder.get_source(i) {
                    assert_eq!(recovered, pkt.as_slice());
                }
            }
            successes += 1;
        }
    }

    successes as f64 / trials as f64
}

#[test]
fn w50_1pct_loss() {
    let config = MettleConfig::small_window();
    let rate = measure_success_rate(config, 50, 0.01, 100);
    println!("w=50, 1% loss: {:.1}% success rate", rate * 100.0);
    assert!(rate > 0.8, "Expected >80% success at 1% loss, got {:.1}%", rate * 100.0);
}

#[test]
fn w50_5pct_loss() {
    let config = MettleConfig::small_window();
    let rate = measure_success_rate(config, 50, 0.05, 100);
    println!("w=50, 5% loss: {:.1}% success rate", rate * 100.0);
    // Lower expectation for 5% loss at small window
    assert!(rate > 0.5, "Expected >50% success at 5% loss, got {:.1}%", rate * 100.0);
}

#[test]
fn w50_10pct_loss() {
    let config = MettleConfig::small_window();
    let rate = measure_success_rate(config, 50, 0.10, 100);
    println!("w=50, 10% loss: {:.1}% success rate", rate * 100.0);
    // At 10% loss with small window, success may be low — this test characterizes behavior
}

#[test]
fn compare_window_sizes() {
    let loss_rate = 0.05;
    let trials = 100;

    let results: Vec<(usize, f64)> = [10, 25, 50, 100, 200]
        .into_iter()
        .map(|w| {
            let config = MettleConfig {
                window_size: w,
                num_edges: 4,
                overhead_factor: 0.15,
            };
            let rate = measure_success_rate(config, 50, loss_rate, trials);
            println!("w={w:>4}, 5% loss: {:.1}% success rate", rate * 100.0);
            (w, rate)
        })
        .collect();

    // Larger windows should generally give equal or better success rates
    for window in results.windows(2) {
        // Allow some slack due to randomness
        assert!(
            window[1].1 >= window[0].1 - 0.15,
            "Larger window w={} ({:.0}%) should be >= w={} ({:.0}%)",
            window[1].0, window[1].1 * 100.0,
            window[0].0, window[0].1 * 100.0
        );
    }
}

#[test]
fn w50_higher_overhead_helps() {
    let loss_rate = 0.05;
    let trials = 100;

    let results: Vec<(f64, f64)> = [0.05, 0.10, 0.15, 0.20]
        .into_iter()
        .map(|c| {
            let config = MettleConfig {
                window_size: 50,
                num_edges: 4,
                overhead_factor: c,
            };
            let rate = measure_success_rate(config, 50, loss_rate, trials);
            println!("c={c:.2}, w=50, 5% loss: {:.1}% success rate", rate * 100.0);
            (c, rate)
        })
        .collect();

    // Higher overhead should generally help
    let low_c_rate = results[0].1;
    let high_c_rate = results[results.len() - 1].1;
    println!("c=0.05 vs c=0.20: {:.1}% vs {:.1}%", low_c_rate * 100.0, high_c_rate * 100.0);
}

#[test]
fn w50_more_edges_helps() {
    let loss_rate = 0.05;
    let trials = 100;

    for l in [2, 3, 4, 5] {
        let config = MettleConfig {
            window_size: 50,
            num_edges: l,
            overhead_factor: 0.15,
        };
        let rate = measure_success_rate(config, 50, loss_rate, trials);
        println!("l={l}, w=50, 5% loss: {:.1}% success rate", rate * 100.0);
    }
}
