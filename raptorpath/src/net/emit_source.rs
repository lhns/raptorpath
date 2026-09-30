//! The window sender's source-symbol emission step: one framed packet in,
//! encoder intake + wire placement + accounting + proactive repair out.
//!
//! The mutable sender locals live in [`SenderState`], the resolve-once
//! configuration in [`SenderPolicy`](super::sender_policy::SenderPolicy),
//! and the shared engine handles in [`SenderCtx`]; `run_window_sender` calls
//! [`emit_source`] from each of its source-emission sites.
//!
//! Ordering constraints:
//!   * the scheduler is locked in short, separate acquisitions, in this
//!     order: the source placement pick, the ETA stamp, the
//!     `charge_in_flight` + Copa `on_sent`/`charge_src`/`on_src_sent` block,
//!     the redundant-source pick and its charge, the worst-loss ε read, the
//!     `deficit.on_send` write, the taper `spare_capacity`/estimator read, the
//!     per-correction worst-loss read, the correction placement pick, and the
//!     correction `charge_in_flight`;
//!   * in the taper block the fec-controller lock is taken before the
//!     scheduler lock, and both are released at the end of that block;
//!   * the `RWM_EMIT_BATCH` taper cache short-circuits the derived
//!     recomputation but still feeds the A* send-rate anchor per symbol;
//!   * the δ-honest shed decision runs inside the `P_lost` retransmit branch,
//!     after the coin flip, and the ρ-budget-refused arm increments
//!     `shed_denied` and serializes.
//!
//! The paced generation coded-emission block, the deficit recovery loop, the
//! NACK/gap repair dispatch, the tail ARQ sweep, the ack/SACK drains and the
//! dynamic store-cap refresh live elsewhere in the sender and read the same
//! `SenderState` fields, which is why they are `pub(crate)`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tracing::warn;

use super::sender_policy::SenderPolicy;
use super::{
    BatchCounter,
    CopaFeed, create_window_encoder, now_us, percap_charge, select_repair_path,
    select_source_path, shed_allowed, shed_deadline_us, window_source_paths,
};
use crate::control::{FecRateController, RepairRateCache, SendRateAnchor, TaperBudget};
use crate::fec::{FecBackend, WindowEncoder, WireSymbol};
use crate::monitor::stats::SharedStats;
use crate::scheduler::Scheduler;
use crate::transport::{QuicTransport, SymbolBatch};
use crate::control::fec_rate::ProtocolHint;

/// Staleness bound for the cached taper/span math (µs): one burst at
/// the service wall is ~3 ms; 50 ms only binds on low-rate paths.
const TAPER_CACHE_MAX_AGE_US: u64 = 50_000;

/// The shared engine handles the emission step needs, built once per
/// `run_window_sender` invocation. Taken by shared reference: all mutation
/// goes through the `Mutex`/atomic handles.
pub(crate) struct SenderCtx<'a> {
    pub scheduler: &'a Arc<parking_lot::Mutex<Scheduler>>,
    pub fec_controller: &'a Arc<parking_lot::Mutex<FecRateController>>,
    pub transport: &'a Arc<QuicTransport>,
    pub stats: &'a Arc<SharedStats>,
    pub batch_counter: &'a BatchCounter,
    pub window_ack_seq: &'a Arc<AtomicU64>,
    /// `Some(..)` in plain in-order mode when the Copa delivery feed is on.
    /// `None` = shipped path.
    pub copa_feed: Option<&'a Arc<CopaFeed>>,
}

/// The window sender's mutable state — the encoder and every local the
/// emission step writes.
///
/// The rest of `run_window_sender` (the ack/SACK drains, the NACK repair
/// dispatch, the DIAG print, the tail sweep) reads and writes these fields
/// too; the struct lets the emission step be an ordinary function.
pub(crate) struct SenderState {
    /// Codec pinned at startup — created once, never rebuilt.
    pub encoder: Box<dyn WindowEncoder>,

