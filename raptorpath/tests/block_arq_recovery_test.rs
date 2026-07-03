//! Block-mode ARQ recovery (P8): symbol loss → Ack diff → fresh repair →
//! block decodes WITHOUT proactive FEC and without decoder eviction.
//!
//! This is the exact regime the L1 harness measured failing: Bulk's
//! completion-exposure glide sets mid-stream r* = 0 (no proactive repairs),
//! so any lost symbol previously stalled the block until the receiver
//! evicted it. There is no in-process two-Net harness, so the wire is
//! simulated at the batch level: every symbol travels as its own
//! SymbolBatch (matching MTU-sized batches in production), the receiver
//! acks each batch it receives, and lost batches are revealed by later
//! acks on the same path (dup-ack analogue) or the timeout sweep.

use bytes::Bytes;
use raptorpath::fec::{EncodingParams, FecBackend, FecStream, WireSymbol};
use raptorpath::net::block_arq::{BlockArq, LATER_ACK_LOSS_THRESHOLD};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_millis(100);

struct SimBlock {
    params: EncodingParams,
    data: Vec<u8>,
    source: Vec<WireSymbol>,
}

/// Encode a block with ZERO proactive repairs (mid-stream Bulk: r* = 0).
fn encode_block(block_id: u64, len: usize, backend: FecBackend) -> SimBlock {
    let data: Vec<u8> = (0..len).map(|i| (i * 31 + block_id as usize) as u8).collect();
    let symbol_size = 64u16;
    let source_symbols = (len as f64 / symbol_size as f64).ceil() as u32;
    let params = EncodingParams {
        source_symbols,
        symbol_size,
        repair_count: 0,
        block_id,
    };
    let mut fec = FecStream::new(&data, params, backend);
    let source = fec.take_source_symbols();
    SimBlock { params, data, source }
}

#[test]
fn loss_ack_diff_repair_decode_raptorq() {
    let now = Instant::now();
    let mut arq = BlockArq::new();
    let block = encode_block(1, 1280, FecBackend::RaptorQ); // k = 20

    // Sender retains the block (as encode_to_interleave_buf does).
    arq.on_block_encoded(1, Bytes::from(block.data.clone()), block.params, FecBackend::RaptorQ);

    // Receiver-side decoder (created by BlockStart).
    let mut decoder = FecBackend::RaptorQ.create_decoder(block.params, block.data.len() as u64);

    // Wire: one symbol per batch; batches 5 and 11 are lost.
    let lost = [5usize, 11usize];
    let mut decoded: Option<Bytes> = None;
    let mut events = Vec::new();
    for (i, sym) in block.source.iter().enumerate() {
        let batch_seq = i as u64;
        arq.on_batch_sent(batch_seq, 0, vec![(sym.block_id, sym.payload_id)], now);
        if lost.contains(&i) {
            continue; // datagram dropped — no Ack for this batch_seq
        }
        // Delivered: receiver decodes what it can and acks the batch.
        if let Some(d) = decoder.add_symbol(sym) {
            decoded = Some(d);
        }
        events.extend(arq.on_ack(batch_seq, 0, &[sym.payload_id], now, TIMEOUT));
    }
    assert!(decoded.is_none(), "k-2 of k source symbols cannot decode alone");

    // The dup-ack evidence revealed both losses (>= 3 later acks each).
    let missing: usize = events.iter().map(|e| e.missing.len()).sum();
    assert_eq!(missing, 2, "both lost symbols detected via Ack diff");

    // Sender mints FRESH repairs (RaptorQ: any repair fills any hole).
    let plans = arq.plan_repairs(events, 0.026); // ε̂ = C2's 2.6%
    assert_eq!(plans.len(), 1);
    assert!(plans[0].symbols.len() >= 2);
    assert!(plans[0].symbols.iter().all(|s| s.is_repair), "fresh repairs, not source resends");

    // Repairs arrive: the block decodes — no eviction, no proactive FEC.
    for sym in &plans[0].symbols {
        if let Some(d) = decoder.add_symbol(sym) {
            decoded = Some(d);
        }
    }
    let decoded = decoded.expect("block must decode after ARQ repairs");
    assert_eq!(&decoded[..block.data.len()], &block.data[..], "decoded data matches");
}

