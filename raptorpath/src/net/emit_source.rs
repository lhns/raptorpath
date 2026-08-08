//! The window sender's SOURCE-SYMBOL emission step: one framed packet in,
//! encoder intake + wire placement + accounting + proactive repair out.
//!
//! History (net seam pass 2, 2026-08-09): this was `macro_rules!
//! send_source_symbol!`, 645 lines defined inline in `run_window_sender` and
//! expanded at six call sites (packer flush ×3, packer push, the plain
//! one-packet-per-symbol path, and the `RWM_EMIT_BATCH` burst drain). It was
//! a macro only because it mutates ~30 captured locals — encoder, retention
//! store, per-path account/loan ledgers, the taper/span cache, the shed
//! ledger and the DIAG gauges — which no ordinary function could reach. Those
//! locals are now the fields of [`SenderState`]; the resolve-once
//! configuration it reads is [`SenderPolicy`](super::sender_policy::
//! SenderPolicy); the shared engine handles are [`SenderCtx`]. The six
//! expansions became six calls to [`emit_source`].
//!
//! BEHAVIOUR CONTRACT: the body is VERBATIM. It was moved by a mechanical
//! transform that only inserts a `st.` / `pol.` / `ctx.` prefix in front of a
//! captured name (never inside a string literal or a comment, never after a
//! `.`); stripping those prefixes reproduces the macro body byte-for-byte
//! modulo one dedent level. Nothing was reordered, merged, split or
//! re-guarded. In particular:
//!   * all ELEVEN scheduler acquisitions are the same eleven, in the same
//!     order, with the same scopes — `place_symbol` for the source pick, the
//!     `select_source_path` non-reliable pick, the `charge_in_flight` +
//!     Copa `on_sent`/`charge_src`/`on_src_sent` block, the redundant-source
//!     pick and its charge, the worst-loss ε read, the `deficit.on_send`
//!     write, the taper `spare_capacity`/estimator read (taken WITH the
//!     fec-controller lock, controller FIRST — unchanged), the per-correction
//!     worst-loss read, the correction placement pick, and the correction
//!     `charge_in_flight`;
//!   * the fec-controller lock is still acquired before the scheduler lock in
//!     the taper block and both are still released at the end of that block;
//!   * the `RWM_EMIT_BATCH` taper cache still short-circuits the derived
//!     recomputation while still feeding the A* send-rate anchor per symbol;
//!   * `borrow_lender` still decides the CHARGE path only, never the flight
//!     path (§16.22.1), and the loan ledger is still written in lockstep with
//!     the account charge;
//!   * the δ-honest shed decision still runs inside the `P_lost` retransmit
//!     branch, after the coin flip, and the ρ-budget-refused arm still
//!     increments `shed_denied` and serializes.
//!
//! NOT covered here: the paced GENERATION coded-emission block, the deficit
//! recovery loop, the NACK/gap repair dispatch, the tail ARQ sweep, the
//! ack/SACK drains and the dynamic store-cap refresh — all still inline in
//! `run_window_sender`, all still reading the same `SenderState` fields
//! through the struct. The state fields are `pub(crate)` for exactly that
//! reason: this module owns the emission step, not the state's lifetime.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tracing::warn;

use super::sender_policy::SenderPolicy;
use super::{
    BorrowAccount, CopaFeed, create_window_encoder, now_us, percap_borrow_lender, percap_charge,
    percap_loan_charge, percap_place_path, select_repair_path, select_source_path, shed_allowed,
    shed_deadline_us, window_source_paths,
};
use crate::control::{FecRateController, SendRateAnchor, TaperBudget};
use crate::fec::{FecBackend, WindowEncoder, WireSymbol};
use crate::monitor::stats::SharedStats;
use crate::scheduler::Scheduler;
use crate::transport::{QuicTransport, SymbolBatch};
use crate::control::fec_rate::ProtocolHint;

/// Staleness bound for the cached taper/span math (µs): one burst at
/// the service wall is ~3 ms; 50 ms only binds on low-rate paths.
const TAPER_CACHE_MAX_AGE_US: u64 = 50_000;

