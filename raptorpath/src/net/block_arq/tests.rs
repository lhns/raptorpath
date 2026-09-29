use super::*;

const T0: Duration = Duration::from_millis(0);
const TIMEOUT: Duration = Duration::from_millis(100);

fn params(k: u32, sym: u16, r: u32, block_id: u64) -> EncodingParams {
    EncodingParams {
        source_symbols: k,
        symbol_size: sym,
        repair_count: r,
        block_id,
    }
}

fn retain_block(arq: &mut BlockArq, block_id: u64, len: usize, backend: FecBackend) -> Bytes {
    let data = Bytes::from(vec![(block_id & 0xff) as u8; len]);
    let k = (len as f64 / 64.0).ceil() as u32;
    arq.on_block_encoded(block_id, data.clone(), params(k, 64, 2, block_id), backend, Instant::now());
    data
}

#[test]
fn ack_full_batch_no_events() {
    let mut arq = BlockArq::new();
    let now = Instant::now();
    arq.on_batch_sent(1, 0, vec![(10, 0), (10, 1)], now);
    let ev = arq.on_ack(1, 0, &[0, 1], now + T0, TIMEOUT);
    assert!(ev.is_empty());
    assert_eq!(arq.ledger_len(), 0);
}

#[test]
fn ack_partial_batch_diffs_missing() {
    let mut arq = BlockArq::new();
    let now = Instant::now();
    arq.on_batch_sent(1, 0, vec![(10, 0), (10, 1), (10, 2)], now);
    let ev = arq.on_ack(1, 0, &[0, 2], now, TIMEOUT);
    assert_eq!(
        ev,
        vec![LossEvent {
            block_id: 10,
            path_id: 0,
            missing: vec![1]
        }]
    );
}

#[test]
fn mixed_block_batch_duplicate_payload_ids() {
    // Two blocks contribute payload_id 3; only one instance arrives.
    // Multiset matching must charge exactly one loss (which block is
    // ambiguous from the Ack alone — datagram atomicity makes this a
    // theoretical case; we only require no over/under-counting).
    let mut arq = BlockArq::new();
    let now = Instant::now();
    arq.on_batch_sent(7, 2, vec![(100, 3), (200, 3), (200, 4)], now);
    let ev = arq.on_ack(7, 2, &[3, 4], now, TIMEOUT);
    let total_missing: usize = ev.iter().map(|e| e.missing.len()).sum();
    assert_eq!(total_missing, 1);
    assert!(ev.iter().all(|e| e.path_id == 2));
}

#[test]
fn dup_ack_threshold_declares_loss() {
    let mut arq = BlockArq::new();
    let now = Instant::now();
    arq.on_batch_sent(1, 0, vec![(10, 0)], now); // will be lost
    for seq in 2..=4 {
        arq.on_batch_sent(seq, 0, vec![(10, seq as u32)], now);
    }
    // Two later acks: not yet lost.
    assert!(arq.on_ack(2, 0, &[2], now, TIMEOUT).is_empty());
    assert!(arq.on_ack(3, 0, &[3], now, TIMEOUT).is_empty());
    // Third later ack crosses LATER_ACK_LOSS_THRESHOLD.
    let ev = arq.on_ack(4, 0, &[4], now, TIMEOUT);
    assert_eq!(
        ev,
        vec![LossEvent {
            block_id: 10,
            path_id: 0,
            missing: vec![0]
        }]
    );
    assert_eq!(arq.ledger_len(), 0);
}

#[test]
fn other_path_acks_do_not_count() {
    let mut arq = BlockArq::new();
    let now = Instant::now();
    arq.on_batch_sent(1, 0, vec![(10, 0)], now);
    for seq in 2..=6 {
        arq.on_batch_sent(seq, 1, vec![(10, seq as u32)], now);
        assert!(
            arq.on_ack(seq, 1, &[seq as u32], now, TIMEOUT).is_empty(),
            "path-1 acks must not declare a path-0 batch lost"
        );
    }
    assert_eq!(arq.ledger_len(), 1);
}

#[test]
fn lost_ack_no_spurious_repair_before_timeout() {
    // Batch delivered but its Ack lost: nothing should fire until the
    // dup-ack evidence or timeout — a sweep inside the timeout is a
    // no-op.
    let mut arq = BlockArq::new();
    let now = Instant::now();
    arq.on_batch_sent(1, 0, vec![(10, 0)], now);
    let ev = arq.sweep(now + Duration::from_millis(50), &|_| TIMEOUT);
    assert!(ev.is_empty());
    assert_eq!(arq.ledger_len(), 1);
    // Past the timeout the batch is delivered-or-lost either way; it
    // fires exactly once (entry removed — no duplicate on re-sweep).
    let ev = arq.sweep(now + Duration::from_millis(150), &|_| TIMEOUT);
    assert_eq!(ev.len(), 1);
    assert!(arq.sweep(now + Duration::from_millis(300), &|_| TIMEOUT).is_empty());
}

