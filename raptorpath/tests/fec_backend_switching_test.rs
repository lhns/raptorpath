//! Integration tests for runtime FEC backend switching (ADR-0030).
//!
//! Tests block-mode per-block switching and the backend selector heuristic
//! with actual encode/decode cycles across different backends.

use raptorpath::control::backend_selector::BackendSelector;
use raptorpath::control::estimator::LossEstimator;
use raptorpath::control::fec_rate::ProtocolHint;
use raptorpath::fec::{EncodingParams, FecBackend, FecStream};
use raptorpath::transport::{ControlMessage, WireMessage};

/// Encode a block with a given backend, decode it with the backend from BlockStart.
fn encode_decode_block(data: &[u8], backend: FecBackend, block_id: u64) -> bool {
    let source_symbols = (data.len() as f64 / 128.0).ceil() as u32;
    let params = EncodingParams {
        source_symbols,
        symbol_size: 128,
        repair_count: source_symbols / 2, // 50% overhead
        block_id,
    };

    let mut stream = FecStream::new(data, params, backend);
    let source = stream.take_source_symbols();
    let repair = stream.generate_repair(params.repair_count);

    // Create decoder using the backend (simulating receiver getting BlockStart.backend)
    let mut decoder = backend.create_decoder(params, data.len() as u64);

    // Feed all source symbols
    for sym in &source {
        if let Some(_) = decoder.add_symbol(sym) {
            return true;
        }
    }

    // Feed repair symbols if needed
    for sym in &repair {
        if let Some(_) = decoder.add_symbol(sym) {
            return true;
        }
    }

    decoder.is_decoded()
}

#[test]
fn test_block_mode_per_block_switching() {
    // Encode 10 blocks alternating RaptorQ/RLC, decode each with matching decoder
    let data = vec![0xAB; 1024];
    let backends = [
        FecBackend::RaptorQ,
        FecBackend::Rlc,
        FecBackend::RaptorQ,
        FecBackend::Mettle,
        FecBackend::RaptorQ,
        FecBackend::Rlc,
        FecBackend::Mettle,
        FecBackend::Rlc,
        FecBackend::RaptorQ,
        FecBackend::Rlc,
    ];

    for (i, &backend) in backends.iter().enumerate() {
        let success = encode_decode_block(&data, backend, i as u64);
        assert!(
            success,
            "block {i} with {:?} failed to decode",
            backend
        );
    }
}

#[test]
fn test_block_mode_receiver_uses_blockstart_backend() {
    // Simulate receiver creating decoder from BlockStart.backend field
    let data = vec![0xCD; 512];
    let backends = [FecBackend::RaptorQ, FecBackend::Rlc, FecBackend::Mettle];

    for &backend in &backends {
        let source_symbols = (data.len() as f64 / 128.0).ceil() as u32;
        let params = EncodingParams {
            source_symbols,
            symbol_size: 128,
            repair_count: source_symbols,
            block_id: 0,
        };

        // Sender creates BlockStart with backend
        let block_start = ControlMessage::BlockStart {
            params,
            transfer_length: data.len() as u64,
            backend,
        };

        // Verify the backend field survives serialization
        let wire = WireMessage::Control(block_start);
        let bytes = wire.serialize().unwrap();
        let decoded = WireMessage::deserialize(&bytes).unwrap();

        match decoded {
            WireMessage::Control(ControlMessage::BlockStart {
                params: dec_params,
                transfer_length: tl,
                backend: dec_backend,
            }) => {
                assert_eq!(dec_backend, backend);
                // Receiver uses dec_backend to create decoder
                let mut decoder = dec_backend.create_decoder(dec_params, tl);
                let mut stream = FecStream::new(&data, params, backend);
                let source = stream.take_source_symbols();
                for sym in &source {
                    decoder.add_symbol(sym);
                }
                assert!(
                    decoder.is_decoded() || decoder.is_complete_source(),
                    "decoder with {:?} backend from BlockStart failed",
                    backend
                );
            }
            _ => panic!("expected BlockStart"),
        }
    }
}

