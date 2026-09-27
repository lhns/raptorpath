//! RLC sliding window encoder/decoder.
//!
//! Implements the `WindowEncoder` and `WindowDecoder` traits using Random Linear
//! Codes over GF(2^8). The encoder maintains a window of recent source symbols and
//! generates repair as random linear combinations over the window. The decoder uses
//! incremental Gaussian elimination with cascading recovery.
//!
//! Wire format:
//! - Source symbols: `block_id` = global seq, `data` = [u16 LE orig_len][padded_packet]
//! - Repair symbols: `block_id` = window_end_seq, `data` =
//!     [window_start(8 LE)][window_count(2 LE)][repair_index(4 LE)][coded_data]

use bytes::Bytes;
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use super::gf256;
use super::traits::{FecBackend, WireSymbol};
use super::window_traits::{WindowDecoder, WindowEncoder};

pub use gf256::generate_window_coefficients;

/// Repair symbol wire header size: 8 (window_start) + 2 (window_count) + 4 (repair_index) = 14
const REPAIR_HEADER_SIZE: usize = 14;

// ---------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------

/// RLC sliding window encoder.
pub struct RlcWindowEncoder {
    symbol_size: u16,
    /// Window of source symbols: (seq, padded_data)
    window: VecDeque<(u64, Vec<u8>)>,
    /// Next sequence number to assign
    next_seq: u64,
    /// Monotonic repair index counter
    repair_counter: u32,
}

impl RlcWindowEncoder {
    pub fn new(symbol_size: u16) -> Self {
        Self {
            symbol_size,
            window: VecDeque::new(),
            next_seq: 0,
            repair_counter: 0,
        }
    }
}

impl WindowEncoder for RlcWindowEncoder {
    fn add_source(&mut self, data: &[u8]) -> WireSymbol {
        let seq = self.next_seq;
        self.next_seq += 1;

        // Pad to symbol_size
        let mut padded = vec![0u8; self.symbol_size as usize];
        let copy_len = data.len().min(self.symbol_size as usize);
        padded[..copy_len].copy_from_slice(&data[..copy_len]);

        self.window.push_back((seq, padded.clone()));

        WireSymbol {
            block_id: seq,
            payload_id: 0,
            is_repair: false,
            data: padded,
            backend: FecBackend::Rlc,
        }
    }

    fn generate_repair(&mut self) -> WireSymbol {
        let symbol_size = self.symbol_size as usize;
        let window_count = self.window.len() as u16;

        if window_count == 0 {
            // Empty window — return a zero repair symbol
            return WireSymbol {
                block_id: 0,
                payload_id: self.repair_counter,
                is_repair: true,
                data: vec![0u8; REPAIR_HEADER_SIZE + symbol_size],
                backend: FecBackend::Rlc,
            };
        }

        let window_start = self.window.front().unwrap().0;
        let window_end = self.window.back().unwrap().0;
        let repair_index = self.repair_counter;
        self.repair_counter += 1;

        let coeffs = generate_window_coefficients(window_start, window_count, repair_index);

        // Compute repair = Σ coeffs[i] * source[i] over GF(2^8)
        let mut coded = vec![0u8; symbol_size];
        for (i, (_, src_data)) in self.window.iter().enumerate() {
            gf256::mul_acc_slice(coeffs[i], src_data, &mut coded);
        }

        // Wire format: [window_start(8)][window_count(2)][repair_index(4)][coded_data]
        let mut wire_data = Vec::with_capacity(REPAIR_HEADER_SIZE + symbol_size);
        wire_data.extend_from_slice(&window_start.to_le_bytes());
        wire_data.extend_from_slice(&window_count.to_le_bytes());
        wire_data.extend_from_slice(&repair_index.to_le_bytes());
        wire_data.extend_from_slice(&coded);

        WireSymbol {
            block_id: window_end,
            payload_id: repair_index,
            is_repair: true,
            data: wire_data,
            backend: FecBackend::Rlc,
        }
    }

