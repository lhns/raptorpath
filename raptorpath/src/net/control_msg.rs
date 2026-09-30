//! Inbound control-message handling: the `ControlMessage` dispatch shared by
//! the receiver's ordered data loop and the control fast path.
//!
//! The caller builds one [`ControlCtx`] per call site; every non-trivial arm
//! is its own `on_*` function. Ordering constraints: the `WindowAck` arm runs
//! its whole re-homed Ack payload (delivery → RTT → loss/pool/stats/cc-window)
//! under one scheduler acquisition, released before `copa_feed_attribute`,
//! and the `Ack` arm `drop(sched)`s before touching the ARQ ledger. The
//! `Option` handles are capabilities: `None` means "this pipeline has no such
//! consumer". `ControlCtx` is taken by shared reference — all mutation goes
//! through the `Mutex`/`DashMap`/atomic handles it carries.
//!
//! Not covered here: the outbound control sends (the report task, the
//! receiver's ack/nack emitters), the Copa attribution machinery
//! (`net::copa_feed_attribute`) and the ARQ repair dispatchers
//! (`send_arq_repairs` / `dispatch_repair_plans`) — those are shared with
//! the send paths and stay at `net` module level, called from here.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tracing::{debug, info, warn};

use super::block_arq::BlockArq;
use super::{
    BatchCounter,
    COPA_SOLE_BYTES_PER_SYMBOL, CopaFeed, MAX_CONCURRENT_DECODERS, arq_loss_timeout,
    copa_feed_attribute, dispatch_repair_plans, now_us, sack_to_gaps, send_arq_repairs,
    worst_loss_rate,
};
use crate::control::FecRateController;
use crate::fec::{EncodingParams, FecBackend, FecDecoder};
use crate::monitor::stats::SharedStats;
use crate::scheduler::Scheduler;
use crate::transport::{ControlMessage, QuicTransport};

/// Everything an inbound control message may need, resolved once by the
/// caller. For the `Option` capability handles, `None` means "the running
/// pipeline has no such consumer".
pub(crate) struct ControlCtx<'a> {
    pub scheduler: &'a Arc<parking_lot::Mutex<Scheduler>>,
    pub fec_controller: &'a Arc<parking_lot::Mutex<FecRateController>>,
    pub decoders: &'a Arc<DashMap<u64, Box<dyn FecDecoder>>>,
    pub sent_counts: &'a Arc<DashMap<(u64, u32), u32>>,
    pub transport: &'a Arc<QuicTransport>,
    pub stats: &'a Arc<SharedStats>,
    /// The SACK→gap producer. The batch rides with its [`super::FireCause`]
    /// tag so the sender's `[FCAUSE]` gauge can say which receiver arm
    /// caused each fire. Label only — no arm branches on it.
    pub nack_tx: Option<&'a tokio::sync::mpsc::Sender<(super::FireCause, u32, Vec<(u64, u64)>)>>,
    /// Some(..) in block mode — Ack diffs drive repair sends.
    pub block_arq: Option<&'a Arc<parking_lot::Mutex<BlockArq>>>,
    pub batch_counter: Option<&'a Arc<BatchCounter>>,
    /// Some(..) in window mode: the peer's cumulative WindowAck point, read
    /// by the local window sender (ack-driven advance, retransmit-buffer and
    /// sent-store pruning). It must be the peer's ack, not the local
    /// receiver's delivery counter (a different seq space): the retention
    /// contract removes a symbol by ack only.
    pub peer_window_ack: Option<&'a Arc<AtomicU64>>,
    /// Some(..) in generation mode: forwards an inbound GenerationDeficit's
    /// (anchor, deficit) vector to the local window sender's recovery loop.
    pub deficit_tx: Option<&'a tokio::sync::mpsc::Sender<Vec<(u64, u32)>>>,
    /// The v8 receiver-seat repair request (paper §7.6), forwarded to the
    /// local window sender's own recovery loop — the parallel of
    /// [`Self::deficit_tx`] one line above: the receiver's report is the
    /// authority in both vocabularies, and the sender is a server of it.
    ///
    /// `Some(..)` only when `RWM_RECV_REQUEST_LAW` (or `RWM_RANK_FEEDBACK`) is
    /// armed on a plain reliable window; `None` on every shipped path, where
    /// an arriving `RepairRequest` is counted (`repair_request_ignored()`)
    /// and dropped, never acted on.
    pub request_tx: Option<&'a tokio::sync::mpsc::Sender<super::RepairRequestBatch>>,
    /// Some(..) in plain-reliable mode: forwards the WindowAck's received-above-
    /// frontier ranges, with the v9 `(next_expected, received_above)` pair, to
    /// the local window sender so its store gate can release out-of-order
    /// deliveries (SACK flow control). None disables it.
    pub sack_tx: Option<&'a tokio::sync::mpsc::Sender<super::SackReport>>,
    /// Some(..) in plain in-order window-reliable mode when
    /// the Copa delivery feed is enabled (RWM_QUIC_CC=passthrough or
    /// RWM_COPA_FEED=1). Each WindowAck's frontier/SACK diff is attributed
    /// per path into the send-interval rate sampler + the Copa cwnd dynamics
    /// (`copa_feed_attribute`), and the resulting per-path cwnd is written
    /// into the pass-through substrate window. None on the shipped path.
    pub copa_feed: Option<&'a Arc<CopaFeed>>,
    /// `RWM_MSTAR_ANCHOR` (ADR-0061): suppress the peer-report RTT
    /// pseudo-sample feed.
    pub mstar_anchor: bool,
}

