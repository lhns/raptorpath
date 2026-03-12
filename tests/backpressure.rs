//! ADR-0011: Channel backpressure tests.
//!
//! Tests that the system handles full channels gracefully.

use bytes::Bytes;
use tokio::sync::mpsc;

/// Verify that try_send correctly handles full channels.
#[tokio::test]
async fn test_try_send_full_channel() {
    let (tx, _rx) = mpsc::channel::<Bytes>(4);

    // Fill the channel
    for i in 0..4 {
        tx.send(Bytes::from(vec![i; 100])).await.unwrap();
    }

    // Next try_send should fail with Full, not block
    let result = tx.try_send(Bytes::from(vec![99; 100]));
    assert!(result.is_err());
    match result.unwrap_err() {
        mpsc::error::TrySendError::Full(_) => {} // expected
        mpsc::error::TrySendError::Closed(_) => panic!("channel should not be closed"),
    }
}

/// Verify that a dropped receiver causes Closed error.
#[tokio::test]
async fn test_try_send_closed_channel() {
    let (tx, rx) = mpsc::channel::<Bytes>(4);
    drop(rx);

    let result = tx.try_send(Bytes::from(vec![0; 100]));
    assert!(result.is_err());
    match result.unwrap_err() {
        mpsc::error::TrySendError::Closed(_) => {} // expected
        mpsc::error::TrySendError::Full(_) => panic!("channel should be closed, not full"),
    }
}

/// Verify that larger channel capacity allows more buffering.
#[tokio::test]
async fn test_large_channel_capacity() {
    // ADR-0011 uses 4096 capacity
    let (tx, _rx) = mpsc::channel::<Bytes>(4096);

    // Should be able to buffer many packets without blocking
    for i in 0..4096u16 {
        tx.try_send(Bytes::from(i.to_be_bytes().to_vec())).unwrap();
    }

    // 4097th should fail
    let result = tx.try_send(Bytes::from(vec![0]));
    assert!(result.is_err());
}

/// Test that draining a full channel allows new sends.
#[tokio::test]
async fn test_drain_and_resend() {
    let (tx, mut rx) = mpsc::channel::<Bytes>(4);

    // Fill
    for i in 0..4 {
        tx.send(Bytes::from(vec![i; 10])).await.unwrap();
    }

    // Full
    assert!(tx.try_send(Bytes::from(vec![99])).is_err());

    // Drain one
    let _ = rx.recv().await.unwrap();

    // Now should succeed
    tx.try_send(Bytes::from(vec![99])).unwrap();
}

/// Simulate receiver dropping packets when TUN write channel is full
/// (mirrors behavior in net/mod.rs receiver task).
#[tokio::test]
async fn test_packet_drop_on_full_channel() {
    let (tx, _rx) = mpsc::channel::<Bytes>(8);

    let mut sent = 0u32;
    let mut dropped = 0u32;

    // Simulate injecting 100 packets — many will be dropped
    for i in 0..100u32 {
        match tx.try_send(Bytes::from(i.to_be_bytes().to_vec())) {
            Ok(()) => sent += 1,
            Err(mpsc::error::TrySendError::Full(_)) => dropped += 1,
            Err(mpsc::error::TrySendError::Closed(_)) => break,
        }
    }

    assert_eq!(sent, 8, "should fill channel exactly");
    assert_eq!(dropped, 92, "remaining packets should be dropped");
}

/// Verify that the pattern used in net/mod.rs works correctly.
#[tokio::test]
async fn test_net_mod_pattern() {
    let (tx, mut rx) = mpsc::channel::<Bytes>(16);

    // Simulate decoded packets arriving
    let packets: Vec<Vec<u8>> = (0..20).map(|i| vec![i; 100]).collect();

    let mut injected = 0;
    let mut dropped = 0;

    for pkt_data in &packets {
        match tx.try_send(Bytes::from(pkt_data.clone())) {
            Ok(()) => injected += 1,
            Err(mpsc::error::TrySendError::Full(_)) => {
                // This is what net/mod.rs does: warn and continue
                dropped += 1;
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                break;
            }
        }
    }

    assert_eq!(injected, 16);
    assert_eq!(dropped, 4);

    // Verify all injected packets are correct
    for i in 0..16u8 {
        let pkt = rx.recv().await.unwrap();
        assert_eq!(pkt[0], i);
    }
}
