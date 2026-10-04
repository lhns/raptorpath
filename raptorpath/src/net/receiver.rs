//! The engine's receiver task: one datagram/stream message in, decode →
//! reassemble → in-order (or unordered) delivery to the TUN, plus every
//! control message the peer sends back.
//!
//! One `loop` around a `tokio::select!` over the message channel, the in-order
//! hold deadline, the deficit deadline and shutdown, preceded by the
//! receiver-local state. `run_impl` keeps setup and spawns; the captures are
//! cloned in `run_impl` and passed at the spawn
//! (`tokio::spawn(receiver::run_receiver(…))` builds the future on the
//! caller's thread; the body runs when the runtime first polls it).
//!
//! Ordering constraints:
//!   * the `rdiag` idle stopwatch brackets exactly the `select!`;
//!   * every `recv_scheduler` / `recv_fec` lock is taken and released within
//!     one statement's scope;
//!   * both fatal exits (the two TUN-inject failures on the window delivery
//!     paths) `break 'recv` out of the loop, so they fall through the one
//!     exit-flush site at the loop's end and then the function ends.
//!
//! Exit flush: the `[SUCC]`/`[ETA]`/`[LAT]`/`[LATE]`/`[REQ]`/`[RANK]` gauges
//! live in one `RecvDiagBlock` (`net/recv_block.rs`) whose destructor prints
//! the block a last time marked `final=1`, so an object that completes in
//! under a second still reports; the loop's exit site calls the same
//! exactly-once flush first, while the decoder is still in scope for a fresh
//! `[RANK]` probe. Diagnostic only.
//!
//! Not covered here: `spawn_receiver_for_path` (the per-path datagram/stream
//! readers that feed this task's channel) and the control fast-path task
//! (`net/tasks/control_fastpath.rs`).
//!
//! The receiver's locals stay locals: no other phase reads them, so a
//! `ReceiverState` struct would add a `st.` prefix everywhere and buy nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bytes::Bytes;
use dashmap::DashMap;
use tracing::{debug, error, info, warn};

use super::control_msg::{ControlCtx, handle_control_message};
use super::delivery::{WindowDelivery, delivered_prefix};
use super::reorder::ReorderBuffer;
use super::{
    CopaFeed, DerivedRoundEcho,
    GAP_ACK_MIN_INTERVAL, GEN_PIPE_MAX_GENS, LOOP_WAKE_US, PathBatchTracker, REPORT_INTERVAL,
    collect_gen_deficits, create_window_decoder, extract_window_packets,
    hole_nack_refresh_floored, hole_refresh, horizon_gate_deficits, now_us, received_sack_ranges,
    shed_armed, shed_recv_budget_ok, shed_recv_hold, stall_threshold_us, window_ack_emission,
};
use crate::control::FecRateController;
use crate::fec::{FecBackend, WindowDecoder};
use crate::monitor::stats::SharedStats;
use crate::scheduler::Scheduler;
use crate::transport::{ControlMessage, QuicTransport, WireMessage};