    fn generate_repair_range(&mut self, start: u64, count: u16) -> Option<WireSymbol> {
        if count == 0 {
            return None;
        }
        let (w_start, w_end) = self.window_span();
        if self.window.is_empty() {
            return None;
        }
        let end = start.saturating_add(count as u64);
        // The full [start, end) range must be retained: the receiver derives a
        // coefficient for EVERY window position from (start, count, index), so a
        // missing (evicted) source would leave a phantom coefficient with no
        // matching term — an inconsistent equation. Bail if not fully covered.
        if start < w_start || end > w_end + 1 {
            return None;
        }
        let symbol_size = self.symbol_size as usize;
        let repair_index = self.repair_counter;
        self.repair_counter += 1;

        let coeffs = generate_window_coefficients(start, count, repair_index);
        let mut coded = vec![0u8; symbol_size];
        for i in 0..count as u64 {
            let seq = start + i;
            let offset = (seq - w_start) as usize;
            match self.window.get(offset) {
                Some((s, src_data)) if *s == seq => {
                    gf256::mul_acc_slice(coeffs[i as usize], src_data, &mut coded);
                }
                // Window is dense & contiguous, so this cannot happen given the
                // range check above; guard anyway rather than emit a bad symbol.
                _ => return None,
            }
        }

        let mut wire_data = Vec::with_capacity(REPAIR_HEADER_SIZE + symbol_size);
        wire_data.extend_from_slice(&start.to_le_bytes());
        wire_data.extend_from_slice(&count.to_le_bytes());
        wire_data.extend_from_slice(&repair_index.to_le_bytes());
        wire_data.extend_from_slice(&coded);

        Some(WireSymbol {
            block_id: end - 1,
            payload_id: repair_index,
            is_repair: true,
            data: wire_data,
            backend: FecBackend::Rlc,
        })
    }

    fn window_span(&self) -> (u64, u64) {
        match (self.window.front(), self.window.back()) {
            (Some((start, _)), Some((end, _))) => (*start, *end),
            _ => (0, 0),
        }
    }

    fn advance(&mut self, oldest_seq: u64) {
        while let Some((seq, _)) = self.window.front() {
            if *seq < oldest_seq {
                self.window.pop_front();
            } else {
                break;
            }
        }
    }

    fn window_size(&self) -> usize {
        self.window.len()
    }

    fn get_source(&self, seq: u64) -> Option<WireSymbol> {
        let (start, _) = self.window_span();
        if seq < start {
            return None;
        }
        let offset = (seq - start) as usize;
        let (front_seq, data) = self.window.get(offset)?;
        if *front_seq != seq {
            return None;
        }
        Some(WireSymbol {
            block_id: seq,
            payload_id: 0,
            is_repair: false,
            data: data.clone(),
            backend: FecBackend::Rlc,
        })
    }
}

// ---------------------------------------------------------------------------
// Decoder
// ---------------------------------------------------------------------------

/// A row in the incremental GE matrix. Each row is pivoted on a specific seq.
struct PivotRow {
    /// The pivot variable (seq) for this row — its coefficient is always 1.
    pivot_seq: u64,
    /// Remaining coefficients (excluding the pivot, which is implicit 1).
    coefficients: BTreeMap<u64, u8>,
    /// Row data (already divided by the original pivot coefficient).
    data: Vec<u8>,
}

/// RLC sliding window decoder.
pub struct RlcWindowDecoder {
    symbol_size: u16,
    /// Recovered source symbols: seq → data
    recovered: BTreeMap<u64, Vec<u8>>,
    /// Incremental GE pivot table: pivot_seq → PivotRow.
    /// Each row has been normalized so that the pivot coefficient is 1.
    pivots: BTreeMap<u64, PivotRow>,
    /// Sequences that have been output (to avoid duplicate delivery)
    output: BTreeSet<u64>,
    /// Deduplication: seen (block_id, payload_id, is_repair) tuples
    seen: HashSet<(u64, u32, bool)>,
    /// Total symbols fed
    total_fed: u64,
    /// Total repair symbols fed
    repairs_fed: u64,
    /// Repair symbols that contributed to recovery
    repairs_useful: u64,
}