pub(crate) fn handle_control_message(path_id: u32, msg: ControlMessage, ctx: &ControlCtx<'_>) {
    match msg {
        // BlockStart: the decoder uses the backend named in the message.
        ControlMessage::BlockStart {
            params,
            transfer_length,
            backend,
        } => on_block_start(ctx, params, transfer_length, backend),

        // ADR-0005 + ADR-0007: handle ACK with echo-based RTT
        ControlMessage::Ack {
            block_id: _,
            batch_seq,
            received_ids,
            echo_send_timestamp_us,
            expected_count,
            received_count,
        } => on_ack(
            ctx,
            path_id,
            batch_seq,
            &received_ids,
            echo_send_timestamp_us,
            expected_count,
            received_count,
        ),

        ControlMessage::BlockResult {
            block_id,
            success,
            symbols_received,
            symbols_needed,
        } => on_block_result(
            ctx,
            path_id,
            block_id,
            success,
            symbols_received,
            symbols_needed,
        ),

        ControlMessage::PathReport {
            path_id: report_path_id,
            loss_rate,
            avg_rtt_us,
            throughput_bps,
            jitter_us,
            symbols_sent: _,
            symbols_received: _,
        } => on_path_report(
            ctx,
            report_path_id,
            loss_rate,
            avg_rtt_us,
            throughput_bps,
            jitter_us,
        ),

        ControlMessage::Ping { timestamp_us } => {
            debug!(path_id, timestamp_us, "ping received");
            ctx.scheduler.lock().touch_path(path_id);
            let _ = ctx.transport.send_control_datagram(path_id, ControlMessage::Pong { echo_timestamp_us: timestamp_us });
        }

        // ADR-0015: handle graceful shutdown from peer
        ControlMessage::Shutdown => {
            info!(path_id, "peer is shutting down");
        }

        ControlMessage::WindowStart { symbol_size, backend, packed } => {
            debug!(path_id, symbol_size, ?backend, packed, "peer entered window mode");
        }

        ControlMessage::WindowAck { next_expected, sack_ranges, echo_send_timestamp_us, jitter_us, cumulative_received, cum_expected, cum_received, received_above } => on_window_ack(
            ctx,
            path_id,
            next_expected,
            received_above,
            sack_ranges,
            echo_send_timestamp_us,
            jitter_us,
            cumulative_received,
            cum_expected,
            cum_received,
        ),

        ControlMessage::GenerationDeficit { deficits } => {
            on_generation_deficit(ctx, path_id, deficits)
        }

        // v8: the receiver-seat repair request. Forwarded when a consumer is
        // armed; otherwise counted and dropped (see `on_repair_request`).
        ControlMessage::RepairRequest { spans, cause } => {
            on_repair_request(ctx, path_id, spans, cause)
        }

        // Never sent by this binary; the real guard (warn + ignore) lives in
        // the receiver loop in `net::mod`.
        ControlMessage::WindowSwitch { flush_seq, new_backend, symbol_size } => {
            debug!(path_id, flush_seq, ?new_backend, symbol_size, "window switch request (handled in receiver loop)");
        }

        _ => {}
    }
}

/// v8 `RepairRequest` arrivals this process has counted and dropped
/// because no consumer was armed.
static REPAIR_REQUEST_IGNORED: AtomicU64 = AtomicU64::new(0);

/// Read the ignored-`RepairRequest` count. Observation only.
pub fn repair_request_ignored() -> u64 {
    REPAIR_REQUEST_IGNORED.load(Ordering::Relaxed)
}

/// Handle BlockStart: the decoder uses the backend named in the message.
fn on_block_start(
    ctx: &ControlCtx<'_>,
    params: EncodingParams,
    transfer_length: u64,
    backend: FecBackend,
) {
    let decoders = ctx.decoders;
    // Evict oldest decoder if at capacity (DoS protection)
    if !decoders.contains_key(&params.block_id)
        && decoders.len() >= MAX_CONCURRENT_DECODERS
    {
        evict_oldest_decoder(decoders);
    }
    decoders
        .entry(params.block_id)
        .or_insert_with(|| backend.create_decoder(params, transfer_length));
    debug!(
        block_id = params.block_id,
        source_symbols = params.source_symbols,
        transfer_length,
        ?backend,
        "received BlockStart"
    );
}

