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

pub mod block_arq;
pub mod framing;
pub mod interleave;
pub mod reorder;

use block_arq::BlockArq;

use crate::control::FecRateController;
use crate::control::fec_rate::ProtocolHint;
use crate::fec::{EncodingParams, FecBackend, FecDecoder, FecStream};
use crate::fec::{MettleWindowDecoder, MettleWindowEncoder, RlcWindowDecoder, RlcWindowEncoder, WindowDecoder, WindowEncoder};
use crate::monitor::stats::SharedStats;
use crate::routing::{self, ManagedDns, ManagedRoute};
use crate::scheduler::{Scheduler, WallClock};
use crate::transport::{ControlMessage, QuicTransport, SymbolBatch, WireMessage};
use crate::tun::{TunConfig, TunInterface};
use bytes::Bytes;
use dashmap::DashMap;
use reorder::ReorderBuffer;
use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Configuration for a raptorpath peer.
#[derive(Debug)]
pub struct PeerConfig {
    pub bind_addrs: Vec<SocketAddr>,
    pub peer_addrs: Vec<SocketAddr>,
    pub tun_name: String,
    pub tun_addr: String,
    pub target_tail_loss: f64,
    pub max_fec_overhead: f64,
    pub protocol_hint: ProtocolHint,
    pub is_server: bool,
    pub status_addr: Option<SocketAddr>,
    /// Additional routes to add through the tunnel (CIDR notation)
    pub routes: Vec<String>,
    /// DNS server to configure on the tunnel interface
    pub dns: Option<IpAddr>,
    /// Block interleaving depth (1 = disabled, 2+ = interleave across N blocks)
    pub interleave_depth: u32,
    /// Optional path to a pinned TLS certificate for server verification
    pub pin_cert: Option<std::path::PathBuf>,
    /// Which FEC backend to use (RaptorQ or Mettle)
    pub fec_backend: FecBackend,
    /// Whether the user explicitly set fec_backend (vs defaulting to RaptorQ)
    pub fec_backend_explicit: bool,
    /// RWM Phase A (paper §15.7/§16.3): RETAIN-UNTIL-ACKED policy on the
    /// sliding-window pipeline. Routes Bulk/Auto onto the window pipeline
    /// (RLC unless fec_backend overrides). Retention lives at the ARQ
    /// layer: a sent-data store retains source bytes until acked (targeted
    /// retransmit for aged holes; store-full ⇒ TUN-read backpressure) while
    /// the coding window slides freely as the FEC horizon; the receiver
    /// holds delivery at holes until recovered, never force-delivering
    /// past them. Default false.
    pub window_reliable: bool,
    /// Enable PI feedback loop in FEC rate controller
    pub enable_pi_feedback: bool,
    /// Symbol size override (0 = use profile default)
    pub symbol_size_override: u16,
    /// Reorder buffer timeout in ms (0 = disabled)
    pub reorder_timeout_ms: u64,
    /// Reorder buffer max capacity
    pub reorder_max_size: usize,
    /// Inner-feedback weight in [0,1] (paper 14.28): mid-stream repair
    /// floor for TCP-in-tunnel payloads. Default 0.0 — the L1 ablation
    /// measured the floor completion-neutral at C2, regressive at C3.
    pub inner_feedback_weight: f64,
    /// Block-granular multipath source affinity (paper 13.8 in-order
    /// coupling refinement, L2 ws1). Default true; false = per-symbol
    /// striping (ablation).
    pub mp_block_affinity: bool,
}

// ADR-0006: These defaults are now overridden by BlockProfile based on protocol hint.
// Kept as fallback for decoder creation when BlockStart hasn't arrived yet.
const DEFAULT_SYMBOL_SIZE: u16 = 1200;
const DEFAULT_MAX_BLOCK_SIZE: usize = 64 * 1024;

/// ADR-0006: Block assembly profile derived from protocol hint.
struct BlockProfile {
    max_block_size: usize,
    flush_timeout: Duration,
    symbol_size: u16,
}

impl BlockProfile {
    fn from_hint(hint: ProtocolHint) -> Self {
        match hint {
            ProtocolHint::Realtime => Self {
                max_block_size: 4 * 1024,           // 4KB — sub-5ms latency
                flush_timeout: Duration::from_millis(2),
                symbol_size: 512,                    // smaller symbols for small packets
            },
            ProtocolHint::Bulk => Self {
                max_block_size: 64 * 1024,          // 64KB — max throughput
                // 5ms, not 50ms (P7 follow-up): while the Copa gate pauses
                // TUN reads, block assembly stalls mid-block; a 50ms flush
                // then serialized with the CC window and clumped the whole
                // pipeline into ~300ms ack bursts at C2 (L1 measurement).
                // 5ms bounds the assembly wait well under one C2 RTT while
                // still filling 64KB blocks at any bulk-transfer rate.
                flush_timeout: Duration::from_millis(5),
                symbol_size: 1200,
            },
            ProtocolHint::Auto => Self {
                max_block_size: 16 * 1024,          // 16KB — balanced
                flush_timeout: Duration::from_millis(10),
                symbol_size: 1200,
            },
        }
    }
}

/// Decoder eviction timeout for incomplete blocks (ADR-0004).
const DECODER_TIMEOUT: Duration = Duration::from_secs(30);
/// Decoder cleanup interval.
const CLEANUP_INTERVAL: Duration = Duration::from_secs(5);
/// Maximum number of concurrent active decoders. When exceeded, the oldest
/// incomplete decoder is evicted before creating a new one. Prevents OOM from
/// a malicious peer opening unlimited block_ids.
const MAX_CONCURRENT_DECODERS: usize = 10_000;
/// RTCP-style report interval (how often we send PathReport + Ping).
const REPORT_INTERVAL: Duration = Duration::from_secs(2);
/// Maximum window size for sliding-window FEC (source symbols in encoder window).
const MAX_WINDOW_SIZE: usize = 200;
/// Default reorder buffer timeout for window mode (milliseconds).
const DEFAULT_REORDER_TIMEOUT_MS: u64 = 20;
/// Maximum packets buffered in the reorder buffer before force-delivery.
const MAX_REORDER_BUFFERED: usize = 500;
/// Reorder buffer drain interval (how often we check for expired entries).
const REORDER_DRAIN_INTERVAL: Duration = Duration::from_millis(5);
/// Block-mode in-order delivery: max decoded blocks held for ordering
/// (64 × 64KB ≈ 4MB worst case) before force-drain.
const BLOCK_REORDER_MAX_BLOCKS: usize = 64;
/// Bounds for the SRTT-adaptive in-order hold (4×SRTT, clamped). The hold
/// must survive TWO ARQ repair rounds, not one: each round is ~2×SRTT
/// (loss declared after ~1.5×SRTT via Ack diff/timeout + 0.5×SRTT for the
/// repair flight) and under GE burst loss the first repair itself dies
/// with the in-burst probability (~50% at C2) — measured at L1: with a
/// 2×SRTT hold, 4 expiries per 3×1.8MB transfer, each one a REAL hole
/// for the inner TCP (SACK recovery halves the inner cwnd for the rest
/// of the transfer). The cost of a longer hold is paid only when a block
/// is truly unrecoverable (bounded stall, then force-delivery).
const BLOCK_REORDER_MIN_HOLD: Duration = Duration::from_millis(60);
const BLOCK_REORDER_MAX_HOLD: Duration = Duration::from_millis(300);
/// Maximum number of gap ranges in a WindowNack message.
pub const MAX_NACK_GAPS: usize = 20;
/// Maximum repair symbols generated per NACK received.
const MAX_NACK_REPAIRS_PER_NACK: usize = 10;
/// Minimum interval between NACK budget/congestion-state refreshes (microseconds).
const NACK_REPAIR_COOLDOWN_US: u64 = 5_000;
/// Minimum interval between gap-advertising WindowAcks while the cumulative
/// delivery point is stalled on a hole (P10b). The dupack analog: without
/// these, a hole silences ALL acks (the cumulative point can't advance), the
/// sender never learns which seqs are missing, and the only reactive repair
/// left is the reorder-hold expiry force-delivery — which the inner TCP sees
/// as a hole and retransmits (measured L1 realtime C2: ~430 inner
/// retransmits / 5×1.8MB with proactive FEC alone).
const GAP_ACK_MIN_INTERVAL: Duration = Duration::from_millis(2);
/// Reliable window (RWM Phase A): cadence for re-advertising a stalled
/// hole via a SACK-bearing WindowAck (2×SRTT, clamped). The receiver never
/// force-delivers past the hole, so this refresh — with the sender's tail
/// sweep as backstop — is the recovery engine when gap acks are lost.
const HOLE_NACK_REFRESH_MIN: Duration = Duration::from_millis(25);
const HOLE_NACK_REFRESH_MAX: Duration = Duration::from_millis(100);
/// Fallback per-seq retransmit cooldown when no SRTT sample exists (µs).
const NACK_RETX_COOLDOWN_FLOOR_US: u64 = 10_000;
/// Tail ARQ sweep timeout clamp (µs): 2×SRTT bounded to [25ms, 100ms].
/// Must sit above the ack arrival time (~1×SRTT + jitter, or the sweep
/// fires spuriously on every in-flight symbol) and below the receiver's
/// reorder hold (60ms floor) plus the inner-TCP RTO (~200ms).
const TAIL_SWEEP_MIN_US: u64 = 25_000;
const TAIL_SWEEP_MAX_US: u64 = 100_000;
/// Upper clamp on the block-mode idle re-announce cadence (P8). The
/// re-announce timeout is otherwise 1.5×SRTT, but under a stalled block the
/// per-path SRTT estimate inflates well past the true RTT (L1 C3: 40 ms link,
/// SRTT seen at 250–460 ms), which would stretch each recovery round to
/// ~0.7 s and risk exhausting the round budget before the block recovers.
/// Capping the cadence keeps recovery brisk (~sub-second) regardless.
const REANNOUNCE_TIMEOUT_MAX: Duration = Duration::from_millis(200);

/// Congestion-aware NACK repair throttle (ADR-0046).
///
/// Tracks loss rate and RTT trends to detect congestion vs wireless loss.
/// When congestion is detected (rising loss AND rising RTT), exponentially
/// reduces NACK repair count. When congestion clears, linearly ramps up.
struct NackCongestionState {
    /// Current multiplier for NACK repairs (0.0 = fully suppressed, 1.0 = normal)
    repair_multiplier: f64,
    /// Previous loss rate sample
    prev_loss_rate: f64,
    /// Consecutive rising-loss periods
    rising_loss_count: u32,
    /// Previous RTT sample
    prev_rtt: Option<Duration>,
    /// Consecutive rising-RTT periods
    rising_rtt_count: u32,
    /// How many consecutive rises trigger backoff
    congestion_threshold: u32,
    /// Per-update recovery step when not congested
    recovery_step: f64,
}

impl NackCongestionState {
    fn new() -> Self {
        Self {
            repair_multiplier: 1.0,
            prev_loss_rate: 0.0,
            rising_loss_count: 0,
            prev_rtt: None,
            rising_rtt_count: 0,
            congestion_threshold: 2,
            recovery_step: 0.1,
        }
    }

    /// Update with current loss rate and RTT. Returns the repair multiplier.
    fn update(&mut self, loss_rate: f64, rtt: Option<Duration>) -> f64 {
        // Detect rising loss (>10% relative increase + 0.1% absolute floor)
        if loss_rate > self.prev_loss_rate * 1.1 + 0.001 {
            self.rising_loss_count += 1;
        } else {
            self.rising_loss_count = 0;
        }
        self.prev_loss_rate = loss_rate;

        // Detect rising RTT
        if let (Some(prev), Some(curr)) = (self.prev_rtt, rtt) {
            if curr > prev + Duration::from_millis(1) {
                self.rising_rtt_count += 1;
            } else {
                self.rising_rtt_count = 0;
            }
        }
        self.prev_rtt = rtt;

        // Congestion = both rising loss AND rising RTT
        let congested = self.rising_loss_count >= self.congestion_threshold
            && self.rising_rtt_count >= self.congestion_threshold;

        if congested {
            // Exponential backoff: halve the multiplier
            self.repair_multiplier = (self.repair_multiplier * 0.5).max(0.0);
        } else if self.rising_loss_count == 0 && self.rising_rtt_count == 0 {
            // Both stable: linearly recover
            self.repair_multiplier = (self.repair_multiplier + self.recovery_step).min(1.0);
        }
        // If only one is rising, hold steady

        self.repair_multiplier
    }

    /// Current repair multiplier.
    fn multiplier(&self) -> f64 {
        self.repair_multiplier
    }
}

/// Returns true if this config should use sliding-window mode instead of block mode.
///
/// The pipeline shape follows from the algorithm's capabilities: streaming-native
/// backends (RLC, METTLE) use the sliding-window pipeline; block-only backends
/// (RaptorQ, Reed-Solomon) always use the block pipeline. By default only
/// Realtime rides the window pipeline; `window_reliable` (RWM Phase A) opts
/// Bulk/Auto onto it with the RETAIN-UNTIL-ACKED policy.
fn is_window_mode(hint: ProtocolHint, backend: FecBackend, window_reliable: bool) -> bool {
    (hint == ProtocolHint::Realtime || window_reliable) && backend.is_streaming()
}

// ---------------------------------------------------------------------------
// RWM Phase A retention policy (paper §15.7/§16.3), unit-tested below.
//
// Reliability is a PIPELINE POLICY, not a codec property — and it lives at
// the ARQ layer, not in the coding window. The coding window keeps sliding
// freely under BOTH policies: it is only the FEC horizon (fungible repair
// coverage for recent, not-yet-localized losses). What differs:
//
//   EVICT              — production Realtime. The retransmit buffer holds
//                        metadata only; source bytes die with window
//                        eviction, losses past the horizon become holes
//                        (bounded memory, bounded delay — correct for δ).
//   RETAIN-UNTIL-ACKED — a sent-data STORE retains every sent source
//                        symbol's bytes until the peer acks it (removal by
//                        ack ONLY — never timeout, never pressure). An aged
//                        SACK-confirmed hole that slid out of the window is
//                        recovered by a TARGETED retransmit of exactly that
//                        symbol from the store (once a loss is localized,
//                        fungibility has no value). Store fullness becomes
//                        backpressure on the TUN — the same contract as the
//                        block path's cwnd gate — never data loss (the
//                        measured F2 failure: dropping un-acked source
//                        flipped bulk 10/10 → 0/10 DNF).
// ---------------------------------------------------------------------------

/// Sent-data store capacity (symbols) for the RETAIN-UNTIL-ACKED policy.
/// Sized to a few BDPs of plain bytes (no coding cost): 1024 × 1200 B
/// ≈ 1.2 MB ≈ 10× the C2 BDP (100 Mbit × 10 ms ≈ 104 symbols). When
/// full, the sender stops reading the TUN until acks drain it (flow
/// control, not loss).
const RELIABLE_STORE_MAX: usize = 1024;

/// Reliable-policy backpressure: when the sent-data store is full of
/// un-acked symbols, stop reading the TUN (the same contract as the block
/// path's cwnd gate) instead of dropping retention. EVICT mode never
/// backpressures on retention (Realtime's correct spend of the budget).
fn store_backpressure(reliable: bool, store_len: usize) -> bool {
    reliable && store_len >= RELIABLE_STORE_MAX
}
/// Dead path timeout: if no report received for this long, deactivate the path.
const DEAD_PATH_TIMEOUT: Duration = Duration::from_secs(6);
/// QUIC/IP overhead subtracted from max_datagram_size to get usable symbol size.
/// 8 bytes wire header + ~40 bytes bincode overhead estimate.
const WIRE_OVERHEAD: usize = 48;
/// Serialized SymbolBatch envelope (WireMessage tag + timestamps + seq).
const BATCH_WIRE_HEADER: usize = 48;
/// Per-symbol serialization overhead inside a batch (ids + flags + len).
const PER_SYMBOL_WIRE_OVERHEAD: usize = 32;

/// Map FecBackend to u8 for atomic stats storage.
fn backend_to_u8(backend: FecBackend) -> u8 {
    match backend {
        FecBackend::RaptorQ => 0,
        FecBackend::Mettle => 1,
        FecBackend::ReedSolomon => 2,
        FecBackend::Rlc => 3,
        FecBackend::Streaming => 4,
    }
}

fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

/// Main entry point.
pub async fn run(config: PeerConfig) -> anyhow::Result<()> {
    run_impl(config, None).await
}

/// Run the engine with a caller-provided TUN (e.g. [`TunInterface::memory`]).
///
/// Skips OS TUN creation and ALL routing/DNS management (setup and cleanup) —
/// nothing OS-touching happens for the injected interface. Everything else is
/// byte-identical to [`run`]. Window-mode note: the MTU clamp only sizes the
/// OS TUN device; with an injected TUN the caller must size its packets to
/// fit one symbol (`profile.symbol_size - 4`) itself.
pub async fn run_with_tun(config: PeerConfig, tun: TunInterface) -> anyhow::Result<()> {
    run_impl(config, Some(tun)).await
}

