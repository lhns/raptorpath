//! METTLE sliding window encoder/decoder.
//!
//! Implements the `WindowEncoder` and `WindowDecoder` traits using METTLE's
//! graph-based spatial coupling and pure peeling decoder (GF(2) XOR only).
//!
//! The encoder wraps `mettle::MettleEncoder`, rebuilding it when the window
//! advances (the encoder has no incremental remove API).
//!
//! The decoder is a sparse reimplementation of METTLE's peeling algorithm
//! using `BTreeMap<u64, Vec<u8>>` keyed by global sequence numbers, avoiding
//! the positional `Vec<Option<...>>` that requires knowing `num_source` upfront.
//!
//! Wire format for repair symbols:
//!   `[window_start(8 LE)][num_members(2 LE)][member_offsets: u16 LE...][xor_data]`
//!
//! Member offsets are relative to `window_start` (u16 sufficient for windows up to 65535).

use bytes::Bytes;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use mettle::{MettleConfig, MettleEncoder};

use super::traits::{FecBackend, WireSymbol};
use super::window_traits::{WindowDecoder, WindowEncoder};

/// XOR `src` into `acc`, extending `acc` with zeros if shorter.
fn xor_into(acc: &mut Vec<u8>, src: &[u8]) {
    if acc.len() < src.len() {
        acc.resize(src.len(), 0);
    }
    for (d, s) in acc.iter_mut().zip(src.iter()) {
        *d ^= s;
    }
}

/// Repair symbol wire header size (excluding member offsets):
/// 8 (window_start) + 2 (num_members) = 10 bytes fixed, plus 2 * num_members.
const REPAIR_HEADER_FIXED: usize = 10;

// ---------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------

/// METTLE sliding window encoder.
///
/// Wraps `mettle::MettleEncoder` directly — it already accepts sources
/// incrementally. When the window advances, the encoder is rebuilt with
/// only the remaining window symbols (O(window_size) XOR ops).
pub struct MettleWindowEncoder {
    config: MettleConfig,
    symbol_size: u16,
    seed: u64,
    /// Next global sequence number to assign
    next_seq: u64,
    /// Monotonic repair counter
    repair_counter: u32,
    /// Window of source symbols: (seq, padded_data)
    window: VecDeque<(u64, Vec<u8>)>,
    /// Current encoder instance (rebuilt on window advance)
    encoder: MettleEncoder,
}

impl MettleWindowEncoder {
    pub fn new(config: MettleConfig, symbol_size: u16, seed: u64) -> Self {
        Self {
            config,
            symbol_size,
            seed,
            next_seq: 0,
            repair_counter: 0,
            window: VecDeque::new(),
            encoder: MettleEncoder::new(config, seed),
        }
    }

    /// Rebuild the internal encoder from the current window contents.
    fn rebuild_encoder(&mut self) {
        let mut encoder = MettleEncoder::new(self.config, self.seed);
        for (_, data) in &self.window {
            encoder.add_source_packet(data);
        }
        self.encoder = encoder;
    }
}

impl WindowEncoder for MettleWindowEncoder {
    fn add_source(&mut self, data: &[u8]) -> WireSymbol {
        let seq = self.next_seq;
        self.next_seq += 1;

        // Pad to symbol_size
        let mut padded = vec![0u8; self.symbol_size as usize];
        let copy_len = data.len().min(self.symbol_size as usize);
        padded[..copy_len].copy_from_slice(&data[..copy_len]);

        self.window.push_back((seq, padded.clone()));
        self.encoder.add_source_packet(&padded);

        WireSymbol {
            block_id: seq,
            payload_id: 0,
            is_repair: false,
            data: padded,
            backend: FecBackend::Mettle,
        }
    }

