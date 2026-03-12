//! Top-level networking orchestration.
//!
//! Ties together TUN interface, FEC codec, scheduler, controller, and transport
//! into the main data path:
//!
//! Sender:
//!   TUN → packet framing → block assembly → FEC encode → scheduler → QUIC paths
//!
//! Receiver:
//!   QUIC paths → FEC decode → packet extraction → TUN injection

pub mod framing;

use crate::control::FecRateController;
use crate::control::fec_rate::ProtocolHint;
use crate::fec::{Decoder, EncodingParams, FecStream};
use crate::scheduler::Scheduler;
use crate::transport::{ControlMessage, QuicTransport, SymbolBatch, WireMessage};
use crate::tun::{TunConfig, TunInterface};
use bytes::Bytes;
use dashmap::DashMap;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Configuration for a raptorpath peer.
pub struct PeerConfig {
    pub bind_addrs: Vec<SocketAddr>,
    pub peer_addrs: Vec<SocketAddr>,
    pub tun_name: String,
    pub tun_addr: String,
    pub target_tail_loss: f64,
    pub max_fec_overhead: f64,
    pub protocol_hint: ProtocolHint,
    pub is_server: bool,
}

/// Symbol size — tuned to fit within typical MTU after QUIC overhead.
const SYMBOL_SIZE: u16 = 1200;
/// Maximum block size before FEC encoding (bytes).
const MAX_BLOCK_SIZE: usize = 64 * 1024; // 64KB blocks
/// Flush timeout for partial blocks (ADR-0001).
const FLUSH_TIMEOUT: Duration = Duration::from_millis(10);
/// Decoder eviction timeout for incomplete blocks (ADR-0004).
const DECODER_TIMEOUT: Duration = Duration::from_secs(30);
/// Decoder cleanup interval.
const CLEANUP_INTERVAL: Duration = Duration::from_secs(5);

fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