#[test]
fn reordered_ack_for_declared_batch_is_harmless() {
    let mut arq = BlockArq::new();
    let now = Instant::now();
    arq.on_batch_sent(1, 0, vec![(10, 0)], now);
    for seq in 2..=4 {
        arq.on_batch_sent(seq, 0, vec![(10, seq as u32)], now);
        arq.on_ack(seq, 0, &[seq as u32], now, TIMEOUT);
    }
    // Batch 1 was declared lost above; its late Ack must not panic or
    // emit events.
    assert!(arq.on_ack(1, 0, &[0], now, TIMEOUT).is_empty());
}

#[test]
fn done_blocks_suppress_events() {
    let mut arq = BlockArq::new();
    let now = Instant::now();
    arq.on_batch_sent(1, 0, vec![(10, 0)], now);
    arq.on_block_done(10);
    let ev = arq.sweep(now + TIMEOUT, &|_| TIMEOUT);
    assert!(ev.is_empty(), "decoded block must not trigger repairs");
}

#[test]
fn ledger_cap_drops_oldest() {
    let mut arq = BlockArq::with_caps(4, RETAIN_MAX_BLOCKS, RETAIN_MAX_BYTES);
    let now = Instant::now();
    for seq in 0..10u64 {
        arq.on_batch_sent(seq, 0, vec![(seq, 0)], now);
    }
    assert_eq!(arq.ledger_len(), 4);
}

#[test]
fn retention_count_and_byte_caps() {
    let mut arq = BlockArq::with_caps(LEDGER_MAX_BATCHES, 4, 10_000);
    for b in 0..8u64 {
        retain_block(&mut arq, b, 1000, FecBackend::RaptorQ);
    }
    let (blocks, bytes) = arq.retained_stats();
    assert_eq!(blocks, 4, "count cap");
    assert!(bytes <= 10_000);

    // Byte cap dominates: 3 × 4000 > 10000 → evicts to 2.
    let mut arq = BlockArq::with_caps(LEDGER_MAX_BATCHES, 64, 10_000);
    for b in 0..3u64 {
        retain_block(&mut arq, b, 4000, FecBackend::RaptorQ);
    }
    let (blocks, bytes) = arq.retained_stats();
    assert_eq!(blocks, 2, "byte cap");
    assert!(bytes <= 10_000);
}

#[test]
fn rateless_repairs_are_fresh_and_distinct() {
    let mut arq = BlockArq::new();
    let data = Bytes::from(vec![7u8; 640]);
    let p = params(10, 64, 3, 42);
    arq.on_block_encoded(42, data, p, FecBackend::RaptorQ, Instant::now());

    let ev = vec![LossEvent {
        block_id: 42,
        path_id: 0,
        missing: vec![2, 5],
    }];
    let plans = arq.plan_repairs(ev, 0.0);
    assert_eq!(plans.len(), 1);
    let first_ids: Vec<u32> = plans[0].symbols.iter().map(|s| s.payload_id).collect();
    assert_eq!(plans[0].symbols.len(), 2);
    assert!(plans[0].symbols.iter().all(|s| s.is_repair));

    // A second round must mint different repair symbols.
    let ev = vec![LossEvent {
        block_id: 42,
        path_id: 0,
        missing: vec![2, 5],
    }];
    let plans2 = arq.plan_repairs(ev, 0.0);
    assert_eq!(plans2.len(), 1);
    for s in &plans2[0].symbols {
        assert!(
            !first_ids.contains(&s.payload_id),
            "fresh repairs must not repeat earlier ESIs"
        );
    }
}

#[test]
fn fixed_rate_resends_exact_missing() {
    let mut arq = BlockArq::new();
    let data = Bytes::from(vec![9u8; 640]);
    let p = params(10, 64, 3, 43);
    arq.on_block_encoded(43, data, p, FecBackend::ReedSolomon, Instant::now());

    // Missing: source 4 and repair 11 (k=10 → repair ids 10..13).
    let ev = vec![LossEvent {
        block_id: 43,
        path_id: 1,
        missing: vec![4, 11],
    }];
    let plans = arq.plan_repairs(ev, 0.0);
    assert_eq!(plans.len(), 1);
    let mut ids: Vec<u32> = plans[0].symbols.iter().map(|s| s.payload_id).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![4, 11], "exact missing symbols resent");
}

