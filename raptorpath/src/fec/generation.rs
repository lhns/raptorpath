//! Generation-based RLC encoder (stable per-generation coding anchor).
//!
//! WHY THIS EXISTS.  The coded *sliding* window (`RlcWindowEncoder` in
//! coded-only mode) codes every symbol over the CURRENT window, whose anchor
//! MOVES with the frontier.  A slow-path symbol arrives one path-delay after
//! its window has slid on, so it is stranded and can only be recovered by a
//! congestion-throttled per-seq ARQ — the anti-aggregation drag L1 measured
//! (×0.26 at C8; paper §16.3, `temporal_oracle.rs`).  The corrected oracle
//! showed the fix is a STABLE anchor: partition the object's source symbols
//! into FIXED generations of `gen_size ≈ W_mp` (384–512 at C8) and code coded
//! symbols WITHIN each generation.  A generation's coding target never moves,
//! so any coded symbol for it — from ANY path, at any time — supplies an
//! interchangeable degree of freedom, and a lost symbol is replaced by the
//! next coded symbol for the SAME generation from EITHER path (fungible
//! cross-path recovery, no per-seq throttle).  Generations pipeline (`M ≥ 2`
//! in flight) so the fast path never idles on a slow generation's tail.
//!
//! WIRE FORMAT — IDENTICAL to the sliding-window RLC repair, by design.  A
//! generation-coded symbol is an RLC combination over the fixed span
//! `[g·G, g·G + gen_len)`, so it carries exactly the same self-describing
//! header the decoder already parses:
//!   `data = [window_start(8 LE)][window_count(2 LE)][coded_index(4 LE)][coded]`
//! where `window_start = g·G` (the generation anchor, STABLE — this is the
//! only substantive difference from the sliding encoder, whose window_start
//! moves every symbol), `window_count = gen_len` (K_G, the generation's live
//! length), `coded_index` a per-symbol monotonic counter.  The generation id
//! and K_G are therefore ON THE WIRE as `window_start / gen_size` and
//! `window_count`.  Because the coefficients only ever touch seqs inside one
//! generation, the existing `RlcWindowDecoder` solves each generation's K_G×K_G
//! system independently the instant K_G linearly-independent symbols for that
//! anchor arrive (decode-on-K), delivering its sources out-of-order — NO
//! decoder change is needed.

use std::collections::BTreeMap;

use super::gf256;
use super::traits::{FecBackend, WireSymbol};
use super::window_traits::WindowEncoder;

pub use gf256::generate_window_coefficients;

/// Repair header: 8 (window_start) + 2 (window_count) + 4 (coded_index) = 14.
/// Byte-identical to `rlc_window`'s repair header so the same decoder path
/// parses generation-coded symbols.
const REPAIR_HEADER_SIZE: usize = 14;

/// Generation-based RLC encoder.  Retains every not-yet-advanced source symbol
/// (partitioned into fixed generations) and emits coded symbols for the
/// pipeline of in-flight generations, round-robin.
pub struct GenerationEncoder {
    symbol_size: u16,
    /// Generation size G (source symbols per generation) — the STABLE coding
    /// unit; a generation's anchor is `g·gen_size` and never moves.
    gen_size: u64,
    /// Pipeline depth M: how many generations ahead of the retention base the
    /// encoder codes concurrently (M ≥ 2 keeps the fast path from idling on a
    /// slow generation's tail).
    pipeline: u64,
    /// Retained source symbols: seq → padded data.  Held until `advance`
    /// (driven by the receiver's per-generation completion) drops them.
    sources: BTreeMap<u64, Vec<u8>>,
    /// Next source seq to assign.
    next_seq: u64,
    /// Lowest generation still retained (the retention frontier, in
    /// generations).  Coding never touches a generation below this.
    base_gen: u64,
    /// Round-robin cursor over the active generation set (a generation id).
    rr: u64,
    /// Monotonic coded-symbol index — distinguishes coded symbols (and their
    /// coefficient seeds) both within and across generations.
    coded_index: u32,
    /// Proactive overhead r: a generation is coded up to
    /// `ceil(current_len·(1+r))` coded symbols before it is considered
    /// "provisioned"; beyond that it is only coded for RECOVERY (when no active
    /// generation is still under budget). This bounds coded emission on a
    /// still-FILLING generation to ~its current source count — the fix for the
    /// startup stall where the whole flow-control window was spent on a
    /// 2-source generation, producing rank-2 symbols that decode only 2 seqs.
    overhead: f64,
    /// Coded symbols emitted per generation (for the budget above).
    emitted: BTreeMap<u64, u32>,
    /// Source intake is idle (object tail). When set, the final PARTIAL
    /// generation is also eligible for recovery coding (it never seals to its
    /// full gen_size, but once no more sources are coming its coded symbols do
    /// span its full — final — width, so recovery supplies useful DoF).
    intake_idle: bool,
}