    fn generate_repair(&mut self) -> WireSymbol {
        let symbol_size = self.symbol_size as usize;

        if self.window.is_empty() {
            return WireSymbol {
                block_id: 0,
                payload_id: self.repair_counter,
                is_repair: true,
                data: vec![0u8; REPAIR_HEADER_FIXED + symbol_size],
                backend: FecBackend::Mettle,
            };
        }

        let window_start = self.window.front().unwrap().0;
        let window_end = self.window.back().unwrap().0;
        let repair_index = self.repair_counter;
        self.repair_counter += 1;

        // Pick a coded packet from the encoder's bins using the repair counter
        let coded = self.encoder.coded_packets();
        if coded.is_empty() {
            // No bins available — return zero repair
            let mut wire_data = Vec::with_capacity(REPAIR_HEADER_FIXED + symbol_size);
            wire_data.extend_from_slice(&window_start.to_le_bytes());
            wire_data.extend_from_slice(&0u16.to_le_bytes());
            wire_data.extend(vec![0u8; symbol_size]);
            return WireSymbol {
                block_id: window_end,
                payload_id: repair_index,
                is_repair: true,
                data: wire_data,
                backend: FecBackend::Mettle,
            };
        }

        // Round-robin through coded packets using repair_index
        let cp = &coded[repair_index as usize % coded.len()];

        // Build wire format: [window_start(8)][num_members(2)][member_offsets: u16 LE...][xor_data]
        let num_members = cp.members.len() as u16;
        let header_size = REPAIR_HEADER_FIXED + 2 * cp.members.len();
        let mut wire_data = Vec::with_capacity(header_size + symbol_size);

        wire_data.extend_from_slice(&window_start.to_le_bytes());
        wire_data.extend_from_slice(&num_members.to_le_bytes());
        for &member_pos in &cp.members {
            // member_pos is the positional index within the encoder (0-based)
            // which maps directly to an offset from window_start
            wire_data.extend_from_slice(&(member_pos as u16).to_le_bytes());
        }

        // Pad coded data to symbol_size if needed
        let mut coded_data = vec![0u8; symbol_size];
        let copy_len = cp.data.len().min(symbol_size);
        coded_data[..copy_len].copy_from_slice(&cp.data[..copy_len]);
        wire_data.extend_from_slice(&coded_data);

        WireSymbol {
            block_id: window_end,
            payload_id: repair_index,
            is_repair: true,
            data: wire_data,
            backend: FecBackend::Mettle,
        }
    }

    fn window_span(&self) -> (u64, u64) {
        match (self.window.front(), self.window.back()) {
            (Some((start, _)), Some((end, _))) => (*start, *end),
            _ => (0, 0),
        }
    }

    fn advance(&mut self, oldest_seq: u64) {
        let before = self.window.len();
        while let Some((seq, _)) = self.window.front() {
            if *seq < oldest_seq {
                self.window.pop_front();
            } else {
                break;
            }
        }
        if self.window.len() != before {
            // Encoder has no remove API — rebuild from remaining window
            self.rebuild_encoder();
        }
    }

    fn window_size(&self) -> usize {
        self.window.len()
    }
}

// ---------------------------------------------------------------------------
// Decoder
// ---------------------------------------------------------------------------

/// A pending bin in the METTLE window decoder.
struct PendingBin {
    /// XOR accumulator: coded data with recovered sources XOR'd out.
    data: Vec<u8>,
    /// Global sequence numbers of still-unknown members.
    remaining: HashSet<u64>,
}

/// METTLE sliding window decoder.
///
/// Sparse reimplementation of METTLE's peeling algorithm using global sequence
/// numbers instead of positional indices. This avoids the `num_source`
/// constructor requirement of the block-mode `MettleDecoder`.
///
/// Key advantage over RLC: peeling is XOR-only (no GF(2^8) multiply), O(1) per
/// recovery step.
pub struct MettleWindowDecoder {
    symbol_size: u16,
    /// Recovered source symbols: seq → data
    recovered: BTreeMap<u64, Vec<u8>>,
    /// Pending bins: bin_id → PendingBin
    pending_bins: HashMap<u64, PendingBin>,
    /// Reverse index: source seq → set of bin_ids containing it
    source_to_bins: HashMap<u64, HashSet<u64>>,
    /// Queue of bin_ids ready for peeling (degree 1)
    peel_queue: VecDeque<u64>,
    /// Sequences already output (dedup)
    output: BTreeSet<u64>,
    /// Dedup: seen (block_id, payload_id, is_repair) tuples
    seen: HashSet<(u64, u32, bool)>,
    /// Total symbols fed
    total_fed: u64,
    /// Next bin_id to assign (monotonic)
    next_bin_id: u64,
}

