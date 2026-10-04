use super::*;
// Only the tests name a backend; the interleaver itself is codec-agnostic.
use crate::fec::FecBackend;

fn sym(block_id: u64, payload_id: u32) -> WireSymbol {
    WireSymbol {
        block_id,
        payload_id,
        is_repair: false,
        data: vec![0u8; 64].into(),
        backend: FecBackend::RaptorQ,
    }
}

#[test]
fn test_depth_1_passthrough() {
    let mut buf = InterleavingBuffer::new(1, Duration::from_secs(10));
    let assignments = vec![(0u32, vec![sym(0, 0), sym(0, 1), sym(0, 2)])];
    buf.push_block(0, assignments);

    // Depth 1 = immediate drain
    assert!(buf.should_drain());
    let result = buf.drain(0.0);
    assert_eq!(result.len(), 1);
    let (pid, syms) = &result[0];
    assert_eq!(*pid, 0);
    assert_eq!(syms.len(), 3);
    // Order preserved when only one block
    assert_eq!(syms[0].payload_id, 0);
    assert_eq!(syms[1].payload_id, 1);
    assert_eq!(syms[2].payload_id, 2);
    assert!(buf.is_empty());
}

#[test]
fn test_interleave_two_blocks() {
    let mut buf = InterleavingBuffer::new(2, Duration::from_secs(10));

    buf.push_block(0, vec![(0u32, vec![sym(0, 0), sym(0, 1), sym(0, 2)])]);
    assert!(!buf.should_drain()); // only 1 block

    buf.push_block(1, vec![(0u32, vec![sym(1, 10), sym(1, 11), sym(1, 12)])]);
    assert!(buf.should_drain()); // 2 blocks = depth reached

    let result = buf.drain(0.0);
    let (_, syms) = result.iter().find(|(pid, _)| *pid == 0).unwrap();
    // Should be interleaved: block0, block1, block0, block1, ...
    assert_eq!(syms.len(), 6);
    assert_eq!(syms[0].block_id, 0);
    assert_eq!(syms[1].block_id, 1);
    assert_eq!(syms[2].block_id, 0);
    assert_eq!(syms[3].block_id, 1);
    assert_eq!(syms[4].block_id, 0);
    assert_eq!(syms[5].block_id, 1);
}

#[test]
fn test_interleave_unequal_sizes() {
    let mut buf = InterleavingBuffer::new(2, Duration::from_secs(10));
    buf.push_block(0, vec![(0u32, vec![sym(0, 0), sym(0, 1), sym(0, 2), sym(0, 3), sym(0, 4)])]);
    buf.push_block(1, vec![(0u32, vec![sym(1, 10), sym(1, 11)])]);

    let result = buf.drain(0.0);
    let (_, syms) = result.iter().find(|(pid, _)| *pid == 0).unwrap();
    assert_eq!(syms.len(), 7);
    // First passes: alternating block0, block1
    assert_eq!(syms[0].block_id, 0);
    assert_eq!(syms[1].block_id, 1);
    assert_eq!(syms[2].block_id, 0);
    assert_eq!(syms[3].block_id, 1);
    // Block 1 exhausted, remaining block 0 symbols
    assert_eq!(syms[4].block_id, 0);
    assert_eq!(syms[5].block_id, 0);
    assert_eq!(syms[6].block_id, 0);
}

#[test]
fn test_drain_all_before_depth() {
    let mut buf = InterleavingBuffer::new(4, Duration::from_secs(10));
    buf.push_block(0, vec![(0u32, vec![sym(0, 0), sym(0, 1)])]);
    buf.push_block(1, vec![(0u32, vec![sym(1, 10)])]);

    // Only 2 blocks, depth is 4
    assert!(!buf.should_drain());

    // drain_all forces it
    let result = buf.drain_all(0.0);
    let (_, syms) = result.iter().find(|(pid, _)| *pid == 0).unwrap();
    assert_eq!(syms.len(), 3);
    assert!(buf.is_empty());
}

#[test]
fn test_time_flush() {
    let mut buf = InterleavingBuffer::new(4, Duration::from_millis(0));
    buf.push_block(0, vec![(0u32, vec![sym(0, 0)])]);

    // Timeout is 0ms, so should drain immediately
    assert!(buf.should_drain());
}

#[test]
fn test_max_buffered_triggers_drain() {
    let mut buf = InterleavingBuffer::new(10, Duration::from_secs(60));
    buf.max_buffered = 5;

    buf.push_block(0, vec![(0u32, vec![sym(0, 0), sym(0, 1), sym(0, 2)])]);
    assert!(!buf.should_drain()); // 3 < 5

    buf.push_block(1, vec![(0u32, vec![sym(1, 10), sym(1, 11), sym(1, 12)])]);
    assert!(buf.should_drain()); // 6 >= 5
}

