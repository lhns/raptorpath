//! ADR-0005: ACK delivery and control message tests.
//!
//! These tests verify the wire protocol and serialization of ACK/control messages.
//! Full end-to-end delivery requires QUIC connections, so we test the serialization
//! and message handling logic.

use raptorpath::transport::{ControlMessage, WireMessage};

#[test]
fn test_ack_serialization_roundtrip() {
    let ack = ControlMessage::Ack {
        block_id: 42,
        batch_seq: 7,
        received_ids: vec![0, 1, 3, 5, 7],
        echo_send_timestamp_us: 1_000_000,
        expected_count: 10,
        received_count: 5,
    };
    let wire = WireMessage::Control(ack);
    let data = wire.serialize().unwrap();
    let decoded = WireMessage::deserialize(&data).unwrap();

    match decoded {
        WireMessage::Control(ControlMessage::Ack {
            block_id,
            batch_seq,
            received_ids,
            echo_send_timestamp_us,
            expected_count,
            received_count,
        }) => {
            assert_eq!(block_id, 42);
            assert_eq!(batch_seq, 7);
            assert_eq!(received_ids, vec![0, 1, 3, 5, 7]);
            assert_eq!(echo_send_timestamp_us, 1_000_000);
            assert_eq!(expected_count, 10);
            assert_eq!(received_count, 5);
        }
        other => panic!("expected Ack, got: {other:?}"),
    }
}

#[test]
fn test_block_result_serialization_roundtrip() {
    let msg = ControlMessage::BlockResult {
        block_id: 99,
        success: true,
        symbols_received: 55,
        symbols_needed: 50,
    };
    let wire = WireMessage::Control(msg);
    let data = wire.serialize().unwrap();
    let decoded = WireMessage::deserialize(&data).unwrap();

    match decoded {
        WireMessage::Control(ControlMessage::BlockResult {
            block_id,
            success,
            symbols_received,
            symbols_needed,
        }) => {
            assert_eq!(block_id, 99);
            assert!(success);
            assert_eq!(symbols_received, 55);
            assert_eq!(symbols_needed, 50);
        }
        other => panic!("expected BlockResult, got: {other:?}"),
    }
}

#[test]
fn test_pong_serialization_roundtrip() {
    let msg = ControlMessage::Pong {
        echo_timestamp_us: 123456789,
    };
    let wire = WireMessage::Control(msg);
    let data = wire.serialize().unwrap();
    let decoded = WireMessage::deserialize(&data).unwrap();

    match decoded {
        WireMessage::Control(ControlMessage::Pong { echo_timestamp_us }) => {
            assert_eq!(echo_timestamp_us, 123456789);
        }
        other => panic!("expected Pong, got: {other:?}"),
    }
}

#[test]
fn test_ping_pong_pair() {
    let ping = ControlMessage::Ping {
        timestamp_us: 999999,
    };
    // Simulate what the receiver does: echo the timestamp as pong
    let pong = match &ping {
        ControlMessage::Ping { timestamp_us } => ControlMessage::Pong {
            echo_timestamp_us: *timestamp_us,
        },
        _ => panic!("expected Ping"),
    };

    match pong {
        ControlMessage::Pong { echo_timestamp_us } => {
            assert_eq!(echo_timestamp_us, 999999);
        }
        _ => panic!("expected Pong"),
    }
}

#[test]
fn test_ack_with_empty_received_ids() {
    let ack = ControlMessage::Ack {
        block_id: 0,
        batch_seq: 0,
        received_ids: vec![],
        echo_send_timestamp_us: 0,
        expected_count: 10,
        received_count: 0,
    };
    let wire = WireMessage::Control(ack);
    let data = wire.serialize().unwrap();
    let decoded = WireMessage::deserialize(&data).unwrap();

    match decoded {
        WireMessage::Control(ControlMessage::Ack { received_ids, .. }) => {
            assert!(received_ids.is_empty());
        }
        _ => panic!("expected Ack"),
    }
}

#[test]
fn test_ack_with_large_received_ids() {
    // Simulate a large block with many symbols
    let ids: Vec<u32> = (0..1000).collect();
    let ack = ControlMessage::Ack {
        block_id: 7,
        batch_seq: 1,
        received_ids: ids.clone(),
        echo_send_timestamp_us: 5_000_000,
        expected_count: 1000,
        received_count: 1000,
    };
    let wire = WireMessage::Control(ack);
    let data = wire.serialize().unwrap();
    let decoded = WireMessage::deserialize(&data).unwrap();

    match decoded {
        WireMessage::Control(ControlMessage::Ack {
            received_ids,
            received_count,
            ..
        }) => {
            assert_eq!(received_ids.len(), 1000);
            assert_eq!(received_count, 1000);
        }
        _ => panic!("expected Ack"),
    }
}

#[test]
fn test_repair_request_roundtrip() {
    let msg = ControlMessage::RepairRequest {
        block_id: 42,
        additional_count: 5,
    };
    let wire = WireMessage::Control(msg);
    let data = wire.serialize().unwrap();
    let decoded = WireMessage::deserialize(&data).unwrap();

    match decoded {
        WireMessage::Control(ControlMessage::RepairRequest {
            block_id,
            additional_count,
        }) => {
            assert_eq!(block_id, 42);
            assert_eq!(additional_count, 5);
        }
        _ => panic!("expected RepairRequest"),
    }
}

#[test]
fn test_path_report_roundtrip() {
    let msg = ControlMessage::PathReport {
        path_id: 2,
        loss_rate: 0.05,
        avg_rtt_us: 15000,
        throughput_bps: 1_000_000.0,
        jitter_us: 500,
        symbols_sent: 1000,
        symbols_received: 950,
    };
    let wire = WireMessage::Control(msg);
    let data = wire.serialize().unwrap();
    let decoded = WireMessage::deserialize(&data).unwrap();

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
            assert_eq!(path_id, 2);
            assert!((loss_rate - 0.05).abs() < 1e-10);
            assert_eq!(avg_rtt_us, 15000);
            assert!((throughput_bps - 1_000_000.0).abs() < 1e-5);
            assert_eq!(jitter_us, 500);
            assert_eq!(symbols_sent, 1000);
            assert_eq!(symbols_received, 950);
        }
        _ => panic!("expected PathReport"),
    }
}

#[test]
fn test_control_message_as_datagram_size() {
    // Verify that ACK messages fit in a typical QUIC datagram
    let ack = ControlMessage::Ack {
        block_id: u64::MAX,
        batch_seq: u64::MAX,
        received_ids: (0..100).collect(),
        echo_send_timestamp_us: u64::MAX,
        expected_count: u32::MAX,
        received_count: u32::MAX,
    };
    let wire = WireMessage::Control(ack);
    let data = wire.serialize().unwrap();

    // QUIC datagrams should fit in a single UDP packet (~1200 bytes for QUIC initial)
    // 100 payload_ids × 4 bytes = 400 bytes + overhead, should be well under 1200
    assert!(
        data.len() < 1200,
        "ACK with 100 IDs should fit in datagram: {} bytes",
        data.len()
    );
}
