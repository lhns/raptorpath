//! Wire protocol serialization/deserialization tests.

use raptorpath::fec::{EncodingParams, FecBackend, WireSymbol};
use raptorpath::transport::{ControlMessage, SymbolBatch, WireMessage};

#[test]
fn test_symbol_batch_roundtrip() {
    let batch = SymbolBatch {
        symbols: vec![
            WireSymbol {
                block_id: 42,
                payload_id: 7,
                is_repair: false,
                data: vec![1, 2, 3, 4],
                backend: FecBackend::RaptorQ,
            },
            WireSymbol {
                block_id: 42,
                payload_id: 100,
                is_repair: true,
                data: vec![5, 6, 7],
                backend: FecBackend::RaptorQ,
            },
        ],
        send_timestamp_us: 1234567890,
        batch_seq: 99,
        path_id: 2,
    };

    let msg = WireMessage::Data(batch);
    let serialized = msg.serialize().unwrap();
    let deserialized = WireMessage::deserialize(&serialized).unwrap();

    match deserialized {
        WireMessage::Data(b) => {
            assert_eq!(b.symbols.len(), 2);
            assert_eq!(b.symbols[0].block_id, 42);
            assert_eq!(b.symbols[0].payload_id, 7);
            assert!(!b.symbols[0].is_repair);
            assert_eq!(b.symbols[1].is_repair, true);
            assert_eq!(b.send_timestamp_us, 1234567890);
            assert_eq!(b.batch_seq, 99);
            assert_eq!(b.path_id, 2);
        }
        _ => panic!("expected Data"),
    }
}

#[test]
fn test_block_start_roundtrip() {
    let msg = WireMessage::Control(ControlMessage::BlockStart {
        params: EncodingParams {
            source_symbols: 55,
            symbol_size: 1200,
            repair_count: 10,
            block_id: 777,
        },
        transfer_length: 65536,
        backend: FecBackend::RaptorQ,
    });

    let bytes = msg.serialize().unwrap();
    let decoded = WireMessage::deserialize(&bytes).unwrap();

    match decoded {
        WireMessage::Control(ControlMessage::BlockStart {
            params,
            transfer_length,
            backend,
        }) => {
            assert_eq!(params.source_symbols, 55);
            assert_eq!(params.block_id, 777);
            assert_eq!(transfer_length, 65536);
            assert_eq!(backend, FecBackend::RaptorQ);
        }
        _ => panic!("expected BlockStart"),
    }
}

#[test]
fn test_ack_roundtrip() {
    let msg = WireMessage::Control(ControlMessage::Ack {
        block_id: 10,
        batch_seq: 3,
        received_ids: vec![0, 1, 3, 5, 7],
        echo_send_timestamp_us: 9999999,
        expected_count: 10,
        received_count: 5,
    });

    let bytes = msg.serialize().unwrap();
    let decoded = WireMessage::deserialize(&bytes).unwrap();

    match decoded {
        WireMessage::Control(ControlMessage::Ack {
            block_id,
            batch_seq,
            received_ids,
            echo_send_timestamp_us,
            expected_count,
            received_count,
        }) => {
            assert_eq!(block_id, 10);
            assert_eq!(batch_seq, 3);
            assert_eq!(received_ids, vec![0, 1, 3, 5, 7]);
            assert_eq!(echo_send_timestamp_us, 9999999);
            assert_eq!(expected_count, 10);
            assert_eq!(received_count, 5);
        }
        _ => panic!("expected Ack"),
    }
}

#[test]
fn test_block_result_roundtrip() {
    let msg = WireMessage::Control(ControlMessage::BlockResult {
        block_id: 42,
        success: false,
        symbols_received: 45,
        symbols_needed: 55,
    });

    let bytes = msg.serialize().unwrap();
    let decoded = WireMessage::deserialize(&bytes).unwrap();

    match decoded {
        WireMessage::Control(ControlMessage::BlockResult {
            block_id,
            success,
            symbols_received,
            symbols_needed,
        }) => {
            assert_eq!(block_id, 42);
            assert!(!success);
            assert_eq!(symbols_received, 45);
            assert_eq!(symbols_needed, 55);
        }
        _ => panic!("expected BlockResult"),
    }
}

#[test]
fn test_path_report_roundtrip() {
    let msg = WireMessage::Control(ControlMessage::PathReport {
        path_id: 1,
        loss_rate: 0.05,
        avg_rtt_us: 15000,
        throughput_bps: 50_000_000.0,
        jitter_us: 250,
        symbols_sent: 5000,
        symbols_received: 4750,
    });

    let bytes = msg.serialize().unwrap();
    let decoded = WireMessage::deserialize(&bytes).unwrap();

    match decoded {
        WireMessage::Control(ControlMessage::PathReport {
            path_id,
            loss_rate,
            avg_rtt_us,
            throughput_bps,
            jitter_us,
            symbols_sent,
            symbols_received,
        }) => {
            assert_eq!(path_id, 1);
            assert!((loss_rate - 0.05).abs() < 1e-10);
            assert_eq!(avg_rtt_us, 15000);
            assert!((throughput_bps - 50_000_000.0).abs() < 1.0);
            assert_eq!(jitter_us, 250);
            assert_eq!(symbols_sent, 5000);
            assert_eq!(symbols_received, 4750);
        }
        _ => panic!("expected PathReport"),
    }
}

