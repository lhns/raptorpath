use super::*;
use crate::fec::rlc_window::RlcWindowDecoder;
use crate::fec::window_traits::WindowDecoder;
use std::collections::BTreeSet;

fn payload(seq: u64) -> Vec<u8> {
    // Distinct, seq-dependent content so recovery is verifiable.
    (0..48).map(|j| (seq as u8).wrapping_mul(3).wrapping_add(j as u8)).collect()
}

/// The core claim: coded symbols over stable generations, with a fraction
/// DROPPED, still let the standard RLC window decoder recover every source
/// — decoding each generation independently once it has K_G symbols, and
/// out of order.
#[test]
fn generations_decode_on_k_out_of_order_with_loss() {
    let symbol_size = 64u16;
    let g = 8usize; // small generation for the test
    let n_gen = 5u64;
    let k = n_gen * g as u64;
    // Pipeline ≥ n_gen so every generation is in the active coding set
    // without needing an external advance() (pipeline BOUNDING is asserted
    // separately in `pipeline_bounds_active_generations`).
    let m = n_gen as usize;

    let mut enc = GenerationEncoder::new(symbol_size, g, m, 0.25);
    let mut dec = RlcWindowDecoder::new(symbol_size);

    // Feed all sources.
    for seq in 0..k {
        let ws = enc.add_source(&payload(seq));
        assert_eq!(ws.block_id, seq);
    }

    // Emit coded symbols round-robin across generations. Emit K_G + 2 per
    // generation worth of coded symbols (overhead for the drop below).
    let per_gen = g as u64 + 2;
    let total_coded = per_gen * n_gen;
    let mut coded: Vec<WireSymbol> = (0..total_coded).map(|_| enc.generate_repair()).collect();

    // Every coded symbol must carry a STABLE anchor: window_start is a
    // multiple of gen_size, window_count ≤ gen_size.
    for c in &coded {
        let ws = u64::from_le_bytes(c.data[0..8].try_into().unwrap());
        let wc = u16::from_le_bytes(c.data[8..10].try_into().unwrap());
        assert_eq!(ws % g as u64, 0, "anchor must be generation-aligned");
        assert!(wc as usize <= g, "window_count ≤ gen_size");
    }

    // Drop 1 in 9 coded symbols (a lossy channel), then deliver the rest in
    // REVERSE order (stress out-of-order / cross-generation interleave).
    coded.retain({
        let mut i = 0u64;
        move |_| {
            i += 1;
            i % 9 != 0
        }
    });
    coded.reverse();

    let mut recovered: BTreeSet<u64> = BTreeSet::new();
    for c in &coded {
        for (seq, data) in dec.add_symbol(c) {
            assert_eq!(&data[..48], payload(seq).as_slice(), "byte-exact recovery");
            recovered.insert(seq);
        }
    }

    for seq in 0..k {
        assert!(recovered.contains(&seq), "seq {seq} not recovered");
    }
}

/// A generation must decode on its OWN K_G independent symbols
/// (decode-on-K), independent of any other generation.
#[test]
fn generation_is_independent_decode_unit() {
    let symbol_size = 64u16;
    let g = 6usize;
    let mut enc = GenerationEncoder::new(symbol_size, g, 4, 0.25);
    for seq in 0..(3 * g as u64) {
        enc.add_source(&payload(seq));
    }

    // Collect coded symbols grouped by generation.
    let mut by_gen: std::collections::HashMap<u64, Vec<WireSymbol>> = Default::default();
    for _ in 0..(3 * (g + 3) as u64) {
        let c = enc.generate_repair();
        let anchor = u64::from_le_bytes(c.data[0..8].try_into().unwrap());
        by_gen.entry(anchor / g as u64).or_default().push(c);
    }

    // Feed generation 2 fully but generation 0/1 NOT AT ALL: gen 2 must
    // decode on its own (out-of-order, no dependency on earlier gens).
    let mut dec = RlcWindowDecoder::new(symbol_size);
    let mut got: BTreeSet<u64> = BTreeSet::new();
    for c in by_gen.get(&2).unwrap().iter().take(g) {
        for (seq, _) in dec.add_symbol(c) {
            got.insert(seq);
        }
    }
    for seq in (2 * g as u64)..(3 * g as u64) {
        assert!(got.contains(&seq), "gen 2 seq {seq} should decode independently");
    }
    // No seq from gens 0/1 should have been produced.
    assert!(got.iter().all(|&s| s >= 2 * g as u64));
}

/// Pipeline depth M bounds how many generations are coded concurrently:
/// with M in flight and no advance, only the M oldest active generations
/// receive coded symbols; advancing past a completed generation rotates the
/// next one into the active set (the pipeline slides).
#[test]
fn pipeline_bounds_active_generations() {
    let symbol_size = 32u16;
    let g = 4usize;
    let m = 2usize;
    let mut enc = GenerationEncoder::new(symbol_size, g, m, 0.25);
    for seq in 0..(4 * g as u64) {
        enc.add_source(&payload(seq));
    }
    // Emit WITHOUT advancing (gated on wants_coding, as production does):
    // only generations {0,1} (the M oldest) may be coded.
    let mut coded_gens: BTreeSet<u64> = BTreeSet::new();
    for _ in 0..100 {
        if !enc.wants_coding() {
            break;
        }
        let c = enc.generate_repair();
        coded_gens.insert(u64::from_le_bytes(c.data[0..8].try_into().unwrap()) / g as u64);
    }
    assert_eq!(coded_gens, BTreeSet::from([0, 1]), "M=2 bounds the active set to gens 0,1");

    // Advance past generation 0 (it "completed"): the active window slides to
    // {1,2}, rotating gen 2 into the pipeline while gen 3 stays excluded.
    // Gen 1 was already PROACTIVELY provisioned to its budget in the first
    // phase, so proactive coding no longer re-emits it (any residual for gen
    // 1 now comes from the DEFICIT loop via `generate_repair_for`, not the
    // proactive round-robin). So the newly-emitted proactive set is exactly
    // {2}: gen 2 rotated in, gen 3 (beyond M) still excluded.
    enc.advance(g as u64);
    let mut after: BTreeSet<u64> = BTreeSet::new();
    for _ in 0..100 {
        if !enc.wants_coding() {
            break;
        }
        let c = enc.generate_repair();
        after.insert(u64::from_le_bytes(c.data[0..8].try_into().unwrap()) / g as u64);
    }
    assert_eq!(after, BTreeSet::from([2]), "gen 2 rotated into the pipeline; gen 3 excluded");
    assert!(!after.contains(&3), "gen 3 is beyond pipeline depth M");
}

