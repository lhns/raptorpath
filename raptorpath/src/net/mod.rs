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

/// FMTCP total-in-flight flow-control gate (docs/research/fmtcp-retry-design.md,
/// change 1 — the crux). The shipped generation gate pauses TUN intake when the
/// retained-source store (`encoder.window_size()`, measured back to the IN-ORDER
/// decode frontier) fills — so a hole freezes the frontier, the store fills to
/// the cap, and the sender IDLES behind the hole (the oracle's in-order-frontier
/// stall, PART 5: 4394 idle sender slots). The FMTCP gate instead pauses ONLY on
/// the per-path BDP in-flight (`cwnd_full`, which drains on the RTT timescale via
/// `expire_in_flight` and is decode-order-INDEPENDENT), so a frozen frontier
/// never stalls intake. `store_len >= mem_ceiling` (ooo_gens·G ≫ BDP) is a LOOSE
/// memory backstop that binds only if recovery genuinely stalls. Extracted pure
/// so the "a frozen in-order frontier does not pause intake" invariant is
/// unit-tested without driving the async sender loop.
fn fmtcp_tx_paused(cwnd_full: bool, store_len: usize, mem_ceiling: usize) -> bool {
    cwnd_full || store_len >= mem_ceiling
}

/// FMTCP per-path in-flight cap decision (change 2, the #64 fix). Given each
/// active path's `(in_flight, per_path_cap)` where the cap = gain·BtlBw_i·RTprop_i
/// (that path's OWN windowed-max bandwidth × its OWN min-RTT), the sender is
/// "full" only when NO path is below its own cap. So the slow path's RTT-inflated
/// cap bounds ONLY the slow path, and the fast path keeps pulling source while the
/// slow path is full. The summed-anchor #64 bug was a single GLOBAL budget
/// gain·Σ_i BtlBw_i·RTprop_i that the fast path stalled behind (and that let the
/// slow path's inflated term over-drive its own queue into bufferbloat). Extracted
/// pure for unit testing.
fn fmtcp_percap_full(per_path: &[(u64, u64)]) -> bool {
    !per_path.iter().any(|&(in_flight, cap)| in_flight < cap.max(1))
}

/// Per-path pace-gate decision (feat/pace-all-traffic). Given a candidate path
/// for a repair symbol, the fast (min-RTprop) path, and the per-path BtlBw pace
/// token buckets (`daps_pace_tok`, refilled at BtlBw_i), decide where — if
/// anywhere — the symbol may be emitted, so that TOTAL per-path emission
/// (source + repair, both charged against the SAME buckets) never exceeds
/// BtlBw_i on ANY path. Returns:
///   * `Some(candidate)` — the candidate's bucket ≥ 1: emit there, one token
///     consumed;
///   * `Some(fast)` — candidate dry but the fast path has a token: spill so the
///     slow path never over-queues;
///   * `None` — BOTH the candidate and the fast path are dry: HOLD (the caller
///     retries next loop as the buckets refill). This is what bounds the FAST
///     path too — source has priority, repair uses only the leftover per-path
///     capacity, so neither path is driven above BtlBw_i.
/// A path with no warmed bucket (anchor not established) is transparent — it
/// emits on the candidate and consumes nothing (mirrors the source pace gate).
/// Extracted pure so the "total per-path emission ≤ BtlBw_i incl. repair"
/// invariant is unit-tested without driving the async sender loop.
fn paced_repair_decision(
    tok: &mut std::collections::HashMap<crate::scheduler::PathId, f64>,
    cand: crate::scheduler::PathId,
    fast: crate::scheduler::PathId,
) -> Option<crate::scheduler::PathId> {
    let mut p = cand;
    if p != fast && tok.get(&p).is_some_and(|&t| t < 1.0) {
        p = fast;
    }
    if tok.get(&p).is_some_and(|&t| t < 1.0) {
        return None;
    }
    if let Some(t) = tok.get_mut(&p) {
        *t -= 1.0;
    }
    Some(p)
}

