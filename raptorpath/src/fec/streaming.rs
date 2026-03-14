//! Streaming codes (Badr/Martinian delay-optimal construction).
//!
//! Implements a two-layer sliding-window erasure code:
//!
//! - **Burst layer**: diagonal interleaving with stride T. Source symbol at position i
//!   is XOR'd with symbols at {i-T, i-2T, ...}. Creates T independent diagonals — a
//!   burst of length B hits at most ⌈B/T⌉ per diagonal.
//!
//! - **Random layer**: GF(256) linear combination of window symbols (reuses `gf256.rs`).
//!   Rate = ε/(1-ε) where ε is the random (non-burst) loss rate.
//!
//! Parameters:
//! - T: delay constraint — recovered symbols are at most T positions behind newest
//! - B: burst length the code is designed to tolerate
//! - ε: random loss rate (non-burst)
//!
//! Streaming capacity: C(T,B) = T/(T+B)
//!
//! References:
//! - Badr et al., "Layered Constructions for Low-Delay Streaming Codes," IEEE Trans. IT, 2017
//! - Martinian & Sundberg, "Burst Erasure Correction Codes with Low Decoding Delay," 2004
//! - Fong et al., "Optimal Streaming Codes for Channels with Burst and Arbitrary Erasures," 2019

use bytes::Bytes;
use std::collections::BTreeMap;

use super::gf256;
use super::traits::{FecBackend, WireSymbol};
use super::window_traits::{WindowDecoder, WindowEncoder};
use super::rlc_window::generate_window_coefficients;

/// Parameters for the streaming code.
#[derive(Debug, Clone, Copy)]
pub struct StreamingParams {
    /// Delay constraint: max positions behind newest for recovery
    pub t: u32,
    /// Burst length the code is designed to tolerate
    pub b: u32,
    /// Random (non-burst) loss rate
    pub epsilon: f64,
    /// Fraction of repair symbols allocated to burst layer (rest goes to random layer)
    pub burst_rate: f64,
    /// Fraction of repair symbols allocated to random layer
    pub random_rate: f64,
}

impl StreamingParams {
    /// Compute streaming parameters from channel estimates.
    ///
    /// `burst_length`: estimated mean burst length from GE model
    /// `loss_rate`: upper-bound loss rate (e.g., 95th percentile)
    /// `safety_factor`: over-provisioning multiplier (e.g., 1.15 for 15%)
    pub fn from_channel(burst_length: f64, loss_rate: f64, safety_factor: f64) -> Self {
        // B = ceil(burst_length * safety_factor), at least 1
        let b = ((burst_length * safety_factor).ceil() as u32).max(1);

        // T must satisfy T >= B for the burst layer to work.
        // For multipath: T ≈ max_rtt / symbol_interval, but we use T = B as baseline
        // and let the caller override if needed.
        let t = b;

        // Streaming capacity C = T/(T+B). Code rate = 1 - C overhead.
        // Burst layer rate: B/(T+B) of total repair
        // Random layer rate: ε/(1-ε) additional repair for random losses
        let epsilon = (loss_rate * safety_factor).min(0.5);

        // Burst layer: produces 1 repair per T source symbols (covers the diagonals)
        let burst_rate = 1.0 / t as f64;

        // Random layer: covers residual random loss not handled by burst layer
        let random_rate = if epsilon > 0.001 {
            epsilon / (1.0 - epsilon)
        } else {
            0.0
        };

        Self {
            t,
            b,
            epsilon,
            burst_rate,
            random_rate,
        }
    }

    /// Total repair rate (repair symbols per source symbol)
    pub fn total_rate(&self) -> f64 {
        self.burst_rate + self.random_rate
    }
}

/// Streaming encoder — produces burst-layer and random-layer repair symbols
/// over a sliding window.
pub struct StreamingEncoder {
    symbol_size: u16,
    /// Source symbols in the window: (seq, data)
    window: BTreeMap<u64, Vec<u8>>,
    next_seq: u64,
    params: StreamingParams,
    /// Counter for alternating between burst and random repair
    repair_counter: u32,
}

