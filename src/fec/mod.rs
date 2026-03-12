//! FEC encoding/decoding using RaptorQ fountain codes.
//!
//! Key design: original (source) symbols are sent first for minimum latency,
//! followed by repair symbols. The receiver can process original data immediately
//! and only needs to wait for the fountain decoder when packets are lost.

mod codec;
mod stream;

pub use codec::{Decoder, Encoder, EncodingParams, WireSymbol};
pub use stream::{FecStream, RepairStream};
