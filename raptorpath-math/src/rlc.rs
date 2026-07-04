//! Simplified RLC sliding window encoder/decoder for wasm.
//!
//! Uses GF(2^8) arithmetic from the gf256 crate. No bytes::Bytes,
//! no WireSymbol trait, no BTreeMap — optimized for wasm size.

use std::collections::{HashMap, VecDeque};

/// RLC sliding window encoder.
pub struct RlcEncoder {
    symbol_size: u16,
    window: VecDeque<(u64, Vec<u8>)>,
    next_seq: u64,
    repair_counter: u32,
}

/// A repair symbol's metadata (for sending through the channel).
pub struct RepairInfo {
    pub window_start: u64,
    pub window_count: u16,
    pub repair_index: u32,
    pub coded_data: Vec<u8>,
}

impl RlcEncoder {
    pub fn new(symbol_size: u16) -> Self {
        Self { symbol_size, window: VecDeque::new(), next_seq: 0, repair_counter: 0 }
    }

    /// Add a source symbol. Returns the assigned sequence number.
    pub fn add_source(&mut self, data: &[u8]) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        let mut padded = vec![0u8; self.symbol_size as usize];
        let copy_len = data.len().min(self.symbol_size as usize);
        padded[..copy_len].copy_from_slice(&data[..copy_len]);
        self.window.push_back((seq, padded));
        seq
    }

    /// Generate a repair symbol (random linear combination of the window).
    pub fn generate_repair(&mut self) -> RepairInfo {
        let ss = self.symbol_size as usize;
        let wc = self.window.len() as u16;
        if wc == 0 {
            return RepairInfo { window_start: 0, window_count: 0, repair_index: 0, coded_data: vec![0u8; ss] };
        }
        let ws = self.window.front().unwrap().0;
        let ri = self.repair_counter;
        self.repair_counter += 1;

        let coeffs = gf256::generate_window_coefficients(ws, wc, ri);
        let mut coded = vec![0u8; ss];
        for (i, (_, src)) in self.window.iter().enumerate() {
            gf256::mul_acc_slice(coeffs[i], src, &mut coded);
        }

        RepairInfo { window_start: ws, window_count: wc, repair_index: ri, coded_data: coded }
    }

    /// Get source data by sequence number (for ARQ retransmit).
    pub fn get_source(&self, seq: u64) -> Option<&[u8]> {
        if self.window.is_empty() { return None; }
        let start = self.window.front().unwrap().0;
        if seq < start { return None; }
        let offset = (seq - start) as usize;
        self.window.get(offset).filter(|(s, _)| *s == seq).map(|(_, d)| d.as_slice())
    }

    pub fn window_size(&self) -> usize { self.window.len() }
    pub fn next_seq(&self) -> u64 { self.next_seq }

    /// Advance window: drop symbols older than oldest_seq.
    pub fn advance(&mut self, oldest_seq: u64) {
        while self.window.front().is_some_and(|(s, _)| *s < oldest_seq) {
            self.window.pop_front();
        }
    }
}

/// Pivot row in incremental Gaussian elimination.
struct PivotRow {
    pivot_seq: u64,
    /// (seq, coeff) pairs for remaining unknowns (excluding pivot which is implicit 1)
    coefficients: Vec<(u64, u8)>,
    data: Vec<u8>,
}

impl PivotRow {
    fn has_coeff(&self, seq: u64) -> Option<u8> {
        self.coefficients.iter().find(|(s, _)| *s == seq).map(|(_, c)| *c)
    }
    fn remove_coeff(&mut self, seq: u64) {
        self.coefficients.retain(|(s, _)| *s != seq);
    }
}

/// RLC sliding window decoder with incremental Gaussian elimination.
pub struct RlcDecoder {
    symbol_size: u16,
    /// Recovered source data, in recovery order.
    recovered: Vec<(u64, Vec<u8>)>,
    /// seq -> index into `recovered` for O(1) membership/lookup. Without
    /// this the linear scans in insert_equation/cascade cost O(W·N) per
    /// repair and O(N) per source feed (N = symbols recovered so far); at
    /// W=512 over a 2000-symbol stream that dominated decode wall-time
    /// (visualizer realtime-high-saturation slowdown; also real transport
    /// decode latency).
    recovered_idx: HashMap<u64, usize>,
    /// Pivot table for incremental GE
    pivots: Vec<PivotRow>,
    /// Total symbols fed
    total_fed: u64,
    repairs_fed: u64,
    repairs_useful: u64,
}