/// feat/gen-substrate-ceiling: `set_pipeline_depth` (the derived M*, #61's
/// dynamic advance quantized to generations) WIDENS the proactive
/// round-robin span at runtime — deepening from M=2 to M=4 makes gens
/// {2,3} (previously beyond the pipeline) proactively codeable, and
/// narrowing back restores the original bound. Retention is untouched.
#[test]
fn set_pipeline_depth_widens_the_proactive_span() {
    let symbol_size = 32u16;
    let g = 4usize;
    let mut enc = GenerationEncoder::new(symbol_size, g, 2, 0.25);
    for seq in 0..(4 * g as u64) {
        enc.add_source(&payload(seq));
    }
    // M=2: gens {0,1} only (as pipeline_bounds_active_generations proves).
    let mut coded: BTreeSet<u64> = BTreeSet::new();
    for _ in 0..100 {
        if !enc.wants_coding() {
            break;
        }
        let c = enc.generate_repair();
        coded.insert(u64::from_le_bytes(c.data[0..8].try_into().unwrap()) / g as u64);
    }
    assert_eq!(coded, BTreeSet::from([0, 1]));
    // Deepen to M*=4: gens {2,3} become proactively codeable (0,1 are at
    // budget already), covering the whole retained span.
    enc.set_pipeline_depth(4);
    let mut deeper: BTreeSet<u64> = BTreeSet::new();
    for _ in 0..200 {
        if !enc.wants_coding() {
            break;
        }
        let c = enc.generate_repair();
        deeper.insert(u64::from_le_bytes(c.data[0..8].try_into().unwrap()) / g as u64);
    }
    assert_eq!(deeper, BTreeSet::from([2, 3]), "M*=4 rotates gens 2,3 into the pipeline");
    // Retention unchanged: every source is still retained.
    assert_eq!(enc.window_size(), 4 * g);
}

/// The deficit-driven recovery path (`generate_repair_for`) emits coded
/// symbols for a SPECIFIC sealed generation BEYOND its proactive budget, and
/// those extra coded symbols still let the decoder finish that generation —
/// the sender arm of per-generation deficit feedback (§16.3).
#[test]
fn generate_repair_for_recovers_beyond_budget() {
    let symbol_size = 64u16;
    let g = 8usize;
    let mut enc = GenerationEncoder::new(symbol_size, g, 2, 0.0);
    for seq in 0..(2 * g as u64) {
        enc.add_source(&payload(seq));
    }
    // Anchor of generation 1.
    let anchor = g as u64;
    // With overhead r=0, the proactive budget for a sealed generation is
    // exactly K_G, so proactive alone leaves NO slack for loss. Drop 2 of
    // generation 1's proactive coded symbols, then top up via the deficit
    // path — the recovery symbols must complete the generation.
    let mut dec = RlcWindowDecoder::new(symbol_size);
    // Proactive coded for gen 1 (budget = K_G = 8); deliver only 6 (drop 2).
    let mut proactive: Vec<WireSymbol> = Vec::new();
    for _ in 0..(4 * g) {
        let c = enc.generate_repair();
        let a = u64::from_le_bytes(c.data[0..8].try_into().unwrap());
        if a == anchor {
            proactive.push(c);
        }
    }
    for c in proactive.iter().take(g - 2) {
        dec.add_symbol(c);
    }
    // Generation 1 is short by ≥2 → not yet decoded.
    assert!(dec.rank_in(anchor, g as u64) < g as u64);
    // Deficit loop: emit 2 MORE coded for generation 1 via the anchor path.
    for _ in 0..2 {
        let c = enc
            .generate_repair_for(anchor)
            .expect("sealed generation must be codeable for recovery");
        assert_eq!(u64::from_le_bytes(c.data[0..8].try_into().unwrap()), anchor);
        dec.add_symbol(&c);
    }
    assert_eq!(
        dec.rank_in(anchor, g as u64),
        g as u64,
        "deficit-driven recovery completed the generation"
    );
}

// -----------------------------------------------------------------------
// SYSTEMATIC + deficit-repair submode (§16.3 oracle). Source rides the wire
// as primary; the encoder emits only the ceil(len·r) repair overhead, and
// the dense decoder solves ONLY the holes (deficit), not the whole generation.
// -----------------------------------------------------------------------

/// The systematic encoder's PROACTIVE budget is the loss-FEC overhead ONLY
/// (`ceil(len·r)`), not coded-only's `ceil(len·(1+r))` — because the K base
/// degrees of freedom ride the wire as raw source, so coded need only cover
/// the r overhead. This is the one-line difference that turns φ from ≈(1+r)
/// into ≈r.
#[test]
fn systematic_budget_is_repair_overhead_only() {
    let symbol_size = 32u16;
    let g = 16usize;
    let r = 0.5f64;

    // Count PROACTIVE coded emitted per sealed generation in each mode.
    let count_proactive = |enc: &mut GenerationEncoder| -> BTreeMap<u64, u32> {
        for seq in 0..(2 * g as u64) {
            enc.add_source(&payload(seq));
        }
        let mut per_gen: BTreeMap<u64, u32> = BTreeMap::new();
        for _ in 0..1000 {
            if !enc.wants_coding() {
                break;
            }
            let c = enc.generate_repair();
            *per_gen
                .entry(u64::from_le_bytes(c.data[0..8].try_into().unwrap()) / g as u64)
                .or_insert(0) += 1;
        }
        per_gen
    };

    let mut sys = GenerationEncoder::new_systematic(symbol_size, g, 2, r);
    let mut coded_only = GenerationEncoder::new(symbol_size, g, 2, r);
    let sys_counts = count_proactive(&mut sys);
    let co_counts = count_proactive(&mut coded_only);

    // Systematic: ceil(16·0.5) = 8 proactive coded per sealed generation.
    for (&gen, &n) in &sys_counts {
        assert_eq!(n, (g as f64 * r).ceil() as u32, "systematic gen {gen} = ceil(len·r)");
    }
    // Coded-only: ceil(16·1.5) = 24 — it must fund the K base + r.
    for (&gen, &n) in &co_counts {
        assert_eq!(n, (g as f64 * (1.0 + r)).ceil() as u32, "coded-only gen {gen} = ceil(len·(1+r))");
    }
    assert!(
        sys_counts.values().sum::<u32>() < co_counts.values().sum::<u32>(),
        "systematic emits strictly less coded than coded-only"
    );
}

