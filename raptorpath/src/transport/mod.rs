//! Transport layer: QUIC-based multipath transport.
//!
//! Each path is a separate QUIC connection. Symbols are sent as
//! unreliable datagrams (QUIC DATAGRAM extension) for minimum overhead.
//! A control stream handles ACKs, loss reports, and path management.
//!
//! Threading Q1: each path's quinn connection lives in that path's I/O owner
//! (`io_owner`), the only code that calls quinn for the path.

mod bbr_rs;
pub mod io_owner;
mod l0_netem;
mod protocol;
mod quic;
mod rcvbuf;

pub use protocol::{
    serialize_data_compact, serialize_symbol_compact_in, wire_compact_active, ControlMessage, Handshake, SymbolBatch,
    WireMessage, COMPACT_DATA_TAG, PROTOCOL_VERSION, WIRE_MAGIC,
};
pub use io_owner::{InboundBatch, IoRtArm, TxBatch};
pub use quic::QuicTransport;
