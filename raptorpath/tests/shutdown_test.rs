//! Shutdown and control message tests.
//! Verifies graceful shutdown components: message serialization,
//! broadcast signaling, and idempotency.

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
