//! FEC encoding/decoding with swappable backends.
//!
//! Supports multiple erasure code implementations:
//! - **RaptorQ** (default): rateless fountain code, near-optimal erasure recovery
//! - **METTLE**: streaming erasure code with pure peeling decoder (research)
//!
//! The backend is selected via [`FecBackend`], which provides factory methods
//! for creating encoders and decoders. The rest of raptorpath uses the
//! [`FecEncoder`] and [`FecDecoder`] traits, making the FEC layer pluggable.

mod traits;
pub(crate) mod mettle_backend;
pub(crate) mod raptorq_backend;
mod stream;

pub use traits::{EncodingParams, FecBackend, FecDecoder, FecEncoder, WireSymbol};
pub use stream::{FecStream, RepairStream};
