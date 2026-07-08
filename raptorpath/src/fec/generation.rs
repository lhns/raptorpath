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
// Dense generation decoder — the FAST decode path for generation coding.
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
// THIS decoder is dense and per-generation.  Every generation-coded symbol's
// coefficients lie inside ONE fixed generation span `[anchor, anchor+K_G)`, so a
// generation is a self-contained K_G×K_G system: we keep a DENSE coefficient row
// (`Vec<u8>` of length K_G, contiguous) per pivot and run Gauss–Jordan
// elimination over GF(256) using the SIMD `mul_acc_slice` kernel (the same kernel
// the encoder uses).  Reduced row-echelon is maintained incrementally, so when a
// generation reaches rank K_G every source is already isolated (identity rows)
// and delivered in one shot — no cascade, no back-substitution pass.  Decode is
// per-generation independent and out-of-order: a later generation decodes the
// instant it has K_G independent symbols, regardless of earlier ones.

/// One reduced pivot row of a generation's Gauss–Jordan system, stored as ONE
/// contiguous buffer `[coeffs (width bytes) | data (symbol_size bytes)]`.  Fusing
/// the coefficient row and the payload row into a single allocation lets a single
/// SIMD `mul_acc_slice` eliminate BOTH in one call — halving the per-call table-
/// build + dispatch overhead that dominates the O(W²) inner loop.  After
/// normalization the pivot column holds 1 and, by the RREF invariant, every OTHER
/// pivot column holds 0.
type GenRow = Vec<u8>;

/// State of one generation's decode, keyed by `(anchor, width)` — see
/// `GenerationDecoder::gens`.
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

    /// Feed one fused equation row (`[coeffs (width) | data (symbol_size)]`, pivot
    /// column at `width`-wide prefix) into a generation's Gauss–Jordan system.
    /// Returns the whole generation's sources the instant it reaches full rank,
    /// else empty.
    fn insert_equation(
        &mut self,
        anchor: u64,
        width: usize,
        mut row: GenRow,
    ) -> Vec<(u64, Bytes)> {
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
            GenSlot::Done => return vec![],
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
            None => return vec![], // linearly dependent — no new information
        };

        // Normalize so the pivot coefficient is 1 (whole fused row at once).
        let lead = row[pcol];
        if lead != 1 {
            scale_inplace(gf256::inv(lead), &mut row);
        }

        // Gauss–Jordan: eliminate the new pivot column from every existing pivot
        // row so the RREF invariant is preserved (each pivot column appears in
        // exactly one row). The new row is already zero at every existing pivot
        // column, so this never disturbs another row's pivot.
        for other in pivots.iter_mut().flatten() {
            let f = other[pcol];
            if f != 0 {
                gf256::mul_acc_slice(f, &row, other);
            }
        }

        pivots[pcol] = Some(row);
        *rank += 1;

        if *rank < width {
            return vec![];
        }

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
                // Deliver each seq exactly once: a pre-loaded (already recovered)
                // source is re-derived here but must not be re-delivered.
                if self.recovered.insert(seq, sym.clone()).is_none() {
                    out.push((seq, Bytes::from(sym)));
                }
            }
        }
        out
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
            // Generation mode is coded-only, so a raw source is unexpected; still,
            // deliver it directly (and record it so any overlapping generation can
            // eliminate it) to keep the trait correct if one ever arrives.
            let seq = symbol.block_id;
            if self.recovered.contains_key(&seq) {
                return vec![];
            }
            let mut data = vec![0u8; self.symbol_size];
            let copy_len = symbol.data.len().min(self.symbol_size);
            data[..copy_len].copy_from_slice(&symbol.data[..copy_len]);
            self.recovered.insert(seq, data.clone());
            return vec![(seq, Bytes::from(data))];
        }

        if symbol.data.len() < REPAIR_HEADER_SIZE {
            return vec![];
        }
        self.repairs_fed += 1;

        let anchor = u64::from_le_bytes(symbol.data[0..8].try_into().unwrap());
        let width = u16::from_le_bytes(symbol.data[8..10].try_into().unwrap()) as usize;
        let repair_index = u32::from_le_bytes(symbol.data[10..14].try_into().unwrap());
        if width == 0 {
            return vec![];
        }
        let coded = &symbol.data[REPAIR_HEADER_SIZE..];

        // Build the fused row: [coeffs (width) | payload (symbol_size)].
        let coeffs = generate_window_coefficients(anchor, width as u16, repair_index);
        let mut row = vec![0u8; width + self.symbol_size];
        row[..width].copy_from_slice(&coeffs);
        let copy_len = coded.len().min(self.symbol_size);
        row[width..width + copy_len].copy_from_slice(&coded[..copy_len]);

        let recovered = self.insert_equation(anchor, width, row);
        if !recovered.is_empty() {
            self.repairs_useful += 1;
        }
        recovered
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
}
