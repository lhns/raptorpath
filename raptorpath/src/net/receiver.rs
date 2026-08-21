//! The engine's RECEIVER task: one datagram/stream message in, decode →
//! reassemble → in-order (or unordered) delivery to the TUN, plus every
//! control message the peer sends back.
//!
//! History (net seam pass 3, 2026-08-09): this was a 1,735-line
//! `tokio::spawn(async move { … })` block inline in `run_impl` — the last
//! large inline task after seam pass 1 lifted the five background tasks
//! (`arq_sweep`, `control_fastpath`, `decoder_gc`, `path_cmd`, `report`) into
//! `net/tasks/`. It is 545 lines of receiver-local state followed by ONE
//! `loop` around a `tokio::select!` over the message channel, the in-order
//! hold deadline, the deficit deadline and shutdown. `run_impl` now keeps
//! setup + spawns.
//!
//! BEHAVIOUR CONTRACT: the body is VERBATIM. It was moved by a mechanical
//! transform that only dedents one level; re-indenting the 1,735 lines
//! reproduces the original block byte-for-byte, with ONE exception, listed
//! here because it is the only textual edit in the move:
//!   * the four reads of `config.reorder_timeout_ms` / `config.reorder_max_size`
//!     (three lines) become the parameters `recv_reorder_timeout_ms` /
//!     `recv_reorder_max_size`. Rust 2021 disjoint capture already copied
//!     exactly those two `u64`/`usize` fields into the spawned block — which
//!     is why `run_impl` can still read `config.status_addr` afterwards — so
//!     passing the two values is the same two copies, taken at the same
//!     point.
//!
//! Everything else about the spawn is unchanged. The captures were CLONED in
//! `run_impl` at their original lines and MOVED into the block at the spawn;
//! they are now cloned at the same lines and PASSED at the same spawn, in the
//! same order — `tokio::spawn(receiver::run_receiver(…))` builds the future
//! on the caller's thread (an `async fn` runs no body until polled), so the
//! task still starts executing exactly when the spawn hands it to the
//! runtime. In particular:
//!   * the `select!` still has the same four arms in the same order, and the
//!     `rdiag` idle stopwatch still brackets exactly that `select!`;
//!   * every `recv_scheduler` / `recv_fec` / `recv_decoders` lock is taken and
//!     released at the same statement, with the same scope — nothing was
//!     hoisted out of or into a guard's lifetime;
//!   * all FOUR early `return`s (two TUN-inject failures on the window
//!     delivery paths, and the two `feed_block_symbol` failures — the live
//!     one and the BlockStart replay) still end the TASK, not a helper: they
//!     are `return` from the same future, and `feed_block_symbol` is still a
//!     closure returning `bool` rather than a function that could swallow
//!     them.
//!
//! NOT covered here: `spawn_receiver_for_path` (the per-path datagram/stream
//! readers that FEED this task's channel) and the control fast-path task,
//! both still in `run_impl` / `net/tasks/control_fastpath.rs`.
//!
//! The receiver's ~90 locals stay LOCALS. Unlike the sender's, they are not
//! read by any other phase — the whole task is one function now, so there is
//! nothing for a `ReceiverState` struct to unblock. Introducing one would add
//! a `st.` prefix to ~400 lines and buy nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bytes::Bytes;
use dashmap::DashMap;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::block_arq::BlockArq;
use super::control_msg::{ControlCtx, handle_control_message};
use super::framing;
use super::reorder::ReorderBuffer;
use super::{
    BLOCK_REORDER_MAX_BLOCKS, BLOCK_REORDER_MIN_HOLD, CopaFeed, DerivedRoundEcho,
    GAP_ACK_MIN_INTERVAL, GEN_PIPE_MAX_GENS, LOOP_WAKE_US, PathBatchTracker, REPORT_INTERVAL,
    collect_gen_deficits, create_window_decoder, deliver_packet, extract_window_packets,
    hole_nack_refresh, hole_refresh_all, horizon_gate_deficits, now_us, received_sack_ranges,
    shed_armed, shed_recv_budget_ok, shed_recv_hold, stall_threshold_us, window_ack_emission,
};
use crate::control::FecRateController;
use crate::fec::{FecBackend, FecDecoder, WindowDecoder};
use crate::monitor::stats::SharedStats;
use crate::scheduler::Scheduler;
use crate::transport::{ControlMessage, QuicTransport, WireMessage};

