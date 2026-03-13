//! Tests for streaming encode/decode behavior.
//!
//! METTLE is designed for streaming: source packets arrive one at a time,
//! and the decoder can recover packets before all coded symbols arrive.

use mettle::{MettleConfig, MettleDecoder, MettleEncoder};

#[test]
fn streaming_encode_one_at_a_time() {
    let config = MettleConfig::small_window();
    let seed = 42;
    let mut encoder = MettleEncoder::new(config, seed);

    // Add packets one at a time and check state after each
    for i in 0..10 {
        encoder.add_source_packet(&vec![i as u8; 100]);
        assert_eq!(encoder.num_source(), i + 1);
        assert_eq!(encoder.source_packets().len(), i + 1);

        // Can generate coded packets at any point
        let coded = encoder.coded_packets();
        assert!(!coded.is_empty());
    }
}

#[test]
fn streaming_decode_incremental_recovery() {
    let config = MettleConfig::small_window();
    let seed = 42;
    let num = 20;

    let packets: Vec<Vec<u8>> = (0..num).map(|i| vec![i as u8; 100]).collect();

    let mut encoder = MettleEncoder::new(config, seed);
    for pkt in &packets {
        encoder.add_source_packet(pkt);
    }
    let coded = encoder.coded_packets();

    let mut decoder = MettleDecoder::new(config, num, seed);

    // Track recovery progress as we feed symbols
    let mut recovery_milestones = vec![];

    // Feed source packets one at a time, recording recovery count
    for (i, pkt) in packets.iter().enumerate() {
        if i != 5 && i != 10 && i != 15 {
            decoder.add_source_packet(i, pkt);
            recovery_milestones.push(decoder.num_recovered());
        }
    }

    // Recovery should increase monotonically
    for window in recovery_milestones.windows(2) {
        assert!(window[1] >= window[0], "Recovery should be monotonically increasing");
    }

    // Feed coded to recover the rest
    for cp in &coded {
        decoder.add_coded_packet(cp);
        if decoder.is_complete() {
            break;
        }
    }

    assert!(decoder.is_complete());
}

#[test]
fn partial_coded_sufficient() {
    // You shouldn't need ALL coded packets — just enough to cover losses
    let config = MettleConfig::small_window();
    let seed = 42;
    let num = 20;

    let packets: Vec<Vec<u8>> = (0..num).map(|i| vec![i as u8; 100]).collect();

    let mut encoder = MettleEncoder::new(config, seed);
    for pkt in &packets {
        encoder.add_source_packet(pkt);
    }
    let coded = encoder.coded_packets();

    // Drop one source packet
    let mut decoder = MettleDecoder::new(config, num, seed);
    for (i, pkt) in packets.iter().enumerate() {
        if i != 7 {
            decoder.add_source_packet(i, pkt);
        }
    }

    // Feed coded packets one at a time until recovered
    let mut coded_needed = 0;
    for cp in &coded {
        coded_needed += 1;
        decoder.add_coded_packet(cp);
        if decoder.is_complete() {
            break;
        }
    }

    assert!(decoder.is_complete());
    // Should need far fewer coded packets than total available
    assert!(
        coded_needed < coded.len(),
        "Needed {coded_needed}/{} coded packets for 1 loss — should be much fewer",
        coded.len()
    );
}

#[test]
fn interleaved_source_and_coded() {
    // In a real network, source and coded packets arrive interleaved
    let config = MettleConfig::small_window();
    let seed = 42;
    let num = 20;

    let packets: Vec<Vec<u8>> = (0..num).map(|i| vec![i as u8; 100]).collect();

    let mut encoder = MettleEncoder::new(config, seed);
    for pkt in &packets {
        encoder.add_source_packet(pkt);
    }
    let coded = encoder.coded_packets();

    let mut decoder = MettleDecoder::new(config, num, seed);
    let mut coded_iter = coded.iter();

    // Simulate interleaved arrival: 3 source, 1 coded, repeat
    let mut source_idx = 0;
    let dropped = vec![5, 12]; // these source packets are "lost"

    while !decoder.is_complete() {
        // Feed up to 3 source packets
        for _ in 0..3 {
            if source_idx < num {
                if !dropped.contains(&source_idx) {
                    decoder.add_source_packet(source_idx, &packets[source_idx]);
                }
                source_idx += 1;
            }
        }
        // Feed 1 coded packet
        if let Some(cp) = coded_iter.next() {
            decoder.add_coded_packet(cp);
        } else {
            break; // ran out of coded packets
        }
    }

    assert!(
        decoder.is_complete(),
        "Interleaved decode failed: {}/{}",
        decoder.num_recovered(),
        num
    );
}
