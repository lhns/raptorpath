//! The window sender's PERIODIC DIAG REPORT: the `[DIAG]` / `[C8CONV-S]`
//! lines and the counters that feed them.
//!
//! History (net seam pass 3, 2026-08-09): `run_window_sender` carried a
//! 439-line report phase at the bottom of its loop that read ~30 locals from
//! every OTHER phase — the emission step's `SenderState`, the resolve-once
//! `SenderPolicy`, the dynamic store-cap block's `tx_paused` /
//! `effective_store_cap` / per-path cap maps, the decoupled-admission law's
//! `wd_*` gauges, the recovery plane's suppression counters — and wrote
//! NOTHING that any other phase reads. That asymmetry is what licenses the
//! move: the report is read-only with respect to the data plane, so it can
//! become an ordinary function taking `&SenderState` / `&SenderPolicy` and
//! shared references to the engine handles.
//!
//! BEHAVIOUR CONTRACT: the body is VERBATIM. It was moved by a mechanical
//! transform that only dedents one level and inserts a `dg.` prefix in front
//! of a captured counter name (never inside a string literal, never inside a
//! comment, never after a `.`); stripping those 82 prefixes and re-indenting
//! reproduces the original 439 lines byte-for-byte. Nothing was reordered,
//! merged, split or re-guarded. In particular:
//!   * the `if pol.diag_on` guard stays at the CALL SITE, so the report's
//!     cost is still zero on the shipped path and the call still happens at
//!     the same point of the loop, after the `wnd2`/relgap tracker and
//!     before the paced generation-coding block;
//!   * the ONE scheduler acquisition is the same one, in the same scope —
//!     `scheduler.lock()` for the per-path `pp` string, released with the
//!     `let (cw, fl, np, min_rtt_us, pp) = { … };` block, still holding
//!     `expire_in_flight()` inside it;
//!   * the two atomics (`window_ack_seq`, the `stats.fec` symbol totals) are
//!     still read with `Ordering::Relaxed`, the same number of times, in the
//!     same order — including the per-iteration `sidle` handoff probe that
//!     runs BEFORE the 250 ms window test;
//!   * the 250 ms window test, the per-window resets (`gd_us`, `gl_sum`,
//!     `wnd2_relgap_max_us`, `sidle_evt_n`, the paused-iteration counters)
//!     and the `diag_last_*` roll-forward all happen where they did.
//!
//! State: [`DiagState`] is the 40 counters whose ONLY consumer is this
//! report. Most are accumulated by other phases of the sender loop (the
//! recovery plane's `mpd_*`, the c8-conversion `c8c_*`, the GDIAG stall
//! attribution) and read/reset here, so they genuinely must be mutable from
//! both sides — hence one struct threaded as `&mut` rather than 40
//! parameters. THREE counters that belong to this family did NOT move, and
//! the reasons are findings about the state split, not oversights:
//!   * `mpd_pf_floor` / `mpd_pf_clock` / `mpd_pf_sum` are `Cell`s captured by
//!     the `mp_thr_of` CLOSURE in the recovery phase. Moving them into
//!     `DiagState` would make that closure hold a shared borrow of `dg`
//!     across the recovery block, which also increments `dg.mpd_*` — a
//!     borrow conflict, not a behaviour question. They stay locals and are
//!     passed in by reference.
//!   * `wnd2_frontier_last` / `wnd2_frontier_change_us` are NOT DIAG-only:
//!     the LIVE `RWM_WIN_DECOUPLE` admission gate reads both to compute the
//!     head span and the stall meter. Only their derived `wnd2_relgap_max_us`
//!     is instrumentation. They stay locals and are passed in by value.
//!
//! NOT covered here: the receiver-side `[RCV]` / `[RDIAG]` / `[FDIAG]` /
//! `[C8CONV-R]` gauges (still in `run_impl`'s receiver task), the span-law
//! `[SPAN]` trace, `[GPIPE]`, `[PFRAC]` and the generation-lifecycle
//! bookkeeping that FEEDS `gl_sum` — all still inline in `run_window_sender`,
//! all still writing these fields through the struct.

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

