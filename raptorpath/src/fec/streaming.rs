//! Streaming codes adapter — wraps the standalone `streaming-codes` crate
//! into raptorpath's `WindowEncoder`/`WindowDecoder` traits.

use bytes::Bytes;
use std::collections::HashSet;

use super::traits::{FecBackend, WireSymbol};
use super::window_traits::{WindowDecoder, WindowEncoder};

pub use streaming_codes::StreamingParams;
use streaming_codes::{
    RepairSymbol, StreamingCoreDecoder, StreamingCoreEncoder, LAYER_BURST, LAYER_RANDOM,
};

/// Repair header: window_start(8) + window_count(2) + repair_index(4) + layer(1) = 15
const STREAMING_REPAIR_HEADER: usize = 15;

/// Streaming encoder adapter — implements `WindowEncoder` by wrapping `StreamingCoreEncoder`.
pub struct StreamingEncoder {
    core: StreamingCoreEncoder,
    symbol_size: u16,
}

impl StreamingEncoder {
    pub fn new(symbol_size: u16, params: StreamingParams) -> Self {
        Self {
            core: StreamingCoreEncoder::new(symbol_size, params),
            symbol_size,
        }
    }
}

impl WindowEncoder for StreamingEncoder {
    fn add_source(&mut self, data: &[u8]) -> WireSymbol {
        let (seq, padded) = self.core.add_source(data);
        WireSymbol {
            block_id: seq,
            payload_id: 0,
            is_repair: false,
            data: padded,
            backend: FecBackend::Streaming,
        }
    }

    fn generate_repair(&mut self) -> WireSymbol {
        let repair = self.core.generate_repair();
        let (_, window_end) = self.core.window_span();
        repair_to_wire(&repair, window_end, self.symbol_size)
    }

    fn window_span(&self) -> (u64, u64) {
        self.core.window_span()
    }

    fn advance(&mut self, oldest_seq: u64) {
        self.core.advance(oldest_seq);
    }

    fn window_size(&self) -> usize {
        self.core.window_size()
    }
}

/// Streaming decoder adapter — implements `WindowDecoder` by wrapping `StreamingCoreDecoder`.
pub struct StreamingDecoder {
    core: StreamingCoreDecoder,
    symbol_size: u16,
    /// Deduplication set
    seen: HashSet<(u64, u32, bool)>,
    /// Total repair symbols fed
    repairs_fed: u64,
    /// Repair symbols that contributed to recovery
    repairs_useful: u64,
}

impl StreamingDecoder {
    pub fn new(symbol_size: u16, params: StreamingParams) -> Self {
        Self {
            core: StreamingCoreDecoder::new(symbol_size, params),
            symbol_size,
            seen: HashSet::new(),
            repairs_fed: 0,
            repairs_useful: 0,
        }
    }
}

impl WindowDecoder for StreamingDecoder {
    fn add_symbol(&mut self, symbol: &WireSymbol) -> Vec<(u64, Bytes)> {
        if symbol.backend != FecBackend::Streaming {
            return vec![];
        }

        let key = (symbol.block_id, symbol.payload_id, symbol.is_repair);
        if !self.seen.insert(key) {
            return vec![];
        }

        if !symbol.is_repair {
            let seq = symbol.block_id;
            self.core
                .add_source(seq, &symbol.data)
                .into_iter()
                .map(|(s, d)| (s, Bytes::from(d)))
                .collect()
        } else {
            self.repairs_fed += 1;
            match wire_to_repair(symbol, self.symbol_size) {
                Some(repair) => {
                    let recovered: Vec<_> = self
                        .core
                        .add_repair(&repair)
                        .into_iter()
                        .map(|(s, d)| (s, Bytes::from(d)))
                        .collect();
                    if !recovered.is_empty() {
                        self.repairs_useful += 1;
                    }
                    recovered
                }
                None => vec![],
            }
        }
    }

    fn advance(&mut self, oldest_seq: u64) {
        self.core.advance(oldest_seq);
    }

    fn total_fed(&self) -> u64 {
        self.core.total_fed()
    }

    fn repairs_fed(&self) -> u64 {
        self.repairs_fed
    }

    fn repairs_useful(&self) -> u64 {
        self.repairs_useful
    }
}

// ---------------------------------------------------------------------------
// Wire format conversion
// ---------------------------------------------------------------------------