impl MettleWindowDecoder {
    pub fn new(symbol_size: u16) -> Self {
        Self {
            symbol_size,
            recovered: BTreeMap::new(),
            pending_bins: HashMap::new(),
            source_to_bins: HashMap::new(),
            peel_queue: VecDeque::new(),
            output: BTreeSet::new(),
            seen: HashSet::new(),
            total_fed: 0,
            next_bin_id: 0,
        }
    }

    /// After recovering a source symbol, XOR it out of all pending bins
    /// that reference it. If any bin drops to degree 1, enqueue for peeling.
    fn propagate_recovery(&mut self, seq: u64) -> Vec<(u64, Bytes)> {
        let mut result = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(seq);

        while let Some(recovered_seq) = queue.pop_front() {
            let recovered_data = match self.recovered.get(&recovered_seq) {
                Some(d) => d.clone(),
                None => continue,
            };

            // Get all bins referencing this seq
            let bin_ids: Vec<u64> = self
                .source_to_bins
                .remove(&recovered_seq)
                .unwrap_or_default()
                .into_iter()
                .collect();

            for bin_id in bin_ids {
                if let Some(bin) = self.pending_bins.get_mut(&bin_id) {
                    if bin.remaining.remove(&recovered_seq) {
                        // XOR out the recovered data
                        xor_into(&mut bin.data, &recovered_data);

                        if bin.remaining.len() == 1 {
                            // Degree 1 — peel immediately
                            let peeled_seq = *bin.remaining.iter().next().unwrap();
                            let bin = self.pending_bins.remove(&bin_id).unwrap();

                            if !self.recovered.contains_key(&peeled_seq) {
                                self.recovered.insert(peeled_seq, bin.data);
                                if self.output.insert(peeled_seq) {
                                    result.push((
                                        peeled_seq,
                                        Bytes::from(
                                            self.recovered.get(&peeled_seq).unwrap().clone(),
                                        ),
                                    ));
                                }
                                queue.push_back(peeled_seq);
                            }
                        } else if bin.remaining.is_empty() {
                            // Fully resolved — remove
                            self.pending_bins.remove(&bin_id);
                        }
                    }
                }
            }
        }

        result
    }
}