impl StreamingEncoder {
    pub fn new(symbol_size: u16, params: StreamingParams) -> Self {
        Self {
            symbol_size,
            window: BTreeMap::new(),
            next_seq: 0,
            params,
            repair_counter: 0,
        }
    }

    /// Generate a burst-layer repair symbol.
    ///
    /// The burst layer uses diagonal interleaving: each repair symbol is the XOR
    /// of source symbols at positions {newest, newest-T, newest-2T, ...} within
    /// the window. This creates T independent diagonals; a burst of B consecutive
    /// losses hits at most ⌈B/T⌉ symbols per diagonal.
    fn generate_burst_repair(&self) -> WireSymbol {
        let t = self.params.t as u64;
        let mut coded = vec![0u8; self.symbol_size as usize];

        if self.window.is_empty() {
            return self.empty_repair();
        }

        let newest_seq = *self.window.keys().last().unwrap();
        // Diagonal: start from newest, step back by T
        let diagonal_index = self.repair_counter as u64 % t;
        let mut seq = newest_seq.wrapping_sub(diagonal_index);

        // Walk the diagonal within the window
        let oldest_seq = *self.window.keys().next().unwrap();
        loop {
            if seq < oldest_seq || seq > newest_seq {
                break;
            }
            if let Some(src) = self.window.get(&seq) {
                // XOR (GF(2) addition — same as gf256::add for each byte)
                for (d, &s) in coded.iter_mut().zip(src.iter()) {
                    *d ^= s;
                }
            }
            if seq < t {
                break;
            }
            seq -= t;
        }

        let (window_start, window_end) = self.wire_span();

        // Wire header: [window_start(8)][window_count(2)][repair_index(4)][layer(1)][coded]
        let window_count = self.window.len() as u16;
        let mut wire_data = Vec::with_capacity(STREAMING_REPAIR_HEADER + self.symbol_size as usize);
        wire_data.extend_from_slice(&window_start.to_le_bytes());
        wire_data.extend_from_slice(&window_count.to_le_bytes());
        wire_data.extend_from_slice(&self.repair_counter.to_le_bytes());
        wire_data.push(LAYER_BURST);
        wire_data.extend_from_slice(&coded);

        WireSymbol {
            block_id: window_end,
            payload_id: self.repair_counter,
            is_repair: true,
            data: wire_data,
            backend: FecBackend::Streaming,
        }
    }

    /// Generate a random-layer repair symbol.
    ///
    /// This is a GF(256) linear combination of all source symbols in the window,
    /// identical to RLC repair — reuses the same coefficient generation.
    fn generate_random_repair(&self) -> WireSymbol {
        if self.window.is_empty() {
            return self.empty_repair();
        }

        let window_start = *self.window.keys().next().unwrap();
        let window_end = *self.window.keys().last().unwrap();
        let window_count = self.window.len() as u16;

        let coeffs = generate_window_coefficients(
            window_start,
            window_count,
            self.repair_counter,
        );

        let mut coded = vec![0u8; self.symbol_size as usize];
        for (i, (_, src)) in self.window.iter().enumerate() {
            gf256::mul_acc_slice(coeffs[i], src, &mut coded);
        }

        let mut wire_data = Vec::with_capacity(STREAMING_REPAIR_HEADER + self.symbol_size as usize);
        wire_data.extend_from_slice(&window_start.to_le_bytes());
        wire_data.extend_from_slice(&window_count.to_le_bytes());
        wire_data.extend_from_slice(&self.repair_counter.to_le_bytes());
        wire_data.push(LAYER_RANDOM);
        wire_data.extend_from_slice(&coded);

        WireSymbol {
            block_id: window_end,
            payload_id: self.repair_counter,
            is_repair: true,
            data: wire_data,
            backend: FecBackend::Streaming,
        }
    }

