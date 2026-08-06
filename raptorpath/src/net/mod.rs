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

// ── Multipath recovery suppression (branch feat/recovery-suppression, env
//    `RWM_RECOV_MP`, default OFF ⇒ shipped path byte-identical) ─────────────
//
// The fifth control-plane wall (goal-gate "Engine Parallelization" STEP 1d):
// under dual-path striping the recovery plane roughly DOUBLES its per-source
// retransmit share and ×2.2–2.5s its repair share vs the same config run
// single-path; at dual-c1 (GE 0.1%, nothing real to recover) the sender
// retransmits 9.3% of source (single-path: 0.2%, ×46) and the dual sink
// aggregates BELOW one path alone. The root defects are two instances of ONE
// mistake: recovery clocks/serials are GLOBAL where multipath demands they be
// PER-PATH.
//
// (1) The hole law. A SACK gap is evidence of loss on a SINGLE path (FIFO
//     within the path); across paths a seq gap is NORMAL — the scheduler
//     CREATED it (striping + inter-path delay skew). The legacy age gate
//     (age ≥ max-path-SRTT/2 since the ORIGINAL send) fires while the
//     symbol's own flight is still in the air, and after a retransmit the
//     clock is never reset to the NEW flight, so an open (scheduler-created)
//     gap re-fires every cooldown while copies are still flying — the
//     feedback flood. The law here is RFC 9002 §6.1.2 time-threshold loss
//     detection generalized per path (the packet-threshold channel is
//     deliberately NOT used across paths — cross-path seq gaps are exactly
//     the RFC 4737 reordering caveat; multipath QUIC solves the same problem
//     with per-path packet-number spaces): a reported gap seq is a candidate
//     hole only once its LIVE flight (the last (re)send) is older than
//     kTimeThreshold = 9/8 of its OWN path's smoothed RTT, with the existing
//     per-seq cooldown floor as the kGranularity analog. Suppression-only:
//     the receiver's hole-refresh keeps re-advertising, so a real hole fires
//     the moment its flight clock expires. N = 1 live path keeps the legacy
//     gates bit-exactly (single-path gaps are FIFO-real; sc2/sc3 inert).
//
// (2) The loss serials, DIAGNOSED and left global. `batch_seq` is a GLOBAL
//     counter, but the receiver's per-path `PathBatchTracker` estimates
//     expected symbols from batch_seq GAPS — so under striping every
//     path-switch reads the other path's run as loss and the per-path loss
//     estimators saturate. The per-path serial-namespace fix
//     (`RWM_RECOV_MP_SERIAL`) was diagnostically TRUE but runtime-REFUTED on
//     the post-wall substrate (honest signal re-heats every SRTT/loss-scaled
//     recovery cadence the poisoned values were accidentally damping; sender
//     CPU ×2.4, dual-c1 181→134 — goal-gate "Multipath Recovery Suppression"
//     2026-07-21) and REMOVED 2026-07-27 per the DEPRECATION REGISTER (no
//     re-test owed: refuted ON the clean substrate). A cheaper serial-
//     namespace implementation is a NEW pre-registered build, not a revival.
//
// Sub-gate for trace attribution: `RWM_RECOV_MP_LAW` (default ON under the
// umbrella) gates (1).

/// RFC 9002 §6.1.2 time threshold for the flight path: kTimeThreshold (9/8)
/// × max of the two smoothed RTT clocks available for the path (Copa EWMA
/// srtt and the estimator's EWMA app-echo RTT — the analog of
/// `max(smoothed_rtt, latest_rtt)`), floored at the existing per-seq
/// retransmit cooldown floor (the kGranularity analog). No new constants.
pub(crate) fn mp_time_threshold_us(srtt_us: u64, ewma_rtt_us: u64) -> u64 {
    let s = srtt_us.max(ewma_rtt_us);
    (s.saturating_mul(9) / 8).max(NACK_RETX_COOLDOWN_FLOOR_US)
}

/// RFC 9002 §6.1.1 packet threshold (kPacketThreshold = 3), generalized per
/// path: the FAST honest loss channel. A seq's original flight on path j is
/// declared lost as soon as ≥3 LATER path-j symbols are known delivered —
/// same-path FIFO evidence (UDP within one 5-tuple does not reorder under
/// netem/typical paths; 3 absorbs rare in-path reordering per the RFC).
/// Scheduler-created cross-path gaps can never trigger it: their same-path
/// successors are exactly as un-arrived as they are. This restores legacy
/// real-loss recovery latency (≈ one skew, not a full RTT) under the
/// time-threshold suppression. Applies to ORIGINAL flights only — a
/// retransmit's wire order is not its seq order, so retransmits are
/// governed by the time threshold alone.
pub(crate) const MP_PACKET_THRESHOLD: usize = 3;

/// The delivered intervals a gap report implies: between consecutive maximal
/// missing runs everything was SACKed, and the seq just past the last gap is
/// the SACK range that bounded it (its extent is unknown — one seq is the
/// provable minimum). Pure; unit-tested.
pub(crate) fn mp_delivered_intervals(gaps: &[(u64, u64)]) -> Vec<(u64, u64)> {
    let mut out = Vec::with_capacity(gaps.len());
    for i in 0..gaps.len() {
        let lo = gaps[i].1 + 1;
        let hi = if i + 1 < gaps.len() {
            gaps[i + 1].0.saturating_sub(1)
        } else {
            gaps[i].1 + 1
        };
        if lo <= hi {
            out.push((lo, hi));
        }
    }
    out
}

/// Fast-loss decision from per-path delivered evidence (sorted seq list):
/// ≥ MP_PACKET_THRESHOLD delivered path-j seqs strictly above `s`.
pub(crate) fn mp_fast_lost(delivered_on_path: &[u64], s: u64) -> bool {
    let above = delivered_on_path.len() - delivered_on_path.partition_point(|&x| x <= s);
    above >= MP_PACKET_THRESHOLD
}

/// The skew-aware hole law (pure, unit-tested): may a reported gap seq be
/// treated as a hole (targeted retransmit eligible) right now?
///
/// * `n_live_paths <= 1`: the law is INERT — single-path gaps are FIFO-real
///   and the legacy gates own the decision (bit-exact shipped behavior).
/// * Unknown flight (no send record): legacy behavior (never suppress a seq
///   we cannot clock — the reliability backstop stays intact).
/// * Otherwise: a hole only once the LIVE flight is at least
///   `threshold_us` old — a gap on path A while the seq's flight is still
///   inside path B's expected-arrival clock is a gap the scheduler created,
///   not a hole.
pub(crate) fn mp_hole_ripe(
    n_live_paths: usize,
    now_us: u64,
    flight_send_us: Option<u64>,
    threshold_us: u64,
) -> bool {
    if n_live_paths <= 1 {
        return true;
    }
    match flight_send_us {
        None => true,
        Some(t) => now_us.saturating_sub(t) >= threshold_us,
    }
}
// ── δ-honest overload shedding (goal-gate "Unified Shedding", fix C;
//    part of the unified machine's realtime semantics under `RWM_UNIFIED`,
//    sub-gate `RWM_UNIFIED_SHED=0` = the serializing control arm) ─────────
//
// The (δ, ρ) semantics, operationally (paper §16.20.8 / §16.26): at small δ
// overload must be SHED, not serialized. A symbol is sheddable iff BOTH
// (1) its projected delivery exceeds the deadline D(δ) — a retransmit fired
// at age > D arrives after the receiver's own δ-horizon give-up; a hole
// held past D only serializes successors past THEIR deadlines — and
// (2) its loss stays within the 1−ρ budget (`residual_loss_after_fec`,
// the ε̂·(1−P_fec) allowance the (δ,ρ,r) design already concedes). Beyond
// the budget the machine SERIALIZES (ρ wins over δ). The reliable-transfer
// contract (RETAIN-UNTIL-ACKED, ρ=1) is excluded BY CONSTRUCTION: the law
// is armed only on the EVICT path (`!reliable`).

/// Is the shed law armed at all? Realtime-EVICT under the unified machine
/// only — NEVER the reliable (ρ = 1) contract, never the legacy machines.
pub(crate) fn shed_armed(unified_on: bool, reliable: bool, gate: bool) -> bool {
    unified_on && !reliable && gate
}

/// The δ deadline D in µs: min(b(hint)·RTprop, 2·RTprop) — the span law's
/// own D (§16.20.3), measured from the symbol's original send. b(Realtime)
/// = ½, so on the realtime path D = RTprop/2: a retransmit older than that
/// lands after the receiver's δ-horizon give-up (send + owd + D) — waste.
pub(crate) fn shed_deadline_us(b_hint: f64, rtprop_us: u64) -> u64 {
    ((b_hint.min(2.0) * rtprop_us as f64) as u64).min(2 * rtprop_us)
}

/// The per-decision shed admission: past-deadline AND within the ρ budget.
/// `budget_frac` = the derived 1−ρ (`residual_loss_after_fec`); the
/// cumulative shed count may never exceed budget_frac × the stream's
/// source count. Cold start (budget 0, no ε̂/r sample) sheds nothing.
pub(crate) fn shed_allowed(
    age_us: u64,
    deadline_us: u64,
    shed_total: u64,
    src_total: u64,
    budget_frac: f64,
) -> bool {
    deadline_us > 0 // no derived deadline yet ⇒ nothing is sheddable
        && age_us > deadline_us
        && ((shed_total + 1) as f64) <= budget_frac * (src_total as f64)
}

/// Receiver-side in-order hold for the window EVICT path. Legacy: 4×SRTT
/// clamped [60, 300] ms (two ARQ repair rounds — the bulk-shaped hold).
/// Under the shed law (unified realtime, budget open): the δ-derived
/// H = b·SRTT with b(Realtime) = ½ — §16.20.3's "the reorder_timeout IS
/// the δ dial" made honest (the EVICT in-order window path exists only for
/// the Realtime hint, so b = ½ structurally). When the receiver's give-up
/// budget (holes ≤ ε̂_recv × frontier — the loss-class bound) is spent, the
/// hold reverts to legacy: serialize, don't shed below ρ.
pub(crate) fn shed_recv_hold(srtt: Duration, shed_on: bool, budget_ok: bool) -> Duration {
    if shed_on && budget_ok {
        srtt / 2
    } else {
        (srtt * 4).clamp(BLOCK_REORDER_MIN_HOLD, BLOCK_REORDER_MAX_HOLD)
    }
}

/// Receiver give-up budget: holes given up so far vs ε̂_recv × frontier.
/// (The receiver owns no r/A*, so its bound is the loss CLASS, not the
/// FEC residual; give-up is intrinsically holes-only, which keeps the
/// realized fraction in the residual class anyway.)
pub(crate) fn shed_recv_budget_ok(holes_given_up: u64, frontier_seqs: u64, eps_recv: f64) -> bool {
    (holes_given_up as f64) < eps_recv * (frontier_seqs as f64)
}

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
/// backends (RLC, METTLE) use the sliding-window pipeline; block-only backends
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
const RELIABLE_STORE_MAX: usize = 1024;

/// Reliable-policy backpressure: when the sent-data store is full of
/// un-acked symbols, stop reading the TUN (the same contract as the block
/// path's cwnd gate) instead of dropping retention. EVICT mode never
/// backpressures on retention (Realtime's correct spend of the budget).
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
fn store_backpressure(reliable: bool, store_len: usize) -> bool {
    reliable && store_len >= RELIABLE_STORE_MAX
}

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
/// slow path's inflated term over-drive its own queue into bufferbloat). The
/// retention-store mirror is [`percap_store_full`]. Extracted pure for unit
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
/// feat/copa-sole-cc: symbols→bytes conversion for the pass-through substrate
/// window (`RWM_QUIC_CC=passthrough`). Copa-lite's cwnd is in SYMBOLS; quinn's
/// congestion window is in BYTES of packet payload. Plain window mode puts one
/// ~1200-byte symbol per datagram plus wire framing (~30–50 B), so 1250 B per
/// symbol converts the window with a few-percent tolerance — Copa's delay
/// signal absorbs the residual (a slightly generous window shows up as queue
/// and is backed off; a slightly tight one only shaves the probe overshoot).
const COPA_SOLE_BYTES_PER_SYMBOL: u64 = 1250;

/// feat/copa-sole-cc: plain-mode Copa delivery-feed state (see the creation
/// site in `run_impl` for the full design note). Sender-side only: seq→path
/// recorded at send, newly-delivered seqs derived from each WindowAck's
/// cumulative frontier + SACK ranges, attributed per path into the
/// BBR-correct send-interval rate sampler + the Copa cwnd dynamics.
pub(crate) struct CopaFeed {
    /// seq → its send commitments: the LAST (re)send's path + timestamp,
    /// plus the previous DISTINCT-path commitment when the seq was
    /// retransmitted cross-path (the flight-witness input, residual (iii)).
    /// Written at source send and at targeted retransmit (a retransmit
    /// re-snapshots the rate sample, so the eventual ack yields a truthful
    /// send-interval). Removed on attribution; entries for seqs the
    /// frontier passed are gone by then.
    seq_path: DashMap<u64, SendCommit>,
    /// Attribution cursor: the next in-order seq not yet attributed plus the
    /// set of above-frontier seqs already attributed via SACK (so a seq is
    /// attributed exactly once). Bounded by the sender's outstanding store.
    cursor: parking_lot::Mutex<CopaFeedCursor>,
    /// feat/anchor-hygiene (`RWM_PLAIN_RS`): SAMPLING-ONLY mode — the #79
    /// send-interval rate sampler generalized to plain window-reliable mode
    /// under ANY substrate CC. The WindowAck frontier/SACK attribution and
    /// the per-seq BBR rate samples run (so the per-path BtlBw/BDP anchor is
    /// fed CLEAN send-interval Δt instead of the ack-interval over-read that
    /// knee-clamps the percap/store caps — goal-gate "Per-Path Outstanding
    /// Accounting" GUARD RESULTS residual (i)), but Copa does NOT own the
    /// substrate window: no pass-through window writes, and the cwnd
    /// dynamics keep their legacy per-batch-Ack call site/cadence.
    sampling_only: bool,
    /// Residual (iii) fix live: apply the flight-time witness
    /// ([`resolve_flight_path`]) at attribution. Follows `RWM_PLAIN_RS`
    /// (sampling-only feed; `RWM_RS_ATTR=0` = the same-binary legacy
    /// last-sent-path control). The full Copa-sole feed keeps legacy
    /// attribution (its arms are study baselines).
    attr_witness: bool,
    /// DIAG: attributed seqs whose commit history crossed paths.
    attr_cross: AtomicU64,
    /// DIAG: of those, attributions the witness credited to the PREVIOUS
    /// commitment (the spurious-retransmit class — the last flight was
    /// younger than its path's RTprop at ack time).
    attr_witness_prev: AtomicU64,
    /// feat/window-mtu (`RWM_WIN_DECOUPLE`): the N1-scoped sampler pause.
    /// The RS sampling composition carries a measured −22…−27 Mbit cost at
    /// the symmetric dual cell ("C8-Aware Pool Law" ATTRIBUTION), so a
    /// sampling-only feed constructed for the N = 1 window law must go
    /// fully inert while ≥ 2 paths are live: `on_sent` records nothing and
    /// attribution only fast-forwards the cursor. Set by the sender loop at
    /// the dyn-cap refresh cadence. Always false for `RWM_PLAIN_RS` and the
    /// full Copa-sole feed (their semantics are unchanged).
    n1_pause: std::sync::atomic::AtomicBool,
}

#[derive(Default)]
struct CopaFeedCursor {
    next: u64,
    sacked: std::collections::BTreeSet<u64>,
}

/// One seq's send-commitment history for delivery attribution (residual
/// (iii), branch `feat/store-borrowing`): the LAST (re)send plus the
/// previous DISTINCT-path commitment, so the attribution site can apply
/// the flight-time witness ([`resolve_flight_path`]) instead of blindly
/// crediting the last-sent path.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SendCommit {
    /// Path + send time (µs) of the most recent (re)send.
    last: (u32, u64),
    /// Path + send time of the previous distinct-path commitment, when the
    /// seq was retransmitted CROSS-path (None = single-path history).
    prev: Option<(u32, u64)>,
}

/// The flight-time witness (residual (iii) fix): which path's flight
/// actually delivered an attributed seq.
///
/// The defect it closes: a seq lost (or presumed lost) on path A and
/// retransmitted on path B is attributed to B when its ack arrives — but
/// if the ack arrives SOONER after the retransmit than B's propagation
/// floor, the retransmitted copy cannot have completed the round trip; the
/// delivering flight was the ORIGINAL copy on A (a spurious retransmit —
/// the gap was ack latency, not loss). Blindly crediting B advances B's
/// per-path delivered counter for a symbol that flew on A, and at an
/// asymmetric cell the fast→slow retransmit stream inflates the SLOW
/// path's Δdelivered — the measured ×3–5 slow-path BtlBw over-read under
/// multipath placement (goal-gate HONEST-CAP RESULTS sub-residual (iii)).
///
/// The witness is a pure floor-clock test, no new constants: credit the
/// LAST commitment only if its flight is at least RTprop(last.path) old at
/// ack time; otherwise credit the previous commitment (whose flight is
/// older by construction). An unknown RTprop (warm-up) counts as
/// qualified — legacy attribution, no behavior cliff.
pub(crate) fn resolve_flight_path(
    commit: &SendCommit,
    now_us: u64,
    mut rtprop_us_of: impl FnMut(u32) -> Option<u64>,
) -> u32 {
    match commit.prev {
        None => commit.last.0,
        Some((prev_path, _)) => {
            let age = now_us.saturating_sub(commit.last.1);
            let qualified = rtprop_us_of(commit.last.0).map_or(true, |rtp| age >= rtp);
            if qualified {
                commit.last.0
            } else {
                prev_path
            }
        }
    }
}

impl CopaFeed {
    fn new() -> Self {
        Self {
            seq_path: DashMap::new(),
            cursor: parking_lot::Mutex::new(CopaFeedCursor::default()),
            sampling_only: false,
            attr_witness: false,
            attr_cross: AtomicU64::new(0),
            attr_witness_prev: AtomicU64::new(0),
            n1_pause: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// feat/window-mtu: pause/resume the N1-scoped sampler (see `n1_pause`).
    fn set_n1_paused(&self, paused: bool) {
        self.n1_pause.store(paused, Ordering::Relaxed);
    }
    fn n1_paused(&self) -> bool {
        self.n1_pause.load(Ordering::Relaxed)
    }

    /// feat/anchor-hygiene (`RWM_PLAIN_RS`): sampling-only construction.
    /// The flight-time witness (residual (iii)) defaults ON here —
    /// `RWM_RS_ATTR=0` restores legacy last-sent-path attribution as the
    /// same-binary control arm.
    fn new_sampling_only(attr_witness: bool) -> Self {
        Self {
            sampling_only: true,
            attr_witness,
            ..Self::new()
        }
    }

    /// True when this feed also OWNS the CC operating point (the Copa-sole
    /// pass-through mode). Sampling-only mode leaves cwnd dynamics, store-cap
    /// law, and percap pipe derivation on their legacy branches.
    fn owns_cc(&self) -> bool {
        !self.sampling_only
    }

    /// DIAG (residual (iii)): (cross-path-history attributions, of which
    /// witness-credited-to-previous-flight). Read only at the DIAG print.
    pub(crate) fn attr_diag(&self) -> (u64, u64) {
        (
            self.attr_cross.load(Ordering::Relaxed),
            self.attr_witness_prev.load(Ordering::Relaxed),
        )
    }

    /// Record a (re)send of source seq `seq` on `path`. A cross-path
    /// retransmit keeps the previous commitment as the flight-witness
    /// fallback (residual (iii)); a same-path resend just refreshes the
    /// send time (its rate sample is re-snapshotted by `on_src_sent`).
    fn on_sent(&self, seq: u64, path: u32) {
        // N1-scoped sampler pause: record nothing while ≥ 2 paths are live.
        if self.n1_paused() {
            return;
        }
        let now = now_us();
        match self.seq_path.entry(seq) {
            dashmap::mapref::entry::Entry::Occupied(mut e) => {
                let cur = *e.get();
                *e.get_mut() = SendCommit {
                    last: (path, now),
                    prev: if cur.last.0 != path {
                        Some(cur.last)
                    } else {
                        cur.prev
                    },
                };
            }
            dashmap::mapref::entry::Entry::Vacant(v) => {
                v.insert(SendCommit {
                    last: (path, now),
                    prev: None,
                });
            }
        }
    }

    /// Diff one WindowAck against the cursor: returns the seqs this ack
    /// NEWLY proves delivered (frontier advance up to `received_up_to`,
    /// inclusive, plus never-before-seen SACKed seqs above it), each exactly
    /// once across the whole ack stream. Out-of-order/duplicate acks yield
    /// an empty diff — never a double attribution.
    fn newly_delivered(&self, received_up_to: u64, sack_ranges: &[(u64, u64)]) -> Vec<u64> {
        // Per-ack safety bound: a corrupt/hostile ack must not trap us in a
        // multi-million-seq loop. Honest ranges are bounded by the sender's
        // outstanding store (≤ a few thousand).
        const MAX_PER_ACK: usize = 65_536;
        let mut newly = Vec::new();
        let mut c = self.cursor.lock();
        while c.next <= received_up_to && newly.len() < MAX_PER_ACK {
            let s = c.next;
            c.next += 1;
            // Already attributed via an earlier SACK → consume the marker.
            if !c.sacked.remove(&s) {
                newly.push(s);
            }
        }
        for &(a, b) in sack_ranges {
            let lo = a.max(c.next);
            let hi = b.min(lo.saturating_add(MAX_PER_ACK as u64));
            for q in lo..=hi {
                if newly.len() >= MAX_PER_ACK {
                    break;
                }
                if c.sacked.insert(q) {
                    newly.push(q);
                }
            }
        }
        newly
    }
}

/// feat/copa-sole-cc: attribute one WindowAck's newly-delivered seqs to their
/// paths and run the per-path Copa machinery on them: send-interval rate
/// sample per seq (`on_src_delivered_seq` — feeds the windowed-max BtlBw with
/// clean Δt), in-flight release, the per-SRTT cwnd update/backoff
/// (`on_delivery_signal`), and finally the pass-through substrate window
/// write (no-op unless RWM_QUIC_CC=passthrough). Call AFTER recording the
/// ack's RTT sample so the update sees the freshest queue signal.
fn copa_feed_attribute(
    feed: &CopaFeed,
    ack_path: u32,
    received_up_to: u64,
    sack_ranges: &[(u64, u64)],
    scheduler: &Arc<parking_lot::Mutex<Scheduler>>,
    transport: &Arc<QuicTransport>,
    stats: &Arc<SharedStats>,
) {
    // feat/window-mtu: paused N1-scoped sampler — no samples, no cwnd work;
    // only fast-forward the attribution cursor so a later resume (paths
    // dropping back to 1) starts clean at the live frontier. Send records
    // from before the pause are dropped un-attributed (bounded by the
    // outstanding store at pause time; the battery topologies never flap).
    if feed.n1_paused() {
        let mut c = feed.cursor.lock();
        if received_up_to >= c.next {
            c.next = received_up_to + 1;
        }
        c.sacked.retain(|&s| s > received_up_to);
        drop(c);
        feed.seq_path.retain(|&s, _| s > received_up_to);
        return;
    }
    let newly = feed.newly_delivered(received_up_to, sack_ranges);
    if newly.is_empty() {
        return;
    }
    let mut per_path: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let now = now_us();
    let mut sched = scheduler.lock();
    for seq in newly {
        // Attribute to the path whose FLIGHT delivered the seq. Default:
        // the path it was last sent on; a seq without a send record
        // (pre-feed traffic, evicted record) falls back to the path the
        // ack arrived on — plain in-order acks ride the arrival path.
        // Residual (iii): when the commit history crossed paths, the
        // flight-time witness decides — an ack arriving sooner after a
        // cross-path retransmit than that path's RTprop proves the
        // delivering copy was the ORIGINAL flight, so the retransmit path's
        // delivered counter must NOT advance (the ×3–5 slow-path BtlBw
        // over-read under multipath placement; `resolve_flight_path`).
        let p = match feed.seq_path.remove(&seq) {
            Some((_, commit)) => {
                if commit.prev.is_some() {
                    feed.attr_cross.fetch_add(1, Ordering::Relaxed);
                    let witness = resolve_flight_path(&commit, now, |pid| {
                        sched
                            .path(pid)
                            .and_then(|ps| ps.min_rtt())
                            .map(|d| d.as_micros() as u64)
                    });
                    if witness != commit.last.0 {
                        feed.attr_witness_prev.fetch_add(1, Ordering::Relaxed);
                    }
                    if feed.attr_witness {
                        witness
                    } else {
                        commit.last.0
                    }
                } else {
                    commit.last.0
                }
            }
            None => ack_path,
        };
        if let Some(ps) = sched.path_mut(p) {
            ps.on_src_delivered_seq(seq);
        }
        *per_path.entry(p).or_insert(0) += 1;
    }
    // feat/anchor-hygiene (`RWM_PLAIN_RS`): sampling-only mode stops here —
    // the rate samples above are the whole job. The cwnd dynamics keep their
    // legacy per-batch-Ack call site, and the substrate window is whatever
    // RWM_QUIC_CC says (this feed does not own the operating point).
    if !feed.owns_cc() {
        return;
    }
    for (p, _n) in per_path {
        if let Some(ps) = sched.path_mut(p) {
            // feat/copa-compete: feed the wire-level loss evidence (the
            // pass-through shim's recorded congestion-event counter) into the
            // competitive AIMD before the update consumes it. No-op unless
            // RWM_COPA_COMPETE is active.
            if crate::scheduler::copa_compete_active() {
                if let Some((ev, _, _)) = transport.cc_passthrough_stats(p) {
                    ps.on_wire_congestion_events(ev);
                }
            }
            // NOT release_in_flight here: the per-batch Ack arm keeps doing
            // the wire-level in-flight release (it covers repairs too);
            // releasing again per attributed source seq would double-count.
            ps.on_delivery_signal();
            transport.set_cc_window_bytes(p, ps.cwnd as u64 * COPA_SOLE_BYTES_PER_SYMBOL);
            if let Some(st) = stats.path(p) {
                st.cwnd.store(ps.cwnd as u64, Ordering::Relaxed);
                st.in_flight.store(ps.in_flight as u64, Ordering::Relaxed);
            }
        }
    }
}

/// Map FecBackend to u8 for atomic stats storage.
fn backend_to_u8(backend: FecBackend) -> u8 {
    match backend {
        FecBackend::RaptorQ => 0,
        FecBackend::Mettle => 1,
        FecBackend::ReedSolomon => 2,
        FecBackend::Rlc => 3,
        // 4 was Streaming (retired 2026-07-28) — kept reserved.
    }
}

fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
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

    // Generation-deficit channel (§16.3): the data-arm's control handler parses
    // inbound GenerationDeficit messages and forwards the (anchor, deficit)
    // vector to the local window sender, which emits exactly the residual coded
    // symbols each frontier generation still needs (bounded, targeted recovery).
    let (deficit_tx, deficit_rx) =
        tokio::sync::mpsc::channel::<Vec<(u64, u32)>>(64);

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
    let (msg_tx, mut msg_rx) = mpsc::channel::<(u32, WireMessage)>(4096);
    // Dedicated channel for stream-origin control: liveness must not queue
    // behind the data flood (see spawn_receiver_for_path).
    let (ctrl_tx, mut ctrl_rx) = mpsc::channel::<(u32, WireMessage)>(256);
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
        if !wanted && (plain_rs || gates.win_decouple) && plain_inorder {
            let feed = CopaFeed::new_sampling_only(gates.rs_attr);
            if plain_rs {
                info!(
                    "plain-mode send-interval SAMPLER ACTIVE (RWM_PLAIN_RS sampling-only: \
                     WindowAck frontier/SACK -> per-path send-interval rate samples; \
                     CC ownership unchanged; flight-witness attribution={} \
                     [residual (iii): cross-path retransmit acks younger than the \
                     retransmit path's RTprop credit the ORIGINAL flight; \
                     RWM_RS_ATTR=0 = legacy last-sent control])",
                    feed.attr_witness
                );
            } else {
                // feat/window-mtu: the N1-scoped anchor for the decoupled
                // window law — same sampling-only machinery, dynamically
                // PAUSED while >= 2 paths are live (the measured RS dual-cell
                // composition cost stays structurally unreachable). Starts
                // PAUSED: the first dyn-cap refresh (~5 ms) unpauses at
                // N = 1, so a dual bring-up never charges a single symbol.
                feed.set_n1_paused(true);
                info!(
                    "win-decouple N1 sampler ACTIVE (RWM_WIN_DECOUPLE: sampling-only \
                     send-interval anchor at N=1 only; inert while >=2 paths live)"
                );
            }
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
    let sender_protocol_hint = config.protocol_hint;
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
                &mut sender_shutdown_rx,
                sender_protocol_hint,
                sender_window_reliable,
                sender_window_coded_only,
                sender_window_generation,
                sender_window_systematic,
                sender_copa_feed,
                sender_gates,
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
    let recv_nack_tx: Option<tokio::sync::mpsc::Sender<Vec<(u64, u64)>>> =
        if window_mode && !window_generation {
            Some(nack_tx)
        } else {
            None
        };
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

    // Engine-receiver saturation probe (roadmap item 2, feat/engine-parallel
    // STEP 1). RWM_RDIAG=1 samples (a) the engine task's busy fraction
    // (1 − time-awaiting-select / wall) and (b) the inbound msg-channel depth
    // (queued behind the single engine task). Distinguishes "the engine task
    // is the service-rate wall" (busy→100%, q deep) from "the wall is
    // upstream" (busy low, q empty). Probe only — no behavior change; the
    // WeakSender adds no channel-close semantics.
    let rdiag_probe = msg_tx.downgrade();

    let recv_gates = gates.clone();
    let receiver_handle = tokio::spawn(async move {
        // Window decoder: created once, long-lived (only used in window
        // mode; codec pinned at startup, §16.4 — never rebuilt).
        let mut window_decoder: Option<Box<dyn WindowDecoder>> = if recv_window_mode {
            Some(create_window_decoder(recv_fec_backend, recv_symbol_size, recv_window_generation))
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
        //
        // ORDERING is a per-stream delivery POLICY, independent of the codec
        // triangle (paper §16.2). Two limits of the reorder horizon H:
        //   - in-order (H = ∞): hold at holes → the reorder buffer.
        //   - unordered (H = 0): emit each decoded unit the instant it
        //     decodes → NO reorder buffer at all (RWM Phase C). Correct and
        //     lowest-latency for any consumer that does not need byte-stream
        //     order (objects reassembled by offset, datagrams, RPC/telemetry)
        //     — the object/perf path is just one such consumer.
        // Unordered is the SIMPLER implementation: the buffer is removed, not
        // added to. The in-order RECEIVED prefix (for retention/ack) is
        // tracked by a lightweight frontier over `received_seqs` instead.
        let mut reorder_buf = if recv_window_ooo {
            None
        } else if recv_window_mode && recv_window_reliable {
            Some(ReorderBuffer::new_reliable())
        } else if recv_window_mode && config.reorder_timeout_ms > 0 {
            Some(ReorderBuffer::new(config.reorder_timeout_ms, config.reorder_max_size))
        } else {
            None
        };
        // ── δ-honest overload shedding, receiver arm (fix C, goal-gate
        // "Unified Shedding") ── the in-order hold for the window EVICT path
        // becomes the δ-derived H = b·SRTT (§16.20.3: "the reorder_timeout
        // IS the δ dial"; b(Realtime) = ½ — this path exists only for the
        // Realtime hint) instead of the bulk-shaped 4×SRTT ∈ [60, 300] ms
        // clamp, WHILE the give-up budget holds: holes given up ≤ ε̂_recv ×
        // frontier (the loss-class bound — give-up is intrinsically
        // holes-only, so the realized fraction stays in the FEC-residual
        // class). Budget spent ⇒ the hold reverts to legacy (serialize:
        // ρ wins over δ). Armed only on the EVICT in-order path under
        // RWM_UNIFIED (the ρ = 1 reliable buffer never gives up, unchanged);
        // `RWM_UNIFIED_SHED=0` = the serializing control arm.
        let recv_shed_on = recv_window_mode
            && !recv_window_reliable
            && !recv_window_ooo
            && reorder_buf.is_some()
            && shed_armed(recv_gates.unified, false, recv_gates.unified_shed);
        if recv_shed_on {
            // Mechanism-liveness echo (MEASUREMENT DISCIPLINE item 1).
            info!(
                "unified overload shedding ACTIVE at receiver (RWM_UNIFIED_SHED: in-order hold = delta-derived b*SRTT within the eps-class give-up budget)"
            );
        }
        // Holes given up (seqs the in-order frontier passed undelivered) and
        // the diag throttle for the [SHED-R] gauge.
        let mut recv_shed_holes: u64 = 0;
        let mut recv_shed_budget_open = true;
        let recv_shed_diag = recv_gates.diag;
        let mut recv_shed_diag_at = Instant::now();
        // RWM Phase C unordered delivery: next in-order seq NOT yet received
        // (the frontier). Walks `received_seqs` to drive the cumulative
        // WindowAck (retention pruning) while delivery itself is unordered.
        let mut ooo_frontier: u64 = 0;
        // Reliable mode: when delivery is stalled on a hole, periodically
        // re-advertise the gap (SACK-bearing WindowAck) — acks are
        // best-effort datagrams, and a lost gap report must not leave
        // recovery to the sender's single-seq tail sweep alone.
        let mut last_hole_nack_at = Instant::now();
        // Track received seqs for WindowNack gap reporting
        let mut received_seqs: BTreeSet<u64> = BTreeSet::new();
        // RWM_REASM_BDP occupancy probe (feat/sack-bdp-reassembly): the maximum
        // reassembly buffer occupancy observed = received-but-not-yet-delivered
        // symbols held behind the in-order frontier. This is the quantity the
        // reliability invariant bounds — it must stay ~BDP (the sender's
        // outstanding cap), never grow to the whole object. `reasm_max_pending`
        // = peak held symbols; `reasm_max_span` = peak (highest_seen − frontier)
        // seq gap. Reported via `[REASM]` under RWM_REASM_BDP.
        let mut reasm_max_pending: usize = 0;
        let mut reasm_max_span: u64 = 0;
        let mut reasm_last_report = Instant::now();
        // feat/c8-conversion DIAGNOSIS gauges (goal-gate "C8 Slow-Path
        // Conversion", RWM_DIAG only — behavior-inert): the RECEIVER-side
        // view of slow-path conversion, per arrival path:
        //  * first[p]  — seqs whose FIRST copy this path delivered (source
        //    arrival or repair-decode output) = the path's real conversion.
        //  * dup[p]    — source arrivals for an already-received seq =
        //    displacement-only deliveries (candidate (b): a cross-path
        //    retransmit or the outrun original got there first).
        //  * lead[p]   — Σ (seq − in-order frontier) at first-copy arrival,
        //    in symbols (candidate (d) arrival-alignment: lead 0 = the
        //    stream was already WAITING on this symbol when it arrived).
        //  * unb_n/ms[p] — frontier unblocks credited to this path: an
        //    arrival that advanced the stalled (≥ 5 ms) in-order frontier,
        //    with the stall time it ended (candidate (c) resolution side).
        let c8r_on = recv_gates.diag;
        let mut c8r_first: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
        let mut c8r_dup: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
        let mut c8r_lead: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
        let mut c8r_unb_n: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
        let mut c8r_unb_ms: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
        let mut c8r_last_adv = Instant::now();
        let mut c8r_last_print = Instant::now();
        // Generation-deficit feedback (§16.3), receiver arm. `gen_widths[anchor]`
        // = the generation's K_g, learned self-describingly from the wire header
        // (`window_count`) of any coded symbol for that anchor. Deficit_g =
        // K_g − rank_in(anchor, K_g). `last_deficit_send` paces the reports to
        // ~once per SRTT (plus an immediate report on decode progress).
        let mut gen_widths: BTreeMap<u64, u16> = BTreeMap::new();
        // Generation size G (mirrors the sender's RWM_GEN default). Lets the
        // receiver SEED a provably-full generation's width (G) from the primary
        // seqs alone — see the seeding in `send_gen_deficits`. This closes the
        // small-G frontier-advance DEADLOCK: a generation whose ENTIRE proactive
        // repair budget was lost on the wire otherwise never enters `gen_widths`
        // (which learned widths only from repair headers), so the receiver
        // reported ZERO deficit for it while the in-order frontier wedged on its
        // hole — the sender was never told to recover it (MEASURED at G=96:
        // in_flight=0/src=0/cod=0). At large G the whole ceil(G·r) budget is
        // never fully lost, which is why only small G wedged.
        let recv_gen_size: u64 = recv_gates.gen_size as u64;
        // Receiver-tail parallelization (PART 1). Number of outstanding
        // generations whose deficit is reported (and anti-wedge-seeded) per
        // round. Legacy = 6 (frontier-first serial tail); env RWM_REPORT_GENS
        // lifts it to cover the whole in-flight range so EVERY hole is repaired
        // in ONE round-trip (parallel tail flush). Unset = byte-identical
        // shipped path. Clamped to the wire cap (MAX_ACK_IDS = 2000).
        // feat/gen-substrate-ceiling: under the derived-depth pipeline the
        // whole M*-generation in-flight range must be reportable in ONE round
        // (a 6-generation frontier-first report would re-serialize the deeper
        // pipeline's recovery — the PART-1 receiver-tail lesson).
        let report_gens: usize = recv_gates
            .report_gens
            .unwrap_or(if recv_gates.gen_pipe { GEN_PIPE_MAX_GENS + 1 } else { 6 })
            .clamp(1, 2000);
        // Repair-coverage horizon (branch `feat/nack-timing`). Base wait, in
        // MILLISECONDS, before a frontier hole's deficit is allowed to fire a
        // reactive NACK — the time for the in-flight proactive repair covering
        // it to arrive + decode (~a generation-span at the send rate, NOT an
        // RTT). Unset / 0 = byte-identical shipped path (report immediately).
        // Small and bounded: a few ms at 100 Mbit buys the whole round-trip an
        // ARQ pull would have cost. Made δ-aware at use: clamped to ≤ ½·SRTT so
        // low-RTT / latency-tight (Realtime) paths never over-wait, and it can
        // never exceed the round-trip it is trying to save.
        let repair_wait_base: Duration = recv_gates
            .repair_wait_ms
            .map(Duration::from_millis)
            .unwrap_or(Duration::ZERO);
        // Per-anchor first-armed instants for the horizon gate (see
        // `horizon_gate_deficits`). Persists across reports so the wait
        // accumulates; an anchor decoded within the horizon is disarmed there.
        let mut deficit_armed: BTreeMap<u64, Instant> = BTreeMap::new();
        let mut last_deficit_send = Instant::now() - Duration::from_secs(1);
        let mut highest_seen_seq: u64 = 0;
        let mut last_nack_time = Instant::now();
        // P10b dupack analog: highest_seen at the last gap-advertising ack,
        // and when it was sent (rate limit) — see GAP_ACK_MIN_INTERVAL.
        let mut last_gap_ack_seen: u64 = 0;
        let mut last_gap_ack_time = Instant::now() - GAP_ACK_MIN_INTERVAL;
        // ADR-0035: PI feedback tracking for window mode
        let mut last_pi_repairs_fed: u64 = 0;
        let mut last_pi_repairs_useful: u64 = 0;

        // ── Proactive-frontier diagnosis (RWM_FDIAG) ──────────────────────
        // Answers PART 1: when the in-order frontier stalls on a hole p, is
        // there already buffered proactive repair covering p (→ the receiver
        // should decode NOW), or is it absent (→ the hole waits on a reactive
        // ARQ source retransmit)? For each stall we record how long the
        // frontier sat on p and how p was ultimately resolved: DECODE (a repair
        // solved it, no round-trip) vs SOURCE (a retransmitted source symbol
        // arrived, a ~1-RTT ARQ round). Off unless RWM_FDIAG is set.
        let fdiag_on = recv_gates.fdiag;
        // Current frontier hole being tracked: (seq, stall_start, saw_buffered_
        // equation_during_stall, source_arrived_for_it). None = not stalled.
        let mut fdiag_hole: Option<(u64, Instant, bool, bool)> = None;
        let mut fdiag_report_at = Instant::now();
        // Aggregate resolution counts + stall time (µs), split by mechanism.
        let mut fdiag_decode_n: u64 = 0;
        let mut fdiag_source_n: u64 = 0;
        let mut fdiag_decode_us: u64 = 0;
        let mut fdiag_source_us: u64 = 0;
        // Of the DECODE resolutions, how many had a buffered equation covering p
        // ALREADY present when the stall began (present-but-waiting-for-rank)
        // vs the covering repair only arrived mid-stall.
        let mut fdiag_present_at_stall: u64 = 0;
        // H2 probe: RAW decoder-call wall-time. `fdiag_addsym_us` accumulates the
        // time spent INSIDE `win_dec.add_symbol()` (GF(256) GE compute) across the
        // whole transfer; `fdiag_addsym_n` is the call count. Compared against the
        // per-hole RESOLUTION wall-time (fdiag_decode_us, which spans hole-armed →
        // frontier-passes and thus includes symbol-arrival WAITING), this isolates
        // whether the "~25-67 ms decode" is compute or waiting-for-rank.
        let mut fdiag_addsym_us: u64 = 0;
        let mut fdiag_addsym_n: u64 = 0;
        // diag/unified-collapse: worst single add_symbol call in the current
        // FDIAG report interval (a mean hides a per-arrival cost blowup).
        let mut fdiag_addsym_max_us: u64 = 0;

        // ── Receiver wedge forensics (fix/frontier-wedge, RWM_DIAG) ────────
        // Names the mechanism when the in-order frontier freezes while the
        // sender demonstrably keeps retransmitting the blocker (the historic
        // c3/C8 ~60 s collapse run). Reported from the reliable hole-refresh
        // timer arm (which fires every 25–100 ms during any stall), once per
        // second after the frontier has been frozen > 1 s:
        //   * blocker seq + its decoder state (seen-as-source / recovered /
        //     output) + received_seqs membership → dup-filter wedge if
        //     seen && hole persists;
        //   * Data batches/symbols processed since the previous report → the
        //     intake rate, distinguishing "retransmits reach the decoder and
        //     are eaten" from "retransmits never reach the receive loop";
        //   * quinn DATAGRAM frame rx/tx per path → whether the wire is
        //     delivering frames that then die before `read_datagram()`.
        let wdiag_on = recv_gates.diag;
        let mut wdiag_frontier_val: u64 = 0;
        let mut wdiag_frontier_at = Instant::now();
        let mut wdiag_last_report = Instant::now();
        let mut wdiag_batches: u64 = 0; // Data batches processed (total)
        let mut wdiag_syms: u64 = 0; // symbols fed (total)
        let mut wdiag_batches_last: u64 = 0;
        let mut wdiag_syms_last: u64 = 0;
        // diag/lossy-residual (goal-gate "Lossy-Single Residual", RWM_DIAG
        // only): receiver INTER-ARRIVAL gap gauge — cumulative time in Data
        // arrival gaps ≥ 3 ms. This is the wire-truth idle gauge for
        // accounting term (b): a GE loss burst pauses arrivals for ≪ 3 ms at
        // c2/c3 packet rates, so stall-class gaps here are genuine wire idle
        // (sender/CC-caused), not loss shadows. Printed once per second as
        // `[WIDLE] idle=<cum ms>/<n>/mx<max ms> arr=<cum Data messages>`; the
        // end-of-run accounting reads the LAST line (cumulative counters).
        const WIDLE_GAP_MIN_US: u64 = 3_000;
        let mut widle_last_arrival_us: u64 = 0;
        let mut widle_us: u64 = 0;
        let mut widle_n: u64 = 0;
        let mut widle_max_us: u64 = 0;
        let mut widle_arrivals: u64 = 0;
        let mut widle_last_print_us: u64 = 0;

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

        // GENERATION-DEFICIT report (§16.3). Compute each frontier generation's
        // residual deficit from the decoder's current rank and send it to the
        // sender. `$force` sends even an empty vector (used on decode progress so
        // the sender clears wants for just-completed generations, and on the
        // periodic timer so a stalled/silent sender is re-pulled). Shared by the
        // data-arm (progress) and the timer arm (liveness) so a sender that has
        // gone quiet keeps being told the true deficit until every generation
        // decodes — the loop that makes deficit-driven recovery robust.
        macro_rules! send_gen_deficits {
            ($dec:expr, $force:expr) => {{
                if recv_window_generation {
                    // ANTI-WEDGE SEEDING (small-G frontier-advance deadlock). Seed
                    // the width (= G) of every generation that is PROVABLY FULL —
                    // one whose end lies at or below the highest seq seen, so its
                    // G source symbols certainly exist — starting at the frontier
                    // generation (where `ooo_frontier` is stuck on a hole). The
                    // deficit for such a generation is then computable from the
                    // primary seqs alone (`rank_in`'s recovered-count branch),
                    // WITHOUT ever having seen a repair for it. Without this, a
                    // generation whose entire ceil(G·r) proactive repair was lost
                    // never entered `gen_widths`, so the receiver reported zero
                    // deficit while its hole wedged the frontier forever. The final
                    // (possibly partial) generation is intentionally left to
                    // repair-header learning (its true width is not yet known to be
                    // G). Bounded to a few generations past the frontier (only the
                    // first MAX_REPORTED_GENS are ever sent anyway).
                    let g_front = ooo_frontier / recv_gen_size;
                    let g_top = highest_seen_seq / recv_gen_size;
                    // PART 1: seed the whole reportable range (not just +7) so a
                    // generation whose entire proactive budget was lost is
                    // NACKed in the SAME round as the frontier, not serially.
                    let g_hi = g_top.min(g_front + report_gens as u64);
                    let mut g = g_front;
                    while g <= g_hi {
                        let anchor = g * recv_gen_size;
                        if anchor + recv_gen_size <= highest_seen_seq + 1 {
                            gen_widths.entry(anchor).or_insert(recv_gen_size as u16);
                        }
                        g += 1;
                    }
                }
                if recv_window_generation && !gen_widths.is_empty() {
                    gen_widths.retain(|&a, &mut k| a + k as u64 > ooo_frontier);
                    // PART 1: report EVERY outstanding generation's deficit (up
                    // to report_gens = the whole in-flight range) in one report,
                    // so the sender repairs all holes in a single round-trip
                    // (parallel tail flush) rather than frontier-first serially.
                    let raw_deficits = collect_gen_deficits(&gen_widths, report_gens, |anchor, k| {
                        $dec.rank_in(anchor, k)
                    });
                    // Repair-coverage horizon (branch `feat/nack-timing`): give
                    // the in-flight proactive repair a chance to decode each hole
                    // before its deficit fires a reactive NACK. δ-aware — clamped
                    // to ≤ ½·SRTT so low-RTT / latency-tight paths never over-wait
                    // and the wait can never exceed the round-trip it would save.
                    let horizon = if repair_wait_base.is_zero() {
                        Duration::ZERO
                    } else {
                        let srtt = {
                            let sched = recv_scheduler.lock();
                            sched
                                .live_paths()
                                .into_iter()
                                .filter_map(|pid| sched.path(pid).map(|p| p.srtt()))
                                .max()
                        };
                        match srtt {
                            Some(s) => repair_wait_base.min(s / 2),
                            None => repair_wait_base,
                        }
                    };
                    let deficits = horizon_gate_deficits(
                        &raw_deficits,
                        &mut deficit_armed,
                        horizon,
                        Instant::now(),
                    );
                    if !deficits.is_empty() || $force {
                        last_deficit_send = Instant::now();
                        if recv_gates.trace {
                            let total: u32 = deficits.iter().map(|(_, d)| d).sum();
                            let withheld = raw_deficits.len().saturating_sub(deficits.len());
                            eprintln!(
                                "[RCV] frontier={} gens_tracked={} deficits={:?} total_deficit={} withheld_by_horizon={} horizon_ms={}",
                                ooo_frontier, gen_widths.len(), deficits, total,
                                withheld, horizon.as_millis()
                            );
                        }
                        let msg = ControlMessage::GenerationDeficit { deficits };
                        for pid in recv_scheduler.lock().live_paths() {
                            let _ = recv_transport.send_control_datagram(pid, msg.clone());
                        }
                    }
                }
            }};
        }

        // RWM_RDIAG state (see rdiag_probe above): idle time awaiting the
        // select, message count, queue-depth samples over each ~500 ms window.
        let rdiag_on = recv_gates.rdiag;
        let mut rdiag_idle_us: u64 = 0;
        let mut rdiag_msgs: u64 = 0;
        let mut rdiag_qsum: u64 = 0;
        let mut rdiag_qmax: usize = 0;
        let mut rdiag_qn: u64 = 0;
        let mut rdiag_last = Instant::now();

        // Emission batching, receiver arm — BUILT AND REFUTED 2026-07-27
        // (goal-gate "Emission Batching"): a bounded burst drain of this
        // loop's inbound queue (± per-burst cumulative-ack coalescing, any
        // burst size) collapsed c1 227.6 → 136–144 Mbit/s with an echo-RTT
        // inflation → dynamic-store-cap growth → spurious sweep/retx flood
        // (retx ×3–6, paused 60%+). The engine receiver stays one-message-
        // per-wake; its ~20–23k msgs/s service wall is the named residual
        // binder. Code removed per the DEPRECATION REGISTER discipline —
        // commits 97bc6ea/47b04ed preserve the refuted mechanism.

        loop {
            // Periodic generation-deficit report deadline (§16.3): re-report the
            // frontier deficit ~once per SRTT even absent new data, so a sender
            // that emitted its budget and went quiet is always re-pulled and a
            // lost report is retransmitted. Only armed once a generation is known.
            let deficit_deadline: Option<tokio::time::Instant> =
                if recv_window_generation && !gen_widths.is_empty() {
                    let srtt = {
                        let sched = recv_scheduler.lock();
                        sched
                            .live_paths()
                            .into_iter()
                            .filter_map(|pid| sched.path(pid).map(|p| p.srtt()))
                            .max()
                    };
                    let interval = srtt
                        .map(|s| s.clamp(Duration::from_millis(3), Duration::from_millis(50)))
                        .unwrap_or(Duration::from_millis(10));
                    let elapsed = last_deficit_send.elapsed();
                    let remaining = interval.saturating_sub(elapsed);
                    Some(tokio::time::Instant::now() + remaining)
                } else {
                    None
                };

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
                } else if recv_window_ooo {
                    // Unordered delivery holds nothing, but a hole in the
                    // received prefix still needs the tail-recovery timer to
                    // re-advertise the gap (SACK WindowAck) so the sender
                    // retransmits it — the same reliability backstop the
                    // in-order buffer's pending_count provided.
                    highest_seen_seq > highest_delivered_seq
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
                        // δ-honest shed (fix C): under the unified realtime
                        // machine the EVICT hold is the δ dial b·SRTT while
                        // the ε̂-class give-up budget is open; legacy 4×SRTT
                        // clamp otherwise (incl. always for block mode and
                        // with the law off — bit-exact legacy).
                        if recv_shed_on {
                            let eps_recv = {
                                let sched = recv_scheduler.lock();
                                sched
                                    .live_paths()
                                    .into_iter()
                                    .filter_map(|pid| {
                                        sched.path(pid).map(|p| p.estimator.loss_rate())
                                    })
                                    .fold(0.0_f64, f64::max)
                            };
                            let frontier = reorder_buf
                                .as_ref()
                                .map(|rb| rb.next_deliver_seq())
                                .unwrap_or(0);
                            recv_shed_budget_open =
                                shed_recv_budget_ok(recv_shed_holes, frontier, eps_recv);
                        }
                        let hold = srtt.map(|s| {
                            shed_recv_hold(s, recv_shed_on, recv_shed_budget_open)
                        });
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
            let rdiag_t0 = if rdiag_on { Some(Instant::now()) } else { None };
            let (path_id, msg) = tokio::select! {
                msg = msg_rx.recv() => {
                    match msg {
                        Some(m) => m,
                        None => break, // channel closed
                    }
                }
                _ = async {
                    match deficit_deadline {
                        Some(d) => tokio::time::sleep_until(d).await,
                        None => std::future::pending().await,
                    }
                } => {
                    // Periodic generation-deficit report (liveness): re-tell the
                    // sender the true residual deficit for every frontier
                    // generation, even with no new arrivals, so a sender that
                    // emitted its budget and stalled is re-pulled to completion.
                    if let Some(ref dec) = window_decoder {
                        send_gen_deficits!(dec, true);
                    }
                    continue;
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
                        // Wedge forensics (fix/frontier-wedge): the frontier
                        // is stalled (this arm only fires with a pending
                        // hole). Once frozen > 1 s, name the blocker's
                        // receiver-side state once per second.
                        if wdiag_on {
                            if highest_delivered_seq != wdiag_frontier_val {
                                wdiag_frontier_val = highest_delivered_seq;
                                wdiag_frontier_at = Instant::now();
                            }
                            let stall = wdiag_frontier_at.elapsed();
                            if stall >= Duration::from_secs(1)
                                && wdiag_last_report.elapsed() >= Duration::from_secs(1)
                            {
                                wdiag_last_report = Instant::now();
                                let blocker = reorder_buf
                                    .as_ref()
                                    .map(|rb| rb.next_deliver_seq())
                                    .unwrap_or(ooo_frontier);
                                let (b_seen, b_rec, b_out) = window_decoder
                                    .as_ref()
                                    .map(|d| d.seq_probe(blocker))
                                    .unwrap_or((false, false, false));
                                let pending = reorder_buf
                                    .as_ref()
                                    .map(|rb| rb.pending_count())
                                    .unwrap_or(0);
                                let d_batches = wdiag_batches - wdiag_batches_last;
                                let d_syms = wdiag_syms - wdiag_syms_last;
                                wdiag_batches_last = wdiag_batches;
                                wdiag_syms_last = wdiag_syms;
                                let mut dg = String::new();
                                for pid in recv_scheduler.lock().live_paths() {
                                    if let Some((rx, tx)) =
                                        recv_transport.datagram_frame_stats(pid)
                                    {
                                        dg.push_str(&format!(
                                            " p{pid}:dg_rx={rx}/dg_tx={tx}"
                                        ));
                                    }
                                }
                                eprintln!(
                                    "[WEDGE] stall={:.1}s frontier={} blocker={} \
                                     seen_src={} recovered={} output={} in_rseqs={} \
                                     pending={} highest_seen={} span={} \
                                     batches/s={} syms/s={}{}",
                                    stall.as_secs_f64(),
                                    highest_delivered_seq,
                                    blocker,
                                    b_seen,
                                    b_rec,
                                    b_out,
                                    received_seqs.contains(&blocker),
                                    pending,
                                    highest_seen_seq,
                                    highest_seen_seq.saturating_sub(highest_delivered_seq),
                                    d_batches,
                                    d_syms,
                                    dg,
                                );
                            }
                        }
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
                        let shed_frontier_before = reorder.next_deliver_seq();
                        let expired = reorder.drain_expired(Instant::now());
                        // δ-honest shed accounting: seqs the frontier passed
                        // minus entries actually delivered = holes given up.
                        if recv_shed_on {
                            recv_shed_holes += reorder
                                .next_deliver_seq()
                                .saturating_sub(shed_frontier_before)
                                .saturating_sub(expired.len() as u64);
                            if recv_shed_diag
                                && recv_shed_diag_at.elapsed() >= Duration::from_millis(500)
                            {
                                recv_shed_diag_at = Instant::now();
                                eprintln!(
                                    "[SHED-R] holes={} frontier={} budget_open={}",
                                    recv_shed_holes,
                                    reorder.next_deliver_seq(),
                                    recv_shed_budget_open,
                                );
                            }
                        }
                        for (dseq, ddata) in expired {
                            debug!(seq = dseq, "window hold expired — force-delivering");
                            for pkt_data in extract_window_packets(&ddata, window_packed) {
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
            if let Some(t0) = rdiag_t0 {
                rdiag_idle_us += t0.elapsed().as_micros() as u64;
                rdiag_msgs += 1;
                if rdiag_msgs % 16 == 0 {
                    if let Some(s) = rdiag_probe.upgrade() {
                        let q = s.max_capacity().saturating_sub(s.capacity());
                        rdiag_qsum += q as u64;
                        rdiag_qmax = rdiag_qmax.max(q);
                        rdiag_qn += 1;
                    }
                }
                let w = rdiag_last.elapsed();
                if w >= Duration::from_millis(500) {
                    let wall_us = w.as_micros() as u64;
                    let busy =
                        100.0 * (1.0 - rdiag_idle_us as f64 / wall_us.max(1) as f64);
                    eprintln!(
                        "[RDIAG] busy={:.0}% msgs={}/s q_avg={:.0} q_max={} cap={}",
                        busy,
                        rdiag_msgs * 1_000_000 / wall_us.max(1),
                        rdiag_qsum as f64 / rdiag_qn.max(1) as f64,
                        rdiag_qmax,
                        rdiag_probe.upgrade().map(|s| s.max_capacity()).unwrap_or(0),
                    );
                    rdiag_idle_us = 0;
                    rdiag_msgs = 0;
                    rdiag_qsum = 0;
                    rdiag_qmax = 0;
                    rdiag_qn = 0;
                    rdiag_last = Instant::now();
                }
            }
            match msg {
                WireMessage::Data(batch) => {
                    let batch_send_ts = batch.send_timestamp_us;
                    let batch_seq = batch.batch_seq;
                    let batch_path_id = batch.path_id;
                    let symbol_count = batch.symbols.len() as u32;
                    if wdiag_on {
                        wdiag_batches += 1;
                        wdiag_syms += symbol_count as u64;
                        // diag/lossy-residual [WIDLE] gauge (see decls).
                        let wnow = now_us();
                        widle_arrivals += 1;
                        if widle_last_arrival_us > 0 {
                            let gap = wnow.saturating_sub(widle_last_arrival_us);
                            if gap >= WIDLE_GAP_MIN_US {
                                widle_us += gap;
                                widle_n += 1;
                                widle_max_us = widle_max_us.max(gap);
                            }
                        }
                        widle_last_arrival_us = wnow;
                        if wnow.saturating_sub(widle_last_print_us) >= 1_000_000 {
                            widle_last_print_us = wnow;
                            eprintln!(
                                "[WIDLE] idle={}ms/{}/mx{}ms arr={}",
                                widle_us / 1000,
                                widle_n,
                                widle_max_us / 1000,
                                widle_arrivals,
                            );
                        }
                    }

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
                        // Generation-deficit feedback: learn each generation's
                        // K_g self-describingly from the wire header (window_start
                        // = anchor, window_count = K_g) of every coded symbol, and
                        // note whether this batch made any decode progress (drives
                        // an immediate deficit report).
                        let mut recovered_any = false;
                        if recv_window_generation {
                            for symbol in &batch.symbols {
                                if symbol.is_repair && symbol.data.len() >= 10 {
                                    // FILLING-generation repair (proactive pacer):
                                    // its wire `window_count` is the FULL generation
                                    // width G even though the generation is only
                                    // partially sent, so it MUST NOT teach
                                    // `gen_widths` — that would make the receiver
                                    // report a K_g−rank deficit of (G − current fill)
                                    // and flood reactive recovery for a generation
                                    // that is not even fully sent yet. The FILL_FLAG
                                    // is bit 31 of the 4-byte coded-index. A filling
                                    // generation enters `gen_widths` only once it is
                                    // PROVABLY FULL (anti-wedge seeding) or a
                                    // sealed/deficit repair arrives — the honest
                                    // deficit path. Present-at-stall recovery of its
                                    // holes is proactive (no deficit needed).
                                    let is_fill = symbol.data.len() >= 14
                                        && (u32::from_le_bytes(
                                            symbol.data[10..14].try_into().unwrap(),
                                        ) & 0x8000_0000)
                                            != 0;
                                    if is_fill {
                                        continue;
                                    }
                                    let anchor = u64::from_le_bytes(
                                        symbol.data[0..8].try_into().unwrap(),
                                    );
                                    let count = u16::from_le_bytes(
                                        symbol.data[8..10].try_into().unwrap(),
                                    );
                                    if count > 0 {
                                        let e = gen_widths.entry(anchor).or_insert(0);
                                        if count > *e {
                                            *e = count;
                                        }
                                    }
                                }
                            }
                        }
                        for symbol in &batch.symbols {
                            // feat/c8-conversion DIAG: a source arrival for a
                            // seq already received = a displacement-only
                            // delivery on this path (its goodput was already
                            // banked by the other copy).
                            if c8r_on
                                && !symbol.is_repair
                                && received_seqs.contains(&symbol.block_id)
                            {
                                *c8r_dup.entry(path_id).or_insert(0) += 1;
                            }
                            let recovered = if fdiag_on {
                                let t_dec = Instant::now();
                                let r = win_dec.add_symbol(symbol);
                                let call_us = t_dec.elapsed().as_micros() as u64;
                                fdiag_addsym_us += call_us;
                                fdiag_addsym_max_us = fdiag_addsym_max_us.max(call_us);
                                fdiag_addsym_n += 1;
                                r
                            } else {
                                win_dec.add_symbol(symbol)
                            };
                            if !recovered.is_empty() {
                                recovered_any = true;
                            }
                            for (seq, sym_data) in recovered {
                                received_seqs.insert(seq);
                                if seq > highest_seen_seq {
                                    highest_seen_seq = seq;
                                }
                                // feat/c8-conversion DIAG: FIRST copy of this
                                // seq, credited to the arrival path, with its
                                // lead over the in-order frontier (symbols).
                                if c8r_on {
                                    let frontier = reorder_buf
                                        .as_ref()
                                        .map(|r| r.next_deliver_seq())
                                        .unwrap_or(ooo_frontier);
                                    *c8r_first.entry(path_id).or_insert(0) += 1;
                                    *c8r_lead.entry(path_id).or_insert(0) +=
                                        seq.saturating_sub(frontier);
                                }

                                // RWM Phase C (paper §16.2, H→∞ corner):
                                // out-of-order OBJECT delivery. Hand each
                                // decoded symbol to the consumer the instant
                                // it decodes — in ANY order. The native object
                                // API reassembles by offset and completes on
                                // total-decoded, so no in-order frontier gates
                                // delivery. Reliability is unchanged: the
                                // reorder buffer still tracks the in-order
                                // RECEIVED prefix (holes held as seq-only
                                // placeholders) that drives the cumulative
                                // WindowAck, so the sender keeps retaining +
                                // retransmitting every hole until acked.
                                // Equivalence (§16.2): identical in completion
                                // time to an in-order buffer deep enough to
                                // hold to completion — the frontier only costs
                                // an INCREMENTAL, low-latency consumer (inner
                                // TCP), never a file.
                                if recv_window_ooo {
                                    for pkt_data in extract_window_packets(&sym_data, window_packed) {
                                        // Deliver immediately (any order). Full
                                        // channel drops rather than blocks: the
                                        // object/native consumer drains far
                                        // faster than the wire, so the bounded
                                        // (8192) channel only fills under a
                                        // pathological burst; blocking here
                                        // instead would wedge the loopback's
                                        // client-feeds-and-drains feedback loop
                                        // (MEASURED deadlock). A rare drop is
                                        // recovered by the sender's retransmit
                                        // (the reliability floor keeps recovery
                                        // from ever being fully suppressed).
                                        if deliver_packet(&recv_tun_tx, Bytes::from(pkt_data), false)
                                            .await
                                            .is_err()
                                        {
                                            error!("TUN inject channel closed");
                                            return;
                                        }
                                    }
                                    // Advance the in-order RECEIVED prefix for
                                    // the cumulative WindowAck (retention
                                    // pruning) — no reorder buffer: the
                                    // frontier walks `received_seqs` (seq was
                                    // inserted just above). Delivery already
                                    // happened, out of order; this only tells
                                    // the sender what it may prune, so holes
                                    // stay retained + retransmitted.
                                    while received_seqs.contains(&ooo_frontier) {
                                        ooo_frontier += 1;
                                    }
                                    highest_delivered_seq = ooo_frontier.saturating_sub(1);
                                    continue;
                                }

                                // ----- in-order delivery (default: TCP-in-
                                // tunnel and Realtime need the frontier) -----
                                let deliverable = if let Some(ref mut reorder) = reorder_buf {
                                    reorder.push(seq, sym_data)
                                } else {
                                    vec![(seq, sym_data)]
                                };

                                // feat/c8-conversion DIAG: this arrival
                                // advanced the in-order frontier — if it had
                                // been stalled ≥ 5 ms, credit the unblock
                                // (and the stall it ended) to this path.
                                if c8r_on && !deliverable.is_empty() {
                                    let gap = c8r_last_adv.elapsed();
                                    if gap >= Duration::from_millis(5) {
                                        *c8r_unb_n.entry(path_id).or_insert(0) += 1;
                                        *c8r_unb_ms.entry(path_id).or_insert(0) +=
                                            gap.as_millis() as u64;
                                    }
                                    c8r_last_adv = Instant::now();
                                }

                                for (dseq, ddata) in deliverable {
                                    for pkt_data in extract_window_packets(&ddata, window_packed) {
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

                        // feat/c8-conversion DIAG: the receiver-side
                        // conversion gauges, cumulative, ~1/s (keys sorted
                        // for stable scraping). fst/dup = first-copy vs
                        // displacement deliveries; lead = mean first-copy
                        // frontier lead (symbols); unb = frontier unblocks
                        // credited / stall ms ended.
                        if c8r_on && c8r_last_print.elapsed() >= Duration::from_secs(1) {
                            c8r_last_print = Instant::now();
                            let mut keys: Vec<u32> = c8r_first
                                .keys()
                                .chain(c8r_dup.keys())
                                .chain(c8r_unb_n.keys())
                                .copied()
                                .collect();
                            keys.sort_unstable();
                            keys.dedup();
                            if !keys.is_empty() {
                                let mut s = String::new();
                                for k in keys {
                                    let f = c8r_first.get(&k).copied().unwrap_or(0);
                                    s.push_str(&format!(
                                        " p{}:fst={} dup={} lead={:.0} unb={}/{}ms",
                                        k,
                                        f,
                                        c8r_dup.get(&k).copied().unwrap_or(0),
                                        c8r_lead.get(&k).copied().unwrap_or(0) as f64
                                            / f.max(1) as f64,
                                        c8r_unb_n.get(&k).copied().unwrap_or(0),
                                        c8r_unb_ms.get(&k).copied().unwrap_or(0),
                                    ));
                                }
                                eprintln!("[C8CONV-R]{}", s);
                            }
                        }

                        // GENERATION-DEFICIT FEEDBACK (§16.3, receiver arm): on
                        // decode progress, report each frontier generation's
                        // residual deficit immediately (progress → the deficit
                        // shrank → tell the sender promptly so it stops over-
                        // sending). The periodic timer arm below drives it
                        // otherwise — crucially even when NO data is arriving, so
                        // a sender that emitted its budget and went quiet is still
                        // re-pulled (the measured silent-sender deadlock).
                        if recv_window_generation && recovered_any {
                            send_gen_deficits!(win_dec, true);
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
                            let (srtt, eps_recv) = {
                                let sched = recv_scheduler.lock();
                                let srtt = sched
                                    .live_paths()
                                    .into_iter()
                                    .filter_map(|pid| sched.path(pid).map(|p| p.srtt()))
                                    .max();
                                // δ-honest shed: ε̂_recv for the give-up
                                // budget (only read when the law is armed).
                                let eps = if recv_shed_on {
                                    sched
                                        .live_paths()
                                        .into_iter()
                                        .filter_map(|pid| {
                                            sched.path(pid).map(|p| p.estimator.loss_rate())
                                        })
                                        .fold(0.0_f64, f64::max)
                                } else {
                                    0.0
                                };
                                (srtt, eps)
                            };
                            if recv_shed_on {
                                recv_shed_budget_open = shed_recv_budget_ok(
                                    recv_shed_holes,
                                    reorder.next_deliver_seq(),
                                    eps_recv,
                                );
                            }
                            if let Some(s) = srtt {
                                reorder.set_timeout(shed_recv_hold(
                                    s,
                                    recv_shed_on,
                                    recv_shed_budget_open,
                                ));
                            }
                            let shed_frontier_before = reorder.next_deliver_seq();
                            let expired = reorder.drain_expired(Instant::now());
                            if recv_shed_on {
                                recv_shed_holes += reorder
                                    .next_deliver_seq()
                                    .saturating_sub(shed_frontier_before)
                                    .saturating_sub(expired.len() as u64);
                            }
                            for (dseq, ddata) in expired {
                                for pkt_data in extract_window_packets(&ddata, window_packed) {
                                    let _ = recv_tun_tx.try_send(Bytes::from(pkt_data));
                                }
                                if dseq > highest_delivered_seq {
                                    highest_delivered_seq = dseq;
                                }
                            }
                        }

                        // ── Proactive-frontier diagnosis (RWM_FDIAG) ──────
                        if fdiag_on {
                            let f = highest_delivered_seq;
                            // Resolve a tracked hole once the frontier passes it.
                            if let Some((hp, t0, present, saw_src)) = fdiag_hole {
                                if f >= hp {
                                    let by_source = saw_src
                                        || batch.symbols.iter().any(|s| {
                                            !s.is_repair && s.block_id == hp
                                        });
                                    let dt = t0.elapsed().as_micros() as u64;
                                    if by_source {
                                        fdiag_source_n += 1;
                                        fdiag_source_us += dt;
                                    } else {
                                        fdiag_decode_n += 1;
                                        fdiag_decode_us += dt;
                                        if present {
                                            fdiag_present_at_stall += 1;
                                        }
                                    }
                                    fdiag_hole = None;
                                } else if batch.symbols.iter().any(|s| {
                                    !s.is_repair && s.block_id == hp
                                }) {
                                    // Still stalled, but the hole's source
                                    // symbol (a retransmit) just arrived — mark
                                    // so the eventual resolution is ARQ, not
                                    // proactive decode.
                                    fdiag_hole = Some((hp, t0, present, true));
                                }
                            }
                            // Arm a new hole when stalled with none tracked.
                            if fdiag_hole.is_none() && highest_seen_seq > f {
                                let (_h, buffered) =
                                    win_dec.frontier_probe(f + 1, highest_seen_seq);
                                fdiag_hole =
                                    Some((f + 1, Instant::now(), buffered > 0, false));
                            }
                            // Periodic aggregate report (~500 ms).
                            if fdiag_report_at.elapsed() >= Duration::from_millis(500) {
                                fdiag_report_at = Instant::now();
                                let (holes, buffered) =
                                    win_dec.frontier_probe(f + 1, highest_seen_seq);
                                let dec_avg = if fdiag_decode_n > 0 {
                                    fdiag_decode_us / fdiag_decode_n
                                } else {
                                    0
                                };
                                let src_avg = if fdiag_source_n > 0 {
                                    fdiag_source_us / fdiag_source_n
                                } else {
                                    0
                                };
                                // H2: mean RAW decode-call compute time (µs) and
                                // TOTAL compute over the transfer — contrast with
                                // the per-hole DECODE resolution wall-time above.
                                let addsym_avg = if fdiag_addsym_n > 0 {
                                    fdiag_addsym_us / fdiag_addsym_n
                                } else {
                                    0
                                };
                                eprintln!(
                                    "[FDIAG] frontier={} seen={} gap={} probe_holes={} probe_buffered={} | DECODE n={} avg={}us present_at_stall={} | SOURCE n={} avg={}us | COMPUTE calls={} avg={}us max={}us total={}ms | rf={} ru={}{}",
                                    f, highest_seen_seq,
                                    highest_seen_seq.saturating_sub(f),
                                    holes, buffered,
                                    fdiag_decode_n, dec_avg, fdiag_present_at_stall,
                                    fdiag_source_n, src_avg,
                                    fdiag_addsym_n, addsym_avg,
                                    std::mem::take(&mut fdiag_addsym_max_us),
                                    fdiag_addsym_us / 1000,
                                    win_dec.repairs_fed(), win_dec.repairs_useful(),
                                    // diag/unified-collapse: decoder-internal
                                    // cost drivers (active rows L, span, memory)
                                    win_dec
                                        .diag_stats()
                                        .map(|s| format!(" | {s}"))
                                        .unwrap_or_default(),
                                );
                                // diag/unified-collapse: transit-layer counters
                                // at the receiver — did datagrams reach quinn?
                                let dg = recv_transport
                                    .datagram_frame_stats(path_id)
                                    .map(|(rx, tx)| format!("dg_rx={rx} dg_tx={tx}"))
                                    .unwrap_or_default();
                                let sh = recv_transport
                                    .l0_transit_stats()
                                    .map(|(e, g, td, ok, er, q)| {
                                        format!(
                                            " shim enq={e} ge={g} tail={td} ok={ok} err={er} q={q}"
                                        )
                                    })
                                    .unwrap_or_default();
                                eprintln!("[FDIAG-T] {dg}{sh}");
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
                                // In OOO / generation mode carry the TOTAL count
                                // of decoded source symbols across ALL generations
                                // (out of order) — the peer's total decode progress
                                // `d`. received_seqs holds every delivered seq
                                // (decode-on-total), so its length IS d. Legacy
                                // (in-order) modes keep the per-path received count.
                                // (FMTCP-era wire field; its sender-side FC consumer
                                // was removed with RWM_FMTCP 2026-07-27 — the field
                                // stays as the wire-format/debug-trace datum.)
                                cumulative_received: if recv_window_ooo {
                                    received_seqs.len() as u64
                                } else {
                                    recv_stats.path(path_id)
                                        .map(|ps| ps.symbols_received.load(Ordering::Relaxed))
                                        .unwrap_or(0)
                                },
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
                            let mut prune_before = highest_delivered_seq.saturating_sub(recv_win_cap * 2);
                            // RELIABILITY INVARIANT (RWM_REASM_BDP): never evict a
                            // received symbol before it is delivered. Under SACK the
                            // sender races ahead of the frozen in-order frontier, so
                            // `highest_seen_seq` runs far above `highest_delivered_seq`
                            // (the hole). The prune is keyed on the DELIVERED frontier
                            // (so `prune_before ≤ highest_delivered_seq` already), but
                            // clamp it explicitly so the composed decoupling can never
                            // drop a received-above-hole symbol the sender has pruned.
                            // The reorder buffer is separately non-evicting (usize::MAX),
                            // so held source symbols survive to delivery regardless.
                            if reasm_bdp_on {
                                prune_before = prune_before.min(highest_delivered_seq);
                            }
                            received_seqs = received_seqs.split_off(&prune_before);
                            if let Some(ref mut wd) = window_decoder {
                                wd.advance(prune_before);
                            }
                            // Occupancy probe: peak reassembly held behind the frontier.
                            if reasm_bdp_on {
                                let pending = reorder_buf
                                    .as_ref()
                                    .map(|rb| rb.pending_count())
                                    .unwrap_or_else(|| {
                                        // OOO mode: no reorder buffer; the held state
                                        // is the received-seq set above the frontier.
                                        received_seqs.range(ooo_frontier..).count()
                                    });
                                reasm_max_pending = reasm_max_pending.max(pending);
                                let span = highest_seen_seq.saturating_sub(highest_delivered_seq);
                                reasm_max_span = reasm_max_span.max(span);
                                if reasm_last_report.elapsed() >= Duration::from_millis(500) {
                                    reasm_last_report = Instant::now();
                                    eprintln!(
                                        "[REASM] frontier={} highest_seen={} span={} pending={} max_pending={} max_span={}",
                                        highest_delivered_seq, highest_seen_seq, span,
                                        pending, reasm_max_pending, reasm_max_span,
                                    );
                                }
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
                        if recv_window_generation { Some(&recv_deficit_tx) } else { None },
                        recv_sack_tx.as_ref(),
                        recv_copa_feed.as_ref(),
                        recv_gates.mstar_anchor,
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
                        // feat/anchor-hygiene (`RWM_CLOCK_GAP`): a report
                        // tick inside a stall quarantine measures the
                        // release flood — skip the sample (the next tick's
                        // Δ/dt spans the disturbance and averages it out).
                        let gap_q = crate::control::anchor::stall_witness()
                            .is_some_and(|w| w.quarantined_now());
                        if delta > 0 && !gap_q {
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
    let ctrl_mstar_anchor = gates.mstar_anchor;
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
                        // the peer-ack atomic (nor the Copa feed) is needed
                        // here.
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        ctrl_mstar_anchor,
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

/// Path-scaled outstanding-pool cap (task #84, env `RWM_STORE_PATHS`).
///
/// The plain-reliable OUTSTANDING ceiling was a per-transfer constant
/// (`RELIABLE_STORE_MAX` = 1024): the dynamic delay cap latches at it on
/// fast paths (the legacy anchor over-reads), so a MULTIPATH sender is
/// store-starved — the pool that must fund Σ per-path (BDP + one recovery
/// round of runway) does not grow with the path count. Measured same-binary
/// at L1 (see the decl site): the knee is ≈2048 outstanding symbols PER
/// LIVE PATH at both C7 and C8, deeper pools re-enter the bufferbloat
/// collapse.
///
/// Returns `Some(cap)` when the path-scaled law applies — flag on, N ≥ 2
/// live paths, and a positive dynamic base (`pipe_sum` = Σ anchor-BDP, or
/// Σ Copa cwnd under the feed): cap = clamp(gain·N·pipe_sum, floor,
/// N·pool). Returns `None` when the caller must use the legacy single-path
/// law — so N = 1 is bit-exact legacy even with the flag ON.
pub fn path_scaled_store_cap(
    on: bool,
    n_live: usize,
    pipe_sum: f64,
    gain: f64,
    floor: usize,
    pool: usize,
) -> Option<usize> {
    if !on || n_live < 2 || pipe_sum <= 0.0 {
        return None;
    }
    let ceiling = n_live.saturating_mul(pool).max(floor);
    Some(((gain * n_live as f64 * pipe_sum).ceil() as usize).clamp(floor, ceiling))
}

/// Capacity-weighted SHARED outstanding pool (env `RWM_STORE_CAPW`) — the
/// ADR-0058 "c8 WATCH" follow-up: the c8-aware pool law.
///
/// The path-scaled pool (`RWM_STORE_PATHS`) scales by path COUNT:
/// cap = clamp(gain·N·Σpipe, floor, N·knee), which at asymmetric cells
/// over-weights the slow path — a 1/5-rate path contributes ×N to the
/// ceiling exactly like a full-rate path, so the pool grants unacked-frontier
/// depth the slow path cannot drain within its recovery round. Under
/// SACK-clocked release (ADR-0060) the pool bounds the UNACKED-FRONTIER SPAN
/// (outstanding = retained − SACK-released), so excess depth = the span the
/// cumulative frontier must resequence across the slow path's stragglers —
/// the measured c8 WATCH (stack 0.72–0.76×Σ vs legacy-1024 0.85–0.87×Σ).
///
/// The law here scales by CAPACITY instead: each live path earns depth for
/// its OWN pipe plus its own recovery round — the honest per-path cap law
/// ([`honest_store_cap`]: cap_i = rate_i·(K_i·RTprop_i + (gain−1)·(R +
/// RTprop_i))) — SUMMED AS ONE SHARED POOL, not per-path accounts: admission
/// still gates on the pooled total, so cross-path borrowing stays free
/// (ADR-0058's pooled-vindicated verdict kept; only the SIZING law changes).
///
///   pool = clamp(Σ_i cap_i, floor, N·knee)
///
/// Degenerates (unit-tested): symmetric N-path → N × the single-path term
/// (≈ N×(single pool) — c7 preserved); N = 1 → not engaged (`None`), the
/// caller keeps the legacy law bit-exactly; over-read anchors → the terms
/// clamp at the N·knee ceiling ≡ the path-scaled law (which is why the law
/// reads honestly only with the `RWM_PLAIN_RS` send-interval sampler).
///
/// `terms` = the per-live-path honest cap (None until that path's anchor is
/// warm). Returns `None` — the caller falls back to the CONFIGURED pooled
/// law (path-scaled / legacy) — unless the gate is on, N ≥ 2, and EVERY live
/// path's anchor is warm (a partial sum would under-provision the unwarm
/// path's share of the shared pool).
pub fn capw_store_cap(
    on: bool,
    terms: &[Option<f64>],
    floor: usize,
    pool: usize,
) -> Option<usize> {
    if !on || terms.len() < 2 || terms.iter().any(|t| !matches!(t, Some(v) if *v > 0.0)) {
        return None;
    }
    let n = terms.len();
    let sum: f64 = terms.iter().map(|t| t.unwrap_or(0.0)).sum();
    let ceiling = n.saturating_mul(pool).max(floor);
    Some((sum.ceil() as usize).clamp(floor, ceiling))
}

/// Per-path outstanding-account cap (task #86, env `RWM_STORE_PERCAP`).
///
/// The #84 residual, named at L1: ONE shared pool cannot be sized for a
/// c2-deep (fast) and a c3-shallow (slow) path at once — the slow path's
/// recovery latency scales with pool dwell (static 8192 collapsed it to
/// 31.8 Mbit/s) while the fast path wants the depth. So each path gets its
/// OWN account, sized to ITS pipe by Little's law on the store itself:
///
///   cap_i = clamp(gain × rate_i × echoRTT_i, floor, pool)
///
/// where `pipe_i` = rate_i × echoRTT_i is passed in by the caller —
/// BtlBw_i (the per-path delivered-rate anchor) × that path's smoothed
/// ack-ECHO RTT (NOT RTprop: the store drains at the ack clock, so the
/// account's residence time includes the queue + ack path; the `pool`
/// ceiling — the measured 2048-per-path knee — bounds the echo-RTT
/// positive feedback). Under the Copa-sole feed the caller passes cwnd_i
/// (Copa's operating point IS the per-path pipe, mirroring the pooled
/// Σcwnd law).
///
/// Warm-up (`pipe_i` = None / non-positive, the anchor not yet
/// established): inherit an equal share of the LEGACY pooled cap
/// (`legacy_cap` / n_live, bounded to [floor, pool]) — converges to the
/// derived cap as the anchor warms. The per-path in-flight cap
/// (`infl_percap_full`, the FMTCP-era #64 fix) is the structural pattern,
/// generalized here to the plain-reliable retention store.
///
/// N = 1 bit-exactness is CALLER-side: the percap law is only engaged for
/// N ≥ 2 live paths (this function is never consulted at N = 1), so
/// singles keep the legacy pooled law even with the flag ON.
pub fn percap_store_cap(
    pipe_i: Option<f64>,
    legacy_cap: usize,
    n_live: usize,
    gain: f64,
    floor: usize,
    pool: usize,
) -> usize {
    let ceiling = pool.max(floor);
    match pipe_i {
        Some(p) if p > 0.0 => ((gain * p).ceil() as usize).clamp(floor, ceiling),
        _ => (legacy_cap / n_live.max(1)).clamp(floor, ceiling),
    }
}

/// Per-path admission gate (task #86): TUN intake is paused only when NO
/// path's outstanding account has headroom below its own cap — one path's
/// full (or recovery-stalled) account never starves another path's
/// admission. `accounts` = (outstanding_i, cap_i) per live path. The exact
/// mirror of [`infl_percap_full`] for the retention store.
pub fn percap_store_full(accounts: &[(usize, usize)]) -> bool {
    !accounts.iter().any(|&(out, cap)| out < cap.max(1))
}

/// Delay-aware redirect bound (roadmap item 1, the #86 c8 fix): the maximum
/// outstanding a cap-full redirect may find on target path j before the
/// redirect is refused and the store reads FULL for the placement instead.
///
/// Derivation (not tuned). The projected dwell of account j is Little's law
/// on the store, D_j = out_j / rate_j (the store drains at the ack clock).
/// The guard law is D_j ≤ κ·echoRTT_j — "j can drain its current account
/// within one echo round". But the app-echo clock is store-dwell-INCLUSIVE:
/// echoRTT_j ≈ RTprop_j + D_j, so on the LOADED echo clock κ = 1 is vacuous
/// (D ≤ RTprop + D holds for every D — exactly the measured c8 feedback,
/// where slow-path echo inflation to 214–811 ms held the account open).
/// Solving D ≤ κ·(RTprop_j + D) for κ < 1 gives D ≤ (κ/(1−κ))·RTprop_j;
/// κ = 1/2 (the redirected symbol must still clear within one round AFTER
/// its own dwell has inflated the echo) gives D ≤ RTprop_j — equivalently
/// κ = 1 on the FLOOR clock:
///
///   bound_j = rate_j × RTprop_j  (the path's honest BDP in symbols)
///
/// i.e. a redirect may never park more than one un-queued pipe on the
/// target. Since cap_j = gain·rate_j·echoSRTT_j (gain 2 = pipe + recovery
/// runway), the guard reserves the runway term AND any knee-clamp headroom
/// (the plain-anchor over-read case) for the path's OWN traffic — redirects
/// consume only the floor-clocked pipe term. Under the Copa-sole feed the
/// caller passes cwnd_j (Copa's operating point IS the bounded-queue pipe).
/// Warm-up (no anchor): cap_j/gain — the same "pipe term only" law applied
/// to the inherited share. Clamped to [1, cap_j]: a one-symbol quantum so a
/// cold account is never permanently redirect-closed, and never above the
/// account's own cap.
pub fn percap_redirect_bound(floor_pipe: Option<f64>, cap_i: usize, gain: f64) -> usize {
    let ceiling = cap_i.max(1);
    match floor_pipe {
        Some(p) if p > 0.0 => (p.ceil() as usize).clamp(1, ceiling),
        _ => ((cap_i as f64 / gain.max(1.0)).ceil() as usize).clamp(1, ceiling),
    }
}

/// Windowed-MIN echo-ratio tracker (feat/percap-honest-cap): K_i = the
/// smallest observed echoSRTT_i/RTprop_i over a ~2-half-window (~10 s, the
/// min-RTT window class) — the path's UNLOADED drain-clock ratio.
///
/// Why the MIN: the app-echo clock is store-dwell-inclusive (echoSRTT ≈
/// RTprop + own-queue dwell + ack-path/batching overhead), so any loaded
/// statistic of the ratio is self-referential — the store's own queue
/// inflates it, which inflates the cap, which deepens the queue (the
/// measured c8 parking spiral, GUARD RESULTS). The windowed MIN is
/// self-queue-PROOF: own dwell can only raise the ratio, so the smallest
/// sample in the window is the honest ack-path/batching overhead with the
/// least self-queue contamination. Anchor-hygiene rule 3 applies: it is a
/// windowed statistic, not a latched constant — the window rolls (two
/// half-window buckets), so a stale unloaded read expires and the ratio
/// re-measures.
pub struct EchoRatioMin {
    cur: f64,
    prev: f64,
    start_us: u64,
    half_us: u64,
}

impl EchoRatioMin {
    pub fn new(half_us: u64) -> Self {
        Self { cur: f64::INFINITY, prev: f64::INFINITY, start_us: 0, half_us: half_us.max(1) }
    }
    /// Feed one ratio sample (echoSRTT/RTprop, clamped ≥ 1 — a smoothed
    /// echo transiently below the windowed-min floor is clock noise, not a
    /// sub-floor drain) and return the current windowed min.
    pub fn observe(&mut self, ratio: f64, now_us: u64) -> f64 {
        if self.start_us == 0 {
            self.start_us = now_us;
        }
        if now_us.saturating_sub(self.start_us) >= self.half_us {
            self.prev = self.cur;
            self.cur = f64::INFINITY;
            self.start_us = now_us;
        }
        if ratio.is_finite() && ratio > 0.0 {
            self.cur = self.cur.min(ratio.max(1.0));
        }
        self.k()
    }
    /// The current windowed-min ratio (1.0 before any sample).
    pub fn k(&self) -> f64 {
        let m = self.cur.min(self.prev);
        if m.is_finite() { m } else { 1.0 }
    }
    /// Feed one echoSRTT/RTprop observation from raw clocks and return the
    /// windowed min. SEED-IDENTITY GUARD: at the estimator's seed instant
    /// the smoothed echo IS the windowed-min sample (bit-equal, ratio ≡ 1)
    /// — an artifact of shared seeding, not a drain-clock measurement.
    /// Feeding it would latch the windowed min at 1.0 for a whole window
    /// (measured at the L1 smoke: khr pinned 1.00 while rtt/rtp read
    /// 16/8 ms). Samples where srtt − RTprop ≤ 5 µs are DISCARDED, not
    /// clamped — no measurement, no sample.
    pub fn observe_srtt_over_rtprop(
        &mut self,
        srtt: Duration,
        rtprop: Option<Duration>,
        now_us: u64,
    ) -> f64 {
        if let Some(rtp) = rtprop {
            let rtp_s = rtp.as_secs_f64();
            let srtt_s = srtt.as_secs_f64();
            if rtp_s > 0.0 && srtt_s - rtp_s > 5e-6 {
                return self.observe(srtt_s / rtp_s, now_us);
            }
        }
        self.k()
    }
}

/// The recovery engine's per-round latency ceiling, in seconds — the
/// hole-refresh / tail-sweep cadence clamp (`HOLE_NACK_REFRESH_MAX` =
/// `TAIL_SWEEP_MAX_US` = 100 ms). A stalled hole in plain window mode is
/// recovered by the SACK re-advertisement + tail-sweep engine, whose round
/// runs on THIS clock (2×SRTT clamped [25, 100] ms), not on RTprop — at a
/// short-RTprop cell (c2: RTprop ≈ 8 ms) a recovery round is ~12× the wire
/// round trip. The honest cap's runway term must fund it (see
/// [`honest_store_cap`]); using the CLAMP CEILING is the honest worst
/// round: GE burst loss routinely drives the engine to it (retransmits
/// re-lost in the same bad state, sweep-cadence refills — MEASURED: sweeps
/// every ~140 ms live at the c2 cell).
pub const HONEST_RECOVERY_ROUND_S: f64 = TAIL_SWEEP_MAX_US as f64 / 1e6;

/// Honest store cap (feat/percap-honest-cap, the GUARD-RESULTS residual (i)
/// fix): the outstanding cap derived on the HONEST plain-mode anchor
/// (`RWM_PLAIN_RS`), replacing both the knee-clamp fallback that the legacy
/// anchor over-read forced AND the loaded-echo-clock cap law whose
/// dwell→echo→cap feedback parked the c8 slow path.
///
/// Derivation (Little's law on the retention store, decomposed on honest
/// clocks; every term measured or a named engine constant — none
/// inflatable by the store's own queue):
///
///   - a retained symbol's UNLOADED residence is K·RTprop (K = the
///     windowed-min echoSRTT/RTprop ratio, [`EchoRatioMin`] — the measured
///     ack-path/batching overhead; the loaded echo is self-referential and
///     is used NOWHERE in this cap). Sustaining rate_i needs
///     rate_i·K_i·RTprop_i outstanding — the RESIDENCE term;
///   - a hole strands the in-order frontier for one recovery round, which
///     runs on the RECOVERY engine's clock, not the wire's: the SACK
///     re-advertisement / tail-sweep cadence bound R = 100 ms
///     ([`HONEST_RECOVERY_ROUND_S`]) plus the retransmit flight RTprop_i.
///     Keeping the pipe fed across it needs (gain−1) rounds of runway —
///     the RUNWAY term (gain 2.0 = 1 round, the same pipe+runway
///     decomposition as the redirect guard):
///
///   cap_i = rate_i·(K_i·RTprop_i + (gain−1)·(R + RTprop_i))
///         = anchor_i·(K_i + gain − 1) + rate_i·(gain−1)·R
///
/// where `anchor_i` = BtlBw_i×RTprop_i (`copa_bdp_anchor`). The legacy
/// floor law gain·anchor_i is the R = 0, K = 1 degenerate — the honest form
/// strictly widens it (K ≥ 1, R > 0), so honest anchors can never shrink a
/// cap below the legacy law: the headroom the anchor over-read supplied by
/// accident (~12× at c2's 8-ms RTprop — the sc2 −20% datum, "Anchor
/// Hygiene" battery (b)) is now supplied EXPLICITLY from the engine's own
/// recovery cadence + the measured echo-ratio. Cross-checks against
/// independently measured good operating points: sc2 → 10.4k·(K·8ms +
/// 108ms) ≈ 1250+ → latches the legacy-proven 1024 store; c8-slow →
/// ~2k·(K·60ms + 160ms) ≈ 470–500 ≈ the guard session's measured good pin
/// (508, dwell 0.26 s); the c8 knee-parking regime (2048, dwell ≈ 1 s)
/// is unreachable for a c3-class rate. Caller clamps to the principled
/// [floor, knee/store] bounds; warm-up (no anchor) returns None and the
/// caller keeps the legacy warm-up share.
pub fn honest_store_cap(
    anchor_bdp: Option<f64>,
    rate: Option<f64>,
    k_ratio: f64,
    gain: f64,
) -> Option<f64> {
    match (anchor_bdp, rate) {
        (Some(a), Some(r)) if a > 0.0 && r > 0.0 => {
            let runway_rounds = (gain - 1.0).max(0.0);
            Some(
                a * (k_ratio.max(1.0) + runway_rounds)
                    + r * runway_rounds * HONEST_RECOVERY_ROUND_S,
            )
        }
        _ => None,
    }
}

/// feat/window-mtu part 1 (`RWM_WIN_DECOUPLE`, goal-gate "Window Decoupling
/// + MTU Scaling"): the retention/memory ceiling once the window and the
/// inflight are decoupled — 4096 × ~1.2 KB ≈ 5 MB. The legacy 1024 latch's
/// memory role only; the wire budget is `win_decouple_allow`.
pub const WIN_STORE_MAX: usize = 4096;

/// The stall-insurance meter's ceiling: one recovery-engine round on the
/// sweep-cadence clamp — the SAME named constant the honest cap's runway
/// uses ([`HONEST_RECOVERY_ROUND_S`]). Fixed by the 2026-08-06 diagnosis
/// amendment (R_ins = R).
pub const WIN_STALL_INS_S: f64 = HONEST_RECOVERY_ROUND_S;

/// The decoupled WIRE budget (part 1 law; diagnosis-amended constants):
///
///   allow = base + rate·min(stall_age, R_ins)
///
/// where `base` = anchor·(K + gain − 1) (residence on the measured unloaded
/// clock + probe headroom; under Copa-sole the caller passes gain·Σcwnd)
/// and the metered term is the stall insurance made EXPLICIT and
/// CONTINUOUS: the 2026-08-06 diagnosis refuted all three pre-registered
/// insurance channels (holes ≤ 7% of any window; release gaps 2–12 ms;
/// no multi-round tail) and named SUB-SWEEP ACK-GRANULARITY COVER — a
/// right-sized static window is consumed by its own queue (Little's law:
/// zero slack), so every frontier micro-freeze idles the wire. Here the
/// allowance grows at exactly the anchor rate while the frontier is
/// frozen (micro or sweep scale alike, no threshold, no mode bit) and
/// falls back to `base` when it advances.
pub fn win_decouple_allow(base: f64, rate: f64, stall_age_s: f64) -> f64 {
    base + rate.max(0.0) * stall_age_s.clamp(0.0, WIN_STALL_INS_S)
}

/// The decoupled RETENTION backstop (part 1 law): the un-SACKed total —
/// head span PLUS recovery-stalled holes — may reach the full metered
/// allowance plus one recovery round of hole capacity (N_hole = 1, from
/// the diagnosis: hole population ≤ 70 everywhere), memory-clamped.
pub fn win_decouple_cap_ret(base: f64, rate: f64, rtprop_s: f64, floor: usize) -> usize {
    let r = rate.max(0.0);
    ((base + r * (WIN_STALL_INS_S + HONEST_RECOVERY_ROUND_S + rtprop_s.max(0.0))).ceil()
        as usize)
        .clamp(floor.min(WIN_STORE_MAX), WIN_STORE_MAX)
}

/// Guard-aware admission gate (roadmap item 1). `accounts` = (outstanding_i,
/// cap_i, redirect_bound_i) per live path. Three regimes:
///
/// - every account cap-full → FULL (the unguarded law, unchanged);
/// - no account cap-full → admit (every pick places on its own account);
/// - SOME account cap-full → a pick landing there must redirect, so
///   admission stays open only while a guard-eligible target exists
///   (out_j < min(cap_j, bound_j)) — otherwise the store reads FULL and the
///   existing admission pause engages: backpressure, don't park. (The #73
///   lesson does NOT recur here: the pause path is the battery-proven
///   percap admission gate, not a new deferral mechanism.)
///
/// With bound_j = cap_j (guard off) this degenerates exactly to
/// [`percap_store_full`].
pub fn percap_store_full_guarded(accounts: &[(usize, usize, usize)]) -> bool {
    let any_open = accounts.iter().any(|&(out, cap, _)| out < cap.max(1));
    if !any_open {
        return true;
    }
    let any_capfull = accounts.iter().any(|&(out, cap, _)| out >= cap.max(1));
    if !any_capfull {
        return false;
    }
    !accounts
        .iter()
        .any(|&(out, cap, bound)| out < cap.min(bound).max(1))
}

/// Per-path placement redirect (task #86 + roadmap item 1): the admission
/// gate only admits while a placement is possible — make it land there.
/// Keeps `chosen` when its OWN account is below its cap (the guard gates
/// redirects only, never a path's own picks); otherwise redirects to the
/// guard-eligible path (out < min(cap, redirect_bound) — see
/// [`percap_redirect_bound`]: the target must be able to drain its account
/// within one floor-clock echo round, so a redirect never parks symbols
/// behind a standing queue) with the most RELATIVE headroom. No eligible
/// target (all-full, or every open account past its dwell bound — racing
/// the guarded gate): keep `chosen` (the gate reads FULL and pauses intake
/// next iteration; the slop is one placement). `accounts` =
/// (path, outstanding_i, cap_i, redirect_bound_i); bound = cap is the
/// unguarded legacy redirect.
pub fn percap_place_path(
    chosen: crate::scheduler::PathId,
    accounts: &[(crate::scheduler::PathId, usize, usize, usize)],
) -> crate::scheduler::PathId {
    if accounts
        .iter()
        .any(|&(p, out, cap, _)| p == chosen && out < cap.max(1))
    {
        return chosen;
    }
    accounts
        .iter()
        .filter(|&&(_, out, cap, bound)| out < cap.min(bound).max(1))
        .max_by(|a, b| {
            let h = |&(_, out, cap, bound): &(crate::scheduler::PathId, usize, usize, usize)| {
                1.0 - out as f64 / cap.min(bound).max(1) as f64
            };
            h(a).partial_cmp(&h(b)).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|&(p, _, _, _)| p)
        .unwrap_or(chosen)
}

/// One live path's account state as the bounded-borrowing law sees it
/// (feat/store-borrowing, paper §16.22).
#[derive(Clone, Copy, Debug)]
pub struct BorrowAccount {
    pub path: u32,
    /// Account occupancy: symbols CHARGED to this path (own + lent-out).
    pub out: usize,
    /// The account's derived cap (honest law / legacy percap law).
    pub cap: usize,
    /// Pipe occupancy: symbols FLYING on this path
    /// (= out − lent + borrowed, corrected by the loan ledger).
    pub fly: usize,
    /// Honest drain rate, sym/s (BtlBw_i under RWM_PLAIN_RS;
    /// cwnd_i/RTprop_i under the Copa-sole feed). None = warm-up.
    pub rate: Option<f64>,
    /// RTprop_i (windowed-min floor clock), seconds. None = warm-up.
    pub rtprop_s: Option<f64>,
}

/// The loan's return latency (paper §16.22.2): the borrowed symbol's
/// expected residence on the BORROWER's pipe, on the floor clock —
/// queue drain plus one flight: T_return(j) = fly_j/rate_j + RTprop_j.
/// None on warm-up (an unmeasured borrower admits no loans).
pub fn percap_t_return(borrower: &BorrowAccount) -> Option<f64> {
    match (borrower.rate, borrower.rtprop_s) {
        (Some(r), Some(rtp)) if r > 0.0 && rtp >= 0.0 => {
            Some(borrower.fly as f64 / r + rtp)
        }
        _ => None,
    }
}

/// The bounded-borrowing law (paper §16.22.2, derived not tuned):
///
///   lend_i→j ≤ max(0, cap_i − out_i − rate_i·T_return(j))
///
/// — lend only headroom the lender cannot use within the loan's return
/// latency (the lender's intake is bounded by its own drain rate, so
/// rate_i·T_return is everything it could possibly place while the loan
/// is out; reserving it yields the post-loan solvency invariant
/// cap_i − out_i ≥ rate_i·T_return). Warm-up on EITHER side lends
/// nothing — the degenerate is isolation, not the pool. The reservation
/// term is what separates this from the pooled law (T_return := 0 ⇒
/// lend up to cap_i − out_i ⇒ pooled Σcap sharing), and it makes lending
/// one-directional at asymmetric cells: a fast lender's reservation
/// toward a slow pipe exceeds its whole cap (rate_i·T_return(slow) ≫
/// cap_i), so the #86 parking direction is unrepresentable.
pub fn percap_lend_room(lender: &BorrowAccount, borrower: &BorrowAccount) -> usize {
    let Some(t_return) = percap_t_return(borrower) else {
        return 0;
    };
    let Some(rate_i) = lender.rate.filter(|r| *r > 0.0) else {
        return 0;
    };
    let reservation = (rate_i * t_return).ceil() as usize;
    lender
        .cap
        .saturating_sub(lender.out)
        .saturating_sub(reservation)
}

/// Pick the lender for a pick landing on cap-full borrower `borrower`:
/// the live sibling with the largest lend room (> 0). None = no loan
/// admissible (the caller falls through to the guarded redirect, then to
/// backpressure).
pub fn percap_borrow_lender(borrower: u32, accounts: &[BorrowAccount]) -> Option<u32> {
    let b = accounts.iter().find(|a| a.path == borrower)?;
    accounts
        .iter()
        .filter(|a| a.path != borrower)
        .map(|a| (a.path, percap_lend_room(a, b)))
        .filter(|&(_, room)| room > 0)
        .max_by_key(|&(_, room)| room)
        .map(|(p, _)| p)
}

/// True when some cap-full borrower j has an open lend edge from some
/// lender i (the admission-gate extension: the store is FULL only when
/// the guarded gate reads full AND no loan is admissible — paper
/// §16.22.4).
pub fn percap_lend_edge_exists(accounts: &[BorrowAccount]) -> bool {
    accounts
        .iter()
        .filter(|b| b.out >= b.cap.max(1))
        .any(|b| {
            accounts
                .iter()
                .filter(|a| a.path != b.path)
                .any(|a| percap_lend_room(a, b) > 0)
        })
}

/// Record one loan in the ledger: `seq` flies on `flyer`, is charged to
/// `lender` (the caller performs the actual `percap_charge` to the
/// lender). Gauges: `lent[lender]` and `borrowed[flyer]` correct the
/// account occupancy into pipe occupancy (fly = out − lent + borrowed).
pub fn percap_loan_charge(
    loans: &mut BTreeMap<u64, (u32, u32)>,
    lent: &mut std::collections::HashMap<u32, usize>,
    borrowed: &mut std::collections::HashMap<u32, usize>,
    seq: u64,
    lender: u32,
    flyer: u32,
) {
    if loans.insert(seq, (lender, flyer)).is_none() {
        *lent.entry(lender).or_insert(0) += 1;
        *borrowed.entry(flyer).or_insert(0) += 1;
    }
}

/// Repay one loan on the ack that releases `seq` (SACK/OOO twin of
/// [`percap_release_seq`]; idempotent the same way).
pub fn percap_loan_release(
    loans: &mut BTreeMap<u64, (u32, u32)>,
    lent: &mut std::collections::HashMap<u32, usize>,
    borrowed: &mut std::collections::HashMap<u32, usize>,
    seq: u64,
) {
    if let Some((lender, flyer)) = loans.remove(&seq) {
        if let Some(l) = lent.get_mut(&lender) {
            *l = l.saturating_sub(1);
        }
        if let Some(b) = borrowed.get_mut(&flyer) {
            *b = b.saturating_sub(1);
        }
    }
}

/// Repay every loan at or below the cumulative ack (the
/// [`percap_release_cumulative`] twin).
pub fn percap_loan_release_cumulative(
    loans: &mut BTreeMap<u64, (u32, u32)>,
    lent: &mut std::collections::HashMap<u32, usize>,
    borrowed: &mut std::collections::HashMap<u32, usize>,
    ack: u64,
) {
    let keep = loans.split_off(&(ack + 1));
    for (lender, flyer) in loans.values() {
        if let Some(l) = lent.get_mut(lender) {
            *l = l.saturating_sub(1);
        }
        if let Some(b) = borrowed.get_mut(flyer) {
            *b = b.saturating_sub(1);
        }
    }
    *loans = keep;
}

/// Charge one retained seq to its placement path's outstanding account
/// (task #86). Called in lockstep with the `sent_store` insert; paired
/// with [`percap_release_seq`] (SACK/OOO removal) and
/// [`percap_release_cumulative`] (frontier advance) — release is by ack
/// ONLY, exactly the retention contract.
pub fn percap_charge(
    acct: &mut BTreeMap<u64, u32>,
    out: &mut std::collections::HashMap<u32, usize>,
    seq: u64,
    path: u32,
) {
    if acct.insert(seq, path).is_none() {
        *out.entry(path).or_insert(0) += 1;
    }
}

/// Release one seq from its account on OOO (SACK-range) removal from the
/// retention store. Idempotent: a seq not (or no longer) in the account
/// map releases nothing — so SACK + cumulative can never double-release.
pub fn percap_release_seq(
    acct: &mut BTreeMap<u64, u32>,
    out: &mut std::collections::HashMap<u32, usize>,
    seq: u64,
) {
    if let Some(pid) = acct.remove(&seq) {
        if let Some(o) = out.get_mut(&pid) {
            *o = o.saturating_sub(1);
        }
    }
}

/// Release every account entry at or below the cumulative ack (the
/// in-order frontier advance — the `sent_store.split_off(ack+1)` twin).
pub fn percap_release_cumulative(
    acct: &mut BTreeMap<u64, u32>,
    out: &mut std::collections::HashMap<u32, usize>,
    ack: u64,
) {
    let keep = acct.split_off(&(ack + 1));
    for pid in acct.values() {
        if let Some(o) = out.get_mut(pid) {
            *o = o.saturating_sub(1);
        }
    }
    *acct = keep;
}

/// SACK-clocked store release (env `RWM_STORE_SACK_RELEASE`, goal-gate
/// "SACK-Clocked Store Release" pre-registration): mark every seq of the
/// SACK range that is currently retained as RELEASED — uncounted from the
/// flow-control outstanding, so the send window opens at path rate instead
/// of frontier latency — while the `sent_store` entry (the ONLY payload
/// copy; `retransmit_buffer` is metadata-only) and every ARQ/recovery map
/// stay untouched until the cumulative frontier passes the seq.
///
/// THE `RWM_SACK_PRUNE` DISTINCTION, BY CONSTRUCTION (that experiment was
/// refuted UNSAFE 2026-07-07: pruning `sent_store` on SACK destroyed the
/// only copy of a received-then-EVICTED symbol → C7/C8 in-order DNF): this
/// law never removes anything. A released symbol remains retransmittable
/// (NACK path serves from `sent_store.get`); worst case under receiver
/// eviction is a wasted retransmit, not a wedge. The race-ahead is bounded
/// because never-received/evicted seqs are never SACKed and still count.
///
/// Returns the seqs NEWLY released by this call (for per-path account
/// release); already-released seqs are skipped — no double-release.
pub fn sack_release_mark<V>(
    sent_store: &BTreeMap<u64, V>,
    released: &mut BTreeSet<u64>,
    start: u64,
    end: u64,
) -> Vec<u64> {
    let mut newly = Vec::new();
    for (&seq, _) in sent_store.range(start..=end) {
        if released.insert(seq) {
            newly.push(seq);
        }
    }
    newly
}

/// The cumulative-frontier twin of [`sack_release_mark`] (the
/// `sent_store.split_off(&(ack+1))` pattern): drop released marks at or
/// below the ack — those slots are now FULLY freed (payload gone from the
/// store, mark gone from the released set).
pub fn sack_release_prune(released: &mut BTreeSet<u64>, ack: u64) {
    *released = released.split_off(&(ack + 1));
}

/// Effective outstanding under the release law: retained minus released.
/// With the gate off the released set is empty and this is exactly
/// `store_len` — the shipped gate unchanged.
pub fn sack_release_outstanding(store_len: usize, released: usize) -> usize {
    store_len.saturating_sub(released)
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
    nack_rx: &mut tokio::sync::mpsc::Receiver<Vec<(u64, u64)>>,
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
    // The engine's env-gate surface, resolved once in run_impl (src/gates.rs).
    gates: crate::gates::RuntimeGates,
) {
    // Generation coding emits coded wire symbols exactly like coded-only; the
    // difference is the coding UNIT (a stable generation vs the moving window)
    // and that per-seq ARQ is disabled below.
    let coded_wire = coded_only || generation;
    let gen_size: usize = gates.gen_size;
    let pipeline: usize = gates.pipeline;
    // Generation-coding proactive overhead r (coded per generation beyond K_G):
    // the encoder provisions each generation to ceil(len·(1+r)) coded before it
    // is only coded for recovery. Covers loss + the MDS margin. RWM_GEN_R env.
    // Systematic-repair provisions only the loss-FEC overhead r (the K base DoF
    // ride the wire as source), so its natural default is smaller than
    // coded-only's (which must also fund the K base). r ≳ 1.5·ε keeps windowed
    // repair ahead of loss (the oracle's provisioning floor; r < ε → DNF). At C8
    // ε_slow ≈ 4.8 %, so 0.15 clears both paths with margin. RWM_GEN_R overrides.
    // The DAPS chain (RWM_DAPS, _BDP, _PACE, RWM_PACE_ALL, RWM_RATE_SAMPLE,
    // RWM_PER_PATH_EST, RWM_DAPS_DEPTH) was REMOVED 2026-07-27 per the
    // DEPRECATION REGISTER (ADR-0065/0066): the original 2026-07-12 arc was
    // voided by the Methodology Audit (generation-inert era), and the live
    // re-ask ("Gen-ON Stack Ablation" 2026-07-13, generation actually ON)
    // measured the stack itself as the sym-C7 collapse (rate-sample −22%,
    // depth −17…−30%). Every surviving idea was re-derived better elsewhere:
    // per-path BDP cap + derived depth → RWM_GEN_PIPE's M* law (ADR-0064),
    // honest per-path anchors → ADR-0061 (the send-interval sampler the
    // CopaFeed/RWM_PLAIN_RS machinery keeps IS that fix — shared, retained),
    // per-path admission → the percap family (ADR-0058).
    // The RWM_FMTCP(+_WIN) decode-on-total composite was REMOVED 2026-07-27
    // per the DEPRECATION REGISTER: re-tested on the FULL clean substrate
    // ("C8-Aware Pool Law" battery, piggybacked arm) → CONFIRMED-REFUTED
    // (c7 18.30/18.98, c8 14.30/15.03 Mbit/s = ×0.11–0.20 of the same-session
    // default stack, both seeds, ≫σ) — the 2026-07-08 pathology was never
    // wall-tainted. Its forced sub-levers (RWM_REASM_BDP, RWM_OOO_RETAIN,
    // RWM_XPATH_REPAIR) survive as independent gates; the per-path in-flight
    // cap + derived win backstop it pioneered live on under gen_pipe/M*.
    // feat/gen-substrate-ceiling (RWM_GEN_PIPE, DEFAULT OFF ⇒ same-binary A/B;
    // shipped non-generation default byte-identical — every use is generation-
    // gated). The JOB-1 diagnosis: the L1 per-path ~10 Mbit/s generation
    // ceiling is the SUBSTRATE — quinn's loss-reactive Cubic window under the
    // datagram path (per connection = per path), COLLAPSED further by bare
    // generation mode's own standing queue (uncapped in-flight → RTT inflated
    // 3–5× → Cubic throughput ∝ 1/RTT). The L0 netem-shim bench (which
    // reproduces RTT/rate/GE-loss but hides them from quinn) measures the app
    // machine at 34 Mbit/s on the same c2 parameters — the wall is NOT the
    // app pipeline. This gate composes the app-side remedies so the substrate
    // sees a queue-lean, BDP-covering pipeline:
    //   1. per-path BDP in-flight cap (infl_bdp 1.5, percap) — queue ≈ 0,
    //      RTT ≈ RTprop (the mechanism behind DAPS's accidental +44% single);
    //   2. DERIVED pipeline depth M* (gen_pipe_depth above, #61's A*) —
    //      generations in flight cover BDP + one deficit round, recomputed
    //      from measured rate/SRTT (no fixed M);
    //   3. coded-emission budget clocked on the SENT frontier (the stalled
    //      cumulative ack must not freeze emission for the still-recovering
    //      oldest generation while M* fresh generations have budget);
    //   4. pace anchored to the windowed-MAX delivered rate (§16.15: the
    //      decode-clocked samples are mostly-low; the legacy decaying EWMA
    //      under-reads between generation decodes and throttles emission);
    //   5. once-per-SRTT deficit action (react_cap 1.0 — the known-good
    //      bounded reactive from the FMTCP-era arm).
    // The substrate CC itself is A/B-able independently via RWM_QUIC_CC (bbr)
    // in transport/quic.rs.
    // §16.20 (d): under RWM_UNIFIED the derived-depth law (M* =
    // ceil(rate·2·RTprop/G)+1, the large-δ limit of A*) is the DEFAULT for
    // generation mode; RWM_GEN_PIPE=0 still reproduces the fixed legacy
    // pipeline as the same-binary A/B arm.
    let gen_pipe = gates.gen_pipe && generation;
    // feat/anchor-hygiene (`RWM_MSTAR_ANCHOR`): the M* anchor-pair repair —
    // (a) the peer-report 50-ms pseudo-sample no longer pins the RTprop floor
    // (PathReport arm; hygiene rules 1+3), (b) the windowed-MAX delivered-rate
    // filter seeds from 500-ms buckets instead of 2-s ones (rule 1: the
    // anchor is live within ~1 bucket of the first acks). (Historic rule (c),
    // the derived (M*+2)·G replacement of the STATIC FMTCP win backstop, was
    // removed with the FMTCP composite 2026-07-27 — the derived-depth idea
    // ships as gen_pipe's M* law itself.)
    // DEFAULT ON (2026-07-21, "Consolidation" battery: plain subset inert
    // within sigma at every bulk cell on both seeds, tail crown unregressed;
    // the generation-gated knee evidence is 16.21's).
    let mstar_anchor = gates.mstar_anchor && generation;
    if mstar_anchor {
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
    // feat/source-backpressure (RWM_SRC_BP) — REMOVED 2026-07-27 per the
    // DEPRECATION REGISTER: deferring the source into per-path pacing budgets
    // stalls the generation-fill pipeline (the source read IS the pipeline
    // clock — C8 −53% both seeds, "Source Backpressure" 2026-07-12; era
    // audit-classified UNCERTAIN). The mechanism space (per-path admission of
    // source) was re-asked BY the percap account family on live code with
    // gauges and lost for a named structural reason (ADR-0058); any future
    // gen-mode re-ask rides that family, not this code.
    // Generation-coding proactive overhead r.
    let gen_repair_floor: f64 = gates
        .gen_r
        .unwrap_or(if systematic { 0.15 } else { 0.20 })
        .clamp(0.0, 2.0);
    // Codec pinned at startup (§16.4) — created once, never rebuilt.
    let mut encoder: Box<dyn WindowEncoder> = if systematic {
        Box::new(crate::fec::GenerationEncoder::new_systematic(symbol_size, gen_size, pipeline, gen_repair_floor))
    } else if generation {
        Box::new(crate::fec::GenerationEncoder::new(symbol_size, gen_size, pipeline, gen_repair_floor))
    } else {
        create_window_encoder(fec_backend, symbol_size)
    };
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
    let mut gen_last_source_us: u64 = now_us(); // last source-intake time
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
    let mut gen_pipe_store_cap: usize = 2 * gen_size;
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
    let mut gen_trace_last_us: u64 = 0;
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
    let gen_rate: f64 = gates.gen_rate;
    // Bootstrap pacing floor (symbols/sec): the rate used before the ack-rate
    // estimator has a sample (primes the first generation). Kept modest so the
    // startup burst can't overrun a bandwidth-limited link's datagram intake;
    // once the ack rate is known the pacing clocks to delivered goodput × 1.5.
    let gen_rate_floor: f64 = gates.gen_rate_floor;
    // ── Fix 1 (transport-substrate): CC-RATE PACING of the SYSTEMATIC SOURCE ──
    // PRIMARY high-RTT lever. The systematic source rides the DROPPABLE QUIC-
    // datagram path driven only by TUN-read intake, gated by a BDP-scaled
    // WINDOW (store_max / infl_cap) but NOT by a RATE. At high RTT the window is
    // BDP-sized, so the source is spent as one big BURST that netem/QUIC drops
    // faster than the receiver decodes — per-generation loss then exceeds the
    // ceil(len·r) proactive budget and the proactive-recovery fraction
    // COLLAPSES (0.95→0.23), forcing reactive round-trips (goal-gate "Proactive
    // FEC vs ARQ"). This paces the source at the measured LINK rate, smoothed
    // over the RTT with a SMALL burst, so no BDP-sized burst ever hits the wire.
    //
    // Rate signal: the delivered-goodput EWMA (`gen_rate_ewma`) is the achieved
    // BtlBw in generation mode — the true CC anchor. The Copa `cwnd` is NOT
    // usable here: window-mode WindowAcks do not drive `record_delivery`, so
    // cwnd is pinned at INITIAL_CWND and cwnd/SRTT would strangle the pipe. The
    // ack-clocked delivered-goodput EWMA already tracks the link and is what the
    // coded bucket uses; the source now shares it. A small headroom lets the
    // rate ramp without the 1.5× overshoot that itself overruns the datagram
    // path. Env-gated (RWM_CC_PACE) so the A/B baseline is byte-identical.
    //
    // feat/copa-wire-signal: DEFAULT ON under the wire-clocked Copa signal.
    // Copa's model assumes a PACED wire (the paper paces at 2·cwnd/RTT; our
    // §12.5 token bucket does the same for the block path), but under
    // RWM_QUIC_CC=passthrough quinn's own pacer derives from the engine
    // window — at Copa's Bulk operating point (cwnd ≈ BDP + 1/δ ≈ 5×BDP at
    // c2) that pacer never binds, the send process degrades to pure
    // ack-clocking, and each GE loss burst's recovery micro-stall idles the
    // bottleneck (MEASURED at the L1 c2 smoke: 55.7 → 67 Mbit/s from this
    // default alone, store no longer pinned at the cap, wire queue p50
    // 3–5 ms). RWM_CC_PACE=0 still forces it off (the #80 A/B arms are
    // reproduced by RWM_COPA_WIRE=0, under which this default is false).
    let cc_pace = gates.cc_pace;
    let cc_pace_headroom: f64 = gates.cc_pace_headroom;
    // Source pacing token bucket (symbols). Refilled at the link rate each loop
    // iteration; the TUN-read select branch is gated on a token being available
    // and one token is consumed per source symbol put on the wire.
    let mut src_tokens: f64 = 0.0;
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
    let mut cc_rate_ceiling: f64 = gen_rate;
    // ── Fix 2 (transport-substrate): BOUNDED REACTIVE under congestion control ─
    // The deficit-driven recovery loop was EXEMPT from the in-flight congestion
    // cap and re-emitted the reported residual on EVERY deficit report. At high
    // RTT the reports are ~RTT stale, so it re-sends the deficit faster than an
    // updated report can shrink it, its own recovery symbols overrun the pipe
    // and drop, the stale deficit persists, and it re-floods — MEASURED
    // recovery_coded 60 k–252 k symbols for a ~5 k-symbol object (up to 120×),
    // which DNFs at RTT200. Two bounds close the loop:
    //   (a) PER-GENERATION RTT SPACING. After emitting recovery for a
    //       generation, do NOT emit for it again for ~1 SRTT — long enough for
    //       those symbols to arrive and the receiver's NEXT deficit report to
    //       reflect them. This is the "send the deficit, wait ~RTT, re-evaluate"
    //       the design intended but never TIMED, so a stale periodic re-report
    //       could no longer trigger an immediate re-flood.
    //   (b) NON-EXEMPT from the in-flight cap. Reactive now also stops at
    //       `cwnd_full` (RWM_INFL_CAP) like proactive — it may not push the pipe
    //       past the congestion cap. The in-flight budget expires on the RTT
    //       timescale, so the frontier is still funded within a bounded delay
    //       (no permanent deadlock), it just cannot BURST past the cap.
    // Enabled by RWM_REACT_CAP (any value; the value optionally scales the
    // spacing — <1 = fraction of SRTT, >=1 = absolute µs). Unset = OFF (legacy
    // exempt behaviour), so Fix 1 measures alone and Fix 2 stacks on top.
    // gen_pipe defaults to once-per-RTT deficit coalescing (1.0·SRTT): "ONE
    // deficit feedback per RTT" — the #59/#60 lesson that a sub-RTT re-flood
    // of the fungible top-up defeats aggregation. RWM_REACT_CAP still overrides.
    let react_cap_cfg: f64 = gates
        .react_cap
        .unwrap_or(if gen_pipe { 1.0 } else { 0.0 })
        .max(0.0);
    let react_cap_on = react_cap_cfg > 0.0;
    // anchor → wall-clock (µs) of the last reactive emission for that generation.
    let mut gen_recover_at: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
    // In-flight coded allowance W (coded symbols the pipe may hold ahead of the
    // decode frontier). MUST be ≥ pipeline·gen_size: coded symbols are striped
    // round-robin across the M active generations, so to let the FIRST
    // generation accumulate its K_G (and thereby decode → advance the ack that
    // grows the target) each of the M active generations needs ~gen_size coded
    // in flight at once. Below M·G the first generation never reaches K_G, ack
    // stays 0, and the target never grows — a startup deadlock. Default
    // (M+1)·gen_size (matches the source-retention store_max) plus decode/loss
    // slack. RWM_GEN_INFLIGHT overrides.
    let gen_inflight_window: f64 = gates
        .gen_inflight
        .unwrap_or((2 * pipeline * gen_size) as f64);
    // RWM Phase C (paper §16.5, the BANDWIDTH knob r): experimental
    // per-symbol repair-rate FLOOR. The Bulk χ glide drives r*→0 mid-stream
    // (§14.26), leaving the window systematic (not rateless-fungible), so a
    // heterogeneous slow path's source symbols are fixed positions the fast
    // path cannot decode around (the measured Phase B C8 wall). Raising r
    // makes the pooled window fungible so completion → K/Σg. Env-gated
    // (RWM_MIN_R, repairs per source symbol, e.g. 0.18 ≈ the slow path's
    // symbol share at C8); 0 = production default (unchanged glide). Test
    // instrument for the raise-r arm, not a shipped control law.
    let repair_rate_floor: f64 = gates.min_r;
    // ── Fix 3 (transport-substrate): OUT-OF-ORDER RETENTION DECOUPLE ──────────
    // Defect #3: generation backpressure caps the send frontier at ~store_max =
    // a few generations ahead of the CUMULATIVE (in-order) decode ack, so ONE
    // hole stalls the whole pipeline even under out-of-order delivery — throughput
    // ∝ generations/RTT = window/RTT, reproducing ARQ's serialization. This
    // raises the retention/backpressure window to `ooo_gens` generations so the
    // sender keeps sending (and proactively coding, via the send-frontier-tracking
    // `set_code_base` below) MANY generations past a stalled in-order frontier;
    // the stalled generation is recovered by the bounded reactive tail (Fix 2)
    // while everything above it completes out of order. Retention still drops on
    // the in-order ack (advance(ack+1)) so RELIABILITY IS UNCHANGED — the sources
    // of every not-yet-in-order-acked generation stay retained for reactive
    // recovery; memory is bounded by `ooo_gens·G`. Env RWM_OOO_RETAIN (value =
    // generation count, default 16; unset = OFF, byte-identical legacy).
    let ooo_retain = gates.ooo_retain && generation;
    let ooo_gens: usize = gates.ooo_gens;
    // Fungible frontier window sizing (§16.5, the FOURTH bound W_mp). A hole
    // at the frontier is raced by coded symbols that combine over the CURRENT
    // window; sustained Σg aggregation needs the window to span the cross-path
    // recovery horizon, W_mp ≳ Σg·(RTT_max+t_slack) ≈ 600 symbols at C8 — 3×
    // the systematic pipeline's MAX_WINDOW_SIZE=200, which §16.5 states would
    // "starve RWM at C8 by construction". Coded-only therefore widens the
    // coding window to W_mp (default 640, RWM_WINDOW override for the sweep);
    // the oracle (oracle_c8_fungible_wmp_window) confirms W≥384 reaches the
    // ×1.19 ceiling while W=200 does not. Systematic modes keep 200.
    let win_cap: usize = if generation {
        // Generation mode retains the whole in-flight pipeline: M generations
        // of G symbols (plus one for the currently-filling head). This is the
        // stable-anchor analogue of W_mp — every not-yet-decoded generation
        // stays retained (and keeps getting coded symbols) until it decodes.
        // Fix 3: RWM_OOO_RETAIN widens this to `ooo_gens` generations so the
        // send frontier can run far past a stalled in-order frontier.
        // gen_pipe: retention ceiling = the M* hard cap (the DYNAMIC intake
        // cap `gen_pipe_store_cap` below is what actually bounds the queue).
        let gens = if gen_pipe {
            GEN_PIPE_MAX_GENS + 1
        } else if ooo_retain {
            ooo_gens + 1
        } else {
            pipeline + 1
        };
        (gen_size * gens).clamp(MAX_WINDOW_SIZE, 1 << 20)
    } else if coded_only {
        gates
            .window_override
            .unwrap_or(640)
            .clamp(MAX_WINDOW_SIZE, 4096)
    } else {
        MAX_WINDOW_SIZE
    };
    // Fungible-frontier retention bound = the coding window itself. This is
    // the §16.5 W_mp bound doing double duty: the backpressure cap must keep
    // the SEND frontier within ONE window of the cumulative ack, so every
    // not-yet-decoded seq stays INSIDE the current coding window and is raced
    // by ongoing coded symbols (fungible in-window refill) rather than aging
    // out and forcing a congestion-throttled targeted ARQ. At the systematic
    // RELIABLE_STORE_MAX=1024 > W the frontier runs ~1024 ahead while the
    // window covers only the last 640, so a lost DOF at the ack ages out to
    // slow ARQ (MEASURED ~4.7 Mbit/s, 80% idle); lifting the cap entirely
    // decouples them and DNFs. Sizing the store to W_mp is what makes the
    // window rateless-fungible in practice. W_mp also comfortably exceeds the
    // BDP (~190 sym at C8), so both paths stay saturated. RWM_STORE overrides.
    let store_max: usize = if generation {
        // Backpressure at the pipeline bound: the send frontier may run at most
        // ~M generations ahead of the cumulative-decode frontier, so exactly M
        // generations are in flight. TUN reads pause here (flow control), never
        // dropping data. Generation mode uses the encoder's retained size as the
        // backpressure signal (no sent_store), so this matches win_cap.
        //
        // Transport-ceiling fix (MEASURED at L1): win_cap = G·(M+1) as the
        // BACKPRESSURE point is 14× the BDP at C2, so the unacked pipeline is a
        // multi-hundred-ms standing queue (RTT inflated to 0.5–1.3 s). That
        // bufferbloat does NOT cap single-path throughput (it is window-
        // INDEPENDENT — a per-symbol processing limit) but it (a) produces
        // catastrophic slow-run outliers (single-path 50 MB×6 stdev 24.8 s at
        // G·(M+1)) and (b) SERIALIZES dual-path aggregation: the fast path
        // stalls on the bloated in-order-frontier cross-path feedback, so
        // symmetric C7 falls BELOW single (×0.65, anti-aggregation).
        //
        // The send frontier needs only TWO generations outstanding to pipeline
        // — one filling head + one sealed-and-recovering — not M+1. Backpressure
        // at 2·G (retention stays at win_cap = G·(M+1) for decode headroom)
        // decouples the standing queue from the retention horizon. MEASURED
        // (G=480, 50 MB×6): single 11.2→15.6 Mbit (stdev 24.8→0.7 s), symmetric
        // C7 9.8→22.3 (×1.43 aggregation), heterogeneous C8 9.45→14.55 — all
        // up, tighter, 0 DNF. RWM_STORE overrides for the sweep.
        // Fix 3: under OOO retention the backpressure window is the wide
        // ooo_gens·G, so the send frontier decouples from the stalled in-order
        // frontier. Otherwise the tight 2·G standing-queue bound.
        // gen_pipe: the static cap is the M* ceiling; the DYNAMIC per-loop cap
        // (`gen_pipe_store_cap` = M*·G) is what gates intake each iteration.
        let default_store = if gen_pipe {
            GEN_PIPE_MAX_GENS * gen_size
        } else if ooo_retain {
            ooo_gens * gen_size
        } else {
            2 * gen_size
        };
        gates
            .store_override
            .unwrap_or(default_store)
            .clamp(gen_size, win_cap)
    } else if coded_only {
        gates.store_override.unwrap_or(win_cap).clamp(win_cap, 1 << 20)
    } else {
        // Plain-reliable (systematic-free, non-generation) MEMORY ceiling for
        // the retention store. RWM_STORE forces a STATIC window (disables the
        // dynamic BDP cap below) for the sweep; the shipped default keeps the
        // large retention ceiling and lets the delay-based `plain_dyn_cap`
        // bound the *outstanding* window instead.
        gates.store_override.unwrap_or(RELIABLE_STORE_MAX)
    };
    // Delay-based send-window cap for the plain-reliable path (paper §12).
    // The fixed RELIABLE_STORE_MAX (1024) is ≈12× the BDP at C2, so the
    // unacked store builds a multi-hundred-ms standing queue (MEASURED RTT
    // 0.41–0.52 s vs 10 ms base). On a CLEAN link that only adds latency, but
    // under loss every hole must traverse that bloated queue to recover, the
    // cumulative-ack (and thus the ack-clocked pacing) freezes for a full
    // bufferbloat-RTT, and single-path throughput COLLAPSES (MEASURED 75→14
    // Mbit at C2). The remedy is to bound the OUTSTANDING window to a
    // BDP-scaled cap so the queue — and hence recovery latency — stays ~1 RTT.
    // BtlBw×RTprop is bufferbloat-robust (windowed-max rate × min-RTT floor),
    // so it tracks the true pipe even while the live RTT is inflated. Active
    // only for the plain-reliable path and only when RWM_STORE is NOT forcing
    // a static window; generation/coded-only keep their own structural caps.
    let plain_dyn_cap =
        reliable && !generation && !coded_only && !gates.store_env_set;
    // Window = gain × BDP. ≥2 keeps the pipe full (≈1 BDP) while leaving ≈1
    // BDP of headroom to keep sending fresh data during a one-RTT recovery
    // round; 2.5 adds jitter/burst slack. RWM_STORE_GAIN overrides.
    let store_bdp_gain: f64 = gates.store_gain;
    // Cap before the BtlBw anchor warms (a few RTTs). Tight so the startup
    // burst can't pre-bloat the queue and inflate the min-RTT floor (which
    // would then inflate the anchor itself); the anchor takes over once
    // samples land. ~1.5× a 100 Mbit / 10 ms BDP.
    let store_boot_cap: usize = gates.store_boot;
    // Floor so a transiently-tiny BDP estimate can't strangle the pipe.
    let store_cap_floor: usize = 64;
    // ── Path-scaled outstanding pool (task #84, env RWM_STORE_PATHS) ──────
    // MEASURED at L1 (2026-07-14, host-passthrough E5-2650v3): the plain-
    // reliable OUTSTANDING ceiling is a per-TRANSFER constant
    // (RELIABLE_STORE_MAX = 1024, which the 2×Σanchor dynamic cap latches at
    // on fast paths because the legacy ack-interval anchor over-reads), so a
    // multipath sender is store-starved: the DIAG shows win=1024/1024 pegged
    // while both paths idle (infl=0 spikes). Same-binary static-store sweep,
    // C7 plain+BBR: 1024→103 Mbit, 2048→122.7, 4096→141.3, 8192→143.7
    // (saturated); C8: 4096→71.5, 8192→31.8 (slow-path bufferbloat collapse);
    // singles: sc2 2048→81.6 / 4096→75.6 / 8192→43.0 (collapse), sc3
    // degrades monotonically with a static pool (the dynamic cap binds at
    // ~684 there and is the right law). The knee is 2048 PER LIVE PATH.
    // Under RWM_STORE_PATHS=1 and N = live_paths ≥ 2 the dynamic-cap value
    // scales ×N and its clamp ceiling becomes N × 2048 (RWM_STORE_PATH_POOL
    // overrides); N = 1 keeps the legacy law bit-exactly, so singles are
    // unaffected even with the flag ON. Default OFF: shipped byte-identical.
    // The engine sink is NOT the binder here: single-path c1 sinks 187.7
    // Mbit/s through the same receiver task, and pinning the C7 receiver to
    // one core costs only −8% at the default store.
    // DEFAULT ON (2026-07-21, "Consolidation" LOO battery: removal from the
    // composed stack re-opens the c7 collapse class (86-97 Mbit runs, both
    // seeds) and drops the mean; no cell regressed >>sigma. The c8 sub-sigma
    // cost vs the legacy pool under SACK-release is the register's WATCHED
    // follow-up — see goal-gate "Consolidation".)
    let store_paths_on = gates.store_paths;
    let store_path_pool: usize = gates.store_path_pool;
    if store_paths_on && plain_dyn_cap {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE): the recorded run
        // must show which outstanding-pool law was active.
        info!(
            pool_per_path = store_path_pool,
            gain = store_bdp_gain,
            "path-scaled outstanding pool ACTIVE (RWM_STORE_PATHS: cap = clamp(gain*N*pipe, floor, N*pool) for N>=2 live paths; N=1 legacy)"
        );
    }
    // ── Capacity-weighted outstanding pool (env RWM_STORE_CAPW) ──────────
    // The ADR-0058 "c8 WATCH" follow-up (goal-gate "C8-Aware Pool Law"):
    // pool = Σ_i honest per-path cap over LIVE paths (capw_store_cap) — each
    // path earns unacked-frontier depth for its OWN pipe + recovery round,
    // summed as ONE shared pool (borrowing stays free, only the sizing law
    // changes vs RWM_STORE_PATHS' count-scaled clamp). Engaged N ≥ 2 with
    // every live anchor warm; until then the configured pooled law
    // (path-scaled / legacy) is the warm-up fallback. Default OFF: shipped
    // byte-identical; the battery arm composes RWM_PLAIN_RS=1 so the anchor
    // terms read ≈1× truth (the legacy over-read clamps this law to the
    // N×knee ceiling ≡ path-scaled — documented at capw_store_cap).
    let capw_on = gates.store_capw && plain_dyn_cap;
    if capw_on {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE item 1).
        info!(
            pool_per_path = store_path_pool,
            gain = store_bdp_gain,
            "capacity-weighted outstanding pool ACTIVE (RWM_STORE_CAPW: pool = sum_i anchor_i*(K_i+gain-1) + rate_i*(gain-1)*R over live paths, clamp [floor, N*knee], N>=2 all-warm; fallback = configured pooled law until anchors warm; N=1 legacy)"
        );
    }
    // ── SACK-clocked store release (env RWM_STORE_SACK_RELEASE) ──────────
    // Goal-gate "SACK-Clocked Store Release" (pre-registered 2026-07-21):
    // the retention store releases slots only on the cumulative frontier,
    // so SACKed-but-not-cumulative symbols hold slots a full frontier round
    // — at c7 the store recycles at frontier latency, not path rate. Under
    // this law a SACKed seq is UNCOUNTED from the flow-control outstanding
    // (the slot returns to the pool / per-path account, the window opens)
    // while sent_store + retransmit_buffer + nack_retx_at + source_path_map
    // are kept UNTOUCHED until the cumulative frontier passes it — release
    // a STORE SLOT, never recoverability (the RWM_SACK_PRUNE lesson; see
    // sack_release_mark). DEFAULT ON (2026-07-21, the pre-registered
    // battery earned the flip: c7 0.96–1.05×Σ both seeds, sc2 +3–4 at
    // N=1, dual-c1 +20–22 composed, no regression; goal-gate
    // "SACK-Clocked Store Release"); RWM_STORE_SACK_RELEASE=0 is the
    // legacy frontier-only-release opt-out arm, under which the released
    // set stays empty and the gate arithmetic is exactly the legacy
    // store_len.
    let store_sack_release_on =
        reliable && !generation && !coded_only && gates.store_sack_release;
    if store_sack_release_on {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE item 1).
        info!(
            "SACK-clocked store release ACTIVE (RWM_STORE_SACK_RELEASE: SACKed seqs \
             uncounted from the outstanding gate, payload + ARQ maps retained until \
             the cumulative frontier — slot release, never recoverability)"
        );
    }
    // ── Frontier-slack placement (env RWM_PLACE_SLACK) ───────────────────
    // Goal-gate "C8 Slow-Path Conversion" (pre-registered 2026-08-06): the
    // §16.3 placement cost's load term becomes max(0, Ê_i − S)/ref with
    // S = clamp(span/R_ack, 0, 250 ms) — span = sent_edge − cum_ack,
    // R_ack = EWMA of the cumulative-ack advance rate (delivery truth,
    // immune to the plain anchor's over-read). S = 0 until R_ack warms and
    // whenever N < 2 (shipped cost bit-exact — the law is a strict
    // continuous generalization; see Scheduler::set_place_slack /
    // place_costs). Plain reliable window only. Default OFF.
    let place_slack_on = gates.place_slack && reliable && !generation;
    if place_slack_on {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE item 1). The INFO
        // prints whenever the gate is CONFIGURED; the law itself engages
        // only at N ≥ 2 with a warm ack-rate (the harness expects the echo
        // per ENV — the c8pool harness-note lesson).
        info!(
            "frontier-slack placement ACTIVE (RWM_PLACE_SLACK: cost_i = \
             max(0, E_i - S)/ref, S = span/R_ack clamped <= 250 ms; S = 0 \
             cold / N = 1 = shipped-identical)"
        );
    }
    // Slack-law state: refresh timer (5 ms cadence), ack-rate sample
    // anchor (>= 50 ms windows), EWMA, and the live S gauge for DIAG.
    let mut ps_refresh_us: u64 = 0;
    let mut ps_rate_last_us: u64 = 0;
    let mut ps_rate_last_ack: u64 = 0;
    let mut ps_rate_ewma: f64 = 0.0;
    let mut ps_slack_gauge: f64 = 0.0;
    // ── Per-path outstanding accounting (task #86, env RWM_STORE_PERCAP) ──
    // The #84 residual: the PATH-SCALED pool is still ONE pool — it cannot
    // fit a c2-deep and a c3-shallow path simultaneously (C8 stuck at
    // 0.79–0.80 of Σ; raising the shared cap to 8192 collapsed the slow
    // path to 31.8 Mbit/s). Here each path gets its OWN account sized to
    // ITS pipe (percap_store_cap: gain·rate_i·echoRTT_i, clamped to
    // [floor, pool]); a symbol placed on path i draws path i's account and
    // is released on the ack that removes it from the retention store
    // (SACK/OOO or cumulative). Admission pauses only when NO live path
    // has account headroom (percap_store_full — the infl_percap_full
    // pattern), and the plain-reliable placement redirects a cap-full pick
    // to the path with headroom (percap_place_path). Engaged only for
    // N ≥ 2 live paths — N = 1 keeps the legacy pooled law bit-exactly.
    // Default OFF: shipped byte-identical. Supersedes RWM_STORE_PATHS'
    // pooled GATE when both are set (the warm-up share still inherits from
    // whichever pooled law is configured, so STORE_PATHS composes as the
    // warm-up baseline rather than conflicting).
    let percap_on = gates.store_percap && plain_dyn_cap;
    // Roadmap item 1 (the #86 c8 follow-up): the delay-aware redirect guard.
    // Default ON whenever percap is on (RWM_PERCAP_GUARD=0 restores the
    // unguarded redirect — the measured c8-regression control arm). The
    // shipped default is untouched: percap itself is default OFF.
    let percap_guard_on = percap_on && gates.percap_guard;
    // Bounded account borrowing (feat/store-borrowing, paper §16.22): a
    // pick landing on a cap-full account may FLY on that pipe while being
    // CHARGED to a sibling account, bounded by
    //   lend_i→j ≤ max(0, cap_i − out_i − rate_i·T_return(j)),
    //   T_return(j) = fly_j/rate_j + RTprop_j (floor clock)
    // — lend only headroom the lender cannot use within the loan's return
    // latency. Requires the percap stack (accounts, guard, honest caps
    // under RWM_PLAIN_RS). Default OFF: shipped byte-identical; the
    // no-borrow percap arm is the same-binary control.
    let percap_borrow_on = percap_on && gates.store_borrow;
    if percap_on {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE).
        info!(
            pool_per_path = store_path_pool,
            gain = store_bdp_gain,
            floor = store_cap_floor,
            "per-path outstanding accounting ACTIVE (RWM_STORE_PERCAP: cap_i = clamp(gain*rate_i*echoRTT_i, floor, pool) per live path for N>=2, warm-up = legacy-pool/N; supersedes RWM_STORE_PATHS' pooled gate; N=1 legacy)"
        );
    }
    if percap_guard_on {
        // Guard mechanism-liveness echo (asserted PRESENT on guarded arms,
        // ABSENT on the RWM_PERCAP_GUARD=0 regression-control arm).
        info!(
            "percap delay-aware redirect guard ACTIVE (roadmap-1: redirect to j only while out_j < bound_j = rate_j*RTprop_j — kappa=1 on the floor clock; Copa feed: cwnd_j; warm-up: cap_j/gain — else the store reads FULL for the placement and admission pauses; RWM_PERCAP_GUARD=0 = unguarded legacy redirect)"
        );
    }
    if percap_borrow_on {
        // Borrowing mechanism-liveness echo (MEASUREMENT DISCIPLINE):
        // asserted PRESENT on PBP-B/C1P-B arms, ABSENT on every no-borrow
        // arm.
        info!(
            "bounded store borrowing ACTIVE (RWM_STORE_BORROW, paper 16.22: a cap-full pick flies on its picked pipe, charged to the lender with max lend_i->j = cap_i - out_i - rate_i*T_return(j), T_return(j) = fly_j/rate_j + RTprop_j; loans repay on ack; symmetric cells lend 0 by theorem; warm-up lends 0)"
        );
    }
    // ── Honest floor-clock store caps (feat/percap-honest-cap) ────────────
    // GUARD-RESULTS residual (i): with the redirect channel closed, the c8
    // parking flowed through the softmax's OWN picks under the knee-clamped
    // slow cap — the legacy plain anchor over-reads ×4.6–7.4 ("Anchor
    // Hygiene" battery (b)) so cap_slow latched at the 2048 knee and the
    // derived differentiation never engaged. With the honest send-interval
    // sampler (RWM_PLAIN_RS) the anchor reads ≈1× truth, and the cap law
    // is re-derived on it: cap_i = anchor_i·(K_i + gain − 1) +
    // rate_i·(gain−1)·R — residence on the measured unloaded drain clock
    // plus runway on the RECOVERY engine's clock (R = the 100-ms hole-
    // refresh/tail-sweep cadence bound), see `honest_store_cap`. Applies
    // to the per-account
    // percap caps AND the N=1/anchor-sum pooled cap (the sc2 −20% fix: the
    // over-read was accidentally load-bearing there; K supplies that
    // headroom explicitly and honestly). Engaged only where the honest
    // sampler is live (plain in-order, no Copa CC ownership — the Σcwnd
    // and per-path cwnd laws are already honest and stay untouched).
    // RWM_HONEST_CAP=0 = the floor-law control arm (reproduces the −20%);
    // both gates default-OFF paths keep the shipped tree byte-identical
    // (RWM_PLAIN_RS itself is default OFF).
    let honest_cap_on = plain_dyn_cap && gates.plain_rs && gates.honest_cap;
    if honest_cap_on {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE): asserted
        // PRESENT on honest-cap arms, ABSENT on knee-clamp control arms.
        info!(
            gain = store_bdp_gain,
            floor = store_cap_floor,
            pool_per_path = store_path_pool,
            "honest floor-clock store caps ACTIVE (RWM_PLAIN_RS+RWM_HONEST_CAP: cap_i = anchor_i*(K_i+gain-1) + rate_i*(gain-1)*R, K_i = windowed-min echoSRTT/RTprop, R = 100ms recovery-round bound; per-account under RWM_STORE_PERCAP, anchor-sum at N=1; RWM_HONEST_CAP=0 = floor-law control)"
        );
    }
    // ── Window/inflight decoupling (env RWM_WIN_DECOUPLE) ────────────────
    // Goal-gate "Window Decoupling + MTU Scaling" part 1 (pre-registered
    // 2026-08-06 + diagnosis amendment): at N = 1 the admission gate moves
    // from the un-SACKed total vs the anchor-sum latch to the live HEAD
    // SPAN (last_sent − SACK/cum frontier — recovery-stalled holes
    // excluded) vs the stall-metered allowance `win_decouple_allow`; the
    // un-SACKed total keeps a retention backstop `win_decouple_cap_ret`
    // (memory clamp 4096). Under Copa-sole the residence term is
    // gain·Σcwnd (the 1024 ceiling truncation — the B1 jitter-cell dwell
    // binder — is released). N ≥ 2 and warm-up keep the configured laws
    // bit-exactly. Default OFF: shipped byte-identical.
    let win_decouple_on = gates.win_decouple && plain_dyn_cap;
    if win_decouple_on {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE item 1). Prints
        // when CONFIGURED; the law engages at N = 1 with a warm anchor
        // (the harness expects the echo per ENV).
        info!(
            gain = store_bdp_gain,
            "window/inflight decoupling ACTIVE (RWM_WIN_DECOUPLE: wire gate = head \
             span vs anchor*(K+gain-1) + rate*min(stall_age, 100ms); holes to \
             retention cap_ret, clamp 4096; N=1 only; Copa-sole ceiling released)"
        );
    }
    // Law state, refreshed with the dyn-cap throttle; wd_engaged gates the
    // decoupled admission test each iteration.
    let mut wd_engaged: bool = false;
    let mut wd_allow_base: f64 = 0.0;
    let mut wd_rate: f64 = 0.0;
    let mut wd_cap_ret: usize = 0;
    // path → windowed-min echo-ratio state (K_i), fed at the dyn-cap
    // refresh cadence; ~10 s window = two 5 s half-buckets.
    const PERCAP_K_HALF_WINDOW_US: u64 = 5_000_000;
    let mut percap_k: std::collections::HashMap<u32, EchoRatioMin> =
        std::collections::HashMap::new();
    // seq → account path, in lockstep with `sent_store` (charge on insert,
    // release on ack-removal ONLY — the retention contract).
    let mut percap_acct: BTreeMap<u64, u32> = BTreeMap::new();
    // path → outstanding gauge (Σ over percap_acct; DIAG `sout=`).
    let mut percap_out: std::collections::HashMap<u32, usize> =
        std::collections::HashMap::new();
    // path → cap_i, refreshed with the dynamic-cap throttle. NON-EMPTY is
    // the "percap law engaged" signal (flag on AND N ≥ 2 live paths).
    let mut percap_caps: std::collections::HashMap<u32, usize> =
        std::collections::HashMap::new();
    // path → redirect_bound_i (roadmap item 1: rate_i×RTprop_i, the
    // floor-clock dwell bound a cap-full redirect may fill the account to).
    // Mirrors cap_i (guard degenerate) when RWM_PERCAP_GUARD=0.
    let mut percap_bounds: std::collections::HashMap<u32, usize> =
        std::collections::HashMap::new();
    // ── Bounded-borrowing loan ledger (feat/store-borrowing, §16.22) ────
    // seq → (lender, flyer) for BORROWED seqs only (sparse; empty when the
    // gate is off). Repaid by the same acks that release the account.
    let mut percap_loans: BTreeMap<u64, (u32, u32)> = BTreeMap::new();
    // path → loans lent out (charged here, flying elsewhere) / borrowed in
    // (flying here, charged elsewhere): fly_i = out_i − lent_i + borrowed_i.
    let mut percap_lent: std::collections::HashMap<u32, usize> =
        std::collections::HashMap::new();
    let mut percap_borrowed: std::collections::HashMap<u32, usize> =
        std::collections::HashMap::new();
    // path → (rate sym/s, RTprop s) snapshot for the borrow law, refreshed
    // with the caps (same cadence, same honest sources).
    let mut percap_rr: std::collections::HashMap<u32, (Option<f64>, Option<f64>)> =
        std::collections::HashMap::new();
    // DIAG: cumulative loans granted (mechanism liveness at the gauge).
    let mut percap_loans_total: u64 = 0;
    // Throttled cache of the dynamic cap (recomputed off the scheduler lock at
    // most every 5 ms; the pipe/BDP move far slower than the select loop).
    let mut dyn_store_cap: usize = store_boot_cap.min(store_max);
    let mut dyn_cap_refresh_us: u64 = 0;
    // Fractional repair accumulator: tracks sub-symbol repair debt.
    // Driven by TaperFunction density when GE data is available,
    // falls back to flat rate from compute_repair_rate_capped.
    let mut repair_debt: f64 = 0.0;
    // Source symbol counter for taper time offset (symbols since window start).
    let mut taper_offset: u64 = 0;
    // ── #85 budget-conserving taper (RWM_TAPER_R, default OFF) ────────────
    // MEASURED (goal-gate "r* Bursty-Loss Provisioning", L1 2026-07-13): the
    // legacy taper accrual below sums to Σ τ(t) = r symbols PER ACK CYCLE
    // (taper_offset resets on cumulative-ack advancement), so the emitted
    // plain-mode proactive overhead is ~r/cycle-length — nearly independent
    // of r's computed magnitude. Legacy r*=0.206 and corrected r*=0.255 both
    // emitted cod/src ≈ 0.03–0.10 at c3-realtime: the whole r* control loop
    // (incl. the §8.4.1 burst-tail correction) was INERT at the wire. With
    // the flag ON, `TaperBudget` makes emission consume r as computed: a
    // per-window budget (emitted ≈ r × source per coding window), the taper
    // shape kept as a re-timing (repair still concentrated at the frontier),
    // paced ≤ 1 repair per source send and spare-capped (existing anchors,
    // no new constants). OFF ⇒ byte-identical legacy emission (A/B arm).
    // L0 VERDICT (2026-07-18, goal-gate "Taper Emission Fix"): the budget
    // law is LIVE at the wire (cod/src 0.03-0.05 → 0.21-0.34 on the
    // c3heavy 2x2) but delivered reliability DEGRADES at realtime and the
    // r* arms stay tied — the emitted repair codes over the LEADING sliding
    // window (in-flight entanglement, the RWM_MIN_R defect class above), so
    // it is recovery-inert within realtime's reorder horizon; quantity was
    // not the only binder. Default stays OFF; flipping it is gated on the
    // solvable-span emission follow-up, not on L1 alone.
    // §16.20 (c): under RWM_UNIFIED the quantity law is the default (the #85
    // fix composes with the trailing solvable-span placement below, which
    // removes the leading-window entanglement that kept it OFF); RWM_TAPER_R=0
    // still reproduces the legacy accrual as the same-binary A/B arm.
    let taper_r_budget = gates.taper_r;
    let mut taper_budget = crate::control::TaperBudget::new();
    if taper_r_budget {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE).
        info!(
            "budget-conserving taper emission ACTIVE (RWM_TAPER_R: plain-mode proactive repair budgeted at r x source per coding window; legacy = r per ack cycle)"
        );
    }
    // §16.20 (c): trailing solvable-span placement for plain-mode proactive
    // repair — span width A* = clamp(rate·D, 1, W) with D = b(hint)·RTprop
    // (§8.8 budgets: Realtime ½, Auto 1, Bulk 2 RTT — capped at 2·RTprop, the
    // deficit-round limit) and trailing offset Δ = ceil(rate·jitter) ≥ 1, so
    // every covered member has LANDED when the repair does (solvable at
    // arrival — the #85 leading-window entanglement removed structurally).
    let unified_span = gates.unified;
    if unified_span {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE).
        info!(
            hint = ?protocol_hint,
            "unified span law ACTIVE (RWM_UNIFIED: plain-mode proactive repair over the trailing solvable span [end-A*, end-Δ), A* from δ)"
        );
    }
    // feat/anchor-hygiene (`RWM_ASTAR_ANCHOR`): the A* rate anchor repaired.
    // Legacy A* reads `est.throughput()` — a 2-s-interval α=0.125 EWMA of the
    // report-tick send rate — which (i) pins A* = 1 for ~10 s of every stream
    // (realtime FEC inert: ru/rf ≈ 9%) and (ii) is flood-poisonable (A* 1→38
    // off the post-stall release burst) — goal-gate COLLAPSE ATTRIBUTION,
    // defect designs A+B. The repair: a windowed-max send-rate anchor
    // (SendRateAnchor) fed by the sender's OWN send events — live within ~1
    // RTT (hygiene rule 1), with gap-spanning/flood buckets DISCARDED
    // (rule 2). Gate off ⇒ the EWMA path byte-identical.
    // goal-gate "Unified Shedding": DEFAULT ON under the unified machine —
    // the span law ships with its repaired anchor (fix A gates the flip
    // battery; without it the realtime spans pin at width 1, ru/rf ≈ 9%).
    // `RWM_ASTAR_ANCHOR=0` / `RWM_ANCHOR_HYGIENE=0` still opt out for A/B.
    let astar_anchor_on = unified_span && gates.astar_anchor;
    let mut astar_anchor = crate::control::SendRateAnchor::new();
    if astar_anchor_on {
        info!("A* send-rate anchor ACTIVE (RWM_ASTAR_ANCHOR: windowed-max send rate over ~8 SRTT, clock-gap sample discard)");
    }
    // ── δ-honest overload shedding (fix C, goal-gate "Unified Shedding") ──
    // Part of the unified machine's REALTIME semantics: armed only on the
    // EVICT path (`!reliable` — the ρ = 1 RETAIN contract is excluded by
    // construction) under RWM_UNIFIED; `RWM_UNIFIED_SHED=0` reproduces the
    // serializing arm for A/B. A hole whose retransmit can no longer meet
    // the δ deadline D = b(hint)·RTprop (the span law's own D) is DROPPED
    // from the ARQ set instead of serializing the stream behind it — but
    // only while cumulative shed stays within the DERIVED 1−ρ budget
    // (`residual_loss_after_fec`: ε̂·(1−P_fec) at the live (r, A*, σ²)
    // operating point). Budget spent ⇒ serialize (ρ wins over δ).
    let shed_on = shed_armed(gates.unified, reliable, gates.unified_shed);
    if shed_on {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE item 1).
        info!(
            "unified overload shedding ACTIVE (RWM_UNIFIED_SHED: past-deadline holes shed within the derived 1-rho budget; =0 = serializing arm)"
        );
    }
    /// Seqs shed by the δ law (never served again; pruned at the cumulative
    /// frontier — the split_off twin).
    let mut shed_seqs: BTreeSet<u64> = BTreeSet::new();
    let mut shed_total: u64 = 0;
    /// Past-deadline candidates the ρ budget REFUSED (the serialize arm of
    /// the law — visible in DIAG so the budget's bite is measurable).
    let mut shed_denied: u64 = 0;
    /// The live derived 1−ρ budget fraction and δ deadline (µs), refreshed
    /// per source symbol alongside the span parameters.
    let mut shed_budget_frac: f64 = 0.0;
    let mut shed_deadline_us_live: u64 = 0;
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
    // a stalled region / has A* pinned / budget saturated. (Own t0: the DIAG
    // block's `diag_start_us` is declared after the send macro — hygiene.)
    let span_diag_start_us: u64 = now_us();
    let mut span_diag_last_us: u64 = 0;
    // ── Removed proactive-repair experiments (DEPRECATION REGISTER) ───────
    // RWM_FRONTIER* ("Proactive Frontier", 2026-07-07: repair anchored at the
    // ½-RTT-stale ack frontier loses the race to its own ARQ — rf=718 emitted,
    // ru=4 useful) and RWM_INLINE_REPAIR ("Repair In-Flight", 2026-07-08:
    // stall-starved + cross-grid stranding — every inline config wedged or
    // crawled) were both refuted on GEOMETRY, not substrate, and REMOVED
    // 2026-07-27. Their goal (repair present at stall) is achieved by
    // RWM_PROACTIVE_PACER below, whose own measured null resolved into the
    // structural presence⊥throughput identity; the unified TRAILING span law
    // (§16.20.3) is the derived realization of the frontier intent. The FDIAG
    // diagnosis instrument (RWM_FDIAG, receiver loop) is retained.
    // ── Proactive-repair pacer (RWM_PROACTIVE_PACER) — present-at-stall ───────
    // A DEDICATED proactive-repair emission on the GENERATION grid, decoupled
    // from BOTH source availability and the ack-clock `target`. For each
    // in-flight generation (still FILLING or recently sealed) it emits
    // proactive repair over the retained contiguous PREFIX at the full
    // generation width (`generate_repair_filling` → same (anchor, G) matrix, no
    // cross-grid stranding), paced by the shared CC token bucket. Fixes BOTH
    // refutations of the interspersed inline repair (goal-gate "Repair
    // In-Flight"): (1) NOT stall-starved — it runs in the main loop every
    // iteration incl. tx_paused wakeups, so repair flows under backpressure when
    // the frontier most needs it; (2) NOT cross-grid stranded — it codes the
    // generation grid, so a buffered filling equation combines directly with the
    // reactive generation deficit. The covering equation reaches the receiver
    // EARLY (around when the hole is sent, not a generation-span later at seal),
    // so it is PRESENT when the frontier detects the hole → proactive decode, no
    // round-trip. Supersedes the sealed batched proactive path when on; the
    // reactive deficit (RWM_REACT_CAP + RWM_REPAIR_WAIT) stays the bounded
    // fallback for holes the proactive repair still misses. Systematic only;
    // shipped path untouched.
    let proactive_pacer = systematic && gates.proactive_pacer;
    // ── Cross-path repair placement (RWM_XPATH_REPAIR) — the C8 realization ────
    // Route proactive (and deficit) REPAIR to the max-spare-capacity path (the
    // underutilized path — the slow path once the fast path is source-saturated)
    // instead of the marginal-cost softmax (which biases repair toward the fast
    // path, so it competes with systematic source — the single-path
    // presence⊥throughput tension). With this on, a fast-path loss is covered by
    // repair already in flight on the SLOW path, WITHOUT displacing fast-path
    // source: presence is bought from the spare path's capacity. Symmetric paths
    // (C7) have equal spare, so `place_repair_spare_path` splits the near-tie set
    // uniformly (no hard-argmax concentration → no C7 regression). Generation/
    // systematic only; shipped path untouched. Default-OFF.
    let xpath_repair = generation && gates.xpath_repair;
    /// Congestion-aware NACK repair throttle (ADR-0046).
    let mut nack_congestion = NackCongestionState::new();
    /// Maps source seq → path it was sent on (for cross-path retransmission).
    // BTreeMap (not HashMap) so the per-path ack attribution can range-query
    // the seqs in a SACK / cumulative-ack span efficiently (feat/per-path-
    // estimator); all other uses (insert/get/remove/retain) are unaffected.
    let mut source_path_map: std::collections::BTreeMap<u64, u32> = std::collections::BTreeMap::new();
    /// Last source path used (for NACK repair path selection outside the send macro).
    let mut last_source_path: u32 = 0;
    /// Wall-clock (us) of the last NEW source-symbol send (ADR-0046
    /// idle-triggered recovery). Initialized to "now" so a transfer that
    /// stalls before sending anything is treated as active until it idles.
    let mut last_source_send_us: u64 = now_us();
    /// NACK repairs sent in the current reporting period (ADR-0050 budget tracking).
    let mut nack_repairs_this_period: u64 = 0;
    /// Source symbols sent in the current reporting period.
    let mut source_symbols_this_period: u64 = 0;
    /// P10b: seq → last NACK-retransmit time (µs). Repeated gap acks for the
    /// same hole (they arrive every GAP_ACK_MIN_INTERVAL while it persists)
    /// must not resend the symbol more than once per SRTT — but MAY resend
    /// after an SRTT, which escalates naturally if the retransmit itself dies.
    /// Value = (last retransmit time µs, path the retransmit flew on). The
    /// path is the RWM_RECOV_MP live-flight input (the retransmit inherits
    /// the in-flight clock of its own path — feat/recovery-suppression);
    /// with the gate off only the time is read (byte-identical behavior).
    let mut nack_retx_at: std::collections::HashMap<u64, (u64, u32)> =
        std::collections::HashMap::new();
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

    // RWM_DIAG (transport-ceiling diagnosis) master gate — declared BEFORE the
    // send macro below so the macro body (GLIFE fill tracking) can see it.
    let diag_on = gates.diag;
    // Per-path store-attribution GAUGE (goal-gate "C8-Aware Pool Law"
    // diagnosis instrument, ADR-0052 class — no behavior): under RWM_DIAG the
    // percap account maps are maintained even when the percap LAW is off, so
    // the DIAG `sout=` field shows each path's share of the POOLED
    // outstanding (which path is holding the unacked-frontier span — the c8
    // pool-arm diagnosis gauge). Behavior-inert by construction: every percap
    // decision site keys on `percap_caps` NON-EMPTY (the "law engaged"
    // signal), and caps are only computed under percap_on — with the law off
    // the maps feed the DIAG print alone.
    let percap_track = percap_on || (diag_on && plain_dyn_cap);
    // ── GDIAG (feat/gen-substrate-ceiling JOB 1) ──────────────────────────
    // Time-weighted attribution of the generation-mode sender loop to the
    // gate that is BINDING its wire emission each instant. In coded-wire
    // generation mode the paced coded block IS the data plane, so whichever
    // gate stops it is the throughput binder. States (post-emission):
    //   emit    — emitted ≥1 coded this iteration (link-flowing)
    //   budget  — wants_coding=false with sealed gens retained: every active
    //             generation is at its ceil(len·(1+r)) proactive budget and
    //             the sender is WAITING ON THE ACK/deficit round (the
    //             window-advance serialization)
    //   fill    — wants_coding=false because the head generation has not
    //             sealed yet (waiting on TUN intake / store backpressure)
    //   target  — ack-clocked flow window `target` exhausted
    //   tokens  — pace token bucket dry (the delivered-rate-EWMA pacer)
    //   cwnd    — in-flight congestion cap
    // Also per-generation lifecycle (GLIFE): anchor → (first_src, sealed,
    // last_emit) µs; on the ack passing a generation its fill/code/ack-wait
    // phases are accumulated. All gated on RWM_DIAG (shipped path untouched).
    let mut gd_last_us = now_us();
    let mut gd_us = [0u64; 6]; // [emit, budget, fill, target, tokens, cwnd]
    let mut gl: std::collections::HashMap<u64, (u64, u64, u64)> =
        std::collections::HashMap::new();
    // (fill_us, code_us, wait_us, n) accumulated over completed generations.
    let mut gl_sum: (u64, u64, u64, u64) = (0, 0, 0, 0);

    // ── feat/recovery-suppression: multipath recovery suppression ─────────
    // (`RWM_RECOV_MP`, DEFAULT ON 2026-07-21 — the "Consolidation" LOO
    // battery: removal costs −12.3/−13.9 Mbit >>sigma at c7 on both seeds
    // (retx 18k vs 5.4k) with the dual-c1 retx flood (13-30k) re-appearing;
    // neutral within sigma everywhere else. `=0` is the legacy global-clock
    // opt-out arm. Plain window reliable mode only — generation mode has no
    // per-seq ARQ to suppress).
    // Sub-gate for trace attribution: _LAW (per-flight hole law, default ON
    // under the umbrella). The _SERIAL per-path batch-namespace arm was
    // REMOVED 2026-07-27 (register: refuted on the clean substrate, ×2.4
    // sender CPU — see the module-header design note (2)).
    let recov_mp = gates.recov_mp && reliable && !generation;
    let recov_mp_law = recov_mp && gates.recov_mp_law;
    if recov_mp {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE item 1).
        info!(
            law = recov_mp_law,
            "multipath recovery suppression ACTIVE (RWM_RECOV_MP: \
             per-flight RFC9002-style time-threshold hole law on the flight \
             path's smoothed clocks; \
             N=1 live path keeps legacy gates bit-exactly)"
        );
    }
    // ── diag/lossy-residual: SINGLE-path hole-law suppression ─────────────
    // (`RWM_RECOV_SP`, default OFF — the A/B arm; goal-gate "Lossy-Single
    // Residual"). The 2026-07-27 diagnosis measured the N=1 reactive plane
    // firing ×4.4–5.7 the realized loss (sc2-100M: fired 3313, y=2659 younger
    // than the law's own threshold, vs ~580 netem drops; sc3: 2556 vs ~510)
    // — the "single-path gaps are FIFO-real" premise of `mp_hole_ripe`'s
    // N=1 bypass is REFUTED on a jittery substrate (netem delay jitter
    // reorders tens of packets deep; the receiver's gap reports name
    // merely-late seqs, and re-fires chase flights still queued behind the
    // store-cap standing queue). The law: at N=1 a gap seq with a LIVE
    // flight (original or retransmit) fires only once the flight is
    // ≥ 9/8×max(smoothed clocks) old (RFC 9002 §6.1.2, same
    // `mp_time_threshold_us`); TIME channel only — the §6.1.1 packet
    // channel is excluded at N=1 (reorder depth ≫ kPacketThreshold).
    // Suppression-only: the receiver's hole-refresh re-advertises until the
    // flight ripens, so real holes still recover.
    let recov_sp = gates.recov_sp && reliable && !generation;
    if recov_sp {
        info!(
            "single-path hole-law suppression ACTIVE (RWM_RECOV_SP: RFC9002 \
             time-threshold on the live flight at N=1; time channel only)"
        );
    }
    // ── feat/c8-conversion: recovery clocks on LIVE paths ────────────────
    // (`RWM_RECOV_MP_LIVE`, default OFF — the A/B arm; goal-gate "C8
    // Slow-Path Conversion"). The hole law's N + per-path clock snapshot
    // read `active_paths()` — the saturation-filtered set (`available() >
    // 0`) whose cwnd-full-path trap collapses the law to the N=1 bypass
    // (legacy age gate on a cross-path clock) mid-transfer; the same
    // filter trap already documented at the Copa-sole store law and
    // `capw_store_cap`. Diagnosis signature (2026-08-06): c8-pbs 412–749
    // of ~1.2–1.5k retransmits fired YOUNG vs their own flight-path law
    // threshold. Under this gate the snapshot uses `live_paths()`.
    let recov_mp_live = gates.recov_mp_live && recov_mp_law;
    if recov_mp_live {
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
    // DIAG (RWM_DIAG): the recovery-plane trace counters — gap-report volume,
    // per-cause suppression, fired-retransmit age attribution (young = the
    // law's spurious class), per-flight-path and per-retx-path emission, and
    // the P_lost-branch retransmit count. Cumulative; printed as `mpr[..]`.
    let mut mpd_gap_reports: u64 = 0;
    let mut mpd_gap_seqs: u64 = 0;
    let mut mpd_supp_cool: u64 = 0;
    let mut mpd_supp_age: u64 = 0;
    let mut mpd_supp_law: u64 = 0;
    let mut mpd_stale: u64 = 0;
    let mut mpd_fired_young: u64 = 0;
    let mut mpd_fired_ripe: u64 = 0;
    let mut mpd_fired_fast: u64 = 0;
    let mut mpd_coalesced: u64 = 0;
    let mut mpd_age_ms_sum: f64 = 0.0;
    let mut mpd_plost_retx: u64 = 0;
    let mut mpd_fired_flight: std::collections::HashMap<u32, u64> =
        std::collections::HashMap::new();
    let mut mpd_fired_on: std::collections::HashMap<u32, u64> =
        std::collections::HashMap::new();

    // ── Emission batching (goal-gate "Emission Batching", RWM_EMIT_BATCH,
    // DEFAULT OFF — same-binary A/B) ──────────────────────────────────────
    // The §16.23 sender-emission service wall (~19.5–20k sym/s ≈ 190 Mbit)
    // is per-SYMBOL loop cost, profiled 2026-07-27 on the c1 cell: taper/
    // span control math (compute_repair_rate + predictive_loss_upper +
    // exp/log ≈ 15–17%/core, recomputed per symbol), plus a full select!
    // iteration (tail-deadline scan, SACK drain, pacing refresh) and the
    // waker churn of one-datagram-per-wakeup handoff to quinn (syscall
    // density is NOT the wall — quinn-udp GSO already batches ~7.6
    // segments/sendmsg on this path). Under the gate the sender:
    //   1. drains TUN intake in pacer-quantum bursts (≤ emit_burst symbols
    //      per loop iteration, ~64 KB — inside the flow-control store
    //      headroom and the cc_pace token bucket, checked per symbol), so
    //      loop-iteration overhead amortizes and quinn's endpoint driver
    //      sees a multi-datagram queue (deeper GSO transmits);
    //   2. refreshes the derived taper/span math once per burst instead of
    //      per symbol (the A* send-rate anchor is still FED per symbol —
    //      only the derived-rate recomputation is amortized).
    // OFF ⇒ per-symbol recompute, bit-identical shipped path. Plain
    // window-reliable mode only (generation/coded emission has its own
    // paced block; realtime packing keeps its per-packet latency path).
    // SINGLE-LIVE-PATH ONLY (measured, 2026-07-27 battery rep 1): the
    // emission service wall is a c1-class single-path binder (§16.23);
    // dual cells are wire/recovery-bound and bursting there AMPLIFIES the
    // wall-#8 striping-gap loss misread (global batch serials + longer
    // same-path arrival runs → per-path pl read up to 0.74 at a 2.6%-loss
    // cell, tail-recovery stretch: c7 167→115, c8 87→52). With N ≥ 2 live
    // paths the emission path stays bit-identical (`emit_batch_live`
    // re-checked per loop iteration — path flaps re-scope within one
    // burst).
    // Realtime (packed) mode is excluded outright: its per-packet latency
    // path must never trade a wakeup for a burst, and its symbol rate is
    // orders below the wall. The taper cache additionally carries a 50 ms
    // staleness bound so a low-rate bulk-hint tunnel (e.g. the tail-matrix
    // message workload riding the bulk tunnel at 50 msg/s) never runs the
    // span/shed law on second-old anchors.
    let emit_batch_on = gates.emit_batch && reliable && !coded_wire && !use_packing;
    let mut emit_batch_live = false;
    let emit_burst: usize = gates.emit_burst;
    if emit_batch_on {
        // Mechanism-liveness echo (MEASUREMENT DISCIPLINE item 1).
        info!(
            burst = emit_burst,
            "emission batching ACTIVE (RWM_EMIT_BATCH: pacer-quantum TUN \
             intake + per-burst taper/span refresh; flow-control and pacing \
             contracts enforced at symbol granularity)"
        );
    }
    // Per-burst cache: (repair_rate, span_params, estimator RTT at refresh).
    let mut taper_cache: Option<(f64, Option<(u64, u64)>, Duration)> = None;
    let mut taper_cache_syms: usize = 0;
    let mut taper_cache_at_us: u64 = 0;
    /// Staleness bound for the cached taper/span math (µs): one burst at
    /// the service wall is ~3 ms; 50 ms only binds on low-rate paths.
    const TAPER_CACHE_MAX_AGE_US: u64 = 50_000;

    // feat/c8-conversion DIAGNOSIS gauges (goal-gate "C8 Slow-Path
    // Conversion", RWM_DIAG only — behavior-inert): why don't slow-path
    // symbols CONVERT to delivered goodput at the heterogeneous dual cell?
    //  * c8c_src_placed[p]  — cumulative FIRST source placements per path
    //    (candidate (a), placement starvation: compare against the path's
    //    capacity share from btlbw/qdisc truth).
    //  * c8c_retx_orig[p]   — cumulative targeted retransmits whose ORIGINAL
    //    placement path was p (candidate (d), arrival-misalignment: slow-
    //    placed symbols being re-served spuriously shows up as
    //    retx_orig[slow]/src_placed[slow] ≫ the path's realized loss rate).
    //  * c8c_stall_ms/n[p]  — cumulative frontier-stall wall time (ack
    //    advance gaps ≥ 5 ms) attributed to the OWNER path of the blocking
    //    hole seq = prev_ack+1 at resolution (candidate (c), HoL coupling:
    //    which path's holes serialize the cumulative frontier).
    //    The receiver-side [C8CONV-R] gauge carries the arrival-side view
    //    (first-copy vs duplicate per path + frontier lead + unblock
    //    attribution — candidates (a)/(b)/(d)).
    // Declared BEFORE `send_source_symbol!` (macro_rules hygiene: the macro
    // body's identifiers resolve against bindings visible at the DEFINITION
    // site).
    let mut c8c_src_placed: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut c8c_retx_orig: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut c8c_stall_ms: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut c8c_stall_n: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut c8c_last_ack_adv_us: u64 = 0;

    // Helper macro: feed a framed symbol to encoder + send + stats + repair debt
    macro_rules! send_source_symbol {
        ($framed:expr) => {{
            let wire_sym = encoder.add_source(&$framed);
            // GDIAG/GLIFE fill tracking: stamp the generation's first-source
            // and sealed instants (RWM_DIAG only; no-op on the shipped path).
            if diag_on && generation {
                let seq = wire_sym.block_id;
                let anchor = seq - (seq % gen_size as u64);
                let e = gl.entry(anchor).or_insert((0, 0, 0));
                if e.0 == 0 {
                    e.0 = now_us();
                }
                if seq % gen_size as u64 == gen_size as u64 - 1 {
                    e.1 = now_us();
                }
            }
            gen_last_source_us = now_us();

            // RWM Phase A retention: the store keeps the sent bytes until
            // the peer acks them — the coding window may slide past this
            // symbol, but the data can no longer be destroyed by eviction.
            // Generation coding turns per-seq ARQ OFF, so it needs NO sent
            // store (recovery is more coded symbols for the generation, never
            // an exact-seq resend); backpressure uses the encoder's retained
            // size instead. The GenerationEncoder itself retains the sources.
            if reliable && !generation {
                sent_store.insert(wire_sym.block_id, wire_sym.clone());
            }

            // Send source symbol. RWM Phase B (§16.3): in reliable multipath
            // mode, stripe by the per-symbol placement law (softmax over
            // marginal cost); single path collapses to that path (byte-
            // identical to Phase A). Non-reliable (realtime/EVICT) mode keeps
            // the single best-path pick + redundant duplicate, unchanged.
            // feat/store-borrowing: when this placement is a LOAN, the
            // account charged (the lender) differs from the flight path.
            // None = charge the flight path (the non-borrow default).
            let mut borrow_lender: Option<u32> = None;
            let source_path = {
                if reliable {
                    let picked = {
                        let sched = scheduler.lock();
                        sched.place_symbol(false, &[]).unwrap_or(0)
                    };
                    // task #86 (RWM_STORE_PERCAP): the admission gate only
                    // admits while SOME path's account has headroom — land
                    // the symbol there. A cap-full pick is redirected to the
                    // live path with the most relative account headroom, so
                    // the shallow path is never over-committed past its own
                    // pipe while the deep path keeps deepening.
                    if !percap_caps.is_empty() {
                        let accounts: Vec<(crate::scheduler::PathId, usize, usize, usize)> =
                            percap_caps
                                .iter()
                                .map(|(&pid, &cap)| {
                                    (
                                        pid,
                                        percap_out.get(&pid).copied().unwrap_or(0),
                                        cap,
                                        // Roadmap item 1: the delay-aware
                                        // redirect bound (= cap when the
                                        // guard is off).
                                        percap_bounds.get(&pid).copied().unwrap_or(cap),
                                    )
                                })
                                .collect();
                        // feat/store-borrowing (§16.22.4): BORROW FIRST —
                        // a pick landing on a cap-full account stays on
                        // its picked PIPE, charged to the lender with the
                        // most lend room; else the guarded redirect; else
                        // keep-chosen (the gate reads FULL next
                        // iteration: backpressure, don't park). Own picks
                        // below cap are never touched.
                        let own_open = accounts
                            .iter()
                            .any(|&(p, out, cap, _)| p == picked && out < cap.max(1));
                        if percap_borrow_on && !own_open {
                            let baccts: Vec<BorrowAccount> = accounts
                                .iter()
                                .map(|&(p, out, cap, _)| {
                                    let (rate, rtprop_s) = percap_rr
                                        .get(&p)
                                        .copied()
                                        .unwrap_or((None, None));
                                    BorrowAccount {
                                        path: p,
                                        out,
                                        cap,
                                        fly: out
                                            .saturating_sub(
                                                percap_lent.get(&p).copied().unwrap_or(0),
                                            )
                                            .saturating_add(
                                                percap_borrowed
                                                    .get(&p)
                                                    .copied()
                                                    .unwrap_or(0),
                                            ),
                                        rate,
                                        rtprop_s,
                                    }
                                })
                                .collect();
                            match percap_borrow_lender(picked, &baccts) {
                                Some(lender) => {
                                    borrow_lender = Some(lender);
                                    picked
                                }
                                None => percap_place_path(picked, &accounts),
                            }
                        } else {
                            percap_place_path(picked, &accounts)
                        }
                    } else {
                        picked
                    }
                } else {
                    let sched = scheduler.lock();
                    select_source_path(&sched)
                }
            };
            last_source_path = source_path;
            // ADR-0046 idle-triggered recovery: stamp the last NEW-source send
            // so the NACK throttle can tell "actively pushing data" (repairs
            // would load a congested path) from "idle except for a hole"
            // (targeted recovery is free).
            last_source_send_us = now_us();
            // Fungible frontier (§16.3): in coded-only mode the wire carries a
            // fresh random linear combination over the CURRENT window (which
            // now includes this just-added source) instead of the raw
            // systematic symbol. Any K independent such combinations, from any
            // path, reconstruct the K window sources — so a coded symbol lost
            // on the slow path is one interchangeable degree of freedom, not a
            // fixed in-order position (removing the §16.7 long-pole cap). The
            // systematic bytes remain in the encoder window + retention store
            // for the targeted-ARQ backstop on aged holes.
            // Generation coding decouples coded emission from source intake:
            // add_source only FILLS the generation here; the paced token-bucket
            // block in the main loop does ALL wire sends (so coded keeps flowing
            // to complete buffered generations even while TUN reads are paused by
            // backpressure — the source-driven emission alone serializes and
            // stalls). So skip the per-source wire send entirely in this mode.
            // Systematic-repair (§16.3 oracle): the RAW source rides the wire as
            // PRIMARY here (striped ∝-goodput via the place_symbol pick above,
            // delivered out-of-order with ZERO decode). Coded repair is emitted
            // separately in the paced generation block (only ceil(len·r) per
            // generation + deficit top-up). Coded-only generation mode SKIPS the
            // per-source send (all its emission is the paced coded block). Both
            // generation submodes keep per-seq ARQ / sent_store / taper repair
            // OFF (gated on `!generation` below), so systematic adds only the
            // source wire-send, nothing else.
            if systematic || !generation {
                let on_wire = if systematic {
                    wire_sym.clone() // raw systematic source is the primary
                } else if coded_wire {
                    encoder.generate_repair()
                } else {
                    wire_sym.clone()
                };
                let batch_seq = batch_counter.fetch_add(1, Ordering::Relaxed);
                let batch = SymbolBatch {
                    symbols: vec![on_wire],
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
                        // feat/copa-sole-cc: record the seq→path commitment +
                        // the BBR rate-sample send snapshot so this seq's
                        // eventual WindowAck attribution yields a clean
                        // SEND-interval delivery-rate sample on this path.
                        // (Bulk back-to-back sends: app_limited = false; an
                        // under-read sample can never lower the max filter.)
                        // feat/window-mtu scope fix: a PAUSED N1-scoped feed
                        // must behave as ABSENT — charging src_inflight /
                        // snapshotting rate samples without the (paused)
                        // attribution to release them leaked src_inflight
                        // ~165k and starved the anchor at duals (measured:
                        // c7-fix 64 Mbit, cap collapsed to boot 128).
                        if let Some(feed) = copa_feed.as_ref().filter(|f| !f.n1_paused()) {
                            feed.on_sent(wire_sym.block_id, source_path);
                            p.charge_src(1);
                            p.on_src_sent(wire_sym.block_id, false);
                        }
                    }
                }
                if let Some(ps) = stats.path(source_path) {
                    ps.symbols_sent.fetch_add(1, Ordering::Relaxed);
                }
                stats.fec.total_source_symbols.fetch_add(1, Ordering::Relaxed);
                source_symbols_this_period += 1;
                // Fix 1: charge the paced source send against the link-rate
                // token bucket (the TUN-read gate refills + admits it).
                if cc_pace {
                    src_tokens -= 1.0;
                }
            }

            // Track which path this source was sent on (for cross-path retransmission)
            source_path_map.insert(wire_sym.block_id, source_path);
            // feat/c8-conversion DIAG: per-path FIRST source placement count.
            if diag_on {
                *c8c_src_placed.entry(source_path).or_insert(0) += 1;
            }

            // task #86: charge this seq to its placement path's outstanding
            // account, in lockstep with the sent_store insert above (percap_on
            // ⊆ plain_dyn_cap ⊆ the reliable && !generation retention mode).
            // Released only by the ack that removes it from the store. A
            // cross-path retransmit does NOT re-attribute: the account bounds
            // the pipe the symbol was ADMITTED against (its dwell there ends
            // at the same ack either way. percap_track ⊇ percap_on: under
            // RWM_DIAG the maps are maintained as a gauge only — see decl).
            if percap_track {
                // feat/store-borrowing: a LOAN charges the LENDER's account
                // while the symbol flies on `source_path` (§16.22.1 — the
                // ledger moves, the wire placement does not). The loan
                // ledger corrects the pipe gauge (fly = out − lent +
                // borrowed) and repays on the same ack that releases the
                // account entry.
                let charge_path = borrow_lender.unwrap_or(source_path);
                percap_charge(&mut percap_acct, &mut percap_out, wire_sym.block_id, charge_path);
                if let Some(lender) = borrow_lender {
                    percap_loan_charge(
                        &mut percap_loans,
                        &mut percap_lent,
                        &mut percap_borrowed,
                        wire_sym.block_id,
                        lender,
                        source_path,
                    );
                    percap_loans_total += 1;
                }
            }

            // Add to retransmit buffer for P_lost-based retransmit decisions.
            // Generation coding disables per-seq ARQ entirely — no retransmit
            // buffer (so the P_lost retransmit branch never fires and the tail
            // ARQ sweep never arms) and no per-seq deficit accounting. Recovery
            // is generation-level (more coded symbols for a short generation).
            if !generation {
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
            // Generation coding does ALL coded emission in the ack-clocked
            // flow-control block in the main loop, so the per-source taper repair
            // is disabled here (it would double-emit and fight the flow control).
            if !generation && encoder.window_size() > 1 {
                // RWM_EMIT_BATCH: the derived taper/span math refreshes at
                // burst granularity; per-symbol (bit-identical) when OFF.
                let taper_recompute = !emit_batch_live
                    || taper_cache.is_none()
                    || taper_cache_syms >= emit_burst
                    || now_us().saturating_sub(taper_cache_at_us)
                        > TAPER_CACHE_MAX_AGE_US;
                let (repair_rate, span_params) = if !taper_recompute {
                    let (rr, span, rtt) = taper_cache.unwrap();
                    taper_cache_syms += 1;
                    // The A* send-rate anchor is FED per symbol regardless —
                    // the cache amortizes only the derived recomputation.
                    if unified_span && astar_anchor_on {
                        astar_anchor.on_send(Instant::now(), 1, rtt);
                    }
                    (rr, span)
                } else {
                let (repair_rate, span_params, taper_rtt) = {
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
                            let taper = crate::control::TaperFunction::from_estimator(est, flat_rate);
                            let rr = if taper_r_budget {
                                // #85 budget law (see TaperBudget decl above):
                                // emission tracks r × source per coding window
                                // — the computed r* is consumed at the wire.
                                taper_budget.accrue(
                                    flat_rate,
                                    taper_offset,
                                    &taper,
                                    encoder.window_size(),
                                    spare,
                                )
                            } else {
                                // LEGACY (measured-inert): taper density at the
                                // current offset; Σ over an ack cycle = r once.
                                let density = taper.density(taper_offset as f64);
                                // Cap by spare capacity (never exceed link headroom)
                                density.min(spare.max(0.0))
                            };
                            // §16.20.3 span parameters (A*, Δ) from the same
                            // measured anchors — see the unified_span decl.
                            let span = if unified_span {
                                let rate_sym = if astar_anchor_on {
                                    // feat/anchor-hygiene: this block runs
                                    // once per SOURCE symbol send — feed the
                                    // windowed-max send-rate anchor here and
                                    // read it back (sym/s directly; no
                                    // byte/EWMA detour). None before the
                                    // first measured bucket ⇒ A* clamps to 1
                                    // — the honest cold-start, ~SRTT/2 long.
                                    let now_i = Instant::now();
                                    astar_anchor.on_send(now_i, 1, est.rtt());
                                    astar_anchor.rate(now_i, est.rtt()).unwrap_or(0.0)
                                } else {
                                    (est.throughput() / symbol_size.max(1) as f64).max(0.0)
                                };
                                let rtprop = est.rtt().as_secs_f64();
                                let b = match protocol_hint {
                                    ProtocolHint::Realtime => 0.5,
                                    ProtocolHint::Auto => 1.0,
                                    ProtocolHint::Bulk => 2.0,
                                };
                                let d = (b * rtprop).min(2.0 * rtprop);
                                let a_star = ((rate_sym * d).ceil() as u64)
                                    .clamp(1, encoder.window_size() as u64);
                                let delta = ((rate_sym * (est.jitter_us() / 1e6)).ceil()
                                    as u64)
                                    .clamp(1, 64);
                                // δ-honest shed law: refresh the derived
                                // deadline D(δ) and the 1−ρ budget at the
                                // live operating point (ε̂, r*, A*, σ²) —
                                // same anchors, no new constants.
                                if shed_on {
                                    shed_deadline_us_live = shed_deadline_us(
                                        b,
                                        est.rtt().as_micros() as u64,
                                    );
                                    shed_budget_frac =
                                        crate::control::fec_rate::residual_loss_after_fec(
                                            est.loss_rate(),
                                            flat_rate,
                                            a_star as f64,
                                            crate::control::fec_rate::burst_variance_factor(est),
                                        );
                                }
                                Some((a_star, delta))
                            } else {
                                None
                            };
                            (rr, span, est.rtt())
                        }
                        None => (0.0, None, Duration::from_millis(50)),
                    }
                };
                taper_cache = Some((repair_rate, span_params, taper_rtt));
                taper_cache_syms = 1;
                taper_cache_at_us = now_us();
                (repair_rate, span_params)
                };
                // diag/unified-collapse: span-law sender trace (RWM_DIAG only).
                if diag_on && unified_span {
                    let dnow = now_us();
                    if dnow.saturating_sub(span_diag_last_us) > 500_000 {
                        span_diag_last_us = dnow;
                        let (ws, we) = encoder.window_span();
                        let ack = window_ack_seq.load(Ordering::Relaxed);
                        let transit = transport
                            .l0_transit_stats()
                            .map(|(e, g, td, ok, er, q)| {
                                format!(
                                    " | shim enq={e} ge={g} tail={td} ok={ok} err={er} q={q}"
                                )
                            })
                            .unwrap_or_default();
                        let dg = transport
                            .datagram_frame_stats(source_path)
                            .map(|(rx, tx)| format!(" dg_rx={rx} dg_tx={tx}"))
                            .unwrap_or_default();
                        // feat/anchor-hygiene: the A* anchor gauge (windowed-
                        // max send rate + gap-discard counters) when active.
                        let ah = if astar_anchor_on {
                            let (g, d) = astar_anchor.stats();
                            format!(
                                " ar={:.0} agap={}/{}",
                                astar_anchor
                                    .rate(Instant::now(), Duration::from_millis(50))
                                    .unwrap_or(0.0),
                                g,
                                d
                            )
                        } else {
                            String::new()
                        };
                        // δ-honest shed gauge (fix C): cumulative shed /
                        // budget-refused counts, the live 1−ρ fraction and
                        // deadline — the law's liveness at the sender.
                        let shg = if shed_on {
                            format!(
                                " shed={}/{} bud={:.4} D={}ms",
                                shed_total,
                                shed_denied,
                                shed_budget_frac,
                                shed_deadline_us_live / 1000,
                            )
                        } else {
                            String::new()
                        };
                        eprintln!(
                            "[SPAN] t={:.1}s ack={} win=[{},{}] wsize={} a_star={:?} delta={:?} owed={:.2} rr={:.3} debt={:.2} retx_buf={}{}{}{}{}",
                            dnow.saturating_sub(span_diag_start_us) as f64 / 1e6,
                            ack,
                            ws,
                            we,
                            encoder.window_size(),
                            span_params.map(|(a, _)| a),
                            span_params.map(|(_, d)| d),
                            taper_budget.owed(),
                            repair_rate,
                            repair_debt,
                            retransmit_buffer.len(),
                            shg,
                            ah,
                            transit,
                            dg,
                        );
                    }
                }
                // RWM Phase C raise-r arm (§16.5): floor the per-symbol
                // repair rate to make the window rateless-fungible. Applied
                // AFTER the spare cap on purpose — the experiment forces the
                // bandwidth spend to test aggregation, on links with headroom.
                let repair_rate = repair_rate.max(repair_rate_floor);
                // Generation coding: a small proactive overhead per generation
                // (the oracle's r ≈ 0.10) so a generation carries K_G(1+r) coded
                // symbols and decodes without waiting on a recovery round for
                // the expected loss. Beyond this, the frontier-retention keeps
                // coding any still-short generation until it decodes (fungible,
                // no per-seq ARQ). RWM_GEN_R overrides.
                let repair_rate = if generation {
                    repair_rate.max(gen_repair_floor)
                } else {
                    repair_rate
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
                        let oldest = retransmit_buffer
                            .iter()
                            .next()
                            .map(|(&s, &v)| (s, v));
                        if let Some((seq, (send_time_us, eps_at_send, _path))) = oldest {
                            let age_secs = (now.saturating_sub(send_time_us)) as f64 / 1_000_000.0;
                            let p = crate::control::fec_rate::p_lost(age_secs, eps_at_send, srtt_secs, rttvar_secs);
                            // Paper Section 3.4: P(retransmit) = P_lost(t_k).
                            // Probabilistic — smooth transition from FEC to ARQ.
                            if rand::random::<f64>() < p {
                                // δ-honest shed (fix C): a candidate older
                                // than D(δ) arrives after the receiver's
                                // δ-horizon give-up — retransmitting it only
                                // serializes the stream behind a missed
                                // deadline. Shed it (within the ρ budget)
                                // and let this correction slot do fresh
                                // span-repair work instead.
                                let age_us_c = now.saturating_sub(send_time_us);
                                if shed_on
                                    && shed_allowed(
                                        age_us_c,
                                        shed_deadline_us_live,
                                        shed_total,
                                        stats.fec.total_source_symbols.load(Ordering::Relaxed),
                                        shed_budget_frac,
                                    )
                                {
                                    retransmit_buffer.remove(&seq);
                                    nack_retx_at.remove(&seq);
                                    shed_seqs.insert(seq);
                                    shed_total += 1;
                                } else {
                                    if shed_on
                                        && shed_deadline_us_live > 0
                                        && age_us_c > shed_deadline_us_live
                                    {
                                        // Past deadline but ρ-budget-refused:
                                        // the serialize arm (visible in DIAG).
                                        shed_denied += 1;
                                    }
                                    use_retransmit = true;
                                    retransmit_seq = seq;
                                }
                            }
                        }

                        if use_retransmit && diag_on {
                            // feat/recovery-suppression trace: the P_lost-
                            // branch retransmit channel (fed by eps_at_send).
                            mpd_plost_retx += 1;
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
                        } else if let Some((a_star, delta)) = span_params {
                            // §16.20.3 trailing solvable-span placement: code
                            // over [max(ws, end−A*), end) with end = newest+1−Δ
                            // — every member already landed when the repair
                            // does (FIFO + jitter guard), so the receiver's
                            // incremental GE solves a covered hole AT ARRIVAL
                            // instead of entangling it with in-flight symbols
                            // (the #85 leading-window defect, removed
                            // structurally). Falls back to the leading-window
                            // repair when the window is too young to trail.
                            let (ws, we) = encoder.window_span();
                            let end = (we + 1).saturating_sub(delta);
                            let start = end.saturating_sub(a_star).max(ws);
                            if end > start {
                                encoder
                                    .generate_repair_range(
                                        start,
                                        (end - start).min(u16::MAX as u64) as u16,
                                    )
                                    .unwrap_or_else(|| encoder.generate_repair())
                            } else {
                                encoder.generate_repair()
                            }
                        } else {
                            // Repair: generate a new FEC symbol (legacy
                            // leading-window emission)
                            encoder.generate_repair()
                        }
                    };

                    // RWM Phase B (§16.3): reliable multipath places the
                    // correction by the law with the ρ_fate penalty against the
                    // paths that carried the window symbols it covers (the
                    // continuous form of best_repair_path_avoiding). Single path
                    // ⇒ that path. Non-reliable keeps the best-goodput pick.
                    let correction_path = {
                        let sched = scheduler.lock();
                        if reliable {
                            let covered = window_source_paths(&*encoder, &source_path_map);
                            sched.place_symbol(true, &covered).unwrap_or(source_path)
                        } else {
                            select_repair_path(&sched, source_path)
                        }
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

    // RWM_DIAG (transport-ceiling diagnosis): once per ~250 ms emit one line
    // isolating the binding single-connection constraint — window occupancy vs
    // store_max, tx_paused duty cycle, cumulative-ack goodput (Mbit/s), the
    // ack-clocked pacing rate vs the link, cwnd/in_flight vs BDP, and the
    // source/coded send rates. Gated on the RWM_DIAG env so the hot path is
    // untouched when off. (`diag_on` itself is declared above the send macro.)
    // Transport-ceiling fix (generation mode): bound the in-flight (unacked)
    // symbols to ~BDP instead of the fixed store_max = G·(M+1). The oversized
    // store_max is decoupled from the pipe (14× BDP at C2), so unpaced source
    // emission builds a multi-hundred-ms standing queue (MEASURED RTT inflated
    // to 0.5–1.3 s), which turns every hole into a ~1 s recovery stall. Cap
    // total in-flight at a BDP-scaled bound so the queue — and thus the
    // recovery-stall latency — stays small. 0 = off (legacy store-only
    // backpressure). The deficit-recovery emission is EXEMPT (it must always be
    // able to fund a frontier hole, else a full-window pipe deadlocks).
    let infl_cap: u64 = gates.infl_cap;
    // PART 1.2 (receiver-tail): BDP-DERIVED in-flight cap. A fixed RWM_INFL_CAP
    // must be hand-tuned per RTT; instead bound total in-flight to
    // gain × Σ copa_bdp_anchor (BtlBw×RTprop, bufferbloat-robust) recomputed
    // live, so the standing queue — and thus the RECOVERY-ROUND RTT — stays
    // ~gain·BDP at ANY RTT. It gates BOTH proactive emission AND (Fix-2
    // non-exempt) reactive/deficit recovery via `cwnd_full`, so the parallel
    // tail flush cannot re-bloat the queue. Env RWM_INFL_BDP=gain (e.g. 2.0);
    // 0/unset = off (legacy static RWM_INFL_CAP / store-only backpressure).
    // gen_pipe remedy 1: the per-path BDP in-flight cap ON (gain 1.5 — the
    // FMTCP-era oracle PART 5c finding: the bare aggregate BDP starves the
    // recovery headroom; ~1.5× over the windowed-max — hence under-estimating —
    // anchor gives the emergent ~1.3× BDP operating point) so the standing
    // queue — and the RTT the SUBSTRATE CC sees — stays ≈ RTprop.
    let infl_bdp_gain: f64 = gates
        .infl_bdp
        .unwrap_or(if gen_pipe { 1.5 } else { 0.0 })
        .max(0.0);
    let infl_bdp_on = infl_bdp_gain > 0.0;
    // The #64 fix (FMTCP-era, retained under gen_pipe): enforce the in-flight
    // cap PER PATH (path i outstanding ≤ gain·BtlBw_i·RTprop_i) rather than as
    // one fungible global Σ budget. The sender is TUN-paused only when EVERY
    // active path is at its own cap, so the fast path keeps pulling fresh
    // source while the slow path is full.
    let infl_percap = gen_pipe;
    // Boot cap before the BtlBw anchor warms (a few RTTs); ~1.5× a 100 Mbit/
    // 10 ms BDP, same rationale as the plain-reliable store_boot_cap.
    let mut dyn_infl_cap: u64 = if infl_bdp_on { 128 } else { infl_cap };
    let mut dyn_infl_refresh_us: u64 = 0;
    // Transport-ceiling fix (generation mode): clock the coded-emission budget
    // to the SENT source frontier instead of the ACKED frontier. The
    // ack-clocked `target = ack·(1+r) + W` DEADLOCKS a small generation: once
    // the proactive budget W is spent, coded stops until the ack advances — but
    // the ack is stalled precisely because the frontier generation is missing
    // the coded it needs to decode (MEASURED: G=96 wedges with in_flight=0,
    // src=0, cod=0). Sourcing the budget from the sent frontier lets the
    // encoder's own per-generation ceil(K_g·(1+r)) cap + the M-generation
    // retention bound govern coded emission (both already bound the datagram
    // buffer), so proactive coverage always completes and small generations —
    // which keep the store near BDP and avoid the bufferbloat stall — work.
    let coded_src_clock = gates.coded_src;
    // PURE-PROACTIVE demonstrator (proactive-FEC-vs-ARQ crossover, directive #4):
    // when set, DISABLE the deficit-driven reactive recovery loop entirely. All
    // recovery then comes from the UPFRONT proactive per-generation budget
    // (ceil(len·r)) — no NACK/deficit round-trips, and (crucially) no
    // recovery-emission path that is EXEMPT from the in-flight congestion cap, so
    // every emitted symbol (systematic source + proactive coded) is bounded by
    // RWM_INFL_CAP and cannot overrun the droppable datagram path. This isolates
    // the clean question: with enough upfront repair (high r) that holes decode
    // on arrival, does proactive FEC beat ARQ at high RTT? Requires r sized to
    // cover the per-generation loss tail — a generation that loses more than its
    // budget never decodes (the object DNFs), which is itself the honest result.
    let no_reactive = gates.no_reactive;
    let diag_start_us = now_us();
    let mut diag_last_us = now_us();
    let mut diag_last_ack: u64 = 0;
    let mut diag_last_src: u64 = 0;
    let mut diag_last_cod: u64 = 0;
    let mut diag_paused_iters: u64 = 0;
    let mut diag_total_iters: u64 = 0;
    // feat/copa-wire-signal wedge forensics (RWM_DIAG only): cumulative tail
    // ARQ sweeps fired, SACK-gap retransmits actually sent, gaps discarded
    // for exhausted budget, and the live budget/cap values — the wedge shows
    // good=0 with in_flight=0 for tens of seconds and these name which stage
    // of the reactive-repair chain is dead.
    let mut diag_sweeps: u64 = 0;
    let mut diag_retx: u64 = 0;
    let mut diag_gaps_dropped: u64 = 0;
    let mut diag_eff_rate: f64 = 0.0;
    // diag/lossy-residual (goal-gate "Lossy-Single Residual", RWM_DIAG only):
    // sender EMISSION-GAP gauge — cumulative time in inter-emission gaps
    // ≥ 3 ms (src+cod handoffs to the transport observed per loop iteration;
    // the loop wakes ≥ every 1 ms, so gap edges are observed within ~1 ms).
    // Prices accounting term (b): engine-caused wire idle during recovery
    // rounds. `sidle=<cum ms>/<n>/<max ms>` in the [DIAG] line; the receiver's
    // [WIDLE] inter-arrival gauge is the wire-truth counterpart.
    const SIDLE_GAP_MIN_US: u64 = 3_000;
    let mut sidle_last_total: u64 = 0;
    let mut sidle_last_change_us: u64 = now_us();
    let mut sidle_us: u64 = 0;
    let mut sidle_n: u64 = 0;
    let mut sidle_max_us: u64 = 0;
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
    let mut wnd2_relgap_max_us: u64 = 0;
    loop {
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
                if store_sack_release_on {
                    // SACK-clocked store release: uncount the slot (window
                    // opens, pool/account freed) — KEEP the payload and
                    // every recovery structure (retransmit_buffer,
                    // nack_retx_at + its per-flight RWM_RECOV_MP loss
                    // clocks, source_path_map) until the cumulative
                    // frontier passes. sack_release_mark skips seqs
                    // already released — no double-release.
                    let newly =
                        sack_release_mark(&sent_store, &mut sack_released, start, end);
                    sack_released_total += newly.len() as u64;
                    if percap_track {
                        for &k in &newly {
                            // Per-path account slot freed on delivery
                            // evidence (idempotent: cumulative release
                            // later finds the seq already gone — the
                            // documented no-double-release contract).
                            percap_release_seq(&mut percap_acct, &mut percap_out, k);
                            if percap_borrow_on {
                                percap_loan_release(
                                    &mut percap_loans,
                                    &mut percap_lent,
                                    &mut percap_borrowed,
                                    k,
                                );
                            }
                        }
                    }
                }
            }
        }

        // ── Frontier-slack refresh (RWM_PLACE_SLACK, 5 ms cadence) ────────
        // S = clamp(span/R_ack, 0, 250 ms); R_ack sampled on >= 50 ms
        // windows of cumulative-ack advance (delivery truth). S stays 0 —
        // the shipped-identical operating point — until R_ack warms or
        // while N < 2 live paths.
        if place_slack_on {
            let pnow = now_us();
            if pnow.saturating_sub(ps_refresh_us) >= 5_000 {
                ps_refresh_us = pnow;
                let ack_now = window_ack_seq.load(Ordering::Relaxed);
                if ps_rate_last_us == 0 {
                    ps_rate_last_us = pnow;
                    ps_rate_last_ack = ack_now;
                } else if pnow.saturating_sub(ps_rate_last_us) >= 50_000 {
                    let dt = pnow.saturating_sub(ps_rate_last_us) as f64 / 1e6;
                    let inst = ack_now.saturating_sub(ps_rate_last_ack) as f64 / dt;
                    ps_rate_ewma = if ps_rate_ewma > 0.0 {
                        0.8 * ps_rate_ewma + 0.2 * inst
                    } else {
                        inst
                    };
                    ps_rate_last_us = pnow;
                    ps_rate_last_ack = ack_now;
                }
                // span = the live stream span (max retained seq − cum ack);
                // the retention store's last key IS the sent edge (removal
                // is by cumulative ack only).
                let span = sent_store
                    .keys()
                    .next_back()
                    .copied()
                    .unwrap_or(ack_now)
                    .saturating_sub(ack_now) as f64;
                let mut slack = 0.0;
                {
                    let mut sched = scheduler.lock();
                    if ps_rate_ewma > 1.0 && sched.live_paths().len() >= 2 {
                        slack = (span / ps_rate_ewma).clamp(0.0, 0.25);
                    }
                    sched.set_place_slack(slack);
                }
                ps_slack_gauge = slack;
            }
        }

        // RWM_EMIT_BATCH scope check (see the gate decl): batching engages
        // only while exactly ONE path is live; re-checked every iteration so
        // path flaps re-scope within one burst. Gate-off pays nothing.
        if emit_batch_on {
            emit_batch_live = scheduler.lock().live_paths().len() == 1;
        }

        // Determine if packer has pending data for flush timer
        let packer_pending = use_packing && packer.is_pending();

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
            encoder.window_size()
        } else {
            sack_release_outstanding(sent_store.len(), sack_released.len())
        };
        // gen_pipe: roll the windowed-MAX rate filter + recompute the derived
        // pipeline depth M* (throttled ~5 ms; the encoder setter is O(1)).
        // feat/anchor-hygiene: under RWM_MSTAR_ANCHOR the bucket span drops
        // 2 s → 500 ms (hygiene rule 1: the anchor seeds from the first
        // measured acks, not after a multi-second pin; the max over 8 buckets
        // keeps a comparable window).
        if gen_pipe {
            let (gp_bucket_us, gp_ring) =
                if mstar_anchor { (500_000u64, 8usize) } else { (2_000_000u64, 4usize) };
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
                let m = gen_pipe_depth(gp_rate_max, rtprop_s, gen_size);
                if m != gen_pipe_m {
                    if diag_on {
                        eprintln!(
                            "[GPIPE] M* {}→{} (rate_max={:.0}sym/s rtprop={:.1}ms)",
                            gen_pipe_m, m, gp_rate_max, rtprop_s * 1000.0
                        );
                    }
                    gen_pipe_m = m;
                    encoder.set_pipeline_depth(m);
                }
                gen_pipe_store_cap = (gen_pipe_m * gen_size).min(store_max);
            }
        }
        // PART 1.2: refresh the BDP-derived in-flight cap (throttled ~5 ms).
        if infl_bdp_on {
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
                    dyn_infl_cap = ((infl_bdp_gain * bdp).ceil() as u64).max(64);
                }
            }
        }
        let eff_infl_cap = if infl_bdp_on { dyn_infl_cap } else { infl_cap };
        // In-flight (unacked) symbols across the pipe, for the BDP in-flight cap.
        // The #64 fix: also decide fullness PER PATH — the sender is "full"
        // (TUN-paused) only when NO active path is below its own cap
        // (gain·BtlBw_i·RTprop_i), so the fast path keeps pulling source while
        // the slow path is at its RTT-inflated cap. Non-gen_pipe keeps the
        // legacy global Σ in-flight ≥ Σ cap test.
        let (pipe_infl, percap_full): (u64, bool) = if eff_infl_cap > 0 {
            let mut sched = scheduler.lock();
            let mut infl = 0u64;
            let mut per_path: Vec<(u64, u64)> = Vec::new();
            for id in sched.active_paths() {
                if let Some(p) = sched.path_mut(id) {
                    p.expire_in_flight();
                    let fl = p.in_flight as u64;
                    infl += fl;
                    // Per-path cap = gain·(BtlBw_i·RTprop_i); fall back to the
                    // global boot cap before the anchor warms.
                    let cap_i = p
                        .copa_bdp_anchor()
                        .map(|b| ((infl_bdp_gain * b).ceil() as u64).max(1))
                        .unwrap_or(eff_infl_cap);
                    per_path.push((fl, cap_i));
                }
            }
            (infl, infl_percap_full(&per_path))
        } else {
            (0, false)
        };
        let cwnd_full = eff_infl_cap > 0
            && if infl_percap { percap_full } else { pipe_infl >= eff_infl_cap };
        // Plain-reliable delay-based window cap (paper §12): bound the
        // outstanding store to gain×BDP so the standing queue stays ~1 RTT and
        // loss recovery does not stall behind a bloated queue. Refreshed off
        // the scheduler lock at most every 5 ms.
        if plain_dyn_cap {
            let dnow = now_us();
            if dnow.saturating_sub(dyn_cap_refresh_us) >= 5_000 {
                dyn_cap_refresh_us = dnow;
                // Σ-cwnd store law only when the feed OWNS the operating
                // point (Copa-sole); the sampling-only feed (RWM_PLAIN_RS)
                // keeps the legacy anchor-sum law — now fed honest samples.
                if copa_feed.as_ref().is_some_and(|f| f.owns_cc()) {
                    // feat/copa-sole-cc: Copa OWNS the operating point, so the
                    // outstanding window is keyed to Σ cwnd (the probe state),
                    // not the BtlBw anchor. With the honest send-interval
                    // sampler the old 2×anchor cap is CIRCULAR: samples can
                    // never read above the store-capped delivered rate, so the
                    // anchor could never grow toward the pipe (L0 MEASURED:
                    // stuck at ~3.2k of 10.4k sym/s, throughput 18 of 66
                    // Mbit/s — the legacy ack-interval over-read was
                    // accidentally load-bearing for the old cap). cwnd escapes
                    // the loop because Copa probes it upward (ramp ×1.5,
                    // +2/SRTT, anchor pull) independent of the cap; gain×cwnd
                    // keeps ~1 cwnd of recovery runway buffered above the
                    // substrate window (quinn enforces cwnd on the wire).
                    // live_paths(), NOT active_paths(): the latter filters by
                    // spare capacity (available() > 0), and a cwnd-SATURATED
                    // path — the normal state of a wire-bound sender — made
                    // cwnd_sum read 0, collapsing the cap to the 128 boot
                    // value and whiplashing the TUN gate (MEASURED at the L1
                    // c2 smoke: effective cap flapping 1024↔128 every few
                    // DIAG ticks, store swinging 400–1024, goodput dips to
                    // 20 Mbit).
                    let (cwnd_sum, n_live, wd_terms): (f64, usize, Option<(f64, f64)>) = {
                        let sched = scheduler.lock();
                        let live = sched.live_paths();
                        let cs: f64 = live
                            .iter()
                            .filter_map(|id| sched.path(*id).map(|p| p.cwnd as f64))
                            .sum();
                        // feat/window-mtu: (rate, RTprop) for the stall meter
                        // and the retention backstop at N = 1 — the feed's
                        // delivered-rate anchor when warm, else Copa's own
                        // cwnd/RTprop (both honest under Copa-sole).
                        let wd = if win_decouple_on && live.len() == 1 {
                            live.first().and_then(|id| sched.path(*id)).and_then(|p| {
                                let rtp = p.min_rtt().map(|d| d.as_secs_f64())?;
                                if rtp <= 0.0 {
                                    return None;
                                }
                                let rate = p
                                    .btlbw_sym_per_s()
                                    .filter(|r| *r > 0.0)
                                    .unwrap_or(p.cwnd as f64 / rtp);
                                Some((rate, rtp))
                            })
                        } else {
                            None
                        };
                        (cs, live.len().max(1), wd)
                    };
                    wd_engaged = false;
                    dyn_store_cap = if let (true, Some((rate, rtp))) =
                        (win_decouple_on && cwnd_sum > 0.0, wd_terms)
                    {
                        // Decoupled law under Copa-sole: residence = Copa's
                        // own gain*cwnd (un-truncated — the B1 dwell-ceiling
                        // release); stall meter + retention backstop per the
                        // amended constants.
                        wd_allow_base = store_bdp_gain * cwnd_sum;
                        wd_rate = rate;
                        wd_cap_ret = win_decouple_cap_ret(
                            wd_allow_base,
                            rate,
                            rtp,
                            store_cap_floor,
                        );
                        wd_engaged = true;
                        wd_cap_ret
                    } else if let Some(cap) = path_scaled_store_cap(
                        store_paths_on,
                        n_live,
                        cwnd_sum,
                        store_bdp_gain,
                        store_cap_floor,
                        store_path_pool,
                    ) {
                        cap
                    } else if cwnd_sum > 0.0 {
                        ((store_bdp_gain * cwnd_sum).ceil() as usize)
                            .clamp(store_cap_floor, store_max)
                    } else {
                        store_boot_cap.min(store_max)
                    };
                } else {
                    // RWM_STORE_CAPW (goal-gate "C8-Aware Pool Law"): the
                    // capacity-weighted shared pool's per-path terms, over
                    // LIVE paths (live_paths(), NOT active_paths() — the
                    // documented cwnd-saturation filter trap above: a
                    // saturated path must keep its earned share). None until
                    // that path's anchor warms; capw_store_cap requires ALL
                    // live paths warm, else the configured fallback below.
                    let capw_terms: Vec<Option<f64>> = if capw_on {
                        let sched = scheduler.lock();
                        sched
                            .live_paths()
                            .iter()
                            .map(|id| {
                                sched.path(*id).and_then(|p| {
                                    let k = percap_k
                                        .entry(*id)
                                        .or_insert_with(|| {
                                            EchoRatioMin::new(PERCAP_K_HALF_WINDOW_US)
                                        })
                                        .observe_srtt_over_rtprop(
                                            p.srtt(),
                                            p.min_rtt(),
                                            dnow,
                                        );
                                    honest_store_cap(
                                        p.copa_bdp_anchor(),
                                        p.btlbw_sym_per_s(),
                                        k,
                                        store_bdp_gain,
                                    )
                                })
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    // feat/percap-honest-cap: alongside the legacy Σanchor
                    // base, accumulate the honest per-path cap sum
                    // Σ anchor_i·(K_i+gain−1) when the honest sampler is
                    // live (see `honest_store_cap`; K_i observed here at
                    // the refresh cadence). hsum = 0.0 whenever
                    // honest_cap_on is false — the legacy expressions below
                    // then run verbatim (shipped byte-identical).
                    let (bdp, hsum, n_live, wd_terms): (
                        f64,
                        f64,
                        usize,
                        Option<(f64, f64, f64, f64)>,
                    ) = {
                        let sched = scheduler.lock();
                        let n = sched.live_paths().len().max(1);
                        let mut bdp = 0.0f64;
                        let mut hsum = 0.0f64;
                        // feat/window-mtu: (anchor, rate, K, RTprop) for the
                        // decoupled law at N = 1 (anchor honest via the
                        // N1-scoped sampling feed).
                        let mut wd: Option<(f64, f64, f64, f64)> = None;
                        for id in sched.active_paths().iter() {
                            if let Some(p) = sched.path(*id) {
                                if let Some(a) = p.copa_bdp_anchor() {
                                    bdp += a;
                                    if honest_cap_on || (win_decouple_on && n == 1) {
                                        let k = percap_k
                                            .entry(*id)
                                            .or_insert_with(|| {
                                                EchoRatioMin::new(PERCAP_K_HALF_WINDOW_US)
                                            })
                                            .observe_srtt_over_rtprop(
                                                p.srtt(),
                                                p.min_rtt(),
                                                dnow,
                                            );
                                        if honest_cap_on {
                                            hsum += honest_store_cap(
                                                Some(a),
                                                p.btlbw_sym_per_s(),
                                                k,
                                                store_bdp_gain,
                                            )
                                            .unwrap_or(0.0);
                                        }
                                        if win_decouple_on && n == 1 {
                                            if let (Some(r), Some(rtp)) = (
                                                p.btlbw_sym_per_s().filter(|r| *r > 0.0),
                                                p.min_rtt().map(|d| d.as_secs_f64()),
                                            ) {
                                                wd = Some((a, r, k, rtp));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        (bdp, hsum, n, wd)
                    };
                    // feat/window-mtu: the N1-scoped sampler pause (see
                    // CopaFeed::n1_pause) — refreshed here at the dyn-cap
                    // cadence. Never touches RWM_PLAIN_RS or Copa-sole
                    // feeds (their semantics are unchanged).
                    if win_decouple_on && !gates.plain_rs {
                        if let Some(f) = &copa_feed {
                            if !f.owns_cc() {
                                f.set_n1_paused(n_live >= 2);
                            }
                        }
                    }
                    wd_engaged = false;
                    dyn_store_cap = if let (true, Some((a, r, k, rtp))) =
                        (win_decouple_on && n_live == 1, wd_terms)
                    {
                        // Decoupled law (part 1, plain/BBR seat): residence
                        // on the honest anchor + probe headroom; the stall
                        // meter and hole capacity live in the gate below and
                        // the retention backstop respectively.
                        wd_allow_base = a * (k.max(1.0) + store_bdp_gain - 1.0);
                        wd_rate = r;
                        wd_cap_ret = win_decouple_cap_ret(
                            wd_allow_base,
                            r,
                            rtp,
                            store_cap_floor,
                        );
                        wd_engaged = true;
                        wd_cap_ret
                    } else if let Some(cap) = capw_store_cap(
                        capw_on,
                        &capw_terms,
                        store_cap_floor,
                        store_path_pool,
                    ) {
                        // Capacity-weighted shared pool ENGAGED (N ≥ 2, all
                        // anchors warm): Σ honest per-path caps, clamped to
                        // [floor, N×knee]. Takes precedence over the hsum /
                        // path-scaled laws — this IS the pool law under test.
                        cap
                    } else if honest_cap_on && hsum > 0.0 {
                        // Honest law: the Σ is already per-path-composed
                        // (each term carries its own K_i and runway), so no
                        // gain× multiplier here. Principled ceilings
                        // unchanged: the legacy store latch at N = 1, the
                        // N×knee pool when the path-scaled pool is
                        // configured.
                        let ceiling = if store_paths_on && n_live >= 2 {
                            n_live.saturating_mul(store_path_pool).max(store_cap_floor)
                        } else {
                            store_max
                        };
                        (hsum.ceil() as usize).clamp(store_cap_floor, ceiling)
                    } else if let Some(cap) = path_scaled_store_cap(
                        store_paths_on,
                        n_live,
                        bdp,
                        store_bdp_gain,
                        store_cap_floor,
                        store_path_pool,
                    ) {
                        cap
                    } else if bdp > 0.0 {
                        ((store_bdp_gain * bdp).ceil() as usize).clamp(store_cap_floor, store_max)
                    } else {
                        store_boot_cap.min(store_max)
                    };
                }
                // ── task #86: per-path account caps (RWM_STORE_PERCAP) ────
                // Computed AFTER the pooled laws above so (a) the shipped /
                // STORE_PATHS expressions stay verbatim (default byte-
                // identical), and (b) the warm-up share inherits the pooled
                // cap in force (`dyn_store_cap` as just computed). Engaged
                // only for N ≥ 2 live paths: percap_caps stays EMPTY at
                // N = 1, so singles run the legacy gate bit-exactly even
                // with the flag ON. pipe_i = Copa cwnd_i under the feed
                // (Copa's operating point is the per-path pipe), else
                // BtlBw_i × echo-SRTT_i — the delivered-rate anchor times
                // the ACK-clock residence time (Little's law on the store;
                // the per-path pool knee bounds the echo-RTT feedback).
                percap_caps.clear();
                percap_bounds.clear();
                if percap_on {
                    // (pipe_i for the cap law, floor_pipe_i for the redirect
                    // guard). Plain: pipe = rate×echoSRTT (loaded clock, the
                    // cap's Little's-law residence time), floor_pipe =
                    // rate×RTprop (the guard's un-inflatable clock — see
                    // percap_redirect_bound: the loaded echo clock is
                    // self-referential and made the c8 redirect bound
                    // vacuous). Copa feed: cwnd_i is both (Copa's operating
                    // point is the bounded-queue pipe).
                    let pipes: Vec<(u32, Option<f64>, Option<f64>, Option<f64>)> = {
                        let sched = scheduler.lock();
                        let live = sched.live_paths();
                        let mut v = Vec::with_capacity(live.len());
                        // feat/store-borrowing: refresh the borrow law's
                        // (rate, RTprop) snapshot at the same cadence from
                        // the same honest sources (Copa feed: cwnd/RTprop
                        // as the drain rate; plain: the send-interval
                        // BtlBw anchor). Empty map when borrowing is off.
                        if percap_borrow_on {
                            percap_rr.clear();
                            for id in live.iter() {
                                if let Some(p) = sched.path(*id) {
                                    let rtp = p.min_rtt().map(|d| d.as_secs_f64());
                                    let rate = if copa_feed
                                        .as_ref()
                                        .is_some_and(|f| f.owns_cc())
                                    {
                                        rtp.filter(|r| *r > 0.0)
                                            .map(|r| p.cwnd as f64 / r)
                                    } else {
                                        p.btlbw_sym_per_s()
                                    };
                                    percap_rr.insert(*id, (rate, rtp));
                                }
                            }
                        }
                        for id in live.iter() {
                            let (pipe, floor_pipe, honest) = match sched.path(*id) {
                                Some(p) => {
                                    if copa_feed.as_ref().is_some_and(|f| f.owns_cc()) {
                                        // Copa-sole: cwnd_i IS the honest
                                        // bounded-queue pipe — unchanged.
                                        (Some(p.cwnd as f64), Some(p.cwnd as f64), None)
                                    } else {
                                        let rate = p.btlbw_sym_per_s();
                                        let pipe = rate
                                            .map(|r| r * p.srtt().as_secs_f64());
                                        let floor_pipe = match (rate, p.min_rtt()) {
                                            (Some(r), Some(rtp)) => {
                                                Some(r * rtp.as_secs_f64())
                                            }
                                            _ => None,
                                        };
                                        // feat/percap-honest-cap: cap_i on
                                        // the honest anchors — residence
                                        // K·RTprop + recovery-clock runway;
                                        // no loaded-echo term (see
                                        // `honest_store_cap`).
                                        let honest = if honest_cap_on {
                                            let k = percap_k
                                                .entry(*id)
                                                .or_insert_with(|| {
                                                    EchoRatioMin::new(
                                                        PERCAP_K_HALF_WINDOW_US,
                                                    )
                                                })
                                                .observe_srtt_over_rtprop(
                                                    p.srtt(),
                                                    p.min_rtt(),
                                                    dnow,
                                                );
                                            honest_store_cap(
                                                floor_pipe,
                                                rate,
                                                k,
                                                store_bdp_gain,
                                            )
                                        } else {
                                            None
                                        };
                                        (pipe, floor_pipe, honest)
                                    }
                                }
                                None => (None, None, None),
                            };
                            v.push((*id, pipe, floor_pipe, honest));
                        }
                        v
                    };
                    if pipes.len() >= 2 {
                        let legacy_cap = dyn_store_cap;
                        let n = pipes.len();
                        for (pid, pipe, floor_pipe, honest) in pipes {
                            // Honest law when derived (warm anchors under
                            // RWM_PLAIN_RS+RWM_HONEST_CAP); else the legacy
                            // percap law — echo-clock caps (the PBP-G-old
                            // control arm) and the warm-up legacy share
                            // (warm-up unchanged: honest = None before the
                            // anchor warms, exactly when pipe = None too).
                            let cap_i = match honest {
                                Some(h) => (h.ceil() as usize).clamp(
                                    store_cap_floor,
                                    store_path_pool.max(store_cap_floor),
                                ),
                                None => percap_store_cap(
                                    pipe,
                                    legacy_cap,
                                    n,
                                    store_bdp_gain,
                                    store_cap_floor,
                                    store_path_pool,
                                ),
                            };
                            percap_caps.insert(pid, cap_i);
                            // Roadmap item 1: the delay-aware redirect bound;
                            // bound = cap (guard degenerate) when unguarded.
                            percap_bounds.insert(
                                pid,
                                if percap_guard_on {
                                    percap_redirect_bound(
                                        floor_pipe,
                                        cap_i,
                                        store_bdp_gain,
                                    )
                                } else {
                                    cap_i
                                },
                            );
                        }
                        // Σ cap_i becomes the pooled MEMORY backstop (binds
                        // only via stranded accounts, e.g. a path that died
                        // with symbols still retained).
                        dyn_store_cap = percap_caps.values().sum();
                    }
                }
            }
        }
        let effective_store_cap = if plain_dyn_cap {
            dyn_store_cap
        } else if gen_pipe {
            // gen_pipe remedy 2: intake bounded at the DERIVED M*·G — deep
            // enough to cover BDP + one deficit round, no deeper (queue-lean).
            gen_pipe_store_cap
        } else {
            store_max
        };
        let tx_paused = if !percap_caps.is_empty() {
            // task #86 (RWM_STORE_PERCAP, N ≥ 2): per-path admission — pause
            // only when NO live path's account has headroom below its own
            // cap, EXCEPT (roadmap item 1) that when some account is
            // cap-full a pick landing there must redirect, so admission
            // stays open only while a guard-eligible target exists
            // (out < min(cap, redirect_bound)) — otherwise the store reads
            // FULL: backpressure, don't park (percap_store_full_guarded;
            // bound = cap when the guard is off, degenerating to the
            // unguarded gate). The pooled store_len test is retained as the
            // Σcap_i memory backstop (effective_store_cap = Σcap_i while
            // percap is engaged): it binds only through stranded accounts.
            let accounts: Vec<(usize, usize, usize)> = percap_caps
                .iter()
                .map(|(pid, &cap)| {
                    (
                        percap_out.get(pid).copied().unwrap_or(0),
                        cap,
                        percap_bounds.get(pid).copied().unwrap_or(cap),
                    )
                })
                .collect();
            // feat/store-borrowing (§16.22.4): the guarded gate plus the
            // loan edges — the store reads FULL only when the guarded gate
            // reads full AND no loan is admissible (a cap-full borrower
            // with a lender inside its lend bound keeps admission open;
            // the placement then borrows instead of redirecting).
            let guarded_full = percap_store_full_guarded(&accounts)
                && !(percap_borrow_on && {
                    let baccts: Vec<BorrowAccount> = percap_caps
                        .iter()
                        .map(|(&pid, &cap)| {
                            let out = percap_out.get(&pid).copied().unwrap_or(0);
                            let (rate, rtprop_s) =
                                percap_rr.get(&pid).copied().unwrap_or((None, None));
                            BorrowAccount {
                                path: pid,
                                out,
                                cap,
                                fly: out
                                    .saturating_sub(
                                        percap_lent.get(&pid).copied().unwrap_or(0),
                                    )
                                    .saturating_add(
                                        percap_borrowed.get(&pid).copied().unwrap_or(0),
                                    ),
                                rate,
                                rtprop_s,
                            }
                        })
                        .collect();
                    percap_lend_edge_exists(&baccts)
                });
            reliable
                && (guarded_full
                    || store_len >= effective_store_cap
                    || cwnd_full)
        } else if wd_engaged {
            // feat/window-mtu (RWM_WIN_DECOUPLE, N = 1): the decoupled gate.
            // Fresh admission tests the live HEAD SPAN against the
            // stall-metered allowance — recovery-stalled holes (below the
            // SACK/cum frontier) never consume the wire budget; they are
            // bounded by the retention backstop instead. During a frontier
            // freeze the allowance grows at the anchor rate (the explicit
            // stall-insurance term), capped at one recovery round.
            let last_sent = sent_store.keys().next_back().copied().unwrap_or(0);
            let wire_out = last_sent.saturating_sub(wnd2_frontier_last) as usize;
            let stall_s =
                now_us().saturating_sub(wnd2_frontier_change_us) as f64 / 1e6;
            let allow = win_decouple_allow(wd_allow_base, wd_rate, stall_s);
            reliable
                && (wire_out >= allow as usize
                    || store_len >= wd_cap_ret
                    || cwnd_full)
        } else {
            reliable && (store_len >= effective_store_cap || cwnd_full)
        };

        // feat/window-mtu wnd2/relgap tracking (see decls): the release
        // frontier is max(highest SACK-released seq, cumulative ack) —
        // O(log n) per iteration. Runs for the DIAG gauge AND for the
        // decoupled law's stall meter (RWM_WIN_DECOUPLE).
        if (diag_on || win_decouple_on) && reliable && !generation {
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
                wnd2_relgap_max_us = wnd2_relgap_max_us
                    .max(tnow.saturating_sub(wnd2_frontier_change_us));
            }
        }
        // RWM_DIAG periodic constraint report (see decls above the loop).
        if diag_on {
            diag_total_iters += 1;
            if tx_paused {
                diag_paused_iters += 1;
            }
            let dnow = now_us();
            // diag/lossy-residual emission-gap gauge (see decls): observe the
            // cumulative wire handoff count (src+cod, retx rides cod) once per
            // iteration; a change closes the current gap — accumulate it when
            // it is a stall-class gap (≥ 3 ms), not a pacing interval.
            {
                let wt = stats.fec.total_source_symbols.load(Ordering::Relaxed)
                    + stats.fec.total_repair_symbols.load(Ordering::Relaxed);
                if wt != sidle_last_total {
                    let gap = dnow.saturating_sub(sidle_last_change_us);
                    if sidle_last_total > 0 && gap >= SIDLE_GAP_MIN_US {
                        sidle_us += gap;
                        sidle_n += 1;
                        sidle_max_us = sidle_max_us.max(gap);
                    }
                    sidle_last_total = wt;
                    sidle_last_change_us = dnow;
                }
            }
            let ddt = dnow.saturating_sub(diag_last_us);
            if ddt >= 250_000 {
                let ack_now = window_ack_seq.load(Ordering::Relaxed);
                let src_now = stats.fec.total_source_symbols.load(Ordering::Relaxed);
                let cod_now = stats.fec.total_repair_symbols.load(Ordering::Relaxed);
                let secs = ddt as f64 / 1_000_000.0;
                // Goodput = cumulative-ack advance (delivered source symbols).
                let dack = ack_now.saturating_sub(diag_last_ack) as f64;
                let good_mbit = dack * (symbol_size as f64) * 8.0 / secs / 1e6;
                let src_rate = src_now.saturating_sub(diag_last_src) as f64 / secs;
                let cod_rate = cod_now.saturating_sub(diag_last_cod) as f64 / secs;
                let paused_frac = diag_paused_iters as f64 / diag_total_iters.max(1) as f64;
                let (cw, fl, np, min_rtt_us, pp) = {
                    let mut sched = scheduler.lock();
                    let mut cw = 0u64;
                    let mut fl = 0u64;
                    let mut np = 0u64;
                    let mut rtt = 0u64;
                    // PART 1 instrumentation: per-path in-flight vs its own BDP
                    // cap + live RTT vs RTprop — the slow-path bufferbloat probe
                    // (is the slow path over its BDP? is its RTT inflated above
                    // RTprop?).  Cap gain = the BDP in-flight gain.
                    let cap_gain = infl_bdp_gain;
                    let mut pp = String::new();
                    let ids = sched.active_paths();
                    for id in &ids {
                        if let Some(p) = sched.path_mut(*id) {
                            p.expire_in_flight();
                            cw += p.cwnd as u64;
                            fl += p.in_flight as u64;
                            np += 1;
                            rtt = rtt.max(p.estimator.rtt().as_micros() as u64);
                            let infl_i = p.in_flight as u64;
                            let bdp_i = p.copa_bdp_anchor().unwrap_or(0.0);
                            let cap_i = (cap_gain * bdp_i).ceil() as u64;
                            let rtt_i = p.estimator.rtt().as_secs_f64() * 1000.0;
                            let rtprop_i =
                                p.min_rtt().map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0);
                            // Per-path SOURCE outstanding gauge (charged by the
                            // CopaFeed at send, released on ack attribution),
                            // the ack-attributed per-path BtlBw_i (sym/s), and
                            // whether the per-path BDP anchor has ESTABLISHED.
                            let sinfl_i = p.src_inflight() as u64;
                            let btlbw_i = p.btlbw_sym_per_s().unwrap_or(0.0);
                            let est_i = if p.anchor_established() { "Y" } else { "n" };
                            // diag/slow-path-anchor: the rate-sample anchor trace
                            // (snapshotted-at-send / of-which-app-limited / acks-
                            // attributed / no-record / rej[interval/zero/applim] /
                            // generated / windowed-max-fill).  Cumulative counters.
                            let (rs_sent, rs_al, rs_attr, rs_nr, rs_iv, rs_zr, rs_al_rej, rs_gen, rs_fill) =
                                p.rs_diag();
                            // feat/copa-wire-signal: the wire clock next to the
                            // app-echo clock — wrtt = quinn packet-timed path RTT
                            // (what Copa's queue term reads under RWM_COPA_WIRE),
                            // rtt = app-layer echo (store-dwell inclusive), rtp =
                            // Copa's floor (wire-clocked when the gate is on; its
                            // distance from the known netem base per path is the
                            // FLOOR-FRESHNESS check).
                            let wrtt_i = transport
                                .wire_rtt(*id)
                                .map(|d| d.as_secs_f64() * 1000.0)
                                .unwrap_or(0.0);
                            // task #86 DIAG: the per-path outstanding ACCOUNT
                            // (store symbols charged to this path / its cap_i)
                            // — the mechanism gauge for RWM_STORE_PERCAP
                            // (zeros when the percap law is not engaged).
                            let sout_i = percap_out.get(id).copied().unwrap_or(0);
                            let scap_i = percap_caps.get(id).copied().unwrap_or(0);
                            // Roadmap item 1: the delay-aware redirect bound
                            // (sbnd) — the guard's mechanism gauge (dwell_i
                            // is sout_i/btlbw_i, computable offline).
                            let sbnd_i = percap_bounds.get(id).copied().unwrap_or(0);
                            // feat/copa-compete DIAG: cmp=<mode><switches>/<δ>
                            // — mode C (competitive) or D (default), the
                            // cumulative competitive entries, and the LIVE δ
                            // the update law is running (== the hint base
                            // unless competing). "-" when switching disabled.
                            let (cmp_on, cmp_in, cmp_sw, cmp_delta, _) =
                                p.copa_compete_diag();
                            let cmp_s = if cmp_on {
                                format!(
                                    "{}{}/{:.4}",
                                    if cmp_in { "C" } else { "D" },
                                    cmp_sw,
                                    cmp_delta
                                )
                            } else {
                                "-".to_string()
                            };
                            // feat/anchor-hygiene DIAG: process-clock stall
                            // witness gauges (stalls detected / samples
                            // discarded, PROCESS-global) — zeros when
                            // RWM_CLOCK_GAP is off.
                            let (gap_g, gap_d) = crate::control::anchor::stall_witness()
                                .map(|w| w.stats())
                                .unwrap_or((0, 0));
                            // feat/percap-honest-cap DIAG: khr = the
                            // windowed-min echoSRTT/RTprop ratio K_i feeding
                            // the honest cap law (1.00 when not engaged).
                            let khr_i = percap_k.get(id).map(|e| e.k()).unwrap_or(1.0);
                            // feat/store-borrowing DIAG: this path's loan
                            // gauges — symbols LENT out (charged here,
                            // flying elsewhere) / BORROWED in (flying
                            // here, charged elsewhere). Zeros when off.
                            let lent_i = percap_lent.get(id).copied().unwrap_or(0);
                            let bor_i = percap_borrowed.get(id).copied().unwrap_or(0);
                            // feat/recovery-suppression DIAG: the per-path
                            // LOSS ESTIMATE the recovery plane actually keys
                            // on (repair_debt, P_lost, NACK budgets) — the
                            // gauge that names the batch-serial poisoning
                            // (global batch_seq gaps read as per-path loss
                            // under striping).
                            let pl_i = p.estimator.loss_rate();
                            pp.push_str(&format!(
                                " p{}:infl={}/sinfl={}/bdp{:.0}(cap{}) sout={}/{}/b{} ln={}/{} khr={:.2} btlbw={:.0} est={} pl={:.4} cmp={} rtt={:.0}/wrtt={:.0}/rtp{:.0}ms gapd={}/{} | ANCHOR sent={} al={} attr={} nr={} rej[iv={} zr={} al={}] gen={} fill={}",
                                id, infl_i, sinfl_i, bdp_i, cap_i, sout_i, scap_i, sbnd_i, lent_i, bor_i, khr_i, btlbw_i, est_i, pl_i, cmp_s, rtt_i, wrtt_i, rtprop_i, gap_g, gap_d,
                                rs_sent, rs_al, rs_attr, rs_nr, rs_iv, rs_zr, rs_al_rej, rs_gen, rs_fill
                            ));
                        }
                    }
                    (cw, fl, np, rtt, pp)
                };
                // BDP in symbols = goodput-rate(sym/s) × RTT — but report the
                // link-capacity BDP too from the measured min RTT and a nominal
                // 100 Mbit (diagnostic reference only).
                let bdp_100m = if min_rtt_us > 0 {
                    (100e6 / 8.0 / symbol_size as f64) * (min_rtt_us as f64 / 1e6)
                } else {
                    0.0
                };
                let eff = if generation { diag_eff_rate } else { 0.0 };
                // GDIAG: stall attribution + generation lifecycle for this
                // window (percentages of attributed wall time; GLIFE means).
                let gd_tot: u64 = gd_us.iter().sum::<u64>().max(1);
                let pct = |i: usize| gd_us[i] as f64 * 100.0 / gd_tot as f64;
                let gln = gl_sum.3.max(1);
                let gdiag = if generation {
                    format!(
                        " stall[emit={:.0}% budget={:.0}% fill={:.0}% target={:.0}% tok={:.0}% cwnd={:.0}%] glife[n={} fill={:.0}ms code={:.0}ms wait={:.0}ms]",
                        pct(0), pct(1), pct(2), pct(3), pct(4), pct(5),
                        gl_sum.3,
                        gl_sum.0 as f64 / gln as f64 / 1000.0,
                        gl_sum.1 as f64 / gln as f64 / 1000.0,
                        gl_sum.2 as f64 / gln as f64 / 1000.0,
                    )
                } else {
                    String::new()
                };
                gd_us = [0; 6];
                gl_sum = (0, 0, 0, 0);
                // Residual (iii) DIAG: cross-path-history attributions and
                // how many the flight witness credited to the previous
                // flight (spurious-retransmit class). Zeros without a feed.
                let (xat_c, xat_w) = copa_feed
                    .as_ref()
                    .map(|f| f.attr_diag())
                    .unwrap_or((0, 0));
                // RWM_STORE_SACK_RELEASE DIAG: currently released (retained
                // but uncounted) / cumulative slots released — the store-
                // dwell mechanism gauge (win= already shows the uncounted
                // outstanding; retained = win + srel_cur). Empty when off.
                let srdiag = if store_sack_release_on {
                    format!(" srel={}/{}", sack_released.len(), sack_released_total)
                } else {
                    String::new()
                };
                // feat/window-mtu part-1 diagnosis gauge (see decls): the
                // outstanding split + release-clumping. head = live head
                // span above the release frontier; hole = unSACKed below it.
                let wnd2diag = if reliable && !generation {
                    let last_sent =
                        sent_store.keys().next_back().copied().unwrap_or(0);
                    let head = last_sent.saturating_sub(wnd2_frontier_last) as usize;
                    let hole = store_len.saturating_sub(head);
                    let relgap_cur =
                        dnow.saturating_sub(wnd2_frontier_change_us) / 1000;
                    let mut s = format!(
                        " wnd2={}/{} relgap={}ms/mx{}ms",
                        head.min(store_len),
                        hole,
                        relgap_cur,
                        wnd2_relgap_max_us / 1000,
                    );
                    // RWM_WIN_DECOUPLE engagement gauge: base allowance /
                    // honest rate / retention backstop (mechanism liveness).
                    if wd_engaged {
                        s.push_str(&format!(
                            " wd=al{:.0}/r{:.0}/ret{}",
                            wd_allow_base, wd_rate, wd_cap_ret
                        ));
                    }
                    wnd2_relgap_max_us = 0;
                    s
                } else {
                    String::new()
                };
                // δ-honest shed DIAG (fix C): cumulative shed / budget-
                // refused, live 1−ρ fraction and deadline. Empty when off.
                let sheddiag = if shed_on {
                    format!(
                        " shed={}/{} bud={:.4} D={}ms",
                        shed_total,
                        shed_denied,
                        shed_budget_frac,
                        shed_deadline_us_live / 1000,
                    )
                } else {
                    String::new()
                };
                // feat/recovery-suppression DIAG: the recovery-plane trace.
                // rep/seqs = gap reports processed / gap seqs walked;
                // fired y/r = retransmits whose live flight was YOUNGER than
                // its path's law threshold (the spurious-by-law class) vs
                // ripe; supp c/a/l = suppressed by cooldown / legacy age
                // gate / the mp law; stale = gap seqs already acked;
                // plost = P_lost-branch retransmits; age = mean flight age
                // at fire (ms); fp/on = per-path fired-flight / sent-on.
                let mpd_fired = mpd_fired_young + mpd_fired_ripe;
                let mut mp_pp = String::new();
                let mut mp_keys: Vec<u32> = mpd_fired_flight
                    .keys()
                    .chain(mpd_fired_on.keys())
                    .copied()
                    .collect();
                mp_keys.sort_unstable();
                mp_keys.dedup();
                for k in mp_keys {
                    mp_pp.push_str(&format!(
                        " p{}:{}/{}",
                        k,
                        mpd_fired_flight.get(&k).copied().unwrap_or(0),
                        mpd_fired_on.get(&k).copied().unwrap_or(0)
                    ));
                }
                let mpr = format!(
                    " mpr[rep={} seqs={} fired={} y={} r={} fast={} coal={} supp={}/{}/{} stale={} plost={} age={:.0}ms fp/on{}]",
                    mpd_gap_reports,
                    mpd_gap_seqs,
                    mpd_fired,
                    mpd_fired_young,
                    mpd_fired_ripe,
                    mpd_fired_fast,
                    mpd_coalesced,
                    mpd_supp_cool,
                    mpd_supp_age,
                    mpd_supp_law,
                    mpd_stale,
                    mpd_plost_retx,
                    if mpd_fired > 0 {
                        mpd_age_ms_sum / mpd_fired as f64
                    } else {
                        0.0
                    },
                    mp_pp,
                );
                eprintln!(
                    "[DIAG] t={:.1}s win={}/{} paused={:.0}% good={:.1}Mbit ackrate_ewma={:.0}sym/s eff_pace={:.0}sym/s src={:.0}sym/s cod={:.0}sym/s cum={}/{}/{} sidle={}ms/{}/mx{}ms cwnd={} infl={} np={} rtt={:.1}ms bdp100={:.0}sym sweeps={} retx={} gapdrop={} nbud={} xattr={}/{} loan={}/{}{}{}{}{}{}{}",
                    dnow.saturating_sub(diag_start_us) as f64 / 1e6,
                    store_len, effective_store_cap,
                    paused_frac * 100.0,
                    good_mbit,
                    if generation { gen_rate_ewma } else { 0.0 },
                    eff,
                    src_rate, cod_rate,
                    // diag/lossy-residual: cumulative src/cod/ack totals (the
                    // end-of-run accounting reads the LAST line) + the
                    // emission-gap gauge (cum stall-gap ms / count / max).
                    src_now, cod_now, ack_now,
                    sidle_us / 1000, sidle_n, sidle_max_us / 1000,
                    cw, fl, np,
                    min_rtt_us as f64 / 1000.0,
                    bdp_100m,
                    diag_sweeps, diag_retx, diag_gaps_dropped, cached_nack_budget,
                    xat_c, xat_w,
                    percap_loans.len(), percap_loans_total,
                    mpr,
                    wnd2diag,
                    srdiag,
                    sheddiag,
                    gdiag,
                    pp,
                );
                // feat/c8-conversion DIAG: the sender-side conversion gauges
                // (cumulative; keys sorted for stable scraping). splace =
                // first source placements; retxo = targeted retransmits by
                // ORIGINAL placement path; stallo = frontier-stall ms/count
                // by blocking-hole owner path.
                {
                    let mut keys: Vec<u32> = c8c_src_placed
                        .keys()
                        .chain(c8c_retx_orig.keys())
                        .chain(c8c_stall_ms.keys())
                        .copied()
                        .collect();
                    keys.sort_unstable();
                    keys.dedup();
                    if !keys.is_empty() {
                        let mut s = String::new();
                        for k in keys {
                            s.push_str(&format!(
                                " p{}:sp={} ro={} st={}ms/{}",
                                k,
                                c8c_src_placed.get(&k).copied().unwrap_or(0),
                                c8c_retx_orig.get(&k).copied().unwrap_or(0),
                                c8c_stall_ms.get(&k).copied().unwrap_or(0),
                                c8c_stall_n.get(&k).copied().unwrap_or(0),
                            ));
                        }
                        // RWM_PLACE_SLACK gauge: the live S (ms) + ack-rate
                        // EWMA (sym/s) — engagement magnitude for the law.
                        if place_slack_on {
                            s.push_str(&format!(
                                " slk={:.0}ms/r{:.0}",
                                ps_slack_gauge * 1000.0,
                                ps_rate_ewma
                            ));
                        }
                        eprintln!("[C8CONV-S]{}", s);
                    }
                }
                diag_last_us = dnow;
                diag_last_ack = ack_now;
                diag_last_src = src_now;
                diag_last_cod = cod_now;
                diag_paused_iters = 0;
                diag_total_iters = 0;
            }
        }

        // Generation coding: paced coded emission (see gen_tokens above). Runs
        // every iteration — including the tx_paused 1 ms wakeups — so coded
        // symbols for the in-flight generations keep flowing while TUN reads are
        // paused, completing buffered generations and keeping M in flight
        // (∝-goodput striping via place_symbol; fungible cross-path, no per-seq
        // ARQ). This is the mechanism that turns the serialized stop-and-wait
        // into a pipelined transfer.
        if generation && gates.trace {
            let now = now_us();
            if now.saturating_sub(gen_trace_last_us) > 200_000 {
                gen_trace_last_us = now;
                let ack_now = window_ack_seq.load(Ordering::Relaxed);
                let want_sum: u64 = gen_want.values().sum();
                let (ws, we) = encoder.window_span();
                eprintln!(
                    "[SND] ack={} coded_total={} wants={} win={} span=({},{}) want_gens={} want_sum={} tx_paused={}",
                    ack_now, gen_coded_total, encoder.wants_coding(), encoder.window_size(),
                    ws, we, gen_want.len(), want_sum, tx_paused
                );
            }
        }
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
        // GDIAG: did ANY coded symbol go on the wire this iteration?
        let mut gd_flow = false;
        if generation && encoder.window_size() > 0 {
            let now = now_us();
            // Object tail: intake is idle (not just paused by backpressure — no
            // new source for a few RTTs while the pipe has room). Let the final
            // partial generation recover; a mid-stream backpressure pause is NOT
            // idle (tx_paused), so this never floods a still-filling generation.
            encoder.set_intake_idle(!tx_paused && now.saturating_sub(gen_last_source_us) > 30_000);
            // Fix 3: advance the PROACTIVE-CODING floor to follow the SEND
            // frontier (the last `pipeline` sealed generations), decoupled from
            // the stalled in-order retention floor. Under RWM_OOO_RETAIN the send
            // frontier runs `ooo_gens` ahead of a stalled generation; without
            // this the coder would keep re-coding the stalled generation and
            // never provision the fresh ones — they would then need reactive
            // recovery and re-serialize. No-op when ooo_retain is off (default).
            if ooo_retain {
                let (_, newest) = encoder.window_span();
                let code_anchor =
                    newest.saturating_sub((pipeline as u64) * (gen_size as u64));
                encoder.set_code_base(code_anchor);
            }
            // ACK-CLOCKED WINDOW FLOW CONTROL. Emit coded symbols up to
            //   total_coded ≤ delivered·(1+r) + W_inflight
            // where `delivered` = cumulative ack (decoded source symbols) and
            // W_inflight is the in-flight coded allowance (≈ BDP + one
            // generation). The delivered·(1+r) term is the steady coded budget
            // to reconstruct what has been delivered (r covers loss + the MDS
            // margin); W_inflight is the burst the pipe may hold ahead of the
            // decode frontier. This self-clocks to the LINK GOODPUT (ack-driven,
            // like a congestion window) and — crucially — BOUNDS the QUIC
            // datagram buffer, so over-emission can't bloat it and strand fresh
            // coded behind stale coded (the ×0.13 pathology of un-clocked
            // emission). RWM_GEN_R (overhead r) and RWM_GEN_INFLIGHT tune it.
            let ack_now = window_ack_seq.load(Ordering::Relaxed);
            // FLOW-CONTROL bound: coded must not run more than W_inflight coded
            // symbols ahead of the DECODE frontier (cumulative ack), which
            // bounds the QUIC datagram buffer (no un-clocked bloat). The encoder
            // itself caps per-generation emission to ceil(len·(1+r)) so this
            // window is never spent producing low-rank symbols over a still-
            // filling generation — the two together give startup-safe, recovery-
            // capable, ack-clocked emission.
            // The proactive budget is clocked to the DECODE frontier (cumulative
            // ack): coded must not run more than `gen_inflight_window` ahead of
            // what the receiver has decoded, which BOUNDS the QUIC datagram buffer
            // (the transport-ceiling fix — loosening this to the sent frontier
            // reintroduces datagram bufferbloat and SERIALIZES symmetric
            // aggregation, MEASURED C7 22.4→14.9). The small-G frontier-advance
            // deadlock is NOT closed here (that would bloat the datagram path) but
            // by the receiver seeding a wedged generation's width so the DEFICIT
            // loop — which is ack-clock-INDEPENDENT — always funds the frontier
            // hole. `RWM_CODED_SRC` still offers the sent-frontier clock as an
            // opt-in for experiments.
            // Fix 3: under OOO retention the cumulative ack is stalled on a hole
            // while the send frontier runs far ahead, so clock the proactive
            // budget on the SENT frontier (like RWM_CODED_SRC) — else coded
            // emission would freeze at the stalled ack and the fresh generations
            // would never be provisioned.
            // gen_pipe remedy 3: same sent-frontier clock — the intake cap
            // (M*·G) + per-generation ceil(len·(1+r)) budgets already bound
            // the outstanding coded, so the stalled ack must not freeze the
            // M*−1 fresh generations' provisioning.
            let target = if coded_src_clock || ooo_retain || gen_pipe {
                let (_, wend) = encoder.window_span();
                (wend as f64) * (1.0 + gen_repair_floor) + gen_inflight_window
            } else {
                (ack_now as f64) * (1.0 + gen_repair_floor) + gen_inflight_window
            };
            // Clock the pacing rate to the DELIVERED goodput (ack rate): sample
            // the ack advance over a ~20 ms window into an EWMA, and pace at
            // 1.5× that (headroom for loss/overhead), clamped to [floor, ceiling].
            // This keeps coded emission from outrunning the receiver's decode so
            // the datagram intake is not overrun and bursts are not dropped
            // (§16.3 the named failure). Before the first sample the floor primes
            // the first generation.
            {
                let dt = now.saturating_sub(gen_rate_sample_us);
                if dt >= 20_000 {
                    let dack = ack_now.saturating_sub(gen_rate_sample_ack) as f64;
                    let inst = dack / (dt as f64 / 1_000_000.0);
                    gen_rate_ewma = if gen_rate_ewma <= 0.0 {
                        inst
                    } else {
                        0.7 * gen_rate_ewma + 0.3 * inst
                    };
                    gen_rate_sample_us = now;
                    gen_rate_sample_ack = ack_now;
                }
            }
            // Fix 1: under CC-rate pacing the coded bucket shares the source's
            // small headroom (the 1.5× overshoot itself overruns the datagram
            // path — 50% more coded than the receiver can decode builds a queue
            // that bursts-drops). Legacy 1.5× kept when cc_pace is off.
            // gen_pipe remedy 4: anchor the pace to the windowed-MAX delivered
            // rate. The decode-clocked EWMA decays toward the floor between
            // generation acks (samples are mostly-low, §16.15), throttling
            // emission exactly while the pipe is waiting; the windowed max is
            // the recovery statistic. Headroom 1.25 (the BBR probe gain — the
            // wire must fund (1+r)/(1−ε) ≈ 1.08× the delivered rate plus ramp
            // margin) instead of the legacy 1.5 whose overshoot bursts drop.
            let eff_factor = if cc_pace {
                cc_pace_headroom
            } else if gen_pipe {
                1.25
            } else {
                1.5
            };
            // Fix 1: under cc_pace clock coded emission on the same frontier-
            // independent CC rate (max with the goodput EWMA) so a stalled
            // in-order ack does not starve coded emission below the link.
            let eff_base = if cc_pace {
                gen_rate_ewma.max(cc_rate_cached)
            } else if gen_pipe {
                gen_rate_ewma.max(gp_rate_max)
            } else {
                gen_rate_ewma
            };
            let eff_rate = (eff_base * eff_factor).clamp(gen_rate_floor, gen_rate);
            diag_eff_rate = eff_rate;
            // Refill the pacing token bucket (capped at a small burst). Under
            // cc_pace the cap is ≈ a few ms of link rate (not 64) so a caught-up
            // bucket can't release a large coded burst onto the datagram path.
            let tok_dt = now.saturating_sub(gen_tok_last_us);
            gen_tok_last_us = now;
            let gen_tok_cap = if cc_pace { (eff_rate * 0.004).clamp(8.0, 64.0) } else { 64.0 };
            gen_tokens = (gen_tokens + eff_rate * (tok_dt as f64 / 1_000_000.0)).min(gen_tok_cap);
            let burst_cap = if cc_pace { 64u32 } else { 256u32 };
            let mut emitted = 0u32;
            while !proactive_pacer
                && (gen_coded_total as f64) < target
                && emitted < burst_cap
                && gen_tokens >= 1.0
                && !cwnd_full
                && encoder.wants_coding()
            {
                let path = {
                    let sched = scheduler.lock();
                    if xpath_repair {
                        sched.place_repair_spare_path().unwrap_or(0)
                    } else {
                        sched.place_symbol(true, &[]).unwrap_or(0)
                    }
                };
                gen_coded_total += 1;
                emitted += 1;
                gd_flow = true;
                gen_tokens -= 1.0;
                let sym = encoder.generate_repair();
                // Count this proactive emission toward the per-generation
                // in-flight accounting so the deficit loop never double-sends
                // what proactive already covered.
                if sym.data.len() >= 8 {
                    let anchor = u64::from_le_bytes(sym.data[0..8].try_into().unwrap());
                    *gen_emitted.entry(anchor).or_insert(0) += 1;
                    if diag_on {
                        gl.entry(anchor).or_insert((0, 0, 0)).2 = now_us();
                    }
                }
                proactive_coded_total += 1;
                let batch_seq = batch_counter.fetch_add(1, Ordering::Relaxed);
                let batch = SymbolBatch {
                    symbols: vec![sym],
                    send_timestamp_us: now_us(),
                    batch_seq,
                    path_id: path,
                };
                if let Err(e) = transport.send_symbols(path, batch) {
                    warn!(path, ?e, "failed to send generation coded symbol");
                }
                {
                    let mut sched = scheduler.lock();
                    if let Some(p) = sched.path_mut(path) {
                        p.charge_in_flight(1);
                    }
                }
                if let Some(ps) = stats.path(path) {
                    ps.symbols_sent.fetch_add(1, Ordering::Relaxed);
                }
                stats.fec.total_repair_symbols.fetch_add(1, Ordering::Relaxed);
            }

            // ── PROACTIVE PACER (RWM_PROACTIVE_PACER) — present-at-stall ──────
            // Emit filling-generation proactive repair on the generation grid,
            // paced by the SAME CC token bucket but WITHOUT the ack-clock
            // `target` gate (the cumulative ack is stalled exactly when the
            // frontier needs repair) and without any source-availability gate
            // (this block runs every loop iteration, incl. tx_paused wakeups).
            // Bounded by each generation's ceil(len·r) budget (wants_filling_
            // coding turns false at budget), the CC rate (gen_tokens) and
            // congestion (cwnd_full). Supersedes the sealed batched proactive
            // path above; the reactive deficit below remains the fallback.
            if proactive_pacer {
                let mut fill_emitted = 0u32;
                while fill_emitted < burst_cap
                    && gen_tokens >= 1.0
                    && !cwnd_full
                    && encoder.wants_filling_coding()
                {
                    let path = {
                        let sched = scheduler.lock();
                        if xpath_repair {
                            sched.place_repair_spare_path().unwrap_or(0)
                        } else {
                            sched.place_symbol(true, &[]).unwrap_or(0)
                        }
                    };
                    let sym = encoder.generate_repair_filling();
                    fill_emitted += 1;
                    gen_tokens -= 1.0;
                    // Count against per-generation in-flight accounting so the
                    // deficit loop never double-sends what proactive covered.
                    if sym.data.len() >= 8 {
                        let anchor = u64::from_le_bytes(sym.data[0..8].try_into().unwrap());
                        *gen_emitted.entry(anchor).or_insert(0) += 1;
                    }
                    proactive_coded_total += 1;
                    let batch_seq = batch_counter.fetch_add(1, Ordering::Relaxed);
                    let batch = SymbolBatch {
                        symbols: vec![sym],
                        send_timestamp_us: now_us(),
                        batch_seq,
                        path_id: path,
                    };
                    if let Err(e) = transport.send_symbols(path, batch) {
                        warn!(path, ?e, "failed to send filling-generation repair");
                    }
                    {
                        let mut sched = scheduler.lock();
                        if let Some(p) = sched.path_mut(path) {
                            p.charge_in_flight(1);
                        }
                    }
                    if let Some(ps) = stats.path(path) {
                        ps.symbols_sent.fetch_add(1, Ordering::Relaxed);
                    }
                    stats.fec.total_repair_symbols.fetch_add(1, Ordering::Relaxed);
                }
            }

            // DEFICIT-DRIVEN RECOVERY EMISSION (§16.3, the named missing
            // mechanism). Emit the residual coded symbols each stalled frontier
            // generation still needs, PACED by the same token bucket, round-
            // robin so no generation starves — but NOT gated by the ack-clocked
            // `target`. That is the crux of the fix: the cumulative ack is
            // stalled EXACTLY when the frontier generation needs recovery, so
            // gating recovery on the ack (as the feedback-free proxy did) is the
            // deadlock. The receiver's per-generation deficit BOUNDS the total
            // (we send only the residual it reports, minus what is already in
            // flight — tracked in gen_want), so bypassing the ack-clock here
            // cannot flood: recovery is bounded AND funds the frontier at once.
            if !no_reactive && !gen_want.is_empty() {
                let rec_burst = 256u32;
                let mut rec_emitted = 0u32;
                'recover: loop {
                    // Fix 2: reactive stops at the shared link budget (gen_tokens)
                    // and — when enabled — is NON-EXEMPT from the in-flight cap
                    // (cwnd_full), so it cannot burst the pipe past congestion
                    // control the way the old exempt loop did.
                    // Recovery is NON-EXEMPT from cwnd_full (congestion control):
                    // exempting it floods the pipe (MEASURED RTT 2.5 s
                    // bufferbloat). The frontier is funded instead by (a) the win
                    // backstop keeping the send frontier within a few generations
                    // of the in-order frontier so the stranded generation is
                    // recent, and (b) recovery running each iteration as the
                    // in-flight budget expires on the RTT timescale.
                    if gen_tokens < 1.0 || rec_emitted >= rec_burst
                        || (react_cap_on && cwnd_full) {
                        break;
                    }
                    let now_r = now_us();
                    let anchors: Vec<u64> = gen_want.keys().copied().collect();
                    if anchors.is_empty() {
                        break;
                    }
                    let mut progressed = false;
                    for a in anchors {
                        if gen_tokens < 1.0 || rec_emitted >= rec_burst
                            || (react_cap_on && cwnd_full) {
                            break 'recover;
                        }
                        let want = gen_want.get(&a).copied().unwrap_or(0);
                        if want == 0 {
                            gen_want.remove(&a);
                            continue;
                        }
                        let sym = match encoder.generate_repair_for(a) {
                            Some(s) => s,
                            None => {
                                // Generation no longer retained/sealed (decoded
                                // and advanced, or not yet sealed) — drop its want.
                                gen_want.remove(&a);
                                continue;
                            }
                        };
                        // SLOW-PATH COVERAGE (§16.3 intent). Deficit recovery funds
                        // a frontier generation whose hole is the long pole — most
                        // often a source lost on the SLOW path. Place it by the
                        // ∝-goodput placement law (softmax over marginal cost),
                        // which already biases the covering repair toward the FAST
                        // path proportionally without STARVING a symmetric second
                        // path — hard argmax concentration serializes symmetric
                        // aggregation (MEASURED C7 regression) for no C8 gain.
                        let path = {
                            let sched = scheduler.lock();
                            if xpath_repair {
                                sched.place_repair_spare_path().unwrap_or(0)
                            } else {
                                sched.place_symbol(true, &[]).unwrap_or(0)
                            }
                        };
                        *gen_emitted.entry(a).or_insert(0) += 1;
                        recovery_coded_total += 1;
                        gd_flow = true;
                        if diag_on {
                            gl.entry(a).or_insert((0, 0, 0)).2 = now_us();
                        }
                        let nw = want - 1;
                        if nw == 0 {
                            gen_want.remove(&a);
                        } else {
                            gen_want.insert(a, nw);
                        }
                        gen_tokens -= 1.0;
                        if react_cap_on {
                            // Stamp this generation's recovery time so the spacing
                            // check above holds off further recovery for ~1 SRTT.
                            gen_recover_at.insert(a, now_r);
                        }
                        rec_emitted += 1;
                        progressed = true;
                        let batch_seq = batch_counter.fetch_add(1, Ordering::Relaxed);
                        let batch = SymbolBatch {
                            symbols: vec![sym],
                            send_timestamp_us: now_us(),
                            batch_seq,
                            path_id: path,
                        };
                        if let Err(e) = transport.send_symbols(path, batch) {
                            warn!(path, ?e, "failed to send generation recovery symbol");
                        }
                        {
                            let mut sched = scheduler.lock();
                            if let Some(p) = sched.path_mut(path) {
                                p.charge_in_flight(1);
                            }
                        }
                        if let Some(ps) = stats.path(path) {
                            ps.symbols_sent.fetch_add(1, Ordering::Relaxed);
                        }
                        stats.fec.total_repair_symbols.fetch_add(1, Ordering::Relaxed);
                    }
                    if !progressed {
                        break;
                    }
                }
            }
        }
        // ── GDIAG attribution: which gate is binding wire emission NOW? ──────
        // Runs every iteration (RWM_DIAG only). See the state legend at the
        // declarations above. In coded-wire generation mode the paced coded
        // block is the whole data plane, so the gate that stopped it this
        // iteration is the throughput binder for the elapsed slice.
        if diag_on && generation {
            let now_g = now_us();
            let dt = now_g.saturating_sub(gd_last_us);
            gd_last_us = now_g;
            let idx = if gd_flow {
                0 // emit: coded flowed
            } else if encoder.window_size() == 0 {
                2 // fill: nothing retained yet (startup/tail)
            } else if !encoder.wants_coding() {
                // Every active generation at budget (ack/deficit round-trip
                // wait) vs the head generation not yet sealed (intake-bound).
                // advance() is generation-aligned, so ≥2·G retained means the
                // two active generations are both full ⇒ sealed-at-budget.
                if store_len >= 2 * gen_size { 1 } else { 2 }
            } else {
                let ack_now = window_ack_seq.load(Ordering::Relaxed);
                let tgt = if coded_src_clock || ooo_retain || gen_pipe {
                    let (_, wend) = encoder.window_span();
                    (wend as f64) * (1.0 + gen_repair_floor) + gen_inflight_window
                } else {
                    (ack_now as f64) * (1.0 + gen_repair_floor) + gen_inflight_window
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
            gd_us[idx] += dt;
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
        if cc_pace {
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
                cc_rate_ceiling = gen_rate * n_live as f64;
            }
            // Pace at the HIGHER of the CC rate and the delivered-goodput EWMA so
            // a stalled in-order frontier (EWMA→0) can't throttle the source ramp.
            let link_est = gen_rate_ewma.max(cc_rate_cached);
            let src_rate = (link_est * cc_pace_headroom).clamp(gen_rate_floor, cc_rate_ceiling);
            let dt = now.saturating_sub(src_tok_last_us);
            src_tok_last_us = now;
            let burst = (src_rate * 0.004).clamp(8.0, 64.0);
            src_tokens = (src_tokens + src_rate * (dt as f64 / 1_000_000.0)).min(burst);
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
                    .map_or(send_us, |&(r, _)| r.max(send_us))
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
            // Fix 1: pacing wake — when source sends are paced-off (bucket
            // empty), wake at 1 ms to refill it. Without this the select could
            // block in read_packet with the pacing gate closed and stall intake.
            _ = tokio::time::sleep(Duration::from_millis(1)),
                if cc_pace && !tx_paused && src_tokens < 1.0 => None,
            p = tun.read_packet(),
                if !tx_paused && (!cc_pace || src_tokens >= 1.0) => Some(p),
            // Generation coding: a 1 ms emission poll so the loop keeps waking to
            // run the paced coded-emission block even when no TUN packet is ready
            // (the tail — all sources read but the last generations still need
            // coded symbols to decode) and when not paused. Without it the loop
            // would block in read_packet and the tail would never complete.
            _ = tokio::time::sleep(Duration::from_millis(1)),
                if generation && !tx_paused && encoder.window_size() > 0 => None,
            gaps = nack_rx.recv() => {
                if let Some(g) = gaps {
                    pending_gaps = Some(g);
                }
                None
            }
            // Generation-deficit feedback (§16.3): the receiver reports how many
            // MORE coded symbols each frontier generation still needs. Rebuild
            // the per-generation want from the report, subtracting what is
            // already in flight (emitted since the last report), then reset the
            // in-flight baseline. The recovery-emission block above drains these
            // wants, paced. A generation ABSENT from the report has decoded (or
            // is not yet frontier), so its want is cleared by the rebuild.
            dv = deficit_rx.recv(), if generation => {
                if let Some(dv) = dv {
                    gen_want.clear();
                    // Pure-proactive demonstrator: drain the channel but never
                    // arm reactive recovery (no round-trips, no exempt-from-cap
                    // emission). Proactive upfront budget is the ONLY recovery.
                    if no_reactive {
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
                    let react_space_us: u64 = if react_cap_on {
                        let srtt_us = {
                            let sched = scheduler.lock();
                            sched.active_paths().iter()
                                .filter_map(|id| sched.path(*id).map(|p| p.srtt().as_micros() as u64))
                                .max().unwrap_or(50_000)
                        };
                        ((srtt_us as f64) * react_cap_cfg).max(1_000.0) as u64
                    } else {
                        0
                    };
                    let now_d = now_us();
                    for (anchor, deficit) in dv {
                        // Fix 2: hold off if we recovered this generation recently.
                        if react_cap_on {
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
            _ = async {
                match tail_deadline {
                    Some(d) => tokio::time::sleep_until(d).await,
                    None => std::future::pending().await,
                }
            } => {
                last_tail_sweep_us = now_us();
                if let Some((&seq, _)) = retransmit_buffer.iter().next() {
                    debug!(seq, "tail ARQ sweep — retransmitting cumulative blocker");
                    diag_sweeps += 1;
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
                    while burst < emit_burst {
                        if reliable
                            && sack_release_outstanding(
                                sent_store.len(),
                                sack_released.len(),
                            ) >= effective_store_cap
                        {
                            break; // store headroom exhausted (flow control)
                        }
                        if cc_pace && src_tokens < 1.0 {
                            break; // pacing bucket dry (Fix 1 contract)
                        }
                        match tun.try_read_packet() {
                            Some(pkt) => {
                                let framed =
                                    framing::frame_window_packet(&pkt, symbol_size);
                                send_source_symbol!(framed);
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
                now_us().saturating_sub(last_source_send_us) > idle_gap_us;
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
                    Ok(g) => {
                        // feat/recovery-suppression: a gap report is a STATE
                        // SNAPSHOT of the receiver's current holes (frontier
                        // + inverted SACK), not a delta — so a queued
                        // backlog is stale by construction and only the
                        // NEWEST snapshot needs processing. Under the mp
                        // law holes legitimately outlive their reports
                        // (suppressed until a loss channel fires), so the
                        // 2 ms gap-ack cadence queues snapshots faster than
                        // they change; coalescing removes that walk tax.
                        // Legacy path (gate off) keeps per-report
                        // processing bit-exactly.
                        let mut g = g;
                        if recov_mp_law {
                            while let Ok(n) = nack_rx.try_recv() {
                                g = n;
                                if diag_on {
                                    mpd_coalesced += 1;
                                }
                            }
                        }
                        g
                    }
                    Err(_) => break,
                },
            };
            if cached_max_repairs == 0 || cached_nack_budget == 0 {
                // Fully suppressed or budget exhausted — drain NACK queue
                diag_gaps_dropped += 1;
                continue;
            }

            // SRTT drives the per-seq retransmit cooldown and the age gate.
            // RWM_RECOV_MP additionally snapshots PER-PATH smoothed clocks
            // (Copa srtt + estimator EWMA) for the per-flight hole law, and
            // the live path count (N=1 ⇒ the law is inert, legacy bit-exact).
            let mut mp_clocks: std::collections::HashMap<u32, (u64, u64)> =
                std::collections::HashMap::new();
            let mut mp_n_paths: usize = 1;
            let srtt_us = {
                let sched = scheduler.lock();
                // RWM_RECOV_MP_LIVE (goal-gate "C8 Slow-Path Conversion"):
                // the law's N + clock snapshot must not lose a cwnd-
                // saturated path (available() == 0 collapses the law to the
                // N=1 bypass mid-transfer). Default OFF = the shipped
                // active_paths() arm.
                let ids = if recov_mp_live {
                    sched.live_paths()
                } else {
                    sched.active_paths()
                };
                if recov_mp_law || recov_sp || diag_on {
                    mp_n_paths = ids.len();
                    for id in &ids {
                        if let Some(p) = sched.path(*id) {
                            mp_clocks.insert(
                                *id,
                                (
                                    p.srtt().as_micros() as u64,
                                    p.estimator.rtt().as_micros() as u64,
                                ),
                            );
                        }
                    }
                }
                ids.iter()
                    .filter_map(|id| sched.path(*id))
                    .map(|p| p.estimator.rtt().as_micros() as u64)
                    .max()
                    .unwrap_or(NACK_RETX_COOLDOWN_FLOOR_US)
            };
            let retx_cooldown_us = srtt_us.max(NACK_RETX_COOLDOWN_FLOOR_US);
            // The per-flight law threshold for a path (falls back to the
            // pooled cooldown clock when the path has no snapshot).
            let mp_thr_of = |mp_clocks: &std::collections::HashMap<u32, (u64, u64)>,
                             p: u32|
             -> u64 {
                mp_clocks
                    .get(&p)
                    .map(|&(a, b)| mp_time_threshold_us(a, b))
                    .unwrap_or(retx_cooldown_us)
            };

            let (win_start, win_end) = encoder.window_span();
            let mut retransmitted: u64 = 0;
            let mut nacked_count: u64 = 0;
            if diag_on {
                mpd_gap_reports += 1;
            }

            // Packet-threshold evidence ingestion (RFC 9002 §6.1.1 per
            // path): fold this report's implied delivered intervals into the
            // per-path sorted evidence lists. Monotone watermark ⇒ each seq
            // ingested at most once over the transfer.
            if recov_mp_law && mp_n_paths > 1 {
                for (lo, hi) in mp_delivered_intervals(&gaps) {
                    let start = lo.max(mp_evid_max + 1);
                    if start > hi {
                        continue;
                    }
                    for (&q, &pj) in source_path_map.range(start..=hi) {
                        mp_delivered.entry(pj).or_default().push(q);
                    }
                    mp_evid_max = mp_evid_max.max(hi);
                }
            }

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
                    if diag_on {
                        mpd_gap_seqs += 1;
                    }
                    // δ-honest shed (fix C): a hole already shed is never
                    // served again (the receiver's own δ-horizon passes it);
                    // a past-deadline hole is shed within the ρ budget — a
                    // retransmit fired at age > D(δ) lands after the
                    // receiver's give-up, pure waste that serializes the
                    // stream. Budget-refused holes fall through to the
                    // legacy ARQ (serialize: ρ wins).
                    if shed_on {
                        if shed_seqs.contains(&seq) {
                            continue;
                        }
                        if let Some(&(send_time_us, _, _)) = retransmit_buffer.get(&seq) {
                            let age = now_repair_us.saturating_sub(send_time_us);
                            if shed_allowed(
                                age,
                                shed_deadline_us_live,
                                shed_total,
                                stats.fec.total_source_symbols.load(Ordering::Relaxed),
                                shed_budget_frac,
                            ) {
                                retransmit_buffer.remove(&seq);
                                nack_retx_at.remove(&seq);
                                shed_seqs.insert(seq);
                                shed_total += 1;
                                continue;
                            }
                            if shed_deadline_us_live > 0 && age > shed_deadline_us_live {
                                shed_denied += 1;
                            }
                        }
                    }
                    // Per-seq cooldown: repeated gap acks for the same
                    // hole must not resend more than once per SRTT.
                    if let Some(&(last, _)) = nack_retx_at.get(&seq) {
                        if now_repair_us.saturating_sub(last) < retx_cooldown_us {
                            if diag_on {
                                mpd_supp_cool += 1;
                            }
                            continue;
                        }
                    }
                    // The seq's LIVE flight: the last retransmit if any
                    // (it inherits the in-flight clock of its own path),
                    // else the original send (feat/recovery-suppression).
                    let mp_flight: Option<(u64, u32)> = nack_retx_at
                        .get(&seq)
                        .copied()
                        .or_else(|| {
                            retransmit_buffer.get(&seq).map(|&(t, _, p)| (t, p))
                        });
                    if recov_mp_law && mp_n_paths > 1 {
                        // The skew-aware hole law — RFC 9002 loss detection
                        // generalized per path, BOTH channels:
                        //  §6.1.1 packet threshold (fast, honest): the
                        //   ORIGINAL flight on path j is lost once ≥3 later
                        //   path-j symbols are delivered (same-path FIFO
                        //   evidence — scheduler-created cross-path gaps
                        //   cannot trigger it). Retransmitted seqs are
                        //   excluded (wire order ≠ seq order for them).
                        //  §6.1.2 time threshold (safety net): a gap whose
                        //   LIVE flight is younger than 9/8× its own path's
                        //   smoothed RTT is a gap the scheduler created,
                        //   not a hole. Suppression-only; the receiver's
                        //   hole-refresh re-advertises until a channel
                        //   fires, so real holes still recover.
                        let time_ripe = match mp_flight {
                            Some((t, p)) => mp_hole_ripe(
                                mp_n_paths,
                                now_repair_us,
                                Some(t),
                                mp_thr_of(&mp_clocks, p),
                            ),
                            None => true,
                        };
                        let mut fast = false;
                        if !time_ripe && !nack_retx_at.contains_key(&seq) {
                            let orig = source_path_map.get(&seq).copied();
                            fast = orig
                                .and_then(|j| mp_delivered.get(&j))
                                .is_some_and(|v| mp_fast_lost(v, seq));
                        }
                        if !time_ripe && !fast {
                            if diag_on {
                                mpd_supp_law += 1;
                            }
                            continue;
                        }
                        if fast && diag_on {
                            mpd_fired_fast += 1;
                        }
                    } else if recov_sp && mp_n_paths <= 1 {
                        // RWM_RECOV_SP (goal-gate "Lossy-Single Residual"):
                        // the same §6.1.2 time threshold applied at N=1 —
                        // a gap seq whose LIVE flight (last retransmit, else
                        // the original) is younger than 9/8×max(smoothed
                        // clocks) is merely late/queued, not lost. TIME
                        // channel only (see the gate's decl note);
                        // suppression-only — the hole-refresh re-advertises.
                        let time_ripe = match mp_flight {
                            Some((t, p)) => {
                                now_repair_us.saturating_sub(t)
                                    >= mp_thr_of(&mp_clocks, p)
                            }
                            None => true,
                        };
                        if !time_ripe {
                            if diag_on {
                                mpd_supp_law += 1;
                            }
                            continue;
                        }
                    } else {
                        // Age gate (legacy): cross-path/jitter skew can
                        // report a seq that is merely late, not lost — only
                        // repair symbols old enough that an in-flight copy
                        // would already have been sacked.
                        if let Some(&(send_time_us, _, _)) = retransmit_buffer.get(&seq) {
                            if now_repair_us.saturating_sub(send_time_us) < srtt_us / 2 {
                                if diag_on {
                                    mpd_supp_age += 1;
                                }
                                continue;
                            }
                        }
                    }
                    // Cross-path: avoid the path that originally carried this
                    // symbol. RWM Phase B (§16.3): the targeted retransmit is
                    // placed by the law with a ρ_fate penalty on the original
                    // path (best path for the exact symbol, minus its fate) —
                    // the continuous form of select_repair_path_avoiding.
                    let original_path = source_path_map.get(&seq).copied().unwrap_or(last_source_path);
                    let nack_path = {
                        let sched = scheduler.lock();
                        if reliable {
                            sched.place_symbol(true, &[original_path]).unwrap_or(last_source_path)
                        } else {
                            select_repair_path_avoiding(&sched, original_path, last_source_path)
                        }
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
                            None => {
                                if diag_on {
                                    mpd_stale += 1;
                                }
                                continue;
                            }
                        }
                    } else {
                        encoder.get_source(seq).unwrap_or_else(|| encoder.generate_repair())
                    };

                    // DIAG (feat/recovery-suppression trace): attribute this
                    // fire — live-flight age vs the per-path law threshold
                    // (young = the law would have suppressed it = the
                    // spurious-by-law class), per-flight-path and per-retx-
                    // path emission counts.
                    if diag_on {
                        if let Some((t, p)) = mp_flight {
                            let age = now_repair_us.saturating_sub(t);
                            let thr = mp_thr_of(&mp_clocks, p);
                            if age < thr {
                                mpd_fired_young += 1;
                            } else {
                                mpd_fired_ripe += 1;
                            }
                            mpd_age_ms_sum += age as f64 / 1000.0;
                            *mpd_fired_flight.entry(p).or_insert(0) += 1;
                        } else {
                            mpd_fired_ripe += 1;
                        }
                        *mpd_fired_on.entry(nack_path).or_insert(0) += 1;
                        // feat/c8-conversion DIAG: retransmit attributed to
                        // the seq's ORIGINAL placement path (conversion-
                        // failure candidate (d): slow-placed symbols being
                        // re-served on the fast path).
                        *c8c_retx_orig.entry(original_path).or_insert(0) += 1;
                    }

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
                    // feat/copa-sole-cc: a retransmit re-commits the seq to
                    // its new path and re-snapshots the rate sample, so the
                    // eventual ack is attributed to the path that actually
                    // delivered it with a truthful send-interval.
                    // (feat/window-mtu scope fix: paused feed = absent feed.)
                    if let Some(feed) = copa_feed.as_ref().filter(|f| !f.n1_paused()) {
                        feed.on_sent(seq, nack_path);
                        let mut sched = scheduler.lock();
                        if let Some(p) = sched.path_mut(nack_path) {
                            p.on_src_sent(seq, false);
                        }
                    }
                    // The retransmit inherits the in-flight state: the next
                    // hole decision for this seq clocks THIS flight on ITS
                    // path (closes the re-NACK-while-flying feedback).
                    nack_retx_at.insert(seq, (now_repair_us, nack_path));
                    stats.fec.total_repair_symbols.fetch_add(1, Ordering::Relaxed);
                    nack_repairs_this_period += 1;
                    cached_nack_budget = cached_nack_budget.saturating_sub(1);
                    diag_retx += 1;
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
                // RWM Phase B (§16.3): place the extra repair margin by the law
                // (fungible repairs cover the whole window → fate over the
                // window's source paths). Single path ⇒ that path.
                let margin_path = {
                    let sched = scheduler.lock();
                    if reliable {
                        let covered = window_source_paths(&*encoder, &source_path_map);
                        sched.place_symbol(true, &covered).unwrap_or(last_source_path)
                    } else {
                        select_repair_path(&sched, last_source_path)
                    }
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
            // feat/c8-conversion DIAG: attribute the just-ended frontier
            // stall (time since the previous cumulative advance, ≥ 5 ms)
            // to the ORIGINAL placement path of the hole that was blocking
            // (seq = prev_ack + 1) — read BEFORE the cleanup below prunes
            // source_path_map to ack+1.
            if diag_on {
                let nowa = now_us();
                if c8c_last_ack_adv_us > 0 {
                    let dt_us = nowa.saturating_sub(c8c_last_ack_adv_us);
                    if dt_us >= 5_000 {
                        if let Some(&owner) = source_path_map.get(&(prev_ack + 1)) {
                            *c8c_stall_ms.entry(owner).or_insert(0) += dt_us / 1000;
                            *c8c_stall_n.entry(owner).or_insert(0) += 1;
                        }
                    }
                }
                c8c_last_ack_adv_us = nowa;
            }
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
            // Generation mode advances by GENERATION: the cumulative ack passes
            // a seq only when its whole generation has decoded and delivered
            // contiguously, so everything at or below `ack` is DONE — drop those
            // generations (advance gen-aligns internally). No W*-behind retention
            // (the coding target is the generation, not a sliding W).
            if generation {
                encoder.advance(ack + 1);
                // GLIFE: fold completed generations into the lifecycle sums
                // (fill = first-source→sealed, code = sealed→last-emit,
                // wait = last-emit→acked). RWM_DIAG only.
                if diag_on {
                    let now_g = now_us();
                    let done: Vec<u64> = gl
                        .keys()
                        .copied()
                        .filter(|&a| a + gen_size as u64 <= ack + 1)
                        .collect();
                    for a in done {
                        if let Some((f, s, e)) = gl.remove(&a) {
                            if f > 0 && s >= f && e >= s {
                                gl_sum.0 += s - f;
                                gl_sum.1 += e - s;
                                gl_sum.2 += now_g.saturating_sub(e);
                                gl_sum.3 += 1;
                            }
                        }
                    }
                }
                // Drop per-generation deficit bookkeeping for generations that
                // have now been fully delivered + dropped (anchors below the
                // retained window start). Keeps the maps bounded to the M
                // in-flight generations.
                let (win_start, _) = encoder.window_span();
                gen_want.retain(|&a, _| a >= win_start);
                gen_emitted.retain(|&a, _| a >= win_start);
                gen_emitted_at_report.retain(|&a, _| a >= win_start);
                gen_recover_at.retain(|&a, _| a >= win_start);
            } else {
                let keep_behind = derived_window
                    .map(|w| w.clamp(16, win_cap))
                    .unwrap_or(win_cap / 2) as u64;
                encoder.advance(ack.saturating_sub(keep_behind));
            }

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
            // δ-honest shed set: pruned on the same cumulative twin (the
            // receiver's frontier passing a shed seq closes its story).
            if !shed_seqs.is_empty() {
                shed_seqs = shed_seqs.split_off(&(ack + 1));
            }
            // RWM Phase A: the sent-data store is drained by acks ONLY —
            // this is the whole retention contract.
            sent_store = sent_store.split_off(&(ack + 1));
            // RWM_STORE_SACK_RELEASE: the released-mark set prunes on the
            // SAME cumulative twin — at/below the frontier the slot is now
            // FULLY freed (payload dropped above, mark dropped here); the
            // subset-of-sent_store invariant is preserved. No-op when off.
            if !sack_released.is_empty() {
                sack_release_prune(&mut sack_released, ack);
            }
            // task #86: cumulative release of the per-path accounts (the
            // split_off twin; seqs already SACK-released are gone from the
            // account map, so no double-release).
            if percap_track {
                percap_release_cumulative(&mut percap_acct, &mut percap_out, ack);
                // feat/store-borrowing: repay every loan the frontier
                // advance just released (the split_off twin — SACK-repaid
                // loans are gone from the ledger, no double-repayment).
                if percap_borrow_on {
                    percap_loan_release_cumulative(
                        &mut percap_loans,
                        &mut percap_lent,
                        &mut percap_borrowed,
                        ack,
                    );
                }
            }
            // Drop NACK-retransmit cooldown entries for delivered seqs (P10b)
            nack_retx_at.retain(|&seq, _| seq > ack);
            // feat/recovery-suppression: drop packet-threshold evidence the
            // frontier passed (counts are only ever taken above a live gap,
            // and gaps are above the frontier).
            if recov_mp_law {
                for v in mp_delivered.values_mut() {
                    let idx = v.partition_point(|&x| x <= ack);
                    v.drain(..idx);
                }
            }
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
        // Generation mode is EXEMPT: dropping a not-yet-decoded generation's
        // sources would make its coded symbols unsolvable (there is no per-seq
        // store to fall back on). Backpressure (store_max) already bounds the
        // retained pipeline to M generations, and advance() only ever drops
        // fully-decoded generations — so no size-pressure eviction is needed.
        if !generation && encoder.window_size() > win_cap {
            let (oldest, _) = encoder.window_span();
            encoder.advance(oldest + (encoder.window_size() - win_cap) as u64);
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

/// Create a window encoder for the given backend. (The retired Streaming arm
/// was the only one that read the FEC controller/scheduler — the signature
/// shrank with it, 2026-07-28.)
fn create_window_encoder(
    backend: FecBackend,
    symbol_size: u16,
) -> Box<dyn WindowEncoder> {
    match backend {
        FecBackend::Mettle => Box::new(MettleWindowEncoder::new(
            mettle::MettleConfig::small_window(),
            symbol_size,
            42,
        )),
        _ => Box::new(RlcWindowEncoder::new(symbol_size)),
    }
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
        FecBackend::Mettle => Box::new(MettleWindowDecoder::new(symbol_size)),
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
    // Some(..) in generation mode: forwards an inbound GenerationDeficit's
    // (anchor, deficit) vector to the local window sender's recovery loop.
    deficit_tx: Option<&tokio::sync::mpsc::Sender<Vec<(u64, u32)>>>,
    // Some(..) in plain-reliable mode: forwards the WindowAck's RECEIVED-above-
    // frontier ranges to the local window sender so it can prune the sent-store
    // for out-of-order deliveries (SACK flow control). None disables it.
    sack_tx: Option<&tokio::sync::mpsc::Sender<Vec<(u64, u64)>>>,
    // feat/copa-sole-cc: Some(..) in PLAIN in-order window-reliable mode when
    // the Copa delivery feed is enabled (RWM_QUIC_CC=passthrough or
    // RWM_COPA_FEED=1). Each WindowAck's frontier/SACK diff is attributed
    // per path into the send-interval rate sampler + the Copa cwnd dynamics
    // (`copa_feed_attribute`), and the resulting per-path cwnd is written
    // into the pass-through substrate window. None = shipped path,
    // byte-identical.
    copa_feed: Option<&Arc<CopaFeed>>,
    // feat/anchor-hygiene (`RWM_MSTAR_ANCHOR`, resolved once in run_impl —
    // src/gates.rs): suppress the peer-report RTT pseudo-sample feed.
    mstar_anchor: bool,
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
            // (`cumulative_received` — the peer's total decoded count `d`,
            // FMTCP-era "change 1" — stays on the wire for the debug trace
            // above; its sender-side consumer was removed with RWM_FMTCP.)
            // Update RTT from echoed timestamp. echo == 0 is the sentinel
            // for timer-driven acks (hold-expiry unwedge) that echo no
            // batch — recording now−0 would poison SRTT with a huge sample.
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

        ControlMessage::GenerationDeficit { deficits } => {
            debug!(
                path_id,
                gen_count = deficits.len(),
                first = ?deficits.first(),
                "generation deficit feedback received"
            );
            // Forward to the local window sender's recovery loop (generation
            // mode only). Best-effort: a dropped report is re-sent by the
            // receiver next SRTT, and the in-flight accounting self-corrects.
            if let Some(tx) = deficit_tx {
                let _ = tx.try_send(deficits);
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

    // ── δ-honest overload shedding (fix C, goal-gate "Unified Shedding") ──

    /// Pre-registered invariant 1: shed ONLY past-deadline AND within the
    /// ρ budget. Fresh data is never shed however large the budget; stale
    /// data is never shed past the budget; a cold (0) deadline sheds
    /// nothing.
    #[test]
    fn shed_only_past_deadline_and_within_rho_budget() {
        let d = 25_000u64; // D(δ) = 25 ms
        // Past deadline, budget open (1% of 1000 sources = 10 allowed).
        assert!(shed_allowed(30_000, d, 0, 1_000, 0.01));
        assert!(shed_allowed(30_000, d, 9, 1_000, 0.01));
        // Budget exactly spent: the 11th shed is refused (serialize).
        assert!(!shed_allowed(30_000, d, 10, 1_000, 0.01));
        // Fresh (within deadline): never shed, however large the budget.
        assert!(!shed_allowed(10_000, d, 0, 1_000, 1.0));
        assert!(!shed_allowed(d, d, 0, 1_000, 1.0), "age == D is not past it");
        // Cold start: no derived deadline or zero budget ⇒ nothing sheds.
        assert!(!shed_allowed(30_000, 0, 0, 1_000, 1.0));
        assert!(!shed_allowed(30_000, d, 0, 1_000, 0.0));
        assert!(!shed_allowed(30_000, d, 0, 0, 0.5), "no sources ⇒ no budget");
    }

    /// Pre-registered invariant 2: the reliable-transfer contract (ρ = 1,
    /// RETAIN-UNTIL-ACKED) is NEVER shed — the law is compiled out on the
    /// reliable path by construction, and it never arms outside the
    /// unified machine or against the explicit =0 opt-out.
    #[test]
    fn shed_never_arms_on_reliable_contract() {
        // The only armed combination: unified + EVICT + gate on.
        assert!(shed_armed(true, false, true));
        // Reliable (bulk/auto window_reliable) NEVER sheds.
        assert!(!shed_armed(true, true, true));
        // Legacy machines (unified off) never shed.
        assert!(!shed_armed(false, false, true));
        // RWM_UNIFIED_SHED=0 = the serializing control arm.
        assert!(!shed_armed(true, false, false));
    }

    /// The shed deadline IS the span law's D (§16.20.3): b·RTprop, capped
    /// at the 2·RTprop deficit-round limit — no new constants.
    #[test]
    fn shed_deadline_is_the_span_law_d() {
        // Realtime b = ½: D = RTprop/2.
        assert_eq!(shed_deadline_us(0.5, 40_000), 20_000);
        // Auto b = 1: D = RTprop.
        assert_eq!(shed_deadline_us(1.0, 40_000), 40_000);
        // Bulk b = 2 caps at the 2·RTprop limit.
        assert_eq!(shed_deadline_us(2.0, 40_000), 80_000);
        assert_eq!(shed_deadline_us(4.0, 40_000), 80_000);
    }

    /// Receiver arm: the in-order hold is the δ dial (b·SRTT, b = ½ on the
    /// realtime-only EVICT path) while the give-up budget is open, and
    /// reverts to the LEGACY 4×SRTT ∈ [60, 300] ms clamp when the law is
    /// off or the budget is spent — bit-exact legacy in both fallbacks.
    #[test]
    fn shed_recv_hold_delta_dial_and_legacy_fallback() {
        let srtt = Duration::from_millis(80);
        assert_eq!(shed_recv_hold(srtt, true, true), Duration::from_millis(40));
        let legacy = (srtt * 4).clamp(BLOCK_REORDER_MIN_HOLD, BLOCK_REORDER_MAX_HOLD);
        assert_eq!(shed_recv_hold(srtt, true, false), legacy, "budget spent ⇒ serialize");
        assert_eq!(shed_recv_hold(srtt, false, true), legacy, "law off ⇒ legacy");
        // Legacy clamps still bind in the fallback (60 ms floor / 300 ms cap).
        assert_eq!(
            shed_recv_hold(Duration::from_millis(10), false, false),
            Duration::from_millis(60)
        );
        assert_eq!(
            shed_recv_hold(Duration::from_millis(200), false, false),
            Duration::from_millis(300)
        );
    }

    /// Receiver give-up budget: the loss-class bound — holes given up may
    /// never exceed ε̂_recv × frontier; a clean channel (ε̂ = 0) never opens
    /// the budget (nothing to shed on a clean channel anyway).
    #[test]
    fn shed_recv_budget_is_loss_class() {
        assert!(shed_recv_budget_ok(0, 1_000, 0.05));
        assert!(shed_recv_budget_ok(49, 1_000, 0.05));
        assert!(!shed_recv_budget_ok(50, 1_000, 0.05));
        assert!(!shed_recv_budget_ok(0, 0, 0.05), "no frontier yet ⇒ closed");
        assert!(!shed_recv_budget_ok(0, 1_000, 0.0), "clean channel ⇒ closed");
    }

    /// PART 1 (receiver-tail parallelization). With the legacy bound (6) a
    /// lossy bulk transfer reports only the first 6 outstanding generations'
    /// deficits per round, so holes are repaired frontier-first — one round-
    /// trip per ~6 generations (serial tail). Lifting `report_gens` to cover
    /// the whole in-flight range reports EVERY outstanding generation's deficit
    /// in ONE report, so the sender repairs all holes in a single round-trip.
    /// This is the "all deficits recover in one round" invariant.
    #[test]
    fn receiver_tail_reports_all_deficits_in_one_round() {
        // 50 outstanding generations, each K=384, each 3 DoF short (rank 381).
        let mut gen_widths: BTreeMap<u64, u16> = BTreeMap::new();
        for g in 0..50u64 {
            gen_widths.insert(g * 384, 384);
        }
        let rank_of = |_anchor: u64, k: u64| k - 3; // deficit 3 in every gen

        // Legacy bound: only the frontier-first 6 generations are reported —
        // the tail is serialized (the remaining 44 wait for future rounds).
        let d6 = collect_gen_deficits(&gen_widths, 6, rank_of);
        assert_eq!(d6.len(), 6, "legacy bound reports only 6 generations");

        // Parallel tail flush: ALL 50 holes reported in a single round.
        let all = collect_gen_deficits(&gen_widths, 256, rank_of);
        assert_eq!(all.len(), 50, "every outstanding generation reported at once");
        assert!(all.iter().all(|&(_, d)| d == 3));
        let total: u32 = all.iter().map(|(_, d)| d).sum();
        assert_eq!(total, 150, "the full residual deficit is requested in one round");

        // Fully-decoded generations (deficit 0) are omitted regardless of cap.
        let none = collect_gen_deficits(&gen_widths, 256, |_a, k| k);
        assert!(none.is_empty(), "decoded generations report no deficit");
    }

    /// Repair-coverage horizon (branch `feat/nack-timing`): a hole covered by
    /// the in-flight proactive repair WITHIN the horizon fires NO reactive NACK;
    /// a hole still uncovered when the horizon EXPIRES falls back to the NACK.
    #[test]
    fn horizon_withholds_nack_until_repair_window_then_falls_back() {
        use std::time::{Duration, Instant};
        let horizon = Duration::from_millis(5);
        let mut armed: BTreeMap<u64, Instant> = BTreeMap::new();
        let t0 = Instant::now();

        // A frontier generation just went deficient. First sight → ARMED and
        // WITHHELD: no reactive NACK yet (give the proactive repair its horizon).
        let d = vec![(0u64, 3u32)];
        let ready = horizon_gate_deficits(&d, &mut armed, horizon, t0);
        assert!(ready.is_empty(), "a fresh hole is withheld, not NACKed immediately");
        assert_eq!(armed.len(), 1, "the fresh hole is armed");

        // The proactive repair decodes it within the horizon → it drops out of
        // the deficit set → disarmed, and NO NACK ever fired (the proactive win).
        let none: Vec<(u64, u32)> = vec![];
        let ready = horizon_gate_deficits(&none, &mut armed, horizon, t0 + Duration::from_millis(2));
        assert!(ready.is_empty());
        assert!(armed.is_empty(), "decoded-within-horizon hole is disarmed with no NACK");

        // A hole that proactive repair does NOT cover: still deficient after the
        // horizon expires → the reactive NACK fires (the reliability fallback).
        let d2 = vec![(384u64, 2u32)];
        let ready = horizon_gate_deficits(&d2, &mut armed, horizon, t0);
        assert!(ready.is_empty(), "still withheld before the horizon");
        let ready = horizon_gate_deficits(&d2, &mut armed, horizon, t0 + Duration::from_millis(6));
        assert_eq!(ready, vec![(384, 2)], "horizon expired uncovered → reactive fallback fires");

        // horizon == 0 restores the immediate (byte-identical) shipped path.
        let mut armed0: BTreeMap<u64, Instant> = BTreeMap::new();
        let ready = horizon_gate_deficits(&d, &mut armed0, Duration::ZERO, t0);
        assert_eq!(ready, d, "horizon 0 reports immediately (shipped path)");
    }

    /// Lossy stream (EVICT / datagram): a full inject channel must DROP and
    /// return immediately — delivery must never block a lossy stream on a
    /// slow consumer (the user requirement: "if loss is allowed it doesn't
    /// actually block").
    #[tokio::test]
    async fn deliver_packet_lossy_drops_never_blocks() {
        let (tx, mut rx) = mpsc::channel::<Bytes>(1);
        tx.try_send(Bytes::from_static(b"a")).unwrap(); // fill to capacity
        let r = tokio::time::timeout(
            Duration::from_millis(250),
            deliver_packet(&tx, Bytes::from_static(b"b"), false),
        )
        .await;
        assert!(r.is_ok(), "lossy delivery must not block on a full channel");
        assert_eq!(r.unwrap(), Ok(()));
        // "b" was dropped: only the original "a" is queued.
        assert_eq!(rx.try_recv().unwrap(), Bytes::from_static(b"a"));
        assert!(rx.try_recv().is_err());
    }

    /// Reliable stream: a full inject channel must BACKPRESSURE (await), not
    /// drop — otherwise the frontier/ack advances past an undelivered symbol
    /// and leaves a permanent hole (the flaky-loopback bug this fixes).
    #[tokio::test]
    async fn deliver_packet_reliable_backpressures_then_delivers() {
        let (tx, mut rx) = mpsc::channel::<Bytes>(1);
        tx.send(Bytes::from_static(b"a")).await.unwrap(); // fill to capacity
        // Must block while full...
        let blocked = tokio::time::timeout(
            Duration::from_millis(150),
            deliver_packet(&tx, Bytes::from_static(b"b"), true),
        )
        .await;
        assert!(blocked.is_err(), "reliable delivery must block on a full channel");
        // ...and lose nothing once the consumer drains a slot.
        assert_eq!(rx.recv().await.unwrap(), Bytes::from_static(b"a"));
        deliver_packet(&tx, Bytes::from_static(b"b"), true)
            .await
            .unwrap();
        assert_eq!(rx.recv().await.unwrap(), Bytes::from_static(b"b"));
    }

    /// A permanently closed channel errors under both policies (the caller
    /// tears the receiver down).
    #[tokio::test]
    async fn deliver_packet_closed_channel_errors() {
        let (tx, rx) = mpsc::channel::<Bytes>(1);
        drop(rx);
        assert!(deliver_packet(&tx, Bytes::from_static(b"x"), true).await.is_err());
        assert!(deliver_packet(&tx, Bytes::from_static(b"x"), false).await.is_err());
    }

    #[test]
    fn test_parse_cidr() {
        let (ip, prefix) = parse_cidr("10.99.0.1/24").unwrap();
        assert_eq!(ip, "10.99.0.1".parse::<IpAddr>().unwrap());
        assert_eq!(prefix, 24);
    }

    /// feat/window-mtu part 1: the stall-metered allowance is continuous —
    /// base at zero stall, grows at exactly the anchor rate through a
    /// frontier freeze, and caps at one recovery round (R_ins = 100 ms).
    /// No threshold, no mode bit: allow(g) is piecewise-linear in g alone.
    #[test]
    fn win_decouple_allow_is_stall_metered_and_capped() {
        let base = 190.0; // sc2-class residence: anchor 83 * (K 1.3 + 1)
        let rate = 10_400.0;
        assert_eq!(win_decouple_allow(base, rate, 0.0), base);
        // 3 ms micro-freeze: +rate*3ms ≈ 31 symbols — the sub-sweep
        // ack-granularity cover the diagnosis named.
        let a3 = win_decouple_allow(base, rate, 0.003);
        assert!((a3 - (base + rate * 0.003)).abs() < 1e-9);
        // Linear through the sweep scale...
        let a80 = win_decouple_allow(base, rate, 0.080);
        assert!((a80 - (base + rate * 0.080)).abs() < 1e-9);
        // ...and capped at R_ins: a 5 s wedge cannot mint an unbounded
        // window (backpressure resumes, bounded).
        let acap = win_decouple_allow(base, rate, 5.0);
        assert!((acap - (base + rate * WIN_STALL_INS_S)).abs() < 1e-9);
        // Negative clock skew clamps to base, never below.
        assert_eq!(win_decouple_allow(base, rate, -1.0), base);
    }

    /// feat/window-mtu part 1: the retention backstop = full metered
    /// allowance + one recovery round of hole capacity (N_hole = 1, from
    /// the diagnosis), memory-clamped at WIN_STORE_MAX and floored.
    #[test]
    fn win_decouple_cap_ret_bounds_holes_and_memory() {
        // sc3-class: anchor 81*(K1.3+1)=186, rate 1.8k, RTprop 45 ms:
        // 186 + 1800*(0.1 + 0.1 + 0.045) = 627 — between the honest cap
        // (~355) and the legacy latch (1024), and every term derived.
        let c = win_decouple_cap_ret(186.0, 1800.0, 0.045, 64);
        assert_eq!(c, 627); // 186 + 1800*(0.1 + 0.1 + 0.045)
        assert!(c > 355 && c < 1024);
        // A jitter-cell Copa seat (base = 2*cwnd ≈ 1100 at 40 ms RTprop,
        // rate ~10.4k) must be allowed ABOVE the legacy 1024 latch — the
        // B1 dwell-ceiling release — and below the memory clamp.
        let copa = win_decouple_cap_ret(1100.0, 10_400.0, 0.040, 64);
        assert!(copa > RELIABLE_STORE_MAX && copa <= WIN_STORE_MAX);
        // Memory clamp binds for an absurd rate; floor binds when cold.
        assert_eq!(win_decouple_cap_ret(1e9, 1e9, 1.0, 64), WIN_STORE_MAX);
        assert_eq!(win_decouple_cap_ret(0.0, 0.0, 0.0, 64), 64);
    }

    /// feat/window-mtu part 1: the decoupled gate excludes recovery-stalled
    /// holes from the wire budget — the head-span arithmetic. A store full
    /// of below-frontier holes must not consume fresh-admission budget;
    /// the SAME totals under the legacy gate would read paused.
    #[test]
    fn win_decouple_head_span_excludes_holes() {
        // 300 un-SACKed total; frontier at 950 of 1000 sent ⇒ head span =
        // 50 (live wire), holes = 250 (below-frontier recovery seats).
        let last_sent: u64 = 1000;
        let frontier: u64 = 950;
        let outstanding: usize = 300;
        let head = last_sent.saturating_sub(frontier) as usize;
        let hole = outstanding.saturating_sub(head);
        assert_eq!(head, 50);
        assert_eq!(hole, 250);
        let allow = win_decouple_allow(190.0, 10_400.0, 0.0);
        // Decoupled: 50 < 190 ⇒ wire keeps feeding. Legacy on the same
        // state: 300 >= 190-class cap ⇒ paused (the D1 channel closed
        // structurally even though the diagnosis measured it small).
        assert!(head < allow as usize);
        assert!(outstanding >= allow as usize);
    }

    /// feat/window-mtu: the N1-scoped sampler pause — a paused feed records
    /// no send commitments and attribution only fast-forwards the cursor
    /// (no samples, no stale-record attribution after resume).
    #[test]
    fn copa_feed_n1_pause_is_fully_inert() {
        let feed = CopaFeed::new_sampling_only(true);
        assert!(!feed.owns_cc());
        feed.set_n1_paused(true);
        feed.on_sent(10, 0);
        assert!(feed.seq_path.is_empty(), "paused on_sent must record nothing");
        // Pre-pause leftovers below the frontier are pruned by the paused
        // attribution path's fast-forward (simulated here directly).
        feed.set_n1_paused(false);
        feed.on_sent(11, 0);
        feed.set_n1_paused(true);
        {
            let mut c = feed.cursor.lock();
            if 20 >= c.next {
                c.next = 21;
            }
            c.sacked.retain(|&s| s > 20);
        }
        feed.seq_path.retain(|&s, _| s > 20);
        assert!(feed.seq_path.is_empty());
        // Resume: the cursor starts at the live frontier — an old ack
        // yields no attributions.
        feed.set_n1_paused(false);
        assert!(feed.newly_delivered(15, &[]).is_empty());
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

    // ----- CopaFeed attribution cursor (feat/copa-sole-cc) -----

    /// Frontier advance attributes each seq exactly once, in order.
    #[test]
    fn copa_feed_frontier_attributes_once() {
        let feed = CopaFeed::new();
        assert_eq!(feed.newly_delivered(2, &[]), vec![0, 1, 2]);
        // Duplicate/stale ack → empty diff, never a re-attribution.
        assert!(feed.newly_delivered(2, &[]).is_empty());
        assert!(feed.newly_delivered(1, &[]).is_empty());
        assert_eq!(feed.newly_delivered(4, &[]), vec![3, 4]);
    }

    /// SACKed seqs above the frontier are attributed immediately and NOT
    /// re-attributed when the frontier later passes them.
    #[test]
    fn copa_feed_sack_dedupes_against_frontier() {
        let feed = CopaFeed::new();
        // Frontier at 1, receiver also has 5..=6 (hole 2..=4).
        assert_eq!(feed.newly_delivered(1, &[(5, 6)]), vec![0, 1, 5, 6]);
        // Same SACK re-advertised → nothing new.
        assert!(feed.newly_delivered(1, &[(5, 6)]).is_empty());
        // Hole repaired: frontier jumps to 7 — only the gap seqs (2..=4)
        // and 7 are new; 5..=6 were consumed from the sacked set.
        assert_eq!(feed.newly_delivered(7, &[]), vec![2, 3, 4, 7]);
    }

    /// seq→path attribution: the seq is charged to the path it was (last)
    /// sent on; unknown seqs fall back to the ack path. A cross-path
    /// retransmit keeps the previous commitment as the flight-witness
    /// fallback (residual (iii)).
    #[test]
    fn copa_feed_seq_path_last_send_wins() {
        let feed = CopaFeed::new();
        feed.on_sent(10, 0);
        feed.on_sent(10, 1); // retransmit on the other path
        let commit = feed.seq_path.remove(&10).map(|(_, c)| c).unwrap();
        assert_eq!(commit.last.0, 1);
        assert_eq!(commit.prev.map(|(p, _)| p), Some(0));
        assert!(feed.seq_path.remove(&10).is_none());
    }

    /// Residual (iii): the flight-time witness. An ack younger than the
    /// retransmit path's RTprop proves the ORIGINAL flight delivered the
    /// seq — the retransmit path's delivered counter must not advance. An
    /// ack older than RTprop credits the retransmit path (a genuine
    /// retransmit delivery). Unknown RTprop / single-path history keep
    /// legacy attribution.
    #[test]
    fn flight_witness_credits_original_path_for_spurious_retransmit() {
        // Original on fast (path 0) at t=0, retransmitted on slow (path 1,
        // RTprop 60 ms) at t=1_000_000.
        let commit = SendCommit {
            last: (1, 1_000_000),
            prev: Some((0, 0)),
        };
        let rtprop = |pid: u32| if pid == 1 { Some(60_000u64) } else { Some(8_000u64) };
        // Ack 5 ms after the retransmit: the slow flight cannot have
        // completed — the fast original delivered it.
        assert_eq!(resolve_flight_path(&commit, 1_005_000, rtprop), 0);
        // Ack 80 ms after the retransmit: the slow flight qualifies.
        assert_eq!(resolve_flight_path(&commit, 1_080_000, rtprop), 1);
        // Exactly RTprop old: qualifies (>=).
        assert_eq!(resolve_flight_path(&commit, 1_060_000, rtprop), 1);
        // Warm-up (no RTprop yet): legacy last-sent attribution.
        assert_eq!(resolve_flight_path(&commit, 1_005_000, |_| None), 1);
        // Single-path history: always the last (= only) commitment.
        let single = SendCommit {
            last: (1, 1_000_000),
            prev: None,
        };
        assert_eq!(resolve_flight_path(&single, 1_000_001, rtprop), 1);
        // A→B→A bounce: prev is the previous DISTINCT path (B), and a
        // young ack after the same-path resend credits B's older flight.
        let feed = CopaFeed::new();
        feed.on_sent(7, 0);
        feed.on_sent(7, 1);
        feed.on_sent(7, 0);
        let c = feed.seq_path.remove(&7).map(|(_, c)| c).unwrap();
        assert_eq!(c.last.0, 0);
        assert_eq!(c.prev.map(|(p, _)| p), Some(1));
    }

    // ----- feat/recovery-suppression: the skew-aware hole law -----

    /// The per-flight time threshold is the RFC 9002 §6.1.2 shape: 9/8 of
    /// the larger smoothed clock, floored at the per-seq cooldown floor
    /// (the kGranularity analog). No new constants.
    #[test]
    fn mp_time_threshold_is_nine_eighths_of_max_clock_with_floor() {
        // 40 ms srtt, 32 ms ewma → 9/8 × 40 ms = 45 ms.
        assert_eq!(mp_time_threshold_us(40_000, 32_000), 45_000);
        // The larger clock wins regardless of which estimator it is.
        assert_eq!(mp_time_threshold_us(32_000, 40_000), 45_000);
        // Tiny clocks floor at NACK_RETX_COOLDOWN_FLOOR_US.
        assert_eq!(mp_time_threshold_us(1_000, 500), NACK_RETX_COOLDOWN_FLOOR_US);
        assert_eq!(mp_time_threshold_us(0, 0), NACK_RETX_COOLDOWN_FLOOR_US);
    }

    /// The skew-aware hole law: a gap on path A while the seq's flight is
    /// still inside path B's expected-arrival clock is NOT a hole; once
    /// B's clock expires it IS. Single path (N=1) keeps legacy behavior
    /// bit-exactly (the law never suppresses), and an unknown flight is
    /// never suppressed (reliability backstop).
    #[test]
    fn mp_hole_law_suppresses_young_cross_path_flights_only() {
        let thr = 45_000u64; // path B's 9/8×srtt clock
        // Dual path, flight sent at t=1_000_000 on B.
        // t = +10 ms: inside B's clock → NOT a hole.
        assert!(!mp_hole_ripe(2, 1_010_000, Some(1_000_000), thr));
        // t = +45 ms: B's clock expired → a hole (retransmit eligible).
        assert!(mp_hole_ripe(2, 1_045_000, Some(1_000_000), thr));
        // t = +44.999 ms: still inside (strict).
        assert!(!mp_hole_ripe(2, 1_044_999, Some(1_000_000), thr));
        // N=1: the law is INERT — always ripe regardless of age (the
        // legacy gates own the decision; sc2/sc3 bit-exact).
        assert!(mp_hole_ripe(1, 1_010_000, Some(1_000_000), thr));
        assert!(mp_hole_ripe(0, 1_010_000, Some(1_000_000), thr));
        // Unknown flight: never suppress (a seq we cannot clock must stay
        // recoverable — the legacy path decides).
        assert!(mp_hole_ripe(2, 1_010_000, None, thr));
    }

    /// The law composes with retransmit flight inheritance: after a
    /// retransmit the LIVE flight is the retransmit on ITS path, so the
    /// seq is suppressed until the NEW flight's clock expires (closes the
    /// re-NACK-while-flying feedback), then ripe again.
    #[test]
    fn mp_hole_law_clocks_the_live_flight_after_retransmit() {
        let thr_b = 45_000u64;
        // Original flight expired → fired at t=1_045_000; the retransmit
        // becomes the live flight at that instant.
        let retx_at = 1_045_000u64;
        // Immediately after: suppressed (the retransmit is still flying).
        assert!(!mp_hole_ripe(2, retx_at + 1_000, Some(retx_at), thr_b));
        // After the retransmit path's clock: ripe again (escalation if the
        // retransmit itself died).
        assert!(mp_hole_ripe(2, retx_at + thr_b, Some(retx_at), thr_b));
    }

    /// The packet-threshold fast channel (RFC 9002 §6.1.1 per path): a gap
    /// report's implied delivered intervals, and the ≥3-same-path-successors
    /// decision. Cross-path skew gaps cannot trigger it (their same-path
    /// successors are equally un-arrived); real same-path losses fire in
    /// ~one skew instead of a full RTT.
    #[test]
    fn mp_packet_threshold_evidence_and_decision() {
        // Report: missing 5..=6 and 9..=9 → delivered 7..=8 (between gaps)
        // and 10 (the seq that bounded the last gap).
        assert_eq!(
            mp_delivered_intervals(&[(5, 6), (9, 9)]),
            vec![(7, 8), (10, 10)]
        );
        // Single gap: only the bounding seq is provable.
        assert_eq!(mp_delivered_intervals(&[(5, 6)]), vec![(7, 7)]);
        // Adjacent gaps produce no between-interval.
        assert_eq!(mp_delivered_intervals(&[(5, 6), (7, 8)]), vec![(9, 9)]);
        assert!(mp_delivered_intervals(&[]).is_empty());

        // Decision: 3 delivered path-j successors above s = lost.
        let ev = vec![10, 12, 14, 16];
        assert!(mp_fast_lost(&ev, 9), ">=3 successors above 9");
        assert!(mp_fast_lost(&ev, 10), "3 above 10 (12,14,16)");
        assert!(!mp_fast_lost(&ev, 12), "only 2 above 12");
        assert!(!mp_fast_lost(&ev, 20), "none above 20");
        assert!(!mp_fast_lost(&[], 0), "no evidence, never lost-fast");
        // The cross-path skew shape: path A delivered 100..102 while s=50
        // flies on B with NO delivered B successors — B's evidence list is
        // empty, so the fast channel never fires for B's flight.
        let b_ev: Vec<u64> = vec![];
        assert!(!mp_fast_lost(&b_ev, 50));
    }

    // ----- feat/recovery-suppression: per-path batch serial namespaces -----

    /// The loss-serial defect, reproduced at the unit: a GLOBAL batch_seq
    /// striped across two paths makes each path's tracker read the OTHER
    /// path's run as loss (expected ≈ 2×received at round-robin — ~50%
    /// phantom loss with zero real loss). Per-path serial namespaces
    /// (each path's stream sequential) read exactly 0% loss on the same
    /// arrival pattern.
    #[test]
    fn per_path_batch_serials_kill_striping_phantom_loss() {
        // GLOBAL counter, round-robin striping, NO loss: path 0 gets
        // even serials, path 1 odd.
        let mut t0 = PathBatchTracker::new();
        let mut t1 = PathBatchTracker::new();
        for s in 0..200u64 {
            if s % 2 == 0 {
                t0.record_batch(s, 1);
            } else {
                t1.record_batch(s, 1);
            }
        }
        // Phantom loss: each path expected ~2× what it received.
        assert!(t0.total_expected >= t0.total_received * 2 - 2);
        assert!(t1.total_expected >= t1.total_received * 2 - 2);

        // PER-PATH serials, same striping, no loss: sequential per path.
        let mut p0 = PathBatchTracker::new();
        let mut p1 = PathBatchTracker::new();
        for s in 0..100u64 {
            p0.record_batch(s, 1);
            p1.record_batch(s, 1);
        }
        assert_eq!(p0.total_expected, p0.total_received, "no phantom loss");
        assert_eq!(p1.total_expected, p1.total_received, "no phantom loss");

        // Per-path serials still see REAL loss: drop serials 10..=14.
        let mut pl = PathBatchTracker::new();
        for s in 0..100u64 {
            if (10..=14).contains(&s) {
                continue; // lost on the wire
            }
            pl.record_batch(s, 1);
        }
        assert_eq!(pl.total_expected - pl.total_received, 5, "real loss still counted");
    }

    /// A hostile/corrupt ack cannot trap the diff in a huge loop.
    #[test]
    fn copa_feed_per_ack_work_is_bounded() {
        let feed = CopaFeed::new();
        let newly = feed.newly_delivered(u64::MAX - 1, &[]);
        assert!(newly.len() <= 65_536);
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
        // control, not eviction. (Out-of-order object mode keeps this cap:
        // relaxing it was MEASURED harmful — the coding window slid off the
        // recovery frontier; see the store_backpressure NOTE.)
        assert!(!store_backpressure(true, RELIABLE_STORE_MAX - 1));
        assert!(store_backpressure(true, RELIABLE_STORE_MAX));
        assert!(store_backpressure(true, RELIABLE_STORE_MAX + 1));
        // EVICT mode never backpressures on retention.
        assert!(!store_backpressure(false, RELIABLE_STORE_MAX * 10));
    }

    // ----- Path-scaled outstanding pool (task #84, RWM_STORE_PATHS) -----------

    #[test]
    fn path_scaled_store_cap_is_legacy_for_singles_and_off() {
        // Flag OFF: always legacy, regardless of path count.
        assert_eq!(path_scaled_store_cap(false, 2, 1000.0, 2.0, 64, 2048), None);
        // Flag ON but a single live path: legacy law bit-exactly (the
        // property that keeps singles byte-identical with the flag set).
        assert_eq!(path_scaled_store_cap(true, 1, 1000.0, 2.0, 64, 2048), None);
        // No dynamic base yet (anchor cold): legacy boot-cap path decides.
        assert_eq!(path_scaled_store_cap(true, 2, 0.0, 2.0, 64, 2048), None);
    }

    #[test]
    fn path_scaled_store_cap_scales_value_and_ceiling_with_paths() {
        // C7-shaped: Σ anchor-BDP ≈ 1076, gain 2, N = 2 → 2·2·1076 = 4304,
        // clamped at the N×2048 = 4096 ceiling (the measured knee: C7
        // 4096 → 141.3 Mbit vs 1024 → 103; deeper pools saturate/collapse).
        assert_eq!(
            path_scaled_store_cap(true, 2, 1076.0, 2.0, 64, 2048),
            Some(4096)
        );
        // Below the ceiling the dynamic value rules (transient anchor sag).
        assert_eq!(
            path_scaled_store_cap(true, 2, 500.0, 2.0, 64, 2048),
            Some(2000)
        );
        // Floor guards a transiently-tiny estimate.
        assert_eq!(path_scaled_store_cap(true, 2, 1.0, 2.0, 64, 2048), Some(64));
        // Three paths: ceiling 3×2048.
        assert_eq!(
            path_scaled_store_cap(true, 3, 4000.0, 2.0, 64, 2048),
            Some(3 * 2048)
        );
    }

    // ----- Capacity-weighted pool (RWM_STORE_CAPW, "C8-Aware Pool Law") -------

    #[test]
    fn capw_store_cap_not_engaged_off_single_or_unwarm() {
        // Flag OFF: never engaged.
        assert_eq!(
            capw_store_cap(false, &[Some(1000.0), Some(400.0)], 64, 2048),
            None
        );
        // N = 1: not engaged — the caller keeps the legacy law bit-exactly
        // (the same singles contract as RWM_STORE_PATHS).
        assert_eq!(capw_store_cap(true, &[Some(1000.0)], 64, 2048), None);
        // Anchors-not-warm fallback: ANY unwarm live path → None → the
        // caller keeps the CONFIGURED pooled law (path-scaled / legacy)
        // until every anchor is live — a partial sum would under-provision
        // the unwarm path's share of the shared pool.
        assert_eq!(capw_store_cap(true, &[Some(1000.0), None], 64, 2048), None);
        assert_eq!(capw_store_cap(true, &[None, None], 64, 2048), None);
        assert_eq!(
            capw_store_cap(true, &[Some(1000.0), Some(0.0)], 64, 2048),
            None
        );
        assert_eq!(capw_store_cap(true, &[], 64, 2048), None);
    }

    #[test]
    fn capw_store_cap_symmetric_is_n_times_single() {
        // The c7 degenerate: N identical paths → pool = N × the single-path
        // honest term (≈ N×(single pool) — symmetric cells preserved).
        let single = honest_store_cap(Some(83.2), Some(10_400.0), 1.5, 2.0).unwrap();
        // 83.2·(1.5+1) + 10 400·1·0.1 = 208 + 1040 = 1248.
        assert!((single - 1248.0).abs() < 1e-6);
        assert_eq!(
            capw_store_cap(true, &[Some(single), Some(single)], 64, 2048),
            Some(2496)
        );
        // Three symmetric paths: 3× (ceiling 3×2048 not binding).
        assert_eq!(
            capw_store_cap(true, &[Some(single); 3].to_vec(), 64, 2048),
            Some(3744)
        );
    }

    #[test]
    fn capw_store_cap_asymmetric_weights_by_capacity_not_path_count() {
        // The c8 shape (the law's target cell): a c2-class fast path
        // (10 400 sym/s, RTprop 8 ms → anchor 83.2) + a c3-class slow path
        // (2000 sym/s, RTprop 40 ms → anchor 80). Each earns its OWN pipe +
        // recovery round — the slow path's 1/5 rate earns ~1/3 of the fast
        // term (its longer RTprop partially offsets), NOT the equal ×knee
        // share the path-count law grants.
        let fast = honest_store_cap(Some(83.2), Some(10_400.0), 1.5, 2.0).unwrap(); // 1248
        let slow = honest_store_cap(Some(80.0), Some(2_000.0), 1.5, 2.0).unwrap(); // 200+200=400
        assert!((slow - 400.0).abs() < 1e-6);
        let pool = capw_store_cap(true, &[Some(fast), Some(slow)], 64, 2048).unwrap();
        assert_eq!(pool, 1648);
        // The verdict shape the diagnosis predicts: strictly between the
        // legacy 1024 latch (fast path under-provisioned) and the
        // path-scaled N×2048 = 4096 (slow path over-provisioned).
        assert!(pool > 1024 && pool < 4096);
        // Capacity weighting: the slow path's contribution is its own term,
        // ~24% of the pool — not the 50% the count-scaled ceiling implies.
        assert!((slow / (fast + slow) - 0.243).abs() < 0.01);
    }

    #[test]
    fn capw_store_cap_overread_anchors_clamp_to_the_path_scaled_ceiling() {
        // The legacy plain anchor over-reads ×4.6–7.4 ("Anchor Hygiene"
        // battery (b)): inflated terms clamp at the N×knee ceiling — the
        // path-scaled degenerate (which is why the battery arm composes
        // RWM_PLAIN_RS=1; without honest anchors the law cannot
        // differentiate). Floor guards transiently-tiny terms.
        assert_eq!(
            capw_store_cap(true, &[Some(6.0 * 1248.0), Some(6.0 * 400.0)], 64, 2048),
            Some(4096)
        );
        assert_eq!(
            capw_store_cap(true, &[Some(1.0), Some(2.0)], 64, 2048),
            Some(64)
        );
    }

    // ----- Per-path outstanding accounting (task #86, RWM_STORE_PERCAP) -------

    #[test]
    fn percap_store_cap_is_rate_x_echo_rtt_with_floor_and_ceiling() {
        // Derived, not tuned: cap_i = gain × rate_i × echoRTT_i. A c2-like
        // fast path (BtlBw ≈ 10 400 sym/s, echo RTT ≈ 80 ms): pipe = 832,
        // gain 2 → 1664 — inside [64, 2048].
        assert_eq!(
            percap_store_cap(Some(10_400.0 * 0.080), 1024, 2, 2.0, 64, 2048),
            1664
        );
        // A c3-like slow path (BtlBw ≈ 2000 sym/s, echo RTT ≈ 60 ms): pipe
        // = 120, gain 2 → 240 — its OWN shallow cap, independent of the
        // fast path's.
        assert_eq!(
            percap_store_cap(Some(2000.0 * 0.060), 1024, 2, 2.0, 64, 2048),
            240
        );
        // Ceiling: the measured 2048-per-path knee bounds a deep pipe (and
        // the echo-RTT positive feedback).
        assert_eq!(percap_store_cap(Some(4000.0), 1024, 2, 2.0, 64, 2048), 2048);
        // Floor: a transiently-tiny anchor cannot strangle the account.
        assert_eq!(percap_store_cap(Some(3.0), 1024, 2, 2.0, 64, 2048), 64);
    }

    #[test]
    fn percap_store_cap_warmup_inherits_equal_legacy_share() {
        // Anchor not established (None): equal share of the legacy pooled
        // cap, bounded — converges to the derived cap once the anchor warms.
        assert_eq!(percap_store_cap(None, 1024, 2, 2.0, 64, 2048), 512);
        assert_eq!(percap_store_cap(None, 1024, 4, 2.0, 64, 2048), 256);
        // Non-positive pipe is warm-up too (cold Copa cwnd cannot happen,
        // but the law must not divide into nonsense).
        assert_eq!(percap_store_cap(Some(0.0), 1024, 2, 2.0, 64, 2048), 512);
        // Share is bounded by the same [floor, pool] clamp.
        assert_eq!(percap_store_cap(None, 100, 2, 2.0, 64, 2048), 64);
        assert_eq!(percap_store_cap(None, 100_000, 2, 2.0, 64, 2048), 2048);
    }

    // ----- Honest floor-clock caps (feat/percap-honest-cap) -------------------

    #[test]
    fn echo_ratio_min_is_self_queue_proof_and_window_expires() {
        let mut e = EchoRatioMin::new(5_000_000);
        // Before any sample: K = 1 (the floor-law degenerate).
        assert_eq!(e.k(), 1.0);
        // Early unloaded samples set the honest drain-clock ratio.
        assert!((e.observe(1.5, 1_000_000) - 1.5).abs() < 1e-9);
        // Self-queue inflation (dwell → echo → ratio) CANNOT raise the min —
        // the c8 parking spiral has no handle on this statistic.
        assert!((e.observe(4.0, 2_000_000) - 1.5).abs() < 1e-9);
        assert!((e.observe(8.0, 3_000_000) - 1.5).abs() < 1e-9);
        // Degenerate samples are inert (NaN) or clamped (≥ 1: a smoothed
        // echo transiently under the windowed-min floor is clock noise).
        assert!((e.observe(f64::NAN, 3_500_000) - 1.5).abs() < 1e-9);
        let mut e2 = EchoRatioMin::new(5_000_000);
        assert_eq!(e2.observe(0.8, 1_000_000), 1.0);
        // Anchor-hygiene rule 3 — the window EXPIRES: a stale unloaded read
        // rolls out after two half-windows and the ratio re-measures.
        assert!((e.observe(3.0, 6_500_000) - 1.5).abs() < 1e-9); // prev bucket holds 1.5
        assert!((e.observe(3.5, 11_600_000) - 3.0).abs() < 1e-9); // 1.5 expired
    }

    #[test]
    fn echo_ratio_seed_identity_sample_is_discarded_not_latched() {
        // The measured L1-smoke defect: at the estimator seed instant
        // srtt ≡ min_rtt (shared seeding), ratio ≡ 1.0 — feeding it latches
        // the windowed min at 1.0 for a whole window (khr pinned 1.00 while
        // rtt/rtp read 16/8 ms). The seed-identity sample must be DISCARDED.
        let mut e = EchoRatioMin::new(5_000_000);
        let ms = |m: u64| std::time::Duration::from_millis(m);
        // Seed instant: srtt == RTprop bit-equal → no sample, K stays 1.0
        // as the DEFAULT (not as a latched measurement).
        assert_eq!(e.observe_srtt_over_rtprop(ms(8), Some(ms(8)), 1_000_000), 1.0);
        // A real measurement then sets the min — it was not latched at 1.0.
        assert!(
            (e.observe_srtt_over_rtprop(ms(16), Some(ms(8)), 2_000_000) - 2.0).abs()
                < 1e-9
        );
        // Warm-up (no RTprop) observes nothing.
        assert!(
            (e.observe_srtt_over_rtprop(ms(16), None, 3_000_000) - 2.0).abs() < 1e-9
        );
    }

    #[test]
    fn honest_store_cap_is_residence_plus_recovery_runway() {
        // Derived, not tuned: cap_i = anchor_i·(K_i + gain − 1) +
        // rate_i·(gain−1)·R — residence (Little's law on the unloaded
        // drain clock) + (gain−1) recovery rounds on the RECOVERY engine's
        // clock (R = 100 ms, the hole-refresh/tail-sweep cadence bound)
        // plus the retransmit flight (the anchor term's second round).
        // c2-like at the MEASURED cell clocks (rate 10 400 sym/s, RTprop
        // 8 ms → anchor 83.2; K = 2): 83.2×3 + 10 400×0.1 = 1289.6.
        let c = honest_store_cap(Some(10_400.0 * 0.008), Some(10_400.0), 2.0, 2.0)
            .unwrap();
        assert!((c - 1289.6).abs() < 1e-6);
        // K clamps at 1 from below (K < 1 is clock noise).
        assert_eq!(
            honest_store_cap(Some(83.2), Some(10_400.0), 0.5, 2.0),
            honest_store_cap(Some(83.2), Some(10_400.0), 1.0, 2.0)
        );
        // gain = 1: pure residence, zero runway — the R term vanishes with
        // (gain−1), never a negative runway.
        let g1 = honest_store_cap(Some(83.2), Some(10_400.0), 1.5, 1.0).unwrap();
        assert!((g1 - 83.2 * 1.5).abs() < 1e-9);
        let g05 = honest_store_cap(Some(83.2), Some(10_400.0), 1.5, 0.5).unwrap();
        assert!((g05 - 83.2 * 1.5).abs() < 1e-9);
        // Warm-up (no anchor / no rate): None — the caller keeps the
        // legacy warm-up share (warm-up unchanged).
        assert_eq!(honest_store_cap(None, Some(10_400.0), 2.0, 2.0), None);
        assert_eq!(honest_store_cap(Some(83.2), None, 2.0, 2.0), None);
        assert_eq!(honest_store_cap(Some(0.0), Some(10_400.0), 2.0, 2.0), None);
    }

    #[test]
    fn honest_caps_shallow_account_sits_at_recovery_budget_not_knee() {
        // The GUARD-RESULTS residual (i) in miniature, on HONEST anchors.
        // Deep c2-like path (rate 10 400, RTprop 8 ms, K 1.5): cap = 1248
        // — DIFFERENTIATED, inside the knee (the over-read arm read btlbw
        // 8–10× truth and knee-clamped to 2048).
        let fast = (honest_store_cap(Some(10_400.0 * 0.008), Some(10_400.0), 1.5, 2.0)
            .unwrap()
            .ceil() as usize)
            .clamp(64, 2048);
        assert_eq!(fast, 1248);
        assert!(fast < 2048, "fast cap must be derived, not knee-clamped");
        // Shallow c8-slow-class path (rate 1 954, RTprop 60 ms → anchor
        // 117.2; K 1.3): cap = 466 ≈ the guard session's MEASURED good pin
        // (508 outstanding, 0.26 s dwell) — its recovery budget, NOT the
        // 2048 knee (≈1 s parked dwell) the over-read held it at. The
        // own-pick parking channel is closed by construction.
        let slow = (honest_store_cap(Some(1_954.0 * 0.060), Some(1_954.0), 1.3, 2.0)
            .unwrap()
            .ceil() as usize)
            .clamp(64, 2048);
        assert_eq!(slow, 466);
        assert!(slow < 2048 / 4, "slow cap must sit at its recovery budget, not the knee");
        // Per-path independence still holds: deepening the fast pipe does
        // not move the slow cap.
        let fast2 = (honest_store_cap(Some(20_800.0 * 0.008), Some(20_800.0), 1.5, 2.0)
            .unwrap()
            .ceil() as usize)
            .clamp(64, 2048);
        assert!(fast2 > fast);
        assert_eq!(
            (honest_store_cap(Some(1_954.0 * 0.060), Some(1_954.0), 1.3, 2.0)
                .unwrap()
                .ceil() as usize)
                .clamp(64, 2048),
            slow
        );
    }

    #[test]
    fn honest_anchor_sum_cap_preserves_sc2_throughput_headroom() {
        // The −20% resolution at the law level ("Anchor Hygiene" battery
        // (b): sc2 P 79.9 → PRS 61.7 because the cap fell from the 1024
        // latch to gain·anchor_honest ≈ 150–170 at the cell's true 8-ms
        // RTprop — a 100-Mbit pipe whose RECOVERY round is ~12× its wire
        // round trip).
        let rate: f64 = 10_400.0;
        let anchor: f64 = rate * 0.008;
        let store_max = 1024usize;
        // The legacy floor law on honest anchors (the RWM_HONEST_CAP=0
        // control arm): gain·anchor = 167 ≪ 1024 — the measured −20% arm.
        let floor_law = ((2.0 * anchor).ceil() as usize).clamp(64, store_max);
        assert_eq!(floor_law, 167);
        // The honest law at the measured drain clocks (K ≈ 2): 1290 →
        // latches the legacy-proven 1024 store — the over-read's accidental
        // headroom re-supplied from the engine's own recovery cadence.
        let honest = (honest_store_cap(Some(anchor), Some(rate), 2.0, 2.0)
            .unwrap()
            .ceil() as usize)
            .clamp(64, store_max);
        assert_eq!(honest, store_max);
        // Monotone: for ANY measured K ≥ 1 the honest cap strictly exceeds
        // the legacy floor law — honest anchors can widen but never shrink
        // the single-path window relative to the control.
        for k in [1.0, 1.2, 1.7, 2.5, 4.0] {
            let c = (honest_store_cap(Some(anchor), Some(rate), k, 2.0)
                .unwrap()
                .ceil() as usize)
                .clamp(64, store_max);
            assert!(c > floor_law);
        }
    }

    #[test]
    fn percap_store_full_pauses_only_when_no_account_has_headroom() {
        // One account below its cap ⇒ admit (the infl_percap_full pattern:
        // the slow path's full account never starves the fast path).
        assert!(!percap_store_full(&[(240, 240), (100, 1664)]));
        // Every account at/over its cap ⇒ paused.
        assert!(percap_store_full(&[(240, 240), (1664, 1664)]));
        assert!(percap_store_full(&[(300, 240), (1700, 1664)]));
        // A zero cap counts as cap 1 (never a permanently-closed account).
        assert!(!percap_store_full(&[(0, 0)]));
        assert!(percap_store_full(&[(1, 0)]));
    }

    #[test]
    fn percap_place_redirects_capfull_pick_to_headroom_path() {
        // bound = cap in these cases: the unguarded legacy redirect law
        // (RWM_PERCAP_GUARD=0), preserved exactly.
        let accounts = [
            (0u32, 240usize, 240usize, 240usize),
            (1u32, 100usize, 1664usize, 1664usize),
        ];
        // Slow path (p0) at ITS cap: a p0 pick redirects to the deep path.
        assert_eq!(percap_place_path(0, &accounts), 1);
        // A pick with its own headroom stays.
        assert_eq!(percap_place_path(1, &accounts), 1);
        // All full (racing the gate): keep the pick — the gate pauses next
        // iteration; the slop is one placement.
        assert_eq!(
            percap_place_path(0, &[(0, 240, 240, 240), (1, 1664, 1664, 1664)]),
            0
        );
        // Redirect goes to the MOST relative headroom.
        assert_eq!(
            percap_place_path(
                2,
                &[(0, 200, 240, 240), (1, 100, 1664, 1664), (2, 50, 50, 50)]
            ),
            1
        );
    }

    #[test]
    fn percap_redirect_bound_is_floor_clock_bdp() {
        // Derived, not tuned: bound_j = rate_j × RTprop_j — κ=1 on the FLOOR
        // clock (κ=1 on the loaded echo clock is vacuous: echoRTT ≈ RTprop +
        // out/rate, the measured c8 feedback). A c3-like slow path (rate ≈
        // 1534 sym/s ≈ 15.7 Mbit of 1279-B symbols, RTprop 60 ms): bound =
        // 93 symbols ≈ one un-queued pipe — vs the knee-adjacent cap 1531
        // the unguarded redirect filled (≈1.3 s dwell).
        assert_eq!(percap_redirect_bound(Some(1534.0 * 0.060), 1531, 2.0), 93);
        // Never above the account's own cap.
        assert_eq!(percap_redirect_bound(Some(5000.0), 2048, 2.0), 2048);
        // Warm-up (no anchor): the share's pipe term, cap/gain.
        assert_eq!(percap_redirect_bound(None, 512, 2.0), 256);
        // One-symbol quantum: a cold/tiny account is never permanently
        // redirect-closed, and degenerate inputs cannot divide into nonsense.
        assert_eq!(percap_redirect_bound(Some(0.5), 240, 2.0), 1);
        assert_eq!(percap_redirect_bound(Some(-1.0), 0, 2.0), 1);
    }

    #[test]
    fn percap_store_full_guarded_backpressures_instead_of_parking() {
        // No account cap-full → admit (every pick lands on its own account).
        assert!(!percap_store_full_guarded(&[(100, 240, 93), (500, 1664, 800)]));
        // Fast cap-full + slow WITHIN its dwell bound → admit (a guard-
        // eligible redirect target exists).
        assert!(!percap_store_full_guarded(&[(50, 240, 93), (1664, 1664, 800)]));
        // Fast cap-full + slow past its dwell bound (though under cap) →
        // FULL: the redirect would park symbols behind the slow path's
        // standing queue — backpressure instead. THE c8 fix.
        assert!(percap_store_full_guarded(&[(120, 240, 93), (1664, 1664, 800)]));
        // All cap-full → FULL (the unguarded law, unchanged).
        assert!(percap_store_full_guarded(&[(240, 240, 93), (1664, 1664, 800)]));
        // bound = cap degenerates exactly to the unguarded gate.
        assert_eq!(
            percap_store_full_guarded(&[(120, 240, 240), (1664, 1664, 1664)]),
            percap_store_full(&[(120, 240), (1664, 1664)])
        );
        assert_eq!(
            percap_store_full_guarded(&[(240, 240, 240), (1664, 1664, 1664)]),
            percap_store_full(&[(240, 240), (1664, 1664)])
        );
    }

    /// The c8 regression in miniature, guarded (roadmap item 1): the fast
    /// account pegs at its cap; overflow redirects fill the slow account
    /// only to its FLOOR-CLOCK dwell bound (rate×RTprop), then redirect
    /// STOPS and the guarded gate reads FULL — admission pauses instead of
    /// parking ~cap symbols (≈1.3 s dwell at L1) on the slow path. Deep-path
    /// redirects within bound are unaffected; the slow path's OWN picks are
    /// never guard-gated.
    #[test]
    fn percap_redirect_guard_stops_at_dwell_bound_and_pauses_admission() {
        const SLOW: u32 = 0;
        const FAST: u32 = 1;
        // Slow: rate 2000 sym/s, echoSRTT 60 ms, RTprop 30 ms → cap 240,
        // bound 60. Fast: rate 10 000, echoSRTT 80 ms, RTprop 40 ms →
        // cap 1600, bound 400.
        let slow_cap = percap_store_cap(Some(2000.0 * 0.060), 1024, 2, 2.0, 64, 2048);
        let fast_cap = percap_store_cap(Some(10_000.0 * 0.080), 1024, 2, 2.0, 64, 2048);
        let slow_bound = percap_redirect_bound(Some(2000.0 * 0.030), slow_cap, 2.0);
        let fast_bound = percap_redirect_bound(Some(10_000.0 * 0.040), fast_cap, 2.0);
        assert_eq!((slow_cap, fast_cap), (240, 1600));
        assert_eq!((slow_bound, fast_bound), (60, 400));

        let mut acct: BTreeMap<u64, u32> = BTreeMap::new();
        let mut out: std::collections::HashMap<u32, usize> =
            std::collections::HashMap::new();
        let accounts4 = |out: &std::collections::HashMap<u32, usize>| {
            [
                (SLOW, out.get(&SLOW).copied().unwrap_or(0), slow_cap, slow_bound),
                (FAST, out.get(&FAST).copied().unwrap_or(0), fast_cap, fast_bound),
            ]
        };
        let accounts3 = |out: &std::collections::HashMap<u32, usize>| {
            accounts4(out).map(|(_, o, c, b)| (o, c, b))
        };
        // The c8 shape: the softmax favors the fast path — every pick FAST.
        let mut seq = 0u64;
        while !percap_store_full_guarded(&accounts3(&out)) {
            let placed = percap_place_path(FAST, &accounts4(&out));
            percap_charge(&mut acct, &mut out, seq, placed);
            seq += 1;
            assert!(seq < 10_000, "guarded gate must close");
        }
        // Fast filled to ITS cap (own picks are never guard-gated); the
        // overflow parked on slow stopped at the DWELL BOUND — 60 symbols
        // (30 ms of dwell at slow's rate), not the 240 cap (120 ms), and
        // nothing like the L1 ~2048 (1.3 s).
        assert_eq!(out[&FAST], fast_cap, "fast account pegs at its own cap");
        assert_eq!(
            out[&SLOW], slow_bound,
            "redirect STOPS at the floor-clock dwell bound, far below the cap"
        );
        // The unguarded gate would still admit here (slow has cap headroom)
        // — that admission IS the measured c8 parking regression; the
        // guarded gate reads FULL instead: backpressure, don't park.
        assert!(!percap_store_full(
            &accounts3(&out).map(|(o, c, _)| (o, c))
        ));
        assert!(percap_store_full_guarded(&accounts3(&out)));
        // Racing the closed gate, a further fast pick finds no eligible
        // target and keeps the pick (one-placement slop, gate closes).
        assert_eq!(percap_place_path(FAST, &accounts4(&out)), FAST);
        // The slow path's OWN picks are not guard-gated: with the gate open
        // (fast drained below cap) a slow pick with cap headroom places
        // directly even though slow is past its redirect bound.
        let fast_seqs: Vec<u64> = acct
            .iter()
            .filter(|&(_, &p)| p == FAST)
            .map(|(&s, _)| s)
            .take(200)
            .collect();
        for s in &fast_seqs {
            percap_release_seq(&mut acct, &mut out, *s);
        }
        assert!(!percap_store_full_guarded(&accounts3(&out)), "gate reopens on drain");
        assert_eq!(
            percap_place_path(SLOW, &accounts4(&out)),
            SLOW,
            "own-pick placement is never guard-gated below the cap"
        );
        // And a fast pick now redirects nowhere (slow still ≥ bound) — it
        // has its own headroom back, so it just places on fast.
        assert_eq!(percap_place_path(FAST, &accounts4(&out)), FAST);
        // Gauges stay Σ-consistent (the charge/release lockstep invariant).
        assert_eq!(out.values().sum::<usize>(), acct.len());
    }

    /// The C8 conflict in miniature (the #84 residual this feature exists
    /// for): a deep c2-like account and a shallow c3-like account coexist —
    /// the shallow path's cap does NOT inflate when the deep path's does,
    /// its outstanding never exceeds its own cap (placements past it
    /// redirect to the deep account), and out-of-order acks release the
    /// right account.
    #[test]
    fn percap_deep_and_shallow_accounts_coexist_without_coupling() {
        // Caps from the derivation itself: fast pipe deepens ×2 mid-run,
        // the slow cap must not move (per-path independence — the exact
        // failure of the SHARED pool, where raising the cap for the fast
        // path collapsed the slow one).
        let slow_cap = percap_store_cap(Some(2000.0 * 0.060), 1024, 2, 2.0, 64, 2048);
        let fast_cap0 = percap_store_cap(Some(5000.0 * 0.080), 1024, 2, 2.0, 64, 2048);
        let fast_cap1 = percap_store_cap(Some(10_000.0 * 0.080), 1024, 2, 2.0, 64, 2048);
        assert_eq!(slow_cap, 240);
        assert_eq!(fast_cap0, 800);
        assert_eq!(fast_cap1, 1600);
        assert_eq!(
            percap_store_cap(Some(2000.0 * 0.060), 1024, 2, 2.0, 64, 2048),
            slow_cap,
            "shallow cap must not inflate when the deep path's pipe grows"
        );

        // Draw/release accounting: stripe placements 50/50 (the scheduler's
        // pick), with the redirect enforcing the accounts.
        let mut acct: BTreeMap<u64, u32> = BTreeMap::new();
        let mut out: std::collections::HashMap<u32, usize> =
            std::collections::HashMap::new();
        const SLOW: u32 = 0;
        const FAST: u32 = 1;
        // bound = cap: this test documents the UNGUARDED accounting law
        // (RWM_PERCAP_GUARD=0); the guarded law has its own miniature below.
        let caps = |out: &std::collections::HashMap<u32, usize>| {
            [
                (SLOW, out.get(&SLOW).copied().unwrap_or(0), 240usize, 240usize),
                (FAST, out.get(&FAST).copied().unwrap_or(0), 1600usize, 1600usize),
            ]
        };
        let mut seq = 0u64;
        // Admit while ANY account has headroom (the gate), place round-robin
        // through the redirect (the placement law).
        loop {
            let accounts: Vec<(usize, usize)> =
                caps(&out).iter().map(|&(_, o, c, _)| (o, c)).collect();
            if percap_store_full(&accounts) {
                break;
            }
            let pick = if seq % 2 == 0 { SLOW } else { FAST };
            let placed = percap_place_path(pick, &caps(&out));
            percap_charge(&mut acct, &mut out, seq, placed);
            seq += 1;
        }
        // The shallow account sits exactly at ITS pipe-derived cap; the deep
        // account filled to ITS OWN cap — the overflow went to the deep
        // path, the shallow path was never over-committed.
        assert_eq!(out[&SLOW], 240, "slow outstanding pinned at its own cap");
        assert_eq!(out[&FAST], 1600, "fast account absorbed the redirect");
        assert_eq!(acct.len(), 240 + 1600);

        // Out-of-order acks (SACK ranges land fast-path seqs first): only
        // the fast account drains; the slow account is untouched.
        let fast_seqs: Vec<u64> = acct
            .iter()
            .filter(|&(_, &p)| p == FAST)
            .map(|(&s, _)| s)
            .take(600)
            .collect();
        for s in &fast_seqs {
            percap_release_seq(&mut acct, &mut out, *s);
        }
        assert_eq!(out[&FAST], 1000);
        assert_eq!(out[&SLOW], 240, "OOO fast acks must not drain the slow account");
        // Idempotence: re-releasing a SACKed seq is a no-op (no
        // double-release when the cumulative frontier passes it later).
        percap_release_seq(&mut acct, &mut out, fast_seqs[0]);
        assert_eq!(out[&FAST], 1000);

        // Cumulative frontier passes the first 300 seqs: each releases its
        // OWN account (already-SACKed ones release nothing).
        let below: (usize, usize) = acct.range(..=299u64).fold((0, 0), |m, (_, &p)| {
            if p == SLOW { (m.0 + 1, m.1) } else { (m.0, m.1 + 1) }
        });
        percap_release_cumulative(&mut acct, &mut out, 299);
        assert_eq!(out[&SLOW], 240 - below.0);
        assert_eq!(out[&FAST], 1000 - below.1);
        // Gauges stay Σ-consistent with the account map (the invariant the
        // sender loop's charge/release lockstep preserves).
        assert_eq!(out.values().sum::<usize>(), acct.len());
        // With headroom restored, admission resumes.
        let accounts: Vec<(usize, usize)> =
            caps(&out).iter().map(|&(_, o, c, _)| (o, c)).collect();
        assert!(!percap_store_full(&accounts));
    }

    /// N = 1 identity: the percap law is engaged only for N ≥ 2 live paths
    /// (caller gates on `pipes.len() >= 2`, so `percap_caps` stays empty and
    /// the tx_paused expression is the legacy branch verbatim). What CAN be
    /// asserted purely: warm-up at N = 1 would be the full legacy cap — the
    /// share degenerates to the pool itself, no behavior cliff on a 2→1
    /// live-path flap while accounts drain.
    #[test]
    fn percap_warmup_share_degenerates_to_legacy_at_n1() {
        assert_eq!(percap_store_cap(None, 1024, 1, 2.0, 64, 2048), 1024);
        assert_eq!(percap_store_cap(None, 0, 1, 2.0, 64, 2048), 64);
    }

    // ----- Bounded account borrowing (feat/store-borrowing, §16.22) --------

    /// The c8 honest miniature (paper §16.22.3(b)): the slow lender lends
    /// exactly its headroom beyond its own reserved intake for the loan's
    /// return latency — and lending toward a slow pipe is IMPOSSIBLE (the
    /// #86 parking direction is unrepresentable, not merely guarded).
    #[test]
    fn borrow_lend_room_reserves_lender_intake_and_is_one_directional() {
        // Honest c8 anchors: slow 2000 sym/s @ 60 ms (cap ≈ 500), fast
        // 10 400 sym/s @ 8 ms (cap ≈ 1230, cap-full and asking).
        let slow = BorrowAccount {
            path: 1,
            out: 150,
            cap: 500,
            fly: 150,
            rate: Some(2_000.0),
            rtprop_s: Some(0.060),
        };
        let fast = BorrowAccount {
            path: 0,
            out: 1230,
            cap: 1230,
            fly: 1230,
            rate: Some(10_400.0),
            rtprop_s: Some(0.008),
        };
        // T_return(fast) = 1230/10400 + 0.008 ≈ 0.1263 s → reservation =
        // ceil(2000·0.1263) = 253 → room = 500 − 150 − 253 = 97: the slow
        // account lends its unused runway, NOT its own short-horizon need.
        assert_eq!(percap_lend_room(&slow, &fast), 97);
        // Post-loan solvency invariant: after lending the full room, the
        // lender still holds ≥ its reserved intake (cap − out − room =
        // reservation).
        assert_eq!(slow.cap - slow.out - percap_lend_room(&slow, &fast), 253);
        // fast → slow (the parking direction): a cap-full slow borrower
        // has T_return = 500/2000 + 0.06 = 0.31 s → the fast lender's
        // reservation = ceil(10400·0.31) = 3224 ≫ cap 1230 → room ≡ 0
        // even with a completely EMPTY fast account.
        let slow_full = BorrowAccount {
            out: 500,
            fly: 500,
            ..slow
        };
        let fast_empty = BorrowAccount {
            out: 0,
            fly: 0,
            ..fast
        };
        assert_eq!(percap_lend_room(&fast_empty, &slow_full), 0);
        // Warm-up on either side lends nothing (isolation, not the pool).
        let cold = BorrowAccount {
            rate: None,
            ..slow
        };
        assert_eq!(percap_lend_room(&cold, &fast), 0);
        let cold_borrower = BorrowAccount {
            rtprop_s: None,
            ..fast
        };
        assert_eq!(percap_lend_room(&slow, &cold_borrower), 0);
        // The lender pick: with a second lender offering less room, the
        // max-room lender wins; with none, no loan.
        let slow2 = BorrowAccount {
            path: 2,
            out: 400,
            ..slow
        };
        assert_eq!(
            percap_borrow_lender(0, &[fast, slow, slow2]),
            Some(1),
            "max lend room (97 vs 0) picks the slack lender"
        );
        assert_eq!(percap_borrow_lender(1, &[fast_empty, slow_full]), None);
    }

    /// The symmetric-neutrality THEOREM (paper §16.22.3(c)): at a
    /// rate/RTprop-symmetric cell a cap-full borrower forces the lender's
    /// reservation above the lender's whole cap (reservation − cap =
    /// anchor > 0), so loans are identically zero for EVERY lender state —
    /// the c7 percap win is preserved by proof, not tuning.
    #[test]
    fn borrow_is_identically_zero_at_symmetric_cells() {
        let mk = |path: u32, out: usize| BorrowAccount {
            path,
            out,
            cap: 1000,
            fly: out,
            rate: Some(5_000.0),
            rtprop_s: Some(0.020),
        };
        // Borrower cap-full (the only time it asks): T_return = 1000/5000
        // + 0.02 = 0.22 s → reservation = 1100 > cap = 1000.
        let borrower = mk(0, 1000);
        for lender_out in [0usize, 100, 500, 999] {
            assert_eq!(
                percap_lend_room(&mk(1, lender_out), &borrower),
                0,
                "symmetric lender (out={lender_out}) must lend 0"
            );
        }
        assert_eq!(percap_borrow_lender(0, &[borrower, mk(1, 0)]), None);
        assert!(!percap_lend_edge_exists(&[borrower, mk(1, 0)]));
    }

    /// The degenerate cases frame the design space (paper §16.22.3(d)):
    /// dropping the reservation (T_return → 0) is the POOLED Σcap law —
    /// lend up to cap − out; the all-cap-full state has no lend edge, so
    /// the borrowed admission gate degenerates to the unguarded FULL.
    #[test]
    fn borrow_degenerates_to_pool_without_reservation_and_to_percap_when_closed() {
        // T_return = 0 (empty pipe, zero RTprop — the reservation term
        // vanishes): room = cap − out, i.e. any account's slack is
        // anyone's — the pooled law. The reservation term is the whole
        // difference between the principled point and the pool.
        let lender = BorrowAccount {
            path: 1,
            out: 300,
            cap: 500,
            fly: 300,
            rate: Some(2_000.0),
            rtprop_s: Some(0.060),
        };
        let degenerate_borrower = BorrowAccount {
            path: 0,
            out: 1230,
            cap: 1230,
            fly: 0,
            rate: Some(10_400.0),
            rtprop_s: Some(0.0),
        };
        assert_eq!(
            percap_lend_room(&lender, &degenerate_borrower),
            lender.cap - lender.out
        );
        // All accounts cap-full → every lender's own headroom is 0 → no
        // edge: the gate reads FULL exactly like the no-borrow gate
        // (aggregate law: borrowing can move headroom, never mint it).
        let full = |path: u32| BorrowAccount {
            path,
            out: 1000,
            cap: 1000,
            fly: 1000,
            rate: Some(5_000.0),
            rtprop_s: Some(0.010),
        };
        assert!(!percap_lend_edge_exists(&[full(0), full(1)]));
    }

    /// The loan ledger lifecycle (the c8 miniature end-to-end): a loan
    /// charges the LENDER's account while flying on the borrower's pipe,
    /// the gauges correct account→pipe occupancy, and the SAME acks that
    /// release the store repay the loan — idempotently, SACK or cumulative.
    #[test]
    fn borrow_loans_charge_lender_fly_on_borrower_and_repay_on_ack() {
        let mut acct: BTreeMap<u64, u32> = BTreeMap::new();
        let mut out: std::collections::HashMap<u32, usize> = Default::default();
        let mut loans: BTreeMap<u64, (u32, u32)> = BTreeMap::new();
        let mut lent: std::collections::HashMap<u32, usize> = Default::default();
        let mut borrowed: std::collections::HashMap<u32, usize> = Default::default();
        // Fast (path 0) cap-full; three picks borrow from slow (path 1):
        // the symbols FLY on 0, are CHARGED to 1.
        for seq in [100u64, 101, 102] {
            percap_charge(&mut acct, &mut out, seq, 1);
            percap_loan_charge(&mut loans, &mut lent, &mut borrowed, seq, 1, 0);
        }
        assert_eq!(out.get(&1), Some(&3), "loans charge the LENDER's account");
        assert_eq!(out.get(&0), None, "the borrower's account is untouched");
        assert_eq!(lent.get(&1), Some(&3));
        assert_eq!(borrowed.get(&0), Some(&3));
        // Pipe gauges: fly_0 = out_0 − lent_0 + borrowed_0 = 0 − 0 + 3;
        // fly_1 = 3 − 3 + 0 = 0 — the slow PIPE carries none of the loan.
        // (Computed by the caller; asserted here from the gauge parts.)
        assert_eq!(0 + borrowed.get(&0).copied().unwrap_or(0), 3);
        assert_eq!(
            out.get(&1).copied().unwrap_or(0) - lent.get(&1).copied().unwrap_or(0),
            0
        );
        // SACK (OOO) repayment of 101: account + ledger release together.
        percap_release_seq(&mut acct, &mut out, 101);
        percap_loan_release(&mut loans, &mut lent, &mut borrowed, 101);
        assert_eq!(out.get(&1), Some(&2));
        assert_eq!(lent.get(&1), Some(&2));
        assert_eq!(borrowed.get(&0), Some(&2));
        // Idempotent re-release (SACK re-advertisement).
        percap_loan_release(&mut loans, &mut lent, &mut borrowed, 101);
        assert_eq!(lent.get(&1), Some(&2));
        // Cumulative frontier advance repays the rest (split_off twin).
        percap_release_cumulative(&mut acct, &mut out, 102);
        percap_loan_release_cumulative(&mut loans, &mut lent, &mut borrowed, 102);
        assert_eq!(out.get(&1), Some(&0));
        assert_eq!(lent.get(&1), Some(&0));
        assert_eq!(borrowed.get(&0), Some(&0));
        assert!(loans.is_empty(), "every loan self-liquidated on ack");
    }

    /// The admission-gate composition (paper §16.22.4): the borrowed gate
    /// opens the guarded-FULL state exactly when a lend edge exists, and
    /// only then.
    #[test]
    fn borrow_admission_gate_opens_only_on_a_real_lend_edge() {
        // Guarded gate reads FULL: fast (path 0) cap-full, slow (path 1)
        // open but past its redirect bound (out 200 ≥ bound 117).
        let accounts = [(1230usize, 1230usize, 1230usize), (200, 500, 117)];
        assert!(percap_store_full_guarded(&accounts));
        // Borrow edge: slow can lend to the cap-full fast borrower
        // (T_return(fast) ≈ 0.126 s, reservation 253, room = 500 − 200 −
        // 253 = 47 > 0) → admission stays open.
        let fast = BorrowAccount {
            path: 0,
            out: 1230,
            cap: 1230,
            fly: 1230,
            rate: Some(10_400.0),
            rtprop_s: Some(0.008),
        };
        let slow = BorrowAccount {
            path: 1,
            out: 200,
            cap: 500,
            fly: 200,
            rate: Some(2_000.0),
            rtprop_s: Some(0.060),
        };
        assert!(percap_lend_edge_exists(&[fast, slow]));
        // The edge closes when the lender's slack is inside its
        // reservation (out 300: room = 500 − 300 − 253 < 0) — the gate
        // then reads FULL exactly like the no-borrow arm: backpressure.
        let slow_reserved = BorrowAccount {
            out: 300,
            fly: 300,
            ..slow
        };
        assert!(!percap_lend_edge_exists(&[fast, slow_reserved]));
    }

    /// feat/gen-substrate-ceiling: the derived pipeline depth M* =
    /// ceil(rate·2·SRTT/G)+1 — #61's A* = clamp(D·rate, 1, W) quantized to
    /// generations — covers BDP + one deficit round, clamps to the legacy 2 on
    /// cold start, and to GEN_PIPE_MAX_GENS at the top.
    #[test]
    fn gen_pipe_depth_covers_bdp_plus_one_deficit_round() {
        // Cold start (no rate / no srtt sample) → the legacy fixed depth 2.
        assert_eq!(gen_pipe_depth(0.0, 0.016, 384), 2);
        assert_eq!(gen_pipe_depth(1500.0, 0.0, 384), 2);
        // c2-class: rate 1500 sym/s, SRTT 16 ms → D·rate = 48 sym ≪ G ⇒
        // ceil(48/384)+1 = 2 — a small-BDP link needs no extra depth.
        assert_eq!(gen_pipe_depth(1500.0, 0.016, 384), 2);
        // Link-class c2: rate 10 000 sym/s, SRTT 40 ms (queue/jitter-inflated)
        // → D·rate = 800 ⇒ ceil(800/384)+1 = 4 generations in flight.
        assert_eq!(gen_pipe_depth(10_000.0, 0.040, 384), 4);
        // High-BDP (RTT200 @ 100 Mbit): 10 000 sym/s × 0.4 s = 4000 sym ⇒
        // ceil(4000/384)+1 = 12.
        assert_eq!(gen_pipe_depth(10_000.0, 0.200, 384), 12);
        // Monotone in rate·srtt, and hard-capped at GEN_PIPE_MAX_GENS.
        assert_eq!(gen_pipe_depth(1e9, 1.0, 384), GEN_PIPE_MAX_GENS);
    }

    // feat/anchor-hygiene (`RWM_PLAIN_RS`): the sampling-only feed must
    // declare that it does NOT own the CC operating point — everything the
    // Copa-sole feed switches (store-cap law, percap pipes, cwnd-dynamics
    // call site, pass-through window writes) keys on `owns_cc()`.
    #[test]
    fn sampling_only_feed_does_not_own_cc() {
        assert!(CopaFeed::new().owns_cc());
        assert!(!CopaFeed::new_sampling_only(true).owns_cc());
    }

    /// Per-path BDP in-flight cap (the #64 fix, gen_pipe remedy 1). The sender
    /// is "full" only when NO path is below its OWN cap (gain·BtlBw_i·RTprop_i).
    /// The slow path's RTT-inflated cap bounds only the slow path; the fast path
    /// with room keeps the pipe moving — unlike the summed-anchor #64 global
    /// budget the fast path stalled behind.
    #[test]
    fn infl_percap_bounds_each_path_independently() {
        // Fast path (cap 100) has room at 40; slow path (RTT-inflated cap 60) is
        // at its cap. NOT full — the fast path keeps pulling source.
        assert!(
            !infl_percap_full(&[(40, 100), (60, 60)]),
            "fast path with room ⇒ not full even when the slow path is at its cap"
        );
        // Every path at/above its own cap ⇒ full (total in-flight ≈ Σ per-path BDP).
        assert!(
            infl_percap_full(&[(100, 100), (60, 60)]),
            "all paths at their per-path cap ⇒ full"
        );
        assert!(
            infl_percap_full(&[(120, 100), (80, 60)]),
            "all paths over their per-path cap ⇒ full"
        );
        // Degenerate zero-cap path never blocks (cap.max(1)); a fresh path with
        // room keeps the sender open.
        assert!(!infl_percap_full(&[(0, 0), (10, 100)]));
        // Single fast path with room ⇒ not full (single-path parity control).
        assert!(!infl_percap_full(&[(50, 145)]));
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

    // ----- SACK-clocked store release (RWM_STORE_SACK_RELEASE) --------------
    // Pre-registered invariants (goal-gate "SACK-Clocked Store Release"):
    // SACKed → released → retransmit-still-possible → cumulative-ack →
    // fully freed; window opens on SACK; no double-release; released slots
    // return to the pool; released seqs keep their per-flight loss clocks.

    #[test]
    fn test_sack_release_every_unacked_symbol_stays_recoverable() {
        // The chain the RWM_SACK_PRUNE refutation forbids breaking: a
        // SACKed symbol leaves the OUTSTANDING COUNT but its payload and
        // ARQ state survive until the cumulative frontier passes it.
        use crate::fec::{RlcWindowEncoder, WindowEncoder, WireSymbol};
        let n = 100u64;
        let mut encoder = RlcWindowEncoder::new(64);
        let mut sent_store: BTreeMap<u64, WireSymbol> = BTreeMap::new();
        let mut released: BTreeSet<u64> = BTreeSet::new();
        for i in 0..n {
            let sym = encoder.add_source(&vec![i as u8; 32]);
            sent_store.insert(sym.block_id, sym);
        }
        // Frontier at 9; hole at 10; receiver SACKed 11..=99.
        let ack = 9u64;
        sent_store = sent_store.split_off(&(ack + 1));
        sack_release_prune(&mut released, ack);
        let newly = sack_release_mark(&sent_store, &mut released, 11, 99);
        assert_eq!(newly.len(), 89, "11..=99 newly released");
        // RELEASED, not removed: outstanding drops to the hole + frontier
        // successor set, but EVERY entry is still in the store.
        assert_eq!(sent_store.len(), 90, "nothing was removed from the store");
        assert_eq!(sack_release_outstanding(sent_store.len(), released.len()), 1);
        // Retransmit still possible for a released symbol (the NACK path
        // serves from sent_store.get — e.g. after a receiver eviction).
        let held = sent_store.get(&50).expect("released symbol still retransmittable");
        assert_eq!(&held.data[..32], &[50u8; 32]);
        // The hole itself was never SACKed → still counted.
        assert!(!released.contains(&10));
        // Cumulative frontier passes everything → fully freed, both maps.
        let ack2 = 99u64;
        sent_store = sent_store.split_off(&(ack2 + 1));
        sack_release_prune(&mut released, ack2);
        assert!(sent_store.is_empty());
        assert!(released.is_empty(), "released marks freed with the store (subset invariant)");
    }

    #[test]
    fn test_sack_release_opens_window_and_returns_slots_to_pool() {
        // The mechanism: outstanding = retained − released re-opens the
        // flow-control gate (RWM_STORE_PATHS' pooled cap composes through
        // the same count) while a hole holds the cumulative frontier.
        let mut sent_store: BTreeMap<u64, u8> = BTreeMap::new();
        let mut released: BTreeSet<u64> = BTreeSet::new();
        for i in 0..1024u64 {
            sent_store.insert(i, 0);
        }
        let cap = 1024usize; // RELIABLE_STORE_MAX-class pooled cap
        // Store pegged at cap across a frontier stall: gate closed.
        assert!(sack_release_outstanding(sent_store.len(), released.len()) >= cap);
        // Hole at 0; receiver SACKs 1..=1023 → slots return to the pool.
        let newly = sack_release_mark(&sent_store, &mut released, 1, 1023);
        assert_eq!(newly.len(), 1023);
        let outstanding = sack_release_outstanding(sent_store.len(), released.len());
        assert_eq!(outstanding, 1, "window opens: only the hole still counts");
        assert!(outstanding < cap, "gate re-opens while the frontier is frozen");
        // The frontier is NOT advanced — retention intact (reliability).
        assert_eq!(sent_store.len(), 1024);
    }

    #[test]
    fn test_sack_release_no_double_release_and_percap_composes() {
        // A re-advertised SACK snapshot (gap reports are state snapshots,
        // re-sent every cadence) must not double-release: neither the
        // released set nor the per-path accounts move twice.
        let mut sent_store: BTreeMap<u64, u8> = BTreeMap::new();
        let mut released: BTreeSet<u64> = BTreeSet::new();
        let mut acct: BTreeMap<u64, u32> = BTreeMap::new();
        let mut out: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
        for i in 0..10u64 {
            sent_store.insert(i, 0);
            percap_charge(&mut acct, &mut out, i, (i % 2) as u32);
        }
        assert_eq!(out[&0], 5);
        assert_eq!(out[&1], 5);
        // First snapshot: release 2..=5 (accounts freed on the newly list —
        // the sender-loop arm's exact arithmetic).
        let newly = sack_release_mark(&sent_store, &mut released, 2, 5);
        assert_eq!(newly, vec![2, 3, 4, 5]);
        for &k in &newly {
            percap_release_seq(&mut acct, &mut out, k);
        }
        assert_eq!(out[&0], 3);
        assert_eq!(out[&1], 3);
        // Same snapshot again (plus overlap): NOTHING newly released.
        let again = sack_release_mark(&mut sent_store, &mut released, 2, 5);
        assert!(again.is_empty(), "idempotent under re-advertised snapshots");
        assert_eq!(released.len(), 4);
        // Cumulative release later cannot double-free the accounts either
        // (percap_release_cumulative finds SACK-released seqs already gone).
        percap_release_cumulative(&mut acct, &mut out, 9);
        assert_eq!(out[&0], 0);
        assert_eq!(out[&1], 0);
    }

    #[test]
    fn test_sack_release_keeps_arq_state_and_flight_clocks() {
        // RWM_RECOV_MP interaction: releasing a slot must not touch the
        // per-flight state — nack_retx_at (the live-flight clock the
        // per-path law times), retransmit_buffer (tail-sweep metadata),
        // source_path_map (seq→path evidence for the packet-threshold
        // channel). The release law takes NONE of them as inputs; this
        // pins the contract the prune arm violates.
        let mut sent_store: BTreeMap<u64, u8> = BTreeMap::new();
        let mut released: BTreeSet<u64> = BTreeSet::new();
        let mut nack_retx_at: std::collections::HashMap<u64, (u64, u32)> =
            std::collections::HashMap::new();
        let mut retransmit_buffer: BTreeMap<u64, (u64, f64, u32)> = BTreeMap::new();
        let mut source_path_map: BTreeMap<u64, u32> = BTreeMap::new();
        for i in 0..20u64 {
            sent_store.insert(i, 0);
            nack_retx_at.insert(i, (1_000 + i, (i % 2) as u32));
            retransmit_buffer.insert(i, (2_000 + i, 0.01, (i % 2) as u32));
            source_path_map.insert(i, (i % 2) as u32);
        }
        let newly = sack_release_mark(&sent_store, &mut released, 5, 19);
        assert_eq!(newly.len(), 15);
        // Released seqs keep their flight clocks + ARQ metadata intact.
        assert_eq!(nack_retx_at.len(), 20);
        assert_eq!(nack_retx_at[&10], (1_010, 0));
        assert_eq!(retransmit_buffer.len(), 20);
        assert_eq!(source_path_map.len(), 20);
        // And the store itself (the payload copy) is untouched.
        assert_eq!(sent_store.len(), 20);
    }

    #[test]
    fn test_sack_release_mark_skips_seqs_not_retained() {
        // Ranges can race the cumulative frontier (the atomic may already
        // be ahead): seqs no longer in the store are never marked, so the
        // released set stays a subset of sent_store keys.
        let mut sent_store: BTreeMap<u64, u8> = BTreeMap::new();
        let mut released: BTreeSet<u64> = BTreeSet::new();
        for i in 50..60u64 {
            sent_store.insert(i, 0);
        }
        let newly = sack_release_mark(&sent_store, &mut released, 0, 100);
        assert_eq!(newly.len(), 10, "only retained seqs are markable");
        assert!(released.iter().all(|k| sent_store.contains_key(k)));
        assert_eq!(sack_release_outstanding(sent_store.len(), released.len()), 0);
    }

    /// SACK + BDP reassembly end-to-end reliability invariant
    /// (feat/sack-bdp-reassembly): the sender advances PAST a hole (SACK-prunes
    /// every out-of-order-received symbol from its store), the receiver HOLDS the
    /// out-of-order symbols in its non-evicting reassembly (bounded by the BDP
    /// the sender's outstanding cap enforces), the receiver prune never evicts a
    /// received-but-undelivered symbol, the hole recovers by retransmit from the
    /// sender's retained store, and EVERY byte is delivered in order. This is the
    /// exact invariant that the prior SACK attempt (#52) violated by evicting a
    /// pruned-but-unconsumed symbol at the receiver.
    #[test]
    fn test_sack_bdp_reassembly_delivers_every_byte_past_a_hole() {
        use crate::fec::{RlcWindowEncoder, WindowEncoder, WireSymbol};
        // ---- SENDER: send 300 source symbols, all retained in the store. ----
        let n = 300u64;
        let mut encoder = RlcWindowEncoder::new(64);
        let mut sent_store: BTreeMap<u64, WireSymbol> = BTreeMap::new();
        for i in 0..n {
            let sym = encoder.add_source(&vec![(i % 251) as u8; 32]);
            sent_store.insert(sym.block_id, sym);
        }

        // ---- RECEIVER: reliable, non-evicting reassembly (the BDP buffer). ----
        let mut reorder = ReorderBuffer::new_reliable();
        let mut received_seqs: BTreeSet<u64> = BTreeSet::new();
        let mut highest_delivered_seq: u64 = 0; // -1 sentinel via next_deliver_seq
        let mut highest_seen_seq: u64 = 0;
        let recv_win_cap: u64 = MAX_WINDOW_SIZE as u64;
        let mut delivered: Vec<u64> = Vec::new();

        // The receiver gets seq 0..=9 in order, then a HOLE at seq 10, then
        // EVERYTHING above (11..=299) out of order. Deliver in wire arrival order.
        let hole = 10u64;
        let arrival: Vec<u64> = (0..=9).chain(11..n).collect();
        for &seq in &arrival {
            received_seqs.insert(seq);
            highest_seen_seq = highest_seen_seq.max(seq);
            for (dseq, _) in reorder.push(seq, Bytes::from(vec![(seq % 251) as u8; 32])) {
                delivered.push(dseq);
                highest_delivered_seq = highest_delivered_seq.max(dseq);
            }
            // The receiver periodically prunes its decoder/received-seq state.
            // INVARIANT (RWM_REASM_BDP clamp): prune_before never exceeds the
            // DELIVERED frontier, so no received-above-hole symbol is evicted.
            let prune_before = highest_delivered_seq
                .saturating_sub(recv_win_cap * 2)
                .min(highest_delivered_seq);
            received_seqs = received_seqs.split_off(&prune_before);
        }

        // Delivery is frozen at the hole: only 0..=9 delivered so far.
        assert_eq!(delivered, (0..=9).collect::<Vec<_>>(), "in-order stalls at the hole");
        // 11..=299 are HELD (received but not delivered) — the reassembly holds
        // them all, none evicted (non-evicting reliable buffer + clamped prune).
        assert_eq!(reorder.pending_count(), (n - 11) as usize, "all OOO symbols held");
        for seq in 11..n {
            assert!(received_seqs.contains(&seq), "seq {seq} must survive prune until delivered");
        }

        // ---- SENDER: SACK-prune the received (out-of-order) symbols. ----
        // The cumulative ack is 9; everything 11..=299 was SACKed.
        let ack = 9u64;
        sent_store = sent_store.split_off(&(ack + 1));
        for (start, end) in received_sack_ranges(&received_seqs, ack, highest_seen_seq) {
            let acked: Vec<u64> = sent_store.range(start..=end).map(|(&k, _)| k).collect();
            for k in acked {
                sent_store.remove(&k);
            }
        }
        // Only the hole stays retained — the sender is free to inject fresh source.
        assert_eq!(sent_store.len(), 1, "sender retains ONLY the unfilled hole");
        assert!(sent_store.contains_key(&hole), "the hole survives for ARQ retransmit");

        // ---- RECOVERY: the hole is retransmitted from the retained store. ----
        let hole_sym = sent_store.get(&hole).expect("hole retained").clone();
        for (dseq, _) in reorder.push(hole, Bytes::copy_from_slice(&hole_sym.data[..32])) {
            delivered.push(dseq);
            highest_delivered_seq = highest_delivered_seq.max(dseq);
        }

        // EVERY byte delivered, in order, exactly once — the reliability invariant.
        assert_eq!(delivered, (0..n).collect::<Vec<_>>(), "every symbol delivered in order");
        assert_eq!(reorder.pending_count(), 0, "reassembly fully drained — nothing stranded");
    }

    /// ADR-0046 idle-triggered recovery (Phase 4 fix). The congestion
    /// multiplier may fully suppress NACK repairs (correct on a congested
    /// straggler), but must NEVER stay suppressed when the sender is idle
    /// except for a confirmed hole — that wedges a reliable transfer.
    #[test]
    fn test_idle_triggered_recovery_floor() {
        let mut st = NackCongestionState::new();
        // Drive congestion: both loss AND RTT rising for >= threshold periods.
        let mut rtt = Duration::from_millis(20);
        let mut loss = 0.02;
        for _ in 0..8 {
            loss += 0.02;
            rtt += Duration::from_millis(5);
            st.update(loss, Some(rtt));
        }
        // Multiplier has collapsed toward 0 (full suppression).
        assert!(st.multiplier() < 0.05, "congestion must suppress: {}", st.multiplier());

        // ACTIVE sender: suppression stands — congestion safety wins, so a
        // retransmit would NOT be forced onto the straggler.
        let active = st.effective_multiplier(false);
        assert_eq!(active, st.multiplier(), "active transfer keeps raw multiplier");
        assert_eq!((MAX_NACK_REPAIRS_PER_NACK as f64 * active).round() as u64, 0,
            "active + suppressed => 0 forced repairs");

        // IDLE sender (no new source in flight): recovery is never fully
        // suppressed — the floor yields >= 1 targeted retransmit per round so
        // the confirmed hole is recovered and the transfer un-wedges.
        let idle = st.effective_multiplier(true);
        assert!(idle >= IDLE_RECOVERY_FLOOR, "idle floor lifts the multiplier: {idle}");
        assert!((MAX_NACK_REPAIRS_PER_NACK as f64 * idle).round() as u64 >= 1,
            "idle floor must permit >= 1 retransmit/round");

        // Continuity: on a clean/uncongested channel the idle floor is a NO-OP
        // (raw multiplier already >= floor), so behavior is unchanged.
        let mut clean = NackCongestionState::new();
        for _ in 0..5 { clean.update(0.0, Some(Duration::from_millis(20))); }
        assert_eq!(clean.effective_multiplier(true), clean.multiplier(),
            "idle floor is a no-op when not suppressed");
        assert!((clean.multiplier() - 1.0).abs() < 1e-9);
    }

}