#[test]
fn test_multi_path_interleave() {
    let mut buf = InterleavingBuffer::new(2, Duration::from_secs(10));

    // Block 0: path 0 gets 2 symbols, path 1 gets 1 symbol
    buf.push_block(0, vec![
        (0u32, vec![sym(0, 0), sym(0, 1)]),
        (1u32, vec![sym(0, 100)]),
    ]);
    // Block 1: path 0 gets 1 symbol, path 1 gets 2 symbols
    buf.push_block(1, vec![
        (0u32, vec![sym(1, 10)]),
        (1u32, vec![sym(1, 110), sym(1, 111)]),
    ]);

    let result = buf.drain(0.0);

    let path0_syms: Vec<_> = result.iter()
        .filter(|(pid, _)| *pid == 0)
        .flat_map(|(_, s)| s.iter())
        .collect();
    assert_eq!(path0_syms.len(), 3);
    // Interleaved: block0, block1, block0
    assert_eq!(path0_syms[0].block_id, 0);
    assert_eq!(path0_syms[1].block_id, 1);
    assert_eq!(path0_syms[2].block_id, 0);

    let path1_syms: Vec<_> = result.iter()
        .filter(|(pid, _)| *pid == 1)
        .flat_map(|(_, s)| s.iter())
        .collect();
    assert_eq!(path1_syms.len(), 3);
    // Interleaved: block0, block1, block1
    assert_eq!(path1_syms[0].block_id, 0);
    assert_eq!(path1_syms[1].block_id, 1);
    assert_eq!(path1_syms[2].block_id, 1);

    assert!(buf.is_empty());
}

#[test]
fn test_empty_drain_returns_empty() {
    let mut buf = InterleavingBuffer::new(2, Duration::from_secs(10));
    assert!(!buf.should_drain());
    assert!(buf.drain(0.0).is_empty());
}

fn repair_sym(block_id: u64, payload_id: u32) -> WireSymbol {
    WireSymbol {
        block_id,
        payload_id,
        is_repair: true,
        data: vec![0u8; 64].into(),
        backend: FecBackend::RaptorQ,
    }
}

#[test]
fn test_taper_schedule_basic() {
    // 4 repairs across 8 sources at 0% loss (steep decay)
    let schedule = compute_taper_schedule(4, 8, 0.0);
    assert_eq!(schedule.iter().sum::<usize>(), 4);
    // Front-loaded: first positions should have more repairs
    assert!(schedule[0] >= schedule[7]);
    // At steep decay, most repairs should be in the first half
    let first_half: usize = schedule[..4].iter().sum();
    assert!(first_half >= 3, "first half should get most repairs: {:?}", schedule);
}

#[test]
fn test_taper_schedule_high_loss() {
    // 6 repairs across 6 sources at 12% loss (gentle decay)
    let schedule = compute_taper_schedule(6, 6, 0.12);
    assert_eq!(schedule.iter().sum::<usize>(), 6);
    // With high loss, distribution should be more spread out than low loss
    let high_loss_second_half: usize = schedule[3..].iter().sum();
    let low_loss_schedule = compute_taper_schedule(6, 6, 0.0);
    let low_loss_second_half: usize = low_loss_schedule[3..].iter().sum();
    assert!(high_loss_second_half >= low_loss_second_half,
        "high loss should spread more: high={:?} low={:?}", schedule, low_loss_schedule);
}

#[test]
fn test_taper_schedule_edge_cases() {
    assert_eq!(compute_taper_schedule(0, 5, 0.0), vec![0; 5]);
    assert_eq!(compute_taper_schedule(5, 0, 0.0), Vec::<usize>::new());
    // 1 repair, 1 source
    assert_eq!(compute_taper_schedule(1, 1, 0.0), vec![1]);
}