/// Main entry point.
pub async fn run(config: PeerConfig) -> anyhow::Result<()> {
    // Parse TUN address
    let (tun_ip, prefix_len) = parse_cidr(&config.tun_addr)?;
    let netmask = prefix_to_netmask(prefix_len);

    // Create TUN interface
    let mut tun = TunInterface::create(TunConfig {
        name: config.tun_name.clone(),
        address: tun_ip,
        netmask,
        mtu: 1500,
    })
    .await?;
    info!("TUN interface {} ready", config.tun_name);

    // Create QUIC transport
    let mut transport = QuicTransport::new(&config.bind_addrs, config.is_server).await?;

    // Set up paths
    let mut scheduler = Scheduler::new();
    for (i, _addr) in config.bind_addrs.iter().enumerate() {
        scheduler.add_path(i as u32);
    }

    // Connect or accept on each path
    if config.is_server {
        for i in 0..config.bind_addrs.len() {
            transport.accept(i as u32).await?;
        }
    } else {
        for (i, peer) in config.peer_addrs.iter().enumerate() {
            transport.connect(i as u32, *peer).await?;
        }
    }
    info!("all paths connected");

    // Shared state
    let block_counter = Arc::new(AtomicU64::new(0));
    let batch_counter = Arc::new(AtomicU64::new(0));
    let fec_controller = Arc::new(parking_lot::Mutex::new(FecRateController::new(
        config.target_tail_loss,
        config.max_fec_overhead,
        config.protocol_hint,
    )));
    let active_decoders: Arc<DashMap<u64, Decoder>> = Arc::new(DashMap::new());

    // Per-path sent symbol counts for loss tracking (sender side)
    // Maps (block_id, path_id) → symbols_sent_count
    let sent_counts: Arc<DashMap<(u64, u32), u32>> = Arc::new(DashMap::new());

    // Channel for received messages from all paths
    // ADR-0011: larger message channel to avoid stalling under load
    let (msg_tx, mut msg_rx) = mpsc::channel::<(u32, WireMessage)>(4096);
    let _recv_handles = transport.spawn_receivers(msg_tx);

    // Sender task: TUN → frame → encode → schedule → send
    let transport_arc = Arc::new(transport);
    let scheduler_arc = Arc::new(parking_lot::Mutex::new(scheduler));

    // Clone tx before moving tun into the sender task
    let recv_tun_tx = tun.tx.clone();

    let sender_transport = transport_arc.clone();
    let sender_scheduler = scheduler_arc.clone();
    let sender_fec = fec_controller.clone();
    let sender_block_counter = block_counter.clone();
    let sender_batch_counter = batch_counter.clone();
    let sender_sent_counts = sent_counts.clone();

    let sender_handle = tokio::spawn(async move {
        let mut block_buf = Vec::with_capacity(MAX_BLOCK_SIZE);
        let mut flush_deadline: Option<tokio::time::Instant> = None;

        loop {
            // ADR-0001: select between packet arrival and flush timeout
            let packet = if let Some(deadline) = flush_deadline {
                tokio::select! {
                    p = tun.read_packet() => p,
                    _ = tokio::time::sleep_until(deadline) => {
                        // Timeout: flush partial block
                        None
                    }
                }
            } else {
                tun.read_packet().await
            };

            match packet {
                Some(pkt) => {
                    // ADR-0002: frame each packet with length prefix
                    framing::frame_packet(&mut block_buf, &pkt);

                    // Start flush timer on first packet in block
                    if flush_deadline.is_none() {
                        flush_deadline =
                            Some(tokio::time::Instant::now() + FLUSH_TIMEOUT);
                    }

                    // Flush if block is full
                    if block_buf.len() >= MAX_BLOCK_SIZE {
                        framing::frame_end(&mut block_buf);
                        encode_and_send_block(
                            &mut block_buf,
                            &sender_block_counter,
                            &sender_batch_counter,
                            &sender_scheduler,
                            &sender_fec,
                            &sender_transport,
                            &sender_sent_counts,
                        );
                        flush_deadline = None;
                    }
                }
                None => {
                    if flush_deadline.is_some() && !block_buf.is_empty() {
                        // ADR-0001: flush partial block on timeout
                        framing::frame_end(&mut block_buf);
                        encode_and_send_block(
                            &mut block_buf,
                            &sender_block_counter,
                            &sender_batch_counter,
                            &sender_scheduler,
                            &sender_fec,
                            &sender_transport,
                            &sender_sent_counts,
                        );
                        flush_deadline = None;
                    } else if flush_deadline.is_none() {
                        // TUN closed (read_packet returned None without timeout)
                        info!("TUN closed");
                        break;
                    }
                }
            }
        }
    });

    // Receiver task: receive → decode → extract packets → TUN inject
    let recv_scheduler = scheduler_arc.clone();
    let recv_fec = fec_controller.clone();
    let recv_decoders = active_decoders.clone();
    let recv_transport = transport_arc.clone();
    // Per-path: track last seen batch_seq and total symbols received for loss detection
    let path_batch_tracking: Arc<DashMap<u32, PathBatchTracker>> = Arc::new(DashMap::new());

    let recv_path_tracking = path_batch_tracking.clone();

    let receiver_handle = tokio::spawn(async move {
        while let Some((path_id, msg)) = msg_rx.recv().await {
            match msg {
                WireMessage::Data(batch) => {
                    let batch_send_ts = batch.send_timestamp_us;
                    let batch_seq = batch.batch_seq;
                    let batch_path_id = batch.path_id;
                    let symbol_count = batch.symbols.len() as u32;

                    // Track batch sequences for loss detection (ADR-0003)
                    let (expected, received_total) = {
                        let mut tracker = recv_path_tracking
                            .entry(path_id)
                            .or_insert_with(PathBatchTracker::new);
                        tracker.record_batch(batch_seq, symbol_count)
                    };

                    for symbol in &batch.symbols {
                        // ADR-0008: get or create decoder with proper params
                        let mut decoder = recv_decoders
                            .entry(symbol.block_id)
                            .or_insert_with(|| {
                                // Decoder without BlockStart — will be updated
                                // This handles symbols arriving before BlockStart
                                Decoder::new(
                                    EncodingParams {
                                        source_symbols: 0,
                                        symbol_size: SYMBOL_SIZE,
                                        repair_count: 0,
                                        block_id: symbol.block_id,
                                    },
                                    MAX_BLOCK_SIZE as u64,
                                )
                            });

                        if let Some(data) = decoder.add_symbol(symbol) {
                            let block_id = symbol.block_id;
                            let total_fed = decoder.total_fed();
                            let source_symbols = decoder.params().source_symbols;
                            drop(decoder);

                            debug!(block_id, "block decoded");

                            // Feed back to FEC controller
                            recv_fec.lock().feedback_update(true);

                            // ADR-0005: send BlockResult to sender
                            let result_msg = ControlMessage::BlockResult {
                                block_id,
                                success: true,
                                symbols_received: total_fed,
                                symbols_needed: source_symbols,
                            };
                            if let Err(e) = recv_transport.send_control_datagram(path_id, result_msg) {
                                debug!(?e, path_id, "failed to send BlockResult");
                            }

                            // ADR-0002: extract individual packets from decoded block
                            let packets = framing::extract_packets(&data);
                            // ADR-0011: use try_send to avoid blocking receiver if TUN is slow
                            for pkt_data in packets {
                                match recv_tun_tx.try_send(Bytes::from(pkt_data)) {
                                    Ok(()) => {}
                                    Err(mpsc::error::TrySendError::Full(_)) => {
                                        warn!("TUN inject channel full, dropping packet");
                                    }
                                    Err(mpsc::error::TrySendError::Closed(_)) => {
                                        error!("TUN inject channel closed");
                                        return;
                                    }
                                }
                            }

                            // ADR-0004: remove completed decoder
                            recv_decoders.remove(&block_id);
                        }
                    }

                    // ADR-0005: send ACK with echo timestamp for RTT
                    // Collect received_ids for symbols in this batch
                    let received_ids: Vec<u32> = batch
                        .symbols
                        .iter()
                        .map(|s| s.payload_id)
                        .collect();
                    let ack = ControlMessage::Ack {
                        block_id: batch
                            .symbols
                            .first()
                            .map(|s| s.block_id)
                            .unwrap_or(0),
                        received_ids,
                        echo_send_timestamp_us: batch_send_ts,
                        expected_count: expected,
                        received_count: symbol_count,
                    };

                    // ADR-0003: update path loss stats with actual sent/received
                    recv_scheduler
                        .lock()
                        .path_mut(path_id)
                        .map(|p| p.estimator.record_batch(expected, symbol_count));

                    // ADR-0005: send ACK as datagram (best-effort, low overhead)
                    if let Err(e) = recv_transport.send_control_datagram(path_id, ack) {
                        debug!(?e, path_id, "failed to send ACK datagram");
                    }
                }
                WireMessage::Control(ctrl_msg) => {
                    handle_control_message(
                        path_id,
                        ctrl_msg,
                        &recv_scheduler,
                        &recv_fec,
                        &recv_decoders,
                        &sent_counts,
                        &recv_transport,
                    );
                }
            }
        }
    });

    // ADR-0004: periodic cleanup of stale decoders
    let cleanup_decoders = active_decoders.clone();
    let cleanup_fec = fec_controller.clone();
    let cleanup_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
        loop {
            interval.tick().await;
            let now = Instant::now();
            let mut timed_out = Vec::new();

            cleanup_decoders.retain(|block_id, decoder| {
                if now.duration_since(decoder.created_at) > DECODER_TIMEOUT {
                    if !decoder.is_decoded() {
                        timed_out.push(*block_id);
                    }
                    false // remove
                } else {
                    true // keep
                }
            });

            // Report timed-out blocks as failures to FEC controller
            if !timed_out.is_empty() {
                let mut ctrl = cleanup_fec.lock();
                for _block_id in &timed_out {
                    ctrl.feedback_update(false);
                }
                warn!(
                    count = timed_out.len(),
                    "evicted timed-out decoders (block decode failures)"
                );
            }
        }
    });

    tokio::select! {
        r = sender_handle => { r?; }
        r = receiver_handle => { r?; }
        _ = cleanup_handle => {}
    }

    Ok(())
}