async fn run_impl(config: PeerConfig, injected_tun: Option<TunInterface>) -> anyhow::Result<()> {
    let tun_injected = injected_tun.is_some();
    // Parse TUN address
    let (tun_ip, prefix_len) = parse_cidr(&config.tun_addr)?;
    let netmask = prefix_to_netmask(prefix_len);

    // Backend selection happens ONCE, here, and is pinned for the life of
    // the stream (paper §16.4: no cross-code algebra ⇒ any mid-stream
    // switch strands in-flight data; the old runtime auto-switch was
    // removed). Computed before TUN creation because window mode
    // constrains the TUN MTU.
    //
    // Realtime auto-selects the streaming backend (delay-optimal on bursty
    // GE channels). Bulk/Auto under `window_reliable` (RWM Phase A)
    // auto-select windowed RLC — the natural sliding-window codec; the
    // bulk profile's symbol_size=1200 puts the window-mode TUN MTU clamp
    // at 1196, so full-size packets are not fragmented.
    let effective_fec_backend = if !config.fec_backend_explicit {
        if config.protocol_hint == ProtocolHint::Realtime {
            info!("Realtime mode: auto-selecting streaming backend for bursty channel protection");
            FecBackend::Streaming
        } else if config.window_reliable {
            info!("reliable window mode (RWM Phase A): auto-selecting RLC windowed backend");
            FecBackend::Rlc
        } else {
            config.fec_backend
        }
    } else {
        config.fec_backend
    };

    // ADR-0006: derive block assembly profile from protocol hint
    let profile = BlockProfile::from_hint(config.protocol_hint);
    let window_mode = is_window_mode(config.protocol_hint, effective_fec_backend, config.window_reliable);
    // The retention policy is per-stream/per-config, NOT global: Realtime
    // keeps its lossy EVICT window unless explicitly opted in.
    let window_reliable = window_mode && config.window_reliable;
    if config.window_reliable && !window_mode {
        warn!(
            backend = ?effective_fec_backend,
            "window_reliable set but the configured FEC backend is not \
             streaming-capable — falling back to the block pipeline"
        );
    }

    // Window mode carries at most ONE packet per source symbol: SymbolPacker
    // frames each packet with a 2-byte length prefix and closes the symbol
    // with a 2-byte end sentinel, and packets that don't fit are TRUNCATED
    // (corrupted on the wire, silently dropped by the peer's IP stack).
    // Clamp the TUN MTU so the inner stack never emits a packet larger than
    // one symbol can carry (L1 realtime finding: MSS-sized TCP segments were
    // truncated at symbol_size=512 and every transfer stalled).
    let tun_mtu: u16 = if window_mode {
        let mtu = profile.symbol_size.saturating_sub(4);
        info!(
            mtu,
            symbol_size = profile.symbol_size,
            "window mode: clamping TUN MTU to fit one packet per symbol"
        );
        mtu
    } else {
        1500
    };

    // Create TUN interface (or use the injected one — memory TUNs need no
    // OS device and no routing/DNS management)
    let mut tun = match injected_tun {
        Some(t) => {
            info!(name = %t.name, "using injected TUN interface (no routes/DNS)");
            t
        }
        None => {
            let tun = TunInterface::create(TunConfig {
                name: config.tun_name.clone(),
                address: tun_ip,
                netmask,
                mtu: tun_mtu,
            })
            .await?;
            info!("TUN interface {} ready", config.tun_name);
            tun
        }
    };

    // Set up routes through the tunnel (skipped for injected TUNs)
    let peer_gateway = routing::infer_peer_ip(tun_ip, prefix_len);
    let mut managed_routes: Vec<ManagedRoute> = Vec::new();
    if tun_injected {
        // no OS interface — nothing to route
    } else if let Some(gw) = peer_gateway {
        for route_cidr in &config.routes {
            let route = ManagedRoute {
                destination: route_cidr.clone(),
                gateway: gw,
                iface: config.tun_name.clone(),
            };
            if let Err(e) = routing::add_route(&route).await {
                warn!(%e, route = %route_cidr, "failed to add route");
            } else {
                managed_routes.push(route);
            }
        }
    } else if !config.routes.is_empty() {
        warn!("cannot infer peer gateway IP — routes not added");
    }

    // Configure DNS on tunnel interface (skipped for injected TUNs)
    let mut managed_dns: Option<ManagedDns> = None;
    if let Some(dns_server) = config.dns {
        if tun_injected {
            warn!("injected TUN: ignoring DNS configuration");
        } else {
            let mut dns = ManagedDns {
                server: dns_server,
                iface: config.tun_name.clone(),
                #[cfg(target_os = "linux")]
                previous_resolv_conf: None,
            };
            if let Err(e) = routing::set_dns(&mut dns).await {
                warn!(%e, "failed to configure DNS");
            } else {
                managed_dns = Some(dns);
            }
        }
    }

    // Create QUIC transport
    let mut transport = QuicTransport::new(
        &config.bind_addrs,
        config.is_server,
        config.pin_cert.as_deref(),
    ).await?;

    // Set up paths with protocol-hint-derived scheduling weights
    let mut scheduler = Scheduler::new_with_hint(Arc::new(WallClock), config.protocol_hint);
    scheduler.set_block_affinity(config.mp_block_affinity);
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
    let fec_controller = Arc::new(parking_lot::Mutex::new({
        let mut ctrl = FecRateController::new_with_toggles(
            config.target_tail_loss,
            config.max_fec_overhead,
            config.protocol_hint,
            effective_fec_backend,
            config.enable_pi_feedback,
            profile.symbol_size,
        );
        // P10a (paper 14.28): inner-feedback repair floor for
        // TCP-in-tunnel payloads. Default weight 0.0 (config::resolve):
        // the L1 C2/C3 ablation measured the floor active but
        // completion-neutral at C2 and regressive at C3 — post-P8/P9b the
        // inner flow absorbs the residual ARQ stalls, and floor repairs
        // displace source symbols in the same inner-limited loop. The
        // knob remains for payloads that measure differently.
        ctrl.set_inner_feedback(config.inner_feedback_weight);
        ctrl
    }));
    info!(
        max_block_size = profile.max_block_size,
        flush_timeout_ms = profile.flush_timeout.as_millis() as u64,
        symbol_size = profile.symbol_size,
        interleave_depth = config.interleave_depth,
        "block assembly profile"
    );

    // ADR-0013: shared monitoring stats
    let stats = Arc::new(SharedStats::new());
    for (i, _) in config.bind_addrs.iter().enumerate() {
        stats.add_path(i as u32);
    }
    // Store target tail loss in stats
    stats.fec.target_tail_loss_bits.store(
        config.target_tail_loss.to_bits(),
        Ordering::Relaxed,
    );

    // ADR-0015: graceful shutdown signaling
    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);
    let mut sender_shutdown_rx = shutdown_tx.subscribe();
    let mut recv_shutdown_rx = shutdown_tx.subscribe();

    // Spawn Ctrl+C handler
    let ctrlc_shutdown_tx = shutdown_tx.clone();
    tokio::spawn(async move {
        if let Ok(()) = tokio::signal::ctrl_c().await {
            info!("received Ctrl+C, initiating graceful shutdown...");
            let _ = ctrlc_shutdown_tx.send(());
        }
    });

    // Shared window ACK: receiver writes, sender reads to advance the encoder window
    let window_ack_seq = Arc::new(AtomicU64::new(0));

    // NACK gap channel: handle_control_message sends gap ranges, window sender receives for targeted repair
    let (nack_tx, nack_rx) = tokio::sync::mpsc::channel::<Vec<(u64, u64)>>(16);

    if window_mode {
        info!(
            symbol_size = profile.symbol_size,
            backend = ?effective_fec_backend,
            "sliding-window FEC mode"
        );
    }

    let active_decoders: Arc<DashMap<u64, Box<dyn FecDecoder>>> = Arc::new(DashMap::new());

    // Per-path sent symbol counts for loss tracking (sender side)
    // Maps (block_id, path_id) → symbols_sent_count
    let sent_counts: Arc<DashMap<(u64, u32), u32>> = Arc::new(DashMap::new());

    // Block-mode ARQ (P8): sender-side batch ledger + retained blocks for
    // Ack-diff-driven repair. Unused in window mode (which has its own
    // retransmit buffer / SACK machinery).
    let block_arq: Arc<parking_lot::Mutex<BlockArq>> =
        Arc::new(parking_lot::Mutex::new(BlockArq::new()));

    // Channel for received messages from all paths
    // ADR-0011: larger message channel to avoid stalling under load
    let (msg_tx, mut msg_rx) = mpsc::channel::<(u32, WireMessage)>(4096);
    // Dedicated channel for stream-origin control: liveness must not queue
    // behind the data flood (see spawn_receiver_for_path).
    let (ctrl_tx, mut ctrl_rx) = mpsc::channel::<(u32, WireMessage)>(256);
    let _recv_handles = transport.spawn_receivers(msg_tx.clone(), ctrl_tx.clone());

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
    let ctrl_sent_counts = sent_counts.clone();
    let sender_stats = stats.clone();
    let sender_block_arq = block_arq.clone();

    let sender_profile_max_block = profile.max_block_size;
    let sender_profile_flush = profile.flush_timeout;
    let sender_profile_symbol_size = profile.symbol_size;
    // Mid-stream FEC backend switching was REMOVED (paper §16.4): a switch
    // strands every in-flight symbol of the old code (no cross-code
    // algebra) and discards the estimator/ARQ state recovery needs — the
    // P9a bring-up measured exactly this (a window-mode switch restarted
    // seq numbering at 0, blinding the ACK/NACK machinery for ~a window of
    // traffic; at lossy cells the repair blackout wedged TCP for minutes).
    // The backend chosen above is pinned for the life of the stream.
    let sender_fec_backend = effective_fec_backend;
    let sender_interleave_depth = config.interleave_depth;
    // Interleave timeout = 2x flush timeout (drain buffered symbols if traffic is sparse)
    let sender_interleave_timeout = profile.flush_timeout * 2;
    let sender_window_mode = window_mode;
    let sender_window_reliable = window_reliable;
    let sender_window_ack = window_ack_seq.clone();
    let mut sender_nack_rx = nack_rx;
    let sender_protocol_hint = config.protocol_hint;

    let sender_handle = tokio::spawn(async move {
        // ----- Sliding-window sender mode -----
        if sender_window_mode {
            run_window_sender(
                &mut tun,
                sender_profile_symbol_size,
                sender_fec_backend,
                &sender_fec,
                &sender_batch_counter,
                &sender_transport,
                &sender_scheduler,
                &sender_stats,
                &sender_window_ack,
                &mut sender_nack_rx,
                &mut sender_shutdown_rx,
                sender_protocol_hint,
                sender_window_reliable,
            )
            .await;
            return;
        }

        // ----- Block-mode sender (existing) -----
        let mut block_buf = Vec::with_capacity(sender_profile_max_block);
        let mut last_tx_paused = false;
        let mut flush_deadline: Option<tokio::time::Instant> = None;
        // Pacing retry: set when the token bucket left symbols in the
        // carry (P7); the select loop resumes the paced drain when it fires.
        let mut pace_deadline: Option<tokio::time::Instant> = None;
        // Symbol-level pacing carry: drained-but-not-yet-sendable symbols
        // wait here between pace ticks (P7 follow-up — the interleaver
        // drain is all-or-nothing, so partial sends need their own queue).
        let mut pace_carry: PaceCarry = PaceCarry::new();
        let mut shutting_down = false;
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

            // Copa backpressure (paper 12 / ADR-0050): stop reading the
            // TUN while the wire budget is exhausted — the inner flow's own
            // CC sees the growing TUN queue and slows down. Without this
            // the encoder ran at TUN speed, saturated the runtime, starved
            // QUIC timers/liveness, and any bulk transfer killed the
            // tunnel within DEAD_PATH_TIMEOUT (L1 harness finding).
            let (tx_paused, dbg_fl, dbg_cw) = {
                let mut sched = sender_scheduler.lock();
                let mut fl = 0u64;
                let mut cw = 0u64;
                for id in sched.live_paths() {
                    if let Some(p) = sched.path_mut(id) {
                        // Time-based budget release first: stranded charges
                        // (lost best-effort ACK datagrams) must reopen the
                        // gate at RTT timescale, not the 2s leak-guard
                        // cadence (P7 follow-up 2, L1 finding).
                        p.expire_in_flight();
                        fl += p.in_flight as u64;
                        cw += p.cwnd as u64;
                    }
                }
                // in_flight is charged once at SCHEDULE time, so it already
                // covers interleaver + pacing carry + wire — the whole
                // committed pipeline.
                (fl >= cw.max(4), fl, cw)
            };
            if tx_paused != last_tx_paused {
                debug!(tx_paused, in_flight = dbg_fl, cwnd = dbg_cw, "backpressure state change");
                last_tx_paused = tx_paused;
            }

            // ADR-0001: select between packet arrival, flush timeout, interleave drain, and shutdown
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
                    p = tun.read_packet(), if !tx_paused => p,
                    _ = flush_sleep => None,
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
                    for pid in sched.active_paths() {
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
                        // ADR-0001: flush partial block on timeout
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
    });

    // Receiver task: receive → decode → extract packets → TUN inject
    let recv_scheduler = scheduler_arc.clone();
    let recv_fec = fec_controller.clone();
    let recv_decoders = active_decoders.clone();
    let recv_fec_backend = effective_fec_backend;
    let recv_transport = transport_arc.clone();
    // Block-mode ARQ: Ack handling (which drives repair) runs in the
    // receiver task, so it needs the shared ledger + the batch counter
    // (repair batches use the same per-path-monotonic sequence space).
    let recv_block_arq = block_arq.clone();
    let recv_batch_counter = batch_counter.clone();
    // Per-path: track last seen batch_seq and total symbols received for loss detection
    let path_batch_tracking: Arc<DashMap<u32, PathBatchTracker>> = Arc::new(DashMap::new());

    let recv_path_tracking = path_batch_tracking.clone();
    let recv_stats = stats.clone();
    let recv_symbol_size = profile.symbol_size;
    let recv_window_mode = window_mode;
    let recv_window_reliable = window_reliable;
    let recv_window_ack = window_ack_seq.clone();
    let recv_nack_tx: Option<tokio::sync::mpsc::Sender<Vec<(u64, u64)>>> = if window_mode {
        Some(nack_tx)
    } else {
        None
    };

    let receiver_handle = tokio::spawn(async move {
        // Window decoder: created once, long-lived (only used in window
        // mode; codec pinned at startup, §16.4 — never rebuilt).
        let mut window_decoder: Option<Box<dyn WindowDecoder>> = if recv_window_mode {
            Some(create_window_decoder(recv_fec_backend, recv_symbol_size))
        } else {
            None
        };
        // Whether the sender packs multiple packets per symbol (set via WindowStart)
        let mut window_packed: bool = false;
        // Track highest delivered seq for window ACK
        let mut highest_delivered_seq: u64 = 0;
        // The highest delivered seq we last advertised in a WindowAck (dedupe
        // for ack sends; the shared window_ack_seq atomic carries the PEER's
        // acks for the local sender and must not be conflated with this).
        let mut last_advertised_ack: u64 = 0;
        // Reorder buffer for window mode — delivers packets in sequence order.
        // Reliable policy (RWM Phase A): holes are held until recovered,
        // never force-delivered past (the buffer is mandatory — in-order
        // delivery IS the reliability contract at the receiver).
        let mut reorder_buf = if recv_window_mode && recv_window_reliable {
            Some(ReorderBuffer::new_reliable())
        } else if recv_window_mode && config.reorder_timeout_ms > 0 {
            Some(ReorderBuffer::new(config.reorder_timeout_ms, config.reorder_max_size))
        } else {
            None
        };
        // Reliable mode: when delivery is stalled on a hole, periodically
        // re-advertise the gap (SACK-bearing WindowAck) — acks are
        // best-effort datagrams, and a lost gap report must not leave
        // recovery to the sender's single-seq tail sweep alone.
        let mut last_hole_nack_at = Instant::now();
        // Track received seqs for WindowNack gap reporting
        let mut received_seqs: BTreeSet<u64> = BTreeSet::new();
        let mut highest_seen_seq: u64 = 0;
        let mut last_nack_time = Instant::now();
        // P10b dupack analog: highest_seen at the last gap-advertising ack,
        // and when it was sent (rate limit) — see GAP_ACK_MIN_INTERVAL.
        let mut last_gap_ack_seen: u64 = 0;
        let mut last_gap_ack_time = Instant::now() - GAP_ACK_MIN_INTERVAL;
        // ADR-0035: PI feedback tracking for window mode
        let mut last_pi_repairs_fed: u64 = 0;
        let mut last_pi_repairs_useful: u64 = 0;

        // Block-mode symbols that arrive BEFORE their BlockStart (datagrams
        // routinely outrace the reliable control stream). A decoder created
        // without the real params can never decode -- its OTI transfer
        // length is wrong and its source array is empty -- so such symbols
        // are buffered here and replayed when BlockStart arrives. L1
        // harness finding: on a real link every small block lost this race
        // and timed out; the tunnel never carried a single packet.
        // Bounds: 32 blocks x 128 symbols x ~1.2 KB ~ 5 MB worst case.
        let mut pre_start_symbols: std::collections::HashMap<u64, Vec<crate::fec::WireSymbol>> =
            std::collections::HashMap::new();

        // Recently decoded block ids (P8): late ARQ repairs — or spurious
        // ones after a lost Ack — arrive AFTER the decoder was removed and
        // would otherwise be buffered as "pre-BlockStart" symbols, wasting
        // pre_start_symbols slots on blocks that are already done.
        // (parking_lot::Mutex, not RefCell: the spawned future must be Send.
        // Single-task access — never contended.)
        let completed_blocks: parking_lot::Mutex<(std::collections::VecDeque<u64>, std::collections::HashSet<u64>)> =
            parking_lot::Mutex::new((std::collections::VecDeque::new(), std::collections::HashSet::new()));
        const COMPLETED_RING_CAP: usize = 512;

        // Block-mode IN-ORDER delivery (L1 C2 finding): block ids are
        // strictly sequential per peer, but blocks decode out of order —
        // a block waiting on an ARQ repair round (~2×SRTT) was overtaken
        // by later blocks and the inner TCP saw a 64KB hole: measured
        // 879 spurious fast-retransmits / 733 SACK-reorder events per
        // 3×1.8MB at C2, halving the inner cwnd repeatedly. Decoded
        // payloads therefore pass through a reorder buffer keyed by
        // block_id (SRTT-adaptive hold, force-delivery on expiry — the
        // same delivery contract window mode already had).
        // (parking_lot::Mutex for the same Send reason as above.)
        let block_inorder_enabled = !recv_window_mode && config.reorder_timeout_ms > 0;
        let block_reorder: parking_lot::Mutex<ReorderBuffer> = parking_lot::Mutex::new(
            ReorderBuffer::new(BLOCK_REORDER_MIN_HOLD.as_millis() as u64, BLOCK_REORDER_MAX_BLOCKS),
        );

        // Instrumentation (L2 ws1, temp): per-block arrival tracking —
        // first-symbol instant + per-path symbol counts — and in-order
        // hold timestamps. Emitted as debug logs on decode/release.
        let block_arrival: parking_lot::Mutex<
            std::collections::HashMap<u64, (Instant, std::collections::HashMap<u32, u32>)>,
        > = parking_lot::Mutex::new(std::collections::HashMap::new());
        let block_held_at: parking_lot::Mutex<std::collections::HashMap<u64, Instant>> =
            parking_lot::Mutex::new(std::collections::HashMap::new());

        // Feed one block-mode symbol into its (existing) decoder; on
        // completion: stats, FEC feedback, BlockResult, packet extraction,
        // TUN inject, decoder removal. Returns false iff the TUN inject
        // channel is closed (receiver must exit). Shared by the data-arm
        // fast path and the BlockStart replay path.
        let feed_block_symbol = |symbol: &crate::fec::WireSymbol, path_id: u32| -> bool {
            let Some(mut decoder) = recv_decoders.get_mut(&symbol.block_id) else {
                return true;
            };
            let feed_start = Instant::now();
            if let Some(data) = decoder.add_symbol(symbol) {
                let block_id = symbol.block_id;
                let total_fed = decoder.total_fed();
                let source_symbols = decoder.params().source_symbols;
                drop(decoder);

                debug!(
                    block_id,
                    decode_us = feed_start.elapsed().as_micros() as u64,
                    "block decoded"
                );
                // Instrumentation (L2 ws1): block completion time from
                // first symbol arrival + per-path arrival composition.
                if let Some((first, counts)) = block_arrival.lock().remove(&block_id) {
                    let mut per_path: Vec<(u32, u32)> = counts.into_iter().collect();
                    per_path.sort_unstable();
                    debug!(
                        block_id,
                        complete_ms = first.elapsed().as_millis() as u64,
                        paths = ?per_path,
                        "block completed"
                    );
                }
                recv_stats.blocks.decoded_ok.fetch_add(1, Ordering::Relaxed);
                recv_fec.lock().feedback_update(true);

                let result_msg = ControlMessage::BlockResult {
                    block_id,
                    success: true,
                    symbols_received: total_fed,
                    symbols_needed: source_symbols,
                };
                if let Err(e) = recv_transport.send_control_datagram(path_id, result_msg) {
                    debug!(?e, path_id, "failed to send BlockResult");
                }

                // In-order delivery: hold out-of-order blocks (see
                // block_reorder above); inject the contiguous prefix.
                let deliverable = if block_inorder_enabled {
                    block_reorder.lock().push(block_id, data)
                } else {
                    vec![(block_id, data)]
                };
                // Instrumentation (L2 ws1): who waits on whom, for how long.
                if block_inorder_enabled {
                    if deliverable.is_empty() {
                        let waiting_on = block_reorder.lock().next_deliver_seq();
                        block_held_at.lock().insert(block_id, Instant::now());
                        debug!(block_id, waiting_on, "in-order held");
                    } else {
                        let mut held = block_held_at.lock();
                        for (bid, _) in &deliverable {
                            if let Some(t) = held.remove(bid) {
                                debug!(
                                    block_id = *bid,
                                    held_ms = t.elapsed().as_millis() as u64,
                                    unblocked_by = block_id,
                                    "in-order hold released"
                                );
                            }
                        }
                    }
                }
                for (_bid, bdata) in deliverable {
                    let packets = framing::extract_packets(&bdata);
                    for pkt_data in packets {
                        match recv_tun_tx.try_send(Bytes::from(pkt_data)) {
                            Ok(()) => {}
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                warn!("TUN inject channel full, dropping packet");
                            }
                            Err(mpsc::error::TrySendError::Closed(_)) => {
                                error!("TUN inject channel closed");
                                return false;
                            }
                        }
                    }
                }

                recv_decoders.remove(&block_id);
                {
                    let mut done = completed_blocks.lock();
                    if done.1.insert(block_id) {
                        done.0.push_back(block_id);
                        while done.0.len() > COMPLETED_RING_CAP {
                            if let Some(old) = done.0.pop_front() {
                                done.1.remove(&old);
                            }
                        }
                    }
                }
                recv_stats
                    .blocks
                    .pending
                    .store(recv_decoders.len() as u64, Ordering::Relaxed);
            }
            true
        };

        loop {
            // In-order hold drain timer (BOTH modes): refresh the
            // SRTT-adaptive timeout and compute the oldest-entry expiry.
            // Only when entries are pending — the common case skips the
            // locks. Window mode MUST have this timer too: its drain used
            // to run only on symbol arrival, and a hole could deadlock the
            // whole tunnel (hole → no delivery advance → no WindowAck →
            // sender window full → no sends → no arrivals → no drain;
            // measured at L1 realtime C2: inner TCP wedged for minutes).
            let reorder_deadline: Option<tokio::time::Instant> = {
                let pending = if block_inorder_enabled {
                    block_reorder.lock().pending_count() > 0
                } else {
                    reorder_buf.as_ref().is_some_and(|rb| rb.pending_count() > 0)
                };
                if pending {
                    let srtt = {
                        let sched = recv_scheduler.lock();
                        sched
                            .live_paths()
                            .into_iter()
                            .filter_map(|pid| sched.path(pid).map(|p| p.srtt()))
                            .max()
                    };
                    let deadline = if recv_window_reliable {
                        // Reliable policy: the hole is never given up on —
                        // this timer instead re-advertises the gap (SACK
                        // WindowAck) at 2×SRTT cadence until recovered.
                        let refresh = srtt
                            .map(|s| (s * 2).clamp(HOLE_NACK_REFRESH_MIN, HOLE_NACK_REFRESH_MAX))
                            .unwrap_or(HOLE_NACK_REFRESH_MAX);
                        Some(last_hole_nack_at + refresh)
                    } else {
                        let hold = srtt
                            .map(|s| (s * 4).clamp(BLOCK_REORDER_MIN_HOLD, BLOCK_REORDER_MAX_HOLD));
                        if block_inorder_enabled {
                            let mut rb = block_reorder.lock();
                            if let Some(h) = hold {
                                rb.set_timeout(h);
                            }
                            rb.oldest_deadline()
                        } else {
                            let rb = reorder_buf.as_mut().expect("pending implies Some");
                            if let Some(h) = hold {
                                rb.set_timeout(h);
                            }
                            rb.oldest_deadline()
                        }
                    };
                    deadline.map(|d| {
                        let remaining = d.saturating_duration_since(Instant::now());
                        tokio::time::Instant::now() + remaining
                    })
                } else {
                    None
                }
            };

            // ADR-0015: select between message arrival, in-order-hold expiry,
            // and shutdown signal
            let (path_id, msg) = tokio::select! {
                msg = msg_rx.recv() => {
                    match msg {
                        Some(m) => m,
                        None => break, // channel closed
                    }
                }
                _ = async {
                    match reorder_deadline {
                        Some(d) => tokio::time::sleep_until(d).await,
                        None => std::future::pending().await,
                    }
                } => {
                    // Reliable window (RWM Phase A): never give up on a
                    // hole. Re-advertise the gap with a SACK-bearing
                    // WindowAck so the sender's targeted-retransmit /
                    // repair machinery races it until recovered — the
                    // hold-expiry force-delivery below is the EVICT
                    // policy's move and is structurally skipped here.
                    if recv_window_reliable {
                        last_hole_nack_at = Instant::now();
                        let sack_ranges = received_sack_ranges(
                            &received_seqs,
                            highest_delivered_seq,
                            highest_seen_seq,
                        );
                        debug!(
                            delivered = highest_delivered_seq,
                            seen = highest_seen_seq,
                            ranges = sack_ranges.len(),
                            "reliable window: hole stalled — re-advertising gap"
                        );
                        let ack_msg = ControlMessage::WindowAck {
                            received_up_to: highest_delivered_seq,
                            sack_ranges,
                            echo_send_timestamp_us: 0,
                            jitter_us: 0,
                            cumulative_received: 0,
                        };
                        for pid in recv_scheduler.lock().live_paths() {
                            let _ = recv_transport.send_control_datagram(pid, ack_msg.clone());
                        }
                        continue;
                    }
                    // Give up on the hole(s): force-deliver expired entries
                    // (plus everything they unblock) so the tunnel never
                    // stalls on an unrecoverable block/symbol.
                    if block_inorder_enabled {
                        let expired = block_reorder.lock().drain_expired(Instant::now());
                        for (bid, bdata) in expired {
                            let held_ms = block_held_at
                                .lock()
                                .remove(&bid)
                                .map(|t| t.elapsed().as_millis() as u64);
                            debug!(block_id = bid, held_ms, "in-order hold expired — force-delivering");
                            for pkt_data in framing::extract_packets(&bdata) {
                                let _ = recv_tun_tx.try_send(Bytes::from(pkt_data));
                            }
                        }
                    } else if let Some(ref mut reorder) = reorder_buf {
                        let expired = reorder.drain_expired(Instant::now());
                        for (dseq, ddata) in expired {
                            debug!(seq = dseq, "window hold expired — force-delivering");
                            let packets: Vec<Vec<u8>> = if window_packed {
                                framing::extract_packets(&ddata)
                            } else {
                                framing::extract_window_packet(&ddata).into_iter().collect()
                            };
                            for pkt_data in packets {
                                let _ = recv_tun_tx.try_send(Bytes::from(pkt_data));
                            }
                            if dseq > highest_delivered_seq {
                                highest_delivered_seq = dseq;
                            }
                        }
                        // Advertise the advanced cumulative point to the
                        // PEER so its sender-side ack state (retransmit
                        // buffer, window advance) opens even with no
                        // further arrivals (the deadlock cycle above) —
                        // send a bare WindowAck now in case none comes.
                        if highest_delivered_seq > last_advertised_ack {
                            last_advertised_ack = highest_delivered_seq;
                            let ack_msg = ControlMessage::WindowAck {
                                received_up_to: highest_delivered_seq,
                                sack_ranges: Vec::new(),
                                echo_send_timestamp_us: 0,
                                jitter_us: 0,
                                cumulative_received: 0,
                            };
                            for pid in recv_scheduler.lock().live_paths() {
                                let _ = recv_transport.send_control_datagram(pid, ack_msg.clone());
                            }
                        }
                    }
                    continue;
                }
                _ = recv_shutdown_rx.recv() => {
                    info!("receiver shutting down");
                    break;
                }
            };
            match msg {
                WireMessage::Data(batch) => {
                    let batch_send_ts = batch.send_timestamp_us;
                    let batch_seq = batch.batch_seq;
                    let batch_path_id = batch.path_id;
                    let symbol_count = batch.symbols.len() as u32;

                    // Touch path as keepalive (received data = path is alive)
                    recv_scheduler.lock().touch_path(path_id);

                    // Record arrival for RTCP-style jitter calculation
                    {
                        let arrival_us = now_us();
                        let mut sched = recv_scheduler.lock();
                        if let Some(path) = sched.path_mut(path_id) {
                            path.estimator.record_arrival(batch_send_ts, arrival_us);
                            // Update jitter in monitoring stats
                            if let Some(ps) = recv_stats.path(path_id) {
                                ps.jitter_us.store(path.estimator.jitter_us() as u64, Ordering::Relaxed);
                            }
                        }
                    }

                    // Track batch sequences for loss detection (ADR-0003)
                    let (expected, _received_total) = {
                        let mut tracker = recv_path_tracking
                            .entry(path_id)
                            .or_insert_with(PathBatchTracker::new);
                        tracker.record_batch(batch_seq, symbol_count)
                    };

                    // Route symbols to window decoder or block decoder
                    if let Some(ref mut win_dec) = window_decoder {
                        // ----- Window-mode receive path -----
                        for symbol in &batch.symbols {
                            let recovered = win_dec.add_symbol(symbol);
                            for (seq, sym_data) in recovered {
                                received_seqs.insert(seq);
                                if seq > highest_seen_seq {
                                    highest_seen_seq = seq;
                                }

                                // Route through reorder buffer if available
                                let deliverable = if let Some(ref mut reorder) = reorder_buf {
                                    reorder.push(seq, sym_data)
                                } else {
                                    vec![(seq, sym_data)]
                                };

                                for (dseq, ddata) in deliverable {
                                    // Extract packets: packed mode uses block-mode framing,
                                    // unpacked mode uses single-packet window framing.
                                    let packets: Vec<Vec<u8>> = if window_packed {
                                        framing::extract_packets(&ddata)
                                    } else {
                                        framing::extract_window_packet(&ddata)
                                            .into_iter()
                                            .collect()
                                    };
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
                                    if dseq > highest_delivered_seq {
                                        highest_delivered_seq = dseq;
                                    }
                                }
                            }
                        }

                        // Drain expired reorder buffer entries.
                        // SRTT-adaptive hold (same delivery contract as
                        // block mode): the static 20ms default sat below
                        // one C2 NACK/repair round, so holes were force-
                        // delivered just before their repair arrived and
                        // the inner TCP retransmitted them (measured:
                        // realtime C2 502 retransmits / 44 SACK recoveries
                        // / 8 RTOs per 5×1.8MB vs bulk's ~66/3/0 with the
                        // 4×SRTT hold).
                        if let Some(ref mut reorder) = reorder_buf {
                            let srtt = {
                                let sched = recv_scheduler.lock();
                                sched
                                    .live_paths()
                                    .into_iter()
                                    .filter_map(|pid| sched.path(pid).map(|p| p.srtt()))
                                    .max()
                            };
                            if let Some(s) = srtt {
                                reorder.set_timeout(
                                    (s * 4).clamp(BLOCK_REORDER_MIN_HOLD, BLOCK_REORDER_MAX_HOLD),
                                );
                            }
                            let expired = reorder.drain_expired(Instant::now());
                            for (dseq, ddata) in expired {
                                let packets: Vec<Vec<u8>> = if window_packed {
                                    framing::extract_packets(&ddata)
                                } else {
                                    framing::extract_window_packet(&ddata)
                                        .into_iter()
                                        .collect()
                                };
                                for pkt_data in packets {
                                    let _ = recv_tun_tx.try_send(Bytes::from(pkt_data));
                                }
                                if dseq > highest_delivered_seq {
                                    highest_delivered_seq = dseq;
                                }
                            }
                        }

                        // Send SACK-extended WindowAck to sender.
                        // P10b: ALSO send while the cumulative point is
                        // stalled on a hole but new (higher) seqs keep
                        // arriving — the dupack analog. The SACK ranges are
                        // the sender's only gap signal; without them a hole
                        // was repaired solely by proactive FEC or the
                        // hold-expiry force-delivery.
                        let cumulative_advanced =
                            highest_delivered_seq > last_advertised_ack;
                        let gap_report_due = highest_seen_seq > highest_delivered_seq
                            && highest_seen_seq > last_gap_ack_seen
                            && last_gap_ack_time.elapsed() >= GAP_ACK_MIN_INTERVAL;
                        if cumulative_advanced || gap_report_due {
                            last_advertised_ack = highest_delivered_seq;
                            last_gap_ack_seen = highest_seen_seq;
                            last_gap_ack_time = Instant::now();
                            // A gap-bearing ack IS a hole re-advertisement:
                            // push the reliable-mode refresh timer out.
                            last_hole_nack_at = last_gap_ack_time;

                            // SACK ranges: what WAS received beyond the
                            // cumulative point (not what's missing).
                            let sack_ranges = received_sack_ranges(
                                &received_seqs,
                                highest_delivered_seq,
                                highest_seen_seq,
                            );

                            let jitter = {
                                let sched = recv_scheduler.lock();
                                sched.path(path_id)
                                    .map(|p| p.estimator.jitter_us() as u32)
                                    .unwrap_or(0)
                            };

                            let ack_msg = ControlMessage::WindowAck {
                                received_up_to: highest_delivered_seq,
                                sack_ranges,
                                echo_send_timestamp_us: batch_send_ts,
                                jitter_us: jitter,
                                cumulative_received: recv_stats.path(path_id)
                                    .map(|ps| ps.symbols_received.load(Ordering::Relaxed))
                                    .unwrap_or(0),
                            };
                            if let Err(e) = recv_transport.send_control_datagram(path_id, ack_msg) {
                                debug!(?e, path_id, "failed to send WindowAck");
                            }
                        }

                        // Periodic tasks (rate-limited by REPORT_INTERVAL)
                        // NACK sending replaced by SACK-extended WindowAck above.
                        let now = Instant::now();
                        if now.duration_since(last_nack_time) >= REPORT_INTERVAL
                            && highest_seen_seq > 0
                        {
                            last_nack_time = now;

                            // ADR-0035: PI feedback for window mode
                            if let Some(ref win_dec) = window_decoder {
                                let fed = win_dec.repairs_fed();
                                let useful = win_dec.repairs_useful();
                                let delta_fed = fed - last_pi_repairs_fed;
                                let delta_useful = useful - last_pi_repairs_useful;
                                if delta_fed > 0 {
                                    recv_fec.lock().feedback_update_window(delta_fed, delta_useful);
                                }
                                last_pi_repairs_fed = fed;
                                last_pi_repairs_useful = useful;
                            }

                            // Prune old entries from received_seqs tracking
                            // AND the window decoder's recovered/pivot/seen
                            // state (it was never advanced before — an
                            // unbounded leak over long streams). Everything
                            // below the delivered prefix minus two windows
                            // is decode-inert: repairs only reference the
                            // sender's current window, which sits at or
                            // above its ack (= our delivered point).
                            let prune_before = highest_delivered_seq.saturating_sub(MAX_WINDOW_SIZE as u64 * 2);
                            received_seqs = received_seqs.split_off(&prune_before);
                            if let Some(ref mut wd) = window_decoder {
                                wd.advance(prune_before);
                            }
                        }
                    } else {
                        // ----- Block-mode receive path (existing) -----
                        for symbol in &batch.symbols {
                            // Instrumentation (L2 ws1): per-path arrival counts.
                            // Debug-gated: the map update stays off the hot
                            // path unless composition logging is wanted.
                            if tracing::enabled!(tracing::Level::DEBUG)
                                && !completed_blocks.lock().1.contains(&symbol.block_id)
                            {
                                let mut arr = block_arrival.lock();
                                let entry = arr
                                    .entry(symbol.block_id)
                                    .or_insert_with(|| (Instant::now(), Default::default()));
                                *entry.1.entry(path_id).or_insert(0) += 1;
                                if arr.len() > 2048 {
                                    arr.clear(); // leak guard (failed blocks)
                                }
                            }
                            if !recv_decoders.contains_key(&symbol.block_id) {
                                // Late/spurious ARQ repair for a block that
                                // already decoded: drop, don't buffer (P8).
                                if completed_blocks.lock().1.contains(&symbol.block_id) {
                                    continue;
                                }
                                // Pre-BlockStart symbol: buffer for replay.
                                // (Creating a decoder without the real
                                // params here would make the block
                                // undecodable -- see pre_start_symbols.)
                                if pre_start_symbols.len() < 32
                                    || pre_start_symbols.contains_key(&symbol.block_id)
                                {
                                    let buf = pre_start_symbols
                                        .entry(symbol.block_id)
                                        .or_default();
                                    if buf.len() < 128 {
                                        buf.push(symbol.clone());
                                    }
                                }
                                continue;
                            }
                            if !feed_block_symbol(symbol, path_id) {
                                return;
                            }
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
                        batch_seq,
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
                    match recv_transport.send_control_datagram(path_id, ack) {
                        Err(e) => debug!(?e, path_id, "failed to send ACK datagram"),
                        Ok(()) => debug!(path_id, batch_seq, symbol_count, "ack sent"),
                    }
                }
                WireMessage::Control(ctrl_msg) => {
                    // Handle WindowStart packed flag in receiver loop
                    if let ControlMessage::WindowStart { packed, .. } = &ctrl_msg {
                        window_packed = *packed;
                    }

                    // Mid-stream backend switching was REMOVED (paper §16.4):
                    // no peer running this code sends WindowSwitch anymore,
                    // and acting on one (rebuilding the decoder mid-stream)
                    // is exactly the seq-space/state hazard that got the
                    // switch pinned off in P9a. Ignore it, loudly.
                    if let ControlMessage::WindowSwitch { flush_seq, new_backend, .. } = &ctrl_msg {
                        warn!(
                            flush_seq,
                            ?new_backend,
                            "ignoring WindowSwitch: mid-stream FEC backend switching \
                             was removed (codec is pinned at stream setup; paper §16.4)"
                        );
                    }

                    let started_block = match &ctrl_msg {
                        ControlMessage::BlockStart { params, .. } => Some(params.block_id),
                        _ => None,
                    };

                    // Re-announced BlockStart for a block we already delivered:
                    // the sender's success BlockResult was lost (best-effort
                    // datagram) so its idle re-announce keeps probing this
                    // block. Re-ack (idempotent) so it stops, and do NOT let
                    // handle_control_message re-create a zombie decoder for a
                    // done block (which the re-announce spares would then feed
                    // forever until the 30 s eviction). P8 idle-recovery.
                    if let Some(bid) = started_block {
                        if completed_blocks.lock().1.contains(&bid) {
                            let reack = ControlMessage::BlockResult {
                                block_id: bid,
                                success: true,
                                symbols_received: 0,
                                symbols_needed: 0,
                            };
                            let _ = recv_transport.send_control_datagram(path_id, reack);
                            pre_start_symbols.remove(&bid);
                            continue;
                        }
                    }

                    handle_control_message(
                        path_id,
                        ctrl_msg,
                        &recv_scheduler,
                        &recv_fec,
                        &recv_decoders,
                        &sent_counts,
                        &recv_transport,
                        recv_fec_backend,
                        &recv_stats,
                        recv_nack_tx.as_ref(),
                        if recv_window_mode { None } else { Some(&recv_block_arq) },
                        Some(&recv_batch_counter),
                        if recv_window_mode { Some(&recv_window_ack) } else { None },
                    );

                    // Replay symbols that outraced this BlockStart -- the
                    // decoder now exists with real params, and small blocks
                    // are often already complete at this point.
                    if let Some(bid) = started_block {
                        if let Some(buffered) = pre_start_symbols.remove(&bid) {
                            debug!(block_id = bid, count = buffered.len(),
                                "replaying pre-BlockStart symbols");
                            for sym in &buffered {
                                if !feed_block_symbol(sym, path_id) {
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    // ADR-0004: periodic cleanup of stale decoders
    let cleanup_decoders = active_decoders.clone();
    let cleanup_fec = fec_controller.clone();
    let cleanup_stats = stats.clone();
    let cleanup_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
        loop {
            interval.tick().await;
            let now = Instant::now();
            let mut timed_out = Vec::new();

            cleanup_decoders.retain(|block_id, decoder| {
                if now.duration_since(decoder.created_at()) > DECODER_TIMEOUT {
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
                // ADR-0013: update monitoring stats for timed-out blocks
                cleanup_stats.blocks.decoded_fail.fetch_add(timed_out.len() as u64, Ordering::Relaxed);
                warn!(
                    count = timed_out.len(),
                    "evicted timed-out decoders (block decode failures)"
                );
            }
        }
    });

    // Block-mode ARQ sweeper (P8): the Ack-diff path needs LATER acks on
    // the same path to reveal a lost batch; the tail of a transfer has
    // none, so a timeout sweep declares those batches delivered-or-lost at
    // SRTT timescale (mirrors the in_flight expiry) and repairs them.
    let sweep_block_arq = block_arq.clone();
    let sweep_scheduler = scheduler_arc.clone();
    let sweep_transport = transport_arc.clone();
    let sweep_stats = stats.clone();
    let sweep_batch_counter = batch_counter.clone();
    let sweep_window_mode = window_mode;
    let mut sweep_shutdown_rx = shutdown_tx.subscribe();
    let arq_sweep_handle = tokio::spawn(async move {
        if sweep_window_mode {
            // Window mode has its own SACK/NACK repair machinery — there is
            // no block-ARQ ledger to sweep. Park until shutdown instead of
            // returning: main()'s select! treats ANY task completing as
            // tunnel shutdown, and an instant return here tore the tunnel
            // down right after startup (L1 realtime bring-up failure).
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

            // Idle re-announce (P8, send-idle recovery): a lost BlockStart
            // orphans a block whose symbols were all delivered-and-acked — the
            // ledger is empty, so `sweep` above sees nothing, yet the block
            // never decodes. Re-send BlockStart + a small spare for any block
            // still retained (un-decoded) and quiet past the loss timeout. The
            // re-announce is driven by THIS timer (not TUN reads), so it fires
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
    });

    // Path management command channel (for runtime add/remove via HTTP API)
    let (path_cmd_tx, mut path_cmd_rx) = mpsc::channel::<crate::monitor::http::PathCommand>(16);

    // ADR-0013: spawn status HTTP endpoint if configured
    if let Some(addr) = config.status_addr {
        let http_stats = stats.clone();
        let http_cmd_tx = path_cmd_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::monitor::http::serve(http_stats, addr, http_cmd_tx).await {
                warn!(?e, "status HTTP endpoint failed");
            }
        });
    }

    // Path command processor: handles runtime add/remove of paths
    let cmd_transport = transport_arc.clone();
    let cmd_scheduler = scheduler_arc.clone();
    let cmd_stats = stats.clone();
    let cmd_msg_tx = msg_tx.clone();
    let cmd_ctrl_tx = ctrl_tx.clone();
    let next_path_id = Arc::new(AtomicU64::new(config.bind_addrs.len() as u64));
    let mut cmd_shutdown_rx = shutdown_tx.subscribe();
    let cmd_handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                cmd = path_cmd_rx.recv() => {
                    let cmd = match cmd {
                        Some(c) => c,
                        None => break,
                    };
                    match cmd {
                        crate::monitor::http::PathCommand::Add { bind_addr, peer_addr } => {
                            let path_id = next_path_id.fetch_add(1, Ordering::Relaxed) as u32;
                            info!(path_id, %bind_addr, ?peer_addr, "adding path at runtime");
                            match cmd_transport.add_path(path_id, bind_addr, peer_addr).await {
                                Ok(conn) => {
                                    cmd_scheduler.lock().add_path(path_id);
                                    cmd_stats.add_path(path_id);
                                    cmd_transport.spawn_receiver_for_path(
                                        path_id,
                                        conn,
                                        cmd_msg_tx.clone(),
                                        cmd_ctrl_tx.clone(),
                                    );
                                    info!(path_id, "path added successfully");
                                }
                                Err(e) => {
                                    warn!(path_id, ?e, "failed to add path");
                                }
                            }
                        }
                        crate::monitor::http::PathCommand::Remove { path_id } => {
                            info!(path_id, "removing path at runtime");
                            cmd_transport.remove_path(path_id);
                            cmd_scheduler.lock().remove_path(path_id);
                            info!(path_id, "path removed");
                        }
                    }
                }
                _ = cmd_shutdown_rx.recv() => break,
            }
        }
    });

    // RTCP-style periodic report + keepalive task
    let report_transport = transport_arc.clone();
    let report_scheduler = scheduler_arc.clone();
    let report_stats = stats.clone();
    let report_symbol_size = profile.symbol_size;
    let mut report_shutdown_rx = shutdown_tx.subscribe();
    let report_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(REPORT_INTERVAL);
        // P10a: local send-rate measurement state (per path): previous
        // symbols_sent counter and the last sample instant.
        let mut sent_prev: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
        let mut sent_prev_t = tokio::time::Instant::now();
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = report_shutdown_rx.recv() => break,
            }

            debug!("report tick");
            let reports: Vec<_> = {
            let mut sched = report_scheduler.lock();

            // P10a (paper 14.28): feed the estimator a LOCAL throughput
            // measurement — the achieved send rate over the report
            // interval. Production previously had NO local feed: the only
            // record_throughput call took the peer's PathReport value,
            // which is the peer's estimator.throughput() — circular, so
            // both sides sat at 0.0 forever and every throughput-gated
            // model term (t_sym: the 14.28 inner-feedback floor, the
            // 14.21 saturation cap, the 8.4 burst B/T term) was silently
            // sentinel-disabled on real links. The send rate is the right
            // t_sym semantics anyway: T_arq counts wire slots of the send
            // process the repairs are interleaved into.
            {
                let now_t = tokio::time::Instant::now();
                let dt = now_t.duration_since(sent_prev_t).as_secs_f64();
                if dt > 0.2 {
                    for pid in sched.all_path_ids() {
                        let sent = report_stats
                            .path(pid)
                            .map(|ps| ps.symbols_sent.load(Ordering::Relaxed))
                            .unwrap_or(0);
                        let prev = sent_prev.insert(pid, sent).unwrap_or(sent);
                        let delta = sent.saturating_sub(prev);
                        // Only feed while actually sending: an idle tunnel
                        // must not decay the operating-rate estimate to 0
                        // (t_sym would blow up and re-disable the floor).
                        if delta > 0 {
                            if let Some(path) = sched.path_mut(pid) {
                                let bps = delta as f64 * report_symbol_size as f64 / dt;
                                path.estimator.record_throughput(bps);
                            }
                        }
                    }
                    sent_prev_t = now_t;
                }
            }

            // Check for dead paths
            let deactivated = sched.check_dead_paths(DEAD_PATH_TIMEOUT);
            for pid in &deactivated {
                if let Some(ps) = report_stats.path(*pid) {
                    ps.active.store(false, Ordering::Relaxed);
                }
            }

            // Query and store MTU per path
            for pid in sched.all_path_ids() {
                if let Some(mtu) = report_transport.max_datagram_size(pid) {
                    if let Some(path) = sched.path_mut(pid) {
                        path.max_datagram_size = Some(mtu);
                    }
                }
            }

            // in_flight leak guard (backstop): time-based expiry
            // (PathState::expire_in_flight, RTT-timescale) is the primary
            // release for stranded budget; the 25% decay remains as a
            // last-resort backstop for anything the expiry can't see
            // (e.g. direct in_flight writes that bypassed the charge log).
            for pid in sched.all_path_ids() {
                if let Some(path) = sched.path_mut(pid) {
                    path.expire_in_flight();
                    if path.in_flight > path.cwnd {
                        path.in_flight -= path.in_flight / 4;
                    }
                }
            }

            // Send PathReport + Ping on each LIVE path (not active_paths:
            // that filters by spare cwnd, and a saturated path still needs
            // its liveness heartbeats — see Scheduler::live_paths).
            let path_ids = sched.live_paths();
            path_ids.iter().filter_map(|&pid| {
                let path = sched.path(pid)?;
                let ps = report_stats.path(pid)?;
                Some((pid, ControlMessage::PathReport {
                    path_id: pid,
                    loss_rate: path.estimator.loss_rate(),
                    avg_rtt_us: path.estimator.rtt().as_micros() as u64,
                    throughput_bps: path.estimator.throughput(),
                    jitter_us: path.estimator.jitter_us() as u64,
                    symbols_sent: ps.symbols_sent.load(Ordering::Relaxed),
                    symbols_received: ps.symbols_received.load(Ordering::Relaxed),
                }))
            }).collect()
            // guard dropped by scope end: the report sends below await on
            // the reliable stream and must not hold the scheduler lock
            };

            for (pid, report) in reports {
                // Liveness must not share fate with the data flood: under
                // load the datagram queue is saturated by symbol batches
                // and report datagrams get dropped, so the peer declares
                // the path dead after DEAD_PATH_TIMEOUT and QUIC idles out
                // (L1 finding: every bulk transfer killed the tunnel in
                // ~6 s). The reliable control stream has its own flow
                // control, so reports and pings survive saturation.
                // Hard deadline on control sends: this task also runs the
                // dead-path checker, so it must NEVER wedge (open_uni can
                // block indefinitely once stream credit is exhausted).
                match tokio::time::timeout(
                    Duration::from_millis(500),
                    report_transport.send_control(pid, report),
                )
                .await
                {
                    Err(_) => warn!(pid, "PathReport send timed out (stream credit?)"),
                    Ok(Err(e)) => warn!(pid, ?e, "failed to send PathReport on control stream"),
                    Ok(Ok(())) => {}
                }
                match tokio::time::timeout(
                    Duration::from_millis(500),
                    report_transport.send_control(pid, ControlMessage::Ping { timestamp_us: now_us() }),
                )
                .await
                {
                    Err(_) => warn!(pid, "Ping send timed out (stream credit?)"),
                    Ok(Err(e)) => warn!(pid, ?e, "failed to send Ping on control stream"),
                    Ok(Ok(())) => debug!(pid, "ping sent on control stream"),
                }
            }
        }
    });

    // Control fast path: liveness-critical messages (PathReport, Ping,
    // Pong) are handled immediately; anything else that arrives via the
    // reliable stream is forwarded to the ordered data loop.
    let ctrl_scheduler = scheduler_arc.clone();
    let ctrl_fec = fec_controller.clone();
    let ctrl_decoders = active_decoders.clone();
    let ctrl_transport = transport_arc.clone();
    let ctrl_stats = stats.clone();
    let ctrl_fec_backend = effective_fec_backend;
    let ctrl_forward_tx = msg_tx.clone();
    let ctrl_handle = tokio::spawn(async move {
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
                        &ctrl_scheduler,
                        &ctrl_fec,
                        &ctrl_decoders,
                        &ctrl_sent_counts,
                        &ctrl_transport,
                        ctrl_fec_backend,
                        &ctrl_stats,
                        None,
                        // The fast path only handles PathReport/Ping/Pong;
                        // Acks (which drive block ARQ) and WindowAcks go
                        // through the data loop, so neither the ledger nor
                        // the peer-ack atomic is needed here.
                        None,
                        None,
                        None,
                    );
                }
                other => {
                    // NEVER await into the data channel: under a symbol
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
    });

    // Any task completing — even cleanly — ends the tunnel, so every arm
    // must say WHICH task exited and why. A silent `_ = handle => {}` arm
    // hid the L1 realtime bring-up failure (arq_sweep returned instantly
    // in window mode and the tunnel shut down with no log line).
    tokio::select! {
        r = sender_handle => { log_task_exit("sender", &r); r?; }
        r = receiver_handle => { log_task_exit("receiver", &r); r?; }
        r = cleanup_handle => { log_task_exit("decoder-cleanup", &r); r?; }
        r = report_handle => { log_task_exit("path-report", &r); r?; }
        r = cmd_handle => { log_task_exit("path-cmd", &r); r?; }
        r = ctrl_handle => { log_task_exit("control-fastpath", &r); r?; }
        r = arq_sweep_handle => { log_task_exit("arq-sweep", &r); r?; }
    }

    // Clean up routes and DNS on shutdown
    for route in &managed_routes {
        routing::remove_route(route).await;
    }
    if let Some(ref dns) = managed_dns {
        routing::revert_dns(dns).await;
    }

    Ok(())
}

