//! UNIFIED RLC-family decoder (task #61, paper §16.20).
//!
//! WHY THIS EXISTS. The two RLC-family production decoders were two
//! computations of the SAME closure over the SAME wire language:
//!
//!   * `RlcWindowDecoder` — the (near-)full global closure (one incremental
//!     GE over the seq line; equations from ANY spans combine), but with the
//!     measured ~200× cost defect (BTreeMap-of-coefficients rows + cascade).
//!   * `GenerationDecoder` — the sparse-aware cost model (§16.18), but a
//!     BLOCK-RESTRICTED closure: equations are keyed by `(anchor, width)`,
//!     systems never share partial rows, and only fully-solved sources
//!     propagate between them. Valid on span-ALIGNED wires (generation mode,
//!     block-diagonal), provably stranding on moving-span wires (the generic
//!     2-loss burst covered by two different sliding spans — §16.20.1).
//!
//! This decoder computes the FULL global closure WITH the sparse-aware cost
//! model — one machine for every span policy the sender may derive from δ
//! (paper §16.20.3):
//!
//!   * KNOWN columns never enter the matrix: received/recovered payloads live
//!     in `recovered`; an incoming row's known columns are eliminated
//!     payload-only (S bytes each). The invariant "no stored row has a
//!     nonzero at a recovered column" is maintained at insert AND at every
//!     delivery (back-elimination worklist), so — unlike the keyed machine —
//!     late sources need no special injection apparatus: a late source is a
//!     unit equation like any other.
//!   * Only CODED rows are matrix rows, kept in global RREF: each row is
//!     dense over its interval SPAN `[start, start+len)`; combining two
//!     overlapping rows yields a row spanning their (interval) union, so
//!     rows stay dense-over-span — no per-coefficient maps, no cascade
//!     allocation. Every stored row's leading coefficient is 1 at its pivot
//!     column, and no other stored row is nonzero at any pivot column
//!     (Gauss–Jordan maintained on insert).
//!   * A row that becomes UNIT delivers immediately and converts to a known
//!     column (dropped from the matrix) — the per-arrival incremental-decode
//!     property that carries the realtime tail win (§16.20.3).
//!   * k = 0 fast path: a repair whose span is fully known is recognized
//!     redundant in O(w) with zero GF work.
//!
//! Cost per solve involving k coded rows of span ≤ L: O(k·L·S + k²·(L+S)) —
//! §16.18's per-generation bound when the wire is aligned (the global system
//! block-diagonalizes by itself), bounded by L ≤ W on sliding wires.
//!
//! DELIVERED-SET CONTRACT (differential-tested):
//!   * vs `RlcWindowDecoder`: identical per-call delivered sets and bytes on
//!     in-order traces; a SUPERSET-or-equal under reorder/duplication. The
//!     one divergence is a documented LEGACY DEFECT, not a semantics change:
//!     when a source arrives late for a seq that is already a stored row's
//!     pivot, the legacy machine DISCARDS that row ("just confirming what we
//!     know", rlc_window.rs `cascade_from_recovered`) — but the row still
//!     carries one independent DoF over its OTHER unknowns. This machine
//!     re-assimilates the displaced equation (exactly what the generation
//!     machine's unit-injection does), so it never loses rank.
//!   * vs `GenerationDecoder` (+ the pre-§16.18 `reference` oracle) on
//!     aligned generation wires: identical delivered sets when blocks are
//!     disjoint; superset-or-equal in the mixed-width same-anchor overlap
//!     the keyed machine documents as separate systems.
//!
//! WIRE FORMAT: identical to `rlc_window`/`generation` (14-byte self-
//! describing repair header, FILL_FLAG variant included).

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use bytes::Bytes;

use super::gf256;
use super::traits::{FecBackend, WireSymbol};
use super::window_traits::WindowDecoder;

pub use gf256::generate_window_coefficients;

/// Repair header: 8 (window_start) + 2 (window_count) + 4 (coded_index).
const REPAIR_HEADER_SIZE: usize = 14;
/// FILLING-generation marker bit (see `generation.rs`).
const FILL_FLAG: u32 = 0x8000_0000;