/// End-to-end proof of the systematic+repair design's four claims over a
/// lossy stream, against the DENSE decoder:
///   (1) a received source is delivered DIRECTLY (zero decode) — the raw
///       symbol on the wire, placed on arrival;
///   (2) windowed REPAIR (coded over the fixed generation) recovers the
///       lost source (holes), fungibly — any coded for the generation works;
///   (3) the DEFICIT-DECODE size == the number of holes, which is ≪ G (the
///       dense solve is O(deficit²), not O(G²)); the known sources pre-load
///       as unit pivots so a generation needs exactly `holes` coded to finish;
///   (4) recovery is fungible repair — NO per-seq retransmit of a specific
///       source symbol is ever used.
#[test]
fn systematic_source_primary_repair_recovers_deficit_only() {
    let symbol_size = 64u16;
    let g = 32usize;
    let n_gen = 4u64;
    let k = n_gen * g as u64;
    let mut enc = GenerationEncoder::new_systematic(symbol_size, g, n_gen as usize, 0.5);
    let mut dec = GenerationDecoder::new(symbol_size);

    // Fill the encoder (retains sources for repair coding) and capture each
    // raw systematic source symbol — this is what rides the wire as PRIMARY.
    let sources: Vec<WireSymbol> = (0..k).map(|seq| enc.add_source(&payload(seq))).collect();
    for s in &sources {
        assert!(!s.is_repair, "primary is a RAW systematic source, not coded");
    }

    // Simulate loss: every 7th source is dropped on the wire (a hole). The
    // rest are delivered DIRECTLY — claim (1): the decoder returns the source
    // immediately, with ZERO decode (no matrix, no repair needed).
    let is_hole = |seq: u64| seq % 7 == 3;
    let mut delivered: BTreeSet<u64> = BTreeSet::new();
    for s in &sources {
        if is_hole(s.block_id) {
            continue; // lost on the wire
        }
        let out = dec.add_symbol(s);
        assert_eq!(out.len(), 1, "received source delivered directly (zero decode)");
        assert_eq!(out[0].0, s.block_id);
        assert_eq!(&out[0].1[..48], payload(s.block_id).as_slice(), "byte-exact source");
        delivered.insert(s.block_id);
    }

    // Now recover the holes with WINDOWED REPAIR ONLY (claims 2–4). For each
    // generation, drive the deficit loop exactly as production does: while the
    // decoder's independent rank over the generation span is < K_G, emit ONE
    // more coded symbol for that generation (fungible — over the whole span,
    // not a specific seq) and feed it. Count how many coded each generation
    // actually consumes to finish.
    for gen in 0..n_gen {
        let anchor = gen * g as u64;
        let holes: u64 = (0..g as u64).filter(|&i| is_hole(anchor + i)).count() as u64;
        assert!(holes > 0, "test needs at least one hole per generation to be meaningful");
        let mut coded_used = 0u64;
        while dec.rank_in(anchor, g as u64) < g as u64 {
            let c = enc
                .generate_repair_for(anchor)
                .expect("sealed generation must be codeable for repair");
            assert_eq!(u64::from_le_bytes(c.data[0..8].try_into().unwrap()), anchor);
            assert!(c.is_repair, "repair is a coded combination, not a source resend");
            for (seq, data) in dec.add_symbol(&c) {
                assert_eq!(&data[..48], payload(seq).as_slice(), "byte-exact repair recovery");
                delivered.insert(seq);
            }
            coded_used += 1;
            assert!(coded_used <= g as u64, "runaway: a generation must finish in ≤ G coded");
        }
        // Claim (3): the deficit-decode consumed EXACTLY `holes` coded — the
        // known sources pre-loaded as unit pivots, so the dense solve is over
        // the holes only. holes ≪ G.
        assert_eq!(coded_used, holes, "gen {gen}: deficit-decode == holes, not G");
        assert!(holes < g as u64 / 2, "deficit ({holes}) must stay ≪ G ({g})");
    }

    // Every source recovered, byte-exact, with no per-seq ARQ anywhere.
    for seq in 0..k {
        assert!(delivered.contains(&seq), "seq {seq} not delivered");
    }
    assert_eq!(delivered.len() as u64, k);
}

/// SMALL-G FRONTIER-ADVANCE DEADLOCK regression (G=96). Reproduces the exact
/// wedge the `feat/c8-final` receiver-seeding fix targets: a FULL generation
/// whose ENTIRE proactive repair budget is lost on the wire. Before the fix
/// the receiver learned a generation's width ONLY from a repair header, so
/// such a generation never entered its deficit map — it reported ZERO deficit
/// while the in-order frontier wedged on its hole forever (MEASURED at G=96:
/// in_flight/src/cod all 0). The fix seeds the width (= G) from the PRIMARY
/// seqs of any provably-full generation, so the deficit is computable from the
/// primaries ALONE. This test asserts that invariant end to end against the
/// dense decoder:
///   (1) with NO repair seen, `rank_in(anchor, G)` == (G − holes) — the
///       deficit is computable from the delivered primaries alone (the
///       receiver-seeding branch);
///   (2) the deficit-driven `generate_repair_for` then completes the
///       generation in EXACTLY `holes` coded symbols (≪ G);
///   (3) every source is recovered byte-exact, no per-seq resend.
#[test]
fn small_g_generation_recovers_from_deficit_when_all_proactive_lost() {
    let symbol_size = 64u16;
    let g = 96usize; // the BDP-scale generation that wedged
    let n_gen = 3u64;
    let k = n_gen * g as u64;
    let mut enc = GenerationEncoder::new_systematic(symbol_size, g, n_gen as usize, 0.15);
    let mut dec = GenerationDecoder::new(symbol_size);

    // Fill the encoder and capture each raw systematic source (the primary).
    let sources: Vec<WireSymbol> = (0..k).map(|seq| enc.add_source(&payload(seq))).collect();

    // Deliver every primary EXCEPT the holes. Crucially, deliver NO repair —
    // model the wedge where the whole ceil(G·r) proactive budget was lost.
    let is_hole = |seq: u64| seq % 13 == 5;
    let mut delivered: BTreeSet<u64> = BTreeSet::new();
    for s in &sources {
        if is_hole(s.block_id) {
            continue;
        }
        for (seq, _) in dec.add_symbol(s) {
            delivered.insert(seq);
        }
    }

    for gen in 0..n_gen {
        let anchor = gen * g as u64;
        let holes: u64 = (0..g as u64).filter(|&i| is_hole(anchor + i)).count() as u64;
        assert!(holes > 0, "test needs a hole per generation");
        // Claim (1): the receiver can compute the deficit from primaries alone,
        // with NO repair header ever seen for this generation. This is the
        // seeded-width path — `rank_in(anchor, G)` counts the recovered
        // primaries (G − holes), so the reported deficit == holes.
        assert_eq!(
            dec.rank_in(anchor, g as u64),
            g as u64 - holes,
            "gen {gen}: deficit computable from primaries with zero repairs seen",
        );
        // Claim (2)+(3): the sender funds exactly that deficit via the
        // generation-targeted recovery path; it completes in `holes` coded.
        let mut coded_used = 0u64;
        while dec.rank_in(anchor, g as u64) < g as u64 {
            let c = enc
                .generate_repair_for(anchor)
                .expect("full generation must be codeable for deficit recovery");
            assert!(c.is_repair, "recovery is coded repair, not a per-seq resend");
            for (seq, data) in dec.add_symbol(&c) {
                assert_eq!(&data[..48], payload(seq).as_slice(), "byte-exact recovery");
                delivered.insert(seq);
            }
            coded_used += 1;
            assert!(coded_used <= g as u64, "runaway recovery");
        }
        assert_eq!(coded_used, holes, "gen {gen}: recovered in exactly `holes` coded");
    }

    for seq in 0..k {
        assert!(delivered.contains(&seq), "seq {seq} not delivered");
    }
}

#[test]
fn advance_drops_whole_generations_only() {
    let symbol_size = 32u16;
    let g = 10usize;
    let mut enc = GenerationEncoder::new(symbol_size, g, 2, 0.25);
    for seq in 0..25u64 {
        enc.add_source(&payload(seq));
    }
    assert_eq!(enc.window_size(), 25);
    // Advance to a NON-aligned seq inside generation 1: must drop only the
    // fully-completed generation 0 (seqs 0..10), keeping gen 1 intact.
    enc.advance(13);
    assert_eq!(enc.window_span().0, 10, "gen 0 dropped, gen 1 start retained");
    assert_eq!(enc.base_gen, 1);
}