/// diag/lossy-residual (goal-gate "Lossy-Single Residual", RWM_DIAG only):
/// sender EMISSION-GAP gauge — cumulative time in inter-emission gaps
/// ≥ 3 ms (src+cod handoffs to the transport observed per loop iteration;
/// the loop wakes ≥ every 1 ms, so gap edges are observed within ~1 ms).
/// Prices accounting term (b): engine-caused wire idle during recovery
/// rounds. `sidle=<cum ms>/<n>/<max ms>` in the [DIAG] line; the receiver's
/// [WIDLE] inter-arrival gauge is the wire-truth counterpart.
const SIDLE_GAP_MIN_US: u64 = 3_000;

/// Every counter whose ONLY consumer is the periodic DIAG report.
///
/// Behaviour-inert by construction: nothing here is read by an emission,
/// admission, pacing or recovery decision. Field docs are the originals,
/// moved here with their code.
pub(crate) struct DiagState {
    // ── GDIAG (feat/gen-substrate-ceiling JOB 1) ─────────────────────────
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
    pub gd_last_us: u64,
    /// [emit, budget, fill, target, tokens, cwnd]
    pub gd_us: [u64; 6],
    /// (fill_us, code_us, wait_us, n) accumulated over completed generations.
    pub gl_sum: (u64, u64, u64, u64),

    // ── The WINDOW sender's wait-reason histogram (`wait[..]`, RWM_DIAG) ──
    //
    // goal-gate "What Binds Throughput", instrument 2. `gd_us` above answers
    // "which gate binds wire emission" for the GENERATION plane only: its
    // accumulator is `if pol.diag_on && generation` and its six buckets
    // (budget / fill / target / tokens / cwnd) are generation-plane concepts
    // that do not exist under `RWM_GEN=0`. Every arm of the three-term
    // battery ran `RWM_GEN=0`, so `stall[` appeared in 0 of 1 116 logs and
    // `sidle` — 34.3 % of wall at c2r100-B, 72.7 % at c2r200-B — was one
    // undifferentiated bucket attributed to nothing.
    //
    // This is the window sender's own attribution, and it is a DIFFERENT
    // measurement, not the same one printed twice: it times the sender
    // loop's `select!` await and charges the elapsed wall time to the arm
    // that WOKE it. That is the direct answer to "when the sender is not
    // sending, what is it waiting on?", it is defined whether or not
    // generation coding is on, and its buckets sum to the whole loop.
    //
    //   tun     — `tun.read_packet()` produced a packet: PRODUCTIVE intake.
    //             The only arm that carries new source data.
    //   paused  — the 1 ms backpressure poll: the store is FULL (`tx_paused`).
    //             This is the outstanding-data limit binding, and it is the
    //             one the three-term law moves.
    //   pace    — the 1 ms pacing poll: the `RWM_CC_PACE` source token
    //             bucket is dry. Zero whenever `RWM_CC_PACE=0`.
    //   gen     — the 1 ms generation emission poll.
    //   nack    — a gap report arrived from the receiver.
    //   defc    — a generation-deficit report arrived.
    //   tail    — the tail-ARQ sweep deadline fired.
    //   flush   — the packer's partial-symbol flush timeout fired.
    //
    // `wait_n` counts loop iterations so the mean await is readable, and
    // `wait_tun_us` is split out so "productive" and "waiting" are separable
    // without re-deriving them from percentages. All gated on RWM_DIAG; the
    // shipped path is untouched.
    pub wait_last_us: u64,
    /// [tun, paused, pace, gen, nack, defc, tail, flush]
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