/// The shared engine handles the emission step needs. Built ONCE per
/// `run_window_sender` invocation from that function's own parameters — the
/// `ControlCtx` shape from net seam pass 1. Taken by SHARED reference: no
/// field is reassigned, all mutation goes through the `Mutex`/atomic handles.
pub(crate) struct SenderCtx<'a> {
    pub scheduler: &'a Arc<parking_lot::Mutex<Scheduler>>,
    pub fec_controller: &'a Arc<parking_lot::Mutex<FecRateController>>,
    pub transport: &'a Arc<QuicTransport>,
    pub stats: &'a Arc<SharedStats>,
    pub batch_counter: &'a AtomicU64,
    pub window_ack_seq: &'a Arc<AtomicU64>,
    /// feat/copa-sole-cc: `Some(..)` in plain in-order mode when the Copa
    /// delivery feed is on. `None` = shipped path.
    pub copa_feed: Option<&'a Arc<CopaFeed>>,
}

/// The window sender's MUTABLE state — every local the old
/// `send_source_symbol!` macro wrote, plus the encoder it drives.
///
/// These fields are still read and written by the rest of `run_window_sender`
/// (the ack/SACK drains, the NACK repair dispatch, the DIAG print, the tail
/// sweep); the struct exists so the emission step can be an ordinary function
/// instead of a macro, NOT to hide the state. Field docs are the originals,
/// moved here with their code.
pub(crate) struct SenderState {
    /// Codec pinned at startup (§16.4) — created once, never rebuilt.
    pub encoder: Box<dyn WindowEncoder>,

    /// RWM Phase A sent-data store (reliable mode only): seq → the exact
    /// source WireSymbol as sent. This is the retention contract — bytes
    /// retained until the peer's cumulative ack passes them (removal by ack
    /// ONLY), so an aged SACK-confirmed hole that slid out of the coding
    /// window is recovered by a targeted retransmit of exactly this symbol.
    /// Bounded by RELIABLE_STORE_MAX via TUN-read backpressure, never by
    /// eviction.
    pub sent_store: BTreeMap<u64, WireSymbol>,
    /// Retransmit buffer: maps seq → (send_time_us, epsilon_at_send, path_id).
    /// Used for P_lost-based retransmit decisions. Symbols are removed on ACK.
    /// METADATA only — under EVICT the source bytes die with window eviction.
    pub retransmit_buffer: BTreeMap<u64, (u64, f64, u32)>,
    /// Maps source seq → path it was sent on (for cross-path retransmission).
    /// BTreeMap (not HashMap) so the per-path ack attribution can range-query
    /// the seqs in a SACK / cumulative-ack span efficiently (feat/per-path-
    /// estimator); all other uses (insert/get/remove/retain) are unaffected.
    pub source_path_map: BTreeMap<u64, u32>,
    /// P10b: seq → last NACK-retransmit time (µs). Repeated gap acks for the
    /// same hole (they arrive every GAP_ACK_MIN_INTERVAL while it persists)
    /// must not resend the symbol more than once per SRTT — but MAY resend
    /// after an SRTT, which escalates naturally if the retransmit itself dies.
    /// Value = (last retransmit time µs, path the retransmit flew on). The
    /// path is the RWM_RECOV_MP live-flight input (the retransmit inherits
    /// the in-flight clock of its own path — feat/recovery-suppression);
    /// with the gate off only the time is read (byte-identical behavior).
    pub nack_retx_at: std::collections::HashMap<u64, (u64, u32)>,

    /// Last source path used (for NACK repair path selection outside the
    /// emission step).
    pub last_source_path: u32,
    /// Wall-clock (us) of the last NEW source-symbol send (ADR-0046
    /// idle-triggered recovery). Initialized to "now" so a transfer that
    /// stalls before sending anything is treated as active until it idles.
    pub last_source_send_us: u64,
    /// Last source-intake time (generation-mode pacing input).
    pub gen_last_source_us: u64,
    /// Source symbols sent in the current reporting period.
    pub source_symbols_this_period: u64,
    /// Source pacing token bucket (symbols). Refilled at the link rate each
    /// loop iteration; one token is consumed per source symbol on the wire.
    pub src_tokens: f64,

