//! Transport layer: QUIC-based multipath transport.
//!
//! Each path is a separate QUIC connection. Symbols are sent as
//! unreliable datagrams (QUIC DATAGRAM extension) for minimum overhead.
//! A control stream handles ACKs, loss reports, and path management.

mod protocol;
mod quic;

pub use protocol::{ControlMessage, Handshake, PROTOCOL_VERSION, SymbolBatch, WireMessage, WIRE_MAGIC};
pub use quic::QuicTransport;
