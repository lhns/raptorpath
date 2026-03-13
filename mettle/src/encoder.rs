//! METTLE encoder — streaming, systematic erasure code encoder.
//!
//! Source packets are assigned sequential positions and "thrown" into bins via
//! hash-determined edges. Each bin accumulates the XOR of all source packets
//! mapped to it. The bins become the coded (repair) packets.

use crate::gf2;
use crate::graph;
use crate::MettleConfig;

/// A coded (repair) packet produced by the encoder.
#[derive(Debug, Clone)]
pub struct CodedPacket {
    /// Bin index this coded packet corresponds to.
    pub bin_index: usize,
    /// XOR accumulation of all source packets mapped to this bin.
    pub data: Vec<u8>,
    /// Which source positions contributed to this bin (needed by decoder).
    pub members: Vec<usize>,
}

/// METTLE streaming encoder.
///
/// Source packets are added one at a time via [`add_source_packet`]. Each is stored
/// for systematic (unmodified) transmission and simultaneously XOR'd into `l` bins
/// determined by the METTLE graph structure.
pub struct MettleEncoder {
    config: MettleConfig,
    /// Bins: XOR accumulator for each bin. `None` means no packet has been thrown in yet.
    bins: Vec<Vec<u8>>,
    /// Which source positions have been thrown into each bin.
    bin_members: Vec<Vec<usize>>,
    /// Whether each bin has been initialized (has any data).
    bin_active: Vec<bool>,
    /// Source packets stored for systematic transmission.
    source_packets: Vec<Vec<u8>>,
    /// Seed for deterministic graph generation.
    seed: u64,
}

impl MettleEncoder {
    /// Create a new encoder with the given configuration.
    pub fn new(config: MettleConfig, seed: u64) -> Self {
        // Pre-allocate bins. We'll grow as needed when packets are added.
        Self {
            config,
            bins: Vec::new(),
            bin_members: Vec::new(),
            bin_active: Vec::new(),
            source_packets: Vec::new(),
            seed,
        }
    }

    /// Add a source packet at the next sequential position.
    ///
    /// The packet is stored for systematic transmission and XOR'd into `l` bins
    /// determined by the METTLE graph structure.
    pub fn add_source_packet(&mut self, packet: &[u8]) {
        let x = self.source_packets.len();
        self.source_packets.push(packet.to_vec());

        // Compute bin indices for this source position
        let bin_indices = graph::compute_bin_indices(x, &self.config, self.seed);

        // Deduplicate: if two edges map to the same bin, only XOR once.
        // Without this, double-XOR cancels the source packet's contribution.
        let mut seen_bins = std::collections::HashSet::new();
        let unique_bins: Vec<usize> = bin_indices
            .into_iter()
            .filter(|b| seen_bins.insert(*b))
            .collect();

        // Ensure bins are large enough
        let max_bin = unique_bins.iter().copied().max().unwrap_or(0);
        if max_bin >= self.bins.len() {
            let new_len = max_bin + 1;
            self.bins.resize_with(new_len, Vec::new);
            self.bin_members.resize_with(new_len, Vec::new);
            self.bin_active.resize(new_len, false);
        }

        // XOR source packet into each unique bin
        for &bin_idx in &unique_bins {
            if !self.bin_active[bin_idx] {
                // First packet into this bin — initialize
                self.bins[bin_idx] = packet.to_vec();
                self.bin_active[bin_idx] = true;
            } else {
                // XOR into existing accumulator
                gf2::xor_into(&mut self.bins[bin_idx], packet);
            }
            self.bin_members[bin_idx].push(x);
        }
    }

    /// Get all source packets (systematic — sent first, unmodified).
    pub fn source_packets(&self) -> &[Vec<u8>] {
        &self.source_packets
    }

    /// Number of source packets encoded so far.
    pub fn num_source(&self) -> usize {
        self.source_packets.len()
    }

    /// Generate coded (repair) packets from all active bins.
    ///
    /// Each bin that has at least one source packet contributing to it becomes
    /// a coded packet. The coded packet's `members` field lists which source
    /// positions are XOR'd into it — the decoder needs this for peeling.
    pub fn coded_packets(&self) -> Vec<CodedPacket> {
        self.bins
            .iter()
            .enumerate()
            .filter(|(i, _)| self.bin_active[*i])
            .map(|(i, data)| CodedPacket {
                bin_index: i,
                data: data.clone(),
                members: self.bin_members[i].clone(),
            })
            .collect()
    }

