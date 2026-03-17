use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use bytes::Bytes;
use gf256::generate_window_coefficients;

use crate::{RepairSymbol, StreamingParams, LAYER_BURST, LAYER_RANDOM};

/// Streaming decoder — processes source and repair symbols, recovers missing symbols
/// using both burst-layer XOR and random-layer GF(256) Gaussian elimination.
pub struct StreamingCoreDecoder {
    symbol_size: u16,
    /// Received/recovered source symbols
    recovered: BTreeMap<u64, Vec<u8>>,
    /// Pending burst-layer repairs
    burst_repairs: Vec<BurstRepair>,
    /// Incremental GE pivot table for random-layer repairs
    pivots: BTreeMap<u64, PivotRow>,
    /// Sequences that have been output
    output: BTreeSet<u64>,
    /// Total symbols fed
    total_fed: u64,
    /// Streaming params for delay constraint
    params: StreamingParams,
}

struct BurstRepair {
    seqs: Vec<u64>,
    data: Vec<u8>,
}

struct PivotRow {
    pivot_seq: u64,
    coefficients: BTreeMap<u64, u8>,
    data: Vec<u8>,
}

impl StreamingCoreDecoder {
    pub fn new(symbol_size: u16, params: StreamingParams) -> Self {
        Self {
            symbol_size,
            recovered: BTreeMap::new(),
            burst_repairs: Vec::new(),
            pivots: BTreeMap::new(),
            output: BTreeSet::new(),
            total_fed: 0,
            params,
        }
    }

    /// Feed a source symbol. Returns newly recovered (seq, data) pairs.
    pub fn add_source(&mut self, seq: u64, data: &[u8]) -> Vec<(u64, Vec<u8>)> {
        self.total_fed += 1;

        let mut padded = vec![0u8; self.symbol_size as usize];
        let copy_len = data.len().min(self.symbol_size as usize);
        padded[..copy_len].copy_from_slice(&data[..copy_len]);

        self.recovered.insert(seq, padded.clone());

        let mut result = Vec::new();
        if self.output.insert(seq) {
            result.push((seq, padded));
        }

        // New source may enable burst recovery
        result.extend(self.try_burst_recovery());
        // And cascade through random pivots
        result.extend(self.cascade_from_recovered(seq));

        result
    }

