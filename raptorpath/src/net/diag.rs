//! The window sender's PERIODIC DIAG REPORT: the `[DIAG]` / `[C8CONV-S]`
//! lines and the counters that feed them.
//!
//! The report reads locals of every other phase of the sender loop and writes
//! nothing any of them reads, so it is read-only with respect to the data
//! plane. Ordering constraints it keeps:
//!   * the `if pol.diag_on` guard is at the call site, so the report costs
//!     nothing on the shipped path;
//!   * it takes one scheduler lock, scoped to the per-path `pp` string, with
//!     `expire_in_flight()` inside it;
//!   * the atomics (`window_ack_seq`, the `stats.fec` symbol totals) are read
//!     with `Ordering::Relaxed`, and the per-iteration `sidle` handoff probe
//!     runs before the 250 ms window test.
//!
//! State: [`DiagState`] holds the counters whose only consumer is this
//! report. Most are accumulated by other phases (the recovery plane's
//! `mpd_*`, the conversion `c8c_*`, the GDIAG stall attribution) and
//! read/reset here, hence one struct threaded as `&mut`. Two families stay
//! locals of `run_window_sender`:
//!   * `mpd_pf_floor` / `mpd_pf_clock` / `mpd_pf_sum` are `Cell`s captured by
//!     the `mp_thr_of` closure in the recovery phase; moving them into
//!     `DiagState` would make that closure borrow `dg` across a block that
//!     also increments `dg.mpd_*`. They are passed in by reference.
//!   * `wnd2_frontier_last` / `wnd2_frontier_change_us` feed only the
//!     `wnd2=`/`relgap=` gauge and are passed in by value.
//!
//! Not covered here: the receiver-side `[RCV]` / `[RDIAG]` / `[FDIAG]` /
//! `[C8CONV-R]` gauges, the span-law `[SPAN]` trace, `[GPIPE]`, `[PFRAC]`
//! and the generation-lifecycle bookkeeping that feeds `gl_sum`.

use std::cell::Cell;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::emit_source::SenderState;
use super::sender_policy::SenderPolicy;
use super::{CopaFeed, EchoRatioMin, LOOP_WAKE_US, now_us, stall_threshold_us};
use crate::monitor::stats::SharedStats;
use crate::scheduler::Scheduler;
use crate::transport::QuicTransport;

/// Sender emission-gap gauge (`RWM_DIAG` only) — cumulative time in inter-emission gaps
/// ≥ 3 ms (src+cod handoffs to the transport observed per loop iteration;
/// the loop wakes ≥ every 1 ms, so gap edges are observed within ~1 ms).
/// Prices accounting term (b): engine-caused wire idle during recovery
/// rounds. `sidle=<cum ms>/<n>/<max ms>` in the [DIAG] line; the receiver's
/// [WIDLE] inter-arrival gauge is the wire-truth counterpart.
const SIDLE_GAP_MIN_US: u64 = 3_000;

/// Every counter whose only consumer is the periodic DIAG report.
///
/// Behaviour-inert by construction: nothing here is read by an emission,
/// admission, pacing or recovery decision.
pub(crate) struct DiagState {
    // ── GDIAG ─────────────────────────────────────────────────────────────
    // Time-weighted attribution of the generation-mode sender loop to the
    // gate that is binding its wire emission each instant. In coded-wire
    // generation mode the paced coded block is the data plane, so whichever
    // gate stops it is the throughput binder. States (post-emission):
    //   emit    — emitted ≥1 coded this iteration (link-flowing)
    //   budget  — wants_coding=false with sealed gens retained: every active
    //             generation is at its ceil(len·(1+r)) proactive budget and
    //             the sender is waiting on the ACK/deficit round (the
    //             window-advance serialization)
    //   fill    — wants_coding=false because the head generation has not
    //             sealed yet (waiting on TUN intake / store backpressure)
    //   target  — ack-clocked flow window `target` exhausted
    //   tokens  — pace token bucket dry (the delivered-rate-EWMA pacer)
    //   cwnd    — in-flight congestion cap
    // Also per-generation lifecycle (GLIFE): anchor → (first_src, sealed,
    // last_emit) µs; on the ack passing a generation its fill/code/ack-wait
    // phases are accumulated. All gated on RWM_DIAG.
    pub gd_last_us: u64,
    /// [emit, budget, fill, target, tokens, cwnd]
    pub gd_us: [u64; 6],
    /// (fill_us, code_us, wait_us, n) accumulated over completed generations.
    pub gl_sum: (u64, u64, u64, u64),

    // ── The window sender's wait-reason histogram (`wait[..]`, RWM_DIAG) ──
    //
    // `gd_us` above attributes the generation plane only (its buckets do not
    // exist under `RWM_GEN=0`). This one splits the sender loop's wall time
    // into the loop BODY (compute) and the `select!` AWAIT, and charges the
    // await to the arm that woke it: "when the sender is not sending, what
    // is it waiting on?". It is defined whether or not generation coding is
    // on, and body + await sum to the whole loop.
    //
    //   busy    — loop-body time: from one `select!` resolution to the start
    //             of the next `select!` await (emission, ack/SACK processing,
    //             the rate law, ...). Until Stage 1d this time was charged to
    //             the NEXT arm that woke (usually `tun`), which is how a
    //             CPU-bound sender (6 ms/iteration) read as "productive".
    //             Resolution: the split is taken at the `select!` statement's
    //             boundaries, so the few statements inside an arm's own body
    //             (the flush arm's one-symbol emit, the deficit-report
    //             rebuild) count with that arm's await; everything after the
    //             `select!` — where the emission and ack work runs — is busy.
    //
    // The eight arm buckets hold AWAIT time only:
    //
    //   tun     — `tun.read_packet()` produced a packet: intake was ready.
    //             The only arm that carries new source data (a large share
    //             is productive only while `busy` is small).
    //   paused  — the 1 ms backpressure poll: the store is full (`tx_paused`),
    //             i.e. the outstanding-data limit binds.
    //   pace    — the 1 ms pacing poll: the `RWM_CC_PACE` source token
    //             bucket is dry. Zero whenever `RWM_CC_PACE=0`.
    //   gen     — the 1 ms generation emission poll.
    //   nack    — a gap report arrived from the receiver.
    //   defc    — a generation-deficit report arrived.
    //   tail    — the tail-ARQ sweep deadline fired.
    //   flush   — the packer's partial-symbol flush timeout fired.
    //
    // The printed line (one per DIAG period):
    //
    //   wait[tun=A% paused=B% ... flush=H% n=N us=U busy=P% busy_us=V]
    //
    //   arm %   — share of the period's AWAIT time (Σ arm buckets), so the
    //             eight arm shares sum to 100 % of the awaiting;
    //   n       — loop iterations charged this period;
    //   us      — the whole loop wall this period, await + body (unchanged
    //             meaning: us/n is still the mean iteration period);
    //   busy    — body time as a share of `us`; busy_us its absolute µs.
    //             A sender pinned by compute shows busy → 100 %.
    //
    // All gated on RWM_DIAG.
    /// The last `select!` resolution instant (µs): the start of the loop body.
    pub wait_last_us: u64,
    /// The current `select!` await's start instant (µs).
    pub wait_await_us: u64,
    /// Loop-body time this window (µs), the `busy` bucket.
    pub wait_busy_us: u64,
    /// [tun, paused, pace, gen, nack, defc, tail, flush] — await time only.
    pub wait_us: [u64; 8],
    /// Iterations charged into `wait_us` this window (all buckets).
    pub wait_n: u64,