/// ADR-0005 + ADR-0007: handle ACK with echo-based RTT
fn on_ack(
    ctx: &ControlCtx<'_>,
    path_id: u32,
    batch_seq: u64,
    received_ids: &[u32],
    echo_send_timestamp_us: u64,
    expected_count: u32,
    received_count: u32,
) {
    let (scheduler, transport, stats) = (ctx.scheduler, ctx.transport, ctx.stats);
    let (copa_feed, block_arq, batch_counter) = (ctx.copa_feed, ctx.block_arq, ctx.batch_counter);

    let mut sched = scheduler.lock();
    sched.touch_path(path_id);
    // `RWM_CLOCK_GAP` (ADR-0061): samples processed in a stall's
    // release-flood quarantine measured the stall, not the path — the
    // RTT/delivered-rate feeds below are skipped (budget release and loss
    // accounting are not: counts stay valid).
    let gap_q = crate::control::anchor::stall_witness()
        .is_some_and(|w| w.quarantined_now());
    // These per-batch Acks are sent by the receiver's data arm in window
    // mode too, so plain window mode drives `on_ack → record_delivery`
    // here — with the ack-interval Δt estimator, whose windowed max
    // over-reads (~×10) under ack bunching and pins cwnd/the plain store
    // cap via the anchor floor. When the plain-mode Copa feed is active it
    // owns delivery accounting + cwnd dynamics with clean send-interval
    // samples (WindowAck frontier/SACK attribution), so this arm must
    // release the wire-level in-flight budget without polluting the max
    // filter through `record_delivery`.
    // A paused N1-scoped feed must behave as absent here: otherwise this arm
    // suppresses the `record_delivery` anchor feed while the paused feed
    // supplies no samples either, and the anchor never establishes (dyn cap
    // stuck at boot 128).
    if let Some(feed) = copa_feed {
        if let Some(p) = sched.path_mut(path_id) {
            p.release_in_flight(received_ids.len() as u32);
            // `RWM_PLAIN_RS`: sampling-only mode keeps the per-batch
            // cwnd-dynamics call site/cadence (this Ack arm, exactly
            // `on_ack` minus the polluted
            // ack-interval `record_delivery` sample — the max filter
            // is fed only clean send-interval samples via the
            // WindowAck attribution). The full Copa-sole feed runs
            // its dynamics in `copa_feed_attribute` instead.
            if !feed.owns_cc() {
                p.on_delivery_signal();
            }
        }
    } else if gap_q {
        // Quarantined: release budget + run the cwnd dynamics at the
        // per-batch cadence, but do not feed the ack-interval rate
        // sample (`record_delivery`) — the flood's collapsed Δt
        // over-reads BtlBw. The first post-quarantine
        // sample spans the whole disturbance (large Δt ⇒ an average,
        // not a spike), so skipping is self-healing.
        if let Some(w) = crate::control::anchor::stall_witness() {
            w.note_discard();
        }
        if let Some(p) = sched.path_mut(path_id) {
            p.release_in_flight(received_ids.len() as u32);
            p.on_delivery_signal();
        }
    } else {
        sched.ack(path_id, received_ids.len() as u32);
    }
    if let Some(p) = sched.path(path_id) {
        debug!(
            path_id,
            acked = received_ids.len(),
            expected_count,
            in_flight = p.in_flight,
            cwnd = p.cwnd,
            "ack processed"
        );
    }

    // ADR-0007: RTT from echoed sender timestamp (same clock, no skew)
    let now = now_us();
    let rtt_us = now.saturating_sub(echo_send_timestamp_us);
    debug!(path_id, rtt_us, batch_seq, "ack rtt sample");
    if let Some(path) = sched.path_mut(path_id) {
        let rtt_duration = Duration::from_micros(rtt_us);
        // `RWM_CLOCK_GAP`: a quarantined echo measured the stall, not the
        // path — discard, don't average.
        if !gap_q {
            path.estimator.record_rtt(rtt_duration);
            // `RWM_COPA_WIRE` (ADR-0062): the CC delay term is wire-clocked
            // (quinn packet-timed RTT — excludes the sender's own store
            // dwell); the estimator above keeps the app-echo RTT for
            // the reliability/tail machinery. Gate off ⇒ app echo.
            let cc_rtt = if crate::scheduler::copa_wire_active() {
                transport.wire_rtt(path_id).unwrap_or(rtt_duration)
            } else {
                rtt_duration
            };
            path.record_rtt_sample(cc_rtt);
        }
        // Wire-level loss evidence for the competitive AIMD (block-mode Ack
        // arm; the WindowAck feed path has its own call). No-op unless
        // RWM_COPA_COMPETE.
        if crate::scheduler::copa_compete_active() {
            if let Some((ev, _, _)) = transport.cc_passthrough_stats(path_id) {
                path.on_wire_congestion_events(ev);
            }
        }

        // ADR-0003: update loss stats from ACK.
        //
        // `RWM_LOSS_SENT_TRUTH` (default off): the wire's `expected_count` is
        // `PathBatchTracker`'s gap estimate. Through wire v8 it read gaps in
        // the global batch_seq, so at N >= 2 the other path's symbols counted
        // as this path's loss; v9 keys it on the per-path `path_seq`, which
        // removes that contamination in the default arm. Under the gate
        // the estimator is fed the sender's own per-path `symbols_sent` delta
        // instead. The release below keeps the wire's `expected_count` in both
        // arms: the gate changes only what the estimator reads.
        let (le, lr) = if crate::scheduler::loss_sent_truth_active() {
            let sent = stats
                .path(path_id)
                .map(|ps| ps.symbols_sent.load(Ordering::Relaxed))
                .unwrap_or(0);
            path.sender_truth_loss_batch(sent, received_count)
        } else {
            (expected_count, received_count)
        };
        if le > 0 {
            path.estimator.record_batch(le, lr);
        }
        // Lost symbols also left the wire: release them from in_flight (the
        // delivery arm above only subtracts received), otherwise losses leak
        // budget and the Copa gate jams.
        //
        // `RWM_RELEASE_1TO1` (default off): `expected_count` is the same
        // gap estimate, here driving the ledger. Through wire v8 it read the
        // global `batch_seq` and over-released at N >= 2 (the in-flight gauge
        // leaked open); v9 reads the per-path `path_seq`, and the model now
        // pins the ledger closed
        // (`v9_counter_delta_release_closes_the_ledger_at_n2`).
        // Under the gate this term is deleted and the lost-symbol release is
        // `expire_in_flight`'s RFC 9002 sweep of the charge log, which is 1:1
        // with the charge by construction.
        if !crate::scheduler::release_1to1_active() && expected_count > 0 {
            path.release_in_flight(expected_count.saturating_sub(received_count));
        }

        // ADR-0013: update path monitoring stats
        if let Some(ps) = stats.path(path_id) {
            ps.rtt_us.store(rtt_us, Ordering::Relaxed);
            ps.loss_rate_e6.store((path.estimator.loss_rate() * 1_000_000.0) as u64, Ordering::Relaxed);
            ps.throughput_bps.store(path.estimator.throughput() as u64, Ordering::Relaxed);
            ps.cwnd.store(path.cwnd as u64, Ordering::Relaxed);
            ps.in_flight.store(path.in_flight as u64, Ordering::Relaxed);
            ps.in_slow_start.store(path.in_slow_start, Ordering::Relaxed);
            ps.symbols_received.fetch_add(received_ids.len() as u64, Ordering::Relaxed);
        }

        // Block mode already drives Copa via
        // `sched.ack` above — publish its cwnd as the pass-through
        // substrate window too (no-op unless RWM_QUIC_CC=passthrough).
        transport.set_cc_window_bytes(
            path_id,
            path.cwnd as u64 * COPA_SOLE_BYTES_PER_SYMBOL,
        );
    }

    // The Ack is P_lost evidence at probability ≈ 1 — diff the
    // batch ledger and repair immediately (one-RTT recovery). The
    // per-path SRTT feeds the timeout leg for older un-acked
    // batches on this path.
    let loss_timeout = sched
        .path(path_id)
        .map(|p| arq_loss_timeout(p.srtt()))
        .unwrap_or(Duration::from_millis(200));
    drop(sched);
    if let (Some(arq), Some(bc)) = (block_arq, batch_counter) {
        let events = arq.lock().on_ack(
            batch_seq,
            path_id,
            received_ids,
            Instant::now(),
            loss_timeout,
        );
        if !events.is_empty() {
            send_arq_repairs(events, arq, scheduler, transport, bc, stats);
        }
    }
}

