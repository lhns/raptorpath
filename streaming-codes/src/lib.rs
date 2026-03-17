//! Streaming codes (Badr/Martinian delay-optimal construction).
//!
//! Implements a two-layer sliding-window erasure code:
//!
//! - **Burst layer**: diagonal interleaving with stride T. Source symbol at position i
//!   is XOR'd with symbols at {i-T, i-2T, ...}. Creates T independent diagonals — a
//!   burst of length B hits at most ⌈B/T⌉ per diagonal.
//!
//! - **Random layer**: GF(256) linear combination of window symbols (reuses `gf256`).
//!   Rate = ε/(1-ε) where ε is the random (non-burst) loss rate.
//!
//! Parameters:
//! - T: delay constraint — recovered symbols are at most T positions behind newest
//! - B: burst length the code is designed to tolerate
//! - ε: random loss rate (non-burst)
//!
//! Streaming capacity: C(T,B) = T/(T+B)
//!
//! References:
//! - Badr et al., "Layered Constructions for Low-Delay Streaming Codes," IEEE Trans. IT, 2017
//! - Martinian & Sundberg, "Burst Erasure Correction Codes with Low Decoding Delay," 2004
//! - Fong et al., "Optimal Streaming Codes for Channels with Burst and Arbitrary Erasures," 2019

mod params;
mod encoder;
mod decoder;

pub use params::StreamingParams;
pub use encoder::{StreamingCoreEncoder, RepairSymbol, LAYER_BURST, LAYER_RANDOM};
pub use decoder::StreamingCoreDecoder;