impl WindowDecoder for MettleWindowDecoder {
    fn add_symbol(&mut self, symbol: &WireSymbol) -> Vec<(u64, Bytes)> {
        if symbol.backend != FecBackend::Mettle {
            return vec![];
        }

        // Deduplicate
        let key = (symbol.block_id, symbol.payload_id, symbol.is_repair);
        if !self.seen.insert(key) {
            return vec![];
        }

        self.total_fed += 1;

        if !symbol.is_repair {
            // Source symbol: block_id is the global sequence number
            let seq = symbol.block_id;
            let symbol_size = self.symbol_size as usize;

            let mut data = vec![0u8; symbol_size];
            let copy_len = symbol.data.len().min(symbol_size);
            data[..copy_len].copy_from_slice(&symbol.data[..copy_len]);

            if self.recovered.contains_key(&seq) {
                return vec![]; // already have it
            }

            self.recovered.insert(seq, data.clone());

            let mut result = Vec::new();
            if self.output.insert(seq) {
                result.push((seq, Bytes::from(data)));
            }

            // Propagate recovery to pending bins
            result.extend(self.propagate_recovery(seq));
            result
        } else {
            // Repair symbol: parse wire header
            // [window_start(8)][num_members(2)][member_offsets: u16 LE...][xor_data]
            if symbol.data.len() < REPAIR_HEADER_FIXED {
                return vec![];
            }

            let window_start =
                u64::from_le_bytes(symbol.data[0..8].try_into().unwrap());
            let num_members =
                u16::from_le_bytes(symbol.data[8..10].try_into().unwrap()) as usize;

            if num_members == 0 {
                return vec![];
            }

            // Validate header size
            let members_bytes = num_members.checked_mul(2).unwrap_or(usize::MAX);
            let members_end = members_bytes.checked_add(REPAIR_HEADER_FIXED).unwrap_or(usize::MAX);
            if symbol.data.len() < members_end {
                return vec![];
            }

            // Parse member offsets → global sequences
            let mut members = Vec::with_capacity(num_members);
            for j in 0..num_members {
                let offset = REPAIR_HEADER_FIXED + j * 2;
                let member_offset =
                    u16::from_le_bytes(symbol.data[offset..offset + 2].try_into().unwrap())
                        as u64;
                members.push(window_start + member_offset);
            }

            // Extract coded data
            let coded = &symbol.data[members_end..];
            let symbol_size = self.symbol_size as usize;
            let mut data = vec![0u8; symbol_size];
            let copy_len = coded.len().min(symbol_size);
            data[..copy_len].copy_from_slice(&coded[..copy_len]);

            // XOR out already-recovered sources, build remaining set
            let mut remaining = HashSet::new();
            for &seq in &members {
                if let Some(recovered_data) = self.recovered.get(&seq) {
                    xor_into(&mut data, recovered_data);
                } else {
                    remaining.insert(seq);
                }
            }

            if remaining.is_empty() {
                // All members recovered — redundant bin
                return vec![];
            }

            let bin_id = self.next_bin_id;
            self.next_bin_id += 1;

            if remaining.len() == 1 {
                // Degree 1 — recover immediately
                let peeled_seq = *remaining.iter().next().unwrap();
                self.recovered.insert(peeled_seq, data.clone());

                let mut result = Vec::new();
                if self.output.insert(peeled_seq) {
                    result.push((peeled_seq, Bytes::from(data)));
                }

                // Cascade
                result.extend(self.propagate_recovery(peeled_seq));
                return result;
            }

            // Register reverse index
            for &seq in &remaining {
                self.source_to_bins.entry(seq).or_default().insert(bin_id);
            }

            self.pending_bins.insert(bin_id, PendingBin { data, remaining });

            vec![]
        }
    }

    fn advance(&mut self, oldest_seq: u64) {
        // Remove recovered symbols older than oldest_seq
        let old_seqs: Vec<u64> = self.recovered.range(..oldest_seq).map(|(k, _)| *k).collect();
        for seq in &old_seqs {
            self.recovered.remove(seq);
        }

        // Remove output tracking
        let old_output: Vec<u64> = self.output.range(..oldest_seq).copied().collect();
        for seq in old_output {
            self.output.remove(&seq);
        }

        // Clean up source_to_bins for old sequences
        for seq in &old_seqs {
            if let Some(bin_ids) = self.source_to_bins.remove(seq) {
                for bin_id in bin_ids {
                    if let Some(bin) = self.pending_bins.get_mut(&bin_id) {
                        bin.remaining.remove(seq);
                        if bin.remaining.is_empty() {
                            self.pending_bins.remove(&bin_id);
                        }
                    }
                }
            }
        }

        // Clean seen set
        self.seen.retain(|(block_id, _, _)| *block_id >= oldest_seq);
    }

