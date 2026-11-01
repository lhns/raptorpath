//! The per-drain prune contract of the LEGACY decoders (RLC window,
//! generation, and the dense reference), mirroring the unified decoder's
//! (`unified.rs` tests, paper §5.2): with `advance(F)` called once per
//! drain, F the decode-inert floor,
//!   * no retained set holds a key below F (the generation decoder keeps the
//!     payloads under a surviving Solving slot, and only those),
//!   * every seq is delivered exactly once with its exact bytes, under
//!     duplicates, reordering and repairs, and
//!   * a source re-fed below F returns nothing.
//! These guard the contract while `advance` changes from scans to
//! pop-from-the-front prunes; `tests/legacy_decoders_prune_by_removal.rs` is
//! the routing gate that the scans are gone.

use super::generation::reference::RefGenerationDecoder;
use super::generation::{GenerationDecoder, GenerationEncoder};
use super::rlc_window::{RlcWindowDecoder, RlcWindowEncoder};
use super::traits::WireSymbol;
use super::window_traits::{WindowDecoder, WindowEncoder};
use bytes::Bytes;
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use std::collections::BTreeMap;

fn payload(seq: u64, ss: usize) -> Vec<u8> {
    (0..ss).map(|j| (seq as u8).wrapping_mul(31).wrapping_add(j as u8)).collect()
}

/// Record a decoder's output: each seq at most once, with its exact bytes.
fn deliver(out: Vec<(u64, Bytes)>, ss: usize, ctx: &str, got: &mut BTreeMap<u64, Bytes>) {
    for (q, b) in out {
        assert_eq!(&b[..ss], &payload(q, ss)[..], "bytes of seq {q} ({ctx})");
        assert!(got.insert(q, b).is_none(), "seq {q} delivered twice ({ctx})");
    }
}

/// Sliding-window (RLC) decoder: sources by `add_source`, repairs over the
/// live window `[lowest hole, newest]`.
fn rlc_scenario(seed: u64) {
    let ss = 48usize;
    let (w_cap, drain, n) = (64u64, 32usize, 2_496usize);
    let ctx = format!("rlc seed {seed}");
    let mut rng = ChaCha8Rng::seed_from_u64(0x1E6A ^ seed);
    let mut enc = RlcWindowEncoder::new(ss as u16);
    let mut dec = RlcWindowDecoder::new(ss as u16);
    let mut syms: Vec<WireSymbol> = Vec::new();
    let mut got: BTreeMap<u64, Bytes> = BTreeMap::new();
    let mut nexp = 0u64;
    let mut round = 0usize;
    while syms.len() < n || nexp < n as u64 {
        round += 1;
        assert!(round < 500, "no progress ({ctx}, nexp {nexp})");
        let mut arr = Vec::new();
        if syms.len() < n {
            for _ in 0..drain {
                let s = enc.add_source(&payload(syms.len() as u64, ss));
                syms.push(s.clone());
                if rng.gen::<f64>() >= 0.10 {
                    arr.push(s);
                }
            }
        }
        let newest = syms.len() as u64 - 1;
        for s in arr.clone() {
            if rng.gen::<f64>() < 0.15 {
                arr.push(s); // immediate duplicate
            }
        }
        for _ in 0..8 {
            let back = rng.gen_range(0..3 * w_cap);
            arr.push(syms[newest.saturating_sub(back) as usize].clone()); // late duplicate
        }
        arr.shuffle(&mut rng);
        for s in &arr {
            deliver(dec.add_symbol(s), ss, &ctx, &mut got);
        }
        let holes: Vec<u64> = (nexp..=newest).filter(|q| !got.contains_key(q)).collect();
        if let Some(&lo) = holes.first() {
            for _ in 0..holes.len() + 2 {
                if let Some(r) = enc.generate_repair_range(lo, (newest + 1 - lo) as u16) {
                    deliver(dec.add_symbol(&r), ss, &ctx, &mut got);
                    if rng.gen::<f64>() < 0.3 {
                        deliver(dec.add_symbol(&r), ss, &ctx, &mut got); // immediate dup
                    }
                }
            }
        }
        while got.contains_key(&nexp) {
            nexp += 1;
        }
        let floor = nexp.saturating_sub(1).saturating_sub(2 * w_cap);
        dec.advance(floor);
        enc.advance(floor);

        // No retained key below the floor: recovered, output, seen repairs,
        // pivot rows AND the columns inside the surviving rows.
        let names = ["recovered", "output", "seen repair", "row pivot", "row column"];
        for (name, m) in names.iter().zip(dec.min_retained_keys()) {
            assert!(m.map_or(true, |k| k >= floor), "{name} holds {m:?} below the floor {floor} ({ctx})");
        }
        // A source re-fed below the floor is a duplicate by definition.
        if floor > 0 {
            for q in [floor - 1, floor.saturating_sub(40)] {
                assert!(dec.add_symbol(&syms[q as usize]).is_empty(), "re-fed seq {q} < floor {floor} ({ctx})");
                assert!(!dec.seq_probe(q).0, "seq {q} below the floor is still in the seen set ({ctx})");
            }
        }
    }
    assert_eq!(nexp, n as u64, "{ctx}");
    assert_eq!(got.len(), n, "every seq delivered exactly once ({ctx})");
}