#[test]
fn test_ping_pong_roundtrip() {
    let ping = WireMessage::Control(ControlMessage::Ping {
        timestamp_us: 123456,
    });
    let bytes = ping.serialize().unwrap();
    match WireMessage::deserialize(&bytes).unwrap() {
        WireMessage::Control(ControlMessage::Ping { timestamp_us }) => {
            assert_eq!(timestamp_us, 123456);
        }
        _ => panic!("expected Ping"),
    }

    let pong = WireMessage::Control(ControlMessage::Pong {
        echo_timestamp_us: 654321,
    });
    let bytes = pong.serialize().unwrap();
    match WireMessage::deserialize(&bytes).unwrap() {
        WireMessage::Control(ControlMessage::Pong { echo_timestamp_us }) => {
            assert_eq!(echo_timestamp_us, 654321);
        }
        _ => panic!("expected Pong"),
    }
}

#[test]
fn test_empty_batch_serialization() {
    let msg = WireMessage::Data(SymbolBatch {
        symbols: vec![],
        send_timestamp_us: 0,
        batch_seq: 0,
        path_id: 0,
    });

    let bytes = msg.serialize().unwrap();
    let decoded = WireMessage::deserialize(&bytes).unwrap();

    match decoded {
        WireMessage::Data(b) => {
            assert!(b.symbols.is_empty());
        }
        _ => panic!("expected Data"),
    }
}

#[test]
fn test_large_symbol_data() {
    let msg = WireMessage::Data(SymbolBatch {
        symbols: vec![WireSymbol {
            block_id: 0,
            payload_id: 0,
            is_repair: false,
            data: vec![0xAB; 1200], // full symbol
            backend: FecBackend::RaptorQ,
        }],
        send_timestamp_us: 0,
        batch_seq: 0,
        path_id: 0,
    });

    let bytes = msg.serialize().unwrap();
    assert!(bytes.len() > 1200);

    let decoded = WireMessage::deserialize(&bytes).unwrap();
    match decoded {
        WireMessage::Data(b) => {
            assert_eq!(b.symbols[0].data.len(), 1200);
        }
        _ => panic!("expected Data"),
    }
}

#[test]
fn test_deserialize_garbage_fails() {
    let garbage = vec![0xFF, 0xFE, 0xFD, 0xFC];
    assert!(WireMessage::deserialize(&garbage).is_err());
}

#[test]
fn test_deserialize_empty_fails() {
    assert!(WireMessage::deserialize(&[]).is_err());
}

#[test]
fn test_oversized_symbol_batch_rejected() {
    // Construct a batch with more than MAX_SYMBOLS_PER_BATCH (1000) symbols
    let symbols: Vec<WireSymbol> = (0..1001)
        .map(|i| WireSymbol {
            block_id: 1,
            payload_id: i,
            is_repair: false,
            data: vec![0u8; 10],
            backend: FecBackend::RaptorQ,
        })
        .collect();
    let batch = SymbolBatch {
        symbols,
        send_timestamp_us: 0,
        batch_seq: 0,
        path_id: 0,
    };
    let msg = WireMessage::Data(batch);
    let bytes = msg.serialize().unwrap();
    // Deserialization should fail due to batch size validation
    assert!(WireMessage::deserialize(&bytes).is_err());
}

#[test]
fn test_oversized_ack_rejected() {
    // Construct an Ack with more than MAX_ACK_IDS (2000) IDs
    let received_ids: Vec<u32> = (0..2001).collect();
    let msg = WireMessage::Control(ControlMessage::Ack {
        block_id: 1,
        batch_seq: 0,
        received_ids,
        echo_send_timestamp_us: 0,
        expected_count: 100,
        received_count: 100,
    });
    let bytes = msg.serialize().unwrap();
    assert!(WireMessage::deserialize(&bytes).is_err());
}

#[test]
fn test_normal_batch_accepted() {
    // A batch at the limit should be fine
    let symbols: Vec<WireSymbol> = (0..1000)
        .map(|i| WireSymbol {
            block_id: 1,
            payload_id: i,
            is_repair: false,
            data: vec![0u8; 10],
            backend: FecBackend::RaptorQ,
        })
        .collect();
    let batch = SymbolBatch {
        symbols,
        send_timestamp_us: 0,
        batch_seq: 0,
        path_id: 0,
    };
    let msg = WireMessage::Data(batch);
    let bytes = msg.serialize().unwrap();
    assert!(WireMessage::deserialize(&bytes).is_ok());
}