impl GenerationEncoder {
    pub fn new(symbol_size: u16, gen_size: usize, pipeline: usize, overhead: f64) -> Self {
        Self {
            symbol_size,
            gen_size: (gen_size.max(1)) as u64,
            pipeline: (pipeline.max(1)) as u64,
            sources: BTreeMap::new(),
            next_seq: 0,
            base_gen: 0,
            rr: 0,
            coded_index: 0,
            overhead: overhead.max(0.0),
            emitted: BTreeMap::new(),
            intake_idle: false,
        }
    }

    /// Number of retained sources currently in generation `g`.
    fn gen_len(&self, g: u64) -> u64 {
        let start = g * self.gen_size;
        self.sources.range(start..start + self.gen_size).count() as u64
    }

    /// Coded-symbol budget that "provisions" generation `g` at its current fill.
    fn gen_budget(&self, g: u64) -> u32 {
        ((self.gen_len(g) as f64) * (1.0 + self.overhead)).ceil() as u32
    }

    /// Whether generation `g` should be coded right now. A generation is coded
    /// ONLY once it is SEALED (all `gen_size` sources present) — or, for the
    /// final partial generation, once intake is idle. Coding a still-FILLING
    /// generation is the trap: its coded span only the few sources present at
    /// emit time (rank ≈ that few), so a fast fill or an exhausted budget leaves
    /// the sealed generation with mostly low-rank symbols and it never reaches
    /// K_G. Only a sealed generation's coded span its full width, so K_G of them
    /// decode it. Within that, `g` is coded up to its proactive budget, and the
    /// FRONTIER generation (base_gen) up to the larger recovery cap.
    fn codeable(&self, g: u64) -> bool {
        if !self.sources.contains_key(&(g * self.gen_size)) {
            return false;
        }
        let sealed = self.gen_len(g) >= self.gen_size || self.intake_idle;
        if !sealed {
            return false;
        }
        // PROACTIVE emission is capped at the per-generation provisioning budget
        // `ceil(len·(1+r))` for EVERY generation, frontier included. Recovery
        // BEYOND the budget (for a sealed generation that lost > r of its coded)
        // is no longer a feedback-free fixed cap — it is driven by per-generation
        // DEFICIT FEEDBACK (§16.3): the receiver reports each frontier
        // generation's residual rank and the sender emits exactly that many more
        // via `generate_repair_for`, which BYPASSES this budget. So the proactive
        // path stays bounded (never floods a generation) and recovery is bounded
        // AND targeted by the deficit loop — the two the feedback-free cap could
        // not do at once (it either flooded or deadlocked the frontier).
        let emitted = self.emitted.get(&g).copied().unwrap_or(0);
        emitted < self.gen_budget(g)
    }