impl RlcDecoder {
    pub fn new(symbol_size: u16) -> Self {
        Self {
            symbol_size,
            recovered: Vec::new(),
            recovered_idx: HashMap::new(),
            pivots: Vec::new(),
            total_fed: 0,
            repairs_fed: 0,
            repairs_useful: 0,
        }
    }

    /// Record a recovered symbol, keeping the index in sync. All writes to
    /// `recovered` go through here so the two never diverge.
    fn add_recovered(&mut self, seq: u64, data: Vec<u8>) {
        self.recovered_idx.insert(seq, self.recovered.len());
        self.recovered.push((seq, data));
    }

    fn is_recovered(&self, seq: u64) -> bool {
        self.recovered_idx.contains_key(&seq)
    }

    fn get_recovered(&self, seq: u64) -> Option<&[u8]> {
        self.recovered_idx
            .get(&seq)
            .map(|&i| self.recovered[i].1.as_slice())
    }

    /// Feed a source symbol that arrived directly. Returns newly recovered seqs.
    pub fn feed_source(&mut self, seq: u64, data: &[u8]) -> Vec<u64> {
        self.total_fed += 1;
        if self.is_recovered(seq) { return vec![]; }
        let mut padded = vec![0u8; self.symbol_size as usize];
        let copy_len = data.len().min(self.symbol_size as usize);
        padded[..copy_len].copy_from_slice(&data[..copy_len]);
        self.add_recovered(seq, padded);

        let mut result = vec![seq];
        result.extend(self.cascade(seq));
        result
    }

    /// Feed a repair symbol. Returns newly recovered seqs.
    pub fn feed_repair(&mut self, window_start: u64, window_count: u16, repair_index: u32, coded_data: &[u8]) -> Vec<u64> {
        self.total_fed += 1;
        self.repairs_fed += 1;

        let coeffs = gf256::generate_window_coefficients(window_start, window_count, repair_index);

        // Build coefficient map: (seq, coeff) for each window position
        let mut coeff_pairs: Vec<(u64, u8)> = Vec::new();
        for i in 0..window_count as usize {
            let seq = window_start + i as u64;
            if coeffs[i] != 0 {
                coeff_pairs.push((seq, coeffs[i]));
            }
        }

        let mut data = vec![0u8; self.symbol_size as usize];
        let copy_len = coded_data.len().min(data.len());
        data[..copy_len].copy_from_slice(&coded_data[..copy_len]);

        self.insert_equation(coeff_pairs, data)
    }

    fn insert_equation(&mut self, mut coefficients: Vec<(u64, u8)>, mut data: Vec<u8>) -> Vec<u64> {
        let ss = self.symbol_size as usize;

        // Eliminate known sources
        coefficients.retain(|&(seq, coeff)| {
            if let Some(src) = self.get_recovered(seq) {
                gf256::mul_acc_slice(coeff, src, &mut data);
                false
            } else {
                true
            }
        });

        // Eliminate against existing pivots
        let pivot_seqs: Vec<u64> = self.pivots.iter().map(|p| p.pivot_seq).collect();
        for pseq in pivot_seqs {
            if let Some(pos) = coefficients.iter().position(|(s, _)| *s == pseq) {
                let coeff = coefficients[pos].1;
                coefficients.remove(pos);
                let pidx = self.pivots.iter().position(|p| p.pivot_seq == pseq).unwrap();
                let pivot = &self.pivots[pidx];
                gf256::mul_acc_slice(coeff, &pivot.data, &mut data);
                for &(other_seq, other_coeff) in &pivot.coefficients {
                    let combined = gf256::mul(coeff, other_coeff);
                    if let Some(entry) = coefficients.iter_mut().find(|(s, _)| *s == other_seq) {
                        entry.1 = gf256::add(entry.1, combined);
                        if entry.1 == 0 { coefficients.retain(|(s, _)| *s != other_seq); }
                    } else if combined != 0 {
                        coefficients.push((other_seq, combined));
                    }
                }
            }
        }

        if coefficients.is_empty() { return vec![]; }

        if coefficients.len() == 1 {
            let (seq, coeff) = coefficients[0];
            let inv = gf256::inv(coeff);
            let mut rd = vec![0u8; ss];
            gf256::mul_slice(inv, &data, &mut rd);
            self.add_recovered(seq, rd);
            self.repairs_useful += 1;

            let mut result = vec![seq];
            result.extend(self.cascade(seq));
            return result;
        }

        // Multiple unknowns → store as pivot row
        let (pivot_seq, pivot_coeff) = coefficients.remove(0);
        let inv = gf256::inv(pivot_coeff);
        let mut norm_data = vec![0u8; ss];
        gf256::mul_slice(inv, &data, &mut norm_data);
        let norm_coeffs: Vec<(u64, u8)> = coefficients.iter().map(|&(s, c)| (s, gf256::mul(inv, c))).collect();

        self.pivots.push(PivotRow { pivot_seq, coefficients: norm_coeffs, data: norm_data });
        vec![]
    }

