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

pub mod ackdiag;
pub mod block_arq;
pub mod block_sender;
pub mod control_msg;
pub mod cpuprof;
pub mod diag;
pub mod emit_source;
pub mod eta;
pub mod framing;
pub mod interleave;
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
pub use sack::*;

use block_arq::BlockArq;
use block_sender::run_block_sender;
use diag::{DiagCtx, DiagInputs, DiagState};
use emit_source::{SenderCtx, SenderState, emit_source};
use sender_policy::SenderPolicy;

use crate::control::FecRateController;
use crate::control::fec_rate::ProtocolHint;
use crate::fec::{EncodingParams, FecBackend, FecDecoder, FecStream};
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
    /// Block interleaving depth (1 = disabled, 2+ = interleave across N blocks)
    pub interleave_depth: u32,
    /// Optional path to a pinned TLS certificate for server verification
    pub pin_cert: Option<std::path::PathBuf>,
    /// Which FEC backend to use (RaptorQ, RS or RLC)
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
    /// Reorder buffer timeout in ms (0 = disabled)
    pub reorder_timeout_ms: u64,
    /// Reorder buffer max capacity
    pub reorder_max_size: usize,
    /// Inner-feedback weight in [0,1] (paper 14.28): mid-stream repair
    /// floor for TCP-in-tunnel payloads. Default 0.0 — the L1 ablation
    /// measured the floor completion-neutral at C2, regressive at C3.
    pub inner_feedback_weight: f64,
    /// The completion feed (paper §14.26 / §16.82): remaining
    /// bytes of the transfer in progress, published by a driver that KNOWS
    /// them. `None` on the tunnel path — an endless stream has no `T_rem` —
    /// and `None` unless `RWM_COMPLETION_EXPOSURE` is armed, so the shipped
    /// engine is byte-identical without it. See [`CompletionFeed`].
    pub completion_feed: Option<Arc<CompletionFeed>>,
    /// Block-granular multipath source affinity (paper 13.8 in-order
    /// coupling refinement, L2 ws1). Default true; false = per-symbol
    /// striping (ablation).
    pub mp_block_affinity: bool,
    /// RWM Phase C (paper §16.2, H→∞ corner): out-of-order OBJECT delivery
    /// on the reliable window. Requires `window_reliable`; set only by the
    /// native object API (perf/MemTun), never the TCP-in-tunnel path. The
    /// receiver delivers each decoded source symbol the instant it decodes
    /// (any order — the consumer reassembles by offset), and the sender's
    /// retention backpressure is relaxed so the in-order frontier's lag on
    /// a slow path no longer throttles the fast path. Default false.
    pub window_out_of_order: bool,
    /// Fungible frontier (paper §16.3 "empty quadrant"): coded-object mode.
    /// On the reliable window, the sender emits ONLY coded (random-linear-
    /// combination) symbols over the window — no raw systematic source — so
    /// any K independent coded symbols from ANY path reconstruct the K
    /// sources and no symbol is a fixed in-order position a slow path can
    /// long-pole. Implies out-of-order delivery; requires `window_reliable`.
    /// Bulk-object / loose-δ ONLY. Default false.
    pub window_coded_only: bool,
    /// Generation-based cross-path fungible coding (paper §16.3, the
    /// oracle-validated stable-anchor fix). Coded symbols are RLC combinations
    /// WITHIN fixed generations of ~W_mp source symbols; each generation is a
    /// stable coding target that decodes out-of-order on any K_G independent
    /// symbols from any path, with generation-level recovery and NO per-seq
    /// ARQ beneath the code. Implies coded-only wire symbols + out-of-order
    /// delivery; requires `window_reliable`. Bulk-object / loose-δ ONLY.
    /// Default false.
    pub window_generation_coding: bool,
    /// Systematic + deficit-driven cross-path REPAIR (§16.3 oracle, the cheaper
    /// realization of generation coding — ×1.19 at C8 without coded-only's two
    /// L1-killers). Reuses the generation machinery (fixed-generation repair
    /// anchors of ~W_mp, deficit feedback, dense `GenerationDecoder`, NO per-seq
    /// ARQ, out-of-order delivery) but sends the RAW SYSTEMATIC SOURCE as primary
    /// (delivered on arrival, ZERO decode) instead of coded-only. Coded symbols
    /// are emitted ONLY as windowed repair (proactive `ceil(len·r)` per
    /// generation + deficit top-up), so decode is O(deficit)≈holes not O(G) and
    /// nothing waits for K_G. Implies generation-style receive + out-of-order;
    /// requires `window_reliable`. Bulk-object / loose-δ ONLY. Default false.
    pub window_systematic_repair: bool,
}

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
// (The reorder-buffer defaults that used to live here — timeout 20 ms, max
// 500 buffered — are supplied by `config::resolve` and reach the receiver as
// `config.reorder_timeout_ms` / `config.reorder_max_size`; the local copies
// had no readers.)
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
pub const MAX_NACK_REPAIRS_PER_NACK: usize = 10;
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

    /// Idle-triggered recovery floor (ADR-0046 hardening; the fix the P10b
    /// NOTE at the call site asked for). The blanket per-round `.max(1)` floor
    /// was correctly REJECTED because forcing a retransmit every round on a
    /// genuinely congested straggler adds load to the long pole (C8 14.0 ->
    /// 9.3 Mbit/s). But full suppression (`multiplier == 0`) can WEDGE a
    /// reliable transfer whose only remaining work is recovering a confirmed
    /// hole — the transfer stalls until the QUIC idle timeout.
    ///
    /// The resolution keys on the ONE state that distinguishes the two: is the
    /// sender still pushing new data (so repairs would pile onto a congested
    /// path), or is it IDLE-except-for-the-hole (no new source in flight ->
    /// no congestion WE are causing -> a targeted retransmit is free)? When
    /// idle, recovery is never fully suppressed: the multiplier is floored so
    /// the confirmed hole gets at least one retransmit per round. When active,
    /// the congestion multiplier governs unchanged — congestion safety still
    /// wins on the straggler. Continuous: `idle == false` returns the raw
    /// multiplier exactly (old behavior, bit for bit).
    fn effective_multiplier(&self, idle: bool) -> f64 {
        if idle {
            self.repair_multiplier.max(IDLE_RECOVERY_FLOOR)
        } else {
            self.repair_multiplier
        }
    }
}

/// Minimum NACK-repair multiplier when the sender is idle-except-for-recovery
/// (ADR-0046 idle-triggered floor). Scaled by `MAX_NACK_REPAIRS_PER_NACK`
/// (10) it yields >= 1 retransmit per round, enough to unwedge a stalled
/// reliable transfer without the rejected blanket floor's straggler load.
const IDLE_RECOVERY_FLOOR: f64 = 0.1;

/// Idle threshold floor (µs) below which `2×SRTT` would be too twitchy: a
/// sender that has sent no new source for at least this long (or 2×SRTT,
/// whichever is larger) is idle-except-for-recovery. 20 ms comfortably
/// exceeds a LAN RTT while staying well under the QUIC idle timeout.
const IDLE_RECOVERY_GAP_FLOOR_US: u64 = 20_000;

/// Returns true if this config should use sliding-window mode instead of block mode.
///
/// The pipeline shape follows from the algorithm's capabilities: streaming-native
/// backends (RLC) use the sliding-window pipeline; block-only backends
/// (RaptorQ, Reed-Solomon) always use the block pipeline. By default only
/// Task #61 (paper §16.20): the UNIFIED machine gate. When set, (a) the
/// receive path uses ONE decoder (`UnifiedDecoder` — the global sparse-aware
/// closure) for BOTH the sliding-window and generation wires, (b) the
/// Realtime hint rides the RLC family (δ-parameterization) instead of
/// switching code families, (c) plain-mode proactive repair follows the
/// quantity law (TaperBudget, #85) + the trailing solvable-span placement
/// with A* = clamp(rate·D, 1, W), D = b(hint)·RTprop (§8.8 budgets:
/// Realtime ½, Auto 1, Bulk 2 RTT), (d) generation mode runs the derived
/// M* pipeline depth (RWM_GEN_PIPE defaults ON), (e) the A* send-rate
/// anchor ships ON (`RWM_ASTAR_ANCHOR`, fix A), and (f) the realtime EVICT
/// path runs δ-honest overload shedding (`RWM_UNIFIED_SHED`, fix C).
///
/// **DEFAULT ON (2026-07-21, goal-gate "Unified Shedding + Flip Battery")**
/// — the pre-registered flip gate was met on both seeds: realtime tails ≥
/// legacy-RLC everywhere within the noise floor and ≤ the streaming machine
/// at every cell (c2 p99 medians 37/40 vs stream 40–43/52; c3 101–111 vs
/// 108–133), ZERO collapse-class reps (the #61 3/10 blocker eliminated),
/// 100% delivered at the c3 perf cell (vs streaming 79/81%) at completer
/// parity, bulk gen-sys parity within σ, knee no-regression.
///
/// **The streaming machine was RETIRED 2026-07-28** after its register
/// re-test clause was discharged cell-by-cell (goal-gate "Streaming Crown
/// Re-Test" 2026-07-27: unified ≤ streaming p99 medians at all 5 historic
/// crown cells × both seeds; the sub-noise cell-5 p999 WATCH is recorded as
/// historical). OPT-OUT SEMANTICS CHANGE: `RWM_UNIFIED=0` + Realtime now
/// selects the LEGACY-RLC windowed machine (`RlcWindowDecoder`) — before
/// the retirement it selected the streaming two-layer code. The legacy-RLC
/// machines stay (their own retirement clause, §17.5, was never re-argued).
pub(crate) fn unified_active() -> bool {
    crate::config::env_flag("RWM_UNIFIED", true)
}

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
///
/// NOTE (RWM Phase C, MEASURED). Relaxing this cap in out-of-order object
/// mode — the hypothesis that the store's cumulative-ack backpressure was
/// coupling the fast path to the slow path's frontier — was tried and
/// REFUTED: with backpressure off the sender drains the whole object into
/// the encoder, the O(200) coding window slides to the newest source and
/// away from the un-received holes, so proactive repairs stop covering them
/// and every hole falls to rate-limited targeted retransmit — C8 collapsed
/// to 2.5 Mbit/s (worse than the 11.4 in-order baseline). The store cap
/// keeps the coding window near the recovery frontier; it is kept. The
/// object-completion equivalence (§16.2) is why out-of-order delivery is a
/// no-op here anyway: in-order-with-retention already completes at
/// decode-on-total. See goal-gate "RWM Phase C".
const RELIABLE_STORE_MAX: usize = 1024;

/// feat/gen-substrate-ceiling: hard ceiling on the derived generation-pipeline
/// depth (bounds sender retention, the receiver reassembly span, and the
/// deficit-report width; 32·G ≈ 12k symbols ≈ 15 MB at 1200 B — the loose
/// memory backstop, ~the ooo_retain default ×2).
const GEN_PIPE_MAX_GENS: usize = 32;

/// feat/gen-substrate-ceiling: DERIVED generation-pipeline depth M* — task
/// #61's dynamic window advance A* = clamp(D·rate, 1, W), quantized to
/// generations (the stable-anchor code advances in whole generations, so the
/// depth of the in-flight generation pipeline IS the window advance).
///
/// First principles: for the decode frontier to keep advancing at the link
/// rate, the generations in flight must cover the time D from a generation's
/// first coded emission to its ack: D = delivery (≈ 1 RTT, the pipe) + one
/// deficit-feedback round for the loss-shortfall tail (≈ 1 RTT: report waits
/// ~SRTT cadence + top-up flight). Hence
///   M* = ceil(rate · 2·RTT / G) + 1
/// (+1 = the currently-filling head generation). `rate` is the windowed-MAX
/// delivered rate (decode-clocked samples are mostly-low with the true rate
/// at the burst top — §16.15's finding — so MAX is the recovery statistic);
/// `rtt_s` is RTprop (min-RTT), NOT the live SRTT — the live RTT includes the
/// queue this pipeline itself creates (positive feedback), while the
/// in-flight cap holds the actual RTT near RTprop (the BBR discipline).
/// Clamped to [2, GEN_PIPE_MAX_GENS]: 2 reproduces the legacy fixed pipeline
/// when the anchors have no sample yet (cold start).
fn gen_pipe_depth(rate_sym_per_s: f64, rtt_s: f64, gen_size: usize) -> usize {
    if rate_sym_per_s <= 0.0 || rtt_s <= 0.0 {
        return 2;
    }
    let d = 2.0 * rtt_s; // delivery + one repair round
    let m = ((rate_sym_per_s * d) / gen_size.max(1) as f64).ceil() as usize + 1;
    m.clamp(2, GEN_PIPE_MAX_GENS)
}

/// Per-path in-flight cap decision (the #64 fix, FMTCP-era provenance —
/// retained because the gen_pipe stack consumes it; the FMTCP composite
/// itself was REMOVED 2026-07-27 per the DEPRECATION REGISTER). Given each
/// active path's `(in_flight, per_path_cap)` where the cap = gain·BtlBw_i·RTprop_i
/// (that path's OWN windowed-max bandwidth × its OWN min-RTT), the sender is
/// "full" only when NO path is below its own cap. So the slow path's RTT-inflated
/// cap bounds ONLY the slow path, and the fast path keeps pulling source while the
/// slow path is full. The summed-anchor #64 bug was a single GLOBAL budget
/// gain·Σ_i BtlBw_i·RTprop_i that the fast path stalled behind (and that let the
/// slow path's inflated term over-drive its own queue into bufferbloat). Extracted pure for unit
/// testing.
fn infl_percap_full(per_path: &[(u64, u64)]) -> bool {
    !per_path.iter().any(|&(in_flight, cap)| in_flight < cap.max(1))
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
/// Lowest base [`now_us`] starts from, µs (~12.7 days): keeps every stamp
/// non-zero (`echo_send_timestamp_us == 0` is the timer-ack sentinel) and
/// leaves headroom below "now" even if the wall clock reads before 1970.
const NOW_US_MIN_BASE: u64 = 1 << 40;

/// The engine clock, µs. MONOTONIC: an `Instant` offset from a base read
/// ONCE from the wall clock at first use, so a wall-clock step (NTP, manual
/// set) can neither freeze nor storm the recovery clocks, and a clock before
/// the UNIX epoch cannot panic. The wall-clock base keeps stamps in the
/// familiar epoch-µs range (same varint width on the wire as before).
///
/// Every consumer is a same-process difference, an echo of this process's
/// own stamp (RTT, ETA book, ping), or — at the peer — an `arrival −
/// send_ts` difference that is min-subtracted or differenced again
/// (`lat.rs` A_x, `eta.rs` receiver ℓ, the estimator's jitter), in which the
/// constant offset between the two hosts' bases cancels. Nothing compares
/// this clock to the peer's as an absolute time.
pub(crate) fn now_us() -> u64 {
    static BASE: std::sync::OnceLock<(Instant, u64)> = std::sync::OnceLock::new();
    let &(t0, base_us) = BASE.get_or_init(|| {
        let wall_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);
        (Instant::now(), wall_us.max(NOW_US_MIN_BASE))
    });
    base_us.saturating_add(t0.elapsed().as_micros() as u64)
}

