//! Threading Q2 — the sender's ownership of the TX state and its inputs.
//!
//! **Ownership by direction (plan rule 2).** The window sender task owns the
//! scheduler's TX half ([`crate::scheduler::Scheduler`]) and the
//! [`FecRateController`] in one [`TxCore`], and uses both by plain `&mut`: no
//! mutex, no cell, no other task can name them. The receiver owns the RX half
//! (`scheduler::RxScheduler`). Cross-direction reads go through the per-path
//! published atomics (`monitor::stats::CrossDirection`).
//!
//! **Inputs by message (plan rules 1, 3, 4).** Everything that acts on TX
//! state from outside the sender reaches it on one of two bounded channels:
//!
//! * the INPUT channel (`InboundBatch`): the TX-direction control messages
//!   (`control_msg::is_tx_control` — WindowAck, the per-batch Ack,
//!   PathReport, Ping), forwarded by the per-path I/O owners as one batch per
//!   owner poll and by the control fast path. This is where the client's ack
//!   handling now runs: in the sender, by [`apply_inputs`].
//! * the COMMAND channel ([`SenderCmd`]): the cold tasks — the 2 s report
//!   tick, the HTTP path add / remove, the receiver's FEC feedback and its
//!   dead-path revival nudge.
//!
//! Both are drained with `recv_many` — one coop-budget unit per drain, never
//! per message — at the loop top (non-blocking: polled once, only when
//! non-empty) and in always-armed `select!` arms. The wake comes from the
//! channel: a parked sender is woken by the send, a busy one finds the batch
//! at its next loop top (P1's `AckWake` Notify is retired). Each drained
//! batch is applied, then the TX view is published once.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::task::Poll;

use tokio::sync::{mpsc, oneshot};

use super::control_msg::{ControlCtx, handle_control_message};
use super::{CopaFeed, FireCause, SackReport};
use crate::control::FecRateController;
use crate::monitor::stats::SharedStats;
use crate::scheduler::{PathId, Scheduler};
use crate::transport::{ControlMessage, InboundBatch, QuicTransport, TxBatch, WireMessage};

/// Depth of the sender's input channel, in owner batches.
///
/// Provenance: a resource bound outside any law — the same bound as the
/// receiver's inbound channel (`MSG_CHANNEL_BATCHES` = ADR-0011's 4096
/// datagrams / `INBOUND_BATCH_MAX`), since the ack lane is the other half of
/// the same owner poll's datagrams. Full = the owner keeps its batch and
/// pauses its reads (back-pressure into quinn's bounded buffer), never a
/// drop.
pub(crate) const SENDER_IN_BATCHES: usize = super::MSG_CHANNEL_BATCHES;

/// Depth of the sender's command channel. Provenance: a resource bound —
/// the cold producers send at most a few commands per 2 s report interval
/// (one tick, one FEC feedback, a revival nudge per dead path, an HTTP path
/// command); 64 is the depth the other cold channels use (`deficit_tx`,
/// `request_tx`, `sack_tx`).
pub(crate) const SENDER_CMD_DEPTH: usize = 64;

/// The TX half's owner: the scheduler (TX half), the FEC controller and the
/// sender `[ETA]` exit flush, which renders from the scheduler it sits beside
/// (so a sender dropped at runtime teardown still prints its final line —
/// there is no lock to try).
pub(crate) struct TxCore {
    pub sched: Scheduler,
    pub fec: FecRateController,
    pub eta: super::eta::SenderEtaFlush,
}

impl TxCore {
    pub(crate) fn new(sched: Scheduler, fec: FecRateController, eta_on: bool) -> Self {
        Self { sched, fec, eta: super::eta::SenderEtaFlush::new(eta_on) }
    }
}

impl Drop for TxCore {
    fn drop(&mut self) {
        self.eta.flush_final(&mut self.sched);
    }
}

