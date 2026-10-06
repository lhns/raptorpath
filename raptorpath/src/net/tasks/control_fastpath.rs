//! Control fast path: liveness-critical messages handled off the reliable
//! stream without queueing behind the data loop.
//!
//! `PathReport`, `Ping` and `Pong` go to `handle_control_message` with the
//! peer-ack atomic and the Copa feed both `None` (the fast
//! path never touches them); everything else is forwarded with `try_send` —
//! never an awaited send — and dropped with a warning on a full data
//! channel. The loop ends when the control channel closes.
//!
//! Threading Q1: the Pong a Ping triggers is staged in this task's own
//! `TxBatch` and handed to the path's I/O owner before the loop waits again.

use std::cell::RefCell;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::warn;

use super::super::control_msg::{ControlCtx, handle_control_message};
use crate::monitor::stats::SharedStats;
use crate::transport::{ControlMessage, InboundBatch, QuicTransport, TxBatch, WireMessage};

/// Control fast path: liveness-critical messages (PathReport, Ping,
/// Pong) are handled immediately; anything else that arrives via the
/// reliable stream is forwarded to the ordered data loop.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_control_fastpath(
    mut ctrl_rx: mpsc::Receiver<(u32, WireMessage)>,
    ctrl_scheduler: Arc<crate::scheduler::SchedMutex>,
    ctrl_transport: Arc<QuicTransport>,
    ctrl_stats: Arc<SharedStats>,
    ctrl_forward_tx: mpsc::Sender<InboundBatch>,
    ctrl_mstar_anchor: bool,
) {
    let out = RefCell::new(TxBatch::new());
    while let Some((path_id, msg)) = ctrl_rx.recv().await {
        match msg {
            WireMessage::Control(
                cm @ (ControlMessage::PathReport { .. }
                | ControlMessage::Ping { .. }
                | ControlMessage::Pong { .. }),
            ) => {
                handle_control_message(
                    path_id,
                    cm,
                    &ControlCtx {
                        scheduler: &ctrl_scheduler,
                        transport: &ctrl_transport,
                        stats: &ctrl_stats,
                        // The fast path only handles PathReport/Ping/Pong;
                        // Acks and WindowAcks go through the data loop, so
                        // neither the peer-ack atomic nor the Copa feed is
                        // needed here.
                        nack_tx: None,
                        peer_window_ack: None,
                        ack_wake: None,
                        deficit_tx: None,
                        request_tx: None,
                        sack_tx: None,
                        copa_feed: None,
                        mstar_anchor: ctrl_mstar_anchor,
                        out: &out,
                    },
                );
            }
            other => {
                // Never await into the data channel: under a symbol
                // flood it is full, an awaited send here stalls the
                // uni-stream accept loop, stream credit (100) runs
                // out, and the report task wedges inside
                // send_control — taking the dead-path checker with
                // it. Dropping a forwarded stream message under
                // overload is survivable; wedging liveness is not.
                if let Err(tokio::sync::mpsc::error::TrySendError::Full(_)) =
                    ctrl_forward_tx.try_send(vec![(path_id, other)])
                {
                    warn!(path_id, "data channel full — dropping forwarded control message");
                }
            }
        }
        // The staged Pong (if any), to the owner, before waiting again.
        let mut b = std::mem::take(&mut *out.borrow_mut());
        ctrl_transport.flush(&mut b).await;
        *out.borrow_mut() = b;
    }
}