    // ── Per-path outstanding accounting (task #86, RWM_STORE_PERCAP) ─────
    /// seq → account path, in lockstep with `sent_store` (charge on insert,
    /// release on ack-removal ONLY — the retention contract).
    pub percap_acct: BTreeMap<u64, u32>,
    /// path → outstanding gauge (Σ over `percap_acct`; DIAG `sout=`).
    pub percap_out: std::collections::HashMap<u32, usize>,
    /// Bounded-borrowing loan ledger (feat/store-borrowing, §16.22):
    /// seq → (lender, flyer) for BORROWED seqs only (sparse; empty when the
    /// gate is off). Repaid by the same acks that release the account.
    pub percap_loans: BTreeMap<u64, (u32, u32)>,
    /// path → loans lent out (charged here, flying elsewhere) / borrowed in
    /// (flying here, charged elsewhere): fly_i = out_i − lent_i + borrowed_i.
    pub percap_lent: std::collections::HashMap<u32, usize>,
    pub percap_borrowed: std::collections::HashMap<u32, usize>,
    /// DIAG: cumulative loans granted (mechanism liveness at the gauge).
    pub percap_loans_total: u64,

    // ── Proactive-repair emission state ──────────────────────────────────
    /// Fractional repair accumulator: tracks sub-symbol repair debt.
    /// Driven by TaperFunction density when GE data is available,
    /// falls back to flat rate from compute_repair_rate_capped.
    pub repair_debt: f64,
    /// Source symbol counter for taper time offset (symbols since window
    /// start).
    pub taper_offset: u64,
    /// #85 budget-conserving taper (RWM_TAPER_R): emission consumes r as
    /// computed (a per-window budget) instead of r per ack cycle.
    pub taper_budget: TaperBudget,
    /// feat/anchor-hygiene (RWM_ASTAR_ANCHOR): windowed-max send-rate anchor
    /// fed by the sender's OWN send events, with gap-spanning buckets
    /// discarded.
    pub astar_anchor: SendRateAnchor,
    /// RWM_EMIT_BATCH per-burst cache: (repair_rate, span_params, estimator
    /// RTT at refresh).
    pub taper_cache: Option<(f64, Option<(u64, u64)>, Duration)>,
    pub taper_cache_syms: usize,
    pub taper_cache_at_us: u64,

    // ── δ-honest overload shedding (fix C, goal-gate "Unified Shedding") ──
    /// Seqs shed by the δ law (never served again; pruned at the cumulative
    /// frontier — the split_off twin).
    pub shed_seqs: BTreeSet<u64>,
    pub shed_total: u64,
    /// Past-deadline candidates the ρ budget REFUSED (the serialize arm of
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
    /// feat/c8-conversion: cumulative FIRST source placements per path.
    pub c8c_src_placed: std::collections::HashMap<u32, u64>,
    /// diag/unified-collapse: last ~500 ms span-law trace stamp.
    pub span_diag_last_us: u64,
    /// feat/recovery-suppression trace: the P_lost-branch retransmit channel.
    pub mpd_plost_retx: u64,
}

impl SenderState {
    /// Build the sender's mutable state. Every initializer here is the one it
    /// had inline in `run_window_sender` and every one of them is PURE
    /// (empty collections, zeroed counters, and the startup-pinned encoder) —
    /// the two wall-clock stamps are passed IN so they keep being sampled at
    /// the exact point in setup they were sampled at before.
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
        // Codec pinned at startup (§16.4) — created once, never rebuilt.
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
            percap_loans: BTreeMap::new(),
            percap_lent: std::collections::HashMap::new(),
            percap_borrowed: std::collections::HashMap::new(),
            percap_loans_total: 0,
            repair_debt: 0.0,
            taper_offset: 0,
            taper_budget: TaperBudget::new(),
            astar_anchor: SendRateAnchor::new(),
            taper_cache: None,
            taper_cache_syms: 0,
            taper_cache_at_us: 0,
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
/// The former `send_source_symbol!($framed)`. `percap_caps` / `percap_bounds`
/// / `percap_rr` / `emit_batch_live` are per-ITERATION inputs (refreshed by
/// the dynamic-cap throttle in the main loop, read-only here), so they stay
/// locals of `run_window_sender` and are passed in rather than living in
/// [`SenderState`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_source(
    framed: &[u8],
    st: &mut SenderState,
    pol: &SenderPolicy,
    ctx: &SenderCtx<'_>,
    percap_caps: &std::collections::HashMap<u32, usize>,
    percap_bounds: &std::collections::HashMap<u32, usize>,
    percap_rr: &std::collections::HashMap<u32, (Option<f64>, Option<f64>)>,
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