/// Encode a block and send it across paths.
fn encode_and_send_block(
    block_buf: &mut Vec<u8>,
    block_counter: &AtomicU64,
    batch_counter: &AtomicU64,
    scheduler: &Arc<parking_lot::Mutex<Scheduler>>,
    fec_controller: &Arc<parking_lot::Mutex<FecRateController>>,
    transport: &Arc<QuicTransport>,
    sent_counts: &Arc<DashMap<(u64, u32), u32>>,
) {
    let block_data = std::mem::replace(block_buf, Vec::with_capacity(MAX_BLOCK_SIZE));

    if block_data.is_empty() {
        return;
    }

    let block_id = block_counter.fetch_add(1, Ordering::Relaxed);
    let source_symbols = (block_data.len() as f64 / SYMBOL_SIZE as f64).ceil() as u32;

    // Compute repair count
    let repair_count = {
        let sched = scheduler.lock();
        let ctrl = fec_controller.lock();

        let worst_estimator = sched
            .active_paths()
            .iter()
            .filter_map(|id| sched.path(*id))
            .max_by(|a, b| {
                a.estimator
                    .loss_rate()
                    .partial_cmp(&b.estimator.loss_rate())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| &p.estimator);

        match worst_estimator {
            Some(est) => ctrl.compute_repair_count(source_symbols, est),
            None => 0,
        }
    };

    let params = EncodingParams {
        source_symbols,
        symbol_size: SYMBOL_SIZE,
        repair_count,
        block_id,
    };

    // ADR-0008: send BlockStart on all paths before symbols
    // (In production this should go via reliable stream; here we use datagrams)
    let block_start = WireMessage::Control(ControlMessage::BlockStart {
        params,
        transfer_length: block_data.len() as u64,
    });
    let block_start_data = block_start.serialize();
    {
        let sched = scheduler.lock();
        for path_id in sched.active_paths() {
            let start_batch = SymbolBatch {
                symbols: vec![],
                send_timestamp_us: now_us(),
                batch_seq: batch_counter.fetch_add(1, Ordering::Relaxed),
                path_id,
            };
            // Send BlockStart as control (piggyback in datagram for now)
            if let Err(e) = transport.send_symbols(path_id, start_batch) {
                warn!(path_id, ?e, "failed to send BlockStart");
            }
        }
    }

    // Encode
    let mut fec_stream = FecStream::new(&block_data, params);
    let source = fec_stream.take_source_symbols();
    let repair = fec_stream.generate_repair(repair_count);

    debug!(
        block_id,
        source_count = source.len(),
        repair_count = repair.len(),
        block_bytes = block_data.len(),
        "encoded block"
    );

    // Schedule across paths
    let assignments = scheduler.lock().schedule(source, repair);

    let now = now_us();

    // ADR-0003: track how many symbols sent per path for this block
    for (path_id, symbols) in &assignments {
        sent_counts.insert((block_id, *path_id), symbols.len() as u32);
    }

    for (path_id, symbols) in assignments {
        let batch_seq = batch_counter.fetch_add(1, Ordering::Relaxed);
        let batch = SymbolBatch {
            symbols,
            send_timestamp_us: now,
            batch_seq,
            path_id,
        };
        if let Err(e) = transport.send_symbols(path_id, batch) {
            warn!(path_id, ?e, "failed to send batch");
        }
    }
}

/// Per-path batch sequence tracker for loss detection on receiver side.
struct PathBatchTracker {
    /// Last seen batch sequence number
    last_seq: Option<u64>,
    /// Total symbols received on this path
    total_received: u64,
    /// Estimated symbols expected (based on sequence gaps)
    total_expected: u64,
}

impl PathBatchTracker {
    fn new() -> Self {
        Self {
            last_seq: None,
            total_received: 0,
            total_expected: 0,
        }
    }

    /// Record a batch arrival. Returns (expected_for_this_batch, received_in_this_batch).
    /// Uses sequence gaps to estimate expected symbols.
    fn record_batch(&mut self, batch_seq: u64, received: u32) -> (u32, u32) {
        let expected = if let Some(last) = self.last_seq {
            let gap = batch_seq.saturating_sub(last);
            if gap > 1 {
                // Missed batches — estimate their symbols based on this batch size
                // This is approximate; with variable batch sizes it's imperfect
                // but better than assuming 0% loss
                (gap as u32) * received
            } else {
                received
            }
        } else {
            received // first batch, no gap info
        };

        self.last_seq = Some(batch_seq);
        self.total_received += received as u64;
        self.total_expected += expected as u64;

        (expected, received)
    }
}

fn handle_control_message(
    path_id: u32,
    msg: ControlMessage,
    scheduler: &Arc<parking_lot::Mutex<Scheduler>>,
    fec_controller: &Arc<parking_lot::Mutex<FecRateController>>,
    decoders: &Arc<DashMap<u64, Decoder>>,
    sent_counts: &Arc<DashMap<(u64, u32), u32>>,
    transport: &Arc<QuicTransport>,
) {
    match msg {
        // ADR-0008: handle BlockStart
        ControlMessage::BlockStart {
            params,
            transfer_length,
        } => {
            decoders
                .entry(params.block_id)
                .or_insert_with(|| Decoder::new(params, transfer_length));
            debug!(
                block_id = params.block_id,
                source_symbols = params.source_symbols,
                transfer_length,
                "received BlockStart"
            );
        }

        // ADR-0005 + ADR-0007: handle ACK with echo-based RTT
        ControlMessage::Ack {
            block_id,
            received_ids,
            echo_send_timestamp_us,
            expected_count,
            received_count,
        } => {
            let mut sched = scheduler.lock();
            sched.ack(path_id, received_ids.len() as u32);

            // ADR-0007: RTT from echoed sender timestamp (same clock, no skew)
            let now = now_us();
            let rtt_us = now.saturating_sub(echo_send_timestamp_us);
            if let Some(path) = sched.path_mut(path_id) {
                path.estimator
                    .record_rtt(Duration::from_micros(rtt_us));

                // ADR-0003: update loss stats from ACK
                if expected_count > 0 {
                    path.estimator
                        .record_batch(expected_count, received_count);
                }
            }
        }

        ControlMessage::BlockResult {
            block_id,
            success,
            symbols_received,
            symbols_needed,
        } => {
            fec_controller.lock().feedback_update(success);

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

            // Clean up sent_counts for this block
            sent_counts.retain(|(bid, _), _| *bid != block_id);
        }

        ControlMessage::PathReport {
            path_id: report_path_id,
            loss_rate: _,
            avg_rtt_us,
            throughput_bps,
        } => {
            let mut sched = scheduler.lock();
            if let Some(path) = sched.path_mut(report_path_id) {
                path.estimator
                    .record_rtt(Duration::from_micros(avg_rtt_us));
                path.estimator.record_throughput(throughput_bps);
            }
        }

        ControlMessage::Ping { timestamp_us } => {
            debug!(path_id, timestamp_us, "ping received");
            let _ = transport.send_control_datagram(path_id, ControlMessage::Pong { echo_timestamp_us: timestamp_us });
        }

        _ => {}
    }
}

fn parse_cidr(cidr: &str) -> anyhow::Result<(IpAddr, u8)> {
    let parts: Vec<&str> = cidr.split('/').collect();
    if parts.len() != 2 {
        anyhow::bail!("invalid CIDR: {cidr}");
    }
    let ip: IpAddr = parts[0].parse()?;
    let prefix: u8 = parts[1].parse()?;
    Ok((ip, prefix))
}

fn prefix_to_netmask(prefix: u8) -> IpAddr {
    let mask = if prefix >= 32 {
        u32::MAX
    } else {
        u32::MAX << (32 - prefix)
    };
    IpAddr::V4(std::net::Ipv4Addr::from(mask))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cidr() {
        let (ip, prefix) = parse_cidr("10.99.0.1/24").unwrap();
        assert_eq!(ip, "10.99.0.1".parse::<IpAddr>().unwrap());
        assert_eq!(prefix, 24);
    }

    #[test]
    fn test_parse_cidr_32() {
        let (ip, prefix) = parse_cidr("192.168.1.1/32").unwrap();
        assert_eq!(ip, "192.168.1.1".parse::<IpAddr>().unwrap());
        assert_eq!(prefix, 32);
    }

    #[test]
    fn test_parse_cidr_invalid() {
        assert!(parse_cidr("10.0.0.1").is_err());
        assert!(parse_cidr("not/valid").is_err());
    }

    #[test]
    fn test_prefix_to_netmask() {
        let mask = prefix_to_netmask(24);
        assert_eq!(mask, "255.255.255.0".parse::<IpAddr>().unwrap());

        let mask = prefix_to_netmask(16);
        assert_eq!(mask, "255.255.0.0".parse::<IpAddr>().unwrap());

        let mask = prefix_to_netmask(32);
        assert_eq!(mask, "255.255.255.255".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn test_path_batch_tracker_no_loss() {
        let mut tracker = PathBatchTracker::new();
        let (expected, received) = tracker.record_batch(0, 10);
        assert_eq!(expected, 10); // first batch
        assert_eq!(received, 10);

        let (expected, received) = tracker.record_batch(1, 10);
        assert_eq!(expected, 10); // sequential, no gap
        assert_eq!(received, 10);
    }

    #[test]
    fn test_path_batch_tracker_with_gap() {
        let mut tracker = PathBatchTracker::new();
        tracker.record_batch(0, 10);

        // Skip batch 1 (lost)
        let (expected, received) = tracker.record_batch(2, 10);
        assert_eq!(expected, 20); // gap of 2, estimates 2*10 expected
        assert_eq!(received, 10);
    }
}
