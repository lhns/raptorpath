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
    let mut arq = BlockArq::with_caps(4, RETAIN_MAX_BYTES);
    let now = Instant::now();
    for seq in 0..10u64 {
        arq.on_batch_sent(seq, 0, vec![(seq, 0)], now);
    }
    assert_eq!(arq.ledger_len(), 4);
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

/// Byte-only horizon: the retained count is whatever fits, never a count cap.
#[test]
fn retention_is_byte_bounded_only() {
    let per = 1000 + RETAIN_PER_BLOCK_OVERHEAD;
    let mut arq = BlockArq::with_caps(LEDGER_MAX_BATCHES, 3 * per);
    for b in 0..8u64 {
        retain_block(&mut arq, b, 1000, FecBackend::RaptorQ);
    }
    let (blocks, bytes) = arq.retained_stats();
    assert_eq!(blocks, 3, "exactly the blocks that fit the byte horizon");
    assert_eq!(bytes, 3 * per, "charge = data + per-block overhead");

    // Many small blocks under the shipped default: far more than 64 retained,
    // all within RETAIN_MAX_BYTES.
    let mut arq = BlockArq::new();
    for b in 0..1000u64 {
        retain_block(&mut arq, b, 1000, FecBackend::RaptorQ);
    }
    let (blocks, bytes) = arq.retained_stats();
    assert_eq!(blocks, 1000);
    assert_eq!(bytes, 1000 * per);
}

/// Tiny (flush-timeout) blocks cannot grow the retained COUNT without bound:
/// the per-block overhead charge caps it at RETAIN_MAX_BLOCKS_DERIVED.
#[test]
fn tiny_blocks_count_bounded_by_overhead_charge() {
    let mut arq = BlockArq::new();
    let n = RETAIN_MAX_BLOCKS_DERIVED as u64 + 500;
    for b in 0..n {
        retain_block(&mut arq, b, 1, FecBackend::RaptorQ);
    }
    let (blocks, bytes) = arq.retained_stats();
    assert!(bytes <= RETAIN_BUDGET_BYTES);
    assert_eq!(blocks, RETAIN_BUDGET_BYTES / (1 + RETAIN_PER_BLOCK_OVERHEAD));
    assert!(blocks <= RETAIN_MAX_BLOCKS_DERIVED);
}

/// A materialized repair encoder is charged to the byte horizon.
#[test]
fn cached_encoder_footprint_is_charged() {
    for backend in [FecBackend::RaptorQ, FecBackend::Rlc, FecBackend::ReedSolomon] {
        let mut arq = BlockArq::new();
        let len: usize = 16 * 1024;
        // RS needs k + r <= 255: use 1200-byte symbols for every backend.
        let p = params(len.div_ceil(1200) as u32, 1200, 2, 7);
        arq.on_block_encoded(7, Bytes::from(vec![7u8; len]), p, backend, Instant::now());
        let before = arq.retained_stats().1;
        assert_eq!(before, len + RETAIN_PER_BLOCK_OVERHEAD);
        let plans = arq.plan_repairs(
            vec![LossEvent {
                block_id: 7,
                path_id: 0,
                missing: vec![0],
            }],
            0.0,
        );
        assert_eq!(plans.len(), 1, "{backend:?}");
        let fp = encoder_footprint_bytes(backend, &p);
        assert!(fp >= len, "{backend:?}: encoder holds at least the block data");
        assert_eq!(arq.retained_stats().1, before + fp, "{backend:?}");
        // A second round reuses the cached encoder: no double charge.
        let _ = arq.plan_repairs(
            vec![LossEvent {
                block_id: 7,
                path_id: 0,
                missing: vec![1],
            }],
            0.0,
        );
        assert_eq!(arq.retained_stats().1, before + fp, "{backend:?}: charged once");
        // Done releases the whole charge.
        arq.on_block_done(7);
        assert_eq!(arq.retained_stats(), (0, 0), "{backend:?}");
    }
}

/// Repair touches the block to the hot end of the LRU and protects its
/// encoder; under pressure, cached encoders are dropped BEFORE any block
/// data, and a later insertion evicts the next-coldest block, not the one
/// under repair, whose dropped encoder is rebuilt on demand.
#[test]
fn repair_touch_protects_block_and_encoders_drop_before_data() {
    let len = 1000;
    let p = |b| params(1, 1000, 0, b);
    let fp = encoder_footprint_bytes(FecBackend::RaptorQ, &p(0));
    let per = len + RETAIN_PER_BLOCK_OVERHEAD;
    // Budget = exactly 3 blocks of data; no headroom for any encoder.
    let mut arq = BlockArq::with_caps(LEDGER_MAX_BATCHES, 3 * per);
    for b in 0..3u64 {
        arq.on_block_encoded(b, Bytes::from(vec![1u8; len]), p(b), FecBackend::RaptorQ, Instant::now());
    }
    let ev = |b| LossEvent {
        block_id: b,
        path_id: 0,
        missing: vec![0],
    };
    let plans = arq.plan_repairs(vec![ev(0)], 0.0);
    assert_eq!(plans.len(), 1);
    // The protected encoder of the block under repair never evicts data:
    // all 3 blocks stay, over budget by exactly that one encoder.
    assert_eq!(arq.retained_stats(), (3, 3 * per + fp));
    assert_eq!(arq.retained_breakdown(), (3 * per, fp, 0));

    arq.on_block_encoded(3, Bytes::from(vec![1u8; len]), p(3), FecBackend::RaptorQ, Instant::now());
    // Tier 1 dropped block 0's (now unprotected) encoder; tier 2 then
    // evicted block 1 (coldest after the touch). Block 0 survives.
    assert_eq!(arq.retained_breakdown(), (3 * per, 0, 1));
    assert_eq!(arq.retained_stats().0, 3);
    assert!(arq.plan_repairs(vec![ev(1)], 0.0).is_empty());
    assert_eq!(arq.repair_skips_evicted(), 1);
    let plans = arq.plan_repairs(vec![ev(0)], 0.0);
    assert_eq!(plans.len(), 1, "repaired block must outlive colder blocks");
    assert!(!plans[0].symbols.is_empty(), "encoder rebuilt on demand");
}