fn repair_to_wire(repair: &RepairSymbol, window_end: u64, _symbol_size: u16) -> WireSymbol {
    let mut wire_data =
        Vec::with_capacity(STREAMING_REPAIR_HEADER + repair.coded.len());
    wire_data.extend_from_slice(&repair.window_start.to_le_bytes());
    wire_data.extend_from_slice(&repair.window_count.to_le_bytes());
    wire_data.extend_from_slice(&repair.repair_index.to_le_bytes());
    wire_data.push(repair.layer);
    wire_data.extend_from_slice(&repair.coded);

    WireSymbol {
        block_id: window_end,
        payload_id: repair.repair_index,
        is_repair: true,
        data: wire_data,
        backend: FecBackend::Streaming,
    }
}

fn wire_to_repair(symbol: &WireSymbol, symbol_size: u16) -> Option<RepairSymbol> {
    if symbol.data.len() < STREAMING_REPAIR_HEADER {
        return None;
    }

    let window_start = u64::from_le_bytes(symbol.data[0..8].try_into().unwrap());
    let window_count = u16::from_le_bytes(symbol.data[8..10].try_into().unwrap());
    let repair_index = u32::from_le_bytes(symbol.data[10..14].try_into().unwrap());
    let layer = symbol.data[14];
    let coded = symbol.data[STREAMING_REPAIR_HEADER..][..symbol_size as usize].to_vec();

    Some(RepairSymbol {
        layer,
        window_start,
        window_count,
        repair_index,
        coded,
    })
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
        let params = make_params(4, 2, 0.0);
        let mut enc = StreamingEncoder::new(64, params);
        let mut dec = StreamingDecoder::new(64, params);

        let mut all_sources = Vec::new();
        let mut all_repairs = Vec::new();

        for i in 0..16u64 {
            let data = vec![(i & 0xFF) as u8; 64];
            let src = enc.add_source(&data);
            all_sources.push((i, src, data));

            for _ in 0..3 {
                all_repairs.push(enc.generate_repair());
            }
        }

        let mut total_recovered = Vec::new();
        for (i, src, _data) in &all_sources {
            if *i == 4 || *i == 5 {
                continue;
            }
            let r = dec.add_symbol(src);
            total_recovered.extend(r);
        }

        for repair in &all_repairs {
            let r = dec.add_symbol(repair);
            total_recovered.extend(r);
        }

        let recovered_seqs: std::collections::BTreeSet<u64> =
            total_recovered.iter().map(|(s, _)| *s).collect();
        assert!(recovered_seqs.contains(&4));
        assert!(recovered_seqs.contains(&5));
    }

    #[test]
    fn test_backend_guard() {
        let params = make_params(4, 2, 0.0);
        let mut dec = StreamingDecoder::new(64, params);

        let rlc_sym = WireSymbol {
            block_id: 0,
            payload_id: 0,
            is_repair: false,
            data: vec![0u8; 64],
            backend: FecBackend::Rlc,
        };
        assert!(dec.add_symbol(&rlc_sym).is_empty());
    }

    /// 500-symbol regression test with GE-channel loss (adapter level).
    #[test]
    fn test_500_symbol_ge_channel_recovery() {
        let p_gb = 0.03;
        let p_bg = 0.5;
        let loss_good = 0.01;
        let loss_bad = 0.3;

        let params = StreamingParams::from_channel(3.0, 0.05, 1.2);
        let num_symbols = 500usize;
        let repair_per_source = 2usize;

        let mut enc = StreamingEncoder::new(64, params);
        let mut dec = StreamingDecoder::new(64, params);

        let mut sources = Vec::with_capacity(num_symbols);
        let mut repairs = Vec::new();
        for i in 0..num_symbols {
            let data = vec![(i % 256) as u8; 64];
            let src = enc.add_source(&data);
            sources.push(src);
            for _ in 0..repair_per_source {
                repairs.push(enc.generate_repair());
            }
        }

        let mut in_bad = false;
        let mut surviving = Vec::new();
        let mut dropped = std::collections::BTreeSet::new();
        let mut rng_state: u64 = 42;
        for (i, src) in sources.iter().enumerate() {
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let r = (rng_state >> 33) as f64 / (1u64 << 31) as f64;
            let loss_prob = if in_bad { loss_bad } else { loss_good };
            if r < loss_prob {
                dropped.insert(i as u64);
            } else {
                surviving.push(src.clone());
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
        for sym in &surviving {
            for (seq, _) in dec.add_symbol(sym) {
                recovered_seqs.insert(seq);
            }
        }
        for sym in &repairs {
            for (seq, _) in dec.add_symbol(sym) {
                recovered_seqs.insert(seq);
            }
        }

        let recovered_dropped = dropped.iter().filter(|s| recovered_seqs.contains(s)).count();
        assert!(!dropped.is_empty());
        assert!(
            recovered_dropped > 0,
            "Should recover >0 dropped symbols out of {} dropped",
            dropped.len()
        );
    }
}
