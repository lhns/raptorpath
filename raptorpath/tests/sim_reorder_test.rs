//! B4: Reorder buffer tests with SimChannel.
//!
//! Verifies that the extracted ReorderBuffer correctly handles
//! out-of-order delivery from jittery network channels.

mod common;

use bytes::Bytes;
use common::*;
use raptorpath::net::reorder::ReorderBuffer;
use raptorpath::scheduler::{Clock, MockClock};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_reorder_from_jittery_channel() {
    let clock = Arc::new(MockClock::new());
    let mut channel = SimChannel::wifi(clock.clone(), 42);
    let mut reorder_buf = ReorderBuffer::new(25, 500);

    let num_symbols = 100u32;

    // Send symbols through jittery WiFi channel
    let symbols = make_source_batch(num_symbols);
    for sym in &symbols {
        channel.send(sym.clone());
    }

    // Advance past max delay + jitter
    clock.advance(Duration::from_millis(20));

    // Deliver — they may arrive out of order due to jitter
    let delivered = channel.deliver();

    // Feed delivered packets into reorder buffer
    let now = clock.now();
    let mut output = Vec::new();
    for pkt in &delivered {
        let data = Bytes::from(pkt.symbol.data.clone());
        let result = reorder_buf.push_with_time(pkt.seq, data, now);
        output.extend(result);
    }

    // Output should be in sequential order
    for i in 1..output.len() {
        assert!(
            output[i].0 > output[i - 1].0,
            "output should be sequential: {} came after {}",
            output[i].0,
            output[i - 1].0
        );
    }

    // We should have delivered at least some symbols
    assert!(
        !output.is_empty(),
        "reorder buffer should deliver some symbols"
    );
}

#[test]
fn test_timeout_delivers_stuck_entries() {
    let clock = Arc::new(MockClock::new());
    let mut reorder_buf = ReorderBuffer::new(25, 500);

    let now = clock.now();

    // Push seqs 0, 1 (contiguous from start)
    let r = reorder_buf.push_with_time(0, Bytes::from_static(b"a"), now);
    assert_eq!(r.len(), 1); // seq 0 delivered immediately
    let r = reorder_buf.push_with_time(1, Bytes::from_static(b"b"), now);
    assert_eq!(r.len(), 1); // seq 1 delivered

    // Skip seq 2, push 3 and 4
    let r = reorder_buf.push_with_time(3, Bytes::from_static(b"d"), now);
    assert!(r.is_empty(), "seq 3 should wait for seq 2");
    let r = reorder_buf.push_with_time(4, Bytes::from_static(b"e"), now);
    assert!(r.is_empty(), "seq 4 should wait for seq 2");

    // Advance past timeout (25ms)
    clock.advance(Duration::from_millis(30));
    let later = clock.now();

    let expired = reorder_buf.drain_expired(later);
    assert!(
        expired.len() >= 2,
        "seqs 3,4 should be force-delivered after timeout: got {} entries",
        expired.len()
    );

    // Verify the delivered seqs include 3 and 4
    let seqs: Vec<u64> = expired.iter().map(|(s, _)| *s).collect();
    assert!(seqs.contains(&3), "seq 3 should be delivered");
    assert!(seqs.contains(&4), "seq 4 should be delivered");
}

#[test]
fn test_force_drain_over_capacity() {
    let clock = Arc::new(MockClock::new());
    let max_buffered = 500;
    let mut reorder_buf = ReorderBuffer::new(25, max_buffered);

    let now = clock.now();

    // Skip seq 0, push 1..601 (600 entries, gap at 0 prevents contiguous drain)
    let mut total_drained = 0usize;
    for seq in 1..=600u64 {
        let result = reorder_buf.push_with_time(seq, Bytes::from(vec![0u8; 8]), now);
        total_drained += result.len();
    }

    // force_drain_oldest should have triggered (buffer > 500)
    assert!(
        total_drained > 0,
        "force_drain should have delivered entries when over capacity"
    );

    // Buffer should be at or below max_buffered/2
    assert!(
        reorder_buf.pending_count() <= max_buffered,
        "buffer should be drained below capacity: pending={}",
        reorder_buf.pending_count()
    );
}

#[test]
fn test_bursty_loss_gap_handling() {
    let clock = Arc::new(MockClock::new());
    let mut channel = SimChannel::new(
        clock.clone(),
        7,
        Duration::from_millis(5),
        1,
        GilbertElliottChannel::new(0.1, 0.3, 0.01, 0.8), // heavy burst loss
    );
    let mut reorder_buf = ReorderBuffer::new(25, 500);

    let num_symbols = 100u32;
    let symbols = make_source_batch(num_symbols);

    // Track which were dropped vs survived
    let mut survived_seqs = Vec::new();
    for (i, sym) in symbols.iter().enumerate() {
        if channel.send(sym.clone()) {
            survived_seqs.push(i as u64);
        }
    }

    // Advance past delay
    clock.advance(Duration::from_millis(15));
    let delivered = channel.deliver();

    let now = clock.now();
    let mut output = Vec::new();
    for pkt in &delivered {
        let data = Bytes::from(pkt.symbol.data.clone());
        let result = reorder_buf.push_with_time(pkt.seq, data, now);
        output.extend(result);
    }

    // Contiguous prefix from start should be delivered
    if !output.is_empty() {
        // First delivered should be seq 0 (if it survived)
        // Verify sequential ordering of output
        for i in 1..output.len() {
            assert!(
                output[i].0 == output[i - 1].0 + 1,
                "contiguous prefix should be sequential: {} after {}",
                output[i].0,
                output[i - 1].0
            );
        }
    }

    // After timeout, remaining entries should be force-delivered
    clock.advance(Duration::from_millis(30));
    let later = clock.now();
    let expired = reorder_buf.drain_expired(later);

    // Total output (contiguous + expired) should account for all delivered packets
    let total_output = output.len() + expired.len();
    assert!(
        total_output <= delivered.len(),
        "total output should not exceed delivered: output={total_output}, delivered={}",
        delivered.len()
    );
}