/// Log why a top-level tunnel task exited. main()'s select! treats any task
/// completing as tunnel shutdown, so the exit must never be silent: a panic
/// or cancellation is an error, a clean return is at least info-worthy.
fn log_task_exit(task: &str, r: &Result<(), tokio::task::JoinError>) {
    match r {
        Ok(()) => info!(task, "tunnel task exited — shutting down tunnel"),
        Err(e) if e.is_panic() => error!(task, %e, "tunnel task PANICKED — shutting down tunnel"),
        Err(e) => error!(task, %e, "tunnel task failed — shutting down tunnel"),
    }
}

// ReorderBuffer extracted to src/net/reorder.rs

// ---------------------------------------------------------------------------
// WindowNack gap computation
// ---------------------------------------------------------------------------

/// Compute gap ranges from a set of received sequences in a window.
/// Returns Vec<(start, end)> of inclusive ranges of missing sequences.
pub fn compute_gap_ranges(
    received: &BTreeSet<u64>,
    window_start: u64,
    window_end: u64,
) -> Vec<(u64, u64)> {
    let mut gaps = Vec::new();
    let mut expected = window_start;

    for &seq in received.range(window_start..=window_end) {
        if seq > expected {
            gaps.push((expected, seq - 1));
            if gaps.len() >= MAX_NACK_GAPS {
                return gaps;
            }
        }
        expected = seq + 1;
    }

    // Trailing gap
    if expected <= window_end && gaps.len() < MAX_NACK_GAPS {
        gaps.push((expected, window_end));
    }

    gaps
}

