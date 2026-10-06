//! Top-level networking orchestration.
//!
//! Ties together TUN interface, FEC codec, scheduler, controller, and transport
//! into the main data path:
//!
//! One pipeline for every hint (the sliding window; ADR-0069 deleted the
//! block pipeline):
//!
//! Sender:
//!   TUN → window framing → RLC window encode → path selection → QUIC paths
//!
//! Receiver:
//!   QUIC paths → window decode → in-order / unordered delivery → TUN injection

pub mod ackdiag;
pub mod control_msg;
pub(crate) mod delivery;
pub mod cpuprof;
pub mod diag;
pub mod emit_source;
pub mod eta;
pub mod framing;
pub mod lat;
pub mod late;
pub mod receiver;
pub mod recv_block;
pub mod reorder;
pub mod rttdump;
pub mod sender_policy;
pub mod succ;
pub mod tasks;
pub mod walldiag;
mod sender_phases;
use sender_phases::{
    emit_generation_coded, on_ack_advance, refresh_store_cap, serve_gaps, AckAdvanceCtx, GenEmitCtx,
    ServeGapsCtx, StoreCapCtx,
    StoreCapState,
};

mod channel_set;
pub use channel_set::*;
mod recovery_laws;
pub use recovery_laws::*;
mod recovery_clock;
pub use recovery_clock::*;
mod holddown;
pub use holddown::*;
mod shed;
pub use shed::*;
mod copa_feed;
pub(crate) use copa_feed::*;
mod store_cap;
pub use store_cap::*;
mod report;
pub use report::*;
mod sack;
// Threading Q2: the sender's TX ownership (`TxCore`) and its input channels.
mod tx_inputs;
pub(crate) use tx_inputs::{
    SENDER_CMD_DEPTH, SENDER_IN_BATCHES, SenderCmd, SenderInputs, TxCore,
};
pub(crate) mod emit_burst;
pub(crate) mod seq_ring;
pub use sack::*;

use diag::{DiagCtx, DiagInputs, DiagState, WAIT_BUCKETS};
use emit_source::{SenderCtx, SenderState, emit_source};
use sender_policy::SenderPolicy;

use crate::control::FecRateController;
use crate::control::fec_rate::ProtocolHint;
use crate::fec::FecBackend;
use crate::fec::{RlcWindowDecoder, RlcWindowEncoder, WindowDecoder, WindowEncoder};
use crate::monitor::stats::{CorrectionKind, SharedStats};
use crate::routing::{self, ManagedDns, ManagedRoute};
use crate::scheduler::{Scheduler, WallClock};
use crate::transport::{ControlMessage, QuicTransport, SymbolBatch, WireMessage};
use crate::tun::{TunConfig, TunInterface};
use bytes::Bytes;
use dashmap::DashMap;
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
    /// Optional path to a pinned TLS certificate for server verification
    pub pin_cert: Option<std::path::PathBuf>,
    /// The FEC backend. Must be streaming-native (`FecBackend::Rlc`): the
    /// block-only codecs went with the block pipeline (ADR-0069), and
    /// naming one is a startup error (`pipeline_backend`).
    pub fec_backend: FecBackend,
    /// Retain-until-acked policy on the sliding-window pipeline (paper §5.1):
    /// the retention contract ρ = 1, an independent dial that never selects
    /// a pipeline. `config::resolve` presets it per named point (true at
    /// Bulk/Auto, false = EVICT at Realtime). Retention lives at the ARQ
    /// layer: a sent-data store
    /// retains source bytes until acked (targeted retransmit for aged holes;
    /// store-full ⇒ TUN-read backpressure) while the coding window slides
    /// freely as the FEC horizon; the receiver holds delivery at holes until
    /// recovered, never force-delivering past them.
    pub window_reliable: bool,
    /// Enable PI feedback loop in FEC rate controller
    pub enable_pi_feedback: bool,
    /// Reorder buffer timeout in ms (0 = disabled)
    pub reorder_timeout_ms: u64,
    /// Reorder buffer max capacity
    pub reorder_max_size: usize,
    /// Inner-feedback weight in [0,1]: mid-stream repair floor for
    /// TCP-in-tunnel payloads. Default 0.0 (measured completion-neutral at
    /// C2, regressive at C3).
    pub inner_feedback_weight: f64,
    /// The completion feed (paper §4.6): remaining bytes of the transfer in
    /// progress, published by a driver that knows them. `None` on the tunnel
    /// path — an endless stream has no `T_rem` — and `None` unless
    /// `RWM_COMPLETION_EXPOSURE` is armed, so the shipped engine is
    /// byte-identical without it. See [`CompletionFeed`].
    pub completion_feed: Option<Arc<CompletionFeed>>,
    /// Out-of-order object delivery on the reliable window (the H → ∞
    /// corner, paper §4.11). Requires `window_reliable`; set only by the
    /// native object API (perf/MemTun), never the TCP-in-tunnel path. The
    /// receiver delivers each decoded source symbol the instant it decodes
    /// (any order — the consumer reassembles by offset), and the sender's
    /// retention backpressure is relaxed so the in-order frontier's lag on
    /// a slow path no longer throttles the fast path. Default false.
    pub window_out_of_order: bool,
    /// Fungible frontier: coded-object mode. On the reliable window, the
    /// sender emits only coded (random-linear-combination) symbols over the
    /// window — no raw systematic source — so any K independent coded symbols
    /// from any path reconstruct the K sources and no symbol is a fixed
    /// in-order position a slow path can long-pole. Implies out-of-order
    /// delivery; requires `window_reliable`. Bulk-object / loose-δ only.
    /// Default false.
    pub window_coded_only: bool,
    /// Generation-based cross-path fungible coding (paper §5.8). Coded
    /// symbols are RLC combinations within fixed generations of ~W_mp source
    /// symbols; each generation is a stable coding target that decodes
    /// out-of-order on any K_G independent symbols from any path, with
    /// generation-level recovery and no per-seq ARQ beneath the code. Implies
    /// coded-only wire symbols + out-of-order delivery; requires
    /// `window_reliable`. Bulk-object / loose-δ only. Default false.
    pub window_generation_coding: bool,
    /// Systematic + deficit-driven cross-path repair (paper §5.8), the
    /// cheaper realization of generation coding. Reuses the generation
    /// machinery (fixed-generation repair anchors of ~W_mp, deficit feedback,
    /// dense `GenerationDecoder`, no per-seq ARQ, out-of-order delivery) but
    /// sends the raw systematic source as primary (delivered on arrival, zero
    /// decode) instead of coded-only. Coded symbols are emitted only as
    /// windowed repair (proactive `ceil(len·r)` per generation + deficit
    /// top-up), so decode is O(deficit)≈holes not O(G) and nothing waits for
    /// K_G. Implies generation-style receive + out-of-order; requires
    /// `window_reliable`. Bulk-object / loose-δ only. Default false.
    pub window_systematic_repair: bool,
}

/// Source-symbol size at each named point (bytes). Realtime's small packets
/// ride 512-byte symbols, Bulk/Auto 1200 (the TUN MTU clamp is
/// `symbol_size - 4`, so a full-size Bulk/Auto packet is not fragmented).
/// A table of preset VALUES at the named points, not a code path.
fn symbol_size_for_hint(hint: ProtocolHint) -> u16 {
    match hint {
        ProtocolHint::Realtime => 512,
        ProtocolHint::Bulk | ProtocolHint::Auto => 1200,
    }
}

/// RTCP-style report interval (how often we send PathReport + Ping).
const REPORT_INTERVAL: Duration = Duration::from_secs(2);

/// The inbound message channel's bound in MESSAGES (ADR-0011: large, so the
/// readers do not stall under load). The value is ADR-0011's literal,
/// unchanged — named, not new.
pub(crate) const MSG_CHANNEL_DEPTH: usize = 4096;

/// Threading Q1: the inbound channel carries I/O-owner batches (one per
/// owner poll, ≤ `INBOUND_BATCH_MAX` = 32 datagrams each), so its depth in
/// batches is `MSG_CHANNEL_DEPTH / INBOUND_BATCH_MAX` = 128: the worst case
/// held is 128 × 32 = 4096 datagrams, ADR-0011's bound unchanged (not 4096
/// batches, which would be up to 131 072 datagrams). The receiver drains
/// with `recv_many` up to this depth (one coop-budget unit per call).
pub(crate) const MSG_CHANNEL_BATCHES: usize =
    MSG_CHANNEL_DEPTH / crate::transport::io_owner::INBOUND_BATCH_MAX;
/// Maximum window size for sliding-window FEC (source symbols in encoder window).
const MAX_WINDOW_SIZE: usize = 200;
// Reorder-buffer defaults are supplied by `config::resolve` and reach the
// receiver as `config.reorder_timeout_ms` / `config.reorder_max_size`.
/// Bounds for the SRTT-adaptive in-order hold (4×SRTT, clamped). The hold
/// must survive two ARQ repair rounds, not one: each round is ~2×SRTT
/// (loss declared after ~1.5×SRTT via Ack diff/timeout + 0.5×SRTT for the
/// repair flight) and under GE burst loss the first repair itself dies
/// with the in-burst probability (~50% at C2). A hold expiry is a real hole
/// for the inner TCP (SACK recovery halves the inner cwnd for the rest of
/// the transfer). The cost of a longer hold is paid only when a symbol is
/// truly unrecoverable (bounded stall, then force-delivery). Read by the
/// EVICT (ρ < 1) in-order hold and the shed law (`net/shed.rs`).
const REORDER_MIN_HOLD: Duration = Duration::from_millis(60);
const REORDER_MAX_HOLD: Duration = Duration::from_millis(300);
/// Per-report gap cap: at most this many missing-seq gaps are acted on per
/// report — the sender's [`sack_to_gaps`] inversion of one WindowAck's SACK
/// ranges (the NACK repair loop's per-report work) and the receiver's
/// `RepairRequest` hole list (`holes_at_least`). It does NOT bound the SACK
/// encoding itself: that is [`MAX_SACK_RANGES`], derived from the control
/// datagram size. (No `WindowNack` message exists on this wire.)
pub const MAX_NACK_GAPS: usize = 20;
/// Maximum repair symbols generated per NACK received.
pub const MAX_NACK_REPAIRS_PER_NACK: usize = 10;
/// Minimum interval between NACK budget/congestion-state refreshes (microseconds).
const NACK_REPAIR_COOLDOWN_US: u64 = 5_000;
/// Minimum interval between gap-advertising WindowAcks while the cumulative
/// delivery point is stalled on a hole. The dupack analog: without these, a
/// hole silences all acks (the cumulative point can't advance), the sender
/// never learns which seqs are missing, and the only reactive repair left is
/// the reorder-hold expiry force-delivery — which the inner TCP sees as a
/// hole and retransmits.
const GAP_ACK_MIN_INTERVAL: Duration = Duration::from_millis(2);
/// Reliable window: cadence for re-advertising a stalled
/// hole via a SACK-bearing WindowAck (2×SRTT, clamped). The receiver never
/// force-delivers past the hole, so this refresh — with the sender's tail
/// sweep as backstop — is the recovery engine when gap acks are lost.
pub const HOLE_NACK_REFRESH_MIN: Duration = Duration::from_millis(25);
pub const HOLE_NACK_REFRESH_MAX: Duration = Duration::from_millis(100);
/// Fallback per-seq retransmit cooldown when no SRTT sample exists (µs).
pub const NACK_RETX_COOLDOWN_FLOOR_US: u64 = 10_000;

/// Tail ARQ sweep timeout clamp (µs): 2×SRTT bounded to [25ms, 100ms].
/// Must sit above the ack arrival time (~1×SRTT + jitter, or the sweep
/// fires spuriously on every in-flight symbol) and below the receiver's
/// reorder hold (60ms floor) plus the inner-TCP RTO (~200ms).
pub const TAIL_SWEEP_MIN_US: u64 = 25_000;
pub const TAIL_SWEEP_MAX_US: u64 = 100_000;

/// Congestion-aware NACK repair throttle.
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

    /// Idle-triggered recovery floor. A blanket per-round `.max(1)` floor
    /// would force a retransmit every round on a genuinely congested
    /// straggler and add load to the long pole. But full suppression
    /// (`multiplier == 0`) can wedge a reliable transfer whose only remaining
    /// work is recovering a confirmed hole — the transfer stalls until the
    /// QUIC idle timeout.
    ///
    /// The floor keys on the one state that distinguishes the two: is the
    /// sender still pushing new data (so repairs would pile onto a congested
    /// path), or is it idle except for the hole (no new source in flight ->
    /// no congestion we are causing -> a targeted retransmit is free)? When
    /// idle, recovery is never fully suppressed: the multiplier is floored so
    /// the confirmed hole gets at least one retransmit per round. When active,
    /// the congestion multiplier governs unchanged. `idle == false` returns
    /// the raw multiplier exactly.
    fn effective_multiplier(&self, idle: bool) -> f64 {
        if idle {
            self.repair_multiplier.max(IDLE_RECOVERY_FLOOR)
        } else {
            self.repair_multiplier
        }
    }
}

/// Minimum NACK-repair multiplier when the sender is idle-except-for-recovery.
/// Scaled by `MAX_NACK_REPAIRS_PER_NACK` (10) it yields >= 1 retransmit per
/// round, enough to unwedge a stalled reliable transfer without a blanket
/// floor's straggler load.
const IDLE_RECOVERY_FLOOR: f64 = 0.1;

/// Idle threshold floor (µs) below which `2×SRTT` would be too twitchy: a
/// sender that has sent no new source for at least this long (or 2×SRTT,
/// whichever is larger) is idle-except-for-recovery. 20 ms comfortably
/// exceeds a LAN RTT while staying well under the QUIC idle timeout.
const IDLE_RECOVERY_GAP_FLOOR_US: u64 = 20_000;

/// The unified machine gate (paper §5.2). When set (the default), (a) the
/// receive path uses one decoder (`UnifiedDecoder` — the global sparse-aware
/// closure) for both the sliding-window and generation wires, (b) the
/// Realtime hint rides the RLC family (δ-parameterization) instead of
/// switching code families, (c) plain-mode proactive repair follows the
/// quantity law (TaperBudget) + the trailing solvable-span placement
/// with A* = clamp(rate·D, 1, W), D = b(hint)·RTprop (paper §5.3/§5.4:
/// Realtime ½, Auto 1, Bulk 2 RTT), (d) generation mode runs the derived
/// M* pipeline depth (`RWM_GEN_PIPE` defaults on), (e) the A* send-rate
/// anchor ships on (`RWM_ASTAR_ANCHOR`), and (f) the realtime evict path
/// runs δ-honest overload shedding (`RWM_UNIFIED_SHED`, paper §5.6).
///
/// `RWM_UNIFIED=0` + Realtime selects the legacy-RLC windowed machine
/// (`RlcWindowDecoder`).
pub(crate) fn unified_active() -> bool {
    crate::gates::get().unified
}

/// The one pipeline's codec (ADR-0069, executed): every hint rides the
/// sliding-window pipeline, so the configured backend must be
/// streaming-native. There is no block pipeline to fall back to: a
/// block-only backend — reachable only through a hand-built library
/// [`PeerConfig`]; `config::resolve` already rejects it by name — is a
/// startup error, never a silent re-route. The retention contract ρ
/// (`window_reliable`) is an independent dial and is not read here.
pub(crate) fn pipeline_backend(config: &PeerConfig) -> anyhow::Result<FecBackend> {
    if !config.fec_backend.is_streaming() {
        anyhow::bail!(
            "fec_backend {:?} is block-only: the block pipeline was removed (ADR-0069); \
             every hint rides the window pipeline on FecBackend::Rlc",
            config.fec_backend
        );
    }
    Ok(config.fec_backend)
}

/// The `[PIPE]` echo (docs/status.md §4; measurement-discipline rules 1 and
/// 15b): the pipeline and the FEC backend the engine pinned, one line per
/// engine start on both endpoints. Since ADR-0069 the pipeline token is
/// always `window`; the line stays byte-compatible so the battery parsers'
/// right-binary witness keeps working. An instrument only.
fn pipe_echo_line(hint: ProtocolHint, backend: FecBackend) -> String {
    let hint = format!("{hint:?}").to_lowercase();
    format!("[PIPE] pipeline=window backend={backend:?} hint={hint}")
}

// ---------------------------------------------------------------------------
// Retention policy (paper §5.1), unit-tested below.
//
// Reliability is a pipeline policy, not a codec property — and it lives at
// the ARQ layer, not in the coding window. The coding window keeps sliding
// freely under both policies: it is only the FEC horizon (fungible repair
// coverage for recent, not-yet-localized losses). What differs:
//
//   EVICT              — production Realtime. The retransmit buffer holds
//                        metadata only; source bytes die with window
//                        eviction, losses past the horizon become holes
//                        (bounded memory, bounded delay — correct for δ).
//   RETAIN-UNTIL-ACKED — a sent-data store retains every sent source
//                        symbol's bytes until the peer acks it (removal by
//                        ack only — never timeout, never pressure). An aged
//                        SACK-confirmed hole that slid out of the window is
//                        recovered by a targeted retransmit of exactly that
//                        symbol from the store (once a loss is localized,
//                        fungibility has no value). Store fullness becomes
//                        backpressure on the TUN — the same contract as the
//                        block path's cwnd gate — never data loss (dropping
//                        un-acked source turns bulk completions into DNFs).
// ---------------------------------------------------------------------------

/// Sent-data store capacity (symbols) for the retain-until-acked policy.
/// Sized to a few BDPs of plain bytes (no coding cost): 1024 × 1200 B
/// ≈ 1.2 MB ≈ 10× the C2 BDP (100 Mbit × 10 ms ≈ 104 symbols). When
/// full, the sender stops reading the TUN until acks drain it (flow
/// control, not loss).
///
/// The cap also applies in out-of-order object mode. Without the store's
/// backpressure the sender drains the whole object into the encoder, the
/// O(200) coding window slides to the newest source and away from the
/// un-received holes, proactive repairs stop covering them and every hole
/// falls to rate-limited targeted retransmit. The cap keeps the coding
/// window near the recovery frontier. In-order delivery with retention
/// already completes the object when its last symbol decodes, so
/// out-of-order delivery gains nothing here.
const RELIABLE_STORE_MAX: usize = 1024;

/// Hard ceiling on the derived generation-pipeline depth (bounds sender
/// retention, the receiver reassembly span, and the deficit-report width;
/// 32·G ≈ 12k symbols ≈ 15 MB at 1200 B — the loose memory backstop, ~the
/// ooo_retain default ×2).
const GEN_PIPE_MAX_GENS: usize = 32;

/// Derived generation-pipeline depth M* (paper §5.3): the dynamic window
/// advance A* = clamp(D·rate, 1, W), quantized to generations (the
/// stable-anchor code advances in whole generations, so the depth of the
/// in-flight generation pipeline is the window advance).
///
/// For the decode frontier to keep advancing at the link rate, the
/// generations in flight must cover the time D from a generation's first
/// coded emission to its ack: D = delivery (≈ 1 RTT, the pipe) + one
/// deficit-feedback round for the loss-shortfall tail (≈ 1 RTT: report waits
/// ~SRTT cadence + top-up flight). Hence
///
/// ```text
///   M* = ceil(rate · 2·RTT / G) + 1
/// ```
///
/// (+1 = the currently-filling head generation). `rate` is the windowed-max
/// delivered rate (decode-clocked samples are mostly low with the true rate
/// at the burst top, so the max is the recovery statistic); `rtt_s` is
/// RTprop (min-RTT), not the live SRTT — the live RTT includes the queue
/// this pipeline itself creates (positive feedback), while the in-flight cap
/// holds the actual RTT near RTprop (the BBR discipline). Clamped to
/// [2, GEN_PIPE_MAX_GENS]: 2 is the fixed pipeline used when the anchors
/// have no sample yet (cold start).
fn gen_pipe_depth(rate_sym_per_s: f64, rtt_s: f64, gen_size: usize) -> usize {
    if rate_sym_per_s <= 0.0 || rtt_s <= 0.0 {
        return 2;
    }
    let d = 2.0 * rtt_s; // delivery + one repair round
    let m = ((rate_sym_per_s * d) / gen_size.max(1) as f64).ceil() as usize + 1;
    m.clamp(2, GEN_PIPE_MAX_GENS)
}

/// Per-path in-flight cap decision, consumed by the generation pipeline.
/// Given each active path's `(in_flight, per_path_cap)` where the cap =
/// gain·BtlBw_i·RTprop_i (that path's own windowed-max bandwidth × its own
/// min-RTT), the sender is "full" only when no path is below its own cap. So
/// the slow path's RTT-inflated cap bounds only the slow path, and the fast
/// path keeps pulling source while the slow path is full. A single global
/// budget gain·Σ_i BtlBw_i·RTprop_i would stall the fast path behind it (and
/// let the slow path's inflated term over-drive its own queue into
/// bufferbloat). Extracted pure for unit testing.
fn infl_percap_full(per_path: &[(u64, u64)]) -> bool {
    !per_path.iter().any(|&(in_flight, cap)| in_flight < cap.max(1))
}