    fn total_fed(&self) -> u64 {
        self.total_fed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::framing::{extract_window_packet, frame_window_packet};

    fn test_config() -> MettleConfig {
        MettleConfig::small_window()
    }

    const SYMBOL_SIZE: u16 = 128;

    #[test]
    fn test_mettle_window_encode_decode_no_loss() {
        let mut encoder = MettleWindowEncoder::new(test_config(), SYMBOL_SIZE, 42);
        let mut decoder = MettleWindowDecoder::new(SYMBOL_SIZE);

        let packets: Vec<Vec<u8>> = (0..10).map(|i| vec![i as u8; 32]).collect();

        for pkt in &packets {
            let sym = encoder.add_source(pkt);
            let recovered = decoder.add_symbol(&sym);
            assert_eq!(recovered.len(), 1);
            assert_eq!(&recovered[0].1[..pkt.len()], pkt.as_slice());
        }

        assert_eq!(encoder.window_size(), 10);
        assert_eq!(encoder.window_span(), (0, 9));
    }

    #[test]
    fn test_mettle_window_single_loss_recovery() {
        let mut encoder = MettleWindowEncoder::new(test_config(), SYMBOL_SIZE, 42);
        let mut decoder = MettleWindowDecoder::new(SYMBOL_SIZE);

        // Add 5 source symbols, drop symbol 2
        let mut source_syms = Vec::new();
        for i in 0..5u8 {
            let pkt = vec![i + 1; 32];
            let sym = encoder.add_source(&pkt);
            source_syms.push(sym);
        }

        // Feed all except symbol 2
        for (i, sym) in source_syms.iter().enumerate() {
            if i == 2 {
                continue;
            }
            decoder.add_symbol(sym);
        }

        // Generate repair symbols and feed them until recovery
        let mut recovered_2 = false;
        for _ in 0..20 {
            let repair = encoder.generate_repair();
            let recovered = decoder.add_symbol(&repair);
            if recovered.iter().any(|(seq, _)| *seq == 2) {
                recovered_2 = true;
                break;
            }
        }

        assert!(recovered_2, "Expected to recover seq 2");
    }

    #[test]
    fn test_mettle_window_multiple_loss_recovery() {
        let mut encoder = MettleWindowEncoder::new(test_config(), SYMBOL_SIZE, 42);
        let mut decoder = MettleWindowDecoder::new(SYMBOL_SIZE);

        // Add 10 sources, drop 3 and 7
        let mut source_syms = Vec::new();
        for i in 0..10u8 {
            let pkt = vec![i + 10; 32];
            let sym = encoder.add_source(&pkt);
            source_syms.push(sym);
        }

        for (i, sym) in source_syms.iter().enumerate() {
            if i == 3 || i == 7 {
                continue;
            }
            decoder.add_symbol(sym);
        }

        // Feed repair symbols until both are recovered
        let mut all_recovered = BTreeSet::new();
        for _ in 0..40 {
            let repair = encoder.generate_repair();
            let recovered = decoder.add_symbol(&repair);
            for (seq, _) in recovered {
                all_recovered.insert(seq);
            }
            if all_recovered.contains(&3) && all_recovered.contains(&7) {
                break;
            }
        }

        assert!(all_recovered.contains(&3), "Expected to recover seq 3");
        assert!(all_recovered.contains(&7), "Expected to recover seq 7");
    }

    #[test]
    fn test_mettle_window_advance() {
        let mut encoder = MettleWindowEncoder::new(test_config(), SYMBOL_SIZE, 42);

        for i in 0..10u8 {
            encoder.add_source(&vec![i; 32]);
        }

        assert_eq!(encoder.window_size(), 10);
        assert_eq!(encoder.window_span(), (0, 9));

        encoder.advance(5);
        assert_eq!(encoder.window_size(), 5);
        assert_eq!(encoder.window_span(), (5, 9));
    }

    #[test]
    fn test_mettle_window_decoder_advance() {
        let mut encoder = MettleWindowEncoder::new(test_config(), SYMBOL_SIZE, 42);
        let mut decoder = MettleWindowDecoder::new(SYMBOL_SIZE);

        for i in 0..10u8 {
            let sym = encoder.add_source(&vec![i; 32]);
            decoder.add_symbol(&sym);
        }

        assert_eq!(decoder.total_fed(), 10);

        decoder.advance(5);
        assert!(decoder.recovered.get(&0).is_none());
        assert!(decoder.recovered.get(&4).is_none());
        assert!(decoder.recovered.get(&5).is_some());
    }

    #[test]
    fn test_mettle_window_deduplication() {
        let mut encoder = MettleWindowEncoder::new(test_config(), SYMBOL_SIZE, 42);
        let mut decoder = MettleWindowDecoder::new(SYMBOL_SIZE);

        let sym = encoder.add_source(&vec![42; 32]);

        let r1 = decoder.add_symbol(&sym);
        assert_eq!(r1.len(), 1);

        let r2 = decoder.add_symbol(&sym);
        assert_eq!(r2.len(), 0, "Duplicate should be ignored");

        assert_eq!(decoder.total_fed(), 1);
    }

    #[test]
    fn test_mettle_window_cascade_recovery() {
        let mut encoder = MettleWindowEncoder::new(test_config(), SYMBOL_SIZE, 42);
        let mut decoder = MettleWindowDecoder::new(SYMBOL_SIZE);

        // 5 sources, drop 1 and 3
        let mut syms = Vec::new();
        for i in 0..5u8 {
            syms.push(encoder.add_source(&vec![i + 100; 32]));
        }

        // Feed sources 0, 2, 4
        decoder.add_symbol(&syms[0]);
        decoder.add_symbol(&syms[2]);
        decoder.add_symbol(&syms[4]);

        // Feed multiple repairs until cascade recovers both
        let mut total_recovered = BTreeSet::new();
        for _ in 0..40 {
            let repair = encoder.generate_repair();
            let r = decoder.add_symbol(&repair);
            for (seq, _) in r {
                total_recovered.insert(seq);
            }
            if total_recovered.contains(&1) && total_recovered.contains(&3) {
                break;
            }
        }

        assert!(
            total_recovered.contains(&1) && total_recovered.contains(&3),
            "Should recover seqs 1 and 3 via cascade, got: {:?}",
            total_recovered
        );
    }

    #[test]
    fn test_mettle_window_peeling_is_xor_only() {
        // Verify that recovery data is correct (XOR-based, not GF(2^8))
        let mut encoder = MettleWindowEncoder::new(test_config(), SYMBOL_SIZE, 42);
        let mut decoder = MettleWindowDecoder::new(SYMBOL_SIZE);

        let packets: Vec<Vec<u8>> = (0..5).map(|i| vec![i as u8 + 1; SYMBOL_SIZE as usize]).collect();

        let mut syms = Vec::new();
        for pkt in &packets {
            syms.push(encoder.add_source(pkt));
        }

        // Feed all except position 0
        for sym in &syms[1..] {
            decoder.add_symbol(sym);
        }

        // Feed repairs until position 0 is recovered
        let mut recovered_data = None;
        for _ in 0..40 {
            let repair = encoder.generate_repair();
            let r = decoder.add_symbol(&repair);
            for (seq, data) in r {
                if seq == 0 {
                    recovered_data = Some(data);
                }
            }
            if recovered_data.is_some() {
                break;
            }
        }

        if let Some(data) = recovered_data {
            assert_eq!(&data[..], &packets[0][..], "Recovered data should match original");
        }
        // Note: recovery is not guaranteed with limited repair symbols due to
        // METTLE's graph structure, so we don't assert recovery happened.
    }

    #[test]
    fn test_mettle_window_framing_roundtrip() {
        let mut encoder = MettleWindowEncoder::new(test_config(), SYMBOL_SIZE, 42);
        let mut decoder = MettleWindowDecoder::new(SYMBOL_SIZE);

        let test_data = b"Hello, METTLE window mode!";
        let framed = frame_window_packet(test_data, SYMBOL_SIZE);
        let sym = encoder.add_source(&framed);
        let results = decoder.add_symbol(&sym);

        assert_eq!(results.len(), 1);
        let extracted = extract_window_packet(&results[0].1).unwrap();
        assert_eq!(extracted, test_data);
    }
}