    /// Feed a repair symbol. Returns newly recovered (seq, data) pairs.
    pub fn add_repair(&mut self, repair: &RepairSymbol) -> Vec<(u64, Vec<u8>)> {
        self.total_fed += 1;
        let coded = repair.coded[..self.symbol_size as usize].to_vec();

        match repair.layer {
            LAYER_BURST => {
                let t = self.params.t as u64;
                let newest = repair.window_start + repair.window_count.saturating_sub(1) as u64;
                let diagonal_index = repair.repair_index as u64 % t;
                let mut seqs = Vec::new();
                let mut seq = newest.wrapping_sub(diagonal_index);
                loop {
                    if seq < repair.window_start || seq > newest {
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
                let coeffs = generate_window_coefficients(
                    repair.window_start,
                    repair.window_count,
                    repair.repair_index,
                );

                let mut coeff_map = BTreeMap::new();
                for (i, &c) in coeffs.iter().enumerate() {
                    let seq = repair.window_start + i as u64;
                    if c != 0 {
                        coeff_map.insert(seq, c);
                    }
                }

                let mut result = self.insert_random_equation(coeff_map, coded);
                result.extend(self.try_burst_recovery());
                result
            }
            _ => vec![],
        }
    }

    /// Advance window: discard state for symbols older than `oldest_seq`.
    pub fn advance(&mut self, oldest_seq: u64) {
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

    /// Total symbols fed to this decoder.
    pub fn total_fed(&self) -> u64 {
        self.total_fed
    }

    fn try_burst_recovery(&mut self) -> Vec<(u64, Vec<u8>)> {
        let mut result = Vec::new();
        let mut changed = true;

        while changed {
            changed = false;
            let mut i = 0;
            while i < self.burst_repairs.len() {
                let missing: Vec<u64> = self.burst_repairs[i]
                    .seqs
                    .iter()
                    .filter(|s| !self.recovered.contains_key(s))
                    .copied()
                    .collect();

                if missing.len() == 1 {
                    let missing_seq = missing[0];
                    let mut recovered_data = self.burst_repairs[i].data.clone();

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
                        result.push((missing_seq, recovered_data));
                    }
                    self.burst_repairs.swap_remove(i);
                    changed = true;
                } else if missing.is_empty() {
                    self.burst_repairs.swap_remove(i);
                    changed = true;
                } else {
                    i += 1;
                }
            }
        }

        result
    }

    fn insert_random_equation(
        &mut self,
        mut coefficients: BTreeMap<u64, u8>,
        mut data: Vec<u8>,
    ) -> Vec<(u64, Vec<u8>)> {
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
                result.push((seq, recovered_data));
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

    fn cascade_from_recovered(&mut self, initial_seq: u64) -> Vec<(u64, Vec<u8>)> {
        let mut result = Vec::new();
        let mut queue = VecDeque::new();
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
                            result.push((recovered_seq, recovered_data));
                        }
                        queue.push_back(recovered_seq);
                    }
                }
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StreamingCoreEncoder;

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
        let mut enc = StreamingCoreEncoder::new(64, params);
        let mut dec = StreamingCoreDecoder::new(64, params);

        for i in 0..20u64 {
            let data = vec![(i & 0xFF) as u8; 64];
            let (seq, padded) = enc.add_source(&data);
            let recovered = dec.add_source(seq, &padded);
            assert_eq!(recovered.len(), 1, "symbol {i} should be delivered");
            assert_eq!(recovered[0].0, i);
            assert_eq!(&recovered[0].1[..], &data[..]);
        }
    }

    #[test]
    fn test_burst_recovery() {
        let params = make_params(4, 2, 0.0);
        let mut enc = StreamingCoreEncoder::new(64, params);
        let mut dec = StreamingCoreDecoder::new(64, params);

        let mut all_sources = Vec::new();
        let mut all_repairs = Vec::new();

        for i in 0..16u64 {
            let data = vec![(i & 0xFF) as u8; 64];
            let (seq, padded) = enc.add_source(&data);
            all_sources.push((seq, padded));

            for _ in 0..3 {
                all_repairs.push(enc.generate_repair());
            }
        }

        let mut total_recovered = Vec::new();
        for (seq, data) in &all_sources {
            if *seq == 4 || *seq == 5 {
                continue;
            }
            let r = dec.add_source(*seq, data);
            total_recovered.extend(r);
        }

        for repair in &all_repairs {
            let r = dec.add_repair(repair);
            total_recovered.extend(r);
        }

        let recovered_seqs: std::collections::BTreeSet<u64> =
            total_recovered.iter().map(|(s, _)| *s).collect();
        assert!(recovered_seqs.contains(&4), "Symbol 4 should be recovered");
        assert!(recovered_seqs.contains(&5), "Symbol 5 should be recovered");
    }