    // ── The recovery-plane trace (RWM_DIAG) ──────────────────────────────
    // Gap-report volume, per-cause suppression, fired-retransmit age
    // attribution (young = the law's spurious class), per-flight-path and
    // per-retx-path emission. Cumulative; printed as `mpr[..]`. The
    // P_lost-branch retransmit count (`mpd_plost_retx`) lives in
    // `SenderState` — the emission step writes it.
    pub mpd_gap_reports: u64,
    pub mpd_gap_seqs: u64,
    pub mpd_supp_cool: u64,
    pub mpd_supp_age: u64,
    pub mpd_supp_law: u64,
    pub mpd_stale: u64,
    pub mpd_fired_young: u64,
    pub mpd_fired_ripe: u64,
    pub mpd_fired_fast: u64,
    pub mpd_coalesced: u64,
    pub mpd_age_ms_sum: f64,
    pub mpd_fired_flight: HashMap<u32, u64>,
    pub mpd_fired_on: HashMap<u32, u64>,

    // ── Slow-path conversion gauges (RWM_DIAG only) ───────────────────────
    // Why do slow-path symbols not convert to delivered goodput on a
    // heterogeneous dual-path cell?
    //  * c8c_src_placed[p]  — cumulative first source placements per path
    //    (candidate (a), placement starvation: compare against the path's
    //    capacity share from btlbw/qdisc truth). Lives in `SenderState`.
    //  * c8c_retx_orig[p]   — cumulative targeted retransmits whose original
    //    placement path was p (candidate (d), arrival-misalignment: slow-
    //    placed symbols being re-served spuriously shows up as
    //    retx_orig[slow]/src_placed[slow] ≫ the path's realized loss rate).
    //  * c8c_stall_ms/n[p]  — cumulative frontier-stall wall time (ack
    //    advance gaps ≥ 5 ms) attributed to the owner path of the blocking
    //    hole seq = prev_ack+1 at resolution (candidate (c), HoL coupling:
    //    which path's holes serialize the cumulative frontier).
    //    The receiver-side [C8CONV-R] gauge carries the arrival-side view
    //    (first-copy vs duplicate per path + frontier lead + unblock
    //    attribution — candidates (a)/(b)/(d)).
    pub c8c_retx_orig: HashMap<u32, u64>,
    pub c8c_stall_ms: HashMap<u32, u64>,
    pub c8c_stall_n: HashMap<u32, u64>,

    // ── The report clock and its per-window deltas ───────────────────────
    pub diag_start_us: u64,
    pub diag_last_us: u64,
    pub diag_last_ack: u64,
    pub diag_last_src: u64,
    pub diag_last_cod: u64,
    pub diag_paused_iters: u64,
    pub diag_total_iters: u64,

    // ── Wedge forensics (RWM_DIAG only) ───────────────────────────────────
    // Cumulative tail ARQ sweeps fired, SACK-gap retransmits actually sent,
    // gaps discarded for exhausted budget, and the live effective rate — the
    // wedge shows good=0 with in_flight=0 for tens of seconds and these name
    // which stage of the reactive-repair chain is dead.
    pub diag_sweeps: u64,
    /// `[DIAG] retx=`: cumulative SOURCE retransmits (SACK-gap copies and
    /// request-law copies). Coded repair — proactive, margin, or a
    /// request-law equation — is `cod=` (`total_repair_symbols`) and never
    /// counted here, so `src`/`cod`/`retx` partition the reactive handoffs.
    pub diag_retx: u64,
    pub diag_gaps_dropped: u64,
    pub diag_eff_rate: f64,

    // ── The emission-gap gauge (see `SIDLE_GAP_MIN_US`) ──────────────────
    pub sidle_last_total: u64,
    pub sidle_last_change_us: u64,
    pub sidle_us: u64,
    pub sidle_n: u64,
    pub sidle_max_us: u64,

    // ── Its derived twin (`RWM_SIDLE_DERIVED`, DIAG-only) ─────────────────
    // The same event stream accumulated against `stall_threshold_us(evt_us)`
    // — the 3 ms constant re-expressed as 3 × the measured mean
    // inter-emission-event interval, floored at 3 ms and capped at the
    // hole-refresh cadence. Printed as `sidle2=` beside `sidle=`, with
    // `evt=<µs>` (the measured interval) and `sthr=<µs>` (the live
    // threshold). `sidle2 ≤ sidle` by construction.
    //
    // `sidle_evt_us` is recomputed once per DIAG window from the events that
    // window observed (zero hot-loop cost). It starts at the loop wake, so
    // before the first window the derived threshold is the 3 ms constant.
    pub sidle_evt_us: u64,
    pub sidle_thr_us: u64,
    pub sidle_evt_n: u64,
    pub sidle2_us: u64,
    pub sidle2_n: u64,
    pub sidle2_max_us: u64,

    /// `relgap=<cur>/mx<max>ms` — time since the release
    /// frontier (max of SACK-release max and cum ack) last advanced, max per
    /// DIAG window: the release-clumping gauge. The frontier itself
    /// (`wnd2_frontier_last` / `wnd2_frontier_change_us`) stays a local of
    /// `run_window_sender`.
    pub wnd2_relgap_max_us: u64,
}

impl DiagState {
    /// The sender loop is about to `select!`: the time since the last
    /// resolution was loop body — charge it to `busy`. Returns the body µs.
    pub fn wait_enter_await(&mut self, now_us: u64) -> u64 {
        let body = now_us.saturating_sub(self.wait_last_us);
        self.wait_busy_us += body;
        self.wait_await_us = now_us;
        body
    }