fn on_block_result(
    ctx: &ControlCtx<'_>,
    path_id: u32,
    block_id: u64,
    success: bool,
    symbols_received: u32,
    symbols_needed: u32,
) {
    let (scheduler, transport, stats) = (ctx.scheduler, ctx.transport, ctx.stats);
    let (fec_controller, sent_counts) = (ctx.fec_controller, ctx.sent_counts);
    let (block_arq, batch_counter) = (ctx.block_arq, ctx.batch_counter);

    fec_controller.lock().feedback_update(success);

    // ADR-0013: update FEC monitoring stats
    {
        let diag = fec_controller.lock().diagnostics();
        stats.fec.actual_failure_rate_bits.store(diag.actual_failure_rate.to_bits(), Ordering::Relaxed);
        stats.fec.pi_correction_e3.store((diag.pi_correction * 1000.0) as i64, Ordering::Relaxed);
    }
    if !success {
        stats.blocks.decoded_fail.fetch_add(1, Ordering::Relaxed);
    }

    // Signal congestion control on block result.
    // If block failed (not enough symbols), that's a congestion signal
    // If block succeeded despite loss, FEC handled it (random loss)
    let had_loss = symbols_received < symbols_needed + (symbols_needed / 5); // rough: needed some repair
    if had_loss || !success {
        let mut sched = scheduler.lock();
        // Signal loss to all paths that sent symbols for this block
        let path_ids: Vec<u32> = sent_counts
            .iter()
            .filter(|entry| entry.key().0 == block_id)
            .map(|entry| entry.key().1)
            .collect();
        for pid in path_ids {
            sched.on_loss(pid, success); // fec_recovered = success
        }
    }

    debug!(
        block_id,
        success,
        symbols_received,
        symbols_needed,
        "block result from peer"
    );

    // Block decoded → drop retained data and suppress pending
    // loss events; block failed → one more repair round with
    // doubled margin (rateless backends only — see block_arq).
    if let Some(arq) = block_arq {
        if success {
            arq.lock().on_block_done(block_id);
        } else if let Some(bc) = batch_counter {
            let deficit = symbols_needed.saturating_sub(symbols_received);
            let eps_hat = worst_loss_rate(scheduler);
            let plan = arq.lock().on_block_failed(block_id, deficit, path_id, eps_hat);
            if let Some(plan) = plan {
                dispatch_repair_plans(
                    vec![plan],
                    arq,
                    scheduler,
                    transport,
                    bc,
                    stats,
                );
            }
        }
    }

    // Clean up sent_counts for this block
    sent_counts.retain(|(bid, _), _| *bid != block_id);
}