    // ── feat/c8-conversion DIAGNOSIS gauges ──────────────────────────────
    // (goal-gate "C8 Slow-Path Conversion", RWM_DIAG only — behavior-inert):
    // why don't slow-path symbols CONVERT to delivered goodput at the
    // heterogeneous dual cell?
    //  * c8c_src_placed[p]  — cumulative FIRST source placements per path
    //    (candidate (a), placement starvation: compare against the path's
    //    capacity share from btlbw/qdisc truth). Lives in `SenderState`.
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

    // ── feat/copa-wire-signal wedge forensics (RWM_DIAG only) ────────────
    // Cumulative tail ARQ sweeps fired, SACK-gap retransmits actually sent,
    // gaps discarded for exhausted budget, and the live effective rate — the
    // wedge shows good=0 with in_flight=0 for tens of seconds and these name
    // which stage of the reactive-repair chain is dead.
    pub diag_sweeps: u64,
    pub diag_retx: u64,
    pub diag_gaps_dropped: u64,
    pub diag_eff_rate: f64,

    // ── The emission-gap gauge (see `SIDLE_GAP_MIN_US`) ──────────────────
    pub sidle_last_total: u64,
    pub sidle_last_change_us: u64,
    pub sidle_us: u64,
    pub sidle_n: u64,
    pub sidle_max_us: u64,

    // ── Its DERIVED twin ─────────────────────────────────────────────────
    // Goal-gate "Unlock The Default 2: derived patience", part 3a
    // (`RWM_SIDLE_DERIVED`, DIAG-only, behaviour-inert). The gauge above is
    // UNCHANGED and keeps printing `sidle=`. These accumulate the SAME event
    // stream against `stall_threshold_us(evt_us)` — the legacy 3 ms
    // re-expressed as 3 × the MEASURED mean inter-emission-EVENT interval,
    // floored at the legacy value and capped at the hole-refresh cadence —
    // and print as `sidle2=` beside it, plus `evt=<µs>` (the measured
    // interval) and `sthr=<µs>` (the live threshold) so the verdict can be
    // read off the same line. `sidle2 ≤ sidle` by construction.
    //
    // `sidle_evt_us` is recomputed ONCE PER DIAG WINDOW from the events that
    // window observed — zero hot-loop cost. It starts at the loop wake, so
    // before the first window the derived threshold IS the legacy constant.
    pub sidle_evt_us: u64,
    pub sidle_thr_us: u64,
    pub sidle_evt_n: u64,
    pub sidle2_us: u64,
    pub sidle2_n: u64,
    pub sidle2_max_us: u64,

    /// feat/window-mtu: `relgap=<cur>/mx<max>ms` — time since the release
    /// frontier (max of SACK-release max and cum ack) last advanced, max per
    /// DIAG window: the release-clumping gauge (D2). The frontier itself
    /// (`wnd2_frontier_last` / `wnd2_frontier_change_us`) stays a local of
    /// `run_window_sender` because the LIVE decoupled-admission law reads it.
    pub wnd2_relgap_max_us: u64,
}