    /// The `select!` resolved at `now_us`: returns the AWAIT µs (to charge to
    /// the arm that woke) and starts the next body.
    pub fn wait_resolve(&mut self, now_us: u64) -> u64 {
        let dt = now_us.saturating_sub(self.wait_await_us.max(self.wait_last_us));
        self.wait_last_us = now_us;
        dt
    }

    /// Build the report's counters. Every initializer is pure (zeroed
    /// counters, empty maps, and `stall_threshold_us(LOOP_WAKE_US)`); the four
    /// wall-clock stamps are passed in so the caller samples them at its own
    /// points in setup.
    pub fn new(
        gd_last_us: u64,
        diag_start_us: u64,
        diag_last_us: u64,
        sidle_last_change_us: u64,
    ) -> Self {
        Self {
            gd_last_us,
            gd_us: [0u64; 6],
            gl_sum: (0, 0, 0, 0),
            // Same wall-clock stamp the generation attribution starts from:
            // both bracket the same loop, so a divergent origin would make
            // the two attributions disagree about the first window's length.
            wait_last_us: gd_last_us,
            wait_await_us: gd_last_us,
            wait_busy_us: 0,
            wait_us: [0u64; 8],
            wait_n: 0,
            mpd_gap_reports: 0,
            mpd_gap_seqs: 0,
            mpd_supp_cool: 0,
            mpd_supp_age: 0,
            mpd_supp_law: 0,
            mpd_stale: 0,
            mpd_fired_young: 0,
            mpd_fired_ripe: 0,
            mpd_fired_fast: 0,
            mpd_coalesced: 0,
            mpd_age_ms_sum: 0.0,
            mpd_fired_flight: HashMap::new(),
            mpd_fired_on: HashMap::new(),
            c8c_retx_orig: HashMap::new(),
            c8c_stall_ms: HashMap::new(),
            c8c_stall_n: HashMap::new(),
            diag_start_us,
            diag_last_us,
            diag_last_ack: 0,
            diag_last_src: 0,
            diag_last_cod: 0,
            diag_paused_iters: 0,
            diag_total_iters: 0,
            diag_sweeps: 0,
            diag_retx: 0,
            diag_gaps_dropped: 0,
            diag_eff_rate: 0.0,
            sidle_last_total: 0,
            sidle_last_change_us,
            sidle_us: 0,
            sidle_n: 0,
            sidle_max_us: 0,
            sidle_evt_us: LOOP_WAKE_US,
            sidle_thr_us: stall_threshold_us(LOOP_WAKE_US),
            sidle_evt_n: 0,
            sidle2_us: 0,
            sidle2_n: 0,
            sidle2_max_us: 0,
            wnd2_relgap_max_us: 0,
        }
    }
}

/// The per-iteration inputs the report reads out of the OTHER phases of the
/// sender loop. Grouped only to keep the call readable — every field is a
/// plain copy of the identically-named local, taken at the call site, and the
/// body reads them under their original names.
pub(crate) struct DiagInputs<'a> {
    /// The live retention/flow-control verdict (dynamic store-cap block).
    pub tx_paused: bool,
    pub store_len: usize,
    pub effective_store_cap: usize,
    /// Per-path echo-ratio state (K_i), refreshed by the dyn-cap throttle.
    pub percap_k: &'a HashMap<u32, EchoRatioMin>,
    /// RWM_STORE_SACK_RELEASE: currently released / cumulative slots.
    pub sack_released: &'a BTreeSet<u64>,
    pub sack_released_total: u64,
    /// RWM_POOL_ANCHOR: honest dual-store engagement + Σ honest caps.
    pub pa_engaged: bool,
    pub pa_sum: f64,
    /// The live release frontier (the `wnd2=`/`relgap=` gauge's input).
    pub wnd2_frontier_last: u64,
    pub wnd2_frontier_change_us: u64,
    /// The live NACK repair budget and the generation-mode pacing EWMA.
    pub cached_nack_budget: u64,
    pub gen_rate_ewma: f64,
    /// The patience-floor split counters. `Cell` because the evaluation
    /// happens inside a shared closure in the recovery phase — see the
    /// module header.
    pub mpd_pf_floor: &'a Cell<u64>,
    pub mpd_pf_clock: &'a Cell<u64>,
    pub mpd_pf_sum: &'a Cell<u64>,
}

/// The shared engine handles the report reads. All by shared reference: the
/// report takes one scheduler lock (scoped to the per-path `pp` string) and
/// otherwise only reads atomics and transport gauges.
pub(crate) struct DiagCtx<'a> {
    pub scheduler: &'a Arc<parking_lot::Mutex<Scheduler>>,
    pub transport: &'a Arc<QuicTransport>,
    pub stats: &'a Arc<SharedStats>,
    pub window_ack_seq: &'a Arc<AtomicU64>,
    pub copa_feed: &'a Option<Arc<CopaFeed>>,
}

