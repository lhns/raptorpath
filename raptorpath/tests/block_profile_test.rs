//! ADR-0006: Protocol hint influences block size and timing.

use raptorpath::transport::{ControlMessage, WireMessage, PROTOCOL_VERSION, WIRE_MAGIC, Handshake};

#[test]
fn test_wire_message_has_version_header() {
    let msg = WireMessage::Control(ControlMessage::Ping { timestamp_us: 42 });
    let data = msg.serialize().unwrap();

    // First 4 bytes should be magic
    assert_eq!(&data[..4], &WIRE_MAGIC);
    // Next 4 bytes should be version
    let version = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    assert_eq!(version, PROTOCOL_VERSION);
}

#[test]
fn test_wire_message_roundtrip_with_version() {
    let msg = WireMessage::Control(ControlMessage::Ack {
        block_id: 99,
        batch_seq: 4,
        received_ids: vec![1, 2, 3],
        echo_send_timestamp_us: 12345,
        expected_count: 10,
        received_count: 3,
    });
    let data = msg.serialize().unwrap();
    let decoded = WireMessage::deserialize(&data).unwrap();

    match decoded {
        WireMessage::Control(ControlMessage::Ack { block_id, received_ids, .. }) => {
            assert_eq!(block_id, 99);
            assert_eq!(received_ids, vec![1, 2, 3]);
        }
        _ => panic!("expected Ack"),
    }
}

#[test]
fn test_wrong_magic_rejected() {
    let msg = WireMessage::Control(ControlMessage::Ping { timestamp_us: 0 });
    let mut data = msg.serialize().unwrap();
    data[0] = b'X'; // corrupt magic
    assert!(WireMessage::deserialize(&data).is_err());
}

#[test]
fn test_wrong_version_rejected() {
    let msg = WireMessage::Control(ControlMessage::Ping { timestamp_us: 0 });
    let mut data = msg.serialize().unwrap();
    // Set version to 99
    let bad_version: u32 = 99;
    data[4..8].copy_from_slice(&bad_version.to_be_bytes());
    assert!(WireMessage::deserialize(&data).is_err());
}

#[test]
fn test_too_short_message_rejected() {
    let data = vec![0u8; 4]; // too short for header
    assert!(WireMessage::deserialize(&data).is_err());
}

#[test]
fn test_handshake_roundtrip() {
    let hs = Handshake {
        version: PROTOCOL_VERSION,
        max_block_size: 64 * 1024,
        symbol_size: 1200,
        path_id: 0,
    };
    let data = hs.serialize().unwrap();
    let decoded = Handshake::deserialize(&data).unwrap();

    assert_eq!(decoded.version, PROTOCOL_VERSION);
    assert_eq!(decoded.max_block_size, 64 * 1024);
    assert_eq!(decoded.symbol_size, 1200);
    assert_eq!(decoded.path_id, 0);
}

#[test]
fn test_handshake_wrong_magic() {
    let hs = Handshake {
        version: PROTOCOL_VERSION,
        max_block_size: 64 * 1024,
        symbol_size: 1200,
        path_id: 0,
    };
    let mut data = hs.serialize().unwrap();
    data[0] = b'Z';
    assert!(Handshake::deserialize(&data).is_err());
}

#[test]
fn test_handshake_wrong_version() {
    let hs = Handshake {
        version: PROTOCOL_VERSION,
        max_block_size: 64 * 1024,
        symbol_size: 1200,
        path_id: 0,
    };
    let mut data = hs.serialize().unwrap();
    let bad: u32 = 255;
    data[4..8].copy_from_slice(&bad.to_be_bytes());
    assert!(Handshake::deserialize(&data).is_err());
}

#[test]
fn test_shutdown_message_roundtrip() {
    let msg = WireMessage::Control(ControlMessage::Shutdown);
    let data = msg.serialize().unwrap();
    let decoded = WireMessage::deserialize(&data).unwrap();

    match decoded {
        WireMessage::Control(ControlMessage::Shutdown) => {}
        _ => panic!("expected Shutdown"),
    }
}

#[test]
fn test_data_message_with_version() {
    use raptorpath::transport::SymbolBatch;
    use raptorpath::fec::{FecBackend, WireSymbol};

    let batch = SymbolBatch {
        symbols: vec![WireSymbol {
            block_id: 0,
            payload_id: 0,
            is_repair: false,
            data: vec![42; 100],
            backend: FecBackend::RaptorQ,
        }],
        send_timestamp_us: 999,
        batch_seq: 1,
        path_id: 0,
    };
    let msg = WireMessage::Data(batch);
    let data = msg.serialize().unwrap();

    // Verify header
    assert_eq!(&data[..4], &WIRE_MAGIC);

    let decoded = WireMessage::deserialize(&data).unwrap();
    match decoded {
        WireMessage::Data(b) => {
            assert_eq!(b.symbols.len(), 1);
            assert_eq!(b.symbols[0].data, vec![42; 100]);
        }
        _ => panic!("expected Data"),
    }
}