/// Dead path timeout: if no report received for this long, deactivate the path.
const DEAD_PATH_TIMEOUT: Duration = Duration::from_secs(6);
/// Lowest base [`now_us`] starts from, µs (~12.7 days): keeps every stamp
/// non-zero (`echo_send_timestamp_us == 0` is the timer-ack sentinel) and
/// leaves headroom below "now" even if the wall clock reads before 1970.
const NOW_US_MIN_BASE: u64 = 1 << 40;

/// The engine clock, µs. Monotonic: an `Instant` offset from a base read
/// once from the wall clock at first use, so a wall-clock step (NTP, manual
/// set) can neither freeze nor storm the recovery clocks, and a clock before
/// the UNIX epoch cannot panic. The wall-clock base keeps stamps in the
/// epoch-µs range (the varint width on the wire).
///
/// Every consumer is a same-process difference, an echo of this process's
/// own stamp (RTT, ETA book, ping), or — at the peer — an `arrival −
/// send_ts` difference that is min-subtracted or differenced again
/// (`lat.rs` A_x, `eta.rs` receiver ℓ, the estimator's jitter), in which the
/// constant offset between the two hosts' bases cancels. Nothing compares
/// this clock to the peer's as an absolute time.
pub(crate) fn now_us() -> u64 {
    now_pair().0
}

/// [`now_us`] together with the `Instant` it was computed from — ONE clock
/// read for a site that needs both the engine µs and an `Instant` (the A*
/// anchor, a tokio deadline). `now_pair().0` is exactly what `now_us()`
/// would have returned at that instant.
pub(crate) fn now_pair() -> (u64, Instant) {
    static BASE: std::sync::OnceLock<(Instant, u64)> = std::sync::OnceLock::new();
    let &(t0, base_us) = BASE.get_or_init(|| {
        let wall_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);
        (Instant::now(), wall_us.max(NOW_US_MIN_BASE))
    });
    let now = Instant::now();
    (base_us.saturating_add(now.saturating_duration_since(t0).as_micros() as u64), now)
}

/// Collect per-generation residual deficits for the deficit-feedback report
/// (paper §5.8), receiver arm. Walks `gen_widths` (anchor→K_g) in anchor
/// order, skipping fully-decoded generations, and returns up to `report_gens`
/// `(anchor, deficit)` pairs where `deficit = K_g − rank_in(anchor, K_g)`.
///
/// `report_gens` covers the whole in-flight range, so the receiver reports
/// every outstanding generation's deficit in one report and the sender
/// repairs all holes in a single round-trip (parallel tail flush). A small
/// bound (frontier ± a few generations) would repair holes frontier-first,
/// roughly one generation per round-trip (serial tail, throughput ∝
/// window/RTT). Extracted as a pure function so the "all deficits recover in
/// one round" invariant is unit-tested without driving the async receiver
/// loop.
fn collect_gen_deficits(
    gen_widths: &BTreeMap<u64, u16>,
    report_gens: usize,
    mut rank_of: impl FnMut(u64, u64) -> u64,
) -> Vec<(u64, u32)> {
    let mut deficits: Vec<(u64, u32)> = Vec::new();
    for (&anchor, &k) in gen_widths.iter() {
        if deficits.len() >= report_gens {
            break;
        }
        let rank = rank_of(anchor, k as u64);
        let deficit = (k as u64).saturating_sub(rank);
        if deficit > 0 {
            deficits.push((anchor, deficit as u32));
        }
    }
    deficits
}

/// Repair-coverage horizon gate: the FEC discipline of waiting for the coded
/// repair before falling back to ARQ.
///
/// In generation mode the deficit report is the reactive NACK, and it would
/// fire the instant a hole appears at the frontier, before the in-flight
/// proactive repair covering that hole (which rides with the surrounding data
/// and arrives ~1 generation-span later) has a chance to decode it. A hole
/// proactive repair would have covered then gets a redundant ARQ round-trip,
/// capping the proactive recovery fraction and binding throughput to the
/// round-trip regime at high RTT.
///
/// A generation's residual deficit is only eligible to be reported (a
/// reactive NACK) once it has been outstanding for at least `horizon` — the
/// time for the covering proactive repair to arrive + decode (~the generation
/// / window span at the current send rate, not an RTT). Newly-deficient
/// anchors are armed (their first-seen instant recorded in `armed`) and
/// withheld; an anchor that decodes within the horizon drops out of
/// `deficits` and is disarmed — a proactive recovery, no round-trip. Only
/// anchors whose horizon has expired are returned (the reactive fallback that
/// keeps reliability intact).
///
/// `horizon == 0` reports immediately (the shipped path). Extracted pure so
/// the "hole covered by proactive repair within the horizon fires no NACK"
/// invariant is unit-tested without the async receiver loop.
fn horizon_gate_deficits(
    deficits: &[(u64, u32)],
    armed: &mut BTreeMap<u64, Instant>,
    horizon: Duration,
    now: Instant,
) -> Vec<(u64, u32)> {
    if horizon.is_zero() {
        return deficits.to_vec();
    }
    // Disarm anchors that no longer carry a deficit (decoded within the
    // horizon → proactive win) so `armed` tracks only live holes.
    let live: std::collections::BTreeSet<u64> = deficits.iter().map(|&(a, _)| a).collect();
    armed.retain(|a, _| live.contains(a));
    let mut ready: Vec<(u64, u32)> = Vec::new();
    for &(anchor, deficit) in deficits {
        let first = *armed.entry(anchor).or_insert(now);
        if now.saturating_duration_since(first) >= horizon {
            ready.push((anchor, deficit));
        }
    }
    ready
}

/// Main entry point.
pub async fn run(config: PeerConfig) -> anyhow::Result<()> {
    run_impl(config, None).await
}

/// Run the engine with a caller-provided TUN (e.g. [`TunInterface::memory`]).
///
/// Skips OS TUN creation and all routing/DNS management (setup and cleanup) —
/// nothing OS-touching happens for the injected interface. Everything else is
/// byte-identical to [`run`]. Note: the MTU clamp only sizes the OS TUN
/// device; with an injected TUN the caller must size its packets to fit one
/// symbol (`symbol_size_for_hint(hint) - 4`) itself.
pub async fn run_with_tun(config: PeerConfig, tun: TunInterface) -> anyhow::Result<()> {
    run_impl(config, Some(tun)).await
}