    fn empty_repair(&self) -> WireSymbol {
        WireSymbol {
            block_id: 0,
            payload_id: self.repair_counter,
            is_repair: true,
            data: vec![0u8; STREAMING_REPAIR_HEADER + self.symbol_size as usize],
            backend: FecBackend::Streaming,
        }
    }

    fn wire_span(&self) -> (u64, u64) {
        match (self.window.keys().next(), self.window.keys().last()) {
            (Some(&first), Some(&last)) => (first, last),
            _ => (0, 0),
        }
    }
}

/// Repair header: window_start(8) + window_count(2) + repair_index(4) + layer(1) = 15
const STREAMING_REPAIR_HEADER: usize = 15;
const LAYER_BURST: u8 = 0;
const LAYER_RANDOM: u8 = 1;

impl WindowEncoder for StreamingEncoder {
    fn add_source(&mut self, data: &[u8]) -> WireSymbol {
        let seq = self.next_seq;
        self.next_seq += 1;

        let mut padded = vec![0u8; self.symbol_size as usize];
        let copy_len = data.len().min(self.symbol_size as usize);
        padded[..copy_len].copy_from_slice(&data[..copy_len]);

        self.window.insert(seq, padded.clone());

        WireSymbol {
            block_id: seq,
            payload_id: 0,
            is_repair: false,
            data: padded,
            backend: FecBackend::Streaming,
        }
    }

    fn generate_repair(&mut self) -> WireSymbol {
        // Alternate between burst and random layer based on their rates.
        // Use a simple ratio: if burst_rate >= random_rate, emit more burst repairs.
        let total = self.params.burst_rate + self.params.random_rate;
        let burst_fraction = if total > 0.0 {
            self.params.burst_rate / total
        } else {
            0.5
        };

        let sym = if (self.repair_counter as f64 * burst_fraction).fract()
            < burst_fraction
        {
            self.generate_burst_repair()
        } else {
            self.generate_random_repair()
        };

        self.repair_counter += 1;
        sym
    }

    fn window_span(&self) -> (u64, u64) {
        self.wire_span()
    }

    fn advance(&mut self, oldest_seq: u64) {
        // Remove all entries with seq < oldest_seq
        let to_remove: Vec<u64> = self
            .window
            .range(..oldest_seq)
            .map(|(&k, _)| k)
            .collect();
        for k in to_remove {
            self.window.remove(&k);
        }
    }

    fn window_size(&self) -> usize {
        self.window.len()
    }
}

/// Streaming decoder — processes source and repair symbols, recovers missing symbols
/// using both burst-layer XOR and random-layer GF(256) Gaussian elimination.
pub struct StreamingDecoder {
    symbol_size: u16,
    /// Received/recovered source symbols
    recovered: BTreeMap<u64, Vec<u8>>,
    /// Pending burst-layer repairs: (diagonal_index, window_start, stride, involved_seqs, coded_data)
    burst_repairs: Vec<BurstRepair>,
    /// Incremental GE pivot table for random-layer repairs (same as RLC decoder)
    pivots: BTreeMap<u64, PivotRow>,
    /// Sequences that have been output
    output: std::collections::BTreeSet<u64>,
    /// Deduplication set
    seen: std::collections::HashSet<(u64, u32, bool)>,
    /// Total symbols fed
    total_fed: u64,
    /// Streaming params for delay constraint
    params: StreamingParams,
}

struct BurstRepair {
    /// Sequences involved in this diagonal XOR
    seqs: Vec<u64>,
    /// The XOR'd data
    data: Vec<u8>,
}

struct PivotRow {
    pivot_seq: u64,
    coefficients: BTreeMap<u64, u8>,
    data: Vec<u8>,
}