/// Per-path pace-gate ADMISSION peek for SOURCE (feat/source-backpressure).
/// SOURCE is payload — it cannot be dropped like a rateless repair symbol (a
/// dropped repair costs nothing, retried on refill), so the repair HOLD of
/// `paced_repair_decision` becomes DEFER (backpressure) here: when neither the
/// DAPS-chosen candidate path NOR the fast (spill) path has a funded per-path
/// BtlBw bucket, the caller must NOT read the next source from the TUN — it
/// lets the app / QUIC send-buffer backpressure — rather than spilling onto the
/// fast path and driving its bucket NEGATIVE (an unmetered burst that becomes
/// the standing fast-path queue).  Returns whether a source symbol can be
/// emitted on SOME path without overdrawing any bucket:
///   * candidate funded (bucket ≥ 1) → admit (it will emit there);
///   * candidate dry but the fast path funded → admit (it spills to fast);
///   * BOTH dry → DEFER (do not admit; the buckets refill at BtlBw_i and the
///     next poll re-checks).  This is the SOURCE analogue of the repair HOLD,
///     making TOTAL per-path emission (source + repair) ≤ BtlBw_i on EVERY path.
/// A path with no warmed bucket (anchor not established) is transparent — it
/// admits (the source pacer decrements nothing there, mirroring the emit path).
/// Pure so the "source deferred, not spilled, when the bucket is dry" invariant
/// is unit-tested without driving the async sender loop.  Read-only (a peek):
/// the actual token is consumed at emission by the source-placement gate.
fn source_pace_admit(
    tok: &std::collections::HashMap<crate::scheduler::PathId, f64>,
    cand: crate::scheduler::PathId,
    fast: crate::scheduler::PathId,
) -> bool {
    // Candidate funded (or unwarmed → transparent)?  Emit on the candidate.
    if tok.get(&cand).map_or(true, |&t| t >= 1.0) {
        return true;
    }
    // Candidate dry: source spills to the fast path — admit only if the fast
    // bucket is funded (or unwarmed).  Both dry → DEFER (backpressure).
    tok.get(&fast).map_or(true, |&t| t >= 1.0)
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
    // FMTCP-class pure decode-on-total aggregation (docs/research/fmtcp-retry-design.md).
    // A single composite env-gate that turns on the oracle-confirmed pure config —
    // total-in-flight flow control + fungible fountain redundancy (NO per-hole ARQ)
    // + decode-on-total out-of-order — on TOP of the reliable window. It SELECTS the
    // systematic-repair generation submode (raw source rides the wire out-of-order,
    // ceil(len·r) fungible repair, stable per-generation anchor) so RWM_FMTCP=1 with
    // --window-reliable is self-contained; the individual sub-levers (xpath repair,
    // OOO retention decouple, per-path BDP in-flight cap, once-per-RTT deficit,
    // receiver reassembly clamp) are forced on in run_window_sender / the receiver.
    // Shipped path is byte-untouched (default config has window_reliable off).
    let fmtcp = crate::config::env_flag("RWM_FMTCP", false);
    let window_systematic = window_reliable && (config.window_systematic_repair || fmtcp);
    let window_generation = window_reliable
        && (config.window_generation_coding || config.window_systematic_repair || fmtcp);
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
    // FMTCP change 1: the peer's TOTAL DECODED count `d` (decoded source symbols
    // across ALL generations, out of order) — distinct from window_ack_seq (the
    // in-order frontier `df`). handle_control_message publishes it from each
    // WindowAck's `cumulative_received` (which the OOO receiver sets to
    // received_seqs.len()); the FMTCP sender gates outstanding = sent_src − d,
    // so a hole that freezes df never stalls intake (total-in-flight FC).
    let window_decoded_seq = Arc::new(AtomicU64::new(0));

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
    let sender_window_decoded = window_decoded_seq.clone();
    let mut sender_nack_rx = nack_rx;
    let mut sender_deficit_rx = deficit_rx;
    let mut sender_sack_rx = sack_rx;
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
                &sender_window_decoded,
                &mut sender_nack_rx,
                &mut sender_deficit_rx,
                &mut sender_sack_rx,
                &mut sender_shutdown_rx,
                sender_protocol_hint,
                sender_window_reliable,
                sender_window_coded_only,
                sender_window_generation,
                sender_window_systematic,
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
        let g = std::env::var("RWM_GEN").ok().and_then(|s| s.parse::<usize>().ok()).unwrap_or(384);
        let m = std::env::var("RWM_PIPELINE").ok().and_then(|s| s.parse::<usize>().ok()).unwrap_or(2);
        ((g.max(1) * (m.max(1) + 1)).max(MAX_WINDOW_SIZE)).min(1 << 20) as u64
    } else if window_coded_only {
        std::env::var("RWM_WINDOW")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(640)
            .clamp(MAX_WINDOW_SIZE, 4096) as u64
    } else {
        MAX_WINDOW_SIZE as u64
    };
    let recv_window_ack = window_ack_seq.clone();
    let recv_window_decoded = window_decoded_seq.clone();
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
    // SACK flow-control producer (feat/sack-flow-control), GATED OFF by default.
    //
    // Rationale / MEASURED finding (L1, 2026-07-07): decoupling the sender's
    // flow control from the in-order cumulative-ack frontier by pruning the
    // sent-store for out-of-order-received (SACKed) symbols does NOT lift lossy
    // single-path throughput (c2 single 16.09 vs 16.07 baseline — the limiter is
    // the receiver-side in-order RECOVERY LATENCY, not sender store
    // backpressure) and is UNSAFE for in-order delivery: with the sender no
    // longer held near the frontier it races the whole object ahead, but the
    // receiver's in-order reassembly window is BOUNDED (MAX_WINDOW_SIZE), so a
    // symbol can be received (→ SACKed → pruned here) and then EVICTED at the
    // receiver before the in-order frontier consumes it — destroying the only
    // retained copy and wedging completion (MEASURED: C7/C8 in-order dual DNF;
    // the OOO-completion arms, which are not frontier-bound, complete). The
    // frontier-coupled backpressure this would remove is precisely what keeps
    // the send frontier inside the receiver's reassembly window. Kept as an
    // env-gated experiment (RWM_SACK_PRUNE=1); default is byte-for-byte base.
    // Only plain-reliable has a per-seq sent-store to prune.
    let sack_prune_enabled = crate::config::env_flag("RWM_SACK_PRUNE", false);
    let recv_sack_tx: Option<tokio::sync::mpsc::Sender<Vec<(u64, u64)>>> =
        if sack_prune_enabled && window_reliable && !window_generation && !window_coded_only {
            Some(sack_tx)
        } else {
            None
        };
    // SACK + BDP reassembly (feat/sack-bdp-reassembly) — the composed root-cause
    // attack on the in-order cumulative-ack frontier serialization. RWM_SACK_PRUNE
    // (above) decouples the SENDER from the frozen frontier (prune on any OOO ack);
    // RWM_REASM_BDP hardens the RECEIVER so that decoupling is SAFE for reliable
    // in-order delivery. The RELIABILITY INVARIANT it guarantees: a received symbol
    // is NEVER evicted from the receiver's reassembly state before it is delivered
    // (its in-order frontier passes), so a symbol the sender has SACK-pruned always
    // survives at the receiver until use → no un-recoverable eviction. Concretely
    // it (a) clamps the window-decoder/received-seq prune so it can never advance
    // ABOVE the delivered frontier (the reorder buffer is already usize::MAX / non-
    // evicting), and (b) probes the reassembly occupancy so the bound can be
    // reported (`[REASM]`). The reassembly stays BDP-bounded because the sender's
    // outstanding is bounded (plain_dyn_cap = gain·BDP store cap, default-on) and
    // working FEC recovers holes fast. Default-off; the shipped path is untouched.
    // FMTCP forces the receiver reassembly clamp on: decode-on-total delivers
    // out of order and the sender runs far past the in-order frontier, so the
    // receiver must (a) never evict an above-frontier symbol before it is
    // delivered (reliability invariant) and (b) probe the reassembly occupancy
    // (stays ≈ aggregate BDP because the total-in-flight FC bounds the sender).
    let reasm_bdp_on = crate::config::env_flag("RWM_REASM_BDP", false) || fmtcp;

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
        let recv_gen_size: u64 = std::env::var("RWM_GEN")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(384)
            .max(1);
        // Receiver-tail parallelization (PART 1). Number of outstanding
        // generations whose deficit is reported (and anti-wedge-seeded) per
        // round. Legacy = 6 (frontier-first serial tail); env RWM_REPORT_GENS
        // lifts it to cover the whole in-flight range so EVERY hole is repaired
        // in ONE round-trip (parallel tail flush). Unset = byte-identical
        // shipped path. Clamped to the wire cap (MAX_ACK_IDS = 2000).
        let report_gens: usize = std::env::var("RWM_REPORT_GENS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(6)
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
        let repair_wait_base: Duration = std::env::var("RWM_REPAIR_WAIT")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
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
        let fdiag_on = crate::config::env_flag("RWM_FDIAG", false);
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
                        if crate::config::env_flag("RWM_TRACE", false) {
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
                            let recovered = if fdiag_on {
                                let t_dec = Instant::now();
                                let r = win_dec.add_symbol(symbol);
                                fdiag_addsym_us += t_dec.elapsed().as_micros() as u64;
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
                                    "[FDIAG] frontier={} seen={} gap={} probe_holes={} probe_buffered={} | DECODE n={} avg={}us present_at_stall={} | SOURCE n={} avg={}us | COMPUTE calls={} avg={}us total={}ms | rf={} ru={}",
                                    f, highest_seen_seq,
                                    highest_seen_seq.saturating_sub(f),
                                    holes, buffered,
                                    fdiag_decode_n, dec_avg, fdiag_present_at_stall,
                                    fdiag_source_n, src_avg,
                                    fdiag_addsym_n, addsym_avg, fdiag_addsym_us / 1000,
                                    win_dec.repairs_fed(), win_dec.repairs_useful(),
                                );
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
                                // FMTCP change 1 (total decode progress). In OOO /
                                // generation mode carry the TOTAL count of decoded
                                // source symbols across ALL generations (out of
                                // order), so the sender can gate outstanding on
                                // total decode progress `d` — NOT the contiguous
                                // in-order frontier `received_up_to` that a hole
                                // freezes. received_seqs holds every delivered seq
                                // (decode-on-total), so its length IS d. Legacy
                                // (in-order) modes keep the per-path received count.
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
                        if recv_window_mode { Some(&recv_window_decoded) } else { None },
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

/// Build one proactive-frontier repair (RLC over `[start, start+count)`) from
/// the retain-until-acked `sent_store` rather than the encoder's coding window.
/// The coding window ages a frontier hole out once the outstanding gap exceeds
/// `win_cap` (MEASURED: gap→494 > win_cap 200 at C2), but `sent_store` retains
/// EVERY un-acked source, so the frontier window is always fully covered here.
/// Wire format is byte-identical to `RlcWindowEncoder::generate_repair`, so the
/// receiver's `RlcWindowDecoder` handles it with no special case. Returns `None`
/// if any source in the range is missing from the store (already acked/evicted).
fn build_frontier_repair(
    sent_store: &std::collections::BTreeMap<u64, crate::fec::WireSymbol>,
    start: u64,
    count: u16,
    symbol_size: u16,
    repair_index: u32,
) -> Option<crate::fec::WireSymbol> {
    if count == 0 {
        return None;
    }
    let ss = symbol_size as usize;
    let coeffs =
        crate::fec::gf256::generate_window_coefficients(start, count, repair_index);
    let mut coded = vec![0u8; ss];
    for i in 0..count as u64 {
        let seq = start + i;
        let src = sent_store.get(&seq)?; // missing ⇒ inconsistent equation
        if src.data.len() == ss {
            crate::fec::gf256::mul_acc_slice(coeffs[i as usize], &src.data, &mut coded);
        } else {
            let mut padded = vec![0u8; ss];
            let n = src.data.len().min(ss);
            padded[..n].copy_from_slice(&src.data[..n]);
            crate::fec::gf256::mul_acc_slice(coeffs[i as usize], &padded, &mut coded);
        }
    }
    let mut wire = Vec::with_capacity(14 + ss);
    wire.extend_from_slice(&start.to_le_bytes());
    wire.extend_from_slice(&count.to_le_bytes());
    wire.extend_from_slice(&repair_index.to_le_bytes());
    wire.extend_from_slice(&coded);
    Some(crate::fec::WireSymbol {
        block_id: start + count as u64 - 1,
        payload_id: repair_index,
        is_repair: true,
        data: wire,
        backend: FecBackend::Rlc,
    })
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
    // FMTCP change 1: the peer's TOTAL DECODED count `d` (out-of-order,
    // across all generations), published by handle_control_message from each
    // WindowAck's `cumulative_received`. The FMTCP flow-control gate keys on
    // sent_src − d (total decode progress), NOT the in-order frontier.
    window_decoded_seq: &Arc<AtomicU64>,
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
) {
    // Generation coding emits coded wire symbols exactly like coded-only; the
    // difference is the coding UNIT (a stable generation vs the moving window)
    // and that per-seq ARQ is disabled below.
    let coded_wire = coded_only || generation;
    let gen_size: usize = std::env::var("RWM_GEN")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(384)
        .max(1);
    let pipeline: usize = std::env::var("RWM_PIPELINE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(2)
        .max(1);
    // Generation-coding proactive overhead r (coded per generation beyond K_G):
    // the encoder provisions each generation to ceil(len·(1+r)) coded before it
    // is only coded for recovery. Covers loss + the MDS margin. RWM_GEN_R env.
    // Systematic-repair provisions only the loss-FEC overhead r (the K base DoF
    // ride the wire as source), so its natural default is smaller than
    // coded-only's (which must also fund the K base). r ≳ 1.5·ε keeps windowed
    // repair ahead of loss (the oracle's provisioning floor; r < ε → DNF). At C8
    // ε_slow ≈ 4.8 %, so 0.15 clears both paths with margin. RWM_GEN_R overrides.
    // FMTCP-class composite gate (docs/research/fmtcp-retry-design.md). When set
    // (and in generation mode) it forces the oracle-confirmed pure config: OOO
    // retention decouple, fungible cross-path repair, per-path BDP in-flight cap,
    // once-per-RTT deficit coalesce, and — the crux — TOTAL-in-flight flow control
    // (the tx_paused gate keys on the per-path BDP in-flight, NOT the in-order
    // frontier store). Sub-levers below OR `fmtcp` into their own env gates.
    // DAPS delay-aware scheduling (RWM_DAPS): the slow path carries FUTURE
    // stream data offset by the per-path latency skew so it arrives IN SYNC with
    // the fast path reaching that position (G. Sarwar, R. Boreli, E. Lochin,
    // A. Mifdaoui, G. Smith, WAINA/PAMS 2013; N. Kuhn et al., IEEE ICC 2014),
    // with the ECF completion-time guard (Y. Lim, E. Nahum, D. Towsley,
    // R. Gibbens, ACM CoNEXT 2017).  It REUSES the FMTCP total-in-flight FC +
    // per-path BDP cap + decode-on-total base, so RWM_DAPS implies that base.
    let daps = crate::config::env_flag("RWM_DAPS", false) && generation;
    let fmtcp = (crate::config::env_flag("RWM_FMTCP", false) || daps) && generation;
    // FMTCP win backstop: bound the send frontier to (pipeline+2) generations
    // past the in-order frontier (anti-bufferbloat; RWM_FMTCP_WIN overrides).
    // DAPS deepens it to a "read-ahead" ≥ max latency skew + recovery slack so
    // the slow path always has FUTURE data to carry (the deep app-side read-
    // ahead + deep receiver reassembly the delay-alignment requires).
    let daps_win_floor = if daps { (pipeline + 6) * gen_size } else { 0 };
    let fmtcp_win_backstop: usize = std::env::var("RWM_FMTCP_WIN")
        .ok().and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(((pipeline + 2) * gen_size).max(daps_win_floor))
        .max(2 * gen_size);
    // DAPS QUEUE MANAGEMENT (feat/daps-queue-mgmt).  DAPS removed the frontier
    // stall but the slow path then BUFFERBLOATED to ~834 ms: the FMTCP per-path
    // BDP cap only gated the aggregate TUN-read PAUSE (the sender paused only
    // when EVERY path was full), so the softmax kept committing a share to the
    // slow path PAST its BDP.  Two bounds, both DAPS-gated, reclaim the slack:
    //  (1) BLEST per-path PLACEMENT cap (`place_source_daps_capped`): a path at
    //      its own BDP is dropped from the eligible set, so slow-path OUTSTANDING
    //      is bounded at gain·BtlBw_slow·RTprop_slow — the standing queue stays
    //      ≈0 (RTT ≈ RTprop ≈ 40 ms) so the DAPS pre-fetch slack is preserved.
    //      RWM_DAPS_BDP=gain (default 1.0 = exactly one BDP; 0 disables).
    //  (2) BBR per-path PACING: each path emits at its own BtlBw, so the future-
    //      offset data flows at the slow path's drain rate WITHOUT queuing.  When
    //      the slow path's BtlBw pace bucket is dry the source spills to the fast
    //      path this instant (no burst on the slow path).  RWM_DAPS_PACE=0
    //      disables (default on under DAPS).  The DAPS offset Δ_j itself is
    //      computed from RTprop (min-filtered) in `daps_offset_syms`, NOT the
    //      bufferbloated RTT, so a bloated RTT can never mis-size the offset.
    let daps_bdp_gain: f64 = if daps {
        std::env::var("RWM_DAPS_BDP")
            .ok().and_then(|s| s.parse::<f64>().ok()).unwrap_or(1.0).max(0.0)
    } else {
        0.0
    };
    let daps_pace_on: bool = daps && crate::config::env_flag("RWM_DAPS_PACE", true);
    // feat/pace-all-traffic: route the CODED/REPAIR emission (proactive, filling,
    // deficit top-up, inline) through the SAME per-path BtlBw pacer as source, so
    // TOTAL per-path emission ≤ BtlBw_i and no standing queue builds (the residual
    // the source-only pacer left).  Extends the DAPS/pace gate — ON by default
    // whenever per-path pacing is on; RWM_PACE_ALL=0 reproduces the source-only
    // pacer (the same-binary A/B baseline).  Shipped non-DAPS default untouched.
    let pace_all_on: bool = daps_pace_on && crate::config::env_flag("RWM_PACE_ALL", true);
    // feat/source-backpressure: bound the SOURCE emission by the per-path BtlBw
    // bucket too.  Pace-all held the REPAIR when both buckets were dry, but the
    // SOURCE placement gate still SPILLED to the fast path unconditionally and
    // decremented its bucket NEGATIVE — an unmetered burst that became the
    // residual ~100 ms fast-path standing queue (and, via the shared buckets,
    // forced repair onto the slow path, re-opening the slow queue).  Source is
    // payload (cannot be dropped), so the discipline is DEFER not discard: when
    // neither the DAPS candidate nor the fast path has a funded bucket, PAUSE
    // the TUN read (the app / QUIC send-buffer backpressures) instead of
    // bursting.  This makes TOTAL per-path emission (source + repair) ≤ BtlBw_i
    // on EVERY path — the source analogue of the repair HOLD.
    //
    // DEFAULT OFF (opt-in via RWM_SRC_BP=1).  L1 REFUTED the hypothesis: unlike
    // the rateless repair HOLD (a dropped repair is free, retried on refill),
    // DEFERRING the source stalls the generation-fill PIPELINE — the source read
    // is the pipeline clock, so pausing it starves coded emission too, producing
    // long paused=100% stalls.  Measured C8 REGRESSED ~53% on BOTH seeds
    // (seed42 14.35→6.60, seed7 15.63→7.39 Mbit/s) and destabilized (σ_s
    // 1.1/1.3→9.5/4.1 s).  The pace-all spill baseline is benign (the fast path
    // drains the spilled source) and already stable at ~0.72–0.79 of the
    // recovery ceiling.  Kept as a gated, unit-tested, oracle-modelled knob for
    // the scientific record; shipped DEFAULT (RWM_SRC_BP unset/0) is the spill
    // baseline — byte-identical to pace-all (the gate computes nothing when off).
    let src_bp_on: bool = daps_pace_on && crate::config::env_flag("RWM_SRC_BP", false);
    // feat/per-path-estimator: drive per-path delivered-rate attribution.
    // On (a) under DAPS — the cap/pacer need per-path BtlBw/BDP — and (b) when
    // RWM_PER_PATH_EST is set standalone, so a PLAIN generation multipath run
    // also establishes per-path BtlBw (the general-fix check: the CC + the
    // placement law get a stable per-path signal, not just DAPS).  Attribution
    // is generation-mode-only (it keys on the source_path_map + OOO acks) and
    // is a NO-OP for the shipped non-generation default (byte-identical).
    let per_path_est: bool =
        generation && (daps || crate::config::env_flag("RWM_PER_PATH_EST", false));
    // feat/btlbw-rate-sample: BBR-correct per-path delivery-rate sampling
    // (send-interval Δt, ack-aggregation robust).  ON by default whenever the
    // per-path estimator runs; RWM_RATE_SAMPLE=0 reproduces the legacy
    // ack-interval anchor (same-binary A/B).  When on, each SOURCE seq is
    // snapshotted at send (`on_src_sent`) and its ack drives `on_src_delivered_seq`
    // (a send-interval rate sample) instead of the legacy `on_src_delivered`.
    let rate_sample: bool = per_path_est && crate::config::env_flag("RWM_RATE_SAMPLE", true);
    // feat/daps-readahead-depth: bound each non-fastest path's DAPS read-ahead
    // DEPTH to its skew-depth `skew_j·BtlBw_j` (queue delay ≤ skew ⇒ the slow
    // segment arrives in-order-aligned, never later than the fast path would
    // deliver that region — the ECF/BLEST completion guard done on DEPTH).  Once
    // path j holds that budget of read-ahead, fresh SOURCE steers to the fast
    // path and REPAIR spills/holds off j (`daps_depth_over_budget`).  This is the
    // structural residual the three prior pacers (§16.11-13) converged on: NOT
    // the source rate anchor (§16.13 fixed that, ×158→×1) but the deep read-ahead
    // over-commit that survives a correct anchor + BDP cap and bloats the slow
    // path to ~3-4 s.  Crucially a DEPTH limiter, NOT a rate throttle — within the
    // budget the path still emits at BtlBw (pace bucket unchanged), so the link
    // stays FULL (escapes §16.13's rate-throttle politeness-idle, C7 20.96→16.97).
    // Requires the correct anchor (rate_sample) so skew·BtlBw_j is right-sized.
    // ON by default under DAPS+rate-sample; RWM_DAPS_DEPTH=0 reproduces the
    // current unbounded read-ahead (the same-binary A/B baseline).  Shipped
    // non-DAPS default byte-identical (gated on rate_sample ⇒ generation && DAPS).
    let daps_depth_on: bool = rate_sample && crate::config::env_flag("RWM_DAPS_DEPTH", true);
    // App-limited (BBR): the source pipeline was starved (idle gap) rather than
    // cwnd/pace-limited when a symbol was sent — such a sample underestimates
    // BtlBw and must not be read as bw dropping.  We flag a send app-limited when
    // it follows an idle gap longer than this (a post-idle burst, the classic
    // starved interval).  Bulk back-to-back sends have ~0 gap ⇒ never flagged.
    let rs_app_limited_gap_us: u64 = 5_000;
    // Per-path BtlBw pace token buckets (symbols), refilled at BtlBw_i each loop.
    let mut daps_pace_tok: std::collections::HashMap<crate::scheduler::PathId, f64> =
        std::collections::HashMap::new();
    let mut daps_pace_last_us: u64 = now_us();
    // Generation-coding proactive overhead r.  FMTCP shipped a FIXED r=0.10
    // (~4× the ~2.6% operating loss — the over-FEC the DAPS work right-sizes).
    // DAPS instead DERIVES r* from §8.4 for the bulk/loose-δ profile:
    //   r* = ε/(1−ε) + z_{δ/ε}·√(εσ²_burst/(W(1−ε))),  z≈0 for the bulk δ≈ε.
    // At the C7/C8 operating loss (c2 GE ε≈0.026) this lands ≈0.04–0.05, NOT
    // 0.10; RWM_GEN_R overrides for the sweep {0.03,0.05,0.10}.
    let daps_r_star: f64 = {
        let eps = 0.026_f64; // c2 wifi operating loss (netem gemodel p=1.3% q=50%)
        let sigma2 = raptorpath_math::burst_variance_factor(0.013, 0.50);
        // bulk/loose tail δ = 0.2·ε (a small burst margin, not the tight δ the
        // realtime profile budgets); z = Φ⁻¹(1 − δ/ε) = Φ⁻¹(0.8).
        let z = raptorpath_math::normal_quantile(0.8);
        raptorpath_math::compute_r_star_with_z(eps, sigma2, gen_size as f64, z)
            .clamp(0.03, 0.10)
    };
    let gen_repair_floor: f64 = std::env::var("RWM_GEN_R")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(if daps { daps_r_star } else if fmtcp { 0.10 } else if systematic { 0.15 } else { 0.20 })
        .clamp(0.0, 2.0);
    // Codec pinned at startup (§16.4) — created once, never rebuilt.
    let mut encoder: Box<dyn WindowEncoder> = if systematic {
        Box::new(crate::fec::GenerationEncoder::new_systematic(symbol_size, gen_size, pipeline, gen_repair_floor))
    } else if generation {
        Box::new(crate::fec::GenerationEncoder::new(symbol_size, gen_size, pipeline, gen_repair_floor))
    } else {
        create_window_encoder(fec_backend, symbol_size, fec_controller, scheduler)
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
    let gen_rate: f64 = std::env::var("RWM_GEN_RATE")
        .ok().and_then(|s| s.parse().ok()).unwrap_or(9000.0);
    // Bootstrap pacing floor (symbols/sec): the rate used before the ack-rate
    // estimator has a sample (primes the first generation). Kept modest so the
    // startup burst can't overrun a bandwidth-limited link's datagram intake;
    // once the ack rate is known the pacing clocks to delivered goodput × 1.5.
    let gen_rate_floor: f64 = std::env::var("RWM_GEN_RATE_FLOOR")
        .ok().and_then(|s| s.parse::<f64>().ok()).unwrap_or(2000.0).clamp(1.0, gen_rate);
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
    let cc_pace = crate::config::env_flag("RWM_CC_PACE", false);
    let cc_pace_headroom: f64 = std::env::var("RWM_CC_PACE_HR")
        .ok().and_then(|s| s.parse::<f64>().ok()).unwrap_or(1.1).clamp(1.0, 2.0);
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
    // FMTCP forces once-per-RTT deficit coalescing (1.0·SRTT): the design's
    // "ONE deficit feedback per RTT" — the #59/#60 lesson that a sub-RTT re-flood
    // of the fungible top-up defeats aggregation. RWM_REACT_CAP still overrides.
    let react_cap_cfg: f64 = std::env::var("RWM_REACT_CAP")
        .ok().and_then(|s| s.parse::<f64>().ok()).unwrap_or(if fmtcp { 1.0 } else { 0.0 }).max(0.0);
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
    let gen_inflight_window: f64 = std::env::var("RWM_GEN_INFLIGHT")
        .ok().and_then(|s| s.parse().ok())
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
    let repair_rate_floor: f64 = std::env::var("RWM_MIN_R")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
        .clamp(0.0, 2.0);
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
    let ooo_retain = (crate::config::env_flag("RWM_OOO_RETAIN", false) || fmtcp) && generation;
    let ooo_gens: usize = std::env::var("RWM_OOO_RETAIN")
        .ok().and_then(|s| s.parse::<usize>().ok()).filter(|&n| n >= 2).unwrap_or(16);
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
        let gens = if ooo_retain { ooo_gens + 1 } else { pipeline + 1 };
        (gen_size * gens).clamp(MAX_WINDOW_SIZE, 1 << 20)
    } else if coded_only {
        std::env::var("RWM_WINDOW")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
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
        let default_store = if ooo_retain { ooo_gens * gen_size } else { 2 * gen_size };
        std::env::var("RWM_STORE")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(default_store)
            .clamp(gen_size, win_cap)
    } else if coded_only {
        std::env::var("RWM_STORE")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(win_cap)
            .clamp(win_cap, 1 << 20)
    } else {
        // Plain-reliable (systematic-free, non-generation) MEMORY ceiling for
        // the retention store. RWM_STORE forces a STATIC window (disables the
        // dynamic BDP cap below) for the sweep; the shipped default keeps the
        // large retention ceiling and lets the delay-based `plain_dyn_cap`
        // bound the *outstanding* window instead.
        std::env::var("RWM_STORE")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(RELIABLE_STORE_MAX)
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
    let plain_dyn_cap = reliable && !generation && !coded_only
        && std::env::var("RWM_STORE").is_err();
    // Window = gain × BDP. ≥2 keeps the pipe full (≈1 BDP) while leaving ≈1
    // BDP of headroom to keep sending fresh data during a one-RTT recovery
    // round; 2.5 adds jitter/burst slack. RWM_STORE_GAIN overrides.
    let store_bdp_gain: f64 = std::env::var("RWM_STORE_GAIN")
        .ok().and_then(|s| s.parse::<f64>().ok()).unwrap_or(2.0).clamp(1.0, 64.0);
    // Cap before the BtlBw anchor warms (a few RTTs). Tight so the startup
    // burst can't pre-bloat the queue and inflate the min-RTT floor (which
    // would then inflate the anchor itself); the anchor takes over once
    // samples land. ~1.5× a 100 Mbit / 10 ms BDP.
    let store_boot_cap: usize = std::env::var("RWM_STORE_BOOT")
        .ok().and_then(|s| s.parse::<usize>().ok()).unwrap_or(128);
    // Floor so a transiently-tiny BDP estimate can't strangle the pipe.
    let store_cap_floor: usize = 64;
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
    // ── Proactive-frontier repair (plain-reliable) ────────────────────────
    // MEASURED root cause of the C2 lossy collapse (goal-gate "Proactive
    // Frontier"): under Bulk's r*→0 pure-ARQ steady state there is NO proactive
    // repair, so every in-order-frontier hole waits a full reactive ARQ round
    // (~1 RTT) → goodput ≈ window/RTT (~16 Mbit). The prior RWM_MIN_R arm added
    // repair over the LEADING window, which entangles the hole with not-yet-
    // received in-flight symbols → the receiver can't solve it until the window
    // tail arrives ~1 RTT later anyway (MEASURED decode stall ~25 ms > the ARQ
    // round it replaced). This instead codes repair over a SMALL TRAILING window
    // at the cumulative-ack frontier [ack+1, ack+1+W_front): all its members are
    // already received EXCEPT the hole, so the receiver's incremental GE solves
    // the hole the instant a covering repair arrives — recovery at the decode
    // rate, no round-trip. Rate is loss-sized (r_front = gain·ε̂) so it is 0 on
    // clean links (no regression) and ~gain·ε under loss. Env overrides:
    //   RWM_FRONTIER      trailing window width (default 32; 0 disables)
    //   RWM_FRONTIER_GAIN r_front = gain·ε̂ (default 4.0)
    //   RWM_FRONTIER_R    force a fixed r_front (bypasses gain·ε̂; for the sweep)
    let frontier_width: u64 = std::env::var("RWM_FRONTIER")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(32);
    let frontier_gain: f64 = std::env::var("RWM_FRONTIER_GAIN")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(4.0)
        .clamp(0.0, 64.0);
    // Offset by which the frontier-repair window TRAILS the send frontier. The
    // window must be pre-positioned: coded over a region whose members are all
    // already RECEIVED by the time the repair arrives (so the receiver's GE can
    // isolate a hole immediately), yet close enough to the send frontier that it
    // covers a symbol WHILE it is still fresh — so a loss is decoded ~½ RTT
    // after it is sent, before it can ever freeze the in-order frontier. Anchor
    // at the receiver's ack instead (½-RTT stale) and the repair only starts
    // covering a hole AFTER it has already stuck — losing the race to the ARQ
    // retransmit (MEASURED: rf=718 emitted, ru=4 useful, all recovery via ARQ).
    let frontier_offset: u64 = std::env::var("RWM_FRONTIER_OFFSET")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(8);
    let frontier_r_forced: Option<f64> = std::env::var("RWM_FRONTIER_R")
        .ok()
        .and_then(|s| s.parse::<f64>().ok());
    // GATED OFF by default (REFUTED at L1 — see goal-gate "Proactive Frontier").
    // Enabled only when RWM_FRONTIER / RWM_FRONTIER_R is explicitly set, so the
    // shipped default is byte-for-byte the pure-ARQ baseline. The flag exists to
    // reproduce the negative result and to drive the FDIAG diagnosis.
    let frontier_experiment =
        std::env::var("RWM_FRONTIER").is_ok() || std::env::var("RWM_FRONTIER_R").is_ok();
    let frontier_enabled = frontier_experiment
        && frontier_width > 0
        && reliable
        && !generation
        && !coded_only;
    let mut frontier_debt: f64 = 0.0;
    // ── Interspersed trailing-window repair (RWM_INLINE_REPAIR) — REFUTED ─────
    // GOAL: emit the systematic proactive repair INTERSPERSED with the source (at
    // the source rate, coded over a small trailing BLOCK of width `inline_w` of
    // already-sent source) so the covering repair arrives within ~1 block — not
    // ~1 generation-span — and is PRESENT when the receiver detects the hole →
    // proactive decode, no round-trip. The decode mechanism WORKS in isolation
    // (see `generate_repair_range` + the generation.rs unit tests: a block repair
    // present at a hole decodes it proactively, and `frontier_probe` reports it
    // buffered). But at L1 (goal-gate "Repair In-Flight") the TRANSPORT-level
    // emission is REFUTED for TWO structural reasons:
    //   (1) STALL-STARVED. It emits from the source-send path, so during
    //       backpressure / frontier-stall — exactly when the covering repair is
    //       most needed — NO source is sent and NO repair is emitted. The batched
    //       proactive block runs every loop iteration (incl. tx_paused wakeups)
    //       and does not have this defect.
    //   (2) CROSS-GRID STRANDING. For W < G the block (width W) and generation
    //       (width G) repairs create SEPARATE Gaussian matrices, so a buffered
    //       block equation cannot combine with reactive generation repair — the
    //       fungible joint-solve is broken (MEASURED probe_buffered rising while
    //       the frontier wedges, gap 900–1100). Unifying the grid (W = G) removes
    //       the stranding but reduces to "small G" (which the batched path already
    //       does, non-stalling). MEASURED: every inline config wedged or crawled;
    //       the fungible small-G batched path reached parity, inline did not.
    // Kept env-gated + default-OFF as a documented negative result (like
    // RWM_FRONTIER). The effective levers for the same goal are (a) BOUNDING the
    // reactive ARQ over-request (RWM_REACT_CAP + RWM_REPAIR_WAIT — the decisive
    // win: recovery_coded 30k→437, FEC 0.32→0.913 = parity) and (b) a SMALLER G
    // (raises present_at_stall 1→16 via the non-stalling fungible batched path).
    // RWM_INLINE_W tunes W. Systematic-repair path only; shipped path untouched.
    let inline_repair = systematic && crate::config::env_flag("RWM_INLINE_REPAIR", false);
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
    let proactive_pacer = systematic && crate::config::env_flag("RWM_PROACTIVE_PACER", false);
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
    // FMTCP forces fungible cross-path repair placement: a fast-path hole is
    // covered by repair already in flight on the SLOW (spare) path, so no block
    // waits on a specific slow-path symbol (the FMTCP fungibility escape).
    let xpath_repair = generation && (crate::config::env_flag("RWM_XPATH_REPAIR", false) || fmtcp);
    let inline_w: u64 = std::env::var("RWM_INLINE_W")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&w| w >= 2)
        .unwrap_or(64);
    // Sub-symbol repair debt: accrue `r` per source, emit one block repair per
    // whole unit — spreads ceil(W·r) repairs across the block, same total r.
    let mut inline_debt: f64 = 0.0;
    // Dedicated repair-index namespace for frontier repairs. Started high so it
    // never collides with the encoder's own `repair_counter` for a coincident
    // (block_id,payload_id) at the receiver's dedup set.
    let mut frontier_ri: u32 = 1u32 << 30;
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
            // App-limited (BBR rate-sample): the idle gap SINCE THE PREVIOUS
            // source send, captured before `last_source_send_us` is refreshed
            // below.  A long gap ⇒ this send follows a starved interval.
            let rs_src_app_limited =
                now_us().saturating_sub(last_source_send_us) > rs_app_limited_gap_us;
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
            let source_path = {
                if reliable && daps {
                    // DAPS delay-aware placement: the just-added source is at the
                    // sender's leading edge, `encoder.window_size()` symbols ahead
                    // of the in-order delivered frontier.  A slow path is eligible
                    // only when this lead ≥ its delay-skew offset Δ_j, so the slow
                    // path carries FUTURE data that arrives in sync (Sarwar 2013 /
                    // Kuhn 2014) with the ECF completion guard (Lim 2017).  The
                    // BLEST per-path BDP cap (daps_bdp_gain) additionally drops a
                    // path at its own BDP from the eligible set so the slow path is
                    // never over-committed (the bufferbloat fix).
                    let lead = encoder.window_size() as f64;
                    let mut sched = scheduler.lock();
                    let mut chosen = sched
                        .place_source_daps_capped_depth(lead, daps_bdp_gain, daps_depth_on)
                        .unwrap_or(0);
                    // BBR per-path pacing: if the picked path's BtlBw pace bucket
                    // is dry, spill to the fast (min-RTprop) path so no burst above
                    // the slow path's drain rate ever hits the wire.  Warm-up
                    // (no bucket yet) is transparent — no restriction, no consume.
                    if daps_pace_on {
                        let fast = sched.fastest_active_path().unwrap_or(0);
                        if chosen != fast
                            && daps_pace_tok.get(&chosen).is_some_and(|&t| t < 1.0)
                        {
                            chosen = fast;
                        }
                        if let Some(t) = daps_pace_tok.get_mut(&chosen) {
                            *t -= 1.0;
                        }
                    }
                    // feat/per-path-estimator: commit this source seq to `chosen`
                    // and charge its per-path SOURCE outstanding gauge (BLEST
                    // in_flight_i).  Released on per-path ack attribution below.
                    if let Some(p) = sched.path_mut(chosen) {
                        p.charge_src(1);
                    }
                    chosen
                } else if reliable {
                    let sched = scheduler.lock();
                    sched.place_symbol(false, &[]).unwrap_or(0)
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

            // ── Interspersed trailing-window repair (in-flight proactive) ──────
            // Emit proactive repair coded over the most-recently-SEALED trailing
            // block of width `inline_w`, paced at the loss overhead r, RIGHT HERE
            // in the same flight as the source. So the repair covering a hole
            // arrives within ~1 block of the hole (not a generation-span later) —
            // present when the receiver detects the hole → proactive decode, no
            // reactive round-trip. Same total overhead as the batched proactive
            // path (which is disabled under this flag); only the timing changes.
            if inline_repair {
                let frontier = wire_sym.block_id + 1; // sources sent so far
                let complete_blocks = frontier / inline_w;
                if complete_blocks >= 1 {
                    inline_debt += gen_repair_floor;
                    // Code the block that JUST sealed and keep coding it while the
                    // next block fills — spreads its ceil(W·r) DoF across a block
                    // span (fungible: all repairs share the (anchor,W) matrix).
                    let anchor = (complete_blocks - 1) * inline_w;
                    while inline_debt >= 1.0 {
                        let sym = match encoder.generate_repair_range(anchor, inline_w as u16) {
                            Some(s) => s,
                            None => break, // block not fully retained (shouldn't happen)
                        };
                        // pace-all-traffic: gate inline repair through the per-path
                        // BtlBw pacer too.  HOLD (discard the rateless symbol, keep
                        // inline_debt) when both paths' buckets are dry, so inline
                        // repair also never drives a path above BtlBw_i.
                        let path = {
                            let sched = scheduler.lock();
                            let cand = sched.place_symbol(true, &[]).unwrap_or(0);
                            match paced_repair_path!(sched, cand) {
                                Some(p) => p,
                                None => break,
                            }
                        };
                        inline_debt -= 1.0;
                        // Proactive (no round-trip) — counts toward the pfrac.
                        proactive_coded_total += 1;
                        let batch_seq = batch_counter.fetch_add(1, Ordering::Relaxed);
                        let batch = SymbolBatch {
                            symbols: vec![sym],
                            send_timestamp_us: now_us(),
                            batch_seq,
                            path_id: path,
                        };
                        if let Err(e) = transport.send_symbols(path, batch) {
                            warn!(path, ?e, "failed to send inline repair symbol");
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
            }

            // Track which path this source was sent on (for cross-path retransmission)
            source_path_map.insert(wire_sym.block_id, source_path);

            // feat/btlbw-rate-sample: snapshot this source seq's send-time state
            // on its DAPS-committed path so its ack yields a SEND-INTERVAL
            // delivery-rate sample (BBR).  Byte-identical when off.
            if rate_sample {
                let mut sched = scheduler.lock();
                if let Some(p) = sched.path_mut(source_path) {
                    p.on_src_sent(wire_sym.block_id, rs_src_app_limited);
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

            // ── Proactive-frontier repair (see decls above the loop) ──────
            // Code repair over the SMALL TRAILING window at the cumulative-ack
            // frontier [ack+1, ack+1+w) so a hole there decodes from in-flight
            // repair the instant a covering repair arrives — no ARQ round-trip,
            // and no entanglement with not-yet-received in-flight symbols (the
            // failure mode of the leading-window taper repair above). Loss-sized
            // rate ⇒ 0 on clean links (no regression). Bounded, cheap, and it
            // FALLS BACK to the existing per-seq ARQ whenever the hole has aged
            // out of the coding window (`generate_repair_range` → None).
            if frontier_enabled {
                let ack = window_ack_seq.load(Ordering::Relaxed);
                let (_, newest) = encoder.window_span();
                // Pre-positioned trailing window: end = newest − offset (tail is
                // already received when the repair lands); start = end − width,
                // clamped to the retained region (> ack). Covers the fresh region
                // so a loss decodes ~½ RTT after send, before it can stick.
                let end = newest.saturating_sub(frontier_offset);
                if end > ack + 1 {
                    let eps = {
                        let sched = scheduler.lock();
                        sched
                            .active_paths()
                            .iter()
                            .filter_map(|id| sched.path(*id))
                            .map(|p| p.estimator.loss_rate())
                            .fold(0.0_f64, f64::max)
                    };
                    let r_front =
                        frontier_r_forced.unwrap_or(frontier_gain * eps).clamp(0.0, 0.5);
                    frontier_debt = (frontier_debt + r_front).min(8.0);
                    // Widest window that stays within [ack+1, end).
                    let avail = end - (ack + 1);
                    while frontier_debt >= 1.0 {
                        frontier_debt -= 1.0;
                        let w = frontier_width.min(avail).max(1) as u16;
                        let start = end - w as u64;
                        let ri = frontier_ri;
                        frontier_ri = frontier_ri.wrapping_add(1);
                        let sym = match build_frontier_repair(
                            &sent_store,
                            start,
                            w,
                            symbol_size,
                            ri,
                        ) {
                            Some(s) => s,
                            None => break, // acked/evicted ⇒ ARQ fallback handles it
                        };
                        let fpath = {
                            let sched = scheduler.lock();
                            if reliable {
                                let covered =
                                    window_source_paths(&*encoder, &source_path_map);
                                sched.place_symbol(true, &covered).unwrap_or(source_path)
                            } else {
                                source_path
                            }
                        };
                        let batch_seq = batch_counter.fetch_add(1, Ordering::Relaxed);
                        let batch = SymbolBatch {
                            symbols: vec![sym],
                            send_timestamp_us: now_us(),
                            batch_seq,
                            path_id: fpath,
                        };
                        if let Err(e) = transport.send_symbols(fpath, batch) {
                            warn!(fpath, ?e, "failed to send frontier repair");
                        }
                        {
                            let mut sched = scheduler.lock();
                            if let Some(p) = sched.path_mut(fpath) {
                                p.charge_in_flight(1);
                            }
                        }
                        if let Some(ps) = stats.path(fpath) {
                            ps.symbols_sent.fetch_add(1, Ordering::Relaxed);
                        }
                        stats.fec.total_repair_symbols.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }};
    }

    // ── PACE-ALL-TRAFFIC (feat/pace-all-traffic) ──────────────────────────────
    // The per-path BBR pacer (`daps_pace_tok`, refilled at BtlBw_i) meters only
    // SOURCE placement; the CODED/REPAIR emission (batched proactive, filling
    // proactive, deficit top-up, inline) was emitted OUTSIDE it — so TOTAL
    // per-path emission (source + repair) exceeded BtlBw_i and a standing queue
    // built on BOTH the slow (~300 ms) and the fast (~140 ms) path.  This gate
    // routes every repair symbol through the SAME per-path bucket as source, so
    // the aggregate per-path send rate never exceeds the path's drain rate (the
    // temporal_oracle PART 6e "PACE" scheduler admits ≤ BtlBw_i *total*).  Given
    // a candidate path it evaluates to:
    //   * Some(candidate) — the candidate's bucket ≥ 1: emit there, one token
    //                       consumed;
    //   * Some(fast)      — candidate dry but the fast (min-RTprop) path has a
    //                       token: spill so the slow path never over-queues;
    //   * None            — BOTH the candidate and the fast path are dry: HOLD
    //                       (retry next loop as the buckets refill at BtlBw_i).
    //                       This is what bounds the FAST path too — source has
    //                       priority, repair uses only the leftover per-path
    //                       capacity, so neither path is driven above BtlBw_i.
    // A path whose anchor has not warmed (no bucket entry yet) is transparent —
    // it emits on the candidate and consumes nothing (mirrors the source gate).
    // Active only when `daps_pace_on`, so the shipped non-DAPS default is
    // byte-identical.  `$sched` is an already-held scheduler lock guard.
    macro_rules! paced_repair_path {
        ($sched:expr, $cand:expr) => {{
            let mut cand = $cand;
            // feat/daps-readahead-depth: repair is per-path read-ahead too.  If the
            // chosen non-fastest path has already filled its skew-depth budget,
            // redirect the repair to the fast path BEFORE pacing — bounding ALL
            // per-path look-ahead to one skew, not just source.  A DEPTH steer, not
            // a rate change; the pace gate below still meters the (possibly
            // redirected) path at BtlBw.  When the fast path is itself dry the
            // pace gate HOLDs (rateless repair is free to retry).
            if daps_depth_on {
                let fast = $sched.fastest_active_path().unwrap_or(0);
                if cand != fast && $sched.daps_depth_over_budget(cand) {
                    cand = fast;
                }
            }
            if pace_all_on {
                let fast = $sched.fastest_active_path().unwrap_or(0);
                paced_repair_decision(&mut daps_pace_tok, cand, fast)
            } else {
                Some(cand)
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
    // untouched when off.
    let diag_on = crate::config::env_flag("RWM_DIAG", false);
    // Transport-ceiling fix (generation mode): bound the in-flight (unacked)
    // symbols to ~BDP instead of the fixed store_max = G·(M+1). The oversized
    // store_max is decoupled from the pipe (14× BDP at C2), so unpaced source
    // emission builds a multi-hundred-ms standing queue (MEASURED RTT inflated
    // to 0.5–1.3 s), which turns every hole into a ~1 s recovery stall. Cap
    // total in-flight at a BDP-scaled bound so the queue — and thus the
    // recovery-stall latency — stays small. 0 = off (legacy store-only
    // backpressure). The deficit-recovery emission is EXEMPT (it must always be
    // able to fund a frontier hole, else a full-window pipe deadlocks).
    let infl_cap: u64 = std::env::var("RWM_INFL_CAP")
        .ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    // PART 1.2 (receiver-tail): BDP-DERIVED in-flight cap. A fixed RWM_INFL_CAP
    // must be hand-tuned per RTT; instead bound total in-flight to
    // gain × Σ copa_bdp_anchor (BtlBw×RTprop, bufferbloat-robust) recomputed
    // live, so the standing queue — and thus the RECOVERY-ROUND RTT — stays
    // ~gain·BDP at ANY RTT. It gates BOTH proactive emission AND (Fix-2
    // non-exempt) reactive/deficit recovery via `cwnd_full`, so the parallel
    // tail flush cannot re-bloat the queue. Env RWM_INFL_BDP=gain (e.g. 2.0);
    // 0/unset = off (legacy static RWM_INFL_CAP / store-only backpressure).
    // FMTCP ships gain 1.5 (oracle PART 5c: the bare aggregate BDP starves the
    // recovery headroom and collapses to 0.93×; ~1.5× over the windowed-max —
    // hence under-estimating — anchor gives the emergent ~1.3× BDP operating
    // point). The FMTCP cap is enforced PER PATH (see `fmtcp_percap` below) —
    // the #64 fix: the slow path's RTT-inflated BtlBw·RTprop bounds ONLY the
    // slow path, never a single global budget the fast path stalls behind.
    let infl_bdp_gain: f64 = std::env::var("RWM_INFL_BDP")
        .ok().and_then(|s| s.parse::<f64>().ok()).unwrap_or(if fmtcp { 1.5 } else { 0.0 }).max(0.0);
    let infl_bdp_on = infl_bdp_gain > 0.0;
    // FMTCP #64 fix: enforce the in-flight cap PER PATH (path i outstanding ≤
    // gain·BtlBw_i·RTprop_i) rather than as one fungible global Σ budget. The
    // sender is TUN-paused only when EVERY active path is at its own cap, so the
    // fast path keeps pulling fresh source while the slow path is full — the
    // total-in-flight escape from the in-order-frontier stall.
    let fmtcp_percap = fmtcp;
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
    let coded_src_clock = crate::config::env_flag("RWM_CODED_SRC", false);
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
    let no_reactive = crate::config::env_flag("RWM_NO_REACTIVE", false);
    let diag_start_us = now_us();
    let mut diag_last_us = now_us();
    let mut diag_last_ack: u64 = 0;
    let mut diag_last_src: u64 = 0;
    let mut diag_last_cod: u64 = 0;
    let mut diag_paused_iters: u64 = 0;
    let mut diag_total_iters: u64 = 0;
    let mut diag_eff_rate: f64 = 0.0;

    loop {
        // SACK flow control (feat/sack-flow-control): drain the receiver's
        // RECEIVED-above-frontier ranges NON-BLOCKING at the top of every
        // iteration (never as a select! branch — a frequently-ready channel
        // there would race, and cancel, the `tun.read_packet()` future and
        // starve/stall intake). An out-of-order-received symbol is DELIVERED:
        // drop it from the retention store and the per-seq ARQ bookkeeping even
        // though the in-order cumulative frontier still sits below it on an
        // unfilled hole. The flow-control gate below keys on `sent_store.len()`
        // (= TRUE outstanding after this pruning), so the send window tracks the
        // real pipe rather than freezing at the frozen frontier. The hole itself
        // (NOT in any received range) stays retained and recovers in the
        // background via the orthogonal NACK / tail-sweep path. The loop wakes at
        // least every 1 ms (backpressure/emission poll) so drains stay prompt.
        while let Ok(ranges) = sack_rx.try_recv() {
            for (start, end) in ranges {
                if end < start {
                    continue;
                }
                let acked: Vec<u64> = sent_store.range(start..=end).map(|(&k, _)| k).collect();
                for k in acked {
                    sent_store.remove(&k);
                    retransmit_buffer.remove(&k);
                    source_path_map.remove(&k);
                    nack_retx_at.remove(&k);
                }
                // feat/per-path-estimator: OOO per-path ack attribution.  In
                // generation mode sent_store is empty (the loop above is inert),
                // and the in-order cumulative frontier STALLS on holes — exactly
                // when the estimator is most starved.  A SACK range is OOO
                // delivery evidence: attribute each newly-received source seq to
                // the path its DAPS placement committed it to (`source_path_map`)
                // and drive that path's delivered-rate estimator, so BtlBw_i
                // keeps establishing even while the frontier is frozen.  Remove
                // the seq so the cumulative pass below cannot double-count it.
                if per_path_est {
                    let attributed: Vec<u64> =
                        source_path_map.range(start..=end).map(|(&k, _)| k).collect();
                    if !attributed.is_empty() {
                        let mut sched = scheduler.lock();
                        for k in attributed {
                            if let Some(&pid) = source_path_map.get(&k) {
                                if let Some(p) = sched.path_mut(pid) {
                                    if rate_sample {
                                        p.on_src_delivered_seq(k);
                                    } else {
                                        p.on_src_delivered(1);
                                    }
                                }
                                source_path_map.remove(&k);
                            }
                        }
                    }
                }
            }
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
        let store_len = if generation { encoder.window_size() } else { sent_store.len() };
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
        // FMTCP (#64 fix): also decide fullness PER PATH — the sender is "full"
        // (TUN-paused) only when NO active path is below its own cap
        // (gain·BtlBw_i·RTprop_i), so the fast path keeps pulling source while
        // the slow path is at its RTT-inflated cap. Non-FMTCP keeps the legacy
        // global Σ in-flight ≥ Σ cap test.
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
            (infl, fmtcp_percap_full(&per_path))
        } else {
            (0, false)
        };
        let cwnd_full = eff_infl_cap > 0
            && if fmtcp_percap { percap_full } else { pipe_infl >= eff_infl_cap };
        // Plain-reliable delay-based window cap (paper §12): bound the
        // outstanding store to gain×BDP so the standing queue stays ~1 RTT and
        // loss recovery does not stall behind a bloated queue. Refreshed off
        // the scheduler lock at most every 5 ms.
        if plain_dyn_cap {
            let dnow = now_us();
            if dnow.saturating_sub(dyn_cap_refresh_us) >= 5_000 {
                dyn_cap_refresh_us = dnow;
                let bdp: f64 = {
                    let sched = scheduler.lock();
                    sched
                        .active_paths()
                        .iter()
                        .filter_map(|id| sched.path(*id).and_then(|p| p.copa_bdp_anchor()))
                        .sum()
                };
                dyn_store_cap = if bdp > 0.0 {
                    ((store_bdp_gain * bdp).ceil() as usize).clamp(store_cap_floor, store_max)
                } else {
                    store_boot_cap.min(store_max)
                };
            }
        }
        let effective_store_cap = if plain_dyn_cap { dyn_store_cap } else { store_max };
        // TOTAL-IN-FLIGHT FLOW CONTROL (FMTCP change 1, the crux). The shipped
        // generation gate is `store_len >= store_cap`, where store_len =
        // encoder.window_size() = retained sources back to the IN-ORDER decode
        // frontier — so a hole freezes the frontier, the store fills to the cap,
        // and the sender idles behind the hole (the oracle's in-order-frontier
        // stall, PART 5). FMTCP instead gates ONLY on the per-path BDP in-flight
        // (`cwnd_full`), which drains on the RTT timescale (expire_in_flight),
        // decode-order-INDEPENDENT — a hole never freezes it. Retention still
        // drops on the in-order ack for reliability (memory bounded ≈ recovery
        // window, since the hole decodes fungibly within ~1 RTT and the ack then
        // advances), but it no longer gates intake. The whole-object memory
        // ceiling win_cap = ooo_gens·G is the loose safety backstop.
        let tx_paused = if fmtcp {
            // FMTCP flow control. The sender pipelines a BOUNDED number of
            // generations PAST the in-order frontier (fmtcp_win_backstop =
            // (pipeline+2)·G): far enough to keep sending across a hole (the
            // decouple that the in-order store gate forbids — the aggregation
            // lever), but bounded so the receiver OOO backlog and the standing
            // queue stay small (MEASURED: an unbounded decouple ballooned win to
            // 4238 and the RTT to 2.5 s of bufferbloat; a decode-progress gate
            // wedged). cwnd_full is the per-path BDP in-flight bound. The
            // window_decoded_seq total-decode signal is published for DIAG /
            // occupancy reporting (the oracle's `d`), not used to gate here.
            reliable && fmtcp_tx_paused(cwnd_full, store_len, fmtcp_win_backstop)
        } else {
            reliable && (store_len >= effective_store_cap || cwnd_full)
        };

        // RWM_DIAG periodic constraint report (see decls above the loop).
        if diag_on {
            diag_total_iters += 1;
            if tx_paused {
                diag_paused_iters += 1;
            }
            let dnow = now_us();
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
                    // RTprop?).  Cap gain = the DAPS placement gain when active,
                    // else the FMTCP aggregate gain.
                    let cap_gain = if daps_bdp_gain > 0.0 { daps_bdp_gain } else { infl_bdp_gain };
                    let mut pp = String::new();
                    let ids = sched.active_paths();
                    // feat/daps-readahead-depth: snapshot each path's skew-depth
                    // budget (skew·BtlBw_j) under the immutable borrow, before the
                    // per-path mutable loop below (borrow-checker).
                    let dbud: std::collections::HashMap<crate::scheduler::PathId, f64> = ids
                        .iter()
                        .map(|id| (*id, sched.daps_depth_budget_syms(*id).unwrap_or(0.0)))
                        .collect();
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
                            // feat/per-path-estimator DIAG: the SOURCE outstanding
                            // gauge (BLEST in_flight_i, the value the cap now keys
                            // on), the ack-attributed per-path BtlBw_i (sym/s), and
                            // whether the per-path BDP anchor has ESTABLISHED — the
                            // signals the DAPS residual was missing.  Piece 2 send-
                            // buffer proxy: charged-source − src_inflight is drained,
                            // so a growing src_inflight relative to bdp is the queue.
                            let sinfl_i = p.src_inflight() as u64;
                            let btlbw_i = p.btlbw_sym_per_s().unwrap_or(0.0);
                            let est_i = if p.anchor_established() { "Y" } else { "n" };
                            // feat/daps-readahead-depth DIAG: the skew-depth budget
                            // (skew·BtlBw_j) this path's read-ahead is bounded to,
                            // to compare the OBSERVED sinfl against Δ×BtlBw at L1.
                            let dbud_i = dbud.get(id).copied().unwrap_or(0.0);
                            // diag/slow-path-anchor: the rate-sample anchor trace
                            // (snapshotted-at-send / of-which-app-limited / acks-
                            // attributed / no-record / rej[interval/zero/applim] /
                            // generated / windowed-max-fill).  Cumulative counters.
                            let (rs_sent, rs_al, rs_attr, rs_nr, rs_iv, rs_zr, rs_al_rej, rs_gen, rs_fill) =
                                p.rs_diag();
                            pp.push_str(&format!(
                                " p{}:infl={}/sinfl={}/bdp{:.0}(cap{}) btlbw={:.0} dbud={:.0} est={} rtt={:.0}/rtp{:.0}ms | ANCHOR sent={} al={} attr={} nr={} rej[iv={} zr={} al={}] gen={} fill={}",
                                id, infl_i, sinfl_i, bdp_i, cap_i, btlbw_i, dbud_i, est_i, rtt_i, rtprop_i,
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
                // FMTCP: total-decode occupancy (the oracle's outstanding = sent_src
                // − d). Bounded ≈ aggregate BDP is the FMTCP signature (vs the whole
                // object). `d` = window_decoded_seq (out-of-order across all gens).
                let fmtcp_out = if fmtcp {
                    src_now.saturating_sub(window_decoded_seq.load(Ordering::Relaxed))
                } else { 0 };
                eprintln!(
                    "[DIAG] t={:.1}s win={}/{} paused={:.0}% good={:.1}Mbit ackrate_ewma={:.0}sym/s eff_pace={:.0}sym/s src={:.0}sym/s cod={:.0}sym/s cwnd={} infl={} np={} rtt={:.1}ms bdp100={:.0}sym fmtcp_out={} winbackstop={}{}",
                    dnow.saturating_sub(diag_start_us) as f64 / 1e6,
                    store_len, effective_store_cap,
                    paused_frac * 100.0,
                    good_mbit,
                    if generation { gen_rate_ewma } else { 0.0 },
                    eff,
                    src_rate, cod_rate,
                    cw, fl, np,
                    min_rtt_us as f64 / 1000.0,
                    bdp_100m,
                    fmtcp_out, fmtcp_win_backstop,
                    pp,
                );
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
        if generation && crate::config::env_flag("RWM_TRACE", false) {
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
        if generation && crate::config::env_flag("RWM_PFRAC", false) {
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
            let target = if coded_src_clock || ooo_retain {
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
            let eff_factor = if cc_pace { cc_pace_headroom } else { 1.5 };
            // Fix 1: under cc_pace clock coded emission on the same frontier-
            // independent CC rate (max with the goodput EWMA) so a stalled
            // in-order ack does not starve coded emission below the link.
            let eff_base = if cc_pace { gen_rate_ewma.max(cc_rate_cached) } else { gen_rate_ewma };
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
            // Under RWM_INLINE_REPAIR the proactive budget is emitted INTERSPERSED
            // with the source (in the send macro), not batched here — so the
            // batched round-robin is disabled and only the reactive deficit loop
            // below runs (the fallback for bursts the inline block missed).
            while !inline_repair
                && !proactive_pacer
                && (gen_coded_total as f64) < target
                && emitted < burst_cap
                && gen_tokens >= 1.0
                && !cwnd_full
                && encoder.wants_coding()
            {
                // pace-all-traffic: pick the candidate path + apply the per-path
                // pace gate FIRST (before generating / charging), so a HOLD when
                // both paths' BtlBw buckets are dry wastes no coded symbol.
                let path = {
                    let sched = scheduler.lock();
                    let cand = if xpath_repair {
                        sched.place_repair_spare_path().unwrap_or(0)
                    } else {
                        sched.place_symbol(true, &[]).unwrap_or(0)
                    };
                    match paced_repair_path!(sched, cand) {
                        Some(p) => p,
                        None => break, // both paths paced-out — retry next loop
                    }
                };
                gen_coded_total += 1;
                emitted += 1;
                gen_tokens -= 1.0;
                let sym = encoder.generate_repair();
                // Count this proactive emission toward the per-generation
                // in-flight accounting so the deficit loop never double-sends
                // what proactive already covered.
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
                    // pace-all-traffic: candidate + per-path pace gate FIRST.
                    let path = {
                        let sched = scheduler.lock();
                        let cand = if xpath_repair {
                            sched.place_repair_spare_path().unwrap_or(0)
                        } else {
                            sched.place_symbol(true, &[]).unwrap_or(0)
                        };
                        match paced_repair_path!(sched, cand) {
                            Some(p) => p,
                            None => break, // both paths paced-out — retry next loop
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
                        // pace-all-traffic: gate that placement through the per-path
                        // BtlBw pacer.  The deficit top-up was the DOMINANT unpaced
                        // repair feeding the standing queue; if BOTH paths are dry
                        // HOLD — discard this (rateless) symbol WITHOUT decrementing
                        // the want, so the generation is re-covered next loop as the
                        // buckets refill (bounds deficit top-up to BtlBw_i per path).
                        let path = {
                            let sched = scheduler.lock();
                            let cand = if xpath_repair {
                                sched.place_repair_spare_path().unwrap_or(0)
                            } else {
                                sched.place_symbol(true, &[]).unwrap_or(0)
                            };
                            match paced_repair_path!(sched, cand) {
                                Some(p) => p,
                                None => break 'recover,
                            }
                        };
                        *gen_emitted.entry(a).or_insert(0) += 1;
                        recovery_coded_total += 1;
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
                let (cw, srtt_s) = {
                    let sched = scheduler.lock();
                    let mut cw = 0.0f64;
                    let mut srtt = 0.0f64;
                    for id in sched.active_paths() {
                        if let Some(p) = sched.path(id) {
                            cw += p.cwnd as f64;
                            srtt = srtt.max(p.srtt().as_secs_f64());
                        }
                    }
                    (cw, srtt)
                };
                cc_rate_cached = if srtt_s > 1e-4 { cw / srtt_s } else { 0.0 };
            }
            // Pace at the HIGHER of the CC rate and the delivered-goodput EWMA so
            // a stalled in-order frontier (EWMA→0) can't throttle the source ramp.
            let link_est = gen_rate_ewma.max(cc_rate_cached);
            let src_rate = (link_est * cc_pace_headroom).clamp(gen_rate_floor, gen_rate);
            let dt = now.saturating_sub(src_tok_last_us);
            src_tok_last_us = now;
            let burst = (src_rate * 0.004).clamp(8.0, 64.0);
            src_tokens = (src_tokens + src_rate * (dt as f64 / 1_000_000.0)).min(burst);
        }
        // DAPS BBR per-path pacing: refill each active path's BtlBw token bucket
        // so the placement gate above emits each path at its own drain rate
        // (the slow path's future-offset data flows at BtlBw_slow without
        // queuing).  Independent of cc_pace; transparent until the anchor warms.
        if daps_pace_on {
            let now = now_us();
            let dts = now.saturating_sub(daps_pace_last_us) as f64 / 1_000_000.0;
            daps_pace_last_us = now;
            let sched = scheduler.lock();
            for id in sched.active_paths() {
                if let Some(btlbw) = sched.path(id).and_then(|p| p.btlbw_sym_per_s()) {
                    let burst = (btlbw * 0.004).clamp(4.0, 64.0); // ≤4 ms burst
                    let t = daps_pace_tok.entry(id).or_insert(burst);
                    *t = (*t + btlbw * dts).min(burst);
                }
            }
        }
        // feat/source-backpressure: peek whether the NEXT source symbol can be
        // admitted without overdrawing any per-path BtlBw bucket.  The source
        // would be placed on the DAPS candidate (or spill to the fast path); if
        // BOTH buckets are dry we DEFER the TUN read (backpressure) rather than
        // spill the fast bucket negative.  Computed here (once per loop) so it
        // gates the `read_packet` select arm below.  Transparent (admit) until
        // the per-path anchors warm, and a NO-OP when src_bp is off.
        let src_pace_ok: bool = if src_bp_on && !tx_paused {
            let lead = encoder.window_size() as f64;
            let sched = scheduler.lock();
            let fast = sched.fastest_active_path().unwrap_or(0);
            let cand = sched
                .place_source_daps_capped_depth(lead, daps_bdp_gain, daps_depth_on)
                .unwrap_or(fast);
            source_pace_admit(&daps_pace_tok, cand, fast)
        } else {
            true
        };

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
            // Fix 1: pacing wake — when source sends are paced-off (bucket
            // empty), wake at 1 ms to refill it. Without this the select could
            // block in read_packet with the pacing gate closed and stall intake.
            _ = tokio::time::sleep(Duration::from_millis(1)),
                if cc_pace && !tx_paused && src_tokens < 1.0 => None,
            // feat/source-backpressure wake: with the TUN read deferred because
            // every per-path bucket is dry, wake at 1 ms to let the buckets
            // refill at BtlBw_i, then re-poll (the source-side analogue of the
            // cc_pace refill wake above — without it the loop could block in
            // read_packet with the source pace gate closed).
            _ = tokio::time::sleep(Duration::from_millis(1)),
                if src_bp_on && !tx_paused && !src_pace_ok
                    && (!cc_pace || src_tokens >= 1.0) => None,
            p = tun.read_packet(),
                if !tx_paused && (!cc_pace || src_tokens >= 1.0)
                    && (!src_bp_on || src_pace_ok) => Some(p),
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
            // Reduce repair_debt proportionally — ACK'd symbols no longer need proactive coverage
            let newly_acked = ack - prev_ack;
            // feat/per-path-estimator: cumulative-frontier per-path ack
            // attribution.  Every source seq the in-order frontier just passed
            // (prev_ack+1..=ack) that is still owned in source_path_map (i.e.
            // NOT already attributed OOO via a SACK above) is now delivered:
            // attribute it to its DAPS placement path and drive that path's
            // delivered-rate estimator + release its SOURCE outstanding gauge.
            // Range-query the map (BTreeMap) so the cost is O(unattributed in
            // span), not O(span).  Runs before the source_path_map.retain below
            // that drops the acked range wholesale.
            if per_path_est {
                let attributed: Vec<u64> = source_path_map
                    .range((prev_ack + 1)..=ack)
                    .map(|(&k, _)| k)
                    .collect();
                if !attributed.is_empty() {
                    let mut sched = scheduler.lock();
                    for k in attributed {
                        if let Some(pid) = source_path_map.remove(&k) {
                            if let Some(p) = sched.path_mut(pid) {
                                if rate_sample {
                                    p.on_src_delivered_seq(k);
                                } else {
                                    p.on_src_delivered(1);
                                }
                            }
                        }
                    }
                }
            }
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
        FecBackend::Streaming => {
            let params = crate::fec::StreamingParams::from_channel(2.0, 0.05, 1.15);
            Box::new(crate::fec::StreamingDecoder::new(symbol_size, params))
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
    // Some(..) in window mode: the PEER's TOTAL DECODED count `d` (FMTCP change
    // 1), published from each WindowAck's `cumulative_received` (monotonic
    // fetch_max) and read by the local FMTCP sender for total-in-flight FC.
    peer_decoded: Option<&Arc<AtomicU64>>,
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
            // FMTCP change 1: publish the peer's TOTAL DECODED count `d` (the OOO
            // receiver sets cumulative_received = received_seqs.len()). Monotonic
            // — d only grows — so fetch_max is correct across multi-path/out-of-
            // order acks. The FMTCP sender gates outstanding = sent_src − d.
            if let Some(pd) = peer_decoded {
                pd.fetch_max(cumulative_received, Ordering::Relaxed);
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
        // control, not eviction. (Out-of-order object mode keeps this cap:
        // relaxing it was MEASURED harmful — the coding window slid off the
        // recovery frontier; see the store_backpressure NOTE.)
        assert!(!store_backpressure(true, RELIABLE_STORE_MAX - 1));
        assert!(store_backpressure(true, RELIABLE_STORE_MAX));
        assert!(store_backpressure(true, RELIABLE_STORE_MAX + 1));
        // EVICT mode never backpressures on retention.
        assert!(!store_backpressure(false, RELIABLE_STORE_MAX * 10));
    }

    // ----- FMTCP-class pure decode-on-total aggregation (change 1 + change 2) --

    /// FMTCP change 1 (flow control decoupled from the in-order frontier, but
    /// BOUNDED). The generation store gate pauses intake at ~2·G (the send
    /// frontier pinned near the in-order frontier — a hole freezes it and the
    /// sender idles: the oracle's in-order-frontier stall). FMTCP instead lets the
    /// send frontier run a BOUNDED number of generations PAST the in-order
    /// frontier (win backstop = (pipeline+2)·G), so the sender keeps sending
    /// across a hole (the aggregation lever) — but not unboundedly (an unbounded
    /// decouple bufferbloated the queue / wedged, MEASURED). store_len here is
    /// `win` = retained back to the in-order frontier.
    #[test]
    fn fmtcp_flow_control_advances_past_a_frozen_frontier() {
        let win_backstop = 1536; // (pipeline 2 + 2)·G, G=384
        // A hole froze the in-order frontier; win has grown 2 generations (768)
        // past it. FMTCP keeps sending — a frozen frontier does NOT immediately
        // stall the sender (unlike the plain 2·G gate).
        assert!(
            !fmtcp_tx_paused(false, 768, win_backstop),
            "FMTCP pipelines a few generations past a frozen frontier"
        );
        // Pauses at the bounded win backstop (anti-bufferbloat — the OOO backlog
        // and standing queue cannot balloon).
        assert!(
            fmtcp_tx_paused(false, win_backstop, win_backstop),
            "FMTCP pauses at the bounded win backstop (few generations)"
        );
        // … or when the per-path BDP in-flight (cwnd_full) is full.
        assert!(
            fmtcp_tx_paused(true, 100, win_backstop),
            "FMTCP also pauses on the per-path BDP in-flight (cwnd_full)"
        );
    }

    /// FMTCP change 2 (per-path BDP in-flight cap, the #64 fix). The sender is
    /// "full" only when NO path is below its OWN cap (gain·BtlBw_i·RTprop_i). The
    /// slow path's RTT-inflated cap bounds only the slow path; the fast path with
    /// room keeps the pipe moving — unlike the summed-anchor #64 global budget the
    /// fast path stalled behind.
    #[test]
    fn fmtcp_percap_bounds_each_path_independently() {
        // Fast path (cap 100) has room at 40; slow path (RTT-inflated cap 60) is
        // at its cap. NOT full — the fast path keeps pulling source.
        assert!(
            !fmtcp_percap_full(&[(40, 100), (60, 60)]),
            "fast path with room ⇒ not full even when the slow path is at its cap"
        );
        // Every path at/above its own cap ⇒ full (total in-flight ≈ Σ per-path BDP).
        assert!(
            fmtcp_percap_full(&[(100, 100), (60, 60)]),
            "all paths at their per-path cap ⇒ full"
        );
        assert!(
            fmtcp_percap_full(&[(120, 100), (80, 60)]),
            "all paths over their per-path cap ⇒ full"
        );
        // Degenerate zero-cap path never blocks (cap.max(1)); a fresh path with
        // room keeps the sender open.
        assert!(!fmtcp_percap_full(&[(0, 0), (10, 100)]));
        // Single fast path with room ⇒ not full (single-path parity control).
        assert!(!fmtcp_percap_full(&[(50, 145)]));
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

    #[test]
    fn test_sack_pruning_advances_sender_past_a_hole() {
        // ROOT-CAUSE FIX (feat/sack-flow-control): the plain-reliable sender's
        // flow control keys on `sent_store.len()` (= outstanding-unacked). Under
        // the OLD contract the store drained by the in-order cumulative frontier
        // ONLY (split_off(&(ack+1))), so a single hole froze the frontier, the
        // store stayed full, and TUN reads stalled for a reactive round-trip —
        // goodput collapsed to window/RTT. This asserts the new SACK-pruning
        // arm: out-of-order-received symbols leave the store immediately, so
        // outstanding tracks TRUE in-flight and the sender keeps injecting.
        use crate::fec::{RlcWindowEncoder, WindowEncoder, WireSymbol};
        let mut encoder = RlcWindowEncoder::new(64);
        let mut sent_store: BTreeMap<u64, WireSymbol> = BTreeMap::new();

        // Send 100 source symbols (seqs 0..=99), all retained.
        let n = 100u64;
        for i in 0..n {
            let sym = encoder.add_source(&vec![i as u8; 32]);
            sent_store.insert(sym.block_id, sym.clone());
        }
        assert_eq!(sent_store.len(), n as usize);

        // Receiver got 0..=9 contiguously (cumulative ack = 9), then a HOLE at
        // seq 10, then received EVERYTHING above it (11..=99) out of order.
        let ack = 9u64;
        // Cumulative frontier prune (removal below the contiguous frontier).
        sent_store = sent_store.split_off(&(ack + 1));
        // Under the OLD contract this is where it ends: the frozen frontier
        // leaves 90 symbols (10..=99) pinned in the store → still "full",
        // sender stalls behind the hole.
        assert_eq!(sent_store.len(), (n - (ack + 1)) as usize); // 90 pinned

        // NEW: the SACK ranges (received-above-frontier) prune the store for the
        // out-of-order deliveries — exactly the sender-loop arm's arithmetic.
        let sack_ranges: Vec<(u64, u64)> = vec![(11, 99)];
        for (start, end) in sack_ranges {
            if end < start {
                continue;
            }
            let acked: Vec<u64> = sent_store.range(start..=end).map(|(&k, _)| k).collect();
            for k in acked {
                sent_store.remove(&k);
            }
        }

        // Only the genuine hole (seq 10) remains retained — outstanding drops
        // from 90 to 1, well under any BDP-scaled cap, so the sender is FREE to
        // read the TUN and inject fresh source instead of freezing on the hole.
        assert_eq!(sent_store.len(), 1, "only the unfilled hole stays retained");
        assert!(sent_store.contains_key(&10), "the hole is retained for ARQ");
        // The hole's exact bytes survive for a targeted retransmit (reliability
        // contract intact: the hole is recovered in the background).
        assert_eq!(&sent_store.get(&10).unwrap().data[..32], &[10u8; 32]);
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

    /// feat/pace-all-traffic: ALL per-path emission (source + repair) is metered
    /// against the SAME per-path BtlBw token bucket, so the TOTAL per-path send
    /// rate never exceeds BtlBw_i — closing the standing queue the SOURCE-only
    /// pacer left (the coded/repair top-up was emitted outside it).
    /// `paced_repair_decision` is the gate. Drive a fast+slow pair with a repair
    /// FLOOD and assert: (a) repair never overdraws a bucket (so per-path repair
    /// ≤ BtlBw_i); (b) TOTAL per-path emission (source + repair) ≤ BtlBw_i·ticks
    /// + one burst; (c) an unpaced dump would blow the slow path's budget over.
    #[test]
    fn pace_all_traffic_bounds_total_per_path_emission_at_btlbw() {
        use std::collections::HashMap;
        let fast = 0u32;
        let slow = 1u32;
        // Heterogeneous C8 rates (symbols per tick). Fast ≫ slow, like c2+c3.
        let btlbw = |id: u32| if id == fast { 20.0f64 } else { 2.0f64 };
        let burst = 8.0; // token-bucket cap (a few ms of link)
        let ticks = 1000u64;

        let mut tok: HashMap<u32, f64> = HashMap::new();
        tok.insert(fast, 0.0);
        tok.insert(slow, 0.0);

        let mut src_emitted: HashMap<u32, u64> = HashMap::new();
        let mut rep_emitted: HashMap<u32, u64> = HashMap::new();

        for _ in 0..ticks {
            // Refill both buckets at BtlBw_i (one tick of link), capped at burst.
            for &id in &[fast, slow] {
                let t = tok.get_mut(&id).unwrap();
                *t = (*t + btlbw(id)).min(burst);
            }
            // SOURCE first (has priority). DAPS offers source on the slow path
            // (future-offset placement); the source pacer spills to fast when the
            // slow bucket is dry — the production source gate (may go negative).
            for _ in 0..3 {
                let cand = slow;
                let pick = if cand != fast && tok.get(&cand).is_some_and(|&t| t < 1.0) {
                    fast
                } else {
                    cand
                };
                *tok.get_mut(&pick).unwrap() -= 1.0;
                *src_emitted.entry(pick).or_insert(0) += 1;
            }
            // REPAIR next: offer a FLOOD (8/tick, far above capacity) on BOTH
            // candidates — the gate must HOLD once the buckets dry.
            for cand in [slow, fast, slow, fast, slow, fast, slow, fast] {
                if let Some(p) = paced_repair_decision(&mut tok, cand, fast) {
                    // (a) repair only ever consumes a bucket that was ≥ 1, so the
                    //     bucket is never negative AFTER a repair emission — repair
                    //     can never overdraw a path past BtlBw_i (any negative
                    //     excursion is SOURCE, which has priority).
                    assert!(
                        *tok.get(&p).unwrap() >= -1e-9,
                        "repair must never drive a per-path bucket negative (path {p})"
                    );
                    *rep_emitted.entry(p).or_insert(0) += 1;
                }
            }
        }

        // (b) TOTAL per-path emission (source + repair) ≤ BtlBw_i·ticks + burst.
        for &id in &[fast, slow] {
            let total = src_emitted.get(&id).copied().unwrap_or(0)
                + rep_emitted.get(&id).copied().unwrap_or(0);
            let ceiling = (btlbw(id) * ticks as f64 + burst).ceil() as u64;
            assert!(
                total <= ceiling,
                "path {id}: total emission {total} must be ≤ BtlBw_i·ticks+burst {ceiling}"
            );
        }
        // (c) The slow path carries far less than an unpaced dump would place on
        //     it (4 slow-candidate offers/tick = 4000), proving pacing bounds it.
        let slow_total = src_emitted.get(&slow).copied().unwrap_or(0)
            + rep_emitted.get(&slow).copied().unwrap_or(0);
        let unpaced_slow_offer = 4 * ticks;
        assert!(
            (slow_total as f64) < 0.6 * unpaced_slow_offer as f64,
            "pacing must cut slow-path emission ({slow_total}) far below an unpaced \
             dump ({unpaced_slow_offer})"
        );
        // Sanity: the fast path still carries the bulk (aggregation preserved).
        let fast_total = src_emitted.get(&fast).copied().unwrap_or(0)
            + rep_emitted.get(&fast).copied().unwrap_or(0);
        assert!(fast_total > slow_total, "fast path carries the bulk of the load");
    }

    /// feat/pace-all-traffic: the HOLD property that bounds the FAST path too.
    /// When BOTH the candidate and the fast path are dry, the gate returns None
    /// (hold) rather than spilling into a negative bucket; a warmed candidate
    /// with a token emits there; a dry candidate spills to a funded fast path;
    /// an un-warmed path (no bucket) is transparent (emits, consumes nothing).
    #[test]
    fn pace_all_traffic_holds_when_both_paths_dry() {
        use std::collections::HashMap;
        let (fast, slow) = (0u32, 1u32);

        // Both dry ⇒ HOLD (this is what bounds the fast path).
        let mut tok: HashMap<u32, f64> = HashMap::from([(fast, 0.5), (slow, 0.5)]);
        assert_eq!(paced_repair_decision(&mut tok, slow, fast), None);
        assert_eq!(paced_repair_decision(&mut tok, fast, fast), None);

        // Slow dry, fast funded ⇒ spill to fast, consume a fast token.
        let mut tok: HashMap<u32, f64> = HashMap::from([(fast, 3.0), (slow, 0.0)]);
        assert_eq!(paced_repair_decision(&mut tok, slow, fast), Some(fast));
        assert!((tok[&fast] - 2.0).abs() < 1e-9, "one fast token consumed");
        assert!((tok[&slow] - 0.0).abs() < 1e-9, "slow bucket untouched");

        // Candidate funded ⇒ emit there, consume its token.
        let mut tok: HashMap<u32, f64> = HashMap::from([(fast, 3.0), (slow, 2.0)]);
        assert_eq!(paced_repair_decision(&mut tok, slow, fast), Some(slow));
        assert!((tok[&slow] - 1.0).abs() < 1e-9, "one slow token consumed");

        // Un-warmed candidate (no bucket) ⇒ transparent: emit, consume nothing.
        let mut tok: HashMap<u32, f64> = HashMap::from([(fast, 3.0)]);
        assert_eq!(paced_repair_decision(&mut tok, slow, fast), Some(slow));
        assert!(!tok.contains_key(&slow), "un-warmed path stays un-metered");
        assert!((tok[&fast] - 3.0).abs() < 1e-9, "fast untouched when candidate transparent");
    }

    /// feat/source-backpressure: the SOURCE admission peek DEFERS (does not
    /// admit) when neither the DAPS candidate nor the fast spill path has a
    /// funded bucket — the source analogue of the repair HOLD, but as
    /// backpressure (pause the TUN read) not discard, since source is payload.
    #[test]
    fn source_backpressure_defers_when_both_paths_dry() {
        use std::collections::HashMap;
        let (fast, slow) = (0u32, 1u32);

        // Both dry ⇒ DEFER (do not admit): the source would otherwise spill to
        // the fast path and drive its bucket negative (the residual burst).
        let tok: HashMap<u32, f64> = HashMap::from([(fast, 0.5), (slow, 0.5)]);
        assert!(!source_pace_admit(&tok, slow, fast), "both dry ⇒ defer source");
        assert!(!source_pace_admit(&tok, fast, fast), "both dry (cand=fast) ⇒ defer");

        // Candidate funded ⇒ admit (it will emit on the candidate).
        let tok: HashMap<u32, f64> = HashMap::from([(fast, 0.0), (slow, 3.0)]);
        assert!(source_pace_admit(&tok, slow, fast), "funded candidate ⇒ admit");

        // Candidate dry but fast funded ⇒ admit (source spills to the fast path,
        // landing on a bucket that IS ≥ 1 — so no negative excursion).
        let tok: HashMap<u32, f64> = HashMap::from([(fast, 3.0), (slow, 0.0)]);
        assert!(source_pace_admit(&tok, slow, fast), "dry candidate + funded fast ⇒ admit");

        // Un-warmed candidate (no bucket) ⇒ transparent: admit.
        let tok: HashMap<u32, f64> = HashMap::from([(fast, 0.0)]);
        assert!(source_pace_admit(&tok, slow, fast), "un-warmed candidate ⇒ transparent admit");
    }

    /// feat/source-backpressure: with source DEFERRED (not spilled) when both
    /// buckets are dry, TOTAL per-path emission (source + repair) never drives a
    /// bucket negative and stays ≤ BtlBw_i·ticks + one burst on EVERY path —
    /// the fast-path bucket in particular is never bursted negative by source.
    /// Contrast the pace-all baseline (source spills unconditionally), which
    /// DOES drive the fast bucket negative under the same offer.
    #[test]
    fn source_backpressure_bounds_total_per_path_emission_no_negative_bucket() {
        use std::collections::HashMap;
        let (fast, slow) = (0u32, 1u32);
        let btlbw = |id: u32| if id == fast { 20.0f64 } else { 2.0f64 };
        let burst = 8.0;
        let ticks = 1000u64;

        // Offer MORE source than the aggregate link can carry (both paths), so
        // the gate is forced to defer — the stress case for the fast bucket.
        let src_offer_per_tick = 30u32; // > 20+2 aggregate BtlBw

        let mut tok: HashMap<u32, f64> = HashMap::from([(fast, 0.0), (slow, 0.0)]);
        let mut min_bucket = f64::INFINITY;
        let mut src_emitted: HashMap<u32, u64> = HashMap::new();
        let mut rep_emitted: HashMap<u32, u64> = HashMap::new();
        let mut deferred = 0u64;

        for _ in 0..ticks {
            for &id in &[fast, slow] {
                let t = tok.get_mut(&id).unwrap();
                *t = (*t + btlbw(id)).min(burst);
            }
            // SOURCE with backpressure: DAPS offers each source on the slow path
            // (future-offset); admit only if a bucket is funded, else DEFER.
            for _ in 0..src_offer_per_tick {
                let cand = slow;
                if !source_pace_admit(&tok, cand, fast) {
                    deferred += 1;
                    continue; // backpressure — the TUN read pauses
                }
                // Admitted: emit on the candidate if funded, else spill to fast
                // (guaranteed funded by the admission peek) — the production
                // source-placement gate, now landing only on a funded bucket.
                let pick = if tok.get(&cand).map_or(true, |&t| t >= 1.0) { cand } else { fast };
                *tok.get_mut(&pick).unwrap() -= 1.0;
                min_bucket = min_bucket.min(tok[&pick]);
                *src_emitted.entry(pick).or_insert(0) += 1;
            }
            // REPAIR flood on the leftover capacity (held when dry).
            for cand in [slow, fast, slow, fast] {
                if let Some(p) = paced_repair_decision(&mut tok, cand, fast) {
                    min_bucket = min_bucket.min(tok[&p]);
                    *rep_emitted.entry(p).or_insert(0) += 1;
                }
            }
        }

        // (a) No bucket ever went negative — source is deferred, never bursted.
        assert!(
            min_bucket >= -1e-9,
            "source backpressure must never drive a bucket negative (min {min_bucket})"
        );
        // (b) The gate actually engaged (the offer exceeded capacity).
        assert!(deferred > 0, "the over-offer must have forced deferrals");
        // (c) TOTAL per-path emission ≤ BtlBw_i·ticks + burst on EVERY path.
        for &id in &[fast, slow] {
            let total = src_emitted.get(&id).copied().unwrap_or(0)
                + rep_emitted.get(&id).copied().unwrap_or(0);
            let ceiling = (btlbw(id) * ticks as f64 + burst).ceil() as u64;
            assert!(
                total <= ceiling,
                "path {id}: total emission {total} must be ≤ BtlBw_i·ticks+burst {ceiling}"
            );
        }

        // Contrast: the pace-all BASELINE spills source unconditionally and DOES
        // drive the fast bucket negative under the same over-offer (the residual
        // this work closes).
        let mut tok2: HashMap<u32, f64> = HashMap::from([(fast, 0.0), (slow, 0.0)]);
        let mut min_bucket_baseline = f64::INFINITY;
        for _ in 0..ticks {
            for &id in &[fast, slow] {
                let t = tok2.get_mut(&id).unwrap();
                *t = (*t + btlbw(id)).min(burst);
            }
            for _ in 0..src_offer_per_tick {
                let cand = slow;
                // Baseline source gate: spill to fast when the candidate is dry,
                // then decrement unconditionally (may go negative).
                let pick = if tok2.get(&cand).map_or(false, |&t| t < 1.0) { fast } else { cand };
                *tok2.get_mut(&pick).unwrap() -= 1.0;
                min_bucket_baseline = min_bucket_baseline.min(tok2[&pick]);
            }
        }
        assert!(
            min_bucket_baseline < -1.0,
            "baseline (spill) MUST drive a bucket negative (min {min_bucket_baseline}) — \
             the residual that backpressure closes"
        );
    }
}