/// Fix 3 (transport-substrate): `set_code_base` moves the PROACTIVE coding
/// window to follow the SEND frontier, decoupled from the retention floor,
/// so a stalled in-order-frontier generation is left to reactive recovery
/// while fresh generations get their upfront proactive budget — the change
/// that breaks the ∝1/RTT serialization. Reliability is preserved: the
/// stalled generation stays retained and reactively codeable.
#[test]
fn set_code_base_moves_proactive_window_past_stalled_generation() {
    let symbol_size = 32u16;
    let g = 10usize;
    let pipeline = 2usize;
    // overhead 1.0 so every generation always "wants coding" (budget large).
    let mut enc = GenerationEncoder::new_systematic(symbol_size, g, pipeline, 1.0);
    for seq in 0..60u64 {
        enc.add_source(&payload(seq)); // 6 sealed generations (0..6)
    }
    let anchor_gen = |s: &WireSymbol| {
        u64::from_le_bytes(s.data[0..8].try_into().unwrap()) / g as u64
    };

    // DEFAULT: coding anchored at the retention floor (base_gen = 0), so the
    // proactive round-robin covers only gens [0, pipeline).
    let a0 = anchor_gen(&enc.generate_repair());
    assert!(a0 < pipeline as u64, "default coding at base_gen window, got gen {a0}");

    // Fix 3: advance the coding floor toward the send frontier. Newest seq =
    // 59 (gen 5); anchor at newest − pipeline·G = 39 (gen 3).
    enc.set_code_base(59u64.saturating_sub((pipeline * g) as u64));
    assert_eq!(enc.code_base, 3);
    for _ in 0..20 {
        let gg = anchor_gen(&enc.generate_repair());
        assert!(gg >= 3, "proactive coding must follow the frontier (>=gen 3), got {gg}");
    }

    // Reliability: the stalled generation 0 is STILL reactively codeable even
    // though proactive coding moved past it (its sources remain retained).
    let rec = enc.generate_repair_for(0).expect("stalled gen 0 still codeable");
    assert_eq!(anchor_gen(&rec), 0);

    // The coding floor never trails the retention floor.
    enc.advance(45); // retention floor → gen 4
    assert!(enc.code_base >= enc.base_gen, "code_base must not trail base_gen");
}

/// "Repair In-Flight" (goal-gate): interspersed trailing-window repair is
/// PRESENT when a hole is detected, so the hole decodes PROACTIVELY — no
/// reactive deficit round-trip. Mirrors the sender's inline emission: the
/// source rides the wire raw (all but one hole delivered), and a repair coded
/// over the trailing block `[anchor, anchor+W)` via `generate_repair_range`
/// arrives right after → the covering equation is BUFFERED at the stall
/// (`frontier_probe` buffered > 0) and the single repair completes the hole.
#[test]
fn interspersed_block_repair_present_at_hole_decodes_proactively() {
    let symbol_size = 64u16;
    let g = 384usize; // production generation
    let w = 64u64; // inline trailing-block width
    let mut enc = GenerationEncoder::new_systematic(symbol_size, g, 2, 0.15);
    let mut dec = GenerationDecoder::new(symbol_size);

    // Fill one generation's worth of source; capture the raw systematic
    // symbols (what rides the wire as primary).
    let sources: Vec<WireSymbol> = (0..g as u64).map(|seq| enc.add_source(&payload(seq))).collect();

    // The hole: one source lost inside the first trailing block [0, W).
    let hole = 21u64;
    assert!(hole < w);

    // Deliver every source of the block EXCEPT the hole (raw, zero decode).
    let mut delivered: BTreeSet<u64> = BTreeSet::new();
    for s in sources.iter().take(w as usize) {
        if s.block_id == hole {
            continue; // lost on the wire
        }
        for (seq, _) in dec.add_symbol(s) {
            delivered.insert(seq);
        }
    }
    assert!(!delivered.contains(&hole), "hole not yet recovered");

    // The interspersed repair covering the block arrives. BEFORE feeding it,
    // the block matrix does not yet exist (no repair seen) so nothing is
    // buffered; feeding the FIRST repair creates the (0,W) matrix, injects the
    // W−1 already-received sources as unit pivots, and — being the single
    // missing DoF — completes the hole PROACTIVELY on arrival.
    let repair = enc
        .generate_repair_range(0, w as u16)
        .expect("trailing block fully retained");
    assert!(repair.is_repair);
    assert_eq!(u64::from_le_bytes(repair.data[0..8].try_into().unwrap()), 0);
    assert_eq!(u16::from_le_bytes(repair.data[8..10].try_into().unwrap()) as u64, w);

    let out = dec.add_symbol(&repair);
    let recovered: BTreeSet<u64> = out.iter().map(|(s, _)| *s).collect();
    assert!(recovered.contains(&hole), "hole decoded PROACTIVELY from the block repair");
    for (seq, data) in &out {
        assert_eq!(&data[..48], payload(*seq).as_slice(), "byte-exact proactive recovery");
    }

    // The recovery was proactive: exactly ONE coded symbol (== the one hole),
    // NOT a whole-generation re-solve and NOT a per-seq source retransmit.
    assert_eq!(recovered.len(), 1, "one repair recovered exactly the one hole");

    // And it needed no reactive round-trip: the block's deficit is now zero,
    // so the sender's deficit loop would emit nothing for it.
    assert_eq!(dec.rank_in(0, w), w, "block fully solved — no residual deficit");
}

/// `frontier_probe` on the dense decoder reports a BUFFERED coded equation
/// covering the frontier hole (the `present_at_stall` signal): with the block
/// matrix short by two holes and only one repair fed, the decoder holds one
/// independent DoF whose pivot lies at a hole column — buffered == 1 — so the
/// receiver knows proactive repair is present and in progress (no ARQ needed
/// yet). This is the metric the L1 harness reads as `present_at_stall`.
#[test]
fn frontier_probe_reports_buffered_proactive_equation() {
    let symbol_size = 64u16;
    let g = 384usize;
    let w = 64u64;
    let mut enc = GenerationEncoder::new_systematic(symbol_size, g, 2, 0.15);
    let mut dec = GenerationDecoder::new(symbol_size);

    let sources: Vec<WireSymbol> = (0..g as u64).map(|seq| enc.add_source(&payload(seq))).collect();
    // TWO holes in the block so one repair cannot finish it (stays "stuck",
    // holding a buffered equation exactly as at a real stall).
    let holes = [10u64, 40u64];
    for s in sources.iter().take(w as usize) {
        if holes.contains(&s.block_id) {
            continue;
        }
        dec.add_symbol(s);
    }
    // No repair yet: holes present, nothing buffered.
    let (h0, b0) = dec.frontier_probe(0, w - 1);
    assert_eq!(h0, 2, "two holes in the block");
    assert_eq!(b0, 0, "no coded equation buffered before any repair");

    // One block repair arrives — insufficient (two holes) so it is HELD as a
    // buffered DoF at a hole column, not yet solving.
    let r1 = enc.generate_repair_range(0, w as u16).expect("block retained");
    assert!(dec.add_symbol(&r1).is_empty(), "one repair cannot finish two holes");
    let (h1, b1) = dec.frontier_probe(0, w - 1);
    assert_eq!(h1, 2, "still two holes");
    assert_eq!(b1, 1, "one PROACTIVE equation buffered at a hole column (present_at_stall)");

    // The second repair completes both holes proactively.
    let r2 = enc.generate_repair_range(0, w as u16).expect("block retained");
    let out = dec.add_symbol(&r2);
    let rec: BTreeSet<u64> = out.iter().map(|(s, _)| *s).collect();
    assert!(rec.contains(&10) && rec.contains(&40), "both holes recovered proactively");
}