/// Bulk geometry (64 KiB blocks, 1200 B symbols): 64 blocks ALL under
/// repair keep all 64 blocks' data retained — the historical Bulk horizon
/// (64 × 64 KiB = 4 MiB of data) — with encoders dropped to fit the budget;
/// the oldest block's next repair still succeeds (encoder rebuilt), with
/// fresh repair indices.
#[test]
fn bulk_geometry_64_blocks_under_repair_keep_all_data() {
    const BULK_BLOCK: usize = 64 * 1024;
    const BULK_SYM: u16 = 1200;
    let k = BULK_BLOCK.div_ceil(BULK_SYM as usize) as u32;
    let mut arq = BlockArq::new();
    let ev = |b| LossEvent {
        block_id: b,
        path_id: 0,
        missing: vec![0],
    };
    let mut first_esi_0 = None;
    for b in 0..64u64 {
        arq.on_block_encoded(
            b,
            Bytes::from(vec![(b & 0xff) as u8; BULK_BLOCK]),
            params(k, BULK_SYM, 0, b),
            FecBackend::RaptorQ,
            Instant::now(),
        );
        let plans = arq.plan_repairs(vec![ev(b)], 0.0);
        assert_eq!(plans.len(), 1, "block {b} repair");
        if b == 0 {
            first_esi_0 = Some(plans[0].symbols[0].payload_id);
        }
    }
    let fp = encoder_footprint_bytes(FecBackend::RaptorQ, &params(k, BULK_SYM, 0, 0));
    let (blocks, _) = arq.retained_stats();
    let (data, enc, drops) = arq.retained_breakdown();
    assert_eq!(blocks, 64, "all 64 Bulk blocks retained under repair");
    assert_eq!(data, 64 * (BULK_BLOCK + RETAIN_PER_BLOCK_OVERHEAD));
    assert!(data - 64 * RETAIN_PER_BLOCK_OVERHEAD >= RETAIN_MAX_BYTES, "data horizon >= 4 MiB");
    assert!(drops > 0, "encoders were dropped to fit the budget");
    assert!(
        data + enc <= RETAIN_BUDGET_BYTES + fp,
        "memory bound: budget + one protected encoder"
    );

    let plans = arq.plan_repairs(vec![ev(0)], 0.0);
    assert_eq!(plans.len(), 1, "oldest block's repair succeeds");
    let esi = plans[0].symbols[0].payload_id;
    assert!(esi > first_esi_0.unwrap(), "fresh repair index after rebuild");
    assert_eq!(arq.retained_stats().0, 64, "the rebuild evicted no block");
}

/// Evicted-block skips are counted; done blocks are not miscounted as
/// evictions for as long as the retention horizon can hold their peers.
#[test]
fn evicted_skips_counted_and_done_ring_spans_the_horizon() {
    let mut arq = BlockArq::with_caps(LEDGER_MAX_BATCHES, 1000 + RETAIN_PER_BLOCK_OVERHEAD);
    retain_block(&mut arq, 0, 1000, FecBackend::RaptorQ);
    retain_block(&mut arq, 1, 1000, FecBackend::RaptorQ); // evicts 0
    let ev = |b| LossEvent {
        block_id: b,
        path_id: 0,
        missing: vec![0],
    };
    assert!(arq.plan_repairs(vec![ev(0)], 0.0).is_empty());
    assert_eq!(arq.repair_skips_evicted(), 1);

    // Done ring: block 100 done, then DONE_RING_CAP - 1 later blocks done.
    // A late event for 100 is still recognised as done (not an eviction).
    let mut arq = BlockArq::new();
    for b in 100..100 + DONE_RING_CAP as u64 {
        arq.on_block_done(b);
    }
    assert!(arq.plan_repairs(vec![ev(100)], 0.0).is_empty());
    assert_eq!(arq.repair_skips_evicted(), 0, "done block must not count as evicted");
    assert!(DONE_RING_CAP >= RETAIN_MAX_BLOCKS_DERIVED);
}

/// The ledger cannot drop a batch whose block is still inside the byte
/// horizon: at the smallest profile symbol, single-symbol batches covering
/// RETAIN_MAX_BYTES fit in the ledger.
#[test]
fn ledger_horizon_covers_retention_horizon() {
    assert!(LEDGER_MAX_BATCHES >= RETAIN_MAX_BYTES / BLOCK_MIN_SYMBOL_SIZE);
    assert!(LEDGER_MAX_BATCHES >= 4096, "never below the historical cap");
}
