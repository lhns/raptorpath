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

use std::collections::{BTreeMap, HashSet};

use bytes::Bytes;

use super::gf256;
use super::traits::{FecBackend, WireSymbol};
use super::window_traits::{WindowDecoder, WindowEncoder};

pub use gf256::generate_window_coefficients;

/// Repair header: 8 (window_start) + 2 (window_count) + 4 (coded_index) = 14.
/// Byte-identical to `rlc_window`'s repair header so the same decoder path
/// parses generation-coded symbols.
const REPAIR_HEADER_SIZE: usize = 14;

/// Marker bit set in the 4-byte wire coded-index of a FILLING-generation repair
/// (see `code_generation_full` / the proactive pacer). When set, the decoder
/// reads a 2-byte `coded_width` immediately after the 14-byte header (a 16-byte
/// header total) and treats coefficient columns `[coded_width, window_count)` as
/// ZERO — the sender only summed the retained contiguous prefix
/// `[anchor, anchor+coded_width)`, but the MATRIX width on the wire is the full
/// generation size `G`, so a filling-generation repair keys to the SAME
/// `(anchor, G)` decoder system as the sealed repairs and the reactive deficit
/// loop (no cross-width stranding — the refutation of the separate-grid inline
/// repair). The real coded-index is the low 31 bits (it never reaches 2^31, so
/// masking is lossless). Never set on the 14-byte sealed/legacy format.
const FILL_FLAG: u32 = 0x8000_0000;

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
    /// Fix 3 (transport-substrate): PROACTIVE-CODING floor (in generations),
    /// decoupled from `base_gen` (the retention floor). The proactive
    /// round-robin codes `[code_base, code_base+pipeline)`. Defaults to
    /// tracking `base_gen` (identical behaviour) unless `set_code_base` advances
    /// it to follow the SEND frontier — which lets freshly-sent generations get
    /// their upfront proactive budget while a stalled in-order-frontier
    /// generation is left to bounded reactive recovery (its sources stay
    /// retained). Always `>= base_gen`.
    code_base: u64,
    /// Round-robin cursor over the active generation set (a generation id).
    rr: u64,
    /// Round-robin cursor for the FILLING-generation proactive pacer
    /// (`generate_repair_filling`), independent of `rr` so the two emission
    /// paths do not fight over one cursor.
    fill_rr: u64,
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
    /// SYSTEMATIC-repair submode (§16.3 oracle "systematic + deficit repair").
    /// When set, the raw source symbols ride the wire as PRIMARY (delivered
    /// out-of-order with ZERO decode) and this encoder emits ONLY REPAIR: the
    /// per-generation proactive budget is `ceil(len·r)` — just the loss-FEC
    /// overhead — instead of the coded-only `ceil(len·(1+r))` that had to supply
    /// EVERY degree of freedom. The K base DoF come from the systematic source
    /// on the wire; coded symbols only cover the holes (deficit-driven top-up
    /// via `generate_repair_for` handles the residual). This is the change that
    /// makes decode O(deficit) not O(G) and delivers source on arrival — the two
    /// L1-killers of coded-only, structurally removed.
    systematic: bool,
}

impl GenerationEncoder {
    pub fn new(symbol_size: u16, gen_size: usize, pipeline: usize, overhead: f64) -> Self {
        Self::new_mode(symbol_size, gen_size, pipeline, overhead, false)
    }

    /// Systematic-repair encoder: source rides the wire as primary; this encoder
    /// emits only the `ceil(len·r)` repair overhead per generation (plus the
    /// deficit-driven top-up). See the `systematic` field.
    pub fn new_systematic(symbol_size: u16, gen_size: usize, pipeline: usize, overhead: f64) -> Self {
        Self::new_mode(symbol_size, gen_size, pipeline, overhead, true)
    }

    fn new_mode(symbol_size: u16, gen_size: usize, pipeline: usize, overhead: f64, systematic: bool) -> Self {
        Self {
            symbol_size,
            gen_size: (gen_size.max(1)) as u64,
            pipeline: (pipeline.max(1)) as u64,
            sources: BTreeMap::new(),
            next_seq: 0,
            base_gen: 0,
            code_base: 0,
            rr: 0,
            fill_rr: 0,
            coded_index: 0,
            overhead: overhead.max(0.0),
            emitted: BTreeMap::new(),
            intake_idle: false,
            systematic,
        }
    }

    /// Number of retained sources currently in generation `g`.
    fn gen_len(&self, g: u64) -> u64 {
        let start = g * self.gen_size;
        self.sources.range(start..start + self.gen_size).count() as u64
    }

    /// Coded-symbol budget that "provisions" generation `g` at its current fill.
    /// Coded-only mode must supply EVERY degree of freedom, so it provisions
    /// `ceil(len·(1+r))` coded (the K base + the r overhead). Systematic-repair
    /// mode gets the K base DoF from the raw source on the wire, so it provisions
    /// only the `ceil(len·r)` loss-FEC overhead; the residual deficit is topped
    /// up by `generate_repair_for`.
    fn gen_budget(&self, g: u64) -> u32 {
        let len = self.gen_len(g) as f64;
        let factor = if self.systematic { self.overhead } else { 1.0 + self.overhead };
        (len * factor).ceil() as u32
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
        // Fix 3: proactive coding is anchored at `code_base` (>= base_gen), the
        // SEND-frontier-tracking coding floor, not the retention floor. Default
        // code_base == base_gen reproduces the original in-order-anchored window.
        let floor = self.code_base.max(self.base_gen);
        let hi = (floor + self.pipeline).min(top + 1);
        if hi <= floor {
            return None;
        }
        let span = hi - floor;
        // Round-robin over CODEABLE generations (sealed/tail-idle and still under
        // their provisioning budget). Once a generation is at budget it drops out
        // of the proactive round-robin; its residual, if any, is recovered by the
        // deficit loop (fungible cross-path, no per-seq ARQ).
        for _ in 0..span {
            if self.rr < floor || self.rr >= hi {
                self.rr = floor;
            }
            let g = self.rr;
            self.rr += 1;
            if self.codeable(g) {
                return Some(g);
            }
        }
        None
    }

    /// Whether generation `g` is eligible for FILLING-generation proactive
    /// coding: it has at least one retained source and is still under its
    /// per-generation provisioning budget `ceil(len·factor)` (systematic
    /// `factor = r`). Unlike `codeable`, this does NOT require the generation to
    /// be sealed — the whole point of the pacer is to emit repair over the
    /// contiguous prefix while the generation is STILL FILLING, so the covering
    /// equation is present at the receiver ~immediately after the hole is sent
    /// (before/around when the in-order frontier detects it), not a full
    /// generation-span later once the generation finally seals.
    fn codeable_filling(&self, g: u64) -> bool {
        if !self.sources.contains_key(&(g * self.gen_size)) {
            return false;
        }
        let emitted = self.emitted.get(&g).copied().unwrap_or(0);
        emitted < self.gen_budget(g)
    }