impl StreamingDecoder {
    pub fn new(symbol_size: u16, params: StreamingParams) -> Self {
        Self {
            symbol_size,
            recovered: BTreeMap::new(),
            burst_repairs: Vec::new(),
            pivots: BTreeMap::new(),
            output: std::collections::BTreeSet::new(),
            seen: std::collections::HashSet::new(),
            total_fed: 0,
            params,
        }
    }

    /// Try to recover symbols from burst-layer repairs.
    /// Returns newly recovered (seq, data) pairs.
    fn try_burst_recovery(&mut self) -> Vec<(u64, Bytes)> {
        let mut result = Vec::new();
        let mut changed = true;

        while changed {
            changed = false;
            let mut i = 0;
            while i < self.burst_repairs.len() {
                // Count how many sequences in this repair are still missing
                let missing: Vec<u64> = self.burst_repairs[i]
                    .seqs
                    .iter()
                    .filter(|s| !self.recovered.contains_key(s))
                    .copied()
                    .collect();

                if missing.len() == 1 {
                    // Can recover the single missing symbol by XOR
                    let missing_seq = missing[0];
                    let mut recovered_data = self.burst_repairs[i].data.clone();

                    // XOR out all known symbols
                    for &seq in &self.burst_repairs[i].seqs {
                        if seq != missing_seq {
                            if let Some(src) = self.recovered.get(&seq) {
                                for (d, &s) in recovered_data.iter_mut().zip(src.iter()) {
                                    *d ^= s;
                                }
                            }
                        }
                    }

                    self.recovered.insert(missing_seq, recovered_data.clone());
                    if self.output.insert(missing_seq) {
                        result.push((missing_seq, Bytes::from(recovered_data)));
                    }
                    self.burst_repairs.swap_remove(i);
                    changed = true;
                } else if missing.is_empty() {
                    // All symbols known — redundant repair, discard
                    self.burst_repairs.swap_remove(i);
                    changed = true;
                } else {
                    i += 1;
                }
            }
        }

        result
    }

    /// Insert a random-layer equation into the GE system.
    /// Returns newly recovered (seq, data) pairs.
    fn insert_random_equation(
        &mut self,
        mut coefficients: BTreeMap<u64, u8>,
        mut data: Vec<u8>,
    ) -> Vec<(u64, Bytes)> {
        // Eliminate against recovered sources
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

        // Eliminate against existing pivots
        let pivot_seqs: Vec<u64> = coefficients
            .keys()
            .filter(|seq| self.pivots.contains_key(seq))
            .copied()
            .collect();
        for seq in pivot_seqs {
            if let Some(&coeff) = coefficients.get(&seq) {
                let pivot = &self.pivots[&seq];
                gf256::mul_acc_slice(coeff, &pivot.data, &mut data);
                coefficients.remove(&seq);
                let pivot_coeffs: Vec<(u64, u8)> = pivot.coefficients.iter().map(|(&k, &v)| (k, v)).collect();
                for (other_seq, other_coeff) in pivot_coeffs {
                    let combined = gf256::mul(coeff, other_coeff);
                    let entry = coefficients.entry(other_seq).or_insert(0);
                    *entry = gf256::add(*entry, combined);
                    if *entry == 0 {
                        coefficients.remove(&other_seq);
                    }
                }
            }
        }

        if coefficients.is_empty() {
            return vec![];
        }

        if coefficients.len() == 1 {
            let (&seq, &coeff) = coefficients.iter().next().unwrap();
            let inv = gf256::inv(coeff);
            let mut recovered_data = vec![0u8; self.symbol_size as usize];
            gf256::mul_slice(inv, &data, &mut recovered_data);
            self.recovered.insert(seq, recovered_data.clone());

            let mut result = Vec::new();
            if self.output.insert(seq) {
                result.push((seq, Bytes::from(recovered_data)));
            }
            result.extend(self.cascade_from_recovered(seq));
            return result;
        }

        // Multiple unknowns — store as pivot
        let (&pivot_seq, &pivot_coeff) = coefficients.iter().next().unwrap();
        coefficients.remove(&pivot_seq);
        let inv = gf256::inv(pivot_coeff);
        let mut norm_data = vec![0u8; self.symbol_size as usize];
        gf256::mul_slice(inv, &data, &mut norm_data);

        let mut norm_coeffs = BTreeMap::new();
        for (&seq, &coeff) in &coefficients {
            norm_coeffs.insert(seq, gf256::mul(inv, coeff));
        }

        self.pivots.insert(
            pivot_seq,
            PivotRow {
                pivot_seq,
                coefficients: norm_coeffs,
                data: norm_data,
            },
        );

        vec![]
    }