/// Emit the periodic `[DIAG]` / `[C8CONV-S]` report.
///
/// Called once per sender-loop iteration under the caller's `if pol.diag_on`
/// guard: the per-iteration part (paused-iteration accounting and the
/// emission-gap probe) runs every time, the printed report every 250 ms.
#[allow(clippy::too_many_arguments)]
pub(crate) fn report(
    st: &SenderState,
    pol: &SenderPolicy,
    dg: &mut DiagState,
    ctx: DiagCtx<'_>,
    inp: DiagInputs<'_>,
    symbol_size: u16,
    reliable: bool,
    generation: bool,
) {
    // Re-bind the inputs under the names the body reads.
    let DiagCtx {
        scheduler,
        transport,
        stats,
        window_ack_seq,
        copa_feed,
    } = ctx;
    let DiagInputs {
        tx_paused,
        store_len,
        effective_store_cap,
        percap_k,
        sack_released,
        sack_released_total,
        pa_engaged,
        pa_sum,
        wnd2_frontier_last,
        wnd2_frontier_change_us,
        cached_nack_budget,
        gen_rate_ewma,
        mpd_pf_floor,
        mpd_pf_clock,
        mpd_pf_sum,
    } = inp;

    dg.diag_total_iters += 1;
    if tx_paused {
        dg.diag_paused_iters += 1;
    }
    let dnow = now_us();
    // Emission-gap gauge (see `SIDLE_GAP_MIN_US`): observe the
    // cumulative wire handoff count (src + coded + source copies) once per
    // iteration; a change closes the current gap — accumulate it when
    // it is a stall-class gap (≥ 3 ms), not a pacing interval.
    {
        let wt = stats.fec.total_source_symbols.load(Ordering::Relaxed)
            + stats.fec.total_repair_symbols.load(Ordering::Relaxed)
            + stats.fec.total_copy_symbols.load(Ordering::Relaxed);
        if wt != dg.sidle_last_total {
            let gap = dnow.saturating_sub(dg.sidle_last_change_us);
            if dg.sidle_last_total > 0 && gap >= SIDLE_GAP_MIN_US {
                dg.sidle_us += gap;
                dg.sidle_n += 1;
                dg.sidle_max_us = dg.sidle_max_us.max(gap);
            }
            // The same gap against the derived threshold (one extra
            // compare per emission event, only under RWM_SIDLE_DERIVED),
            // so both numbers come off the same run.
            if pol.sidle_derived {
                dg.sidle_evt_n += 1;
                if dg.sidle_last_total > 0 && gap >= dg.sidle_thr_us {
                    dg.sidle2_us += gap;
                    dg.sidle2_n += 1;
                    dg.sidle2_max_us = dg.sidle2_max_us.max(gap);
                }
            }
            dg.sidle_last_total = wt;
            dg.sidle_last_change_us = dnow;
        }
    }
    let ddt = dnow.saturating_sub(dg.diag_last_us);
    if ddt >= 250_000 {
        let ack_now = window_ack_seq.load(Ordering::Relaxed);
        let src_now = stats.fec.total_source_symbols.load(Ordering::Relaxed);
        let cod_now = stats.fec.total_repair_symbols.load(Ordering::Relaxed);
        let secs = ddt as f64 / 1_000_000.0;
        // Goodput = cumulative-ack advance (delivered source symbols).
        let dack = ack_now.saturating_sub(dg.diag_last_ack) as f64;
        let good_mbit = dack * (symbol_size as f64) * 8.0 / secs / 1e6;
        let src_rate = src_now.saturating_sub(dg.diag_last_src) as f64 / secs;
        let cod_rate = cod_now.saturating_sub(dg.diag_last_cod) as f64 / secs;
        let paused_frac = dg.diag_paused_iters as f64 / dg.diag_total_iters.max(1) as f64;
        // Re-derive the stall threshold from this window's
        // measured emission-event rate (window duration / events
        // observed). A window with no events keeps the previous
        // interval rather than inventing one. Once per 250 ms.
        if pol.sidle_derived {
            if dg.sidle_evt_n > 0 {
                dg.sidle_evt_us = ddt / dg.sidle_evt_n;
                dg.sidle_thr_us = stall_threshold_us(dg.sidle_evt_us);
            }
            dg.sidle_evt_n = 0;
        }
        let (cw, fl, np, np_act, min_rtt_us, pp) = {
            let mut sched = scheduler.lock();
            let mut cw = 0u64;
            let mut fl = 0u64;
            let mut np = 0u64;
            let mut rtt = 0u64;
            // Per-path in-flight vs its own BDP
            // cap + live RTT vs RTprop — the slow-path bufferbloat probe
            // (is the slow path over its BDP? is its RTT inflated above
            // RTprop?).  Cap gain = the BDP in-flight gain.
            let cap_gain = pol.infl_bdp_gain;
            let mut pp = String::new();
            // `np=` counts registered paths (`all_path_ids()`, sorted so the
            // blocks are stable to scrape). `active_paths()` is the
            // saturation-filtered set (`cwnd − in_flight > 0`): taking the
            // blocks over it would drop a cwnd-full path's block exactly when
            // the path is busiest. That count is printed as `np_act=`:
            // `np_act < np` is a tick with a saturated path.
            //
            // `expire_in_flight()` is called on the active set only. The
            // report task expires every path on its own 2 s cadence; expiring
            // a saturated path here would release stranded budget earlier
            // under `RWM_DIAG` — a behaviour change under a diagnosis gate —
            // so a saturated path's `in_flight` is only read.
            let act = sched.active_paths();
            let np_act = act.len() as u64;
            let mut ids = sched.all_path_ids();
            ids.sort_unstable();
            for id in &ids {
                if let Some(p) = sched.path_mut(*id) {
                    if act.contains(id) {
                        p.expire_in_flight();
                    }
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
                    // The same RTprop in µs (`rtp_us=`): the whole-ms `rtp`
                    // rounds a 0.4 ms loopback floor to 0 and a 12.4 ms one
                    // to 12. 0 = no sample, as for `rtp`.
                    let rtprop_us_i = p.min_rtt().map(|d| d.as_micros() as u64).unwrap_or(0);
                    // Per-path SOURCE outstanding gauge (charged by the
                    // CopaFeed at send, released on ack attribution),
                    // the ack-attributed per-path BtlBw_i (sym/s), and
                    // whether the per-path BDP anchor has ESTABLISHED.
                    let sinfl_i = p.src_inflight() as u64;
                    let btlbw_i = p.btlbw_sym_per_s().unwrap_or(0.0);
                    let est_i = if p.anchor_established() { "Y" } else { "n" };
                    // The rate-sample anchor trace
                    // (snapshotted-at-send / of-which-app-limited / acks-
                    // attributed / no-record / rej[interval/zero/applim] /
                    // generated / windowed-max-fill).  Cumulative counters.
                    let (rs_sent, rs_al, rs_attr, rs_nr, rs_iv, rs_zr, rs_al_rej, rs_gen, rs_fill) =
                        p.rs_diag();
                    // The wire clock next to the
                    // app-echo clock — wrtt = quinn packet-timed path RTT
                    // (what Copa's queue term reads under RWM_COPA_WIRE),
                    // rtt = app-layer echo (store-dwell inclusive), rtp =
                    // Copa's floor (wire-clocked when the gate is on; its
                    // distance from the known netem base per path is the
                    // floor-freshness check).
                    let wrtt_i = transport
                        .wire_rtt(*id)
                        .map(|d| d.as_secs_f64() * 1000.0)
                        .unwrap_or(0.0);
                    // Quinn's own
                    // congestion state for this path — qcwnd bytes
                    // (= 2 × quinn-internal BtlBŵ × RTprop under the
                    // BBR default, so qcwnd ≫ true BDP·MTU is the
                    // in-vivo max-filter over-read signature),
                    // congestion events, lost/sent packets.
                    let (qcwnd_i, qce_i, qlost_i, qsent_i) = transport
                        .quinn_path_stats(*id)
                        .unwrap_or((0, 0, 0, 0));
                    // The per-path outstanding account —
                    // retained store symbols charged to this path (its
                    // share of the pooled outstanding).
                    let sout_i = st.percap_out.get(id).copied().unwrap_or(0);
                    // cmp=<mode><switches>/<δ>
                    // — mode C (competitive) or D (default), the
                    // cumulative competitive entries, and the live δ
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
                    // Process-clock stall
                    // witness gauges (stalls detected / samples
                    // discarded, process-global) — zeros when
                    // RWM_CLOCK_GAP is off.
                    let (gap_g, gap_d) = crate::control::anchor::stall_witness()
                        .map(|w| w.stats())
                        .unwrap_or((0, 0));
                    // khr = the
                    // windowed-min echoSRTT/RTprop ratio K_i feeding
                    // the honest cap laws (1.00 when not engaged).
                    let khr_i = percap_k.get(id).map(|e| e.k()).unwrap_or(1.0);
                    // `RWM_HONEST_K`: kraw = the raw-sample windowed-min
                    // ratio the K consumers substitute under the gate ("-"
                    // when the gate is off). khr stays the smoothed read
                    // either way, so khr − kraw is the smoothing bias.
                    let kraw_s = p
                        .k_raw()
                        .map(|k| format!("{k:.2}"))
                        .unwrap_or_else(|| "-".to_string());
                    // The RTT second moment (paper §7.4): the standard-
                    // deviation estimate `√(EWMA[(rtt−srtt)²])`
                    // (`Path::rtt_sigma_us`) and the number of samples folded
                    // into it, per path, beside `rtt`/`wrtt`/`rtp`. The
                    // clock family needs σ as a measured input.
                    //
                    // Format: `sig_us=<µs>/n<count>`, `-` for σ before the
                    // first (rtt, srtt) pair, the `kraw` convention.
                    // The count is evidence, not a gate: the EWMA is seeded at
                    // 0 and runs at β = 1/4, so it retains 0.75^n of that seed
                    // and a σ read at n = 2 is biased low by about half. A
                    // parser that wants a trustworthy σ discards small n
                    // itself; the gauge does not decide for it, because a
                    // field that disappears below a threshold cannot be told
                    // apart from a path that was never sampled.
                    //
                    // Not `ANCHOR_MIN_SAMPLES`-gated: the 8-sample
                    // rule governs the delivered-rate anchor (`bw_samples`)
                    // and touches nothing here. σ's warm-up is the EWMA's own,
                    // it is reported, and it is a different clock.
                    let sig_s = p
                        .rtt_sigma_us()
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    let sig_n = p.rtt_sigma_samples();
                    // The three candidate dispersion gauges, beside the
                    // shipped one and read by nothing. Every clock in the
                    // family is `W = mean + k(α)·σ̂`, so a clock
                    // is only as good as its σ estimator. All three run on
                    // the same sample stream and print on the same line, so
                    // they compare paired, per path per interval.
                    //
                    // They decompose over three axes (memory, deviation
                    // power, reference) — see the block comment above
                    // `Path::cand_quantile`.
                    //
                    // Format — the `sig_us` convention exactly:
                    // `<µs|->/n<count>`, `-` before the first sample, the count
                    // describing that value's OWN sample set (window fill for
                    // the two window-class gauges, difference count for `msd`,
                    // lifetime EWMA count for `rvar`). No threshold gates any
                    // of them; warm-up exclusions are parser rules.
                    // No consumer, no gate, no default.
                    let cand = |v: Option<u64>| {
                        v.map(|x| x.to_string()).unwrap_or_else(|| "-".to_string())
                    };
                    let rvar_s = cand(p.rtt_mdev_us());
                    let rvar_n = p.rtt_mdev_samples();
                    let qsp_s = cand(p.rtt_qspread_us());
                    let qsp_n = p.rtt_qspread_samples();
                    let msd_s = cand(p.rtt_msd_us());
                    let msd_n = p.rtt_msd_samples();
                    // Candidate 4 (`tlag_us=`, paper §7.4): the same
                    // functional as `msd_us` at a fixed time lag `τ = RTprop`
                    // instead of a fixed sample lag, so it does not track the
                    // sample rate. Its count is the pair count `|P(τ)|`, and
                    // `-` iff that count is 0 (value and count come from one
                    // pair-set function). Read by nothing.
                    let tlag_s = cand(p.rtt_tlag_us());
                    let tlag_n = p.rtt_tlag_samples();
                    // The per-path
                    // loss estimate the recovery plane actually keys
                    // on (repair_debt, P_lost, NACK budgets) — the
                    // gauge that names the batch-serial poisoning
                    // (global batch_seq gaps read as per-path loss
                    // under striping).
                    let pl_i = p.estimator.loss_rate();
                    // RWM_POOL_ANCHOR: the per-path send-
                    // interval anchor rate (0 = no surviving bucket
                    // / feed off) + its gap/discard hygiene gauges
                    // — vs btlbw (the ack-interval read, which
                    // feeds cwnd only).
                    let sr_i = p.send_rate_anchor().unwrap_or(0.0);
                    let (sa_g, sa_d) = p.send_anchor_stats();
                    pp.push_str(&format!(
                        " p{}:infl={}/sinfl={}/bdp{:.0}(cap{}) sout={} khr={:.2}/kraw={} btlbw={:.0} sr={:.0}/g{}d{} est={} pl={:.4} cmp={} rtt={:.0}/wrtt={:.0}/rtp{:.0}ms rtp_us={} sig_us={}/n{} rvar_us={}/n{} qsp_us={}/n{} msd_us={}/n{} tlag_us={}/n{} gapd={}/{} qcwnd={} qce={} qlp={}/{} | ANCHOR sent={} al={} attr={} nr={} rej[iv={} zr={} al={}] gen={} fill={}",
                        id, infl_i, sinfl_i, bdp_i, cap_i, sout_i, khr_i, kraw_s, btlbw_i, sr_i, sa_g, sa_d, est_i, pl_i, cmp_s, rtt_i, wrtt_i, rtprop_i, rtprop_us_i, sig_s, sig_n, rvar_s, rvar_n, qsp_s, qsp_n, msd_s, msd_n, tlag_s, tlag_n, gap_g, gap_d,
                        qcwnd_i, qce_i, qlost_i, qsent_i,                                rs_sent, rs_al, rs_attr, rs_nr, rs_iv, rs_zr, rs_al_rej, rs_gen, rs_fill
                    ));
                }
            }
            (cw, fl, np, np_act, rtt, pp)
        };
        // BDP in symbols = goodput-rate(sym/s) × RTT — but report the
        // link-capacity BDP too from the measured min RTT and a nominal
        // 100 Mbit (diagnostic reference only).
        let bdp_100m = if min_rtt_us > 0 {
            (100e6 / 8.0 / symbol_size as f64) * (min_rtt_us as f64 / 1e6)
        } else {
            0.0
        };
        let eff = if generation { dg.diag_eff_rate } else { 0.0 };
        // GDIAG: stall attribution + generation lifecycle for this
        // window (percentages of attributed wall time; GLIFE means).
        let gd_tot: u64 = dg.gd_us.iter().sum::<u64>().max(1);
        let pct = |i: usize| dg.gd_us[i] as f64 * 100.0 / gd_tot as f64;
        let gln = dg.gl_sum.3.max(1);
        let gdiag = if generation {
            format!(
                " stall[emit={:.0}% budget={:.0}% fill={:.0}% target={:.0}% tok={:.0}% cwnd={:.0}%] glife[n={} fill={:.0}ms code={:.0}ms wait={:.0}ms]",
                pct(0), pct(1), pct(2), pct(3), pct(4), pct(5),
                dg.gl_sum.3,
                dg.gl_sum.0 as f64 / gln as f64 / 1000.0,
                dg.gl_sum.1 as f64 / gln as f64 / 1000.0,
                dg.gl_sum.2 as f64 / gln as f64 / 1000.0,
            )
        } else {
            String::new()
        };
        dg.gd_us = [0; 6];
        dg.gl_sum = (0, 0, 0, 0);
        // WAIT: the window sender's select!-arm wait-reason attribution.
        // Unconditional on the RWM_DIAG surface, and printed even when every
        // bucket is zero: a gauge that disappears when it has nothing to say
        // is a gauge you cannot prove ran.
        let waitdiag = wait_line(&dg.wait_us, dg.wait_busy_us, dg.wait_n);
        dg.wait_us = [0; 8];
        dg.wait_busy_us = 0;
        dg.wait_n = 0;
        // DGQ: the datagram send-queue audit. Cumulative, not
        // per-window — eviction is a whole-run accounting question and the
        // end-of-run reading is the one that matters, exactly like the
        // `cum=` totals above. Per LIVE path, so a dual cell shows both.
        //
        //   hand — handoffs quinn accepted
        //   tx   — DATAGRAM frames quinn transmitted (its own stats)
        //   full — handoffs that entered with a byte-full send queue: the
        //          eviction predicate (see `DatagramQueueAudit`)
        //   err  — handoffs quinn rejected
        //   sp   — send-buffer space, bytes, at the last handoff
        //
        // `hand − tx` is the eviction estimate that does not rest on the
        // predicate; `sp` corrects it for what is still queued. Absent
        // entirely without RWM_DIAG, and absent for a path that has sent
        // nothing — a gauge that reads 0 for two different reasons is not a
        // gauge.
        let dgq = {
            // Registered paths, like `np=` above: a byte-full send queue is
            // the saturated case, which `active_paths()` would filter out.
            // Read-only stats; sorted for stable scraping.
            let ids: Vec<u32> = {
                let mut v = scheduler.lock().all_path_ids();
                v.sort_unstable();
                v
            };
            let mut s = String::new();
            for id in ids {
                if let Some((hand, full, err, sp, tx)) = transport.datagram_queue_stats(id) {
                    s.push_str(&format!(
                        " dgq{id}[hand={hand} tx={tx} full={full} err={err} sp={sp}]"
                    ));
                }
            }
            s
        };
        // Cross-path-history attributions and
        // how many the flight witness credited to the previous
        // flight (spurious-retransmit class). Zeros without a feed.
        let (xat_c, xat_w) = copa_feed
            .as_ref()
            .map(|f| f.attr_diag())
            .unwrap_or((0, 0));
        // RWM_STORE_SACK_RELEASE: currently released (retained
        // but uncounted) / cumulative slots released — the store-
        // dwell mechanism gauge (win= already shows the uncounted
        // outstanding; retained = win + srel_cur). Empty when off.
        let srdiag = if pol.store_sack_release_on {
            format!(" srel={}/{}", sack_released.len(), sack_released_total)
        } else {
            String::new()
        };
        // RWM_POOL_ANCHOR: the honest dual-store law's
        // engagement + its Σ honest caps before clamping (the
        // mechanism gauge — win=/cap shows the clamped result).
        // Empty when not engaged (N = 1, warm-up, or gate off).
        let padiag = if pa_engaged {
            format!(" pa=on/{:.0}", pa_sum)
        } else {
            String::new()
        };
        // The
        // outstanding split + release-clumping. head = live head
        // span above the release frontier; hole = unSACKed below it.
        let wnd2diag = if reliable && !generation {
            let last_sent =
                st.sent_store.keys().next_back().copied().unwrap_or(0);
            let head = last_sent.saturating_sub(wnd2_frontier_last) as usize;
            let hole = store_len.saturating_sub(head);
            let relgap_cur =
                dnow.saturating_sub(wnd2_frontier_change_us) / 1000;
            let s = format!(
                " wnd2={}/{} relgap={}ms/mx{}ms",
                head.min(store_len),
                hole,
                relgap_cur,
                dg.wnd2_relgap_max_us / 1000,
            );
            dg.wnd2_relgap_max_us = 0;
            s
        } else {
            String::new()
        };
        // δ-honest shed: cumulative shed / budget-
        // refused, live 1−ρ fraction and deadline. Empty when off.
        let sheddiag = if pol.shed_on {
            format!(
                " shed={}/{} bud={:.4} D={}ms",
                st.shed_total,
                st.shed_denied,
                st.shed_budget_frac,
                st.shed_deadline_us_live / 1000,
            )
        } else {
            String::new()
        };
        // The recovery-plane trace.
        // rep/seqs = gap reports processed / gap seqs walked;
        // fired y/r = retransmits whose live flight was YOUNGER than
        // its path's law threshold (the spurious-by-law class) vs
        // ripe; supp c/a/l = suppressed by cooldown / age
        // gate / the mp law; stale = gap seqs already acked;
        // plost = P_lost-branch retransmits; age = mean flight age
        // at fire (ms); fp/on = per-path fired-flight / sent-on.
        let mpd_fired = dg.mpd_fired_young + dg.mpd_fired_ripe;
        let mut mp_pp = String::new();
        let mut mp_keys: Vec<u32> = dg.mpd_fired_flight
            .keys()
            .chain(dg.mpd_fired_on.keys())
            .copied()
            .collect();
        mp_keys.sort_unstable();
        mp_keys.dedup();
        for k in mp_keys {
            mp_pp.push_str(&format!(
                " p{}:{}/{}",
                k,
                dg.mpd_fired_flight.get(&k).copied().unwrap_or(0),
                dg.mpd_fired_on.get(&k).copied().unwrap_or(0)
            ));
        }
        // The patience-floor split.
        // `pf=<floor-bound>/<clock-bound>/<mean floor µs>` — how many
        // §6.1.2 threshold evaluations were pinned by the
        // kGranularity floor versus governed by the 9/8·srtt clock.
        let pf = format!(
            " pf={}/{}/{}",
            mpd_pf_floor.get(),
            mpd_pf_clock.get(),
            {
                let n = mpd_pf_floor.get() + mpd_pf_clock.get();
                if n > 0 { mpd_pf_sum.get() / n } else { 0 }
            }
        );
        let mpr = format!(
            " mpr[rep={} seqs={} fired={} y={} r={} fast={} coal={} supp={}/{}/{} stale={} plost={} taper={} age={:.0}ms{} fp/on{}]",
            dg.mpd_gap_reports,
            dg.mpd_gap_seqs,
            mpd_fired,
            dg.mpd_fired_young,
            dg.mpd_fired_ripe,
            dg.mpd_fired_fast,
            dg.mpd_coalesced,
            dg.mpd_supp_cool,
            dg.mpd_supp_age,
            dg.mpd_supp_law,
            dg.mpd_stale,
            st.mpd_plost_retx,
            // The ungated taper-copy count, so the waste split is
            // readable on an arm that never set RWM_DIAG's own counters.
            stats.fec.taper_copy.load(Ordering::Relaxed),
            if mpd_fired > 0 {
                dg.mpd_age_ms_sum / mpd_fired as f64
            } else {
                0.0
            },
            pf,
            mp_pp,
        );
        // The derived stall gauge printed beside the 3 ms one.
        // `sidle2=<cum ms>/<n>/mx<max ms> evt=<µs> sthr=<µs>`.
        // Empty (and nothing computed) unless RWM_SIDLE_DERIVED.
        let sd2 = if pol.sidle_derived {
            format!(
                " sidle2={}ms/{}/mx{}ms evt={}us sthr={}us",
                dg.sidle2_us / 1000,
                dg.sidle2_n,
                dg.sidle2_max_us / 1000,
                dg.sidle_evt_us,
                dg.sidle_thr_us,
            )
        } else {
            String::new()
        };
        eprintln!(
            "[DIAG] t={:.1}s win={}/{} paused={:.0}% good={:.1}Mbit ackrate_ewma={:.0}sym/s eff_pace={:.0}sym/s src={:.0}sym/s cod={:.0}sym/s cum={}/{}/{} sidle={}ms/{}/mx{}ms cwnd={} infl={} np={} np_act={} rtt={:.1}ms bdp100={:.0}sym sweeps={} retx={} gapdrop={} nbud={} xattr={}/{}{}{}{}{}{}{}{}{}{}{}",
            dnow.saturating_sub(dg.diag_start_us) as f64 / 1e6,
            store_len, effective_store_cap,
            paused_frac * 100.0,
            good_mbit,
            if generation { gen_rate_ewma } else { 0.0 },
            eff,
            src_rate, cod_rate,
            // Cumulative src/cod/ack totals (the
            // end-of-run accounting reads the LAST line) + the
            // emission-gap gauge (cum stall-gap ms / count / max).
            src_now, cod_now, ack_now,
            dg.sidle_us / 1000, dg.sidle_n, dg.sidle_max_us / 1000,
            cw, fl, np, np_act,
            min_rtt_us as f64 / 1000.0,
            bdp_100m,
            dg.diag_sweeps, dg.diag_retx, dg.diag_gaps_dropped, cached_nack_budget,
            xat_c, xat_w,
            mpr,
            sd2,
            wnd2diag,
            srdiag,
            padiag,
            sheddiag,
            waitdiag,
            dgq,
            gdiag,
            pp,
        );
        // ── `[ETA]` sender readout ────────────────────────────────
        // The prediction the placement law made, and what came back. Same
        // 250 ms cadence, same `RWM_DIAG` gate, same cumulative
        // last-line-wins convention as `[DIAG]` itself. The cold-price binds
        // accumulated by `place_costs` are drained here and nowhere else --
        // `place_costs` itself only bumps a `Cell`. A sender that never
        // stamped a placement stays silent, so an absent line reads as an
        // unreached feed and never as an unset gate.
        //
        // The pre-stated witness is `sigma_sender >= sigma_recv`
        // (`net/eta.rs`): the sender's error rides a round trip and the
        // receiver's lateness only the forward leg. Both lines print
        // `sig_us=` in the same units so the pair is readable off one run.
        {
            let mut sched = scheduler.lock();
            sched.drain_place_bind();
            if sched.eta().is_sender_site() {
                eprintln!("{}", sched.eta().line());
            }
        }
        // The sender-side conversion gauges
        // (cumulative; keys sorted for stable scraping). splace =
        // first source placements; retxo = targeted retransmits by
        // ORIGINAL placement path; stallo = frontier-stall ms/count
        // by blocking-hole owner path.
        {
            let mut keys: Vec<u32> = st.c8c_src_placed
                .keys()
                .chain(dg.c8c_retx_orig.keys())
                .chain(dg.c8c_stall_ms.keys())
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
                        st.c8c_src_placed.get(&k).copied().unwrap_or(0),
                        dg.c8c_retx_orig.get(&k).copied().unwrap_or(0),
                        dg.c8c_stall_ms.get(&k).copied().unwrap_or(0),
                        dg.c8c_stall_n.get(&k).copied().unwrap_or(0),
                    ));
                }
                eprintln!("[C8CONV-S]{}", s);
            }
        }
        dg.diag_last_us = dnow;
        dg.diag_last_ack = ack_now;
        dg.diag_last_src = src_now;
        dg.diag_last_cod = cod_now;
        dg.diag_paused_iters = 0;
        dg.diag_total_iters = 0;
    }
}

