//! Transport layer: QUIC-based multipath transport.
//!
//! Each path is a separate QUIC connection. Symbols are sent as
//! unreliable datagrams (QUIC DATAGRAM extension) for minimum overhead.
//! A control stream handles ACKs, loss reports, and path management.

mod protocol;
mod quic;

pub use protocol::{
    serialize_data_compact, wire_compact_active, ControlMessage, Handshake, SymbolBatch,
    WireMessage, COMPACT_DATA_TAG, PROTOCOL_VERSION, WIRE_MAGIC,
};
pub use quic::QuicTransport;