/// One coded row of the global RREF system, dense over its interval span.
struct URow {
    /// Absolute seq of `coeffs[0]`.
    start: u64,
    /// Coefficients over `[start, start + coeffs.len())`. The row's pivot is
    /// its first nonzero column (normalized to 1); columns of recovered
    /// sources and of other rows' pivots are zero (RREF invariant).
    coeffs: Vec<u8>,
    /// Payload accumulator (symbol_size bytes).
    data: Vec<u8>,
}

impl URow {
    #[inline]
    fn end(&self) -> u64 {
        self.start + self.coeffs.len() as u64
    }
    #[inline]
    fn coeff_at(&self, seq: u64) -> u8 {
        if seq < self.start || seq >= self.end() {
            0
        } else {
            self.coeffs[(seq - self.start) as usize]
        }
    }
    #[inline]
    fn zero_at(&mut self, seq: u64) {
        if seq >= self.start && seq < self.end() {
            self.coeffs[(seq - self.start) as usize] = 0;
        }
    }
    /// First nonzero column (absolute seq), if any.
    fn lead(&self) -> Option<u64> {
        self.coeffs
            .iter()
            .position(|&c| c != 0)
            .map(|i| self.start + i as u64)
    }
    /// Number of nonzero coefficients, early-exiting at 2 (unit detection).
    fn nnz_capped2(&self) -> u32 {
        let mut n = 0u32;
        for &c in &self.coeffs {
            if c != 0 {
                n += 1;
                if n > 1 {
                    break;
                }
            }
        }
        n
    }
    /// Grow the span to cover `[start, end)` (zero-padded).
    fn grow(&mut self, start: u64, end: u64) {
        if start < self.start {
            let pad = (self.start - start) as usize;
            let mut nc = vec![0u8; pad + self.coeffs.len()];
            nc[pad..].copy_from_slice(&self.coeffs);
            self.coeffs = nc;
            self.start = start;
        }
        if end > self.end() {
            let add = (end - self.end()) as usize;
            self.coeffs.extend(std::iter::repeat(0).take(add));
        }
    }
    /// self -= f × other (coeffs over the union span, payload full-width).
    fn sub_scaled(&mut self, f: u8, other: &URow) {
        if f == 0 {
            return;
        }
        self.grow(other.start.min(self.start), other.end().max(self.end()));
        let off = (other.start - self.start) as usize;
        gf256::mul_acc_slice(f, &other.coeffs, &mut self.coeffs[off..off + other.coeffs.len()]);
        gf256::mul_acc_slice(f, &other.data, &mut self.data);
    }
    /// Scale the whole row by a scalar (normalization).
    fn scale(&mut self, c: u8) {
        if c == 1 {
            return;
        }
        for x in self.coeffs.iter_mut() {
            *x = gf256::mul(c, *x);
        }
        for x in self.data.iter_mut() {
            *x = gf256::mul(c, *x);
        }
    }
}

/// Work item for the assimilation loop: a payload that became known, or a
/// coded equation to (re-)insert.
enum Item {
    Known(u64, Vec<u8>),
    Eq(URow),
}

/// Unified global sparse-aware RLC decoder. Drop-in `WindowDecoder` for BOTH
/// the sliding-window and the generation wire (env `RWM_UNIFIED`).
pub struct UnifiedDecoder {
    symbol_size: usize,
    /// Known payloads: seq → data. Invariant: no stored row has a nonzero
    /// coefficient at any of these seqs (payload-only elimination at insert +
    /// back-elimination at every recovery), so entries below the advance
    /// frontier are freely prunable.
    recovered: BTreeMap<u64, Vec<u8>>,
    /// Seqs returned to the caller (once-only delivery).
    delivered: BTreeSet<u64>,
    /// Global RREF: pivot seq → row.
    rows: BTreeMap<u64, URow>,
    /// Wire dedup: (block_id, payload_id, is_repair).
    seen: HashSet<(u64, u32, bool)>,
    total_fed: u64,
    repairs_fed: u64,
    repairs_useful: u64,
}

impl UnifiedDecoder {
    pub fn new(symbol_size: u16) -> Self {
        Self {
            symbol_size: symbol_size as usize,
            recovered: BTreeMap::new(),
            delivered: BTreeSet::new(),
            rows: BTreeMap::new(),
            seen: HashSet::new(),
            total_fed: 0,
            repairs_fed: 0,
            repairs_useful: 0,
        }
    }