#[test]
fn test_tapered_two_blocks() {
    let mut buf = InterleavingBuffer::new_tapered(2, Duration::from_secs(10));

    // Block 0: 3 sources + 2 repairs on path 0
    buf.push_block(0, vec![(0u32, vec![
        sym(0, 0), sym(0, 1), sym(0, 2),
        repair_sym(0, 100), repair_sym(0, 101),
    ])]);
    assert!(!buf.should_drain());

    // Block 1: 4 sources + 1 repair on path 0
    buf.push_block(1, vec![(0u32, vec![
        sym(1, 10), sym(1, 11), sym(1, 12), sym(1, 13),
        repair_sym(1, 110),
    ])]);
    assert!(buf.should_drain());

    let result = buf.drain(0.05);
    let (_, syms) = result.iter().find(|(pid, _)| *pid == 0).unwrap();

    // Block 0's sources come first (3 sources)
    // Then block 1's sources (4) interleaved with block 0's repairs (2)
    // Block 1's repairs (1) become pending

    // Count: 3 block0 sources + 4 block1 sources + 2 block0 repairs = 9
    assert_eq!(syms.len(), 9, "got {:?}", syms.iter().map(|s| (s.block_id, s.payload_id, s.is_repair)).collect::<Vec<_>>());

    // Block 0's repairs should appear interleaved among block 1's sources
    let block0_repair_positions: Vec<usize> = syms.iter()
        .enumerate()
        .filter(|(_, s)| s.block_id == 0 && s.is_repair)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(block0_repair_positions.len(), 2);

    // Repairs should be front-loaded (appear early in the block 1 source section)
    // Block 0 sources are positions 0..3, so repairs should be in positions 3..9
    for &pos in &block0_repair_positions {
        assert!(pos >= 3, "block 0 repairs should come after block 0 sources");
    }

    // Block 1's repair is pending (not emitted yet)
    assert!(!buf.is_empty(), "block 1 repair should be pending");

    // Flush pending
    let remaining = buf.drain_all(0.05);
    if !remaining.is_empty() {
        let (_, rem_syms) = remaining.iter().find(|(pid, _)| *pid == 0).unwrap();
        // Should be block 1's repair
        assert_eq!(rem_syms.len(), 1);
        assert!(rem_syms[0].is_repair);
        assert_eq!(rem_syms[0].block_id, 1);
    }
    assert!(buf.is_empty());
}

#[test]
fn test_tapered_front_loading() {
    // Verify repairs are front-loaded: with low loss, repairs should cluster
    // near the beginning of the source stream
    let mut buf = InterleavingBuffer::new_tapered(2, Duration::from_secs(10));

    // Block 0: 10 sources + 6 repairs
    let mut block0_syms: Vec<WireSymbol> = (0..10).map(|i| sym(0, i)).collect();
    block0_syms.extend((0..6).map(|i| repair_sym(0, 100 + i)));
    buf.push_block(0, vec![(0u32, block0_syms)]);

    // Block 1: 10 sources (no repairs)
    let block1_syms: Vec<WireSymbol> = (0..10).map(|i| sym(1, i)).collect();
    buf.push_block(1, vec![(0u32, block1_syms)]);

    let result = buf.drain(0.0); // 0% loss = steepest decay
    let (_, syms) = result.iter().find(|(pid, _)| *pid == 0).unwrap();

    // All 26 symbols should be present (10 src0 + 10 src1 + 6 rep0)
    assert_eq!(syms.len(), 26);

    // Find positions of block 0 repairs within the block 1 source section
    // Block 0 sources are first 10, then block 1 sources + interleaved repairs
    let repair_positions: Vec<usize> = syms.iter()
        .enumerate()
        .filter(|(_, s)| s.is_repair)
        .map(|(i, _)| i)
        .collect();

    // Average position of repairs should be closer to the start of the
    // block 1 section (position 10) than the end (position 25)
    let avg_pos: f64 = repair_positions.iter().map(|&p| p as f64).sum::<f64>()
        / repair_positions.len() as f64;
    let midpoint = 10.0 + (26.0 - 10.0) / 2.0; // 18.0
    assert!(avg_pos < midpoint,
        "repairs should be front-loaded: avg pos {avg_pos} should be < midpoint {midpoint}, positions: {repair_positions:?}");
}

#[test]
fn test_tapered_high_vs_low_loss_spread() {
    // High loss should spread repairs more evenly than low loss
    fn drain_and_get_repair_positions(loss: f64) -> Vec<usize> {
        let mut buf = InterleavingBuffer::new_tapered(2, Duration::from_secs(10));
        let mut block0: Vec<WireSymbol> = (0..8).map(|i| sym(0, i)).collect();
        block0.extend((0..4).map(|i| repair_sym(0, 100 + i)));
        buf.push_block(0, vec![(0u32, block0)]);
        let block1: Vec<WireSymbol> = (0..8).map(|i| sym(1, i)).collect();
        buf.push_block(1, vec![(0u32, block1)]);

        let result = buf.drain(loss);
        let (_, syms) = result.iter().find(|(pid, _)| *pid == 0).unwrap();
        syms.iter()
            .enumerate()
            .filter(|(_, s)| s.is_repair)
            .map(|(i, _)| i)
            .collect()
    }

    let low_loss_positions = drain_and_get_repair_positions(0.0);
    let high_loss_positions = drain_and_get_repair_positions(0.15);

    // Both should have 4 repairs
    assert_eq!(low_loss_positions.len(), 4);
    assert_eq!(high_loss_positions.len(), 4);

    // High loss repairs should have higher average position (more spread)
    let low_avg: f64 = low_loss_positions.iter().map(|&p| p as f64).sum::<f64>() / 4.0;
    let high_avg: f64 = high_loss_positions.iter().map(|&p| p as f64).sum::<f64>() / 4.0;
    assert!(high_avg >= low_avg,
        "high loss should spread repairs more: low_avg={low_avg}, high_avg={high_avg}");
}
