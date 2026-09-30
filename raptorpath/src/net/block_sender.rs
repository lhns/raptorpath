//! The block-mode sender loop: TUN → framing → block assembly → FEC encode
//! → interleaver → paced batch sends. The sibling of `block_arq`, and the
//! other half of the sender task from `run_window_sender`; the two halves
//! share no local state.
//!
//! Loop shape: the interleaver is tapered iff depth ≥ 2; each iteration
//! takes one Copa backpressure sample under one scheduler guard dropped
//! before the `select!`; the `select!` arms are gated by `tx_paused`; the
//! shutdown flush is frame_end → encode → forced drain → Shutdown control
//! datagram on every active path → break. `tun` is moved in by value and
//! dropped when this future completes.
//!
//! Not covered here: the window-mode sender (`run_window_sender`, in
//! `net::mod`), the block-ARQ ledger and sweeper (`net::block_arq`,
//! `net::tasks::arq_sweep`), and the encode/drain helpers themselves
//! (`encode_to_interleave_buf`, `send_interleaved_batches`) — those are
//! shared with the ARQ repair paths and stay at `net` module level.

use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use dashmap::DashMap;
use tracing::{debug, info};

use super::{
    BatchCounter,
    PaceCarry, encode_to_interleave_buf, framing, interleave, send_interleaved_batches,
};
use crate::control::FecRateController;
use crate::fec::FecBackend;
use crate::monitor::stats::SharedStats;
use crate::net::block_arq::BlockArq;
use crate::scheduler::Scheduler;
use crate::transport::{ControlMessage, QuicTransport};
use crate::tun::TunInterface;