    /// Drive the assimilation worklist to fixpoint. Returns
    /// `(first_eq_added_rank, deliveries)` — the flag reports whether the
    /// FIRST `Item::Eq` (the wire equation, when called from a repair)
    /// contributed a new independent DoF (the honest `repairs_useful`
    /// signal, same semantics as the generation machine).
    fn assimilate(&mut self, initial: Vec<Item>) -> (bool, Vec<(u64, Bytes)>) {
        let mut out: Vec<(u64, Bytes)> = Vec::new();
        let mut work: VecDeque<Item> = initial.into();
        let mut first_eq_rank: Option<bool> = None;
        let mut is_first_eq_pending = matches!(work.front(), Some(Item::Eq(_)));
        while let Some(item) = work.pop_front() {
            match item {
                Item::Known(s, d) => {
                    if self.recovered.contains_key(&s) {
                        continue;
                    }
                    self.recovered.insert(s, d.clone());
                    if self.delivered.insert(s) {
                        out.push((s, Bytes::from(d.clone())));
                    }
                    // A row PIVOTING at s is displaced, not discarded: fold
                    // the now-known payload out of it and re-assimilate the
                    // remaining equation (it still carries one DoF over its
                    // other unknowns — the legacy sliding machine's drop here
                    // is a rank-loss defect, see module docs).
                    if let Some(mut row) = self.rows.remove(&s) {
                        let f = row.coeff_at(s); // normalized: 1
                        row.zero_at(s);
                        gf256::mul_acc_slice(f, &d, &mut row.data);
                        if row.nnz_capped2() > 0 {
                            work.push_back(Item::Eq(row));
                        }
                    }
                    // Back-eliminate column s from every covering row,
                    // payload-only. Rows that become unit deliver in turn.
                    let covering: Vec<u64> = self
                        .rows
                        .iter()
                        .filter(|(_, r)| r.coeff_at(s) != 0)
                        .map(|(&p, _)| p)
                        .collect();
                    for p in covering {
                        let row = self.rows.get_mut(&p).expect("collected above");
                        let f = row.coeff_at(s);
                        row.zero_at(s);
                        // borrow: split the payload read from the row mutation
                        let dref = d.clone();
                        gf256::mul_acc_slice(f, &dref, &mut row.data);
                        if row.nnz_capped2() == 1 {
                            let row = self.rows.remove(&p).expect("present");
                            let ls = row.lead().expect("unit");
                            // pivot column normalized to 1 ⇒ data IS payload
                            debug_assert_eq!(ls, p);
                            work.push_back(Item::Known(ls, row.data));
                        }
                    }
                }
                Item::Eq(mut row) => {
                    let counting = is_first_eq_pending;
                    is_first_eq_pending = false;
                    // 1) eliminate KNOWN columns payload-only.
                    let (rs, re) = (row.start, row.end());
                    let known_in: Vec<u64> = self
                        .recovered
                        .range(rs..re)
                        .filter(|(&s, _)| row.coeff_at(s) != 0)
                        .map(|(&s, _)| s)
                        .collect();
                    for s in known_in {
                        let f = row.coeff_at(s);
                        row.zero_at(s);
                        let d = self.recovered.get(&s).expect("collected above");
                        gf256::mul_acc_slice(f, d, &mut row.data);
                    }
                    // 2) forward-reduce against existing pivots, left→right.
                    //    A pivot row has zeros before its pivot column, so one
                    //    scan fully reduces even as the span grows rightward.
                    let mut s = row.start;
                    while s < row.end() {
                        let c = row.coeff_at(s);
                        if c != 0 {
                            if let Some(prow) = self.rows.remove(&s) {
                                row.sub_scaled(c, &prow);
                                self.rows.insert(s, prow);
                            }
                        }
                        s += 1;
                    }
                    // 3) leading nonzero = the new pivot; none ⇒ dependent.
                    let Some(pcol) = row.lead() else {
                        if counting {
                            first_eq_rank = Some(false);
                        }
                        continue;
                    };
                    if counting {
                        first_eq_rank = Some(true);
                    }
                    let lead_c = row.coeff_at(pcol);
                    row.scale(gf256::inv(lead_c));
                    if row.nnz_capped2() == 1 {
                        // already a unit equation
                        work.push_back(Item::Known(pcol, row.data));
                        continue;
                    }
                    // 4) Gauss–Jordan: clear pcol from every other stored row.
                    let others: Vec<u64> = self
                        .rows
                        .iter()
                        .filter(|(_, r)| r.coeff_at(pcol) != 0)
                        .map(|(&p, _)| p)
                        .collect();
                    let mut touched: Vec<u64> = Vec::new();
                    for p in others {
                        let mut other = self.rows.remove(&p).expect("collected above");
                        let f = other.coeff_at(pcol);
                        other.sub_scaled(f, &row);
                        debug_assert_eq!(other.coeff_at(pcol), 0);
                        self.rows.insert(p, other);
                        touched.push(p);
                    }
                    self.rows.insert(pcol, row);
                    // 5) unit sweep over touched rows.
                    for p in touched {
                        let Some(r) = self.rows.get(&p) else { continue };
                        if r.nnz_capped2() == 1 {
                            let r = self.rows.remove(&p).expect("present");
                            let ls = r.lead().expect("unit");
                            debug_assert_eq!(ls, p);
                            work.push_back(Item::Known(ls, r.data));
                        }
                    }
                }
            }
        }
        (first_eq_rank.unwrap_or(false), out)
    }
}

