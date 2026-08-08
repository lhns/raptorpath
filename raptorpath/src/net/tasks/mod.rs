//! The long-lived background tasks `run_impl` spawns beside the sender and
//! receiver: decoder GC, the block-ARQ sweeper, the path-command processor,
//! the RTCP-style report/keepalive loop, and the control fast path.
//!
//! History (net seam pass, 2026-08-08): these five loops lived inline in
//! `run_impl`, each preceded by a block of `let x_foo = foo.clone();`
//! bindings whose only purpose was to feed the `async move` block. They
//! share NO state with the rest of `run_impl` — every capture is a
//! pre-cloned `Arc`, a `Copy` scalar, or a channel endpoint owned by the
//! task — so each moves out as a free `pub(crate) async fn` taking exactly
//! those captures as parameters.
//!
//! Behavior contract: each `run_*` body is the former `async move` block
//! VERBATIM — same statement order, same lock scopes (including the
//! deliberate scope-end guard drops in `run_report`), same `select!` arms,
//! same early returns, same log sites. The clone-then-move at the call site
//! becomes clone-then-pass, which is the same clone at the same point in
//! `run_impl`; `tokio::spawn(run_x(..))` polls the returned future exactly
//! where the `async move` block was polled. No task was merged, reordered,
//! or given a different shutdown path.
//!
//! NOT covered here: the sender and receiver tasks (the two big loops —
//! the block-mode half of the sender lives in `net::block_sender`), the
//! status-HTTP `serve` spawn (five lines inside a `if let Some(addr)`
//! conditional — moving it would trade a smaller `run_impl` for an extra
//! indirection with no seam), and the Ctrl-C handler.

pub mod arq_sweep;
pub mod control_fastpath;
pub mod decoder_gc;
pub mod path_cmd;
pub mod report;

pub(crate) use arq_sweep::run_arq_sweep;
pub(crate) use control_fastpath::run_control_fastpath;
pub(crate) use decoder_gc::run_decoder_gc;
pub(crate) use path_cmd::run_path_cmd;
pub(crate) use report::run_report;