// -----------------------------------------------------------------------
// Dense GenerationDecoder — the fast decode path.
// -----------------------------------------------------------------------

/// Same core claim as `generations_decode_on_k_out_of_order_with_loss`, but
/// against the DENSE `GenerationDecoder`: coded symbols over stable
/// generations, with a fraction dropped and the rest delivered in reverse
/// order, still recover every source — each generation independently on K_G.
#[test]
fn gen_decoder_decode_on_k_out_of_order_with_loss() {
    let symbol_size = 64u16;
    let g = 8usize;
    let n_gen = 5u64;
    let k = n_gen * g as u64;
    let m = n_gen as usize;

    let mut enc = GenerationEncoder::new(symbol_size, g, m, 0.25);
    let mut dec = GenerationDecoder::new(symbol_size);

    for seq in 0..k {
        enc.add_source(&payload(seq));
    }

    let per_gen = g as u64 + 2;
    let total_coded = per_gen * n_gen;
    let mut coded: Vec<WireSymbol> = (0..total_coded).map(|_| enc.generate_repair()).collect();

    // Drop 1 in 9, deliver the rest reversed (out-of-order / interleaved).
    coded.retain({
        let mut i = 0u64;
        move |_| {
            i += 1;
            i % 9 != 0
        }
    });
    coded.reverse();

    let mut recovered: BTreeSet<u64> = BTreeSet::new();
    for c in &coded {
        for (seq, data) in dec.add_symbol(c) {
            assert_eq!(&data[..48], payload(seq).as_slice(), "byte-exact recovery");
            recovered.insert(seq);
        }
    }
    for seq in 0..k {
        assert!(recovered.contains(&seq), "seq {seq} not recovered");
    }
}

/// A generation decodes on its OWN K_G symbols, independent of others, and
/// `rank_in` tracks its independent rank (the deficit-feedback signal).
#[test]
fn gen_decoder_independent_and_rank_in() {
    let symbol_size = 64u16;
    let g = 6usize;
    let mut enc = GenerationEncoder::new(symbol_size, g, 4, 0.25);
    for seq in 0..(3 * g as u64) {
        enc.add_source(&payload(seq));
    }

    let mut by_gen: std::collections::HashMap<u64, Vec<WireSymbol>> = Default::default();
    for _ in 0..(3 * (g + 3) as u64) {
        let c = enc.generate_repair();
        let anchor = u64::from_le_bytes(c.data[0..8].try_into().unwrap());
        by_gen.entry(anchor).or_default().push(c);
    }

    let anchor2 = 2 * g as u64;
    let mut dec = GenerationDecoder::new(symbol_size);
    let syms = by_gen.get(&anchor2).unwrap();

    // Feed K_G-1: rank climbs but nothing delivers yet.
    let mut got: BTreeSet<u64> = BTreeSet::new();
    for c in syms.iter().take(g - 1) {
        for (seq, _) in dec.add_symbol(c) {
            got.insert(seq);
        }
    }
    assert!(got.is_empty(), "must not deliver before K_G");
    assert_eq!(dec.rank_in(anchor2, g as u64), g as u64 - 1);

    // The K_G-th independent symbol completes gen 2 (and only gen 2).
    for c in syms.iter().skip(g - 1).take(1) {
        for (seq, _) in dec.add_symbol(c) {
            got.insert(seq);
        }
    }
    assert_eq!(dec.rank_in(anchor2, g as u64), g as u64, "gen 2 full rank");
    for seq in (2 * g as u64)..(3 * g as u64) {
        assert!(got.contains(&seq), "gen 2 seq {seq} should decode independently");
    }
    assert!(got.iter().all(|&s| s >= 2 * g as u64), "no cross-generation leakage");
}

/// The dense decoder recovers EXACTLY the same sources as the reference
/// sparse `RlcWindowDecoder` from an identical lossy, reordered coded stream.
#[test]
fn gen_decoder_matches_rlc_window() {
    let symbol_size = 96u16;
    let g = 12usize;
    let n_gen = 4u64;
    let k = n_gen * g as u64;
    let mut enc = GenerationEncoder::new(symbol_size, g, n_gen as usize, 0.5);
    for seq in 0..k {
        enc.add_source(&payload(seq));
    }
    let mut coded: Vec<WireSymbol> =
        (0..(g as u64 + 4) * n_gen).map(|_| enc.generate_repair()).collect();
    coded.retain({
        let mut i = 0u64;
        move |_| {
            i += 1;
            i % 7 != 0
        }
    });
    coded.reverse();

    let mut dense = GenerationDecoder::new(symbol_size);
    let mut sparse = RlcWindowDecoder::new(symbol_size);
    let mut dense_out: BTreeMap<u64, Vec<u8>> = BTreeMap::new();
    let mut sparse_out: BTreeMap<u64, Vec<u8>> = BTreeMap::new();
    for c in &coded {
        for (seq, d) in dense.add_symbol(c) {
            dense_out.insert(seq, d.to_vec());
        }
        for (seq, d) in sparse.add_symbol(c) {
            sparse_out.insert(seq, d.to_vec());
        }
    }
    assert_eq!(dense_out.keys().collect::<Vec<_>>(), sparse_out.keys().collect::<Vec<_>>());
    for (seq, d) in &dense_out {
        assert_eq!(&d[..48], payload(*seq).as_slice(), "dense byte-exact seq {seq}");
        assert_eq!(d, sparse_out.get(seq).unwrap(), "dense == sparse seq {seq}");
    }
    assert_eq!(dense_out.len() as u64, k, "all sources recovered");
}

