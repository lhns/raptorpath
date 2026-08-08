//! Runtime path add/remove processor, fed by the status-HTTP API.
//!
//! Behavior contract: the body is the former inline `async move` block from
//! `run_impl` verbatim — same `select!` between the command channel and the
//! shutdown broadcast, same `break` on a closed channel, same order of
//! `add_path` on transport → scheduler → stats → `spawn_receiver_for_path`,
//! and `remove_path` on transport → scheduler. `next_path_id` is still
//! seeded from `config.bind_addrs.len()` at the `run_impl` call site, so the
//! first runtime path keeps the same id it had before.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::monitor::stats::SharedStats;
use crate::scheduler::Scheduler;
use crate::transport::{QuicTransport, WireMessage};

/// Path command processor: handles runtime add/remove of paths.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_path_cmd(
    mut path_cmd_rx: mpsc::Receiver<crate::monitor::http::PathCommand>,
    cmd_transport: Arc<QuicTransport>,
    cmd_scheduler: Arc<parking_lot::Mutex<Scheduler>>,
    cmd_stats: Arc<SharedStats>,
    cmd_msg_tx: mpsc::Sender<(u32, WireMessage)>,
    cmd_ctrl_tx: mpsc::Sender<(u32, WireMessage)>,
    next_path_id: Arc<AtomicU64>,
    mut cmd_shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) {
    loop {
        tokio::select! {
            cmd = path_cmd_rx.recv() => {
                let cmd = match cmd {
                    Some(c) => c,
                    None => break,
                };
                match cmd {
                    crate::monitor::http::PathCommand::Add { bind_addr, peer_addr } => {
                        let path_id = next_path_id.fetch_add(1, Ordering::Relaxed) as u32;
                        info!(path_id, %bind_addr, ?peer_addr, "adding path at runtime");
                        match cmd_transport.add_path(path_id, bind_addr, peer_addr).await {
                            Ok(conn) => {
                                cmd_scheduler.lock().add_path(path_id);
                                cmd_stats.add_path(path_id);
                                cmd_transport.spawn_receiver_for_path(
                                    path_id,
                                    conn,
                                    cmd_msg_tx.clone(),
                                    cmd_ctrl_tx.clone(),
                                );
                                info!(path_id, "path added successfully");
                            }
                            Err(e) => {
                                warn!(path_id, ?e, "failed to add path");
                            }
                        }
                    }
                    crate::monitor::http::PathCommand::Remove { path_id } => {
                        info!(path_id, "removing path at runtime");
                        cmd_transport.remove_path(path_id);
                        cmd_scheduler.lock().remove_path(path_id);
                        info!(path_id, "path removed");
                    }
                }
            }
            _ = cmd_shutdown_rx.recv() => break,
        }
    }
}