#[test]
fn margin_accumulates_fractionally_and_doubles_on_retry() {
    let mut arq = BlockArq::new();
    for b in 0..20u64 {
        let data = Bytes::from(vec![b as u8; 640]);
        arq.on_block_encoded(b, data, params(10, 64, 2, b), FecBackend::RaptorQ, Instant::now());
    }
    // ε̂ = 0.25, 1 missing per event, round 0 → 0.25 margin per event:
    // every 4th event carries one extra symbol.
    let mut total = 0usize;
    for b in 0..8u64 {
        let plans = arq.plan_repairs(
            vec![LossEvent {
                block_id: b,
                path_id: 0,
                missing: vec![0],
            }],
            0.25,
        );
        total += plans[0].symbols.len();
    }
    assert_eq!(total, 8 + 2, "8 missing + floor(8×0.25) margin symbols");

    // Retry on an already-repaired block doubles the margin rate:
    // round 1 → 1 missing × 0.25 × 2 = 0.5 debt (was 0.25).
    let before = arq.margin_debt;
    let plans = arq.plan_repairs(
        vec![LossEvent {
            block_id: 0,
            path_id: 0,
            missing: vec![0],
        }],
        0.25,
    );
    assert_eq!(plans.len(), 1);
    let debt_gain = arq.margin_debt - before + plans[0].symbols.len() as f64 - 1.0;
    assert!(
        (debt_gain - 0.5).abs() < 1e-9,
        "round-1 margin must be doubled (got {debt_gain})"
    );
}

#[test]
fn repair_rounds_capped() {
    let mut arq = BlockArq::new();
    let data = Bytes::from(vec![1u8; 640]);
    arq.on_block_encoded(50, data, params(10, 64, 2, 50), FecBackend::RaptorQ, Instant::now());
    for round in 0..MAX_REPAIR_ROUNDS + 2 {
        let plans = arq.plan_repairs(
            vec![LossEvent {
                block_id: 50,
                path_id: 0,
                missing: vec![0],
            }],
            0.0,
        );
        if round < MAX_REPAIR_ROUNDS {
            assert_eq!(plans.len(), 1, "round {round} should plan");
        } else {
            assert!(plans.is_empty(), "round {round} must be capped");
        }
    }
}

#[test]
fn evicted_block_yields_no_plan() {
    let mut arq = BlockArq::new();
    let ev = vec![LossEvent {
        block_id: 999,
        path_id: 0,
        missing: vec![0],
    }];
    assert!(arq.plan_repairs(ev, 0.1).is_empty());
}

#[test]
fn block_failed_mints_deficit_repairs() {
    let mut arq = BlockArq::new();
    let data = Bytes::from(vec![3u8; 640]);
    arq.on_block_encoded(60, data, params(10, 64, 2, 60), FecBackend::RaptorQ, Instant::now());
    let plan = arq.on_block_failed(60, 3, 0, 0.0).expect("plan");
    assert_eq!(plan.symbols.len(), 3);
    assert!(plan.symbols.iter().all(|s| s.is_repair));
    // Fixed-rate backends cannot mint post-hoc without ids.
    let data = Bytes::from(vec![3u8; 640]);
    arq.on_block_encoded(61, data, params(10, 64, 2, 61), FecBackend::ReedSolomon, Instant::now());
    assert!(arq.on_block_failed(61, 3, 0, 0.0).is_none());
}

