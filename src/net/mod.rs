//! Top-level networking orchestration.
//!
//! Ties together TUN interface, FEC codec, scheduler, controller, and transport
//! into the main data path:
//!
//! Sender:
//!   TUN → block assembly → FEC encode → scheduler → QUIC paths
//!
//! Receiver:
//!   QUIC paths → FEC decode → TUN injection

use crate::control::FecRateController;
use crate::control::fec_rate::ProtocolHint;
use crate::fec::{Decoder, EncodingParams, FecStream};
use crate::scheduler::Scheduler;
use crate::transport::{ControlMessage, QuicTransport, SymbolBatch, WireMessage};
use crate::tun::{TunConfig, TunInterface};
use bytes::Bytes;
use dashmap::DashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
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

    // Channel for received messages from all paths
    let (msg_tx, mut msg_rx) = mpsc::channel::<(u32, WireMessage)>(512);
    let _recv_handles = transport.spawn_receivers(msg_tx);

    // Sender task: TUN → encode → schedule → send
    let transport_arc = Arc::new(transport);
    let scheduler_arc = Arc::new(parking_lot::Mutex::new(scheduler));

    // Clone tx before moving tun into the sender task
    let recv_tun_tx = tun.tx.clone();

    let sender_transport = transport_arc.clone();
    let sender_scheduler = scheduler_arc.clone();
    let sender_fec = fec_controller.clone();
    let sender_block_counter = block_counter.clone();
    let sender_batch_counter = batch_counter.clone();

    let sender_handle = tokio::spawn(async move {
        let mut block_buf = Vec::with_capacity(MAX_BLOCK_SIZE);

        loop {
            // Read packets from TUN and assemble into blocks
            let packet = match tun.read_packet().await {
                Some(p) => p,
                None => {
                    info!("TUN closed");
                    break;
                }
            };

            block_buf.extend_from_slice(&packet);

            // When block is full enough, encode and send
            if block_buf.len() >= MAX_BLOCK_SIZE {
                let block_data = std::mem::replace(
                    &mut block_buf,
                    Vec::with_capacity(MAX_BLOCK_SIZE),
                );

                let block_id = sender_block_counter.fetch_add(1, Ordering::Relaxed);
                let source_symbols =
                    (block_data.len() as f64 / SYMBOL_SIZE as f64).ceil() as u32;

                let params = EncodingParams {
                    source_symbols,
                    symbol_size: SYMBOL_SIZE,
                    repair_count: 0, // computed below
                    block_id,
                };

                // Compute repair count using the controller + worst-path estimator
                let repair_count = {
                    let sched = sender_scheduler.lock();
                    let ctrl = sender_fec.lock();

                    // Use the worst path's estimator for conservative FEC
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

                // Encode
                let mut fec_stream = FecStream::new(
                    &block_data,
                    EncodingParams {
                        repair_count,
                        ..params
                    },
                );

                // Get source symbols first (zero latency)
                let source = fec_stream.take_source_symbols();
                // Then generate repair symbols
                let repair = fec_stream.generate_repair(repair_count);

                debug!(
                    block_id,
                    source_count = source.len(),
                    repair_count = repair.len(),
                    "encoded block"
                );

                // Schedule across paths
                let assignments = sender_scheduler.lock().schedule(source, repair);

                // Send over QUIC
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as u64;

                for (path_id, symbols) in assignments {
                    let batch_seq = sender_batch_counter.fetch_add(1, Ordering::Relaxed);
                    let batch = SymbolBatch {
                        symbols,
                        send_timestamp_us: now,
                        batch_seq,
                    };
                    if let Err(e) = sender_transport.send_symbols(path_id, batch) {
                        warn!(path_id, ?e, "failed to send batch");
                    }
                }
            }
        }
    });

    // Receiver task: receive → decode → TUN inject
    let recv_scheduler = scheduler_arc.clone();
    let recv_fec = fec_controller;
    let recv_decoders = active_decoders;

    let receiver_handle = tokio::spawn(async move {
        while let Some((path_id, msg)) = msg_rx.recv().await {
            match msg {
                WireMessage::Data(batch) => {
                    for symbol in &batch.symbols {
                        // Get or create decoder for this block
                        let mut decoder = recv_decoders
                            .entry(symbol.block_id)
                            .or_insert_with(|| {
                                // We need the encoding params from a BlockStart message
                                // For now, create a basic decoder
                                Decoder::new(
                                    EncodingParams {
                                        source_symbols: 0, // will be set by BlockStart
                                        symbol_size: SYMBOL_SIZE,
                                        repair_count: 0,
                                        block_id: symbol.block_id,
                                    },
                                    MAX_BLOCK_SIZE as u64,
                                )
                            });

                        if let Some(data) = decoder.add_symbol(symbol) {
                            // Block decoded! Inject packets into TUN
                            debug!(block_id = symbol.block_id, "block decoded");

                            // Feed back to FEC controller
                            recv_fec.lock().feedback_update(true);

                            // Inject the decoded data as packets into TUN
                            if recv_tun_tx.send(data).await.is_err() {
                                error!("TUN inject channel closed");
                                return;
                            }
                        }
                    }

                    // Update path loss stats
                    let received = batch.symbols.len() as u32;
                    recv_scheduler
                        .lock()
                        .path_mut(path_id)
                        .map(|p| p.estimator.record_batch(received, received));
                }
                WireMessage::Control(ctrl_msg) => {
                    handle_control_message(
                        path_id,
                        ctrl_msg,
                        &recv_scheduler,
                        &recv_fec,
                    );
                }
            }
        }
    });

    tokio::select! {
        r = sender_handle => { r?; }
        r = receiver_handle => { r?; }
    }

    Ok(())
}

fn handle_control_message(
    path_id: u32,
    msg: ControlMessage,
    scheduler: &Arc<parking_lot::Mutex<Scheduler>>,
    fec_controller: &Arc<parking_lot::Mutex<FecRateController>>,
) {
    match msg {
        ControlMessage::Ack {
            block_id,
            received_ids,
            recv_timestamp_us,
        } => {
            let mut sched = scheduler.lock();
            sched.ack(path_id, received_ids.len() as u32);

            // RTT calculation
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64;
            let rtt_us = now.saturating_sub(recv_timestamp_us);
            if let Some(path) = sched.path_mut(path_id) {
                path.estimator
                    .record_rtt(std::time::Duration::from_micros(rtt_us));
            }
        }
        ControlMessage::BlockResult {
            block_id,
            success,
            symbols_received,
            symbols_needed,
        } => {
            fec_controller.lock().feedback_update(success);
            debug!(
                block_id,
                success,
                symbols_received,
                symbols_needed,
                "block result"
            );
        }
        ControlMessage::PathReport {
            path_id: _,
            loss_rate,
            avg_rtt_us,
            throughput_bps,
        } => {
            let mut sched = scheduler.lock();
            if let Some(path) = sched.path_mut(path_id) {
                path.estimator
                    .record_rtt(std::time::Duration::from_micros(avg_rtt_us));
                path.estimator.record_throughput(throughput_bps);
            }
        }
        ControlMessage::Ping { timestamp_us } => {
            debug!(path_id, timestamp_us, "ping received");
            // TODO: send pong
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