async fn run_impl(config: PeerConfig, injected_tun: Option<TunInterface>) -> anyhow::Result<()> {
    let tun_injected = injected_tun.is_some();
    // The RWM_* env-gate surface, resolved once for this engine (src/gates.rs).
    let gates = crate::gates::get().clone();
    // Liveness echo (measurement-discipline rule 15): one `[GATES]` line
    // naming every gate resolved above and its resolved value, on both
    // endpoints, once per engine start. Two-sided by construction — the off
    // values are printed too, so a battery can assert "gate absent in the
    // control" as mechanically as "gate present in the arm". One formatted
    // line, never on the hot path.
    gates.echo();
    // Parse TUN address
    let (tun_ip, prefix_len) = parse_cidr(&config.tun_addr)?;
    let netmask = prefix_to_netmask(prefix_len);

    // Backend selection happens once, here, and is pinned for the life of
    // the stream (paper §5.10: no cross-code algebra ⇒ any mid-stream
    // switch strands in-flight data). One pipeline for every hint
    // (ADR-0069): the codec is the window pipeline's RLC, and a block-only
    // backend is a startup error.
    let effective_fec_backend = pipeline_backend(&config)?;
    if config.protocol_hint == ProtocolHint::Realtime {
        if unified_active() {
            // Paper §5.2: one code family across the δ axis — Realtime is
            // the small-δ parameterization of the RLC span machine, not a
            // different code. (Mechanism-liveness echo.)
            info!("RWM_UNIFIED: Realtime rides the RLC span machine (small-δ parameterization, no code-family switch)");
        } else {
            // Mechanism-liveness echo for the legacy opt-out arm.
            info!("Realtime mode (RWM_UNIFIED=0): riding the legacy-RLC windowed machine");
        }
    } else if config.window_reliable {
        // Liveness echo the L1 parsers witness on every window arm
        // (`stage3_parse.py` WIN_LINE); the text is kept byte-stable.
        info!("reliable window mode (retain until acked): auto-selecting RLC windowed backend");
    }

    // The source-symbol size at this named point.
    let symbol_size = symbol_size_for_hint(config.protocol_hint);
    info!("{}", pipe_echo_line(config.protocol_hint, effective_fec_backend));
    // The retention contract ρ (per-stream, resolved per named point by
    // `config::resolve`; an independent dial, never a pipeline selector).
    let window_reliable = config.window_reliable;
    // Fungible frontier (coded-object): coded-only presupposes the reliable
    // window (retention is the ARQ backstop for aged holes). It is a
    // bulk-object mode that pays a window-fill decode latency, so it always
    // implies out-of-order object delivery.
    let window_coded_only = window_reliable && config.window_coded_only;
    // Generation-based fungible coding (paper §5.8). Composes on top of the
    // reliable window: coded symbols are RLC combinations within fixed
    // generations (stable target) rather than the moving sliding window, and
    // the per-seq ARQ beneath the code is switched off (recovery is
    // generation-level). Implies coded-only wire symbols + out-of-order object
    // delivery.
    // Systematic + deficit-repair: a submode of the generation machinery.
    // `window_systematic` reuses all of generation mode's receive path (dense
    // decoder, gen deficit feedback, out-of-order delivery, no per-seq ARQ) —
    // so `window_generation` is true whenever either flag is set — and
    // differs only on the sender: raw source rides the wire as primary and
    // the encoder emits only the `ceil(len·r)` repair overhead (see the
    // `systematic` arg to `run_window_sender`).
    let window_systematic = window_reliable && config.window_systematic_repair;
    let window_generation = window_reliable
        && (config.window_generation_coding || config.window_systematic_repair);

    // The window pipeline carries at most one packet per source symbol: SymbolPacker
    // frames each packet with a 2-byte length prefix and closes the symbol
    // with a 2-byte end sentinel, and packets that don't fit are truncated
    // (corrupted on the wire, silently dropped by the peer's IP stack).
    // Clamp the TUN MTU so the inner stack never emits a packet larger than
    // one symbol can carry (MSS-sized TCP segments would be truncated at
    // symbol_size=512 and stall every transfer).
    let tun_mtu: u16 = symbol_size.saturating_sub(4);
    info!(
        mtu = tun_mtu,
        symbol_size,
        "window mode: clamping TUN MTU to fit one packet per symbol"
    );

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
    let transport = QuicTransport::new(
        &config.bind_addrs,
        config.is_server,
        config.pin_cert.as_deref(),
    ).await?;

    // Set up paths with protocol-hint-derived scheduling weights
    let mut scheduler = Scheduler::new_with_hint(Arc::new(WallClock), config.protocol_hint);
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
    let batch_counter = Arc::new(BatchCounter::new());
    // Threading Q2: the FEC controller is TX state, owned by the sender task
    // (moved into its `TxCore` below; no mutex). Its one receiver input, the
    // decoder's PI feedback, reaches it as a `SenderCmd`.
    let fec_controller = {
        let mut ctrl = FecRateController::new_with_toggles(
            config.target_tail_loss,
            config.max_fec_overhead,
            config.protocol_hint,
            effective_fec_backend,
            config.enable_pi_feedback,
            symbol_size,
        );
        // Inner-feedback repair floor for TCP-in-tunnel payloads. Default
        // weight 0.0 (config::resolve): the floor measured completion-neutral
        // at C2 and regressive at C3 — the inner flow absorbs the residual
        // ARQ stalls, and floor repairs displace source symbols in the same
        // inner-limited loop. The knob remains for payloads that measure
        // differently.
        ctrl.set_inner_feedback(config.inner_feedback_weight);
        ctrl
    };
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
    let recv_shutdown_rx = shutdown_tx.subscribe();

    // Spawn the shutdown-signal handler. SIGINT (`ctrl_c`) and, on unix,
    // SIGTERM are one trigger on one code path: both resolve `shutdown_signal`
    // and fire the same broadcast. SIGTERM matters because every tools/l1
    // harness stops the server with `pkill -x raptorpath` (SIGTERM); an
    // unhandled SIGTERM is an abrupt kill in which no destructor runs, so the
    // receiver's exit flush (`net/recv_block.rs`, `final=1`) would never be
    // seen on L1.
    let ctrlc_shutdown_tx = shutdown_tx.clone();
    tokio::spawn(async move {
        if shutdown_signal().await {
            info!("received SIGINT/SIGTERM, initiating graceful shutdown...");
            let _ = ctrlc_shutdown_tx.send(());
        }
    });

    // The peer's cumulative window ACK point: written by the sender's own ack
    // handling (threading Q2: `on_window_ack` runs in the sender), read by the
    // sender's advance / store-prune sites and the `[ACKDIAG]` gauge.
    let window_ack_seq = Arc::new(AtomicU64::new(0));
    // Threading Q2 — the sender's two input channels (plan rules 1, 3, 4).
    //
    // `sender_in`: TX-direction control input — the client's WindowAcks and
    // per-batch Acks, forwarded by each path's I/O owner as ONE batch per
    // owner poll (the owner routes by message, `control_msg::is_tx_control`),
    // and the PathReport / Ping the control fast path forwards. The sender
    // drains it with `recv_many` (one coop-budget unit per drain, never per
    // message) at its loop top and in an always-armed `select!` arm, and
    // runs `on_window_ack` itself: the ack wake is the channel's own (a
    // parked sender is woken by the send; a busy one finds the batch at its
    // next loop top), which retires P1's `AckWake` Notify.
    //
    // `sender_cmd`: the cold tasks' requests on TX state (the 2 s report
    // tick, path add / remove, the receiver's FEC feedback and dead-path
    // revival nudge): messages, never a lock.
    let (sender_in_tx, sender_in_rx) =
        mpsc::channel::<crate::transport::InboundBatch>(SENDER_IN_BATCHES);
    let (sender_cmd_tx, sender_cmd_rx) = mpsc::channel::<SenderCmd>(SENDER_CMD_DEPTH);

    // NACK gap channel: handle_control_message sends gap ranges, window sender
    // receives for targeted repair. The batch rides with its [`FireCause`] tag
    // (`[FCAUSE]`) — a label for the counters only; nothing branches on it.
    // The tuple's middle slot is the path the ack carrying this gap report
    // arrived on (`control_msg.rs`'s `path_id`). It is a label for the
    // attribution counters exactly as `FireCause` is; nothing in the repair
    // loop branches on it.
    let (nack_tx, nack_rx) =
        tokio::sync::mpsc::channel::<(FireCause, u32, Vec<(u64, u64)>)>(16);

    // Generation-deficit channel (paper §5.8): the data-arm's control handler parses
    // inbound GenerationDeficit messages and forwards the (anchor, deficit)
    // vector to the local window sender, which emits exactly the residual coded
    // symbols each frontier generation still needs (bounded, targeted recovery).
    let (deficit_tx, deficit_rx) =
        tokio::sync::mpsc::channel::<Vec<(u64, u32)>>(64);

    // Receiver-seat repair-request channel (paper §7.6, arms (A)/(B)): the
    // data-arm's control handler parses inbound `RepairRequest` messages and
    // forwards `(cause, spans)` to the local window sender, which serves each
    // span from `sent_store` at `m = 1` and from `generate_repair_range` above
    // it. The exact parallel of `deficit_tx` above, at the same depth and with
    // the same best-effort contract: the two are one mechanism in two
    // vocabularies.
    //
    // Never produced with both arms absent: `recv_request_tx` is `None`, so
    // an arriving `RepairRequest` is counted and dropped.
    let (request_tx, request_rx) =
        tokio::sync::mpsc::channel::<RepairRequestBatch>(64);

    // SACK channel: forwards the receiver's received-above-frontier ranges
    // (the SACK ranges themselves, not the inverted gaps) to the
    // plain-reliable window sender, where the SACK-clocked release (paper
    // §6.3, ADR-0060) uncounts those symbols from flow control so it keys on
    // true outstanding-unacked, not the in-order cumulative-ack frontier that
    // freezes on every hole. Plain-reliable only (generation / coded-only
    // have their own structural backpressure).
    let (sack_tx, sack_rx) =
        tokio::sync::mpsc::channel::<SackReport>(64);

    info!(
        symbol_size,
        backend = ?effective_fec_backend,
        "sliding-window FEC mode"
    );

    // Channel for received messages from all paths
    // ADR-0011: larger message channel to avoid stalling under load.
    // Threading Q1: each item is one I/O-owner poll's datagrams (a batch of
    // ≤ INBOUND_BATCH_MAX); the depth in batches keeps the worst case at
    // ADR-0011's 4096 datagrams (see `MSG_CHANNEL_BATCHES`).
    let (msg_tx, msg_rx) = mpsc::channel::<crate::transport::InboundBatch>(MSG_CHANNEL_BATCHES);
    // Dedicated channel for stream-origin control: liveness must not queue
    // behind the data flood (see `QuicTransport::start_readers_for_path`).
    let (ctrl_tx, ctrl_rx) = mpsc::channel::<(u32, WireMessage)>(256);
    transport
        .start_readers(msg_tx.clone(), ctrl_tx.clone(), sender_in_tx.clone())
        .await;

    // Sender task: TUN → frame → encode → schedule → send
    let transport_arc = Arc::new(transport);
    // Threading Q2: the scheduler's TX half publishes its first view (SRTT,
    // RTprop, liveness) before any task runs, so the receiver never reads an
    // unpublished path.
    scheduler.publish_tx_view(&stats);

    // ── Plain-mode Copa delivery feed ───────────────────────────────────────
    // In plain window-reliable mode WindowAcks carry RTT only (only the
    // block-path `ControlMessage::Ack` drives `Scheduler::ack →
    // PathState::on_ack`), so without this feed the per-path Copa cwnd sits
    // at INITIAL_CWND and cannot own a substrate window. The feed is
    // sender-side only (no wire/receiver change): each plain source send
    // records seq→path + a BBR rate-sample snapshot (`on_src_sent`), and each
    // WindowAck's cumulative-frontier advance + newly-SACKed seqs are
    // attributed back to the path that carried them (`on_src_delivered_seq` —
    // send-interval Δt, ack-aggregation robust) followed by the Copa cwnd
    // dynamics (`on_delivery_signal`). It uses the BBR-correct sampler, not
    // the ack-interval `record_delivery`: the ack-interval Δt spikes on
    // frontier jumps and its windowed-max over-reads (paper §2.7), which
    // would pin cwnd ≫ BDP via the anchor floor — bufferbloat by estimator.
    // RTT floor + delivery signal are then both live in plain mode, per path
    // (per connection = per path), and the resulting cwnd is written into the
    // pass-through substrate window.
    //
    // Off by default: enabled by `RWM_QUIC_CC=passthrough` (Copa-sole flies
    // blind without it) or standalone by `RWM_COPA_FEED=1`. In-order plain
    // mode only: the OOO/generation modes deliver out of order (the in-order
    // frontier is not their delivery signal).
    let copa_feed_plain: Option<Arc<CopaFeed>> = {
        let wanted = transport_arc.cc_passthrough_active() || gates.copa_feed;
        let plain_inorder = window_reliable
            && !window_generation
            && !window_coded_only
            && !config.window_out_of_order;
        // `RWM_PLAIN_RS` (paper §2.7): the send-interval sampler without
        // Copa ownership — plain mode under any substrate CC gets an
        // honest per-path BtlBw anchor (the WindowAck attribution machinery
        // reused sampling-only). The full feed (`wanted`) takes precedence.
        let plain_rs = gates.plain_rs;
        if !wanted && plain_rs && plain_inorder {
            let feed = CopaFeed::new_sampling_only(gates.rs_attr);
            info!(
                "plain-mode send-interval SAMPLER ACTIVE (RWM_PLAIN_RS sampling-only: \
                 WindowAck frontier/SACK -> per-path send-interval rate samples; \
                 CC ownership unchanged; flight-witness attribution={} \
                 [residual (iii): cross-path retransmit acks younger than the \
                 retransmit path's RTprop credit the ORIGINAL flight; \
                 RWM_RS_ATTR=0 = legacy last-sent control])",
                feed.attr_witness
            );
            Some(Arc::new(feed))
        } else if wanted && plain_inorder {
            info!(
                "plain-mode Copa delivery feed ACTIVE (WindowAck frontier/SACK → per-path send-interval rate samples + cwnd dynamics)"
            );
            // Mechanism-liveness echo (measurement-discipline rule 1): which
            // clock feeds Copa's delay term, and the hint-mapped δ the update
            // law targets.
            info!(
                copa_wire = crate::scheduler::copa_wire_active(),
                hint = ?config.protocol_hint,
                delta = crate::scheduler::copa_delta_for_hint(config.protocol_hint),
                cc_pace = gates.cc_pace,
                compete = crate::scheduler::copa_compete_active(),
                "Copa queue-signal clock: wire={} (quinn packet-timed RTT; =false is the #80 app-echo arm)",
                crate::scheduler::copa_wire_active(),
            );
            Some(Arc::new(CopaFeed::new()))
        } else {
            None
        }
    };
    let sender_copa_feed = copa_feed_plain.clone();

    // ── Honest anchor inputs (ADR-0061) ──
    // Mechanism-liveness echoes (measurement-discipline rules 1/15): asserted
    // present on the arms, absent on the controls; the [GATES] line carries
    // the two-sided value either way. Emitted in `run_impl` so both roles
    // echo.
    if crate::scheduler::honest_anchor_active() {
        info!(
            "O(1) windowed-max rate filter ACTIVE (RWM_HONEST_ANCHOR: max_bw read \
             off a monotonic max-deque maintained beside bw_samples — the \
             VALUE-IDENTICAL statistic, same [1s,10s] window, same evictions, \
             amortized O(1) per accepted sample instead of the O(window) \
             full-window fold that costs +61-64% sender CPU/byte at c1 under \
             RWM_PLAIN_RS; zero constants; RWM_HONEST_ANCHOR=0 = legacy fold \
             control, value-identical by unit-pinned equivalence)"
        );
    }
    if crate::scheduler::honest_k_active() {
        info!(
            "raw-sample echo-ratio floor ACTIVE (RWM_HONEST_K: EchoRatioMin fed \
             the RAW per-sample rtt/RTprop ratio at the sample clock in \
             PathState::record_rtt, consumed as k_raw.unwrap_or(legacy) by every \
             honest-cap/three-term K read — the windowed MIN reads the delay \
             distribution's FLOOR instead of the smoothed series' near-mean \
             minimum (the measured jit25 x1.34 inversion); same window, clamp \
             and seed-identity guard; zero constants; RWM_HONEST_K=0 = \
             smoothed-at-refresh control)"
        );
    }

    // ── Window-mode control-datagram merge (env RWM_ACK_MERGE) ────────────
    // Mechanism-liveness echo (measurement-discipline rule 1) — emitted in
    // `run_impl` so it fires in both roles: the receiver is what suppresses
    // the legacy Ack, the sender is what re-homes its consumers. Placed
    // beside the CopaFeed construction on purpose: whether a feed exists
    // decides how much work the re-homing has to do, and in the shipped
    // default it does not exist.
    if gates.ack_merge {
        info!(
            copa_feed = copa_feed_plain.is_some(),
            "ack-merge ACTIVE (RWM_ACK_MERGE: WINDOW mode sends ONE control \
             datagram per data message instead of two — the legacy per-batch \
             Ack is suppressed, the SACK WindowAck goes unconditional at that \
             cadence and carries the Ack's payload in the v6 cumulative \
             cum_expected/cum_received counters, every Ack-arm consumer \
             re-homed onto the counter diff; BLOCK mode bit-exact; the \
             delivery statistic/cadence/counts unchanged)"
        );
    }

    // ── Derived stall gauge — mechanism-liveness echo (measurement-discipline
    //    rule 1). Emitted in `run_impl` so both roles echo: the gauge has a
    //    sender arm (`sidle2=`) and a receiver arm (`idle2=`).
    if gates.sidle_derived {
        info!(
            loop_wake_us = LOOP_WAKE_US,
            legacy_stall_us = 3_000u64,
            "derived stall gauge ACTIVE (RWM_SIDLE_DERIVED: DIAG-only and \
             behaviour-inert — the legacy sidle=/idle= fields are printed \
             UNCHANGED and sidle2=/idle2= are added beside them, counting \
             the same event stream against 3 × the MEASURED inter-event \
             interval, floored at the legacy 3 ms and capped at the \
             hole-refresh cadence)"
        );
    }

    // The ack handling's channel seats (threading Q2: `on_window_ack` runs
    // in the sender, which is also the consumer of both channels).
    // Generation coding (paper §5.8) turns the per-seq targeted ARQ off
    // beneath the code — the per-seq reliability layer makes the moving
    // window path-affine and invokes the NACK congestion throttle. With no
    // NACK producer, a short generation is recovered by more coded symbols
    // for that generation (fungible, cross-path), never by resending a
    // specific seq. So the SACK→gap producer is suppressed in generation mode.
    //
    // ── The collision seam (paper §7.6), one `&&` ────────────
    //
    // The request law (`RWM_RECV_REQUEST_LAW`) makes the receiver's report
    // the single authority for a repair. Its identifiability argument depends
    // on that: `rho_heal(l) = pi0*f(l)` holds exactly on `[0, l*)` only
    // because no copy has flown there, and a copy flying is precisely the
    // censoring of paper §7.4. So while the arm is armed the per-seq
    // SACK->gap producer is suppressed at its source — here, where it is
    // armed — rather than filtered downstream, so `[FCAUSE] gap_data` going
    // to zero on the treatment arm is the proof that the seam closed.
    //
    // `sack_tx` below is deliberately not touched: SACK drives store slot
    // release and never recoverability; pruning `sent_store` on SACK is
    // unsafe (in-order duals wedge) and the safe realization is ADR-0060's
    // release.
    //
    // The two backstops the seam leaves standing are untouched and out of
    // domain by construction: the `p_lost` taper and the end-of-stream tail
    // sweep, whose producer is the sender's own `tail_deadline` arm and not
    // this channel.
    //
    // With the arm absent this is the shipped predicate.
    let recv_request_law = request_law_armed(
        window_reliable,
        window_generation,
        gates.recv_request_law,
    );
    // Arm (B) alone is the vocabulary-only wiring test: the trigger stays the
    // shipped 2 ms sampler, so the gap producer stays armed and only the
    // message the receiver sends changes. The two arms compose; neither
    // selects a machine.
    let recv_rank_feedback = request_law_armed(
        window_reliable,
        window_generation,
        gates.rank_feedback,
    );
    let sender_nack_tx: Option<
        tokio::sync::mpsc::Sender<(FireCause, u32, Vec<(u64, u64)>)>,
    > =
        if !window_generation && !recv_request_law {
            Some(nack_tx)
        } else {
            None
        };
    // The request channel's producer seat: `Some(..)` iff either arm is live,
    // so arm (B) alone still has a server. `None` on every shipped path => an
    // arriving `RepairRequest` is counted and dropped.
    let recv_request_tx: Option<tokio::sync::mpsc::Sender<RepairRequestBatch>> =
        if recv_request_law || recv_rank_feedback {
            Some(request_tx)
        } else {
            None
        };
    if recv_request_law || recv_rank_feedback {
        // Mechanism-liveness echo (measurement-discipline rule 1), emitted in
        // `run_impl` so both roles echo: the receiver builds the message, the
        // sender serves it.
        info!(
            request_law = recv_request_law,
            rank_feedback = recv_rank_feedback,
            gap_producer_armed = sender_nack_tx.is_some(),
            "receiver-seat repair request ACTIVE (paper §7.6: the receiver \
             REQUESTS repairs at lateness l >= l*_recv instead of the sender \
             inferring them from inverted SACK ranges; the per-seq SACK->gap \
             producer is suppressed by the collision seam while the request \
             law is armed; sack_tx UNTOUCHED - ADR-0060 store release)"
        );
    }
    // SACK forwarding channel producer, for the SACK-clocked store release
    // (env `RWM_STORE_SACK_RELEASE`, default on; paper §6.3, ADR-0060): the
    // sender uncounts SACKed ranges from the flow-control outstanding — see
    // the sender-loop drain — but never prunes the payload, which is the only
    // retransmittable copy of a received-then-evicted symbol (slot release,
    // never recoverability). `=0` is the frontier-only-release opt-out.
    let store_sack_release_enabled = gates.store_sack_release;
    let sender_sack_tx: Option<tokio::sync::mpsc::Sender<SackReport>> =
        if store_sack_release_enabled
            && window_reliable
            && !window_generation
            && !window_coded_only
        {
            Some(sack_tx)
        } else {
            None
        };
    // Clone tx before moving tun into the sender task
    let recv_tun_tx = tun.tx.clone();

    let sender_transport = transport_arc.clone();
    let sender_batch_counter = batch_counter.clone();
    let sender_stats = stats.clone();

    let sender_profile_symbol_size = symbol_size;
    // The backend chosen above is pinned for the life of the stream (paper
    // §5.10): a mid-stream switch strands every in-flight symbol of the old
    // code (no cross-code algebra) and discards the estimator/ARQ state
    // recovery needs.
    let sender_fec_backend = effective_fec_backend;
    let sender_window_reliable = window_reliable;
    let sender_window_coded_only = window_coded_only;
    let sender_window_generation = window_generation;
    let sender_window_systematic = window_systematic;
    let sender_window_ack = window_ack_seq.clone();
    let mut sender_in_rx = sender_in_rx;
    let mut sender_cmd_rx = sender_cmd_rx;
    let mut sender_nack_rx = nack_rx;
    let mut sender_deficit_rx = deficit_rx;
    let mut sender_sack_rx = sack_rx;
    let mut sender_request_rx = request_rx;
    let sender_protocol_hint = config.protocol_hint;
    // Paper §4.6: the completion feed, if a driver published one. `None` on
    // every shipped path.
    let sender_completion_feed = config.completion_feed.clone();
    let sender_gates = gates.clone();

    // Threading Q2: the TX half's owner. The scheduler and the FEC controller
    // move into the sender task and are used there by plain `&mut` — no other
    // task can reach them.
    let sender_tx_diag = gates.diag;
    let sender_handle = tokio::spawn(crate::task_obs::timed("sender", async move {
        let mut txc = TxCore::new(scheduler, fec_controller, sender_tx_diag);
        run_window_sender(
            &mut tun,
            sender_profile_symbol_size,
            sender_fec_backend,
            &mut txc,
            &sender_batch_counter,
            &sender_transport,
            &sender_stats,
            &sender_window_ack,
            SenderInputs {
                acks: &mut sender_in_rx,
                cmds: &mut sender_cmd_rx,
                nack_tx: sender_nack_tx.as_ref(),
                sack_tx: sender_sack_tx.as_ref(),
            },
            &mut sender_nack_rx,
            &mut sender_deficit_rx,
            &mut sender_sack_rx,
            &mut sender_request_rx,
            &mut sender_shutdown_rx,
            sender_protocol_hint,
            sender_window_reliable,
            sender_window_coded_only,
            sender_window_generation,
            sender_window_systematic,
            sender_copa_feed,
            sender_completion_feed,
            sender_gates,
        )
        .await;
    }));

    // Receiver task: receive → decode → extract packets → TUN inject
    // (threading Q2: it owns the scheduler's RX half as a local; its TX-state
    // inputs go to the sender as messages).
    let recv_sender_in = sender_in_tx.clone();
    let recv_sender_cmd = sender_cmd_tx.clone();
    let recv_fec_backend = effective_fec_backend;
    let recv_transport = transport_arc.clone();
    // Per-path: track last seen batch_seq and total symbols received for loss detection
    let path_batch_tracking: Arc<DashMap<u32, PathBatchTracker>> = Arc::new(DashMap::new());

    let recv_path_tracking = path_batch_tracking.clone();
    let recv_stats = stats.clone();
    let recv_symbol_size = symbol_size;
    let recv_window_reliable = window_reliable;
    // Out-of-order object delivery (the H → ∞ corner, paper §4.11) is only
    // meaningful on the reliable window (it needs retention to guarantee
    // every hole is eventually recovered). The run() tunnel path never sets
    // window_out_of_order — only the native object API does. Coded-only
    // (fungible frontier) also forces out-of-order delivery: with no
    // systematic source on the wire the decoder emits each source seq only
    // when it is recovered by GE, in arbitrary order, so the receiver must
    // deliver-on-decode (there is no systematic in-order arrival to hold to).
    // Generation coding (paper §5.8) is likewise out-of-order: each
    // generation decodes on any K_G coded symbols and its sources are emitted
    // as they are recovered, reassembled by offset at the object layer.
    let recv_window_ooo = window_reliable
        && (config.window_out_of_order || window_coded_only || window_generation);
    // Fungible frontier (paper §5.5): the decoder must retain the wider W_mp
    // coding window so a coded symbol can still combine over its full span;
    // mirror the sender's win_cap (default 640, RWM_WINDOW override) or keep
    // 200. Generation mode retains the whole in-flight pipeline (M
    // generations of G symbols) so no not-yet-decoded generation is ever
    // pruned early.
    let recv_win_cap: u64 = if window_generation {
        let g = gates.gen_size;
        let mut m = gates.pipeline;
        // Under the derived-depth pipeline the sender may run up to
        // GEN_PIPE_MAX_GENS generations of read-ahead, so the receiver must
        // retain that whole span (prune bound only).
        if gates.gen_pipe {
            m = m.max(GEN_PIPE_MAX_GENS);
        }
        ((g.max(1) * (m.max(1) + 1)).max(MAX_WINDOW_SIZE)).min(1 << 20) as u64
    } else if window_coded_only {
        gates
            .window_override
            .unwrap_or(640)
            .clamp(MAX_WINDOW_SIZE, 4096) as u64
    } else {
        MAX_WINDOW_SIZE as u64
    };
    let recv_window_generation = window_generation;
    // Receiver arm of the deficit-feedback loop: the data-arm control handler
    // forwards inbound GenerationDeficit vectors to the local sender's recovery
    // loop over this clone (generation mode only).
    let recv_deficit_tx = deficit_tx.clone();
    // SACK + BDP reassembly (RWM_REASM_BDP) hardens the receiver so a sender
    // decoupled from the in-order frontier is safe for reliable in-order
    // delivery. The invariant it guarantees: a received symbol is never
    // evicted from the receiver's reassembly state before it is delivered
    // (its in-order frontier passes), so a symbol whose sender-side slot was
    // released on SACK always survives at the receiver until use → no
    // un-recoverable eviction. Concretely it (a) clamps the
    // window-decoder/received-seq prune so it can never advance above the
    // delivered frontier (the reorder buffer is already usize::MAX /
    // non-evicting), and (b) probes the reassembly occupancy so the bound can
    // be reported (`[REASM]`). The reassembly stays BDP-bounded because the
    // sender's outstanding is bounded (plain_dyn_cap = gain·BDP store cap,
    // default on) and working FEC recovers holes fast. Default off.
    let reasm_bdp_on = gates.reasm_bdp;

    // ack-merge (RWM_ACK_MERGE): hoisted for the receiver's per-batch hot
    // path.
    let ack_merge_recv = gates.ack_merge;
    // ack-merge density gauge (`[CTLD]`), RWM_DIAG only — behavior-inert.
    let recv_diag_on = gates.diag;

    // Engine-receiver saturation probe. RWM_RDIAG=1 samples (a) the engine
    // task's busy fraction
    // (1 − time-awaiting-select / wall) and (b) the inbound msg-channel depth
    // (queued behind the single engine task). Distinguishes "the engine task
    // is the service-rate wall" (busy→100%, q deep) from "the wall is
    // upstream" (busy low, q empty). Probe only — no behavior change; the
    // WeakSender adds no channel-close semantics.
    let rdiag_probe = msg_tx.downgrade();

    let recv_gates = gates.clone();
    // The receiver task (net/receiver.rs). `run_receiver` is an `async fn`,
    // so building its future here runs none of its body — the task starts
    // executing when the runtime polls it.
    let receiver_handle = tokio::spawn(crate::task_obs::timed("receiver", receiver::run_receiver(
        recv_shutdown_rx,
        msg_rx,
        recv_tun_tx,
        recv_sender_in,
        recv_sender_cmd,
        recv_fec_backend,
        recv_transport,
        recv_path_tracking,
        recv_stats,
        recv_symbol_size,
        recv_window_reliable,
        recv_window_ooo,
        recv_win_cap,
        recv_window_generation,
        recv_deficit_tx,
        // Paper §7.6 arms (A)/(B): the request producer and the two
        // resolved arm predicates. `None`/`false` on every shipped path.
        recv_request_tx,
        recv_request_law,
        recv_rank_feedback,
        reasm_bdp_on,
        ack_merge_recv,
        recv_diag_on,
        rdiag_probe,
        recv_gates,
        config.reorder_timeout_ms,
        config.reorder_max_size,
        // The contract's own dial position at the receiver: the same
        // `config` field the sender's policy reads.
        config.protocol_hint,
    )));

    // Path management command channel (for runtime add/remove via HTTP API)
    let (path_cmd_tx, path_cmd_rx) = mpsc::channel::<crate::monitor::http::PathCommand>(16);

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
    let cmd_sender = sender_cmd_tx.clone();
    let cmd_stats = stats.clone();
    let cmd_msg_tx = msg_tx.clone();
    let cmd_ctrl_tx = ctrl_tx.clone();
    let next_path_id = Arc::new(AtomicU64::new(config.bind_addrs.len() as u64));
    let cmd_shutdown_rx = shutdown_tx.subscribe();
    let cmd_handle = tokio::spawn(crate::task_obs::timed("pathcmd", tasks::run_path_cmd(
        path_cmd_rx,
        cmd_transport,
        cmd_sender,
        cmd_stats,
        cmd_msg_tx,
        cmd_ctrl_tx,
        sender_in_tx.clone(),
        next_path_id,
        cmd_shutdown_rx,
    )));

    // RTCP-style periodic report + keepalive task
    let report_transport = transport_arc.clone();
    let report_sender = sender_cmd_tx.clone();
    let report_stats = stats.clone();
    let report_symbol_size = symbol_size;
    let report_shutdown_rx = shutdown_tx.subscribe();
    let report_handle = tokio::spawn(crate::task_obs::timed("report", tasks::run_report(
        report_transport,
        report_sender,
        report_stats,
        report_symbol_size,
        report_shutdown_rx,
    )));

    // Control fast path: liveness-critical messages that arrive on the
    // reliable stream (PathReport, Ping) go straight to the sender's input
    // channel — they act on TX state, which the sender owns (threading Q2) —
    // and never queue behind the data flood; anything else is forwarded to
    // the receiver's ordered data loop.
    let ctrl_forward_tx = msg_tx.clone();
    let ctrl_handle = tokio::spawn(crate::task_obs::timed(
        "ctrlfast",
        tasks::run_control_fastpath(ctrl_rx, sender_in_tx.clone(), ctrl_forward_tx),
    ));

    // Any task completing — even cleanly — ends the tunnel, so every arm
    // must say which task exited and why; a silent `_ = handle => {}` arm
    // would let a task that returns instantly shut the tunnel down with no
    // log line.
    tokio::select! {
        r = sender_handle => { log_task_exit("sender", &r); r?; }
        r = receiver_handle => { log_task_exit("receiver", &r); r?; }
        r = report_handle => { log_task_exit("path-report", &r); r?; }
        r = cmd_handle => { log_task_exit("path-cmd", &r); r?; }
        r = ctrl_handle => { log_task_exit("control-fastpath", &r); r?; }
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

/// Extract application packets from a delivered window symbol's payload.
/// Packed mode carries block-mode framing (multiple packets per symbol);
/// unpacked mode carries a single window-framed packet.
fn extract_window_packets(data: &Bytes, packed: bool) -> Vec<Vec<u8>> {
    if packed {
        framing::extract_packets(data)
    } else {
        framing::extract_window_packet(data).into_iter().collect()
    }
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

/// The source paths that carried the symbols currently in the coding window —
/// the `covered_paths` argument for repair placement (paper §5.7). One
/// entry per in-window source symbol (with multiplicity), so the placement
/// law's fate term is the fraction of the repair's coverage on each path. A
/// fungible repair covers the whole window; entries that predate the window
/// (still in the retained map) are excluded by the span filter.
///
/// Written into the caller's scratch `out` (cleared first) rather than a
/// fresh `Vec`: this runs once per correction symbol and the window is up to
/// `win_cap` entries long.
fn window_source_paths_into(
    encoder: &dyn WindowEncoder,
    source_path_map: &seq_ring::SeqRing<u32>,
    out: &mut Vec<u32>,
) {
    out.clear();
    let (win_start, win_end) = encoder.window_span();
    out.extend((win_start..=win_end).filter_map(|seq| source_path_map.get(&seq).copied()));
}

/// Sliding-window sender loop. Reads packets from TUN, frames them as individual
/// source symbols, sends them immediately, and periodically generates repair symbols.
async fn run_window_sender(
    tun: &mut TunInterface,
    symbol_size: u16,
    fec_backend: FecBackend,
    // Threading Q2: the scheduler's TX half and the FEC controller, owned by
    // this task and used by plain `&mut` (no mutex anywhere).
    txc: &mut TxCore,
    batch_counter: &BatchCounter,
    transport: &Arc<QuicTransport>,
    stats: &Arc<SharedStats>,
    window_ack_seq: &Arc<AtomicU64>,
    // Threading Q2: the sender's input channels (the TX-direction control
    // input — acks, PathReport, Ping — and the cold tasks' commands) and the
    // ack handling's channel seats.
    inputs: SenderInputs<'_>,
    nack_rx: &mut tokio::sync::mpsc::Receiver<(FireCause, u32, Vec<(u64, u64)>)>,
    // Generation-deficit feedback (paper §5.8): each element is the
    // receiver's reported (generation_anchor, residual_deficit) vector.
    // Drives the bounded, targeted recovery emission.
    deficit_rx: &mut tokio::sync::mpsc::Receiver<Vec<(u64, u32)>>,
    // SACK-clocked release (paper §6.3): the receiver's received-above-
    // frontier ranges. Draining these uncounts out-of-order deliveries from
    // the plain-reliable flow-control gate (store_len) so it tracks true
    // outstanding, decoupling the sender from the in-order cumulative
    // frontier. Only fed in plain-reliable mode; never producing otherwise.
    sack_rx: &mut tokio::sync::mpsc::Receiver<SackReport>,
    // Receiver-seat repair requests (paper §7.6 arms (A)/(B)): each
    // element is `(cause, spans)` with `spans[i] = (start, count,
    // deficit)`. Drives the span-serving loop that mirrors the
    // generation-deficit recovery emission. Never produced with both arms
    // absent — `recv_request_tx` is `None` there — so the select arm
    // below is disarmed and this receiver is silent for the whole run.
    request_rx: &mut tokio::sync::mpsc::Receiver<RepairRequestBatch>,
    shutdown_rx: &mut tokio::sync::broadcast::Receiver<()>,
    protocol_hint: ProtocolHint,
    // Retain-until-acked retention at the ARQ layer (see the policy block
    // above RELIABLE_STORE_MAX).
    reliable: bool,
    // Fungible frontier: emit only coded (random-linear-combination) symbols
    // over the window in place of raw systematic source. The source bytes are
    // still fed to the encoder window and the retention store (so ARQ can
    // retransmit the exact symbol for an aged, localized hole), but nothing
    // systematic goes on the wire during normal flow — every transmitted
    // payload symbol is a fungible combination, so no specific symbol is a
    // fixed in-order position a slow path long-poles.
    coded_only: bool,
    // Generation-based coding (paper §5.8). Codes coded symbols within fixed
    // generations of `RWM_GEN` (default 384) source symbols — a stable
    // anchor, unlike the moving sliding window — with `RWM_PIPELINE`
    // (default 2) generations concurrently in flight. Implies coded_only wire
    // symbols. It turns the per-seq targeted ARQ off (no retransmit store, no
    // NACK loop, no tail sweep): a short generation is recovered by more
    // coded symbols for that generation (fungible cross-path), never by
    // resending a specific seq — the per-seq layer makes the moving window
    // path-affine.
    generation: bool,
    // Systematic + deficit-repair (paper §5.8). A submode of `generation`
    // (the caller passes `generation=true` alongside this): the raw
    // systematic source rides the wire as primary (striped work-conserving,
    // delivered out-of-order with zero decode at the receiver's dense
    // decoder) instead of coded-only's "every symbol coded". The paced coded
    // block still runs but, via `GenerationEncoder::new_systematic`, emits
    // only the `ceil(len·r)` repair overhead per generation (plus the
    // deficit-driven top-up) — coded symbols cover only the holes, so decode
    // is O(deficit) not O(G). This avoids coded-only's decode-on-K latency
    // and O(G²) decode while keeping the same fungible cross-path recovery
    // and no per-seq ARQ.
    systematic: bool,
    // Some(..) in plain in-order mode when the Copa delivery feed is on —
    // source sends (and targeted retransmits) record seq→path + a BBR
    // send-interval rate-sample snapshot so the WindowAck handler can
    // attribute deliveries per path. None = shipped path.
    copa_feed: Option<Arc<CopaFeed>>,
    // Paper §4.6: remaining bytes of the transfer, when a driver knows them.
    // `None` on every shipped path; read only under
    // `RWM_COMPLETION_EXPOSURE`, so the rate is byte-identical without it.
    completion_feed: Option<Arc<CompletionFeed>>,
    // The engine's env-gate surface, resolved once in run_impl (src/gates.rs).
    gates: crate::gates::RuntimeGates,
) {
    // Threading Q2: the TX half, by field (disjoint `&mut` borrows of the
    // sender's own `TxCore`).
    let TxCore { sched: scheduler, fec: fec_controller, eta: eta_final } = txc;
    let SenderInputs { acks: sender_in, cmds: sender_cmds, nack_tx: ack_nack_tx, sack_tx: ack_sack_tx } = inputs;
    // The sender's input drain buffers (reused; one `recv_many` per drain).
    let mut in_buf: Vec<crate::transport::InboundBatch> = Vec::new();
    let mut cmd_buf: Vec<SenderCmd> = Vec::new();
    let mut in_closed = false;
    let mut cmds_closed = false;
    // ── The sender's resolve-once policy ────
    // The derived sender constants resolve together in
    // `SenderPolicy::resolve` (net/sender_policy.rs). The mechanism-liveness
    // `info!` echoes below read `pol`; so does the span-law trace's own
    // `now_us()` t0, which rebinds `pol` where it is sampled.
    let pol = SenderPolicy::resolve(
        &gates,
        symbol_size,
        protocol_hint,
        reliable,
        coded_only,
        generation,
        systematic,
    );
    if pol.mstar_anchor {
        info!("M* anchor hygiene ACTIVE (RWM_MSTAR_ANCHOR: measured RTprop floor + fast-seed rate filter)");
    } else if gates.mstar_anchor {
        // Mechanism-liveness echo (measurement-discipline rule 1) for the
        // plain-mode subset of the M* anchor hygiene, which is not
        // generation-gated: (a) the peer-report RTT does not feed the local
        // estimators (it would pin a 50-ms pseudo-sample floor, PathReport
        // arm) and (b) the estimator RTT EWMA seeds from its first measured
        // sample instead of crawling from the 50-ms constant (LossEstimator
        // rtt_seed_from_sample).
        info!("M* peer-report RTT-feed suppression ACTIVE (RWM_MSTAR_ANCHOR plain-live subset: local-echo-only RTT feed + estimator seed-from-sample)");
    }
    // The last cumulative point `on_ack_advance` processed (wire v9 count
    // form: 0 = nothing acked yet, so a first ack of `next_expected = 1`
    // -- seq 0 delivered -- advances and prunes seq 0).
    let mut prev_ack: u64 = 0;
    // Generation-mode paced coded emission (see the emission block in the loop).
    // The token bucket is clocked at the delivered goodput — measured from the
    // cumulative-ack (window_ack_seq) progress, i.e. the receiver-driven rate at
    // which decoded source symbols are completing. This is the true link
    // goodput and is non-circular (unlike the send-rate estimator or the
    // window-mode cwnd, which never grows past INITIAL_CWND). A small headroom
    // factor lets the rate ramp; a bootstrap floor primes the first generation
    // before any ack exists. Decouples coded emission from TUN intake so a
    // generation buffered under backpressure keeps accumulating its K_G.
    let mut gen_coded_total: u64 = 0; // cumulative coded symbols emitted
    // Sampled here, at setup; moved into SenderState below.
    let gen_last_source_us: u64 = now_us(); // last source-intake time
    // Delivered-goodput pacing: clock the token-bucket refill to the measured
    // ack (decode) rate rather than a fixed ceiling, so coded emission never
    // outruns the receiver's O(G²) decode/intake (a bursty overrun drops coded
    // symbols on the droppable datagram path). EWMA of ack deltas; a
    // bootstrap floor primes the first generation before any ack.
    let mut gen_rate_ewma: f64 = 0.0;
    let mut gen_rate_sample_us: u64 = now_us();
    let mut gen_rate_sample_ack: u64 = 0;
    // ── gen_pipe state (inert unless RWM_GEN_PIPE) ─
    // Derived pipeline depth M* + dynamic intake cap, recomputed every ~5 ms
    // from the windowed-max delivered rate and SRTT (gen_pipe_depth above).
    let mut gen_pipe_m: usize = 2;
    let mut gen_pipe_store_cap: usize = 2 * pol.gen_size;
    let mut gen_pipe_refresh_us: u64 = 0;
    // Windowed-max delivered-rate filter. The cumulative ack advances in
    // whole-generation bursts, so a rate bucket must span many generations to
    // read the true rate rather than the burst/gap alternation: bucket span
    // 2 s (≥ 4·G/R for R ≥ 768 sym/s ⇒ ≤ 25% quantization), max over the
    // last 4 buckets (8 s window, ≫ any deficit round).
    let mut gp_bucket_start_us: u64 = now_us();
    let mut gp_bucket_ack: u64 = 0;
    let mut gp_rates: std::collections::VecDeque<f64> = std::collections::VecDeque::new();
    let mut gp_rate_max: f64 = 0.0;
    // Per-generation deficit-feedback recovery state (paper §5.8), the
    // rateless-with-feedback loop:
    //   * `gen_want[a]`  — coded symbols still to emit for generation anchored at
    //                      `a`, from the last deficit report (= reported deficit
    //                      minus what was already in flight). Consumed, paced,
    //                      round-robin, by the recovery-emission block below.
    //   * `gen_emitted[a]` — cumulative coded symbols this sender has put on the
    //                      wire for generation `a` (proactive + recovery). The
    //                      receiver's reported deficit reflects everything it has
    //                      received, so `emitted − emitted_at_report` is the count
    //                      still in flight (not yet reflected) — subtracted from
    //                      the fresh deficit so we never double-send. "Send the
    //                      deficit, wait ~RTT for the updated deficit, re-evaluate."
    let mut gen_want: BTreeMap<u64, u64> = BTreeMap::new();
    // Proactive vs reactive recovery accounting. `proactive_coded_total`
    // counts coded symbols emitted by the open-loop per-generation
    // provisioning round-robin (`generate_repair`, upfront repair — no
    // feedback round-trip). `recovery_coded_total` counts coded symbols
    // emitted by the deficit-driven recovery loop (`generate_repair_for`,
    // which fires only after a receiver GenerationDeficit report — one
    // feedback round-trip). The proactive-recovery fraction =
    // proactive/(proactive+recovery) says whether holes are recovered from
    // upfront repair (fraction→1, zero round-trips) or by reactive
    // round-trips (fraction low). Printed on RWM_PFRAC/RWM_TRACE.
    let mut proactive_coded_total: u64 = 0;
    let mut recovery_coded_total: u64 = 0;
    let mut pfrac_last_us: u64 = 0;
    // `[SHEDH]` (paper §11.2, receiver hold): last emission of the
    // receiver-hold bind gauge, on the same 1 s cadence convention.
    let mut shedh_last_us: u64 = 0;
    // `[CHI]` (paper §4.6): last emission of the completion-exposure gauge,
    // same cadence.
    let mut chi_last_us: u64 = 0;
    // ── Paper §7.6 arms (A)/(B): the request-serving seat ───────────────
    //
    // The receiver-seat request law makes the receiver the authority and the
    // sender a server of spans. The bookkeeping below is the
    // generation-deficit machinery's, term for term, because it is the same
    // mechanism in a second vocabulary:
    //
    //   * `req_want[a]`     -- equations still owed over the span anchored at
    //                          `a`, with the span's own width `m`.
    //   * `req_emitted[a]`  -- cumulative answers this sender has put on the
    //                          wire for that span, ever.
    //   * `req_at_report[a]`-- `req_emitted[a]` as of the last report, so the
    //                          in-flight subtraction is "sent since the
    //                          receiver last looked" and a sub-RTT report
    //                          stream cannot re-send the same answers.
    //
    // The first-report special case is load-bearing (see the deficit
    // consumer's own note): with no baseline the in-flight count is 0, not
    // `emitted`. Initialising the baseline to 0 instead would treat every
    // earlier proactive symbol as in flight, send nothing, and deadlock the
    // loop.
    let mut req_want: BTreeMap<u64, (u16, u64)> = BTreeMap::new();
    let mut req_emitted: std::collections::HashMap<u64, u64> =
        std::collections::HashMap::new();
    let mut req_at_report: std::collections::HashMap<u64, u64> =
        std::collections::HashMap::new();
    // `[REQS]` — the serving gauge, cumulative, 1 s cadence, last line wins
    // (the `[RFA]`/`[SHEDH]` convention; a `Drop` never survives the
    // harness's SIGKILL of a server).
    //
    // Request fires are not counted into `[FCAUSE]`: `[FCAUSE]` classifies
    // fires of the SACK->gap machinery, and the proof that the collision
    // seam closed is `[FCAUSE] gap_data -> 0` on the treatment arm. Folding
    // request fires into that line would erase that reading. The cause tag
    // the receiver stamped rides here instead, in `[FCAUSE]`'s own class
    // vocabulary, so no information is lost and no existing counter changes
    // meaning.
    let mut reqs_last_us: u64 = 0;
    // Reports consumed, spans in them, and the largest `m` ever asked for.
    let mut reqs_reports: u64 = 0;
    let mut reqs_spans: u64 = 0;
    let mut reqs_m_max: u64 = 0;
    // Answers that reached the wire, by kind: a per-seq copy out of
    // `sent_store` (the `m = 1` corner — the shipped bytes exactly, which is
    // what keeps `[RFA] dup_src` comparable to the control) and a span-coded
    // equation.
    let mut reqs_copy: u64 = 0;
    let mut reqs_coded: u64 = 0;
    // WA1, the soundness precondition, counted and never assumed:
    // `generate_repair_range` refuses unless the whole span is still
    // retained. `wa1_some` is an answer in the requested vocabulary;
    // `wa1_none` is the refusal, which falls back to per-seq copies out of
    // `sent_store`. A request law whose answers silently degrade to copies is
    // the shipped machine with extra latency, so the split is a count.
    let mut reqs_wa1_some: u64 = 0;
    let mut reqs_wa1_none: u64 = 0;
    // A requested seq the store no longer holds: removal is by ack only, so
    // the receiver has it and the request is stale. Skipped, and counted.
    let mut reqs_stale: u64 = 0;
    // Answers the per-iteration budget refused. A serving loop that is always
    // budget-bound is not measuring the law it was built for.
    let mut reqs_budget_bound: u64 = 0;
    // The last cause tag the receiver stamped, in `[FCAUSE]`'s vocabulary.
    let mut reqs_cause: FireCause = FireCause::Other;
    // The arms, resolved once from the same three predicates `run_impl` used
    // to decide whether to give the receiver a producer at all — so the
    // producer and the consumer can never disagree about whether the arm is
    // live. Arm (B) alone is served too: it is the vocabulary-only wiring
    // test, and an unserved request is a deadlock, not a control.
    let request_arm = reliable
        && !generation
        && (gates.recv_request_law || gates.rank_feedback);
    let mut gen_emitted: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
    let mut gen_emitted_at_report: std::collections::HashMap<u64, u64> =
        std::collections::HashMap::new();
    // Fixed-rate pacing (token bucket) for coded emission: without it the flow
    // window is spent as one instantaneous burst of datagrams, which the QUIC
    // datagram path drops (unreliable, droppable) faster than the receiver can
    // decode — so a sealed generation never accumulates rank. The rate is a
    // generous ceiling (RWM_GEN_RATE symbols/sec, ~100 Mbit at 1.5 kB); the
    // ack-clocked flow window is the real limiter, this just spreads the bursts.
    let mut gen_tokens: f64 = 0.0;
    let mut gen_tok_last_us: u64 = now_us();
    let mut src_tok_last_us: u64 = now_us();
    // Rate signal: the delivered-goodput EWMA is clocked on the in-order
    // cumulative ack, which stalls at 0 whenever a hole wedges the frontier —
    // exactly the high-RTT-lossy case — so alone it would collapse `eff_pace`
    // to the bootstrap floor (2000 sym/s ≈ 24 Mbit) and throttle the source
    // ramp below the link. The Copa cwnd/SRTT is the frontier-independent CC
    // rate (cwnd grows on delivery feedback regardless of the in-order hole);
    // pace at max(cwnd/SRTT, goodput-EWMA)×headroom so a stalled in-order
    // frontier cannot starve the pace rate. Cached off the scheduler lock
    // every 5 ms.
    let mut cc_rate_cached: f64 = 0.0;
    let mut cc_rate_refresh_us: u64 = 0;
    // Pace ceiling = gen_rate × live-path count (single-link burst guard,
    // scaled so it cannot clamp a multi-path aggregate — see the refresh
    // block below). Starts at one link's worth.
    let mut cc_rate_ceiling: f64 = pol.gen_rate;
    // anchor → wall-clock (µs) of the last reactive emission for that generation.
    let mut gen_recover_at: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
    if pol.store_paths_on && pol.plain_dyn_cap {
        // Mechanism-liveness echo (measurement-discipline rule 1): the recorded run
        // must show which outstanding-pool law was active.
        info!(
            pool_per_path = pol.store_path_pool,
            gain = pol.store_bdp_gain,
            "path-scaled outstanding pool ACTIVE (RWM_STORE_PATHS: cap = clamp(gain*N*pipe, floor, N*pool) for N>=2 live paths; N=1 legacy)"
        );
    }
    if pol.pool_anchor_on {
        // Mechanism-liveness echo (measurement-discipline rule 1).
        info!(
            pool_per_path = pol.store_path_pool,
            gain = pol.store_bdp_gain,
            "pool-anchor honest dual-store law ACTIVE (RWM_POOL_ANCHOR: N>=2 pooled cap = sum_i honest_store_cap(sr_i*RTprop_i, sr_i, K_i, gain) on the per-path send-interval anchor, clamp [floor, N*knee]; all-warm else path-scaled fallback; Copa cwnd feed untouched; N=1 legacy)"
        );
    }
    if pol.store_sack_release_on {
        // Mechanism-liveness echo (measurement-discipline rule 1).
        info!(
            "SACK-clocked store release ACTIVE (RWM_STORE_SACK_RELEASE: SACKed seqs \
             uncounted from the outstanding gate, payload + ARQ maps retained until \
             the cumulative frontier — slot release, never recoverability)"
        );
    }
    if pol.three_term_on {
        // Mechanism-liveness echo (measurement-discipline rules 1/15): asserted
        // present on the three-term arm, absent on the default arm; the
        // per-tick `[3T]` line carries the three terms separately.
        info!(
            rho = pol.contract_rho,
            b = pol.delta_b,
            "three-term outstanding limit ACTIVE (RWM_THREE_TERM: \
             the plain dyn-store-cap is Sigma_i rate_i*K_i*RTprop_i (network window) + \
             Sigma_i rate_i*stall(delta,rho,i) (emission slack) + 2*rate_fast*skew \
             (resequencing span), each Little's law over a measured signal with no fitted \
             coefficient; the span term is identically 0 at one path because \
             skew = (max RTprop - min RTprop)/2 is 0 over a one-element set, which is what \
             retires the active_paths()/live_paths() topology branch without an if N==1; \
             RWM_THREE_TERM=0 = the shipped-default control arm)"
        );
    }
    if pol.honest_cap_on {
        // Mechanism-liveness echo (measurement-discipline rule 1): asserted
        // present on honest-cap arms, absent on knee-clamp control arms.
        info!(
            gain = pol.store_bdp_gain,
            floor = pol.store_cap_floor,
            pool_per_path = pol.store_path_pool,
            "honest floor-clock store caps ACTIVE (RWM_PLAIN_RS+RWM_HONEST_CAP: cap_i = anchor_i*(K_i+gain-1) + rate_i*(gain-1)*R, K_i = windowed-min echoSRTT/RTprop, R = 100ms recovery-round bound; anchor-sum at N=1; RWM_HONEST_CAP=0 = floor-law control)"
        );
    }
    // The store-cap refresh phase's own state (see `StoreCapState`).
    let mut scs = StoreCapState::new(&pol);
    if pol.taper_r_budget {
        // Mechanism-liveness echo (measurement-discipline rule 1).
        info!(
            "budget-conserving taper emission ACTIVE (RWM_TAPER_R: plain-mode proactive repair budgeted at r x source per coding window; legacy = r per ack cycle)"
        );
    }
    if pol.unified_span {
        // Mechanism-liveness echo (measurement-discipline rule 1).
        info!(
            hint = ?protocol_hint,
            "unified span law ACTIVE (RWM_UNIFIED: plain-mode proactive repair over the trailing solvable span [end-A*, end-Δ), A* from δ)"
        );
    }
    if pol.astar_anchor_on {
        info!("A* send-rate anchor ACTIVE (RWM_ASTAR_ANCHOR: windowed-max send rate over ~8 SRTT, clock-gap sample discard)");
    }
    if pol.shed_on {
        // Mechanism-liveness echo (measurement-discipline rule 1).
        info!(
            "unified overload shedding ACTIVE (RWM_UNIFIED_SHED: past-deadline holes shed within the derived 1-rho budget; =0 = serializing arm)"
        );
    }
    // `RWM_CLOCK_GAP` (ADR-0061): the process-clock stall witness — a
    // dedicated 50-ms timer tick; a tick interval ≫ the period is a
    // whole-process scheduler stall (the timer wheel itself froze), and the
    // ack-fed estimator feed sites (Ack/WindowAck/PathReport arms + the
    // report-tick throughput feed) discard samples for the quarantine window
    // (the post-stall release flood would poison BtlBw, cwnd and the RTT
    // EWMA). Ack silences with a live process never trip it (see
    // control::anchor::StallWitness; an arrival-clock witness would mis-fire
    // on normal recovery quiet periods).
    if let Some(w) = crate::control::anchor::stall_witness() {
        info!("clock-gap estimator hygiene ACTIVE (RWM_CLOCK_GAP: process-clock stall witness, post-stall sample discard at the ack feed sites)");
        tokio::spawn(async {
            let mut iv = tokio::time::interval(Duration::from_millis(50));
            iv.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                iv.tick().await;
                w.tick_now();
            }
        });
    }
    // ~500 ms sender-side span-law trace (RWM_DIAG only) — the live A*/Δ,
    // owed budget, window span vs the
    // cumulative ack. Names whether a collapse rep's emission is re-covering
    // a stalled region / has A* pinned / budget saturated. (Own t0, distinct
    // from the DIAG block's own `diag_start_us` below; carried into the
    // emission step as `SenderPolicy::span_diag_start_us`.)
    let pol = SenderPolicy { span_diag_start_us: now_us(), ..pol };
    /// Congestion-aware NACK repair throttle.
    let mut nack_congestion = NackCongestionState::new();
    // Sampled here, at setup; moved into SenderState below.
    let last_source_send_us: u64 = now_us();
    /// NACK repairs sent in the current reporting period (ADR-0050 budget tracking).
    let mut nack_repairs_this_period: u64 = 0;
    /// Cached throttle/ADR-0050 budget state, refreshed every
    /// NACK_REPAIR_COOLDOWN_US (gap acks arrive far more often than the
    /// budget inputs move; recomputing per ack would just churn locks).
    let mut last_budget_refresh_us: u64 = 0;
    let mut cached_max_repairs: u64 = MAX_NACK_REPAIRS_PER_NACK as u64;
    let mut cached_nack_budget: u64 = MAX_NACK_REPAIRS_PER_NACK as u64;
    /// When the tail sweep last fired (µs) — rearm point. Must advance
    /// on every fire even if the retransmit was skipped (cooldown/budget
    /// exhausted), or a past deadline keeps the timer arm permanently ready
    /// and the select! busy-spins, starving TUN reads.
    let mut last_tail_sweep_us: u64 = 0;
    /// The derived recovery round's one-shot mechanism-liveness echo at the
    /// sender site (ACTIVE + DIVERGED). Observation only.
    let mut derived_round_echo = DerivedRoundEcho::default();
    // `[RACK]` (paper §9.1): the shipped clamp's recovery-fire false-alarm
    // accounting against RFC 8985's 1/16 spurious budget.
    let mut rack_echo = RackClockGauge::new();
    // `[FCAUSE]`'s configuration contract: under generation coding the
    // SACK→gap producer is suppressed (`recv_nack_tx = None`), so both
    // `gap_` classes are structurally empty and only the sender's own
    // tail-sweep timer can fire. The line says which machine it measured.
    rack_echo.set_send_generation(pol.generation);
    // `[QCLK]` — the realized recovery clock as a distribution, at the site
    // that runs it; see the gauge's decl.
    let mut qclk_echo = QuantileClockGauge::new("sender");
    // `[HOLD]` (paper §7.4) — the hold-down arm, on the path that takes most
    // of the fires. Constructed on every arm, control included: with
    // `RWM_HOLDDOWN_Q` absent it stamps nothing, allocates nothing,
    // suppresses nothing, and prints `q=unset` — so an absent arm is read off
    // the run's own output rather than inferred.
    let mut hold_echo = HoldDownGauge::new("sender", pol.holddown_q);
    hold_echo.set_send_generation(pol.generation);



    /// SACK-clocked store release (RWM_STORE_SACK_RELEASE): seqs currently
    /// retained in `sent_store` but uncounted from the flow-control
    /// outstanding (SACKed by the receiver, cumulative frontier not yet
    /// past them). Invariant: subset of `sent_store` keys — maintained by
    /// marking only retained seqs and pruning with the same cumulative
    /// `split_off` twin. Empty whenever the gate is off.
    let mut sack_released: BTreeSet<u64> = BTreeSet::new();
    /// DIAG: cumulative count of slots released by the law (mechanism
    /// liveness at the gauge — `srel=cur/cum`).
    let mut sack_released_total: u64 = 0;
    /// Wire v9: the newest `(next_expected, received_above)` pair off the
    /// SACK-bearing acks -- the store gate's evidence past the SACK prefix
    /// cap (`store_gate_released`). Stays at its default (no evidence)
    /// whenever the release law is off.
    let mut above_report = AboveReport::default();

    let mut packer = framing::SymbolPacker::new(symbol_size, std::time::Duration::from_millis(1));

    // Announce window mode to peer on all paths
    // (P1 D2: the path set is collected and the guard dropped before any
    // quinn call — no quinn call is made with the scheduler held.)
    // (Threading Q1: staged and handed to each path's I/O owner in one
    // control-lane batch, before the sender state — and its own staging —
    // exists.)
    {
        let paths = control_broadcast_paths(&*scheduler);
        let mut announce = crate::transport::TxBatch::new();
        for pid in paths {
            let _ = transport.send_control_datagram(
                &mut announce,
                pid,
                ControlMessage::WindowStart { symbol_size, backend: fec_backend, packed: pol.use_packing },
            );
        }
        transport.flush(&mut announce).await;
    }

    // GDIAG / GLIFE stall attribution (the gauge and its documentation live
    // in net/diag.rs): the clock stamp is sampled here.
    let gd_last_us = now_us();

    if pol.recov_mp {
        // Mechanism-liveness echo (measurement-discipline rule 1).
        info!(
            law = pol.recov_mp_law,
            "multipath recovery suppression ACTIVE (RWM_RECOV_MP: \
             per-flight RFC9002-style time-threshold hole law on the flight \
             path's smoothed clocks; \
             N=1 live path keeps legacy gates bit-exactly)"
        );
    }
    if pol.recov_sp {
        info!(
            "single-path hole-law suppression ACTIVE (RWM_RECOV_SP: RFC9002 \
             time-threshold on the live flight at N=1; time channel only)"
        );
    }
    // Per-path delivered-seq evidence for the RFC 9002 §6.1.1 packet
    // threshold (recov_mp_law): sorted, appended monotonically from each gap
    // report's implied delivered intervals (each seq ingested at most once —
    // `mp_evid_max` is the ingestion watermark), pruned at the cumulative
    // ack. Bounded by the outstanding span.
    let mut mp_delivered: std::collections::HashMap<u32, Vec<u64>> =
        std::collections::HashMap::new();
    let mut mp_evid_max: u64 = 0;
    // Derived-patience gauge: every `mp_time_threshold_us` evaluation is
    // classified: did the kGranularity floor win, or the 9/8·srtt clock? Plus
    // the running
    // sum of the floors actually used, for the mean. Printed as
    // `pf=<floor>/<clock>/<mean floor µs>` inside `mpr[…]`. `Cell` because
    // the evaluation happens inside a shared closure.
    let mpd_pf_floor: std::cell::Cell<u64> = std::cell::Cell::new(0);
    let mpd_pf_clock: std::cell::Cell<u64> = std::cell::Cell::new(0);
    let mpd_pf_sum: std::cell::Cell<u64> = std::cell::Cell::new(0);

    if pol.emit_batch_on {
        // Mechanism-liveness echo (measurement-discipline rule 1).
        info!(
            burst = pol.emit_burst,
            "emission batching ACTIVE (RWM_EMIT_BATCH: pacer-quantum TUN \
             intake + per-burst taper/span refresh; flow-control and pacing \
             contracts enforced at symbol granularity)"
        );
    } else if !crate::gates::get().emit_batch {
        // Echoed both ways (measurement-discipline rule 15c): with the default
        // ON, the `=0` control arm must witness that the knob reached the
        // binary. Keyed on the KNOB, not on the composed `emit_batch_on`, so
        // a scope-excluded run (next branch) never reads as the control arm.
        info!("emission batching OFF (RWM_EMIT_BATCH=0: per-symbol emission)");
    } else {
        // Knob on, policy out of scope: the pre-existing scope of
        // `SenderPolicy::emit_batch_on` (ρ < 1, Realtime packing, coded wire;
        // status §3.3 lists the first two as NO-MODE-SWITCH debts).
        info!(
            reliable = pol.reliable,
            packing = pol.use_packing,
            coded_wire = pol.coded_wire,
            "emission batching out of scope (RWM_EMIT_BATCH=1 but per-symbol \
             emission: the burst intake serves the window-reliable plain-source \
             emitter only)"
        );
    }

    // Slow-path conversion diagnosis gauges (RWM_DIAG only — behavior-inert).
    // The cumulative per-path maps live in `DiagState` (net/diag.rs) with
    // their documentation; `c8c_src_placed`, which the emission step writes,
    // is in `SenderState`. The stall clock below stays a local: it is the
    // ack-advance edge detector, not a reported counter.
    let mut c8c_last_ack_adv_us: u64 = 0;

    // ── The emission seam ───────────────────
    // The per-symbol emission step mutates the sender's loop state, which
    // lives in `SenderState`; the resolve-once configuration the step reads
    // is `SenderPolicy`; the shared engine handles are `SenderCtx`. Each
    // emission site is one `emit_source(..)` call — see net/emit_source.rs.
    // Both wall-clock stamps below were sampled during setup (above) and are
    // moved in, not re-sampled.
    let mut st = SenderState::new(
        fec_backend,
        symbol_size,
        pol.gen_size,
        pol.pipeline,
        systematic,
        generation,
        pol.gen_repair_floor,
        gen_last_source_us,
        last_source_send_us,
    );
    // The emission step's handles, built at each emission site: the
    // scheduler and the FEC controller are lent to it as `&mut` for that call
    // only (threading Q2).
    macro_rules! sctx {
        () => {
            SenderCtx {
                scheduler: &mut *scheduler,
                fec_controller: &mut *fec_controller,
                transport,
                stats,
                batch_counter,
                window_ack_seq,
                copa_feed: copa_feed.as_ref(),
            }
        };
    }
    // Threading Q2: what the input handling (`on_window_ack` & co., run
    // here) reads besides the TX half — shared handles only.
    let input_ctx = tx_inputs::InputCtx {
        transport,
        stats,
        peer_window_ack: window_ack_seq,
        nack_tx: ack_nack_tx,
        sack_tx: ack_sack_tx,
        copa_feed: copa_feed.as_ref(),
        mstar_anchor: gates.mstar_anchor,
    };

    // Retention backpressure state (reliable mode), for edge-triggered logs.
    let mut last_tx_paused = false;

    // Boot cap before the BtlBw anchor warms (a few RTTs); ~1.5× a 100 Mbit/
    // 10 ms BDP, same rationale as the plain-reliable store_boot_cap.
    let mut dyn_infl_cap: u64 = if pol.infl_bdp_on { 128 } else { pol.infl_cap };
    let mut dyn_infl_refresh_us: u64 = 0;
    // ── `[CCAP]` — the composed law's engagement + bind-fraction gauge ────
    // (CLAUDE.md formula-first rule: "every clamp gets a bind-fraction gauge,
    // reported"). An arm that is bit-identical to control must be readable as
    // a null result and not as a null effect: a law that was configured but
    // never engaged (a cold live path at every refresh) and a law that
    // engaged and changed nothing are different findings. These counters
    // separate them, and they report the two surviving bounds — the memory
    // bound `WIN_STORE_MAX` (a resource limit stated outside the law) and
    // the constant `store_cap_floor` = 64 — so neither can bind silently.
    //
    // The counters live inside `SenderTeardownGauges`, whose destructor is
    // the single emission site for both `[WALL]` and `[CCAP]` — see that type
    // for why the teardown `select!` arms are the wrong site (the `perf`
    // harness takes neither).
    // The sender `[ETA]` exit flush (`net/eta.rs` `SenderEtaFlush`):
    // exactly once, `final=1`, at the clean exits below or — when this task
    // is dropped instead — by `TxCore`'s destructor, which owns the
    // scheduler it renders from (threading Q2: no lock to try).
    let mut ccap = SenderTeardownGauges::new(
        // The `[CCAP]` line is emitted for either door into the brake,
        // because its `brake=<closed>/<ticks>` field is the primary readout
        // of both the extraction arm and the composed arm.
        pol.composed_cap || pol.late_brake,
        pol.store_cap_floor,
    );
    // `[SUMCAP]` (paper §6.1): the `×N` deletion's engagement echo. Fed on
    // both arms at every pooled-law refresh — including the counterfactual, so
    // "did the gate change anything" is answerable from one run — and emitted
    // only on the on arm, so the shipped default's output is unchanged.
    let mut sumcap = SumCapGauge::new(
        pol.sum_cap,
        pol.store_cap_floor,
        pol.store_path_pool,
        pol.delta_cap,
        pol.delta_b,
    );
    // `[DCAP]` (paper §6.1): the δ-priced value multiplier's engagement echo.
    // Fed on both arms at every pooled-law refresh — including the
    // counterfactual against the shipped `gain`, so "did the derived
    // multiplier change anything" is answerable from one run — and emitted
    // only on the on arm, so the shipped default's output is unchanged.
    let mut dcap = DeltaCapGauge::new(
        pol.delta_cap,
        pol.store_cap_floor,
        pol.store_path_pool,
        pol.sum_cap,
        pol.delta_b,
    );
    // The periodic DIAG report clock (net/diag.rs): both stamps are sampled
    // here.
    let diag_start_us = now_us();
    let diag_last_us = now_us();
    // Emission-gap gauge (net/diag.rs): the stamp is sampled here.
    let sidle_last_change_us = now_us();
    // Window/MTU diagnosis (behavior-inert, RWM_DIAG only): the outstanding
    // split a frontier-decoupled law would gate on. `wnd2=<head>/<hole>` —
    // head = last_sent − release_frontier (the live head span: in-flight +
    // queue, everything above the highest SACK/cum-covered seq), hole =
    // unSACKed total − head (recovery-stalled seqs below the frontier).
    // `relgap=<cur>/mx<max>ms` — time since the release frontier (max of
    // SACK-release max and cum ack) last advanced, max per DIAG window: the
    // release-clumping gauge.
    let mut wnd2_frontier_last: u64 = 0;
    let mut wnd2_frontier_change_us: u64 = now_us();
    // The DIAG report's counters (net/diag.rs). All four wall-clock stamps
    // above were sampled during setup and are moved in, not re-sampled.
    let mut dg = DiagState::new(gd_last_us, diag_start_us, diag_last_us, sidle_last_change_us);
    // The last `select!` resolution instant, for the dead-wall gauge's body
    // time when `[DIAG]` (which keeps its own) is off.
    let mut wall_resolved_us: u64 = gd_last_us;
    // Scratch for the SACK drain's newly-released seqs (reused across
    // iterations; `sack_release_mark_into` clears it per range).
    let mut sack_newly: Vec<u64> = Vec::new();
    // Threading P1, D11: ONE tail-sweep `Sleep` for the life of the loop,
    // `reset()` when the deadline moves (compared as the µs deadline, not
    // the converted `Instant`, which jitters by the clock-pair conversion).
    // The arm is guarded by `tail_deadline.is_some()`, so with no deadline it
    // is disabled exactly as the old `pending()` future was. Until then the
    // loop built a fresh `sleep_until` future every iteration, registered it
    // in tokio's global timer wheel when the `select!` polled it and
    // deregistered it on drop.
    let tail_sleep = tokio::time::sleep_until(tokio::time::Instant::now() + Duration::from_secs(86_400));
    tokio::pin!(tail_sleep);
    let mut tail_sleep_us: Option<u64> = None;
    loop {
        // Scheduler reads in this loop are per phase, each under its own
        // acquisition, taken where the value is used. A loop-top snapshot
        // shared by the phases would not help:
        //   1. Every phase below composes its scheduler-derived inputs under
        //      one acquisition already, and the only rate×RTprop product in
        //      play (`copa_bdp_anchor` = `max_bw × min_rtt`) is atomic inside
        //      one `CopaState`. No derived value spans two acquisitions.
        //   2. The phases are independently throttled (~5 ms each, separate
        //      stamps), so they fire on different iterations and would
        //      consume different snapshots regardless.
        //   3. The one skew-exposed site, the reactive deficit-spacing read,
        //      sits after the `select!` await; a loop-top capture would serve
        //      it a value older by the whole park, where the per-phase read
        //      is fresh.
        // Intra-iteration consistency, if ever needed, must capture per
        // phase-group after the await, not once at the top.

        // ── Threading Q2: the sender's inputs, at the loop top ────────────
        // Non-blocking: each channel is drained by ONE `recv_many` polled
        // once, and only when it is non-empty (one coop-budget unit per
        // drain, never per message — the P2a lesson). Commands first (a path
        // added at runtime exists before its first ack), then the
        // TX-direction control input: the client's WindowAcks run
        // `on_window_ack` here, in the task that owns the scheduler, so their
        // SACK report and gap batch are on this loop's own channels before
        // the SACK drain below reads them. The TX view is published once per
        // drained batch; the receiver's RX publication is mirrored in every
        // iteration (`sync_rx`).
        {
            let mut touched = false;
            if !cmds_closed && !sender_cmds.is_empty() {
                if let Some(n) = tx_inputs::poll_once(
                    sender_cmds.recv_many(&mut cmd_buf, SENDER_CMD_DEPTH),
                )
                .await
                {
                    if n == 0 {
                        cmds_closed = true;
                    } else {
                        tx_inputs::apply_cmds(&mut cmd_buf, scheduler, fec_controller, stats);
                        touched = true;
                    }
                }
            }
            if !in_closed && !sender_in.is_empty() {
                if let Some(n) = tx_inputs::poll_once(
                    sender_in.recv_many(&mut in_buf, SENDER_IN_BATCHES),
                )
                .await
                {
                    if n == 0 {
                        in_closed = true;
                    } else {
                        tx_inputs::apply_inputs(&mut in_buf, scheduler, &mut st.tx, &input_ctx);
                        touched = true;
                    }
                }
            }
            if touched {
                scheduler.publish_tx_view(stats);
            }
            scheduler.sync_rx(stats);
        }

        // SACK drain: consume the receiver's received-above-frontier ranges
        // non-blocking at the top of every iteration (never as a select! branch
        // — a frequently-ready channel there would race, and cancel, the
        // `tun.read_packet()` future and starve/stall intake). An out-of-order-
        // received symbol is delivery evidence: the release law uncounts its
        // slot from the flow-control outstanding (the window opens at path
        // rate) while payload + ARQ maps stay retained until the cumulative
        // frontier passes it. The hole itself (not in any received range) stays
        // retained and recovers in the background via the orthogonal NACK /
        // tail-sweep path. The SACK reports are produced by this task's own
        // ack handling (threading Q2: the input drain just above and the
        // input arm of the `select!`), so a report is on this channel by the
        // next loop top after its ack.
        while let Ok(report) = sack_rx.try_recv() {
            if pol.store_sack_release_on {
                // v9: the count the gate converges on past the SACK cap.
                above_report.fold(report.next_expected, report.received_above);
            }
            for (start, end) in report.ranges {
                if end < start {
                    continue;
                }
                if pol.store_sack_release_on {
                    // SACK-clocked store release: uncount the slot (window
                    // opens, pool/account freed) — keep the payload and
                    // every recovery structure (retransmit_buffer,
                    // nack_retx_at + its per-flight RWM_RECOV_MP loss
                    // clocks, source_path_map) until the cumulative
                    // frontier passes. sack_release_mark skips seqs
                    // already released — no double-release.
                    sack_release_mark_into(
                        &st.sent_store,
                        &mut sack_released,
                        start,
                        end,
                        &mut sack_newly,
                    );
                    sack_released_total += sack_newly.len() as u64;
                    if pol.percap_track {
                        for &k in &sack_newly {
                            // Per-path account slot freed on delivery
                            // evidence (idempotent: cumulative release
                            // later finds the seq already gone — the
                            // documented no-double-release contract).
                            percap_release_seq(&mut st.percap_acct, &mut st.percap_out, k);
                        }
                    }
                }
            }
        }

        // Determine if packer has pending data for flush timer
        let packer_pending = pol.use_packing && packer.is_pending();

        // Retention backpressure: when the sent-data store is full of
        // un-acked symbols, stop reading the TUN — the inner flow sees the
        // growing TUN queue and slows down (flow control), and this loop
        // keeps servicing acks/NACKs/tail sweeps so the store drains.
        // Retention is never released by pressure, only by acks.
        // Generation mode keeps no sent_store — its backpressure signal is the
        // encoder's retained source count (= symbols in the in-flight pipeline
        // of M generations). Pausing TUN reads at store_max holds the send
        // frontier ~M generations ahead of the cumulative-decode frontier.
        // RWM_STORE_SACK_RELEASE: outstanding = retained − released. With
        // the gate off the released set is empty and no report is folded,
        // so this is exactly the shipped `sent_store.len()`; with it on,
        // released slots return to the pool (RWM_STORE_PATHS composes
        // through this same count) while their payloads stay retained for
        // recovery. Wire v9: "released" is ONE always-computed expression,
        // `max(|SACK marks|, received_above − not_retained)` clamped to the
        // retained count (`store_gate_released`), so the gate converges past
        // the `MAX_SACK_RANGES` prefix at a stalled frontier.
        let store_len = if generation {
            st.encoder.window_size()
        } else {
            sack_release_outstanding(
                st.sent_store.len(),
                store_gate_released(&st.sent_store, sack_released.len(), above_report),
            )
        };
        // gen_pipe: roll the windowed-max rate filter + recompute the derived
        // pipeline depth M* (throttled ~5 ms; the encoder setter is O(1)).
        // Under RWM_MSTAR_ANCHOR the bucket span drops 2 s → 500 ms (ADR-0061:
        // the anchor seeds from the first measured acks, not after a
        // multi-second pin; the max over 8 buckets keeps a comparable window).
        if pol.gen_pipe {
            let (gp_bucket_us, gp_ring) =
                if pol.mstar_anchor { (500_000u64, 8usize) } else { (2_000_000u64, 4usize) };
            let nowp = now_us();
            if nowp.saturating_sub(gp_bucket_start_us) >= gp_bucket_us {
                let ack_now = window_ack_seq.load(Ordering::Relaxed);
                let dt_s = (nowp - gp_bucket_start_us) as f64 / 1e6;
                let r = ack_now.saturating_sub(gp_bucket_ack) as f64 / dt_s;
                gp_bucket_start_us = nowp;
                gp_bucket_ack = ack_now;
                gp_rates.push_back(r);
                while gp_rates.len() > gp_ring {
                    gp_rates.pop_front();
                }
                gp_rate_max = gp_rates.iter().copied().fold(0.0, f64::max);
            }
            if nowp.saturating_sub(gen_pipe_refresh_us) >= 5_000 {
                gen_pipe_refresh_us = nowp;
                // RTprop (min-RTT), not the live SRTT: the live RTT includes
                // the queue this very pipeline creates — deriving depth from
                // it is positive feedback (deeper ⇒ more queue ⇒ deeper). The
                // in-flight cap holds the actual RTT near RTprop, so RTprop is
                // the self-consistent anchor (the BBR discipline).
                let rtprop_s = channel_max_rtprop_s(&*scheduler);
                let m = gen_pipe_depth(gp_rate_max, rtprop_s, pol.gen_size);
                if m != gen_pipe_m {
                    if pol.diag_on {
                        crate::readout!(
                            "[GPIPE] M* {}→{} (rate_max={:.0}sym/s rtprop={:.1}ms)",
                            gen_pipe_m, m, gp_rate_max, rtprop_s * 1000.0
                        );
                    }
                    gen_pipe_m = m;
                    st.encoder.set_pipeline_depth(m);
                }
                gen_pipe_store_cap = (gen_pipe_m * pol.gen_size).min(pol.store_max);
            }
        }
        // Refresh the BDP-derived in-flight cap (throttled ~5 ms).
        if pol.infl_bdp_on {
            let dnow = now_us();
            if dnow.saturating_sub(dyn_infl_refresh_us) >= 5_000 {
                dyn_infl_refresh_us = dnow;
                let bdp: f64 = channel_bdp_anchor_sum(&*scheduler);
                if bdp > 0.0 {
                    dyn_infl_cap = ((pol.infl_bdp_gain * bdp).ceil() as u64).max(64);
                }
            }
        }
        let eff_infl_cap = if pol.infl_bdp_on { dyn_infl_cap } else { pol.infl_cap };
        // In-flight (unacked) symbols across the pipe, for the BDP in-flight cap.
        // Fullness is also decided per path — the sender is "full"
        // (TUN-paused) only when no active path is below its own cap
        // (gain·BtlBw_i·RTprop_i), so the fast path keeps pulling source while
        // the slow path is at its RTT-inflated cap. Without per-path caps the
        // test is the global Σ in-flight ≥ Σ cap.
        // ── The late-stage brake, and the composed arm's derived cap ──────
        // `RWM_COMPOSED_CAP` (paper §10) arms this brake with no new
        // constant: the per-path cap is the path's own cwnd, i.e. the
        // congestion controller's own window, which is what a congestion
        // brake ought to be made of. `eff_infl_cap` is then irrelevant to
        // arming — the composed arm uses neither RWM_INFL_CAP's static total
        // nor RWM_INFL_BDP's gain·BDP, and neither changes meaning.
        //
        // The path set is load-bearing here. With cap_i = cwnd_i, "path i is
        // full" is `in_flight_i >= cwnd_i`, which is exactly
        // `available()_i == 0` — and `active_paths()` is *active and
        // available() > 0*. Iterating it would ask a question whose answer is
        // false by construction on every tick: the gate would resolve on, cost
        // a lock, and never brake — a null effect that reads like a null
        // result. That is why the brake reads the channel's membership
        // (`channel_paths` = `live_paths()`) — as does the gain·BDP per-path
        // test, whose full-test must not drop a cwnd-full path's in-flight
        // from Σ in_flight nor treat an all-saturated channel as the empty
        // (vacuously full) set. `cwnd_full` under this arm means: every live
        // path is at or above its own congestion window.
        //
        // `RWM_LATE_BRAKE` arms the same brake without the composed pool law
        // (`composed_cap` also forces `three_term_on`). Everything below reads
        // `cwnd_brake` rather than `composed_cap` so the two gates cannot
        // drift into two brakes — the composed arm is still exactly this
        // brake, reached by a different door.
        let cwnd_brake = pol.composed_cap || pol.late_brake;
        let brake_armed = eff_infl_cap > 0 || cwnd_brake;
        let (pipe_infl, percap_full): (u64, bool) = if brake_armed {
            let sched = &mut *scheduler;
            let mut infl = 0u64;
            let mut per_path: Vec<(u64, u64)> = Vec::new();
            let ids = channel_paths(&sched);
            for id in ids {
                if let Some(p) = sched.path_mut(id) {
                    p.expire_in_flight();
                    let fl = p.in_flight as u64;
                    infl += fl;
                    let cap_i = if cwnd_brake {
                        // The path's own congestion window. Derived, not
                        // configured; always warm (cwnd has an initial value),
                        // so this branch has no cold-start fallback to pick.
                        p.cwnd as u64
                    } else {
                        // Per-path cap = gain·(BtlBw_i·RTprop_i); fall back to
                        // the global boot cap before the anchor warms.
                        p.copa_bdp_anchor()
                            .map(|b| ((pol.infl_bdp_gain * b).ceil() as u64).max(1))
                            .unwrap_or(eff_infl_cap)
                    };
                    per_path.push((fl, cap_i));
                }
            }
            (infl, infl_percap_full(&per_path))
        } else {
            (0, false)
        };
        let cwnd_full = brake_armed
            && if pol.infl_percap || cwnd_brake {
                percap_full
            } else {
                pipe_infl >= eff_infl_cap
            };
        // `[CCAP]` engagement gauge: the brake's own liveness, counted every
        // iteration so an arm that never braked reads as a null result rather
        // than a null effect. Counted for both doors into the brake — the
        // composed arm and the `RWM_LATE_BRAKE` arm — because the distinction
        // the gauge exists to draw (`brake=0/N` armed-and-never-closed vs
        // `brake=0/0` never-armed) is what a measurement of either needs to
        // read.
        if cwnd_brake {
            ccap.brake_ticks += 1;
            if cwnd_full {
                ccap.brake_closed += 1;
            }
        }
        // Plain-reliable delay-based window cap (paper §6.1): bound the
        // outstanding store to gain×BDP so the standing queue stays ~1 RTT and
        // loss recovery does not stall behind a bloated queue. Refreshed off
        // the scheduler lock at most every 5 ms.
        refresh_store_cap(
            &mut scs,
            StoreCapCtx {
                pol: &pol,
                gates: &gates,
                scheduler: &mut *scheduler,
                copa_feed: &copa_feed,
                sumcap: &mut sumcap,
                dcap: &mut dcap,
                ccap: &mut ccap,
            },
        );
        let effective_store_cap = if pol.plain_dyn_cap {
            scs.dyn_store_cap
        } else if pol.gen_pipe {
            // gen_pipe: intake bounded at the derived M*·G — deep
            // enough to cover BDP + one deficit round, no deeper (queue-lean).
            gen_pipe_store_cap
        } else {
            pol.store_max
        };
        let tx_paused = reliable && (store_len >= effective_store_cap || cwnd_full);

        // wnd2/relgap tracking (see decls): the release
        // frontier is max(highest SACK-released seq, cumulative ack) —
        // O(log n) per iteration. Runs for the DIAG gauge only.
        if pol.diag_on && reliable && !generation {
            let tnow = now_us();
            let frontier = sack_released
                .iter()
                .next_back()
                .copied()
                .unwrap_or(0)
                .max(window_ack_seq.load(Ordering::Relaxed));
            if frontier > wnd2_frontier_last {
                wnd2_frontier_last = frontier;
                wnd2_frontier_change_us = tnow;
            } else {
                dg.wnd2_relgap_max_us = dg.wnd2_relgap_max_us
                    .max(tnow.saturating_sub(wnd2_frontier_change_us));
            }
        }
        // RWM_DIAG periodic constraint report (net/diag.rs; see the decls
        // above the loop). The guard is here so the shipped path pays
        // nothing.
        if pol.diag_on {
            diag::report(
                &st,
                &pol,
                &mut dg,
                DiagCtx {
                    scheduler: &mut *scheduler,
                    transport,
                    stats,
                    window_ack_seq,
                    copa_feed: &copa_feed,
                },
                DiagInputs {
                    tx_paused,
                    store_len,
                    effective_store_cap,
                    percap_k: &scs.percap_k,
                    sack_released: &sack_released,
                    sack_released_total,
                    gate_released: store_gate_released(
                        &st.sent_store,
                        sack_released.len(),
                        above_report,
                    ),
                    pa_engaged: scs.pa_engaged,
                    pa_sum: scs.pa_sum,
                    wnd2_frontier_last,
                    wnd2_frontier_change_us,
                    cached_nack_budget,
                    gen_rate_ewma,
                    mpd_pf_floor: &mpd_pf_floor,
                    mpd_pf_clock: &mpd_pf_clock,
                    mpd_pf_sum: &mpd_pf_sum,
                },
                symbol_size,
                reliable,
                generation,
            );
        }
        // The ack-cadence gauge (`RWM_ACKDIAG`, net/ackdiag.rs). Its own gate
        // and its own ~2 s cadence, deliberately independent of `RWM_DIAG`:
        // it is runnable on an arm that is not paying for the 250 ms report.
        // The guard is here for the same reason the DIAG one is — the shipped
        // path pays nothing.
        if pol.ackdiag_on {
            ackdiag::maybe_report(scheduler, stats, window_ack_seq);
        }

        // Generation coding: paced coded emission (see gen_tokens above). Runs
        // every iteration — including the tx_paused 1 ms wakeups — so coded
        // symbols for the in-flight generations keep flowing while TUN reads are
        // paused, completing buffered generations and keeping M in flight
        // (∝-goodput striping via place_symbol; fungible cross-path, no per-seq
        // ARQ). This is the mechanism that turns the serialized stop-and-wait
        // into a pipelined transfer.
        // Proactive-recovery fraction trace (RWM_PFRAC): the share of coded
        // repair emitted proactively (upfront, no round-trip) vs reactively
        // (deficit-driven, one round-trip). Cumulative over the transfer. A
        // high proactive fraction shows holes are recovered from upfront repair.
        if generation && gates.pfrac {
            let now = now_us();
            if now.saturating_sub(pfrac_last_us) > 500_000 {
                pfrac_last_us = now;
                let tot = proactive_coded_total + recovery_coded_total;
                let frac = if tot > 0 {
                    proactive_coded_total as f64 / tot as f64
                } else {
                    0.0
                };
                crate::readout!(
                    "[PFRAC] proactive_coded={} recovery_coded={} total_coded={} proactive_fraction={:.4}",
                    proactive_coded_total, recovery_coded_total, tot, frac
                );
            }
        }
        // `[SHEDH]` — the receiver hold's bind gauge (paper §11.2), on the
        // 1 s cadence, cumulative, last line wins.
        //
        // Emitted from the sender loop although it is fed at the receiver:
        // the counters are process-global and both endpoints of a tunnel run
        // both tasks, so this reaches the log without touching the receiver
        // task at all. A periodic emission also survives the harness's
        // SIGKILL of the server, where a `Drop` never runs.
        if gates.diag {
            let now = now_us();
            if now.saturating_sub(shedh_last_us) > 1_000_000 {
                shedh_last_us = now;
                crate::readout!("{}", shedh_report_line());
            }
        }
        // `[CHI]` — the completion-exposure gauge (paper §4.6), same 1 s
        // cadence, and on both arms: the control's `n=0 max=0.0000` is the
        // two-sided half of the reachability claim, and an arm whose χ never
        // left 0 must be read that way rather than inferred.
        if gates.diag {
            let now = now_us();
            if now.saturating_sub(chi_last_us) > 1_000_000 {
                chi_last_us = now;
                crate::readout!("{}", chi_report_line());
            }
        }
        // `[REQS]` — the request-serving gauge (paper §7.6 arms (A)/(B)),
        // same 1 s cadence, cumulative, last line wins — and on both arms,
        // because `[REQS] n=0 served=0` on the control is the two-sided half
        // of the reachability claim: an arm that never reached the wire must
        // be read that way rather than inferred from a missing line.
        if gates.diag {
            let now = now_us();
            if now.saturating_sub(reqs_last_us) > 1_000_000 {
                reqs_last_us = now;
                crate::readout!(
                    "{}",
                    reqs_report_line(
                        request_arm,
                        reqs_reports,
                        reqs_spans,
                        reqs_m_max,
                        reqs_copy,
                        reqs_coded,
                        reqs_wa1_some,
                        reqs_wa1_none,
                        reqs_stale,
                        reqs_budget_bound,
                        req_want.len() as u64,
                        reqs_cause,
                    )
                );
            }
        }
        // GDIAG: did any coded symbol go on the wire this iteration?
        let gd_flow = emit_generation_coded(GenEmitCtx {
            pol: &pol,
            generation,
            tx_paused,
            cwnd_full,
            gp_rate_max,
            cc_rate_cached,
            scheduler: &mut *scheduler,
            transport,
            stats,
            batch_counter,
            window_ack_seq,
            st: &mut st,
            dg: &mut dg,
            gen_want: &mut gen_want,
            gen_emitted: &mut gen_emitted,
            gen_recover_at: &mut gen_recover_at,
            gen_coded_total: &mut gen_coded_total,
            gen_rate_ewma: &mut gen_rate_ewma,
            gen_rate_sample_us: &mut gen_rate_sample_us,
            gen_rate_sample_ack: &mut gen_rate_sample_ack,
            gen_tokens: &mut gen_tokens,
            gen_tok_last_us: &mut gen_tok_last_us,
            proactive_coded_total: &mut proactive_coded_total,
            recovery_coded_total: &mut recovery_coded_total,
        });
        // ── GDIAG attribution: which gate is binding wire emission NOW? ──────
        // Runs every iteration (RWM_DIAG only). See the state legend at the
        // declarations above. In coded-wire generation mode the paced coded
        // block is the whole data plane, so the gate that stopped it this
        // iteration is the throughput binder for the elapsed slice.
        if pol.diag_on && generation {
            let now_g = now_us();
            let dt = now_g.saturating_sub(dg.gd_last_us);
            dg.gd_last_us = now_g;
            let idx = if gd_flow {
                0 // emit: coded flowed
            } else if st.encoder.window_size() == 0 {
                2 // fill: nothing retained yet (startup/tail)
            } else if !st.encoder.wants_coding() {
                // Every active generation at budget (ack/deficit round-trip
                // wait) vs the head generation not yet sealed (intake-bound).
                // advance() is generation-aligned, so ≥2·G retained means the
                // two active generations are both full ⇒ sealed-at-budget.
                if store_len >= 2 * pol.gen_size { 1 } else { 2 }
            } else {
                let ack_now = window_ack_seq.load(Ordering::Relaxed);
                let tgt = if pol.coded_src_clock || pol.ooo_retain || pol.gen_pipe {
                    let (_, wend) = st.encoder.window_span();
                    (wend as f64) * (1.0 + pol.gen_repair_floor) + pol.gen_inflight_window
                } else {
                    (ack_now as f64) * (1.0 + pol.gen_repair_floor) + pol.gen_inflight_window
                };
                if cwnd_full {
                    5 // cwnd
                } else if (gen_coded_total as f64) >= tgt {
                    3 // target (ack-clocked coded flow window)
                } else if gen_tokens < 1.0 {
                    4 // tokens (delivered-rate pacer)
                } else {
                    0
                }
            };
            dg.gd_us[idx] += dt;
        }
        if tx_paused != last_tx_paused {
            debug!(
                tx_paused,
                store_len,
                "reliable-window backpressure state change"
            );
            last_tx_paused = tx_paused;
        }

        // Refill the source-pacing token bucket at the measured link rate
        // (delivered-goodput EWMA × headroom, clamped to the same
        // [floor, ceiling] as the coded bucket). The small burst cap (≈ a few
        // ms of link rate, not the BDP) prevents the datagram burst-overrun:
        // at high RTT the flow window is BDP-sized, but emission is metered to
        // the link so no BDP-sized burst reaches the droppable path.
        //
        // Clock: ONE read for the rest of this iteration's pre-`select!`
        // phase — the pacing refill here, the tail-sweep deadline and the
        // three 1 ms poll deadlines below (which previously read the clock
        // once each; `tokio::time::sleep(d)` is `sleep_until(now + d)`). The
        // phase between this read and the `select!` is synchronous loop body
        // (one scheduler-lock take for the tail clock), so the reuse is stale
        // by microseconds — well inside one loop iteration. The tail deadline
        // is converted with the SAME read's `Instant`, so its absolute
        // instant is exact, not stale.
        let (pre_now_us, pre_now_i) = now_pair();
        let poll_1ms = tokio::time::Instant::from_std(pre_now_i) + Duration::from_millis(1);
        if pol.cc_pace {
            let now = pre_now_us;
            // Refresh the Copa cwnd/SRTT rate estimate (frontier-independent) at
            // most every 5 ms.
            if now.saturating_sub(cc_rate_refresh_us) >= 5_000 {
                cc_rate_refresh_us = now;
                // The aggregate CC rate is the sum of per-path rates
                // Σ cwnd_i/SRTT_i over live paths. Σcwnd / max(SRTT) would
                // under-read a heterogeneous aggregate (the fast path's rate
                // divided by the slow path's SRTT), and active_paths()'
                // spare-capacity filter would drop a saturated path from the
                // sum entirely. The pace ceiling also scales by the live path
                // count: gen_rate (9 000 sym/s ≈ 90 Mbit) is a single-link
                // burst guard, and as an aggregate clamp it would cap a
                // two-path intake at one path's worth.
                let (rate, n_live) = {
                    let sched = &*scheduler;
                    let mut r = 0.0f64;
                    let mut n = 0usize;
                    for (_, p) in sched.live_paths_iter() {
                        let s = p.srtt().as_secs_f64();
                        if s > 1e-4 {
                            r += p.cwnd as f64 / s;
                        }
                        n += 1;
                    }
                    (r, n.max(1))
                };
                cc_rate_cached = rate;
                cc_rate_ceiling = pol.gen_rate * n_live as f64;
            }
            // Pace at the higher of the CC rate and the delivered-goodput EWMA so
            // a stalled in-order frontier (EWMA→0) can't throttle the source ramp.
            let link_est = gen_rate_ewma.max(cc_rate_cached);
            let src_rate = (link_est * pol.cc_pace_headroom).clamp(pol.gen_rate_floor, cc_rate_ceiling);
            let dt = now.saturating_sub(src_tok_last_us);
            src_tok_last_us = now;
            let burst = (src_rate * 0.004).clamp(8.0, 64.0);
            st.src_tokens = (st.src_tokens + src_rate * (dt as f64 / 1_000_000.0)).min(burst);
        }
        // Gap reports must wake this loop even when the TUN is idle. The inner
        // TCP stalls exactly when a hole blocks delivery — no new TUN packets
        // — so draining the NACK channel only after a TUN read would stall
        // repairs precisely when they are the only thing that can unstall the
        // tunnel.
        // `[FCAUSE]`: the gap batch carries the cause that produced it, so a
        // fire can be attributed to the sender's own tail-sweep timer or to
        // one of the receiver's two arms. Label only — the loop below reads
        // it for counters and for nothing else.
        let mut pending_gaps: Option<(FireCause, u32, Vec<(u64, u64)>)> = None;

        // Tail sweep: the last symbols of a burst have no successors,
        // so the receiver can never SACK a gap behind them — the sender must
        // detect that stall itself (the analog of block mode's ARQ sweeper).
        // When un-ACKed symbols exist, arm a timer at oldest-activity +
        // 2×SRTT; on expiry synthesize a gap report for the cumulative
        // blocker (per-seq cooldown + budgets all apply downstream).
        let tail_deadline: Option<(u64, tokio::time::Instant)> =
            st.retransmit_buffer.first().map(|(seq, &(send_us, _, _))| {
                let last_activity_us = st.nack_retx_at
                    .get(&seq)
                    .map_or(send_us, |&(r, _)| r.max(send_us))
                    .max(last_tail_sweep_us);
                let (srtt_us, jitter_us, sigma_us) = {
                    let sched = &*scheduler;
                    // Three passes over the same path set (no per-iteration
                    // `Vec`s: this block runs on every loop iteration).
                    let paths = || recovery_clock_paths_iter(&sched).map(|(_, p)| p);
                    let pooled = pooled_recovery_srtt_iter(
                        paths().map(|p| p.estimator.rtt().as_micros() as u64),
                    );
                    // The jitter feeding the derived floor is pooled the same
                    // way the clock is (max over the same path set), so floor
                    // and clock can never come from different paths.
                    let jit = paths().map(|p| p.rtt_jitter_us()).max().unwrap_or(0);
                    // The measured σ, max over the same set — read only by the
                    // `[QCLK]` readout below.
                    let sg = paths().filter_map(|p| p.rtt_sigma_us()).max();
                    (pooled, jit, sg)
                };
                let timeout_us = sweep_timeout_us(pol.derived_sweep, srtt_us, jitter_us);
                // `timeout_us` is what the engine will use — never recomputed
                // here, so this gauge cannot report a clock that did not run.
                qclk_echo.record(timeout_us, srtt_us, sigma_us);
                if pol.derived_sweep {
                    derived_round_echo.observe(
                        "sender-tail-sweep",
                        srtt_us,
                        jitter_us,
                        timeout_us,
                        tail_sweep_timeout_us(srtt_us),
                    );
                }
                let deadline_us = last_activity_us + timeout_us;
                let remaining = Duration::from_micros(deadline_us.saturating_sub(pre_now_us));
                (deadline_us, tokio::time::Instant::from_std(pre_now_i) + remaining)
            });
        // D11: move the one persistent tail `Sleep` only when the deadline
        // itself moved.
        if let Some((d_us, d_at)) = tail_deadline {
            if tail_sleep_us != Some(d_us) {
                tail_sleep.as_mut().reset(d_at);
                tail_sleep_us = Some(d_us);
            }
        }

        // Threading Q1: hand everything this iteration staged — the Law-0
        // burst, its repairs, any control datagram — to the paths' I/O
        // owners, one batch per path, before the loop waits. A full owner
        // channel back-pressures here (charged to `busy`, as the quinn
        // handoff it replaces was). Nothing is staged across a wait.
        transport.flush(&mut st.tx).await;
        // Wait attribution: which `select!` arm woke this iteration. Every arm below writes its
        // bucket index; the charge happens once, after the await, and reads
        // the clock only under RWM_DIAG. The index is written unconditionally
        // — a local `usize` store — so the attribution can never disagree
        // with the branch actually taken. `usize::MAX` means "shutdown", the
        // one arm that returns instead of falling through.
        let mut wait_arm: usize = usize::MAX;
        // Loop-body / await split (Stage 1d): the time from the previous
        // `select!` resolution to here was loop BODY (compute), not waiting.
        // `[DIAG]` charges it to `busy`; the dead-wall gauge reads it to
        // tell a busy wake from a productive one. Clock read only when one
        // of the two instruments is on.
        let (await_start_us, body_us) = if pol.diag_on || pol.walldiag_on {
            let t = now_us();
            let body = if pol.diag_on {
                dg.wait_enter_await(t)
            } else {
                t.saturating_sub(wall_resolved_us)
            };
            (t, body)
        } else {
            (0, 0)
        };
        let packet = tokio::select! {
            // Backpressure poll (reliable): with TUN reads gated off, wake
            // at ack timescale to observe store drain via the ack path
            // below (mirrors the block sender's 1 ms backpressure poll).
            _ = tokio::time::sleep_until(poll_1ms), if tx_paused => { wait_arm = 1; None },
            // Pacing wake — when source sends are paced-off (bucket
            // empty), wake at 1 ms to refill it. Without this the select could
            // block in read_packet with the pacing gate closed and stall intake.
            _ = tokio::time::sleep_until(poll_1ms),
                if pol.cc_pace && !tx_paused && st.src_tokens < 1.0 => { wait_arm = 2; None },
            // Threading Q2 — the input arm: TX-direction control input (the
            // client's acks, PathReport, Ping), drained whole by ONE
            // `recv_many` and handled here, in the sender (`on_window_ack`
            // runs in the task that owns the scheduler). ALWAYS armed: no
            // other task handles acks now, so an idle-intake sender (the
            // tail) must still take them; the wake is the channel's own — a
            // busy sender takes the batch at its loop top instead, with no
            // wake (plan rule 4). It also ends a paused / pacing-dry wait at
            // once, which P1's `AckWake` arm did.
            n = sender_in.recv_many(&mut in_buf, SENDER_IN_BATCHES), if !in_closed => {
                wait_arm = 8;
                if n == 0 {
                    in_closed = true;
                } else {
                    tx_inputs::apply_inputs(&mut in_buf, scheduler, &mut st.tx, &input_ctx);
                    scheduler.publish_tx_view(stats);
                }
                None
            }
            // Threading Q2 — the cold tasks' commands (report tick, path
            // add / remove, FEC feedback, revival nudge): always armed, one
            // `recv_many` per wake.
            n = sender_cmds.recv_many(&mut cmd_buf, SENDER_CMD_DEPTH), if !cmds_closed => {
                wait_arm = 9;
                if n == 0 {
                    cmds_closed = true;
                } else {
                    tx_inputs::apply_cmds(&mut cmd_buf, scheduler, fec_controller, stats);
                    scheduler.publish_tx_view(stats);
                }
                None
            }
            p = tun.read_packet(),
                if !tx_paused && (!pol.cc_pace || st.src_tokens >= 1.0) => { wait_arm = 0; Some(p) },
            // Generation coding: a 1 ms emission poll so the loop keeps waking to
            // run the paced coded-emission block even when no TUN packet is ready
            // (the tail — all sources read but the last generations still need
            // coded symbols to decode) and when not paused. Without it the loop
            // would block in read_packet and the tail would never complete.
            _ = tokio::time::sleep_until(poll_1ms),
                if generation && !tx_paused && st.encoder.window_size() > 0 => { wait_arm = 3; None },
            gaps = nack_rx.recv() => {
                wait_arm = 4;
                if let Some(g) = gaps {
                    pending_gaps = Some(g);
                }
                None
            }
            // `[FCAUSE]`: the tail-sweep arm below is the only producer the
            // sender's recovery clock clocks — `tail_deadline` is computed from
            // `sweep_timeout_us`. Every other fire in this loop is clocked by
            // the receiver or by data arrival.
            // Generation-deficit feedback (paper §5.8): the receiver reports how
            // many more coded symbols each frontier generation still needs.
            // Rebuild the per-generation want from the report, subtracting what
            // is already in flight (emitted since the last report), then reset
            // the in-flight baseline. The recovery-emission block above drains
            // these wants, paced. A generation absent from the report has
            // decoded (or is not yet frontier), so its want is cleared by the
            // rebuild.
            dv = deficit_rx.recv(), if generation => {
                wait_arm = 5;
                if let Some(dv) = dv {
                    gen_want.clear();
                    // Pure-proactive demonstrator: drain the channel but never
                    // arm reactive recovery (no round-trips, no exempt-from-cap
                    // emission). Proactive upfront budget is the only recovery.
                    if pol.no_reactive {
                        let _ = dv;
                    } else {
                    // RTT-spacing gate. Reports arrive on every decode progress
                    // (sub-RTT), each resetting the in-flight baseline — so the
                    // in-flight subtraction shrinks to "sent since the last
                    // (recent) report" and without a gate the sender would
                    // re-send ~the full deficit every few ms (a reactive flood).
                    // Gate at the report: a generation recovered < react_space_us
                    // ago is skipped (its recovery is still in flight, not yet
                    // reflected), so we act on its deficit at most once per
                    // ~SRTT. react_space_us = react_cap_cfg × SRTT (1.0 = 1 SRTT).
                    let react_space_us: u64 = if pol.react_cap_on {
                        // Read here, after the `select!` await: this is the one
                        // phase whose value would be stale under a loop-top
                        // snapshot (see the note at the top of the loop).
                        let srtt_us = channel_max_srtt_us(&*scheduler).unwrap_or(50_000);
                        ((srtt_us as f64) * pol.react_cap_cfg).max(1_000.0) as u64
                    } else {
                        0
                    };
                    let now_d = now_us();
                    for (anchor, deficit) in dv {
                        // Hold off if we recovered this generation recently.
                        if pol.react_cap_on {
                            if let Some(&last) = gen_recover_at.get(&anchor) {
                                if now_d.saturating_sub(last) < react_space_us {
                                    continue;
                                }
                            }
                        }
                        let emitted = gen_emitted.get(&anchor).copied().unwrap_or(0);
                        // In-flight = coded emitted for this generation that the
                        // receiver's current deficit does not yet reflect (sent
                        // since the last report). On the first report there is no
                        // baseline: the proactive emissions are already reflected
                        // in the reported deficit (the receiver counted them), so
                        // in-flight is 0 — send the full deficit. (Initialising the
                        // baseline to 0 instead would wrongly treat the whole
                        // proactive budget as in flight, send nothing and
                        // deadlock.)
                        let in_flight = match gen_emitted_at_report.get(&anchor) {
                            Some(&b) => emitted.saturating_sub(b),
                            None => 0,
                        };
                        let to_send = (deficit as u64).saturating_sub(in_flight);
                        gen_emitted_at_report.insert(anchor, emitted);
                        if to_send > 0 {
                            gen_want.insert(anchor, to_send);
                        }
                    }
                    }
                }
                None
            }
            // Paper §7.6 arms (A)/(B) — the receiver's request, consumed.
            // Sits beside the generation-deficit arm above and is its exact
            // parallel: one report replaces the outstanding wants (it is a
            // state snapshot of the receiver's open holes, not a delta), the
            // in-flight baseline is subtracted, and the serving loop drains
            // what is left, paced by the same `cached_nack_budget` the gap
            // loop is paced by.
            //
            // Disarmed ⇒ this arm never resolves: `recv_request_tx` is `None`,
            // so nothing is ever sent on the channel, and the `if` guard is
            // false, so the future is not even polled.
            //
            // It claims no `wait_arm` bucket: `dg.wait_us` is `[u64; 9]` and
            // its nine arms belong to the shipped `[DIAG]` line. The request
            // arm's own accounting is `[REQS]`'s business.
            rq = request_rx.recv(), if request_arm => {
                if let Some((cause, spans)) = rq {
                    reqs_reports += 1;
                    reqs_cause = FireCause::from_u8(cause);
                    req_want.clear();
                    for (start, count, deficit) in spans {
                        reqs_spans += 1;
                        let m = count.max(1);
                        reqs_m_max = reqs_m_max.max(m as u64);
                        let emitted = req_emitted.get(&start).copied().unwrap_or(0);
                        // The first-report special case. See the state block:
                        // without it the loop deadlocks.
                        let in_flight = match req_at_report.get(&start) {
                            Some(&b) => emitted.saturating_sub(b),
                            None => 0,
                        };
                        let to_send = (deficit as u64).saturating_sub(in_flight);
                        req_at_report.insert(start, emitted);
                        if to_send > 0 {
                            req_want.insert(start, (m, to_send));
                        }
                    }
                }
                None
            }
            _ = &mut tail_sleep, if tail_deadline.is_some() => {
                wait_arm = 6;
                last_tail_sweep_us = now_us();
                if let Some((seq, _)) = st.retransmit_buffer.first() {
                    debug!(seq, "tail ARQ sweep — retransmitting cumulative blocker");
                    dg.diag_sweeps += 1;
                    // The tail sweep is the sender's own producer — no ack, and
                    // so no arrival path. `u32::MAX` is the gauge's existing
                    // unattributed sentinel and reads as cross-path by
                    // construction, which is what "no receiver said so" means.
                    pending_gaps = Some((FireCause::Timer, u32::MAX, vec![(seq, seq)]));
                }
                None
            }
            _ = shutdown_rx.recv() => {
                // Flush any remaining packed data before shutdown
                if pol.use_packing {
                    if let Some(packed) = packer.flush() {
                        emit_source(
                    packed.into(),
                    &mut st,
                    &pol,
                    &mut sctx!(),
                );
                    }
                }
                // Send Shutdown on all paths (P1 D2: guard dropped first).
                let paths = control_broadcast_paths(&*scheduler);
                for pid in paths {
                    let _ = transport.send_control_datagram(&mut st.tx, pid, ControlMessage::Shutdown);
                }
                // The packer's last symbol and the Shutdowns, after every
                // burst staged before them (data lane first per path).
                transport.flush(&mut st.tx).await;
                eta_final.flush_final(scheduler);
                // `[WALL]` and `[CCAP]` are not emitted here. They are emitted
                // by `ccap`'s destructor (`SenderTeardownGauges`), which is on
                // this path and on every other way this sender can end —
                // including the `perf` harness's, which reaches none of the
                // `select!`'s exit arms at all.
                info!("window sender shut down gracefully");
                return;
            }
            _ = tokio::time::sleep(packer.time_until_flush()), if packer_pending => {
                wait_arm = 7;
                // Flush timeout expired — emit partial packed symbol
                if let Some(packed) = packer.flush() {
                    emit_source(
                    packed.into(),
                    &mut st,
                    &pol,
                    &mut sctx!(),
                );
                }
                None
            }
        };
        // Charge the elapsed wall time to the arm that woke us: the window
        // sender's wait-reason attribution, which says where idle sender time
        // (`sidle`) goes. Unlike `gd_us` it has no `generation` guard: the
        // arms it names exist in every window mode.
        let resolved_us = if pol.diag_on || pol.walldiag_on { now_us() } else { 0 };
        if pol.diag_on && wait_arm < WAIT_BUCKETS {
            // The await only: the body before it went to `busy` above.
            let dt = dg.wait_resolve(resolved_us);
            dg.wait_us[wait_arm] += dt;
            dg.wait_n += 1;
            // `wake[..]` (P1, D1): which arm woke the loop, as a cumulative
            // COUNT (the wait shares above are time). `timer_acked`
            // (threading Q2): a paused / pacing 1 ms timer wake that resolved
            // with acks queued on the input channel — the input arm lost the
            // `select!` race (both ready at one poll), or the wiring is
            // broken.
            dg.wake_n[wait_arm] += 1;
            if (wait_arm == 1 || wait_arm == 2) && !sender_in.is_empty() {
                dg.wake_timer_acked += 1;
            }
        }
        wall_resolved_us = resolved_us;
        // ── The dead-wall gauge (`RWM_WALLDIAG`, net/walldiag.rs) ─────────
        // The one feed site of the onset/duration instrument, placed beside
        // the wait-arm charge above because it consumes the same `wait_arm` —
        // but on its own gate, so it is collectable on arms that cannot
        // afford the 250 ms `[DIAG]` report.
        //
        // Scalars only, no engine handle: the arm that woke the loop, the
        // wall clock of the last new source symbol (`last_source_send_us`,
        // maintained unconditionally by the emission step), and the engine's
        // monotone retransmit counter — plus (Stage 1d) the loop-body time
        // before this wake's await and the await itself, so a busy wake is
        // not read as productive. `productive(t)` is evaluated inside
        // the gauge — see `net/walldiag.rs` for the measurand.
        if pol.walldiag_on {
            if let Some(g) = walldiag::gauge() {
                g.observe(
                    resolved_us,
                    wait_arm,
                    st.last_source_send_us,
                    dg.diag_retx,
                    body_us,
                    resolved_us.saturating_sub(await_start_us),
                );
            }
        }

        if let Some(packet) = packet {
            let pkt = match packet {
                Some(p) => {
                    // `RWM_COMPLETION_EXPOSURE`: the admitted bytes drain the
                    // object's remaining (`CompletionFeed`). `None` otherwise.
                    if let Some(f) = completion_feed.as_ref() {
                        f.consume(p.len() as u64);
                    }
                    p
                }
                None => {
                    // Flush remaining packed data before exit
                    if pol.use_packing {
                        if let Some(packed) = packer.flush() {
                            emit_source(
                    packed.into(),
                    &mut st,
                    &pol,
                    &mut sctx!(),
                );
                        }
                    }
                    // `[WALL]`/`[CCAP]`: see the shutdown arm above — emitted
                    // by `ccap`'s destructor, on every exit path.
                    eta_final.flush_final(scheduler);
                    // The packer's last symbol, to the owners.
                    transport.flush(&mut st.tx).await;
                    info!("TUN closed");
                    return;
                }
            };

            if pol.use_packing {
                // Pack multiple small packets into one symbol
                if let Some(packed) = packer.push(&pkt) {
                    emit_source(
                    packed.into(),
                    &mut st,
                    &pol,
                    &mut sctx!(),
                );
                }
            } else {
                // Legacy: one packet per symbol (padded)
                let framed = framing::frame_window_packet(&pkt, symbol_size);
                // [EMIT-BURST-BEGIN] (source pin: no path-count read here)
                let eb_gauge = pol.diag_on && pol.emit_batch_on;
                if eb_gauge {
                    dg.eb.begin();
                }
                emit_source(
                    framed.into(),
                    &mut st,
                    &pol,
                    &mut sctx!(),
                );
                if eb_gauge {
                    dg.eb.on_symbol(st.last_source_path);
                }
                // ── RWM_EMIT_BATCH pacer-quantum burst intake ─────────────
                // Drain already-queued TUN packets without re-arming the
                // select! (per-iteration overhead — tail-deadline scan, SACK
                // drain, pacing refresh — amortizes over the burst, and
                // quinn's endpoint driver sees a multi-datagram queue for
                // deeper GSO transmits). Contracts enforced per symbol: the
                // pooled store backstop from the live local counters (the
                // emission step updates sent_store/sack_released), and the cc_pace
                // token bucket. Burst quantum ≤ emit_burst ≈ 64 KB.
                // Law 0: the bound is the quantum at EVERY path count -- no
                // live-path scope (the v8 striping-gap reason is gone in v9).
                if pol.emit_batch_on {
                    // Law 0 (`emit_burst::emit_burst_bound`).
                    let bound = emit_burst::emit_burst_bound(pol.emit_burst);
                    let mut burst = 1usize;
                    let mut why = emit_burst::BurstEnd::Cap;
                    while burst < bound {
                        if reliable
                            && sack_release_outstanding(
                                st.sent_store.len(),
                                store_gate_released(
                                    &st.sent_store,
                                    sack_released.len(),
                                    above_report,
                                ),
                            ) >= effective_store_cap
                        {
                            why = emit_burst::BurstEnd::Store;
                            break; // store headroom exhausted (flow control)
                        }
                        if pol.cc_pace && st.src_tokens < 1.0 {
                            why = emit_burst::BurstEnd::Tokens;
                            break; // pacing bucket dry (pacing contract)
                        }
                        match tun.try_read_packet() {
                            Some(pkt) => {
                                if let Some(f) = completion_feed.as_ref() {
                                    f.consume(pkt.len() as u64);
                                }
                                let framed =
                                    framing::frame_window_packet(&pkt, symbol_size);
                                emit_source(
                    framed.into(),
                    &mut st,
                    &pol,
                    &mut sctx!(),
                );
                                if eb_gauge {
                                    dg.eb.on_symbol(st.last_source_path);
                                }
                                burst += 1;
                            }
                            None => {
                                why = emit_burst::BurstEnd::Drained;
                                break; // intake drained (or closed — the
                                       // blocking read owns shutdown)
                            }
                        }
                    }
                    if eb_gauge {
                        dg.eb.end(why);
                    }
                }
                // [EMIT-BURST-END]
            }
        }

        // Process gap reports → retransmit exact source symbols + repair margin.
        // Congestion backoff + ADR-0050 budget state is refreshed at
        // NACK_REPAIR_COOLDOWN_US cadence; processing itself is not gated on
        // the cadence — per-seq cooldowns already bound the send rate, and
        // delaying a repair round costs a reorder-hold expiry at the receiver.
        let now_repair_us = now_us();
        if now_repair_us.saturating_sub(last_budget_refresh_us) >= NACK_REPAIR_COOLDOWN_US {
            last_budget_refresh_us = now_repair_us;
            // Update congestion state from scheduler
            let (current_loss, current_rtt) = nack_congestion_inputs(&*scheduler);
            nack_congestion.update(current_loss, current_rtt);
            // Idle-triggered recovery: if no new source symbol has been sent
            // for > 2×SRTT, the sender is idle-except-for-recovery — no
            // traffic we emit is contributing to congestion, so a stalled
            // confirmed hole must not stay suppressed. The idle floor lifts the
            // multiplier just enough for >=1 targeted retransmit per round;
            // while actively pushing data the raw multiplier governs unchanged
            // (congestion safety still wins on a straggler).
            let srtt_us_recent = current_rtt
                .map(|d| d.as_micros() as u64)
                .unwrap_or(IDLE_RECOVERY_GAP_FLOOR_US);
            let idle_gap_us = (2 * srtt_us_recent).max(IDLE_RECOVERY_GAP_FLOOR_US);
            // Same instant as the cadence check above (µs apart, no await).
            let sender_idle =
                now_repair_us.saturating_sub(st.last_source_send_us) > idle_gap_us;
            let nack_multiplier = nack_congestion.effective_multiplier(sender_idle);
            cached_max_repairs =
                (MAX_NACK_REPAIRS_PER_NACK as f64 * nack_multiplier).round() as u64;
            // No blanket `cached_max_repairs.max(1)` floor here: forcing a
            // retransmit every round on a genuinely congested lossy path adds
            // load to the straggler and costs goodput. The stall such a floor
            // would target — a datagram-loss burst collapses the multiplier to
            // 0 and wedges a reliable transfer until the QUIC idle timeout — is
            // handled by the idle-triggered floor above
            // (`effective_multiplier`): it only fires when no new source has
            // been sent for > 2×SRTT, i.e. exactly when there is no straggler
            // load to protect.

            // ADR-0050: compute NACK budget from BudgetAllocator
            cached_nack_budget = {
                let ctrl = &*fec_controller;
                let sched = &*scheduler;
                let worst_est = worst_eps_estimator(&sched);
                match worst_est {
                    Some(est) => {
                        let p_upper = est.predictive_loss_upper(1.0 - ctrl.target_tail_loss());
                        let nack_eff = est.nack_effectiveness();
                        let budget = crate::control::fec_rate::BudgetAllocator::compute(
                            p_upper, ctrl.codec_overhead(), current_loss * 0.5, nack_eff,
                        );
                        let nack_cap_symbols = (budget.nack_cap() * st.source_symbols_this_period as f64) as u64;
                        // Floor at one full repair burst per refresh
                        // interval. The raw cap is nack_cap (≈ loss_rate/2)
                        // × sources-this-period, but the period resets every
                        // 10 acked seqs, so the u64 cast would truncate it to
                        // 0 almost always — silently suppressing the entire
                        // reactive repair path. Congestion safety lives in
                        // the throttle multiplier (cached_max_repairs), which
                        // can still zero out repairs under real congestion;
                        // this floor only guarantees wireless-loss repairs
                        // are never starved by quantization.
                        nack_cap_symbols
                            .saturating_sub(nack_repairs_this_period)
                            .max(MAX_NACK_REPAIRS_PER_NACK as u64)
                    }
                    None => cached_max_repairs,
                }
            };
        }

        // ── Paper §7.6 arms (A)/(B): serve the receiver's requests ──────
        //
        // Mirrors the deficit-driven recovery emission above, one vocabulary
        // over: round-robin over the requested spans so none starves, paced
        // by the same `cached_nack_budget` / `cached_max_repairs` the gap loop
        // is paced by (so an arm's wire cost is comparable to the control's by
        // construction), and bounded by the receiver's own reported deficit
        // minus what is already in flight.
        //
        // Two answers, one expression:
        //   * `m = 1` — the copy, served from `sent_store` exactly as the gap
        //     loop serves it, byte for byte. This is the shipped answer, and
        //     the new message's own limit rather than a special case beside
        //     it.
        //   * `m > 1` — one coded equation over `[a, a+m)` from
        //     `generate_repair_range`, which refuses unless the whole range is
        //     still retained. The refusal is counted (`WA1`) and falls back to
        //     per-seq copies out of `sent_store`, because an answer that
        //     silently degrades to a copy is the shipped machine with extra
        //     latency and a measurement has to be able to read that.
        //
        // Disarmed ⇒ `req_want` is empty forever (nothing ever fills it) and
        // this block is one `is_empty()` test per iteration.
        if request_arm && !req_want.is_empty() {
            let mut served: u64 = 0;
            'serve: loop {
                if served >= cached_max_repairs || cached_nack_budget == 0 {
                    if !req_want.is_empty() {
                        reqs_budget_bound += 1;
                    }
                    break;
                }
                let anchors: Vec<u64> = req_want.keys().copied().collect();
                if anchors.is_empty() {
                    break;
                }
                let mut progressed = false;
                for a in anchors {
                    if served >= cached_max_repairs || cached_nack_budget == 0 {
                        reqs_budget_bound += 1;
                        break 'serve;
                    }
                    let (m, want) = match req_want.get(&a) {
                        Some(&v) => v,
                        None => continue,
                    };
                    if want == 0 {
                        req_want.remove(&a);
                        continue;
                    }
                    // The answer. `m > 1` asks for an equation over the span;
                    // `m = 1` asks for the seq itself. `None` from either is a
                    // counted refusal, never a silent skip.
                    let coded = if m > 1 {
                        let sym = st.encoder.generate_repair_range(a, m);
                        if sym.is_some() {
                            reqs_wa1_some += 1;
                        } else {
                            reqs_wa1_none += 1;
                        }
                        sym
                    } else {
                        None
                    };
                    // The seqs this answer is placed against: the whole span
                    // for a coded equation, the anchor alone for a copy.
                    let (sym, copy_seq) = match coded {
                        Some(sym) => (Some(sym), None),
                        None => {
                            // The copy — and the fallback when the span was
                            // no longer retained. `a` is the receiver's own
                            // first missing seq, so it is the copy that closes
                            // the head of the span.
                            match st.sent_store.get(&a) {
                                Some(v) => (Some(v.clone()), Some(a)),
                                // Not in the store ⇒ already acked (removal is
                                // by ack only): the receiver has it and the
                                // request is stale. Drop the want and count it.
                                None => {
                                    reqs_stale += 1;
                                    req_want.remove(&a);
                                    continue;
                                }
                            }
                        }
                    };
                    let sym = match sym {
                        Some(sym) => sym,
                        None => {
                            req_want.remove(&a);
                            continue;
                        }
                    };
                    // Placement: the shipped reliable law, with the paths that
                    // carried the covered source as the fate penalty — the
                    // gap loop's own `place_symbol(true, &[original])` for a
                    // copy, and the span's own source paths for an equation.
                    let covered: Vec<u32> = match copy_seq {
                        Some(seq) => vec![
                            st.source_path_map
                                .get(&seq)
                                .copied()
                                .unwrap_or(st.last_source_path),
                        ],
                        None => (a..a.saturating_add(m as u64))
                            .filter_map(|q| st.source_path_map.get(&q).copied())
                            .collect(),
                    };
                    let path = {
                        let sched = &*scheduler;
                        sched
                            .place_symbol(true, &covered)
                            .unwrap_or(st.last_source_path)
                    };
                    let now_r = now_us();
                    let seqs = batch_counter.next(path);
                    let batch = SymbolBatch::new(vec![sym], now_r, seqs, path);
                    let sent = match transport.send_symbols(&mut st.tx, path, batch) {
                        Ok(()) => true,
                        Err(e) => {
                            warn!(path, ?e, "failed to send requested repair");
                            false
                        }
                    };
                    // The three meters, at the handoff, exactly as every other
                    // correction channel meters them (the gap loop's two
                    // bypass channels are the exception, not the rule, and a
                    // new channel does not inherit an exception).
                    {
                        let sched = &mut *scheduler;
                        if let Some(p) = sched.path_mut(path) {
                            p.charge_in_flight(1);
                            p.consume_pace_tokens(1);
                        }
                    }
                    if let Some(ps) = stats.path_ref(path) {
                        ps.symbols_sent.fetch_add(1, Ordering::Relaxed);
                    }
                    stats.fec.record_correction(
                        if copy_seq.is_some() {
                            CorrectionKind::SourceCopy
                        } else {
                            CorrectionKind::Coded
                        },
                        sent,
                    );
                    if let Some(seq) = copy_seq {
                        reqs_copy += 1;
                        // The retransmit inherits the in-flight state and the
                        // hold-down gauge's "a copy of this seq reached the
                        // wire" stamp, both exactly as the gap loop sets them,
                        // so no downstream gauge can tell the two producers
                        // apart where it should not.
                        st.nack_retx_at.insert(seq, (now_r, path));
                        hold_echo.on_retx(seq, now_r, path);
                        if let Some(feed) = copa_feed.as_ref() {
                            feed.on_sent(seq, path);
                            let sched = &mut *scheduler;
                            if let Some(p) = sched.path_mut(path) {
                                p.on_src_sent(seq, false);
                            }
                        }
                        // `[DIAG] retx=` counts source retransmits only; a
                        // coded answer is `cod=`.
                        dg.diag_retx += 1;
                    } else {
                        reqs_coded += 1;
                    }
                    recovery_coded_total += 1;
                    nack_repairs_this_period += 1;
                    cached_nack_budget = cached_nack_budget.saturating_sub(1);
                    served += 1;
                    progressed = true;
                    *req_emitted.entry(a).or_insert(0) += 1;
                    let nw = want - 1;
                    if nw == 0 {
                        req_want.remove(&a);
                    } else {
                        req_want.insert(a, (m, nw));
                    }
                }
                if !progressed {
                    break;
                }
            }
        }

        serve_gaps(ServeGapsCtx {
            pol: &pol,
            reliable,
            now_repair_us,
            cached_max_repairs,
            scheduler: &mut *scheduler,
            fec_controller: &mut *fec_controller,
            transport,
            stats,
            batch_counter,
            copa_feed: &copa_feed,
            mpd_pf_floor: &mpd_pf_floor,
            mpd_pf_clock: &mpd_pf_clock,
            mpd_pf_sum: &mpd_pf_sum,
            nack_rx,
            st: &mut st,
            dg: &mut dg,
            rack_echo: &mut rack_echo,
            hold_echo: &mut hold_echo,
            mp_delivered: &mut mp_delivered,
            pending_gaps: &mut pending_gaps,
            cached_nack_budget: &mut cached_nack_budget,
            nack_repairs_this_period: &mut nack_repairs_this_period,
            mp_evid_max: &mut mp_evid_max,
        });
        // Advance encoder window based on receiver ACKs (v9: the peer's
        // `next_expected`, the count of its delivered prefix).
        let next_expected = window_ack_seq.load(Ordering::Relaxed);
        on_ack_advance(AckAdvanceCtx {
            pol: &pol,
            gates: &gates,
            next_expected,
            generation,
            reliable,
            scheduler: &mut *scheduler,
            fec_controller: &mut *fec_controller,
            completion_feed: &completion_feed,
            mp_delivered: &mut mp_delivered,
            st: &mut st,
            dg: &mut dg,
            hold_echo: &mut hold_echo,
            gen_want: &mut gen_want,
            gen_emitted: &mut gen_emitted,
            gen_emitted_at_report: &mut gen_emitted_at_report,
            gen_recover_at: &mut gen_recover_at,
            req_want: &mut req_want,
            req_emitted: &mut req_emitted,
            req_at_report: &mut req_at_report,
            sack_released: &mut sack_released,
            prev_ack: &mut prev_ack,
            c8c_last_ack_adv_us: &mut c8c_last_ack_adv_us,
            nack_repairs_this_period: &mut nack_repairs_this_period,
        });
        // Cap window size — the coding window slides freely under both
        // policies (it is only the FEC horizon). Under retain this eviction
        // destroys no data: the sent-data store still holds the bytes, and
        // an aged hole is recovered by a targeted retransmit from it.
        // Generation mode is exempt: dropping a not-yet-decoded generation's
        // sources would make its coded symbols unsolvable (there is no per-seq
        // store to fall back on). Backpressure (store_max) already bounds the
        // retained pipeline to M generations, and advance() only ever drops
        // fully-decoded generations — so no size-pressure eviction is needed.
        if !generation && st.encoder.window_size() > pol.win_cap {
            let (oldest, _) = st.encoder.window_span();
            st.encoder.advance(oldest + (st.encoder.window_size() - pol.win_cap) as u64);
            // Clean up source_path_map for evicted sequences (EVICT only:
            // reliable mode keeps attribution while the store holds them).
            if !reliable {
                let (win_start, _) = st.encoder.window_span();
                st.source_path_map.prune_below(win_start);
            }
        }

        // The codec is chosen at startup and never changes mid-stream (paper
        // §5.10) — a new stream gets a new context, so no cross-code boundary
        // can exist inside one.
    }
}

/// Create a window encoder. RLC is the only window backend; `backend` stays
/// in the signature as the selection point for another window codec.
fn create_window_encoder(
    _backend: FecBackend,
    symbol_size: u16,
) -> Box<dyn WindowEncoder> {
    Box::new(RlcWindowEncoder::new(symbol_size))
}

/// Create a window decoder for the given backend.
///
/// With the unified gate off, generation mode uses the dense per-generation
/// `GenerationDecoder` (Gauss–Jordan over GF(256) with SIMD row ops) rather
/// than the sparse sliding-window `RlcWindowDecoder`: a sparse decoder runs
/// far below the link rate at an aggregating G, making decode — not the
/// network — the binding constraint (paper §5.8). The wire format is
/// identical, so this is a pure receiver-side swap.
fn create_window_decoder(
    backend: FecBackend,
    symbol_size: u16,
    generation: bool,
) -> Box<dyn WindowDecoder> {
    match backend {
        // Paper §5.2: under RWM_UNIFIED (the default) the whole RLC family —
        // sliding-window and generation wires — decodes on one machine, the
        // global sparse-aware closure. Differential tests (fec::unified /
        // fec::generation) prove it equal to both legacy decoders on their
        // own wires; the legacy machines below are the `RWM_UNIFIED=0` arms.
        _ if unified_active() => {
            info!(generation, "RWM_UNIFIED: receive path on the unified global decoder (one machine, both wires)");
            Box::new(crate::fec::UnifiedDecoder::new(symbol_size))
        }
        _ if generation => Box::new(crate::fec::GenerationDecoder::new(symbol_size)),
        _ => Box::new(RlcWindowDecoder::new(symbol_size)),
    }
}

/// The sender's batch sequencer (wire v9): every emitted `SymbolBatch` takes
/// ONE call, which stamps both sequences it carries.
///
/// - `batch_seq`, global across paths: the receiver echoes it in
///   `ControlMessage::Ack.batch_seq`, so it stays unique connection-wide.
/// - `path_seq`, monotonic per `path_id` (0, 1, 2, ... on each path): the
///   receiver's `PathBatchTracker` reads loss from gaps in it. Through v8 the
///   tracker read gaps in the global counter, so at N >= 2 every other
///   path's batches counted as this path's losses and ε̂ depended on the
///   striping share (≈ 0.5 at 50/50, 0.74 at share 0.1).
///
/// Per-path sequences (threading P1, D17) are one atomic per path in a
/// dense table, so the emission path takes no lock; ids at or above
/// [`BATCH_DENSE_PATHS`] fall back to a mutex map with identical numbering.
pub(crate) struct BatchCounter {
    global: AtomicU64,
    dense: [AtomicU64; BATCH_DENSE_PATHS],
    overflow: parking_lot::Mutex<std::collections::HashMap<u32, u64>>,
}

/// Path ids `0..BATCH_DENSE_PATHS` get a lock-free sequence slot in
/// [`BatchCounter`]. A resource bound (64 × 8 B), not a law constant: paths
/// are numbered from 0 in bind order, so every shipped topology (N ≤ 8) is
/// dense; an id past it (a runtime `add_path` with a large id) takes the
/// cold fallback map, numbered identically.
pub(crate) const BATCH_DENSE_PATHS: usize = 64;

impl BatchCounter {
    pub(crate) fn new() -> Self {
        Self {
            global: AtomicU64::new(0),
            dense: std::array::from_fn(|_| AtomicU64::new(0)),
            overflow: parking_lot::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// `(batch_seq, path_seq)` for one batch about to leave on `path_id` --
    /// exactly the `seqs` argument of [`SymbolBatch::new`].
    pub(crate) fn next(&self, path_id: u32) -> (u64, u64) {
        let global = self.global.fetch_add(1, Ordering::Relaxed);
        let path_seq = match self.dense.get(path_id as usize) {
            Some(slot) => slot.fetch_add(1, Ordering::Relaxed),
            None => {
                let mut m = self.overflow.lock();
                let c = m.entry(path_id).or_insert(0);
                let s = *c;
                *c += 1;
                s
            }
        };
        (global, path_seq)
    }
}

/// Per-path batch sequence tracker for loss detection on receiver side.
///
/// Keyed on the batch's own per-path sequence (`SymbolBatch.path_seq`,
/// wire v9) through [`Self::record`] -- the one entry point the receiver
/// uses -- so a gap is a batch lost on THIS path and never another path's
/// batch.
///
/// Reorder is not loss. The tracker keeps the HIGHEST sequence seen; a
/// batch above it charges the gap, and a late arrival (at or below it)
/// credits its symbols as received without adding to expected, which gives
/// back exactly the loss its absence was charged:
///
/// ```text
///   seq > highest:   expected += (seq − highest) · received;  highest = seq
///   seq ≤ highest:   expected += 0                            (late arrival)
///   always:          received += received
/// ```
///
/// Window batches carry one symbol each, so the totals are exact:
/// `total_expected − total_received` is the number of sequences below the
/// highest that have not arrived (the `gap × received` charge below is
/// `gap × 1`; `test_path_batch_tracker_with_gap` pins the arithmetic). A late arrival's
/// per-batch pair is `(0, received)`; the consumers carry that credit
/// (`scheduler::PathState::credit_ack_pair`, `LossEstimator::record_rx_batch`).
pub(crate) struct PathBatchTracker {
    /// Highest per-path batch sequence seen so far.
    highest_seq: Option<u64>,
    /// Total symbols received on this path
    total_received: u64,
    /// Estimated symbols expected (based on sequence gaps)
    total_expected: u64,
    /// Debug-only duplicate witness: QUIC never retransmits a datagram and
    /// deduplicates packet numbers, so a repeated `path_seq` cannot arrive;
    /// the tracker asserts it rather than handling it. Bounded to the most
    /// recent `DUP_WITNESS_SPAN` sequences.
    #[cfg(debug_assertions)]
    seen: std::collections::BTreeSet<u64>,
}

#[cfg(debug_assertions)]
const DUP_WITNESS_SPAN: u64 = 1 << 16;

impl PathBatchTracker {
    fn new() -> Self {
        Self {
            highest_seq: None,
            total_received: 0,
            total_expected: 0,
            #[cfg(debug_assertions)]
            seen: std::collections::BTreeSet::new(),
        }
    }

    /// Record one arrived batch, keyed on its PER-PATH sequence (v9). The
    /// receiver's only entry point: it is what makes the gap a loss on this
    /// path rather than another path's batch. Returns (expected, received).
    pub(crate) fn record(&mut self, batch: &SymbolBatch) -> (u32, u32) {
        self.record_batch(batch.path_seq, batch.symbols.len() as u32)
    }

    /// Record a batch arrival. Returns (expected_for_this_batch, received_in_this_batch).
    /// Uses sequence gaps to estimate expected symbols. `batch_seq` must be a
    /// per-path sequence (see [`Self::record`]).
    fn record_batch(&mut self, batch_seq: u64, received: u32) -> (u32, u32) {
        #[cfg(debug_assertions)]
        {
            assert!(
                self.seen.insert(batch_seq),
                "duplicate path_seq {batch_seq}: a QUIC datagram cannot arrive twice"
            );
            let floor = self
                .highest_seq
                .unwrap_or(batch_seq)
                .max(batch_seq)
                .saturating_sub(DUP_WITNESS_SPAN);
            while self.seen.first().is_some_and(|&s| s < floor) {
                self.seen.pop_first();
            }
        }
        let expected = match self.highest_seq {
            // First batch: no gap information.
            None => received,
            // Above the highest: this batch plus the (gap − 1) missing
            // batches before it, each estimated at this batch's size.
            Some(h) if batch_seq > h => {
                let gap = (batch_seq - h).min(u32::MAX as u64) as u32;
                gap.saturating_mul(received)
            }
            // A late arrival: its slot was already charged as missing by
            // the batch that jumped over it, so it adds nothing to expected
            // and its symbols credit the charge back.
            Some(_) => 0,
        };

        if self.highest_seq.is_none_or(|h| batch_seq > h) {
            self.highest_seq = Some(batch_seq);
        }
        self.total_received += received as u64;
        self.total_expected += expected as u64;

        (expected, received)
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
mod tests;
#[cfg(test)]
mod hotpath_pin;

/// Resolves when the process is asked to stop: SIGINT (`ctrl_c`, every
/// platform) or SIGTERM (unix). Returns `true` when a signal arrived and
/// `false` only if the listener could not be installed. One awaited future,
/// one trigger — the two signals are not two shutdown paths.
async fn shutdown_signal() -> bool {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = match signal(SignalKind::terminate()) {
            Ok(t) => t,
            Err(_) => return tokio::signal::ctrl_c().await.is_ok(),
        };
        tokio::select! {
            r = tokio::signal::ctrl_c() => r.is_ok(),
            _ = term.recv() => true,
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await.is_ok()
    }
}
