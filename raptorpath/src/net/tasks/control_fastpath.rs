//! Control fast path: liveness-critical messages that arrive on the reliable
//! stream, routed without queueing behind the data loop.
//!
//! Threading Q2: `PathReport` and `Ping` act on TX state (the keepalive
//! touch, the RTT feed, the Pong) and go to the window sender's input
//! channel — the sender owns the scheduler's TX half, applies the keepalive
//! touch and answers the Ping itself. `Pong` was handled as a no-op and is
//! dropped here. Everything else is forwarded to the receiver's ordered data
//! loop. Every forward is a `try_send` — never an awaited send — and dropped
//! with a warning on a full channel. The loop ends when the control channel
//! closes.

use tokio::sync::mpsc;
use tracing::warn;

use crate::transport::{ControlMessage, InboundBatch, WireMessage};

/// Control fast path: PathReport / Ping to the sender, anything else to the
/// receiver's data loop.
pub(crate) async fn run_control_fastpath(
    mut ctrl_rx: mpsc::Receiver<(u32, WireMessage)>,
    ctrl_sender_in: mpsc::Sender<InboundBatch>,
    ctrl_forward_tx: mpsc::Sender<InboundBatch>,
) {
    while let Some((path_id, msg)) = ctrl_rx.recv().await {
        match msg {
            WireMessage::Control(ControlMessage::Pong { .. }) => {}
            WireMessage::Control(
                cm @ (ControlMessage::PathReport { .. } | ControlMessage::Ping { .. }),
            ) => {
                if let Err(tokio::sync::mpsc::error::TrySendError::Full(_)) =
                    ctrl_sender_in.try_send(vec![(path_id, WireMessage::Control(cm))])
                {
                    warn!(path_id, "sender input full — dropping a liveness control message");
                }
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
    }
}
