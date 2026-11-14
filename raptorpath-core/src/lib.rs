//! The sans-IO decision core (state machine in, verdicts out).
//!
//! Boundary (docs/derivations/shared-core-design.md, section 3):
//! - depends on `raptorpath-math` only; `gf256` arrives through math;
//! - std, no I/O: no clock reads, no env, no threads, no sockets, no async
//!   runtime, QUIC stack, logging or RNG crate, no statics or atomics. Time
//!   and random draws are arguments;
//! - no hint type: a protocol hint can only arrive as a delta number (the
//!   no-mode-switch invariant, CLAUDE.md);
//! - the caller owns every state struct; decisions return verdicts.
//!
//! The crate is empty at this step (design doc phase 0.1); decisions move in
//! from the engine in the later steps. `tests/deny_list.rs` enforces the
//! boundary on this crate's source.
