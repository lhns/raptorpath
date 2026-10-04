//! Packet framing for the window pipeline's source symbols.
//!
//! Length-prefixed framing (`frame_packet`/`frame_end`/`extract_packets`)
//! carries several packets in one symbol in packed mode (`SymbolPacker`);
//! `frame_window_packet` carries one packet per symbol. (The block pipeline
//! that once used the length-prefixed form for whole FEC blocks is gone,
//! ADR-0069.)
//!
//! Wire format per packet: [u16 BE length][packet data]
//! End sentinel:           [u16 0x0000]

/// Frame multiple packets into a block buffer with length prefixes.
/// Each packet is prefixed with its length as a big-endian u16.
pub fn frame_packet(block_buf: &mut Vec<u8>, packet: &[u8]) {
    assert!(
        packet.len() <= u16::MAX as usize,
        "packet too large to frame: {} bytes",
        packet.len()
    );
    block_buf.extend_from_slice(&(packet.len() as u16).to_be_bytes());
    block_buf.extend_from_slice(packet);
}

/// Write the end-of-block sentinel (zero-length marker).
pub fn frame_end(block_buf: &mut Vec<u8>) {
    block_buf.extend_from_slice(&0u16.to_be_bytes());
}

/// Extract individual packets from a decoded block.
/// Returns a Vec of packets. Stops at end-of-block sentinel or end of data.
pub fn extract_packets(data: &[u8]) -> Vec<Vec<u8>> {
    let mut packets = Vec::new();
    let mut cursor = 0;

    while cursor + 2 <= data.len() {
        let len = u16::from_be_bytes([data[cursor], data[cursor + 1]]) as usize;
        cursor += 2;

        if len == 0 {
            break; // end-of-block sentinel
        }

        if cursor + len > data.len() {
            // Truncated packet — block may have been padded by FEC
            break;
        }

        packets.push(data[cursor..cursor + len].to_vec());
        cursor += len;
    }

    packets
}

// ---------------------------------------------------------------------------
// Window-mode framing: each source symbol = one packet (padded to symbol_size)
// ---------------------------------------------------------------------------

/// Frame a single packet as a window-mode source symbol.
/// Returns a padded buffer of `symbol_size` bytes with a 2-byte length prefix.
pub fn frame_window_packet(data: &[u8], symbol_size: u16) -> Vec<u8> {
    // `frm` seam (`RWM_CPUPROF`, default OFF): the per-source-symbol copy out
    // of the TUN read buffer into a `symbol_size` padded frame. One of the
    // named copies the sender CPU decomposition exists to size.
    crate::net::cpuprof::timed(crate::net::cpuprof::Seam::Frm, || {
        frame_window_packet_inner(data, symbol_size)
    })
}

fn frame_window_packet_inner(data: &[u8], symbol_size: u16) -> Vec<u8> {
    let size = symbol_size as usize;
    let mut buf = vec![0u8; size];
    let max_payload = size.saturating_sub(2);
    let len = data.len().min(max_payload);
    buf[0..2].copy_from_slice(&(len as u16).to_le_bytes());
    buf[2..2 + len].copy_from_slice(&data[..len]);
    buf
}

/// Extract the original packet from a window-mode source symbol.
pub fn extract_window_packet(symbol_data: &[u8]) -> Option<Vec<u8>> {
    if symbol_data.len() < 2 {
        return None;
    }
    let len = u16::from_le_bytes([symbol_data[0], symbol_data[1]]) as usize;
    if len == 0 || 2 + len > symbol_data.len() {
        return None;
    }
    Some(symbol_data[2..2 + len].to_vec())
}

// ---------------------------------------------------------------------------
// SymbolPacker: accumulate multiple small packets into one symbol
// ---------------------------------------------------------------------------

use std::time::{Duration, Instant};