    /// Code one coded symbol over the STABLE span of generation `g`, advancing
    /// the monotonic coded index and the per-generation emission counter. Shared
    /// by the round-robin proactive path (`generate_repair`) and the deficit-
    /// driven recovery path (`generate_repair_for`).
    fn code_generation(&mut self, g: u64) -> WireSymbol {
        let symbol_size = self.symbol_size as usize;
        let coded_index = self.coded_index;
        self.coded_index += 1;
        *self.emitted.entry(g).or_insert(0) += 1;
        let (gen_start, syms) = self.generation_symbols(g);
        let gen_len = syms.len() as u16;

        // Coefficients over the STABLE generation span [gen_start, gen_start +
        // gen_len).  Seeded by (gen_start, gen_len, coded_index) — the decoder
        // regenerates them from the wire header identically.
        let coeffs = generate_window_coefficients(gen_start, gen_len, coded_index);
        let mut coded = vec![0u8; symbol_size];
        for (i, src) in syms.iter().enumerate() {
            gf256::mul_acc_slice(coeffs[i], src, &mut coded);
        }

        let mut wire_data = Vec::with_capacity(REPAIR_HEADER_SIZE + symbol_size);
        wire_data.extend_from_slice(&gen_start.to_le_bytes());
        wire_data.extend_from_slice(&gen_len.to_le_bytes());
        wire_data.extend_from_slice(&coded_index.to_le_bytes());
        wire_data.extend_from_slice(&coded);

        WireSymbol {
            block_id: gen_start + gen_len.saturating_sub(1) as u64,
            payload_id: coded_index,
            is_repair: true,
            data: wire_data,
            backend: FecBackend::Rlc,
        }
    }

    /// Generation id that a source seq belongs to.
    fn gen_of(&self, seq: u64) -> u64 {
        seq / self.gen_size
    }

    /// The highest generation that has at least one retained source (or
    /// `base_gen` when empty).
    fn top_gen(&self) -> u64 {
        self.sources
            .keys()
            .next_back()
            .map(|&s| self.gen_of(s))
            .unwrap_or(self.base_gen)
    }

    /// The contiguous retained sources of generation `g`, in seq order.
    /// Returns `(gen_start, symbols)`; `symbols[i]` is seq `gen_start + i`.
    fn generation_symbols(&self, g: u64) -> (u64, Vec<&Vec<u8>>) {
        let gen_start = g * self.gen_size;
        let mut out = Vec::new();
        let mut seq = gen_start;
        while seq < gen_start + self.gen_size {
            match self.sources.get(&seq) {
                Some(d) => out.push(d),
                None => break, // gap ⇒ generation not (yet) contiguous past here
            }
            seq += 1;
        }
        (gen_start, out)
    }

    /// Pick the next generation to PROACTIVELY code for: round-robin over the
    /// `pipeline` oldest retained generations that are still under their
    /// provisioning budget (`emitted < ceil(len·(1+r))`). This is the open-loop
    /// path that provisions each sealed generation with its baseline K_G(1+r)
    /// coded symbols so it decodes without waiting a feedback round for the
    /// expected loss. RECOVERY beyond the budget (for a generation that lost more
    /// than `r` of its coded) is NOT done here — it is driven by per-generation
    /// deficit feedback via `generate_repair_for`, which the receiver bounds and
    /// targets exactly. Returns `None` when every active generation is at budget
    /// (or nothing is retained), at which point `wants_coding` is false and the
    /// sender's emission is purely deficit-driven until a generation decodes.
    fn next_active_gen(&mut self) -> Option<u64> {
        let top = self.top_gen();
        let hi = (self.base_gen + self.pipeline).min(top + 1);
        if hi <= self.base_gen {
            return None;
        }
        let span = hi - self.base_gen;
        // Round-robin over CODEABLE generations (sealed/tail-idle and still under
        // their provisioning budget). Once a generation is at budget it drops out
        // of the proactive round-robin; its residual, if any, is recovered by the
        // deficit loop (fungible cross-path, no per-seq ARQ).
        for _ in 0..span {
            if self.rr < self.base_gen || self.rr >= hi {
                self.rr = self.base_gen;
            }
            let g = self.rr;
            self.rr += 1;
            if self.codeable(g) {
                return Some(g);
            }
        }
        None
    }
}