#[test]
fn rlc_window_prunes_to_the_floor_and_delivers_exactly_once() {
    for seed in 0..6 {
        rlc_scenario(seed);
    }
}

/// Generation decoders (sparse and dense reference): systematic sources, plus
/// deficit repairs `generate_repair_for(anchor)` for every generation that
/// still has holes. `probe` returns
/// `[recovered min, seen repair min, gens min end, live Solving anchor]`.
fn generation_scenario<D: WindowDecoder>(name: &str, seed: u64, mut dec: D, probe: fn(&D) -> [Option<u64>; 4]) {
    let ss = 48usize;
    let (g, n_gen) = (16usize, 60u64);
    let n = n_gen * g as u64;
    let ctx = format!("{name} seed {seed}");
    let mut rng = ChaCha8Rng::seed_from_u64(0x6E4 ^ seed);
    let mut enc = GenerationEncoder::new_systematic(ss as u16, g, 4, 0.25);
    let mut syms: Vec<WireSymbol> = Vec::new();
    let mut got: BTreeMap<u64, Bytes> = BTreeMap::new();
    let mut nexp = 0u64;
    for gen in 0..n_gen {
        let mut arr = Vec::new();
        for _ in 0..g {
            let s = enc.add_source(&payload(syms.len() as u64, ss));
            syms.push(s.clone());
            if rng.gen::<f64>() >= 0.10 {
                arr.push(s);
            }
        }
        let newest = syms.len() as u64 - 1;
        for _ in 0..6 {
            let back = rng.gen_range(0..3 * g as u64);
            arr.push(syms[newest.saturating_sub(back) as usize].clone()); // duplicate, possibly late
        }
        arr.shuffle(&mut rng);
        for s in &arr {
            deliver(dec.add_symbol(s), ss, &ctx, &mut got);
        }
        // Deficit repairs for every generation from the frontier's on.
        for gi in (nexp / g as u64)..=gen {
            let anchor = gi * g as u64;
            let holes = (anchor..anchor + g as u64).filter(|q| !got.contains_key(q)).count();
            if holes == 0 {
                continue;
            }
            for _ in 0..holes + 3 {
                if let Some(r) = enc.generate_repair_for(anchor) {
                    deliver(dec.add_symbol(&r), ss, &ctx, &mut got);
                }
            }
        }
        while got.contains_key(&nexp) {
            nexp += 1;
        }
        let floor = nexp.saturating_sub(1).saturating_sub(2 * g as u64);
        dec.advance(floor);

        let [recovered, seen_repair, gens_end, live] = probe(&dec);
        // Payloads are kept under a surviving Solving slot (its known
        // columns eliminate against them), and only there.
        let keep = floor.min(live.unwrap_or(u64::MAX));
        assert!(recovered.map_or(true, |k| k >= keep), "recovered {recovered:?} below {keep} ({ctx})");
        assert!(seen_repair.map_or(true, |k| k >= floor), "seen repair {seen_repair:?} < floor {floor} ({ctx})");
        // A generation wholly below the floor is dropped.
        assert!(gens_end.map_or(true, |e| e > floor), "gen ending at {gens_end:?} <= floor {floor} ({ctx})");
        // A source re-fed below the floor returns nothing.
        if floor > 0 {
            for q in [floor - 1, floor.saturating_sub(40)] {
                assert!(dec.add_symbol(&syms[q as usize]).is_empty(), "re-fed seq {q} < floor {floor} ({ctx})");
            }
        }
    }
    assert_eq!(nexp, n, "{ctx}: every seq below the frontier");
    assert_eq!(got.len() as u64, n, "every seq delivered exactly once ({ctx})");
}

#[test]
fn generation_decoder_prunes_to_the_floor_and_delivers_exactly_once() {
    for seed in 0..6 {
        generation_scenario("generation", seed, GenerationDecoder::new(48), |d| d.min_retained_keys());
    }
}

#[test]
fn reference_generation_decoder_prunes_to_the_floor_and_delivers_exactly_once() {
    for seed in 0..6 {
        generation_scenario("reference", seed, RefGenerationDecoder::new(48), |d| d.min_retained_keys());
    }
}
