//! Sliding window FEC traits.
//!
//! These traits define the interface for continuous, streaming FEC that operates
//! over a sliding window of source symbols rather than fixed blocks.
//! They coexist with the block-based `FecEncoder`/`FecDecoder` traits.

use bytes::Bytes;

use super::traits::WireSymbol;

/// Sliding window encoder — continuously accepts source symbols and emits repair.
pub trait WindowEncoder: Send {
    /// Add the next source symbol to the window. Returns it as a WireSymbol
    /// ready for transmission.
    fn add_source(&mut self, data: &[u8]) -> WireSymbol;

    /// Generate one repair symbol covering the current window.
    fn generate_repair(&mut self) -> WireSymbol;

    /// Current window span: (oldest_seq, newest_seq).
    /// Returns (0, 0) if the window is empty.
    fn window_span(&self) -> (u64, u64);

    /// Advance window: drop symbols older than `oldest_seq`.
    /// Called when the receiver acknowledges receipt up to this point.
    fn advance(&mut self, oldest_seq: u64);

    /// Number of source symbols currently in the window.
    fn window_size(&self) -> usize;
}

/// Sliding window decoder — processes symbols as they arrive.
pub trait WindowDecoder: Send + Sync {
    /// Feed a received symbol (source or repair).
    /// Returns newly decodable source symbols as (seq, data) pairs.
    fn add_symbol(&mut self, symbol: &WireSymbol) -> Vec<(u64, Bytes)>;

    /// Advance window: discard state for symbols older than `oldest_seq`.
    fn advance(&mut self, oldest_seq: u64);

    /// Total symbols fed to this decoder.
    fn total_fed(&self) -> u64;

    /// Number of repair symbols fed to this decoder.
    fn repairs_fed(&self) -> u64 { 0 }

    /// Number of repair symbols that contributed to recovery (useful repairs).
    fn repairs_useful(&self) -> u64 { 0 }
}