    /// Sent-data store (reliable mode only): seq → the exact source
    /// WireSymbol as sent. This is the retention contract — bytes retained
    /// until the peer's cumulative ack passes them (removal by ack only), so
    /// an aged SACK-confirmed hole that slid out of the coding window is
    /// recovered by a targeted retransmit of exactly this symbol. Bounded by
    /// RELIABLE_STORE_MAX via TUN-read backpressure, never by eviction.
    pub sent_store: BTreeMap<u64, WireSymbol>,
    /// Retransmit buffer: maps seq → (send_time_us, epsilon_at_send, path_id).
    /// Used for P_lost-based retransmit decisions. Symbols are removed on ACK.
    /// Metadata only — under EVICT the source bytes die with window eviction.
    pub retransmit_buffer: BTreeMap<u64, (u64, f64, u32)>,
    /// Maps source seq → path it was sent on (for cross-path retransmission).
    /// BTreeMap (not HashMap) so the per-path ack attribution can range-query
    /// the seqs in a SACK / cumulative-ack span.
    pub source_path_map: BTreeMap<u64, u32>,
    /// seq → last NACK-retransmit time (µs). Repeated gap acks for the same
    /// hole (they arrive every GAP_ACK_MIN_INTERVAL while it persists) must
    /// not resend the symbol more than once per SRTT — but may resend after
    /// an SRTT, which escalates naturally if the retransmit itself dies.
    /// Value = (last retransmit time µs, path the retransmit flew on). The
    /// path is the `RWM_RECOV_MP` live-flight input (the retransmit inherits
    /// the in-flight clock of its own path); with the gate off only the time
    /// is read.
    pub nack_retx_at: std::collections::HashMap<u64, (u64, u32)>,

    /// Last source path used (for NACK repair path selection outside the
    /// emission step).
    pub last_source_path: u32,
    /// Wall-clock (µs) of the last new source-symbol send (idle-triggered
    /// recovery). Initialized to "now" so a transfer that stalls before
    /// sending anything is treated as active until it idles.
    pub last_source_send_us: u64,
    /// Last source-intake time (generation-mode pacing input).
    pub gen_last_source_us: u64,
    /// Source symbols sent in the current reporting period.
    pub source_symbols_this_period: u64,
    /// Source pacing token bucket (symbols). Refilled at the link rate each
    /// loop iteration; one token is consumed per source symbol on the wire.
    pub src_tokens: f64,

    // ── Per-path store attribution (the `[DIAG] sout=` gauge) ────────────
    /// seq → account path, in lockstep with `sent_store` (charge on insert,
    /// release on ack-removal only — the retention contract).
    pub percap_acct: BTreeMap<u64, u32>,
    /// path → outstanding gauge (Σ over `percap_acct`; DIAG `sout=`).
    pub percap_out: std::collections::HashMap<u32, usize>,

    // ── Proactive-repair emission state ──────────────────────────────────
    /// Fractional repair accumulator: tracks sub-symbol repair debt.
    /// Driven by TaperFunction density when GE data is available,
    /// falls back to flat rate from compute_repair_rate_capped.
    pub repair_debt: f64,
    /// Source symbol counter for taper time offset (symbols since window
    /// start).
    pub taper_offset: u64,
    /// Budget-conserving taper (`RWM_TAPER_R`): emission consumes r as
    /// computed (a per-window budget) instead of r per ack cycle.
    pub taper_budget: TaperBudget,
    /// Windowed-max send-rate anchor (`RWM_ASTAR_ANCHOR`), fed by the
    /// sender's own send events, with gap-spanning buckets discarded.
    pub astar_anchor: SendRateAnchor,
    /// RWM_EMIT_BATCH per-burst cache: (repair_rate, span_params, estimator
    /// RTT at refresh).
    pub taper_cache: Option<(f64, Option<(u64, u64)>, Duration)>,
    pub taper_cache_syms: usize,
    pub taper_cache_at_us: u64,
    /// Stage 1c: the cadenced repair rate every window-path reader shares
    /// (see [`RepairRateCache`] and [`cadenced_repair_rate`]).
    pub rate_cache: RepairRateCache,

    // ── δ-honest overload shedding ──────────────────────────────────────
    /// Seqs shed by the δ law (never served again; pruned at the cumulative
    /// frontier — the split_off twin).
    pub shed_seqs: BTreeSet<u64>,
    pub shed_total: u64,
    /// Past-deadline candidates the ρ budget refused (the serialize arm of
    /// the law — visible in DIAG so the budget's bite is measurable).
    pub shed_denied: u64,
    /// The live derived 1−ρ budget fraction and δ deadline (µs), refreshed
    /// per source symbol alongside the span parameters.
    pub shed_budget_frac: f64,
    pub shed_deadline_us_live: u64,

    // ── Instruments (RWM_DIAG only; behaviour-inert) ─────────────────────
    /// GLIFE per-generation lifecycle: anchor → (first_src, sealed,
    /// last_emit) µs.
    pub gl: std::collections::HashMap<u64, (u64, u64, u64)>,
    /// Cumulative first source placements per path.
    pub c8c_src_placed: std::collections::HashMap<u32, u64>,
    /// Last ~500 ms span-law trace stamp.
    pub span_diag_last_us: u64,
    /// Recovery-suppression trace: the P_lost-branch retransmit channel.
    pub mpd_plost_retx: u64,
}