    /// Code ONE proactive symbol over the retained contiguous prefix of
    /// generation `g` at the FULL generation MATRIX width `G`. The symbol sums
    /// only the present prefix `[gen_start, gen_start + w)` (w = current fill)
    /// with coefficients drawn from the full-width seed, and carries `w` as the
    /// wire `coded_width` (with `FILL_FLAG` set) so the decoder zeroes columns
    /// `[w, G)`. Because the wire `window_count` is `G` regardless of `w`, every
    /// symbol for `g` — filling or sealed, proactive or reactive — lands in the
    /// SAME `(anchor, G)` decoder matrix and combines fungibly. This is what
    /// makes filling-generation repair present-at-stall WITHOUT the cross-grid
    /// stranding that refuted the separate-block inline repair.
    fn code_generation_full(&mut self, g: u64) -> WireSymbol {
        let symbol_size = self.symbol_size as usize;
        let coded_index = self.coded_index;
        self.coded_index += 1;
        *self.emitted.entry(g).or_insert(0) += 1;
        let gen_start = g * self.gen_size;
        let (_s, syms) = self.generation_symbols(g); // contiguous present prefix
        let coded_width = syms.len() as u16;
        let full = self.gen_size as u16; // stable MATRIX width

        // Coefficients over the FULL generation span [gen_start, gen_start+G);
        // only the first `coded_width` are actually applied (the rest map to
        // not-yet-generated seqs and are zero on both sides).
        let coeffs = generate_window_coefficients(gen_start, full, coded_index);
        let mut coded = vec![0u8; symbol_size];
        for (i, src) in syms.iter().enumerate() {
            gf256::mul_acc_slice(coeffs[i], src, &mut coded);
        }

        let wire_index = coded_index | FILL_FLAG;
        let mut wire_data = Vec::with_capacity(REPAIR_HEADER_SIZE + 2 + symbol_size);
        wire_data.extend_from_slice(&gen_start.to_le_bytes()); // 8: anchor
        wire_data.extend_from_slice(&full.to_le_bytes()); // 2: matrix width = G
        wire_data.extend_from_slice(&wire_index.to_le_bytes()); // 4: index | FILL_FLAG
        wire_data.extend_from_slice(&coded_width.to_le_bytes()); // 2: prefix width w
        wire_data.extend_from_slice(&coded);

        WireSymbol {
            block_id: gen_start + full.saturating_sub(1) as u64,
            payload_id: coded_index, // dedup uses the REAL index (no flag)
            is_repair: true,
            data: wire_data,
            backend: FecBackend::Rlc,
        }
    }

