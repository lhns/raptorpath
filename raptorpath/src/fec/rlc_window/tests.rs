use super::*;

#[test]
fn test_window_encode_decode_no_loss() {
    let symbol_size = 64u16;
    let mut encoder = RlcWindowEncoder::new(symbol_size);
    let mut decoder = RlcWindowDecoder::new(symbol_size);

    let packets: Vec<Vec<u8>> = (0..10)
        .map(|i| vec![i as u8; 32])
        .collect();

    for pkt in &packets {
        let sym = encoder.add_source(pkt);
        let recovered = decoder.add_symbol(&sym);
        assert_eq!(recovered.len(), 1);
        assert_eq!(&recovered[0].1[..pkt.len()], pkt.as_slice());
    }

    assert_eq!(encoder.window_size(), 10);
    assert_eq!(encoder.window_span(), (0, 9));
}

#[test]
fn test_window_single_loss_recovery() {
    let symbol_size = 64u16;
    let mut encoder = RlcWindowEncoder::new(symbol_size);
    let mut decoder = RlcWindowDecoder::new(symbol_size);

    // Add 5 source symbols, drop symbol 2
    let mut source_syms = Vec::new();
    for i in 0..5u8 {
        let pkt = vec![i + 1; 32];
        let sym = encoder.add_source(&pkt);
        source_syms.push(sym);
    }

    // Feed all except symbol 2
    for (i, sym) in source_syms.iter().enumerate() {
        if i == 2 {
            continue;
        }
        decoder.add_symbol(sym);
    }

    // Generate a repair symbol and feed it
    let repair = encoder.generate_repair();
    let recovered = decoder.add_symbol(&repair);

    // Symbol 2 should be recovered
    assert!(
        recovered.iter().any(|(seq, _)| *seq == 2),
        "Expected to recover seq 2, got: {:?}",
        recovered.iter().map(|(s, _)| s).collect::<Vec<_>>()
    );

    // Verify the recovered data
    let (_, data) = recovered.iter().find(|(seq, _)| *seq == 2).unwrap();
    assert_eq!(&data[..32], &[3u8; 32]);
}

#[test]
fn test_window_multiple_loss_recovery() {
    let symbol_size = 64u16;
    let mut encoder = RlcWindowEncoder::new(symbol_size);
    let mut decoder = RlcWindowDecoder::new(symbol_size);

    // Add 10 source symbols, drop symbols 3 and 7
    let mut source_syms = Vec::new();
    for i in 0..10u8 {
        let pkt = vec![i + 10; 32];
        let sym = encoder.add_source(&pkt);
        source_syms.push(sym);
    }

    // Feed all except 3 and 7
    for (i, sym) in source_syms.iter().enumerate() {
        if i == 3 || i == 7 {
            continue;
        }
        decoder.add_symbol(sym);
    }

    // Generate 2 repair symbols (one for each lost source)
    let repair1 = encoder.generate_repair();
    let repair2 = encoder.generate_repair();

    let mut all_recovered = Vec::new();
    let r1 = decoder.add_symbol(&repair1);
    all_recovered.extend(r1);
    let r2 = decoder.add_symbol(&repair2);
    all_recovered.extend(r2);

    let recovered_seqs: BTreeSet<u64> =
        all_recovered.iter().map(|(seq, _)| *seq).collect();
    assert!(
        recovered_seqs.contains(&3),
        "Expected to recover seq 3"
    );
    assert!(
        recovered_seqs.contains(&7),
        "Expected to recover seq 7"
    );
}

#[test]
fn test_window_advance() {
    let symbol_size = 64u16;
    let mut encoder = RlcWindowEncoder::new(symbol_size);

    for i in 0..10u8 {
        encoder.add_source(&vec![i; 32]);
    }

    assert_eq!(encoder.window_size(), 10);
    assert_eq!(encoder.window_span(), (0, 9));

    encoder.advance(5);
    assert_eq!(encoder.window_size(), 5);
    assert_eq!(encoder.window_span(), (5, 9));
}