/// Microbenchmark: dense vs sparse generation decode throughput at
/// G ∈ {96,192,384,512}, 1200 B symbols. Run with:
///   cargo test -p raptorpath --release --lib \
///     fec::generation::tests::bench_generation_decode_throughput -- --ignored --nocapture
/// The dense decoder must clear the ~100 Mbit link rate with margin at G=384.
#[test]
#[ignore]
fn bench_generation_decode_throughput() {
    use std::time::Instant;
    let symbol_size = 1200u16;
    let ss = symbol_size as usize;
    // Enough generations that per-object setup is amortized.
    println!("\n  generation decode throughput (1200 B symbols, single core)");
    println!(
        "  {:>5}  {:>8}  {:>12}  {:>12}  {:>7}",
        "G", "gens", "dense Mbit/s", "sparse Mbit/s", "speedup"
    );
    for &g in &[96usize, 192, 384, 512] {
        // Keep total payload roughly constant (~16 MB) across G.
        let target_bytes = 16 * 1024 * 1024;
        let n_gen = (target_bytes / (g * ss)).max(2) as u64;
        let k = n_gen * g as u64;

        // Build one exact coded set per generation (K_G symbols each; no loss —
        // the decode cost is what we measure, delivered in arrival order).
        let mut enc = GenerationEncoder::new(symbol_size, g, n_gen as usize, 0.0);
        for seq in 0..k {
            enc.add_source(&payload_ss(seq, ss));
        }
        // Exactly K_G coded per generation, grouped so each generation gets a
        // solvable set.
        let mut by_gen: BTreeMap<u64, Vec<WireSymbol>> = BTreeMap::new();
        // Over-emit then take K_G independent per generation.
        for _ in 0..(k * 2) {
            let c = enc.generate_repair();
            let anchor = u64::from_le_bytes(c.data[0..8].try_into().unwrap());
            by_gen.entry(anchor).or_default().push(c);
        }
        let mut stream: Vec<WireSymbol> = Vec::new();
        for (_, syms) in by_gen.iter() {
            for c in syms.iter().take(g) {
                stream.push(c.clone());
            }
        }
        let payload_bits = (k * ss as u64 * 8) as f64;

        let t0 = Instant::now();
        let mut dense = GenerationDecoder::new(symbol_size);
        let mut dn = 0u64;
        for c in &stream {
            dn += dense.add_symbol(c).len() as u64;
        }
        let dense_s = t0.elapsed().as_secs_f64();
        assert_eq!(dn, k, "dense must decode all sources (G={g})");

        let t1 = Instant::now();
        let mut sparse = RlcWindowDecoder::new(symbol_size);
        let mut sn = 0u64;
        for c in &stream {
            sn += sparse.add_symbol(c).len() as u64;
        }
        let sparse_s = t1.elapsed().as_secs_f64();
        assert_eq!(sn, k, "sparse must decode all sources (G={g})");

        let dense_mbit = payload_bits / dense_s / 1e6;
        let sparse_mbit = payload_bits / sparse_s / 1e6;
        println!(
            "  {:>5}  {:>8}  {:>12.1}  {:>12.1}  {:>6.1}×",
            g, n_gen, dense_mbit, sparse_mbit, dense_mbit / sparse_mbit
        );
    }
    println!();
}

fn payload_ss(seq: u64, ss: usize) -> Vec<u8> {
    (0..ss)
        .map(|j| (seq as u8).wrapping_mul(31).wrapping_add(j as u8))
        .collect()
}

/// DIAGNOSIS (feat/fec-recovery-bug). Reproduces the PRODUCTION arrival
/// pattern the existing tests DON'T: a generation's first repair arrives
/// BEFORE some of its own (non-lost) sources, which then arrive LATE. In
/// production, sources and repairs interleave and reorder, so this is the
/// common case, not the corner. If the decoder freezes its known-source
/// pre-load at slot-creation and never injects late sources into the
/// existing matrix, coded repair can NEVER complete the generation (it would
/// need `width − sources_present_at_first_repair` repairs, not `holes`), and
/// recovery is forced onto ARQ raw retransmit.
#[test]
fn diag_late_source_after_first_repair_still_recovers_from_coded() {
    let symbol_size = 64u16;
    let g = 32usize;
    let mut enc = GenerationEncoder::new_systematic(symbol_size, g, 2, 0.5);
    let mut dec = GenerationDecoder::new(symbol_size);

    let sources: Vec<WireSymbol> = (0..g as u64).map(|seq| enc.add_source(&payload(seq))).collect();

    // TRUE holes (lost on the wire, never delivered as raw): seq 5 and 20.
    let holes: BTreeSet<u64> = [5u64, 20u64].into_iter().collect();
    // LATE sources: not lost, but they arrive AFTER the first repair.
    let late: BTreeSet<u64> = [10u64, 11u64, 12u64, 25u64].into_iter().collect();

    // Phase 1: deliver the EARLY sources (not hole, not late).
    for s in &sources {
        if holes.contains(&s.block_id) || late.contains(&s.block_id) {
            continue;
        }
        dec.add_symbol(s);
    }

    // Phase 2: first repair for the generation arrives NOW (creates the
    // matrix slot, pre-loading only the early sources).
    let anchor = 0u64;
    let first_repair = enc.generate_repair_for(anchor).expect("sealed gen codeable");
    dec.add_symbol(&first_repair);

    // Phase 3: the LATE (non-lost) sources arrive.
    for s in &sources {
        if late.contains(&s.block_id) {
            dec.add_symbol(s);
        }
    }

    // Phase 4: deficit top-up — emit coded repair until the generation
    // decodes, counting how many the decoder actually needed. The design
    // intends `holes` (== 2); the frozen-pre-load bug would demand ~`late`
    // more (matrix can't see the late sources) or never finish.
    let mut coded_used = 1u64; // first_repair already fed
    let mut delivered: BTreeSet<u64> = BTreeSet::new();
    while dec.rank_in(anchor, g as u64) < g as u64 {
        let c = enc.generate_repair_for(anchor).expect("codeable");
        for (seq, _) in dec.add_symbol(&c) {
            delivered.insert(seq);
        }
        coded_used += 1;
        assert!(coded_used <= g as u64, "runaway: coded_used={coded_used} — generation cannot complete from coded repair (frozen pre-load bug)");
    }
    // With correct late-source injection the generation completes in exactly
    // `holes` coded repairs regardless of the source arrival order.
    assert_eq!(coded_used, holes.len() as u64, "coded_used should equal holes ({}), got {coded_used}", holes.len());
}

