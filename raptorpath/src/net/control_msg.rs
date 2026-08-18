//! Inbound control-message handling: the `ControlMessage` dispatch shared by
//! the receiver's ordered data loop and the control fast path.
//!
//! History (net seam pass, 2026-08-08): `handle_control_message` took 17
//! parameters — most of them `Option<&…>` capability handles whose Some/None
//! pattern encodes which pipeline the engine is running — and its body was a
//! single 620-line `match` whose two largest arms (`Ack`, `WindowAck`) were
//! 170 and 210 lines. The parameter list is now a [`ControlCtx`] built once
//! at each call site, and every non-trivial arm is its own `on_*` function.
//!
//! Behavior contract: the arm bodies are VERBATIM. Nothing was reordered,
//! merged or re-guarded; in particular the `WindowAck` arm still runs its
//! whole re-homed Ack payload (delivery → RTT → loss/pool/stats/cc-window)
//! under ONE scheduler acquisition, released before `copa_feed_attribute`,
//! and the `Ack` arm still `drop(sched)`s before touching the ARQ ledger.
//! The `Option` handles keep their exact roles: `None` still means "this
//! pipeline has no such consumer", so the shipped configurations select the
//! same statements they did before. `ControlCtx` is taken by SHARED
//! reference — no arm mutates the context itself; all mutation goes through
//! the `Mutex`/`DashMap`/atomic handles it carries, exactly as before.
//!
//! NOT covered here: the outbound control sends (the report task, the
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
/// caller. The fields are exactly the former parameters of
/// `handle_control_message`, with their original types and their original
/// documented meaning — including the `Option` capability handles, where
/// `None` means "the running pipeline has no such consumer" and selects the
/// same statements it always did.
pub(crate) struct ControlCtx<'a> {
    pub scheduler: &'a Arc<parking_lot::Mutex<Scheduler>>,
    pub fec_controller: &'a Arc<parking_lot::Mutex<FecRateController>>,
    pub decoders: &'a Arc<DashMap<u64, Box<dyn FecDecoder>>>,
    pub sent_counts: &'a Arc<DashMap<(u64, u32), u32>>,
    pub transport: &'a Arc<QuicTransport>,
    /// DEAD as of this extraction and kept deliberately: the former
    /// `fec_backend` parameter is read by NO arm — `BlockStart` builds its
    /// decoder from the backend carried ON THE WIRE (ADR-0030), which is the
    /// whole point of that field, and mid-stream switching was removed
    /// (§16.4). Dropping it would be behaviour-neutral but is a signature
    /// change beyond this refactor's remit, so the seam keeps it verbatim.
    #[allow(dead_code)]
    pub fec_backend: FecBackend,
    pub stats: &'a Arc<SharedStats>,
    pub nack_tx: Option<&'a tokio::sync::mpsc::Sender<Vec<(u64, u64)>>>,
    /// P8: Some(..) in block mode — Ack diffs drive repair sends.
    pub block_arq: Option<&'a Arc<parking_lot::Mutex<BlockArq>>>,
    pub batch_counter: Option<&'a Arc<AtomicU64>>,
    /// Some(..) in window mode: the PEER's cumulative WindowAck point, read
    /// by the local window sender (ack-driven advance, retransmit-buffer and
    /// sent-store pruning). Historically this atomic was only ever written
    /// with the LOCAL receiver's inbound delivery counter — a different seq
    /// space entirely — so the sender's ack state was fed garbage; the RWM
    /// Phase A retention contract (removal by ack ONLY) needs the real ack.
    pub peer_window_ack: Option<&'a Arc<AtomicU64>>,
    /// Some(..) in generation mode: forwards an inbound GenerationDeficit's
    /// (anchor, deficit) vector to the local window sender's recovery loop.
    pub deficit_tx: Option<&'a tokio::sync::mpsc::Sender<Vec<(u64, u32)>>>,
    /// Some(..) in plain-reliable mode: forwards the WindowAck's RECEIVED-above-
    /// frontier ranges to the local window sender so it can prune the sent-store
    /// for out-of-order deliveries (SACK flow control). None disables it.
    pub sack_tx: Option<&'a tokio::sync::mpsc::Sender<Vec<(u64, u64)>>>,
    /// feat/copa-sole-cc: Some(..) in PLAIN in-order window-reliable mode when
    /// the Copa delivery feed is enabled (RWM_QUIC_CC=passthrough or
    /// RWM_COPA_FEED=1). Each WindowAck's frontier/SACK diff is attributed
    /// per path into the send-interval rate sampler + the Copa cwnd dynamics
    /// (`copa_feed_attribute`), and the resulting per-path cwnd is written
    /// into the pass-through substrate window. None = shipped path,
    /// byte-identical.
    pub copa_feed: Option<&'a Arc<CopaFeed>>,
    /// feat/anchor-hygiene (`RWM_MSTAR_ANCHOR`, resolved once in run_impl —
    /// src/gates.rs): suppress the peer-report RTT pseudo-sample feed.
    pub mstar_anchor: bool,
}