/// The block-mode sender loop (the `else` half of the sender task).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_block_sender(
    mut tun: TunInterface,
    sender_transport: Arc<QuicTransport>,
    sender_scheduler: Arc<parking_lot::Mutex<Scheduler>>,
    sender_fec: Arc<parking_lot::Mutex<FecRateController>>,
    sender_block_counter: Arc<AtomicU64>,
    sender_batch_counter: Arc<BatchCounter>,
    sender_sent_counts: Arc<DashMap<(u64, u32), u32>>,
    sender_stats: Arc<SharedStats>,
    sender_block_arq: Arc<parking_lot::Mutex<BlockArq>>,
    sender_profile_max_block: usize,
    sender_profile_flush: std::time::Duration,
    sender_profile_symbol_size: u16,
    sender_fec_backend: FecBackend,
    sender_interleave_depth: u32,
    sender_interleave_timeout: std::time::Duration,
    mut sender_shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) {
    // ----- Block-mode sender -----
    let mut block_buf = Vec::with_capacity(sender_profile_max_block);
    let mut last_tx_paused = false;
    let mut flush_deadline: Option<tokio::time::Instant> = None;
    // Pacing retry: set when the token bucket left symbols in the
    // carry; the select loop resumes the paced drain when it fires.
    let mut pace_deadline: Option<tokio::time::Instant> = None;
    // Symbol-level pacing carry: drained-but-not-yet-sendable symbols
    // wait here between pace ticks (the interleaver drain is
    // all-or-nothing, so partial sends need their own queue).
    let mut pace_carry: PaceCarry = PaceCarry::new();
    let mut shutting_down = false;
    // Block-pipeline flow control (see `feed_gate`): parked on this while
    // the retention window is full of unconfirmed blocks; `on_block_done`
    // wakes it.
    let room = sender_block_arq.lock().room_notify();
    let mut last_retention_full = false;
    let mut ileave = if sender_interleave_depth >= 2 {
        interleave::InterleavingBuffer::new_tapered(
            sender_interleave_depth as usize,
            sender_interleave_timeout,
        )
    } else {
        interleave::InterleavingBuffer::new(
            sender_interleave_depth as usize,
            sender_interleave_timeout,
        )
    };

    loop {
        // Compute interleave drain deadline
        let ileave_deadline = ileave.oldest_deadline().map(|d| {
            // Convert std Instant to tokio Instant (offset from now)
            let std_now = std::time::Instant::now();
            let remaining = d.saturating_duration_since(std_now);
            tokio::time::Instant::now() + remaining
        });

        // Copa backpressure (paper §8.7): stop reading the TUN while the
        // wire budget is exhausted — the inner flow's own CC sees the
        // growing TUN queue and slows down. Without this the encoder runs
        // at TUN speed, saturates the runtime, starves QUIC
        // timers/liveness, and a bulk transfer kills the tunnel within
        // DEAD_PATH_TIMEOUT.
        let (tx_paused, dbg_fl, dbg_cw) = {
            let mut sched = sender_scheduler.lock();
            let mut fl = 0u64;
            let mut cw = 0u64;
            for id in sched.live_paths() {
                if let Some(p) = sched.path_mut(id) {
                    // Time-based budget release first: stranded charges
                    // (lost best-effort ACK datagrams) must reopen the
                    // gate at RTT timescale, not the 2s leak-guard
                    // cadence.
                    p.expire_in_flight();
                    fl += p.in_flight as u64;
                    cw += p.cwnd as u64;
                }
            }
            // in_flight is charged once at schedule time, so it already
            // covers interleaver + pacing carry + wire — the whole
            // committed pipeline.
            (fl >= cw.max(4), fl, cw)
        };
        if tx_paused != last_tx_paused {
            debug!(tx_paused, in_flight = dbg_fl, cwnd = dbg_cw, "backpressure state change");
            last_tx_paused = tx_paused;
        }
        let gate = feed_gate(tx_paused, &mut sender_block_arq.lock(), sender_profile_max_block);
        if gate.retention_full != last_retention_full {
            // Bind-fraction gauge of the flow-control clamp (loop
            // iterations that found the window full / all checks).
            let (checks, full) = sender_block_arq.lock().window_gauge();
            debug!(
                retention_full = gate.retention_full,
                window_checks = checks,
                window_full = full,
                "block retention window state change (flow control)"
            );
            last_retention_full = gate.retention_full;
        }

        // Select between packet arrival, flush timeout, interleave drain, and shutdown
        let packet = {
            let flush_sleep = async {
                match flush_deadline {
                    Some(d) => tokio::time::sleep_until(d).await,
                    None => std::future::pending().await,
                }
            };
            let ileave_sleep = async {
                match ileave_deadline {
                    Some(d) => tokio::time::sleep_until(d).await,
                    None => std::future::pending().await,
                }
            };
            let pace_sleep = async {
                match pace_deadline {
                    Some(d) => tokio::time::sleep_until(d).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_millis(1)), if tx_paused => {
                    continue;
                }
                // Retention window full: park until a confirmation frees
                // room (no poll — `on_block_done` stores a Notify permit).
                _ = room.notified(), if gate.retention_full => {
                    continue;
                }
                p = tun.read_packet(), if gate.read_tun => p,
                _ = flush_sleep, if gate.flush => None,
                _ = pace_sleep => {
                    // Pacing tokens should be available again — retry
                    // the blocked drain.
                    pace_deadline = send_interleaved_batches(
                        &mut ileave,
                        &mut pace_carry,
                        &sender_batch_counter,
                        &sender_transport,
                        &sender_scheduler,
                        &sender_stats,
                        &sender_block_arq,
                        false,
                    )
                    .map(|d| tokio::time::Instant::now() + d);
                    continue;
                }
                _ = ileave_sleep => {
                    // Interleave timeout — drain and send buffered symbols
                    if ileave.should_drain() || !ileave.is_empty() {
                        pace_deadline = send_interleaved_batches(
                            &mut ileave,
                            &mut pace_carry,
                            &sender_batch_counter,
                            &sender_transport,
                            &sender_scheduler,
                            &sender_stats,
                            &sender_block_arq,
                            false,
                        )
                        .map(|d| tokio::time::Instant::now() + d);
                    }
                    continue;
                }
                _ = sender_shutdown_rx.recv() => { shutting_down = true; None }
            }
        };

        // ADR-0015: flush partial block and notify peer on shutdown
        if shutting_down {
            if !block_buf.is_empty() {
                framing::frame_end(&mut block_buf);
                encode_to_interleave_buf(
                    &mut block_buf,
                    &sender_block_counter,
                    &sender_batch_counter,
                    &sender_scheduler,
                    &sender_fec,
                    &sender_transport,
                    &sender_sent_counts,
                    &sender_stats,
                    sender_profile_symbol_size,
                    sender_profile_max_block,
                    &mut ileave,
                    sender_fec_backend,
                    &sender_block_arq,
                );
            }
            // Force-drain all remaining interleaved symbols (bypasses
            // the pacing gate — shutdown flush must not strand data)
            send_interleaved_batches(
                &mut ileave,
                &mut pace_carry,
                &sender_batch_counter,
                &sender_transport,
                &sender_scheduler,
                &sender_stats,
                &sender_block_arq,
                true,
            );
            // Send Shutdown control message to peer on all paths
            {
                let sched = sender_scheduler.lock();
                for pid in super::control_broadcast_paths(&sched) {
                    let _ = sender_transport.send_control_datagram(
                        pid,
                        ControlMessage::Shutdown,
                    );
                }
            }
            info!("sender shut down gracefully");
            break;
        }

        match packet {
            Some(pkt) => {
                // ADR-0002: frame each packet with length prefix
                framing::frame_packet(&mut block_buf, &pkt);

                // Start flush timer on first packet in block
                if flush_deadline.is_none() {
                    flush_deadline =
                        Some(tokio::time::Instant::now() + sender_profile_flush);
                }

                // Flush if block is full
                if block_buf.len() >= sender_profile_max_block {
                    framing::frame_end(&mut block_buf);
                    encode_to_interleave_buf(
                        &mut block_buf,
                        &sender_block_counter,
                        &sender_batch_counter,
                        &sender_scheduler,
                        &sender_fec,
                        &sender_transport,
                        &sender_sent_counts,
                        &sender_stats,
                        sender_profile_symbol_size,
                        sender_profile_max_block,
                        &mut ileave,
                        sender_fec_backend,
                        &sender_block_arq,
                    );
                    flush_deadline = None;
                    // Check if interleave buffer is ready to drain
                    if ileave.should_drain() {
                        pace_deadline = send_interleaved_batches(
                            &mut ileave,
                            &mut pace_carry,
                            &sender_batch_counter,
                            &sender_transport,
                            &sender_scheduler,
                            &sender_stats,
                            &sender_block_arq,
                            false,
                        )
                        .map(|d| tokio::time::Instant::now() + d);
                    }
                }
            }
            None => {
                if flush_deadline.is_some() && !block_buf.is_empty() {
                    // Flush partial block on timeout
                    framing::frame_end(&mut block_buf);
                    encode_to_interleave_buf(
                        &mut block_buf,
                        &sender_block_counter,
                        &sender_batch_counter,
                        &sender_scheduler,
                        &sender_fec,
                        &sender_transport,
                        &sender_sent_counts,
                        &sender_stats,
                        sender_profile_symbol_size,
                        sender_profile_max_block,
                        &mut ileave,
                        sender_fec_backend,
                        &sender_block_arq,
                    );
                    flush_deadline = None;
                    // Check if interleave buffer is ready to drain
                    if ileave.should_drain() {
                        pace_deadline = send_interleaved_batches(
                            &mut ileave,
                            &mut pace_carry,
                            &sender_batch_counter,
                            &sender_transport,
                            &sender_scheduler,
                            &sender_stats,
                            &sender_block_arq,
                            false,
                        )
                        .map(|d| tokio::time::Instant::now() + d);
                    }
                } else if flush_deadline.is_none() {
                    // TUN closed (read_packet returned None without timeout)
                    info!("TUN closed");
                    break;
                }
            }
        }
    }
}