impl SenderState {
    /// Build the sender's mutable state. Every initializer is pure (empty
    /// collections, zeroed counters, and the startup-pinned encoder); the two
    /// wall-clock stamps are passed in so the caller controls when in setup
    /// they are sampled.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        fec_backend: FecBackend,
        symbol_size: u16,
        gen_size: usize,
        pipeline: usize,
        systematic: bool,
        generation: bool,
        gen_repair_floor: f64,
        gen_last_source_us: u64,
        last_source_send_us: u64,
    ) -> Self {
        // Codec pinned at startup — created once, never rebuilt.
        let encoder: Box<dyn WindowEncoder> = if systematic {
            Box::new(crate::fec::GenerationEncoder::new_systematic(
                symbol_size,
                gen_size,
                pipeline,
                gen_repair_floor,
            ))
        } else if generation {
            Box::new(crate::fec::GenerationEncoder::new(
                symbol_size,
                gen_size,
                pipeline,
                gen_repair_floor,
            ))
        } else {
            create_window_encoder(fec_backend, symbol_size)
        };
        Self {
            encoder,
            sent_store: BTreeMap::new(),
            retransmit_buffer: BTreeMap::new(),
            source_path_map: BTreeMap::new(),
            nack_retx_at: std::collections::HashMap::new(),
            last_source_path: 0,
            last_source_send_us,
            gen_last_source_us,
            source_symbols_this_period: 0,
            src_tokens: 0.0,
            percap_acct: BTreeMap::new(),
            percap_out: std::collections::HashMap::new(),
            repair_debt: 0.0,
            taper_offset: 0,
            taper_budget: TaperBudget::new(),
            astar_anchor: SendRateAnchor::new(),
            taper_cache: None,
            taper_cache_syms: 0,
            taper_cache_at_us: 0,
            rate_cache: RepairRateCache::default(),
            shed_seqs: BTreeSet::new(),
            shed_total: 0,
            shed_denied: 0,
            shed_budget_frac: 0.0,
            shed_deadline_us_live: 0,
            gl: std::collections::HashMap::new(),
            c8c_src_placed: std::collections::HashMap::new(),
            span_diag_last_us: 0,
            mpd_plost_retx: 0,
        }
    }
}