pub(crate) fn handle_control_message(path_id: u32, msg: ControlMessage, ctx: &ControlCtx<'_>) {
    match msg {
        // ADR-0008: handle BlockStart — use backend from message (ADR-0030)
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

        ControlMessage::WindowAck { received_up_to, sack_ranges, echo_send_timestamp_us, jitter_us, cumulative_received, cum_expected, cum_received } => on_window_ack(
            ctx,
            path_id,
            received_up_to,
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

        // ADR-0030: never sent by this binary; the real guard (warn + ignore)
        // lives in the receiver loop in `net::mod`.
        ControlMessage::WindowSwitch { flush_seq, new_backend, symbol_size } => {
            debug!(path_id, flush_seq, ?new_backend, symbol_size, "window switch request (handled in receiver loop)");
        }

        _ => {}
    }
}

/// ADR-0008: handle BlockStart — use backend from message (ADR-0030)
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
    // feat/anchor-hygiene (`RWM_CLOCK_GAP`): samples processed in a
    // stall's release-flood quarantine measured the stall, not the
    // path — the RTT/delivered-rate feeds below are skipped (budget
    // release and loss accounting are NOT: counts stay valid).
    let gap_q = crate::control::anchor::stall_witness()
        .is_some_and(|w| w.quarantined_now());
    // NOTE (feat/copa-sole-cc code-fact correction): these per-batch
    // Acks are sent by the receiver's data arm in WINDOW mode too
    // (the send site sits AFTER the window/block branch), so plain
    // window mode has ALWAYS driven `on_ack → record_delivery` here —
    // with the ack-interval Δt estimator, whose windowed max
    // over-reads ~×10 under ack bunching (MEASURED on the L0 shim:
    // btlbw 108k vs true ~10.4k sym/s) and pins cwnd/the plain store
    // cap via the anchor floor. When the plain-mode Copa feed is
    // active it owns delivery accounting + cwnd dynamics with clean
    // SEND-interval samples (WindowAck frontier/SACK attribution), so
    // this arm must release the wire-level in-flight budget WITHOUT
    // polluting the max filter through `record_delivery`.
    // feat/window-mtu scope fix: a PAUSED N1-scoped feed must behave
    // as ABSENT here too — otherwise this arm suppresses the legacy
    // `record_delivery` anchor feed while the paused feed supplies no
    // samples either, and the anchor never establishes (measured at
    // duals: btlbw=0/est=n on both paths, dyn cap stuck at boot 128).
    if let Some(feed) = copa_feed.filter(|f| !f.n1_paused()) {
        if let Some(p) = sched.path_mut(path_id) {
            p.release_in_flight(received_ids.len() as u32);
            // feat/anchor-hygiene (`RWM_PLAIN_RS`): sampling-only mode
            // keeps the LEGACY cwnd-dynamics call site/cadence (this
            // per-batch Ack arm, exactly `on_ack` minus the polluted
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
        // legacy cadence, but do NOT feed the ack-interval rate
        // sample (`record_delivery`) — the flood's collapsed Δt is
        // the measured ×13 BtlBw over-read. The first post-quarantine
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
        // feat/anchor-hygiene (`RWM_CLOCK_GAP`): a quarantined echo
        // measured the stall, not the path — discard, don't average.
        if !gap_q {
            path.estimator.record_rtt(rtt_duration);
            // feat/copa-wire-signal: the CC delay term is wire-clocked
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
        // feat/copa-compete: wire-level loss evidence for the
        // competitive AIMD (block-mode Ack arm; the WindowAck feed
        // path has its own call). No-op unless RWM_COPA_COMPETE.
        if crate::scheduler::copa_compete_active() {
            if let Some((ev, _, _)) = transport.cc_passthrough_stats(path_id) {
                path.on_wire_congestion_events(ev);
            }
        }

        // ADR-0003: update loss stats from ACK.
        //
        // fix/loss-crosspath (`RWM_LOSS_SENT_TRUTH`, default OFF): the wire's
        // `expected_count` is `PathBatchTracker`'s GLOBAL-batch_seq gap
        // estimate, which at N >= 2 reads the OTHER path's symbols as this
        // path's loss (measured 37-93x over realized — READOUT 4). Under the
        // gate the estimator is fed the sender's own per-path
        // `symbols_sent` delta instead. RELEASE below deliberately keeps the
        // wire's `expected_count` in BOTH arms: the gate changes what the
        // ESTIMATOR reads, nothing else (the coupled in_flight over-release
        // is a separate defect, named in the branch record).
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
        // fix/accounting-ledger (`RWM_RELEASE_1TO1`, default OFF — MECHANICAL
        // DEFECT SWEEP item 5, defects 2+3): `expected_count` is
        // `PathBatchTracker`'s GLOBAL-`batch_seq` gap estimate — the SAME
        // contaminated operand `RWM_LOSS_SENT_TRUTH` removed from the
        // estimator, here driving the LEDGER. At N >= 2 it over-releases ~1
        // slot per delivered symbol at c7 and ~5 on c8's slow leg, and
        // `release_in_flight` saturates at zero, so the excess is spent: the
        // gauge leaks OPEN (measured: `in_flight == 0` on > 90% of acks at
        // which the path is loaded — `legacy_counter_delta_release_leaks_the_
        // in_flight_gauge_open_at_n2`). Under the gate this term is DELETED
        // and the lost-symbol release is `expire_in_flight`'s RFC 9002 sweep
        // of the charge log, which is 1:1 with the charge by construction.
        if !crate::scheduler::release_1to1_active() && expected_count > 0 {
            path.release_in_flight(expected_count.saturating_sub(received_count));
        }

        // Delivery-clocked pool anchor (RWM_POOL_DELIV, goal-gate
        // "Ship The Wins 1b" arm A): THE delivery event for this
        // path's shadow rate sampler. Delivered advances the rate
        // numerator; LOST advances the accounted cursor only (a lost
        // symbol left the wire too — that alignment is what lets an
        // aggregate cursor resolve send spacing without a per-seq
        // key). Placed here so it sees exactly the counts the legacy
        // anchor sees: this build changes the Δt STATISTIC, not the
        // per-path attribution. `gap_q` drops a stall-poisoned event
        // exactly as the RTT/rate feeds above drop it. Feeds nothing
        // but the N ≥ 2 pool law; no-op with the gate off.
        path.on_pool_delivery(
            received_ids.len() as u32,
            expected_count.saturating_sub(received_count),
            gap_q,
        );

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

        // feat/copa-sole-cc: block mode already drives Copa via
        // `sched.ack` above — publish its cwnd as the pass-through
        // substrate window too (no-op unless RWM_QUIC_CC=passthrough).
        transport.set_cc_window_bytes(
            path_id,
            path.cwnd as u64 * COPA_SOLE_BYTES_PER_SYMBOL,
        );
    }

    // P8: the Ack is P_lost evidence at probability ≈ 1 — diff the
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

    // ADR-0009: signal congestion control on block result
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

    // P8: block decoded → drop retained data and suppress pending
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
        // feat/anchor-hygiene (`RWM_MSTAR_ANCHOR`), hygiene rules
        // 1+3: the peer's `avg_rtt_us` is the peer's ESTIMATOR VALUE
        // (its own EWMA — seeded at the 50-ms DEFAULT_SRTT class and,
        // on a pure receiver, never fed by a measurement), NOT an RTT
        // measurement. Recording it as a sample every ~2 s planted a
        // perpetual 50-ms "sample" in the 10-s min-RTT floor window —
        // the measured M* floor-freshness FAIL at the r200 knee cell
        // (goal-gate #61: rtp=50 ms at a 200-ms-RTprop cell, M*
        // pinned at the cold-start floor). Under the gate the local
        // RTT estimators are fed ONLY by locally measured echo
        // samples (Ack/WindowAck arms); the report keeps its
        // keepalive/monitoring/loss roles. Floors now EXPIRE with
        // their min-window as designed. (`RWM_CLOCK_GAP`: reports
        // processed in a stall quarantine are skipped too.)
        let gap_q = crate::control::anchor::stall_witness()
            .is_some_and(|w| w.quarantined_now());
        if !mstar_anchor && !gap_q {
            path.estimator.record_rtt(rtt_duration);
            // feat/copa-wire-signal: wire-clocked CC delay term (see
            // the Ack arm above).
            let cc_rtt = if crate::scheduler::copa_wire_active() {
                transport.wire_rtt(report_path_id).unwrap_or(rtt_duration)
            } else {
                rtt_duration
            };
            path.record_rtt_sample(cc_rtt);
        }
        // P10a: do NOT feed the peer's reported throughput into
        // the estimator. The field carries the PEER's estimator
        // value — historically 0.0 (circular feed, see the report
        // task), and now the peer's own SEND rate, which for an
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
    received_up_to: u64,
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

    debug!(path_id, received_up_to, sack_count = sack_ranges.len(), cumulative_received, "SACK window ACK received");
    // Publish the peer's cumulative ack point for the window sender
    // (fetch_max: acks arrive on multiple paths, out of order).
    if let Some(pa) = peer_window_ack {
        pa.fetch_max(received_up_to, Ordering::Relaxed);
    }
    // (`cumulative_received` — the peer's total decoded count `d`,
    // FMTCP-era "change 1" — stays on the wire for the debug trace
    // above; its sender-side consumer was removed with RWM_FMTCP.)
    // Update RTT from echoed timestamp. echo == 0 is the sentinel
    // for timer-driven acks (hold-expiry unwedge) that echo no
    // batch — recording now−0 would poison SRTT with a huge sample.
    // ack-merge (RWM_ACK_MERGE, goal-gate "Unlock The Default 1"):
    // in window mode the legacy per-batch `Ack` is suppressed, so
    // EVERY consumer of its arm is re-homed here, driven by the diff
    // of the v6 cumulative counters. The whole arm runs under ONE
    // scheduler lock in the legacy Ack arm's own internal order
    // (delivery → RTT → loss/pool/stats/cc-window) — one acquisition
    // where the unmerged pair took two, which is stall source (b) of
    // the pre-registration, removed rather than merely relocated.
    let am_on = crate::scheduler::ack_merge_active();
    let now = now_us();
    let rtt_us = now.saturating_sub(echo_send_timestamp_us);
    {
        let mut sched = scheduler.lock();
        sched.touch_path(path_id);
        // feat/anchor-hygiene (`RWM_CLOCK_GAP`): quarantined echoes
        // (stall release flood) measured the stall — discard.
        let gap_q = crate::control::anchor::stall_witness()
            .is_some_and(|w| w.quarantined_now());
        if gap_q {
            if let Some(w) = crate::control::anchor::stall_witness() {
                w.note_discard();
            }
        }

        // ── re-homing PART 1: the delivery signal ────────────────
        // Mirrors the legacy Ack arm's three-way branch VERBATIM
        // (feed-present / quarantined / neither), so a configuration
        // that does have a CopaFeed behaves exactly as it does today.
        // `record_delivery` (inside `sched.ack`) is PORTED on purpose:
        // with no feed constructed — the shipped default and every arm
        // of this battery — it is the ONLY window-mode rate anchor,
        // and dropping it is the trap recorded in the Ack arm
        // (max_bw = 0 ⇒ the anchor floor never establishes ⇒ the
        // dynamic store cap sticks at boot 128). The merged ack
        // arrives on exactly the cadence the Ack did, so its
        // ack-interval Δt statistic is unperturbed. `note_discard` is
        // NOT repeated here — the quarantine block above already
        // charged it once for this ack.
        let (d_expected, d_received) = if am_on {
            sched
                .path_mut(path_id)
                .map(|p| p.ack_merge_counter_delta(cum_expected, cum_received))
                .unwrap_or((0, 0))
        } else {
            (0, 0)
        };
        // ACK-CADENCE GAUGE feed (`RWM_ACKDIAG`, net/ackdiag.rs — readouts 1,
        // 2 and 4). EVERY WindowAck arrival is noted, including the (0, 0)
        // sentinel/stale class below: that class IS readout 2's zero-delta
        // fraction, and skipping it would report a cadence the sender does
        // not have. Absent entirely with the gate off (a `OnceLock` null
        // check — no clock read, no lock, no allocation), and the gauge owns
        // all of its state, so nothing here can be observed by this arm.
        //
        // NOTE for the reader of an `RWM_ACK_MERGE=0` log: in that arm the
        // (expected, received) payload rides the LEGACY per-batch `Ack`, so
        // `ack_merge_counter_delta` is never called and the delta readouts
        // are STRUCTURALLY zero. The SPACING readout is valid in both arms —
        // it measures WindowAck arrivals, which the merge does not create.
        if let Some(g) = crate::net::ackdiag::gauge() {
            g.note_ack(path_id, d_expected, d_received);
        }
        let am_live = d_expected > 0 || d_received > 0;
        if am_live {
            if let Some(feed) = copa_feed.filter(|f| !f.n1_paused()) {
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
                // feat/copa-wire-signal: wire-clocked CC delay term —
                // the #80 battery proved the app-echo RTT reads the
                // sender's OWN reservoir dwell as network queue (arm
                // D). The estimator keeps the app echo (end-to-end
                // tail machinery); Copa gets the packet-timed RTT.
                let cc_rtt = if crate::scheduler::copa_wire_active() {
                    transport.wire_rtt(path_id).unwrap_or(rtt_duration)
                } else {
                    rtt_duration
                };
                path.record_rtt_sample(cc_rtt);
            }
        }

        // ── re-homing PART 2: loss, pool, stats, cc window ───────
        // The remainder of the legacy Ack arm, in its own order and
        // with its own guards. The loss feed is the SENDER'S ONLY
        // loss signal and the counter diff is what makes it survive
        // the merge exactly (sums, not events).
        if am_live {
            if let Some(path) = sched.path_mut(path_id) {
                // feat/copa-compete: wire-level loss evidence for the
                // competitive AIMD. Re-homed only when no live feed
                // exists — `copa_feed_attribute` below carries its own
                // call, so this keeps the event exactly-once in BOTH
                // configurations. (Compete implies a passthrough/feed
                // config in practice, so this branch is the
                // belt-and-braces one.)
                if crate::scheduler::copa_compete_active()
                    && copa_feed.filter(|f| !f.n1_paused()).is_none()
                {
                    if let Some((ev, _, _)) = transport.cc_passthrough_stats(path_id) {
                        path.on_wire_congestion_events(ev);
                    }
                }
                // ADR-0003: loss stats from the ack's counter delta.
                //
                // fix/loss-crosspath (`RWM_LOSS_SENT_TRUTH`, default OFF).
                // `d_expected` is the DIFF OF A CONTAMINATED CUMULATIVE:
                // `cum_expected` is `PathBatchTracker::total_expected`
                // (`net/mod.rs:7576`), summed from `gap x received` over a
                // GLOBAL `batch_seq`, so the merged-ack counter path carries
                // the SAME cross-path inflation the legacy `Ack` did —
                // differencing it cannot remove it. The clean pair is the
                // sender's own `symbols_sent` against the receiver's
                // `cum_received` (which never had sequence arithmetic in it).
                // Release keeps the legacy `d_expected` in both arms.
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
                // fix/accounting-ledger (`RWM_RELEASE_1TO1`, default OFF).
                // `d_expected` is the DIFF OF A CONTAMINATED CUMULATIVE
                // (`PathBatchTracker::total_expected`, summed from
                // `gap x received` over a GLOBAL `batch_seq`), so differencing
                // it cannot remove the cross-path inflation — the merged-ack
                // arm leaks the budget gauge exactly as the legacy `Ack` does.
                // Under the gate the term is deleted and `expire_in_flight`'s
                // RFC 9002 sweep of the charge log is the whole lost-symbol
                // release.
                if !crate::scheduler::release_1to1_active() && d_expected > 0 {
                    path.release_in_flight(d_expected.saturating_sub(d_received));
                }
                // Delivery-clocked pool anchor (RWM_POOL_DELIV): the
                // same delivery event the legacy arm fed it.
                path.on_pool_delivery(
                    d_received,
                    d_expected.saturating_sub(d_received),
                    gap_q,
                );
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
                // feat/copa-sole-cc: publish the cwnd as the
                // pass-through substrate window (no-op unless
                // RWM_QUIC_CC=passthrough).
                transport.set_cc_window_bytes(
                    path_id,
                    path.cwnd as u64 * COPA_SOLE_BYTES_PER_SYMBOL,
                );
            }
        }
    }
    // feat/copa-sole-cc: plain-mode Copa delivery feed. Diff this
    // ack's cumulative frontier + SACK ranges against the attribution
    // cursor and drive the per-path Copa machinery (send-interval
    // rate samples, in-flight release, cwnd dynamics, pass-through
    // window write). After the RTT recording above so the cwnd
    // update sees the freshest queue signal.
    if let Some(feed) = copa_feed {
        copa_feed_attribute(
            feed,
            path_id,
            received_up_to,
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
    // P10b: SACK ranges drive reactive repair. Sacked-but-undelivered
    // seqs imply the seqs BETWEEN them are missing at the receiver —
    // invert the ranges into gaps and feed the window sender's NACK
    // repair machinery (exact source retransmission, ADR-0046/0050
    // budgets, per-seq cooldown). Before this the gap info was
    // dropped here and the nack channel had no producer at all
    // (WindowNack is deprecated and never sent), so window mode had
    // NO functioning reactive repair path.
    if !sack_ranges.is_empty() {
        // SACK flow control (feat/sack-flow-control): the RECEIVED
        // ranges themselves let the plain-reliable sender prune its
        // sent-store for out-of-order deliveries, so its flow-control
        // window tracks TRUE outstanding rather than freezing on the
        // in-order cumulative frontier. Forward before inverting to
        // gaps (which drive the orthogonal targeted-retransmit path).
        if let Some(tx) = sack_tx {
            let _ = tx.try_send(sack_ranges.clone());
        }
        let gaps = sack_to_gaps(received_up_to, &sack_ranges);
        if !gaps.is_empty() {
            debug!(path_id, gap_count = gaps.len(), first_gap = ?gaps.first(), "SACK gaps → NACK repair");
            if let Some(tx) = nack_tx {
                let _ = tx.try_send(gaps);
            }
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
