//! Generation-based RLC encoder (stable per-generation coding anchor).
//!
//! Why it exists.  The coded *sliding* window (`RlcWindowEncoder` in
//! coded-only mode) codes every symbol over the current window, whose anchor
//! moves with the frontier.  A slow-path symbol arrives one path-delay after
//! its window has slid on, so it is stranded and can only be recovered by a
//! congestion-throttled per-seq ARQ — an anti-aggregation drag on
//! heterogeneous paths (paper §5.8, `temporal_oracle.rs`).  A stable anchor
//! fixes it: partition the object's source symbols into fixed generations of
//! `gen_size ≈ W_mp` and code coded symbols within each generation.  A
//! generation's coding target never moves, so any coded symbol for it — from
//! any path, at any time — supplies an interchangeable degree of freedom, and
//! a lost symbol is replaced by the next coded symbol for the same generation
//! from either path (fungible cross-path recovery, no per-seq throttle).
//! Generations pipeline (`M ≥ 2` in flight) so the fast path never idles on a
//! slow generation's tail.
//!
//! Wire format — identical to the sliding-window RLC repair, by design.  A
//! generation-coded symbol is an RLC combination over the fixed span
//! `[g·G, g·G + gen_len)`, so it carries exactly the same self-describing
//! header the decoder already parses:
//!   `data = [window_start(8 LE)][window_count(2 LE)][coded_index(4 LE)][coded]`
//! where `window_start = g·G` (the generation anchor, stable — this is the
//! only substantive difference from the sliding encoder, whose window_start
//! moves every symbol), `window_count = gen_len` (K_G, the generation's live
//! length), `coded_index` a per-symbol monotonic counter.  The generation id
//! and K_G are therefore on the wire as `window_start / gen_size` and
//! `window_count`.  Because the coefficients only ever touch seqs inside one
//! generation, the existing `RlcWindowDecoder` solves each generation's K_G×K_G
//! system independently the instant K_G linearly-independent symbols for that
//! anchor arrive (decode-on-K), delivering its sources out-of-order — no
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

/// Marker bit set in the 4-byte wire coded-index of a filling-generation repair
/// (see `code_generation_full` / the proactive pacer). When set, the decoder
/// reads a 2-byte `coded_width` immediately after the 14-byte header (a 16-byte
/// header total) and treats coefficient columns `[coded_width, window_count)` as
/// zero — the sender only summed the retained contiguous prefix
/// `[anchor, anchor+coded_width)`, but the matrix width on the wire is the full
/// generation size `G`, so a filling-generation repair keys to the same
/// `(anchor, G)` decoder system as the sealed repairs and the reactive deficit
/// loop (no cross-width stranding, which a separate-grid inline repair
/// suffers). The real coded-index is the low 31 bits (it never reaches 2^31,
/// so masking is lossless). Never set on the 14-byte sealed format.
const FILL_FLAG: u32 = 0x8000_0000;