/// Invert SACK ranges into missing-seq gaps (P10b).
///
/// `sack_ranges` are inclusive, ascending, disjoint ranges of seqs the
/// receiver HAS beyond the cumulative point `received_up_to`. Every seq
/// between the cumulative point and a sacked range that is not itself
/// sacked is missing at the receiver. (Seqs above the last sacked range
/// are NOT reported — they may simply still be in flight.)
pub fn sack_to_gaps(received_up_to: u64, sack_ranges: &[(u64, u64)]) -> Vec<(u64, u64)> {
    let mut gaps = Vec::new();
    let mut expected = received_up_to + 1;
    for &(start, end) in sack_ranges {
        if start > expected {
            gaps.push((expected, start - 1));
            if gaps.len() >= MAX_NACK_GAPS {
                return gaps;
            }
        }
        expected = expected.max(end.saturating_add(1));
    }
    gaps
}

/// Receiver-side SACK encoding: the inclusive, ascending, disjoint ranges
/// of seqs the receiver HAS in (`delivered`, `seen`] — the inverse of
/// [`sack_to_gaps`]. Shared by the data-arm WindowAck and the reliable
/// window's stalled-hole re-advertisement (RWM Phase A).
pub fn received_sack_ranges(
    received: &BTreeSet<u64>,
    delivered: u64,
    seen: u64,
) -> Vec<(u64, u64)> {
    let gaps = compute_gap_ranges(received, delivered, seen);
    let mut sack_ranges = Vec::new();
    let mut cursor = delivered + 1;
    for &(gap_start, gap_end) in &gaps {
        if cursor < gap_start {
            sack_ranges.push((cursor, gap_start - 1));
        }
        cursor = gap_end + 1;
    }
    if cursor <= seen {
        sack_ranges.push((cursor, seen));
    }
    sack_ranges
}