/// A cold task's request on TX state.
pub(crate) enum SenderCmd {
    /// The 2 s report tick's TX part (`tasks::report::report_tick_tx`): the
    /// send-rate feed (per path, bytes/s, already gated by the report task),
    /// the MTU store, the dead-path check, the in-flight expiry and the
    /// PathReport build. The reply carries the reports to send.
    ReportTick {
        rates: Vec<(PathId, f64)>,
        mtus: Vec<(PathId, usize)>,
        reply: oneshot::Sender<Vec<(PathId, ControlMessage)>>,
    },
    /// A path added at runtime: the scheduler adds it, publishes, replies.
    AddPath { path: PathId, done: oneshot::Sender<()> },
    /// A path removed at runtime.
    RemovePath(PathId),
    /// The receiver decoder's PI feedback (`FecRateController::
    /// feedback_update_window`, the controller's one receiver-side site).
    FecFeedback { fed: u64, useful: u64 },
    /// The receiver saw an arrival on a path the sender published as dead:
    /// apply the arrival stamp now (revival) rather than at the sender's next
    /// natural wake.
    Revive(PathId),
}

/// The sender's two input receivers and the ack handling's channel seats
/// (the sender is also these channels' consumer).
pub(crate) struct SenderInputs<'a> {
    pub acks: &'a mut mpsc::Receiver<InboundBatch>,
    pub cmds: &'a mut mpsc::Receiver<SenderCmd>,
    pub nack_tx: Option<&'a mpsc::Sender<(FireCause, u32, Vec<(u64, u64)>)>>,
    pub sack_tx: Option<&'a mpsc::Sender<SackReport>>,
}

/// What the input handling reads besides the TX half.
pub(crate) struct InputCtx<'a> {
    pub transport: &'a Arc<QuicTransport>,
    pub stats: &'a Arc<SharedStats>,
    pub peer_window_ack: &'a Arc<AtomicU64>,
    pub nack_tx: Option<&'a mpsc::Sender<(FireCause, u32, Vec<(u64, u64)>)>>,
    pub sack_tx: Option<&'a mpsc::Sender<SackReport>>,
    pub copa_feed: Option<&'a Arc<CopaFeed>>,
    pub mstar_anchor: bool,
}

/// Poll `f` once with the task's own context: `Some(output)` if it is ready
/// now, `None` otherwise (its waker is then registered, harmlessly).
pub(crate) async fn poll_once<F: Future>(f: F) -> Option<F::Output> {
    let mut f = std::pin::pin!(f);
    std::future::poll_fn(|cx| {
        Poll::Ready(match f.as_mut().poll(cx) {
            Poll::Ready(v) => Some(v),
            Poll::Pending => None,
        })
    })
    .await
}

/// Apply drained input batches (in order) to the TX half. Returns the
/// number of control messages handled. The batches' `Vec`s are left empty.
pub(crate) fn apply_inputs(
    batches: &mut Vec<InboundBatch>,
    sched: &mut Scheduler,
    out: &mut TxBatch,
    c: &InputCtx<'_>,
) -> u64 {
    let mut n = 0u64;
    for batch in batches.drain(..) {
        for (pid, msg) in batch {
            if let WireMessage::Control(cm) = msg {
                n += 1;
                handle_control_message(
                    pid,
                    cm,
                    &mut ControlCtx {
                        scheduler: &mut *sched,
                        transport: c.transport,
                        stats: c.stats,
                        nack_tx: c.nack_tx,
                        peer_window_ack: Some(c.peer_window_ack),
                        sack_tx: c.sack_tx,
                        copa_feed: c.copa_feed,
                        mstar_anchor: c.mstar_anchor,
                        out: &mut *out,
                    },
                );
            }
        }
    }
    n
}

/// Apply drained cold commands (in order) to the TX half.
pub(crate) fn apply_cmds(
    cmds: &mut Vec<SenderCmd>,
    sched: &mut Scheduler,
    fec: &mut FecRateController,
    stats: &SharedStats,
) {
    use std::sync::atomic::Ordering::Relaxed;
    for cmd in cmds.drain(..) {
        match cmd {
            SenderCmd::ReportTick { rates, mtus, reply } => {
                let reports = super::tasks::report::report_tick_tx(sched, stats, &rates, &mtus);
                let _ = reply.send(reports);
            }
            SenderCmd::AddPath { path, done } => {
                sched.add_path(path);
                let _ = done.send(());
            }
            SenderCmd::RemovePath(path) => {
                sched.remove_path(path);
                if let Some(ps) = stats.path_ref(path) {
                    ps.active.store(false, Relaxed);
                }
            }
            SenderCmd::FecFeedback { fed, useful } => {
                fec.feedback_update_window(fed, useful);
            }
            SenderCmd::Revive(_path) => {
                // The stamp itself is applied by `sync_rx` (run by the caller
                // after every drain); the command only ends the sender's wait.
            }
        }
    }
    sched.sync_rx(stats);
}