/// Generation-based RLC encoder.  Retains every not-yet-advanced source symbol
/// (partitioned into fixed generations) and emits coded symbols for the
/// pipeline of in-flight generations, round-robin.
pub struct GenerationEncoder {
    symbol_size: u16,
    /// Generation size G (source symbols per generation) — the stable coding
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
    /// Proactive-coding floor (in generations), decoupled from `base_gen`
    /// (the retention floor). The proactive round-robin codes
    /// `[code_base, code_base+pipeline)`. Tracks `base_gen` unless
    /// `set_code_base` advances it to follow the send frontier — which lets
    /// freshly-sent generations get
    /// their upfront proactive budget while a stalled in-order-frontier
    /// generation is left to bounded reactive recovery (its sources stay
    /// retained). Always `>= base_gen`.
    code_base: u64,
    /// Round-robin cursor over the active generation set (a generation id).
    rr: u64,
    /// Round-robin cursor for the filling-generation proactive pacer
    /// (`generate_repair_filling`), independent of `rr` so the two emission
    /// paths do not fight over one cursor.
    fill_rr: u64,
    /// Monotonic coded-symbol index — distinguishes coded symbols (and their
    /// coefficient seeds) both within and across generations.
    coded_index: u32,
    /// Proactive overhead r: a generation is coded up to
    /// `ceil(current_len·(1+r))` coded symbols before it is considered
    /// "provisioned"; beyond that it is only coded for recovery (when no active
    /// generation is still under budget). This bounds coded emission on a
    /// still-filling generation to ~its current source count, which prevents
    /// a startup stall where the whole flow-control window is spent on a
    /// 2-source generation, producing rank-2 symbols that decode only 2 seqs.
    overhead: f64,
    /// Coded symbols emitted per generation (for the budget above).
    emitted: BTreeMap<u64, u32>,
    /// Source intake is idle (object tail). When set, the final partial
    /// generation is also eligible for recovery coding (it never seals to its
    /// full gen_size, but once no more sources are coming its coded symbols do
    /// span its full — final — width, so recovery supplies useful DoF).
    intake_idle: bool,
    /// Systematic-repair submode (paper §5.8). When set, the raw source
    /// symbols ride the wire as primary (delivered out-of-order with zero
    /// decode) and this encoder emits only repair: the per-generation
    /// proactive budget is `ceil(len·r)` — just the loss-FEC overhead —
    /// instead of the coded-only `ceil(len·(1+r))` that has to supply every
    /// degree of freedom. The K base DoF come from the systematic source on
    /// the wire; coded symbols only cover the holes (deficit-driven top-up via
    /// `generate_repair_for` handles the residual). Decode is O(deficit), not
    /// O(G), and source is delivered on arrival.
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
    /// Coded-only mode must supply every degree of freedom, so it provisions
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
    /// only once it is sealed (all `gen_size` sources present) — or, for the
    /// final partial generation, once intake is idle. Coding a still-filling
    /// generation is the trap: its coded span only the few sources present at
    /// emit time (rank ≈ that few), so a fast fill or an exhausted budget leaves
    /// the sealed generation with mostly low-rank symbols and it never reaches
    /// K_G. Only a sealed generation's coded span its full width, so K_G of them
    /// decode it. Within that, `g` is coded up to its proactive budget, and the
    /// frontier generation (base_gen) up to the larger recovery cap.
    fn codeable(&self, g: u64) -> bool {
        if !self.sources.contains_key(&(g * self.gen_size)) {
            return false;
        }
        let sealed = self.gen_len(g) >= self.gen_size || self.intake_idle;
        if !sealed {
            return false;
        }
        // Proactive emission is capped at the per-generation provisioning budget
        // `ceil(len·(1+r))` for every generation, frontier included. Recovery
        // beyond the budget (for a sealed generation that lost > r of its coded)
        // is driven by per-generation deficit feedback: the
        // receiver reports each frontier generation's residual rank and the
        // sender emits exactly that many more via `generate_repair_for`, which
        // bypasses this budget. So the proactive path stays bounded (never
        // floods a generation) and recovery is bounded and targeted by the
        // deficit loop — which a feedback-free fixed cap cannot do at once (it
        // either floods or deadlocks the frontier).
        let emitted = self.emitted.get(&g).copied().unwrap_or(0);
        emitted < self.gen_budget(g)
    }

    /// Code one coded symbol over the stable span of generation `g`, advancing
    /// the monotonic coded index and the per-generation emission counter. Shared
    /// by the round-robin proactive path (`generate_repair`) and the deficit-
    /// driven recovery path (`generate_repair_for`).
    fn code_generation(&mut self, g: u64) -> WireSymbol {
        // `enc` seam (`RWM_CPUPROF`, default off): the GF coding extent. The
        // wrapper is here rather than at the callers so no call site can be
        // added later that bypasses the instrument.
        crate::net::cpuprof::timed(crate::net::cpuprof::Seam::Enc, || {
            self.code_generation_inner(g)
        })
    }

