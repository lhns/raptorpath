//! The `WindowSwitch` wire round-trip (kept as a hostile-peer/version
//! guard) and the window flush-and-switch encode/decode. There is no
//! mid-stream backend switch on the data path (paper §5.10). The per-block
//! backend tests went with the block pipeline (ADR-0069).

use raptorpath::fec::FecBackend;
use raptorpath::transport::{ControlMessage, WireMessage};

#[test]
fn test_window_switch_message_roundtrip() {
    let msg = WireMessage::Control(ControlMessage::WindowSwitch {
        flush_seq: 42,
        new_backend: FecBackend::Rlc,
        symbol_size: 512,
    });

    let bytes = msg.serialize().unwrap();
    let decoded = WireMessage::deserialize(&bytes).unwrap();

    match decoded {
        WireMessage::Control(ControlMessage::WindowSwitch {
            flush_seq,
            new_backend,
            symbol_size,
        }) => {
            assert_eq!(flush_seq, 42);
            assert_eq!(new_backend, FecBackend::Rlc);
            assert_eq!(symbol_size, 512);
        }
        _ => panic!("expected WindowSwitch"),
    }
}

#[test]
fn test_window_flush_and_switch_encode_decode() {
    // Simulate window-mode: encode with RLC, flush, then restart with a fresh RLC window
    use raptorpath::fec::{RlcWindowEncoder, RlcWindowDecoder, WindowEncoder, WindowDecoder};
    use raptorpath::net::framing::{frame_window_packet, extract_window_packet};

    let symbol_size: u16 = 128;

    // Phase 1: RLC encoding
    let mut rlc_encoder = RlcWindowEncoder::new(symbol_size);
    let mut rlc_decoder = RlcWindowDecoder::new(symbol_size);

    let packets: Vec<Vec<u8>> = (0..5)
        .map(|i| vec![i as u8 + 1; 50])
        .collect();

    let mut recovered_phase1 = Vec::new();
    for pkt in &packets {
        let framed = frame_window_packet(pkt, symbol_size);
        let sym = rlc_encoder.add_source(&framed);
        for (seq, data) in rlc_decoder.add_symbol(&sym) {
            if let Some(p) = extract_window_packet(&data) {
                recovered_phase1.push((seq, p));
            }
        }
    }

    assert_eq!(recovered_phase1.len(), 5, "all 5 packets should be recovered in phase 1");

    // Phase 2: fresh encoder/decoder pair (simulate flush point)
    let mut phase2_encoder = RlcWindowEncoder::new(symbol_size);
    let mut phase2_decoder = RlcWindowDecoder::new(symbol_size);

    let packets2: Vec<Vec<u8>> = (5..10)
        .map(|i| vec![i as u8 + 1; 50])
        .collect();

    let mut recovered_phase2 = Vec::new();
    for pkt in &packets2 {
        let framed = frame_window_packet(pkt, symbol_size);
        let sym = phase2_encoder.add_source(&framed);
        for (seq, data) in phase2_decoder.add_symbol(&sym) {
            if let Some(p) = extract_window_packet(&data) {
                recovered_phase2.push((seq, p));
            }
        }
    }

    assert_eq!(recovered_phase2.len(), 5, "all 5 packets should be recovered in phase 2");

    // Verify data integrity
    for (i, (_, pkt)) in recovered_phase1.iter().enumerate() {
        assert_eq!(pkt, &vec![i as u8 + 1; 50]);
    }
    for (i, (_, pkt)) in recovered_phase2.iter().enumerate() {
        assert_eq!(pkt, &vec![(i + 5) as u8 + 1; 50]);
    }
}