impl WindowEncoder for GenerationEncoder {
    fn add_source(&mut self, data: &[u8]) -> WireSymbol {
        let seq = self.next_seq;
        self.next_seq += 1;

        let mut padded = vec![0u8; self.symbol_size as usize];
        let copy_len = data.len().min(self.symbol_size as usize);
        padded[..copy_len].copy_from_slice(&data[..copy_len]);
        self.sources.insert(seq, padded.clone());

        // The systematic form is returned for retention/bookkeeping; in
        // generation mode the sender puts a coded combination on the wire
        // (never this raw symbol), so no fixed in-order position exists.
        WireSymbol {
            block_id: seq,
            payload_id: 0,
            is_repair: false,
            data: padded,
            backend: FecBackend::Rlc,
        }
    }

    fn generate_repair(&mut self) -> WireSymbol {
        match self.next_active_gen() {
            Some(g) => self.code_generation(g),
            None => {
                // Nothing retained — emit an inert zero symbol (matches the
                // sliding encoder's empty-window contract).
                WireSymbol {
                    block_id: 0,
                    payload_id: self.coded_index,
                    is_repair: true,
                    data: vec![0u8; REPAIR_HEADER_SIZE + self.symbol_size as usize],
                    backend: FecBackend::Rlc,
                }
            }
        }
    }

    fn generate_repair_for(&mut self, anchor: u64) -> Option<WireSymbol> {
        // Deficit-driven recovery for a SPECIFIC generation (§16.3). Bypasses
        // the proactive per-generation budget: the receiver's deficit already
        // bounds how many are emitted, so there is no cap to apply here — the
        // only gate is that the generation is retained and SEALED (its coded
        // must span the full generation width, else they are low-rank and never
        // help the generation reach K_G).
        if self.gen_size == 0 || anchor % self.gen_size != 0 {
            return None;
        }
        let g = anchor / self.gen_size;
        if !self.sources.contains_key(&anchor) {
            return None; // generation not retained (already advanced past, or not yet started)
        }
        let sealed = self.gen_len(g) >= self.gen_size || self.intake_idle;
        if !sealed {
            return None;
        }
        Some(self.code_generation(g))
    }

    fn window_span(&self) -> (u64, u64) {
        match (self.sources.keys().next(), self.sources.keys().next_back()) {
            (Some(&s), Some(&e)) => (s, e),
            _ => (0, 0),
        }
    }

    fn advance(&mut self, oldest_seq: u64) {
        // Generation-align the drop so a generation is never split (its coded
        // symbols must always cover a contiguous [gen_start, gen_start+len)).
        let gen_floor = self.gen_of(oldest_seq) * self.gen_size;
        let drop: Vec<u64> = self.sources.range(..gen_floor).map(|(&k, _)| k).collect();
        for k in drop {
            self.sources.remove(&k);
        }
        self.base_gen = self.gen_of(gen_floor);
        if self.rr < self.base_gen {
            self.rr = self.base_gen;
        }
        // Drop per-generation emission counters for dropped generations.
        let drop_gens: Vec<u64> = self.emitted.range(..self.base_gen).map(|(&k, _)| k).collect();
        for k in drop_gens {
            self.emitted.remove(&k);
        }
    }

    fn window_size(&self) -> usize {
        self.sources.len()
    }

    fn get_source(&self, seq: u64) -> Option<WireSymbol> {
        self.sources.get(&seq).map(|data| WireSymbol {
            block_id: seq,
            payload_id: 0,
            is_repair: false,
            data: data.clone(),
            backend: FecBackend::Rlc,
        })
    }

    fn set_intake_idle(&mut self, idle: bool) {
        self.intake_idle = idle;
    }

    fn wants_coding(&self) -> bool {
        let top = self.top_gen();
        let hi = (self.base_gen + self.pipeline).min(top + 1);
        if hi <= self.base_gen {
            return false;
        }
        (self.base_gen..hi).any(|g| self.codeable(g))
    }
}

#[cfg(test)]
mod tests {
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
}