#[test]
fn idle_reannounce_recovers_orphaned_block() {
    // The idle stall: a block's BlockStart datagram is lost,
    // its symbols are all delivered-and-acked, so the ARQ ledger is empty
    // and `sweep` sees nothing — yet the block never decoded. The idle
    // re-announce must re-send BlockStart (+ spare) once the block has been
    // quiet past the loss timeout, and stop the instant it completes.
    let mut arq = BlockArq::new();
    let t0 = Instant::now();
    let data = Bytes::from(vec![7u8; 640]);
    arq.on_block_encoded(70, data, params(10, 64, 2, 70), FecBackend::RaptorQ, t0);
    // Its batch was sent and acked (ledger cleared): activity at t0.
    arq.on_batch_sent(1, 0, vec![(70, 0)], t0);
    arq.on_ack(1, 0, &[0], t0, TIMEOUT);
    assert_eq!(arq.ledger_len(), 0, "ledger empty — sweep is blind here");
    assert!(arq.sweep(t0 + TIMEOUT, &|_| TIMEOUT).is_empty());

    // Before the timeout: no re-announce (block might still be pipelining).
    let early = arq.idle_reannounce(t0 + Duration::from_millis(20), &|_| TIMEOUT, 0, 0.1);
    assert!(early.is_empty(), "must not fire during normal pipelining");

    // Past the timeout: re-announce fires with a BlockStart + spare.
    let plans = arq.idle_reannounce(t0 + Duration::from_millis(150), &|_| TIMEOUT, 0, 0.1);
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].block_id, 70);
    assert!(!plans[0].symbols.is_empty(), "spare repair accompanies re-announce");

    // It backs off within a round (last_activity refreshed): immediate
    // re-call is a no-op until the next timeout elapses.
    assert!(arq
        .idle_reannounce(t0 + Duration::from_millis(160), &|_| TIMEOUT, 0, 0.1)
        .is_empty());

    // Block completes → re-announce stops permanently.
    arq.on_block_done(70);
    assert!(arq
        .idle_reannounce(t0 + Duration::from_millis(400), &|_| TIMEOUT, 0, 0.1)
        .is_empty());
}

#[test]
fn idle_reannounce_bounded_by_round_cap() {
    let mut arq = BlockArq::new();
    let t0 = Instant::now();
    let data = Bytes::from(vec![1u8; 640]);
    arq.on_block_encoded(71, data, params(10, 64, 2, 71), FecBackend::RaptorQ, t0);
    let mut fired = 0u8;
    let mut counts: Vec<usize> = Vec::new();
    // Keep the block un-done and always past-timeout: it must fire at most
    // MAX_REANNOUNCE_ROUNDS times, then give way to the receiver backstop.
    for r in 0..(MAX_REANNOUNCE_ROUNDS as u32 + 4) {
        let now = t0 + Duration::from_millis(200 * (r as u64 + 1));
        let plans = arq.idle_reannounce(now, &|_| TIMEOUT, 0, 0.1);
        if let Some(p) = plans.first() {
            fired += 1;
            counts.push(p.symbols.len());
        }
    }
    assert_eq!(fired, MAX_REANNOUNCE_ROUNDS, "re-announce is bounded");
    // Escalation: the spare grows across rounds (cheap probe -> full block).
    assert!(
        counts.last().unwrap() > counts.first().unwrap(),
        "spare must escalate: {counts:?}"
    );
}

/// Retain one block with the Auto hint's geometry (16 KiB blocks, 1200 B
/// symbols, `BlockProfile::from_hint(Auto)` in `net/mod.rs`).
fn retain_auto_block(arq: &mut BlockArq, block_id: u64) {
    const AUTO_BLOCK: usize = 16 * 1024;
    const AUTO_SYM: u16 = 1200;
    let data = Bytes::from(vec![(block_id & 0xff) as u8; AUTO_BLOCK]);
    let k = AUTO_BLOCK.div_ceil(AUTO_SYM as usize) as u32;
    arq.on_block_encoded(
        block_id,
        data,
        params(k, AUTO_SYM, 0, block_id),
        FecBackend::RaptorQ,
        Instant::now(),
    );
}

/// The retention horizon is a BYTE horizon: the shipped defaults must keep
/// an Auto-geometry block (16 KiB) repairable for as long as a Bulk one
/// (64 KiB) in byte terms. 80 in-flight Auto blocks = 1.25 MiB, well under
/// `RETAIN_MAX_BYTES` (4 MiB); the oldest un-done block must still be
/// retained and `plan_repairs` must still plan its repair. (Regression: a
/// 64-block count cap bound first for Auto — a 1 MiB horizon, 4x shorter
/// than Bulk's — and `plan_repairs` silently skipped the evicted block.)
#[test]
fn auto_geometry_retention_is_byte_bounded_not_count_bounded() {
    let mut arq = BlockArq::new();
    const N: u64 = 80;
    for b in 0..N {
        retain_auto_block(&mut arq, b);
    }
    let (blocks, bytes) = arq.retained_stats();
    assert!(bytes <= RETAIN_MAX_BYTES, "byte cap must still hold: {bytes}");
    assert_eq!(
        blocks, N as usize,
        "all {N} Auto blocks (1.25 MiB < 4 MiB) must be retained"
    );
    let plans = arq.plan_repairs(
        vec![LossEvent {
            block_id: 0,
            path_id: 0,
            missing: vec![0],
        }],
        0.0,
    );
    assert_eq!(plans.len(), 1, "oldest un-done block 0 must still be repairable");
    assert_eq!(plans[0].block_id, 0);
    assert!(!plans[0].symbols.is_empty());
}