    /// Pick the next generation to code via the FILLING pacer: round-robin over
    /// the `pipeline` oldest retained generations that are still under budget
    /// (filling OR sealed). The oldest retained generations are exactly the ones
    /// the receiver's in-order frontier is at (retention floor = cumulative ack),
    /// so their coded repair is what must be present when the frontier stalls.
    fn next_fill_gen(&mut self) -> Option<u64> {
        let top = self.top_gen();
        let floor = self.code_base.max(self.base_gen);
        let hi = (floor + self.pipeline).min(top + 1);
        if hi <= floor {
            return None;
        }
        let span = hi - floor;
        for _ in 0..span {
            if self.fill_rr < floor || self.fill_rr >= hi {
                self.fill_rr = floor;
            }
            let g = self.fill_rr;
            self.fill_rr += 1;
            if self.codeable_filling(g) {
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

    fn generate_repair_filling(&mut self) -> WireSymbol {
        match self.next_fill_gen() {
            Some(g) => self.code_generation_full(g),
            None => WireSymbol {
                block_id: 0,
                payload_id: self.coded_index,
                is_repair: true,
                data: vec![0u8; REPAIR_HEADER_SIZE + self.symbol_size as usize],
                backend: FecBackend::Rlc,
            },
        }
    }

    fn wants_filling_coding(&self) -> bool {
        let top = self.top_gen();
        let floor = self.code_base.max(self.base_gen);
        let hi = (floor + self.pipeline).min(top + 1);
        if hi <= floor {
            return false;
        }
        (floor..hi).any(|g| self.codeable_filling(g))
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

    /// Interspersed trailing-window repair (goal-gate "Repair In-Flight"). Code
    /// ONE coded symbol over the ARBITRARY seq range `[start, start+count)` — a
    /// small trailing BLOCK of already-sent source, distinct from (and smaller
    /// than) the fixed generation. Unlike `generate_repair` (which round-robins
    /// SEALED generations, so a generation's repair only flows AFTER all G of its
    /// sources are sent → arrives ~1 generation-span behind), this codes over a
    /// block whose members were ALL just sent, so the covering repair arrives
    /// ~immediately after the hole it covers — present when the receiver detects
    /// the hole → proactive decode, no reactive round-trip. The wire header
    /// carries `(start, count, coded_index)`, so the dense decoder solves it in a
    /// `(start,count)` matrix exactly like a generation of that span (with the raw
    /// sources injected as unit pivots). Returns `None` unless the FULL range is
    /// retained — a missing source would make the coded equation inconsistent with
    /// the receiver's regenerated coefficients.
    fn generate_repair_range(&mut self, start: u64, count: u16) -> Option<WireSymbol> {
        if count == 0 {
            return None;
        }
        let width = count as u64;
        for seq in start..start + width {
            if !self.sources.contains_key(&seq) {
                return None;
            }
        }
        let symbol_size = self.symbol_size as usize;
        let coded_index = self.coded_index;
        self.coded_index += 1;

        let coeffs = generate_window_coefficients(start, count, coded_index);
        let mut coded = vec![0u8; symbol_size];
        for i in 0..width {
            let src = self.sources.get(&(start + i)).expect("range checked present");
            gf256::mul_acc_slice(coeffs[i as usize], src, &mut coded);
        }

        let mut wire_data = Vec::with_capacity(REPAIR_HEADER_SIZE + symbol_size);
        wire_data.extend_from_slice(&start.to_le_bytes());
        wire_data.extend_from_slice(&count.to_le_bytes());
        wire_data.extend_from_slice(&coded_index.to_le_bytes());
        wire_data.extend_from_slice(&coded);

        Some(WireSymbol {
            block_id: start + width - 1,
            payload_id: coded_index,
            is_repair: true,
            data: wire_data,
            backend: FecBackend::Rlc,
        })
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
        // Fix 3: the coding floor never trails the retention floor.
        if self.code_base < self.base_gen {
            self.code_base = self.base_gen;
        }
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

    fn set_code_base(&mut self, anchor_seq: u64) {
        // Advance the proactive-coding floor toward the SEND frontier, clamped
        // to [retention floor, top gen]. Monotonic (never retreats) so the
        // round-robin stays ahead of already-provisioned generations.
        let g = self.gen_of(anchor_seq);
        let top = self.top_gen();
        let want = g.clamp(self.base_gen, top);
        if want > self.code_base {
            self.code_base = want;
            if self.rr < self.code_base {
                self.rr = self.code_base;
            }
        }
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

    fn set_pipeline_depth(&mut self, m: usize) {
        self.pipeline = (m.max(1)) as u64;
    }

    fn wants_coding(&self) -> bool {
        let top = self.top_gen();
        let floor = self.code_base.max(self.base_gen);
        let hi = (floor + self.pipeline).min(top + 1);
        if hi <= floor {
            return false;
        }
        (floor..hi).any(|g| self.codeable(g))
    }
}

// ===========================================================================
// Sparse-aware generation decoder — the FAST decode path for generation coding.
// ===========================================================================
//
// WHY THIS EXISTS.  The generation *encoder* above produces RLC-repair symbols
// with the identical self-describing wire header as the sliding window, so the
// sparse `RlcWindowDecoder` CAN decode them (and does, in the unit tests).  But
// that decoder stores each pivot row's coefficients as a `BTreeMap<u64,u8>` and
// cascades single-unknown resolutions one at a time — allocation-heavy and
// pointer-chasing, measured ~200× below a dense GF(256) solver.  At the oracle's
// aggregating G=384 that put decode BELOW the link rate, so heterogeneous
// multipath had no headroom to aggregate (goal-gate "Generation Coding": C8 =
// 10.97 Mbit/s, aggregation factor 1.00 — DECODE-BOUND, not network-bound).
//
// THE COST MODEL (goal-gate "Decode-CPU Ceiling").  The previous dense decoder
// (kept verbatim in `reference` below) materialized EVERY known source as a
// full-width unit pivot row and reduced every incoming row against ALL of them
// with fused (G+S)-byte SIMD ops: O(G·(G+S)) per row even when the row's only
// job was to eliminate already-known sources.  In systematic mode — where G−k
// of a generation's DoF arrive as raw source and only k ≈ ε·G repair rows carry
// new information — that turned an O(k·G·S + k³) problem into O(G²·S)-class
// work (~90 ms CPU per 384-symbol generation ≈ 4 000 sym/s ≈ 39 Mbit/s/core on
// the L1 VM), which bound the whole generation transport at ~34 Mbit/s.
//
// THIS decoder is sparse-aware and per-generation:
//   • KNOWN sources never enter the matrix.  A generation slot keeps a `known`
//     bitmap; an incoming row's known columns are eliminated PAYLOAD-ONLY
//     (S bytes per known column, against `recovered`) instead of via fused
//     (G+S)-byte unit-row ops — and slot creation stores NO payload copies.
//   • Only CODED rows are matrix rows (`pivots`, ≤ k + deficit extras in
//     systematic mode), kept in reduced row-echelon form over the non-known
//     columns exactly as before, using the same fused-row SIMD elimination.
//   • A pivot row that reduces to a UNIT row is delivered immediately and
//     CONVERTED to a `known` column (dropped from the matrix), so the active
//     system stays k×k.  Completion is `known_count == width` — the moment the
//     last column is known the whole generation has been delivered, with no
//     separate full-rank back-substitution pass (the RREF invariant makes the
//     final sweep deliver every remaining row; see `insert_equation`).
//   • A generation whose span is FULLY recovered before its first repair
//     arrives (k = 0, the common case at low ε) never creates a matrix at all:
//     the repair is recognized as redundant in O(G) with zero GF work.
// Per generation the cost is O(k·G·S + k²·(G+S)): the irreducible payload-only
// elimination of the known mass from each of ~k dense repair rows, plus the
// k×k active elimination.  In coded-only mode (no raw source on the wire)
// nothing is ever known, every row is a coded pivot, and the arithmetic
// degenerates to exactly the previous dense decoder's O(G²·(G+S)) — that mode's
// cost is information-theoretic (all G DoF arrive dense), not implementation.
//
// DELIVERED OUTPUT is the same (seq, payload) SET with identical bytes as the
// reference decoder on any consistent symbol stream (asserted by the
// old-vs-new differential test below); the only observable difference is the
// ORDER of seqs *within* one `add_symbol` return on the call that completes a
// generation (incremental sweep order vs the old ascending full-rank sweep).
// Every consumer keys on seq (reassembly/reorder buffers), so ordering within
// a call is semantically inert.

/// One reduced CODED pivot row of a generation's system, stored as ONE
/// contiguous buffer `[coeffs (width bytes) | data (symbol_size bytes)]`.
/// Fusing the coefficient row and the payload row into a single allocation lets
/// a single SIMD `mul_acc_slice` eliminate BOTH in one call.  After
/// normalization the pivot column holds 1 and, by the RREF invariant, every
/// OTHER pivot column (and every `known` column) holds 0.
type GenRow = Vec<u8>;

/// State of one generation's decode, keyed by `(anchor, width)` — see
/// `GenerationDecoder::gens`.
enum GenSlot {
    /// Still accumulating independent degrees of freedom.
    Solving {
        width: usize,
        /// Source-KNOWN columns.  `known[c]` ⇒ the source payload for
        /// `anchor + c` lives in `GenerationDecoder::recovered` and every
        /// matrix row is zero at column `c` (incoming rows are reduced
        /// payload-only against `recovered` before insertion).  Known columns
        /// are never materialized as matrix rows — this is the sparse-aware
        /// core: the G−k known DoF cost O(S) each instead of a fused
        /// (G+S)-byte row op against a stored unit row.
        known: Vec<bool>,
        /// Number of `true` entries in `known`.
        known_count: usize,
        /// `pivots[c]` is the reduced CODED row whose pivot column is `c`
        /// (or `None`).  Only coded rows live here (≤ holes + deficit margin
        /// in systematic mode); a row that becomes UNIT is delivered and
        /// converted to a `known` column immediately.
        pivots: Vec<Option<GenRow>>,
        /// Number of `Some` entries in `pivots`.  The generation's rank is
        /// `known_count + coded_rows`.
        coded_rows: usize,
    },
    /// Fully decoded and delivered — further coded symbols for it are redundant.
    Done,
}

/// Sparse-aware per-generation RLC decoder.  Drop-in `WindowDecoder` used in
/// generation mode in place of the sparse `RlcWindowDecoder`.
pub struct GenerationDecoder {
    symbol_size: usize,
    /// `(anchor, width)` → decode state.  Keying by BOTH anchor and width (not
    /// anchor alone) is load-bearing: the object stream reuses the absolute seq
    /// space across objects, so a single anchor legitimately hosts DIFFERENT
    /// generations of different K_G at different times — the encoder's fixed
    /// generation `g` accumulates one object's short tail (coded at that partial
    /// width once intake goes idle) AND the next object's fill (coded at the full
    /// width once sealed).  Those are distinct linear systems over different
    /// source sets; keying by `(anchor, width)` lets them coexist instead of one
    /// resetting/thrashing the other's pivots at the object boundary.
    gens: BTreeMap<(u64, usize), GenSlot>,
    /// Sources already recovered: seq → payload.  Three jobs: (1) the
    /// delivered-seq set (its keys) for dedup and `rank_in`; (2) known-source
    /// ELIMINATION — a fresh generation slot marks every already-recovered
    /// source in its span `known`, and incoming rows are reduced against these
    /// payloads directly (payload-only, no unit pivot rows); (3) the payload
    /// store those eliminations read from — which is why `advance` never prunes
    /// a seq still covered by a live Solving slot (the old dense decoder held
    /// private copies in its unit rows; this one holds none).
    recovered: BTreeMap<u64, Vec<u8>>,
    /// Wire-symbol dedup: (block_id, payload_id, is_repair).
    seen: HashSet<(u64, u32, bool)>,
    total_fed: u64,
    repairs_fed: u64,
    repairs_useful: u64,
}

impl GenerationDecoder {
    pub fn new(symbol_size: u16) -> Self {
        Self {
            symbol_size: symbol_size as usize,
            gens: BTreeMap::new(),
            recovered: BTreeMap::new(),
            seen: HashSet::new(),
            total_fed: 0,
            repairs_fed: 0,
            repairs_useful: 0,
        }
    }

    /// Feed one fused equation row (`[coeffs (width) | data (symbol_size)]`)
    /// into a generation's system.  Returns `(added_rank, delivered)`:
    /// `added_rank` is true iff the row contributed a new independent degree of
    /// freedom (the honest "useful" signal, counted per rank-add); `delivered`
    /// is every source this row's information newly resolved — incrementally
    /// (unit rows deliver the instant they isolate) AND at completion (the last
    /// row's sweep delivers the rest; no separate full-rank pass exists).
    fn insert_equation(
        &mut self,
        anchor: u64,
        width: usize,
        mut row: GenRow,
    ) -> (bool, Vec<(u64, Bytes)>) {
        let ss = self.symbol_size;
        if !self.gens.contains_key(&(anchor, width)) {
            // Fresh generation: mark already-recovered sources in its span as
            // KNOWN columns (flags only — no payload copies, no unit rows).
            // k = 0 fast path: a span that is already fully recovered needs no
            // matrix at all — the row is necessarily dependent (every column
            // eliminates against a known source), so skip slot creation and
            // all GF work.  This is the common case at low loss: a complete
            // generation's proactive repairs cost O(width) each, not O(G·S).
            let have = self.recovered.range(anchor..anchor + width as u64).count();
            if have == width {
                return (false, vec![]);
            }
            let mut known = vec![false; width];
            for (&seq, _) in self.recovered.range(anchor..anchor + width as u64) {
                known[(seq - anchor) as usize] = true;
            }
            self.gens.insert(
                (anchor, width),
                GenSlot::Solving {
                    width,
                    known,
                    known_count: have,
                    pivots: (0..width).map(|_| None).collect(),
                    coded_rows: 0,
                },
            );
        }
        let slot = self.gens.get_mut(&(anchor, width)).expect("just inserted or present");

        let (known, known_count, pivots, coded_rows, width) = match slot {
            // Done ⇒ this generation already delivered; symbol redundant.
            GenSlot::Done => return (false, vec![]),
            GenSlot::Solving { known, known_count, pivots, coded_rows, width } => {
                (known, known_count, pivots, coded_rows, *width)
            }
        };

        // Forward-reduce the incoming row.  KNOWN columns are eliminated
        // payload-only against `recovered` (S bytes, the sparse-aware saving);
        // coded pivot columns use the fused (width+S)-byte row op.  Because the
        // coded rows are in RREF over the non-known columns (and zero at every
        // known column), a single left-to-right pass fully reduces the row.
        for c in 0..width {
            let factor = row[c];
            if factor == 0 {
                continue;
            }
            if known[c] {
                let src = self
                    .recovered
                    .get(&(anchor + c as u64))
                    .expect("known column ⇒ payload retained while the slot lives");
                gf256::mul_acc_slice(factor, src, &mut row[width..]);
                row[c] = 0;
            } else if let Some(prow) = &pivots[c] {
                gf256::mul_acc_slice(factor, prow, &mut row);
            }
        }

        // First surviving nonzero coefficient is the new pivot column.
        let pcol = match row[..width].iter().position(|&x| x != 0) {
            Some(c) => c,
            None => return (false, vec![]), // linearly dependent — no new information
        };

        // Normalize so the pivot coefficient is 1 (whole fused row at once).
        let lead = row[pcol];
        if lead != 1 {
            scale_inplace(gf256::inv(lead), &mut row);
        }

        // Gauss–Jordan: eliminate the new pivot column from every existing
        // coded row so the RREF invariant is preserved.  Track which rows we
        // MODIFY: only those (plus the new pivot row) can have newly become
        // UNIT rows.
        let mut touched: Vec<usize> = Vec::new();
        for c in 0..width {
            if c == pcol {
                continue;
            }
            if let Some(other) = pivots[c].as_mut() {
                let f = other[pcol];
                if f != 0 {
                    gf256::mul_acc_slice(f, &row, other);
                    touched.push(c);
                }
            }
        }

        pivots[pcol] = Some(row);
        *coded_rows += 1;
        touched.push(pcol);

        // UNIT sweep: a touched row whose coefficient half has a single nonzero
        // (its own pivot, normalized to 1) IS its column's source.  Deliver it,
        // mark the column KNOWN, and DROP the row — the matrix stays k×k and the
        // RREF invariant is untouched (every other row is already zero at a
        // pivot column).  When the last column turns known the generation is
        // COMPLETE: by RREF, pivots over ALL remaining free columns force every
        // remaining row to be unit, so this same sweep delivers the whole tail —
        // the old dense decoder's full-rank pass, folded into the increment.
        let mut out: Vec<(u64, Bytes)> = Vec::new();
        for &c in &touched {
            let Some(prow) = pivots[c].as_ref() else { continue };
            // Early-exit nonzero count: dense rows bail at the second nonzero.
            let mut nz = 0u32;
            for &x in &prow[..width] {
                if x != 0 {
                    nz += 1;
                    if nz > 1 {
                        break;
                    }
                }
            }
            if nz != 1 {
                continue;
            }
            let mut sym = pivots[c].take().expect("checked Some above");
            *coded_rows -= 1;
            known[c] = true;
            *known_count += 1;
            sym.drain(..width);
            sym.truncate(ss);
            let seq = anchor + c as u64;
            // Deliver each seq exactly once (a source another slot/path already
            // recovered must not re-deliver).
            if !self.recovered.contains_key(&seq) {
                self.recovered.insert(seq, sym.clone());
                out.push((seq, Bytes::from(sym)));
            }
        }

        if *known_count == width {
            *slot = GenSlot::Done;
        }
        (true, out)
    }

    /// Inject an already-received RAW source (seq → payload) as a unit equation
    /// into EVERY existing Solving generation matrix whose fixed span covers
    /// `seq`.
    ///
    /// WHY THIS EXISTS (feat/fec-recovery-bug — the proactive-FEC-dead bug). A
    /// generation's decode matrix learns the sources it already knows ONLY at
    /// slot creation (the first repair for that generation). In production, source
    /// and repair symbols INTERLEAVE and reorder, so a generation's own non-lost
    /// sources routinely arrive AFTER its first repair. Without this injection
    /// those late sources land in `recovered` but are invisible to the matrix,
    /// which then treats them as permanent unknowns: `rank_in` reports a deficit of
    /// `G − matrix_rank` (inflated by the late-source count, NOT the true hole
    /// count), the sender floods `G − rank` coded repairs where only `holes` were
    /// needed, and the surplus repairs merely re-derive already-received sources —
    /// linearly wasted (the measured repairs_useful ≈ 7 / repairs_fed ≈ 4600). By
    /// feeding each late source into the live matrix as the unit equation
    /// `e_c · x = data` (c = seq − anchor), the unknown space shrinks to the real
    /// holes the instant the source arrives, so the reported deficit == holes and
    /// coded repair actually recovers holes proactively. If the injection is the
    /// last missing degree of freedom it completes the generation and returns its
    /// remaining holes.
    ///
    /// COST (sparse-aware): the injected unit row reduces against nothing (its
    /// column is free), pivots at `c`, and the elimination touches only the ≤ k
    /// coded rows with a nonzero at `c` — O(k·S), not the old O(k·(G+S)) plus a
    /// full-width unit-row insert.  A column that is already `known` is skipped
    /// before any row is built.
    fn inject_source_into_active_gens(&mut self, seq: u64, data: &[u8]) -> Vec<(u64, Bytes)> {
        let ss = self.symbol_size;
        // Collect covering Solving slots first (avoid aliasing the &mut self used
        // by insert_equation). A source is covered by slot (anchor,width) iff
        // anchor ≤ seq < anchor+width. Widths are small in count, so this is cheap.
        // Skip slots that already KNOW this column — the unit equation would be
        // linearly dependent (the old decoder discovered that with a fused row op;
        // the known bitmap answers it for free).
        let covering: Vec<(u64, usize)> = self
            .gens
            .iter()
            .filter(|(&(anchor, width), slot)| {
                match slot {
                    GenSlot::Solving { known, .. } => {
                        anchor <= seq
                            && seq < anchor + width as u64
                            && !known[(seq - anchor) as usize]
                    }
                    _ => false,
                }
            })
            .map(|(&k, _)| k)
            .collect();
        let mut out = Vec::new();
        for (anchor, width) in covering {
            let c = (seq - anchor) as usize;
            let mut row = vec![0u8; width + ss];
            row[c] = 1;
            let n = data.len().min(ss);
            row[width..width + n].copy_from_slice(&data[..n]);
            // insert_equation reduces the unit row against the matrix: if column
            // c is already resolved it is linearly dependent (no-op); otherwise it
            // becomes the pivot for c, shrinking the deficit by one.
            let (_added, delivered) = self.insert_equation(anchor, width, row);
            out.extend(delivered);
        }
        out
    }

    /// Transitively propagate just-delivered sources into EVERY other active
    /// generation matrix, returning all sources delivered along the way.
    ///
    /// WHY (goal-gate "Repair In-Flight"). Two coding grids now coexist: the small
    /// inline trailing-BLOCK (width W, the in-flight proactive channel) and the
    /// wide GENERATION (width G, the reactive deficit loop's unit). A hole
    /// recovered by a block repair lands in `recovered`, but the covering G-matrix
    /// (created later, by a deficit repair) only learns `recovered` AT CREATION
    /// and never after — so a hole recovered by the block AFTER the G-matrix
    /// exists stays an unknown in it, `rank_in(G)` under-counts, the receiver
    /// OVER-reports the generation's deficit, and the sender FLOODS redundant
    /// reactive repair (MEASURED recovery_coded 30k→94k, pfrac collapse). Feeding
    /// each block-recovered hole into the G-matrix (as a unit equation) keeps
    /// every matrix's rank consistent, so the deficit reflects the true residual
    /// and the reactive flood is eliminated. A worklist handles the transitive
    /// case (a G-matrix a block completion finishes delivers its own holes,
    /// propagated in turn); each seq is delivered at most once (guarded by
    /// `recovered`), so it terminates.
    fn propagate(&mut self, initial: Vec<(u64, Bytes)>) -> Vec<(u64, Bytes)> {
        let mut all: Vec<(u64, Bytes)> = Vec::new();
        let mut queue: std::collections::VecDeque<(u64, Bytes)> = initial.into_iter().collect();
        while let Some((seq, data)) = queue.pop_front() {
            let more = self.inject_source_into_active_gens(seq, &data);
            all.push((seq, data));
            for m in more {
                queue.push_back(m);
            }
        }
        all
    }
}

/// Scale a byte slice in place by a GF(256) scalar. Scalar path is used only for
/// the O(width) per-pivot normalization, negligible against the O(width²)
/// elimination that runs on the SIMD kernel.
#[inline]
fn scale_inplace(coeff: u8, buf: &mut [u8]) {
    if coeff == 1 {
        return;
    }
    for b in buf.iter_mut() {
        *b = gf256::mul(coeff, *b);
    }
}

impl WindowDecoder for GenerationDecoder {
    fn add_symbol(&mut self, symbol: &WireSymbol) -> Vec<(u64, Bytes)> {
        if symbol.backend != FecBackend::Rlc {
            return vec![];
        }
        let key = (symbol.block_id, symbol.payload_id, symbol.is_repair);
        if !self.seen.insert(key) {
            return vec![];
        }
        self.total_fed += 1;

        if !symbol.is_repair {
            // SYSTEMATIC mode: the raw source rides the wire as PRIMARY. Deliver it
            // directly (zero decode) and record it so overlapping generations can
            // eliminate it.
            let seq = symbol.block_id;
            if self.recovered.contains_key(&seq) {
                return vec![];
            }
            let mut data = vec![0u8; self.symbol_size];
            let copy_len = symbol.data.len().min(self.symbol_size);
            data[..copy_len].copy_from_slice(&symbol.data[..copy_len]);
            self.recovered.insert(seq, data.clone());
            // feat/fec-recovery-bug FIX: a source that arrives AFTER a covering
            // generation's matrix was created must be injected into that live
            // matrix as a unit equation — otherwise the matrix keeps treating it
            // as an unknown, inflating the reported deficit and wasting coded
            // repair (the proactive-FEC-dead bug). Inject now; if it completes a
            // generation, deliver that generation's remaining holes too.
            let mut out = vec![(seq, Bytes::from(data.clone()))];
            let delivered = self.inject_source_into_active_gens(seq, &data);
            out.extend(self.propagate(delivered));
            return out;
        }

        if symbol.data.len() < REPAIR_HEADER_SIZE {
            return vec![];
        }
        self.repairs_fed += 1;

        let anchor = u64::from_le_bytes(symbol.data[0..8].try_into().unwrap());
        let width = u16::from_le_bytes(symbol.data[8..10].try_into().unwrap()) as usize;
        let wire_index = u32::from_le_bytes(symbol.data[10..14].try_into().unwrap());
        if width == 0 {
            return vec![];
        }
        // FILLING-generation repair (FILL_FLAG): the sender summed only the
        // contiguous prefix [anchor, anchor+coded_width), but the matrix width is
        // the full generation `width` (= G). Read the 2-byte prefix width after
        // the 14-byte header and zero coefficient columns [coded_width, width).
        // The real coded-index (the coefficient seed) is the low 31 bits.
        let (repair_index, coded_width, header_end) = if wire_index & FILL_FLAG != 0 {
            if symbol.data.len() < REPAIR_HEADER_SIZE + 2 {
                return vec![];
            }
            let cw = u16::from_le_bytes(
                symbol.data[REPAIR_HEADER_SIZE..REPAIR_HEADER_SIZE + 2].try_into().unwrap(),
            ) as usize;
            (wire_index & !FILL_FLAG, cw.min(width), REPAIR_HEADER_SIZE + 2)
        } else {
            (wire_index, width, REPAIR_HEADER_SIZE)
        };
        let coded = &symbol.data[header_end..];

        // Build the fused row: [coeffs (width) | payload (symbol_size)]. For a
        // filling repair only the first `coded_width` coefficient columns are
        // populated; the rest stay zero (their seqs were not yet generated when
        // the sender coded this symbol), keeping the equation consistent while
        // still living in the full-width (anchor, G) system.
        let coeffs = generate_window_coefficients(anchor, width as u16, repair_index);
        let mut row = vec![0u8; width + self.symbol_size];
        row[..coded_width].copy_from_slice(&coeffs[..coded_width]);
        let copy_len = coded.len().min(self.symbol_size);
        row[width..width + copy_len].copy_from_slice(&coded[..copy_len]);

        let (added_rank, recovered) = self.insert_equation(anchor, width, row);
        // repairs_useful counts repairs that contributed a NEW degree of freedom
        // (rank-add), the honest per-hole "useful" signal — not per-generation
        // completions. With the late-source-injection fix the matrix's unknown
        // space is the real holes, so a useful repair == a hole recovered.
        if added_rank {
            self.repairs_useful += 1;
        }
        // Propagate any recovered holes into the OTHER coding grid's matrices
        // (block ↔ generation) so every matrix's rank is consistent and the
        // reactive deficit does not over-report holes the inline block already
        // recovered — see `propagate`.
        self.propagate(recovered)
    }

    fn advance(&mut self, oldest_seq: u64) {
        // Drop whole generations that end at or before the retention frontier.
        let drop: Vec<(u64, usize)> = self
            .gens
            .keys()
            .filter(|&&(anchor, width)| anchor + width as u64 <= oldest_seq)
            .copied()
            .collect();
        for k in drop {
            self.gens.remove(&k);
        }
        // Prune recovered payloads below the frontier — EXCEPT the span of any
        // surviving Solving slot: its `known` columns eliminate against these
        // payloads (the old dense decoder held private copies inside its unit
        // pivot rows; this decoder holds none, so the store must outlive the
        // slot).  Memory is the same order either way — one payload per known
        // column per live span — and a live slot's span is bounded (slots drop
        // above the moment their whole span passes the frontier).
        let live_floor = self
            .gens
            .iter()
            .filter(|(_, slot)| matches!(slot, GenSlot::Solving { .. }))
            .map(|(&(anchor, _), _)| anchor)
            .min()
            .unwrap_or(u64::MAX);
        let prune_to = oldest_seq.min(live_floor);
        let old: Vec<u64> = self.recovered.range(..prune_to).map(|(&k, _)| k).collect();
        for s in old {
            self.recovered.remove(&s);
        }
        self.seen.retain(|(block_id, _, _)| *block_id >= oldest_seq);
    }

    fn rank_in(&self, start: u64, count: u64) -> u64 {
        // Deficit feedback asks about the generation of exactly `count` = K_g.
        match self.gens.get(&(start, count as usize)) {
            Some(GenSlot::Done) => count,
            Some(GenSlot::Solving { known_count, coded_rows, .. }) => {
                (*known_count + *coded_rows) as u64
            }
            None => {
                // No matrix yet: count any already-recovered sources in the span
                // (they mark the generation `known` the moment its first coded
                // symbol arrives).
                let end = start.saturating_add(count);
                self.recovered.range(start..end).count() as u64
            }
        }
    }

    fn frontier_probe(&self, frontier: u64, horizon: u64) -> (u64, u64) {
        // Proactive-frontier diagnosis (RWM_FDIAG, `present_at_stall`). Span is
        // contiguous in seq space, so holes = span_len − recovered_in_span.
        // `buffered` = coded degrees of freedom already covering the span: coded
        // pivot rows in any Solving matrix whose pivot column maps to a seq in
        // the span that is NOT yet a recovered source.  (Known columns ARE
        // recovered sources, so — exactly as in the old dense decoder, where
        // unit pivots were filtered by the `recovered` check — only coded rows
        // count.)  A pivot at a hole column is a coded DoF that has advanced
        // into hole territory and will complete the hole once enough accumulate.
        // `buffered > 0` at a stall ⇒ proactive repair is PRESENT and the hole
        // will decode without a reactive round-trip (the in-flight win).
        let end = horizon.saturating_add(1);
        if end <= frontier {
            return (0, 0);
        }
        let span = end - frontier;
        let recovered = self.recovered.range(frontier..end).count() as u64;
        let holes = span.saturating_sub(recovered);
        let mut buffered = 0u64;
        for (&(anchor, width), slot) in &self.gens {
            if let GenSlot::Solving { pivots, .. } = slot {
                if anchor >= end || anchor + width as u64 <= frontier {
                    continue; // matrix does not overlap the probe span
                }
                for (c, p) in pivots.iter().enumerate() {
                    if p.is_some() {
                        let seq = anchor + c as u64;
                        if seq >= frontier && seq < end && !self.recovered.contains_key(&seq) {
                            buffered += 1;
                        }
                    }
                }
            }
        }
        (holes, buffered)
    }

    fn total_fed(&self) -> u64 {
        self.total_fed
    }
    fn repairs_fed(&self) -> u64 {
        self.repairs_fed
    }
    fn repairs_useful(&self) -> u64 {
        self.repairs_useful
    }
}


// ===========================================================================
// REFERENCE decoder (pre-sparse rewrite) -- differential-test oracle only.
// ===========================================================================

/// Byte-exact copy of the dense `GenerationDecoder` as of commit 02d240c,
/// KEPT ONLY as the oracle for the old-vs-new differential test and the
/// old-vs-new L0 micro-bench (`tests/gen_decode_bench.rs`). Never constructed
/// by the engine. Do not modify: its value is that it preserves the exact
/// pre-rewrite behaviour.
#[doc(hidden)]
#[allow(dead_code)]
pub mod reference {
    use super::*;
    use std::collections::HashSet;

    type GenRow = Vec<u8>;

    /// State of one generation's decode, keyed by `(anchor, width)` — see
    /// `RefGenerationDecoder::gens`.
    enum GenSlot {
        /// Still accumulating independent degrees of freedom.
        Solving {
            width: usize,
            /// `pivots[c]` is the reduced row whose pivot column is `c` (or `None`).
            pivots: Vec<Option<GenRow>>,
            rank: usize,
        },
        /// Fully decoded and delivered — further coded symbols for it are redundant.
        Done,
    }

    /// Dense per-generation RLC decoder.  Drop-in `WindowDecoder` used in generation
    /// mode in place of the sparse `RlcWindowDecoder`.
    pub struct RefGenerationDecoder {
        symbol_size: usize,
        /// `(anchor, width)` → decode state.  Keying by BOTH anchor and width (not
        /// anchor alone) is load-bearing: the object stream reuses the absolute seq
        /// space across objects, so a single anchor legitimately hosts DIFFERENT
        /// generations of different K_G at different times — the encoder's fixed
        /// generation `g` accumulates one object's short tail (coded at that partial
        /// width once intake goes idle) AND the next object's fill (coded at the full
        /// width once sealed).  Those are distinct linear systems over different
        /// source sets; keying by `(anchor, width)` lets them coexist instead of one
        /// resetting/thrashing the other's pivots at the object boundary.
        gens: BTreeMap<(u64, usize), GenSlot>,
        /// Sources already recovered: seq → payload.  Two jobs: (1) the delivered-seq
        /// set (its keys) for dedup and `rank_in`; (2) known-source ELIMINATION — when
        /// a fresh generation is created, every already-recovered source in its span is
        /// pre-loaded as a unit pivot row, so a coded symbol that introduces only ONE
        /// new unknown resolves it immediately (the sparse decoder's Step-1 behaviour).
        /// This is what lets a trickle channel that re-codes overlapping seqs at
        /// growing widths (e.g. the reverse per-object ACK stream: widths 1,2,3 over
        /// the same anchor) make progress instead of demanding a full-rank fresh solve
        /// each width.  For the common large-object case, generation spans are
        /// disjoint, so nothing is ever pre-known and this costs nothing.  Pruned on
        /// `advance`.
        recovered: BTreeMap<u64, Vec<u8>>,
        /// Wire-symbol dedup: (block_id, payload_id, is_repair).
        seen: HashSet<(u64, u32, bool)>,
        total_fed: u64,
        repairs_fed: u64,
        repairs_useful: u64,
    }

    impl RefGenerationDecoder {
        pub fn new(symbol_size: u16) -> Self {
            Self {
                symbol_size: symbol_size as usize,
                gens: BTreeMap::new(),
                recovered: BTreeMap::new(),
                seen: HashSet::new(),
                total_fed: 0,
                repairs_fed: 0,
                repairs_useful: 0,
            }
        }

        /// Feed one fused equation row (`[coeffs (width) | data (symbol_size)]`, pivot
        /// column at `width`-wide prefix) into a generation's Gauss–Jordan system.
        /// Returns `(added_rank, delivered)`: `added_rank` is true iff the row
        /// contributed a new independent degree of freedom (i.e. it was NOT linearly
        /// dependent on what the generation already knew — the honest "useful" signal,
        /// counted per rank-add not per generation-completion); `delivered` is the
        /// whole generation's sources the instant it reaches full rank, else empty.
        fn insert_equation(
            &mut self,
            anchor: u64,
            width: usize,
            mut row: GenRow,
        ) -> (bool, Vec<(u64, Bytes)>) {
            let ss = self.symbol_size;
            if !self.gens.contains_key(&(anchor, width)) {
                // Fresh generation: pre-load already-recovered sources in its span as
                // unit pivot rows (RREF form). Zero-cost when the span is disjoint from
                // everything recovered so far (the large-object common case).
                let mut pivots: Vec<Option<GenRow>> = (0..width).map(|_| None).collect();
                let mut rank = 0usize;
                for (c, slot) in pivots.iter_mut().enumerate() {
                    if let Some(data) = self.recovered.get(&(anchor + c as u64)) {
                        let mut prow = vec![0u8; width + ss];
                        prow[c] = 1;
                        let n = data.len().min(ss);
                        prow[width..width + n].copy_from_slice(&data[..n]);
                        *slot = Some(prow);
                        rank += 1;
                    }
                }
                self.gens.insert((anchor, width), GenSlot::Solving { width, pivots, rank });
            }
            let slot = self.gens.get_mut(&(anchor, width)).expect("just inserted or present");

            let (pivots, rank, width) = match slot {
                // Done ⇒ this generation already delivered; symbol redundant.
                GenSlot::Done => return (false, vec![]),
                GenSlot::Solving { pivots, rank, width } => (pivots, rank, *width),
            };

            // Forward-reduce the incoming row against existing pivots. Because the
            // system is in RREF, each pivot row is zero at every other pivot column,
            // so a single left-to-right pass fully reduces the row against all of
            // them (an elimination at column `c` can only touch NON-pivot columns).
            // ONE fused `mul_acc_slice` clears both the coefficient and the payload
            // halves of the row per pivot.
            for c in 0..width {
                let factor = row[c];
                if factor == 0 {
                    continue;
                }
                if let Some(prow) = &pivots[c] {
                    gf256::mul_acc_slice(factor, prow, &mut row);
                }
            }

            // First surviving nonzero coefficient is the new pivot column.
            let pcol = match row[..width].iter().position(|&x| x != 0) {
                Some(c) => c,
                None => return (false, vec![]), // linearly dependent — no new information
            };

            // Normalize so the pivot coefficient is 1 (whole fused row at once).
            let lead = row[pcol];
            if lead != 1 {
                scale_inplace(gf256::inv(lead), &mut row);
            }

            // Gauss–Jordan: eliminate the new pivot column from every existing pivot
            // row so the RREF invariant is preserved (each pivot column appears in
            // exactly one row). The new row is already zero at every existing pivot
            // column, so this never disturbs another row's pivot. Track which rows we
            // MODIFY: only those (plus the new pivot row) can have newly become UNIT
            // rows — a single-nonzero-coefficient row whose payload IS its source —
            // which enables INCREMENTAL delivery of a recovered hole BEFORE the whole
            // generation reaches full rank. That is the present-at-stall path for a
            // still-FILLING generation, whose matrix width `G` exceeds its current
            // fill so it would otherwise never reach full rank to deliver anything.
            let mut touched: Vec<usize> = Vec::new();
            for c in 0..width {
                if c == pcol {
                    continue;
                }
                if let Some(other) = pivots[c].as_mut() {
                    let f = other[pcol];
                    if f != 0 {
                        gf256::mul_acc_slice(f, &row, other);
                        touched.push(c);
                    }
                }
            }

            pivots[pcol] = Some(row);
            *rank += 1;
            touched.push(pcol);

            if *rank == width {
                // Full rank: every column is a pivot, so by RREF each pivot row is the
                // unit row for its column and its payload half IS the source symbol.
                let mut out = Vec::with_capacity(width);
                if let GenSlot::Solving { pivots, .. } =
                    std::mem::replace(slot, GenSlot::Done)
                {
                    for (c, prow) in pivots.into_iter().enumerate() {
                        let mut sym = prow.expect("full rank ⇒ every pivot present");
                        // Keep only the payload half.
                        sym.drain(..width);
                        sym.truncate(ss);
                        let seq = anchor + c as u64;
                        // Deliver each seq exactly once: a pre-loaded (already
                        // recovered) source is re-derived here but must not re-deliver.
                        if self.recovered.insert(seq, sym.clone()).is_none() {
                            out.push((seq, Bytes::from(sym)));
                        }
                    }
                }
                return (true, out);
            }

            // Sub-full rank (typically a still-FILLING generation): deliver any pivot
            // row that is now a UNIT row — its coefficient half is nonzero ONLY at its
            // pivot column, so its payload half IS the source — and whose seq has not
            // yet been delivered. Only `touched` rows can newly qualify. The matrix
            // stays Solving (its rows remain needed for elimination); `recovered`
            // guards single delivery per seq.
            let mut pending: Vec<(u64, Vec<u8>)> = Vec::new();
            for &c in &touched {
                if let Some(prow) = &pivots[c] {
                    if prow[..width].iter().filter(|&&x| x != 0).count() == 1 {
                        let seq = anchor + c as u64;
                        if !self.recovered.contains_key(&seq) {
                            let mut sym = prow[width..].to_vec();
                            sym.truncate(ss);
                            pending.push((seq, sym));
                        }
                    }
                }
            }
            let mut out = Vec::with_capacity(pending.len());
            for (seq, sym) in pending {
                if self.recovered.insert(seq, sym.clone()).is_none() {
                    out.push((seq, Bytes::from(sym)));
                }
            }
            (true, out)
        }

        /// Inject an already-received RAW source (seq → payload) as a unit pivot into
        /// EVERY existing Solving generation matrix whose fixed span covers `seq`.
        ///
        /// WHY THIS EXISTS (feat/fec-recovery-bug — the proactive-FEC-dead bug). A
        /// generation's decode matrix pre-loads the sources it already knows ONLY at
        /// slot creation (the first repair for that generation). In production, source
        /// and repair symbols INTERLEAVE and reorder, so a generation's own non-lost
        /// sources routinely arrive AFTER its first repair. Without this injection
        /// those late sources land in `recovered` but are invisible to the matrix,
        /// which then treats them as permanent unknowns: `rank_in` reports a deficit of
        /// `G − matrix_rank` (inflated by the late-source count, NOT the true hole
        /// count), the sender floods `G − rank` coded repairs where only `holes` were
        /// needed, and the surplus repairs merely re-derive already-received sources —
        /// linearly wasted (the measured repairs_useful ≈ 7 / repairs_fed ≈ 4600). By
        /// feeding each late source into the live matrix as the unit equation
        /// `e_c · x = data` (c = seq − anchor), the unknown space shrinks to the real
        /// holes the instant the source arrives, so the reported deficit == holes and
        /// coded repair actually recovers holes proactively. If the injection is the
        /// last missing degree of freedom it completes the generation and returns its
        /// remaining holes.
        fn inject_source_into_active_gens(&mut self, seq: u64, data: &[u8]) -> Vec<(u64, Bytes)> {
            let ss = self.symbol_size;
            // Collect covering Solving slots first (avoid aliasing the &mut self used
            // by insert_equation). A source is covered by slot (anchor,width) iff
            // anchor ≤ seq < anchor+width. Widths are small in count, so this is cheap.
            let covering: Vec<(u64, usize)> = self
                .gens
                .iter()
                .filter(|(&(anchor, width), slot)| {
                    matches!(slot, GenSlot::Solving { .. })
                        && anchor <= seq
                        && seq < anchor + width as u64
                })
                .map(|(&k, _)| k)
                .collect();
            let mut out = Vec::new();
            for (anchor, width) in covering {
                let c = (seq - anchor) as usize;
                let mut row = vec![0u8; width + ss];
                row[c] = 1;
                let n = data.len().min(ss);
                row[width..width + n].copy_from_slice(&data[..n]);
                // insert_equation reduces the unit row against the matrix: if column
                // c is already known it is linearly dependent (no-op); otherwise it
                // becomes the pivot for c, shrinking the deficit by one.
                let (_added, delivered) = self.insert_equation(anchor, width, row);
                out.extend(delivered);
            }
            out
        }

        /// Transitively propagate just-delivered sources into EVERY other active
        /// generation matrix, returning all sources delivered along the way.
        ///
        /// WHY (goal-gate "Repair In-Flight"). Two coding grids now coexist: the small
        /// inline trailing-BLOCK (width W, the in-flight proactive channel) and the
        /// wide GENERATION (width G, the reactive deficit loop's unit). A hole
        /// recovered by a block repair lands in `recovered`, but the covering G-matrix
        /// (created later, by a deficit repair) is only pre-loaded with `recovered`
        /// AT CREATION and never after — so a hole recovered by the block AFTER the
        /// G-matrix exists stays an unknown in it, `rank_in(G)` under-counts, the
        /// receiver OVER-reports the generation's deficit, and the sender FLOODS
        /// redundant reactive repair (MEASURED recovery_coded 30k→94k, pfrac
        /// collapse). Feeding each block-recovered hole into the G-matrix (as a unit
        /// pivot) keeps every matrix's rank consistent, so the deficit reflects the
        /// true residual and the reactive flood is eliminated. A worklist handles the
        /// transitive case (a G-matrix a block completion finishes delivers its own
        /// holes, propagated in turn); each seq is delivered at most once (guarded by
        /// `recovered`), so it terminates.
        fn propagate(&mut self, initial: Vec<(u64, Bytes)>) -> Vec<(u64, Bytes)> {
            let mut all: Vec<(u64, Bytes)> = Vec::new();
            let mut queue: std::collections::VecDeque<(u64, Bytes)> = initial.into_iter().collect();
            while let Some((seq, data)) = queue.pop_front() {
                let more = self.inject_source_into_active_gens(seq, &data);
                all.push((seq, data));
                for m in more {
                    queue.push_back(m);
                }
            }
            all
        }
    }

    /// Recover a fused row's `width` (coefficient count) given the payload size.
    #[inline]
    fn width_of(row: &[u8], symbol_size: usize) -> usize {
        row.len().saturating_sub(symbol_size)
    }

    /// Scale a byte slice in place by a GF(256) scalar. Scalar path is used only for
    /// the O(width) per-pivot normalization, negligible against the O(width²)
    /// elimination that runs on the SIMD kernel.
    #[inline]
    fn scale_inplace(coeff: u8, buf: &mut [u8]) {
        if coeff == 1 {
            return;
        }
        for b in buf.iter_mut() {
            *b = gf256::mul(coeff, *b);
        }
    }

    impl WindowDecoder for RefGenerationDecoder {
        fn add_symbol(&mut self, symbol: &WireSymbol) -> Vec<(u64, Bytes)> {
            if symbol.backend != FecBackend::Rlc {
                return vec![];
            }
            let key = (symbol.block_id, symbol.payload_id, symbol.is_repair);
            if !self.seen.insert(key) {
                return vec![];
            }
            self.total_fed += 1;

            if !symbol.is_repair {
                // SYSTEMATIC mode: the raw source rides the wire as PRIMARY. Deliver it
                // directly (zero decode) and record it so overlapping generations can
                // eliminate it.
                let seq = symbol.block_id;
                if self.recovered.contains_key(&seq) {
                    return vec![];
                }
                let mut data = vec![0u8; self.symbol_size];
                let copy_len = symbol.data.len().min(self.symbol_size);
                data[..copy_len].copy_from_slice(&symbol.data[..copy_len]);
                self.recovered.insert(seq, data.clone());
                // feat/fec-recovery-bug FIX: a source that arrives AFTER a covering
                // generation's matrix was created must be injected into that live
                // matrix as a unit pivot — otherwise the matrix keeps treating it as
                // an unknown, inflating the reported deficit and wasting coded repair
                // (the proactive-FEC-dead bug). Inject now; if it completes a
                // generation, deliver that generation's remaining holes too.
                let mut out = vec![(seq, Bytes::from(data.clone()))];
                let delivered = self.inject_source_into_active_gens(seq, &data);
                out.extend(self.propagate(delivered));
                return out;
            }

            if symbol.data.len() < REPAIR_HEADER_SIZE {
                return vec![];
            }
            self.repairs_fed += 1;

            let anchor = u64::from_le_bytes(symbol.data[0..8].try_into().unwrap());
            let width = u16::from_le_bytes(symbol.data[8..10].try_into().unwrap()) as usize;
            let wire_index = u32::from_le_bytes(symbol.data[10..14].try_into().unwrap());
            if width == 0 {
                return vec![];
            }
            // FILLING-generation repair (FILL_FLAG): the sender summed only the
            // contiguous prefix [anchor, anchor+coded_width), but the matrix width is
            // the full generation `width` (= G). Read the 2-byte prefix width after
            // the 14-byte header and zero coefficient columns [coded_width, width).
            // The real coded-index (the coefficient seed) is the low 31 bits.
            let (repair_index, coded_width, header_end) = if wire_index & FILL_FLAG != 0 {
                if symbol.data.len() < REPAIR_HEADER_SIZE + 2 {
                    return vec![];
                }
                let cw = u16::from_le_bytes(
                    symbol.data[REPAIR_HEADER_SIZE..REPAIR_HEADER_SIZE + 2].try_into().unwrap(),
                ) as usize;
                (wire_index & !FILL_FLAG, cw.min(width), REPAIR_HEADER_SIZE + 2)
            } else {
                (wire_index, width, REPAIR_HEADER_SIZE)
            };
            let coded = &symbol.data[header_end..];

            // Build the fused row: [coeffs (width) | payload (symbol_size)]. For a
            // filling repair only the first `coded_width` coefficient columns are
            // populated; the rest stay zero (their seqs were not yet generated when
            // the sender coded this symbol), keeping the equation consistent while
            // still living in the full-width (anchor, G) system.
            let coeffs = generate_window_coefficients(anchor, width as u16, repair_index);
            let mut row = vec![0u8; width + self.symbol_size];
            row[..coded_width].copy_from_slice(&coeffs[..coded_width]);
            let copy_len = coded.len().min(self.symbol_size);
            row[width..width + copy_len].copy_from_slice(&coded[..copy_len]);

            let (added_rank, recovered) = self.insert_equation(anchor, width, row);
            // repairs_useful counts repairs that contributed a NEW degree of freedom
            // (rank-add), the honest per-hole "useful" signal — not per-generation
            // completions. With the late-source-injection fix the matrix's unknown
            // space is the real holes, so a useful repair == a hole recovered.
            if added_rank {
                self.repairs_useful += 1;
            }
            // Propagate any recovered holes into the OTHER coding grid's matrices
            // (block ↔ generation) so every matrix's rank is consistent and the
            // reactive deficit does not over-report holes the inline block already
            // recovered — see `propagate`.
            self.propagate(recovered)
        }

        fn advance(&mut self, oldest_seq: u64) {
            // Drop whole generations that end at or before the retention frontier.
            let drop: Vec<(u64, usize)> = self
                .gens
                .keys()
                .filter(|&&(anchor, width)| anchor + width as u64 <= oldest_seq)
                .copied()
                .collect();
            for k in drop {
                self.gens.remove(&k);
            }
            let old: Vec<u64> = self.recovered.range(..oldest_seq).map(|(&k, _)| k).collect();
            for s in old {
                self.recovered.remove(&s);
            }
            self.seen.retain(|(block_id, _, _)| *block_id >= oldest_seq);
        }

        fn rank_in(&self, start: u64, count: u64) -> u64 {
            // Deficit feedback asks about the generation of exactly `count` = K_g.
            match self.gens.get(&(start, count as usize)) {
                Some(GenSlot::Done) => count,
                Some(GenSlot::Solving { rank, .. }) => *rank as u64,
                None => {
                    // No matrix yet: count any already-recovered sources in the span
                    // (they pre-load the generation the moment its first coded arrives).
                    let end = start.saturating_add(count);
                    self.recovered.range(start..end).count() as u64
                }
            }
        }

        fn frontier_probe(&self, frontier: u64, horizon: u64) -> (u64, u64) {
            // Proactive-frontier diagnosis (RWM_FDIAG, `present_at_stall`). Span is
            // contiguous in seq space, so holes = span_len − recovered_in_span.
            // `buffered` = coded degrees of freedom already covering the span: pivot
            // rows in any Solving matrix whose pivot column maps to a seq in the span
            // that is NOT yet a recovered source. After the raw sources are injected
            // as unit pivots, a coded equation reduces to a pivot at the FIRST free
            // (hole) column — so a pivot at a hole column is a coded DoF that has
            // advanced into hole territory and will complete the hole once enough
            // accumulate. `buffered > 0` at a stall ⇒ proactive repair is PRESENT and
            // the hole will decode without a reactive round-trip (the in-flight win).
            let end = horizon.saturating_add(1);
            if end <= frontier {
                return (0, 0);
            }
            let span = end - frontier;
            let recovered = self.recovered.range(frontier..end).count() as u64;
            let holes = span.saturating_sub(recovered);
            let mut buffered = 0u64;
            for (&(anchor, width), slot) in &self.gens {
                if let GenSlot::Solving { pivots, .. } = slot {
                    if anchor >= end || anchor + width as u64 <= frontier {
                        continue; // matrix does not overlap the probe span
                    }
                    for (c, p) in pivots.iter().enumerate() {
                        if p.is_some() {
                            let seq = anchor + c as u64;
                            if seq >= frontier && seq < end && !self.recovered.contains_key(&seq) {
                                buffered += 1;
                            }
                        }
                    }
                }
            }
            (holes, buffered)
        }

        fn total_fed(&self) -> u64 {
            self.total_fed
        }
        fn repairs_fed(&self) -> u64 {
            self.repairs_fed
        }
        fn repairs_useful(&self) -> u64 {
            self.repairs_useful
        }
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
}