/// Render the `wait[..]` token of the `[DIAG]` line (see the `DiagState`
/// field docs for each token's meaning). Arm shares are over the AWAIT
/// time; `us` is await + body; `busy` is body over `us`.
pub(crate) fn wait_line(wait_us: &[u64; 8], busy_us: u64, n: u64) -> String {
    let await_tot: u64 = wait_us.iter().sum::<u64>();
    let wpct = |i: usize| wait_us[i] as f64 * 100.0 / await_tot.max(1) as f64;
    let us = await_tot + busy_us;
    format!(
        " wait[tun={:.0}% paused={:.0}% pace={:.0}% gen={:.0}% nack={:.0}% \
         defc={:.0}% tail={:.0}% flush={:.0}% n={} us={} busy={:.0}% busy_us={}]",
        wpct(0), wpct(1), wpct(2), wpct(3), wpct(4), wpct(5), wpct(6), wpct(7),
        n,
        us,
        busy_us as f64 * 100.0 / us.max(1) as f64,
        busy_us,
    )
}

#[cfg(test)]
mod wait_attribution_tests {
    /// The wait-reason histogram's silent failure mode, gated at `cargo test`.
    ///
    /// The histogram charges each loop iteration's elapsed wall time to the
    /// `select!` arm that woke it. A new `select!` arm without its
    /// `wait_arm = N` would be silently charged to whichever arm ran last —
    /// the histogram keeps summing to 100 % and lies. No runtime assertion
    /// can catch that, because the omission has no runtime symptom.
    ///
    /// So this scrapes the source, the same test-only reflection technique
    /// `gates::forwarding_audit` uses on the `RWM_*` surface, and asserts:
    ///
    ///   * every bucket index 0..8 is assigned exactly once, so no two arms
    ///     share a bucket and no bucket is dead;
    ///   * the number of `select!` arms in `run_window_sender`'s sender loop
    ///     equals the number of attributions plus the one arm that `return`s
    ///     (shutdown) — i.e. every arm that falls through is attributed;
    ///   * `wait_us` is sized to match.
    ///
    /// Order-insensitive by construction: it counts occurrences in a string
    /// and compares totals, so it does not depend on `RandomState`, on file
    /// order, or on which arm the runtime happens to poll first.
    fn sender_loop_source() -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/net/mod.rs");
        let src = std::fs::read_to_string(p).expect("read src/net/mod.rs");
        // The window sender's `select!`: from the `wait_arm` declaration to
        // the charge that closes it. Both are unique strings.
        let start = src
            .find("let mut wait_arm: usize = usize::MAX;")
            .expect("the wait-attribution declaration must exist");
        let end = src[start..]
            .find("dg.wait_us[wait_arm] += dt;")
            .expect("the wait-attribution charge must exist")
            + start;
        src[start..end].to_string()
    }

    #[test]
    fn every_wait_bucket_is_assigned_exactly_once() {
        let body = sender_loop_source();
        for i in 0..8usize {
            let needle = format!("wait_arm = {i};");
            let n = body.matches(&needle).count();
            assert_eq!(
                n, 1,
                "wait bucket {i} is assigned {n} times, expected exactly 1 — \
                 a duplicated bucket merges two wait reasons and a missing \
                 one leaves an arm's time charged to its predecessor"
            );
        }
        assert_eq!(
            body.matches("wait_arm = ").count(),
            8,
            "there must be exactly 8 attributions, one per bucket"
        );
    }

    #[test]
    fn every_select_arm_that_falls_through_is_attributed() {
        let body = sender_loop_source();
        // `select!` arms are `<pat> = <fut>[, if <cond>] => …`. Counting `=>`
        // at the arm level is fragile; counting the futures is not — every
        // arm in this loop awaits one of exactly these, and each occurrence
        // inside the scraped region is one arm.
        let arms = body.matches("tokio::time::sleep(").count()
            + body.matches("tun.read_packet()").count()
            + body.matches("nack_rx.recv()").count()
            + body.matches("deficit_rx.recv()").count()
            + body.matches("shutdown_rx.recv()").count()
            + body.matches("tail_deadline").count().min(1);
        let attributed = body.matches("wait_arm = ").count();
        assert_eq!(
            arms,
            attributed + 1,
            "every `select!` arm must set a wait bucket except the shutdown \
             arm, which returns instead of falling through (arms={arms}, \
             attributed={attributed})"
        );
    }

    #[test]
    fn the_histogram_is_wide_enough_for_every_bucket() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/net/diag.rs");
        let src = std::fs::read_to_string(p).expect("read src/net/diag.rs");
        assert!(
            src.contains("pub wait_us: [u64; 8],"),
            "wait_us must be sized 8 — the bucket count the sender assigns"
        );
        // And it must be printed unconditionally: an `if generation` around
        // `waitdiag` would hide it on every `RWM_GEN=0` run, as happened to
        // `stall[`.
        let w = src
            .find("let waitdiag = ")
            .expect("the waitdiag gauge must exist");
        let head = &src[w..w + 40];
        assert!(
            !head.contains("if "),
            "waitdiag must not be conditional — that is the defect this \
             instrument exists to fix: {head}"
        );
    }

    /// Stage 1d: a synthetic busy iteration (6 ms of loop body, then a
    /// 10 µs await woken by `tun`) lands in `busy`, not in `tun` — the
    /// accounting that used to charge the body to the next arm.
    #[test]
    fn a_busy_iteration_lands_in_busy_not_in_the_next_arm() {
        let mut dg = super::DiagState::new(1_000_000, 1_000_000, 1_000_000, 1_000_000);
        // Iteration 1: 100 µs body, 900 µs await on `paused` (arm 1).
        assert_eq!(dg.wait_enter_await(1_000_100), 100);
        let dt = dg.wait_resolve(1_001_000);
        assert_eq!(dt, 900);
        dg.wait_us[1] += dt;
        // Iteration 2: 6 ms body (the CPU-bound state), 10 µs await on `tun`.
        assert_eq!(dg.wait_enter_await(1_007_000), 6_000);
        let dt = dg.wait_resolve(1_007_010);
        assert_eq!(dt, 10, "the tun arm is charged its await only");
        dg.wait_us[0] += dt;
        dg.wait_n = 2;
        assert_eq!(dg.wait_busy_us, 6_100);
        assert_eq!(dg.wait_us[0], 10);
        assert_eq!(dg.wait_us[1], 900);
        // Body + await is the whole loop wall.
        assert_eq!(dg.wait_busy_us + dg.wait_us.iter().sum::<u64>(), 7_010);
        let line = super::wait_line(&dg.wait_us, dg.wait_busy_us, dg.wait_n);
        assert_eq!(
            line,
            " wait[tun=1% paused=99% pace=0% gen=0% nack=0% defc=0% tail=0% flush=0% \
             n=2 us=7010 busy=87% busy_us=6100]"
        );
    }

    /// The sender loop really routes through the split: the body is charged
    /// at the `select!` entry and the arm gets `wait_resolve`'s await
    /// (MEASUREMENT DISCIPLINE rule 1 — the mechanism executes).
    #[test]
    fn the_sender_loop_charges_body_before_the_select() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/net/mod.rs");
        let src = std::fs::read_to_string(p).expect("read src/net/mod.rs");
        let enter = src.find("dg.wait_enter_await(t)").expect("body charge exists");
        let select = src[enter..].find("tokio::select!").expect("select follows") + enter;
        let resolve = src[select..].find("dg.wait_resolve(resolved_us)").expect("resolve exists")
            + select;
        let charge = src[resolve..].find("dg.wait_us[wait_arm] += dt;").expect("charge") + resolve;
        assert!(enter < select && select < resolve && resolve < charge);
    }
}