fn on_path_report(
    ctx: &ControlCtx<'_>,
    report_path_id: u32,
    loss_rate: f64,
    avg_rtt_us: u64,
    throughput_bps: f64,
    jitter_us: u64,
) {
    let (scheduler, transport, stats) = (ctx.scheduler, ctx.transport, ctx.stats);
    let mstar_anchor = ctx.mstar_anchor;

    let mut sched = scheduler.lock();
    // Touch path — this doubles as keepalive
    sched.touch_path(report_path_id);
    if let Some(path) = sched.path_mut(report_path_id) {
        let rtt_duration = Duration::from_micros(avg_rtt_us);
        // `RWM_MSTAR_ANCHOR` (ADR-0061): the peer's `avg_rtt_us` is the
        // peer's estimator value (its own EWMA — seeded at the 50-ms
        // DEFAULT_SRTT class and, on a pure receiver, never fed by a
        // measurement), not an RTT measurement. Recording it as a sample
        // every ~2 s would plant a perpetual 50-ms "sample" in the 10-s
        // min-RTT floor window. Under the gate the local RTT estimators are
        // fed only by locally measured echo samples (Ack/WindowAck arms);
        // the report keeps its keepalive/monitoring/loss roles, and floors
        // expire with their min-window. (`RWM_CLOCK_GAP`: reports processed
        // in a stall quarantine are skipped too.)
        let gap_q = crate::control::anchor::stall_witness()
            .is_some_and(|w| w.quarantined_now());
        if !mstar_anchor && !gap_q {
            path.estimator.record_rtt(rtt_duration);
            // Wire-clocked CC delay term (see the Ack arm above).
            let cc_rtt = if crate::scheduler::copa_wire_active() {
                transport.wire_rtt(report_path_id).unwrap_or(rtt_duration)
            } else {
                rtt_duration
            };
            path.record_rtt_sample(cc_rtt);
        }
        // Do not feed the peer's reported throughput into the
        // estimator. The field carries the peer's own send rate,
        // which for an
        // asymmetric workload (bulk up, ACK trickle down) would
        // drag this side's t_sym estimate toward the reverse
        // direction's rate. Local send-rate measurement in the
        // report task is the sole throughput feed.
        let _ = throughput_bps;
        // Record peer's reported loss for cross-validation
        if loss_rate > 0.0 {
            let approx_sent = 100u32;
            let approx_received = ((1.0 - loss_rate) * approx_sent as f64) as u32;
            path.estimator.record_batch(approx_sent, approx_received);
        }
    }
    // Update monitoring stats with peer's jitter
    if let Some(ps) = stats.path(report_path_id) {
        ps.rtt_us.store(avg_rtt_us, Ordering::Relaxed);
        ps.jitter_us.store(jitter_us, Ordering::Relaxed);
    }
}

