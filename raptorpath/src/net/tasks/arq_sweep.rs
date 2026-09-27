//! Block-mode ARQ sweeper: timeout-driven repair for batches no later
//! Ack will ever reveal, plus the send-idle BlockStart re-announce.
//!
//! Shape: window mode parks until shutdown (see the comment in place); the
//! tick is 25 ms; the `timeouts` map is built under one scheduler guard
//! that is dropped before `sweep`; the two dispatch sites run in order
//! (`send_arq_repairs` then `dispatch_repair_plans`).

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::super::block_arq::BlockArq;
use super::super::{
    REANNOUNCE_TIMEOUT_MAX, arq_loss_timeout, dispatch_repair_plans, send_arq_repairs,
    worst_loss_rate,
};
use crate::monitor::stats::SharedStats;
use crate::scheduler::Scheduler;
use crate::transport::QuicTransport;
use std::sync::atomic::AtomicU64;

/// Block-mode ARQ sweeper: the Ack-diff path needs later acks on
/// the same path to reveal a lost batch; the tail of a transfer has
/// none, so a timeout sweep declares those batches delivered-or-lost at
/// SRTT timescale (mirrors the in_flight expiry) and repairs them.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_arq_sweep(
    sweep_block_arq: Arc<parking_lot::Mutex<BlockArq>>,
    sweep_scheduler: Arc<parking_lot::Mutex<Scheduler>>,
    sweep_transport: Arc<QuicTransport>,
    sweep_stats: Arc<SharedStats>,
    sweep_batch_counter: Arc<AtomicU64>,
    sweep_window_mode: bool,
    mut sweep_shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) {
    if sweep_window_mode {
        // Window mode has its own SACK/NACK repair machinery — there is
        // no block-ARQ ledger to sweep. Park until shutdown instead of
        // returning: main()'s select! treats any task completing as
        // tunnel shutdown, so an instant return here would tear the tunnel
        // down right after startup.
        let _ = sweep_shutdown_rx.recv().await;
        return;
    }
    let mut interval = tokio::time::interval(Duration::from_millis(25));
    loop {
        tokio::select! {
            _ = interval.tick() => {}
            _ = sweep_shutdown_rx.recv() => break,
        }
        let timeouts: std::collections::HashMap<u32, Duration> = {
            let sched = sweep_scheduler.lock();
            sched
                .all_path_ids()
                .into_iter()
                .filter_map(|pid| sched.path(pid).map(|p| (pid, arq_loss_timeout(p.srtt()))))
                .collect()
        };
        let events = sweep_block_arq.lock().sweep(Instant::now(), &|pid| {
            timeouts
                .get(&pid)
                .copied()
                .unwrap_or(Duration::from_millis(200))
        });
        if !events.is_empty() {
            send_arq_repairs(
                events,
                &sweep_block_arq,
                &sweep_scheduler,
                &sweep_transport,
                &sweep_batch_counter,
                &sweep_stats,
            );
        }

        // Idle re-announce (send-idle recovery): a lost BlockStart
        // orphans a block whose symbols were all delivered-and-acked — the
        // ledger is empty, so `sweep` above sees nothing, yet the block
        // never decodes. Re-send BlockStart + a small spare for any block
        // still retained (un-decoded) and quiet past the loss timeout. The
        // re-announce is driven by this timer (not TUN reads), so it fires
        // while the sender is idle awaiting the app-level ack.
        let default_path = {
            let sched = sweep_scheduler.lock();
            sched.best_repair_path_avoiding(u32::MAX).unwrap_or(0)
        };
        let eps_hat = worst_loss_rate(&sweep_scheduler);
        let reann = sweep_block_arq.lock().idle_reannounce(
            Instant::now(),
            &|pid| {
                timeouts
                    .get(&pid)
                    .copied()
                    .unwrap_or(Duration::from_millis(200))
                    .min(REANNOUNCE_TIMEOUT_MAX)
            },
            default_path,
            eps_hat,
        );
        if !reann.is_empty() {
            dispatch_repair_plans(
                reann,
                &sweep_block_arq,
                &sweep_scheduler,
                &sweep_transport,
                &sweep_batch_counter,
                &sweep_stats,
            );
        }
    }
}
