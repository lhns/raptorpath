//! # METTLE — Streaming Erasure Code with Peeling Decoder
//!
//! Research implementation of the METTLE (Multi-Edge Type with Touch-less Leading Edge)
//! streaming erasure code, based on Yu, Yang, Meng, Xu (Georgia Tech), arxiv 2602.10020, 2026.
//!
//! METTLE is an SC-MET-LDGM code that achieves:
//! - **Pure peeling decoding** — no Gaussian elimination fallback, O(1) per symbol
//! - **Streaming operation** — genuine on-the-fly encoding/decoding
//! - **GF(2) only** — all operations are packet-level XOR, no field multiplication
//! - **Latency decoupled from block size** — depends on window size `w`, not `k`
//!
//! ## Patent Notice
//!
//! The METTLE scheme is covered by a provisional patent filed by the original authors.
//! This implementation is for **research and evaluation purposes only**.
//!
//! ## Quick Start
//!
//! ```rust
//! use mettle::{MettleEncoder, MettleDecoder, MettleConfig};
//!
//! let config = MettleConfig::default();
//! let seed = 42u64;
//!
//! // Encode
//! let mut encoder = MettleEncoder::new(config, seed);
//! let packets: Vec<Vec<u8>> = (0..20).map(|i| vec![i as u8; 1200]).collect();
//! for pkt in &packets {
//!     encoder.add_source_packet(pkt);
//! }
//! let source = encoder.source_packets().to_vec();
//! let coded = encoder.coded_packets();
//!
//! // Decode (simulate 20% source loss)
//! let mut decoder = MettleDecoder::new(config, packets.len(), seed);
//! for (i, pkt) in source.iter().enumerate() {
//!     if i % 5 != 0 {  // drop every 5th source packet
//!         decoder.add_source_packet(i, pkt);
//!     }
//! }
//! for cp in &coded {
//!     decoder.add_coded_packet(cp);
//!     if decoder.is_complete() { break; }
//! }
//! assert!(decoder.is_complete());
//! ```

pub mod decoder;
pub mod encoder;
pub(crate) mod gf2;
pub(crate) mod graph;

pub use decoder::MettleDecoder;
pub use encoder::{CodedPacket, MettleEncoder};

/// Configuration for the METTLE code.
///
/// - `window_size` (`w`): controls how many future bins each source packet can reach.
///   Larger w = better coding efficiency but higher decoding latency. Paper uses w=600.
/// - `num_edges` (`l`): number of bins each source packet is XOR'd into. Paper uses l=4.
/// - `overhead_factor` (`c`): rate overhead — the code produces `(1+c)` bins per source
///   packet on average. Paper uses c ≈ 0.1.
#[derive(Debug, Clone, Copy)]
pub struct MettleConfig {
    /// Window size (w). Controls trade-off between efficiency and latency.
    pub window_size: usize,
    /// Number of edges per source packet (l). Typically 4.
    pub num_edges: usize,
    /// Overhead factor (c). Rate = 1/(1+c). Typically 0.05-0.15.
    pub overhead_factor: f64,
}

impl Default for MettleConfig {
    fn default() -> Self {
        Self {
            window_size: 600,
            num_edges: 4,
            overhead_factor: 0.1,
        }
    }
}

impl MettleConfig {
    /// Configuration tuned for small windows (~50 symbols), as used in raptorpath.
    /// Uses higher overhead and more edges to compensate for reduced spatial coupling.
    pub fn small_window() -> Self {
        Self {
            window_size: 50,
            num_edges: 4,
            overhead_factor: 0.15,
        }
    }
}