/// The engine's receiver task. Consumes `(path_id, WireMessage)` from the
/// shared inbound channel until the channel closes or shutdown fires.
///
/// Parameters are the block's former captures, in the order `run_impl`
/// declares them: same clones, same values, moved at the same spawn.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_receiver(
    mut recv_shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    mut msg_rx: tokio::sync::mpsc::Receiver<(u32, WireMessage)>,
    sent_counts: Arc<DashMap<(u64, u32), u32>>,
    recv_copa_feed: Option<Arc<CopaFeed>>,
    recv_tun_tx: tokio::sync::mpsc::Sender<Bytes>,
    recv_scheduler: Arc<parking_lot::Mutex<Scheduler>>,
    recv_fec: Arc<parking_lot::Mutex<FecRateController>>,
    recv_decoders: Arc<DashMap<u64, Box<dyn FecDecoder>>>,
    recv_fec_backend: FecBackend,
    recv_transport: Arc<QuicTransport>,
    recv_block_arq: Arc<parking_lot::Mutex<BlockArq>>,
    recv_batch_counter: Arc<AtomicU64>,
    recv_path_tracking: Arc<DashMap<u32, PathBatchTracker>>,
    recv_stats: Arc<SharedStats>,
    recv_symbol_size: u16,
    recv_window_mode: bool,
    recv_window_reliable: bool,
    recv_window_ooo: bool,
    recv_win_cap: u64,
    recv_window_ack: Arc<AtomicU64>,
    recv_window_generation: bool,
    recv_deficit_tx: tokio::sync::mpsc::Sender<Vec<(u64, u32)>>,
    recv_nack_tx: Option<tokio::sync::mpsc::Sender<Vec<(u64, u64)>>>,
    recv_sack_tx: Option<tokio::sync::mpsc::Sender<Vec<(u64, u64)>>>,
    reasm_bdp_on: bool,
    ack_merge_recv: bool,
    recv_diag_on: bool,
    rdiag_probe: tokio::sync::mpsc::WeakSender<(u32, WireMessage)>,
    recv_gates: crate::gates::RuntimeGates,
    // The two `config` fields Rust 2021's disjoint capture copied into the
    // spawned block (see the module header).
    recv_reorder_timeout_ms: u64,
    recv_reorder_max_size: usize,
) {
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
    } else if recv_window_mode && recv_reorder_timeout_ms > 0 {
        Some(ReorderBuffer::new(recv_reorder_timeout_ms, recv_reorder_max_size))
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
    // goal-gate "The Derived Recovery Clamp" (`RWM_DERIVED_SWEEP`, default
    // OFF): the stalled-hole refresh cadence on the derived round.
    let recv_derived_sweep = recv_gates.derived_sweep;
    let recv_rack_clocks = recv_gates.rack_clocks;
    let recv_rack_reo_mult = recv_gates.rack_reo_mult;
    let recv_quantile_clocks = recv_gates.quantile_clocks;
    // Paper §16.76: WHICH of the two rival `W` laws the armed clock evaluates.
    // `cantelli` on every shipped arm; read only when the quantile gate is on.
    let recv_w_form = recv_gates.w_form;
    // The contract's alpha at the RECEIVER. The protocol hint is not plumbed
    // to this task, so the quantile arm reads the Auto point of the dial here
    // while the sender reads the tunnel's own. Recorded as a stated limitation
    // of the REFUTED arm (paper 16.68) rather than papered over: it means the
    // two sites can disagree on alpha at Realtime and Bulk, which is a reason
    // this arm may not be scored across hints without plumbing the hint first.
    let recv_contract_alpha_base =
        crate::net::contract_alpha(crate::control::fec_rate::ProtocolHint::Auto);
    // `RWM_ALPHA_OVERRIDE` (EXPERIMENT, absent by default) replaces it when
    // set. It is a NUMBER and not a hint mapping, so — unlike the contract's
    // α — it reaches BOTH sites identically and the hint-plumbing limitation
    // above does not apply to an overridden arm. That is the one respect in
    // which a swept arm is better defined than the contract arm, and it is
    // why the sweep can be scored across sites at all.
    let recv_contract_alpha = crate::net::resolved_alpha(
        crate::control::fec_rate::ProtocolHint::Auto,
        recv_gates.alpha_override,
    );
    eprintln!(
        "{}",
        crate::net::qalpha_report_line(
            "receiver",
            recv_quantile_clocks,
            recv_w_form,
            recv_contract_alpha_base,
            recv_gates.alpha_override,
            recv_contract_alpha,
        )
    );
    // `[QCLK]` at the receiver — the hole-refresh cadence this site realizes.
    // NOTE the harness SIGKILLs the server, so a `Drop` never reaches a server
    // log (the `[RFA]` lesson); this gauge is therefore ALSO emitted on the
    // same 1 s cadence, last line wins.
    let mut recv_qclk_echo = crate::net::QuantileClockGauge::new(
        "receiver",
        recv_quantile_clocks,
        recv_w_form,
        recv_contract_alpha,
    );
    let mut qclk_report_at = Instant::now();
    // The receiver site's one-shot mechanism-liveness echo (ACTIVE +
    // DIVERGED). Observation only; emitted on the armed arm alone.
    let mut recv_derived_echo = DerivedRoundEcho::default();
    // `[RACK]` bind-fraction gauge at the receiver site (paper §16.68).
    let mut recv_rack_echo =
        crate::net::RackClockGauge::new(recv_rack_clocks, recv_rack_reo_mult);
    // `[RFA]` is a PLAIN-WINDOW instrument (the same configuration scope the
    // sender's `fa=` has — `recv_nack_tx` is None under generation). The line
    // echoes which machine it measured so no row is ever read out of scope.
    recv_rack_echo.set_recv_generation(recv_window_generation);
    // `[RFA]` cadence — see the readout site. Cumulative, 1 s, last line wins.
    let mut rfa_report_at = Instant::now();
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
    // ack-merge CONTROL-DATAGRAM DENSITY gauge (goal-gate "Unlock The
    // Default 1", RWM_DIAG only — behavior-inert). `[CTLD] p<id>
    // tx=<n> rx=<n>` = quinn's own `frame_tx.datagram` / `frame_rx.datagram`
    // for the path, read at the receiver: since a window-mode RECEIVER
    // sends nothing but control datagrams, `tx` IS the control-frame count
    // and `tx / MB` IS the density prediction 1 is about.
    //
    // THE INSTRUMENT LESSON, recorded where the instrument lives: the
    // pre-registration proposed measuring this off the ACK-direction
    // qdisc PACKET counters (`QDISC srv0/srv1`). The pre-battery smoke
    // showed that cannot work — those packets are dominated by quinn's own
    // transport-level ACK cadence (~1 per 2 data packets: 55–57k ack-side
    // packets against 89k data packets), and our control datagrams ride
    // COALESCED inside them. Merging two datagram frames into one changes
    // the frame count, not the packet count. Frames are the quantity the
    // mechanism is about, so frames are what this counts.
    let mut ctld_last_report = Instant::now();
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
    // Goal-gate "Unlock The Default 2", part 3a — the receiver twin of
    // the derived stall gauge (`RWM_SIDLE_DERIVED`, DIAG-only). Same law
    // (`stall_threshold_us`) over the Data-ARRIVAL event stream: the
    // legacy 3 ms is 3 × an ASSUMED nominal inter-arrival interval, and
    // the measured one is what the law substitutes. `idle=` is printed
    // unchanged; `idle2=` is added beside it. Recomputed once per 1 s
    // print window from that window's own arrival count.
    let widle_derived = crate::scheduler::sidle_derived_active();
    let mut widle_evt_us: u64 = LOOP_WAKE_US;
    let mut widle_thr_us: u64 = stall_threshold_us(LOOP_WAKE_US);
    let mut widle_evt_n: u64 = 0;
    let mut widle2_us: u64 = 0;
    let mut widle2_n: u64 = 0;
    let mut widle2_max_us: u64 = 0;

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
    let block_inorder_enabled = !recv_window_mode && recv_reorder_timeout_ms > 0;
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
                let (srtt, srtt_jitter_us, min_rtt, sigma_us, w_q_us) = {
                    let sched = recv_scheduler.lock();
                    let live: Vec<_> = sched
                        .live_paths()
                        .into_iter()
                        .filter_map(|pid| sched.path(pid))
                        .collect();
                    // Same path set for all three, so the DERIVED floor, the
                    // RACK law's `min_RTT` and their clock can never come from
                    // different paths.
                    (
                        live.iter().map(|p| p.srtt()).max(),
                        live.iter().map(|p| p.rtt_jitter_us()).max().unwrap_or(0),
                        live.iter().filter_map(|p| p.min_rtt()).min(),
                        live.iter().filter_map(|p| p.rtt_sigma_us()).max(),
                        // §16.76's `W_q`, MAX over the SAME path set, for the
                        // same reason σ is. `None` (window short of `N(α)`)
                        // ⇒ the quantile arm falls through — UNSCOREABLE.
                        live.iter()
                            .filter_map(|p| p.rtt_tail_quantile_us(recv_contract_alpha))
                            .max(),
                    )
                };
                let deadline = if recv_window_reliable {
                    // Reliable policy: the hole is never given up on —
                    // this timer instead re-advertises the gap (SACK
                    // WindowAck) at 2×SRTT cadence until recovered.
                    // goal-gate "The Derived Recovery Clamp": under
                    // `RWM_DERIVED_SWEEP` the cadence is the DERIVED round
                    // (no ceiling); OFF ⇒ `hole_nack_refresh` verbatim.
                    // Paper §16.68: `RWM_RACK_CLOCKS` REPLACES
                    // `RWM_DERIVED_SWEEP` here — rival laws for one quantity.
                    let refresh = hole_refresh_all(
                        recv_w_form,
                        recv_quantile_clocks,
                        recv_rack_clocks,
                        recv_derived_sweep,
                        srtt,
                        srtt_jitter_us,
                        min_rtt,
                        sigma_us,
                        w_q_us,
                        recv_rack_reo_mult,
                        recv_contract_alpha,
                    );
                    // Every arm, control included — see the sender's twin.
                    recv_qclk_echo.record(
                        refresh.as_micros() as u64,
                        srtt.map_or(0, |s| s.as_micros() as u64),
                        sigma_us,
                        w_q_us,
                    );
                    if recv_rack_clocks {
                        if let (Some(sv), Some(mv)) = (srtt, min_rtt) {
                            recv_rack_echo.record(
                                sv.as_micros() as u64,
                                mv.as_micros() as u64,
                                refresh.as_micros() as u64,
                                hole_nack_refresh(srtt).as_micros() as u64,
                            );
                        }
                    }
                    if recv_derived_sweep && !recv_rack_clocks {
                        if let Some(s) = srtt {
                            recv_derived_echo.observe(
                                "receiver-hole-refresh",
                                s.as_micros() as u64,
                                srtt_jitter_us,
                                refresh.as_micros() as u64,
                                hole_nack_refresh(srtt).as_micros() as u64,
                            );
                        }
                    }
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
                    // 3a: the SAME gap against the DERIVED threshold.
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
                            // Re-derive from THIS window's measured
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
                        eprintln!(
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
                    if let Some(path) = sched.path_mut(path_id) {
                        path.estimator.record_arrival(batch_send_ts, arrival_us);
                        // Update jitter in monitoring stats
                        if let Some(ps) = recv_stats.path(path_id) {
                            ps.jitter_us.store(path.estimator.jitter_us() as u64, Ordering::Relaxed);
                        }
                    }
                }

                // Track batch sequences for loss detection (ADR-0003)
                // ack-merge (RWM_ACK_MERGE): read the tracker's CUMULATIVE
                // totals in the same borrow — they are the v6 WindowAck
                // counter payload, i.e. the legacy Ack's (expected,
                // received) pair carried as running sums so the sender can
                // diff them. Cumulative, not per-ack: a dropped control
                // datagram then costs nothing.
                let (expected, _received_total, cum_expected, cum_received) = {
                    let mut tracker = recv_path_tracking
                        .entry(path_id)
                        .or_insert_with(PathBatchTracker::new);
                    let (e, r) = tracker.record_batch(batch_seq, symbol_count);
                    (e, r, tracker.total_expected, tracker.total_received)
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
                        // ── `[RFA]`: THE REALIZED FALSE-REPAIR CLASS, read
                        // BEFORE the symbol is fed (the probe is about the
                        // state this arrival is ABOUT to change). See the
                        // event-class definition in `net/mod.rs` beside
                        // `RACK_SPURIOUS_BUDGET`. `seq_probe` is `&self` and
                        // the counters feed nothing but the gauge: READ-ONLY,
                        // no control flow, no gate, always fed so the datum
                        // exists on every arm exactly as `fa=` does.
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
                        for (seq, sym_data) in recovered {
                            // A seq that came out of the decoder rather than
                            // off its OWN source arrival was reconstructed
                            // from coded repair — `[RFA]`'s `fill_coded`, a
                            // repair that WORKED. (A source arrival can
                            // cascade other seqs out of the row space; those
                            // are coded fills too, hence the seq test rather
                            // than `symbol.is_repair` alone.)
                            if symbol.is_repair || seq != symbol.block_id {
                                recv_rack_echo.record_recv_coded_fill();
                            }
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
                        }
                    }

                    // ── `[RFA]` PERIODIC READOUT ──────────────────────────
                    // The gauge's `Drop` is the authoritative emission, but
                    // every L1 harness in `tools/l1` SIGKILLs the server, so
                    // a receiver-site `Drop` is not reachable there — which
                    // is why the server log has never carried a `[RACK]`
                    // line on ANY arm. Cumulative counters on a 1 s cadence
                    // under the EXISTING diagnosis gates (no new gate; both
                    // already ride the two-sided `[GATES]` echo), so the
                    // LAST line is the reading whatever kills the process.
                    // A run with no repair-class event stays silent.
                    if (recv_gates.diag || fdiag_on)
                        && recv_rack_echo.is_receiver_site()
                        && rfa_report_at.elapsed() >= Duration::from_secs(1)
                    {
                        rfa_report_at = Instant::now();
                        eprintln!("{}", recv_rack_echo.rfa_line());
                    }

                    // ── `[QCLK]` PERIODIC READOUT ─────────────────────────
                    // Same SIGKILL reason, same 1 s cadence, same existing
                    // gates, same last-line-wins convention. A run that never
                    // evaluated a recovery clock stays silent, so an absent
                    // line reads as an unreached evaluation site and never as
                    // an unset gate.
                    if (recv_gates.diag || fdiag_on)
                        && recv_qclk_echo.evals() > 0
                        && qclk_report_at.elapsed() >= Duration::from_secs(1)
                    {
                        qclk_report_at = Instant::now();
                        eprintln!("{}", recv_qclk_echo.line());
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
                    // ack-merge (RWM_ACK_MERGE): what the ack ADVERTISES
                    // (the cumulative point + SACK ranges) is unchanged —
                    // `advertise` is the shipped predicate verbatim, so
                    // GAP_ACK_MIN_INTERVAL still rate-limits gap reports
                    // and the depth-16 nack/sack channels see no new
                    // pressure. What changes is only WHETHER A DATAGRAM
                    // IS SENT: under the merge this ack also carries the
                    // suppressed legacy Ack's payload, so it must go out
                    // once per data message — exactly the cadence the Ack
                    // had. Gate off ⇒ `emit == advertise` ⇒ byte-identical.
                    let (emit_ack, advertise) = window_ack_emission(
                        cumulative_advanced,
                        gap_report_due,
                        ack_merge_recv,
                    );
                    if emit_ack {
                        if cumulative_advanced {
                            last_advertised_ack = highest_delivered_seq;
                        }
                        // SACK ranges: what WAS received beyond the
                        // cumulative point (not what's missing). Only on
                        // an advertising ack — a merge-only ack carries
                        // the counters and the echo, never a gap report
                        // (that is what preserves the gap rate limit).
                        let sack_ranges = if advertise {
                            last_gap_ack_seen = highest_seen_seq;
                            last_gap_ack_time = Instant::now();
                            // A gap-bearing ack IS a hole re-advertisement:
                            // push the reliable-mode refresh timer out.
                            last_hole_nack_at = last_gap_ack_time;
                            received_sack_ranges(
                                &received_seqs,
                                highest_delivered_seq,
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
                            // v6 ack-merge counters: the legacy Ack's
                            // (expected, received) pair as per-path running
                            // sums. Always populated on a data-triggered
                            // ack (gate or no gate — one wire format per
                            // binary); the sender only CONSUMES them under
                            // RWM_ACK_MERGE.
                            cum_expected,
                            cum_received,
                        };
                        if let Err(e) = recv_transport.send_control_datagram(path_id, ack_msg) {
                            debug!(?e, path_id, "failed to send WindowAck");
                        }
                    }

                    // ack-merge CONTROL-DATAGRAM DENSITY gauge (prediction
                    // 1's instrument — see the declaration of
                    // `ctld_last_report` for why this and not the qdisc
                    // packet counters). Cumulative per path, 1 Hz.
                    if recv_diag_on
                        && ctld_last_report.elapsed() >= Duration::from_secs(1)
                    {
                        ctld_last_report = Instant::now();
                        let mut line = String::from("[CTLD]");
                        for pid in recv_scheduler.lock().live_paths() {
                            if let Some((rx, tx)) =
                                recv_transport.datagram_frame_stats(pid)
                            {
                                line.push_str(&format!(" p{pid} tx={tx} rx={rx}"));
                            }
                        }
                        eprintln!("{line}");
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
                //
                // ack-merge (RWM_ACK_MERGE, goal-gate "Unlock The Default
                // 1"): THIS is the second control datagram per data
                // message. Its send site sits after the window/block
                // branch closes, so it has always fired in WINDOW mode too
                // (the recorded correction in the sender's Ack arm) — one
                // legacy Ack for every SACK WindowAck, against quinn-perf's
                // ~1 ack per ~24 packets. Under the merge, window mode
                // suppresses it entirely: its payload rides the WindowAck's
                // v6 cumulative counters and its consumers are re-homed
                // onto the counter diff. BLOCK MODE IS UNTOUCHED — it has
                // no WindowAck to merge into, `block_arq` is live only
                // there, and its dup-ack loss channel keeps the per-batch
                // Ack it is built on.
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
                recv_scheduler
                    .lock()
                    .path_mut(path_id)
                    .map(|p| p.estimator.record_batch(expected, symbol_count));

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
                    &ControlCtx {
                        scheduler: &recv_scheduler,
                        fec_controller: &recv_fec,
                        decoders: &recv_decoders,
                        sent_counts: &sent_counts,
                        transport: &recv_transport,
                        fec_backend: recv_fec_backend,
                        stats: &recv_stats,
                        nack_tx: recv_nack_tx.as_ref(),
                        block_arq: if recv_window_mode { None } else { Some(&recv_block_arq) },
                        batch_counter: Some(&recv_batch_counter),
                        peer_window_ack: if recv_window_mode { Some(&recv_window_ack) } else { None },
                        deficit_tx: if recv_window_generation { Some(&recv_deficit_tx) } else { None },
                        sack_tx: recv_sack_tx.as_ref(),
                        copa_feed: recv_copa_feed.as_ref(),
                        mstar_anchor: recv_gates.mstar_anchor,
                    },
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
}