impl RlcWindowDecoder {
    pub fn new(symbol_size: u16) -> Self {
        Self {
            symbol_size,
            recovered: BTreeMap::new(),
            pivots: BTreeMap::new(),
            output: BTreeSet::new(),
            seen: HashSet::new(),
            total_fed: 0,
            repairs_fed: 0,
            repairs_useful: 0,
        }
    }

    /// Try to insert an equation (coefficients + data) into the pivot table.
    /// Returns any newly recovered (seq, data) pairs.
    fn insert_equation(
        &mut self,
        mut coefficients: BTreeMap<u64, u8>,
        mut data: Vec<u8>,
    ) -> Vec<(u64, Bytes)> {
        let symbol_size = self.symbol_size as usize;

        // Step 1: Eliminate against recovered sources
        let known_seqs: Vec<u64> = coefficients
            .keys()
            .filter(|seq| self.recovered.contains_key(seq))
            .copied()
            .collect();
        for seq in known_seqs {
            if let Some(&coeff) = coefficients.get(&seq) {
                let src = self.recovered.get(&seq).unwrap();
                gf256::mul_acc_slice(coeff, src, &mut data);
                coefficients.remove(&seq);
            }
        }

        // Step 2: Eliminate against existing pivot rows
        let pivot_seqs: Vec<u64> = coefficients
            .keys()
            .filter(|seq| self.pivots.contains_key(seq))
            .copied()
            .collect();
        for seq in pivot_seqs {
            if let Some(&coeff) = coefficients.get(&seq) {
                let pivot = self.pivots.get(&seq).unwrap();
                // Eliminate: subtract coeff * pivot_row from this equation
                // pivot_row has implicit coeff=1 on pivot_seq
                gf256::mul_acc_slice(coeff, &pivot.data, &mut data);
                // Eliminate the pivot variable
                coefficients.remove(&seq);
                // Add pivot's remaining variables * coeff
                for (&other_seq, &other_coeff) in &pivot.coefficients {
                    let combined = gf256::mul(coeff, other_coeff);
                    let entry = coefficients.entry(other_seq).or_insert(0);
                    *entry = gf256::add(*entry, combined);
                    if *entry == 0 {
                        coefficients.remove(&other_seq);
                    }
                }
            }
        }

        // Step 3: Check what's left
        if coefficients.is_empty() {
            // Fully reduced — redundant equation
            return vec![];
        }

        if coefficients.len() == 1 {
            // Single unknown → recover immediately
            let (&seq, &coeff) = coefficients.iter().next().unwrap();
            let inv = gf256::inv(coeff);
            let mut recovered_data = vec![0u8; symbol_size];
            gf256::mul_slice(inv, &data, &mut recovered_data);
            self.recovered.insert(seq, recovered_data.clone());

            let mut result = Vec::new();
            if self.output.insert(seq) {
                result.push((seq, Bytes::from(recovered_data)));
            }

            // Cascade: this recovery may resolve pivot rows
            result.extend(self.cascade_from_recovered(seq));
            return result;
        }

        // Multiple unknowns → pick the first as pivot, normalize, and store
        let (&pivot_seq, &pivot_coeff) = coefficients.iter().next().unwrap();
        coefficients.remove(&pivot_seq);

        // Normalize: divide entire row by pivot_coeff so pivot becomes 1
        let inv = gf256::inv(pivot_coeff);
        let mut norm_data = vec![0u8; symbol_size];
        gf256::mul_slice(inv, &data, &mut norm_data);

        let mut norm_coeffs = BTreeMap::new();
        for (&seq, &coeff) in &coefficients {
            norm_coeffs.insert(seq, gf256::mul(inv, coeff));
        }

        self.pivots.insert(pivot_seq, PivotRow {
            pivot_seq,
            coefficients: norm_coeffs,
            data: norm_data,
        });

        vec![]
    }