    // RWM Phase A retention: the store keeps the sent bytes until
    // the peer acks them — the coding window may slide past this
    // symbol, but the data can no longer be destroyed by eviction.
    // Generation coding turns per-seq ARQ OFF, so it needs NO sent
    // store (recovery is more coded symbols for the generation, never
    // an exact-seq resend); backpressure uses the encoder's retained
    // size instead. The GenerationEncoder itself retains the sources.
    if pol.reliable && !pol.generation {
        st.sent_store.insert(wire_sym.block_id, wire_sym.clone());
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
        if pol.reliable {
            let picked = {
                let sched = ctx.scheduler.lock();
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
                                st.percap_out.get(&pid).copied().unwrap_or(0),
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
                if pol.percap_borrow_on && !own_open {
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
                                        st.percap_lent.get(&p).copied().unwrap_or(0),
                                    )
                                    .saturating_add(
                                        st.percap_borrowed
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
            let sched = ctx.scheduler.lock();
            select_source_path(&sched)
        }
    };
    st.last_source_path = source_path;
    // ADR-0046 idle-triggered recovery: stamp the last NEW-source send
    // so the NACK throttle can tell "actively pushing data" (repairs
    // would load a congested path) from "idle except for a hole"
    // (targeted recovery is free).
    st.last_source_send_us = now_us();
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
    if pol.systematic || !pol.generation {
        let on_wire = if pol.systematic {
            wire_sym.clone() // raw systematic source is the primary
        } else if pol.coded_wire {
            st.encoder.generate_repair()
        } else {
            wire_sym.clone()
        };
        let batch_seq = ctx.batch_counter.fetch_add(1, Ordering::Relaxed);
        let batch = SymbolBatch {
            symbols: vec![on_wire],
            send_timestamp_us: now_us(),
            batch_seq,
            path_id: source_path,
        };
        if let Err(e) = ctx.transport.send_symbols(source_path, batch) {
            warn!(source_path, ?e, "failed to send window source symbol");
        }
        {
            let mut sched = ctx.scheduler.lock();
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
                if let Some(feed) = ctx.copa_feed.as_ref().filter(|f| !f.n1_paused()) {
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
        // Fix 1: charge the paced source send against the link-rate
        // token bucket (the TUN-read gate refills + admits it).
        if pol.cc_pace {
            st.src_tokens -= 1.0;
        }
    }

    // Track which path this source was sent on (for cross-path retransmission)
    st.source_path_map.insert(wire_sym.block_id, source_path);
    // feat/c8-conversion DIAG: per-path FIRST source placement count.
    if pol.diag_on {
        *st.c8c_src_placed.entry(source_path).or_insert(0) += 1;
    }

    // task #86: charge this seq to its placement path's outstanding
    // account, in lockstep with the sent_store insert above (percap_on
    // ⊆ plain_dyn_cap ⊆ the reliable && !generation retention mode).
    // Released only by the ack that removes it from the store. A
    // cross-path retransmit does NOT re-attribute: the account bounds
    // the pipe the symbol was ADMITTED against (its dwell there ends
    // at the same ack either way. percap_track ⊇ percap_on: under
    // RWM_DIAG the maps are maintained as a gauge only — see decl).
    if pol.percap_track {
        // feat/store-borrowing: a LOAN charges the LENDER's account
        // while the symbol flies on `source_path` (§16.22.1 — the
        // ledger moves, the wire placement does not). The loan
        // ledger corrects the pipe gauge (fly = out − lent +
        // borrowed) and repays on the same ack that releases the
        // account entry.
        let charge_path = borrow_lender.unwrap_or(source_path);
        percap_charge(&mut st.percap_acct, &mut st.percap_out, wire_sym.block_id, charge_path);
        if let Some(lender) = borrow_lender {
            percap_loan_charge(
                &mut st.percap_loans,
                &mut st.percap_lent,
                &mut st.percap_borrowed,
                wire_sym.block_id,
                lender,
                source_path,
            );
            st.percap_loans_total += 1;
        }
    }

    // Add to retransmit buffer for P_lost-based retransmit decisions.
    // Generation coding disables per-seq ARQ entirely — no retransmit
    // buffer (so the P_lost retransmit branch never fires and the tail
    // ARQ sweep never arms) and no per-seq deficit accounting. Recovery
    // is generation-level (more coded symbols for a short generation).
    if !pol.generation {
        let epsilon = {
            let sched = ctx.scheduler.lock();
            sched.active_paths().iter()
                .filter_map(|id| sched.path(*id))
                .max_by(|a, b| a.estimator.loss_rate().partial_cmp(&b.estimator.loss_rate()).unwrap_or(std::cmp::Ordering::Equal))
                .map(|p| p.estimator.loss_rate())
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
            let batch_seq = ctx.batch_counter.fetch_add(1, Ordering::Relaxed);
            let batch = SymbolBatch {
                symbols: vec![wire_sym],
                send_timestamp_us: now_us(),
                batch_seq,
                path_id: alt,
            };
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

    // Taper-driven repair accumulator with cwnd budget gate (ADR-0050).
    // Uses TaperFunction density τ(t) = A×(1-q)^t when GE data is available,
    // capped by spare capacity. Falls back to flat rate otherwise.
    // Generation coding does ALL coded emission in the ack-clocked
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
        let (repair_rate, span_params, taper_rtt) = {
            let ctrl = ctx.fec_controller.lock();
            let sched = ctx.scheduler.lock();
            let spare = sched.spare_capacity();
            let path_estimator = sched
                .active_paths()
                .iter()
                .filter_map(|id| sched.path(*id))
                .max_by(|a, b| a.estimator.loss_rate().partial_cmp(&b.estimator.loss_rate()).unwrap_or(std::cmp::Ordering::Equal))
                .map(|p| &p.estimator);
            match path_estimator {
                Some(est) => {
                    let flat_rate = ctrl.compute_repair_rate_capped(est, spare, st.encoder.window_size());
                    let taper = crate::control::TaperFunction::from_estimator(est, flat_rate);
                    let rr = if pol.taper_r_budget {
                        // #85 budget law (see TaperBudget decl above):
                        // emission tracks r × source per coding window
                        // — the computed r* is consumed at the wire.
                        st.taper_budget.accrue(
                            flat_rate,
                            st.taper_offset,
                            &taper,
                            st.encoder.window_size(),
                            spare,
                        )
                    } else {
                        // LEGACY (measured-inert): taper density at the
                        // current offset; Σ over an ack cycle = r once.
                        let density = taper.density(st.taper_offset as f64);
                        // Cap by spare capacity (never exceed link headroom)
                        density.min(spare.max(0.0))
                    };
                    // §16.20.3 span parameters (A*, Δ) from the same
                    // measured anchors — see the unified_span decl.
                    let span = if pol.unified_span {
                        let rate_sym = if pol.astar_anchor_on {
                            // feat/anchor-hygiene: this block runs
                            // once per SOURCE symbol send — feed the
                            // windowed-max send-rate anchor here and
                            // read it back (sym/s directly; no
                            // byte/EWMA detour). None before the
                            // first measured bucket ⇒ A* clamps to 1
                            // — the honest cold-start, ~SRTT/2 long.
                            let now_i = Instant::now();
                            st.astar_anchor.on_send(now_i, 1, est.rtt());
                            st.astar_anchor.rate(now_i, est.rtt()).unwrap_or(0.0)
                        } else {
                            (est.throughput() / pol.symbol_size.max(1) as f64).max(0.0)
                        };
                        let rtprop = est.rtt().as_secs_f64();
                        let b = match pol.protocol_hint {
                            ProtocolHint::Realtime => 0.5,
                            ProtocolHint::Auto => 1.0,
                            ProtocolHint::Bulk => 2.0,
                        };
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
        // diag/unified-collapse: span-law sender trace (RWM_DIAG only).
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
                // feat/anchor-hygiene: the A* anchor gauge (windowed-
                // max send rate + gap-discard counters) when active.
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
                // δ-honest shed gauge (fix C): cumulative shed /
                // budget-refused counts, the live 1−ρ fraction and
                // deadline — the law's liveness at the sender.
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
                eprintln!(
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
        // RWM Phase C raise-r arm (§16.5): floor the per-symbol
        // repair rate to make the window rateless-fungible. Applied
        // AFTER the spare cap on purpose — the experiment forces the
        // bandwidth spend to test aggregation, on links with headroom.
        let repair_rate = repair_rate.max(pol.repair_rate_floor);
        // Generation coding: a small proactive overhead per generation
        // (the oracle's r ≈ 0.10) so a generation carries K_G(1+r) coded
        // symbols and decodes without waiting on a recovery round for
        // the expected loss. Beyond this, the frontier-retention keeps
        // coding any still-short generation until it decodes (fungible,
        // no per-seq ARQ). RWM_GEN_R overrides.
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
            let correction_sym = {
                let now = now_us();
                let (srtt_secs, rttvar_secs, epsilon) = {
                    let sched = ctx.scheduler.lock();
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
                let oldest = st.retransmit_buffer
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
                                // Past deadline but ρ-budget-refused:
                                // the serialize arm (visible in DIAG).
                                st.shed_denied += 1;
                            }
                            use_retransmit = true;
                            retransmit_seq = seq;
                        }
                    }
                }

                if use_retransmit && pol.diag_on {
                    // feat/recovery-suppression trace: the P_lost-
                    // branch retransmit channel (fed by eps_at_send).
                    st.mpd_plost_retx += 1;
                }
                if use_retransmit {
                    // Retransmit: exact source symbol — from the
                    // sent-data store (reliable: survives window
                    // eviction) or the encoder window (EVICT).
                    st.sent_store
                        .get(&retransmit_seq)
                        .cloned()
                        .or_else(|| st.encoder.get_source(retransmit_seq))
                        .unwrap_or_else(|| st.encoder.generate_repair())
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
                    // Repair: generate a new FEC symbol (legacy
                    // leading-window emission)
                    st.encoder.generate_repair()
                }
            };

            // RWM Phase B (§16.3): reliable multipath places the
            // correction by the law with the ρ_fate penalty against the
            // paths that carried the window symbols it covers (the
            // continuous form of best_repair_path_avoiding). Single path
            // ⇒ that path. Non-reliable keeps the best-goodput pick.
            let correction_path = {
                let sched = ctx.scheduler.lock();
                if pol.reliable {
                    let covered = window_source_paths(&*st.encoder, &st.source_path_map);
                    sched.place_symbol(true, &covered).unwrap_or(source_path)
                } else {
                    select_repair_path(&sched, source_path)
                }
            };
            let batch_seq = ctx.batch_counter.fetch_add(1, Ordering::Relaxed);
            let batch = SymbolBatch {
                symbols: vec![correction_sym],
                send_timestamp_us: now_us(),
                batch_seq,
                path_id: correction_path,
            };
            if let Err(e) = ctx.transport.send_symbols(correction_path, batch) {
                warn!(correction_path, ?e, "failed to send correction symbol");
            }
            {
                let mut sched = ctx.scheduler.lock();
                if let Some(p) = sched.path_mut(correction_path) {
                    p.charge_in_flight(1);
                }
            }
            if let Some(ps) = ctx.stats.path(correction_path) {
                ps.symbols_sent.fetch_add(1, Ordering::Relaxed);
            }
            ctx.stats.fec.total_repair_symbols.fetch_add(1, Ordering::Relaxed);
        }
    }

}