/// The engine's receiver task. Consumes `(path_id, WireMessage)` from the
/// shared inbound channel until the channel closes or shutdown fires.
///
/// Parameters are the task's captures, in the order `run_impl` declares
/// them.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_receiver(
    mut recv_shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    mut msg_rx: tokio::sync::mpsc::Receiver<(u32, WireMessage)>,
    recv_copa_feed: Option<Arc<CopaFeed>>,
    recv_tun_tx: tokio::sync::mpsc::Sender<Bytes>,
    recv_scheduler: Arc<parking_lot::Mutex<Scheduler>>,
    recv_fec: Arc<parking_lot::Mutex<FecRateController>>,
    recv_fec_backend: FecBackend,
    recv_transport: Arc<QuicTransport>,
    recv_path_tracking: Arc<DashMap<u32, PathBatchTracker>>,
    recv_stats: Arc<SharedStats>,
    recv_symbol_size: u16,
    recv_window_reliable: bool,
    recv_window_ooo: bool,
    recv_win_cap: u64,
    recv_window_ack: Arc<AtomicU64>,
    recv_window_generation: bool,
    recv_deficit_tx: tokio::sync::mpsc::Sender<Vec<(u64, u32)>>,
    recv_nack_tx: Option<tokio::sync::mpsc::Sender<(super::FireCause, u32, Vec<(u64, u64)>)>>,
    recv_sack_tx: Option<tokio::sync::mpsc::Sender<super::SackReport>>,
    // Request arms (A)/(B) (paper §7.6). `Some(..)` iff a request arm is live
    // on a plain reliable window; the local sender's own consumer sits on the
    // far end. `None` by default, where an arriving `RepairRequest` is counted
    // and dropped.
    recv_request_tx: Option<tokio::sync::mpsc::Sender<super::RepairRequestBatch>>,
    // Arm (A): the receiver requests at `l >= l*_recv`, and the per-seq
    // SACK->gap producer is suppressed by the collision seam. Resolved in
    // `run_impl` beside that seam so the two can never disagree.
    recv_request_law: bool,
    // Arm (B): the request carries the derived `m` instead of `m = 1`.
    // Composes with (A); alone it is the vocabulary-only wiring test.
    recv_rank_feedback: bool,
    reasm_bdp_on: bool,
    ack_merge_recv: bool,
    recv_diag_on: bool,
    rdiag_probe: tokio::sync::mpsc::WeakSender<(u32, WireMessage)>,
    recv_gates: crate::gates::RuntimeGates,
    // `config.reorder_timeout_ms` / `config.reorder_max_size`.
    recv_reorder_timeout_ms: u64,
    recv_reorder_max_size: usize,
    // The contract's dial position (`config.protocol_hint`), the same value
    // `SenderPolicy::resolve` reads.
    recv_protocol_hint: crate::control::fec_rate::ProtocolHint,
) {
    // Window decoder: created once, long-lived (codec pinned at startup —
    // never rebuilt).
    let mut window_decoder: Box<dyn WindowDecoder> =
        create_window_decoder(recv_fec_backend, recv_symbol_size, recv_window_generation);
    // Whether the sender packs multiple packets per symbol (set via WindowStart)
    let mut window_packed: bool = false;
    // The cumulative point for the window ACK (wire v9 `next_expected`): the
    // COUNT of the contiguous delivered prefix -- every seq below it has
    // been handed to the consumer; 0 = nothing delivered. (Through v8 this
    // was `highest_delivered_seq`, where 0 meant both "nothing" and "seq 0".)
    let mut next_expected: u64 = 0;
    // The `next_expected` we last advertised in a WindowAck (dedupe for ack
    // sends; the shared window_ack_seq atomic carries the PEER's acks for the
    // local sender and must not be conflated with this). 0 = nothing yet, so
    // a delivered seq 0 alone (`next_expected = 1`) is advertised.
    let mut last_advertised_ack: u64 = 0;
    // Reorder buffer for window mode — delivers packets in sequence order.
    // Reliable policy (ρ = 1): holes are held until recovered, never
    // force-delivered past (in-order delivery is the reliability contract at
    // the receiver).
    //
    // Ordering is a per-stream delivery policy, independent of the codec
    // triangle. Two limits of the reorder horizon H:
    //   - in-order (H = ∞): hold at holes → the reorder buffer.
    //   - unordered (H = 0): emit each decoded unit the instant it decodes →
    //     no reorder buffer at all. Correct and lowest-latency for any
    //     consumer that does not need byte-stream order (objects reassembled
    //     by offset, datagrams, RPC/telemetry).
    // The in-order received prefix (for retention/ack) is then tracked by a
    // lightweight frontier over `received_seqs`.
    let mut reorder_buf = if recv_window_ooo {
        None
    } else if recv_window_reliable {
        Some(ReorderBuffer::new_reliable())
    } else if recv_reorder_timeout_ms > 0 {
        Some(ReorderBuffer::new(recv_reorder_timeout_ms, recv_reorder_max_size))
    } else {
        None
    };
    // Hand-off of window symbols to the consumer channel (`net/delivery.rs`):
    // ρ = 1 holds on a full channel (never drops), ρ < 1 drops.
    let mut window_delivery = WindowDelivery::new(recv_window_reliable);
    // δ-honest overload shedding, receiver arm (paper §5.6): the in-order hold
    // for the window EVICT path becomes the δ-derived H = b·SRTT (the reorder
    // timeout is the δ dial; b(Realtime) = ½ — this path exists only for the
    // Realtime hint) instead of the bulk-shaped 4×SRTT ∈ [60, 300] ms clamp,
    // while the give-up budget holds: holes given up ≤ ε̂_recv × frontier
    // (give-up is holes-only, so the realized fraction stays in the
    // FEC-residual class). Budget spent ⇒ the hold reverts to the 4×SRTT clamp
    // (serialize: ρ wins over δ). Armed only on the EVICT in-order path under
    // RWM_UNIFIED (the ρ = 1 reliable buffer never gives up);
    // `RWM_UNIFIED_SHED=0` = the serializing arm.
    let recv_shed_on = !recv_window_reliable
        && !recv_window_ooo
        && reorder_buf.is_some()
        && shed_armed(recv_gates.unified, false, recv_gates.unified_shed);
    if recv_shed_on {
        // Mechanism-liveness echo (`docs/measurement-discipline.md` rule 1).
        info!(
            "unified overload shedding ACTIVE at receiver (RWM_UNIFIED_SHED: in-order hold = delta-derived b*SRTT within the eps-class give-up budget)"
        );
    }
    // Holes given up (seqs the in-order frontier passed undelivered) and
    // the diag throttle for the [SHED-R] gauge.
    let mut recv_shed_holes: u64 = 0;
    let mut recv_shed_budget_open = true;
    let recv_shed_diag = recv_gates.diag;
    // `RWM_DERIVED_SWEEP` (default off): the stalled-hole refresh cadence on
    // the derived round.
    let recv_derived_sweep = recv_gates.derived_sweep;
    // The hole-refresh clamp band's floor (`RWM_REFRESH_FLOOR_US`), resolved
    // once. Absent ⇒ `HOLE_NACK_REFRESH_MIN` ⇒ `hole_nack_refresh` unchanged.
    // Echoed on `[GATES]` at both endpoints.
    let recv_refresh_floor = recv_gates
        .refresh_floor_us
        .map_or(crate::net::HOLE_NACK_REFRESH_MIN, Duration::from_micros);
    // `[QCLK]` at the receiver — the hole-refresh cadence this site realizes.
    // The harness SIGKILLs the server, so a `Drop` never reaches a server log;
    // this gauge is therefore also emitted on the same 1 s cadence, last line
    // wins.
    let mut recv_qclk_echo = crate::net::QuantileClockGauge::new("receiver");
    let mut qclk_report_at = Instant::now();
    // The receiver site's one-shot mechanism-liveness echo (ACTIVE +
    // DIVERGED). Observation only; emitted on the armed arm alone.
    let mut recv_derived_echo = DerivedRoundEcho::default();
    // `[RACK]` / `[RFA]` fire accounting at the receiver site (paper §7.1).
    let mut recv_rack_echo = crate::net::RackClockGauge::new();
    // `[RFA]` is a PLAIN-WINDOW instrument (the same configuration scope the
    // sender's `fa=` has — `recv_nack_tx` is None under generation). The line
    // echoes which machine it measured so no row is ever read out of scope.
    recv_rack_echo.set_recv_generation(recv_window_generation);
    // `[RFA]` cadence — see the readout site. Cumulative, 1 s, last line wins.
    let mut rfa_report_at = Instant::now();
    // `[SUCC]`: the same-flow successor-arrival distribution — per hole,
    // detection → resolution, in three disjoint outcomes. See `net/succ.rs`
    // for the measurand, the origin event, and why it is detection rather
    // than hole creation. Always fed, on every arm: the datum must exist
    // wherever `gap_data` fires do. Only the raw dump is gated.
    //
    // `[ETA]` (`net/eta.rs`): per path, how late each arrival was against the
    // sender's own prediction, relative to that path's running best. Always
    // fed — including the `eta_rel = 0` sentinel, which is counted as the
    // bind fraction rather than filtered. Read-only; nothing branches on it.
    //
    // This gauge and the four below it (`[LAT]`, `[LATE]`, `[RANK]`,
    // `[SUCC]`) plus the `[REQ]` counters live in one `RecvDiagBlock`
    // (`net/recv_block.rs`), whose destructor prints the block a last time,
    // marked `final=1`, when the task ends. They are fed through
    // `blk.<gauge>`.
    let recv_eta = crate::net::eta::RecvEta::default();
    // `[LAT]` (`net/lat.rs`): delivered latency, decomposed. Per delivered
    // source symbol on the in-order window path: `A_x` (queue + sender dwell
    // above this path's floor), the reorder wait `R` classed by the `[SUCC]`
    // resolution record of the hole that released it, and the repair wait
    // `P`. Always fed; read-only.
    let recv_lat = crate::net::lat::LatGauge::default();
    // `[LATE]` / `[RANK]` (`net/late.rs`): the receiver's own seat. `[LATE]`
    // brackets every hole's lateness, classes it, and computes the request
    // law's hypothetical threshold `l*_recv` (paper §7.6) read-only from the
    // receiver's own heal density, with both its clamps' bind fractions.
    // `[RANK]` re-reads `frontier_probe` as (holes, pivots, deficit,
    // tail_overcount) unconditionally. Neither decides anything.
    // The contract's price at this seat: `delta_price` is the one surface a
    // hint names a δ on, and `RWM_DELTA` stands between the presets on it.
    // It enters `[LATE]` through the request bar and nowhere else.
    let recv_delta = crate::net::delta_price(recv_protocol_hint);
    let recv_late = crate::net::late::LateGauge::new(recv_delta);
    // `[RANK]`: the highest seq seen at the previous readout. The span above
    // it arrived within this interval and is legitimately still in flight
    // rather than missing — the `tail_overcount` correction, reported and
    // never silently subtracted.
    let mut rank_prev_seen: u64 = 0;
    // `[LATE]`: the observed knee `H`. An arrival gap taken while the
    // frontier is frozen is the store-cap headroom running out — the
    // `[WIDLE]` measurand, read here ungated so `H` exists on every
    // diagnosed run.
    let mut late_last_arrival_us: u64 = 0;
    // `[LATE]`: whether the 2 ms `GAP_ACK_MIN_INTERVAL` floor, rather than
    // the lateness, decided when a hole could be reported at all — the
    // `sampler_bind` fraction, `blk.late_sampler_bound`.
    // `[REQ]` (paper §7.6 arms (A)/(B)): what this receiver asked for.
    // Reports built and put on the wire, spans in them, the widest `m` and the
    // acting threshold at the last build. Cumulative, printed on the `[LATE]`
    // cadence, last line wins — on both arms, so `on=0 sent=0` is the
    // control's own reading rather than a missing line. The counters are
    // `blk.req_*`.
    // `[RFA] late_after_aban`'s exact denominator: the seqs the in-order
    // frontier moved past without delivering — read off `[SUCC]`'s own
    // abandonment sweep. A later source copy for one of these is the EVICT
    // seat's repair waste; a copy for a seq that was delivered is an ordinary
    // duplicate. Bounded (a declared resource bound, oldest-first).
    const ABANDONED_TRACK_MAX: usize = 65_536;
    let mut abandoned_seqs: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
    let recv_succ = crate::net::succ::SuccGauge::new(
        recv_window_generation,
        recv_gates.succ_dump,
        crate::net::succ::dump_max(),
    );
    // The block. Gate = the cadence site's own (`RWM_DIAG || RWM_FDIAG`),
    // captured once so the destructor keys on the same predicate the cadence
    // does.
    let mut blk = crate::net::recv_block::RecvDiagBlock::new(
        recv_gates.diag || recv_gates.fdiag,
        recv_succ,
        recv_eta,
        recv_lat,
        recv_late,
        recv_request_law,
        recv_rank_feedback,
    );
    let mut succ_report_at = Instant::now();
    let mut recv_shed_diag_at = Instant::now();
    // Unordered delivery: next in-order seq not yet received (the frontier).
    // Walks `received_seqs` to drive the cumulative WindowAck (retention
    // pruning) while delivery itself is unordered.
    let mut ooo_frontier: u64 = 0;
    // Reliable mode: when delivery is stalled on a hole, periodically
    // re-advertise the gap (SACK-bearing WindowAck) — acks are
    // best-effort datagrams, and a lost gap report must not leave
    // recovery to the sender's single-seq tail sweep alone.
    let mut last_hole_nack_at = Instant::now();
    // Track received seqs for WindowNack gap reporting
    let mut received_seqs: BTreeSet<u64> = BTreeSet::new();
    // Wire v9 `received_above`: |received_seqs ∩ [next_expected, ∞)|,
    // maintained incrementally (every WindowAck carries it, including one per
    // data message under the ack merge).
    let mut recv_above = crate::net::ReceivedAbove::new();
    // `RWM_REASM_BDP` occupancy probe: the maximum reassembly buffer
    // occupancy observed = received-but-not-yet-delivered symbols held behind
    // the in-order frontier. The reliability invariant bounds it — it must
    // stay ~BDP (the sender's outstanding cap), never grow to the whole
    // object. `reasm_max_pending` = peak held symbols; `reasm_max_span` = peak
    // (highest_seen − frontier) seq gap. Reported via `[REASM]`.
    let mut reasm_max_pending: usize = 0;
    let mut reasm_max_span: u64 = 0;
    let mut reasm_last_report = Instant::now();
    // Ack-merge control-datagram density gauge (RWM_DIAG only). `[CTLD]
    // p<id> tx=<n> rx=<n>` = quinn's own `frame_tx.datagram` /
    // `frame_rx.datagram` for the path, read at the receiver: a window-mode
    // receiver sends nothing but control datagrams, so `tx` is the
    // control-frame count. Frames, not packets: the ack-direction packet
    // counters are dominated by quinn's own transport-level ACK cadence, and
    // control datagrams ride coalesced inside those packets, so merging two
    // datagram frames changes the frame count, not the packet count.
    let mut ctld_last_report = Instant::now();
    // Slow-path conversion gauges (RWM_DIAG only): the receiver-side view of
    // conversion, per arrival path:
    //  * first[p]  — seqs whose first copy this path delivered (source
    //    arrival or repair-decode output) = the path's real conversion.
    //  * dup[p]    — source arrivals for an already-received seq =
    //    displacement-only deliveries (a cross-path retransmit or the
    //    outrun original got there first).
    //  * lead[p]   — Σ (seq − in-order frontier) at first-copy arrival, in
    //    symbols (lead 0 = the stream was already waiting on this symbol).
    //  * unb_n/ms[p] — frontier unblocks credited to this path: an arrival
    //    that advanced the stalled (≥ 5 ms) in-order frontier, with the stall
    //    time it ended.
    let c8r_on = recv_gates.diag;
    let mut c8r_first: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut c8r_dup: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut c8r_lead: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut c8r_unb_n: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut c8r_unb_ms: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut c8r_last_adv = Instant::now();
    let mut c8r_last_print = Instant::now();
    // Generation-deficit feedback, receiver arm (paper §5.8).
    // `gen_widths[anchor]` = the generation's K_g, learned from the wire
    // header (`window_count`) of any coded symbol for that anchor.
    // Deficit_g = K_g − rank_in(anchor, K_g). `last_deficit_send` paces the
    // reports to ~once per SRTT (plus an immediate report on decode progress).
    let mut gen_widths: BTreeMap<u64, u16> = BTreeMap::new();
    // Generation size G (mirrors the sender's RWM_GEN default). Lets the
    // receiver seed a provably-full generation's width (G) from the primary
    // seqs alone — see the seeding in `send_gen_deficits`. This closes the
    // small-G frontier-advance deadlock: a generation whose entire proactive
    // repair budget was lost on the wire would otherwise never enter
    // `gen_widths` (learned only from repair headers), so the receiver would
    // report zero deficit for it while the in-order frontier wedged on its
    // hole. At large G the whole ceil(G·r) budget is never fully lost.
    let recv_gen_size: u64 = recv_gates.gen_size as u64;
    // Number of outstanding generations whose deficit is reported (and
    // anti-wedge-seeded) per round. Default 6 (frontier-first serial tail);
    // `RWM_REPORT_GENS` lifts it to cover the whole in-flight range so every
    // hole is repaired in one round trip. Clamped to the wire cap
    // (MAX_ACK_IDS = 2000). Under the derived-depth pipeline (gen_pipe) the
    // whole M*-generation in-flight range must be reportable in one round, or
    // a frontier-first report re-serializes the deeper pipeline's recovery.
    let report_gens: usize = recv_gates
        .report_gens
        .unwrap_or(if recv_gates.gen_pipe { GEN_PIPE_MAX_GENS + 1 } else { 6 })
        .clamp(1, 2000);
    // Repair-coverage horizon (`RWM_REPAIR_WAIT`). Base wait, in
    // milliseconds, before a frontier hole's deficit may fire a reactive
    // NACK — the time for the in-flight proactive repair covering it to
    // arrive + decode (~a generation-span at the send rate, not an RTT).
    // Unset / 0 = report immediately. Made δ-aware at use: clamped to
    // ≤ ½·SRTT so low-RTT / latency-tight paths never over-wait, and it can
    // never exceed the round trip it is trying to save.
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
    // One past the highest seq seen (0 = nothing seen): the exclusive twin of
    // `highest_seen_seq`, so "a seq above the cumulative point has been
    // seen" is `seen_end > next_expected` with no seq-0 ambiguity.
    let mut seen_end: u64 = 0;
    let mut last_nack_time = Instant::now();
    // Dupack analog: highest_seen at the last gap-advertising ack, and when
    // it was sent (rate limit) — see GAP_ACK_MIN_INTERVAL.
    let mut last_gap_ack_seen: u64 = 0;
    let mut last_gap_ack_time = Instant::now() - GAP_ACK_MIN_INTERVAL;
    // Controller feedback tracking for window mode (repairs fed / useful).
    let mut last_pi_repairs_fed: u64 = 0;
    let mut last_pi_repairs_useful: u64 = 0;

    // ── Proactive-frontier diagnosis (RWM_FDIAG) ──────────────────────
    // When the in-order frontier stalls on a hole p, is there already
    // buffered proactive repair covering p (→ the receiver should decode
    // now), or is it absent (→ the hole waits on a reactive ARQ source
    // retransmit)? For each stall we record how long the frontier sat on p
    // and how p was resolved: DECODE (a repair solved it, no round trip) vs
    // SOURCE (a retransmitted source symbol arrived, a ~1-RTT ARQ round).
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
    // already present when the stall began (present-but-waiting-for-rank)
    // vs the covering repair only arrived mid-stall.
    let mut fdiag_present_at_stall: u64 = 0;
    // Raw decoder-call wall-time: `fdiag_addsym_us` accumulates the time
    // spent inside `win_dec.add_symbol()` (GF(256) GE compute) across the
    // whole transfer; `fdiag_addsym_n` is the call count. Compared against
    // the per-hole resolution wall-time (fdiag_decode_us, which spans
    // hole-armed → frontier-passes and includes symbol-arrival waiting), this
    // separates compute from waiting-for-rank.
    let mut fdiag_addsym_us: u64 = 0;
    let mut fdiag_addsym_n: u64 = 0;
    // Worst single add_symbol call in the current FDIAG report interval (a
    // mean hides a per-arrival cost blowup).
    let mut fdiag_addsym_max_us: u64 = 0;

    // ── Receiver wedge forensics (RWM_DIAG) ──────────────────────────────
    // Names the mechanism when the in-order frontier freezes while the
    // sender keeps retransmitting the blocker. Reported from the reliable
    // hole-refresh timer arm (which fires every 25–100 ms during any stall),
    // once per second after the frontier has been frozen > 1 s:
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
    // Receiver inter-arrival gap gauge (RWM_DIAG only): cumulative time in
    // Data arrival gaps ≥ 3 ms. The wire-truth idle gauge: a loss burst
    // pauses arrivals for ≪ 3 ms at these packet rates, so stall-class gaps
    // here are genuine wire idle (sender/CC-caused), not loss shadows.
    // Printed once per second as
    // `[WIDLE] idle=<cum ms>/<n>/mx<max ms> arr=<cum Data messages>`; the
    // counters are cumulative, so the last line is the run's total.
    const WIDLE_GAP_MIN_US: u64 = 3_000;
    let mut widle_last_arrival_us: u64 = 0;
    let mut widle_us: u64 = 0;
    let mut widle_n: u64 = 0;
    let mut widle_max_us: u64 = 0;
    let mut widle_arrivals: u64 = 0;
    let mut widle_last_print_us: u64 = 0;
    // The receiver twin of the derived stall gauge (`RWM_SIDLE_DERIVED`,
    // DIAG-only). Same law (`stall_threshold_us`) over the Data-arrival
    // event stream: the fixed 3 ms is 3 × an assumed nominal inter-arrival
    // interval, and the measured one is what the law substitutes. `idle=` is
    // printed unchanged; `idle2=` is added beside it. Recomputed once per 1 s
    // print window from that window's own arrival count.
    let widle_derived = crate::scheduler::sidle_derived_active();
    let mut widle_evt_us: u64 = LOOP_WAKE_US;
    let mut widle_thr_us: u64 = stall_threshold_us(LOOP_WAKE_US);
    let mut widle_evt_n: u64 = 0;
    let mut widle2_us: u64 = 0;
    let mut widle2_n: u64 = 0;
    let mut widle2_max_us: u64 = 0;

    // Generation-deficit report (paper §5.8). Compute each frontier
    // generation's residual deficit from the decoder's current rank and send
    // it to the sender. `$force` sends even an empty vector (used on decode
    // progress so the sender clears wants for just-completed generations, and
    // on the periodic timer so a stalled/silent sender is re-pulled). Shared
    // by the data arm (progress) and the timer arm (liveness), so a sender
    // that has gone quiet keeps being told the true deficit until every
    // generation decodes.
    macro_rules! send_gen_deficits {
        ($dec:expr, $force:expr) => {{
            if recv_window_generation {
                // Anti-wedge seeding (small-G frontier-advance deadlock). Seed
                // the width (= G) of every generation that is provably full —
                // one whose end lies at or below the highest seq seen, so its
                // G source symbols certainly exist — starting at the frontier
                // generation (where `ooo_frontier` is stuck on a hole). The
                // deficit for such a generation is then computable from the
                // primary seqs alone (`rank_in`'s recovered-count branch),
                // without ever having seen a repair for it. Without this, a
                // generation whose entire ceil(G·r) proactive repair was lost
                // would never enter `gen_widths`, so the receiver would report
                // zero deficit while its hole wedged the frontier. The final
                // (possibly partial) generation is left to repair-header
                // learning (its true width is not yet known to be G). Seeds
                // the whole reportable range, so a generation whose proactive
                // budget was lost is NACKed in the same round as the frontier.
                let g_front = ooo_frontier / recv_gen_size;
                let g_top = highest_seen_seq / recv_gen_size;
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
                // Report every outstanding generation's deficit (up to
                // report_gens = the whole in-flight range) in one report, so
                // the sender repairs all holes in a single round trip rather
                // than frontier-first serially.
                let raw_deficits = collect_gen_deficits(&gen_widths, report_gens, |anchor, k| {
                    $dec.rank_in(anchor, k)
                });
                // Repair-coverage horizon: give the in-flight proactive repair
                // a chance to decode each hole before its deficit fires a
                // reactive NACK. δ-aware — clamped to ≤ ½·SRTT so low-RTT /
                // latency-tight paths never over-wait and the wait can never
                // exceed the round trip it would save.
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
                        crate::readout!(
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


    // ── Request arms (A)/(B): the receiver-seat repair request (paper §7.6)
    //
    // `REQUEST <=> l >= l*_recv`. `l*_recv` is `[LATE]`'s own threshold — the
    // acting name and the hypothetical name are one function, so the number
    // this arm requests at and the number the gauge prints can never be two
    // laws — and the hole set is `[SUCC]`'s own open map, the only place the
    // receiver's holes and their exposure instants exist.
    //
    // An unestimable `l*` acts as 0, and `[LATE]` still prints `-`: the two
    // states stay distinguishable on the line; the decision takes the
    // conservative limit — with no evidence at all, request immediately.
    // Requesting nothing until the first hole closes would make the arm's
    // bootstrap depend on the very backstop it is measuring.
    //
    // One path, not a broadcast. A request is a state snapshot of the
    // receiver's open holes and is re-built on every report; broadcasting it
    // would hand the sender N identical snapshots whose in-flight baselines
    // have not moved between them, and it would serve each one. A dropped
    // datagram costs one cadence, the same contract the SACK gap report runs
    // under.
    //
    // `$dec` is the window decoder (for `frontier_probe`'s rank deficit),
    // `$pid` the path to send on, `$cause` the `[FCAUSE]` class of the arm
    // that built it — the DATA arm or the timer-driven REFRESH arm. The tag
    // is carried, not acted on.
    macro_rules! send_repair_request {
        ($dec:expr, $pid:expr, $cause:expr) => {{
            // The arm, read off the two resolved predicates rather than off
            // `recv_request_tx`: that channel is the local sender's seat (it
            // rides in `ControlCtx` so an inbound request reaches this
            // process's own window sender), and the request this macro builds
            // goes on the wire to the peer.
            if recv_request_law || recv_rank_feedback {
                let (lstar_opt, _knee_bound) = blk.late.request_lateness();
                let lstar = lstar_opt.unwrap_or(0);
                blk.req_lstar_us = lstar;
                let now_rq = Instant::now();
                let holes = blk.succ.holes_at_least(
                    now_rq,
                    lstar,
                    crate::net::MAX_NACK_GAPS,
                );
                blk.req_holes_n = holes.len() as u64;
                if !holes.is_empty() {
                    // `A*` at the receiver — a declared resource bound. The
                    // sender's retained trailing span is not observable from
                    // here, so `m` is bounded by the two quantities that are:
                    // the receiver's own outstanding span (asking about seqs
                    // it has not seen is meaningless) and the retained window
                    // both ends share (`generate_repair_range` refuses beyond
                    // what the encoder holds). The refusal is still counted
                    // (`[REQS] wa1_none`).
                    let a_star = seen_end
                        .saturating_sub(next_expected)
                        .min(recv_win_cap);
                    let m = crate::net::late::request_m(
                        blk.late.pi0(),
                        recv_rank_feedback,
                        a_star,
                    );
                    let mut spans: Vec<(u64, u16, u32)> = Vec::new();
                    if m <= 1 {
                        // The copy — `(s, 1, 1)` is 'resend seq s' exactly, so
                        // arm (A) changes only the timing of the bytes on the
                        // wire.
                        for &h in &holes {
                            spans.push((h, 1, 1));
                        }
                    } else {
                        // The span. `k = holes - pivots` over `[a, a+m)` from
                        // `frontier_probe`. Spans do not overlap: a hole
                        // already covered by the previous span is already
                        // asked for.
                        let mut cursor = 0u64;
                        for &h in &holes {
                            if h < cursor {
                                continue;
                            }
                            let end = h.saturating_add(m as u64);
                            let (hn, pv) = $dec.frontier_probe(h, end.saturating_sub(1));
                            let k = hn.saturating_sub(pv).max(1).min(u32::MAX as u64) as u32;
                            spans.push((h, m, k));
                            cursor = end;
                        }
                    }
                    blk.req_m_max = blk.req_m_max.max(m as u64);
                    blk.req_span_n += spans.len() as u64;
                    let msg = ControlMessage::RepairRequest {
                        spans,
                        cause: $cause.as_u8(),
                    };
                    if recv_transport.send_control_datagram($pid, msg).is_ok() {
                        blk.req_sent += 1;
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

    'recv: loop {
        // Periodic generation-deficit report deadline: re-report the frontier
        // deficit ~once per SRTT even absent new data, so a sender that
        // emitted its budget and went quiet is always re-pulled and a lost
        // report is retransmitted. Only armed once a generation is known.
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

        // In-order hold drain timer (both modes): refresh the SRTT-adaptive
        // timeout and compute the oldest-entry expiry. Only when entries are
        // pending — the common case skips the locks. Window mode needs this
        // timer too: a drain that ran only on symbol arrival lets a hole
        // deadlock the whole tunnel (hole → no delivery advance → no
        // WindowAck → sender window full → no sends → no arrivals → no drain).
        let reorder_deadline: Option<tokio::time::Instant> = {
            let pending = if recv_window_ooo {
                // Unordered delivery holds nothing, but a hole in the
                // received prefix still needs the tail-recovery timer to
                // re-advertise the gap (SACK WindowAck) so the sender
                // retransmits it — the same reliability backstop the
                // in-order buffer's pending_count provided.
                seen_end > next_expected
            } else {
                reorder_buf.as_ref().is_some_and(|rb| rb.pending_count() > 0)
            };
            if pending {
                let (srtt, srtt_jitter_us, sigma_us) = {
                    let sched = recv_scheduler.lock();
                    let live: Vec<_> = sched
                        .live_paths()
                        .into_iter()
                        .filter_map(|pid| sched.path(pid))
                        .collect();
                    // Same path set for the clock and the DERIVED floor, so
                    // the two can never come from different paths. σ feeds
                    // only the `[QCLK]` readout.
                    (
                        live.iter().map(|p| p.srtt()).max(),
                        live.iter().map(|p| p.rtt_jitter_us()).max().unwrap_or(0),
                        live.iter().filter_map(|p| p.rtt_sigma_us()).max(),
                    )
                };
                let deadline = if recv_window_reliable {
                    // Reliable policy: the hole is never given up on — this
                    // timer instead re-advertises the gap (SACK WindowAck) at
                    // 2×SRTT cadence until recovered. Under
                    // `RWM_DERIVED_SWEEP` the cadence is the derived round (no
                    // ceiling); off ⇒ `hole_nack_refresh` unchanged.
                    let refresh = hole_refresh(
                        recv_derived_sweep,
                        srtt,
                        srtt_jitter_us,
                        recv_refresh_floor,
                    );
                    // Every arm, control included — see the sender's twin.
                    recv_qclk_echo.record(
                        refresh.as_micros() as u64,
                        srtt.map_or(0, |s| s.as_micros() as u64),
                        sigma_us,
                    );
                    if recv_derived_sweep {
                        if let Some(s) = srtt {
                            recv_derived_echo.observe(
                                "receiver-hole-refresh",
                                s.as_micros() as u64,
                                srtt_jitter_us,
                                refresh.as_micros() as u64,
                                hole_nack_refresh_floored(srtt, recv_refresh_floor).as_micros() as u64,
                            );
                        }
                    }
                    // ── Request arm (A): the deadline term (paper §7.6) ──
                    //
                    //     deadline = min( refresh ,  earliest A_hat + l* )
                    //
                    // The refresh cadence is clocked by the last report, so a
                    // hole that gets no further arrivals — exactly the case
                    // the request law exists for, since an arrival is what
                    // exposes a hole — would wait a whole refresh before it
                    // could be asked for. `earliest A_hat + l*` is the instant
                    // the oldest open hole becomes requestable, and taking the
                    // `min` makes `l >= l*` the binding condition rather than
                    // an upper bound on it.
                    //
                    // Arm absent ⇒ this reduces to `last_hole_nack_at +
                    // refresh`, with no allocation and no clock read.
                    let mut hole_deadline = last_hole_nack_at + refresh;
                    if recv_request_law {
                        if let Some(a_hat) = blk.succ.oldest_open_at() {
                            let due = a_hat
                                + Duration::from_micros(
                                    blk.late.request_lateness().0.unwrap_or(0),
                                );
                            if due < hole_deadline {
                                hole_deadline = due;
                            }
                        }
                    }
                    Some(hole_deadline)
                } else {
                    // δ-honest shed (paper §5.6): under the unified realtime
                    // machine the EVICT hold is the δ dial b·SRTT while the
                    // ε̂-class give-up budget is open; the 4×SRTT clamp
                    // otherwise (always with the law off).
                    if recv_shed_on {
                        let eps_recv = {
                            let sched = recv_scheduler.lock();
                            sched
                                .live_paths()
                                .into_iter()
                                .filter_map(|pid| {
                                    sched.path(pid).map(|p| p.estimator.rx_loss_rate())
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
                    let rb = reorder_buf.as_mut().expect("pending implies Some");
                    if let Some(h) = hold {
                        rb.set_timeout(h);
                    }
                    rb.oldest_deadline()
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
                {
                    let dec = &window_decoder;
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
                // Reliable window (ρ = 1): never give up on a hole.
                // Re-advertise the gap with a SACK-bearing WindowAck so the
                // sender's targeted-retransmit / repair machinery races it
                // until recovered — the hold-expiry force-delivery below is
                // the EVICT policy's move and is structurally skipped here.
                if recv_window_reliable {
                    last_hole_nack_at = Instant::now();
                    // Wedge forensics: the frontier is stalled (this arm only
                    // fires with a pending hole). Once frozen > 1 s, name the
                    // blocker's receiver-side state once per second.
                    if wdiag_on {
                        if next_expected != wdiag_frontier_val {
                            wdiag_frontier_val = next_expected;
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
                            let (b_seen, b_rec, b_out) = window_decoder.seq_probe(blocker);
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
                            crate::readout!(
                                "[WEDGE] stall={:.1}s frontier={} blocker={} \
                                 seen_src={} recovered={} output={} in_rseqs={} \
                                 pending={} highest_seen={} span={} \
                                 batches/s={} syms/s={}{}",
                                stall.as_secs_f64(),
                                next_expected,
                                blocker,
                                b_seen,
                                b_rec,
                                b_out,
                                received_seqs.contains(&blocker),
                                pending,
                                highest_seen_seq,
                                seen_end.saturating_sub(next_expected),
                                d_batches,
                                d_syms,
                                dg,
                            );
                        }
                    }
                    let sack_ranges = received_sack_ranges(
                        &received_seqs,
                        next_expected,
                        highest_seen_seq,
                    );
                    let received_above = recv_above.sync(&received_seqs, next_expected);
                    debug!(
                        next_expected,
                        seen = highest_seen_seq,
                        ranges = sack_ranges.len(),
                        received_above,
                        "reliable window: hole stalled — re-advertising gap"
                    );
                    let ack_msg = ControlMessage::WindowAck {
                        next_expected,
                        received_above,
                        sack_ranges,
                        echo_send_timestamp_us: 0,
                        jitter_us: 0,
                        cumulative_received: 0,
                        // Timer-driven hole re-advertisement: ONE message
                        // broadcast to every live path, so it cannot carry
                        // a per-path counter. 0 = the "no counter payload"
                        // sentinel, exactly parallel to the echo == 0
                        // timer-ack sentinel this site already uses.
                        cum_expected: 0,
                        cum_received: 0,
                    };
                    for pid in recv_scheduler.lock().live_paths() {
                        let _ = recv_transport.send_control_datagram(pid, ack_msg.clone());
                    }
                    // Request arms (A)/(B): the timer arm's request. This is
                    // the arm the deadline term above pulls forward to
                    // `earliest A_hat + l*`, and it is the only producer for a
                    // hole that gets no further arrivals. Cause tag: the
                    // receiver's timer-driven refresh, in `[FCAUSE]`'s own
                    // vocabulary.
                    if recv_request_law || recv_rank_feedback {
                        let pid_rq = recv_scheduler.lock().live_paths().first().copied();
                        if let Some(pid) = pid_rq {
                            let dec = &window_decoder;
                            send_repair_request!(
                                dec,
                                pid,
                                crate::net::FireCause::GapRefresh
                            );
                        }
                    }
                    continue;
                }
                // Give up on the hole(s): force-deliver expired entries
                // (plus everything they unblock) so the tunnel never
                // stalls on an unrecoverable symbol.
                if let Some(ref mut reorder) = reorder_buf {
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
                            crate::readout!(
                                "[SHED-R] holes={} frontier={} budget_open={}",
                                recv_shed_holes,
                                reorder.next_deliver_seq(),
                                recv_shed_budget_open,
                            );
                        }
                    }
                    // `[SUCC]`: the hold-expiry give-up is the OTHER site the
                    // frontier can jump a hole at, and it is swept here rather
                    // than at the next arrival so an abandoned hole's age is
                    // stamped when it was abandoned. Read-only.
                    for rec in
                        blk.succ.abandon_below(reorder.next_deliver_seq(), Instant::now())
                    {
                        blk.late.note_hole(rec.outcome, rec.cross, rec.us, rec.hi_us);
                        abandoned_seqs.insert(rec.seq);
                    }
                    while abandoned_seqs.len() > ABANDONED_TRACK_MAX {
                        if let Some(&oldest) = abandoned_seqs.iter().next() {
                            abandoned_seqs.remove(&oldest);
                        }
                    }
                    for (dseq, ddata, _) in expired {
                        debug!(seq = dseq, "window hold expired — force-delivering");
                        for pkt_data in extract_window_packets(&ddata, window_packed) {
                            let _ = recv_tun_tx.try_send(Bytes::from(pkt_data));
                        }
                        next_expected = next_expected.max(dseq + 1);
                    }
                    // Advertise the advanced cumulative point to the
                    // PEER so its sender-side ack state (retransmit
                    // buffer, window advance) opens even with no
                    // further arrivals (the deadlock cycle above) —
                    // send a bare WindowAck now in case none comes.
                    if next_expected > last_advertised_ack {
                        last_advertised_ack = next_expected;
                        let ack_msg = ControlMessage::WindowAck {
                            next_expected,
                            received_above: recv_above.sync(&received_seqs, next_expected),
                            sack_ranges: Vec::new(),
                            echo_send_timestamp_us: 0,
                            jitter_us: 0,
                            cumulative_received: 0,
                            // Hold-expiry unwedge: same broadcast, same
                            // "no counter payload" sentinel as above.
                            cum_expected: 0,
                            cum_received: 0,
                        };
                        for pid in recv_scheduler.lock().live_paths() {
                            let _ = recv_transport.send_control_datagram(pid, ack_msg.clone());
                        }
                    }
                }
                continue;
            }
            // ρ = 1 retry wake: packets held on a full consumer channel go
            // out the moment it has room (event-driven, never blocks the
            // loop), and the ack they unblock is advertised right away.
            // While held, the cumulative ack stalls, so the sender's tail sweep
            // retransmits the blocker: back-pressure, expected (not loss).
            permit = recv_tun_tx.reserve(), if window_delivery.is_holding() => {
                let flushed = match permit {
                    Ok(p) => window_delivery.flush_with_permit(p, &recv_tun_tx),
                    Err(_) => Err(super::delivery::Closed),
                };
                if flushed.is_err() {
                    error!("TUN inject channel closed");
                    break 'recv;
                }
                let next_unreleased = reorder_buf
                    .as_ref()
                    .map(|rb| rb.next_deliver_seq())
                    .unwrap_or(ooo_frontier);
                next_expected = next_expected.max(delivered_prefix(
                    next_unreleased,
                    window_delivery.lowest_held(),
                ));
                if next_expected > last_advertised_ack {
                    last_advertised_ack = next_expected;
                    let ack_msg = ControlMessage::WindowAck {
                        next_expected,
                        received_above: recv_above.sync(&received_seqs, next_expected),
                        sack_ranges: Vec::new(),
                        echo_send_timestamp_us: 0,
                        jitter_us: 0,
                        cumulative_received: 0,
                        // Timer-style broadcast: the "no counter payload"
                        // sentinel, as the hold-expiry unwedge above.
                        cum_expected: 0,
                        cum_received: 0,
                    };
                    for pid in recv_scheduler.lock().live_paths() {
                        let _ = recv_transport.send_control_datagram(pid, ack_msg.clone());
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
                crate::readout!(
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
                // v8: the sender's own delivery prediction for this batch,
                // us relative to `batch_send_ts`. 0 = no prediction.
                let batch_eta_rel_us = batch.eta_rel_us;
                // `[LAT]`: the SOURCE seqs of this batch, read before the
                // decoder consumes it. (A repair symbol's `block_id` is its
                // window anchor, not a seq it delivers.)
                let lat_arrivals: Vec<u64> = batch
                    .symbols
                    .iter()
                    .filter(|sym| !sym.is_repair)
                    .map(|sym| sym.block_id)
                    .collect();
                let batch_seq = batch.batch_seq;
                let batch_path_id = batch.path_id;
                let symbol_count = batch.symbols.len() as u32;
                if wdiag_on {
                    wdiag_batches += 1;
                    wdiag_syms += symbol_count as u64;
                    // `[WIDLE]` gauge (see decls).
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
                    // The same gap against the derived threshold.
                    if widle_derived {
                        widle_evt_n += 1;
                        if widle_last_arrival_us > 0 {
                            let gap = wnow.saturating_sub(widle_last_arrival_us);
                            if gap >= widle_thr_us {
                                widle2_us += gap;
                                widle2_n += 1;
                                widle2_max_us = widle2_max_us.max(gap);
                            }
                        }
                    }
                    widle_last_arrival_us = wnow;
                    if wnow.saturating_sub(widle_last_print_us) >= 1_000_000 {
                        let wdt = wnow.saturating_sub(widle_last_print_us);
                        widle_last_print_us = wnow;
                        let w2 = if widle_derived {
                            // Re-derive from this window's measured
                            // arrival rate (see the decls).
                            if widle_evt_n > 0 {
                                widle_evt_us = wdt / widle_evt_n;
                                widle_thr_us = stall_threshold_us(widle_evt_us);
                            }
                            widle_evt_n = 0;
                            format!(
                                " idle2={}ms/{}/mx{}ms evt={}us sthr={}us",
                                widle2_us / 1000,
                                widle2_n,
                                widle2_max_us / 1000,
                                widle_evt_us,
                                widle_thr_us,
                            )
                        } else {
                            String::new()
                        };
                        crate::readout!(
                            "[WIDLE] idle={}ms/{}/mx{}ms arr={}{}",
                            widle_us / 1000,
                            widle_n,
                            widle_max_us / 1000,
                            widle_arrivals,
                            w2,
                        );
                    }
                }

                // Touch path as keepalive (received data = path is alive)
                recv_scheduler.lock().touch_path(path_id);

                // Record arrival for RTCP-style jitter calculation
                {
                    let arrival_us = now_us();
                    let mut sched = recv_scheduler.lock();
                    // `[ETA]`'s reference lag, read in the borrow that is
                    // already open so the gauge costs no second acquisition:
                    // RTprop when this receiver has one, its SRTT otherwise,
                    // and the source of that SRTT (Copa's wire clock vs the
                    // app echo, which differ by the sender's own reservoir
                    // dwell). Both are printed; the gauge never picks
                    // silently.
                    let (eta_tau_us, eta_src) = match sched.path(path_id) {
                        Some(p) => (
                            p.min_rtt()
                                .map(|d| d.as_micros() as u64)
                                .unwrap_or_else(|| p.srtt().as_micros() as u64),
                            if crate::scheduler::copa_wire_active() {
                                crate::net::eta::SrttSource::Wire
                            } else {
                                crate::net::eta::SrttSource::Echo
                            },
                        ),
                        None => (0, crate::net::eta::SrttSource::Echo),
                    };
                    if let Some(path) = sched.path_mut(path_id) {
                        path.estimator.record_arrival(batch_send_ts, arrival_us);
                        // Update jitter in monitoring stats
                        if let Some(ps) = recv_stats.path(path_id) {
                            ps.jitter_us.store(path.estimator.jitter_us() as u64, Ordering::Relaxed);
                        }
                    }
                    drop(sched);
                    blk.eta.observe(
                        path_id,
                        batch_send_ts,
                        arrival_us,
                        batch_eta_rel_us,
                        eta_tau_us,
                        eta_src,
                    );
                    // `[LAT]`'s `A_x`, on the same instant and the same two
                    // clock readings the `[ETA]` gauge just used — one
                    // arrival, one pair of timestamps, so the two gauges can
                    // never describe different events. Source symbols only: a
                    // seq the decoder reconstructs never rode a wire as itself
                    // and has no queueing time of its own, so it is absent
                    // from `ax` rather than credited a zero.
                    for sym in &lat_arrivals {
                        blk.lat.note_arrival(*sym, path_id, batch_send_ts, arrival_us);
                    }
                    // `[LATE]`'s knee: the gap since the previous arrival,
                    // counted only while the in-order frontier is behind the
                    // highest seq seen. A gap with the frontier caught up is
                    // the application idling; a gap with the frontier frozen
                    // is the store running out of headroom, which is what `H`
                    // is. Ungated — the `[WIDLE]` machinery's own 3 ms floor.
                    if late_last_arrival_us > 0 && seen_end > next_expected {
                        let gap = arrival_us.saturating_sub(late_last_arrival_us);
                        if gap >= WIDLE_GAP_MIN_US {
                            blk.late.note_knee(gap);
                        }
                    }
                    late_last_arrival_us = arrival_us;
                }

                // Track batch sequences for loss detection (ADR-0003).
                // Ack-merge (RWM_ACK_MERGE): read the tracker's cumulative
                // totals in the same borrow — they are the WindowAck counter
                // payload, the (expected, received) pair carried as running
                // sums so the sender can diff them. Cumulative, not per-ack:
                // a dropped control datagram then costs nothing.
                let (expected, _received_total, cum_expected, cum_received) = {
                    let mut tracker = recv_path_tracking
                        .entry(path_id)
                        .or_insert_with(PathBatchTracker::new);
                    // v9: keyed on the batch's per-path sequence
                    // (`path_seq`), never the global `batch_seq` -- the
                    // other paths' batches are not this path's losses.
                    let (e, r) = tracker.record(&batch);
                    (e, r, tracker.total_expected, tracker.total_received)
                };

                // The window receive path (the one pipeline, ADR-0069).
                {
                    let win_dec = &mut window_decoder;
                    // Generation-deficit feedback: learn each generation's
                    // K_g self-describingly from the wire header (window_start
                    // = anchor, window_count = K_g) of every coded symbol, and
                    // note whether this batch made any decode progress (drives
                    // an immediate deficit report).
                    let mut recovered_any = false;
                    if recv_window_generation {
                        for symbol in &batch.symbols {
                            if symbol.is_repair && symbol.data.len() >= 10 {
                                // Filling-generation repair (proactive pacer):
                                // its wire `window_count` is the full generation
                                // width G even though the generation is only
                                // partially sent, so it must not teach
                                // `gen_widths` — that would make the receiver
                                // report a K_g−rank deficit of (G − current fill)
                                // and flood reactive recovery for a generation
                                // that is not fully sent yet. The FILL_FLAG is
                                // bit 31 of the 4-byte coded-index. A filling
                                // generation enters `gen_widths` only once it is
                                // provably full (anti-wedge seeding) or a
                                // sealed/deficit repair arrives. Present-at-stall
                                // recovery of its holes is proactive (no deficit
                                // needed).
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
                        // Conversion DIAG: a source arrival for a seq already
                        // received = a displacement-only delivery on this path
                        // (its goodput was already banked by the other copy).
                        if c8r_on
                            && !symbol.is_repair
                            && received_seqs.contains(&symbol.block_id)
                        {
                            *c8r_dup.entry(path_id).or_insert(0) += 1;
                        }
                        // `[RFA]`: the realized false-repair class, read before
                        // the symbol is fed (the probe is about the state this
                        // arrival is about to change). See the event-class
                        // definition in `net/mod.rs` beside
                        // `RACK_SPURIOUS_BUDGET`. `seq_probe` is `&self` and the
                        // counters feed nothing but the gauge: read-only, no
                        // gate, always fed so the datum exists on every arm.
                        let rfa_class = (!symbol.is_repair).then(|| {
                            let (seen_src, rec, _out) = win_dec.seq_probe(symbol.block_id);
                            crate::net::classify_recv_repair(
                                seen_src,
                                rec,
                                symbol.block_id < highest_seen_seq,
                            )
                        });
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
                        match rfa_class {
                            Some(c) => recv_rack_echo.record_recv_source(c),
                            None => recv_rack_echo.record_recv_repair_arrival(),
                        }
                        // `[SUCC]`, in two passes, and the order is the
                        // semantics. One `add_symbol` can emit several seqs in
                        // arbitrary order; if the batch's high-water mark were
                        // raised while some of its own seqs were still
                        // unresolved, the gauge would open a hole for a seq
                        // this very batch is closing and then close it at 0 µs
                        // — a manufactured sample at the bottom of the
                        // histogram. So: resolve the whole batch first, then
                        // advance the mark. Read-only; see `net/succ.rs`.
                        let mut lat_release: crate::net::lat::Release = None;
                        {
                            let succ_now = Instant::now();
                            // `path_id` is the path this arrival landed on —
                            // the exposer at `observe_high`, the closer at
                            // `resolve`. Nothing branches on it.
                            for (seq, _) in &recovered {
                                // `[LAT]`: the record of the hole this arrival
                                // closed classes every reorder wait the
                                // arrival releases. The last resolution of the
                                // batch is the releasing one (the deliverable
                                // prefix starts at the frontier), and `None` —
                                // the ordinary in-order case — classes as no
                                // wait at all.
                                if let Some(rec) = blk.succ.resolve_at(
                                    *seq,
                                    symbol.is_repair || *seq != symbol.block_id,
                                    succ_now,
                                    path_id,
                                    batch_send_ts,
                                ) {
                                    lat_release = Some(rec);
                                    blk.late.note_hole(
                                        rec.outcome,
                                        rec.cross,
                                        rec.us,
                                        rec.hi_us,
                                    );
                                    // `d` — the `[FDIAG]` SOURCE class: a hole
                                    // closed by its own source symbol
                                    // (original or the sender's copy) is the
                                    // ARQ/late-reorder resolution whose mean
                                    // time the knee cap subtracts.
                                    if matches!(
                                        rec.outcome,
                                        crate::net::succ::HoleOutcome::Original
                                            | crate::net::succ::HoleOutcome::Retransmit
                                    ) {
                                        blk.late.note_source_resolution(rec.us);
                                    }
                                }
                            }
                            for (seq, _) in &recovered {
                                blk.succ.observe_high_at(
                                    *seq,
                                    succ_now,
                                    path_id,
                                    batch_send_ts,
                                );
                            }
                        }
                        for (seq, sym_data) in recovered {
                            // A seq that came out of the decoder rather than
                            // off its own source arrival was reconstructed
                            // from coded repair — `[RFA]`'s `fill_coded`, a
                            // repair that worked. (A source arrival can
                            // cascade other seqs out of the row space; those
                            // are coded fills too, hence the seq test rather
                            // than `symbol.is_repair` alone.)
                            if symbol.is_repair || seq != symbol.block_id {
                                recv_rack_echo.record_recv_coded_fill();
                            }
                            let newly = received_seqs.insert(seq);
                            recv_above.on_insert(seq, newly);
                            if seq > highest_seen_seq {
                                highest_seen_seq = seq;
                            }
                            seen_end = seen_end.max(seq + 1);
                            // Conversion DIAG: first copy of this seq,
                            // credited to the arrival path, with its lead
                            // over the in-order frontier (symbols).
                            if c8r_on {
                                let frontier = reorder_buf
                                    .as_ref()
                                    .map(|r| r.next_deliver_seq())
                                    .unwrap_or(ooo_frontier);
                                *c8r_first.entry(path_id).or_insert(0) += 1;
                                *c8r_lead.entry(path_id).or_insert(0) +=
                                    seq.saturating_sub(frontier);
                            }

                            // Unordered delivery (the H = 0 corner): hand each
                            // decoded symbol to the consumer the instant it
                            // decodes, in any order. The native object API
                            // reassembles by offset and completes on
                            // total-decoded, so no in-order frontier gates
                            // delivery. Reliability is unchanged: the in-order
                            // received prefix still drives the cumulative
                            // WindowAck, so the sender keeps retaining +
                            // retransmitting every hole until acked. The
                            // completion time equals an in-order buffer deep
                            // enough to hold to completion — the frontier
                            // only costs an incremental, low-latency consumer
                            // (inner TCP), never a file.
                            if recv_window_ooo {
                                // Deliver immediately (any order). This arm
                                // is reliable (ρ = 1), so a full channel
                                // HOLDS the packets (`window_delivery`),
                                // never drops and never blocks: blocking
                                // here would deadlock the loopback's
                                // client-feeds-and-drains feedback loop, and
                                // a drop would be permanent (the ack below
                                // lets the sender discard its only copy).
                                // Held packets go out on the `reserve()`
                                // wake in the select above.
                                if window_delivery
                                    .offer(
                                        &recv_tun_tx,
                                        seq,
                                        extract_window_packets(&sym_data, window_packed),
                                    )
                                    .is_err()
                                {
                                    error!("TUN inject channel closed");
                                    break 'recv;
                                }
                                // Advance the in-order received prefix for the
                                // cumulative WindowAck (retention pruning) —
                                // no reorder buffer: the frontier walks
                                // `received_seqs` (seq was inserted just
                                // above). Delivery happened out of order;
                                // this tells the sender what it may prune,
                                // so holes stay retained + retransmitted.
                                // The ack stops below the lowest seq still
                                // held for the consumer (a received seq is
                                // not yet a delivered one).
                                while received_seqs.contains(&ooo_frontier) {
                                    ooo_frontier += 1;
                                }
                                next_expected = next_expected.max(delivered_prefix(
                                    ooo_frontier,
                                    window_delivery.lowest_held(),
                                ));
                                continue;
                            }

                            // ----- in-order delivery (default: TCP-in-
                            // tunnel and Realtime need the frontier) -----
                            let deliverable = if let Some(ref mut reorder) = reorder_buf {
                                // `[RFA] late_after_aban`: this seq's copy
                                // arrived for a seq the frontier had already
                                // moved past without delivering — the EVICT
                                // seat's repair waste, a counter
                                // `tail_matrix.sh` scrapes by name.
                                //
                                // The `abandoned_seqs` test is what makes it
                                // that: a copy for a seq that was delivered is
                                // an ordinary duplicate (already `[RFA]
                                // dup_src`), and conflating the two would
                                // report waste on the reliable window, where
                                // the reorder buffer never delivers past a
                                // hole and the count is structurally zero.
                                if abandoned_seqs.remove(&seq) {
                                    recv_rack_echo.record_late_after_aban();
                                }
                                reorder.push(seq, sym_data)
                            } else {
                                // Never buffered: the delivery instant is
                                // its own stamp, so `[LAT]`'s reorder wait is
                                // exactly 0.
                                vec![(seq, sym_data, Instant::now())]
                            };

                            // Conversion DIAG: this arrival advanced the
                            // in-order frontier — if it had been stalled
                            // ≥ 5 ms, credit the unblock (and the stall it
                            // ended) to this path.
                            if c8r_on && !deliverable.is_empty() {
                                let gap = c8r_last_adv.elapsed();
                                if gap >= Duration::from_millis(5) {
                                    *c8r_unb_n.entry(path_id).or_insert(0) += 1;
                                    *c8r_unb_ms.entry(path_id).or_insert(0) +=
                                        gap.as_millis() as u64;
                                }
                                c8r_last_adv = Instant::now();
                            }

                            let lat_now = Instant::now();
                            for (dseq, ddata, dbuf) in deliverable {
                                // `[LAT]`: the reorder wait is
                                // `t_deliver - buffered_at`, which only the
                                // buffer ever held; its class is the release
                                // record above. Fed before the packet leaves,
                                // so a full TUN channel cannot lose the datum.
                                blk.lat.note_delivery(
                                    dseq,
                                    lat_now.saturating_duration_since(dbuf).as_micros() as u64,
                                    lat_release,
                                );
                                if window_delivery
                                    .offer(
                                        &recv_tun_tx,
                                        dseq,
                                        extract_window_packets(&ddata, window_packed),
                                    )
                                    .is_err()
                                {
                                    error!("TUN inject channel closed");
                                    break 'recv;
                                }
                                next_expected = next_expected.max(dseq + 1);
                            }
                            // ρ = 1: never ack past a seq still held for the
                            // consumer (EVICT never holds: a no-op there). v9:
                            // a held seq 0 reads `next_expected = 0` --
                            // expressible, where v8 clamped it onto "seq 0
                            // delivered".
                            next_expected =
                                delivered_prefix(next_expected, window_delivery.lowest_held());

                            // `[SUCC]` abandonment, read off the frontier
                            // itself rather than off any give-up decision: a
                            // hole the in-order frontier has moved past was
                            // given up, whatever moved it. Under the reliable
                            // window this is structurally unreachable
                            // (`ReorderBuffer::new_reliable` never delivers
                            // past a hole) — so `aban_n = 0` there is a
                            // configuration fact, and a nonzero reading is a
                            // finding about the engine. Read-only.
                            if let Some(ref reorder) = reorder_buf {
                                for rec in blk.succ.abandon_below(
                                    reorder.next_deliver_seq(),
                                    Instant::now(),
                                ) {
                                    blk.late.note_hole(
                                        rec.outcome,
                                        rec.cross,
                                        rec.us,
                                        rec.hi_us,
                                    );
                                    abandoned_seqs.insert(rec.seq);
                                }
                                while abandoned_seqs.len() > ABANDONED_TRACK_MAX {
                                    if let Some(&oldest) = abandoned_seqs.iter().next() {
                                        abandoned_seqs.remove(&oldest);
                                    }
                                }
                            }
                        }
                    }

                    // Conversion DIAG: the receiver-side conversion gauges,
                    // cumulative, ~1/s (keys sorted for stable scraping).
                    // fst/dup = first-copy vs displacement deliveries; lead =
                    // mean first-copy frontier lead (symbols); unb = frontier
                    // unblocks credited / stall ms ended.
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
                            crate::readout!("[C8CONV-R]{}", s);
                        }
                    }

                    // Generation-deficit feedback (paper §5.8, receiver arm):
                    // on decode progress, report each frontier generation's
                    // residual deficit immediately (the deficit shrank → tell
                    // the sender promptly so it stops over-sending). The
                    // periodic timer arm drives it otherwise — even when no
                    // data is arriving, so a sender that emitted its budget
                    // and went quiet is still re-pulled.
                    if recv_window_generation && recovered_any {
                        send_gen_deficits!(win_dec, true);
                    }

                    // Drain expired reorder buffer entries. SRTT-adaptive hold
                    // (the EVICT delivery contract): a static 20 ms
                    // hold sits below one NACK/repair round, so holes would be
                    // force-delivered just before their repair arrived and the
                    // inner TCP would retransmit them.
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
                                        sched.path(pid).map(|p| p.estimator.rx_loss_rate())
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
                        for (dseq, ddata, _) in expired {
                            for pkt_data in extract_window_packets(&ddata, window_packed) {
                                let _ = recv_tun_tx.try_send(Bytes::from(pkt_data));
                            }
                            next_expected = next_expected.max(dseq + 1);
                        }
                    }

                    // ── Proactive-frontier diagnosis (RWM_FDIAG) ──────
                    if fdiag_on {
                        // `f` = the first undelivered seq (v9 count form).
                        let f = next_expected;
                        // Resolve a tracked hole once the frontier passes it.
                        if let Some((hp, t0, present, saw_src)) = fdiag_hole {
                            if f > hp {
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
                        if fdiag_hole.is_none() && seen_end > f {
                            let (_h, buffered) =
                                win_dec.frontier_probe(f, highest_seen_seq);
                            fdiag_hole =
                                Some((f, Instant::now(), buffered > 0, false));
                        }
                        // Periodic aggregate report (~500 ms).
                        if fdiag_report_at.elapsed() >= Duration::from_millis(500) {
                            fdiag_report_at = Instant::now();
                            let (holes, buffered) =
                                win_dec.frontier_probe(f, highest_seen_seq);
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
                            // Mean raw decode-call compute time (µs) and total
                            // compute over the transfer — contrast with the
                            // per-hole DECODE resolution wall-time above.
                            let addsym_avg = if fdiag_addsym_n > 0 {
                                fdiag_addsym_us / fdiag_addsym_n
                            } else {
                                0
                            };
                            crate::readout!(
                                "[FDIAG] frontier={} seen={} gap={} probe_holes={} probe_buffered={} | DECODE n={} avg={}us present_at_stall={} | SOURCE n={} avg={}us | COMPUTE calls={} avg={}us max={}us total={}ms | rf={} ru={}{}",
                                f, highest_seen_seq,
                                seen_end.saturating_sub(f),
                                holes, buffered,
                                fdiag_decode_n, dec_avg, fdiag_present_at_stall,
                                fdiag_source_n, src_avg,
                                fdiag_addsym_n, addsym_avg,
                                std::mem::take(&mut fdiag_addsym_max_us),
                                fdiag_addsym_us / 1000,
                                win_dec.repairs_fed(), win_dec.repairs_useful(),
                                // Decoder-internal cost drivers (active rows
                                // L, span, memory).
                                win_dec
                                    .diag_stats()
                                    .map(|s| format!(" | {s}"))
                                    .unwrap_or_default(),
                            );
                        }
                    }

                    // ── `[RFA]` periodic readout ──────────────────────────
                    // The gauge's `Drop` is the authoritative emission, but
                    // the L1 harnesses SIGKILL the server, so a receiver-site
                    // `Drop` is not reachable there. Cumulative counters on a
                    // 1 s cadence under the existing diagnosis gates, so the
                    // last line is the reading whatever kills the process. A
                    // run with no repair-class event stays silent.
                    if (recv_gates.diag || fdiag_on)
                        && recv_rack_echo.is_receiver_site()
                        && rfa_report_at.elapsed() >= Duration::from_secs(1)
                    {
                        rfa_report_at = Instant::now();
                        crate::readout!("{}", recv_rack_echo.rfa_line());
                    }

                    // ── `[QCLK]` periodic readout ─────────────────────────
                    // Same SIGKILL reason, same 1 s cadence, same gates, same
                    // last-line-wins convention. A run that never evaluated a
                    // recovery clock stays silent, so an absent line reads as
                    // an unreached evaluation site and never as an unset gate.
                    if (recv_gates.diag || fdiag_on)
                        && recv_qclk_echo.evals() > 0
                        && qclk_report_at.elapsed() >= Duration::from_secs(1)
                    {
                        qclk_report_at = Instant::now();
                        crate::readout!("{}", recv_qclk_echo.line());
                    }

                    // ── `[SUCC]` periodic readout ─────────────────────────
                    // Same SIGKILL reason, same 1 s cadence, same gates, same
                    // last-line-wins convention as `[RFA]` and `[QCLK]` above.
                    // `is_receiver_site()` keeps a site that saw no arrival
                    // silent, so an absent line reads as an unreached feed and
                    // never as an unset gate.
                    //
                    // The block (`net/recv_block.rs`): `[SUCC]`, then `[ETA]` /
                    // `[LAT]` / `[LATE]` beside it under its gate, `[REQ]`, and
                    // `[RANK]` off a fresh `frontier_probe`. The block also
                    // prints once more at the task's exit, marked `final=1`
                    // (see the end of the loop), so a transfer shorter than
                    // this cadence and the last partial second of a longer one
                    // are not lost. The raw dump is flushed with it so no
                    // recorded sample is left in a partial batch.
                    if (recv_gates.diag || fdiag_on)
                        && blk.succ.is_receiver_site()
                        && succ_report_at.elapsed() >= Duration::from_secs(1)
                    {
                        succ_report_at = Instant::now();
                        // `[RANK]`: `frontier_probe` re-read unconditionally
                        // (it is otherwise reachable only under `RWM_FDIAG`),
                        // plus the tail correction: the span that arrived
                        // since the previous readout is still in flight rather
                        // than missing, and is reported beside the deficit
                        // rather than subtracted from it.
                        let probe = rank_probe(
                            &**win_dec,
                            next_expected,
                            seen_end,
                            &mut rank_prev_seen,
                        );
                        for l in blk.render_cadence(Some(probe)) {
                            crate::readout!("{l}");
                        }
                        // `[RFA] rep_redundant`: repairs fed minus repairs that
                        // recovered anything — the false measurand under coded
                        // answers, where "the original arrived anyway" is
                        // inexpressible and waste shows up as an equation that
                        // added no rank. Read off the decoder at the readout so
                        // the gauge holds no second copy.
                        recv_rack_echo.set_rep_redundant(
                            win_dec.repairs_fed().saturating_sub(win_dec.repairs_useful()),
                        );
                    }

                    // Send SACK-extended WindowAck to sender. Also send while
                    // the cumulative point is stalled on a hole but new
                    // (higher) seqs keep arriving — the dupack analog. The
                    // SACK ranges are the sender's only gap signal; without
                    // them a hole would be repaired solely by proactive FEC or
                    // the hold-expiry force-delivery.
                    let cumulative_advanced = next_expected > last_advertised_ack;
                    let gap_pending = seen_end > next_expected
                        && highest_seen_seq > last_gap_ack_seen;
                    let gap_report_due =
                        gap_pending && last_gap_ack_time.elapsed() >= GAP_ACK_MIN_INTERVAL;
                    // `[LATE] sampler_bind`: a hole was ready to be reported
                    // and the 2 ms floor — not its lateness — is what held the
                    // report back. A threshold that always binds turns the law
                    // it gates into a constant, so it is counted.
                    blk.late_sampler_bound |= gap_pending && !gap_report_due;
                    // Ack-merge (RWM_ACK_MERGE): what the ack advertises (the
                    // cumulative point + SACK ranges) is unchanged —
                    // `advertise` is the gap-report predicate, so
                    // GAP_ACK_MIN_INTERVAL still rate-limits gap reports and
                    // the depth-16 nack/sack channels see no new pressure.
                    // What changes is only whether a datagram is sent: under
                    // the merge this ack also carries the suppressed per-batch
                    // Ack's payload, so it must go out once per data message —
                    // the per-batch Ack's cadence. Gate off ⇒
                    // `emit == advertise`.
                    let (emit_ack, advertise) = window_ack_emission(
                        cumulative_advanced,
                        gap_report_due,
                        ack_merge_recv,
                    );
                    if emit_ack {
                        if cumulative_advanced {
                            last_advertised_ack = next_expected;
                        }
                        // SACK ranges: what was received beyond the cumulative
                        // point (not what's missing). Only on an advertising
                        // ack — a merge-only ack carries the counters and the
                        // echo, never a gap report (that is what preserves the
                        // gap rate limit).
                        let sack_ranges = if advertise {
                            last_gap_ack_seen = highest_seen_seq;
                            last_gap_ack_time = Instant::now();
                            // A gap-bearing ack is a hole re-advertisement:
                            // push the reliable-mode refresh timer out.
                            last_hole_nack_at = last_gap_ack_time;
                            received_sack_ranges(
                                &received_seqs,
                                next_expected,
                                highest_seen_seq,
                            )
                        } else {
                            Vec::new()
                        };

                        let jitter = {
                            let sched = recv_scheduler.lock();
                            sched.path(path_id)
                                .map(|p| p.estimator.jitter_us() as u32)
                                .unwrap_or(0)
                        };

                        let ack_msg = ControlMessage::WindowAck {
                            next_expected,
                            // v9: on every ack, merge-only ones included.
                            received_above: recv_above.sync(&received_seqs, next_expected),
                            sack_ranges,
                            echo_send_timestamp_us: batch_send_ts,
                            jitter_us: jitter,
                            // In OOO / generation mode carry the total count
                            // of decoded source symbols across all generations
                            // (out of order) — the peer's total decode progress
                            // `d`. received_seqs holds every delivered seq
                            // (decode-on-total), so its length is d. In-order
                            // modes keep the per-path received count. A
                            // wire-format/debug-trace datum; no sender-side
                            // flow control consumes it.
                            cumulative_received: if recv_window_ooo {
                                received_seqs.len() as u64
                            } else {
                                recv_stats.path(path_id)
                                    .map(|ps| ps.symbols_received.load(Ordering::Relaxed))
                                    .unwrap_or(0)
                            },
                            // Ack-merge counters: the per-batch Ack's
                            // (expected, received) pair as per-path running
                            // sums. Always populated on a data-triggered ack
                            // (one wire format per binary); the sender only
                            // consumes them under RWM_ACK_MERGE.
                            cum_expected,
                            cum_received,
                        };
                        if let Err(e) = recv_transport.send_control_datagram(path_id, ack_msg) {
                            debug!(?e, path_id, "failed to send WindowAck");
                        }
                        // Request arms (A)/(B) (paper §7.6): the data arm's
                        // request, on exactly the cadence the gap report it
                        // replaces runs on. `advertise` is the gap-report
                        // predicate, so `GAP_ACK_MIN_INTERVAL` still
                        // rate-limits this — the 2 ms literal stands, and
                        // `[LATE] sampler_bind` says whether it, rather than
                        // `l*`, set the time.
                        if advertise {
                            send_repair_request!(
                                win_dec,
                                path_id,
                                crate::net::FireCause::GapData
                            );
                        }
                    }

                    // Ack-merge control-datagram density gauge (see the
                    // declaration of `ctld_last_report` for why frames and not
                    // the qdisc packet counters). Cumulative per path, 1 Hz.
                    if recv_diag_on
                        && ctld_last_report.elapsed() >= Duration::from_secs(1)
                    {
                        ctld_last_report = Instant::now();
                        let mut line = String::from("[CTLD]");
                        let live = recv_scheduler.lock().live_paths();
                        for &pid in &live {
                            if let Some((rx, tx)) =
                                recv_transport.datagram_frame_stats(pid)
                            {
                                line.push_str(&format!(" p{pid} tx={tx} rx={rx}"));
                            }
                        }
                        // S10 local-drop token, appended after the pairs:
                        // `dgrx<id>[fr=<quinn frame_rx> rd=<app read>
                        // ev=<fr − rd>]` — datagrams quinn accepted but the
                        // app never read (its incoming buffer dropped them,
                        // plus what is still buffered). The tracker reads
                        // them as loss; they are not wire loss.
                        for &pid in &live {
                            if let Some((fr, rd)) = recv_transport.datagram_rx_audit(pid) {
                                line.push_str(&format!(
                                    " dgrx{pid}[fr={fr} rd={rd} ev={}]",
                                    fr.saturating_sub(rd)
                                ));
                            }
                        }
                        crate::readout!("{line}");
                    }

                    // Periodic tasks (rate-limited by REPORT_INTERVAL)
                    // NACK sending replaced by SACK-extended WindowAck above.
                    let now = Instant::now();
                    if now.duration_since(last_nack_time) >= REPORT_INTERVAL
                        && highest_seen_seq > 0
                    {
                        last_nack_time = now;

                        // Controller feedback for the window decoder.
                        {
                            let win_dec = &window_decoder;
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

                        // Prune old entries from received_seqs tracking and
                        // the window decoder's recovered/pivot/seen state
                        // (unbounded otherwise over long streams). Everything
                        // below the delivered prefix minus two windows is
                        // decode-inert: repairs only reference the sender's
                        // current window, which sits at or above its ack
                        // (= our delivered point).
                        // (v9 count form: `next_expected − 1` is the v8
                        // `highest_delivered_seq`, so the prune point is
                        // unchanged.)
                        let delivered_last = next_expected.saturating_sub(1);
                        let mut prune_before = delivered_last.saturating_sub(recv_win_cap * 2);
                        // Reliability invariant (RWM_REASM_BDP): never evict a
                        // received symbol before it is delivered. Under SACK the
                        // sender races ahead of the frozen in-order frontier, so
                        // `highest_seen_seq` runs far above the cumulative point
                        // (the hole). The prune is keyed on the delivered frontier
                        // (so `prune_before ≤ delivered_last` already), but
                        // clamp it explicitly so a received-above-hole symbol the
                        // sender has pruned is never dropped. The reorder buffer is
                        // separately non-evicting (usize::MAX), so held source
                        // symbols survive to delivery regardless.
                        if reasm_bdp_on {
                            prune_before = prune_before.min(delivered_last);
                        }
                        // Sync the `received_above` counter first: the prune
                        // removes only seqs below the cumulative point, which
                        // a synced counter no longer counts.
                        recv_above.sync(&received_seqs, next_expected);
                        received_seqs = received_seqs.split_off(&prune_before);
                        window_decoder.advance(prune_before);
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
                            let span = seen_end.saturating_sub(next_expected);
                            reasm_max_span = reasm_max_span.max(span);
                            if reasm_last_report.elapsed() >= Duration::from_millis(500) {
                                reasm_last_report = Instant::now();
                                crate::readout!(
                                    "[REASM] frontier={} highest_seen={} span={} pending={} max_pending={} max_span={}",
                                    next_expected, highest_seen_seq, span,
                                    pending, reasm_max_pending, reasm_max_span,
                                );
                            }
                        }
                    }
                }

                // ADR-0005: send ACK with echo timestamp for RTT.
                //
                // Ack-merge (RWM_ACK_MERGE): this is the second control
                // datagram per data message: one per-batch Ack for every SACK
                // WindowAck. Under the merge it is suppressed entirely: its
                // payload rides the WindowAck's cumulative counters and its
                // consumers use the counter diff.
                let suppress_legacy_ack = ack_merge_recv;
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
                record_incoming_loss(&mut recv_scheduler.lock(), path_id, expected, symbol_count);

                // ADR-0005: send ACK as datagram (best-effort, low overhead)
                if !suppress_legacy_ack {
                    match recv_transport.send_control_datagram(path_id, ack) {
                        Err(e) => debug!(?e, path_id, "failed to send ACK datagram"),
                        Ok(()) => debug!(path_id, batch_seq, symbol_count, "ack sent"),
                    }
                }
            }
            WireMessage::Control(ctrl_msg) => {
                // Handle WindowStart packed flag in receiver loop
                if let ControlMessage::WindowStart { packed, .. } = &ctrl_msg {
                    window_packed = *packed;
                }

                // Mid-stream backend switching is not supported (paper §5.10):
                // no peer running this code sends WindowSwitch, and acting on
                // one (rebuilding the decoder mid-stream) is a
                // seq-space/state hazard. Ignore it, loudly.
                if let ControlMessage::WindowSwitch { flush_seq, new_backend, .. } = &ctrl_msg {
                    warn!(
                        flush_seq,
                        ?new_backend,
                        "ignoring WindowSwitch: mid-stream FEC backend switching \
                         was removed (codec is pinned at stream setup; paper §5.10)"
                    );
                }

                handle_control_message(
                    path_id,
                    ctrl_msg,
                    &ControlCtx {
                        scheduler: &recv_scheduler,
                        transport: &recv_transport,
                        stats: &recv_stats,
                        nack_tx: recv_nack_tx.as_ref(),
                        peer_window_ack: Some(&recv_window_ack),
                        deficit_tx: if recv_window_generation { Some(&recv_deficit_tx) } else { None },
                        sack_tx: recv_sack_tx.as_ref(),
                        request_tx: recv_request_tx.as_ref(),
                        copa_feed: recv_copa_feed.as_ref(),
                        mstar_anchor: recv_gates.mstar_anchor,
                    },
                );
            }
        }
    }

    // ── The exit flush ────────────────────────────────────────────────────
    // Every way out of the loop lands here: the channel closing, the shutdown
    // broadcast, and the two failure exits (`break 'recv` above: the two
    // TUN-inject closures) — they all end the task at this one site, and nothing runs after it. The block
    // prints once more, marked `final=1`, off a fresh `[RANK]` probe while the
    // decoder is still in scope. A task whose future is dropped instead
    // (runtime teardown) never reaches this line; `RecvDiagBlock`'s destructor
    // flushes it then, without a probe. The two share one flag, so exactly one
    // final block is ever printed.
    let probe = Some(rank_probe(
        window_decoder.as_ref(),
        next_expected,
        seen_end,
        &mut rank_prev_seen,
    ));
    blk.flush_final(probe);
}

/// The receiver's own loss feed (feed D): one arrived batch's
/// `(expected, received)` from `PathBatchTracker`, i.e. loss on the
/// INCOMING direction of `path_id`. It feeds the RX slot only
/// (`LossEstimator::record_rx_batch`); the TX estimator belongs to this
/// endpoint's sender and the outgoing direction.
pub(crate) fn record_incoming_loss(
    sched: &mut crate::scheduler::Scheduler,
    path_id: u32,
    expected: u32,
    received: u32,
) {
    if let Some(p) = sched.path_mut(path_id) {
        p.estimator.record_rx_batch(expected, received);
    }
}

/// `[RANK]`'s frontier reading `(holes, pivots, tail_overcount)` over the
/// span `[next_expected, seen_end)`, with the tail correction: the span
/// that arrived since the previous readout is still in flight rather than
/// missing, and is reported beside the deficit rather than subtracted from
/// it. Shared by the cadence readout and the exit flush so the two can never
/// disagree on what a probe is. Advances `rank_prev_seen`.
fn rank_probe(
    win_dec: &dyn WindowDecoder,
    next_expected: u64,
    seen_end: u64,
    rank_prev_seen: &mut u64,
) -> (u64, u64, u64) {
    // v9 exclusive forms: the span is `[next_expected, seen_end)`, empty when
    // nothing above the cumulative point has been seen. `rank_prev_seen`
    // keeps its inclusive "highest seen" meaning.
    let highest_seen_seq = seen_end.saturating_sub(1);
    if seen_end <= next_expected {
        *rank_prev_seen = highest_seen_seq;
        return (0, 0, 0);
    }
    let (holes, pivots) = win_dec.frontier_probe(next_expected, highest_seen_seq);
    let tail_lo = rank_prev_seen.saturating_add(1).max(next_expected);
    let tail_holes = if highest_seen_seq >= tail_lo {
        win_dec.frontier_probe(tail_lo, highest_seen_seq).0
    } else {
        0
    };
    *rank_prev_seen = highest_seen_seq;
    (holes, pivots, tail_holes)
}