#[allow(clippy::too_many_arguments)]
fn on_window_ack(
    ctx: &ControlCtx<'_>,
    path_id: u32,
    next_expected: u64,
    received_above: u32,
    sack_ranges: Vec<(u64, u64)>,
    echo_send_timestamp_us: u64,
    jitter_us: u32,
    cumulative_received: u64,
    cum_expected: u64,
    cum_received: u64,
) {
    let (scheduler, transport, stats) = (ctx.scheduler, ctx.transport, ctx.stats);
    let (copa_feed, peer_window_ack) = (ctx.copa_feed, ctx.peer_window_ack);
    let (nack_tx, sack_tx) = (ctx.nack_tx, ctx.sack_tx);

    debug!(path_id, next_expected, received_above, sack_count = sack_ranges.len(), cumulative_received, "SACK window ACK received");
    // Publish the peer's cumulative ack point for the window sender
    // (fetch_max: acks arrive on multiple paths, out of order). v9: the
    // value is the COUNT of the delivered prefix, so `0` is "nothing
    // delivered" and the sender prunes everything `< next_expected`.
    if let Some(pa) = peer_window_ack {
        pa.fetch_max(next_expected, Ordering::Relaxed);
    }
    // (`cumulative_received` — the peer's total decoded count — is used
    // only by the debug trace above.)
    // Update RTT from echoed timestamp. echo == 0 is the sentinel
    // for timer-driven acks (hold-expiry unwedge) that echo no
    // batch — recording now−0 would poison SRTT with a huge sample.
    // `RWM_ACK_MERGE` (default on): in window mode the per-batch `Ack` is
    // suppressed, so every consumer of its arm is re-homed here, driven by
    // the diff of the v6 cumulative counters. The whole arm runs under one
    // scheduler lock in the Ack arm's own order (delivery → RTT →
    // loss/pool/stats/cc-window): one acquisition where the unmerged pair
    // took two.
    let am_on = crate::scheduler::ack_merge_active();
    let now = now_us();
    let rtt_us = now.saturating_sub(echo_send_timestamp_us);
    {
        let mut sched = scheduler.lock();
        sched.touch_path(path_id);
        // `RWM_CLOCK_GAP`: quarantined echoes (stall release flood)
        // measured the stall — discard.
        let gap_q = crate::control::anchor::stall_witness()
            .is_some_and(|w| w.quarantined_now());
        if gap_q {
            if let Some(w) = crate::control::anchor::stall_witness() {
                w.note_discard();
            }
        }

        // ── Re-homing part 1: the delivery signal ────────────────
        // Mirrors the Ack arm's three-way branch (feed-present /
        // quarantined / neither). `record_delivery` (inside `sched.ack`)
        // is kept on purpose: with no feed constructed (the shipped
        // default) it is the only window-mode rate anchor, and without
        // it max_bw = 0 ⇒ the anchor floor never establishes ⇒ the
        // dynamic store cap sticks at boot 128. The merged ack arrives
        // on the cadence the Ack did, so its ack-interval Δt statistic
        // is unperturbed. `note_discard` is not repeated here — the
        // quarantine block above already charged it once for this ack.
        let (d_expected, d_received) = if am_on {
            sched
                .path_mut(path_id)
                .map(|p| p.ack_merge_counter_delta(cum_expected, cum_received))
                .unwrap_or((0, 0))
        } else {
            (0, 0)
        };
        // Ack-cadence gauge feed (`RWM_ACKDIAG`, net/ackdiag.rs). Every
        // WindowAck arrival is noted, including the (0, 0) sentinel/stale
        // class below: that class is the zero-delta fraction, and skipping
        // it would report a cadence the sender does not have. Absent with
        // the gate off (a null check — no clock read, no lock, no
        // allocation), and the gauge owns all of its state.
        //
        // Under `RWM_ACK_MERGE=0` the (expected, received) payload rides the
        // per-batch `Ack`, so `ack_merge_counter_delta` is never called and
        // the delta readouts are structurally zero. The spacing readout is
        // valid in both arms.
        if let Some(g) = crate::net::ackdiag::gauge() {
            g.note_ack(path_id, d_expected, d_received);
        }
        let am_live = d_expected > 0 || d_received > 0;
        if am_live {
            if let Some(feed) = copa_feed {
                if let Some(p) = sched.path_mut(path_id) {
                    p.release_in_flight(d_received);
                    if !feed.owns_cc() {
                        p.on_delivery_signal();
                    }
                }
            } else if gap_q {
                if let Some(p) = sched.path_mut(path_id) {
                    p.release_in_flight(d_received);
                    p.on_delivery_signal();
                }
            } else {
                sched.ack(path_id, d_received);
            }
        }

        if echo_send_timestamp_us > 0 && !gap_q {
            if let Some(path) = sched.path_mut(path_id) {
                let rtt_duration = Duration::from_micros(rtt_us);
                path.estimator.record_rtt(rtt_duration);
                // Wire-clocked CC delay term: the app-echo RTT reads the
                // sender's own reservoir dwell as network queue. The
                // estimator keeps the app echo (end-to-end tail
                // machinery); Copa gets the packet-timed RTT.
                let cc_rtt = if crate::scheduler::copa_wire_active() {
                    transport.wire_rtt(path_id).unwrap_or(rtt_duration)
                } else {
                    rtt_duration
                };
                path.record_rtt_sample(cc_rtt);
            }
            // ── `[ETA]`: the prediction error ──────────────────────────
            // `e = rtt_us - (eta_rel + RTprop/2)`, keyed by the echo, which
            // is the batch's own `send_timestamp_us`. A no-op unless this
            // echo keys a stamped placement (`net/eta.rs`), so repairs and
            // the tail sweep — which carry the 0 sentinel — contribute
            // nothing. Read-only: `on_ack` touches only the gauge.
            //
            // Placed after `record_rtt_sample` so `min_rtt()` is this ack's
            // own RTprop and not the previous one's.
            let rtprop_us = sched
                .path(path_id)
                .and_then(|p| p.min_rtt())
                .map(|d| d.as_micros() as u64)
                .unwrap_or(0);
            sched.eta_mut().on_ack(path_id, echo_send_timestamp_us, rtt_us, rtprop_us);
        }

        // ── Re-homing part 2: loss, pool, stats, cc window ───────
        // The remainder of the Ack arm, in its own order and with its
        // own guards. The loss feed is the sender's only loss signal
        // and the counter diff is what makes it survive the merge
        // exactly (sums, not events).
        if am_live {
            if let Some(path) = sched.path_mut(path_id) {
                // Wire-level loss evidence for the competitive AIMD, only
                // when no live feed exists — `copa_feed_attribute` below
                // carries its own call, so the event is exactly-once in
                // both configurations.
                if crate::scheduler::copa_compete_active()
                    && copa_feed.is_none()
                {
                    if let Some((ev, _, _)) = transport.cc_passthrough_stats(path_id) {
                        path.on_wire_congestion_events(ev);
                    }
                }
                // ADR-0003: loss stats from the ack's counter delta.
                //
                // `RWM_LOSS_SENT_TRUTH` (default off). `d_expected` is the
                // diff of `cum_expected` = `PathBatchTracker::total_expected`,
                // summed from `gap x received`. Through wire v8 the gap was
                // read in the global `batch_seq` and carried the cross-path
                // inflation; v9 reads it in the per-path `path_seq`, so the
                // default pair is per-path honest. The gate's pair is the
                // sender's own `symbols_sent` against the receiver's
                // `cum_received`. Release keeps `d_expected` in both arms.
                let (le, lr) = if crate::scheduler::loss_sent_truth_active() {
                    let sent = stats
                        .path(path_id)
                        .map(|ps| ps.symbols_sent.load(Ordering::Relaxed))
                        .unwrap_or(0);
                    path.sender_truth_loss_delta(sent, cum_received)
                } else {
                    (d_expected, d_received)
                };
                if le > 0 {
                    path.estimator.record_batch(le, lr);
                }
                // Lost symbols also left the wire: release them from in_flight
                // (the delivery branch above only subtracts received),
                // otherwise losses leak budget and the Copa gate jams.
                //
                // `RWM_RELEASE_1TO1` (default off): the same contaminated
                // `d_expected` leaks the budget gauge here as in the Ack arm.
                // Under the gate the term is deleted and `expire_in_flight`'s
                // RFC 9002 sweep of the charge log is the whole lost-symbol
                // release.
                if !crate::scheduler::release_1to1_active() && d_expected > 0 {
                    path.release_in_flight(d_expected.saturating_sub(d_received));
                }
                // ADR-0013: path monitoring stats.
                if let Some(ps) = stats.path(path_id) {
                    ps.loss_rate_e6.store(
                        (path.estimator.loss_rate() * 1_000_000.0) as u64,
                        Ordering::Relaxed,
                    );
                    ps.throughput_bps
                        .store(path.estimator.throughput() as u64, Ordering::Relaxed);
                    ps.cwnd.store(path.cwnd as u64, Ordering::Relaxed);
                    ps.in_flight.store(path.in_flight as u64, Ordering::Relaxed);
                    ps.in_slow_start.store(path.in_slow_start, Ordering::Relaxed);
                    ps.symbols_received
                        .fetch_add(d_received as u64, Ordering::Relaxed);
                }
                // Publish the cwnd as the
                // pass-through substrate window (no-op unless
                // RWM_QUIC_CC=passthrough).
                transport.set_cc_window_bytes(
                    path_id,
                    path.cwnd as u64 * COPA_SOLE_BYTES_PER_SYMBOL,
                );
            }
        }
    }
    // Plain-mode Copa delivery feed. Diff this
    // ack's cumulative frontier + SACK ranges against the attribution
    // cursor and drive the per-path Copa machinery (send-interval
    // rate samples, in-flight release, cwnd dynamics, pass-through
    // window write). After the RTT recording above so the cwnd
    // update sees the freshest queue signal.
    if let Some(feed) = copa_feed {
        copa_feed_attribute(
            feed,
            path_id,
            next_expected,
            &sack_ranges,
            scheduler,
            transport,
            stats,
        );
    }
    // Update monitoring stats
    if echo_send_timestamp_us > 0 {
        if let Some(ps) = stats.path(path_id) {
            ps.rtt_us.store(rtt_us, Ordering::Relaxed);
            ps.jitter_us.store(jitter_us as u64, Ordering::Relaxed);
        }
    }
    // The sender reads window_ack_seq via AtomicU64 in the sender loop.
    // SACK ranges drive reactive repair. Sacked-but-undelivered seqs
    // imply the seqs between them are missing at the receiver — invert
    // the ranges into gaps and feed the window sender's NACK repair
    // machinery (exact source retransmission, NACK budgets, per-seq
    // cooldown). This is window mode's only reactive-repair producer
    // (WindowNack is never sent).
    if !sack_ranges.is_empty() {
        // SACK flow control: the received ranges themselves let the
        // plain-reliable sender prune its sent-store for out-of-order
        // deliveries, so its flow-control window tracks true
        // outstanding rather than freezing on the
        // in-order cumulative frontier. Forward before inverting to
        // gaps (which drive the orthogonal targeted-retransmit path).
        //
        // v9: the report also carries `(next_expected, received_above)` --
        // the store gate's convergence evidence past the SACK prefix cap.
        // It rides the SACK-bearing acks only (the channel's existing
        // cadence: `GAP_ACK_MIN_INTERVAL` while the frontier is stalled, plus
        // the timer re-advertisement), so the depth-16 channel sees no new
        // pressure; a merge-only ack's count is superseded by the next
        // SACK-bearing one.
        if let Some(tx) = sack_tx {
            let _ = tx.try_send(super::SackReport {
                next_expected,
                received_above,
                ranges: sack_ranges.clone(),
            });
        }
        let gaps = sack_to_gaps(next_expected, &sack_ranges);
        if !gaps.is_empty() {
            debug!(path_id, gap_count = gaps.len(), first_gap = ?gaps.first(), "SACK gaps → NACK repair");
            if let Some(tx) = nack_tx {
                // `[FCAUSE]`: tag which receiver arm sent
                // this gap report, read off a field already in scope. The
                // receiver's timer-driven hole re-advertisement broadcasts one
                // message to every live path and so cannot carry a per-path
                // echo — it stamps `echo_send_timestamp_us: 0`, the same "no
                // counter payload" sentinel this handler already branches on
                // for its RTT update above. The data arm carries the real
                // batch timestamp. Classification only: both tags take the
                // identical path from here on.
                let cause = if echo_send_timestamp_us == 0 {
                    super::FireCause::GapRefresh
                } else {
                    super::FireCause::GapData
                };
                // `path_id` — the path this ack arrived on — rides with
                // the batch so the sender can say whether a hole was
                // resolved by a report from the path its original flew
                // on or from another. A label, like `cause`.
                let _ = tx.try_send((cause, path_id, gaps));
            }
        }
    }
}