/// Select the best source path for a window-mode symbol: lowest RTT with capacity.
/// Falls back to path 0 if no active paths.
fn select_source_path(scheduler: &Scheduler) -> u32 {
    scheduler.best_source_path().unwrap_or(0)
}

/// Select the best repair path for a window-mode symbol: highest goodput with capacity.
/// Falls back to `fallback` if no active paths.
fn select_repair_path(scheduler: &Scheduler, fallback: u32) -> u32 {
    scheduler.best_repair_path().unwrap_or(fallback)
}

/// Select the best repair path while avoiding a specific path (cross-path diversity).
/// Falls back to any available repair path if no alternative exists.
fn select_repair_path_avoiding(scheduler: &Scheduler, avoid: u32, fallback: u32) -> u32 {
    scheduler.best_repair_path_avoiding(avoid).unwrap_or(fallback)
}

/// Sliding-window sender loop. Reads packets from TUN, frames them as individual
/// source symbols, sends them immediately, and periodically generates repair symbols.
async fn run_window_sender(
    tun: &mut TunInterface,
    symbol_size: u16,
    fec_backend: FecBackend,
    fec_controller: &Arc<parking_lot::Mutex<FecRateController>>,
    batch_counter: &AtomicU64,
    transport: &Arc<QuicTransport>,
    scheduler: &Arc<parking_lot::Mutex<Scheduler>>,
    stats: &Arc<SharedStats>,
    window_ack_seq: &Arc<AtomicU64>,
    nack_rx: &mut tokio::sync::mpsc::Receiver<Vec<(u64, u64)>>,
    shutdown_rx: &mut tokio::sync::broadcast::Receiver<()>,
    protocol_hint: ProtocolHint,
    // RWM Phase A: RETAIN-UNTIL-ACKED retention at the ARQ layer (see the
    // policy block above RELIABLE_STORE_MAX).
    reliable: bool,
) {
    // Codec pinned at startup (§16.4) — created once, never rebuilt.
    let mut encoder: Box<dyn WindowEncoder> =
        create_window_encoder(fec_backend, symbol_size, fec_controller, scheduler);
    let mut prev_ack: u64 = 0;
    // Fractional repair accumulator: tracks sub-symbol repair debt.
    // Driven by TaperFunction density when GE data is available,
    // falls back to flat rate from compute_repair_rate_capped.
    let mut repair_debt: f64 = 0.0;
    // Source symbol counter for taper time offset (symbols since window start).
    let mut taper_offset: u64 = 0;
    /// Congestion-aware NACK repair throttle (ADR-0046).
    let mut nack_congestion = NackCongestionState::new();
    /// Maps source seq → path it was sent on (for cross-path retransmission).
    let mut source_path_map: std::collections::HashMap<u64, u32> = std::collections::HashMap::new();
    /// Last source path used (for NACK repair path selection outside the send macro).
    let mut last_source_path: u32 = 0;
    /// NACK repairs sent in the current reporting period (ADR-0050 budget tracking).
    let mut nack_repairs_this_period: u64 = 0;
    /// Source symbols sent in the current reporting period.
    let mut source_symbols_this_period: u64 = 0;
    /// P10b: seq → last NACK-retransmit time (µs). Repeated gap acks for the
    /// same hole (they arrive every GAP_ACK_MIN_INTERVAL while it persists)
    /// must not resend the symbol more than once per SRTT — but MAY resend
    /// after an SRTT, which escalates naturally if the retransmit itself dies.
    let mut nack_retx_at: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
    /// P10b: cached ADR-0046/0050 budget state, refreshed every
    /// NACK_REPAIR_COOLDOWN_US (gap acks arrive far more often than the
    /// budget inputs move; recomputing per ack would just churn locks).
    let mut last_budget_refresh_us: u64 = 0;
    let mut cached_max_repairs: u64 = MAX_NACK_REPAIRS_PER_NACK as u64;
    let mut cached_nack_budget: u64 = MAX_NACK_REPAIRS_PER_NACK as u64;
    /// P10b: when the tail sweep last FIRED (µs) — rearm point. Must advance
    /// on every fire even if the retransmit was skipped (cooldown/budget
    /// exhausted), or a past deadline keeps the timer arm permanently ready
    /// and the select! busy-spins, starving TUN reads.
    let mut last_tail_sweep_us: u64 = 0;

    /// Retransmit buffer: maps seq → (send_time_us, epsilon_at_send, path_id).
    /// Used for P_lost-based retransmit decisions. Symbols are removed on ACK.
    /// METADATA only — under EVICT the source bytes die with window eviction.
    let mut retransmit_buffer: std::collections::BTreeMap<u64, (u64, f64, u32)> = std::collections::BTreeMap::new();

    /// RWM Phase A sent-data store (reliable mode only): seq → the exact
    /// source WireSymbol as sent. This is the retention contract — bytes
    /// retained until the peer's cumulative ack passes them (removal by ack
    /// ONLY), so an aged SACK-confirmed hole that slid out of the coding
    /// window is recovered by a targeted retransmit of exactly this symbol.
    /// Bounded by RELIABLE_STORE_MAX via TUN-read backpressure, never by
    /// eviction.
    let mut sent_store: BTreeMap<u64, crate::fec::WireSymbol> = BTreeMap::new();

    // Symbol packer: accumulate small packets into packed symbols for Realtime mode
    let use_packing = protocol_hint == ProtocolHint::Realtime;
    let mut packer = framing::SymbolPacker::new(symbol_size, std::time::Duration::from_millis(1));

    // Announce window mode to peer on all paths
    {
        let sched = scheduler.lock();
        for pid in sched.active_paths() {
            let _ = transport.send_control_datagram(
                pid,
                ControlMessage::WindowStart { symbol_size, backend: fec_backend, packed: use_packing },
            );
        }
    }

    // Helper macro: feed a framed symbol to encoder + send + stats + repair debt
    macro_rules! send_source_symbol {
        ($framed:expr) => {{
            let wire_sym = encoder.add_source(&$framed);

            // RWM Phase A retention: the store keeps the sent bytes until
            // the peer acks them — the coding window may slide past this
            // symbol, but the data can no longer be destroyed by eviction.
            if reliable {
                sent_store.insert(wire_sym.block_id, wire_sym.clone());
            }

            // Send source symbol — pick best path by lowest RTT with capacity
            let source_path = {
                let sched = scheduler.lock();
                select_source_path(&sched)
            };
            last_source_path = source_path;
            let batch_seq = batch_counter.fetch_add(1, Ordering::Relaxed);
            let batch = SymbolBatch {
                symbols: vec![wire_sym.clone()],
                send_timestamp_us: now_us(),
                batch_seq,
                path_id: source_path,
            };
            if let Err(e) = transport.send_symbols(source_path, batch) {
                warn!(source_path, ?e, "failed to send window source symbol");
            }
            {
                let mut sched = scheduler.lock();
                if let Some(p) = sched.path_mut(source_path) {
                    p.charge_in_flight(1);
                }
            }
            if let Some(ps) = stats.path(source_path) {
                ps.symbols_sent.fetch_add(1, Ordering::Relaxed);
            }
            stats.fec.total_source_symbols.fetch_add(1, Ordering::Relaxed);
            source_symbols_this_period += 1;

            // Track which path this source was sent on (for cross-path retransmission)
            source_path_map.insert(wire_sym.block_id, source_path);

            // Add to retransmit buffer for P_lost-based retransmit decisions
            {
                let epsilon = {
                    let sched = scheduler.lock();
                    sched.active_paths().iter()
                        .filter_map(|id| sched.path(*id))
                        .max_by(|a, b| a.estimator.loss_rate().partial_cmp(&b.estimator.loss_rate()).unwrap_or(std::cmp::Ordering::Equal))
                        .map(|p| p.estimator.loss_rate())
                        .unwrap_or(0.0)
                };
                retransmit_buffer.insert(wire_sym.block_id, (now_us(), epsilon, source_path));
                // Track correction deficit: this symbol needs epsilon coverage
                let mut sched = scheduler.lock();
                sched.deficit.on_send(wire_sym.block_id, source_path, epsilon);
            }

            // Redundant send for Realtime: duplicate source on second-best path
            if protocol_hint == ProtocolHint::Realtime {
                let alt_path = {
                    let sched = scheduler.lock();
                    sched.redundant_source_path(source_path)
                };
                if let Some(alt) = alt_path {
                    let batch_seq = batch_counter.fetch_add(1, Ordering::Relaxed);
                    let batch = SymbolBatch {
                        symbols: vec![wire_sym],
                        send_timestamp_us: now_us(),
                        batch_seq,
                        path_id: alt,
                    };
                    if let Err(e) = transport.send_symbols(alt, batch) {
                        warn!(alt, ?e, "failed to send redundant source symbol");
                    }
                    {
                        let mut sched = scheduler.lock();
                        if let Some(p) = sched.path_mut(alt) {
                            p.charge_in_flight(1);
                        }
                    }
                    if let Some(ps) = stats.path(alt) {
                        ps.symbols_sent.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }

            // Taper-driven repair accumulator with cwnd budget gate (ADR-0050).
            // Uses TaperFunction density τ(t) = A×(1-q)^t when GE data is available,
            // capped by spare capacity. Falls back to flat rate otherwise.
            if encoder.window_size() > 1 {
                let repair_rate = {
                    let ctrl = fec_controller.lock();
                    let sched = scheduler.lock();
                    let spare = sched.spare_capacity();
                    let path_estimator = sched
                        .active_paths()
                        .iter()
                        .filter_map(|id| sched.path(*id))
                        .max_by(|a, b| a.estimator.loss_rate().partial_cmp(&b.estimator.loss_rate()).unwrap_or(std::cmp::Ordering::Equal))
                        .map(|p| &p.estimator);
                    match path_estimator {
                        Some(est) => {
                            let flat_rate = ctrl.compute_repair_rate_capped(est, spare, encoder.window_size());
                            // Use taper density at current offset if GE model is valid
                            let taper = crate::control::TaperFunction::from_estimator(est, flat_rate);
                            let density = taper.density(taper_offset as f64);
                            // Cap by spare capacity (never exceed link headroom)
                            density.min(spare.max(0.0))
                        }
                        None => 0.0,
                    }
                };
                repair_debt += repair_rate;
                taper_offset += 1;

                while repair_debt >= 1.0 && encoder.window_size() > 0 {
                    repair_debt -= 1.0;

                    // P_lost-based correction symbol decision:
                    // Check oldest un-ACKed symbol in retransmit buffer.
                    // If P_lost is high enough, retransmit it (immediate decode).
                    // Otherwise, generate a new repair symbol (FEC).
                    let correction_sym = {
                        let now = now_us();
                        let (srtt_secs, rttvar_secs, epsilon) = {
                            let sched = scheduler.lock();
                            let worst = sched.active_paths().iter()
                                .filter_map(|id| sched.path(*id))
                                .max_by(|a, b| a.estimator.loss_rate().partial_cmp(&b.estimator.loss_rate()).unwrap_or(std::cmp::Ordering::Equal));
                            match worst {
                                Some(p) => (p.estimator.rtt().as_secs_f64(), p.estimator.rtt().as_secs_f64() * 0.1, p.estimator.loss_rate()),
                                None => (0.05, 0.005, 0.0),
                            }
                        };

                        // Find oldest retransmit candidate and compute P_lost
                        let mut use_retransmit = false;
                        let mut retransmit_seq = 0u64;
                        if let Some((&seq, &(send_time_us, eps_at_send, _path))) = retransmit_buffer.iter().next() {
                            let age_secs = (now.saturating_sub(send_time_us)) as f64 / 1_000_000.0;
                            let p = crate::control::fec_rate::p_lost(age_secs, eps_at_send, srtt_secs, rttvar_secs);
                            // Paper Section 3.4: P(retransmit) = P_lost(t_k).
                            // Probabilistic — smooth transition from FEC to ARQ.
                            if rand::random::<f64>() < p {
                                use_retransmit = true;
                                retransmit_seq = seq;
                            }
                        }

                        if use_retransmit {
                            // Retransmit: exact source symbol — from the
                            // sent-data store (reliable: survives window
                            // eviction) or the encoder window (EVICT).
                            sent_store
                                .get(&retransmit_seq)
                                .cloned()
                                .or_else(|| encoder.get_source(retransmit_seq))
                                .unwrap_or_else(|| encoder.generate_repair())
                        } else {
                            // Repair: generate a new FEC symbol
                            encoder.generate_repair()
                        }
                    };

                    let correction_path = {
                        let sched = scheduler.lock();
                        select_repair_path(&sched, source_path)
                    };
                    let batch_seq = batch_counter.fetch_add(1, Ordering::Relaxed);
                    let batch = SymbolBatch {
                        symbols: vec![correction_sym],
                        send_timestamp_us: now_us(),
                        batch_seq,
                        path_id: correction_path,
                    };
                    if let Err(e) = transport.send_symbols(correction_path, batch) {
                        warn!(correction_path, ?e, "failed to send correction symbol");
                    }
                    {
                        let mut sched = scheduler.lock();
                        if let Some(p) = sched.path_mut(correction_path) {
                            p.charge_in_flight(1);
                        }
                    }
                    if let Some(ps) = stats.path(correction_path) {
                        ps.symbols_sent.fetch_add(1, Ordering::Relaxed);
                    }
                    stats.fec.total_repair_symbols.fetch_add(1, Ordering::Relaxed);
                }
            }
        }};
    }

    // Retention backpressure state (reliable mode), for edge-triggered logs.
    let mut last_tx_paused = false;

    loop {
        // Determine if packer has pending data for flush timer
        let packer_pending = use_packing && packer.is_pending();

        // RWM Phase A backpressure: when the sent-data store is full of
        // un-acked symbols, stop reading the TUN — the inner flow sees the
        // growing TUN queue and slows down (flow control), and this loop
        // keeps servicing acks/NACKs/tail sweeps so the store drains.
        // Retention is never released by pressure, only by acks.
        let tx_paused = store_backpressure(reliable, sent_store.len());
        if tx_paused != last_tx_paused {
            debug!(
                tx_paused,
                store_len = sent_store.len(),
                "reliable-window backpressure state change"
            );
            last_tx_paused = tx_paused;
        }

        // P10b: gap reports must wake this loop even when the TUN is idle.
        // The inner TCP stalls exactly when a hole blocks delivery — no new
        // TUN packets — and the old structure only drained the NACK channel
        // after a TUN read, so repairs stalled precisely when they were the
        // only thing that could unstall the tunnel.
        let mut pending_gaps: Option<Vec<(u64, u64)>> = None;

        // P10b tail sweep: the LAST symbols of a burst have no successors,
        // so the receiver can never SACK a gap behind them — the sender must
        // detect that stall itself (block mode's P8 ARQ sweeper analog).
        // When un-ACKed symbols exist, arm a timer at oldest-activity +
        // 2×SRTT; on expiry synthesize a gap report for the cumulative
        // blocker (per-seq cooldown + budgets all apply downstream).
        let tail_deadline: Option<tokio::time::Instant> =
            retransmit_buffer.iter().next().map(|(&seq, &(send_us, _, _))| {
                let last_activity_us = nack_retx_at
                    .get(&seq)
                    .map_or(send_us, |&r| r.max(send_us))
                    .max(last_tail_sweep_us);
                let srtt_us = {
                    let sched = scheduler.lock();
                    sched
                        .active_paths()
                        .iter()
                        .filter_map(|id| sched.path(*id))
                        .map(|p| p.estimator.rtt().as_micros() as u64)
                        .max()
                        .unwrap_or(NACK_RETX_COOLDOWN_FLOOR_US)
                };
                let timeout_us = (srtt_us * 2).clamp(TAIL_SWEEP_MIN_US, TAIL_SWEEP_MAX_US);
                let deadline_us = last_activity_us + timeout_us;
                let remaining = Duration::from_micros(deadline_us.saturating_sub(now_us()));
                tokio::time::Instant::now() + remaining
            });

        let packet = tokio::select! {
            // Backpressure poll (reliable): with TUN reads gated off, wake
            // at ack timescale to observe store drain via the ack path
            // below (mirrors the block sender's 1 ms backpressure poll).
            _ = tokio::time::sleep(Duration::from_millis(1)), if tx_paused => None,
            p = tun.read_packet(), if !tx_paused => Some(p),
            gaps = nack_rx.recv() => {
                if let Some(g) = gaps {
                    pending_gaps = Some(g);
                }
                None
            }
            _ = async {
                match tail_deadline {
                    Some(d) => tokio::time::sleep_until(d).await,
                    None => std::future::pending().await,
                }
            } => {
                last_tail_sweep_us = now_us();
                if let Some((&seq, _)) = retransmit_buffer.iter().next() {
                    debug!(seq, "tail ARQ sweep — retransmitting cumulative blocker");
                    pending_gaps = Some(vec![(seq, seq)]);
                }
                None
            }
            _ = shutdown_rx.recv() => {
                // Flush any remaining packed data before shutdown
                if use_packing {
                    if let Some(packed) = packer.flush() {
                        send_source_symbol!(packed);
                    }
                }
                // Send Shutdown on all paths
                let sched = scheduler.lock();
                for pid in sched.active_paths() {
                    let _ = transport.send_control_datagram(pid, ControlMessage::Shutdown);
                }
                info!("window sender shut down gracefully");
                return;
            }
            _ = tokio::time::sleep(packer.time_until_flush()), if packer_pending => {
                // Flush timeout expired — emit partial packed symbol
                if let Some(packed) = packer.flush() {
                    send_source_symbol!(packed);
                }
                None
            }
        };

        if let Some(packet) = packet {
            let pkt = match packet {
                Some(p) => p,
                None => {
                    // Flush remaining packed data before exit
                    if use_packing {
                        if let Some(packed) = packer.flush() {
                            send_source_symbol!(packed);
                        }
                    }
                    info!("TUN closed");
                    return;
                }
            };

            if use_packing {
                // Pack multiple small packets into one symbol
                if let Some(packed) = packer.push(&pkt) {
                    send_source_symbol!(packed);
                }
            } else {
                // Legacy: one packet per symbol (padded)
                let framed = framing::frame_window_packet(&pkt, symbol_size);
                send_source_symbol!(framed);
            }
        }

        // Process gap reports → retransmit exact source symbols + repair margin.
        // ADR-0046 congestion backoff + ADR-0050 budget state is refreshed at
        // NACK_REPAIR_COOLDOWN_US cadence; processing itself is NOT gated on
        // the cadence — per-seq cooldowns already bound the send rate, and
        // delaying a repair round costs a reorder-hold expiry at the receiver.
        let now_repair_us = now_us();
        if now_repair_us.saturating_sub(last_budget_refresh_us) >= NACK_REPAIR_COOLDOWN_US {
            last_budget_refresh_us = now_repair_us;
            // Update congestion state from scheduler
            let (current_loss, current_rtt) = {
                let sched = scheduler.lock();
                let worst = sched
                    .active_paths()
                    .iter()
                    .filter_map(|id| sched.path(*id))
                    .max_by(|a, b| {
                        a.estimator
                            .loss_rate()
                            .partial_cmp(&b.estimator.loss_rate())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                match worst {
                    Some(p) => (p.estimator.loss_rate(), p.copa_min_rtt()),
                    None => (0.0, None),
                }
            };
            let nack_multiplier = nack_congestion.update(current_loss, current_rtt);
            cached_max_repairs =
                (MAX_NACK_REPAIRS_PER_NACK as f64 * nack_multiplier).round() as u64;

            // ADR-0050: compute NACK budget from BudgetAllocator
            cached_nack_budget = {
                let ctrl = fec_controller.lock();
                let sched = scheduler.lock();
                let worst_est = sched
                    .active_paths()
                    .iter()
                    .filter_map(|id| sched.path(*id))
                    .max_by(|a, b| a.estimator.loss_rate().partial_cmp(&b.estimator.loss_rate()).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|p| &p.estimator);
                match worst_est {
                    Some(est) => {
                        let p_upper = est.predictive_loss_upper(1.0 - ctrl.target_tail_loss());
                        let nack_eff = est.nack_effectiveness();
                        let budget = crate::control::fec_rate::BudgetAllocator::compute(
                            p_upper, ctrl.codec_overhead(), current_loss * 0.5, nack_eff,
                        );
                        let nack_cap_symbols = (budget.nack_cap() * source_symbols_this_period as f64) as u64;
                        // P10b: floor at one full repair burst per refresh
                        // interval. The raw cap is nack_cap (≈ loss_rate/2)
                        // × sources-this-period, but the period resets every
                        // 10 acked seqs, so the u64 cast truncated it to 0
                        // almost always — silently suppressing the entire
                        // reactive repair path. Congestion safety lives
                        // in the ADR-0046 multiplier (cached_max_repairs),
                        // which can still zero out repairs under real
                        // congestion; this floor only guarantees wireless-
                        // loss repairs are never starved by quantization.
                        // (L1 C2: floor+sweep took 287 → 38 inner
                        // retransmits per 5×1.8MB.)
                        nack_cap_symbols
                            .saturating_sub(nack_repairs_this_period)
                            .max(MAX_NACK_REPAIRS_PER_NACK as u64)
                    }
                    None => cached_max_repairs,
                }
            };
        }

        loop {
            let gaps = match pending_gaps.take() {
                Some(g) => g,
                None => match nack_rx.try_recv() {
                    Ok(g) => g,
                    Err(_) => break,
                },
            };
            if cached_max_repairs == 0 || cached_nack_budget == 0 {
                // Fully suppressed or budget exhausted — drain NACK queue
                continue;
            }

            // SRTT drives the per-seq retransmit cooldown and the age gate.
            let srtt_us = {
                let sched = scheduler.lock();
                sched
                    .active_paths()
                    .iter()
                    .filter_map(|id| sched.path(*id))
                    .map(|p| p.estimator.rtt().as_micros() as u64)
                    .max()
                    .unwrap_or(NACK_RETX_COOLDOWN_FLOOR_US)
            };
            let retx_cooldown_us = srtt_us.max(NACK_RETX_COOLDOWN_FLOOR_US);

            let (win_start, win_end) = encoder.window_span();
            let mut retransmitted: u64 = 0;
            let mut nacked_count: u64 = 0;

            'gaps: for &(gap_start, gap_end) in &gaps {
                // EVICT: only the coding window can serve a gap — older
                // seqs are gone. RETAIN: the sent-data store serves ANY
                // un-acked seq (targeted ARQ for holes that aged out of
                // the FEC horizon), so gaps are not clamped to the window.
                let (clamped_start, clamped_end) = if reliable {
                    (gap_start, gap_end)
                } else {
                    (gap_start.max(win_start), gap_end.min(win_end))
                };
                if clamped_start > clamped_end {
                    continue;
                }
                nacked_count += clamped_end - clamped_start + 1;

                for seq in clamped_start..=clamped_end {
                    if retransmitted >= cached_max_repairs || cached_nack_budget == 0 {
                        break 'gaps;
                    }
                    // Per-seq cooldown: repeated gap acks for the same
                    // hole must not resend more than once per SRTT.
                    if let Some(&last) = nack_retx_at.get(&seq) {
                        if now_repair_us.saturating_sub(last) < retx_cooldown_us {
                            continue;
                        }
                    }
                    // Age gate: cross-path/jitter skew can report a seq
                    // that is merely late, not lost — only repair
                    // symbols old enough that an in-flight copy would
                    // already have been sacked.
                    if let Some(&(send_time_us, _, _)) = retransmit_buffer.get(&seq) {
                        if now_repair_us.saturating_sub(send_time_us) < srtt_us / 2 {
                            continue;
                        }
                    }
                    // Cross-path: avoid the path that originally carried this symbol
                    let original_path = source_path_map.get(&seq).copied().unwrap_or(last_source_path);
                    let nack_path = {
                        let sched = scheduler.lock();
                        select_repair_path_avoiding(&sched, original_path, last_source_path)
                    };

                    // Exact source retransmission first — reliable mode
                    // serves from the sent-data store (survives window
                    // eviction; a stale gap for an already-acked seq has
                    // nothing to serve and is skipped) — else fall back
                    // to the encoder window, then to a fungible repair.
                    let sym = if reliable {
                        match sent_store.get(&seq) {
                            Some(s) => s.clone(),
                            // Not in the store ⇒ already acked (removal is
                            // by ack only): the receiver has it; skip.
                            None => continue,
                        }
                    } else {
                        encoder.get_source(seq).unwrap_or_else(|| encoder.generate_repair())
                    };

                    let batch_seq = batch_counter.fetch_add(1, Ordering::Relaxed);
                    let batch = SymbolBatch {
                        symbols: vec![sym],
                        send_timestamp_us: now_us(),
                        batch_seq,
                        path_id: nack_path,
                    };
                    if let Err(e) = transport.send_symbols(nack_path, batch) {
                        warn!(nack_path, ?e, "failed to send NACK retransmission");
                    }
                    debug!(seq, nack_path, "SACK-gap retransmit");
                    nack_retx_at.insert(seq, now_repair_us);
                    stats.fec.total_repair_symbols.fetch_add(1, Ordering::Relaxed);
                    nack_repairs_this_period += 1;
                    cached_nack_budget = cached_nack_budget.saturating_sub(1);
                    retransmitted += 1;
                }
            }

            // Repair margin: extra repairs proportional to loss rate
            if retransmitted > 0 {
                let current_loss = {
                    let sched = scheduler.lock();
                    sched
                        .active_paths()
                        .iter()
                        .filter_map(|id| sched.path(*id))
                        .map(|p| p.estimator.loss_rate())
                        .fold(0.0f64, f64::max)
                };
                let margin = (retransmitted as f64 * current_loss).ceil() as u64;
                let margin_path = {
                    let sched = scheduler.lock();
                    select_repair_path(&sched, last_source_path)
                };
                for _ in 0..margin {
                    if encoder.window_size() == 0 {
                        break;
                    }
                    let repair_sym = encoder.generate_repair();
                    let batch_seq = batch_counter.fetch_add(1, Ordering::Relaxed);
                    let batch = SymbolBatch {
                        symbols: vec![repair_sym],
                        send_timestamp_us: now_us(),
                        batch_seq,
                        path_id: margin_path,
                    };
                    if let Err(e) = transport.send_symbols(margin_path, batch) {
                        warn!(margin_path, ?e, "failed to send NACK repair margin");
                    }
                    stats.fec.total_repair_symbols.fetch_add(1, Ordering::Relaxed);
                    nack_repairs_this_period += 1;
                    cached_nack_budget = cached_nack_budget.saturating_sub(1);
                }
            }

            // Reduce repair_debt — NACK'd symbols are handled reactively now
            let repair_rate = {
                let ctrl = fec_controller.lock();
                let sched = scheduler.lock();
                let path_est = sched.active_paths().iter()
                    .filter_map(|id| sched.path(*id))
                    .max_by(|a, b| a.estimator.loss_rate().partial_cmp(&b.estimator.loss_rate()).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|p| &p.estimator);
                match path_est {
                    Some(est) => ctrl.compute_repair_rate(est, encoder.window_size()),
                    None => 0.0,
                }
            };
            let debt_reduction = nacked_count as f64 * repair_rate;
            repair_debt = (repair_debt - debt_reduction).max(0.0);
        }

        // Advance encoder window based on receiver ACKs
        let ack = window_ack_seq.load(Ordering::Relaxed);
        if ack > prev_ack {
            // Reduce repair_debt proportionally — ACK'd symbols no longer need proactive coverage
            let newly_acked = ack - prev_ack;
            // Compute the repair rate AND the derived window target (paper
            // Section 8.8) from the worst (highest-loss) active path, under a
            // single lock acquisition.
            let (repair_rate, derived_window) = {
                let ctrl = fec_controller.lock();
                let sched = scheduler.lock();
                let path_est = sched.active_paths().iter()
                    .filter_map(|id| sched.path(*id))
                    .max_by(|a, b| a.estimator.loss_rate().partial_cmp(&b.estimator.loss_rate()).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|p| &p.estimator);
                match path_est {
                    Some(est) => (
                        ctrl.compute_repair_rate(est, encoder.window_size()),
                        ctrl.derive_window(est),
                    ),
                    None => (0.0, None),
                }
            };
            let debt_reduction = newly_acked as f64 * repair_rate;
            repair_debt = (repair_debt - debt_reduction).max(0.0);

            // Keep the encoder window at the derived W* (paper 8.8), bounded by
            // the sender's hard ceiling; fall back to MAX_WINDOW_SIZE/2 when the
            // estimator has no throughput/RTT sample yet (cold start).
            let keep_behind = derived_window
                .map(|w| w.clamp(16, MAX_WINDOW_SIZE))
                .unwrap_or(MAX_WINDOW_SIZE / 2) as u64;
            encoder.advance(ack.saturating_sub(keep_behind));

            // Reset budget period counters on significant window advancement
            if newly_acked >= 10 {
                nack_repairs_this_period = 0;
                source_symbols_this_period = 0;
            }

            // Clean up source_path_map and retransmit buffer for ACKed/evicted
            // sequences. Reliable mode keeps path attribution for everything
            // still in the store (aged holes retransmit cross-path too).
            let (win_start, _) = encoder.window_span();
            let path_map_floor = if reliable { ack + 1 } else { win_start };
            source_path_map.retain(|&seq, _| seq >= path_map_floor);
            // Remove ACKed symbols from retransmit buffer (all seqs <= ack)
            retransmit_buffer = retransmit_buffer.split_off(&(ack + 1));
            // RWM Phase A: the sent-data store is drained by acks ONLY —
            // this is the whole retention contract.
            sent_store = sent_store.split_off(&(ack + 1));
            // Drop NACK-retransmit cooldown entries for delivered seqs (P10b)
            nack_retx_at.retain(|&seq, _| seq > ack);
            // Update correction deficit: ACKed symbols no longer need coverage
            {
                let mut sched = scheduler.lock();
                sched.deficit.on_ack_cumulative(ack);
            }
            // Reset taper offset on window advancement (new correction cycle)
            taper_offset = 0;

            prev_ack = ack;
        }

        // Cap window size — the coding window slides freely under BOTH
        // policies (it is only the FEC horizon). Under RETAIN this eviction
        // destroys no data: the sent-data store still holds the bytes, and
        // an aged hole is recovered by a targeted retransmit from it.
        if encoder.window_size() > MAX_WINDOW_SIZE {
            let (oldest, _) = encoder.window_span();
            encoder.advance(oldest + (encoder.window_size() - MAX_WINDOW_SIZE) as u64);
            // Clean up source_path_map for evicted sequences (EVICT only:
            // reliable mode keeps attribution while the store holds them).
            if !reliable {
                let (win_start, _) = encoder.window_span();
                source_path_map.retain(|&seq, _| seq >= win_start);
            }
        }

        // NOTE (paper §16.4): the window-mode runtime backend switch that
        // lived here (ADR-0030, pinned off since the P9a bring-up measured
        // its seq-space restart blinding the ACK/NACK machinery) has been
        // DELETED. The codec is chosen at startup and never changes
        // mid-stream — a new stream gets a new context, so no cross-code
        // boundary can exist inside one.
    }
}

/// Create a window encoder for the given backend.
fn create_window_encoder(
    backend: FecBackend,
    symbol_size: u16,
    fec_controller: &Arc<parking_lot::Mutex<FecRateController>>,
    scheduler: &Arc<parking_lot::Mutex<Scheduler>>,
) -> Box<dyn WindowEncoder> {
    match backend {
        FecBackend::Mettle => Box::new(MettleWindowEncoder::new(
            mettle::MettleConfig::small_window(),
            symbol_size,
            42,
        )),
        FecBackend::Streaming => {
            let params = {
                let ctrl = fec_controller.lock();
                let sched = scheduler.lock();
                let estimator = sched
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
                match estimator {
                    Some(est) => ctrl.compute_streaming_params(est),
                    None => crate::fec::StreamingParams::from_channel(2.0, 0.05, 1.15),
                }
            };
            Box::new(crate::fec::StreamingEncoder::new(symbol_size, params))
        }
        _ => Box::new(RlcWindowEncoder::new(symbol_size)),
    }
}

/// Create a window decoder for the given backend.
fn create_window_decoder(backend: FecBackend, symbol_size: u16) -> Box<dyn WindowDecoder> {
    match backend {
        FecBackend::Mettle => Box::new(MettleWindowDecoder::new(symbol_size)),
        FecBackend::Streaming => {
            let params = crate::fec::StreamingParams::from_channel(2.0, 0.05, 1.15);
            Box::new(crate::fec::StreamingDecoder::new(symbol_size, params))
        }
        _ => Box::new(RlcWindowDecoder::new(symbol_size)),
    }
}

/// Encode a block and push symbols into the interleaving buffer.
fn encode_to_interleave_buf(
    block_buf: &mut Vec<u8>,
    block_counter: &AtomicU64,
    batch_counter: &AtomicU64,
    scheduler: &Arc<parking_lot::Mutex<Scheduler>>,
    fec_controller: &Arc<parking_lot::Mutex<FecRateController>>,
    transport: &Arc<QuicTransport>,
    sent_counts: &Arc<DashMap<(u64, u32), u32>>,
    stats: &Arc<SharedStats>,
    symbol_size: u16,
    max_block_size: usize,
    ileave: &mut interleave::InterleavingBuffer,
    // Pinned at startup — mid-stream backend switching was removed (§16.4).
    fec_backend: FecBackend,
    block_arq: &Arc<parking_lot::Mutex<BlockArq>>,
) {
    let block_data = std::mem::replace(block_buf, Vec::with_capacity(max_block_size));

    if block_data.is_empty() {
        return;
    }
    // P8: Bytes so the ARQ retention can share the buffer refcounted.
    let block_data = Bytes::from(block_data);

    let block_id = block_counter.fetch_add(1, Ordering::Relaxed);

    // MTU-aware symbol sizing: use PMTU-discovered max datagram size if available,
    // otherwise fall back to the profile default. We take the minimum MTU across
    // all active paths to avoid fragmentation on any path.
    // For METTLE, repair symbols carry extra in-band metadata (bin membership lists)
    // that must be subtracted from the available MTU.
    let fec_wire_overhead = fec_backend.repair_wire_overhead(
        mettle::MettleConfig::small_window().num_edges,
    );
    let effective_symbol_size = {
        let sched = scheduler.lock();
        let total_overhead = WIRE_OVERHEAD + fec_wire_overhead;
        match sched.min_mtu() {
            Some(mtu) if mtu > total_overhead => {
                let mtu_based = (mtu - total_overhead) as u16;
                // Clamp: don't go below 64 bytes or above the profile default
                mtu_based.clamp(64, symbol_size)
            }
            // Pre-PMTUD: assume QUIC's 1200-byte initial MTU, not the
            // profile default (L1 finding: symbol 1200 + overhead never
            // fit a fresh connection's datagram limit).
            _ => symbol_size.min((1200 - total_overhead.min(1136)) as u16),
        }
    };
    let source_symbols = (block_data.len() as f64 / effective_symbol_size as f64).ceil() as u32;

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
            Some(est) => ctrl.compute_repair_count(source_symbols, est, source_symbols as usize),
            None => 0,
        }
    };

    let params = EncodingParams {
        source_symbols,
        symbol_size: effective_symbol_size,
        repair_count,
        block_id,
    };

    // ADR-0008: send BlockStart on all paths before symbols. This must be
    // the REAL control message — a regression had replaced it with an
    // empty SymbolBatch, so no receiver ever learned block params and
    // block mode could not decode over a real link (found by the L1
    // harness; in-process L0 tests bypass this wire layer). Sent as a
    // datagram for latency; symbols that still outrace it are buffered
    // and replayed by the receiver (pre_start_symbols).
    {
        let sched = scheduler.lock();
        // live_paths: a saturated path still receives symbols already
        // scheduled/interleaved for it — it must get the BlockStart too.
        for path_id in sched.live_paths() {
            let msg = ControlMessage::BlockStart {
                params,
                transfer_length: block_data.len() as u64,
                backend: fec_backend,
            };
            if let Err(e) = transport.send_control_datagram(path_id, msg) {
                warn!(path_id, ?e, "failed to send BlockStart");
            }
        }
    }

    // Encode (ADR-0030: use selector's current backend)
    let mut fec_stream = FecStream::new(&block_data, params, fec_backend);
    let source = fec_stream.take_source_symbols();
    let repair = fec_stream.generate_repair(repair_count);

    // P8: retain the source data so Ack-diff-detected losses can be
    // repaired with fresh symbols (LRU, byte-capped — see block_arq).
    block_arq
        .lock()
        .on_block_encoded(block_id, block_data.clone(), params, fec_backend, Instant::now());

    debug!(
        block_id,
        source_count = source.len(),
        repair_count = repair.len(),
        block_bytes = block_data.len(),
        "encoded block"
    );

    // ADR-0013: update monitoring stats
    stats.blocks.encoded.fetch_add(1, Ordering::Relaxed);
    stats.fec.total_source_symbols.fetch_add(source_symbols as u64, Ordering::Relaxed);
    stats.fec.total_repair_symbols.fetch_add(repair_count as u64, Ordering::Relaxed);

    // Schedule across paths (assigns symbols to paths but doesn't send yet)
    let assignments = scheduler.lock().schedule(source, repair);

    // ADR-0003: track how many symbols sent per path for this block
    for (path_id, symbols) in &assignments {
        if let Some(ps) = stats.path(*path_id) {
            ps.symbols_sent.fetch_add(symbols.len() as u64, Ordering::Relaxed);
        }
        sent_counts.insert((block_id, *path_id), symbols.len() as u32);
        // Instrumentation (L2 ws1): per-block per-path source/repair split.
        let rep = symbols.iter().filter(|s| s.is_repair).count();
        debug!(
            block_id,
            path_id = *path_id,
            src = symbols.len() - rep,
            rep,
            "block path assignment"
        );
    }

    // Push into interleaving buffer instead of sending directly
    ileave.push_block(block_id, assignments);
}

/// Per-path carry queue for symbol-level pacing: symbols drained from the
/// interleaver but not yet sendable under the token bucket wait here, in
/// send order, until the next pace tick. Carried symbols are already
/// counted in the in_flight budget (charged at schedule time).
type PaceCarry = std::collections::HashMap<u32, std::collections::VecDeque<crate::fec::WireSymbol>>;

/// Drain interleaved symbols from the buffer and send them on the wire.
///
/// Token-bucket pacing (paper Section 12.5, P7), SYMBOL-level: the
/// interleaver's drain is all-or-nothing, so drained symbols first land in
/// the per-path `carry` queue; each call then sends only up to
/// floor(tokens) symbols per path (tokens refill at cwnd/SRTT with burst
/// allowance max(10, cwnd/8)) and the remainder stays in the carry for the
/// next pace tick. No whole-block overdrafts: the first L1 run of the
/// batch-granular gate showed every 56-symbol block serializing into
/// ~5.4ms of self-queue at C2 — above Bulk's 2.5ms backoff threshold — so
/// EVERY block bought a ×0.92 backoff and cwnd pinned just under one
/// block. The TUN-read gate (in_flight >= cwnd, where in_flight is the
/// schedule-time budget covering interleaver + carry + wire) remains the
/// outer backpressure.
///
/// Returns `Some(delay)` when symbols remain in the carry — the caller
/// should retry after `delay`, the refill time for the next token on the
/// most-ready pending path. Returns `None` when everything is sent.
/// `force` bypasses the pacing gate entirely (shutdown flush).
fn send_interleaved_batches(
    ileave: &mut interleave::InterleavingBuffer,
    carry: &mut PaceCarry,
    batch_counter: &AtomicU64,
    transport: &Arc<QuicTransport>,
    scheduler: &Arc<parking_lot::Mutex<Scheduler>>,
    stats: &Arc<SharedStats>,
    block_arq: &Arc<parking_lot::Mutex<BlockArq>>,
    force: bool,
) -> Option<std::time::Duration> {
    // 1) Move any drainable interleaver content into the carry queue.
    if !ileave.is_empty() {
        // Worst-path loss rate for tapered interleaving decay.
        let loss_rate = {
            let sched = scheduler.lock();
            sched
                .active_paths()
                .iter()
                .filter_map(|id| sched.path(*id))
                .map(|p| p.estimator.loss_rate())
                .fold(0.0f64, f64::max)
        };
        let batches = if ileave.should_drain() {
            ileave.drain(loss_rate)
        } else {
            ileave.drain_all(loss_rate)
        };
        for (path_id, symbols) in batches {
            carry.entry(path_id).or_default().extend(symbols);
        }
    }
    carry.retain(|_, q| !q.is_empty());
    if carry.is_empty() {
        return None;
    }

    // 2) Per-path send budgets from the token buckets. Budgets are
    //    computed (not consumed) here; actual sends are charged in step 4.
    //    Unknown paths (removed mid-flight) get flushed unconditionally —
    //    their sends fail at the transport with a warn, as before.
    let budgets: Vec<(u32, usize)> = {
        let mut sched = scheduler.lock();
        carry
            .iter()
            .map(|(pid, q)| {
                let n = if force {
                    q.len()
                } else if let Some(p) = sched.path_mut(*pid) {
                    p.pace_refill();
                    p.pace_tokens().max(0.0) as usize
                } else {
                    q.len()
                };
                (*pid, n.min(q.len()))
            })
            .collect()
    };

    let now = now_us();
    // Sent counts per path, for pacing-token charges below.
    let mut sent_per_path: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    // Symbols individually larger than their path's CURRENT datagram
    // limit (quinn PMTUD can shrink a path's limit mid-flight on
    // blackhole suspicion — the lossy-path GE channel triggers this;
    // symbols were sized at encode time against the then-current
    // min-MTU). Dropping them silently orphaned whole blocks (L1 C8
    // finding: 529 drops in one run, mass decoder timeouts); instead
    // they are rerouted to a path whose limit still fits them.
    let mut oversized: Vec<(u32, crate::fec::WireSymbol)> = Vec::new();
    // P8: (batch_seq, path, symbol ids) of every batch that left, for the
    // ARQ ledger (recorded in one lock at the end).
    let mut sent_records: Vec<(u64, u32, Vec<(u64, u32)>)> = Vec::new();
    let send_instant = Instant::now();

    // 3) Send up to the budget per path, chunked to the path MTU.
    for (path_id, budget_syms) in budgets {
        if budget_syms == 0 {
            continue;
        }
        let symbols: Vec<crate::fec::WireSymbol> = {
            let q = carry.get_mut(&path_id).expect("budget from carry key");
            q.drain(..budget_syms).collect()
        };
        // QUIC datagrams have a hard size limit (1200 bytes initial MTU
        // until PMTUD raises it). Chunk the drain so every serialized
        // SymbolBatch fits — L1 harness finding: multi-symbol batches were
        // dropped with "datagram too large" on any real-MTU link, killing
        // the tunnel entirely.
        let max_dgram = transport
            .max_datagram_size(path_id)
            .unwrap_or(1200)
            .max(256);
        let budget = max_dgram - BATCH_WIRE_HEADER;
        let mut chunk: Vec<crate::fec::WireSymbol> = Vec::new();
        let mut chunk_bytes = 0usize;
        for sym in symbols {
            let sym_bytes = sym.data.len() + PER_SYMBOL_WIRE_OVERHEAD;
            if sym_bytes > budget {
                // Cannot fit this path's datagram limit at any chunking.
                oversized.push((path_id, sym));
                continue;
            }
            if !chunk.is_empty() && chunk_bytes + sym_bytes > budget {
                let batch_seq = batch_counter.fetch_add(1, Ordering::Relaxed);
                let ids: Vec<(u64, u32)> =
                    chunk.iter().map(|s| (s.block_id, s.payload_id)).collect();
                let batch = SymbolBatch {
                    symbols: std::mem::take(&mut chunk),
                    send_timestamp_us: now,
                    batch_seq,
                    path_id,
                };
                let n = batch.symbols.len() as u32;
                if let Err(e) = transport.send_symbols(path_id, batch) {
                    warn!(path_id, ?e, "failed to send interleaved batch");
                } else {
                    *sent_per_path.entry(path_id).or_default() += n;
                    sent_records.push((batch_seq, path_id, ids));
                }
                chunk_bytes = 0;
            }
            chunk_bytes += sym_bytes;
            chunk.push(sym);
        }
        if !chunk.is_empty() {
            let batch_seq = batch_counter.fetch_add(1, Ordering::Relaxed);
            let ids: Vec<(u64, u32)> = chunk.iter().map(|s| (s.block_id, s.payload_id)).collect();
            let batch = SymbolBatch {
                symbols: chunk,
                send_timestamp_us: now,
                batch_seq,
                path_id,
            };
            let n = batch.symbols.len() as u32;
            if let Err(e) = transport.send_symbols(path_id, batch) {
                warn!(path_id, ?e, "failed to send interleaved batch");
            } else {
                *sent_per_path.entry(path_id).or_default() += n;
                sent_records.push((batch_seq, path_id, ids));
            }
        }
    }

    // Reroute oversized symbols to the widest live path that fits them
    // (in_flight bookkeeping moves with the symbol; the rerouted symbols
    // ride the target's carry queue and go out on the next pace tick).
    if !oversized.is_empty() {
        let live: Vec<u32> = scheduler.lock().live_paths();
        let mut moved: std::collections::HashMap<u32, Vec<crate::fec::WireSymbol>> =
            std::collections::HashMap::new();
        let mut moves: Vec<(u32, u32)> = Vec::new(); // (from, to)
        let mut dropped = 0usize;
        for (from, sym) in oversized {
            let sym_bytes = sym.data.len() + PER_SYMBOL_WIRE_OVERHEAD;
            let target = live
                .iter()
                .copied()
                .filter(|pid| {
                    let lim = transport.max_datagram_size(*pid).unwrap_or(1200).max(256);
                    lim - BATCH_WIRE_HEADER.min(lim) >= sym_bytes
                })
                .max_by_key(|pid| transport.max_datagram_size(*pid).unwrap_or(1200));
            match target {
                Some(to) => {
                    moved.entry(to).or_default().push(sym);
                    moves.push((from, to));
                }
                None => dropped += 1,
            }
        }
        let mut n_moved = 0usize;
        for (to, syms) in moved {
            n_moved += syms.len();
            let q = carry.entry(to).or_default();
            for sym in syms.into_iter().rev() {
                q.push_front(sym);
            }
        }
        if n_moved > 0 || dropped > 0 {
            warn!(n_moved, dropped, "oversized symbols rerouted (path datagram limit shrank)");
        }
        let mut sched = scheduler.lock();
        for (from, to) in moves {
            if let Some(p) = sched.path_mut(from) {
                p.release_in_flight(1);
            }
            if let Some(p) = sched.path_mut(to) {
                p.charge_in_flight(1);
            }
        }
    }

    // P8: record what left the wire in the ARQ ledger (Ack diff + timeout
    // sweep drive repair from these entries).
    if !sent_records.is_empty() {
        let mut arq = block_arq.lock();
        for (batch_seq, path_id, ids) in sent_records {
            arq.on_batch_sent(batch_seq, path_id, ids, send_instant);
        }
    }

    // 4) Charge pacing tokens for what actually left, and compute the next
    //    pace tick if symbols remain in the carry. in_flight is NOT charged
    //    here: the budget was already charged once at SCHEDULE time
    //    (Scheduler::schedule → charge_in_flight); charging again at send
    //    time double-counted every symbol and leaked the gate shut (L1
    //    finding: 2s leak-guard duty cycles at ~30 KB/s).
    carry.retain(|_, q| !q.is_empty());
    let mut sched = scheduler.lock();
    for (pid, n) in sent_per_path {
        if let Some(p) = sched.path_mut(pid) {
            p.consume_pace_tokens(n);
        }
    }
    if carry.is_empty() {
        return None;
    }
    // Wake when the most-ready pending path refills its next token
    // (clamped to 500us..50ms: the lower bound coalesces sub-timer-
    // resolution wakeups into small runs the burst allowance absorbs; the
    // upper bound keeps a long-SRTT path from wedging the drain loop).
    let mut delay = std::time::Duration::from_millis(50);
    for pid in carry.keys() {
        if let Some(p) = sched.path(*pid) {
            delay = delay.min(p.pace_delay());
        }
    }
    Some(delay.max(std::time::Duration::from_micros(500)))
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

// ---------------------------------------------------------------------------
// Block-mode ARQ repair dispatch (P8)
// ---------------------------------------------------------------------------

/// Loss-declaration timeout for un-acked batches: delivered-or-lost either
/// way once the Ack would have arrived (RFC 9002-style time threshold,
/// aligned with — and never longer than — the in_flight budget expiry).
fn arq_loss_timeout(srtt: Duration) -> Duration {
    (srtt.mul_f64(1.5))
        .max(Duration::from_millis(50))
        .min(Duration::from_secs(2))
}

/// Worst-path loss estimate (the same ε̂ the proactive FEC sizing uses).
fn worst_loss_rate(scheduler: &Arc<parking_lot::Mutex<Scheduler>>) -> f64 {
    let sched = scheduler.lock();
    sched
        .active_paths()
        .iter()
        .filter_map(|id| sched.path(*id))
        .map(|p| p.estimator.loss_rate())
        .fold(0.0f64, f64::max)
}

/// Turn loss events into repair sends: plan under the ARQ lock, then send
/// paced/charged like normal corrections, then record the repair batches
/// back into the ledger (a lost repair triggers the next round).
fn send_arq_repairs(
    events: Vec<block_arq::LossEvent>,
    block_arq: &Arc<parking_lot::Mutex<BlockArq>>,
    scheduler: &Arc<parking_lot::Mutex<Scheduler>>,
    transport: &Arc<QuicTransport>,
    batch_counter: &AtomicU64,
    stats: &Arc<SharedStats>,
) {
    let eps_hat = worst_loss_rate(scheduler);
    let plans = block_arq.lock().plan_repairs(events, eps_hat);
    dispatch_repair_plans(plans, block_arq, scheduler, transport, batch_counter, stats);
}

fn dispatch_repair_plans(
    plans: Vec<block_arq::RepairPlan>,
    block_arq: &Arc<parking_lot::Mutex<BlockArq>>,
    scheduler: &Arc<parking_lot::Mutex<Scheduler>>,
    transport: &Arc<QuicTransport>,
    batch_counter: &AtomicU64,
    stats: &Arc<SharedStats>,
) {
    for plan in plans {
        // Cross-path diversity: prefer a path other than the one the loss
        // was observed on (it may be in a GE burst).
        let path_id = {
            let sched = scheduler.lock();
            select_repair_path_avoiding(&sched, plan.avoid_path, plan.avoid_path)
        };

        // Defensive BlockStart re-announce: covers the case where the
        // original BlockStart datagram was itself lost (the symbols would
        // otherwise sit in the receiver's pre-start buffer forever).
        let _ = transport.send_control_datagram(
            path_id,
            ControlMessage::BlockStart {
                params: plan.params,
                transfer_length: plan.transfer_length,
                backend: plan.backend,
            },
        );

        // Chunk to the path MTU, exactly like the normal drain path.
        let max_dgram = transport.max_datagram_size(path_id).unwrap_or(1200).max(256);
        let budget = max_dgram - BATCH_WIRE_HEADER;
        let now = now_us();
        let send_instant = Instant::now();
        let mut sent_total = 0u32;
        let mut sent_records: Vec<(u64, Vec<(u64, u32)>)> = Vec::new();
        let mut chunk: Vec<crate::fec::WireSymbol> = Vec::new();
        let mut chunk_bytes = 0usize;
        let flush =
            |chunk: &mut Vec<crate::fec::WireSymbol>, records: &mut Vec<(u64, Vec<(u64, u32)>)>, total: &mut u32| {
                if chunk.is_empty() {
                    return;
                }
                let batch_seq = batch_counter.fetch_add(1, Ordering::Relaxed);
                let ids: Vec<(u64, u32)> =
                    chunk.iter().map(|s| (s.block_id, s.payload_id)).collect();
                let n = chunk.len() as u32;
                let batch = SymbolBatch {
                    symbols: std::mem::take(chunk),
                    send_timestamp_us: now,
                    batch_seq,
                    path_id,
                };
                if let Err(e) = transport.send_symbols(path_id, batch) {
                    warn!(path_id, ?e, "failed to send ARQ repair batch");
                } else {
                    *total += n;
                    records.push((batch_seq, ids));
                }
            };
        for sym in plan.symbols {
            let sym_bytes = sym.data.len() + PER_SYMBOL_WIRE_OVERHEAD;
            if !chunk.is_empty() && chunk_bytes + sym_bytes > budget {
                flush(&mut chunk, &mut sent_records, &mut sent_total);
                chunk_bytes = 0;
            }
            chunk_bytes += sym_bytes;
            chunk.push(sym);
        }
        flush(&mut chunk, &mut sent_records, &mut sent_total);

        if sent_total > 0 {
            // Charge like any correction: in_flight budget (released by the
            // repair batch's own Ack or the expiry) + pacing tokens (may go
            // negative — recovery latency wins over strict pacing for these
            // few symbols; the debt delays the next paced drain instead).
            {
                let mut sched = scheduler.lock();
                if let Some(p) = sched.path_mut(path_id) {
                    p.charge_in_flight(sent_total);
                    p.consume_pace_tokens(sent_total);
                }
            }
            if let Some(ps) = stats.path(path_id) {
                ps.symbols_sent.fetch_add(sent_total as u64, Ordering::Relaxed);
            }
            stats
                .fec
                .total_repair_symbols
                .fetch_add(sent_total as u64, Ordering::Relaxed);

            let mut arq = block_arq.lock();
            for (batch_seq, ids) in sent_records {
                arq.on_batch_sent(batch_seq, path_id, ids, send_instant);
            }
            debug!(
                block_id = plan.block_id,
                path_id,
                count = sent_total,
                "sent ARQ repair symbols"
            );
        }
    }
}

fn handle_control_message(
    path_id: u32,
    msg: ControlMessage,
    scheduler: &Arc<parking_lot::Mutex<Scheduler>>,
    fec_controller: &Arc<parking_lot::Mutex<FecRateController>>,
    decoders: &Arc<DashMap<u64, Box<dyn FecDecoder>>>,
    sent_counts: &Arc<DashMap<(u64, u32), u32>>,
    transport: &Arc<QuicTransport>,
    fec_backend: FecBackend,
    stats: &Arc<SharedStats>,
    nack_tx: Option<&tokio::sync::mpsc::Sender<Vec<(u64, u64)>>>,
    // P8: Some(..) in block mode — Ack diffs drive repair sends.
    block_arq: Option<&Arc<parking_lot::Mutex<BlockArq>>>,
    batch_counter: Option<&Arc<AtomicU64>>,
    // Some(..) in window mode: the PEER's cumulative WindowAck point, read
    // by the local window sender (ack-driven advance, retransmit-buffer and
    // sent-store pruning). Historically this atomic was only ever written
    // with the LOCAL receiver's inbound delivery counter — a different seq
    // space entirely — so the sender's ack state was fed garbage; the RWM
    // Phase A retention contract (removal by ack ONLY) needs the real ack.
    peer_window_ack: Option<&Arc<AtomicU64>>,
) {
    match msg {
        // ADR-0008: handle BlockStart — use backend from message (ADR-0030)
        ControlMessage::BlockStart {
            params,
            transfer_length,
            backend,
        } => {
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

        // ADR-0005 + ADR-0007: handle ACK with echo-based RTT
        ControlMessage::Ack {
            block_id: _,
            batch_seq,
            received_ids,
            echo_send_timestamp_us,
            expected_count,
            received_count,
        } => {
            let mut sched = scheduler.lock();
            sched.touch_path(path_id);
            sched.ack(path_id, received_ids.len() as u32);
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
                path.estimator.record_rtt(rtt_duration);
                path.record_rtt_sample(rtt_duration);

                // ADR-0003: update loss stats from ACK
                if expected_count > 0 {
                    path.estimator
                        .record_batch(expected_count, received_count);
                    // Lost symbols also left the wire: release them from
                    // in_flight (sched.ack above only subtracts received),
                    // otherwise losses leak budget and the Copa gate jams.
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
                    &received_ids,
                    Instant::now(),
                    loss_timeout,
                );
                if !events.is_empty() {
                    send_arq_repairs(events, arq, scheduler, transport, bc, stats);
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

        ControlMessage::PathReport {
            path_id: report_path_id,
            loss_rate,
            avg_rtt_us,
            throughput_bps,
            jitter_us,
            symbols_sent: _,
            symbols_received: _,
        } => {
            let mut sched = scheduler.lock();
            // Touch path — this doubles as keepalive
            sched.touch_path(report_path_id);
            if let Some(path) = sched.path_mut(report_path_id) {
                let rtt_duration = Duration::from_micros(avg_rtt_us);
                path.estimator.record_rtt(rtt_duration);
                path.record_rtt_sample(rtt_duration);
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

        ControlMessage::Ping { timestamp_us } => {
            debug!(path_id, timestamp_us, "ping received");
            scheduler.lock().touch_path(path_id);
            let _ = transport.send_control_datagram(path_id, ControlMessage::Pong { echo_timestamp_us: timestamp_us });
        }

        // ADR-0015: handle graceful shutdown from peer
        ControlMessage::Shutdown => {
            info!(path_id, "peer is shutting down");
        }

        ControlMessage::PathAdd { path_id: new_path_id, bind_addr } => {
            info!(new_path_id, %bind_addr, "peer announced new path");
            // The peer is adding a path. We'll handle the connection setup
            // through the path command processor.
        }

        ControlMessage::PathRemove { path_id: removed_id } => {
            info!(removed_id, "peer removed path");
            scheduler.lock().remove_path(removed_id);
        }

        ControlMessage::WindowStart { symbol_size, backend, packed } => {
            debug!(path_id, symbol_size, ?backend, packed, "peer entered window mode");
        }

        ControlMessage::WindowAck { received_up_to, sack_ranges, echo_send_timestamp_us, jitter_us, cumulative_received } => {
            debug!(path_id, received_up_to, sack_count = sack_ranges.len(), cumulative_received, "SACK window ACK received");
            // Publish the peer's cumulative ack point for the window sender
            // (fetch_max: acks arrive on multiple paths, out of order).
            if let Some(pa) = peer_window_ack {
                pa.fetch_max(received_up_to, Ordering::Relaxed);
            }
            // Update RTT from echoed timestamp. echo == 0 is the sentinel
            // for timer-driven acks (hold-expiry unwedge) that echo no
            // batch — recording now−0 would poison SRTT with a huge sample.
            let now = now_us();
            let rtt_us = now.saturating_sub(echo_send_timestamp_us);
            {
                let mut sched = scheduler.lock();
                sched.touch_path(path_id);
                if echo_send_timestamp_us > 0 {
                    if let Some(path) = sched.path_mut(path_id) {
                        let rtt_duration = Duration::from_micros(rtt_us);
                        path.estimator.record_rtt(rtt_duration);
                        path.record_rtt_sample(rtt_duration);
                    }
                }
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
                let gaps = sack_to_gaps(received_up_to, &sack_ranges);
                if !gaps.is_empty() {
                    debug!(path_id, gap_count = gaps.len(), first_gap = ?gaps.first(), "SACK gaps → NACK repair");
                    if let Some(tx) = nack_tx {
                        let _ = tx.try_send(gaps);
                    }
                }
            }
        }

        ControlMessage::WindowNack { gaps } => {
            debug!(path_id, gap_count = gaps.len(), "window NACK received");
            // Send NackAck back to receiver for RX path loss measurement
            // Use gap count as a lightweight nack_id proxy
            let nack_id = gaps.len() as u32;
            let _ = transport.send_control_datagram(
                path_id,
                ControlMessage::NackAck { nack_id },
            );
            if let Some(tx) = nack_tx {
                let _ = tx.try_send(gaps);
            }
        }

        ControlMessage::NackAck { nack_id } => {
            debug!(path_id, nack_id, "NackAck received — RX path alive");
            // NackAck reception is tracked by the receiver for RX loss estimation.
            // The receiver updates its estimator based on how many NackAcks come back
            // vs how many NACKs were sent. This is handled at the application level
            // in the receiver loop, not here, since we need access to the NACK counter.
        }

        // ADR-0030: WindowSwitch/WindowSwitchAck handled in receiver/sender loops directly
        ControlMessage::WindowSwitch { flush_seq, new_backend, symbol_size } => {
            debug!(path_id, flush_seq, ?new_backend, symbol_size, "window switch request (handled in receiver loop)");
        }

        ControlMessage::WindowSwitchAck { flush_seq } => {
            debug!(path_id, flush_seq, "window switch ack (handled in sender loop)");
        }

        _ => {}
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

    // ----- sack_to_gaps (P10b SACK-driven reactive repair) -----

    #[test]
    fn test_sack_to_gaps_single_hole() {
        // Delivered up to 4; receiver has 7..=9 → 5..=6 missing.
        assert_eq!(sack_to_gaps(4, &[(7, 9)]), vec![(5, 6)]);
    }

    #[test]
    fn test_sack_to_gaps_multiple_holes() {
        // Delivered up to 0; has 2..=3 and 6..=6 → 1 and 4..=5 missing.
        assert_eq!(sack_to_gaps(0, &[(2, 3), (6, 6)]), vec![(1, 1), (4, 5)]);
    }

    #[test]
    fn test_sack_to_gaps_adjacent_range_no_gap() {
        // Sack range starts right after the cumulative point → nothing
        // missing below it, and seqs above it are NOT reported (may be
        // in flight).
        assert!(sack_to_gaps(4, &[(5, 9)]).is_empty());
    }

    #[test]
    fn test_sack_to_gaps_round_trips_receiver_encoding() {
        // The receiver converts its missing-gap view into received
        // (SACK) ranges; sack_to_gaps must invert that exactly.
        let mut received = BTreeSet::new();
        for seq in [11u64, 12, 15, 18, 19, 20] {
            received.insert(seq);
        }
        let highest_delivered = 10u64; // 0..=10 contiguous
        let highest_seen = 20u64;
        let gaps = compute_gap_ranges(&received, highest_delivered, highest_seen);
        // Receiver-side conversion (as in the WindowAck send path)
        let mut sack_ranges = Vec::new();
        let mut cursor = highest_delivered + 1;
        for &(gap_start, gap_end) in &gaps {
            if cursor < gap_start {
                sack_ranges.push((cursor, gap_start - 1));
            }
            cursor = gap_end + 1;
        }
        if cursor <= highest_seen {
            sack_ranges.push((cursor, highest_seen));
        }
        assert_eq!(sack_ranges, vec![(11, 12), (15, 15), (18, 20)]);
        // Sender-side inversion recovers the missing seqs 13..=14, 16..=17
        assert_eq!(sack_to_gaps(highest_delivered, &sack_ranges), vec![(13, 14), (16, 17)]);
    }

    #[test]
    fn test_sack_to_gaps_caps_at_max_gaps() {
        // 2×MAX_NACK_GAPS isolated received seqs → gap list is capped.
        let sack: Vec<(u64, u64)> = (0..(MAX_NACK_GAPS as u64 * 2))
            .map(|i| (2 + i * 2, 2 + i * 2))
            .collect();
        let gaps = sack_to_gaps(0, &sack);
        assert_eq!(gaps.len(), MAX_NACK_GAPS);
    }

    #[test]
    fn test_received_sack_ranges_inverts_to_gaps() {
        // The extracted helper must produce exactly what the data-arm
        // WindowAck used to compute inline, and round-trip via sack_to_gaps.
        let mut received = BTreeSet::new();
        for seq in [3u64, 4, 7] {
            received.insert(seq);
        }
        let ranges = received_sack_ranges(&received, 2, 7);
        assert_eq!(ranges, vec![(3, 4), (7, 7)]);
        assert_eq!(sack_to_gaps(2, &ranges), vec![(5, 6)]);
    }

    // ----- RWM Phase A: RETAIN-UNTIL-ACKED retention (paper §15.7/§16.3) -----

    #[test]
    fn test_store_backpressure_engages_at_store_full() {
        // Reliable: TUN reads stop exactly when the store fills — flow
        // control, not eviction.
        assert!(!store_backpressure(true, RELIABLE_STORE_MAX - 1));
        assert!(store_backpressure(true, RELIABLE_STORE_MAX));
        assert!(store_backpressure(true, RELIABLE_STORE_MAX + 1));
        // EVICT mode never backpressures on retention.
        assert!(!store_backpressure(false, RELIABLE_STORE_MAX * 10));
    }

    #[test]
    fn test_sent_store_retention_survives_window_eviction() {
        // The sender-loop invariant: the coding window slides freely (cap
        // eviction), but the sent-data store still serves the EXACT bytes
        // of any un-acked symbol for targeted retransmit — and entries
        // leave the store by ack ONLY (the same split_off the loop runs).
        use crate::fec::{RlcWindowEncoder, WindowEncoder, WireSymbol};
        let mut encoder = RlcWindowEncoder::new(64);
        let mut sent_store: BTreeMap<u64, WireSymbol> = BTreeMap::new();

        for i in 0..(MAX_WINDOW_SIZE as u64 + 100) {
            let framed = vec![i as u8; 32];
            let sym = encoder.add_source(&framed);
            sent_store.insert(sym.block_id, sym.clone());
            // The loop's cap eviction (identical arithmetic).
            if encoder.window_size() > MAX_WINDOW_SIZE {
                let (oldest, _) = encoder.window_span();
                encoder.advance(oldest + (encoder.window_size() - MAX_WINDOW_SIZE) as u64);
            }
        }

        // Seq 10 slid out of the coding window (EVICT would have lost it —
        // the measured F2 failure)…
        assert!(encoder.get_source(10).is_none(), "seq 10 must be past the FEC horizon");
        // …but the store still holds the exact sent bytes for targeted ARQ.
        let held = sent_store.get(&10).expect("store retains un-acked symbol bytes");
        assert_eq!(held.block_id, 10);
        assert!(!held.is_repair);
        assert_eq!(&held.data[..32], &[10u8; 32]);

        // Removal by ack ONLY: pruning at ack=49 drops exactly seqs 0..=49.
        let ack = 49u64;
        sent_store = sent_store.split_off(&(ack + 1));
        assert!(sent_store.get(&ack).is_none());
        assert!(sent_store.get(&(ack + 1)).is_some());
        assert_eq!(sent_store.len(), (MAX_WINDOW_SIZE + 100) - 50);
    }
}
