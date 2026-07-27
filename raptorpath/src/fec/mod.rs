//! FEC encoding/decoding with swappable backends.
//!
//! Supports multiple erasure code implementations:
//! - **RaptorQ** (default): rateless fountain code, near-optimal recovery (~1% overhead)
//! - **Reed-Solomon**: MDS code, zero overhead, any k of n suffices (GF(2^8), max n=255)
//! - **RLC**: random linear code over GF(2^8), near-MDS, truly rateless (RFC 8681)
//! - **METTLE**: graph-based peeling code, fast XOR-only decode (research, patent-encumbered)
//!
//! Block-mode backends (RaptorQ, RS, METTLE, RLC) use [`FecEncoder`]/[`FecDecoder`].
//! Window-mode backends (RLC, METTLE) use [`WindowEncoder`]/[`WindowDecoder`].
//!
//! (The **Streaming** two-layer code (Badr/Martinian) — `fec/streaming.rs` +
//! the `streaming-codes` crate — was RETIRED 2026-07-28: displaced by the
//! unified span machine (ADR-0064), register re-test clause discharged
//! cell-by-cell by goal-gate "Streaming Crown Re-Test" 2026-07-27.)

mod traits;
pub(crate) mod mettle_backend;
pub(crate) mod raptorq_backend;
pub(crate) mod rs_backend;
pub(crate) mod gf256;
pub(crate) mod rlc_backend;
pub(crate) mod window_traits;
pub(crate) mod rlc_window;
pub(crate) mod generation;
pub(crate) mod unified;
pub(crate) mod mettle_window;
mod stream;

pub use traits::{EncodingParams, FecBackend, FecDecoder, FecEncoder, WireSymbol};
pub use stream::{FecStream, RepairStream};
pub use window_traits::{WindowEncoder, WindowDecoder};
pub use rlc_window::{RlcWindowEncoder, RlcWindowDecoder};
pub use generation::{GenerationDecoder, GenerationEncoder};
pub use unified::UnifiedDecoder;
#[doc(hidden)]
pub use generation::reference;
pub use mettle_window::{MettleWindowEncoder, MettleWindowDecoder};
pub use raptorq_backend::{RaptorqEncoder, RaptorqDecoder};