    fn code_generation_inner(&mut self, g: u64) -> WireSymbol {
        let symbol_size = self.symbol_size as usize;
        let coded_index = self.coded_index;
        self.coded_index += 1;
        *self.emitted.entry(g).or_insert(0) += 1;
        let (gen_start, syms) = self.generation_symbols(g);
        let gen_len = syms.len() as u16;

        // Coefficients over the stable generation span [gen_start, gen_start +
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

    /// Pick the next generation to proactively code for: round-robin over the
    /// `pipeline` oldest retained generations that are still under their
    /// provisioning budget (`emitted < ceil(len·(1+r))`). This is the open-loop
    /// path that provisions each sealed generation with its baseline K_G(1+r)
    /// coded symbols so it decodes without waiting a feedback round for the
    /// expected loss. Recovery beyond the budget (for a generation that lost more
    /// than `r` of its coded) is not done here — it is driven by per-generation
    /// deficit feedback via `generate_repair_for`, which the receiver bounds and
    /// targets exactly. Returns `None` when every active generation is at budget
    /// (or nothing is retained), at which point `wants_coding` is false and the
    /// sender's emission is purely deficit-driven until a generation decodes.
    fn next_active_gen(&mut self) -> Option<u64> {
        let top = self.top_gen();
        // Proactive coding is anchored at `code_base` (>= base_gen), the
        // send-frontier-tracking coding floor, not the retention floor. With
        // code_base == base_gen this is the in-order-anchored window.
        let floor = self.code_base.max(self.base_gen);
        let hi = (floor + self.pipeline).min(top + 1);
        if hi <= floor {
            return None;
        }
        let span = hi - floor;
        // Round-robin over codeable generations (sealed/tail-idle and still under
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

    /// Whether generation `g` is eligible for filling-generation proactive
    /// coding: it has at least one retained source and is still under its
    /// per-generation provisioning budget `ceil(len·factor)` (systematic
    /// `factor = r`). Unlike `codeable`, this does not require the generation to
    /// be sealed — the whole point of the pacer is to emit repair over the
    /// contiguous prefix while the generation is still filling, so the covering
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

    /// Code one proactive symbol over the retained contiguous prefix of
    /// generation `g` at the full generation matrix width `G`. The symbol sums
    /// only the present prefix `[gen_start, gen_start + w)` (w = current fill)
    /// with coefficients drawn from the full-width seed, and carries `w` as the
    /// wire `coded_width` (with `FILL_FLAG` set) so the decoder zeroes columns
    /// `[w, G)`. Because the wire `window_count` is `G` regardless of `w`, every
    /// symbol for `g` — filling or sealed, proactive or reactive — lands in the
    /// same `(anchor, G)` decoder matrix and combines fungibly. This is what
    /// makes filling-generation repair present-at-stall without the cross-grid
    /// stranding a separate-block inline repair suffers.
    fn code_generation_full(&mut self, g: u64) -> WireSymbol {
        // `enc` seam — the filling variant. Same seam as `code_generation`:
        // both are the same GF work on the same symbol budget, and splitting
        // them would give the decomposition a column whose meaning depends on
        // which emission path the arm happened to take.
        crate::net::cpuprof::timed(crate::net::cpuprof::Seam::Enc, || {
            self.code_generation_full_inner(g)
        })
    }

    fn code_generation_full_inner(&mut self, g: u64) -> WireSymbol {
        let symbol_size = self.symbol_size as usize;
        let coded_index = self.coded_index;
        self.coded_index += 1;
        *self.emitted.entry(g).or_insert(0) += 1;
        let gen_start = g * self.gen_size;
        let (_s, syms) = self.generation_symbols(g); // contiguous present prefix
        let coded_width = syms.len() as u16;
        let full = self.gen_size as u16; // stable MATRIX width

        // Coefficients over the full generation span [gen_start, gen_start+G);
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

    /// Pick the next generation to code via the filling pacer: round-robin over
    /// the `pipeline` oldest retained generations that are still under budget
    /// (filling or sealed). The oldest retained generations are exactly the ones
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
        // `src` seam: the pad allocation, the payload copy, and the
        // retention-store `insert` — which is a second full copy of every
        // source symbol, and one of the named suspects the decomposition
        // exists to size.
        crate::net::cpuprof::timed(crate::net::cpuprof::Seam::Src, || {
            self.add_source_inner(data)
        })
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
        // Deficit-driven recovery for a specific generation.
        // Bypasses the proactive per-generation budget: the receiver's deficit
        // already bounds how many are emitted, so there is no cap to apply
        // here — the only gate is that the generation is retained and sealed (its coded
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

    /// Interspersed trailing-window repair. Code one coded symbol over the
    /// arbitrary seq range `[start, start+count)` — a small trailing block of
    /// already-sent source, distinct from (and smaller than) the fixed
    /// generation. Unlike `generate_repair` (which round-robins sealed
    /// generations, so a generation's repair only flows after all G of its
    /// sources are sent → arrives ~1 generation-span behind), this codes over a
    /// block whose members were all just sent, so the covering repair arrives
    /// ~immediately after the hole it covers — present when the receiver detects
    /// the hole → proactive decode, no reactive round-trip. The wire header
    /// carries `(start, count, coded_index)`, so the dense decoder solves it in a
    /// `(start,count)` matrix exactly like a generation of that span (with the raw
    /// sources injected as unit pivots). Returns `None` unless the full range is
    /// retained — a missing source would make the coded equation inconsistent with
    /// the receiver's regenerated coefficients.
    fn generate_repair_range(&mut self, start: u64, count: u16) -> Option<WireSymbol> {
        // `enc` seam. The retention scan below is inside the extent on
        // purpose: it is a per-symbol walk of the range and it is real coding
        // cost, charged to the seam that pays it.
        crate::net::cpuprof::timed(crate::net::cpuprof::Seam::Enc, || {
            self.generate_repair_range_inner(start, count)
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
        // The coding floor never trails the retention floor.
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
        // Advance the proactive-coding floor toward the send frontier, clamped
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

/// The `RWM_CPUPROF` seam bodies for the two [`WindowEncoder`] entry points
/// above. They live in an inherent block because a trait impl may only
/// contain the trait's own items — the split is a language requirement, not
/// a design choice, and the timed wrappers stay on the trait methods so no
/// caller can reach the work without passing the instrument.
impl GenerationEncoder {
    fn add_source_inner(&mut self, data: &[u8]) -> WireSymbol {
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

    fn generate_repair_range_inner(&mut self, start: u64, count: u16) -> Option<WireSymbol> {
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
}

// ===========================================================================
// Sparse-aware generation decoder — the fast decode path for generation coding.
// ===========================================================================
//
// Why it exists.  The generation *encoder* above produces RLC-repair symbols
// with the identical self-describing wire header as the sliding window, so the
// sparse `RlcWindowDecoder` can decode them (and does, in the unit tests).  But
// that decoder stores each pivot row's coefficients as a `BTreeMap<u64,u8>` and
// cascades single-unknown resolutions one at a time — allocation-heavy and
// pointer-chasing, far below a dense GF(256) solver, which at G=384 puts
// decode below the link rate.
//
// The cost model.  The dense decoder (kept verbatim in `reference` below)
// materializes every known source as a full-width unit pivot row and reduces
// every incoming row against all of them with fused (G+S)-byte SIMD ops:
// O(G·(G+S)) per row even when the row's only job is to eliminate
// already-known sources.  In systematic mode — where G−k of a generation's
// DoF arrive as raw source and only k ≈ ε·G repair rows carry new
// information — that turns an O(k·G·S + k³) problem into O(G²·S)-class work.
//
// This decoder is sparse-aware and per-generation:
//   • Known sources never enter the matrix.  A generation slot keeps a `known`
//     bitmap; an incoming row's known columns are eliminated payload-only
//     (S bytes per known column, against `recovered`) instead of via fused
//     (G+S)-byte unit-row ops — and slot creation stores no payload copies.
//   • Only coded rows are matrix rows (`pivots`, ≤ k + deficit extras in
//     systematic mode), kept in reduced row-echelon form over the non-known
//     columns exactly as before, using the same fused-row SIMD elimination.
//   • A pivot row that reduces to a unit row is delivered immediately and
//     converted to a `known` column (dropped from the matrix), so the active
//     system stays k×k.  Completion is `known_count == width` — the moment the
//     last column is known the whole generation has been delivered, with no
//     separate full-rank back-substitution pass (the RREF invariant makes the
//     final sweep deliver every remaining row; see `insert_equation`).
//   • A generation whose span is fully recovered before its first repair
//     arrives (k = 0, the common case at low ε) never creates a matrix at all:
//     the repair is recognized as redundant in O(G) with zero GF work.
// Per generation the cost is O(k·G·S + k²·(G+S)): the irreducible payload-only
// elimination of the known mass from each of ~k dense repair rows, plus the
// k×k active elimination.  In coded-only mode (no raw source on the wire)
// nothing is ever known, every row is a coded pivot, and the arithmetic
// degenerates to exactly the dense reference decoder's O(G²·(G+S)) — that mode's
// cost is information-theoretic (all G DoF arrive dense), not implementation.
//
// Delivered output is the same (seq, payload) set with identical bytes as the
// reference decoder on any consistent symbol stream (asserted by the
// differential test); the only observable difference is the
// order of seqs *within* one `add_symbol` return on the call that completes a
// generation (incremental sweep order vs the reference's ascending full-rank sweep).
// Every consumer keys on seq (reassembly/reorder buffers), so ordering within
// a call is semantically inert.

/// One reduced coded pivot row of a generation's system, stored as one
/// contiguous buffer `[coeffs (width bytes) | data (symbol_size bytes)]`.
/// Fusing the coefficient row and the payload row into a single allocation lets
/// a single SIMD `mul_acc_slice` eliminate both in one call.  After
/// normalization the pivot column holds 1 and, by the RREF invariant, every
/// other pivot column (and every `known` column) holds 0.
type GenRow = Vec<u8>;

/// State of one generation's decode, keyed by `(anchor, width)` — see
/// `GenerationDecoder::gens`.
enum GenSlot {
    /// Still accumulating independent degrees of freedom.
    Solving {
        width: usize,
        /// Source-known columns.  `known[c]` ⇒ the source payload for
        /// `anchor + c` lives in `GenerationDecoder::recovered` and every
        /// matrix row is zero at column `c` (incoming rows are reduced
        /// payload-only against `recovered` before insertion).  Known columns
        /// are never materialized as matrix rows — this is the sparse-aware
        /// core: the G−k known DoF cost O(S) each instead of a fused
        /// (G+S)-byte row op against a stored unit row.
        known: Vec<bool>,
        /// Number of `true` entries in `known`.
        known_count: usize,
        /// `pivots[c]` is the reduced coded row whose pivot column is `c`
        /// (or `None`).  Only coded rows live here (≤ holes + deficit margin
        /// in systematic mode); a row that becomes unit is delivered and
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
    /// `(anchor, width)` → decode state.  Keying by both anchor and width (not
    /// anchor alone) is load-bearing: the object stream reuses the absolute seq
    /// space across objects, so a single anchor legitimately hosts different
    /// generations of different K_G at different times — the encoder's fixed
    /// generation `g` accumulates one object's short tail (coded at that partial
    /// width once intake goes idle) and the next object's fill (coded at the full
    /// width once sealed).  Those are distinct linear systems over different
    /// source sets; keying by `(anchor, width)` lets them coexist instead of one
    /// resetting/thrashing the other's pivots at the object boundary.
    gens: BTreeMap<(u64, usize), GenSlot>,
    /// Sources already recovered: seq → payload.  Three jobs: (1) the
    /// delivered-seq set (its keys) for dedup and `rank_in`; (2) known-source
    /// elimination — a fresh generation slot marks every already-recovered
    /// source in its span `known`, and incoming rows are reduced against these
    /// payloads directly (payload-only, no unit pivot rows); (3) the payload
    /// store those eliminations read from — which is why `advance` never prunes
    /// a seq still covered by a live Solving slot (this decoder holds no
    /// private copies in unit rows).
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
            // known columns (flags only — no payload copies, no unit rows).
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

        // Forward-reduce the incoming row.  Known columns are eliminated
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
        // modify: only those (plus the new pivot row) can have newly become
        // unit rows.
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

        // Unit sweep: a touched row whose coefficient half has a single nonzero
        // (its own pivot, normalized to 1) is its column's source.  Deliver it,
        // mark the column known, and drop the row — the matrix stays k×k and the
        // RREF invariant is untouched (every other row is already zero at a
        // pivot column).  When the last column turns known the generation is
        // complete: by RREF, pivots over all remaining free columns force every
        // remaining row to be unit, so this same sweep delivers the whole tail —
        // the dense decoder's full-rank pass, folded into the increment.
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

    /// Inject an already-received raw source (seq → payload) as a unit equation
    /// into every existing Solving generation matrix whose fixed span covers
    /// `seq`.
    ///
    /// Why: a generation's decode matrix learns the sources it already knows
    /// only at slot creation (the first repair for that generation). Source
    /// and repair symbols interleave and reorder, so a generation's own
    /// non-lost sources routinely arrive after its first repair. Without this
    /// injection those late sources land in `recovered` but are invisible to
    /// the matrix, which then treats them as permanent unknowns: `rank_in`
    /// reports a deficit of `G − matrix_rank` (inflated by the late-source
    /// count, not the true hole count), the sender floods `G − rank` coded
    /// repairs where only `holes` were needed, and the surplus repairs merely
    /// re-derive already-received sources. By
    /// feeding each late source into the live matrix as the unit equation
    /// `e_c · x = data` (c = seq − anchor), the unknown space shrinks to the real
    /// holes the instant the source arrives, so the reported deficit == holes and
    /// coded repair actually recovers holes proactively. If the injection is the
    /// last missing degree of freedom it completes the generation and returns its
    /// remaining holes.
    ///
    /// Cost (sparse-aware): the injected unit row reduces against nothing (its
    /// column is free), pivots at `c`, and the elimination touches only the ≤ k
    /// coded rows with a nonzero at `c` — O(k·S), not a dense O(k·(G+S)) plus a
    /// full-width unit-row insert.  A column that is already `known` is skipped
    /// before any row is built.
    fn inject_source_into_active_gens(&mut self, seq: u64, data: &[u8]) -> Vec<(u64, Bytes)> {
        let ss = self.symbol_size;
        // Collect covering Solving slots first (avoid aliasing the &mut self used
        // by insert_equation). A source is covered by slot (anchor,width) iff
        // anchor ≤ seq < anchor+width. Widths are small in count, so this is cheap.
        // Skip slots that already know this column — the unit equation would be
        // linearly dependent (the known bitmap answers that for free, without
        // a fused row op).
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

    /// Transitively propagate just-delivered sources into every other active
    /// generation matrix, returning all sources delivered along the way.
    ///
    /// Why: two coding grids coexist: the small inline trailing block (width
    /// W, the in-flight proactive channel) and the wide generation (width G,
    /// the reactive deficit loop's unit). A hole recovered by a block repair
    /// lands in `recovered`, but the covering G-matrix (created later, by a
    /// deficit repair) only learns `recovered` at creation and never after —
    /// so a hole recovered by the block after the G-matrix exists stays an
    /// unknown in it, `rank_in(G)` under-counts, the receiver over-reports the
    /// generation's deficit, and the sender floods redundant reactive repair.
    /// Feeding each block-recovered hole into the G-matrix (as a unit equation) keeps
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
            // Systematic mode: the raw source rides the wire as primary. Deliver it
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
            // A source that arrives after a covering generation's matrix was
            // created must be injected into that live matrix as a unit
            // equation — otherwise the matrix keeps treating it as an unknown,
            // inflating the reported deficit and wasting coded repair. Inject
            // now; if it completes a generation, deliver that generation's
            // remaining holes too.
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
        // repairs_useful counts repairs that contributed a new degree of freedom
        // (rank-add), the honest per-hole "useful" signal — not per-generation
        // completions. With late-source injection the matrix's unknown
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
        // Prune recovered payloads below the frontier — except the span of any
        // surviving Solving slot: its `known` columns eliminate against these
        // payloads (this decoder holds no private copies in unit pivot rows,
        // so the store must outlive the slot).  Memory is the same order as
        // the dense decoder's — one payload per known
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
        // the span that is not yet a recovered source.  (Known columns are
        // recovered sources, so only coded rows count.)  A pivot at a hole
        // column is a coded DoF that has advanced into hole territory and will
        // complete the hole once enough accumulate.
        // `buffered > 0` at a stall ⇒ proactive repair is present and the hole
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
// Reference dense decoder -- differential-test oracle only.
// ===========================================================================

/// The dense decoder, kept only as a differential-test oracle
/// (see `generation/reference.rs`). Never constructed by the engine.
#[doc(hidden)]
#[allow(dead_code)] // frozen byte-exact copy: kept whole, unused helpers included
pub mod reference;


#[cfg(test)]
mod tests;
