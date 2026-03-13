//! METTLE peeling decoder — the core innovation.
//!
//! The peeling decoder recovers source packets from a combination of received
//! source packets (systematic) and coded packets (bins). The algorithm:
//!
//! 1. When a source packet arrives directly, mark it as recovered and XOR it out
//!    of all bins that contain it, reducing their degree.
//! 2. When a coded packet arrives, add it to the pending bins. If it has degree 1
//!    (only one unknown source), immediately recover that source via XOR.
//! 3. Any time a bin reaches degree 1, the peeling cascade fires: recover the
//!    source, XOR it out of all other bins, check for new degree-1 bins, repeat.
//!
//! METTLE's graph structure (specifically the TLE — Touch-less Leading Edge)
//! guarantees that peeling almost always has a starting point, avoiding the
//! Gaussian elimination fallback that RaptorQ requires.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::encoder::CodedPacket;
use crate::gf2;
use crate::MettleConfig;

/// A pending bin in the decoder, tracking its XOR accumulator and remaining members.
struct PendingBin {
    /// XOR accumulator: originally the coded packet data, progressively XOR'd with
    /// recovered source packets to reduce degree.
    data: Vec<u8>,
    /// Source positions still unknown (not yet recovered) in this bin.
    remaining: HashSet<usize>,
}

/// METTLE peeling decoder.
///
/// Accepts source packets (systematic) and coded packets, and uses the peeling
/// algorithm to recover lost source packets.
pub struct MettleDecoder {
    config: MettleConfig,
    /// Recovered source packets. `None` = not yet recovered.
    recovered: Vec<Option<Vec<u8>>>,
    /// Pending bins indexed by bin_index.
    pending_bins: HashMap<usize, PendingBin>,
    /// Queue of bin indices that have degree 1 and are ready for peeling.
    peel_queue: VecDeque<usize>,
    /// Total number of source packets expected.
    num_source: usize,
    /// Number of source packets recovered so far.
    num_recovered: usize,
    /// Reverse index: source position → set of bin indices containing it.
    source_to_bins: HashMap<usize, HashSet<usize>>,
    /// Seed for reproducible graph generation (must match encoder).
    seed: u64,
    /// Total symbols fed (for compatibility with raptorpath interface).
    total_fed: u32,
    /// IDs of all symbols seen (for deduplication and ACK reporting).
    seen_ids: HashSet<u32>,
}

impl MettleDecoder {
    /// Create a new decoder.
    ///
    /// `num_source` must match the encoder's total number of source packets.
    /// `seed` must match the encoder's seed for graph generation to be consistent.
    pub fn new(config: MettleConfig, num_source: usize, seed: u64) -> Self {
        Self {
            config,
            recovered: vec![None; num_source],
            pending_bins: HashMap::new(),
            peel_queue: VecDeque::new(),
            num_source,
            num_recovered: 0,
            source_to_bins: HashMap::new(),
            seed,
            total_fed: 0,
            seen_ids: HashSet::new(),
        }
    }

    /// Feed a received source packet (systematic/direct).
    ///
    /// Returns `true` if this packet was new (not already recovered).
    /// Triggers peeling cascade on all bins containing this source position.
    pub fn add_source_packet(&mut self, position: usize, data: &[u8]) -> bool {
        self.total_fed += 1;
        self.seen_ids.insert(position as u32);

        if position >= self.num_source {
            return false;
        }
        if self.recovered[position].is_some() {
            return false; // already have it
        }

        // Mark as recovered
        self.recovered[position] = Some(data.to_vec());
        self.num_recovered += 1;

        // XOR this source out of all pending bins that contain it
        self.propagate_recovery(position);

        true
    }