    /// After recovering a source symbol, check if any pivot rows now reduce
    /// to single-unknown (i.e., the recovered seq was in their coefficients).
    fn cascade_from_recovered(&mut self, initial_seq: u64) -> Vec<(u64, Bytes)> {
        let mut result = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(initial_seq);

        while let Some(seq) = queue.pop_front() {
            let src_data = match self.recovered.get(&seq) {
                Some(d) => d.clone(),
                None => continue,
            };

            // Also check if `seq` itself was a pivot — if so, that pivot is now resolved
            if let Some(pivot) = self.pivots.remove(&seq) {
                // This pivot row had pivot_seq = seq with implicit coeff=1
                // The row is: seq + Σ(other_coeffs * other_seqs) = data
                // Since we know seq now, it's redundant — but let's not waste it;
                // the pivot was for this seq, so it's just confirming what we already know.
                let _ = pivot;
            }

            // Reduce all other pivot rows that reference this seq
            let affected_pivots: Vec<u64> = self.pivots
                .iter()
                .filter(|(_, row)| row.coefficients.contains_key(&seq))
                .map(|(&k, _)| k)
                .collect();

            for pivot_seq in affected_pivots {
                let row = self.pivots.get_mut(&pivot_seq).unwrap();
                if let Some(&coeff) = row.coefficients.get(&seq) {
                    // Eliminate: row -= coeff * src_data (pivot coeff is implicit 1, not affected)
                    gf256::mul_acc_slice(coeff, &src_data, &mut row.data);
                    row.coefficients.remove(&seq);

                    // Check if this pivot row now has 0 remaining unknowns
                    // (meaning the pivot variable is the only unknown → recover it)
                    if row.coefficients.is_empty() {
                        let recovered_seq = row.pivot_seq;
                        let recovered_data = row.data.clone();
                        self.pivots.remove(&recovered_seq);

                        self.recovered.insert(recovered_seq, recovered_data.clone());
                        if self.output.insert(recovered_seq) {
                            result.push((recovered_seq, Bytes::from(recovered_data)));
                        }
                        queue.push_back(recovered_seq);
                    }
                }
            }
        }

        result
    }
}

impl WindowDecoder for RlcWindowDecoder {
    fn add_symbol(&mut self, symbol: &WireSymbol) -> Vec<(u64, Bytes)> {
        if symbol.backend != FecBackend::Rlc {
            return vec![];
        }

        // Deduplicate
        let key = (symbol.block_id, symbol.payload_id, symbol.is_repair);
        if !self.seen.insert(key) {
            return vec![];
        }

        self.total_fed += 1;

        if !symbol.is_repair {
            // Source symbol: block_id is the sequence number
            let seq = symbol.block_id;
            let symbol_size = self.symbol_size as usize;

            let mut data = vec![0u8; symbol_size];
            let copy_len = symbol.data.len().min(symbol_size);
            data[..copy_len].copy_from_slice(&symbol.data[..copy_len]);

            self.recovered.insert(seq, data.clone());

            let mut result = Vec::new();
            if self.output.insert(seq) {
                result.push((seq, Bytes::from(data)));
            }

            // Cascade: this source may resolve pivot rows
            result.extend(self.cascade_from_recovered(seq));
            result
        } else {
            // Repair symbol: parse header
            if symbol.data.len() < REPAIR_HEADER_SIZE {
                return vec![];
            }

            self.repairs_fed += 1;

            let window_start =
                u64::from_le_bytes(symbol.data[0..8].try_into().unwrap());
            let window_count =
                u16::from_le_bytes(symbol.data[8..10].try_into().unwrap());
            let repair_index =
                u32::from_le_bytes(symbol.data[10..14].try_into().unwrap());
            let coded = &symbol.data[14..];

            if window_count == 0 {
                return vec![];
            }

            let symbol_size = self.symbol_size as usize;
            let mut data = vec![0u8; symbol_size];
            let copy_len = coded.len().min(symbol_size);
            data[..copy_len].copy_from_slice(&coded[..copy_len]);

            // Generate coefficients for this repair symbol
            let coeffs =
                generate_window_coefficients(window_start, window_count, repair_index);

            // Build coefficient map: seq → coeff
            let mut coeff_map = BTreeMap::new();
            for i in 0..window_count as u64 {
                let seq = window_start + i;
                coeff_map.insert(seq, coeffs[i as usize]);
            }

            // Insert into incremental GE system
            let recovered = self.insert_equation(coeff_map, data);
            if !recovered.is_empty() {
                self.repairs_useful += 1;
            }
            recovered
        }
    }

