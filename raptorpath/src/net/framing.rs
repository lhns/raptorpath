//! Packet framing for the window pipeline's source symbols (ADR-0078).
//!
//! One format for every source symbol, at every hint: packets back to back,
//! each `[u16 LE length][bytes]`, in a symbol of `symbol_size` bytes. Zero
//! padding is the terminator: a length of 0, or fewer than 2 bytes left,
//! ends the symbol. A one-packet symbol is a packed symbol with one packet, so
//! "packed" is only how many packets a sender puts in a symbol, never a format
//! the receiver must be told about.
//!
//! [`put_packet`] is the only writer (used by `frame_window_packet` and
//! `SymbolPacker`) and [`unframe`] the only reader ([`read_packets`] only copies
//! its packets out). Every writer
//! zero-fills the symbol and every decoder hands out a full zero-padded
//! symbol, so no separate sentinel is written. (The block pipeline that once
//! used a BE length-prefixed form is gone, ADR-0069.)

/// A symbol the reader cannot unframe: a length that overruns the symbol, or
/// non-zero bytes that yield no packet. No writer produces one, so it is an
/// engine bug (or a symbol-size disagreement), never a loss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unframable;

impl std::fmt::Display for Unframable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unframable source symbol")
    }
}

impl std::error::Error for Unframable {}

/// Write `[len LE][pkt]` at the start of `dst` and return `2 + pkt.len()`.
/// `dst` must hold at least that many bytes and the packet at most
/// `u16::MAX`; callers clamp to the symbol capacity first.
pub fn put_packet(dst: &mut [u8], pkt: &[u8]) -> usize {
    assert!(pkt.len() <= u16::MAX as usize, "packet too large to frame: {} bytes", pkt.len());
    let n = 2 + pkt.len();
    dst[..2].copy_from_slice(&(pkt.len() as u16).to_le_bytes());
    dst[2..n].copy_from_slice(pkt);
    n
}

/// The packets of one validated source symbol, as slices of it (no
/// allocation). Made only by [`unframe`], which has checked the whole chain.
#[derive(Clone)]
pub struct Packets<'a> {
    rest: &'a [u8],
}

impl<'a> Iterator for Packets<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        let r = self.rest;
        if r.len() < 2 {
            return None;
        }
        let len = u16::from_le_bytes([r[0], r[1]]) as usize;
        if len == 0 || 2 + len > r.len() {
            return None; // terminator (an overrun cannot occur after `unframe`)
        }
        self.rest = &r[2 + len..];
        Some(&r[2..2 + len])
    }
}

/// The one reader: walk the chain of one source symbol once and return its
/// packets as slices. Stops at a zero length or when fewer than 2 bytes remain.
/// `Err` when a length overruns the symbol, or when no packet comes out of a
/// symbol with a non-zero byte. An all-zero symbol yields no packets (the frame
/// of an empty TUN packet). Bytes after the terminator are not checked: no
/// writer leaves any. Allocation-free: the receiver copies each packet out
/// once, where it needs an owned one.
pub fn unframe(sym: &[u8]) -> Result<Packets<'_>, Unframable> {
    let (mut cursor, mut n) = (0usize, 0usize);
    while cursor + 2 <= sym.len() {
        let len = u16::from_le_bytes([sym[cursor], sym[cursor + 1]]) as usize;
        if len == 0 {
            break;
        }
        cursor += 2;
        if cursor + len > sym.len() {
            return Err(Unframable);
        }
        cursor += len;
        n += 1;
    }
    if n == 0 && sym.iter().any(|&b| b != 0) {
        return Err(Unframable);
    }
    Ok(Packets { rest: sym })
}

/// [`unframe`], with every packet copied into its own `Vec` (tests, tools).
pub fn read_packets(sym: &[u8]) -> Result<Vec<Vec<u8>>, Unframable> {
    Ok(unframe(sym)?.map(<[u8]>::to_vec).collect())
}

// ---------------------------------------------------------------------------
// One-packet symbols
// ---------------------------------------------------------------------------

/// Frame a single packet as a source symbol: a zeroed buffer of `symbol_size`
/// bytes holding `[len LE][packet]` (truncated to `symbol_size - 2`).
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
    put_packet(&mut buf, &data[..len]);
    buf
}

// ---------------------------------------------------------------------------
// SymbolPacker: accumulate multiple small packets into one symbol
// ---------------------------------------------------------------------------

use std::time::{Duration, Instant};

/// Packs multiple small packets into a single FEC symbol with the one framing
/// (`put_packet`: LE u16 length + data per packet; the zero padding of the
/// symbol is the terminator).
///
/// This dramatically reduces padding waste for small packets (VoIP 160B,
/// DNS 60B, TCP ACK 52B) that would otherwise each consume a full 512B symbol.
///
/// A symbol with one packet is exactly what `frame_window_packet` writes, and
/// `read_packets` reads both; the receiver never learns whether a sender packs.
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

    /// Maximum payload capacity per symbol (symbol_size minus 2 bytes, which
    /// always leave room for a zero length field as the terminator).
    fn capacity(&self) -> usize {
        (self.symbol_size as usize).saturating_sub(2)
    }

    /// Bytes needed to frame a packet: 2-byte length prefix + packet data.
    fn framed_len(packet: &[u8]) -> usize {
        2 + packet.len()
    }

    /// Frame `packet` onto the end of the buffer. The buffer only grows by
    /// appending and is cleared at `emit`, so no stale byte can ever sit
    /// where a terminator is read.
    fn append(&mut self, packet: &[u8]) {
        let at = self.buffer.len();
        self.buffer.resize(at + 2 + packet.len(), 0);
        put_packet(&mut self.buffer[at..], packet);
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
            self.append(&packet[..truncated_len]);
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

        self.append(packet);
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

    /// Emit the current buffer as a zero-padded symbol (the padding is the terminator).
    fn emit(&mut self) -> Vec<u8> {
        let size = self.symbol_size as usize;
        let mut symbol = vec![0u8; size];
        let copy_len = self.buffer.len().min(size);
        symbol[..copy_len].copy_from_slice(&self.buffer[..copy_len]);
        self.buffer.clear();
        symbol
    }
}

#[cfg(test)]
mod tests;