/// Feed one framed packet to the encoder, place it on the wire, account for
/// it, and emit the proactive repair its taper budget owes.
///
/// `emit_batch_live` is a per-iteration input (re-scoped in the main loop,
/// read-only here), so it is passed in rather than living in [`SenderState`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_source(
    framed: &[u8],
    st: &mut SenderState,
    pol: &SenderPolicy,
    ctx: &SenderCtx<'_>,
    emit_batch_live: bool,
) {
    let wire_sym = st.encoder.add_source(framed);
    // GDIAG/GLIFE fill tracking: stamp the generation's first-source
    // and sealed instants (RWM_DIAG only; no-op on the shipped path).
    if pol.diag_on && pol.generation {
        let seq = wire_sym.block_id;
        let anchor = seq - (seq % pol.gen_size as u64);
        let e = st.gl.entry(anchor).or_insert((0, 0, 0));
        if e.0 == 0 {
            e.0 = now_us();
        }
        if seq % pol.gen_size as u64 == pol.gen_size as u64 - 1 {
            e.1 = now_us();
        }
    }
    st.gen_last_source_us = now_us();

    // Retention: the store keeps the sent bytes until the peer acks them —
    // the coding window may slide past this symbol, but the data can no
    // longer be destroyed by eviction. Generation coding turns per-seq ARQ
    // off, so it needs no sent store (recovery is more coded symbols for the
    // generation, never an exact-seq resend); backpressure uses the
    // encoder's retained size instead.
    if pol.reliable && !pol.generation {
        st.sent_store.insert(wire_sym.block_id, wire_sym.clone());
    }

    // Send the source symbol. In reliable multipath mode, stripe by the
    // per-symbol placement law (softmax over marginal cost, paper §5.7);
    // a single path collapses to that path. Non-reliable (realtime/EVICT)
    // mode keeps the single best-path pick + redundant duplicate.
    let source_path = {
        if pol.reliable {
            let sched = ctx.scheduler.lock();
            sched.place_symbol(false, &[]).unwrap_or(0)
        } else {
            let sched = ctx.scheduler.lock();
            select_source_path(&sched)
        }
    };
    st.last_source_path = source_path;
    // ── `[ETA]`: stamp the sender's own prediction ───────────────────────
    // `E_picked` is the `expected_delivery_load()` of the path the placement
    // law just chose. It is the same quantity the law's `load` term is built
    // from (there de-dimensionalised by `ref_srtt` and offset by the
    // deadline; here in raw seconds), so the receiver is told the sender's
    // own model rather than a second one.
    //
    // Read before `charge_in_flight` below, which is why it is its own short
    // acquisition: charging first would price this symbol's own backlog into
    // its own prediction.
    //
    // The same call updates `F-hat` (the running max of stamped arrival
    // times). Neither feeds a decision: the wire field feeds two gauges.
    let src_send_ts_us = now_us();
    let eta_rel_us = {
        let mut sched = ctx.scheduler.lock();
        let e = sched
            .path(source_path)
            .map(|p| p.expected_delivery_load())
            .unwrap_or(0.0);
        // Non-finite (a path with no cwnd yet) reads as no prediction — the
        // 0 sentinel — rather than as a fabricated number.
        let us = if e.is_finite() && e > 0.0 { (e * 1e6) as u64 } else { 0 };
        sched.eta_mut().stamp(source_path, src_send_ts_us, us);
        us
    };
    // Idle-triggered recovery: stamp the last new-source send so the NACK
    // throttle can tell "actively pushing data" (repairs would load a
    // congested path) from "idle except for a hole" (targeted recovery is
    // free).
    st.last_source_send_us = now_us();
    // Fungible frontier (paper §5.2): in coded-only mode the wire carries a
    // fresh random linear combination over the current window (which now
    // includes this source) instead of the raw systematic symbol. Any K
    // independent such combinations, from any path, reconstruct the K window
    // sources — so a coded symbol lost on the slow path is one
    // interchangeable degree of freedom, not a fixed in-order position. The
    // systematic bytes remain in the encoder window + retention store for the
    // targeted-ARQ backstop on aged holes.
    //
    // Generation coding decouples coded emission from source intake:
    // add_source only fills the generation here, and the paced token-bucket
    // block in the main loop does all wire sends (so coded symbols keep
    // flowing to complete buffered generations even while TUN reads are
    // paused by backpressure). Systematic-repair: the raw source rides the
    // wire as primary here (striped by the place_symbol pick above, delivered
    // out of order with zero decode); coded repair is emitted in the paced
    // generation block (ceil(len·r) per generation + deficit top-up).
    // Coded-only generation mode skips the per-source send. Both generation
    // submodes keep per-seq ARQ / sent_store / taper repair off (gated on
    // `!generation` below).
    if pol.systematic || !pol.generation {
        let on_wire = if pol.systematic {
            wire_sym.clone() // raw systematic source is the primary
        } else if pol.coded_wire {
            st.encoder.generate_repair()
        } else {
            wire_sym.clone()
        };
        let seqs = ctx.batch_counter.next(source_path);
        // The batch's `send_timestamp_us` is the key the ack's echo will
        // carry, so it must be the same instant the ETA stamp registered —
        // hence `src_send_ts_us` rather than a second `now_us()`.
        let batch = SymbolBatch::new(vec![on_wire], src_send_ts_us, seqs, source_path)
            .with_eta(eta_rel_us);
        if let Err(e) = ctx.transport.send_symbols(source_path, batch) {
            warn!(source_path, ?e, "failed to send window source symbol");
        }
        {
            let mut sched = ctx.scheduler.lock();
            if let Some(p) = sched.path_mut(source_path) {
                p.charge_in_flight(1);
                // Record the seq→path commitment + the BBR rate-sample send
                // snapshot so this seq's eventual WindowAck attribution
                // yields a clean send-interval delivery-rate sample on this
                // path. (Bulk back-to-back sends: app_limited = false; an
                // under-read sample can never lower the max filter.) A
                // paused feed must behave as absent: charging src_inflight
                // without the attribution to release it leaks src_inflight
                // and starves the anchor.
                if let Some(feed) = ctx.copa_feed.as_ref() {
                    feed.on_sent(wire_sym.block_id, source_path);
                    p.charge_src(1);
                    p.on_src_sent(wire_sym.block_id, false);
                }
            }
        }
        if let Some(ps) = ctx.stats.path(source_path) {
            ps.symbols_sent.fetch_add(1, Ordering::Relaxed);
        }
        ctx.stats.fec.total_source_symbols.fetch_add(1, Ordering::Relaxed);
        st.source_symbols_this_period += 1;
        // Charge the paced source send against the link-rate token bucket
        // (the TUN-read gate refills + admits it).
        if pol.cc_pace {
            st.src_tokens -= 1.0;
        }
    }

    // Track which path this source was sent on (for cross-path retransmission)
    st.source_path_map.insert(wire_sym.block_id, source_path);
    // DIAG: per-path first source placement count.
    if pol.diag_on {
        *st.c8c_src_placed.entry(source_path).or_insert(0) += 1;
    }

    // The `[DIAG] sout=` gauge: charge this seq to its placement path's
    // outstanding account, in lockstep with the sent_store insert above
    // (percap_track ⊆ plain_dyn_cap ⊆ the reliable && !generation retention
    // mode, under RWM_DIAG only). Released only by the ack that removes it
    // from the store. A cross-path retransmit does not re-attribute.
    if pol.percap_track {
        percap_charge(&mut st.percap_acct, &mut st.percap_out, wire_sym.block_id, source_path);
    }

    // Add to retransmit buffer for P_lost-based retransmit decisions.
    // Generation coding disables per-seq ARQ entirely — no retransmit
    // buffer (so the P_lost retransmit branch never fires and the tail
    // ARQ sweep never arms) and no per-seq deficit accounting. Recovery
    // is generation-level (more coded symbols for a short generation).
    if !pol.generation {
        let epsilon = {
            let sched = ctx.scheduler.lock();
            super::worst_eps_estimator(&sched)
                .map(|e| e.loss_rate())
                .unwrap_or(0.0)
        };
        st.retransmit_buffer.insert(wire_sym.block_id, (now_us(), epsilon, source_path));
        // Track correction deficit: this symbol needs epsilon coverage
        let mut sched = ctx.scheduler.lock();
        sched.deficit.on_send(wire_sym.block_id, source_path, epsilon);
    }

    // Redundant send for Realtime: duplicate source on second-best path
    if pol.protocol_hint == ProtocolHint::Realtime {
        let alt_path = {
            let sched = ctx.scheduler.lock();
            sched.redundant_source_path(source_path)
        };
        if let Some(alt) = alt_path {
            let seqs = ctx.batch_counter.next(alt);
            let batch = SymbolBatch::new(vec![wire_sym], now_us(), seqs, alt);
            if let Err(e) = ctx.transport.send_symbols(alt, batch) {
                warn!(alt, ?e, "failed to send redundant source symbol");
            }
            {
                let mut sched = ctx.scheduler.lock();
                if let Some(p) = sched.path_mut(alt) {
                    p.charge_in_flight(1);
                }
            }
            if let Some(ps) = ctx.stats.path(alt) {
                ps.symbols_sent.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    // Taper-driven repair accumulator with cwnd budget gate (ADR-0050,
    // paper §3.3).
    // Uses TaperFunction density τ(t) = A×(1-q)^t when GE data is available,
    // capped by spare capacity. Falls back to flat rate otherwise.
    // Generation coding does all coded emission in the ack-clocked
    // flow-control block in the main loop, so the per-source taper repair
    // is disabled here (it would double-emit and fight the flow control).
    if !pol.generation && st.encoder.window_size() > 1 {
        // RWM_EMIT_BATCH: the derived taper/span math refreshes at
        // burst granularity; per-symbol (bit-identical) when OFF.
        let taper_recompute = !emit_batch_live
            || st.taper_cache.is_none()
            || st.taper_cache_syms >= pol.emit_burst
            || now_us().saturating_sub(st.taper_cache_at_us)
                > TAPER_CACHE_MAX_AGE_US;
        let (repair_rate, span_params) = if !taper_recompute {
            let (rr, span, rtt) = st.taper_cache.unwrap();
            st.taper_cache_syms += 1;
            // The A* send-rate anchor is FED per symbol regardless —
            // the cache amortizes only the derived recomputation.
            if pol.unified_span && pol.astar_anchor_on {
                st.astar_anchor.on_send(Instant::now(), 1, rtt);
            }
            (rr, span)
        } else {
        // The cadenced rate (Stage 1c): evaluated outside both locks on a
        // miss, at most once per `RepairRateCache` period.
        let window_rate = cadenced_repair_rate(
            &mut st.rate_cache,
            ctx.fec_controller,
            ctx.scheduler,
            st.encoder.window_size(),
            now_us(),
        );
        let (repair_rate, span_params, taper_rtt) = {
            let sched = ctx.scheduler.lock();
            let spare = sched.spare_capacity();
            let path_estimator = super::worst_eps_estimator(&sched);
            match path_estimator {
                Some(est) => {
                    // `compute_repair_rate_capped`'s spare-capacity clamp,
                    // applied per symbol to the cadenced rate.
                    let flat_rate = window_rate.min(spare.max(0.0));
                    let taper = crate::control::TaperFunction::from_estimator(est, flat_rate);
                    let rr = if pol.taper_r_budget {
                        // Budget law (see `TaperBudget`): emission tracks
                        // r × source per coding window — the computed r* is
                        // consumed at the wire.
                        st.taper_budget.accrue(
                            flat_rate,
                            st.taper_offset,
                            &taper,
                            st.encoder.window_size(),
                            spare,
                        )
                    } else {
                        // `RWM_TAPER_R=0`: taper density at the current
                        // offset; Σ over an ack cycle = r once.
                        let density = taper.density(st.taper_offset as f64);
                        // Cap by spare capacity (never exceed link headroom)
                        density.min(spare.max(0.0))
                    };
                    // Span parameters (A*, Δ) from the same measured anchors
                    // (paper §5.3).
                    let span = if pol.unified_span {
                        let rate_sym = if pol.astar_anchor_on {
                            // This block runs once per source symbol send —
                            // feed the windowed-max send-rate anchor here and
                            // read it back (sym/s directly). None before the
                            // first measured bucket ⇒ A* clamps to 1, the
                            // cold start, ~SRTT/2 long.
                            let now_i = Instant::now();
                            st.astar_anchor.on_send(now_i, 1, est.rtt());
                            st.astar_anchor.rate(now_i, est.rtt()).unwrap_or(0.0)
                        } else {
                            (est.throughput() / pol.symbol_size.max(1) as f64).max(0.0)
                        };
                        let rtprop = est.rtt().as_secs_f64();
                        // The δ dial's named points, once (see
                        // `net::delta_budget_b`): the three-term store law
                        // reads the same map.
                        let b = super::delta_budget_b(pol.protocol_hint);
                        let d = (b * rtprop).min(2.0 * rtprop);
                        let a_star = ((rate_sym * d).ceil() as u64)
                            .clamp(1, st.encoder.window_size() as u64);
                        let delta = ((rate_sym * (est.jitter_us() / 1e6)).ceil()
                            as u64)
                            .clamp(1, 64);
                        // δ-honest shed law: refresh the derived
                        // deadline D(δ) and the 1−ρ budget at the
                        // live operating point (ε̂, r*, A*, σ²) —
                        // same anchors, no new constants.
                        if pol.shed_on {
                            st.shed_deadline_us_live = shed_deadline_us(
                                b,
                                est.rtt().as_micros() as u64,
                            );
                            st.shed_budget_frac =
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
        st.taper_cache = Some((repair_rate, span_params, taper_rtt));
        st.taper_cache_syms = 1;
        st.taper_cache_at_us = now_us();
        (repair_rate, span_params)
        };
        // Span-law sender trace (RWM_DIAG only).
        if pol.diag_on && pol.unified_span {
            let dnow = now_us();
            if dnow.saturating_sub(st.span_diag_last_us) > 500_000 {
                st.span_diag_last_us = dnow;
                let (ws, we) = st.encoder.window_span();
                let ack = ctx.window_ack_seq.load(Ordering::Relaxed);
                let transit = ctx.transport
                    .l0_transit_stats()
                    .map(|(e, g, td, ok, er, q)| {
                        format!(
                            " | shim enq={e} ge={g} tail={td} ok={ok} err={er} q={q}"
                        )
                    })
                    .unwrap_or_default();
                let dg = ctx.transport
                    .datagram_frame_stats(source_path)
                    .map(|(rx, tx)| format!(" dg_rx={rx} dg_tx={tx}"))
                    .unwrap_or_default();
                // The A* anchor gauge (windowed-max send rate + gap-discard
                // counters) when active.
                let ah = if pol.astar_anchor_on {
                    let (g, d) = st.astar_anchor.stats();
                    format!(
                        " ar={:.0} agap={}/{}",
                        st.astar_anchor
                            .rate(Instant::now(), Duration::from_millis(50))
                            .unwrap_or(0.0),
                        g,
                        d
                    )
                } else {
                    String::new()
                };
                // δ-honest shed gauge: cumulative shed / budget-refused
                // counts, the live 1−ρ fraction and deadline — the law's
                // liveness at the sender.
                let shg = if pol.shed_on {
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
                crate::readout!(
                    "[SPAN] t={:.1}s ack={} win=[{},{}] wsize={} a_star={:?} delta={:?} owed={:.2} rr={:.3} debt={:.2} retx_buf={}{}{}{}{}",
                    dnow.saturating_sub(pol.span_diag_start_us) as f64 / 1e6,
                    ack,
                    ws,
                    we,
                    st.encoder.window_size(),
                    span_params.map(|(a, _)| a),
                    span_params.map(|(_, d)| d),
                    st.taper_budget.owed(),
                    repair_rate,
                    st.repair_debt,
                    st.retransmit_buffer.len(),
                    shg,
                    ah,
                    transit,
                    dg,
                );
            }
        }
        // `repair_rate_floor` (`RWM_MIN_R`): floor the per-symbol
        // repair rate. Applied after the spare cap on purpose — the arm
        // forces the bandwidth spend.
        let repair_rate = repair_rate.max(pol.repair_rate_floor);
        // Generation coding: a small proactive overhead per generation so a
        // generation carries K_G(1+r) coded symbols and decodes without
        // waiting on a recovery round for the expected loss. Beyond this, the
        // frontier retention keeps coding any still-short generation until it
        // decodes. RWM_GEN_R overrides.
        let repair_rate = if pol.generation {
            repair_rate.max(pol.gen_repair_floor)
        } else {
            repair_rate
        };
        st.repair_debt += repair_rate;
        st.taper_offset += 1;

        while st.repair_debt >= 1.0 && st.encoder.window_size() > 0 {
            st.repair_debt -= 1.0;

            // P_lost-based correction symbol decision:
            // Check oldest un-ACKed symbol in retransmit buffer.
            // If P_lost is high enough, retransmit it (immediate decode).
            // Otherwise, generate a new repair symbol (FEC).
            let (correction_sym, correction_kind) = {
                let now = now_us();
                let (srtt_secs, rttvar_secs, epsilon) =
                    super::p_lost_inputs(&ctx.scheduler.lock());

                // Find oldest retransmit candidate and compute P_lost
                let mut use_retransmit = false;
                let mut retransmit_seq = 0u64;
                let oldest = st.retransmit_buffer
                    .iter()
                    .next()
                    .map(|(&s, &v)| (s, v));
                if let Some((seq, (send_time_us, eps_at_send, _path))) = oldest {
                    let age_secs = (now.saturating_sub(send_time_us)) as f64 / 1_000_000.0;
                    let p = crate::control::fec_rate::p_lost(age_secs, eps_at_send, srtt_secs, rttvar_secs);
                    // P(retransmit) = P_lost(t_k) (paper §3.2):
                    // probabilistic — a smooth transition from FEC to ARQ.
                    if rand::random::<f64>() < p {
                        // δ-honest shed: a candidate older than D(δ) arrives
                        // after the receiver's δ-horizon give-up —
                        // retransmitting it only serializes the stream
                        // behind a missed deadline. Shed it (within the ρ
                        // budget) and let this correction slot do fresh
                        // span-repair work instead.
                        let age_us_c = now.saturating_sub(send_time_us);
                        if pol.shed_on
                            && shed_allowed(
                                age_us_c,
                                st.shed_deadline_us_live,
                                st.shed_total,
                                ctx.stats.fec.total_source_symbols.load(Ordering::Relaxed),
                                st.shed_budget_frac,
                            )
                        {
                            st.retransmit_buffer.remove(&seq);
                            st.nack_retx_at.remove(&seq);
                            st.shed_seqs.insert(seq);
                            st.shed_total += 1;
                        } else {
                            if pol.shed_on
                                && st.shed_deadline_us_live > 0
                                && age_us_c > st.shed_deadline_us_live
                            {
                                // Past deadline but ρ-budget-refused: the
                                // serialize arm (visible in DIAG).
                                st.shed_denied += 1;
                            }
                            use_retransmit = true;
                            retransmit_seq = seq;
                        }
                    }
                }

                if use_retransmit {
                    // The taper copy, counted on every arm: this correction
                    // slot carries a copy of an already-sent seq rather than
                    // a fresh coded symbol.
                    ctx.stats.fec.taper_copy.fetch_add(1, Ordering::Relaxed);
                }
                if use_retransmit && pol.diag_on {
                    // Recovery-suppression trace: the P_lost-branch
                    // retransmit channel (fed by eps_at_send).
                    st.mpd_plost_retx += 1;
                }
                // A taper copy (`use_retransmit`) is a source COPY, not coded.
                let kind = if use_retransmit {
                    crate::monitor::stats::CorrectionKind::SourceCopy
                } else {
                    crate::monitor::stats::CorrectionKind::Coded
                };
                let sym = if use_retransmit {
                    // Retransmit: exact source symbol — from the sent-data
                    // store (reliable: survives window eviction) or the
                    // encoder window (EVICT).
                    st.sent_store
                        .get(&retransmit_seq)
                        .cloned()
                        .or_else(|| st.encoder.get_source(retransmit_seq))
                        .unwrap_or_else(|| st.encoder.generate_repair())
                } else if let Some((a_star, delta)) = span_params {
                    // Trailing solvable-span placement (paper §5.3): code
                    // over [max(ws, end−A*), end) with end = newest+1−Δ —
                    // every member has already landed when the repair does
                    // (FIFO + jitter guard), so the receiver's incremental GE
                    // solves a covered hole at arrival instead of entangling
                    // it with in-flight symbols. Falls back to the
                    // leading-window repair when the window is too young to
                    // trail.
                    let (ws, we) = st.encoder.window_span();
                    let end = (we + 1).saturating_sub(delta);
                    let start = end.saturating_sub(a_star).max(ws);
                    if end > start {
                        st.encoder
                            .generate_repair_range(
                                start,
                                (end - start).min(u16::MAX as u64) as u16,
                            )
                            .unwrap_or_else(|| st.encoder.generate_repair())
                    } else {
                        st.encoder.generate_repair()
                    }
                } else {
                    // Repair: a new FEC symbol over the leading window.
                    st.encoder.generate_repair()
                };
                (sym, kind)
            };

            // Reliable multipath places the correction by the law with the
            // ρ_fate penalty against the paths that carried the window
            // symbols it covers (the continuous form of
            // best_repair_path_avoiding). Single path ⇒ that path.
            // Non-reliable keeps the best-goodput pick.
            let correction_path = {
                let sched = ctx.scheduler.lock();
                if pol.reliable {
                    let covered = window_source_paths(&*st.encoder, &st.source_path_map);
                    sched.place_symbol(true, &covered).unwrap_or(source_path)
                } else {
                    select_repair_path(&sched, source_path)
                }
            };
            let seqs = ctx.batch_counter.next(correction_path);
            let batch = SymbolBatch::new(vec![correction_sym], now_us(), seqs, correction_path);
            let sent = match ctx.transport.send_symbols(correction_path, batch) {
                Ok(()) => true,
                Err(e) => {
                    warn!(correction_path, ?e, "failed to send correction symbol");
                    false
                }
            };
            {
                let mut sched = ctx.scheduler.lock();
                if let Some(p) = sched.path_mut(correction_path) {
                    p.charge_in_flight(1);
                }
            }
            if let Some(ps) = ctx.stats.path(correction_path) {
                ps.symbols_sent.fetch_add(1, Ordering::Relaxed);
            }
            ctx.stats.fec.record_correction(correction_kind, sent);
        }
    }

}

/// The worst-ε channel path and its estimator — the path every window-path
/// rate reader provisions for ([`super::worst_eps_channel_path`]: max
/// `loss_rate()` over [`super::channel_paths`], ties to the last maximum).
pub(crate) fn worst_eps_path(
    sched: &crate::scheduler::Scheduler,
) -> Option<(u32, &crate::control::LossEstimator)> {
    super::worst_eps_channel_path(sched).map(|(id, p)| (id, &p.estimator))
}

/// The window sender's repair rate on the Stage-1c cadence (see
/// [`RepairRateCache`]): source emission, `serve_gaps` and the
/// cumulative-ack advance all read it here. On a hit no evaluation runs; on
/// a miss the inputs are snapshotted under the controller → scheduler locks
/// (the one lock order used wherever both are held) and the ~0.1 ms rate
/// mix is evaluated after both are released, so ack processing never waits
/// on the solver. 0.0 only when the channel has no path at all.
pub(crate) fn cadenced_repair_rate(
    cache: &mut RepairRateCache,
    fec_controller: &parking_lot::Mutex<FecRateController>,
    scheduler: &parking_lot::Mutex<Scheduler>,
    window: usize,
    now_us: u64,
) -> f64 {
    let (path, period, snap) = {
        let ctrl = fec_controller.lock();
        let sched = scheduler.lock();
        let Some((path, est)) = worst_eps_path(&sched) else {
            return 0.0;
        };
        if let Some(rate) = cache.hit(now_us, window, path) {
            return rate;
        }
        (path, RepairRateCache::period_us(est.rtt()), ctrl.rate_snapshot(est, window))
    };
    let t0 = std::time::Instant::now();
    let rate = snap.rate();
    cache.add_eval_ns(t0.elapsed().as_nanos() as u64);
    cache.store(now_us, window, path, period, rate);
    rate
}
