//! FEC encoding/decoding with swappable backends.
//!
//! Supports multiple erasure code implementations:
//! - **RaptorQ** (default): rateless fountain code, near-optimal recovery (~1% overhead)
//! - **Reed-Solomon**: MDS code, zero overhead, any k of n suffices (GF(2^8), max n=255)
//! - **RLC**: random linear code over GF(2^8), near-MDS, truly rateless (RFC 8681)
//!
//! Block-mode backends (RaptorQ, RS, RLC) use [`FecEncoder`]/[`FecDecoder`].
//! Window-mode backends (RLC) use [`WindowEncoder`]/[`WindowDecoder`].

mod traits;
pub(crate) mod raptorq_backend;
pub(crate) mod rs_backend;
pub(crate) mod gf256;
pub(crate) mod rlc_backend;
pub(crate) mod window_traits;
pub(crate) mod rlc_window;
pub(crate) mod generation;
pub(crate) mod unified;
mod stream;

pub use traits::{EncodingParams, FecBackend, FecDecoder, FecEncoder, WireSymbol};
pub use stream::FecStream;
pub use window_traits::{WindowEncoder, WindowDecoder};
pub use rlc_window::{RlcWindowEncoder, RlcWindowDecoder};
pub use generation::{GenerationDecoder, GenerationEncoder};
pub use unified::UnifiedDecoder;
#[doc(hidden)]
pub use generation::reference;
pub use raptorq_backend::{RaptorqEncoder, RaptorqDecoder};
