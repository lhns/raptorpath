//! Window-pipeline (RLC sliding window) recovery across Gilbert-Elliott
//! channel scenarios — the window half of the former
//! `fec_realworld_recovery_test.rs`, whose block-codec parts went with the
//! block pipeline (ADR-0069).
//!
//! Run with: cargo test --test window_realworld_recovery_test -- --nocapture
//!
//! The printed table is a characterization; the asserted claim is the
//! benign-channel one: on the Datacenter scenario (0.1% i.i.d. loss, no bad
//! state) a 2x-loss repair budget recovers 100% of lost packets.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use raptorpath::fec::{RlcWindowDecoder, RlcWindowEncoder, WindowDecoder, WindowEncoder, WireSymbol};
use std::collections::BTreeSet;

const NUM_TRIALS: u64 = 10;
const SYMBOL_SIZE: u16 = 1200;

struct GilbertElliottChannel {
    p_gb: f64,
    p_bg: f64,
    loss_good: f64,
    loss_bad: f64,
}

impl GilbertElliottChannel {
    fn apply<T: Clone>(&self, symbols: &[T], rng: &mut ChaCha8Rng) -> (Vec<T>, BTreeSet<usize>) {
        use rand::Rng;
        let mut in_bad = false;
        let mut surviving = Vec::new();
        let mut dropped = BTreeSet::new();

        for (i, sym) in symbols.iter().enumerate() {
            let loss_prob = if in_bad { self.loss_bad } else { self.loss_good };
            if rng.gen::<f64>() < loss_prob {
                dropped.insert(i);
            } else {
                surviving.push(sym.clone());
            }
            let transition: f64 = rng.gen();
            if in_bad {
                if transition < self.p_bg {
                    in_bad = false;
                }
            } else if transition < self.p_gb {
                in_bad = true;
            }
        }

        (surviving, dropped)
    }
}

struct Scenario {
    name: &'static str,
    channel: GilbertElliottChannel,
    stationary_loss: f64,
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "Datacenter",
            channel: GilbertElliottChannel {
                p_gb: 0.0,
                p_bg: 1.0,
                loss_good: 0.001,
                loss_bad: 0.0,
            },
            stationary_loss: 0.001,
        },
        Scenario {
            name: "WiFi",
            channel: GilbertElliottChannel {
                p_gb: 0.03,
                p_bg: 0.5,
                loss_good: 0.01,
                loss_bad: 0.3,
            },
            stationary_loss: 0.025,
        },
        Scenario {
            name: "LTE",
            channel: GilbertElliottChannel {
                p_gb: 0.02,
                p_bg: 0.25,
                loss_good: 0.005,
                loss_bad: 0.4,
            },
            stationary_loss: 0.035,
        },
        Scenario {
            name: "Congested",
            channel: GilbertElliottChannel {
                p_gb: 0.08,
                p_bg: 0.15,
                loss_good: 0.05,
                loss_bad: 0.6,
            },
            stationary_loss: 0.12,
        },
    ]
}

fn window_recovery_rlc(scenario: &Scenario) -> f64 {
    let num_symbols = 500usize;
    let mut total_lost = 0usize;
    let mut total_recovered = 0usize;

    for seed in 0..NUM_TRIALS {
        let mut encoder = RlcWindowEncoder::new(SYMBOL_SIZE);
        let packet_data: Vec<Vec<u8>> = (0..num_symbols)
            .map(|i| vec![(i % 256) as u8; 1000])
            .collect();
        let sources: Vec<WireSymbol> = packet_data
            .iter()
            .map(|pkt| encoder.add_source(pkt))
            .collect();

        let repair_count = (num_symbols as f64 * scenario.stationary_loss * 2.0)
            .ceil() as usize;
        let repair_count = repair_count.max(5);
        let repairs: Vec<WireSymbol> = (0..repair_count)
            .map(|_| encoder.generate_repair())
            .collect();

        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let (surviving, dropped) = scenario.channel.apply(&sources, &mut rng);

        let mut decoder = RlcWindowDecoder::new(SYMBOL_SIZE);
        let mut recovered_seqs = BTreeSet::new();

        for sym in &surviving {
            for (seq, _) in decoder.add_symbol(sym) {
                recovered_seqs.insert(seq);
            }
        }
        for sym in &repairs {
            for (seq, _) in decoder.add_symbol(sym) {
                recovered_seqs.insert(seq);
            }
        }

        let lost_seqs: BTreeSet<u64> = dropped.iter().map(|&i| i as u64).collect();
        total_lost += lost_seqs.len();
        total_recovered += lost_seqs
            .iter()
            .filter(|s| recovered_seqs.contains(s))
            .count();
    }

    if total_lost == 0 {
        100.0
    } else {
        total_recovered as f64 / total_lost as f64 * 100.0
    }
}

#[test]
fn window_realworld_recovery() {
    let scenarios = scenarios();
    println!(
        "\n=== Window-mode FEC Recovery (500 symbols, 2x loss overhead, {} trials) ===",
        NUM_TRIALS
    );
    println!(
        "{:>16} {:>12} {:>12} {:>12} {:>12}",
        "", scenarios[0].name, scenarios[1].name, scenarios[2].name, scenarios[3].name
    );

    {
        let rates: Vec<f64> = scenarios.iter().map(|s| window_recovery_rlc(s)).collect();
        println!(
            "{:>16} {:>11.1}% {:>11.1}% {:>11.1}% {:>11.1}%",
            "RLC Window", rates[0], rates[1], rates[2], rates[3]
        );
        assert_eq!(
            rates[0], 100.0,
            "[RLC Window] Datacenter (0.1% iid loss, 2x-loss repair budget) \
             must recover every lost packet"
        );
    }
}