    /// Feed a received coded packet (bin).
    ///
    /// Returns `true` if any new source packets were recovered as a result.
    pub fn add_coded_packet(&mut self, packet: &CodedPacket) -> bool {
        self.total_fed += 1;
        // Use bin_index offset by num_source to avoid ID collision with source packet IDs
        self.seen_ids.insert((self.num_source + packet.bin_index) as u32);

        if self.pending_bins.contains_key(&packet.bin_index) {
            return false; // duplicate bin
        }

        // Determine which members are still unknown
        let mut data = packet.data.clone();
        let mut remaining = HashSet::new();

        for &pos in &packet.members {
            if pos < self.num_source {
                if let Some(ref recovered_data) = self.recovered[pos] {
                    // Already recovered — XOR it out immediately
                    gf2::xor_into(&mut data, recovered_data);
                } else {
                    remaining.insert(pos);
                }
            }
        }

        if remaining.is_empty() {
            // All members already recovered — this bin is redundant
            return false;
        }

        let bin_index = packet.bin_index;

        // Register reverse index
        for &pos in &remaining {
            self.source_to_bins
                .entry(pos)
                .or_default()
                .insert(bin_index);
        }

        let is_degree_one = remaining.len() == 1;

        self.pending_bins.insert(
            bin_index,
            PendingBin { data, remaining },
        );

        if is_degree_one {
            self.peel_queue.push_back(bin_index);
        }

        // Run peeling cascade
        let before = self.num_recovered;
        self.peel();
        self.num_recovered > before
    }

    /// Run the peeling decoder cascade.
    ///
    /// Processes all degree-1 bins: recovers the single unknown source packet,
    /// XOR's it out of all other bins, and checks for new degree-1 bins.
    fn peel(&mut self) {
        while let Some(bin_index) = self.peel_queue.pop_front() {
            // The bin might have been modified since it was queued
            let bin = match self.pending_bins.get(&bin_index) {
                Some(b) if b.remaining.len() == 1 => {
                    // Still degree 1 — proceed
                    self.pending_bins.remove(&bin_index).unwrap()
                }
                _ => continue, // No longer degree 1 (or removed)
            };

            // The single remaining member's data is the bin's XOR accumulator
            let pos = *bin.remaining.iter().next().unwrap();

            if self.recovered[pos].is_some() {
                continue; // Race: already recovered by another path
            }

            // Recover!
            self.recovered[pos] = Some(bin.data);
            self.num_recovered += 1;

            // Propagate this recovery to all other bins containing pos
            self.propagate_recovery(pos);

            if self.num_recovered == self.num_source {
                return; // All done
            }
        }
    }

    /// XOR a newly recovered source packet out of all pending bins that contain it.
    /// If any bin drops to degree 1, enqueue it for peeling.
    fn propagate_recovery(&mut self, position: usize) {
        let recovered_data = self.recovered[position].as_ref().unwrap().clone();

        // Get all bins containing this position
        let bin_indices: Vec<usize> = self
            .source_to_bins
            .remove(&position)
            .unwrap_or_default()
            .into_iter()
            .collect();

        for bin_idx in bin_indices {
            if let Some(bin) = self.pending_bins.get_mut(&bin_idx) {
                if bin.remaining.remove(&position) {
                    // XOR out the recovered data
                    gf2::xor_into(&mut bin.data, &recovered_data);

                    if bin.remaining.len() == 1 {
                        self.peel_queue.push_back(bin_idx);
                    } else if bin.remaining.is_empty() {
                        // Fully resolved — remove
                        self.pending_bins.remove(&bin_idx);
                    }
                }
            }
        }
    }

    /// Check if all source packets have been recovered.
    pub fn is_complete(&self) -> bool {
        self.num_recovered == self.num_source
    }

    /// Get recovered data as a single concatenated buffer (all source packets in order).
    /// Returns `None` if not all source packets have been recovered.
    pub fn recovered_data(&self) -> Option<Vec<u8>> {
        if !self.is_complete() {
            return None;
        }
        Some(
            self.recovered
                .iter()
                .filter_map(|s| s.as_ref())
                .flat_map(|s| s.iter().copied())
                .collect(),
        )
    }

    /// Get a specific recovered source packet.
    pub fn get_source(&self, position: usize) -> Option<&[u8]> {
        self.recovered.get(position)?.as_deref()
    }

    /// Number of source packets recovered so far.
    pub fn num_recovered(&self) -> usize {
        self.num_recovered
    }

    /// Total symbols fed to this decoder (source + coded).
    pub fn total_fed(&self) -> u32 {
        self.total_fed
    }

