//! Runtime path add/remove processor, fed by the status-HTTP API.
//!
//! Order of operations: `add_path` on transport → stats → the sender's
//! scheduler (threading Q2: a [`SenderCmd::AddPath`] message, awaited, so
//! the TX half knows the path before its first ack can arrive) →
//! `start_readers_for_path` (the path's I/O owner starts forwarding); and
//! `remove_path` on transport → the sender's scheduler
//! ([`SenderCmd::RemovePath`]). `next_path_id` is seeded from
//! `config.bind_addrs.len()` at the `run_impl` call site, so runtime path ids
//! follow the configured ones.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::mpsc;
use tracing::{info, warn};

use super::super::SenderCmd;
use crate::monitor::stats::SharedStats;
use crate::transport::{QuicTransport, WireMessage};

/// Path command processor: handles runtime add/remove of paths.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_path_cmd(
    mut path_cmd_rx: mpsc::Receiver<crate::monitor::http::PathCommand>,
    cmd_transport: Arc<QuicTransport>,
    cmd_sender: mpsc::Sender<SenderCmd>,
    cmd_stats: Arc<SharedStats>,
    cmd_msg_tx: mpsc::Sender<crate::transport::InboundBatch>,
    cmd_ctrl_tx: mpsc::Sender<(u32, WireMessage)>,
    cmd_sender_in: mpsc::Sender<crate::transport::InboundBatch>,
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
                            Ok(()) => {
                                cmd_stats.add_path(path_id);
                                let (done, done_rx) = tokio::sync::oneshot::channel();
                                if cmd_sender
                                    .send(SenderCmd::AddPath { path: path_id, done })
                                    .await
                                    .is_err()
                                    || done_rx.await.is_err()
                                {
                                    warn!(path_id, "the sender is gone — path not added to the scheduler");
                                    break;
                                }
                                cmd_transport
                                    .start_readers_for_path(
                                        path_id,
                                        cmd_msg_tx.clone(),
                                        cmd_ctrl_tx.clone(),
                                        cmd_sender_in.clone(),
                                    )
                                    .await;
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
                        if cmd_sender.send(SenderCmd::RemovePath(path_id)).await.is_err() {
                            break;
                        }
                        info!(path_id, "path removed");
                    }
                }
            }
            _ = cmd_shutdown_rx.recv() => break,
        }
    }
}