#[test]
fn tail_loss_recovered_by_sweep() {
    // The LAST batch of a transfer is lost: no later acks on the path can
    // reveal it — only the timeout sweep does.
    let now = Instant::now();
    let mut arq = BlockArq::new();
    let block = encode_block(2, 640, FecBackend::RaptorQ); // k = 10
    arq.on_block_encoded(2, Bytes::from(block.data.clone()), block.params, FecBackend::RaptorQ);
    let mut decoder = FecBackend::RaptorQ.create_decoder(block.params, block.data.len() as u64);

    let last = block.source.len() - 1;
    let mut decoded: Option<Bytes> = None;
    for (i, sym) in block.source.iter().enumerate() {
        let batch_seq = i as u64;
        arq.on_batch_sent(batch_seq, 0, vec![(sym.block_id, sym.payload_id)], now);
        if i == last {
            continue; // tail loss
        }
        if let Some(d) = decoder.add_symbol(sym) {
            decoded = Some(d);
        }
        let ev = arq.on_ack(batch_seq, 0, &[sym.payload_id], now, TIMEOUT);
        assert!(ev.is_empty(), "earlier acks cannot implicate a later batch");
    }
    assert!(decoded.is_none());

    // Inside the timeout: nothing fires (a delayed Ack must not cause
    // spurious repairs).
    assert!(arq.sweep(now + TIMEOUT / 2, &|_| TIMEOUT).is_empty());

    // Past the timeout: the tail batch is declared lost, repaired, decoded.
    let events = arq.sweep(now + TIMEOUT * 2, &|_| TIMEOUT);
    assert_eq!(events.len(), 1);
    let plans = arq.plan_repairs(events, 0.026);
    assert_eq!(plans.len(), 1);
    for sym in &plans[0].symbols {
        if let Some(d) = decoder.add_symbol(sym) {
            decoded = Some(d);
        }
    }
    let decoded = decoded.expect("tail loss recovered via sweep");
    assert_eq!(&decoded[..block.data.len()], &block.data[..]);
}

#[test]
fn lost_repair_triggers_second_round() {
    // Round 1's repair batch is itself lost; its ledger entry (repair
    // batches re-enter the ledger) drives round 2, which decodes.
    let now = Instant::now();
    let mut arq = BlockArq::new();
    let block = encode_block(3, 640, FecBackend::RaptorQ);
    arq.on_block_encoded(3, Bytes::from(block.data.clone()), block.params, FecBackend::RaptorQ);
    let mut decoder = FecBackend::RaptorQ.create_decoder(block.params, block.data.len() as u64);

    let mut decoded: Option<Bytes> = None;
    let mut events = Vec::new();
    for (i, sym) in block.source.iter().enumerate() {
        arq.on_batch_sent(i as u64, 0, vec![(sym.block_id, sym.payload_id)], now);
        if i == 0 {
            continue; // lose source symbol 0
        }
        if let Some(d) = decoder.add_symbol(sym) {
            decoded = Some(d);
        }
        events.extend(arq.on_ack(i as u64, 0, &[sym.payload_id], now, TIMEOUT));
    }
    // Loss detected (>= LATER_ACK_LOSS_THRESHOLD later acks arrived).
    assert!(block.source.len() > LATER_ACK_LOSS_THRESHOLD as usize);
    assert!(!events.is_empty(), "dup-ack evidence must fire during the ack stream");
    let round1 = arq.plan_repairs(events, 0.026);
    assert_eq!(round1.len(), 1);
    let round1_ids: Vec<u32> = round1[0].symbols.iter().map(|s| s.payload_id).collect();

    // The repair batch is sent... and lost. Record it, never ack it.
    let repair_seq = 1000u64;
    arq.on_batch_sent(
        repair_seq,
        0,
        round1[0].symbols.iter().map(|s| (s.block_id, s.payload_id)).collect(),
        now + TIMEOUT,
    );
    let events = arq.sweep(now + TIMEOUT * 3, &|_| TIMEOUT);
    assert!(!events.is_empty(), "lost repair batch must re-fire");

    // Round 2 mints DIFFERENT fresh repairs; they decode the block.
    let round2 = arq.plan_repairs(events, 0.026);
    assert_eq!(round2.len(), 1);
    for s in &round2[0].symbols {
        assert!(!round1_ids.contains(&s.payload_id), "round 2 must not repeat round 1 ESIs");
        if let Some(d) = decoder.add_symbol(s) {
            decoded = Some(d);
        }
    }
    let decoded = decoded.expect("block decodes after second repair round");
    assert_eq!(&decoded[..block.data.len()], &block.data[..]);
}

#[test]
fn fixed_rate_backend_source_resend_decodes() {
    // Reed-Solomon cannot mint post-hoc repairs — the fallback resends the
    // exact missing SOURCE symbols from retained data.
    let now = Instant::now();
    let mut arq = BlockArq::new();
    let block = encode_block(4, 640, FecBackend::ReedSolomon);
    arq.on_block_encoded(4, Bytes::from(block.data.clone()), block.params, FecBackend::ReedSolomon);
    let mut decoder =
        FecBackend::ReedSolomon.create_decoder(block.params, block.data.len() as u64);

    let mut decoded: Option<Bytes> = None;
    let mut events = Vec::new();
    for (i, sym) in block.source.iter().enumerate() {
        arq.on_batch_sent(i as u64, 0, vec![(sym.block_id, sym.payload_id)], now);
        if i == 3 {
            continue;
        }
        if let Some(d) = decoder.add_symbol(sym) {
            decoded = Some(d);
        }
        events.extend(arq.on_ack(i as u64, 0, &[sym.payload_id], now, TIMEOUT));
    }
    assert!(decoded.is_none());

    let plans = arq.plan_repairs(events, 0.0);
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].symbols.len(), 1);
    assert_eq!(plans[0].symbols[0].payload_id, 3, "exact missing source symbol");
    assert!(!plans[0].symbols[0].is_repair);

    for sym in &plans[0].symbols {
        if let Some(d) = decoder.add_symbol(sym) {
            decoded = Some(d);
        }
    }
    let decoded = decoded.expect("RS block decodes after source resend");
    assert_eq!(&decoded[..block.data.len()], &block.data[..]);
}
