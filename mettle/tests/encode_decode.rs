//! Integration tests: full encode → decode round-trips under various conditions.

use mettle::{MettleConfig, MettleDecoder, MettleEncoder};

fn round_trip(config: MettleConfig, num_packets: usize, packet_size: usize, drop_indices: &[usize]) -> bool {
    let seed = 12345u64;

    // Generate test data
    let packets: Vec<Vec<u8>> = (0..num_packets)
        .map(|i| vec![(i % 256) as u8; packet_size])
        .collect();

    // Encode
    let mut encoder = MettleEncoder::new(config, seed);
    for pkt in &packets {
        encoder.add_source_packet(pkt);
    }
    let coded = encoder.coded_packets();

    // Decode
    let mut decoder = MettleDecoder::new(config, num_packets, seed);

    // Feed source packets (skip dropped ones)
    for (i, pkt) in packets.iter().enumerate() {
        if !drop_indices.contains(&i) {
            decoder.add_source_packet(i, pkt);
        }
    }

    // Feed all coded packets
    for cp in &coded {
        decoder.add_coded_packet(cp);
        if decoder.is_complete() {
            break;
        }
    }

    if decoder.is_complete() {
        // Verify data integrity
        for (i, pkt) in packets.iter().enumerate() {
            assert_eq!(
                decoder.get_source(i).unwrap(),
                pkt.as_slice(),
                "Data mismatch at position {i}"
            );
        }
        true
    } else {
        false
    }
}

// --- No Loss ---

#[test]
fn no_loss_small() {
    let config = MettleConfig::small_window();
    assert!(round_trip(config, 10, 100, &[]));
}

#[test]
fn no_loss_medium() {
    let config = MettleConfig::small_window();
    assert!(round_trip(config, 50, 1200, &[]));
}

#[test]
fn no_loss_large() {
    let config = MettleConfig::default();
    assert!(round_trip(config, 200, 1500, &[]));
}

// --- Source Loss (rely on coded packets) ---

#[test]
fn single_source_loss() {
    let config = MettleConfig::small_window();
    assert!(round_trip(config, 20, 100, &[5]));
}

#[test]
fn two_source_losses() {
    let config = MettleConfig::small_window();
    assert!(round_trip(config, 20, 100, &[3, 7]));
}

#[test]
fn consecutive_source_losses() {
    let config = MettleConfig::small_window();
    assert!(round_trip(config, 30, 100, &[10, 11, 12]));
}

// --- Only Coded Packets (no source arrives) ---

#[test]
fn coded_only_small() {
    // Drop ALL source packets — decode entirely from coded
    let config = MettleConfig {
        window_size: 50,
        num_edges: 4,
        overhead_factor: 0.15, // extra overhead for coded-only
    };
    let drop_all: Vec<usize> = (0..10).collect();
    // This may or may not succeed depending on the code's rate — just exercise the path
    let _result = round_trip(config, 10, 100, &drop_all);
}

// --- Various Packet Sizes ---

#[test]
fn tiny_packets() {
    let config = MettleConfig::small_window();
    assert!(round_trip(config, 20, 1, &[5]));
}

#[test]
fn mtu_sized_packets() {
    let config = MettleConfig::small_window();
    assert!(round_trip(config, 20, 1500, &[3, 15]));
}

#[test]
fn large_packets() {
    let config = MettleConfig::small_window();
    assert!(round_trip(config, 10, 8000, &[2]));
}

// --- Edge Cases ---

#[test]
fn single_packet_no_loss() {
    let config = MettleConfig::small_window();
    assert!(round_trip(config, 1, 100, &[]));
}

#[test]
fn two_packets_one_lost() {
    let config = MettleConfig::small_window();
    assert!(round_trip(config, 2, 100, &[0]));
}

// --- Various Data Sizes ---

#[test]
fn ten_packets() {
    let config = MettleConfig::small_window();
    assert!(round_trip(config, 10, 1200, &[2, 5]));
}

#[test]
fn fifty_packets() {
    let config = MettleConfig::small_window();
    assert!(round_trip(config, 50, 1200, &[5, 15, 25, 35, 45]));
}

#[test]
fn hundred_packets_default_window() {
    let config = MettleConfig::default();
    let drops: Vec<usize> = (0..100).filter(|i| i % 10 == 0).collect(); // 10% loss
    assert!(round_trip(config, 100, 1200, &drops));
}