/// What the block sender may do this iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FeedGate {
    /// Read the next TUN packet (grows the block under assembly).
    pub read_tun: bool,
    /// Let the flush timeout encode the partial block under assembly.
    pub flush: bool,
    /// The retention window is full of unconfirmed blocks.
    pub retention_full: bool,
}

/// The block sender's admission gate: ONE rule composing the two
/// back-pressure signals.
///
///   retention_full = ¬ arq.has_room(next_block_bytes)
///   read_tun       = ¬ copa_paused ∧ ¬ retention_full
///   flush          = ¬ retention_full
///
/// Copa's wire budget stops TUN reads only (a partial block may still be
/// flushed into the already-charged pipeline, as before). The retention
/// window stops everything that would ENCODE a new block — TUN reads (which
/// grow the block under assembly toward `max_block_size`) and the flush
/// timeout — because under the reliable contract an unconfirmed block is
/// never evicted: a full window must pause the producer instead. Draining
/// already-encoded symbols (interleaver / pacing carry), the ARQ repair and
/// the ack/control paths are untouched. Only the shutdown flush encodes
/// ungated (bounded overshoot: one block). `next_block_bytes` is the
/// profile's full block size, so every block admitted while the gate is
/// open fits (up to the one packet a block can overrun it by).
pub(crate) fn feed_gate(copa_paused: bool, arq: &mut BlockArq, next_block_bytes: usize) -> FeedGate {
    let retention_full = !arq.has_room(next_block_bytes);
    FeedGate {
        read_tun: !copa_paused && !retention_full,
        flush: !retention_full,
        retention_full,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fec::EncodingParams;
    use bytes::Bytes;
    use std::time::Instant;

    const AUTO_BLOCK: usize = 16 * 1024;

    fn admit(arq: &mut BlockArq, id: u64) {
        let p = EncodingParams {
            source_symbols: AUTO_BLOCK.div_ceil(1200) as u32,
            symbol_size: 1200,
            repair_count: 0,
            block_id: id,
        };
        arq.on_block_encoded(id, Bytes::from(vec![0u8; AUTO_BLOCK]), p, FecBackend::RaptorQ, Instant::now());
    }

    /// The feed loop pauses (no TUN read, no flush-encode) instead of
    /// evicting when the window is full of unconfirmed blocks, and resumes
    /// once the oldest is confirmed. Copa's gate composes (either closes it).
    #[test]
    fn sender_pauses_on_full_window_and_resumes_on_done() {
        let mut arq = BlockArq::new();
        let mut id = 0u64;
        // Drive the gate exactly as the loop does: admit while it is open.
        while feed_gate(false, &mut arq, AUTO_BLOCK).read_tun {
            admit(&mut arq, id);
            id += 1;
            assert!(id < 100_000, "the gate never closed: the sender would evict");
        }
        let g = feed_gate(false, &mut arq, AUTO_BLOCK);
        assert_eq!(
            g,
            FeedGate { read_tun: false, flush: false, retention_full: true },
            "full window: pause, never evict"
        );
        assert_eq!(arq.retained_stats().0, id as usize, "nothing evicted while paused");

        // Copa pause composes with an open window (and vice versa).
        arq.on_block_done(0);
        assert_eq!(
            feed_gate(false, &mut arq, AUTO_BLOCK),
            FeedGate { read_tun: true, flush: true, retention_full: false },
            "done of the oldest resumes the feed"
        );
        assert_eq!(
            feed_gate(true, &mut arq, AUTO_BLOCK),
            FeedGate { read_tun: false, flush: true, retention_full: false },
            "Copa pause alone stops TUN reads, not the flush"
        );
    }
}