impl WindowDecoder for UnifiedDecoder {
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
            let seq = symbol.block_id;
            if self.recovered.contains_key(&seq) {
                return vec![];
            }
            let mut data = vec![0u8; self.symbol_size];
            let n = symbol.data.len().min(self.symbol_size);
            data[..n].copy_from_slice(&symbol.data[..n]);
            let (_, out) = self.assimilate(vec![Item::Known(seq, data)]);
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
        // FILL_FLAG: sender summed only the prefix [anchor, anchor+cw); the
        // matrix width on the wire is the full generation (see generation.rs).
        let (repair_index, coded_width, header_end) = if wire_index & FILL_FLAG != 0 {
            if symbol.data.len() < REPAIR_HEADER_SIZE + 2 {
                return vec![];
            }
            let cw = u16::from_le_bytes(
                symbol.data[REPAIR_HEADER_SIZE..REPAIR_HEADER_SIZE + 2]
                    .try_into()
                    .unwrap(),
            ) as usize;
            (wire_index & !FILL_FLAG, cw.min(width), REPAIR_HEADER_SIZE + 2)
        } else {
            (wire_index, width, REPAIR_HEADER_SIZE)
        };
        let coded = &symbol.data[header_end..];
        // k = 0 fast path: fully-known span ⇒ redundant, zero GF work.
        if self.recovered.range(anchor..anchor + width as u64).count() == width {
            return vec![];
        }
        let mut coeffs = generate_window_coefficients(anchor, width as u16, repair_index);
        for c in coeffs.iter_mut().skip(coded_width) {
            *c = 0;
        }
        let row = URow {
            start: anchor,
            coeffs,
            data: {
                let mut d = vec![0u8; self.symbol_size];
                let n = coded.len().min(self.symbol_size);
                d[..n].copy_from_slice(&coded[..n]);
                d
            },
        };
        let (added, out) = self.assimilate(vec![Item::Eq(row)]);
        if added {
            self.repairs_useful += 1;
        }
        out
    }

    fn advance(&mut self, oldest_seq: u64) {
        // Rows pivoting below the frontier: the ack contract says everything
        // below is received/recovered, so these rows are stale — mirror the
        // legacy sliding decoder and drop them.
        let stale: Vec<u64> = self.rows.range(..oldest_seq).map(|(&p, _)| p).collect();
        for p in stale {
            self.rows.remove(&p);
        }
        // Stored rows never reference recovered columns (invariant), so the
        // payload store below the frontier is freely prunable.
        let old: Vec<u64> = self.recovered.range(..oldest_seq).map(|(&s, _)| s).collect();
        for s in old {
            self.recovered.remove(&s);
        }
        let old_d: Vec<u64> = self.delivered.range(..oldest_seq).copied().collect();
        for s in old_d {
            self.delivered.remove(&s);
        }
        self.seen.retain(|(block_id, _, _)| *block_id >= oldest_seq);
    }

    fn rank_in(&self, start: u64, count: u64) -> u64 {
        let end = start.saturating_add(count);
        let recovered = self.recovered.range(start..end).count() as u64;
        let pivots = self.rows.range(start..end).count() as u64;
        recovered + pivots
    }

    fn frontier_probe(&self, frontier: u64, horizon: u64) -> (u64, u64) {
        let end = horizon.saturating_add(1);
        if end <= frontier {
            return (0, 0);
        }
        let span = end - frontier;
        let recovered = self.recovered.range(frontier..end).count() as u64;
        let holes = span.saturating_sub(recovered);
        let pivots = self.rows.range(frontier..end).count() as u64;
        (holes, pivots)
    }

    fn seq_probe(&self, seq: u64) -> (bool, bool, bool) {
        (
            self.seen.contains(&(seq, 0, false)),
            self.recovered.contains_key(&seq),
            self.delivered.contains(&seq),
        )
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

    /// diag/unified-collapse (roadmap item 3): the live cost drivers of the
    /// global RREF — active coded-row count L, widest row span, total
    /// coefficient bytes (matrix memory), payload-store and dedup-set sizes.
    fn diag_stats(&self) -> Option<String> {
        let rows = self.rows.len();
        let (mut max_span, mut coeff_bytes) = (0usize, 0usize);
        for r in self.rows.values() {
            max_span = max_span.max(r.coeffs.len());
            coeff_bytes += r.coeffs.len();
        }
        Some(format!(
            "rows={} max_span={} coeff_kb={} recovered={} seen={}",
            rows,
            max_span,
            coeff_bytes / 1024,
            self.recovered.len(),
            self.seen.len()
        ))
    }
}