/// The v8 receiver-seat repair request (paper §7.6).
///
/// With no consumer armed it is counted and dropped, never acted on and
/// never panicked on, so a peer that speaks the arm to a binary that does
/// not is read off `repair_request_ignored()` rather than inferred from a
/// silence. With the arm armed it is forwarded to the local window sender
/// exactly as a `GenerationDeficit` is — one
/// channel, best-effort, and a dropped report is re-sent by the receiver on
/// its next cadence.
fn on_repair_request(
    ctx: &ControlCtx<'_>,
    path_id: u32,
    spans: Vec<(u64, u16, u32)>,
    cause: u8,
) {
    debug!(
        path_id,
        spans = spans.len(),
        first = ?spans.first(),
        cause,
        "receiver-seat repair request"
    );
    match ctx.request_tx {
        // Best-effort, the `deficit_tx` contract exactly: the receiver
        // re-reports on its own cadence and the sender's want bookkeeping
        // self-corrects against the in-flight baseline.
        Some(tx) => {
            let _ = tx.try_send((cause, spans));
        }
        // No consumer: the arm is absent (or this is a generation / block
        // seat, where the span vocabulary has no server). Count it so the
        // absence is a reading.
        None => {
            REPAIR_REQUEST_IGNORED.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn on_generation_deficit(ctx: &ControlCtx<'_>, path_id: u32, deficits: Vec<(u64, u32)>) {
    debug!(
        path_id,
        gen_count = deficits.len(),
        first = ?deficits.first(),
        "generation deficit feedback received"
    );
    // Forward to the local window sender's recovery loop (generation
    // mode only). Best-effort: a dropped report is re-sent by the
    // receiver next SRTT, and the in-flight accounting self-corrects.
    if let Some(tx) = ctx.deficit_tx {
        let _ = tx.try_send(deficits);
    }
}

/// Evict the oldest incomplete decoder from the map. Used to enforce
/// `MAX_CONCURRENT_DECODERS` and prevent OOM from a peer flooding block_ids.
fn evict_oldest_decoder(decoders: &DashMap<u64, Box<dyn FecDecoder>>) {
    let oldest = decoders
        .iter()
        .filter(|entry| !entry.value().is_decoded())
        .min_by_key(|entry| entry.value().created_at())
        .map(|entry| *entry.key());

    if let Some(block_id) = oldest {
        decoders.remove(&block_id);
        warn!(block_id, "evicted oldest decoder (concurrent decoder limit reached)");
    }
}