    /// All seen symbol IDs (for ACK reporting).
    pub fn seen_ids(&self) -> &HashSet<u32> {
        &self.seen_ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MettleEncoder;

    fn test_config() -> MettleConfig {
        MettleConfig {
            window_size: 50,
            num_edges: 4,
            overhead_factor: 0.1,
        }
    }

    #[test]
    fn decode_all_source_no_loss() {
        let config = test_config();
        let seed = 42;
        let num = 20;

        let packets: Vec<Vec<u8>> = (0..num).map(|i| vec![i as u8; 100]).collect();

        let mut decoder = MettleDecoder::new(config, num, seed);
        for (i, pkt) in packets.iter().enumerate() {
            decoder.add_source_packet(i, pkt);
        }

        assert!(decoder.is_complete());
        let data = decoder.recovered_data().unwrap();
        let expected: Vec<u8> = packets.iter().flat_map(|p| p.iter().copied()).collect();
        assert_eq!(data, expected);
    }

    #[test]
    fn decode_with_peeling() {
        let config = test_config();
        let seed = 42;
        let num = 20;

        let packets: Vec<Vec<u8>> = (0..num).map(|i| vec![i as u8; 100]).collect();

        // Encode
        let mut encoder = MettleEncoder::new(config, seed);
        for pkt in &packets {
            encoder.add_source_packet(pkt);
        }
        let coded = encoder.coded_packets();

        // Decode: drop some source packets, rely on coded packets
        let mut decoder = MettleDecoder::new(config, num, seed);

        // Feed all source except positions 3 and 7
        for (i, pkt) in packets.iter().enumerate() {
            if i != 3 && i != 7 {
                decoder.add_source_packet(i, pkt);
            }
        }

        // Feed all coded packets
        for cp in &coded {
            decoder.add_coded_packet(cp);
            if decoder.is_complete() {
                break;
            }
        }

        assert!(
            decoder.is_complete(),
            "Failed to decode: recovered {}/{num}",
            decoder.num_recovered()
        );

        // Verify recovered data
        for (i, pkt) in packets.iter().enumerate() {
            assert_eq!(decoder.get_source(i).unwrap(), pkt.as_slice(), "Mismatch at position {i}");
        }
    }

    #[test]
    fn decode_source_only_is_fast_path() {
        let config = test_config();
        let mut decoder = MettleDecoder::new(config, 5, 42);

        for i in 0..5 {
            decoder.add_source_packet(i, &vec![i as u8; 50]);
        }

        assert!(decoder.is_complete());
        assert_eq!(decoder.num_recovered(), 5);
    }

    #[test]
    fn duplicate_source_ignored() {
        let config = test_config();
        let mut decoder = MettleDecoder::new(config, 3, 42);

        assert!(decoder.add_source_packet(0, &[1, 2, 3]));
        assert!(!decoder.add_source_packet(0, &[1, 2, 3])); // duplicate
        assert_eq!(decoder.num_recovered(), 1);
    }

    #[test]
    fn tle_guarantees_peeling_start() {
        // With only TLE edges (l=1), each bin has exactly 1 member.
        // Every coded packet should directly decode its source.
        let config = MettleConfig {
            window_size: 50,
            num_edges: 1,
            overhead_factor: 0.1,
        };
        let seed = 42;
        let num = 10;

        let packets: Vec<Vec<u8>> = (0..num).map(|i| vec![i as u8; 100]).collect();

        let mut encoder = MettleEncoder::new(config, seed);
        for pkt in &packets {
            encoder.add_source_packet(pkt);
        }
        let coded = encoder.coded_packets();

        // Feed only coded packets (no source)
        let mut decoder = MettleDecoder::new(config, num, seed);
        for cp in &coded {
            let recovered = decoder.add_coded_packet(cp);
            // Each TLE-only coded packet should decode exactly one source
            assert!(recovered, "TLE bin {} did not decode", cp.bin_index);
        }

        assert!(decoder.is_complete());
    }

    #[test]
    fn partial_recovery_before_complete() {
        let config = test_config();
        let seed = 42;
        let num = 10;

        let packets: Vec<Vec<u8>> = (0..num).map(|i| vec![i as u8; 100]).collect();

        let mut encoder = MettleEncoder::new(config, seed);
        for pkt in &packets {
            encoder.add_source_packet(pkt);
        }

        let mut decoder = MettleDecoder::new(config, num, seed);

        // Feed half the source packets
        for i in 0..5 {
            decoder.add_source_packet(i, &packets[i]);
        }

        assert!(!decoder.is_complete());
        assert_eq!(decoder.num_recovered(), 5);

        // The first 5 should be recoverable
        for i in 0..5 {
            assert!(decoder.get_source(i).is_some());
        }
    }
}