/// PROACTIVE PACER (present-at-stall). The dedicated filling-generation pacer
/// must emit repair for an in-flight generation that is STILL FILLING —
/// under source backpressure (no further sources added) — and that repair,
/// coded over the retained contiguous PREFIX at the full generation width,
/// must recover an EARLY hole in the generation. This is the mechanism the
/// sealed-only proactive path structurally cannot do (it waits a full
/// generation-span for the seal). Verifies: (1) the pacer emits without the
/// generation ever sealing; (2) the filling repair keys to the (anchor, G)
/// matrix and recovers the early hole present-at-stall; (3) it combines with
/// a LATER sealed-generation deficit repair in the SAME matrix (no
/// cross-width stranding).
#[test]
fn proactive_pacer_recovers_filling_generation_hole_under_backpressure() {
    let symbol_size = 64u16;
    let g = 32usize;
    let r = 0.5f64;
    let mut enc = GenerationEncoder::new_systematic(symbol_size, g, 2, r);
    let mut dec = GenerationDecoder::new(symbol_size);

    // Fill ONLY the first HALF of generation 0 (a still-filling generation:
    // 16 of 32 sources). Model backpressure: no more sources will be added
    // for a while, but the receiver's frontier is already advancing over the
    // sent prefix and will stall on a hole in it.
    let w = g / 2; // 16 sources sent so far
    let sources: Vec<WireSymbol> = (0..w as u64).map(|seq| enc.add_source(&payload(seq))).collect();

    // The generation is NOT sealed: the sealed-only proactive path would emit
    // nothing for it.
    assert!(!enc.wants_coding(), "sealed-only path must have nothing to emit for a filling gen");
    // But the FILLING pacer DOES want to code it.
    assert!(enc.wants_filling_coding(), "pacer must want to code the in-flight generation");

    // Deliver the sent prefix EXCEPT an early hole at seq 5.
    let hole = 5u64;
    let mut delivered: BTreeSet<u64> = BTreeSet::new();
    for s in &sources {
        if s.block_id == hole {
            continue; // lost — the frontier stalls here
        }
        for (seq, _) in dec.add_symbol(s) {
            delivered.insert(seq);
        }
    }
    assert!(!delivered.contains(&hole), "hole not yet recovered");
    // present-at-stall probe: one hole in [0, w), no buffered repair yet.
    let (holes, buffered) = dec.frontier_probe(0, w as u64 - 1);
    assert_eq!(holes, 1);
    assert_eq!(buffered, 0, "no proactive repair present before the pacer runs");

    // Run the pacer: emit filling repair until it recovers the hole. Each
    // symbol codes over the present prefix [0, 16) at full width G=32.
    let mut recovered_hole = false;
    for _ in 0..(w as f64 * r).ceil() as u32 + 2 {
        if !enc.wants_filling_coding() {
            break;
        }
        let sym = enc.generate_repair_filling();
        // Wire invariants: full width G, FILL_FLAG set, coded_width = prefix.
        let width = u16::from_le_bytes(sym.data[8..10].try_into().unwrap());
        let wire_index = u32::from_le_bytes(sym.data[10..14].try_into().unwrap());
        assert_eq!(width as usize, g, "matrix width is the full generation G");
        assert_ne!(wire_index & FILL_FLAG, 0, "FILL_FLAG must be set");
        let coded_width = u16::from_le_bytes(sym.data[14..16].try_into().unwrap());
        assert_eq!(coded_width as usize, w, "coded_width = current prefix fill");
        for (seq, data) in dec.add_symbol(&sym) {
            assert_eq!(&data[..48], payload(seq).as_slice(), "byte-exact recovery");
            if seq == hole {
                recovered_hole = true;
            }
            delivered.insert(seq);
        }
        if recovered_hole {
            break;
        }
    }
    assert!(recovered_hole, "pacer must recover the early hole while the generation is still filling");

    // Now SEAL the generation (add the remaining sources) and top up via the
    // reactive deficit path over the SAME (anchor, G) matrix — the filling
    // repair and the sealed deficit repair must combine (no stranding).
    for seq in w as u64..g as u64 {
        enc.add_source(&payload(seq));
    }
    // Deliver the second half except one late hole.
    let hole2 = 20u64;
    for seq in w as u64..g as u64 {
        if seq == hole2 {
            continue;
        }
        for (s, _) in dec.add_symbol(&enc.get_source(seq).unwrap()) {
            delivered.insert(s);
        }
    }
    // Deficit loop over the sealed generation completes it in the SAME matrix.
    let mut guard = 0;
    while dec.rank_in(0, g as u64) < g as u64 {
        let c = enc.generate_repair_for(0).expect("sealed gen codeable");
        for (seq, _) in dec.add_symbol(&c) {
            delivered.insert(seq);
        }
        guard += 1;
        assert!(guard <= g, "must complete in ≤ G coded (matrices combined)");
    }
    for seq in 0..g as u64 {
        assert!(delivered.contains(&seq), "seq {seq} not delivered");
    }
}

// -----------------------------------------------------------------------
// Differential test: sparse-aware decoder vs the pre-rewrite reference.
// -----------------------------------------------------------------------

/// The sparse-aware `GenerationDecoder` must deliver EXACTLY the same
/// (seq, payload) set as the pre-rewrite dense `reference::RefGenerationDecoder`
/// on randomized traces — per `add_symbol` CALL (as a seq-sorted set; the
/// intra-call ORDER on a completing call is the one documented divergence),
/// with identical `added-rank` accounting (`repairs_useful`), `rank_in`,
/// and `total_fed`/`repairs_fed` at every step.  Traces randomize:
/// systematic vs coded-only wire, loss, reordering (late sources), FILL_FLAG
/// filling repairs, duplicate symbols, deficit top-ups, and `advance`.
#[test]
fn sparse_decoder_matches_reference_on_random_traces() {
    use crate::fec::window_traits::WindowDecoder as _;

    let symbol_size = 96u16;

    // SplitMix64 (deterministic, seeds 42 and 7 — the L1 discipline pair).
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
            z ^ (z >> 31)
        }
        fn chance(&mut self, p: f64) -> bool {
            (self.next() as f64 / u64::MAX as f64) < p
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    for seed in [42u64, 7, 1337, 2026] {
        let mut rng = Rng(seed);
        let g = 8 + rng.below(12) as usize; // generation size 8..19
        let n_gen = 4u64;
        let systematic = rng.chance(0.6);
        let eps = 0.05 + (rng.below(20) as f64) / 100.0; // 5..25 % loss
        let late = 0.15;
        let r = if systematic { 0.25 } else { 1.0 + 0.25 };

        let mut enc = if systematic {
            GenerationEncoder::new_systematic(symbol_size, g, n_gen as usize, 0.25)
        } else {
            GenerationEncoder::new(symbol_size, g, n_gen as usize, 0.25)
        };
        let _ = r;

        // Build the wire trace.
        let mut trace: Vec<WireSymbol> = Vec::new();
        for gen in 0..n_gen {
            let anchor = gen * g as u64;
            let mut late_src: Vec<WireSymbol> = Vec::new();
            for i in 0..g as u64 {
                let seq = anchor + i;
                let sym = enc.add_source(&payload(seq));
                // Occasional FILL_FLAG filling repair mid-fill.
                if rng.chance(0.15) && enc.wants_filling_coding() {
                    let rep = enc.generate_repair_filling();
                    if !rng.chance(eps) {
                        trace.push(rep);
                    }
                }
                if systematic {
                    if rng.chance(eps) {
                        // lost on the wire
                    } else if rng.chance(late) {
                        late_src.push(sym);
                    } else {
                        trace.push(sym);
                    }
                }
            }
            // Sealed proactive repairs (round-robin budget).
            while enc.wants_coding() {
                let rep = enc.generate_repair();
                if !rng.chance(eps) {
                    trace.push(rep);
                }
            }
            // Deficit top-up: enough full-width coded DoF to complete the
            // generation regardless of what was lost above.
            for _ in 0..(g + 3) {
                if let Some(rep) = enc.generate_repair_for(anchor) {
                    if !rng.chance(eps / 2.0) {
                        trace.push(rep);
                    }
                }
            }
            trace.extend(late_src);
            // Occasional duplicates (dedup path must behave identically).
            if rng.chance(0.5) && !trace.is_empty() {
                let dup = trace[trace.len() - 1 - (rng.below(trace.len().min(5) as u64) as usize)].clone();
                trace.push(dup);
            }
        }

        let mut dnew = GenerationDecoder::new(symbol_size);
        let mut dref = reference::RefGenerationDecoder::new(symbol_size);
        let total = n_gen * g as u64;

        let mut got_new: BTreeSet<u64> = BTreeSet::new();
        for (i, sym) in trace.iter().enumerate() {
            let mut out_new = dnew.add_symbol(sym);
            let mut out_ref = dref.add_symbol(sym);
            out_new.sort_by_key(|(s, _)| *s);
            out_ref.sort_by_key(|(s, _)| *s);
            assert_eq!(
                out_new.len(),
                out_ref.len(),
                "seed {seed} sym {i}: delivered-count mismatch (new {:?} vs ref {:?})",
                out_new.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
                out_ref.iter().map(|(s, _)| *s).collect::<Vec<_>>()
            );
            for ((sn, dn), (sr, dr)) in out_new.iter().zip(out_ref.iter()) {
                assert_eq!(sn, sr, "seed {seed} sym {i}: seq mismatch");
                assert_eq!(dn, dr, "seed {seed} sym {i} seq {sn}: payload mismatch");
                got_new.insert(*sn);
            }
            assert_eq!(dnew.total_fed(), dref.total_fed(), "seed {seed} sym {i}: total_fed");
            assert_eq!(dnew.repairs_fed(), dref.repairs_fed(), "seed {seed} sym {i}: repairs_fed");
            assert_eq!(
                dnew.repairs_useful(),
                dref.repairs_useful(),
                "seed {seed} sym {i}: repairs_useful (added-rank accounting)"
            );
            // Deficit-feedback signal must agree for every generation.
            for gen in 0..n_gen {
                let anchor = gen * g as u64;
                assert_eq!(
                    dnew.rank_in(anchor, g as u64),
                    dref.rank_in(anchor, g as u64),
                    "seed {seed} sym {i}: rank_in(gen {gen})"
                );
            }
            // Mid-trace advance (retention prune) once per trace.
            if i == trace.len() / 2 {
                let adv = g as u64; // one whole generation behind
                dnew.advance(adv);
                dref.advance(adv);
            }
        }

        // Every source must have been recovered byte-exactly by BOTH.
        assert_eq!(
            got_new.len() as u64,
            total,
            "seed {seed}: not all sources recovered (systematic={systematic} eps={eps:.2})"
        );
    }
}

