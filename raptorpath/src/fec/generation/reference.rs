//! Byte-exact copy of the dense `GenerationDecoder` as of commit 02d240c,
//! KEPT ONLY as the oracle for the old-vs-new differential test and the
//! old-vs-new L0 micro-bench (`tests/gen_decode_bench.rs`). Never constructed
//! by the engine. Do not modify: its value is that it preserves the exact
//! pre-rewrite behaviour.

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