    fn cascade(&mut self, initial_seq: u64) -> Vec<u64> {
        let mut result = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(initial_seq);

        while let Some(seq) = queue.pop_front() {
            let src_data = match self.get_recovered(seq) {
                Some(d) => d.to_vec(),
                None => continue,
            };

            // Remove any pivot for this seq (it's resolved)
            self.pivots.retain(|p| p.pivot_seq != seq);

            // Reduce all pivots that reference this seq
            let mut newly_recovered = Vec::new();
            for pivot in &mut self.pivots {
                if let Some(coeff) = pivot.has_coeff(seq) {
                    gf256::mul_acc_slice(coeff, &src_data, &mut pivot.data);
                    pivot.remove_coeff(seq);
                    if pivot.coefficients.is_empty() {
                        // Pivot variable is now the only unknown → recover
                        newly_recovered.push((pivot.pivot_seq, pivot.data.clone()));
                    }
                }
            }

            for (rseq, rdata) in newly_recovered {
                self.pivots.retain(|p| p.pivot_seq != rseq);
                if !self.is_recovered(rseq) {
                    self.add_recovered(rseq, rdata);
                    self.repairs_useful += 1;
                    result.push(rseq);
                    queue.push_back(rseq);
                }
            }
        }

        result
    }

    pub fn total_fed(&self) -> u64 { self.total_fed }
    pub fn repairs_fed(&self) -> u64 { self.repairs_fed }
    pub fn repairs_useful(&self) -> u64 { self.repairs_useful }
    pub fn recovered_count(&self) -> usize { self.recovered.len() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_only_roundtrip() {
        let mut enc = RlcEncoder::new(16);
        let mut dec = RlcDecoder::new(16);

        for i in 0..10u8 {
            let data = vec![i; 16];
            let seq = enc.add_source(&data);
            let recovered = dec.feed_source(seq, &data);
            assert_eq!(recovered, vec![seq as u64]);
        }
        assert_eq!(dec.recovered_count(), 10);
    }

    #[test]
    fn test_repair_recovers_single_loss() {
        let mut enc = RlcEncoder::new(16);
        let mut dec = RlcDecoder::new(16);

        // Send 5 source symbols, lose #2
        for i in 0..5u8 {
            let data = vec![i + 1; 16];
            let seq = enc.add_source(&data);
            if i != 2 {
                dec.feed_source(seq, &data);
            }
        }
        assert_eq!(dec.recovered_count(), 4); // missing seq 2

        // Generate and feed repair
        let repair = enc.generate_repair();
        let recovered = dec.feed_repair(repair.window_start, repair.window_count, repair.repair_index, &repair.coded_data);

        assert!(recovered.contains(&2), "Should recover seq 2: {:?}", recovered);
        assert_eq!(dec.recovered_count(), 5);
    }

    #[test]
    fn test_repair_recovers_burst_loss() {
        let mut enc = RlcEncoder::new(16);
        let mut dec = RlcDecoder::new(16);

        // Send 10 source, lose 3 consecutive (seq 3,4,5)
        for i in 0..10u8 {
            let data = vec![i + 1; 16];
            let seq = enc.add_source(&data);
            if !(3..=5).contains(&i) {
                dec.feed_source(seq, &data);
            }
        }
        assert_eq!(dec.recovered_count(), 7);

        // Need 3 repairs for 3 losses
        for _ in 0..3 {
            let repair = enc.generate_repair();
            dec.feed_repair(repair.window_start, repair.window_count, repair.repair_index, &repair.coded_data);
        }

        assert_eq!(dec.recovered_count(), 10, "All 10 should be recovered");
    }
}