#[test]
fn test_window_decoder_advance() {
    let symbol_size = 64u16;
    let mut encoder = RlcWindowEncoder::new(symbol_size);
    let mut decoder = RlcWindowDecoder::new(symbol_size);

    for i in 0..10u8 {
        let sym = encoder.add_source(&vec![i; 32]);
        decoder.add_symbol(&sym);
    }

    assert_eq!(decoder.total_fed(), 10);

    decoder.advance(5);
    // Old recovered symbols should be cleaned up
    assert!(decoder.recovered.get(&0).is_none());
    assert!(decoder.recovered.get(&4).is_none());
    assert!(decoder.recovered.get(&5).is_some());
}

#[test]
fn test_window_deduplication() {
    let symbol_size = 64u16;
    let mut encoder = RlcWindowEncoder::new(symbol_size);
    let mut decoder = RlcWindowDecoder::new(symbol_size);

    let sym = encoder.add_source(&vec![42; 32]);

    let r1 = decoder.add_symbol(&sym);
    assert_eq!(r1.len(), 1);

    let r2 = decoder.add_symbol(&sym);
    assert_eq!(r2.len(), 0, "Duplicate should be ignored");

    assert_eq!(decoder.total_fed(), 1);
}

#[test]
fn test_window_repair_only_recovery() {
    let symbol_size = 64u16;
    let mut encoder = RlcWindowEncoder::new(symbol_size);
    let mut decoder = RlcWindowDecoder::new(symbol_size);

    // Add 3 sources, drop all of them
    let packets: Vec<Vec<u8>> = (0..3).map(|i| vec![i as u8 + 1; 32]).collect();
    for pkt in &packets {
        encoder.add_source(pkt);
    }

    // Generate 3 repair symbols (enough to recover 3 sources)
    let mut all_recovered = Vec::new();
    for _ in 0..3 {
        let repair = encoder.generate_repair();
        let r = decoder.add_symbol(&repair);
        all_recovered.extend(r);
    }

    let recovered_seqs: BTreeSet<u64> =
        all_recovered.iter().map(|(seq, _)| *seq).collect();
    assert_eq!(
        recovered_seqs.len(),
        3,
        "Should recover all 3 sources from 3 repairs"
    );
    assert!(recovered_seqs.contains(&0));
    assert!(recovered_seqs.contains(&1));
    assert!(recovered_seqs.contains(&2));

    // Verify data integrity
    for (seq, data) in &all_recovered {
        let expected = vec![*seq as u8 + 1; 32];
        assert_eq!(
            &data[..32],
            expected.as_slice(),
            "Data mismatch for seq {seq}"
        );
    }
}

#[test]
fn test_window_cascade_recovery() {
    let symbol_size = 64u16;
    let mut encoder = RlcWindowEncoder::new(symbol_size);
    let mut decoder = RlcWindowDecoder::new(symbol_size);

    // 5 sources, drop 1 and 3
    let mut syms = Vec::new();
    for i in 0..5u8 {
        syms.push(encoder.add_source(&vec![i + 100; 32]));
    }

    // Feed sources 0, 2, 4
    decoder.add_symbol(&syms[0]);
    decoder.add_symbol(&syms[2]);
    decoder.add_symbol(&syms[4]);

    // Generate 2 repairs, feed both
    // The second repair might cascade off the first
    let r1 = encoder.generate_repair();
    let r2 = encoder.generate_repair();

    let mut total_recovered = Vec::new();
    total_recovered.extend(decoder.add_symbol(&r1));
    total_recovered.extend(decoder.add_symbol(&r2));

    let recovered_seqs: BTreeSet<u64> =
        total_recovered.iter().map(|(seq, _)| *seq).collect();
    assert!(
        recovered_seqs.contains(&1) && recovered_seqs.contains(&3),
        "Should recover seqs 1 and 3 via cascade, got: {:?}",
        recovered_seqs
    );
}