// -----------------------------------------------------------------------
// Differential test: UNIFIED global decoder vs the keyed generation
// machine AND the pre-§16.18 reference oracle on ALIGNED generation
// wires (task #61, paper §16.20). On span-aligned traces the global
// system is block-diagonal, so the unified machine must agree EXACTLY —
// per call, sets, bytes, rank_in, and the added-rank accounting.
// -----------------------------------------------------------------------
#[test]
fn unified_matches_generation_and_reference_on_aligned_traces() {
    use crate::fec::unified::UnifiedDecoder;
    use crate::fec::window_traits::WindowDecoder as _;

    let symbol_size = 96u16;

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
            z ^ (z >> 31)
        }
        fn chance(&mut self, p: f64) -> bool {
            (self.next() as f64 / u64::MAX as f64) < p
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    for seed in [42u64, 7, 1337, 2026, 61] {
        let mut rng = Rng(seed ^ 0x0061);
        let g = 8 + rng.below(12) as usize;
        let n_gen = 4u64;
        let systematic = rng.chance(0.6);
        let eps = 0.05 + (rng.below(20) as f64) / 100.0;
        let late = 0.15;

        let mut enc = if systematic {
            GenerationEncoder::new_systematic(symbol_size, g, n_gen as usize, 0.25)
        } else {
            GenerationEncoder::new(symbol_size, g, n_gen as usize, 0.25)
        };

        let mut trace: Vec<WireSymbol> = Vec::new();
        for gen in 0..n_gen {
            let anchor = gen * g as u64;
            let mut late_src: Vec<WireSymbol> = Vec::new();
            for i in 0..g as u64 {
                let seq = anchor + i;
                let sym = enc.add_source(&payload(seq));
                if rng.chance(0.15) && enc.wants_filling_coding() {
                    let rep = enc.generate_repair_filling();
                    if !rng.chance(eps) {
                        trace.push(rep);
                    }
                }
                if systematic {
                    if rng.chance(eps) {
                        // lost on the wire
                    } else if rng.chance(late) {
                        late_src.push(sym);
                    } else {
                        trace.push(sym);
                    }
                }
            }
            while enc.wants_coding() {
                let rep = enc.generate_repair();
                if !rng.chance(eps) {
                    trace.push(rep);
                }
            }
            for _ in 0..(g + 3) {
                if let Some(rep) = enc.generate_repair_for(anchor) {
                    if !rng.chance(eps / 2.0) {
                        trace.push(rep);
                    }
                }
            }
            trace.extend(late_src);
            if rng.chance(0.5) && !trace.is_empty() {
                let dup = trace
                    [trace.len() - 1 - (rng.below(trace.len().min(5) as u64) as usize)]
                .clone();
                trace.push(dup);
            }
        }

        let mut dgen = GenerationDecoder::new(symbol_size);
        let mut dref = reference::RefGenerationDecoder::new(symbol_size);
        let mut duni = UnifiedDecoder::new(symbol_size);
        let total = n_gen * g as u64;

        let mut got_uni: BTreeSet<u64> = BTreeSet::new();
        for (i, sym) in trace.iter().enumerate() {
            let mut out_gen = dgen.add_symbol(sym);
            let mut out_ref = dref.add_symbol(sym);
            let mut out_uni = duni.add_symbol(sym);
            out_gen.sort_by_key(|(s, _)| *s);
            out_ref.sort_by_key(|(s, _)| *s);
            out_uni.sort_by_key(|(s, _)| *s);
            assert_eq!(
                out_uni.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
                out_gen.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
                "seed {seed} sym {i}: unified vs generation delivered set"
            );
            assert_eq!(
                out_uni.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
                out_ref.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
                "seed {seed} sym {i}: unified vs reference delivered set"
            );
            for ((su, du), (sg, dg)) in out_uni.iter().zip(out_gen.iter()) {
                assert_eq!(su, sg);
                assert_eq!(du, dg, "seed {seed} sym {i} seq {su}: payload bytes");
                got_uni.insert(*su);
            }
            assert_eq!(duni.total_fed(), dgen.total_fed(), "seed {seed} sym {i}: total_fed");
            assert_eq!(
                duni.repairs_fed(),
                dgen.repairs_fed(),
                "seed {seed} sym {i}: repairs_fed"
            );
            assert_eq!(
                duni.repairs_useful(),
                dgen.repairs_useful(),
                "seed {seed} sym {i}: repairs_useful (added-rank accounting)"
            );
            for gen in 0..n_gen {
                let anchor = gen * g as u64;
                assert_eq!(
                    duni.rank_in(anchor, g as u64),
                    dgen.rank_in(anchor, g as u64),
                    "seed {seed} sym {i}: rank_in(gen {gen})"
                );
            }
            if i == trace.len() / 2 {
                let adv = g as u64;
                dgen.advance(adv);
                dref.advance(adv);
                duni.advance(adv);
            }
        }
        assert_eq!(
            got_uni.len() as u64,
            total,
            "seed {seed}: unified did not recover all sources (systematic={systematic} eps={eps:.2})"
        );
    }
}