    #[test]
    fn test_random_loss_recovery() {
        let params = make_params(8, 2, 0.15);
        let mut enc = StreamingCoreEncoder::new(64, params);
        let mut dec = StreamingCoreDecoder::new(64, params);

        let mut all_sources = Vec::new();
        let mut all_repairs = Vec::new();

        for i in 0..20u64 {
            let data = vec![(i & 0xFF) as u8; 64];
            let (seq, padded) = enc.add_source(&data);
            all_sources.push((seq, padded));
            all_repairs.push(enc.generate_repair());
            all_repairs.push(enc.generate_repair());
        }

        let mut total_recovered = Vec::new();
        for (seq, data) in &all_sources {
            if seq % 5 == 3 {
                continue;
            }
            let r = dec.add_source(*seq, data);
            total_recovered.extend(r);
        }

        for repair in &all_repairs {
            let r = dec.add_repair(repair);
            total_recovered.extend(r);
        }

        let recovered_seqs: std::collections::BTreeSet<u64> =
            total_recovered.iter().map(|(s, _)| *s).collect();

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
        let mut enc = StreamingCoreEncoder::new(64, params);
        let mut dec = StreamingCoreDecoder::new(64, params);

        let mut all_sources = Vec::new();
        let mut all_repairs = Vec::new();

        for i in 0..30u64 {
            let data = vec![(i & 0xFF) as u8; 64];
            let (seq, padded) = enc.add_source(&data);
            all_sources.push((seq, padded));
            all_repairs.push(enc.generate_repair());
            all_repairs.push(enc.generate_repair());
        }

        let mut total_recovered = Vec::new();
        for (seq, data) in &all_sources {
            if (*seq >= 10 && *seq <= 12) || *seq == 20 {
                continue;
            }
            let r = dec.add_source(*seq, data);
            total_recovered.extend(r);
        }

        for repair in &all_repairs {
            let r = dec.add_repair(repair);
            total_recovered.extend(r);
        }

        let recovered_seqs: std::collections::BTreeSet<u64> =
            total_recovered.iter().map(|(s, _)| *s).collect();
        let recovered_dropped = [10u64, 11, 12, 20]
            .iter()
            .filter(|&&s| recovered_seqs.contains(&s))
            .count();
        assert!(
            recovered_dropped >= 1,
            "Should recover at least 1 of 4 dropped symbols, got {recovered_dropped}"
        );
    }

    /// 500-symbol regression test with GE-channel loss.
    #[test]
    fn test_500_symbol_ge_channel_recovery() {
        let p_gb = 0.03;
        let p_bg = 0.5;
        let loss_good = 0.01;
        let loss_bad = 0.3;

        let params = StreamingParams::from_channel(3.0, 0.05, 1.2);
        let num_symbols = 500usize;
        let repair_per_source = 2usize;

        let mut enc = StreamingCoreEncoder::new(64, params);
        let mut dec = StreamingCoreDecoder::new(64, params);

        let mut sources = Vec::with_capacity(num_symbols);
        let mut repairs = Vec::new();
        for i in 0..num_symbols {
            let data = vec![(i % 256) as u8; 64];
            let (seq, padded) = enc.add_source(&data);
            sources.push((seq, padded));
            for _ in 0..repair_per_source {
                repairs.push(enc.generate_repair());
            }
        }

        let mut in_bad = false;
        let mut surviving = Vec::new();
        let mut dropped = std::collections::BTreeSet::new();
        let mut rng_state: u64 = 42;
        for (seq, data) in &sources {
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let r = (rng_state >> 33) as f64 / (1u64 << 31) as f64;
            let loss_prob = if in_bad { loss_bad } else { loss_good };
            if r < loss_prob {
                dropped.insert(*seq);
            } else {
                surviving.push((*seq, data.clone()));
            }
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let tr = (rng_state >> 33) as f64 / (1u64 << 31) as f64;
            if in_bad {
                if tr < p_bg { in_bad = false; }
            } else {
                if tr < p_gb { in_bad = true; }
            }
        }

        let mut recovered_seqs = std::collections::BTreeSet::new();
        for (seq, data) in &surviving {
            for (s, _) in dec.add_source(*seq, data) {
                recovered_seqs.insert(s);
            }
        }
        for sym in &repairs {
            for (s, _) in dec.add_repair(sym) {
                recovered_seqs.insert(s);
            }
        }

        let recovered_dropped = dropped.iter().filter(|s| recovered_seqs.contains(s)).count();
        assert!(!dropped.is_empty(), "GE channel should have dropped some symbols");
        assert!(
            recovered_dropped > 0,
            "Should recover >0 dropped symbols out of {} dropped",
            dropped.len()
        );
    }
}