#[test]
fn test_frontier_range_repair_decodes_hole_no_retransmit() {
    // Proactive-frontier mechanism (isolation): a repair coded over a small
    // TRAILING window whose members are ALL already received EXCEPT the hole
    // must decode that hole IMMEDIATELY from the single repair — no source
    // retransmit. (At L1 this does not lift throughput; see goal-gate
    // "Proactive Frontier" — but the coding/decoding mechanism is correct.)
    let symbol_size = 64u16;
    let mut encoder = RlcWindowEncoder::new(symbol_size);
    let mut decoder = RlcWindowDecoder::new(symbol_size);

    // 40 sources; the hole is seq 20 (an OLD, mid-stream position).
    let mut syms = Vec::new();
    for i in 0..40u64 {
        let pkt = vec![(i as u8).wrapping_add(1); 40];
        syms.push(encoder.add_source(&pkt));
    }
    // Feed every source EXCEPT the hole (seq 20). All neighbours received.
    for (i, sym) in syms.iter().enumerate() {
        if i as u64 != 20 {
            decoder.add_symbol(sym);
        }
    }
    assert!(
        decoder.frontier_probe(20, 39).0 >= 1,
        "seq 20 should register as a hole before repair"
    );

    // ONE frontier repair over a trailing window [16, 16+8) that contains
    // the hole and 7 already-received members. It must isolate + solve 20.
    let repair = encoder
        .generate_repair_range(16, 8)
        .expect("range fully retained");
    assert!(repair.is_repair);
    let recovered = decoder.add_symbol(&repair);

    assert!(
        recovered.iter().any(|(seq, _)| *seq == 20),
        "one trailing-window repair must decode the hole, got: {:?}",
        recovered.iter().map(|(s, _)| s).collect::<Vec<_>>()
    );
    let (_, data) = recovered.iter().find(|(seq, _)| *seq == 20).unwrap();
    assert_eq!(&data[..40], vec![21u8; 40].as_slice());
    // No hole remains in the window ⇒ proactive decode advanced it fully.
    assert_eq!(decoder.frontier_probe(16, 39).0, 0);
}

#[test]
fn test_frontier_range_repair_rejects_unretained_range() {
    let symbol_size = 64u16;
    let mut encoder = RlcWindowEncoder::new(symbol_size);
    for i in 0..10u8 {
        encoder.add_source(&vec![i; 32]);
    }
    encoder.advance(5); // window now [5,9]
    // Range starting below the retained window is inconsistent ⇒ None.
    assert!(encoder.generate_repair_range(2, 4).is_none());
    // Range extending past the newest retained seq ⇒ None.
    assert!(encoder.generate_repair_range(8, 5).is_none());
    // Fully-retained range ⇒ Some.
    assert!(encoder.generate_repair_range(6, 3).is_some());
}

#[test]
fn test_get_source_retrieves_correct_data() {
    let symbol_size = 64u16;
    let mut encoder = RlcWindowEncoder::new(symbol_size);

    for i in 0..5u8 {
        encoder.add_source(&vec![i + 10; 32]);
    }

    // Retrieve each source by seq
    for seq in 0..5u64 {
        let sym = encoder.get_source(seq).expect("should find source");
        assert_eq!(sym.block_id, seq);
        assert!(!sym.is_repair);
        assert_eq!(sym.backend, FecBackend::Rlc);
        assert_eq!(sym.data[0], (seq as u8) + 10);
    }

    // Out of range returns None
    assert!(encoder.get_source(5).is_none());
    assert!(encoder.get_source(100).is_none());
}

#[test]
fn test_get_source_returns_none_after_advance() {
    let symbol_size = 64u16;
    let mut encoder = RlcWindowEncoder::new(symbol_size);

    for i in 0..10u8 {
        encoder.add_source(&vec![i; 32]);
    }

    encoder.advance(5);

    // Evicted sequences return None
    for seq in 0..5u64 {
        assert!(encoder.get_source(seq).is_none(), "seq {seq} should be evicted");
    }
    // Remaining sequences still available
    for seq in 5..10u64 {
        assert!(encoder.get_source(seq).is_some(), "seq {seq} should still be available");
    }
}
