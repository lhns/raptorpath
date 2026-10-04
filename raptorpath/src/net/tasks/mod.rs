//! The long-lived background tasks `run_impl` spawns beside the sender and
//! receiver: the path-command processor, the RTCP-style report/keepalive
//! loop, and the control fast path.
//!
//! They share no state with the rest of `run_impl` — every capture is a
//! pre-cloned `Arc`, a `Copy` scalar, or a channel endpoint owned by the
//! task — so each is a free `pub(crate) async fn` taking exactly those
//! captures as parameters, spawned with `tokio::spawn(run_x(..))`.
//!
//! Not here: the sender and receiver tasks, the status-HTTP `serve` spawn,
//! and the Ctrl-C handler.

pub mod control_fastpath;
pub mod path_cmd;
pub mod report;

pub(crate) use control_fastpath::run_control_fastpath;
pub(crate) use path_cmd::run_path_cmd;
pub(crate) use report::run_report;
