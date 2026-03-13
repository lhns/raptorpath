//! Tests focused on the peeling decoder mechanics.

use mettle::{MettleConfig, MettleDecoder, MettleEncoder};

fn make_encoder_decoder(
    config: MettleConfig,
    num: usize,
    seed: u64,
) -> (Vec<Vec<u8>>, MettleEncoder, MettleDecoder) {
    let packets: Vec<Vec<u8>> = (0..num).map(|i| vec![(i % 256) as u8; 100]).collect();

    let mut encoder = MettleEncoder::new(config, seed);
    for pkt in &packets {
        encoder.add_source_packet(pkt);
    }

    let decoder = MettleDecoder::new(config, num, seed);
    (packets, encoder, decoder)
}

#[test]
fn peeling_cascade_propagates() {
    let config = MettleConfig::small_window();
    let (packets, encoder, mut decoder) = make_encoder_decoder(config, 20, 42);
    let coded = encoder.coded_packets();

    // Feed all source except one
    for (i, pkt) in packets.iter().enumerate() {
        if i != 10 {
            decoder.add_source_packet(i, pkt);
        }
    }

    // Feed coded packets — peeling should recover position 10
    let mut recovered = false;
    for cp in &coded {
        if decoder.add_coded_packet(cp) {
            recovered = true;
        }
        if decoder.is_complete() {
            break;
        }
    }

    assert!(recovered, "Peeling should have recovered the lost packet");
    assert!(decoder.is_complete());
    assert_eq!(decoder.get_source(10).unwrap(), packets[10].as_slice());
}

#[test]
fn peeling_with_multiple_losses() {
    let config = MettleConfig::small_window();
    let (packets, encoder, mut decoder) = make_encoder_decoder(config, 30, 42);
    let coded = encoder.coded_packets();

    // Drop 3 source packets
    let dropped = vec![5, 15, 25];
    for (i, pkt) in packets.iter().enumerate() {
        if !dropped.contains(&i) {
            decoder.add_source_packet(i, pkt);
        }
    }

    for cp in &coded {
        decoder.add_coded_packet(cp);
        if decoder.is_complete() {
            break;
        }
    }

    assert!(
        decoder.is_complete(),
        "Should recover 3 lost packets via peeling: {}/30",
        decoder.num_recovered()
    );

    for &pos in &dropped {
        assert_eq!(decoder.get_source(pos).unwrap(), packets[pos].as_slice());
    }
}

#[test]
fn degree_one_bins_decode_immediately() {
    // Use TLE-only (l=1) so every coded packet is degree 1
    let config = MettleConfig {
        window_size: 50,
        num_edges: 1,
        overhead_factor: 0.1,
    };
    let seed = 42;
    let num = 10;

    let packets: Vec<Vec<u8>> = (0..num).map(|i| vec![i as u8; 100]).collect();

    let mut encoder = MettleEncoder::new(config, seed);
    for pkt in &packets {
        encoder.add_source_packet(pkt);
    }
    let coded = encoder.coded_packets();

    let mut decoder = MettleDecoder::new(config, num, seed);

    // Feed only coded packets — each should immediately recover one source
    let mut count = 0;
    for cp in &coded {
        if decoder.add_coded_packet(cp) {
            count += 1;
        }
    }

    assert_eq!(count, num, "Each TLE bin should decode exactly one source");
    assert!(decoder.is_complete());
}

#[test]
fn coded_before_source_still_works() {
    // Feed coded packets first, then source — peeling should work in either order
    let config = MettleConfig::small_window();
    let (packets, encoder, mut decoder) = make_encoder_decoder(config, 15, 42);
    let coded = encoder.coded_packets();

    // Feed coded packets first
    for cp in &coded {
        decoder.add_coded_packet(cp);
    }

    // Then feed all but one source packet
    for (i, pkt) in packets.iter().enumerate() {
        if i != 7 {
            decoder.add_source_packet(i, pkt);
        }
    }

    assert!(
        decoder.is_complete(),
        "Should decode with coded-first, source-second: {}/15",
        decoder.num_recovered()
    );
}

#[test]
fn total_fed_tracks_correctly() {
    let config = MettleConfig::small_window();
    let (packets, encoder, mut decoder) = make_encoder_decoder(config, 5, 42);
    let coded = encoder.coded_packets();

    for (i, pkt) in packets.iter().enumerate() {
        decoder.add_source_packet(i, pkt);
    }
    for cp in &coded {
        decoder.add_coded_packet(cp);
    }

    assert_eq!(decoder.total_fed(), (packets.len() + coded.len()) as u32);
}

#[test]
fn seen_ids_includes_all_fed_symbols() {
    let config = MettleConfig::small_window();
    let (packets, encoder, mut decoder) = make_encoder_decoder(config, 5, 42);
    let coded = encoder.coded_packets();

    for (i, pkt) in packets.iter().enumerate() {
        decoder.add_source_packet(i, pkt);
    }
    for cp in &coded {
        decoder.add_coded_packet(cp);
    }

    let ids = decoder.seen_ids();
    assert_eq!(ids.len(), packets.len() + coded.len());
}