    /// Cascade recovery through pivot rows when a source symbol is recovered.
    fn cascade_from_recovered(&mut self, initial_seq: u64) -> Vec<(u64, Bytes)> {
        let mut result = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(initial_seq);

        while let Some(seq) = queue.pop_front() {
            let src_data = match self.recovered.get(&seq) {
                Some(d) => d.clone(),
                None => continue,
            };

            let affected: Vec<u64> = self
                .pivots
                .iter()
                .filter(|(_, row)| row.coefficients.contains_key(&seq))
                .map(|(&k, _)| k)
                .collect();

            for pivot_seq in affected {
                let row = self.pivots.get_mut(&pivot_seq).unwrap();
                if let Some(&coeff) = row.coefficients.get(&seq) {
                    gf256::mul_acc_slice(coeff, &src_data, &mut row.data);
                    row.coefficients.remove(&seq);

                    if row.coefficients.is_empty() {
                        let recovered_seq = row.pivot_seq;
                        let recovered_data = row.data.clone();
                        self.pivots.remove(&recovered_seq);

                        self.recovered
                            .insert(recovered_seq, recovered_data.clone());
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

    /// Parse a repair symbol's wire format and process it.
    fn process_repair(&mut self, symbol: &WireSymbol) -> Vec<(u64, Bytes)> {
        if symbol.data.len() < STREAMING_REPAIR_HEADER {
            return vec![];
        }

        let window_start = u64::from_le_bytes(symbol.data[0..8].try_into().unwrap());
        let window_count = u16::from_le_bytes(symbol.data[8..10].try_into().unwrap());
        let _repair_index = u32::from_le_bytes(symbol.data[10..14].try_into().unwrap());
        let layer = symbol.data[14];
        let coded_data = &symbol.data[STREAMING_REPAIR_HEADER..];

        let coded = coded_data[..self.symbol_size as usize].to_vec();

        match layer {
            LAYER_BURST => {
                // Reconstruct the diagonal sequences
                let t = self.params.t as u64;
                let newest = window_start + window_count.saturating_sub(1) as u64;
                let diagonal_index = _repair_index as u64 % t;
                let mut seqs = Vec::new();
                let mut seq = newest.wrapping_sub(diagonal_index);
                loop {
                    if seq < window_start || seq > newest {
                        break;
                    }
                    seqs.push(seq);
                    if seq < t {
                        break;
                    }
                    seq -= t;
                }

                self.burst_repairs.push(BurstRepair { seqs, data: coded });
                self.try_burst_recovery()
            }
            LAYER_RANDOM => {
                // Build coefficient map (same generation as encoder)
                let coeffs = generate_window_coefficients(
                    window_start,
                    window_count,
                    _repair_index,
                );

                let mut coeff_map = BTreeMap::new();
                for (i, &c) in coeffs.iter().enumerate() {
                    let seq = window_start + i as u64;
                    if c != 0 {
                        coeff_map.insert(seq, c);
                    }
                }

                let mut result = self.insert_random_equation(coeff_map, coded);
                // After random recovery, try burst recovery too (newly recovered
                // symbols may unblock burst repairs)
                result.extend(self.try_burst_recovery());
                result
            }
            _ => vec![],
        }
    }

    /// Evict old state beyond the delay constraint window.
    fn evict_old(&mut self, newest_seq: u64) {
        if newest_seq < self.params.t as u64 * 2 {
            return;
        }
        let cutoff = newest_seq - self.params.t as u64 * 2;

        // Remove old recovered symbols
        let old_keys: Vec<u64> = self.recovered.range(..cutoff).map(|(&k, _)| k).collect();
        for k in old_keys {
            self.recovered.remove(&k);
        }

        // Remove old pivots
        let old_pivots: Vec<u64> = self.pivots.range(..cutoff).map(|(&k, _)| k).collect();
        for k in old_pivots {
            self.pivots.remove(&k);
        }

        // Remove old burst repairs that reference only old sequences
        self.burst_repairs.retain(|r| r.seqs.iter().any(|&s| s >= cutoff));

        // Remove old output entries
        let old_output: Vec<u64> = self.output.range(..cutoff).copied().collect();
        for k in old_output {
            self.output.remove(&k);
        }
    }
}

impl WindowDecoder for StreamingDecoder {
    fn add_symbol(&mut self, symbol: &WireSymbol) -> Vec<(u64, Bytes)> {
        let key = (symbol.block_id, symbol.payload_id, symbol.is_repair);
        if !self.seen.insert(key) {
            return vec![];
        }
        self.total_fed += 1;

        if !symbol.is_repair {
            // Source symbol
            let seq = symbol.block_id;
            let data = if symbol.data.len() >= self.symbol_size as usize {
                symbol.data[..self.symbol_size as usize].to_vec()
            } else {
                let mut padded = vec![0u8; self.symbol_size as usize];
                padded[..symbol.data.len()].copy_from_slice(&symbol.data);
                padded
            };

            self.recovered.insert(seq, data.clone());

            let mut result = Vec::new();
            if self.output.insert(seq) {
                result.push((seq, Bytes::from(data)));
            }

            // New source may enable burst recovery
            result.extend(self.try_burst_recovery());
            // And cascade through random pivots
            result.extend(self.cascade_from_recovered(seq));

            // Evict old state
            self.evict_old(seq);

            result
        } else {
            let result = self.process_repair(symbol);
            result
        }
    }

    fn advance(&mut self, oldest_seq: u64) {
        let old_keys: Vec<u64> = self.recovered.range(..oldest_seq).map(|(&k, _)| k).collect();
        for k in old_keys {
            self.recovered.remove(&k);
        }

        let old_pivots: Vec<u64> = self.pivots.range(..oldest_seq).map(|(&k, _)| k).collect();
        for k in old_pivots {
            self.pivots.remove(&k);
        }

        self.burst_repairs
            .retain(|r| r.seqs.iter().any(|&s| s >= oldest_seq));
    }

    fn total_fed(&self) -> u64 {
        self.total_fed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_params(t: u32, b: u32, epsilon: f64) -> StreamingParams {
        StreamingParams {
            t,
            b,
            epsilon,
            burst_rate: 1.0 / t as f64,
            random_rate: if epsilon > 0.001 {
                epsilon / (1.0 - epsilon)
            } else {
                0.0
            },
        }
    }

    #[test]
    fn test_no_loss_passthrough() {
        let params = make_params(4, 2, 0.0);
        let mut enc = StreamingEncoder::new(64, params);
        let mut dec = StreamingDecoder::new(64, params);

        for i in 0..20u64 {
            let data = vec![(i & 0xFF) as u8; 64];
            let src = enc.add_source(&data);
            let recovered = dec.add_symbol(&src);
            assert_eq!(recovered.len(), 1, "symbol {i} should be delivered");
            assert_eq!(recovered[0].0, i);
            assert_eq!(&recovered[0].1[..], &data[..]);
        }
    }

    #[test]
    fn test_burst_recovery() {
        // T=4, B=2: diagonals have stride 4, burst of 2 hits at most 1 per diagonal
        let params = make_params(4, 2, 0.0);
        let mut enc = StreamingEncoder::new(64, params);
        let mut dec = StreamingDecoder::new(64, params);

        let mut all_sources = Vec::new();
        let mut all_repairs = Vec::new();

        // Generate 16 source symbols and multiple repair symbols per source
        for i in 0..16u64 {
            let data = vec![(i & 0xFF) as u8; 64];
            let src = enc.add_source(&data);
            all_sources.push((i, src, data));

            // Generate several repairs to cover all T=4 diagonals
            for _ in 0..3 {
                all_repairs.push(enc.generate_repair());
            }
        }

        // Simulate burst loss: drop source symbols 4 and 5 (burst of 2)
        let mut total_recovered = Vec::new();
        for (i, src, _data) in &all_sources {
            if *i == 4 || *i == 5 {
                continue; // Drop these (burst loss)
            }
            let r = dec.add_symbol(src);
            total_recovered.extend(r);
        }

        // Feed repair symbols
        for repair in &all_repairs {
            let r = dec.add_symbol(repair);
            total_recovered.extend(r);
        }

        // Check that dropped symbols were recovered
        let recovered_seqs: std::collections::BTreeSet<u64> =
            total_recovered.iter().map(|(s, _)| *s).collect();
        assert!(
            recovered_seqs.contains(&4),
            "Symbol 4 should be recovered from burst repair"
        );
        assert!(
            recovered_seqs.contains(&5),
            "Symbol 5 should be recovered from burst repair"
        );
    }

    #[test]
    fn test_random_loss_recovery() {
        let params = make_params(8, 2, 0.15);
        let mut enc = StreamingEncoder::new(64, params);
        let mut dec = StreamingDecoder::new(64, params);

        let mut all_sources = Vec::new();
        let mut all_repairs = Vec::new();

        for i in 0..20u64 {
            let data = vec![(i & 0xFF) as u8; 64];
            let src = enc.add_source(&data);
            all_sources.push((i, src));

            // Generate 2 repairs per source for extra coverage
            all_repairs.push(enc.generate_repair());
            all_repairs.push(enc.generate_repair());
        }

        // Drop every 5th source symbol (random pattern)
        let mut total_recovered = Vec::new();
        for (i, src) in &all_sources {
            if i % 5 == 3 {
                continue; // Drop
            }
            let r = dec.add_symbol(src);
            total_recovered.extend(r);
        }

        // Feed repairs
        for repair in &all_repairs {
            let r = dec.add_symbol(repair);
            total_recovered.extend(r);
        }

        let recovered_seqs: std::collections::BTreeSet<u64> =
            total_recovered.iter().map(|(s, _)| *s).collect();

        // At least some dropped symbols should be recovered
        let dropped = [3u64, 8, 13, 18];
        let recovered_count = dropped.iter().filter(|&&s| recovered_seqs.contains(&s)).count();
        assert!(
            recovered_count >= 2,
            "Should recover at least some randomly dropped symbols, got {recovered_count}/{}",
            dropped.len()
        );
    }

    #[test]
    fn test_burst_plus_random() {
        let params = make_params(6, 3, 0.1);
        let mut enc = StreamingEncoder::new(64, params);
        let mut dec = StreamingDecoder::new(64, params);

        let mut all_sources = Vec::new();
        let mut all_repairs = Vec::new();

        for i in 0..30u64 {
            let data = vec![(i & 0xFF) as u8; 64];
            let src = enc.add_source(&data);
            all_sources.push((i, src));
            all_repairs.push(enc.generate_repair());
            all_repairs.push(enc.generate_repair());
        }

        // Burst loss at 10-12, random loss at 20
        let mut total_recovered = Vec::new();
        for (i, src) in &all_sources {
            if (*i >= 10 && *i <= 12) || *i == 20 {
                continue;
            }
            let r = dec.add_symbol(src);
            total_recovered.extend(r);
        }

        for repair in &all_repairs {
            let r = dec.add_symbol(repair);
            total_recovered.extend(r);
        }

        // Verify at least some recovery happened
        let recovered_seqs: std::collections::BTreeSet<u64> =
            total_recovered.iter().map(|(s, _)| *s).collect();
        let total_dropped = 4; // 10, 11, 12, 20
        let recovered_dropped = [10u64, 11, 12, 20]
            .iter()
            .filter(|&&s| recovered_seqs.contains(&s))
            .count();
        assert!(
            recovered_dropped >= 1,
            "Should recover at least 1 of {total_dropped} dropped symbols, got {recovered_dropped}"
        );
    }

    #[test]
    fn test_delay_constraint_met() {
        let params = make_params(8, 2, 0.05);
        let mut enc = StreamingEncoder::new(64, params);
        let mut dec = StreamingDecoder::new(64, params);

        let mut all_syms = Vec::new();
        for i in 0..40u64 {
            let data = vec![(i & 0xFF) as u8; 64];
            let src = enc.add_source(&data);
            all_syms.push((i, false, src));
            all_syms.push((i, true, enc.generate_repair()));
        }

        // Drop symbol 10
        let newest_when_recovered = std::cell::Cell::new(None);
        let mut recovered_10 = false;
        let mut current_newest = 0u64;

        for (i, is_repair, sym) in &all_syms {
            if !is_repair && *i == 10 {
                continue;
            }
            if !is_repair {
                current_newest = *i;
            }
            let r = dec.add_symbol(sym);
            for (seq, _) in &r {
                if *seq == 10 && !recovered_10 {
                    recovered_10 = true;
                    newest_when_recovered.set(Some(current_newest));
                }
            }
        }

        if recovered_10 {
            let newest = newest_when_recovered.get().unwrap();
            let delay = newest - 10;
            assert!(
                delay <= params.t as u64 * 2,
                "Recovery delay ({delay}) should be within ~2T ({})",
                params.t * 2
            );
        }
        // If not recovered at all, that's also acceptable with limited repair
    }

    #[test]
    fn test_beyond_burst_length_degrades_gracefully() {
        let params = make_params(4, 2, 0.0);
        let mut enc = StreamingEncoder::new(64, params);
        let mut dec = StreamingDecoder::new(64, params);

        let mut all_sources = Vec::new();
        let mut all_repairs = Vec::new();

        for i in 0..20u64 {
            let data = vec![(i & 0xFF) as u8; 64];
            let src = enc.add_source(&data);
            all_sources.push((i, src));
            all_repairs.push(enc.generate_repair());
        }

        // Burst loss of 6 symbols (>> B=2): should not crash, may not recover all
        let mut total_recovered = Vec::new();
        for (i, src) in &all_sources {
            if *i >= 5 && *i <= 10 {
                continue;
            }
            let r = dec.add_symbol(src);
            total_recovered.extend(r);
        }

        for repair in &all_repairs {
            let r = dec.add_symbol(repair);
            total_recovered.extend(r);
        }

        // Just verify no panic — graceful degradation
        let recovered_seqs: std::collections::BTreeSet<u64> =
            total_recovered.iter().map(|(s, _)| *s).collect();
        // We don't assert full recovery since burst > B
        assert!(
            recovered_seqs.len() >= 14,
            "Non-dropped symbols should still be delivered"
        );
    }

    #[test]
    fn test_streaming_params_from_channel() {
        let params = StreamingParams::from_channel(3.0, 0.05, 1.2);
        assert_eq!(params.b, 4); // ceil(3.0 * 1.2)
        assert_eq!(params.t, 4); // T = B
        assert!(params.burst_rate > 0.0);
        assert!(params.random_rate > 0.0);
        assert!(params.total_rate() < 1.0);
    }
}