#[test]
fn test_backend_selector_switches_on_loss_change() {
    let mut selector = BackendSelector::new(
        FecBackend::RaptorQ,
        None,
        ProtocolHint::Auto,
        0.01,
        0.10,
        0, // no delay
        false,
    );

    // Start with low loss — should stay RaptorQ
    let mut est = LossEstimator::new();
    for _ in 0..100 {
        est.record_batch(1000, 998); // 0.2% loss
    }
    assert!(selector.evaluate(&est).is_none());
    assert_eq!(selector.current(), FecBackend::RaptorQ);

    // Switch to high loss — should switch to Mettle after debounce
    let mut est_high = LossEstimator::new();
    for _ in 0..100 {
        est_high.record_batch(1000, 850); // 15% loss
    }

    selector.evaluate(&est_high);
    selector.evaluate(&est_high);
    let switch = selector.evaluate(&est_high);
    assert!(switch.is_some(), "should switch after 3 evaluations");

    // Encode with the new backend and verify it works
    let data = vec![0xEF; 256];
    let success = encode_decode_block(&data, selector.current(), 99);
    assert!(success, "encoding with switched backend should work");
}

#[test]
fn test_window_switch_message_roundtrip() {
    let msg = WireMessage::Control(ControlMessage::WindowSwitch {
        flush_seq: 42,
        new_backend: FecBackend::Mettle,
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
            assert_eq!(new_backend, FecBackend::Mettle);
            assert_eq!(symbol_size, 512);
        }
        _ => panic!("expected WindowSwitch"),
    }
}

#[test]
fn test_window_switch_ack_roundtrip() {
    let msg = WireMessage::Control(ControlMessage::WindowSwitchAck { flush_seq: 100 });

    let bytes = msg.serialize().unwrap();
    let decoded = WireMessage::deserialize(&bytes).unwrap();

    match decoded {
        WireMessage::Control(ControlMessage::WindowSwitchAck { flush_seq }) => {
            assert_eq!(flush_seq, 100);
        }
        _ => panic!("expected WindowSwitchAck"),
    }
}

#[test]
fn test_window_flush_and_switch_encode_decode() {
    // Simulate window-mode: encode with RLC, then switch to Mettle
    use raptorpath::fec::{RlcWindowEncoder, RlcWindowDecoder, MettleWindowEncoder, MettleWindowDecoder, WindowEncoder, WindowDecoder};
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

    // Phase 2: Switch to Mettle (simulate flush point)
    let mut mettle_encoder = MettleWindowEncoder::new(
        mettle::MettleConfig::small_window(),
        symbol_size,
        42,
    );
    let mut mettle_decoder = MettleWindowDecoder::new(symbol_size);

    let packets2: Vec<Vec<u8>> = (5..10)
        .map(|i| vec![i as u8 + 1; 50])
        .collect();

    let mut recovered_phase2 = Vec::new();
    for pkt in &packets2 {
        let framed = frame_window_packet(pkt, symbol_size);
        let sym = mettle_encoder.add_source(&framed);
        for (seq, data) in mettle_decoder.add_symbol(&sym) {
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

#[test]
fn test_forced_backend_blocks_auto_switch() {
    let mut selector = BackendSelector::new(
        FecBackend::RaptorQ,
        Some(FecBackend::RaptorQ), // forced
        ProtocolHint::Auto,
        0.01,
        0.10,
        0,
        false,
    );

    // Even with very high loss, forced backend should not switch
    let mut est = LossEstimator::new();
    for _ in 0..200 {
        est.record_batch(1000, 500); // 50% loss
    }

    for _ in 0..20 {
        assert!(
            selector.evaluate(&est).is_none(),
            "forced backend should never auto-switch"
        );
    }
    assert_eq!(selector.current(), FecBackend::RaptorQ);
}
