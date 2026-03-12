//! Wire protocol serialization/deserialization tests.

use raptorpath::fec::{EncodingParams, WireSymbol};
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
            },
            WireSymbol {
                block_id: 42,
                payload_id: 100,
                is_repair: true,
                data: vec![5, 6, 7],
            },
        ],
        send_timestamp_us: 1234567890,
        batch_seq: 99,
        path_id: 2,
    };

    let msg = WireMessage::Data(batch);
    let serialized = msg.serialize();
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
    });

    let bytes = msg.serialize();
    let decoded = WireMessage::deserialize(&bytes).unwrap();

    match decoded {
        WireMessage::Control(ControlMessage::BlockStart {
            params,
            transfer_length,
        }) => {
            assert_eq!(params.source_symbols, 55);
            assert_eq!(params.block_id, 777);
            assert_eq!(transfer_length, 65536);
        }
        _ => panic!("expected BlockStart"),
    }
}

#[test]
fn test_ack_roundtrip() {
    let msg = WireMessage::Control(ControlMessage::Ack {
        block_id: 10,
        received_ids: vec![0, 1, 3, 5, 7],
        echo_send_timestamp_us: 9999999,
        expected_count: 10,
        received_count: 5,
    });

    let bytes = msg.serialize();
    let decoded = WireMessage::deserialize(&bytes).unwrap();

    match decoded {
        WireMessage::Control(ControlMessage::Ack {
            block_id,
            received_ids,
            echo_send_timestamp_us,
            expected_count,
            received_count,
        }) => {
            assert_eq!(block_id, 10);
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

    let bytes = msg.serialize();
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
    });

    let bytes = msg.serialize();
    let decoded = WireMessage::deserialize(&bytes).unwrap();

    match decoded {
        WireMessage::Control(ControlMessage::PathReport {
            path_id,
            loss_rate,
            avg_rtt_us,
            throughput_bps,
        }) => {
            assert_eq!(path_id, 1);
            assert!((loss_rate - 0.05).abs() < 1e-10);
            assert_eq!(avg_rtt_us, 15000);
            assert!((throughput_bps - 50_000_000.0).abs() < 1.0);
        }
        _ => panic!("expected PathReport"),
    }
}

#[test]
fn test_ping_pong_roundtrip() {
    let ping = WireMessage::Control(ControlMessage::Ping {
        timestamp_us: 123456,
    });
    let bytes = ping.serialize();
    match WireMessage::deserialize(&bytes).unwrap() {
        WireMessage::Control(ControlMessage::Ping { timestamp_us }) => {
            assert_eq!(timestamp_us, 123456);
        }
        _ => panic!("expected Ping"),
    }

    let pong = WireMessage::Control(ControlMessage::Pong {
        echo_timestamp_us: 654321,
    });
    let bytes = pong.serialize();
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

    let bytes = msg.serialize();
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
        }],
        send_timestamp_us: 0,
        batch_seq: 0,
        path_id: 0,
    });

    let bytes = msg.serialize();
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