impl DiagState {
    /// Build the report's counters. Every initializer here is the one it had
    /// inline in `run_window_sender` and every one of them is PURE (zeroed
    /// counters, empty maps, and `stall_threshold_us(LOOP_WAKE_US)`) — the
    /// FOUR wall-clock stamps are passed IN so they keep being sampled at the
    /// exact points in setup they were sampled at before, in the same order.
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
    /// Per-path store caps / delay-aware redirect bounds / echo-ratio state,
    /// refreshed by the dyn-cap throttle.
    pub percap_caps: &'a HashMap<u32, usize>,
    pub percap_bounds: &'a HashMap<u32, usize>,
    pub percap_k: &'a HashMap<u32, EchoRatioMin>,
    /// RWM_STORE_SACK_RELEASE: currently released / cumulative slots.
    pub sack_released: &'a BTreeSet<u64>,
    pub sack_released_total: u64,
    /// RWM_POOL_ANCHOR: honest dual-store engagement + Σ honest caps.
    pub pa_engaged: bool,
    pub pa_sum: f64,
    /// RWM_WIN_DECOUPLE: the live release frontier (read by the ADMISSION
    /// law, hence still a local) and the law's engagement gauges.
    pub wnd2_frontier_last: u64,
    pub wnd2_frontier_change_us: u64,
    pub wd_engaged: bool,
    pub wd_allow_base: f64,
    pub wd_rate: f64,
    pub wd_cap_ret: usize,
    /// The live NACK repair budget and the generation-mode pacing EWMA.
    pub cached_nack_budget: u64,
    pub gen_rate_ewma: f64,
    /// RWM_PLACE_SLACK gauges.
    pub ps_slack_gauge: f64,
    pub ps_rate_ewma: f64,
    /// The patience-floor split counters. `Cell` because the evaluation
    /// happens inside a shared closure in the recovery phase — see the
    /// module header.
    pub mpd_pf_floor: &'a Cell<u64>,
    pub mpd_pf_clock: &'a Cell<u64>,
    pub mpd_pf_sum: &'a Cell<u64>,
}