/// Packs multiple small packets into a single FEC symbol using the
/// length-prefix framing (BE u16 length + data per packet, 0x0000 sentinel).
///
/// This dramatically reduces padding waste for small packets (VoIP 160B,
/// DNS 60B, TCP ACK 52B) that would otherwise each consume a full 512B symbol.
///
/// The packed symbol format matches `extract_packets()` — no new parser needed.
pub struct SymbolPacker {
    symbol_size: u16,
    buffer: Vec<u8>,
    flush_timeout: Duration,
    last_push: Instant,
}

impl SymbolPacker {
    /// Create a new packer with the given symbol size and flush timeout.
    pub fn new(symbol_size: u16, flush_timeout: Duration) -> Self {
        Self {
            symbol_size,
            buffer: Vec::with_capacity(symbol_size as usize),
            flush_timeout,
            last_push: Instant::now(),
        }
    }

    /// Maximum payload capacity per symbol (symbol_size minus 2-byte sentinel).
    fn capacity(&self) -> usize {
        (self.symbol_size as usize).saturating_sub(2)
    }

    /// Bytes needed to frame a packet: 2-byte length prefix + packet data.
    fn framed_len(packet: &[u8]) -> usize {
        2 + packet.len()
    }

    /// Append a packet to the buffer. If adding this packet would exceed the
    /// symbol capacity, the current buffer is flushed as a packed symbol first,
    /// and the new packet starts a fresh buffer.
    ///
    /// Returns `Some(packed_symbol)` if the buffer was flushed, `None` otherwise.
    pub fn push(&mut self, packet: &[u8]) -> Option<Vec<u8>> {
        let framed = Self::framed_len(packet);
        let cap = self.capacity();

        // Packet too large to fit even in an empty symbol — emit it solo
        if framed > cap {
            let result = if !self.buffer.is_empty() {
                Some(self.emit())
            } else {
                None
            };
            // Buffer the truncated framed packet (truncated to capacity).
            let max_payload = cap;
            let truncated_len = packet.len().min(max_payload.saturating_sub(2));
            frame_packet(&mut self.buffer, &packet[..truncated_len]);
            self.last_push = Instant::now();
            // The buffer now has exactly one (possibly truncated) packet.
            // It will be flushed on the next push or flush call.
            return result.or_else(|| Some(self.emit()));
        }

        // If adding this packet would exceed capacity, flush first
        let result = if self.buffer.len() + framed > cap {
            Some(self.emit())
        } else {
            None
        };

        frame_packet(&mut self.buffer, packet);
        self.last_push = Instant::now();
        result
    }

    /// Force-emit the current buffer as a padded symbol (even if partially full).
    /// Returns `None` if the buffer is empty.
    pub fn flush(&mut self) -> Option<Vec<u8>> {
        if self.buffer.is_empty() {
            return None;
        }
        Some(self.emit())
    }

    /// Returns true if the flush timeout has elapsed since the last push.
    // Test-only consumer: this file's packer tests.
    pub fn should_flush(&self) -> bool {
        !self.buffer.is_empty() && self.last_push.elapsed() >= self.flush_timeout
    }

    /// Returns true if the buffer contains data waiting to be emitted.
    pub fn is_pending(&self) -> bool {
        !self.buffer.is_empty()
    }

    /// Returns the duration until the flush timeout expires, or zero if already expired.
    pub fn time_until_flush(&self) -> Duration {
        if self.buffer.is_empty() {
            self.flush_timeout
        } else {
            self.flush_timeout
                .checked_sub(self.last_push.elapsed())
                .unwrap_or(Duration::ZERO)
        }
    }

    /// Emit the current buffer as a padded symbol with end-of-block sentinel.
    fn emit(&mut self) -> Vec<u8> {
        let size = self.symbol_size as usize;
        frame_end(&mut self.buffer);
        let mut symbol = vec![0u8; size];
        let copy_len = self.buffer.len().min(size);
        symbol[..copy_len].copy_from_slice(&self.buffer[..copy_len]);
        self.buffer.clear();
        symbol
    }
}

#[cfg(test)]
mod tests;