    /// Get the coded packet for a specific bin index.
    pub fn coded_packet(&self, bin_index: usize) -> Option<CodedPacket> {
        if bin_index < self.bins.len() && self.bin_active[bin_index] {
            Some(CodedPacket {
                bin_index,
                data: self.bins[bin_index].clone(),
                members: self.bin_members[bin_index].clone(),
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> MettleConfig {
        MettleConfig {
            window_size: 50,
            num_edges: 4,
            overhead_factor: 0.1,
        }
    }

    #[test]
    fn single_packet_encode() {
        let config = default_config();
        let mut encoder = MettleEncoder::new(config, 42);
        encoder.add_source_packet(&[1, 2, 3, 4]);

        assert_eq!(encoder.num_source(), 1);
        assert_eq!(encoder.source_packets().len(), 1);
        assert_eq!(encoder.source_packets()[0], vec![1, 2, 3, 4]);

        let coded = encoder.coded_packets();
        // Should have at least 1 and at most `num_edges` coded packets
        // (edges may collide to the same bin after dedup)
        assert!(!coded.is_empty());
        assert!(coded.len() <= config.num_edges);
        // Each coded packet should equal the source (only one contributor)
        for cp in &coded {
            assert_eq!(cp.data, vec![1, 2, 3, 4]);
            assert_eq!(cp.members, vec![0]);
        }
    }

    #[test]
    fn multiple_packets_encode() {
        let config = default_config();
        let mut encoder = MettleEncoder::new(config, 42);

        for i in 0..10 {
            encoder.add_source_packet(&vec![i as u8; 100]);
        }

        assert_eq!(encoder.num_source(), 10);

        let coded = encoder.coded_packets();
        // Should have some coded packets — exact count depends on graph structure
        assert!(!coded.is_empty());

        // Each coded packet should have at least one member
        for cp in &coded {
            assert!(!cp.members.is_empty());
            assert_eq!(cp.data.len(), 100);
        }
    }

    #[test]
    fn tle_bins_are_degree_one_initially() {
        // The TLE edge for each source packet goes to a unique bin.
        // Before any stochastic edges land there, these bins should have exactly 1 member.
        let config = MettleConfig {
            window_size: 50,
            num_edges: 1, // Only TLE edge
            overhead_factor: 0.1,
        };
        let mut encoder = MettleEncoder::new(config, 42);

        for i in 0..20 {
            encoder.add_source_packet(&vec![i as u8; 100]);
        }

        let coded = encoder.coded_packets();
        // With only TLE edges and c > 0, each bin should have exactly 1 member
        for cp in &coded {
            assert_eq!(
                cp.members.len(),
                1,
                "TLE-only bin {} has {} members: {:?}",
                cp.bin_index,
                cp.members.len(),
                cp.members
            );
        }
    }

    #[test]
    fn source_packets_unchanged() {
        let config = default_config();
        let mut encoder = MettleEncoder::new(config, 42);

        let original = vec![0xAB; 1200];
        encoder.add_source_packet(&original);

        // Source packet should be returned unmodified (systematic property)
        assert_eq!(encoder.source_packets()[0], original);
    }

    #[test]
    fn deterministic_encoding() {
        let config = default_config();
        let packets: Vec<Vec<u8>> = (0..10).map(|i| vec![i as u8; 100]).collect();

        let mut enc1 = MettleEncoder::new(config, 42);
        let mut enc2 = MettleEncoder::new(config, 42);

        for pkt in &packets {
            enc1.add_source_packet(pkt);
            enc2.add_source_packet(pkt);
        }

        let coded1 = enc1.coded_packets();
        let coded2 = enc2.coded_packets();

        assert_eq!(coded1.len(), coded2.len());
        for (a, b) in coded1.iter().zip(coded2.iter()) {
            assert_eq!(a.bin_index, b.bin_index);
            assert_eq!(a.data, b.data);
            assert_eq!(a.members, b.members);
        }
    }
}
