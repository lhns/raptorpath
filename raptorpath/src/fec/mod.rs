//! FEC encoding/decoding for the one (sliding-window) pipeline.
//!
//! The code is RLC — random linear combinations over GF(2^8), near-MDS,
//! truly rateless (RFC 8681) — behind [`WindowEncoder`]/[`WindowDecoder`]:
//! the sliding-window machine, the generation machine and the unified
//! decoder over both wires. The block-only codecs (RaptorQ, Reed-Solomon,
//! block RLC) were removed with the block pipeline (ADR-0069).

mod traits;
pub(crate) mod gf256;
pub(crate) mod window_traits;
pub(crate) mod rlc_window;
pub(crate) mod generation;
pub(crate) mod unified;

pub use traits::{EncodingParams, FecBackend, WireSymbol};
pub use window_traits::{WindowEncoder, WindowDecoder};
pub use rlc_window::{RlcWindowEncoder, RlcWindowDecoder};
pub use generation::{GenerationDecoder, GenerationEncoder};
pub use unified::UnifiedDecoder;
#[doc(hidden)]
pub use generation::reference;
