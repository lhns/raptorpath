use std::collections::BTreeMap;

use gf256::generate_window_coefficients;

use crate::StreamingParams;

/// Layer identifier for burst-layer repair symbols.
pub const LAYER_BURST: u8 = 0;
/// Layer identifier for random-layer repair symbols.
pub const LAYER_RANDOM: u8 = 1;

/// A repair symbol produced by the streaming encoder.
#[derive(Debug, Clone)]
pub struct RepairSymbol {
    /// Which layer produced this repair (LAYER_BURST or LAYER_RANDOM)
    pub layer: u8,
    /// Start of the encoding window
    pub window_start: u64,
    /// Number of source symbols in the encoding window
    pub window_count: u16,
    /// Monotonic repair index
    pub repair_index: u32,
    /// The coded data (same length as symbol_size)
    pub coded: Vec<u8>,
}

/// Streaming encoder — produces burst-layer and random-layer repair symbols
/// over a sliding window.
pub struct StreamingCoreEncoder {
    symbol_size: u16,
    /// Source symbols in the window: (seq, data)
    window: BTreeMap<u64, Vec<u8>>,
    next_seq: u64,
    params: StreamingParams,
    /// Counter for alternating between burst and random repair
    repair_counter: u32,
}

impl StreamingCoreEncoder {
    pub fn new(symbol_size: u16, params: StreamingParams) -> Self {
        Self {
            symbol_size,
            window: BTreeMap::new(),
            next_seq: 0,
            params,
            repair_counter: 0,
        }
    }

    /// Add a source symbol. Returns (seq, padded_data).
    pub fn add_source(&mut self, data: &[u8]) -> (u64, Vec<u8>) {
        let seq = self.next_seq;
        self.next_seq += 1;

        let mut padded = vec![0u8; self.symbol_size as usize];
        let copy_len = data.len().min(self.symbol_size as usize);
        padded[..copy_len].copy_from_slice(&data[..copy_len]);

        self.window.insert(seq, padded.clone());

        (seq, padded)
    }

    /// Generate a repair symbol covering the current window.
    pub fn generate_repair(&mut self) -> RepairSymbol {
        // Alternate between burst and random layer based on their rates.
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

    /// Current window span: (oldest_seq, newest_seq). Returns (0, 0) if empty.
    pub fn window_span(&self) -> (u64, u64) {
        match (self.window.keys().next(), self.window.keys().last()) {
            (Some(&first), Some(&last)) => (first, last),
            _ => (0, 0),
        }
    }

    /// Advance window: drop symbols older than `oldest_seq`.
    pub fn advance(&mut self, oldest_seq: u64) {
        let to_remove: Vec<u64> = self
            .window
            .range(..oldest_seq)
            .map(|(&k, _)| k)
            .collect();
        for k in to_remove {
            self.window.remove(&k);
        }
    }

    /// Number of source symbols currently in the window.
    pub fn window_size(&self) -> usize {
        self.window.len()
    }

    fn generate_burst_repair(&self) -> RepairSymbol {
        let t = self.params.t as u64;
        let mut coded = vec![0u8; self.symbol_size as usize];

        if self.window.is_empty() {
            return self.empty_repair(LAYER_BURST);
        }

        let newest_seq = *self.window.keys().last().unwrap();
        let diagonal_index = self.repair_counter as u64 % t;
        let mut seq = newest_seq.wrapping_sub(diagonal_index);

        let oldest_seq = *self.window.keys().next().unwrap();
        loop {
            if seq < oldest_seq || seq > newest_seq {
                break;
            }
            if let Some(src) = self.window.get(&seq) {
                for (d, &s) in coded.iter_mut().zip(src.iter()) {
                    *d ^= s;
                }
            }
            if seq < t {
                break;
            }
            seq -= t;
        }

        let (window_start, _) = self.window_span();
        let window_count = self.window.len() as u16;

        RepairSymbol {
            layer: LAYER_BURST,
            window_start,
            window_count,
            repair_index: self.repair_counter,
            coded,
        }
    }

    fn generate_random_repair(&self) -> RepairSymbol {
        if self.window.is_empty() {
            return self.empty_repair(LAYER_RANDOM);
        }

        let window_start = *self.window.keys().next().unwrap();
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

        RepairSymbol {
            layer: LAYER_RANDOM,
            window_start,
            window_count,
            repair_index: self.repair_counter,
            coded,
        }
    }

    fn empty_repair(&self, layer: u8) -> RepairSymbol {
        RepairSymbol {
            layer,
            window_start: 0,
            window_count: 0,
            repair_index: self.repair_counter,
            coded: vec![0u8; self.symbol_size as usize],
        }
    }
}