    fn advance(&mut self, oldest_seq: u64) {
        // Remove recovered symbols older than oldest_seq
        let old_seqs: Vec<u64> = self
            .recovered
            .range(..oldest_seq)
            .map(|(k, _)| *k)
            .collect();
        for seq in old_seqs {
            self.recovered.remove(&seq);
        }

        // Remove output tracking older than oldest_seq
        let old_output: Vec<u64> = self.output.range(..oldest_seq).copied().collect();
        for seq in old_output {
            self.output.remove(&seq);
        }

        // Remove pivot rows for old sequences
        let old_pivots: Vec<u64> = self.pivots.range(..oldest_seq).map(|(k, _)| *k).collect();
        for seq in old_pivots {
            self.pivots.remove(&seq);
        }
        // Also clean coefficients in remaining pivot rows
        for (_, row) in self.pivots.iter_mut() {
            row.coefficients.retain(|seq, _| *seq >= oldest_seq);
        }

        // Clean up seen set for old sequences
        self.seen
            .retain(|(block_id, _, _)| *block_id >= oldest_seq);
    }

    fn rank_in(&self, start: u64, count: u64) -> u64 {
        // Independent rank restricted to the generation span. In generation
        // mode every coded symbol's coefficients lie inside one generation, so a
        // pivot row pivoted in [start,end) is an independent DoF for exactly that
        // generation, and a recovered source is a solved DoF. The two sets are
        // disjoint (a recovered seq is never a live pivot), so their counts sum
        // to the generation's current rank.
        let end = start.saturating_add(count);
        let recovered = self.recovered.range(start..end).count() as u64;
        let pivots = self.pivots.range(start..end).count() as u64;
        recovered + pivots
    }

    fn frontier_probe(&self, frontier: u64, horizon: u64) -> (u64, u64) {
        // Span is contiguous in seq space (sender numbers sources densely), so
        // holes = span_len − recovered_in_span. Buffered equations = pivot rows
        // whose pivot_seq lies in the span (each is one independent DoF the
        // decoder is holding, waiting for enough rank to solve the frontier).
        let end = horizon.saturating_add(1);
        if end <= frontier {
            return (0, 0);
        }
        let span = end - frontier;
        let recovered = self.recovered.range(frontier..end).count() as u64;
        let holes = span.saturating_sub(recovered);
        let pivots = self.pivots.range(frontier..end).count() as u64;
        (holes, pivots)
    }

    fn seq_probe(&self, seq: u64) -> (bool, bool, bool) {
        (
            self.seen.contains(&(seq, 0, false)),
            self.recovered.contains_key(&seq),
            self.output.contains(&seq),
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

    /// Sliding-decoder counterpart of the unified decoder's snapshot (pivot
    /// rows / widest row / state sizes).
    fn diag_stats(&self) -> Option<String> {
        let (mut max_span, mut coeff_n) = (0usize, 0usize);
        for r in self.pivots.values() {
            let span = r
                .coefficients
                .keys()
                .next_back()
                .zip(r.coefficients.keys().next())
                .map(|(hi, lo)| (hi - lo + 1) as usize)
                .unwrap_or(0);
            max_span = max_span.max(span);
            coeff_n += r.coefficients.len();
        }
        Some(format!(
            "rows={} max_span={} coeff_n={} recovered={} seen={}",
            self.pivots.len(),
            max_span,
            coeff_n,
            self.recovered.len(),
            self.seen.len()
        ))
    }
}

#[cfg(test)]
mod tests;