// ---------------------------------------------------------------------------
// Differential tests — unified vs legacy sliding machine (moving spans).
// The aligned-wire differential (vs GenerationDecoder + the pre-§16.18
// reference oracle) lives in generation.rs next to the existing old-vs-new
// differential, sharing its trace generator.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::rlc_window::{RlcWindowDecoder, RlcWindowEncoder};
    use super::super::window_traits::{WindowDecoder as _, WindowEncoder};
    use super::*;
    use rand::prelude::*;
    use rand_chacha::ChaCha8Rng;

    fn sorted(mut v: Vec<(u64, Bytes)>) -> Vec<(u64, Bytes)> {
        v.sort_by_key(|(s, _)| *s);
        v
    }

    /// The §16.20.1 minimal trap: two holes covered by two DIFFERENT moving
    /// spans that are jointly determining. The global machine must solve
    /// both; this is exactly where the keyed machine strands.
    #[test]
    fn unified_solves_cross_span_joint_system() {
        let ss = 64u16;
        let mut enc = RlcWindowEncoder::new(ss);
        let mut dec = UnifiedDecoder::new(ss);
        let mut syms = Vec::new();
        for i in 0..20u64 {
            syms.push(enc.add_source(&vec![i as u8 + 1; 32]));
        }
        // holes at 3 and 9; feed everything else
        for (i, s) in syms.iter().enumerate() {
            if i != 3 && i != 9 {
                dec.add_symbol(s);
            }
        }
        // two repairs over two DIFFERENT spans, both covering {3, 9}
        let r1 = enc.generate_repair_range(0, 12).unwrap(); // span [0,12)
        let r2 = enc.generate_repair_range(2, 10).unwrap(); // span [2,12)
        let out1 = dec.add_symbol(&r1);
        assert!(out1.is_empty(), "rank 1 over 2 unknowns must not deliver");
        let out2 = sorted(dec.add_symbol(&r2));
        let seqs: Vec<u64> = out2.iter().map(|(s, _)| *s).collect();
        assert_eq!(seqs, vec![3, 9], "joint cross-span system must solve both holes");
        assert_eq!(&out2[0].1[..32], &vec![4u8; 32][..]);
        assert_eq!(&out2[1].1[..32], &vec![10u8; 32][..]);
    }

    /// In-order traces (loss only, no reorder/dup): the late-source-on-pivot
    /// corner cannot fire, so unified and legacy sliding machines must agree
    /// EXACTLY, per call, on delivered sets AND bytes.
    #[test]
    fn unified_matches_legacy_sliding_exactly_in_order() {
        let ss = 48u16;
        for seed in 0..20u64 {
            let mut rng = ChaCha8Rng::seed_from_u64(0xA11 ^ seed);
            let mut enc = RlcWindowEncoder::new(ss);
            let mut d_leg = RlcWindowDecoder::new(ss);
            let mut d_uni = UnifiedDecoder::new(ss);
            let loss: f64 = 0.05 + 0.20 * (seed as f64 / 20.0);
            let n_src = 250usize;
            let mut acked = 0u64;
            for i in 0..n_src {
                let mut payload = vec![0u8; 40];
                rng.fill(&mut payload[..]);
                payload[0] = (i & 0xff) as u8;
                let s = enc.add_source(&payload);
                let mut feed: Vec<WireSymbol> = Vec::new();
                if rng.gen::<f64>() >= loss {
                    feed.push(s);
                }
                if rng.gen::<f64>() < 0.30 {
                    let r = enc.generate_repair();
                    if rng.gen::<f64>() >= loss {
                        feed.push(r);
                    }
                }
                if rng.gen::<f64>() < 0.25 {
                    let (ws, we) = enc.window_span();
                    if we > ws + 4 {
                        let w = 4 + (rng.gen::<u64>() % 8).min(we - ws);
                        let start = we.saturating_sub(w).max(ws);
                        let cnt = (we - start) as u16;
                        if let Some(r) = enc.generate_repair_range(start, cnt) {
                            if rng.gen::<f64>() >= loss {
                                feed.push(r);
                            }
                        }
                    }
                }
                if rng.gen::<f64>() < 0.15 && acked + 24 < i as u64 {
                    acked = i as u64 - 24;
                    enc.advance(acked);
                    d_leg.advance(acked);
                    d_uni.advance(acked);
                }
                for sym in &feed {
                    let a = sorted(d_leg.add_symbol(sym));
                    let b = sorted(d_uni.add_symbol(sym));
                    assert_eq!(
                        a.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
                        b.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
                        "per-call delivered sets diverged (seed {seed})"
                    );
                    for ((s1, p1), (s2, p2)) in a.iter().zip(b.iter()) {
                        assert_eq!(s1, s2);
                        assert_eq!(p1, p2, "bytes diverged at seq {s1} (seed {seed})");
                    }
                }
            }
        }
    }

    /// Adversarial traces (reorder, duplication, loss): unified must deliver
    /// a SUPERSET-or-equal of the legacy sliding machine at every point,
    /// with identical bytes on the common seqs. Extras must be exactly the
    /// documented legacy defect (rank dropped on late-source-on-pivot) —
    /// counted and printed, never the other direction.
    #[test]
    fn unified_superset_of_legacy_sliding_under_reorder() {
        let ss = 48u16;
        let mut total_extra = 0usize;
        for seed in 0..30u64 {
            let mut rng = ChaCha8Rng::seed_from_u64(0xD1FF ^ seed);
            let mut enc = RlcWindowEncoder::new(ss);
            let mut d_leg = RlcWindowDecoder::new(ss);
            let mut d_uni = UnifiedDecoder::new(ss);

            let n_src = 300usize;
            let loss: f64 = 0.05 + 0.20 * (seed as f64 / 30.0);
            let mut wire: Vec<WireSymbol> = Vec::new();
            let mut pending: Vec<WireSymbol> = Vec::new();

            for i in 0..n_src {
                let mut payload = vec![0u8; 40];
                rng.fill(&mut payload[..]);
                payload[0] = (i & 0xff) as u8;
                let s = enc.add_source(&payload);
                if rng.gen::<f64>() >= loss {
                    pending.push(s);
                }
                if rng.gen::<f64>() < 0.30 {
                    let r = enc.generate_repair();
                    if rng.gen::<f64>() >= loss {
                        pending.push(r);
                    }
                }
                if rng.gen::<f64>() < 0.25 {
                    let (ws, we) = enc.window_span();
                    if we > ws + 4 {
                        let w = 4 + (rng.gen::<u64>() % 8).min(we - ws);
                        let start = we.saturating_sub(w).max(ws);
                        let cnt = (we - start) as u16;
                        if let Some(r) = enc.generate_repair_range(start, cnt) {
                            if rng.gen::<f64>() >= loss {
                                pending.push(r);
                            }
                        }
                    }
                }
                // reorder: flush pending in a shuffled burst now and then
                if rng.gen::<f64>() < 0.4 || i == n_src - 1 {
                    pending.shuffle(&mut rng);
                    wire.append(&mut pending);
                }
                // duplicate an already-queued symbol occasionally
                if !wire.is_empty() && rng.gen::<f64>() < 0.1 {
                    let dup = wire[rng.gen::<usize>() % wire.len()].clone();
                    wire.push(dup);
                }
            }
            pending.shuffle(&mut rng);
            wire.append(&mut pending);

            let mut leg_set: BTreeMap<u64, Bytes> = BTreeMap::new();
            let mut uni_set: BTreeMap<u64, Bytes> = BTreeMap::new();
            for sym in &wire {
                for (s, d) in d_leg.add_symbol(sym) {
                    leg_set.insert(s, d);
                }
                for (s, d) in d_uni.add_symbol(sym) {
                    uni_set.insert(s, d);
                }
                for (s, d) in &leg_set {
                    let u = uni_set.get(s);
                    assert!(
                        u.is_some(),
                        "unified must deliver everything legacy does (seed {seed}, seq {s})"
                    );
                    assert_eq!(u.unwrap(), d, "bytes diverged at seq {s} (seed {seed})");
                }
            }
            total_extra += uni_set.len() - leg_set.len();
        }
        println!(
            "unified extra deliveries over legacy sliding (rank-loss defect recovered): {total_extra}"
        );
    }

    /// Decoder-side advance parity and probe surfaces vs the legacy machine.
    #[test]
    fn unified_advance_and_probe_parity() {
        let ss = 32u16;
        let mut enc = RlcWindowEncoder::new(ss);
        let mut d_leg = RlcWindowDecoder::new(ss);
        let mut d_uni = UnifiedDecoder::new(ss);
        let mut syms = Vec::new();
        for i in 0..40u64 {
            syms.push(enc.add_source(&vec![(i + 1) as u8; 24]));
        }
        for (i, s) in syms.iter().enumerate() {
            if i % 7 != 3 {
                d_leg.add_symbol(s);
                d_uni.add_symbol(s);
            }
        }
        assert_eq!(d_uni.frontier_probe(0, 39), d_leg.frontier_probe(0, 39));
        assert_eq!(d_uni.rank_in(0, 40), d_leg.rank_in(0, 40));
        d_leg.advance(20);
        d_uni.advance(20);
        assert_eq!(d_uni.frontier_probe(20, 39), d_leg.frontier_probe(20, 39));
        // a trailing repair over the retained region still recovers the hole
        let r = enc.generate_repair_range(28, 8).unwrap(); // covers hole 31
        let a = sorted(d_leg.add_symbol(&r));
        let b = sorted(d_uni.add_symbol(&r));
        assert_eq!(
            a.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
            b.iter().map(|(s, _)| *s).collect::<Vec<_>>()
        );
        assert!(b.iter().any(|(s, _)| *s == 31));
    }

    /// The legacy rank-loss defect, isolated: a repair pivots at a lost seq;
    /// the source then arrives late. Legacy discards the displaced row and
    /// cannot recover a second hole; unified re-assimilates it and can.
    #[test]
    fn unified_recovers_rank_legacy_drops_on_late_source() {
        let ss = 32u16;
        let mut enc = RlcWindowEncoder::new(ss);
        let mut d_leg = RlcWindowDecoder::new(ss);
        let mut d_uni = UnifiedDecoder::new(ss);
        let mut syms = Vec::new();
        for i in 0..10u64 {
            syms.push(enc.add_source(&vec![(i + 1) as u8; 24]));
        }
        // holes: 2 and 5. Feed everything else FIRST.
        for (i, s) in syms.iter().enumerate() {
            if i != 2 && i != 5 {
                d_leg.add_symbol(s);
                d_uni.add_symbol(s);
            }
        }
        // one repair over [0,10): reduces to a 2-unknown row pivoting at 2.
        let r = enc.generate_repair_range(0, 10).unwrap();
        assert!(d_leg.add_symbol(&r).is_empty());
        assert!(d_uni.add_symbol(&r).is_empty());
        // seq 2's source arrives LATE: the pivot row is displaced. Legacy
        // discards it (rank lost); unified folds it into an equation over
        // seq 5 — recovering BOTH.
        let a = sorted(d_leg.add_symbol(&syms[2]));
        let b = sorted(d_uni.add_symbol(&syms[2]));
        let sa: Vec<u64> = a.iter().map(|(s, _)| *s).collect();
        let sb: Vec<u64> = b.iter().map(|(s, _)| *s).collect();
        assert_eq!(sa, vec![2], "legacy: only the late source itself");
        assert_eq!(sb, vec![2, 5], "unified: displaced row recovers seq 5 too");
        assert_eq!(&b[1].1[..24], &vec![6u8; 24][..]);
    }
}
