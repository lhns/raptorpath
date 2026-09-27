//! Control fast path: liveness-critical messages handled off the reliable
//! stream without queueing behind the data loop.
//!
//! `PathReport`, `Ping` and `Pong` go to `handle_control_message` with the
//! ARQ ledger, the peer-ack atomic and the Copa feed all `None` (the fast
//! path never touches them); everything else is forwarded with `try_send` —
//! never an awaited send — and dropped with a warning on a full data
//! channel. The loop ends when the control channel closes.

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::mpsc;
use tracing::warn;

use super::super::control_msg::{ControlCtx, handle_control_message};
use crate::control::FecRateController;
use crate::fec::FecDecoder;
use crate::monitor::stats::SharedStats;
use crate::scheduler::Scheduler;
use crate::transport::{ControlMessage, QuicTransport, WireMessage};

/// Control fast path: liveness-critical messages (PathReport, Ping,
/// Pong) are handled immediately; anything else that arrives via the
/// reliable stream is forwarded to the ordered data loop.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_control_fastpath(
    mut ctrl_rx: mpsc::Receiver<(u32, WireMessage)>,
    ctrl_scheduler: Arc<parking_lot::Mutex<Scheduler>>,
    ctrl_fec: Arc<parking_lot::Mutex<FecRateController>>,
    ctrl_decoders: Arc<DashMap<u64, Box<dyn FecDecoder>>>,
    ctrl_sent_counts: Arc<DashMap<(u64, u32), u32>>,
    ctrl_transport: Arc<QuicTransport>,
    ctrl_stats: Arc<SharedStats>,
    ctrl_forward_tx: mpsc::Sender<(u32, WireMessage)>,
    ctrl_mstar_anchor: bool,
) {
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
                        fec_controller: &ctrl_fec,
                        decoders: &ctrl_decoders,
                        sent_counts: &ctrl_sent_counts,
                        transport: &ctrl_transport,
                        stats: &ctrl_stats,
                        // The fast path only handles PathReport/Ping/Pong;
                        // Acks (which drive block ARQ) and WindowAcks go
                        // through the data loop, so neither the ledger nor
                        // the peer-ack atomic (nor the Copa feed) is needed
                        // here.
                        nack_tx: None,
                        block_arq: None,
                        batch_counter: None,
                        peer_window_ack: None,
                        deficit_tx: None,
                        request_tx: None,
                        sack_tx: None,
                        copa_feed: None,
                        mstar_anchor: ctrl_mstar_anchor,
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
                    ctrl_forward_tx.try_send((path_id, other))
                {
                    warn!(path_id, "data channel full — dropping forwarded control message");
                }
            }
        }
    }
}