/// Collect per-generation residual deficits for the deficit-feedback report
/// (§16.3), receiver arm. Walks `gen_widths` (anchor→K_g) in anchor order,
/// skipping fully-decoded generations, and returns up to `report_gens`
/// `(anchor, deficit)` pairs where `deficit = K_g − rank_in(anchor, K_g)`.
///
/// PART 1 (receiver-tail parallelization). The legacy bound was 6 — the
/// receiver reported only the frontier ± a handful of generations, so a lossy
/// bulk transfer's holes were NACKed/repaired FRONTIER-FIRST, roughly one
/// generation per round-trip (serial tail, throughput ∝ window/RTT). Lifting
/// `report_gens` to cover the whole in-flight range makes the receiver report
/// EVERY outstanding generation's deficit in ONE report, so the sender repairs
/// all holes in a single round-trip (parallel tail flush). Extracted as a pure
/// function so the "all deficits recover in one round" invariant is unit-tested
/// without driving the whole async receiver loop.
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

/// Repair-coverage horizon gate (branch `feat/nack-timing`): the classic
/// FEC discipline of WAITING FOR THE CODED REPAIR before falling back to ARQ.
///
/// ROOT CAUSE it addresses (measured across 4 sessions). In generation mode the
/// deficit report IS the reactive NACK — and it fires the instant a hole appears
/// at the frontier, BEFORE the in-flight proactive repair covering that hole
/// (which rides with the surrounding data and arrives ~1 generation-span later)
/// has a chance to decode it. So a hole proactive repair WOULD have covered gets
/// a redundant ARQ round-trip instead, pinning the proactive recovery fraction at
/// ~0.4 and the throughput to the round-trip-bound regime at high RTT.
///
/// THE GATE. A generation's residual deficit is only ELIGIBLE to be reported
/// (a reactive NACK) once it has been outstanding for at least `horizon` — the
/// time for the covering proactive repair to arrive + decode (~the generation /
/// window span at the current send rate, NOT an RTT). Newly-deficient anchors are
/// ARMED (their first-seen instant recorded in `armed`) and WITHHELD; an anchor
/// that decodes within the horizon drops out of `deficits` and is disarmed — a
/// proactive recovery, NO round-trip. Only anchors whose horizon has expired are
/// returned (the reactive fallback that keeps reliability intact).
///
/// `horizon == 0` restores the byte-identical shipped path (report immediately).
/// Extracted pure so the "hole covered by proactive repair within the horizon
/// fires no NACK" invariant is unit-tested without the async receiver loop.
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
    // The RWM_* env-gate surface, resolved ONCE for this engine (src/gates.rs
    // — the consolidation-pass extraction of the former inline gate block).
    // Deprecation warnings (register Class-C gates) fire inside resolve().
    let gates = crate::gates::RuntimeGates::resolve();
    // LIVENESS ECHO (goal-gate "Gate-Forwarding Audit", 2026-08-09;
    // MEASUREMENT DISCIPLINE item 15): one `[GATES]` line naming every gate
    // resolved above and its RESOLVED value, on BOTH endpoints, once per
    // engine start. Two-sided by construction — the OFF values are printed
    // too, so a battery can assert "gate absent in the control" as
    // mechanically as "gate present in the arm". Cheap: one formatted line,
    // never on the hot path.
    gates.echo();
    // Parse TUN address
    let (tun_ip, prefix_len) = parse_cidr(&config.tun_addr)?;
    let netmask = prefix_to_netmask(prefix_len);

    // Backend selection happens ONCE, here, and is pinned for the life of
    // the stream (paper §16.4: no cross-code algebra ⇒ any mid-stream
    // switch strands in-flight data; the old runtime auto-switch was
    // removed). Computed before TUN creation because window mode
    // constrains the TUN MTU.
    //
    // Realtime rides the RLC family either way since the streaming machine's
    // retirement (2026-07-28): under the unified default it is the small-δ
    // parameterization of the span machine; under `RWM_UNIFIED=0` it falls
    // back to the LEGACY-RLC windowed machine (`RlcWindowDecoder`) — an
    // OPT-OUT SEMANTICS CHANGE, stated in the register row: before the
    // retirement, `RWM_UNIFIED=0` + Realtime selected the streaming
    // two-layer code. Bulk/Auto under `window_reliable` (RWM Phase A)
    // auto-select windowed RLC — the natural sliding-window codec; the
    // bulk profile's symbol_size=1200 puts the window-mode TUN MTU clamp
    // at 1196, so full-size packets are not fragmented.
    let effective_fec_backend = if !config.fec_backend_explicit {
        if config.protocol_hint == ProtocolHint::Realtime {
            if unified_active() {
                // §16.20: one code family across the δ axis — Realtime is the
                // small-δ parameterization of the RLC span machine, not a
                // different code. (Mechanism-liveness echo for the A/B.)
                info!("RWM_UNIFIED: Realtime rides the RLC span machine (small-δ parameterization, no code-family switch)");
            } else {
                // Mechanism-liveness echo for the legacy opt-out arm (the
                // pre-retirement echo was "auto-selecting streaming backend").
                info!("Realtime mode (RWM_UNIFIED=0): streaming machine retired — riding the legacy-RLC windowed machine");
            }
            FecBackend::Rlc
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
    // Fungible frontier (§16.3 coded-object): coded-only presupposes the
    // reliable window (retention is the ARQ backstop for aged holes). It is a
    // bulk-object mode that pays a window-fill decode latency, so it always
    // implies out-of-order object delivery.
    let window_coded_only = window_reliable && config.window_coded_only;
    // Generation-based fungible coding (§16.3 stable anchor). Composes ON TOP
    // of the reliable window: coded symbols are RLC combinations within FIXED
    // generations (stable target) rather than the moving sliding window, and
    // the per-seq ARQ beneath the code is switched OFF (recovery is
    // generation-level). Implies coded-only wire symbols + out-of-order object
    // delivery.
    // Systematic + deficit-repair (§16.3 oracle): a submode of the generation
    // machinery. `window_systematic` reuses ALL of generation mode's receive
    // path (dense decoder, gen deficit feedback, out-of-order delivery, no
    // per-seq ARQ) — so `window_generation` is TRUE whenever either flag is set
    // — and differs only on the SENDER: raw source rides the wire as primary and
    // the encoder emits only the `ceil(len·r)` repair overhead (see the
    // `systematic` arg to `run_window_sender`).
    // (The RWM_FMTCP decode-on-total composite that used to OR into these two
    // flags was REMOVED 2026-07-27: register RE-TESTED → CONFIRMED-REFUTED on
    // the clean substrate, "C8-Aware Pool Law" battery.)
    let window_systematic = window_reliable && config.window_systematic_repair;
    let window_generation = window_reliable
        && (config.window_generation_coding || config.window_systematic_repair);
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
    let recv_shutdown_rx = shutdown_tx.subscribe();

    // Spawn the shutdown-signal handler. SIGINT (`ctrl_c`) and, on unix,
    // SIGTERM are ONE trigger on ONE code path: both resolve `shutdown_signal`
    // and fire the same broadcast. SIGTERM matters because every tools/l1
    // harness stops the server with `pkill -x raptorpath` (SIGTERM), and
    // until it was handled that was an abrupt kill — no destructor ran, so
    // the receiver's exit flush (`net/recv_block.rs`, `final=1`) could never
    // be seen on L1 (goal-gate "OPERATOR SANCTION (2026-09-08 ~14:00Z)").
    let ctrlc_shutdown_tx = shutdown_tx.clone();
    tokio::spawn(async move {
        if shutdown_signal().await {
            info!("received SIGINT/SIGTERM, initiating graceful shutdown...");
            let _ = ctrlc_shutdown_tx.send(());
        }
    });

    // Shared window ACK: receiver writes, sender reads to advance the encoder window
    let window_ack_seq = Arc::new(AtomicU64::new(0));

    // NACK gap channel: handle_control_message sends gap ranges, window sender
    // receives for targeted repair. The batch rides with its [`FireCause`] tag
    // (`[FCAUSE]`) — a LABEL for the counters only; nothing branches on it.
    // A0.2: the tuple's middle slot is the PATH THE ACK CARRYING THIS GAP
    // REPORT ARRIVED ON (`control_msg.rs`'s `path_id`, already in scope
    // there). It is a LABEL for the attribution counters exactly as
    // `FireCause` is; nothing in the repair loop branches on it.
    let (nack_tx, nack_rx) =
        tokio::sync::mpsc::channel::<(FireCause, u32, Vec<(u64, u64)>)>(16);

    // Generation-deficit channel (§16.3): the data-arm's control handler parses
    // inbound GenerationDeficit messages and forwards the (anchor, deficit)
    // vector to the local window sender, which emits exactly the residual coded
    // symbols each frontier generation still needs (bounded, targeted recovery).
    let (deficit_tx, deficit_rx) =
        tokio::sync::mpsc::channel::<Vec<(u64, u32)>>(64);

    // Receiver-seat repair-request channel (paper 16.83, arms (A)/(B)): the
    // data-arm's control handler parses inbound `RepairRequest` messages and
    // forwards `(cause, spans)` to the local window sender, which serves each
    // span from `sent_store` at `m = 1` and from `generate_repair_range` above
    // it. THE EXACT PARALLEL OF `deficit_tx` ABOVE, at the same depth and with
    // the same best-effort contract -- the two are one mechanism in two
    // vocabularies, and writing them differently would be the defect.
    //
    // NEVER PRODUCED with both arms absent: `recv_request_tx` is `None`, so
    // an arriving `RepairRequest` is counted and dropped exactly as it is in
    // v8.
    let (request_tx, request_rx) =
        tokio::sync::mpsc::channel::<RepairRequestBatch>(64);

    // SACK flow-control channel (feat/sack-flow-control): forwards the
    // receiver's RECEIVED-above-frontier ranges (the SACK ranges themselves,
    // NOT the inverted gaps) to the plain-reliable window sender. The sender
    // prunes its sent-store for out-of-order-received symbols so its flow
    // control keys on TRUE outstanding-unacked, not the in-order cumulative-ack
    // frontier that freezes on every hole. Plain-reliable only (generation /
    // coded-only have their own structural backpressure and are left as-is).
    let (sack_tx, sack_rx) =
        tokio::sync::mpsc::channel::<Vec<(u64, u64)>>(64);

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
    let (msg_tx, msg_rx) = mpsc::channel::<(u32, WireMessage)>(4096);
    // Dedicated channel for stream-origin control: liveness must not queue
    // behind the data flood (see spawn_receiver_for_path).
    let (ctrl_tx, ctrl_rx) = mpsc::channel::<(u32, WireMessage)>(256);
    let _recv_handles = transport.spawn_receivers(msg_tx.clone(), ctrl_tx.clone());

    // Sender task: TUN → frame → encode → schedule → send
    let transport_arc = Arc::new(transport);
    let scheduler_arc = Arc::new(parking_lot::Mutex::new(scheduler));

    // ── feat/copa-sole-cc: plain-mode Copa delivery feed ───────────────────
    // PLAIN window-reliable mode never fed Copa's delivery-rate estimator:
    // WindowAcks recorded RTT only (verdict-audit 2026-07-13 finding — only
    // the block-path `ControlMessage::Ack` drives `Scheduler::ack →
    // PathState::on_ack`), so the per-path Copa cwnd sat pinned at
    // INITIAL_CWND and could not own a substrate window. This feed closes
    // that, sender-side only (no wire/receiver change): each plain source
    // send records seq→path + a BBR rate-sample snapshot (`on_src_sent`),
    // and each WindowAck's cumulative-frontier advance + newly-SACKed seqs
    // are attributed back to the path that carried them
    // (`on_src_delivered_seq` — SEND-interval Δt, ack-aggregation robust)
    // followed by the Copa cwnd dynamics (`on_delivery_signal`). The
    // BBR-correct sampler and NOT the legacy ack-interval `record_delivery`:
    // the ack-interval Δt spikes on frontier jumps and its windowed-max
    // over-read (×19 on the plain L0 smoke; §16.13 measured ×145-class in
    // gen mode) would pin cwnd ≫ BDP via the anchor floor — bufferbloat by
    // estimator, exactly what Copa-sole must not do. RTT floor + delivery
    // signal are then BOTH live in plain mode, per path (per connection =
    // per path), and the resulting cwnd is written into the pass-through
    // substrate window.
    //
    // Gated OFF by default (shipped plain path byte-identical): enabled by
    // `RWM_QUIC_CC=passthrough` (Copa-sole flies blind without it) or
    // standalone by `RWM_COPA_FEED=1` for the A/B. In-order plain mode ONLY:
    // the OOO/generation modes deliver out of order (the in-order frontier
    // is not their delivery signal).
    let copa_feed_plain: Option<Arc<CopaFeed>> = {
        let wanted = transport_arc.cc_passthrough_active() || gates.copa_feed;
        let plain_inorder = window_reliable
            && !window_generation
            && !window_coded_only
            && !config.window_out_of_order;
        // feat/anchor-hygiene (`RWM_PLAIN_RS`): the send-interval sampler
        // WITHOUT Copa ownership — plain mode under any substrate CC gets an
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
            // feat/copa-wire-signal mechanism-liveness echo (MEASUREMENT
            // DISCIPLINE): which clock feeds Copa's delay term, and the
            // hint-mapped δ the update law targets.
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
    let recv_copa_feed = copa_feed_plain.clone();

    // ── Honest Inputs (goal-gate "Honest Inputs", anchor-hygiene family) ──
    // Mechanism-liveness echoes (MEASUREMENT DISCIPLINE 1/15): asserted
    // PRESENT on the fix arms, ABSENT on the controls; the [GATES] line
    // carries the two-sided value either way. Emitted in `run_impl` so both
    // roles echo.
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

    // ── Window-mode control-datagram MERGE (env RWM_ACK_MERGE) ────────────
    // Goal-gate "Unlock The Default 1: ack-merge". Mechanism-liveness echo
    // (MEASUREMENT DISCIPLINE item 1) — emitted in `run_impl` so it fires in
    // BOTH roles: the receiver is what suppresses the legacy Ack, the sender
    // is what re-homes its consumers, and the battery asserts the echo on
    // both logs. Recorded here beside the CopaFeed construction on purpose:
    // whether a feed exists is exactly what decides how much work the
    // re-homing has to do, and in the shipped default it does not exist.
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

    // ── Derived stall gauge (goal-gate "Unlock The Default 2") —
    //    mechanism-liveness echo (MEASUREMENT DISCIPLINE item 1). Emitted in
    //    `run_impl` so both roles echo: the gauge has a sender arm
    //    (`sidle2=`) and a receiver arm (`idle2=`).
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
    let sender_window_coded_only = window_coded_only;
    let sender_window_generation = window_generation;
    let sender_window_systematic = window_systematic;
    let sender_window_ack = window_ack_seq.clone();
    let mut sender_nack_rx = nack_rx;
    let mut sender_deficit_rx = deficit_rx;
    let mut sender_sack_rx = sack_rx;
    let mut sender_request_rx = request_rx;
    let sender_protocol_hint = config.protocol_hint;
    // §14.26/§16.82: the completion feed, if a driver published
    // one. `None` on every shipped path.
    let sender_completion_feed = config.completion_feed.clone();
    let sender_gates = gates.clone();

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
            return;
        }

        run_block_sender(
            tun,
            sender_transport,
            sender_scheduler,
            sender_fec,
            sender_block_counter,
            sender_batch_counter,
            sender_sent_counts,
            sender_stats,
            sender_block_arq,
            sender_profile_max_block,
            sender_profile_flush,
            sender_profile_symbol_size,
            sender_fec_backend,
            sender_interleave_depth,
            sender_interleave_timeout,
            sender_shutdown_rx,
        )
        .await;
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
    // RWM Phase C (paper §16.2, H→∞): out-of-order object delivery is only
    // meaningful on the reliable window (it needs retention to guarantee
    // every hole is eventually recovered). The run() tunnel path never sets
    // window_out_of_order — only the native object API does. Coded-only
    // (fungible frontier, §16.3) ALSO forces out-of-order delivery: with no
    // systematic source on the wire the decoder emits each source seq only
    // when it is recovered by GE, in arbitrary order, so the receiver must
    // deliver-on-decode (there is no systematic in-order arrival to hold to).
    // Generation coding (§16.3 stable anchor) is likewise out-of-order: each
    // generation decodes on any K_G coded symbols and its sources are emitted
    // as they are recovered, reassembled by offset at the object layer.
    let recv_window_ooo = window_reliable
        && (config.window_out_of_order || window_coded_only || window_generation);
    // Fungible frontier (§16.5): the decoder must retain the wider W_mp coding
    // window so a coded symbol can still combine over its full span; mirror
    // the sender's win_cap (default 640, RWM_WINDOW override) or keep 200.
    // Generation mode retains the whole in-flight pipeline (M generations of
    // G symbols) so no not-yet-decoded generation is ever pruned early.
    let recv_win_cap: u64 = if window_generation {
        let g = gates.gen_size;
        let mut m = gates.pipeline;
        // feat/gen-substrate-ceiling: under the derived-depth pipeline the
        // sender may run up to GEN_PIPE_MAX_GENS generations of read-ahead, so
        // the receiver must retain that whole span (prune bound only).
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
    let recv_window_ack = window_ack_seq.clone();
    let recv_window_generation = window_generation;
    // Receiver arm of the deficit-feedback loop: the data-arm control handler
    // forwards inbound GenerationDeficit vectors to the LOCAL sender's recovery
    // loop over this clone (generation mode only).
    let recv_deficit_tx = deficit_tx.clone();
    // Generation coding (§16.3) turns the per-seq targeted ARQ OFF beneath the
    // code — the per-seq reliability layer is exactly what made the moving
    // window path-affine and invoked the ADR-0046 throttle (measured ×0.26).
    // With no NACK producer, a short generation is recovered by MORE coded
    // symbols for that generation (fungible, cross-path), never by resending a
    // specific seq. So the SACK→gap producer is suppressed in generation mode.
    //
    // ── PAPER 16.83.4 -- THE COLLISION SEAM (a), ONE `&&` ────────────
    //
    // The request law (`RWM_RECV_REQUEST_LAW`) makes the RECEIVER'S REPORT
    // the single authority for a repair. Its identifiability argument depends
    // on that: `rho_heal(l) = pi0*f(l)` holds EXACTLY on `[0, l*)` only
    // because no copy has flown there, and a copy flying is precisely what
    // 16.77.8a's censoring is. So while the arm is armed the per-seq
    // SACK->gap producer is suppressed at its source -- HERE, where it is
    // ARMED -- rather than filtered downstream, so `[FCAUSE] gap_data` going
    // to zero on the treatment arm IS the proof that the seam closed.
    //
    // `sack_tx` BELOW IS DELIBERATELY NOT TOUCHED, and the justification is
    // the one already written there: SACK drives store SLOT RELEASE and never
    // recoverability; pruning `sent_store` on SACK was refuted structurally
    // UNSAFE on 2026-07-07 (C7/C8 in-order DNF) and the safe realization is
    // ADR-0060's release. A request law that touched `sack_tx` would be
    // re-running a refuted experiment.
    //
    // The two backstops 16.83.4 leaves standing are untouched and OUT OF
    // DOMAIN by construction: the `p_lost` taper (measured ZERO at 4 of 4
    // cells) and the end-of-stream tail sweep, whose producer is the sender's
    // own `tail_deadline` arm and not this channel.
    //
    // ABSENT => this is the shipped predicate, character for character.
    let recv_request_law = request_law_armed(
        window_mode,
        window_reliable,
        window_generation,
        gates.recv_request_law,
    );
    // Arm (B) alone is the VOCABULARY-ONLY wiring test: the trigger stays the
    // shipped 2 ms sampler, so the gap producer stays armed and only the
    // message the receiver sends changes. The two arms COMPOSE; neither
    // selects a machine.
    let recv_rank_feedback = request_law_armed(
        window_mode,
        window_reliable,
        window_generation,
        gates.rank_feedback,
    );
    let recv_nack_tx: Option<
        tokio::sync::mpsc::Sender<(FireCause, u32, Vec<(u64, u64)>)>,
    > =
        if window_mode && !window_generation && !recv_request_law {
            Some(nack_tx)
        } else {
            None
        };
    // The request channel's producer seat: `Some(..)` iff EITHER arm is live,
    // so arm (B) alone still has a server. `None` on every shipped path => an
    // arriving `RepairRequest` keeps its v8 counted-and-dropped behaviour.
    let recv_request_tx: Option<tokio::sync::mpsc::Sender<RepairRequestBatch>> =
        if recv_request_law || recv_rank_feedback {
            Some(request_tx)
        } else {
            None
        };
    if recv_request_law || recv_rank_feedback {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE item 1), emitted in
        // `run_impl` so BOTH roles echo: the receiver builds the message, the
        // sender serves it, and the battery asserts the echo on both logs.
        info!(
            request_law = recv_request_law,
            rank_feedback = recv_rank_feedback,
            gap_producer_armed = recv_nack_tx.is_some(),
            "receiver-seat repair request ACTIVE (paper 16.83: the receiver \
             REQUESTS repairs at lateness l >= l*_recv instead of the sender \
             inferring them from inverted SACK ranges; the per-seq SACK->gap \
             producer is suppressed by the collision seam while the request \
             law is armed; sack_tx UNTOUCHED - ADR-0060 store release)"
        );
    }
    // SACK forwarding channel producer. Historical note: the original consumer
    // was the RWM_SACK_PRUNE experiment (feat/sack-flow-control, 2026-07-07),
    // refuted structurally UNSAFE — pruning `sent_store` on SACK destroys the
    // only retransmittable copy of a received-then-evicted symbol (C7/C8
    // in-order dual DNF). REMOVED 2026-07-27 per the DEPRECATION REGISTER
    // (deprecate-HARD, no re-test owed); the safe realization of the same goal
    // is the SACK-clocked store release below (slot release, never
    // recoverability — ADR-0060).
    // SACK-clocked store release (env `RWM_STORE_SACK_RELEASE`, goal-gate
    // "SACK-Clocked Store Release"): the SENDER uncounts SACKed ranges from
    // the flow-control outstanding — see the sender-loop drain.
    // DEFAULT ON (2026-07-21): the pre-registered battery earned the flip
    // (c7 0.96–1.05×Σ both seeds, sc2 +3–4, no regression; =0 is the
    // legacy frontier-only-release opt-out arm).
    let store_sack_release_enabled = gates.store_sack_release;
    let recv_sack_tx: Option<tokio::sync::mpsc::Sender<Vec<(u64, u64)>>> =
        if store_sack_release_enabled
            && window_reliable
            && !window_generation
            && !window_coded_only
        {
            Some(sack_tx)
        } else {
            None
        };
    // SACK + BDP reassembly (feat/sack-bdp-reassembly): RWM_REASM_BDP hardens
    // the RECEIVER so a sender decoupled from the in-order frontier is SAFE for
    // reliable in-order delivery. The RELIABILITY INVARIANT it guarantees: a
    // received symbol is NEVER evicted from the receiver's reassembly state
    // before it is delivered (its in-order frontier passes), so a symbol whose
    // sender-side slot was released on SACK always
    // survives at the receiver until use → no un-recoverable eviction. Concretely
    // it (a) clamps the window-decoder/received-seq prune so it can never advance
    // ABOVE the delivered frontier (the reorder buffer is already usize::MAX / non-
    // evicting), and (b) probes the reassembly occupancy so the bound can be
    // reported (`[REASM]`). The reassembly stays BDP-bounded because the sender's
    // outstanding is bounded (plain_dyn_cap = gain·BDP store cap, default-on) and
    // working FEC recovers holes fast. Default-off; the shipped path is untouched.
    let reasm_bdp_on = gates.reasm_bdp;

    // ack-merge (RWM_ACK_MERGE, goal-gate "Unlock The Default 1"): hoisted
    // for the receiver's per-batch hot path. Scoped to WINDOW mode — block
    // mode must stay bit-exact, and `recv_window_mode` is the same predicate
    // the block_arq wiring already uses to pass `None` in window mode.
    let ack_merge_recv = gates.ack_merge && recv_window_mode;
    // ack-merge density gauge (`[CTLD]`), RWM_DIAG only — behavior-inert.
    let recv_diag_on = gates.diag;

    // Engine-receiver saturation probe (roadmap item 2, feat/engine-parallel
    // STEP 1). RWM_RDIAG=1 samples (a) the engine task's busy fraction
    // (1 − time-awaiting-select / wall) and (b) the inbound msg-channel depth
    // (queued behind the single engine task). Distinguishes "the engine task
    // is the service-rate wall" (busy→100%, q deep) from "the wall is
    // upstream" (busy low, q empty). Probe only — no behavior change; the
    // WeakSender adds no channel-close semantics.
    let rdiag_probe = msg_tx.downgrade();

    let recv_gates = gates.clone();
    // The receiver task (net seam pass 3 → net/receiver.rs). Every argument
    // below is the local the `async move` block used to CAPTURE, cloned at
    // the same line as before and passed at the same spawn; `run_receiver`
    // is an `async fn`, so building its future here runs none of its body —
    // the task still starts executing when the runtime polls it.
    let receiver_handle = tokio::spawn(receiver::run_receiver(
        recv_shutdown_rx,
        msg_rx,
        sent_counts,
        recv_copa_feed,
        recv_tun_tx,
        recv_scheduler,
        recv_fec,
        recv_decoders,
        recv_fec_backend,
        recv_transport,
        recv_block_arq,
        recv_batch_counter,
        recv_path_tracking,
        recv_stats,
        recv_symbol_size,
        recv_window_mode,
        recv_window_reliable,
        recv_window_ooo,
        recv_win_cap,
        recv_window_ack,
        recv_window_generation,
        recv_deficit_tx,
        recv_nack_tx,
        recv_sack_tx,
        // Paper 16.83 arms (A)/(B): the request producer and the two
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
        // The contract's own dial position at the RECEIVER (§16.81, in
        // flight): the same `config` field the sender's policy reads.
        config.protocol_hint,
    ));

    // ADR-0004: periodic cleanup of stale decoders
    let cleanup_decoders = active_decoders.clone();
    let cleanup_fec = fec_controller.clone();
    let cleanup_stats = stats.clone();
    let cleanup_handle = tokio::spawn(tasks::run_decoder_gc(
        cleanup_decoders,
        cleanup_fec,
        cleanup_stats,
    ));

    // Block-mode ARQ sweeper (P8) — see `net::tasks::arq_sweep`.
    let sweep_block_arq = block_arq.clone();
    let sweep_scheduler = scheduler_arc.clone();
    let sweep_transport = transport_arc.clone();
    let sweep_stats = stats.clone();
    let sweep_batch_counter = batch_counter.clone();
    let sweep_window_mode = window_mode;
    let sweep_shutdown_rx = shutdown_tx.subscribe();
    let arq_sweep_handle = tokio::spawn(tasks::run_arq_sweep(
        sweep_block_arq,
        sweep_scheduler,
        sweep_transport,
        sweep_stats,
        sweep_batch_counter,
        sweep_window_mode,
        sweep_shutdown_rx,
    ));

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
    let cmd_scheduler = scheduler_arc.clone();
    let cmd_stats = stats.clone();
    let cmd_msg_tx = msg_tx.clone();
    let cmd_ctrl_tx = ctrl_tx.clone();
    let next_path_id = Arc::new(AtomicU64::new(config.bind_addrs.len() as u64));
    let cmd_shutdown_rx = shutdown_tx.subscribe();
    let cmd_handle = tokio::spawn(tasks::run_path_cmd(
        path_cmd_rx,
        cmd_transport,
        cmd_scheduler,
        cmd_stats,
        cmd_msg_tx,
        cmd_ctrl_tx,
        next_path_id,
        cmd_shutdown_rx,
    ));

    // RTCP-style periodic report + keepalive task
    let report_transport = transport_arc.clone();
    let report_scheduler = scheduler_arc.clone();
    let report_stats = stats.clone();
    let report_symbol_size = profile.symbol_size;
    let report_shutdown_rx = shutdown_tx.subscribe();
    let report_handle = tokio::spawn(tasks::run_report(
        report_transport,
        report_scheduler,
        report_stats,
        report_symbol_size,
        report_shutdown_rx,
    ));

    // Control fast path: liveness-critical messages (PathReport, Ping,
    // Pong) are handled immediately; anything else that arrives via the
    // reliable stream is forwarded to the ordered data loop.
    let ctrl_scheduler = scheduler_arc.clone();
    let ctrl_fec = fec_controller.clone();
    let ctrl_decoders = active_decoders.clone();
    let ctrl_transport = transport_arc.clone();
    let ctrl_stats = stats.clone();
    let ctrl_forward_tx = msg_tx.clone();
    let ctrl_mstar_anchor = gates.mstar_anchor;
    let ctrl_handle = tokio::spawn(tasks::run_control_fastpath(
        ctrl_rx,
        ctrl_scheduler,
        ctrl_fec,
        ctrl_decoders,
        ctrl_sent_counts,
        ctrl_transport,
        ctrl_stats,
        ctrl_forward_tx,
        ctrl_mstar_anchor,
    ));

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

/// Deliver a decoded packet to the TUN inject channel under the stream's
/// delivery policy.
///
/// - **Reliable** streams must NOT silently drop: the delivery frontier/ack
///   advances over DECODED seqs, so a dropped packet would advance the ack
///   past a symbol the consumer never received — a permanent hole. A full
///   channel therefore BACKPRESSURES the receiver (await). The consumer
///   (object app / kernel-TUN writer) always drains, so this cannot wedge.
/// - **Lossy** streams (EVICT / δ < ∞, and lossy-unordered datagram) must
///   NEVER block — a stale packet is worthless and blocking would stall the
///   whole stream on one slow consumer, so a full channel DROPS.
///
/// Returns `Err(())` only when the channel is permanently closed.
async fn deliver_packet(
    tx: &mpsc::Sender<Bytes>,
    pkt: Bytes,
    reliable: bool,
) -> Result<(), ()> {
    if reliable {
        tx.send(pkt).await.map_err(|_| ())
    } else {
        match tx.try_send(pkt) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                warn!("TUN inject channel full, dropping packet (lossy stream)");
                Ok(())
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Err(()),
        }
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
/// the `covered_paths` argument for RWM repair placement (§16.3 ρ_fate). One
/// entry per in-window source symbol (with multiplicity), so the placement
/// law's fate term is the fraction of the repair's coverage on each path. A
/// fungible repair covers the whole window; entries that predate the window
/// (still in the retained map) are excluded by the span filter.
fn window_source_paths(
    encoder: &dyn WindowEncoder,
    source_path_map: &std::collections::BTreeMap<u64, u32>,
) -> Vec<u32> {
    let (win_start, win_end) = encoder.window_span();
    (win_start..=win_end)
        .filter_map(|seq| source_path_map.get(&seq).copied())
        .collect()
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
    nack_rx: &mut tokio::sync::mpsc::Receiver<(FireCause, u32, Vec<(u64, u64)>)>,
    // Generation-deficit feedback (§16.3): each element is the receiver's
    // reported (generation_anchor, residual_deficit) vector. Drives the
    // bounded, targeted recovery emission that replaces the feedback-free cap.
    deficit_rx: &mut tokio::sync::mpsc::Receiver<Vec<(u64, u32)>>,
    // SACK flow-control (feat/sack-flow-control): the receiver's RECEIVED-above-
    // frontier ranges. Draining these prunes the sent-store for out-of-order
    // deliveries so the plain-reliable flow-control gate (store_len) tracks TRUE
    // outstanding, decoupling the sender from the in-order cumulative frontier.
    // Only fed in plain-reliable mode; empty (never producing) otherwise.
    sack_rx: &mut tokio::sync::mpsc::Receiver<Vec<(u64, u64)>>,
    // Receiver-seat repair requests (paper 16.83 arms (A)/(B)): each
    // element is `(cause, spans)` with `spans[i] = (start, count,
    // deficit)`. Drives the span-serving loop that mirrors the
    // generation-deficit recovery emission. NEVER PRODUCED with both arms
    // absent -- `recv_request_tx` is `None` there -- so the select arm
    // below is disarmed and this receiver is silent for the whole run.
    request_rx: &mut tokio::sync::mpsc::Receiver<RepairRequestBatch>,
    shutdown_rx: &mut tokio::sync::broadcast::Receiver<()>,
    protocol_hint: ProtocolHint,
    // RWM Phase A: RETAIN-UNTIL-ACKED retention at the ARQ layer (see the
    // policy block above RELIABLE_STORE_MAX).
    reliable: bool,
    // Fungible frontier (§16.3 "empty quadrant"): emit ONLY coded (random-
    // linear-combination) symbols over the window in place of raw systematic
    // source. The source bytes are still fed to the encoder window and the
    // retention store (so ARQ can retransmit the exact symbol for an aged,
    // localized hole), but nothing systematic goes on the wire during normal
    // flow — every transmitted payload symbol is a fungible combination, so
    // no specific symbol is a fixed in-order position a slow path long-poles.
    coded_only: bool,
    // Generation-based coding (§16.3, the oracle-validated stable-anchor fix).
    // Codes coded symbols WITHIN fixed generations of `RWM_GEN` (default 384)
    // source symbols — a STABLE anchor, unlike the moving sliding window — with
    // `RWM_PIPELINE` (default 2) generations concurrently in flight. Implies
    // coded_only wire symbols. Crucially it turns the per-seq targeted ARQ OFF
    // (no retransmit store, no NACK loop, no tail sweep): a short generation is
    // recovered by MORE coded symbols for that generation (fungible cross-path),
    // never by resending a specific seq — the per-seq layer is what made the
    // moving window path-affine and drove the ×0.26 drag.
    generation: bool,
    // Systematic + deficit-repair (§16.3 oracle). A submode of `generation`
    // (the caller passes `generation=true` alongside this): the RAW SYSTEMATIC
    // source rides the wire as PRIMARY (striped work-conserving, delivered
    // out-of-order with ZERO decode at the receiver's dense decoder) instead of
    // coded-only's "every symbol coded". The paced coded block still runs but,
    // via `GenerationEncoder::new_systematic`, emits only the `ceil(len·r)`
    // repair overhead per generation (plus the deficit-driven top-up) — coded
    // symbols cover only the HOLES, so decode is O(deficit) not O(G). Removes
    // the two coded-only L1-killers (decode-on-K latency + O(G²) decode) while
    // keeping the same fungible cross-path recovery and no per-seq ARQ.
    systematic: bool,
    // feat/copa-sole-cc: Some(..) in plain in-order mode when the Copa
    // delivery feed is on — source sends (and targeted retransmits) record
    // seq→path + a BBR send-interval rate-sample snapshot so the WindowAck
    // handler can attribute deliveries per path. None = shipped path.
    copa_feed: Option<Arc<CopaFeed>>,
    // §14.26/§16.82: remaining bytes of the transfer, when a
    // driver knows them. `None` on every shipped path; read ONLY under
    // `RWM_COMPLETION_EXPOSURE`, so the rate is byte-identical without it.
    completion_feed: Option<Arc<CompletionFeed>>,
    // The engine's env-gate surface, resolved once in run_impl (src/gates.rs).
    gates: crate::gates::RuntimeGates,
) {
    // ── The sender's resolve-once policy (net seam pass 2, 2026-08-09) ────
    // The ~56 derived constants that used to be declared one at a time
    // across 1,300 lines of setup — every one of them a `let` WITHOUT `mut`,
    // so structurally incapable of being reassigned — now resolve together,
    // verbatim and in their original order, in `SenderPolicy::resolve`
    // (net/sender_policy.rs). The mechanism-liveness `info!` echoes stay
    // below, in their original order, reading `pol`; so does the span-law
    // trace's own `now_us()` t0, which rebinds `pol` where it was sampled.
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
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE item 1) for the
        // PLAIN-mode subset of the M* repair, which is NOT generation-gated:
        // (a) the peer-report RTT no longer feeds the local estimators (the
        // 50-ms pseudo-sample floor pin, PathReport arm) and (b) the
        // estimator RTT EWMA seeds from its first measured sample instead
        // of crawling from the 50-ms constant (LossEstimator
        // rtt_seed_from_sample). The consolidation battery's LOO arm keys
        // on this echo in plain cells.
        info!("M* peer-report RTT-feed suppression ACTIVE (RWM_MSTAR_ANCHOR plain-live subset: local-echo-only RTT feed + estimator seed-from-sample)");
    }
    let mut prev_ack: u64 = 0;
    // Generation-mode paced coded emission (see the emission block in the loop).
    // The token bucket is clocked at the DELIVERED goodput — measured from the
    // cumulative-ack (window_ack_seq) progress, i.e. the receiver-driven rate at
    // which decoded source symbols are completing. This is the true link
    // goodput and is NON-circular (unlike the send-rate estimator or the stuck
    // window-mode cwnd, which never grows past INITIAL_CWND). A small headroom
    // factor lets the rate ramp; a bootstrap floor primes the first generation
    // before any ack exists. Decouples coded emission from TUN intake so a
    // generation buffered under backpressure keeps accumulating its K_G.
    let mut gen_coded_total: u64 = 0; // cumulative coded symbols emitted
    // Sampled HERE, at its original point in setup; moved into SenderState below.
    let gen_last_source_us: u64 = now_us(); // last source-intake time
    // Delivered-goodput pacing (§16.3): clock the token-bucket refill to the
    // measured ack (decode) rate rather than a fixed ceiling, so coded emission
    // never outruns the receiver's O(G²) decode/intake — the fix for the bursty
    // overrun that drops coded on the droppable datagram path. EWMA of ack
    // deltas; a bootstrap floor primes the first generation before any ack.
    let mut gen_rate_ewma: f64 = 0.0;
    let mut gen_rate_sample_us: u64 = now_us();
    let mut gen_rate_sample_ack: u64 = 0;
    // ── gen_pipe state (feat/gen-substrate-ceiling; inert unless RWM_GEN_PIPE) ─
    // Derived pipeline depth M* + dynamic intake cap, recomputed every ~5 ms
    // from the windowed-MAX delivered rate and SRTT (gen_pipe_depth above).
    let mut gen_pipe_m: usize = 2;
    let mut gen_pipe_store_cap: usize = 2 * pol.gen_size;
    let mut gen_pipe_refresh_us: u64 = 0;
    // Windowed-MAX delivered-rate filter. The cumulative ack advances in
    // whole-generation bursts, so a rate bucket must span MANY generations to
    // read the true rate rather than the burst/gap alternation: bucket span
    // 2 s (≥ 4·G/R for R ≥ 768 sym/s ⇒ ≤ 25% quantization), max over the
    // last 4 buckets (8 s window, ≫ any deficit round).
    let mut gp_bucket_start_us: u64 = now_us();
    let mut gp_bucket_ack: u64 = 0;
    let mut gp_rates: std::collections::VecDeque<f64> = std::collections::VecDeque::new();
    let mut gp_rate_max: f64 = 0.0;
    // Per-generation deficit-feedback recovery state (§16.3). This closes the
    // rateless-with-feedback loop that the feedback-free recovery cap could not:
    //   * `gen_want[a]`  — coded symbols still to emit for generation anchored at
    //                      `a`, from the LAST deficit report (= reported deficit
    //                      minus what was already in flight). Consumed, paced,
    //                      round-robin, by the recovery-emission block below.
    //   * `gen_emitted[a]` — cumulative coded symbols this sender has put on the
    //                      wire for generation `a` (proactive + recovery). The
    //                      receiver's reported deficit reflects everything it has
    //                      RECEIVED, so `emitted − emitted_at_report` is the count
    //                      still in flight (not yet reflected) — subtracted from
    //                      the fresh deficit so we never double-send. "Send the
    //                      deficit, wait ~RTT for the updated deficit, re-evaluate."
    let mut gen_want: BTreeMap<u64, u64> = BTreeMap::new();
    // PROACTIVE vs REACTIVE recovery accounting (proactive-FEC-vs-ARQ crossover
    // instrumentation). `proactive_coded_total` counts coded symbols emitted by
    // the open-loop per-generation provisioning round-robin (`generate_repair`,
    // upfront repair — NO feedback round-trip). `recovery_coded_total` counts
    // coded symbols emitted by the deficit-driven recovery loop
    // (`generate_repair_for`, which fires ONLY after a receiver GenerationDeficit
    // report — one feedback round-trip). The proactive-recovery FRACTION =
    // proactive/(proactive+recovery) tells us whether Mode B genuinely recovers
    // holes from upfront repair (fraction→1, zero round-trips) or is secretly
    // paying reactive round-trips (fraction low). Printed on RWM_PFRAC/RWM_TRACE.
    let mut proactive_coded_total: u64 = 0;
    let mut recovery_coded_total: u64 = 0;
    let mut pfrac_last_us: u64 = 0;
    // `[SHEDH]` (paper §16.81 ρ leg): last emission of the
    // receiver-hold bind gauge, on the same 1 s cadence convention.
    let mut shedh_last_us: u64 = 0;
    // `[CHI]` (paper §16.82): last emission of the
    // completion-exposure gauge, same cadence.
    let mut chi_last_us: u64 = 0;
    // ── PAPER 16.83 ARMS (A)/(B): THE REQUEST-SERVING SEAT ───────────────
    //
    // The receiver-seat request law makes the RECEIVER the authority and the
    // sender a SERVER of spans. The bookkeeping below is the
    // generation-deficit machinery's, term for term, because it is the same
    // mechanism in a second vocabulary:
    //
    //   * `req_want[a]`     -- equations still owed over the span anchored at
    //                          `a`, with the span's own width `m`.
    //   * `req_emitted[a]`  -- cumulative answers this sender has put on the
    //                          wire for that span, ever.
    //   * `req_at_report[a]`-- `req_emitted[a]` as of the LAST report, so the
    //                          in-flight subtraction is "sent since the
    //                          receiver last looked" and a sub-RTT report
    //                          stream cannot re-send the same answers.
    //
    // **THE FIRST-REPORT SPECIAL CASE IS LOAD-BEARING AND IS COPIED
    // DELIBERATELY** (see the deficit consumer's own note): with no baseline
    // the in-flight count is 0, not `emitted`. Initialising the baseline to 0
    // instead would treat every earlier proactive symbol as in flight, send
    // nothing, and DEADLOCK the loop -- the measured first-report deadlock.
    let mut req_want: BTreeMap<u64, (u16, u64)> = BTreeMap::new();
    let mut req_emitted: std::collections::HashMap<u64, u64> =
        std::collections::HashMap::new();
    let mut req_at_report: std::collections::HashMap<u64, u64> =
        std::collections::HashMap::new();
    // `[REQS]` -- the SERVING gauge, cumulative, 1 s cadence, LAST LINE WINS
    // (the `[RFA]`/`[SHEDH]` convention; a `Drop` never survives the
    // harness's SIGKILL of a server).
    //
    // **WHY THE REQUEST FIRES ARE NOT COUNTED INTO `[FCAUSE]`.** `[FCAUSE]`
    // classifies fires of the SACK->gap machinery, and the whole proof that
    // the collision seam closed is `[FCAUSE] gap_data -> 0` on the treatment
    // arm. Folding request fires into that line would erase exactly the
    // reading the arm exists to produce. The cause tag the receiver stamped
    // rides here instead, in `[FCAUSE]`'s own class vocabulary, so no
    // information is lost and no existing counter changes meaning.
    let mut reqs_last_us: u64 = 0;
    // Reports consumed, spans in them, and the largest `m` ever asked for.
    let mut reqs_reports: u64 = 0;
    let mut reqs_spans: u64 = 0;
    let mut reqs_m_max: u64 = 0;
    // Answers that reached the wire, by kind: a per-seq COPY out of
    // `sent_store` (the `m = 1` corner -- today's bytes exactly, which is what
    // keeps `[RFA] dup_src` comparable to CTL) and a SPAN-CODED equation.
    let mut reqs_copy: u64 = 0;
    let mut reqs_coded: u64 = 0;
    // **WA1 -- THE SOUNDNESS PRECONDITION, COUNTED AND NEVER ASSUMED**
    // (16.83.3): `generate_repair_range` refuses unless the WHOLE span is
    // still retained. `wa1_some` is an answer in the requested vocabulary;
    // `wa1_none` is the refusal, which falls back to per-seq copies out of
    // `sent_store`. A request law whose answers silently degrade to copies is
    // the shipped machine with extra latency, so the split is a COUNT.
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
    // The arms, resolved ONCE from the same three predicates `run_impl` used
    // to decide whether to give the receiver a producer at all -- so the
    // producer and the consumer can never disagree about whether the arm is
    // live. Arm (B) alone is served too: it is the VOCABULARY-only wiring
    // test, and an unserved request is a deadlock, not a control.
    let request_arm = reliable
        && !generation
        && (gates.recv_request_law || gates.rank_feedback);
    let mut gen_emitted: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
    let mut gen_emitted_at_report: std::collections::HashMap<u64, u64> =
        std::collections::HashMap::new();
    // Fixed-rate pacing (token bucket) for coded emission: without it the flow
    // window is spent as one instantaneous burst of datagrams, which the QUIC
    // datagram path DROPS (unreliable, droppable) faster than the receiver can
    // decode — so a sealed generation never accumulates rank. The rate is a
    // generous ceiling (RWM_GEN_RATE symbols/sec, ~100 Mbit at 1.5 kB); the
    // ack-clocked flow window is the real limiter, this just spreads the bursts.
    let mut gen_tokens: f64 = 0.0;
    let mut gen_tok_last_us: u64 = now_us();
    let mut src_tok_last_us: u64 = now_us();
    // Fix 1 (rate signal): the delivered-goodput EWMA is clocked on the IN-ORDER
    // cumulative ack, which STALLS at 0 whenever a hole wedges the frontier —
    // exactly the high-RTT-lossy case — so `eff_pace` collapses to the bootstrap
    // FLOOR (2000 sym/s ≈ 24 Mbit) and THROTTLES the source ramp below the link
    // (ARQ, unpaced, ramps freely → FEC loses). The Copa cwnd/SRTT is the
    // frontier-INDEPENDENT CC rate (cwnd grows on delivery feedback regardless of
    // the in-order hole); pace at max(cwnd/SRTT, goodput-EWMA)×headroom so a
    // stalled in-order frontier can no longer starve the pace rate. Cached off
    // the scheduler lock every 5 ms. This is the directive's "cwnd/RTT via Copa".
    let mut cc_rate_cached: f64 = 0.0;
    let mut cc_rate_refresh_us: u64 = 0;
    // Pace ceiling = gen_rate × live-path count (single-link burst guard,
    // scaled so it cannot clamp a multi-path aggregate — see the refresh
    // block below). Starts at one link's worth.
    let mut cc_rate_ceiling: f64 = pol.gen_rate;
    // anchor → wall-clock (µs) of the last reactive emission for that generation.
    let mut gen_recover_at: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
    if pol.store_paths_on && pol.plain_dyn_cap {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE): the recorded run
        // must show which outstanding-pool law was active.
        info!(
            pool_per_path = pol.store_path_pool,
            gain = pol.store_bdp_gain,
            "path-scaled outstanding pool ACTIVE (RWM_STORE_PATHS: cap = clamp(gain*N*pipe, floor, N*pool) for N>=2 live paths; N=1 legacy)"
        );
    }
    if pol.pool_anchor_on {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE item 1).
        info!(
            pool_per_path = pol.store_path_pool,
            gain = pol.store_bdp_gain,
            "pool-anchor honest dual-store law ACTIVE (RWM_POOL_ANCHOR: N>=2 pooled cap = sum_i honest_store_cap(sr_i*RTprop_i, sr_i, K_i, gain) on the per-path send-interval anchor, clamp [floor, N*knee]; all-warm else path-scaled fallback; Copa cwnd feed untouched; N=1 legacy)"
        );
    }
    if pol.store_sack_release_on {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE item 1).
        info!(
            "SACK-clocked store release ACTIVE (RWM_STORE_SACK_RELEASE: SACKed seqs \
             uncounted from the outstanding gate, payload + ARQ maps retained until \
             the cumulative frontier — slot release, never recoverability)"
        );
    }
    if pol.store_cap_unified {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE 1): asserted
        // PRESENT on the unified arm, ABSENT on the default arm.
        info!(
            "unified store-cap path set ACTIVE (RWM_STORE_CAP_UNIFIED, goal-gate \"Store-Cap Triplication\": the plain dyn-store-cap phase's Sigma-anchor base and honest per-path cap sum iterate live_paths() instead of the cwnd-saturation-filtered active_paths(), so the path-scaled law's Sigma-base and its xN multiplier range over the SAME set; Copa-sole, capw and pool-anchor already read live_paths(); RWM_STORE_CAP_UNIFIED=0 = the shipped-default control arm)"
        );
    }
    if pol.three_term_on {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE 1/15): asserted
        // PRESENT on the three-term arm, ABSENT on the default arm; the
        // per-tick `[3T]` line carries the three terms separately.
        info!(
            rho = pol.contract_rho,
            b = pol.delta_b,
            "three-term outstanding limit ACTIVE (RWM_THREE_TERM, goal-gate \"Three-Term Law\": \
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
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE): asserted
        // PRESENT on honest-cap arms, ABSENT on knee-clamp control arms.
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
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE).
        info!(
            "budget-conserving taper emission ACTIVE (RWM_TAPER_R: plain-mode proactive repair budgeted at r x source per coding window; legacy = r per ack cycle)"
        );
    }
    if pol.unified_span {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE).
        info!(
            hint = ?protocol_hint,
            "unified span law ACTIVE (RWM_UNIFIED: plain-mode proactive repair over the trailing solvable span [end-A*, end-Δ), A* from δ)"
        );
    }
    if pol.astar_anchor_on {
        info!("A* send-rate anchor ACTIVE (RWM_ASTAR_ANCHOR: windowed-max send rate over ~8 SRTT, clock-gap sample discard)");
    }
    if pol.shed_on {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE item 1).
        info!(
            "unified overload shedding ACTIVE (RWM_UNIFIED_SHED: past-deadline holes shed within the derived 1-rho budget; =0 = serializing arm)"
        );
    }
    // feat/anchor-hygiene (`RWM_CLOCK_GAP`): the PROCESS-clock stall witness
    // — a dedicated 50-ms timer tick; a tick interval ≫ the period is a
    // whole-process scheduler stall (the timer wheel itself froze), and the
    // ack-fed estimator feed sites (Ack/WindowAck/PathReport arms + the
    // report-tick throughput feed) discard samples for the quarantine
    // window (the release flood — the measured BtlBw ×13 / cwnd ×16 /
    // EWMA-RTT ×3 post-stall poisoning). Ack SILENCES with a live process
    // never trip it (see control::anchor::StallWitness — the arrival-clock
    // variant mis-fired on normal recovery quiet periods, measured).
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
    // diag/unified-collapse (roadmap item 3): ~500 ms sender-side span-law
    // trace (RWM_DIAG only) — the live A*/Δ, owed budget, window span vs the
    // cumulative ack. Names whether a collapse rep's emission is re-covering
    // a stalled region / has A* pinned / budget saturated. (Own t0, distinct
    // from the DIAG block's own `diag_start_us` below; carried into the
    // emission step as `SenderPolicy::span_diag_start_us`.)
    let pol = SenderPolicy { span_diag_start_us: now_us(), ..pol };
    /// Congestion-aware NACK repair throttle (ADR-0046).
    let mut nack_congestion = NackCongestionState::new();
    // Sampled HERE, at its original point in setup; moved into SenderState below.
    let last_source_send_us: u64 = now_us();
    /// NACK repairs sent in the current reporting period (ADR-0050 budget tracking).
    let mut nack_repairs_this_period: u64 = 0;
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
    /// goal-gate "The Derived Recovery Clamp": the sender site's one-shot
    /// mechanism-liveness echo (ACTIVE + DIVERGED). Observation only.
    let mut derived_round_echo = DerivedRoundEcho::default();
    // `[RACK]` (paper §16.68.1): the shipped clamp's recovery-fire
    // false-alarm accounting.
    let mut rack_echo = RackClockGauge::new();
    // `[FCAUSE]`'s configuration contract: under generation coding the
    // SACK→gap producer is suppressed (`recv_nack_tx = None`), so both
    // `gap_` classes are structurally empty and only the sender's own
    // tail-sweep timer can fire. The line says which machine it measured.
    rack_echo.set_send_generation(pol.generation);
    // `[QCLK]` — the REALIZED recovery clock as a DISTRIBUTION, at the site
    // that runs it; see the gauge's decl.
    let mut qclk_echo = QuantileClockGauge::new("sender");
    // `[HOLD]` (paper §16.77) — THE HOLD-DOWN ARM, on the path the fire-cause
    // pass measured at 98.99 % of the fires. Constructed on EVERY arm, control
    // included: with `RWM_HOLDDOWN_Q` absent it stamps nothing, allocates
    // nothing, suppresses nothing, and prints `q=unset` — so an absent arm is
    // READ off the run's own output rather than inferred.
    let mut hold_echo = HoldDownGauge::new("sender", pol.holddown_q);
    hold_echo.set_send_generation(pol.generation);



    /// SACK-clocked store release (RWM_STORE_SACK_RELEASE): seqs currently
    /// retained in `sent_store` but UNCOUNTED from the flow-control
    /// outstanding (SACKed by the receiver, cumulative frontier not yet
    /// past them). Invariant: subset of `sent_store` keys — maintained by
    /// marking only retained seqs and pruning with the same cumulative
    /// `split_off` twin. Empty whenever the gate is off.
    let mut sack_released: BTreeSet<u64> = BTreeSet::new();
    /// DIAG: cumulative count of slots released by the law (mechanism
    /// liveness at the gauge — `srel=cur/cum`).
    let mut sack_released_total: u64 = 0;

    let mut packer = framing::SymbolPacker::new(symbol_size, std::time::Duration::from_millis(1));

    // Announce window mode to peer on all paths
    {
        let sched = scheduler.lock();
        for pid in sched.active_paths() {
            let _ = transport.send_control_datagram(
                pid,
                ControlMessage::WindowStart { symbol_size, backend: fec_backend, packed: pol.use_packing },
            );
        }
    }

    // GDIAG / GLIFE stall attribution (net seam pass 3 → net/diag.rs, where
    // the gauge and its documentation now live): the clock stamp stays
    // sampled here, at its original point.
    let gd_last_us = now_us();

    if pol.recov_mp {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE item 1).
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
    if pol.recov_mp_live {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE item 1).
        info!(
            "recovery clocks on LIVE paths ACTIVE (RWM_RECOV_MP_LIVE: hole-law \
             N + per-path clock snapshot ignore the available()>0 saturation \
             filter)"
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
    // Goal-gate "Unlock The Default 2: derived patience" — THE mechanism
    // gauge the falsification clause requires ("patience demonstrably
    // derived"). Every `mp_time_threshold_us` evaluation is classified: did
    // the kGranularity FLOOR win, or the 9/8·srtt CLOCK? Plus the running
    // sum of the floors actually used, for the mean. Printed as
    // `pf=<floor>/<clock>/<mean floor µs>` inside `mpr[…]`. `Cell` because
    // the evaluation happens inside a shared closure.
    let mpd_pf_floor: std::cell::Cell<u64> = std::cell::Cell::new(0);
    let mpd_pf_clock: std::cell::Cell<u64> = std::cell::Cell::new(0);
    let mpd_pf_sum: std::cell::Cell<u64> = std::cell::Cell::new(0);

    let mut emit_batch_live = false;
    if pol.emit_batch_on {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE item 1).
        info!(
            burst = pol.emit_burst,
            "emission batching ACTIVE (RWM_EMIT_BATCH: pacer-quantum TUN \
             intake + per-burst taper/span refresh; flow-control and pacing \
             contracts enforced at symbol granularity)"
        );
    }

    // feat/c8-conversion DIAGNOSIS gauges (goal-gate "C8 Slow-Path
    // Conversion", RWM_DIAG only — behavior-inert). The three cumulative
    // per-path maps moved to `DiagState` (net/diag.rs) with their
    // documentation; `c8c_src_placed`, which the emission step writes, is in
    // `SenderState`. The stall clock below stays a local: it is the
    // ack-advance edge detector, not a reported counter.
    let mut c8c_last_ack_adv_us: u64 = 0;

    // ── The emission seam (net seam pass 2, 2026-08-09) ───────────────────
    // `send_source_symbol!` was a 645-line macro for ONE reason: it mutates
    // ~30 of the locals declared above and no ordinary function could reach
    // them. Those locals are now the fields of `SenderState`; the
    // resolve-once configuration the step reads is `SenderPolicy`; the shared
    // engine handles are `SenderCtx`. The six former expansions are six
    // `emit_source(..)` calls. Body VERBATIM — see net/emit_source.rs.
    // Both wall-clock stamps below were sampled at their ORIGINAL points in
    // this setup (above) and are moved in, not re-sampled.
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
    let sctx = SenderCtx {
        scheduler,
        fec_controller,
        transport,
        stats,
        batch_counter,
        window_ack_seq,
        copa_feed: copa_feed.as_ref(),
    };

    // Retention backpressure state (reliable mode), for edge-triggered logs.
    let mut last_tx_paused = false;

    // Boot cap before the BtlBw anchor warms (a few RTTs); ~1.5× a 100 Mbit/
    // 10 ms BDP, same rationale as the plain-reliable store_boot_cap.
    let mut dyn_infl_cap: u64 = if pol.infl_bdp_on { 128 } else { pol.infl_cap };
    let mut dyn_infl_refresh_us: u64 = 0;
    // ── `[CCAP]` — the composed law's ENGAGEMENT + BIND-FRACTION gauge ────
    // (paper §16.56; CLAUDE.md FORMULA-FIRST, "every clamp gets a
    // bind-fraction gauge, reported"). ADR-0070's postmortem is about
    // measurements that could not see the property under test, so an arm
    // that is bit-identical to control must be readable AS A NULL RESULT and
    // not as a null effect: a law that was CONFIGURED but never ENGAGED (a
    // cold live path at every refresh) and a law that engaged and changed
    // nothing are different findings. These counters separate them, and they
    // report the two surviving bounds — the MEMORY bound `WIN_STORE_MAX`
    // (a resource limit stated outside the law) and the one paroled constant
    // `store_cap_floor` = 64 — so neither can ever bind silently again.
    //
    // The counters live INSIDE `SenderTeardownGauges`, whose destructor is the
    // single emission site for both `[WALL]` and `[CCAP]` — see that type for
    // why the two teardown `select!` arms were the wrong site (the `perf`
    // harness takes neither).
    let mut ccap = SenderTeardownGauges::new(
        // The `[CCAP]` line is emitted for EITHER door into the brake, because
        // its `brake=<closed>/<ticks>` field is the extraction arm's primary
        // readout (§16.60.1) just as it was the composed arm's.
        pol.composed_cap || pol.late_brake,
        pol.store_cap_floor,
    );
    // `[SUMCAP]` (paper §16.62): the `×N` deletion's engagement echo. Fed on
    // BOTH arms at every pooled-law refresh — including the COUNTERFACTUAL, so
    // "did the gate change anything" is answerable from one run — and emitted
    // only on the ON arm, so the shipped default's output is unchanged.
    let mut sumcap = SumCapGauge::new(
        pol.sum_cap,
        pol.store_cap_floor,
        pol.store_path_pool,
        pol.delta_cap,
        pol.delta_b,
    );
    // `[DCAP]` (paper §16.67): the δ-priced VALUE multiplier's engagement echo.
    // Fed on BOTH arms at every pooled-law refresh — including the
    // COUNTERFACTUAL against the shipped `gain`, so "did the derived
    // multiplier change anything" is answerable from one run — and emitted
    // only on the ON arm, so the shipped default's output is unchanged.
    let mut dcap = DeltaCapGauge::new(
        pol.delta_cap,
        pol.store_cap_floor,
        pol.store_path_pool,
        pol.sum_cap,
        pol.delta_b,
    );
    // The periodic DIAG report clock (net seam pass 3 → net/diag.rs): both
    // stamps stay sampled here, at their original points.
    let diag_start_us = now_us();
    let diag_last_us = now_us();
    // diag/lossy-residual emission-gap gauge (net seam pass 3 → net/diag.rs):
    // the stamp stays sampled here, at its original point.
    let sidle_last_change_us = now_us();
    // feat/window-mtu DIAG (goal-gate "Window Decoupling + MTU Scaling",
    // part 1 diagnosis — behavior-inert, RWM_DIAG only): the outstanding
    // split the decoupled law would gate on. `wnd2=<head>/<hole>` — head =
    // last_sent − release_frontier (the live head span: in-flight + queue,
    // everything above the highest SACK/cum-covered seq), hole = unSACKed
    // total − head (recovery-stalled seqs BELOW the frontier — the seats
    // the 1024-latch insures). `relgap=<cur>/mx<max>ms` — time since the
    // release frontier (max of SACK-release max and cum ack) last advanced,
    // max per DIAG window: the release-clumping gauge (D2). Insurance-term
    // decision rule: see the pre-registration.
    let mut wnd2_frontier_last: u64 = 0;
    let mut wnd2_frontier_change_us: u64 = now_us();
    // The DIAG report's counters (net seam pass 3 → net/diag.rs). All four
    // wall-clock stamps above were sampled at their ORIGINAL points in this
    // setup and are moved in, not re-sampled.
    let mut dg = DiagState::new(gd_last_us, diag_start_us, diag_last_us, sidle_last_change_us);
    loop {
        // Scheduler reads in this loop are PER PHASE, each under its own
        // acquisition, taken where the value is used. A loop-top snapshot
        // shared by the phases (`RWM_SCHED_SNAPSHOT`, net seam pass 2) was
        // built and DELETED unmeasured on 2026-08-10 — its premise did not
        // survive reading (ADR-0066 deprecation register; goal-gate
        // "Scheduler-Snapshot Adjudication"). Three findings, if it is ever
        // proposed again:
        //   1. The "BDP that never existed" it claimed to prevent is not
        //      reachable from these sites. Every phase below composes its
        //      scheduler-derived inputs under ONE acquisition already, and
        //      the only rate×RTprop product in play (`copa_bdp_anchor` =
        //      `max_bw × min_rtt`) is atomic inside one `CopaState`. No
        //      derived value spans two acquisitions.
        //   2. It could not deliver the coherence it promised: the phases it
        //      served are independently throttled (~5 ms each, separate
        //      stamps), so they fire on DIFFERENT iterations and would have
        //      consumed DIFFERENT snapshots regardless.
        //   3. It made the one genuinely skew-exposed site WORSE. The
        //      reactive deficit-spacing read sits after the `select!` await;
        //      a loop-top capture serves it a value older by the whole park,
        //      where the per-phase read is fresh.
        // A future attempt at intra-iteration consistency must capture per
        // phase-group AFTER the await, not once at the top.

        // SACK drain: consume the receiver's RECEIVED-above-frontier ranges
        // NON-BLOCKING at the top of every iteration (never as a select! branch
        // — a frequently-ready channel there would race, and cancel, the
        // `tun.read_packet()` future and starve/stall intake). An out-of-order-
        // received symbol is delivery EVIDENCE: the release law uncounts its
        // slot from the flow-control outstanding (the window opens at path
        // rate) while payload + ARQ maps stay retained until the cumulative
        // frontier passes it. The hole itself (NOT in any received range) stays
        // retained and recovers in the background via the orthogonal NACK /
        // tail-sweep path. The loop wakes at least every 1 ms
        // (backpressure/emission poll) so drains stay prompt.
        while let Ok(ranges) = sack_rx.try_recv() {
            for (start, end) in ranges {
                if end < start {
                    continue;
                }
                if pol.store_sack_release_on {
                    // SACK-clocked store release: uncount the slot (window
                    // opens, pool/account freed) — KEEP the payload and
                    // every recovery structure (retransmit_buffer,
                    // nack_retx_at + its per-flight RWM_RECOV_MP loss
                    // clocks, source_path_map) until the cumulative
                    // frontier passes. sack_release_mark skips seqs
                    // already released — no double-release.
                    let newly =
                        sack_release_mark(&st.sent_store, &mut sack_released, start, end);
                    sack_released_total += newly.len() as u64;
                    if pol.percap_track {
                        for &k in &newly {
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

        // RWM_EMIT_BATCH scope check (see the gate decl): batching engages
        // only while exactly ONE path is live; re-checked every iteration so
        // path flaps re-scope within one burst. Gate-off pays nothing.
        if pol.emit_batch_on {
            emit_batch_live = scheduler.lock().live_paths().len() == 1;
        }

        // Determine if packer has pending data for flush timer
        let packer_pending = pol.use_packing && packer.is_pending();

        // RWM Phase A backpressure: when the sent-data store is full of
        // un-acked symbols, stop reading the TUN — the inner flow sees the
        // growing TUN queue and slows down (flow control), and this loop
        // keeps servicing acks/NACKs/tail sweeps so the store drains.
        // Retention is never released by pressure, only by acks.
        // Generation mode keeps no sent_store — its backpressure signal is the
        // encoder's retained source count (= symbols in the in-flight pipeline
        // of M generations). Pausing TUN reads at store_max holds the send
        // frontier ~M generations ahead of the cumulative-decode frontier.
        // RWM_STORE_SACK_RELEASE: outstanding = retained − released. With
        // the gate off the released set is empty and this is exactly the
        // shipped `sent_store.len()`; with it on, SACK-released slots
        // return to the pool (RWM_STORE_PATHS composes through this same
        // count) while their payloads stay retained for recovery.
        let store_len = if generation {
            st.encoder.window_size()
        } else {
            sack_release_outstanding(st.sent_store.len(), sack_released.len())
        };
        // gen_pipe: roll the windowed-MAX rate filter + recompute the derived
        // pipeline depth M* (throttled ~5 ms; the encoder setter is O(1)).
        // feat/anchor-hygiene: under RWM_MSTAR_ANCHOR the bucket span drops
        // 2 s → 500 ms (hygiene rule 1: the anchor seeds from the first
        // measured acks, not after a multi-second pin; the max over 8 buckets
        // keeps a comparable window).
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
                // RTprop (min-RTT), NOT the live SRTT: the live RTT includes
                // the queue this very pipeline creates — deriving depth from
                // it is positive feedback (deeper ⇒ more queue ⇒ deeper). The
                // in-flight cap holds the actual RTT near RTprop, so RTprop is
                // the self-consistent anchor (the BBR discipline).
                let rtprop_s = {
                    let sched = scheduler.lock();
                    sched
                        .active_paths()
                        .iter()
                        .filter_map(|id| {
                            sched.path(*id).map(|p| {
                                p.min_rtt()
                                    .map(|d| d.as_secs_f64())
                                    .unwrap_or_else(|| p.srtt().as_secs_f64())
                            })
                        })
                        .fold(0.0, f64::max)
                };
                let m = gen_pipe_depth(gp_rate_max, rtprop_s, pol.gen_size);
                if m != gen_pipe_m {
                    if pol.diag_on {
                        eprintln!(
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
        // PART 1.2: refresh the BDP-derived in-flight cap (throttled ~5 ms).
        if pol.infl_bdp_on {
            let dnow = now_us();
            if dnow.saturating_sub(dyn_infl_refresh_us) >= 5_000 {
                dyn_infl_refresh_us = dnow;
                let bdp: f64 = {
                    let sched = scheduler.lock();
                    sched
                        .active_paths()
                        .iter()
                        .filter_map(|id| sched.path(*id).and_then(|p| p.copa_bdp_anchor()))
                        .sum()
                };
                if bdp > 0.0 {
                    dyn_infl_cap = ((pol.infl_bdp_gain * bdp).ceil() as u64).max(64);
                }
            }
        }
        let eff_infl_cap = if pol.infl_bdp_on { dyn_infl_cap } else { pol.infl_cap };
        // In-flight (unacked) symbols across the pipe, for the BDP in-flight cap.
        // The #64 fix: also decide fullness PER PATH — the sender is "full"
        // (TUN-paused) only when NO active path is below its own cap
        // (gain·BtlBw_i·RTprop_i), so the fast path keeps pulling source while
        // the slow path is at its RTT-inflated cap. Non-gen_pipe keeps the
        // legacy global Σ in-flight ≥ Σ cap test.
        // ── THE LATE-STAGE BRAKE, and the composed arm's derived cap ──────
        // `RWM_COMPOSED_CAP` (paper §16.56) arms this brake with NO NEW
        // CONSTANT: the per-path cap is the path's OWN cwnd, i.e. the
        // congestion controller's own window, which is what a congestion
        // brake ought to be made of. `eff_infl_cap` is then irrelevant to
        // arming — the composed arm uses neither RWM_INFL_CAP's static total
        // nor RWM_INFL_BDP's gain·BDP, and neither changes meaning.
        //
        // THE SET IS LOAD-BEARING HERE (§16.56, and ADR-0070 finding 1
        // appearing a second time). With cap_i = cwnd_i, "path i is full" is
        // `in_flight_i >= cwnd_i`, which is EXACTLY `available()_i == 0` —
        // and `active_paths()` is *active AND available() > 0*. Iterating it
        // would ask a question whose answer is FALSE BY CONSTRUCTION on
        // every tick, forever: the gate would resolve ON, cost a lock, and
        // never brake. That is a null EFFECT wearing a null RESULT's clothes
        // (§16.53's DIVERGED lesson), and it is why the composed brake reads
        // `live_paths()`. `cwnd_full` under this arm means: EVERY LIVE PATH
        // is at or above its own congestion window.
        //
        // `RWM_LATE_BRAKE` (§16.60.1) arms THE SAME brake without the composed
        // pool law: the extraction exists because `composed_cap` also forces
        // `three_term_on`, so before it there was no way to ask for the brake
        // alone. Everything below reads `cwnd_brake` rather than `composed_cap`
        // so the two gates cannot drift into two brakes — the composed arm is
        // still exactly this brake, reached by a different door.
        let cwnd_brake = pol.composed_cap || pol.late_brake;
        let brake_armed = eff_infl_cap > 0 || cwnd_brake;
        let (pipe_infl, percap_full): (u64, bool) = if brake_armed {
            let mut sched = scheduler.lock();
            let mut infl = 0u64;
            let mut per_path: Vec<(u64, u64)> = Vec::new();
            let ids = if cwnd_brake { sched.live_paths() } else { sched.active_paths() };
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
        // iteration so an arm that never braked reads as a NULL RESULT rather
        // than a null effect. Counted for BOTH doors into the brake — the
        // composed arm and the extracted `RWM_LATE_BRAKE` arm — because the
        // distinction the gauge exists to draw (`brake=0/N` armed-and-never-
        // closed vs `brake=0/0` never-armed) is exactly what an extraction
        // battery needs to read.
        if cwnd_brake {
            ccap.brake_ticks += 1;
            if cwnd_full {
                ccap.brake_closed += 1;
            }
        }
        // Plain-reliable delay-based window cap (paper §12): bound the
        // outstanding store to gain×BDP so the standing queue stays ~1 RTT and
        // loss recovery does not stall behind a bloated queue. Refreshed off
        // the scheduler lock at most every 5 ms.
        refresh_store_cap(
            &mut scs,
            StoreCapCtx {
                pol: &pol,
                gates: &gates,
                scheduler,
                copa_feed: &copa_feed,
                sumcap: &mut sumcap,
                dcap: &mut dcap,
                ccap: &mut ccap,
            },
        );
        let effective_store_cap = if pol.plain_dyn_cap {
            scs.dyn_store_cap
        } else if pol.gen_pipe {
            // gen_pipe remedy 2: intake bounded at the DERIVED M*·G — deep
            // enough to cover BDP + one deficit round, no deeper (queue-lean).
            gen_pipe_store_cap
        } else {
            pol.store_max
        };
        let tx_paused = reliable && (store_len >= effective_store_cap || cwnd_full);

        // feat/window-mtu wnd2/relgap tracking (see decls): the release
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
        // RWM_DIAG periodic constraint report (net seam pass 3 → net/diag.rs;
        // see the decls above the loop). The guard stays HERE so the shipped
        // path still pays nothing and the report still runs at this exact
        // point of the iteration.
        if pol.diag_on {
            diag::report(
                &st,
                &pol,
                &mut dg,
                DiagCtx {
                    scheduler,
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
        // The ACK-CADENCE GAUGE (`RWM_ACKDIAG`, net/ackdiag.rs — matrix row
        // 21's missing instrument). Its own gate and its own ~2 s cadence,
        // deliberately independent of `RWM_DIAG`: the point is to be runnable
        // on an arm that is not paying for the 250 ms report. The guard stays
        // HERE for the same reason the DIAG one does — the shipped path pays
        // nothing.
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
        // Proactive-recovery FRACTION trace (RWM_PFRAC): the share of coded
        // repair emitted PROACTIVELY (upfront, no round-trip) vs REACTIVELY
        // (deficit-driven, one round-trip). Cumulative over the transfer. A high
        // proactive fraction proves Mode B recovers holes from upfront repair.
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
                eprintln!(
                    "[PFRAC] proactive_coded={} recovery_coded={} total_coded={} proactive_fraction={:.4}",
                    proactive_coded_total, recovery_coded_total, tot, frac
                );
            }
        }
        // `[SHEDH]` — the receiver hold's bind gauge (§16.81 ρ leg, in
        // flight), on the 1 s cadence, cumulative, LAST LINE WINS.
        //
        // Emitted from the SENDER loop although it is fed at the RECEIVER:
        // the counters are process-global and both endpoints of a tunnel run
        // both tasks, so this reaches the log without touching the receiver
        // task at all. It is also the only place a periodic emission survives
        // the harness's SIGKILL of the server — a `Drop` never runs there
        // (the `[RFA]` lesson).
        if gates.diag {
            let now = now_us();
            if now.saturating_sub(shedh_last_us) > 1_000_000 {
                shedh_last_us = now;
                eprintln!("{}", shedh_report_line());
            }
        }
        // `[CHI]` — the completion-exposure gauge (§16.82), same
        // 1 s cadence, and on BOTH arms: the control's `n=0 max=0.0000` is the
        // two-sided half of the reachability claim, and an arm whose χ never
        // left 0 must be READ that way rather than inferred.
        if gates.diag {
            let now = now_us();
            if now.saturating_sub(chi_last_us) > 1_000_000 {
                chi_last_us = now;
                eprintln!("{}", chi_report_line());
            }
        }
        // `[REQS]` -- THE REQUEST-SERVING GAUGE (paper 16.83 arms (A)/(B)),
        // same 1 s cadence, cumulative, LAST LINE WINS -- and on BOTH arms,
        // because `[REQS] n=0 served=0` on the control is the two-sided half
        // of the reachability claim: an arm that never reached the wire must
        // be READ that way rather than inferred from a missing line.
        if gates.diag {
            let now = now_us();
            if now.saturating_sub(reqs_last_us) > 1_000_000 {
                reqs_last_us = now;
                eprintln!(
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
        // GDIAG: did ANY coded symbol go on the wire this iteration?
        let gd_flow = emit_generation_coded(GenEmitCtx {
            pol: &pol,
            generation,
            tx_paused,
            cwnd_full,
            gp_rate_max,
            cc_rate_cached,
            scheduler,
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

        // Fix 1: refill the source-pacing token bucket at the measured link
        // rate (delivered-goodput EWMA × headroom, clamped to the same
        // [floor, ceiling] as the coded bucket). The SMALL burst cap (≈ a few
        // ms of link rate, NOT the BDP) is what kills the datagram burst-
        // overrun: at high RTT the flow window is BDP-sized, but emission is now
        // metered to the link so no BDP-sized burst reaches the droppable path.
        if pol.cc_pace {
            let now = now_us();
            // Refresh the Copa cwnd/SRTT rate estimate (frontier-independent) at
            // most every 5 ms.
            if now.saturating_sub(cc_rate_refresh_us) >= 5_000 {
                cc_rate_refresh_us = now;
                // feat/copa-wire-signal: the aggregate CC rate is the SUM of
                // per-path rates Σ cwnd_i/SRTT_i over LIVE paths. The old
                // Σcwnd / max(SRTT) under-reads a heterogeneous aggregate
                // (the fast path's rate divided by the slow path's SRTT),
                // and active_paths()' spare-capacity filter dropped a
                // saturated path from the sum entirely. Also scale the
                // pace CEILING by the live path count: gen_rate (9 000
                // sym/s ≈ 90 Mbit) is a single-link burst guard, and as an
                // aggregate clamp it silently capped C7's two-path intake
                // at one path's worth (MEASURED: C7 = ×1.00 of own single
                // vs C0's ×1.7 aggregation).
                let (rate, n_live) = {
                    let sched = scheduler.lock();
                    let mut r = 0.0f64;
                    let mut n = 0usize;
                    for id in sched.live_paths() {
                        if let Some(p) = sched.path(id) {
                            let s = p.srtt().as_secs_f64();
                            if s > 1e-4 {
                                r += p.cwnd as f64 / s;
                            }
                            n += 1;
                        }
                    }
                    (r, n.max(1))
                };
                cc_rate_cached = rate;
                cc_rate_ceiling = pol.gen_rate * n_live as f64;
            }
            // Pace at the HIGHER of the CC rate and the delivered-goodput EWMA so
            // a stalled in-order frontier (EWMA→0) can't throttle the source ramp.
            let link_est = gen_rate_ewma.max(cc_rate_cached);
            let src_rate = (link_est * pol.cc_pace_headroom).clamp(pol.gen_rate_floor, cc_rate_ceiling);
            let dt = now.saturating_sub(src_tok_last_us);
            src_tok_last_us = now;
            let burst = (src_rate * 0.004).clamp(8.0, 64.0);
            st.src_tokens = (st.src_tokens + src_rate * (dt as f64 / 1_000_000.0)).min(burst);
        }
        // P10b: gap reports must wake this loop even when the TUN is idle.
        // The inner TCP stalls exactly when a hole blocks delivery — no new
        // TUN packets — and the old structure only drained the NACK channel
        // after a TUN read, so repairs stalled precisely when they were the
        // only thing that could unstall the tunnel.
        // `[FCAUSE]`: the gap batch carries the CAUSE that produced it, so a
        // fire can be attributed to the sender's own tail-sweep timer or to
        // one of the receiver's two arms. Label only — the loop below reads
        // it for counters and for nothing else.
        let mut pending_gaps: Option<(FireCause, u32, Vec<(u64, u64)>)> = None;

        // P10b tail sweep: the LAST symbols of a burst have no successors,
        // so the receiver can never SACK a gap behind them — the sender must
        // detect that stall itself (block mode's P8 ARQ sweeper analog).
        // When un-ACKed symbols exist, arm a timer at oldest-activity +
        // 2×SRTT; on expiry synthesize a gap report for the cumulative
        // blocker (per-seq cooldown + budgets all apply downstream).
        let tail_deadline: Option<tokio::time::Instant> =
            st.retransmit_buffer.iter().next().map(|(&seq, &(send_us, _, _))| {
                let last_activity_us = st.nack_retx_at
                    .get(&seq)
                    .map_or(send_us, |&(r, _)| r.max(send_us))
                    .max(last_tail_sweep_us);
                let (srtt_us, jitter_us, sigma_us) = {
                    let sched = scheduler.lock();
                    let paths: Vec<_> = recovery_clock_paths(&sched)
                        .iter()
                        .filter_map(|id| sched.path(*id))
                        .collect();
                    let pooled: Vec<u64> =
                        paths.iter().map(|p| p.estimator.rtt().as_micros() as u64).collect();
                    // The jitter feeding the DERIVED floor is pooled the same
                    // way the clock is (max over the same path set), so floor
                    // and clock can never come from different paths.
                    let jit = paths.iter().map(|p| p.rtt_jitter_us()).max().unwrap_or(0);
                    // The measured σ, MAX over the same set — read only by the
                    // `[QCLK]` readout below.
                    let sg = paths.iter().filter_map(|p| p.rtt_sigma_us()).max();
                    (pooled_recovery_srtt_us(&pooled), jit, sg)
                };
                let timeout_us = sweep_timeout_us(pol.derived_sweep, srtt_us, jitter_us);
                // `timeout_us` is what the engine WILL use — never recomputed
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
                let remaining = Duration::from_micros(deadline_us.saturating_sub(now_us()));
                tokio::time::Instant::now() + remaining
            });

        // WAIT attribution (goal-gate "What Binds Throughput", instrument 2):
        // which `select!` arm woke this iteration. Every arm below writes its
        // bucket index; the charge happens once, after the await, and reads
        // the clock only under RWM_DIAG. The index is written unconditionally
        // — a local `usize` store — so the attribution can never disagree
        // with the branch actually taken. `usize::MAX` means "shutdown", the
        // one arm that returns instead of falling through.
        let mut wait_arm: usize = usize::MAX;
        let packet = tokio::select! {
            // Backpressure poll (reliable): with TUN reads gated off, wake
            // at ack timescale to observe store drain via the ack path
            // below (mirrors the block sender's 1 ms backpressure poll).
            _ = tokio::time::sleep(Duration::from_millis(1)), if tx_paused => { wait_arm = 1; None },
            // Fix 1: pacing wake — when source sends are paced-off (bucket
            // empty), wake at 1 ms to refill it. Without this the select could
            // block in read_packet with the pacing gate closed and stall intake.
            _ = tokio::time::sleep(Duration::from_millis(1)),
                if pol.cc_pace && !tx_paused && st.src_tokens < 1.0 => { wait_arm = 2; None },
            p = tun.read_packet(),
                if !tx_paused && (!pol.cc_pace || st.src_tokens >= 1.0) => { wait_arm = 0; Some(p) },
            // Generation coding: a 1 ms emission poll so the loop keeps waking to
            // run the paced coded-emission block even when no TUN packet is ready
            // (the tail — all sources read but the last generations still need
            // coded symbols to decode) and when not paused. Without it the loop
            // would block in read_packet and the tail would never complete.
            _ = tokio::time::sleep(Duration::from_millis(1)),
                if generation && !tx_paused && st.encoder.window_size() > 0 => { wait_arm = 3; None },
            gaps = nack_rx.recv() => {
                wait_arm = 4;
                if let Some(g) = gaps {
                    pending_gaps = Some(g);
                }
                None
            }
            // NOTE (`[FCAUSE]`): the tail-sweep arm below is the ONLY producer
            // the sender's recovery clock clocks — `tail_deadline` is computed
            // from `sweep_timeout_us`. Every other fire in this loop is clocked
            // by the RECEIVER or by data arrival.
            // Generation-deficit feedback (§16.3): the receiver reports how many
            // MORE coded symbols each frontier generation still needs. Rebuild
            // the per-generation want from the report, subtracting what is
            // already in flight (emitted since the last report), then reset the
            // in-flight baseline. The recovery-emission block above drains these
            // wants, paced. A generation ABSENT from the report has decoded (or
            // is not yet frontier), so its want is cleared by the rebuild.
            dv = deficit_rx.recv(), if generation => {
                wait_arm = 5;
                if let Some(dv) = dv {
                    gen_want.clear();
                    // Pure-proactive demonstrator: drain the channel but never
                    // arm reactive recovery (no round-trips, no exempt-from-cap
                    // emission). Proactive upfront budget is the ONLY recovery.
                    if pol.no_reactive {
                        let _ = dv;
                    } else {
                    // Fix 2: RTT-spacing gate. Reports arrive on EVERY decode
                    // progress (sub-RTT), each resetting the in-flight baseline —
                    // so the in-flight subtraction shrinks to "sent since the last
                    // (recent) report" and the sender re-sends ~the full deficit
                    // every few ms → the measured 60k–252k reactive flood. Gate at
                    // the report: a generation recovered < react_space_us ago is
                    // SKIPPED (its recovery is still in flight, not yet reflected),
                    // so we act on its deficit at most once per ~SRTT. Absent this
                    // window the baseline logic alone cannot bound a sub-RTT report
                    // stream. react_space_us = react_cap_cfg × SRTT (1.0 = 1 SRTT).
                    let react_space_us: u64 = if pol.react_cap_on {
                        // Read HERE, after the `select!` await: this is the one
                        // phase whose value would be stale under a loop-top
                        // snapshot (see the note at the top of the loop).
                        let srtt_us = {
                            let sched = scheduler.lock();
                            sched.active_paths().iter()
                                .filter_map(|id| sched.path(*id).map(|p| p.srtt().as_micros() as u64))
                                .max().unwrap_or(50_000)
                        };
                        ((srtt_us as f64) * pol.react_cap_cfg).max(1_000.0) as u64
                    } else {
                        0
                    };
                    let now_d = now_us();
                    for (anchor, deficit) in dv {
                        // Fix 2: hold off if we recovered this generation recently.
                        if pol.react_cap_on {
                            if let Some(&last) = gen_recover_at.get(&anchor) {
                                if now_d.saturating_sub(last) < react_space_us {
                                    continue;
                                }
                            }
                        }
                        let emitted = gen_emitted.get(&anchor).copied().unwrap_or(0);
                        // In-flight = coded emitted for this generation that the
                        // receiver's CURRENT deficit does not yet reflect (sent
                        // since the last report). On the FIRST report there is no
                        // baseline: the proactive emissions are already reflected
                        // in the reported deficit (the receiver counted them), so
                        // in-flight is 0 — send the full deficit. (Initialising the
                        // baseline to 0 instead would wrongly treat the whole
                        // proactive budget as in flight and send nothing — the
                        // measured first-report deadlock.)
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
            // PAPER 16.83 ARMS (A)/(B) -- THE RECEIVER'S REQUEST, CONSUMED.
            // Sits beside the generation-deficit arm above and is its exact
            // parallel: one report REPLACES the outstanding wants (it is a
            // STATE SNAPSHOT of the receiver's open holes, not a delta), the
            // in-flight baseline is subtracted, and the serving loop drains
            // what is left, paced by the SAME `cached_nack_budget` the gap
            // loop is paced by.
            //
            // DISARMED ⇒ this arm never resolves: `recv_request_tx` is `None`,
            // so nothing is ever sent on the channel, AND the `if` guard is
            // false, so the future is not even polled.
            //
            // NOTE it claims no `wait_arm` bucket: `dg.wait_us` is `[u64; 8]`
            // and its eight arms belong to the shipped `[DIAG]` line. The
            // request arm's own wall-time accounting is `[REQS]`'s business,
            // and inventing a ninth bucket here would re-shape a line this
            // change does not own.
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
                        // THE FIRST-REPORT SPECIAL CASE. See the state block:
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
            _ = async {
                match tail_deadline {
                    Some(d) => tokio::time::sleep_until(d).await,
                    None => std::future::pending().await,
                }
            } => {
                wait_arm = 6;
                last_tail_sweep_us = now_us();
                if let Some((&seq, _)) = st.retransmit_buffer.iter().next() {
                    debug!(seq, "tail ARQ sweep — retransmitting cumulative blocker");
                    dg.diag_sweeps += 1;
                    // A0.2: the tail sweep is the SENDER's own producer — no ack, and
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
                    &packed,
                    &mut st,
                    &pol,
                    &sctx,
                    emit_batch_live,
                );
                    }
                }
                // Send Shutdown on all paths
                let sched = scheduler.lock();
                for pid in sched.active_paths() {
                    let _ = transport.send_control_datagram(pid, ControlMessage::Shutdown);
                }
                // `[WALL]` and `[CCAP]` are NOT emitted here. They are emitted
                // by `ccap`'s destructor (`SenderTeardownGauges`), which is on
                // this path AND on every other way this sender can end —
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
                    &packed,
                    &mut st,
                    &pol,
                    &sctx,
                    emit_batch_live,
                );
                }
                None
            }
        };
        // Charge the elapsed wall time to the arm that woke us. This is the
        // window sender's wait-reason attribution — the instrument whose
        // absence left `sidle` (34.3 % of wall at c2r100-B, 72.7 % at
        // c2r200-B) attributed to nothing across all 419 invocations of the
        // three-term battery. Unlike `gd_us` it has NO `generation` guard:
        // the arms it names exist in window mode, which is what every arm of
        // every battery has actually run.
        if pol.diag_on && wait_arm < 8 {
            let now_w = now_us();
            let dt = now_w.saturating_sub(dg.wait_last_us);
            dg.wait_last_us = now_w;
            dg.wait_us[wait_arm] += dt;
            dg.wait_n += 1;
        }
        // ── THE DEAD-WALL GAUGE (`RWM_WALLDIAG`, net/walldiag.rs) ─────────
        // The ONE feed site of the onset/duration instrument, deliberately
        // placed beside the wait-arm charge above because it consumes the
        // same `wait_arm` — but on its OWN gate, because the statistic it
        // replaces has to be collectable on c8 arms that cannot afford the
        // 250 ms `[DIAG]` report (that is the whole reason the tick-share
        // statistic was only ever available under RWM_DIAG).
        //
        // Three scalars, no engine handle: the arm that woke the loop, the
        // wall clock of the last NEW source symbol (`last_source_send_us`,
        // maintained unconditionally by the emission step), and the engine's
        // monotone retransmit counter. `productive(t)` is evaluated inside
        // the gauge — see `net/walldiag.rs` for the measurand.
        if pol.walldiag_on {
            if let Some(g) = walldiag::gauge() {
                g.observe(now_us(), wait_arm, st.last_source_send_us, dg.diag_retx);
            }
        }

        if let Some(packet) = packet {
            let pkt = match packet {
                Some(p) => p,
                None => {
                    // Flush remaining packed data before exit
                    if pol.use_packing {
                        if let Some(packed) = packer.flush() {
                            emit_source(
                    &packed,
                    &mut st,
                    &pol,
                    &sctx,
                    emit_batch_live,
                );
                        }
                    }
                    // `[WALL]`/`[CCAP]`: see the shutdown arm above — emitted
                    // by `ccap`'s destructor, on every exit path.
                    info!("TUN closed");
                    return;
                }
            };

            if pol.use_packing {
                // Pack multiple small packets into one symbol
                if let Some(packed) = packer.push(&pkt) {
                    emit_source(
                    &packed,
                    &mut st,
                    &pol,
                    &sctx,
                    emit_batch_live,
                );
                }
            } else {
                // Legacy: one packet per symbol (padded)
                let framed = framing::frame_window_packet(&pkt, symbol_size);
                emit_source(
                    &framed,
                    &mut st,
                    &pol,
                    &sctx,
                    emit_batch_live,
                );
                // ── RWM_EMIT_BATCH pacer-quantum burst intake ─────────────
                // Drain already-queued TUN packets without re-arming the
                // select! (per-iteration overhead — tail-deadline scan, SACK
                // drain, pacing refresh — amortizes over the burst, and
                // quinn's endpoint driver sees a multi-datagram queue for
                // deeper GSO transmits). Contracts enforced per symbol: the
                // pooled store backstop from the LIVE local counters (the
                // macro updates sent_store/sack_released), and the cc_pace
                // token bucket. Burst quantum ≤ emit_burst ≈ 64 KB.
                if emit_batch_live {
                    let mut burst = 1usize;
                    while burst < pol.emit_burst {
                        if reliable
                            && sack_release_outstanding(
                                st.sent_store.len(),
                                sack_released.len(),
                            ) >= effective_store_cap
                        {
                            break; // store headroom exhausted (flow control)
                        }
                        if pol.cc_pace && st.src_tokens < 1.0 {
                            break; // pacing bucket dry (Fix 1 contract)
                        }
                        match tun.try_read_packet() {
                            Some(pkt) => {
                                let framed =
                                    framing::frame_window_packet(&pkt, symbol_size);
                                emit_source(
                    &framed,
                    &mut st,
                    &pol,
                    &sctx,
                    emit_batch_live,
                );
                                burst += 1;
                            }
                            None => break, // intake drained (or closed — the
                                           // blocking read owns shutdown)
                        }
                    }
                }
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
            nack_congestion.update(current_loss, current_rtt);
            // ADR-0046 idle-triggered recovery: if no NEW source symbol has
            // been sent for > 2×SRTT, the sender is idle-except-for-recovery —
            // no traffic WE emit is contributing to congestion, so a stalled
            // confirmed hole must not stay suppressed. The idle floor lifts the
            // multiplier just enough for >=1 targeted retransmit per round;
            // while actively pushing data the raw multiplier governs unchanged
            // (congestion safety still wins on a straggler).
            let srtt_us_recent = current_rtt
                .map(|d| d.as_micros() as u64)
                .unwrap_or(IDLE_RECOVERY_GAP_FLOOR_US);
            let idle_gap_us = (2 * srtt_us_recent).max(IDLE_RECOVERY_GAP_FLOOR_US);
            let sender_idle =
                now_us().saturating_sub(st.last_source_send_us) > idle_gap_us;
            let nack_multiplier = nack_congestion.effective_multiplier(sender_idle);
            cached_max_repairs =
                (MAX_NACK_REPAIRS_PER_NACK as f64 * nack_multiplier).round() as u64;
            // NOTE (RWM Phase C -> ADR-0046 hardening): a *blanket*
            // `cached_max_repairs.max(1)` floor here (recovery on EVERY round
            // regardless of load) was tried and REJECTED — forcing a
            // retransmit every round on a genuinely congested lossy path
            // MEASURABLY regressed C8 goodput (14.0 → 9.3 Mbit/s): the forced
            // repairs add load to the straggler. The rare stall it targeted —
            // a datagram-loss burst collapses the multiplier to 0 and wedges a
            // reliable transfer until the QUIC idle timeout — is now handled by
            // the IDLE-TRIGGERED floor above (`effective_multiplier`): it only
            // fires when no new source has been sent for > 2×SRTT, i.e. exactly
            // when there is no straggler load to protect. Active-transfer
            // behavior is unchanged (raw multiplier), so congestion safety
            // still wins on the straggler.

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
                        let nack_cap_symbols = (budget.nack_cap() * st.source_symbols_this_period as f64) as u64;
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

        // ── PAPER 16.83 ARMS (A)/(B): SERVE THE RECEIVER'S REQUESTS ──────
        //
        // Mirrors the DEFICIT-DRIVEN RECOVERY EMISSION above, one vocabulary
        // over: round-robin over the requested spans so none starves, paced
        // by the SAME `cached_nack_budget` / `cached_max_repairs` the gap loop
        // is paced by (so an arm's wire cost is comparable to the control's by
        // construction, not by hope), and bounded by the receiver's own
        // reported deficit minus what is already in flight.
        //
        // TWO ANSWERS, ONE EXPRESSION:
        //   * `m = 1` -- the COPY, served from `sent_store` exactly as the gap
        //     loop serves it, byte for byte. This is the shipped answer, and
        //     16.83.3's own limit of the new message rather than a special
        //     case beside it.
        //   * `m > 1` -- ONE coded equation over `[a, a+m)` from
        //     `generate_repair_range`, which REFUSES unless the whole range is
        //     still retained. The refusal is COUNTED (`WA1`) and falls back to
        //     per-seq copies out of `sent_store`, because an answer that
        //     silently degrades to a copy is the shipped machine with extra
        //     latency and the battery has to be able to read that.
        //
        // DISARMED ⇒ `req_want` is empty forever (nothing ever fills it) and
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
                    // THE ANSWER. `m > 1` asks for an equation over the span;
                    // `m = 1` asks for the seq itself. `None` from either is a
                    // COUNTED refusal, never a silent skip.
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
                            // The COPY -- and the FALLBACK when the span was
                            // no longer retained. `a` is the receiver's own
                            // first missing seq, so it is the copy that closes
                            // the head of the span.
                            match st.sent_store.get(&a) {
                                Some(v) => (Some(v.clone()), Some(a)),
                                // Not in the store ⇒ already acked (removal is
                                // by ack ONLY): the receiver has it and the
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
                    // PLACEMENT: the shipped reliable law, with the paths that
                    // carried the covered source as the fate penalty -- the
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
                        let sched = scheduler.lock();
                        sched
                            .place_symbol(true, &covered)
                            .unwrap_or(st.last_source_path)
                    };
                    let now_r = now_us();
                    let batch_seq = batch_counter.fetch_add(1, Ordering::Relaxed);
                    let batch = SymbolBatch::new(vec![sym], now_r, batch_seq, path);
                    let sent = match transport.send_symbols(path, batch) {
                        Ok(()) => true,
                        Err(e) => {
                            warn!(path, ?e, "failed to send requested repair");
                            false
                        }
                    };
                    // The three meters, at the handoff, exactly as every other
                    // correction channel meters them (the accounting-ledger
                    // finding: the gap loop's two BYPASS channels are the
                    // exception, not the rule, and a NEW channel does not
                    // inherit an exception).
                    {
                        let mut sched = scheduler.lock();
                        if let Some(p) = sched.path_mut(path) {
                            p.charge_in_flight(1);
                            p.consume_pace_tokens(1);
                        }
                    }
                    if let Some(ps) = stats.path(path) {
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
                            let mut sched = scheduler.lock();
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
            scheduler,
            fec_controller,
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
        // Advance encoder window based on receiver ACKs
        let ack = window_ack_seq.load(Ordering::Relaxed);
        on_ack_advance(AckAdvanceCtx {
            pol: &pol,
            gates: &gates,
            ack,
            generation,
            reliable,
            scheduler,
            fec_controller,
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
        // Cap window size — the coding window slides freely under BOTH
        // policies (it is only the FEC horizon). Under RETAIN this eviction
        // destroys no data: the sent-data store still holds the bytes, and
        // an aged hole is recovered by a targeted retransmit from it.
        // Generation mode is EXEMPT: dropping a not-yet-decoded generation's
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
                st.source_path_map.retain(|&seq, _| seq >= win_start);
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

/// Create a window encoder. RLC is the only window backend left (the retired
/// Streaming arm was the only other one, and the only one that read the FEC
/// controller/scheduler — the signature shrank with it, 2026-07-28); the
/// degenerate one-arm `match backend` it left behind is gone, but `backend`
/// stays in the signature as the selection point a future window codec
/// re-enters at.
fn create_window_encoder(
    _backend: FecBackend,
    symbol_size: u16,
) -> Box<dyn WindowEncoder> {
    Box::new(RlcWindowEncoder::new(symbol_size))
}

/// Create a window decoder for the given backend.
///
/// In GENERATION mode the RLC backend uses the dense per-generation
/// `GenerationDecoder` (Gauss–Jordan over GF(256) with SIMD row ops) rather than
/// the sparse sliding-window `RlcWindowDecoder`: the sparse decoder's
/// BTreeMap-of-coefficients + cascade sat ~200× below the link rate at the
/// oracle's aggregating G, making decode — not the network — the binding
/// constraint (goal-gate "Generation Coding"). The wire format is identical, so
/// this is a pure receiver-side swap.
fn create_window_decoder(
    backend: FecBackend,
    symbol_size: u16,
    generation: bool,
) -> Box<dyn WindowDecoder> {
    match backend {
        // Task #61 (paper §16.20): under RWM_UNIFIED the whole RLC family —
        // sliding-window AND generation wires — decodes on ONE machine, the
        // global sparse-aware closure. Differential-proven equal to both
        // legacy decoders on their own wires (fec::unified / fec::generation
        // differential tests); the legacy machines stay compilable below as
        // the A/B arms until the queued L1 parity battery flips the default.
        _ if unified_active() => {
            info!(generation, "RWM_UNIFIED: receive path on the unified global decoder (one machine, both wires)");
            Box::new(crate::fec::UnifiedDecoder::new(symbol_size))
        }
        _ if generation => Box::new(crate::fec::GenerationDecoder::new(symbol_size)),
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
    // Repair symbols may carry in-band metadata that must be subtracted from
    // the available MTU (RLC: a repair-index header).
    let fec_wire_overhead = fec_backend.repair_wire_overhead();
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
                let batch = SymbolBatch::new(std::mem::take(&mut chunk), now, batch_seq, path_id);
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
            let batch = SymbolBatch::new(chunk, now, batch_seq, path_id);
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
pub(crate) struct PathBatchTracker {
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
                let batch = SymbolBatch::new(std::mem::take(chunk), now, batch_seq, path_id);
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
