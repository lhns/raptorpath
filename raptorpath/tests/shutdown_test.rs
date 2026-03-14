//! Shutdown and control message tests.
//! Verifies graceful shutdown components: message serialization,
//! broadcast signaling, partial block flush, and idempotency.

use raptorpath::fec::{FecBackend, EncodingParams, FecStream};
use raptorpath::net::framing::{extract_packets, frame_end, frame_packet};
use raptorpath::transport::{ControlMessage, WireMessage};
use tokio::sync::broadcast;

#[tokio::test]
async fn test_shutdown_message_serializes() {
    let msg = WireMessage::Control(ControlMessage::Shutdown);
    let bytes = msg.serialize().unwrap();
    let deserialized = WireMessage::deserialize(&bytes).expect("deserialization must succeed");

    match deserialized {
        WireMessage::Control(ControlMessage::Shutdown) => {} // ok
        other => panic!("expected Shutdown, got {:?}", other),
    }
}

#[tokio::test]
async fn test_broadcast_channel_delivers_shutdown() {
    let (tx, _) = broadcast::channel::<()>(1);
    let mut rx1 = tx.subscribe();
    let mut rx2 = tx.subscribe();
    let mut rx3 = tx.subscribe();

    tx.send(()).expect("send must succeed");

    rx1.recv().await.expect("receiver 1 must get shutdown");
    rx2.recv().await.expect("receiver 2 must get shutdown");
    rx3.recv().await.expect("receiver 3 must get shutdown");
}

#[tokio::test]
async fn test_broadcast_shutdown_with_select() {
    let (tx, _) = broadcast::channel::<()>(1);
    let mut rx = tx.subscribe();

    // Send shutdown before entering select — it should be ready immediately
    tx.send(()).expect("send must succeed");

    let was_shutdown = tokio::select! {
        _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
            false
        }
        result = rx.recv() => {
            result.is_ok()
        }
    };

    assert!(was_shutdown, "shutdown branch must fire before sleep");
}

#[tokio::test]
async fn test_partial_block_flush_before_shutdown() {
    // Simulate: 1 packet framed but no frame_end yet (partial block).
    // On shutdown signal, frame_end and encode. Verify roundtrip.
    let packet = vec![0x42u8; 300];

    let mut block = Vec::new();
    frame_packet(&mut block, &packet);
    // Partial block — no frame_end yet.

    // Shutdown signal arrives: flush the partial block.
    frame_end(&mut block);

    // Encode
    let symbol_size: u16 = 256;
    let source_symbols = (block.len() as f64 / symbol_size as f64).ceil() as u32;
    let params = EncodingParams {
        source_symbols,
        symbol_size,
        repair_count: 0,
        block_id: 0,
    };

    let mut fec = FecStream::new(&block, params, FecBackend::RaptorQ);
    let source = fec.take_source_symbols();

    // Decode
    let mut decoder = FecBackend::RaptorQ.create_decoder(params, block.len() as u64);
    let mut decoded_data = None;
    for sym in &source {
        if let Some(data) = decoder.add_symbol(sym) {
            decoded_data = Some(data);
            break;
        }
    }

    let recovered = decoded_data.expect("partial block flush must decode");
    let extracted = extract_packets(&recovered);
    assert_eq!(extracted.len(), 1, "expected 1 packet from flushed partial block");
    assert_eq!(extracted[0], packet, "flushed packet must match original");
}

#[tokio::test]
async fn test_shutdown_idempotent() {
    // Sending shutdown twice on broadcast. Receivers should get at least one.
    let (tx, _) = broadcast::channel::<()>(2);
    let mut rx = tx.subscribe();

    tx.send(()).expect("first send must succeed");
    tx.send(()).expect("second send must succeed");

    let first = rx.recv().await;
    assert!(first.is_ok(), "receiver must get at least one shutdown signal");

    // Second recv should also succeed (channel has capacity 2)
    let second = rx.recv().await;
    assert!(second.is_ok(), "receiver should get second signal too");
}

#[tokio::test]
async fn test_shutdown_control_message_variants() {
    // Verify that Shutdown roundtrips alongside other control message variants
    let messages = vec![
        ControlMessage::Shutdown,
        ControlMessage::Ping { timestamp_us: 12345 },
        ControlMessage::Pong {
            echo_timestamp_us: 12345,
        },
        ControlMessage::RepairRequest {
            block_id: 7,
            additional_count: 3,
        },
    ];

    for original in &messages {
        let wire = WireMessage::Control(original.clone());
        let bytes = wire.serialize().unwrap();
        let deserialized =
            WireMessage::deserialize(&bytes).expect("deserialization must succeed");

        match (&wire, &deserialized) {
            (WireMessage::Control(a), WireMessage::Control(b)) => {
                // Verify they serialize to the same bytes (roundtrip identity)
                let bytes_a = WireMessage::Control(a.clone()).serialize().unwrap();
                let bytes_b = WireMessage::Control(b.clone()).serialize().unwrap();
                assert_eq!(bytes_a, bytes_b, "roundtrip must be identical");
            }
            _ => panic!("expected Control variant"),
        }
    }
}