/// The shared engine handles the report reads. All by shared reference: the
/// report takes ONE scheduler lock (scoped to the per-path `pp` string) and
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
/// Called ONCE PER SENDER-LOOP ITERATION under the caller's `if pol.diag_on`
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
    // The report body below is the ORIGINAL 439 lines, VERBATIM. These
    // bindings re-establish the names it read as locals of
    // `run_window_sender` so that nothing inside it had to be rewritten.
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
        percap_caps,
        percap_bounds,
        percap_k,
        sack_released,
        sack_released_total,
        pa_engaged,
        pa_sum,
        wnd2_frontier_last,
        wnd2_frontier_change_us,
        wd_engaged,
        wd_allow_base,
        wd_rate,
        wd_cap_ret,
        cached_nack_budget,
        gen_rate_ewma,
        ps_slack_gauge,
        ps_rate_ewma,
        mpd_pf_floor,
        mpd_pf_clock,
        mpd_pf_sum,
    } = inp;

    // ─────────────────────────────────────────────────────────────────────
    // BODY (verbatim; `dg.` prefixes only)
    // ─────────────────────────────────────────────────────────────────────
    dg.diag_total_iters += 1;
    if tx_paused {
        dg.diag_paused_iters += 1;
    }
    let dnow = now_us();
    // diag/lossy-residual emission-gap gauge (see decls): observe the
    // cumulative wire handoff count (src+cod, retx rides cod) once per
    // iteration; a change closes the current gap — accumulate it when
    // it is a stall-class gap (≥ 3 ms), not a pacing interval.
    {
        let wt = stats.fec.total_source_symbols.load(Ordering::Relaxed)
            + stats.fec.total_repair_symbols.load(Ordering::Relaxed);
        if wt != dg.sidle_last_total {
            let gap = dnow.saturating_sub(dg.sidle_last_change_us);
            if dg.sidle_last_total > 0 && gap >= SIDLE_GAP_MIN_US {
                dg.sidle_us += gap;
                dg.sidle_n += 1;
                dg.sidle_max_us = dg.sidle_max_us.max(gap);
            }
            // 3a: the SAME gap against the DERIVED threshold. One
            // extra compare per emission event, only under
            // RWM_SIDLE_DERIVED; the legacy accumulation above is
            // untouched, so both numbers come off the same run.
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
        // 3a: re-derive the stall threshold from THIS window's
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
            let cap_gain = pol.infl_bdp_gain;
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
                    // goal-gate "Ship The Wins 2: shal8 anchor" DIAG
                    // (P-D1 gauge, behavior-inert): quinn's OWN
                    // congestion state for this path — qcwnd bytes
                    // (= 2 × quinn-internal BtlBŵ × RTprop under the
                    // BBR default, so qcwnd ≫ true BDP·MTU is the
                    // in-vivo max-filter over-read signature),
                    // congestion events, lost/sent packets.
                    let (qcwnd_i, qce_i, qlost_i, qsent_i) = transport
                        .quinn_path_stats(*id)
                        .unwrap_or((0, 0, 0, 0));
                    // task #86 DIAG: the per-path outstanding ACCOUNT
                    // (store symbols charged to this path / its cap_i)
                    // — the mechanism gauge for RWM_STORE_PERCAP
                    // (zeros when the percap law is not engaged).
                    let sout_i = st.percap_out.get(id).copied().unwrap_or(0);
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
                    // goal-gate "Honest Inputs" DIAG (`RWM_HONEST_K`):
                    // kraw = the RAW-sample windowed-min ratio the K
                    // consumers substitute under the gate ("-" when the
                    // gate is off). khr stays the legacy smoothed read
                    // either way, so khr − kraw IS the smoothing bias,
                    // measured in-cell — the jit25 decomposition gauge.
                    let kraw_s = p
                        .k_raw()
                        .map(|k| format!("{k:.2}"))
                        .unwrap_or_else(|| "-".to_string());
                    // feat/store-borrowing DIAG: this path's loan
                    // gauges — symbols LENT out (charged here,
                    // flying elsewhere) / BORROWED in (flying
                    // here, charged elsewhere). Zeros when off.
                    let lent_i = st.percap_lent.get(id).copied().unwrap_or(0);
                    let bor_i = st.percap_borrowed.get(id).copied().unwrap_or(0);
                    // feat/recovery-suppression DIAG: the per-path
                    // LOSS ESTIMATE the recovery plane actually keys
                    // on (repair_debt, P_lost, NACK budgets) — the
                    // gauge that names the batch-serial poisoning
                    // (global batch_seq gaps read as per-path loss
                    // under striping).
                    let pl_i = p.estimator.loss_rate();
                    // RWM_POOL_ANCHOR DIAG: the per-path send-
                    // interval anchor rate (0 = no surviving bucket
                    // / feed off) + its gap/discard hygiene gauges
                    // — vs btlbw (the legacy ack-interval read,
                    // deliberately left feeding cwnd only).
                    let sr_i = p.send_rate_anchor().unwrap_or(0.0);
                    let (sa_g, sa_d) = p.send_anchor_stats();
                    // RWM_POOL_DELIV DIAG (arm A): the DELIVERY-clocked
                    // term alone (0 = no accepted sample / gate off) and
                    // its guard counters — the mechanism witness that
                    // separates arm A from attempt 1 in the logs.
                    // dr vs sr IS the pre-registered prediction 1.
                    let dr_i = p.deliv_rate_anchor().unwrap_or(0.0);
                    let (da_ok, da_sh, da_g, da_d) = p.deliv_anchor_stats();
                    pp.push_str(&format!(
                        " p{}:infl={}/sinfl={}/bdp{:.0}(cap{}) sout={}/{}/b{} ln={}/{} khr={:.2}/kraw={} btlbw={:.0} sr={:.0}/g{}d{} dr={:.0}/a{}s{}g{}d{} est={} pl={:.4} cmp={} rtt={:.0}/wrtt={:.0}/rtp{:.0}ms gapd={}/{} qcwnd={} qce={} qlp={}/{} | ANCHOR sent={} al={} attr={} nr={} rej[iv={} zr={} al={}] gen={} fill={}",
                        id, infl_i, sinfl_i, bdp_i, cap_i, sout_i, scap_i, sbnd_i, lent_i, bor_i, khr_i, kraw_s, btlbw_i, sr_i, sa_g, sa_d, dr_i, da_ok, da_sh, da_g, da_d, est_i, pl_i, cmp_s, rtt_i, wrtt_i, rtprop_i, gap_g, gap_d,
                        qcwnd_i, qce_i, qlost_i, qsent_i,                                rs_sent, rs_al, rs_attr, rs_nr, rs_iv, rs_zr, rs_al_rej, rs_gen, rs_fill
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
        // UNCONDITIONAL on the RWM_DIAG surface — every battery arm sets it,
        // and this is the gauge whose ABSENCE (`stall[` in 0 of 1 116 logs)
        // left `sidle` unattributed for the whole three-term battery. It is
        // printed even when every bucket is zero: a gauge that disappears
        // when it has nothing to say is a gauge you cannot prove ran.
        let w_tot: u64 = dg.wait_us.iter().sum::<u64>().max(1);
        let wpct = |i: usize| dg.wait_us[i] as f64 * 100.0 / w_tot as f64;
        let waitdiag = format!(
            " wait[tun={:.0}% paused={:.0}% pace={:.0}% gen={:.0}% nack={:.0}% \
             defc={:.0}% tail={:.0}% flush={:.0}% n={} us={}]",
            wpct(0), wpct(1), wpct(2), wpct(3), wpct(4), wpct(5), wpct(6), wpct(7),
            dg.wait_n,
            dg.wait_us.iter().sum::<u64>(),
        );
        dg.wait_us = [0; 8];
        dg.wait_n = 0;
        // DGQ: the datagram send-queue audit (instrument 3). CUMULATIVE, not
        // per-window — eviction is a whole-run accounting question and the
        // end-of-run reading is the one that matters, exactly like the
        // `cum=` totals above. Per LIVE path, so a dual cell shows both.
        //
        //   hand — handoffs quinn ACCEPTED
        //   tx   — DATAGRAM frames quinn TRANSMITTED (its own stats)
        //   full — handoffs that entered with a byte-full send queue: the
        //          eviction predicate (see `DatagramQueueAudit`)
        //   err  — handoffs quinn REJECTED
        //   sp   — send-buffer space, bytes, at the last handoff
        //
        // `hand − tx` is the eviction estimate that does not rest on the
        // predicate; `sp` corrects it for what is still queued. Absent
        // entirely without RWM_DIAG, and absent for a path that has sent
        // nothing — a gauge that reads 0 for two different reasons is not a
        // gauge.
        let dgq = {
            let ids: Vec<u32> = { scheduler.lock().active_paths().to_vec() };
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
        let srdiag = if pol.store_sack_release_on {
            format!(" srel={}/{}", sack_released.len(), sack_released_total)
        } else {
            String::new()
        };
        // RWM_POOL_ANCHOR DIAG: the honest dual-store law's
        // engagement + its Σ honest caps before clamping (the
        // mechanism gauge — win=/cap shows the clamped result).
        // Empty when not engaged (N = 1, warm-up, or gate off).
        let padiag = if pa_engaged {
            format!(" pa=on/{:.0}", pa_sum)
        } else {
            String::new()
        };
        // feat/window-mtu part-1 diagnosis gauge (see decls): the
        // outstanding split + release-clumping. head = live head
        // span above the release frontier; hole = unSACKed below it.
        let wnd2diag = if reliable && !generation {
            let last_sent =
                st.sent_store.keys().next_back().copied().unwrap_or(0);
            let head = last_sent.saturating_sub(wnd2_frontier_last) as usize;
            let hole = store_len.saturating_sub(head);
            let relgap_cur =
                dnow.saturating_sub(wnd2_frontier_change_us) / 1000;
            let mut s = format!(
                " wnd2={}/{} relgap={}ms/mx{}ms",
                head.min(store_len),
                hole,
                relgap_cur,
                dg.wnd2_relgap_max_us / 1000,
            );
            // RWM_WIN_DECOUPLE engagement gauge: base allowance /
            // honest rate / retention backstop (mechanism liveness).
            if wd_engaged {
                s.push_str(&format!(
                    " wd=al{:.0}/r{:.0}/ret{}",
                    wd_allow_base, wd_rate, wd_cap_ret
                ));
            }
            dg.wnd2_relgap_max_us = 0;
            s
        } else {
            String::new()
        };
        // δ-honest shed DIAG (fix C): cumulative shed / budget-
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
        // feat/recovery-suppression DIAG: the recovery-plane trace.
        // rep/seqs = gap reports processed / gap seqs walked;
        // fired y/r = retransmits whose live flight was YOUNGER than
        // its path's law threshold (the spurious-by-law class) vs
        // ripe; supp c/a/l = suppressed by cooldown / legacy age
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
        // Goal-gate "Unlock The Default 2": the patience-floor split.
        // `pf=<floor-bound>/<clock-bound>/<mean floor µs>` — how many
        // §6.1.2 threshold evaluations were pinned by the
        // kGranularity FLOOR versus governed by the 9/8·srtt CLOCK.
        // "Patience is derived" means the floor term stops winning.
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
            " mpr[rep={} seqs={} fired={} y={} r={} fast={} coal={} supp={}/{}/{} stale={} plost={} age={:.0}ms{} fp/on{}]",
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
            if mpd_fired > 0 {
                dg.mpd_age_ms_sum / mpd_fired as f64
            } else {
                0.0
            },
            pf,
            mp_pp,
        );
        // Goal-gate "Unlock The Default 2", part 3a: the DERIVED
        // stall gauge printed beside the untouched legacy one.
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
            "[DIAG] t={:.1}s win={}/{} paused={:.0}% good={:.1}Mbit ackrate_ewma={:.0}sym/s eff_pace={:.0}sym/s src={:.0}sym/s cod={:.0}sym/s cum={}/{}/{} sidle={}ms/{}/mx{}ms cwnd={} infl={} np={} rtt={:.1}ms bdp100={:.0}sym sweeps={} retx={} gapdrop={} nbud={} xattr={}/{} loan={}/{}{}{}{}{}{}{}{}{}{}{}",
            dnow.saturating_sub(dg.diag_start_us) as f64 / 1e6,
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
            dg.sidle_us / 1000, dg.sidle_n, dg.sidle_max_us / 1000,
            cw, fl, np,
            min_rtt_us as f64 / 1000.0,
            bdp_100m,
            dg.diag_sweeps, dg.diag_retx, dg.diag_gaps_dropped, cached_nack_budget,
            xat_c, xat_w,
            st.percap_loans.len(), st.percap_loans_total,
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
        // feat/c8-conversion DIAG: the sender-side conversion gauges
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
                // RWM_PLACE_SLACK gauge: the live S (ms) + ack-rate
                // EWMA (sym/s) — engagement magnitude for the law.
                if pol.place_slack_on {
                    s.push_str(&format!(
                        " slk={:.0}ms/r{:.0}",
                        ps_slack_gauge * 1000.0,
                        ps_rate_ewma
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

#[cfg(test)]
mod wait_attribution_tests {
    /// **The instrument's real failure mode, gated at `cargo test`.**
    ///
    /// The window sender's wait-reason histogram (goal-gate "What Binds
    /// Throughput", instrument 2) charges each loop iteration's elapsed wall
    /// time to the `select!` arm that woke it. If someone later adds a
    /// `select!` arm and forgets its `wait_arm = N`, that arm's time is
    /// SILENTLY charged to whichever arm ran last — the histogram keeps
    /// summing to 100 %, keeps looking healthy, and lies. No runtime
    /// assertion can catch that, because the omission has no runtime symptom.
    ///
    /// So this scrapes the source, the same test-only reflection technique
    /// `gates::forwarding_audit` uses on the `RWM_*` surface, and asserts:
    ///
    ///   * every bucket index 0..8 is assigned EXACTLY once, so no two arms
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
        // And it must be printed UNCONDITIONALLY: the whole point is that
        // `stall[` was gated on `generation` and so appeared in 0 of the
        // battery's 1 116 logs. A `if generation` around `waitdiag` would
        // reintroduce exactly that defect.
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
}
