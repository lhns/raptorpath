//! Runtime experiment/feature gates — the `RWM_*` environment surface of the
//! window/generation engine, resolved ONCE at engine start.
//!
//! History (code-consolidation pass, 2026-07-27): `net/mod.rs` grew a
//! ~70-env-var gate block read inline mid-function across `run_impl`, the
//! receiver task and `run_window_sender`. This module centralizes the ENV
//! resolution: every gate is read exactly once per engine start
//! (`RuntimeGates::resolve()`), documented in one place with its default and
//! its decision record (ADR / goal-gate section), and the resolved struct is
//! passed to the tasks that consume it. When a register Class-C gate exists,
//! its deprecation warning (`config::deprecated_env_flag`) fires here, once
//! (none currently — the 2026-07-27 consolidation passes executed the whole
//! DEPRECATION REGISTER).
//!
//! Behavior contract: `resolve()` reproduces the exact per-site semantics the
//! scattered reads had (same defaults, same parse/clamp rules, same chaining
//! through `unified_active()` / `copa_wire_active()` / the
//! `RWM_ANCHOR_HYGIENE` umbrella). Fields whose EFFECTIVE default depends on
//! the runtime MODE (generation / systematic — e.g. `RWM_GEN_R`,
//! `RWM_REACT_CAP`, `RWM_INFL_BDP`, `RWM_REPORT_GENS`) store the raw override
//! (`Option<_>`) and the mode-dependent default stays at the use site.
//!
//! NOT covered here (deliberately — each is already a resolve-once site in
//! its own module): the substrate-CC policy `RWM_QUIC_CC` (transport/quic.rs,
//! ADR-0054), the MTU floor `RWM_MTU_FLOOR` (transport, ADR-0055), the
//! compact v5 DATA framing `RWM_WIRE_COMPACT` (transport/protocol.rs,
//! default ON since 2026-08-06 — goal-gate "Window Decoupling + MTU
//! Scaling"), the Copa
//! wire/δ family `RWM_COPA_WIRE`/`RWM_COPA_DELTA`/`RWM_COPA_COMPETE`
//! (scheduler, cached `OnceLock`, ADR-0062), the stall-witness umbrella
//! member `RWM_CLOCK_GAP` (control/anchor.rs, ADR-0061), the RS trace knob
//! `RWM_RS_TRACE` (scheduler CopaState), the estimator heavy-math cadence
//! `RWM_EST_CADENCE` (control/estimator.rs `OnceLock`, default OFF —
//! goal-gate "Receiver Per-Message Wall" + "Ship The Wins 1": BOCD at its
//! design cadence instead of per message; the composed default flip with
//! `RWM_POOL_ANCHOR`/`RWM_EMIT_BATCH` was measured 2026-08-07 and REVERTED
//! by its pre-set c7 clause — the `pool_anchor` field below reads the same
//! `scheduler::pool_anchor_active()` resolution the send-path feed
//! consults, riding the est resolution), and the harness/bench-only knobs
//! (`RWM_L0_*`, `RWM_B_*`, `RWM_SL_*`, `RWM_PERF_TIMEOUT_S`, …).

use crate::config::{anchor_gate, anchor_gate_default, env_flag};

fn env_parse<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok().and_then(|s| s.parse::<T>().ok())
}

/// The engine's env-gate surface, resolved once at engine start.
///
/// Grouping mirrors the regime map: the unified machine, anchor hygiene,
/// store/flow-control laws, the generation stack, CC/pacing, the recovery
/// plane, and the instruments.
#[derive(Debug, Clone)]
pub struct RuntimeGates {
    // ── The unified machine (ADR-0064) ───────────────────────────────────
    /// `RWM_UNIFIED` (default ON): the one-span-machine default; `=0` is the
    /// legacy opt-out arm. OPT-OUT SEMANTICS since 2026-07-28 (streaming
    /// machine DELETED, register RE-TESTED/CLEARED via the crown re-test):
    /// `=0` + Realtime selects the LEGACY-RLC windowed machine — it can no
    /// longer select the streaming two-layer code.
    pub unified: bool,
    /// `RWM_UNIFIED_SHED` (default ON): δ-honest overload shedding on the
    /// EVICT path within the derived 1−ρ budget; `=0` = serializing arm.
    pub unified_shed: bool,
    /// `RWM_TAPER_R` (default = `unified`): budget-conserving taper emission
    /// (#85 quantity fix); `=0` = legacy per-ack-cycle accrual (ADR-0063/64).
    pub taper_r: bool,

    // ── Anchor hygiene (ADR-0061; `RWM_ANCHOR_HYGIENE` umbrella) ─────────
    /// `RWM_ASTAR_ANCHOR` (umbrella default ON): windowed-max send-rate A*
    /// anchor with clock-gap discard; engages only under the unified span.
    pub astar_anchor: bool,
    /// `RWM_MSTAR_ANCHOR` (umbrella default ON): measured RTprop floor +
    /// fast-seed rate filter + derived (M*+2)·G win backstop; the plain-live
    /// subset (peer-report RTT-feed suppression) is not generation-gated.
    pub mstar_anchor: bool,
    /// `RWM_PLAIN_RS` (umbrella default OFF): plain-mode BBR send-interval
    /// sampler (sampling-only CopaFeed); the honest-cap law's anchor input.
    pub plain_rs: bool,
    /// `RWM_HONEST_ANCHOR` (**DEFAULT ON since 2026-08-11**, flip-battery
    /// F7; `=0` = the legacy full-window fold, kept re-runnable): the BtlBw
    /// windowed-max read off a monotonic max-deque — value-identical
    /// statistic, O(1) amortized instead of the per-sample full-window fold
    /// whose O(window·rate) cost under `RWM_PLAIN_RS` is the measured c1
    /// −35% (sender CPU/byte +61…64%, latlever CPU gauge).
    /// Resolved via `scheduler::honest_anchor_active()` (cached — CopaState
    /// construction reads the same resolution).
    pub honest_anchor: bool,
    /// `RWM_HONEST_K` (umbrella default OFF; goal-gate "Honest Inputs"):
    /// `EchoRatioMin` fed the RAW per-sample echo/RTprop ratio at the
    /// sample clock instead of the smoothed SRTT at the refresh clock —
    /// the windowed MIN reads the delay distribution's floor (the measured
    /// jit25 ×1.34 inversion removed). Resolved via
    /// `scheduler::honest_k_active()` (cached).
    pub honest_k: bool,

    // ── Store / flow-control laws ─────────────────────────────────────────
    /// `RWM_STORE_SACK_RELEASE` (default ON): SACK-clocked slot release —
    /// slot uncounted, recoverability retained (ADR-0060; supersedes the
    /// removed `RWM_SACK_PRUNE`).
    pub store_sack_release: bool,
    /// `RWM_STORE_PATHS` (default ON): path-scaled outstanding pool for
    /// N ≥ 2 live paths; N = 1 keeps the legacy law bit-exactly (ADR-0058).
    pub store_paths: bool,
    /// `RWM_STORE_PATH_POOL` (default 2048): per-live-path pool knee.
    pub store_path_pool: usize,
    /// `RWM_STORE` (unset = mode default): STATIC store/retention override —
    /// setting it disables the plain-mode dynamic BDP cap (the sweep knob).
    pub store_override: Option<usize>,
    /// Whether `RWM_STORE` was SET at all (even unparsable) — the dynamic-cap
    /// disable keys on presence, the value on parse (legacy semantics kept).
    pub store_env_set: bool,
    /// `RWM_STORE_GAIN` (default 2.0, clamped [1, 64]): window = gain × BDP.
    pub store_gain: f64,
    /// `RWM_STORE_BOOT` (default 128): outstanding cap before the BtlBw
    /// anchor warms.
    pub store_boot: usize,
    /// `RWM_STORE_CAPW` (default OFF): capacity-weighted SHARED outstanding
    /// pool — pool = Σ_i honest per-path cap over live paths (each path earns
    /// depth for its OWN pipe + recovery round; one pool, so borrowing stays
    /// free — ADR-0058's pooled verdict kept). The c8-aware pool law
    /// (the ADR-0058 "c8 WATCH" follow-up). Engaged N ≥ 2 with warm anchors;
    /// falls back to the configured pooled law until anchors live. Reads
    /// honestly only with the `RWM_PLAIN_RS` sampler (the battery arm
    /// composes it); with the over-reading legacy anchor it clamps to the
    /// N×knee ceiling ≡ the path-scaled law.
    pub store_capw: bool,
    /// `RWM_STORE_PERCAP` (default OFF): per-path outstanding accounts
    /// (task #86; symmetric-cell tool, c8 successor named — ADR-0058).
    pub store_percap: bool,
    /// `RWM_PERCAP_GUARD` (default ON under percap): delay-aware redirect
    /// bound; `=0` = the measured c8-regression control arm.
    pub percap_guard: bool,
    /// `RWM_STORE_BORROW` (default OFF): bounded account borrowing
    /// (§16.22; loans ≡ 0 at symmetric cells by theorem).
    pub store_borrow: bool,
    /// `RWM_HONEST_CAP` (default ON where `plain_rs` is live): honest
    /// floor-clock store caps on the send-interval anchor (§16.23).
    pub honest_cap: bool,
    /// `RWM_POOL_ANCHOR` (default = the `RWM_EST_CADENCE` resolution — OFF
    /// with everything unset, ON with the est opt-in; goal-gate "Ship The
    /// Wins 1"): at N ≥ 2 live paths the pooled-store cap is Σ_i
    /// honest_store_cap on the per-path hygiene-grade SEND-interval anchor
    /// (`SendRateAnchor` fed at `charge_in_flight`; ratcheted half-window
    /// mean — burst-immune, clock-gap discard) clamped [floor, N·knee] —
    /// replacing the legacy ack-interval windowed-max as the CAP's rate
    /// input (the §16.35 c7 blocker: the est-cadence ack clock's burst
    /// peaks inflated it a further ×3.4–3.7). The Copa cwnd feed and all
    /// N = 1 laws are bit-exactly untouched; no CopaFeed machinery runs
    /// (the −22…−27 c7 RS price unreachable). MEASURED 2026-08-07: it
    /// recovers most of est's c7 deficit (est-only 0.938/0.949×Σ →
    /// est+pa 0.968/0.959) but the honest pool becomes the binder (the
    /// send side has no un-self-referential uncapped rate source) — the
    /// composed default flip failed its pre-set c7 ≥ 0.97 clause and
    /// REVERTED. `=0` under the est opt-in = the blocker-reproduction arm;
    /// resolved via `scheduler::pool_anchor_active()` (cached — the
    /// send-path feed reads the same resolution).
    pub pool_anchor: bool,
    /// `RWM_POOL_DELIV` (default = the `pool_anchor` resolution ⇒ OFF with
    /// everything unset; goal-gate "Ship The Wins 1b" arm A): the N ≥ 2 pool
    /// law's rate input gains a per-path DELIVERY-CLOCKED term
    /// (`DeliveryRateAnchor` — BBR `GenerateRateSample`: delivered /
    /// max(send_elapsed, ack_elapsed), windowed-max ≈10·RTprop, sub-RTprop
    /// samples rejected-and-accumulated, ADR-0061 clock-gap discard), read as
    /// `max(delivery, send_mean)` — ONE formula, no branch. It exists to test
    /// attempt 1's named binder: a send-derived rate cannot ratchet above the
    /// cap-limited carried rate, but a delivery clock is bounded by
    /// delivered-packet PHYSICS and CAN. Shadow-only: no cwnd/`max_bw`/
    /// pacing/`src_inflight` consumer can reach it; N = 1 untouched.
    pub pool_deliv: bool,
    /// `RWM_FLOOR_BOUND` (default OFF — a pure A/B arm; goal-gate "Ship The
    /// Wins 1b" arm B): bound the BtlBw anchor FLOOR by the honest
    /// send-anchor rate (`min(gain·max_bw·RTprop, gain·sr·RTprop)`) so the
    /// ack-interval over-read cannot inflate cwnd (measured 5860 vs 1779) —
    /// making the prior default's ACCIDENTAL Σcwnd-governor escape derived.
    /// Still a floor, never a cap; legacy verbatim with the anchor cold.
    pub floor_bound: bool,
    /// `RWM_ACK_MERGE` (**default ON since 2026-08-08**; `=0` is the opt-out
    /// A/B arm. Goal-gate "Unlock The Default 1: ack-merge" built it and
    /// "Ack-Merge Flip" shipped it; paper §16.42): in WINDOW MODE ONLY,
    /// suppress the legacy
    /// per-batch `ControlMessage::Ack` (whose send site sits after the
    /// window/block branch and so fires in window mode too), make the SACK
    /// `WindowAck` unconditional at exactly that cadence, and carry the
    /// `Ack`'s payload in the v6 cumulative `cum_expected`/`cum_received`
    /// counters — TWO control datagrams per data message become ONE.
    /// Every `Ack`-arm consumer is re-homed onto the counter diff with its
    /// own guard preserved (`gap_q`, the `copa_feed`/`n1_paused` three-way
    /// branch, the `expected > 0` guard). Block mode keeps the legacy `Ack`
    /// bit-exactly. Changes the datagram COUNT only: the delivery statistic,
    /// its cadence and its counts are unperturbed. Resolved via
    /// `scheduler::ack_merge_active()` (cached — the receiver arm and the
    /// sender arm read the same resolution).
    ///
    /// FLIPPED ON by its own pre-registered gate set (2026-08-08, ×8 both
    /// seeds, full scope + sustained + crown): c1 +12.7% / +13.0% with
    /// receiver CPU per bit −9.1% / −8.4%, every no-regression gate held
    /// (c7/c8/sc2/sc3 within σ of their own same-session controls, crown
    /// 1000/1000 in 32/32 reps, dnf 0/164). With the gate ON by default the
    /// window-mode `!suppress_legacy_ack` branch in `net/mod.rs` is DEAD
    /// unless the operator sets `RWM_ACK_MERGE=0`; it is scheduled for
    /// deletion in refactor seam **B2**. BLOCK MODE KEEPS IT — `block_arq`'s
    /// dup-ack loss channel (`LATER_ACK_LOSS_THRESHOLD`) is built on the
    /// legacy `Ack`'s 1:1 per-batch cadence, and the gate is scoped
    /// `gates.ack_merge && recv_window_mode` precisely so that stays true.
    pub ack_merge: bool,
    /// `RWM_LOSS_SENT_TRUTH` (**default OFF**) — feed the per-path loss
    /// estimator the sender's own `symbols_sent` delta instead of the
    /// receiver's global-`batch_seq` gap estimate. See
    /// `scheduler::loss_sent_truth_active()` and
    /// `PathState::sender_truth_loss_delta` for the law and its provenance;
    /// the defect is measured in goal-gate "Ack-Cadence Measurement (VM)"
    /// READOUT 4 (apparent/realized 37-93x at every multipath cell).
    pub loss_sent_truth: bool,
    /// `RWM_RELEASE_1TO1` (**default OFF**) — release the in-flight
    /// budget by the sender's own `d(symbols_sent) - d(cum_received)` instead
    /// of the contaminated `expected - received` counter delta. SIBLING of
    /// [`Self::loss_sent_truth`]: same clean operand pair, independent
    /// cursors, one gate per quantity (that one feeds the ESTIMATOR, this one
    /// feeds the LEDGER). See `scheduler::release_1to1_active()` and
    /// `PathState::sender_truth_release_delta`.
    pub release_1to1: bool,
    /// `RWM_CHARGE_RECOVERY` (**default OFF**) — meter the SACK-gap
    /// retransmit and the NACK repair margin at their wire handoff
    /// (`charge_in_flight` + `consume_pace_tokens` + `symbols_sent`), as every
    /// other channel already does. See `scheduler::charge_recovery_active()`;
    /// the divergence is PIPELINE VERIFICATION MATRIX rows 2 + 6 and is
    /// bounded by `pacer_debit_bounds_only_the_source_arm_not_the_wire`.
    pub charge_recovery: bool,
    /// `RWM_PATIENCE_DERIVED` (default OFF — the A/B arm; goal-gate "Unlock
    /// The Default 2: derived patience"): the `NACK_RETX_COOLDOWN_FLOOR_US`
    /// = 10 ms literal — 10× RFC 9002's kGranularity, and at c2/c7 at or
    /// ABOVE the 9/8·srtt term it was meant to floor — becomes
    /// `net::patience_floor_us` = the engine's timer granularity (the sender
    /// loop's 1 ms wake, coinciding with RFC 9002's RECOMMENDED value) + the
    /// path's OWN measured RTT jitter (`PathState::rtt_jitter_us`), clamped
    /// at one srtt, with the legacy floor kept verbatim before the first
    /// clock sample. Applied at the two BEHAVIOURAL sites only: the
    /// kGranularity analog inside `mp_time_threshold_split` and the per-seq
    /// retransmit cooldown. The tail-sweep fallback is left alone (it feeds
    /// `(srtt·2).clamp(25 ms, 100 ms)`, so every value ≤ 12.5 ms is
    /// identical — INERT, unit-tested). RFC 9002's kTimeThreshold 9/8 and
    /// kPacketThreshold 3 are UNTOUCHED.
    pub patience_derived: bool,
    /// `RWM_SIDLE_DERIVED` (default OFF — DIAG-only and behaviour-inert;
    /// goal-gate "Unlock The Default 2"): print `sidle2=`/`idle2=` beside
    /// the UNCHANGED legacy `sidle=`/`idle=` gauges, computed by
    /// `net::stall_threshold_us` (the legacy 3 ms re-expressed as 3 × the
    /// MEASURED inter-emission-event interval, floored at the legacy value
    /// and capped at the hole-refresh cadence). Answers whether §16.37's and
    /// §16.39's stall evidence was a fixed-threshold artifact of a batched
    /// emitter — measured on every arm, controls included.
    pub sidle_derived: bool,
    /// `RWM_WIN_DECOUPLE` (default OFF — the A/B arm; goal-gate "Window
    /// Decoupling + MTU Scaling" part 1): window/inflight decoupling at
    /// N = 1 plain reliable window. The 1024-latch's three roles split:
    /// wire budget = the live HEAD SPAN (last_sent − SACK/cum frontier;
    /// recovery-stalled holes excluded) gated at
    /// allow = anchor·(K + gain − 1) + rate·min(stall_age, R) — the
    /// stall-insurance term explicit and continuous (grows at the anchor
    /// rate during any frontier freeze, resets on advance); hole/retention
    /// capacity = cap_ret (residence + R_ins + one recovery round),
    /// memory-clamped at 4096. Under Copa-sole the residence term is
    /// gain·Σcwnd and the 1024 clamp ceiling lifts to cap_ret (the B1
    /// jitter-cell dwell-ceiling release). N ≥ 2 keeps the configured
    /// pooled laws bit-exactly; the N1-scoped sampling anchor pauses.
    pub win_decouple: bool,

    // ── Placement (goal-gate "C8 Slow-Path Conversion") ──────────────────
    /// `RWM_PLACE_SLACK` (default OFF — the A/B arm): frontier-slack
    /// placement — the §16.3 marginal cost's load term becomes
    /// max(0, Ê_i − S)/ref with S = the measured frontier slack
    /// (stream span / cumulative-ack rate, clamped ≤ 250 ms; 0 until the
    /// ack-rate EWMA has a sample, 0 at N = 1). S = 0 reproduces the
    /// shipped cost bit-exactly — a strict continuous generalization
    /// (deadline-aware water-filling: the slow path earns placements up to
    /// the backlog it can deliver by frontier need-time). Plain reliable
    /// window only.
    pub place_slack: bool,
    /// `RWM_COLD_PLACE` (anchor-hygiene family member, default OFF): price an
    /// UNMEASURED leg's latency anchor at the active set's fastest MEASURED
    /// srtt instead of the 50-ms `DEFAULT_SRTT`-class seed that
    /// `PathState::srtt()` hands back before the first sample. Zero constants
    /// — the cold price is another leg's measurement — and OFF is
    /// bit-identical (the cold price IS `p.srtt()` with the gate off).
    ///
    /// It binds only where a leg is cold WHILE another is warm, i.e. a LATE
    /// JOIN; it is measured INERT at every SF-bench geometry, where all legs
    /// start cold together. `scheduler::cold_place_active` carries the law,
    /// the retraction of the `c7x4` "lock-in" that motivated it, and the
    /// tests that bound both directions.
    pub cold_place: bool,

    // ── Generation stack ──────────────────────────────────────────────────
    /// `RWM_GEN` (default 384, min 1): generation size G.
    pub gen_size: usize,
    /// `RWM_PIPELINE` (default 2, min 1): legacy fixed pipeline depth M.
    pub pipeline: usize,
    /// `RWM_GEN_PIPE` (default = `unified`): derived pipeline depth M* +
    /// dynamic intake cap (ADR-0064 §16.20(d)); `=0` = fixed legacy M arm.
    ///
    /// The `RWM_FMTCP`(+`_WIN`) decode-on-total composite that used to sit
    /// beside this gate was REMOVED 2026-07-27 (register: RE-TESTED on the
    /// clean substrate by the "C8-Aware Pool Law" battery → CONFIRMED-REFUTED,
    /// c7/c8 ×0.11–0.20 of the default stack; ADR-0066). Its surviving ideas
    /// live on derived: per-path in-flight cap + M* depth here, honest
    /// anchors in ADR-0061, per-path admission in the percap family.
    pub gen_pipe: bool,
    /// `RWM_GEN_R` (unset = mode default 0.15 systematic / 0.20 coded-only;
    /// clamped [0, 2] at the use site): proactive overhead r.
    pub gen_r: Option<f64>,
    /// `RWM_GEN_RATE` (default 9000 sym/s): coded-emission pace ceiling.
    pub gen_rate: f64,
    /// `RWM_GEN_RATE_FLOOR` (default 2000, clamped [1, gen_rate]): bootstrap
    /// pacing floor before the ack-rate estimator has a sample.
    pub gen_rate_floor: f64,
    /// `RWM_GEN_INFLIGHT` (unset = 2·M·G): in-flight coded allowance W.
    pub gen_inflight: Option<f64>,
    /// `RWM_OOO_RETAIN` set at all (flag semantics): out-of-order retention
    /// decouple (Fix 3).
    pub ooo_retain: bool,
    /// `RWM_OOO_RETAIN` numeric value (≥ 2, default 16): retention depth in
    /// generations for the decouple.
    pub ooo_gens: usize,
    /// `RWM_WINDOW` (default 640, clamped [MAX_WINDOW_SIZE, 4096] at use):
    /// coded-only W_mp coding-window override (§16.5).
    pub window_override: Option<usize>,
    /// `RWM_REPORT_GENS` (unset = M*+1 under gen_pipe, else 6; clamped
    /// [1, 2000] at use): generations reported per deficit round.
    pub report_gens: Option<usize>,
    /// `RWM_REPAIR_WAIT` (ms; unset/0 = report immediately): repair-coverage
    /// horizon before a frontier hole may fire a reactive NACK.
    pub repair_wait_ms: Option<u64>,
    /// `RWM_CODED_SRC` (default OFF): clock the coded budget on the SENT
    /// frontier instead of the acked frontier (small-G wedge demonstrator).
    pub coded_src: bool,
    /// `RWM_NO_REACTIVE` (default OFF): pure-proactive demonstrator — the
    /// deficit-driven reactive loop disabled entirely.
    pub no_reactive: bool,
    /// `RWM_XPATH_REPAIR` (default OFF): route repair to the
    /// max-spare-capacity path (the C8 fungibility realization).
    pub xpath_repair: bool,
    /// `RWM_PROACTIVE_PACER` (default OFF): present-at-stall filling-repair
    /// pacer — the documented resolution of the removed frontier/inline
    /// family (presence⊥throughput evidence arm; ADR-0066).
    pub proactive_pacer: bool,
    /// `RWM_REASM_BDP` (default OFF): receiver reassembly clamp — never
    /// evict an undelivered above-frontier symbol.
    pub reasm_bdp: bool,
    /// `RWM_MIN_R` (default 0, clamped [0, 2]): per-symbol repair-rate floor
    /// (raise-r test instrument, not a shipped control law).
    pub min_r: f64,

    // ── CC / pacing ───────────────────────────────────────────────────────
    /// `RWM_CC_PACE` (default = `copa_wire_active()`): CC-rate pacing of the
    /// systematic source (paced wire assumption of the Copa model).
    pub cc_pace: bool,
    /// `RWM_CC_PACE_HR` (default 1.1, clamped [1, 2]): pace headroom.
    pub cc_pace_headroom: f64,
    /// `RWM_REACT_CAP` (unset = 1.0 under gen_pipe else OFF; <1 =
    /// fraction of SRTT, ≥1 = absolute µs): bounded-reactive spacing.
    pub react_cap: Option<f64>,
    /// `RWM_INFL_CAP` (default 0 = off): static total in-flight cap.
    pub infl_cap: u64,
    /// `RWM_INFL_BDP` (unset = 1.5 under gen_pipe else off): BDP-derived
    /// in-flight cap gain.
    pub infl_bdp: Option<f64>,
    /// `RWM_COPA_FEED` (default OFF): standalone plain-mode Copa delivery
    /// feed (also implied by `RWM_QUIC_CC=passthrough`) — ADR-0062.
    pub copa_feed: bool,
    /// `RWM_RS_ATTR` (default ON): flight-time witness for cross-path ack
    /// attribution in the sampling-only feed; `=0` = last-sent-path arm.
    pub rs_attr: bool,

    // ── Emission (goal-gate "Emission Batching", 2026-07-27) ──────────────
    /// `RWM_EMIT_BATCH` (default OFF — the A/B arm; the "Ship The Wins 1"
    /// composed flip measured c1 463–482 with est+pool-anchor but was
    /// REVERTED by the pre-set c7 clause, 2026-08-07): pacer-quantum
    /// emission batching on the plain window-reliable sender. Burst TUN
    /// intake (≤ `emit_burst` symbols per loop iteration, inside the
    /// flow-control store headroom and the pacing bucket) + per-burst
    /// taper/span-math refresh (per-symbol when OFF — bit-identical shipped
    /// path). Perf-only: ordering/pacing contracts and the delivered set
    /// unchanged. Single-live-path scope only; Realtime packing excluded
    /// (§16.28). Part of the documented fast single-path opt-in
    /// (`RWM_EMIT_BATCH=1 RWM_EST_CADENCE=1`: 446–508 Mbit/s at c1).
    pub emit_batch: bool,
    /// `RWM_EMIT_BURST` (default 64 symbols ≈ 64 KB payload — the BBR-style
    /// pacer quantum; clamped [2, 512]): emission burst quantum.
    ///
    /// SENDER-ONLY by measurement: the receiver-arm variants (engine-loop
    /// burst drain ± per-burst ack coalescing, gates `RWM_EMIT_BATCH_RECV`/
    /// `RWM_EMIT_ACK`) were built and REFUTED 2026-07-27 — any engine-
    /// receiver drain collapsed c1 227.6 → 136–144 Mbit/s (echo-RTT
    /// inflation → store-cap growth → spurious retx flood); removed, see
    /// goal-gate "Emission Batching".
    pub emit_burst: usize,

    // ── Recovery plane (ADR-0059) ─────────────────────────────────────────
    /// `RWM_RECOV_MP` (default ON): multipath recovery suppression — per-
    /// flight RFC 9002-style hole law on the flight path's smoothed clocks.
    pub recov_mp: bool,
    /// `RWM_RECOV_MP_LAW` (default ON under the umbrella): the per-flight
    /// hole-law sub-gate (trace attribution).
    pub recov_mp_law: bool,
    /// `RWM_RECOV_MP_LIVE` (default OFF — the A/B arm; goal-gate "C8
    /// Slow-Path Conversion"): the hole law's `mp_n_paths` + per-path
    /// clock snapshot read `live_paths()` instead of the
    /// saturation-filtered `active_paths()` (`available() > 0`), whose
    /// cwnd-full-path trap collapses the law to the N = 1 bypass (legacy
    /// age gate, cross-path clock) mid-transfer — the same filter trap
    /// already fixed at the Copa-sole store law and `capw_store_cap`, here
    /// at the recovery plane. Diagnosis signature: c8-pbs 412–749 of
    /// ~1.2–1.5k retransmits fired YOUNG vs their own flight-path law
    /// threshold (2026-08-06).
    pub recov_mp_live: bool,
    /// `RWM_STORE_CAP_UNIFIED` (default OFF — the A/B arm; goal-gate
    /// "Store-Cap Triplication", 2026-08-09): the plain (non-Copa-sole)
    /// dynamic store cap's Σ-anchor base and honest per-path cap sum read
    /// `live_paths()` instead of the saturation-filtered `active_paths()`
    /// (`available() > 0`). This is the SAME filter trap already fixed at
    /// the Copa-sole store law (`cwnd_sum`) and at `capw_store_cap`/
    /// `RWM_POOL_ANCHOR`, and armed at the recovery plane as
    /// `RWM_RECOV_MP_LIVE` — here at the law that is actually SHIPPED ON:
    /// `path_scaled_store_cap` multiplies its Σ-base by `n_live`, counted
    /// from `live_paths()`, while the base itself was summed over
    /// `active_paths()`, so a cwnd-saturated path is counted in the ×N and
    /// omitted from the Σ. A wire-bound sender is cwnd-saturated by
    /// definition; when the filter empties the set the cap falls to
    /// `store_boot_cap` (128). Population measured by the `sf=` gauge
    /// (`net::store_cap_sf_gauge`). `=0`/unset is the shipped default,
    /// bit-exactly.
    pub store_cap_unified: bool,
    /// `RWM_THREE_TERM` (default OFF — the A/B arm; goal-gate "Three-Term
    /// Law", 2026-08-10): the plain (non-Copa-sole) dynamic store cap is
    /// computed as the composed THREE-TERM law
    ///
    /// ```text
    ///   limit = Σ_i rate_i·K_i·RTprop_i            (network window)
    ///         + Σ_i rate_i·stall(δ, ρ, i)          (emission slack)
    ///         + 2·rate_fast·skew                   (resequencing span)
    /// ```
    ///
    /// — paper §16.43/§16.44, `net::three_term_store_cap`. Every term is
    /// Little's law over a signal the engine already measures and NONE of
    /// them contains a fitted coefficient. The point of the gate is the
    /// THIRD term: it is identically zero at a single path because
    /// `skew = (max RTprop − min RTprop)/2` over a one-element set is zero
    /// BY ARITHMETIC — which is how the `active_paths()` / `live_paths()`
    /// topology branch dies without an `if N == 1`. `=0`/unset is the
    /// shipped default, bit-exactly: the existing law chain runs verbatim.
    pub three_term: bool,
    /// `RWM_COMPOSED_CAP` (default OFF — the A/B arm; paper §16.56, ADR-0070
    /// Deliverable 2): THE COMPOSED CAP LAW, as ONE arm. The formula and
    /// every symbol's provenance are in the paper, written BEFORE this gate
    /// existed (CLAUDE.md FORMULA-FIRST):
    ///
    /// ```text
    ///   cap = Σᵢ over live_paths [ rateᵢ·RTpropᵢ + rateᵢ·stall(δ,ρ,srttᵢ) ]
    ///       + 2·rate_fast·skew
    /// ```
    ///
    /// **It IS [`crate::net::three_term_store_cap`]** on honest inputs — not
    /// a resemblance and not a second implementation, so there is nothing to
    /// drift. What this gate adds over `RWM_THREE_TERM` is the COMPOSITION
    /// ADR-0070 says has never been measured anywhere: the pool law, the
    /// unified live set at BOTH seats, and the late-stage per-path brake.
    /// Exactly three things, and no fourth:
    ///
    /// 1. **The pool law** in the plain dyn-cap chain — what `RWM_THREE_TERM`
    ///    already selects; this gate reaches the same seat.
    /// 2. **The unified live set.** The pool law already reads `live_paths()`
    ///    unconditionally, so the pool needs nothing. The BRAKE does — see 3.
    /// 3. **The late-stage per-path brake** (`cwnd_full`, ADR-0070 finding 7:
    ///    "the correct architecture, DISABLED WITHOUT A DECISION"), with its
    ///    per-path cap equal to **the path's OWN cwnd**. NO NEW CONSTANT: the
    ///    cap is the congestion controller's own window, which is what a
    ///    congestion brake ought to be made of. Neither `RWM_INFL_CAP`'s
    ///    static total nor `RWM_INFL_BDP`'s `gain·BDP` is used, and neither
    ///    changes meaning.
    ///
    /// **The trap that makes point 2 load-bearing** (§16.56, written down
    /// before it could be walked into): with the per-path cap set to the
    /// path's own cwnd, "path i is full" is `in_flightᵢ ≥ cwndᵢ`, i.e.
    /// exactly `available()ᵢ == 0` — and `active_paths()` is *active AND
    /// `available() > 0`*. A brake iterating `active_paths()` would ask a
    /// question whose answer is FALSE BY CONSTRUCTION on every tick, forever:
    /// it would resolve ON, cost a lock, and never brake. That is a null
    /// EFFECT wearing a null RESULT's clothes — §16.53's DIVERGED lesson. The
    /// composed brake reads `live_paths()`, so `cwnd_full` here means **every
    /// LIVE path is at or above its own congestion window**.
    ///
    /// **No ceiling of its own.** No `N·knee`, no swept pool, no arbitrary
    /// clamp: δ prices the queue as a latency budget (§16.47 measured the cap
    /// doing exactly that, 12/12). `WIN_STORE_MAX` survives beside the law as
    /// a MEMORY bound — a resource limit that may abort, never a term that
    /// shapes — and the one paroled constant, `store_cap_floor` = 64 (whose
    /// provenance ADR-0070 finding 5 records as ABSENT), stays. Both are
    /// NAMED with their bind fractions in the `[CCAP]` echo, per the
    /// FORMULA-FIRST clamp rule, so neither can bind silently again.
    ///
    /// `=0`/unset is the shipped default, bit-exactly.
    pub composed_cap: bool,
    /// `RWM_SUM_CAP` (**DEFAULT ON since 2026-08-19**, ladder battery rung N;
    /// `=0` = the displaced quadratic, kept re-runnable; paper §16.60/§16.64,
    /// ADR-0070 finding 2): **THE `×N` DELETION.** The pooled law's count
    /// multiplier is removed from the VALUE and kept in the CEILING:
    ///
    /// ```text
    ///   `=0` arm   cap = clamp( gain · N · Σᵢ(max_bwᵢ·min_rttᵢ), floor, N·knee )
    ///   SHIPPED    cap = clamp( gain     · Σᵢ(max_bwᵢ·min_rttᵢ), floor, N·knee )
    /// ```
    ///
    /// The quantity the law's own decl comment names — *"Σ per-path (BDP + one
    /// recovery round of runway)"* — is `Σᵢ(gain·anchorᵢ) = gain·Σ`, which is
    /// ALREADY linear in the path count because the Σ is. The shipped
    /// expression multiplies that already-summed base by the count a second
    /// time, making the value QUADRATIC in N where its own sentence is LINEAR.
    /// ADR-0070 finding 2 records the multiplier's provenance as **ABSENT** —
    /// not in the birth commit message, not in the doc comment, not at the
    /// decl site, not in the ledger — and contradicted by name in three places
    /// in this repository. This gate is the arm that finally ran the A/B that
    /// had never been run, and **it delivered**: goal-gate "Ladder Battery —
    /// RESULTS" measured the corrected law INTERIOR at both scoreable duals
    /// (`pin` 0.000, `eng` 1.000, `chg_frac` 1.000, no CAPBIND WARN) against a
    /// control reproducing the shipped 4096 pin, with goodput UP at the
    /// pre-registered risk cell (c8) on BOTH seeds. Hence the flip.
    ///
    /// **Exactly one factor changes.** Gain, floor, ceiling, Σ-set and
    /// estimator are untouched, and no constant is introduced — so `gain`'s
    /// fossil status (finding 3) and the knee's staleness (finding 4) are
    /// carried, identical on both arms, and cancel out of the comparison.
    /// [`crate::net::pooled_store_cap`] carries both forms in ONE expression
    /// with the multiplier as a VALUE, so there is no second implementation to
    /// drift from the paper.
    ///
    /// **It composes with [`Self::store_cap_unified`], and the four
    /// combinations are four distinct formulas** — U selects the Σ's path SET,
    /// this gate selects the count MULTIPLIER, and they are independent axes of
    /// the same law (asserted by `net::tests::law_shape`).
    ///
    /// **Reading a null.** At `N = 1` the law is not engaged at all
    /// (`n_live < 2` ⇒ `None`), so singles are byte-identical BY CONSTRUCTION.
    /// At `N ≥ 2` the correction is only VISIBLE where the value is interior:
    /// the `=0` form pins at `Σ ≥ knee/gain` (path-count-FREE, 1024), the
    /// shipped form at `Σ ≥ N·knee/gain` (1024 PER PATH). An arm whose
    /// `[SUMCAP]` echo reads a high `pin=` fraction measured the CLAMP, not the
    /// law, and MEASUREMENT DISCIPLINE 18 requires that be reported as the
    /// finding rather than filed as a null — which is why the echo carries
    /// `eng=`, `pin=` and `chg=` and not a mean.
    ///
    /// **The honest bound on the flip**, carried here because the
    /// recommendation carried it: the ladder's noise floor at c8 is wide
    /// (2σ = 27.07 Mbit/s against a 77–86 Mbit/s base, n = 21/24 at a bistable
    /// cell), so the session excludes a **large** c8 regression, not a small
    /// one. Carried with it: the c8 CAP-MAGNITUDE clause was FALSIFIED AS
    /// WRITTEN (`cap` 2308.7 vs a ±20 % band of [2416, 3624]) because the wire
    /// presented Σ = 1154.3, 23–28 % below both published anchors — a finding
    /// about Σ, not about the law (`cap ≡ ask`, `pin = 0`).
    ///
    /// `=0` is the DISPLACED QUADRATIC, kept fully re-runnable as the A/B arm
    /// with no deprecation warning (ADR-0066 register row); its shape stays
    /// pinned by `net::tests::law_shape::path_scaled_store_cap_value_is_quadratic_in_n_the_documented_defect`.
    pub sum_cap: bool,
    /// `RWM_LATE_BRAKE` (default OFF — the A/B arm; paper §16.60.1, ADR-0070
    /// finding 7): the late-stage per-path cwnd brake, **EXTRACTED** from
    /// [`Self::composed_cap`] so it can be armed WITHOUT the composed pool law
    /// that §16.57 refuted on magnitude.
    ///
    /// ```text
    ///   brake closes  ⟺  ∀ i ∈ live_paths() :  in_flightᵢ ≥ cwndᵢ
    /// ```
    ///
    /// Identical code path, identical per-path cap (**the path's OWN cwnd** —
    /// derived, never configured, always warm), identical set (`live_paths()`,
    /// because with `capᵢ = cwndᵢ` the predicate is exactly `available()ᵢ == 0`
    /// and an `active_paths()` brake would resolve ON and never close — the
    /// §16.53 DIVERGED lesson). **No constant appears in the predicate at all.**
    ///
    /// Why an extraction was needed: the brake arms on
    /// `eff_infl_cap > 0 || composed_cap`, and `composed_cap` also forces
    /// `three_term_on` — so the only two pre-existing ways to arm a brake give
    /// either the refuted composed pool law, or [`Self::infl_cap`]'s GLOBAL
    /// `Σ in_flight ≥ n` test against an operator-invented constant
    /// (`infl_percap` rides `gen_pipe`, which is off on the plain seat).
    /// Neither `RWM_INFL_CAP` nor `RWM_INFL_BDP` changes meaning here.
    ///
    /// `=0`/unset is the shipped default, bit-exactly: `cwnd_full` stays
    /// permanently false on the plain seat and the store cap remains the sole
    /// brake on outstanding (PIPELINE VERIFICATION MATRIX row 17).
    pub late_brake: bool,
    /// `RWM_RECOV_SP` (default OFF — the A/B arm; goal-gate "Lossy-Single
    /// Residual"): SINGLE-path per-flight time-threshold suppression — the
    /// RFC 9002 §6.1.2 hole law applied at N = 1 (time channel ONLY; the
    /// §6.1.1 packet channel is excluded at N = 1 by measurement: netem
    /// jitter reorders tens of packets deep on one path, far past
    /// kPacketThreshold). Measured without it (2026-07-27 diagnosis): the
    /// singles reactive plane fires ×4.4–5.7 the realized loss (sc2-100M:
    /// 3313 fired vs ~580 drops, 80% younger than the law's own threshold),
    /// costing ~2.7 Mbit at sc2 / ~1.7 at sc3 of pure wire waste.
    pub recov_sp: bool,
    /// `RWM_DERIVED_SWEEP` (default OFF — the A/B arm; goal-gate "The
    /// Derived Recovery Clamp"): both recovery clocks — the sender's tail
    /// sweep and the receiver's stalled-hole refresh — read
    /// `net::derived_recovery_round_us` (2·SRTT floored by the DERIVED
    /// patience floor, NO ceiling) instead of `2·SRTT` clamped to the
    /// undocumented [25 ms, 100 ms]. OFF ⇒ both sites byte-identical to the
    /// shipped law. Zero new constants: the `2` and the floor are already in
    /// the tree. See `net/mod.rs`'s block comment for the provenance of the
    /// two literals this replaces and why neither of the ceiling's stated
    /// referents (the EVICT reorder hold; an inner-TCP RTO) exists on the
    /// measured stack.
    pub derived_sweep: bool,
    /// `RWM_DELTA_CAP` (**DEFAULT ON since 2026-08-19**, candidates battery
    /// rung D; `=0` = the displaced `gain = 2.0` fossil, kept re-runnable;
    /// paper §16.67/§16.70/§16.71, ADR-0071 family 2): the pooled outstanding
    /// cap's VALUE multiplier is the δ-priced, CoDel-DERIVED standing-queue
    /// setpoint instead of the shipped `gain = 2.0` fossil.
    ///
    /// ```text
    ///   SHIPPED   cap  = clamp( (1 + q(δ)) · Σᵢ(bwᵢ·RTpropᵢ),  floor,  N·knee )
    ///   `=0` arm  cap  = clamp(  gain      · Σᵢ(bwᵢ·RTpropᵢ),  floor,  N·knee )
    ///   q(δ) = 0.05 + 0.05·(clamp(b(δ), ½, 2) − ½)/(2 − ½)   ==  (b+1)/30
    /// ```
    ///
    /// **THE SHIPPED FORMULA AFTER BOTH 2026-08-19 FLIPS**, stated whole
    /// because the two gates compose and neither record is readable alone —
    /// [`Self::sum_cap`] (ON, §16.64) deleted the COUNT multiplier and this
    /// gate (ON, §16.71) replaced the VALUE multiplier, so the pooled law the
    /// engine computes unset is
    ///
    /// ```text
    ///   cap = clamp( (1 + q(δ)) · Σᵢ(bwᵢ · RTpropᵢ),  floor,  N · knee )
    /// ```
    ///
    /// with the `N·knee` knee **measured INERT at both scoreable duals**
    /// (`pin` = 0.0000 at c7 and c8) rather than assumed inert. `gain = 2.0`
    /// no longer appears in the shipped VALUE at all: it survives only on the
    /// `=0` arm and at the other cap seats.
    ///
    /// RFC 8289 (CoDel) §3.2 DERIVES the permitted standing queue from
    /// Kleinrock power maximisation and states it as *"between 5% and 10% of
    /// the TCP connection's RTT"*. This gate maps the δ dial CONTINUOUSLY onto
    /// that band — Realtime 5.00 %, Auto 6.67 %, Bulk 10.00 % — with both band
    /// endpoints cited and both dial endpoints READ from `net::delta_budget_b`,
    /// so the map has no free parameter. No threshold, no hint test, no second
    /// code path: it substitutes ONE FACTOR in one expression
    /// (`net::pool_value_multiplier`), exactly as [`Self::sum_cap`] substitutes
    /// the count multiplier, and the two are INDEPENDENT AXES of one law.
    ///
    /// **The design decision it embodies**: the δ dial's own points permit
    /// 50/100/200 % of an RTprop, 10–40× the derived band, so the derived law
    /// COMPRESSES the dial's authority 20× at Bulk. §16.67 states the
    /// justification (CoDel's power function falls monotonically past
    /// `f ≈ 0.1`; §16.57 measured 43–48 % worse latency at goodput parity for
    /// 2.4× the queue) and states that the user may reject it.
    ///
    /// As `q → 0` the law reduces to `Σᵢ bwᵢ·RTpropᵢ`, which IS ADR-0071
    /// candidate (d) ZERO. Bit-identical at N = 1 BY CONSTRUCTION (the pooled
    /// seat returns `None` at `n_live < 2` before any multiplier is read).
    /// Engagement, both clamp bind fractions and the counterfactual against
    /// `gain` are reported by the `[DCAP]` echo.
    ///
    /// **Measured on the wire before it shipped** (goal-gate "Candidates
    /// Battery — RESULTS", 2026-08-19, rung D): **D-LAT six of six** — goodput
    /// PARITY at every dual on both seeds (no reading outside 2σ_pooled in
    /// either direction) with `q_p50` strictly down at every one, by 10–16 ms
    /// at c7, 113–117 ms at c8 and 130–200 ms at c8L; INTERIOR with the
    /// ceiling provably inert at c7 and c8 (`pin` = 0.0000, `eng` = `chg` =
    /// 1.00, cap inside its pre-registered ±20 % band at both); `eng = 0/0` at
    /// c1 and sc2, the N = 1 identity confirmed on the wire; and c8's paired
    /// dead wall SHORTENED (18 of 23 non-zero pairs favour the arm, sign test
    /// p ≈ 0.011 — B-WALL resolving for the first time in this tree).
    ///
    /// **The honest bounds on the flip**, carried here because the
    /// recommendation carried them:
    ///
    /// * **Goodput is PARITY, not a win** — the honest claim is *"free"*, not
    ///   *"faster"*. Worst readings −2.64 Mbit/s against 2σ 38.09 (c8L s42),
    ///   −1.28 against 4.28 (c7 s7); best +7.83 against 11.13 (c8 s42).
    /// * **c8L is a PARTIAL delivery, not a verdict.** `pin` = 0.23 there
    ///   falls in the gap BETWEEN the contract's two pre-declared branches
    ///   (`≤ 0.10` primary era, `> 0.50` secondary era) and neither branch is
    ///   claimed after the fact. §16.67's "interior EVERYWHERE incl. c8L" is
    ///   therefore NOT delivered; the named instrument is the WITHIN-RUN Σ
    ///   series, which needs no VM arm.
    /// * **The probe is not unanimous**: `ping_p50` agrees with `q_p50` on
    ///   five of six rows and disagrees in SIGN on one (c8 s42, +20.5 ms). The
    ///   claim rests on `q_p50`, the sender-side measurand this law governs.
    /// * **The c8/seed-7 abort class is ARM-CORRELATED** (20 % on the control
    ///   against 75 % on the RACK arm), so excluding aborts from denominators
    ///   there is a selection on the treatment and the surviving c8 seed-7
    ///   reps are a biased sample of unknown direction. Scoped to c8 seed 7:
    ///   seed 42 is abort-free at every cell and no headline verdict rests on
    ///   c8 seed 7 alone. The owed instrument is an abort-cause witness.
    /// * **No support at single-path cells**, and none is claimed: the law
    ///   cannot regress a single-path deployment and cannot help one either.
    ///
    /// `=0` is the DISPLACED `gain = 2.0` FOSSIL, kept fully re-runnable as
    /// the A/B arm with no deprecation warning (ADR-0066 register row); the
    /// substitution's shape stays pinned two-sidedly by
    /// `formula_agreement::the_delta_cap_substitutes_one_factor_and_reduces_to_candidate_d`.
    pub delta_cap: bool,
    /// `RWM_RACK_CLOCKS` (default OFF — the A/B arm; paper §16.68): both
    /// recovery clocks read RFC 8985 §6.2 Step 4's reordering window,
    /// transplanted VERBATIM, instead of `2·SRTT` clamped to [25 ms, 100 ms].
    ///
    /// ```text
    ///   round = max( min( mult · min_rtt / 4,  srtt ),  TIMER_GRANULARITY_US )
    /// ```
    ///
    /// **Every constant is RACK's own** (`/4`; the `SRTT` ceiling's *"MUST be
    /// bounded … SHOULD be SRTT"*; `mult ∈ [1, 17]`), and the floor is the
    /// tree's existing kGranularity analogue. **REPLACES**
    /// [`Self::derived_sweep`] when both are set — the two are rival laws for
    /// one quantity, not composable axes. With no min-RTT sample the armed
    /// fallback runs verbatim (information availability, not a mode).
    ///
    /// §16.68 records what writing this down established: RFC 8985 publishes
    /// **no RTT-relative ceiling for a re-probe cadence** (§7.2's PTO is
    /// bounded only by `TCP_RTO_expiration()`, a 1-second-minimum absolute),
    /// so the cross-check's Tier-2 item 2.1 asked for a construction its own
    /// source does not contain. Component-verified at recovery_bench: at the
    /// shipped `mult = 1` this law is 8–46× TIGHTER than the clamp it replaces
    /// and its `SRTT` ceiling is UNREACHABLE within RACK's own `mult ≤ 17` at
    /// four of five cells. `=0`/unset is byte-identical at both sites.
    pub rack_clocks: bool,
    /// `RWM_RACK_REO_MULT` (default **1** — RFC 8985 §6.2 Step 4's own initial
    /// `RACK.reo_wnd_mult`; clamped to RACK's own `[1, 17]`).
    ///
    /// RACK advances this on **DSACK-detected spurious recoveries**, and this
    /// transport has no DSACK and no spurious-recovery detector — so the
    /// adaptive half of RFC 8985 §6.2 Step 4 is STRUCTURALLY INERT here and the
    /// law's `SRTT` ceiling can never bind at `mult = 1`
    /// (`min_rtt ≤ srtt ⇒ min_rtt/4 < srtt` identically). A bound that provably
    /// never binds turns its law into a constant and hides its shape from every
    /// measurement taken through it, which is the defect CLAUDE.md's
    /// bind-fraction rule exists to catch. This knob therefore exists so a
    /// battery can drive the cited parameter over its CITED RANGE and make the
    /// bound REACHABLE (gauge reachability); exposing a published parameter is
    /// not inventing a constant, and leaving the ceiling unreachable would be
    /// the defect. Read only when [`Self::rack_clocks`] is on.
    pub rack_reo_mult: u64,
    /// `RWM_QUANTILE_CLOCKS` (default OFF — the A/B arm; paper §16.69): both
    /// recovery clocks read the DERIVED quantile round
    /// `W(α) = srtt + √((1−α)/α)·σ` — Cantelli's distribution-free
    /// one-sided bound at the false-alarm rate α the CONTRACT declares on the
    /// r leg (`target_tail_loss × ζ(hint)`, continuous in the dial).
    ///
    /// **Zero fitted coefficients, and REFUTED-WITH-RECORD.** §16.69 records
    /// three independent reasons it does not close on this stack — the bound
    /// needs k = 316 at the contract's own `α = 1e-5`; the empirical route
    /// needs ~1e5 samples the Copa min-deque discards by construction; and
    /// pricing α off `target_tail_loss` equates P(symbol never delivered)
    /// with P(retransmit wasted), whose repair needs a cost ratio that exists
    /// nowhere in this repository. Shipped OFF so the refutation is
    /// REPRODUCIBLE rather than asserted. OUTRANKS [`Self::rack_clocks`] and
    /// [`Self::derived_sweep`] when set — rival laws for one quantity.
    pub quantile_clocks: bool,
    /// `RWM_ALPHA_OVERRIDE` (**ABSENT by default**) — the EXPERIMENT knob that
    /// sets the quantile law's α **directly**, replacing the contract's
    /// `target_tail_loss × ζ(hint)` at the seat [`crate::net::contract_alpha`]
    /// occupies. Read ONLY when [`Self::quantile_clocks`] is armed; on the
    /// default arm it is inert and the engine is byte-identical without it.
    ///
    /// **Why it exists, and why it is not a law.** §16.69 refuted the quantile
    /// clock three ways. Two of the three are consequences of α = 1e-5 alone,
    /// and the third is a CATEGORY ERROR in *what feeds α* — not in the
    /// Cantelli construction `W(α) = srtt + √((1−α)/α)·σ`, which was never the
    /// defective part (`docs/research/cost-ratio-memo.md`, §5 step 3: *"the
    /// change is what feeds α, not the Cantelli construction"*). The cost-ratio
    /// memo lays out four candidate mappings and **recommends none**; each
    /// picks a different α at the same cell. This knob makes α the ONE free
    /// variable of a sweep so the cost curve can be measured before any mapping
    /// is written into a law. **It is not a mapping, it is not continuous in
    /// any dial, and nothing may ship reading it** — a shipped law must derive
    /// α from the (δ, ρ, r) triangle, which is the decision the sweep informs.
    ///
    /// **ABSENT, not defaulted, and garbage resolves back to ABSENT VISIBLY.**
    /// Unset, empty, unparseable, non-finite, or outside the open-closed range
    /// `(0, 1]` on which `k(α) = √((1−α)/α)` is finite and non-negative ⇒
    /// `None` ⇒ the contract's own α, exactly as before. The `[GATES]` echo
    /// prints the RESOLVED value (`unset` or the number), so "my override did
    /// not take" is READ off the run's own output rather than inferred — the
    /// `RWM_ACKDIAG_WINDOW_US` precedent, whose echo is its resolved µs and not
    /// a flag.
    pub alpha_override: Option<f64>,
    /// `RWM_HOLDDOWN_Q` (**ABSENT by default**; paper §16.77) — the EXPERIMENT
    /// knob that sets the level `q` of the SENDER'S HOLD-DOWN on a reported
    /// hole: the sender does not answer a receiver gap report with a repair
    /// until the hole has been outstanding for at least `T(q) = W_q(1 − q)`,
    /// §16.76's order statistic evaluated on the hole-resolution stream.
    ///
    /// **Why it exists.** The fire-cause pass counted **0.59 % of 107 597
    /// classified recovery fires from a timer and 98.99 % from the sender
    /// answering a gap report**. Every clock this tree has written — the
    /// shipped `[25, 100] ms` clamp, [`Self::derived_sweep`],
    /// [`Self::rack_clocks`], [`Self::quantile_clocks`] — sets the TIMER, and
    /// `fa ⊥ W` is the measured consequence. This is the first knob pointed at
    /// the other 99 %. It is NOT a rival law for the timer's quantity and it
    /// does not sit in that precedence chain: it is a different decision, at a
    /// different site, and it composes with every one of them.
    ///
    /// **ABSENT, not defaulted, and garbage resolves back to ABSENT VISIBLY.**
    /// Unset, empty, unparseable, non-finite, or outside the OPEN interval
    /// `(0, 1)` ⇒ `None` ⇒ no hold-down at all, which is today's machine
    /// byte-identically. The domain is the law's own and not a taste: at
    /// `q ≤ 0` the hold-down is zero — the shipped behaviour, expressed by the
    /// gate being absent rather than by an armed arm — and at `q ≥ 1` the
    /// window law `N = ⌈K/(1−q)⌉` diverges (§16.77.10). The `[GATES]` echo
    /// prints the RESOLVED value, so "my arm did not take" is READ.
    ///
    /// **Nothing may ship reading it.** A shipped hold-down must derive `q`
    /// from the (δ, ρ, r) triangle through §16.77.2's stationarity condition,
    /// continuous in every dial; that is the decision this arm informs and
    /// does not take.
    pub holddown_q: Option<f64>,
    /// `RWM_W_FORM` (**`cantelli` by default**; paper §16.76) — WHICH of the
    /// two rival `W` laws the armed quantile clock evaluates. Read ONLY when
    /// [`Self::quantile_clocks`] is armed; on the default arm it is inert and
    /// the engine is byte-identical without it.
    ///
    /// ```text
    ///   cantelli   W(α)   = srtt + √((1−α)/α)·σ           §16.69  (default)
    ///   quantile   W_q(α) = X_(N(α)−K+1),  N = ⌈K/α⌉      §16.76
    /// ```
    ///
    /// **Why it exists.** §16.74.5 made `σ̂` a precondition of the whole
    /// `mean + k(α)·σ̂` family, and two batteries failed to find an estimator
    /// meeting it. The τ-lag battery's clause `B` then measured why the search
    /// was misdirected: the shipped estimator supplies a **conditional**
    /// spread at 3–5 % of the **marginal** dispersion the Cantelli form
    /// requires (20–300× at seven of eight sender legs), and the marginal
    /// quantity is itself regime-dominated — one rep in eighty moved pooled
    /// `R_total` by 33×. **`quantile` removes the `σ̂` term rather than
    /// estimating it**, which is available on `[0.002, 0.40]` for the reason
    /// §16.69's own reason 2 gives and unavailable at the contract's own α for
    /// the same reason — where it says so on its echo instead of
    /// extrapolating.
    ///
    /// **It is an A/B EXPERIMENT ARM and it is not a mode switch.** The two
    /// values are RIVAL LAWS FOR ONE QUANTITY, in the same precedence chain
    /// `quantile / rack / derived` already occupies; nothing keys on a
    /// threshold in the (δ, ρ, r) triangle, and **nothing may ship reading
    /// it** — the same rule [`Self::alpha_override`] carries.
    ///
    /// **Garbage resolves back to ABSENT VISIBLY.** Unset, empty or
    /// unparseable ⇒ `cantelli` ⇒ today's behaviour, and the `[GATES]` echo
    /// prints the RESOLVED token so *"my arm did not take"* is READ off the
    /// run's own output rather than inferred — the `RWM_ALPHA_OVERRIDE`
    /// precedent, and the failure mode that produced the 31 Mbit/s anomaly.
    pub w_form: crate::net::WForm,

    // NOTE: `RWM_SCHED_SNAPSHOT` (the net-seam-pass-2 per-iteration scheduler
    // snapshot) lived here and was DELETED unmeasured on 2026-08-10 — its
    // stated hazard was not reachable from the sites it served. ADR-0066
    // deprecation register; goal-gate "Scheduler-Snapshot Adjudication".

    // ── Instruments (ADR-0052; no behavior) ───────────────────────────────
    /// `RWM_DIAG` (default OFF): the transport-ceiling / recovery-plane DIAG.
    pub diag: bool,
    /// `RWM_ACKDIAG` (default OFF): the ACK-CADENCE GAUGE — the sender-side
    /// instrument for PIPELINE VERIFICATION MATRIX row 21, whose STREAM SHAPE
    /// had "no instrument of any kind, anywhere". Per path and per ~2 s
    /// window it prints the WindowAck inter-arrival distribution, the
    /// per-ack `d_received` distribution and zero-delta fraction, the
    /// REALIZED rate-sampler over-read x (per accepted `record_delivery`
    /// sample, normalized by the window's own long-run delivered rate — the
    /// number three separate benches had to INVENT, landing ×24–2400 against
    /// the wire's ×4.6–7.4), and the repair-counting reconciliation
    /// (`symbols_sent` vs Σ`d_received` vs Σ`d_expected`). Observation only:
    /// the gauge owns all its state and no engine decision can reach it
    /// (`net::ackdiag::tests::ackdiag_is_observation_only`). Zero cost off —
    /// the process-global is a `OnceLock<Option<…>>` that resolves to `None`,
    /// so every feed site is a null check. Independent of `RWM_DIAG` on
    /// purpose: it must be runnable on an arm that is not paying for the
    /// 250 ms `[DIAG]` report.
    pub ackdiag: bool,
    /// `RWM_RTT_DUMP` (default OFF): the RAW RTT SAMPLE DUMP — the instrument
    /// that makes the estimator battery's clause `B` EXACT.
    ///
    /// The scored battery (goal-gate, "THE SIGMA ESTIMATOR — THE SCORED
    /// RESULT" §7) rejected three of four candidates on `B` and then recorded
    /// that `B`'s own reference was the part of the bar most in need of
    /// scrutiny: a uniform 30–90× gap across ALL FOUR gauges is *"not four
    /// independent biases; it is one property of the COMPARISON"* — a 20 Hz
    /// ICMP probe riding the whole shaped path against a kHz sender's estimate
    /// of its own ack path. `B` was written REJECT-only for that reason and
    /// was UNSCOREABLE for `msd_us` entirely.
    ///
    /// This gauge emits the sender's raw RTT sample stream, so each candidate
    /// is scored against **the same functional computed offline over the
    /// identical samples**. The reference becomes exact and like-for-like by
    /// construction, and `B` can ACQUIT rather than only convict.
    ///
    /// **It ships OFF and its pass is SEPARATE from the scored battery on
    /// purpose**: at a kHz leg it writes megabytes of stderr, which is a
    /// sender-side cost, and sender-side dispersion is exactly what clause `S`
    /// measures. Running it on the scored invocations would perturb the
    /// measurement it exists to explain.
    ///
    /// Observation only: the gauge owns all its state and no engine decision
    /// can reach it. Zero cost off — the process-global is a
    /// `OnceLock<Option<…>>` that resolves to `None`, so the feed site is a
    /// null check.
    pub rtt_dump: bool,
    /// `RWM_SUCC_DUMP` (default OFF): the RAW SUCCESSOR-ARRIVAL SAMPLE DUMP,
    /// beside the always-on `[SUCC]` quantile line.
    ///
    /// The fire-cause pass named the successor measurand —
    /// `P(the next in-flight symbol arrives by t | a hole is outstanding)` —
    /// and recorded that it *"has never been measured on this engine"*, and
    /// that *"a derivation written against an uncharacterized distribution
    /// would repeat the exact defect just corrected."* `[SUCC]` characterizes
    /// it. Its quantile line is emitted on EVERY arm, ungated by this knob, so
    /// no pass depends on the dump being on.
    ///
    /// This gate adds the RAW `(outcome, µs)` records, so the derivation that
    /// follows can compute any functional over the exact samples rather than
    /// over the gauge's log buckets — the `[RTTDUMP]` lesson (a clause scored
    /// against a summary statistic could neither acquit nor be re-derived),
    /// applied BEFORE the battery rather than after it.
    ///
    /// It ships OFF for `[RTTDUMP]`'s reason: at a lossy cell it writes
    /// megabytes of stderr at the RECEIVER, and receiver-side cost is
    /// goodput-visible. The quantile line costs a fixed ~12 kB of buckets and
    /// is what the scored pass reads.
    ///
    /// Observation only: the gauge holds no engine handle and nothing in the
    /// engine branches on any value it computes
    /// (`net::succ::tests::succ_is_observation_only`).
    pub succ_dump: bool,
    /// `RWM_WALLDIAG` (default OFF): the DEAD-WALL ONSET/DURATION instrument
    /// — the statistic-stability prerequisite recorded at the close of the
    /// mode-hunt work (#93) and made step 2 of ADR-0070's validation path.
    ///
    /// The statistic it replaces was a per-rep FLAG over two tick-share
    /// medians (`wait_tun` = 0 % ∧ `wait_paused` = 0 %) and it proved
    /// UNSTABLE — arm orderings INVERTED between pools collected minutes
    /// apart. A tick-share is a fraction of sender-loop WAKEUPS, whose rate
    /// is an output of the mechanism under test, and a conjunction of two
    /// whole-run medians cannot tell one long terminal wall from a hundred
    /// scattered micro-gaps. This gate measures the wall's ONSET (as a
    /// fraction of the transfer wall) and DURATION (ms) instead, plus the
    /// retransmit count inside it — per RUN, one `[WALL]` line at teardown.
    /// See `net/walldiag.rs` for the measurand, stated before the code.
    ///
    /// Observation only: the gauge owns all its state and takes NO engine
    /// handle at all (`net::walldiag::tests::walldiag_is_observation_only`).
    /// Zero cost off — the process-global is a `OnceLock<Option<…>>` that
    /// resolves to `None`, so the single feed site is a null check.
    /// Independent of `RWM_DIAG` on purpose, exactly as `RWM_ACKDIAG` is:
    /// the c8 arms whose statistic this stabilises are the arms that cannot
    /// afford the 250 ms `[DIAG]` report.
    pub walldiag: bool,
    /// `RWM_CPUPROF` (default OFF): the SENDER CPU DECOMPOSITION — the
    /// instrument the c9 scored battery's sender ceiling has no successor
    /// without. That battery measured the client saturating at 68.5 ms/MB of
    /// CPU and predicted its own goodput from it to within 1 % (1.51 cores /
    /// 68.5 ms/MB = 176.3 Mbit/s against 176.4 measured), and then could not
    /// say where the 68.5 ms/MB GOES. This gate attributes it to five named
    /// seams — GF coding, source admission, framing, wire serialization, and
    /// the datagram handoff — and reports the UNATTRIBUTED remainder as a
    /// first-class column rather than as an error term, because the remainder
    /// is where quinn's driver, its `sendmsg`, and rustls/ring's AEAD packet
    /// protection all live and none of them is reachable from the sender task.
    ///
    /// One `[CPUPROF]` line per run, at sender teardown. See
    /// `net/cpuprof.rs` for the measurand, stated before the code, and for
    /// the three honesty clauses (wall seams against a CPU denominator; the
    /// gauge's span is not the process's; the denominator is PROCESS CPU
    /// because tokio may migrate the sender task).
    ///
    /// Observation only: the gauge owns all its state and its whole input is
    /// a seam index and a duration
    /// (`net::cpuprof::tests::cpuprof_is_observation_only`). Zero cost off —
    /// the process-global is a `OnceLock<Option<…>>` that resolves to `None`,
    /// so every seam is a null check around a direct call. Independent of
    /// `RWM_DIAG` on purpose, exactly as `RWM_WALLDIAG` and `RWM_ACKDIAG`
    /// are: the cell whose ceiling this takes apart is sender-CPU-bound, and
    /// adding the 250 ms `[DIAG]` report to the arm under measurement would
    /// change the quantity being measured.
    pub cpuprof: bool,
    /// `RWM_RDIAG` (default OFF): engine-receiver saturation probe.
    pub rdiag: bool,
    /// `RWM_FDIAG` (default OFF): proactive-frontier diagnosis instrument
    /// (retained after the frontier mechanism's removal — ADR-0066).
    pub fdiag: bool,
    /// `RWM_TRACE` (default OFF): generation-lifecycle trace prints.
    pub trace: bool,
    /// `RWM_PFRAC` (default OFF): proactive-vs-reactive recovery fraction.
    pub pfrac: bool,
}

impl RuntimeGates {
    /// Read the whole gate surface from the environment — call once at engine
    /// start.
    pub fn resolve() -> Self {
        let unified = crate::net::unified_active();
        let gen_rate: f64 = env_parse::<f64>("RWM_GEN_RATE").unwrap_or(9000.0);
        RuntimeGates {
            unified,
            unified_shed: env_flag("RWM_UNIFIED_SHED", true),
            taper_r: env_flag("RWM_TAPER_R", unified),
            astar_anchor: anchor_gate_default("RWM_ASTAR_ANCHOR", true),
            mstar_anchor: anchor_gate_default("RWM_MSTAR_ANCHOR", true),
            plain_rs: anchor_gate("RWM_PLAIN_RS"),
            honest_anchor: crate::scheduler::honest_anchor_active(),
            honest_k: crate::scheduler::honest_k_active(),
            store_sack_release: env_flag("RWM_STORE_SACK_RELEASE", true),
            store_paths: env_flag("RWM_STORE_PATHS", true),
            store_path_pool: env_parse::<usize>("RWM_STORE_PATH_POOL").unwrap_or(2048),
            store_override: env_parse::<usize>("RWM_STORE"),
            store_env_set: std::env::var("RWM_STORE").is_ok(),
            store_gain: env_parse::<f64>("RWM_STORE_GAIN")
                .unwrap_or(2.0)
                .clamp(1.0, 64.0),
            store_boot: env_parse::<usize>("RWM_STORE_BOOT").unwrap_or(128),
            store_capw: env_flag("RWM_STORE_CAPW", false),
            store_percap: env_flag("RWM_STORE_PERCAP", false),
            percap_guard: env_flag("RWM_PERCAP_GUARD", true),
            store_borrow: env_flag("RWM_STORE_BORROW", false),
            honest_cap: env_flag("RWM_HONEST_CAP", true),
            pool_anchor: crate::scheduler::pool_anchor_active(),
            pool_deliv: crate::scheduler::pool_deliv_active(),
            floor_bound: crate::scheduler::floor_bound_active(),
            ack_merge: crate::scheduler::ack_merge_active(),
            loss_sent_truth: crate::scheduler::loss_sent_truth_active(),
            release_1to1: crate::scheduler::release_1to1_active(),
            charge_recovery: crate::scheduler::charge_recovery_active(),
            patience_derived: crate::scheduler::patience_derived_active(),
            sidle_derived: crate::scheduler::sidle_derived_active(),
            win_decouple: env_flag("RWM_WIN_DECOUPLE", false),
            place_slack: env_flag("RWM_PLACE_SLACK", false),
            cold_place: crate::scheduler::cold_place_active(),
            gen_size: env_parse::<usize>("RWM_GEN").unwrap_or(384).max(1),
            pipeline: env_parse::<usize>("RWM_PIPELINE").unwrap_or(2).max(1),
            gen_pipe: env_flag("RWM_GEN_PIPE", unified),
            gen_r: env_parse::<f64>("RWM_GEN_R"),
            gen_rate,
            gen_rate_floor: env_parse::<f64>("RWM_GEN_RATE_FLOOR")
                .unwrap_or(2000.0)
                .clamp(1.0, gen_rate),
            gen_inflight: env_parse::<f64>("RWM_GEN_INFLIGHT"),
            ooo_retain: env_flag("RWM_OOO_RETAIN", false),
            ooo_gens: env_parse::<usize>("RWM_OOO_RETAIN")
                .filter(|&n| n >= 2)
                .unwrap_or(16),
            window_override: env_parse::<usize>("RWM_WINDOW"),
            report_gens: env_parse::<usize>("RWM_REPORT_GENS"),
            repair_wait_ms: env_parse::<u64>("RWM_REPAIR_WAIT"),
            coded_src: env_flag("RWM_CODED_SRC", false),
            no_reactive: env_flag("RWM_NO_REACTIVE", false),
            xpath_repair: env_flag("RWM_XPATH_REPAIR", false),
            proactive_pacer: env_flag("RWM_PROACTIVE_PACER", false),
            reasm_bdp: env_flag("RWM_REASM_BDP", false),
            min_r: env_parse::<f64>("RWM_MIN_R").unwrap_or(0.0).clamp(0.0, 2.0),
            cc_pace: env_flag("RWM_CC_PACE", crate::scheduler::copa_wire_active()),
            cc_pace_headroom: env_parse::<f64>("RWM_CC_PACE_HR")
                .unwrap_or(1.1)
                .clamp(1.0, 2.0),
            react_cap: env_parse::<f64>("RWM_REACT_CAP"),
            infl_cap: env_parse::<u64>("RWM_INFL_CAP").unwrap_or(0),
            infl_bdp: env_parse::<f64>("RWM_INFL_BDP"),
            copa_feed: env_flag("RWM_COPA_FEED", false),
            rs_attr: env_flag("RWM_RS_ATTR", true),
            emit_batch: env_flag("RWM_EMIT_BATCH", false),
            emit_burst: env_parse::<usize>("RWM_EMIT_BURST")
                .unwrap_or(64)
                .clamp(2, 512),
            recov_mp: env_flag("RWM_RECOV_MP", true),
            recov_mp_law: env_flag("RWM_RECOV_MP_LAW", true),
            recov_mp_live: env_flag("RWM_RECOV_MP_LIVE", false),
            store_cap_unified: env_flag("RWM_STORE_CAP_UNIFIED", false),
            three_term: env_flag("RWM_THREE_TERM", false),
            composed_cap: env_flag("RWM_COMPOSED_CAP", false),
            // DEFAULT ON since 2026-08-19 — the ladder battery's one
            // FLIP-RECOMMENDED gate (goal-gate "Ladder Battery — RESULTS",
            // paper §16.64). `RWM_SUM_CAP=0` remains the re-runnable A/B arm:
            // the shipped quadratic `gain·N·Σ`, kept with its provenance per
            // the deprecation register, no deprecation warning.
            sum_cap: env_flag("RWM_SUM_CAP", true),
            late_brake: env_flag("RWM_LATE_BRAKE", false),
            recov_sp: env_flag("RWM_RECOV_SP", false),
            derived_sweep: env_flag("RWM_DERIVED_SWEEP", false),
            // DEFAULT ON since 2026-08-19 — the candidates battery's one
            // FLIP-RECOMMENDED gate (goal-gate "Candidates Battery — RESULTS",
            // paper §16.71). Plain `env_flag`, not `anchor_gate_default`: this
            // is a store-cap gate with no umbrella family, exactly as
            // `RWM_SUM_CAP` above, so there is no umbrella semantic to
            // preserve. `RWM_DELTA_CAP=0` remains the re-runnable A/B arm: the
            // shipped `gain = 2.0` fossil, kept with its provenance per the
            // deprecation register, no deprecation warning.
            delta_cap: env_flag("RWM_DELTA_CAP", true),
            rack_clocks: env_flag("RWM_RACK_CLOCKS", false),
            quantile_clocks: env_flag("RWM_QUANTILE_CLOCKS", false),
            // ABSENT by default; garbage resolves back to ABSENT and the echo
            // prints `unset`, so a mistyped arm is READ rather than inferred.
            // The range is the law's own domain, not a taste: `k(α)` is
            // undefined at α ≤ 0 and negative-radicand above 1.
            alpha_override: env_parse::<f64>("RWM_ALPHA_OVERRIDE")
                .filter(|a| a.is_finite() && *a > 0.0 && *a <= 1.0),
            // ABSENT by default; garbage resolves back to ABSENT and the echo
            // prints `unset`. The range is the law's own domain, not a taste:
            // `q ≤ 0` IS the shipped machine (expressed by absence) and the
            // window law diverges at `q ≥ 1`. Paper §16.77.
            holddown_q: env_parse::<f64>("RWM_HOLDDOWN_Q")
                .filter(|q| q.is_finite() && *q > 0.0 && *q < 1.0),
            // ABSENT by default; garbage resolves back to `cantelli` — today's
            // law — and the echo prints the RESOLVED token, so a mistyped arm
            // is READ rather than inferred. Paper §16.76.
            w_form: std::env::var("RWM_W_FORM")
                .ok()
                .and_then(|v| crate::net::WForm::parse(&v))
                .unwrap_or_default(),
            // RFC 8985 §6.2 Step 4's own initial value, over RACK's own range.
            rack_reo_mult: env_parse::<u64>("RWM_RACK_REO_MULT")
                .unwrap_or(crate::net::RACK_REO_WND_MULT_INIT)
                .clamp(crate::net::RACK_REO_WND_MULT_INIT, crate::net::RACK_REO_WND_MULT_MAX),
            diag: env_flag("RWM_DIAG", false),
            ackdiag: env_flag("RWM_ACKDIAG", false),
            rtt_dump: env_flag("RWM_RTT_DUMP", false),
            succ_dump: env_flag("RWM_SUCC_DUMP", false),
            walldiag: env_flag("RWM_WALLDIAG", false),
            cpuprof: env_flag("RWM_CPUPROF", false),
            rdiag: env_flag("RWM_RDIAG", false),
            fdiag: env_flag("RWM_FDIAG", false),
            trace: env_flag("RWM_TRACE", false),
            pfrac: env_flag("RWM_PFRAC", false),
        }
    }

    /// The `[GATES]` LIVENESS ECHO — one line naming every gate resolved here
    /// and its RESOLVED value (goal-gate "Gate-Forwarding Audit", 2026-08-09;
    /// MEASUREMENT DISCIPLINE item 15).
    ///
    /// Why it exists: before this, 41 of the 76 `RWM_*` knobs had no echo at
    /// all, so a battery arm keyed on one of them could not be proven live —
    /// its verdict was unfalsifiable in principle, whatever the numbers said.
    /// The audit's rule is now that every arm asserts its gate's echo, which
    /// requires every gate to HAVE one. Rather than 41 hand-written `info!`s
    /// that can go stale one at a time, this is ONE line covering the whole
    /// `RuntimeGates` surface, emitted once at engine start.
    ///
    /// Cheap and off the hot path: called exactly once, from the same place
    /// `resolve()` is. The pre-existing per-mechanism `... ACTIVE (RWM_X: …)`
    /// echoes are deliberately KEPT — they carry the law's statement and the
    /// ledger's assertions are written against them; this line is the total
    /// backstop underneath them, and it is two-sided by construction (it
    /// prints the OFF values too, so "gate absent" is as checkable as "gate
    /// present" — the `sp=1` / `sp=0` discipline generalized to every gate).
    ///
    /// Gates resolved OUTSIDE this struct keep their own echoes and are
    /// enumerated in `EXTERNALLY_ECHOED` below, which the coverage test reads.
    pub fn echo_line(&self) -> String {
        let b = |v: bool| if v { "1" } else { "0" };
        let o = |v: &Option<f64>| v.map_or("unset".to_string(), |x| x.to_string());
        let ou = |v: &Option<usize>| v.map_or("unset".to_string(), |x| x.to_string());
        format!(
            "[GATES] RWM_UNIFIED={} RWM_UNIFIED_SHED={} RWM_TAPER_R={} \
             RWM_ASTAR_ANCHOR={} RWM_MSTAR_ANCHOR={} RWM_PLAIN_RS={} \
             RWM_HONEST_ANCHOR={} RWM_HONEST_K={} \
             RWM_STORE_SACK_RELEASE={} RWM_STORE_PATHS={} RWM_STORE_PATH_POOL={} \
             RWM_STORE={} RWM_STORE_GAIN={} RWM_STORE_BOOT={} RWM_STORE_CAPW={} \
             RWM_STORE_CAP_UNIFIED={} RWM_THREE_TERM={} RWM_COMPOSED_CAP={} \
             RWM_SUM_CAP={} RWM_LATE_BRAKE={} RWM_DELTA_CAP={} \
             RWM_STORE_PERCAP={} RWM_PERCAP_GUARD={} RWM_STORE_BORROW={} \
             RWM_HONEST_CAP={} RWM_POOL_ANCHOR={} RWM_POOL_DELIV={} \
             RWM_FLOOR_BOUND={} RWM_ACK_MERGE={} RWM_LOSS_SENT_TRUTH={} \
             RWM_RELEASE_1TO1={} RWM_CHARGE_RECOVERY={} \
             RWM_PATIENCE_DERIVED={} \
             RWM_SIDLE_DERIVED={} RWM_WIN_DECOUPLE={} RWM_PLACE_SLACK={} \
             RWM_COLD_PLACE={} \
             RWM_GEN={} RWM_PIPELINE={} RWM_GEN_PIPE={} RWM_GEN_R={} \
             RWM_GEN_RATE={} RWM_GEN_RATE_FLOOR={} RWM_GEN_INFLIGHT={} \
             RWM_OOO_RETAIN={}/{} RWM_WINDOW={} RWM_REPORT_GENS={} \
             RWM_REPAIR_WAIT={} RWM_CODED_SRC={} RWM_NO_REACTIVE={} \
             RWM_XPATH_REPAIR={} RWM_PROACTIVE_PACER={} RWM_REASM_BDP={} \
             RWM_MIN_R={} RWM_CC_PACE={} RWM_CC_PACE_HR={} RWM_REACT_CAP={} \
             RWM_INFL_CAP={} RWM_INFL_BDP={} RWM_COPA_FEED={} RWM_RS_ATTR={} \
             RWM_EMIT_BATCH={} RWM_EMIT_BURST={} RWM_RECOV_MP={} \
             RWM_RECOV_MP_LAW={} RWM_RECOV_MP_LIVE={} RWM_RECOV_SP={} \
             RWM_DERIVED_SWEEP={} RWM_RACK_CLOCKS={} RWM_RACK_REO_MULT={} RWM_QUANTILE_CLOCKS={} \
             RWM_ALPHA_OVERRIDE={} RWM_W_FORM={} RWM_HOLDDOWN_Q={} \
             RWM_DIAG={} RWM_ACKDIAG={} RWM_ACKDIAG_WINDOW_US={} \
             RWM_RTT_DUMP={} RWM_RTT_DUMP_MAX={} \
             RWM_SUCC_DUMP={} RWM_SUCC_DUMP_MAX={} \
             RWM_WALLDIAG={} RWM_CPUPROF={} RWM_RDIAG={} \
             RWM_FDIAG={} RWM_TRACE={} RWM_PFRAC={}",
            b(self.unified), b(self.unified_shed), b(self.taper_r),
            b(self.astar_anchor), b(self.mstar_anchor), b(self.plain_rs),
            b(self.honest_anchor), b(self.honest_k),
            b(self.store_sack_release), b(self.store_paths), self.store_path_pool,
            ou(&self.store_override), self.store_gain, self.store_boot, b(self.store_capw),
            b(self.store_cap_unified), b(self.three_term), b(self.composed_cap),
            b(self.sum_cap), b(self.late_brake), b(self.delta_cap),
            b(self.store_percap), b(self.percap_guard), b(self.store_borrow),
            b(self.honest_cap), b(self.pool_anchor), b(self.pool_deliv),
            b(self.floor_bound), b(self.ack_merge), b(self.loss_sent_truth),
            b(self.release_1to1), b(self.charge_recovery),
            b(self.patience_derived),
            b(self.sidle_derived), b(self.win_decouple), b(self.place_slack),
            b(self.cold_place),
            self.gen_size, self.pipeline, b(self.gen_pipe), o(&self.gen_r),
            self.gen_rate, self.gen_rate_floor, o(&self.gen_inflight),
            b(self.ooo_retain), self.ooo_gens, ou(&self.window_override),
            ou(&self.report_gens),
            self.repair_wait_ms.map_or("unset".to_string(), |v| v.to_string()),
            b(self.coded_src), b(self.no_reactive),
            b(self.xpath_repair), b(self.proactive_pacer), b(self.reasm_bdp),
            self.min_r, b(self.cc_pace), self.cc_pace_headroom, o(&self.react_cap),
            self.infl_cap, o(&self.infl_bdp), b(self.copa_feed), b(self.rs_attr),
            b(self.emit_batch), self.emit_burst, b(self.recov_mp),
            b(self.recov_mp_law), b(self.recov_mp_live), b(self.recov_sp),
            b(self.derived_sweep), b(self.rack_clocks), self.rack_reo_mult, b(self.quantile_clocks),
            // THE EXPERIMENT α, echoed as its RESOLVED value and never as a
            // flag — the `RWM_ACKDIAG_WINDOW_US` precedent one line below,
            // and for the same reason. α is the ONE variable of the sweep
            // this knob exists for, so a row whose α is not readable off its
            // own run is not a row. `unset` means the contract's own
            // `target_tail_loss × ζ(hint)` is in force; a number means it is
            // not. A mistyped or out-of-domain override resolves back to
            // `unset` and prints as `unset`, so "my arm did not take" is READ
            // rather than inferred — the failure mode that produced the
            // 31 Mbit/s anomaly, where a configuration axis had no echo at all.
            o(&self.alpha_override),
            // THE `W` LAW, echoed as its RESOLVED TOKEN and never as a flag —
            // same precedent, same reason. `cantelli` means today's law is in
            // force; `quantile` means §16.76's is. A mistyped or unknown value
            // resolves back to `cantelli` and prints as `cantelli`, so an arm
            // that did not take is READ rather than inferred. Paper §16.76.
            self.w_form.as_str(),
            // THE HOLD-DOWN LEVEL, echoed as its RESOLVED value and never as a
            // flag — same precedent, same reason. `unset` means no hold-down
            // and today's machine; a number means the sender is waiting before
            // it answers a gap report. Paper §16.77.
            o(&self.holddown_q),
            // The ack-cadence gauge's WINDOW is echoed as its RESOLVED value in
            // µs, not as a flag: it is the unit every `[ACKDIAG]` series is
            // measured in, so a ledger whose windows are 250 ms and one whose
            // windows are 2 s are different measurements and the difference has
            // to be readable from the run's own output. A mistyped override
            // resolves back to the default and this prints 2000000, so "my arm
            // did not take" is visible rather than inferred.
            b(self.diag), b(self.ackdiag), crate::net::ackdiag::window_us(),
            // The raw-sample dump's CAP is echoed as its RESOLVED value, for
            // the reason `RWM_ACKDIAG_WINDOW_US` two lines above is: a leg
            // whose dump was truncated at 400 000 samples and one that was not
            // are different measurements of clause `B`, and the difference has
            // to be readable off the run's own output rather than inferred.
            b(self.rtt_dump), crate::net::rttdump::dump_max(),
            // The successor dump's CAP, echoed as its RESOLVED value for the
            // reason `RWM_RTT_DUMP_MAX` one line above is: a receiver whose
            // raw record stream was truncated and one that was not are
            // different inputs to the derivation that reads them, and the
            // difference has to be readable off the run's own output.
            b(self.succ_dump), crate::net::succ::dump_max(),
            b(self.walldiag), b(self.cpuprof), b(self.rdiag),
            b(self.fdiag), b(self.trace), b(self.pfrac),
        )
    }

    /// Emit the `[GATES]` echo. Call ONCE, right after [`Self::resolve`].
    pub fn echo(&self) {
        tracing::info!("{}", self.echo_line());
    }
}

/// `RWM_*` knobs the harness forwards that are NOT resolved by
/// [`RuntimeGates`], each with the reason the coverage test accepts it.
/// Every entry either has its OWN resolve-time echo elsewhere in the engine
/// or is a harness/L0-sim knob with no engine gate behind it.
#[cfg(test)]
const EXTERNALLY_ECHOED: &[(&str, &str)] = &[
    ("RWM_ANCHOR_HYGIENE", "umbrella; folded into the astar/mstar/plain_rs values this line prints"),
    ("RWM_CLOCK_GAP", "own echo: 'clock-gap estimator hygiene ACTIVE' (control/anchor.rs wiring, net/mod.rs)"),
    ("RWM_COPA_WIRE", "own echo: scheduler Copa family resolve"),
    ("RWM_COPA_DELTA", "own echo: scheduler Copa family resolve"),
    ("RWM_COPA_COMPETE", "own echo: scheduler Copa family resolve"),
    ("RWM_EST_CADENCE", "own echo: 'estimator heavy-math cadence ACTIVE' (control/estimator.rs)"),
    ("RWM_MTU_FLOOR", "own echo: 'MTU floor: …' / 'MTU floor OFF' (transport/quic.rs)"),
    ("RWM_QUIC_CC", "own echo: 'quinn congestion controller: …' (transport/quic.rs)"),
    ("RWM_WIRE_COMPACT", "own echo: compact v5 DATA framing (transport/quic.rs part-2 echo)"),
    ("RWM_RS_TRACE", "instrument: its own [RSTRACE] output IS the echo"),
    ("RWM_RSTAR_TAIL", "r* provisioning knob, read at the tail-provisioning site"),
    ("RWM_PLACE_T", "placement temperature, read by the perf harness path"),
    ("RWM_PERF_TIMEOUT_S", "harness-only: per-run completion timeout (src/perf.rs)"),
    ("RWM_L0_NETEM", "L0 sim harness knob, no engine gate"),
    ("RWM_L0_SEED", "L0 sim harness knob, no engine gate"),
];

#[cfg(test)]
mod forwarding_audit {
    use super::EXTERNALLY_ECHOED;
    use std::collections::BTreeSet;

    /// Every `RWM_*` string literal the ENGINE reads, scraped from the crate
    /// source. Test-only reflection: there is no runtime registry of gates,
    /// and building one would not survive a gate added the old way (an
    /// inline `env::var` at a new site), which is exactly the failure this
    /// audit exists to prevent.
    fn engine_gate_surface() -> BTreeSet<String> {
        fn walk(dir: &std::path::Path, out: &mut String) {
            for e in std::fs::read_dir(dir).expect("read src dir").flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push_str(&std::fs::read_to_string(&p).unwrap_or_default());
                }
            }
        }
        let mut src = String::new();
        walk(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut src,
        );
        let mut out = BTreeSet::new();
        // Match the `"RWM_..."` literal form every gate read uses.
        let bytes = src.as_bytes();
        let mut i = 0;
        while let Some(off) = src[i..].find("\"RWM_") {
            let start = i + off + 1;
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_uppercase() || bytes[end].is_ascii_digit() || bytes[end] == b'_') {
                end += 1;
            }
            if end < bytes.len() && bytes[end] == b'"' {
                let name = &src[start..end];
                // `len > 4` drops the bare `"RWM_"` prefix literal this
                // scraper itself contains; RWM_TEST_* are `config::env_flag`'s
                // own unit-test fixtures, not gates.
                if name.len() > 4 && !name.starts_with("RWM_TEST_") {
                    out.insert(name.to_string());
                }
            }
            i = start;
        }
        assert!(out.len() > 60, "gate scrape found only {} names", out.len());
        out
    }

    /// The harness's `RWM_FORWARD` array in `tools/l1/lib.sh`.
    fn harness_forward_list() -> BTreeSet<String> {
        let lib = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/l1/lib.sh"),
        )
        .expect("tools/l1/lib.sh must exist — it is the single forwarding list");
        let body = lib
            .split_once("RWM_FORWARD=(")
            .expect("lib.sh must define RWM_FORWARD=(")
            .1
            .split_once(')')
            .expect("unterminated RWM_FORWARD array")
            .0;
        body.split_whitespace().map(str::to_string).collect()
    }

    /// **The audit's enforcement gate** (goal-gate "Gate-Forwarding Audit",
    /// 2026-08-09). Adding an `RWM_*` gate to the engine without adding it to
    /// `tools/l1/lib.sh`'s `RWM_FORWARD` fails HERE, at `cargo test`, instead
    /// of silently producing a battery arm whose knob may never reach the
    /// wire. This is the structural fix for the defect the ack-merge flip
    /// battery found: `RWM_ACK_MERGE` had never been added to
    /// `perf_rwm_c.sh`'s hand-rolled allowlist, and eleven more gates were in
    /// the same state — every one of them undetectable by any test.
    #[test]
    fn gate_forwarding_list_covers_the_engine_surface() {
        let engine = engine_gate_surface();
        let fwd = harness_forward_list();
        let missing: Vec<_> = engine.difference(&fwd).collect();
        assert!(
            missing.is_empty(),
            "these RWM_* gates are read by the engine but are NOT in \
             tools/l1/lib.sh's RWM_FORWARD, so no L1 driver forwards them \
             explicitly: {missing:?}"
        );
        // The reverse direction keeps the list from accumulating dead knobs
        // (the audit removed 16 such entries from perf_rwm_c.sh, e.g. the
        // whole RWM_DAPS_* family, RWM_SACK_PRUNE and RWM_FRONTIER_*).
        let stale: Vec<_> = fwd.difference(&engine).collect();
        assert!(
            stale.is_empty(),
            "RWM_FORWARD names knobs the engine no longer reads: {stale:?}"
        );
    }

    /// Every forwarded gate must have a LIVENESS ECHO — either in the
    /// `[GATES]` line or its own, registered in `EXTERNALLY_ECHOED` with a
    /// reason. A gate with no echo cannot be proven live, so any battery
    /// verdict resting on it is unfalsifiable (MEASUREMENT DISCIPLINE 1/15).
    #[test]
    fn every_forwarded_gate_has_a_liveness_echo() {
        let line = super::RuntimeGates::resolve().echo_line();
        let known: BTreeSet<&str> = EXTERNALLY_ECHOED.iter().map(|(n, _)| *n).collect();
        let unechoed: Vec<_> = harness_forward_list()
            .into_iter()
            .filter(|g| !line.contains(g.as_str()) && !known.contains(g.as_str()))
            .collect();
        assert!(
            unechoed.is_empty(),
            "these gates have NO liveness echo — add them to \
             RuntimeGates::echo_line() or register them in EXTERNALLY_ECHOED \
             with the echo they already own: {unechoed:?}"
        );
    }

    /// The echo is TWO-SIDED: it prints the OFF value too, so a battery can
    /// assert both "gate present in the arm" and "gate absent in the control"
    /// — the `sp=1`/`sp=0` discipline (goal-gate "Lossy-Single Residual")
    /// generalized to the whole surface.
    #[test]
    fn the_gates_echo_is_two_sided() {
        let line = super::RuntimeGates::resolve().echo_line();
        assert!(line.starts_with("[GATES] "));
        assert!(
            line.contains("RWM_RECOV_SP=0"),
            "a default-OFF gate must still be NAMED with its 0 value: {line}"
        );
        assert!(
            line.contains("RWM_ACK_MERGE=1"),
            "a default-ON gate must be named with its 1 value: {line}"
        );
        // goal-gate "The Derived Recovery Clamp": the OFF-VALUE echo the
        // battery's control arm asserts.
        assert!(
            line.contains("RWM_DERIVED_SWEEP=0"),
            "RWM_DERIVED_SWEEP must print its OFF value: {line}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Default-env resolution reproduces the shipped defaults (the ADR-0067
    /// consolidated stack): the CORE laws ON, every experiment gate OFF.
    /// (Set-env semantics are `config::env_flag`'s and are tested there;
    /// integration tests exercise gate activation per feature.)
    #[test]
    fn default_env_resolves_the_shipped_stack() {
        // NOTE: relies on the test env not exporting RWM_* overrides — same
        // assumption every engine-default test in this crate makes.
        let g = RuntimeGates::resolve();
        // CORE (default ON)
        assert!(g.unified && g.unified_shed && g.taper_r);
        assert!(g.astar_anchor && g.mstar_anchor);
        assert!(g.store_sack_release && g.store_paths);
        assert!(g.recov_mp && g.recov_mp_law);
        assert!(!g.recov_sp, "RWM_RECOV_SP ships default OFF (A/B arm)");
        assert!(
            !g.derived_sweep,
            "RWM_DERIVED_SWEEP ships default OFF (A/B arm — goal-gate \
             \"The Derived Recovery Clamp\")"
        );
        // The three laws added 2026-08-19 (paper 16.67 / 16.68 / 16.69).
        // RWM_RACK_REO_MULT defaults to RFC 8985 6.2 Step 4's own initial
        // reo_wnd_mult of 1, so an unset run is RACK's own starting point and
        // not an operator-chosen number.
        //
        // THE CoDel-DERIVED SETPOINT (paper 16.67/16.70/16.71, ADR-0071
        // family 2) - FLIPPED DEFAULT ON 2026-08-19. The battery that scored
        // it (goal-gate "Candidates Battery - RESULTS", rung D) measured
        // D-LAT six of six: goodput parity at every dual on both seeds with
        // q_p50 down 10-200 ms at every one; interior with the ceiling
        // provably inert at c7 and c8 (pin 0.0000); bit-identical at N = 1
        // (eng 0/0 at c1 and sc2); and c8's paired dead wall shortened
        // (p = 0.011). Pinned ON here so the flip cannot drift back silently;
        // the OFF-value property now belongs to the `=0` arm, asserted below
        // on an explicit arm rather than on the default.
        assert!(
            g.delta_cap,
            "RWM_DELTA_CAP ships DEFAULT ON since 2026-08-19 (candidates \
             battery rung D DELIVERED: D-LAT 6/6 - goodput parity at every \
             dual both seeds with q_p50 down 10-200 ms; interior with the \
             ceiling inert at c7/c8; bit-identical at N = 1). `=0` remains \
             the re-runnable A/B arm - the displaced gain = 2.0 fossil."
        );
        assert!(
            !g.rack_clocks,
            "RWM_RACK_CLOCKS ships default OFF (A/B arm - paper 16.68; the \
             candidates battery scored it REFUTED-WITH-RECORD on RACK's own \
             false-alarm bar at every arm and every cell, so the SAME program \
             that flipped RWM_DELTA_CAP does not flip this)"
        );
        assert!(
            !g.quantile_clocks,
            "RWM_QUANTILE_CLOCKS ships default OFF (A/B arm - paper 16.69,              REFUTED-WITH-RECORD and shipped only so the refutation is              reproducible)"
        );
        assert_eq!(
            g.rack_reo_mult,
            crate::net::RACK_REO_WND_MULT_INIT,
            "RWM_RACK_REO_MULT must default to RACK's OWN initial value"
        );
        // The gates echo is what a battery parses; assert the three new names
        // are on it with their resolved values, two-sided.
        let line = g.echo_line();
        for tok in [
            // Flipped 2026-08-19: the echo must name the SHIPPED value.
            "RWM_DELTA_CAP=1",
            "RWM_RACK_CLOCKS=0",
            "RWM_QUANTILE_CLOCKS=0",
            "RWM_RACK_REO_MULT=1",
            // The EXPERIMENT α knob is ABSENT on every shipped arm, and its
            // echo says so in the same line every battery already scrapes.
            "RWM_ALPHA_OVERRIDE=unset",
            // The `W` law's RESOLVED token. `cantelli` is today's behaviour
            // and is what the default arm must echo (paper 16.76).
            "RWM_W_FORM=cantelli",
        ] {
            assert!(line.contains(tok), "the [GATES] echo is missing {tok}: {line}");
        }
        assert!(
            g.alpha_override.is_none(),
            "RWM_ALPHA_OVERRIDE is an EXPERIMENT knob and is ABSENT by default \
             - it is not a mapping, it is not continuous in any dial, and \
             nothing shipped may read it (paper 16.69; \
             docs/research/cost-ratio-memo.md)"
        );
        // THE ARMED ARM'S ECHO, two-sided: a swept row must be able to state
        // its own alpha off its own log. Set by FIELD rather than through the
        // environment - env mutation is process-global in a parallel runner.
        let mut swept = g.clone();
        swept.alpha_override = Some(0.184);
        assert!(
            swept.echo_line().contains("RWM_ALPHA_OVERRIDE=0.184"),
            "the armed arm's echo must NAME the RESOLVED alpha, not a flag - \
             the RWM_ACKDIAG_WINDOW_US precedent: {}",
            swept.echo_line()
        );
        // THE `W` FORM SHIPS AS `cantelli` AND THE ARMED ARM ECHOES ITS OWN
        // RESOLVED TOKEN (paper 16.76). Both sides asserted: an arm that did
        // not take must be readable, not inferred.
        assert_eq!(
            g.w_form,
            crate::net::WForm::Cantelli,
            "RWM_W_FORM ships as `cantelli` - the quantile-native law is an \
             A/B arm inside RWM_QUANTILE_CLOCKS (itself default OFF and \
             REFUTED-STANDING) and nothing shipped may read it (paper 16.76)"
        );
        let mut qform = g.clone();
        qform.w_form = crate::net::WForm::Quantile;
        assert!(
            qform.echo_line().contains("RWM_W_FORM=quantile"),
            "the armed arm's echo must NAME the RESOLVED W law: {}",
            qform.echo_line()
        );
        // GARBAGE RESOLVES BACK TO ABSENT, and `absent` is `cantelli`. Parsed
        // by FIELD rather than through the environment - env mutation is
        // process-global in a parallel runner.
        for junk in ["", " ", "quantle", "1", "CANTELLI-ish", "true"] {
            assert!(
                crate::net::WForm::parse(junk).is_none(),
                "RWM_W_FORM={junk:?} must resolve back to ABSENT, not to an arm"
            );
        }
        assert_eq!(crate::net::WForm::parse(" Quantile ").unwrap(), crate::net::WForm::Quantile);
        assert_eq!(crate::net::WForm::parse("cantelli").unwrap(), crate::net::WForm::Cantelli);
        // THE `=0` ARM'S OFF-VALUE PROPERTY, which the default assertion above
        // used to carry (MEASUREMENT DISCIPLINE 15, two-sided): a battery
        // re-running the displaced `gain = 2.0` fossil must be able to assert
        // the gate ABSENT on both endpoints, not merely unmentioned. RE-HOMED
        // onto an EXPLICIT arm now that the default is ON, so the property
        // survives the flip instead of being retired by it. Set by field
        // rather than through the environment: env mutation is process-global
        // state in a parallel runner.
        let mut off_arm = g.clone();
        off_arm.delta_cap = false;
        assert!(
            off_arm.echo_line().contains("RWM_DELTA_CAP=0"),
            "the `=0` arm's echo must NAME the delta-cap gate with its 0 value \
             - the displaced gain = 2.0 fossil stays re-runnable and \
             scrapeable: {}",
            off_arm.echo_line()
        );
        assert!(g.gen_pipe, "gen_pipe default rides unified_active()");
        // The est×honest-anchor composed flip (goal-gate "Ship The Wins 1",
        // 2026-08-07) was measured and REVERTED by its pre-set c7 clause:
        // everything unset ⇒ est-cadence OFF (estimator's own default test)
        // ⇒ pool-anchor OFF (it rides the est resolution), emit-batch OFF.
        // The composed opt-in (est=1 ⇒ pa on, + eb=1) stays the documented
        // fast single-path configuration (c1 446–508).
        assert!(
            !g.pool_anchor,
            "RWM_POOL_ANCHOR default rides the RWM_EST_CADENCE resolution (OFF unset)"
        );
        // "Ship The Wins 1b" (2026-08-07): arm A rides the pool-anchor
        // resolution (⇒ OFF unset), arm B is a pure A/B arm (always OFF
        // unset). Neither may reach the shipped default stack.
        assert!(
            !g.pool_deliv,
            "RWM_POOL_DELIV default rides the RWM_POOL_ANCHOR resolution (OFF unset)"
        );
        assert!(
            !g.floor_bound,
            "RWM_FLOOR_BOUND ships default OFF (A/B arm)"
        );
        // "Ack-Merge Flip" (2026-08-08): the window-mode control-datagram
        // merge PASSED its own pre-registered gate set at full scope (×8,
        // both seeds, + sustained + crown) and is now part of the shipped
        // stack. c1 +12.7%/+13.0% with receiver CPU per bit −9.1%/−8.4%;
        // control-datagram density 1.96 → 1.00 at c1 and 1.05 → 1.00 at c7,
        // and the response tracks the density removed cell by cell. Every
        // no-regression gate held within σ of its own same-session control.
        assert!(
            g.ack_merge,
            "RWM_ACK_MERGE ships default ON since 2026-08-08 (paper §16.42); \
             RWM_ACK_MERGE=0 is the opt-out arm"
        );
        // "Cross-Path Loss Contamination" (2026-08-18) and its successor
        // "The Accounting Ledger" (fix/accounting-ledger): the three honest-
        // accounting gates all ship OFF. The loss estimator's re-heats every
        // SRTT/loss-scaled recovery cadence (the named follow-up); the two
        // ledger gates change the ADMISSION gauge's operand and must be
        // measured on the wire before any flip.
        assert!(
            !g.loss_sent_truth,
            "RWM_LOSS_SENT_TRUTH ships default OFF pending the cadence re-derivation"
        );
        assert!(
            !g.release_1to1,
            "RWM_RELEASE_1TO1 ships default OFF (A/B arm)"
        );
        assert!(
            !g.charge_recovery,
            "RWM_CHARGE_RECOVERY ships default OFF (A/B arm)"
        );
        // "Unlock The Default 2: derived patience" (2026-08-07): the derived
        // recovery-patience floor is a pure A/B arm and must not reach the
        // shipped default stack until its pre-registered gate set passes;
        // the derived stall gauge is DIAG-only and also ships OFF.
        assert!(
            !g.patience_derived,
            "RWM_PATIENCE_DERIVED ships default OFF (A/B arm)"
        );
        assert!(
            !g.sidle_derived,
            "RWM_SIDLE_DERIVED ships default OFF (DIAG-only A/B gauge)"
        );
        // Experiments / instruments (default OFF)
        assert!(!g.store_percap && !g.store_borrow && !g.plain_rs);
        // "Honest Inputs" (2026-08-10): both fixes ship default OFF (A/B
        // arms; anchor-hygiene umbrella members). The OFF-VALUE PROPERTY,
        // two-sided on the echo (MEASUREMENT DISCIPLINE 15): a battery must
        // be able to assert the gates ABSENT on the control arm.
        assert!(
            g.honest_anchor,
            "RWM_HONEST_ANCHOR ships DEFAULT ON since 2026-08-11 (flip-battery F7 \
             swept: goodput within 2σ every cell/seed, CPU/byte 0.90–1.03×; \
             value-identical by the unit-pinned equivalence). `=0` remains the \
             re-runnable legacy-fold A/B arm."
        );
        assert!(
            !g.honest_k,
            "RWM_HONEST_K ships default OFF (A/B arm — goal-gate \"Honest Inputs\"; \
             flip battery: rode only the failed BHU composition, khr−kraw ≈ 0 in-cell)"
        );
        assert!(
            g.echo_line().contains("RWM_HONEST_ANCHOR=1")
                && g.echo_line().contains("RWM_HONEST_K=0"),
            "the default echo must NAME both Honest-Inputs gates with their shipped \
             values (anchor=1 since the 2026-08-11 flip, K=0): {}",
            g.echo_line()
        );
        assert!(!g.emit_batch, "emission batching ships OFF (the composed flip reverted)");
        assert_eq!(g.emit_burst, 64);
        assert!(!g.store_capw, "RWM_STORE_CAPW ships default OFF (A/B arm)");
        assert!(!g.win_decouple, "RWM_WIN_DECOUPLE ships default OFF (A/B arm)");
        assert!(!g.place_slack, "RWM_PLACE_SLACK ships default OFF (A/B arm)");
        assert!(
            !g.cold_place,
            "RWM_COLD_PLACE ships default OFF (A/B arm) — the cold-start \
             placement repair must be opted into"
        );
        assert!(
            !g.recov_mp_live,
            "RWM_RECOV_MP_LIVE ships default OFF (A/B arm)"
        );
        assert!(
            !g.store_cap_unified,
            "RWM_STORE_CAP_UNIFIED ships default OFF (A/B arm)"
        );
        assert!(
            !g.three_term,
            "RWM_THREE_TERM ships default OFF (A/B arm — goal-gate \"Three-Term Law\")"
        );
        // The gate's OFF-VALUE PROPERTY, asserted on the echo itself
        // (MEASUREMENT DISCIPLINE 15, two-sided): a battery must be able to
        // assert the gate ABSENT in the control arm, not merely unmentioned.
        assert!(
            g.echo_line().contains("RWM_THREE_TERM=0"),
            "the default echo must NAME the three-term gate with its 0 value: {}",
            g.echo_line()
        );
        // The COMPOSED CAP LAW (paper §16.56, ADR-0070 Deliverable 2) is an
        // A/B arm and ships OFF, with the same two-sided OFF-value property.
        assert!(
            !g.composed_cap,
            "RWM_COMPOSED_CAP ships default OFF (A/B arm — paper §16.56)"
        );
        assert!(
            g.echo_line().contains("RWM_COMPOSED_CAP=0"),
            "the default echo must NAME the composed-cap gate with its 0 value: {}",
            g.echo_line()
        );
        // THE `×N` DELETION (paper §16.60/§16.64, ADR-0070 finding 2) —
        // **FLIPPED DEFAULT ON 2026-08-19**. The A/B that ADR-0070 said had
        // never been run was run (goal-gate "Ladder Battery — RESULTS", rung
        // N): interior at both scoreable duals (`pin` 0.000, `eng` 1.000,
        // `chg_frac` 1.000), the control reproducing the shipped 4096 pin,
        // goodput UP at c8 on both seeds, CPU 0.937–1.005×, all guards green.
        // Pinned ON here so the flip cannot drift back silently; the OFF-value
        // property now belongs to the `=0` arm, asserted below on an explicit
        // arm rather than on the default.
        assert!(
            g.sum_cap,
            "RWM_SUM_CAP ships DEFAULT ON since 2026-08-19 (ladder battery rung \
             N DELIVERED: interior at both duals, pin 0.000 / eng 1.000 / \
             chg_frac 1.000, goodput ≥ control at c8 both seeds). `=0` remains \
             the re-runnable A/B arm — the displaced quadratic."
        );
        assert!(
            g.echo_line().contains("RWM_SUM_CAP=1"),
            "the default echo must NAME the sum-cap gate with its shipped 1 \
             value (flipped 2026-08-19): {}",
            g.echo_line()
        );
        // THE `=0` ARM'S OFF-VALUE PROPERTY, which the default assertion above
        // used to carry (MEASUREMENT DISCIPLINE 15, two-sided): a battery
        // re-running the displaced quadratic must be able to assert the gate
        // ABSENT on both endpoints, not merely unmentioned. Asserted on an
        // EXPLICIT arm now that the default is ON, so the property survives the
        // flip instead of being retired by it. Set by field rather than through
        // the environment: env mutation is process-global state in a parallel
        // runner.
        let mut off_arm = g.clone();
        off_arm.sum_cap = false;
        assert!(
            off_arm.echo_line().contains("RWM_SUM_CAP=0"),
            "the `=0` arm's echo must NAME the sum-cap gate with its 0 value — \
             the displaced quadratic stays re-runnable and scrapeable: {}",
            off_arm.echo_line()
        );
        // THE EXTRACTED LATE-STAGE BRAKE (§16.60.1, ADR-0070 finding 7) is
        // still an A/B arm and still ships OFF: the ladder scored it
        // DELIVERED-AS-ARMED but NEEDS-MORE for effect (B-WALL closed on
        // power), so it is NOT flipped by the same program that flipped
        // `RWM_SUM_CAP`.
        assert!(
            !g.late_brake,
            "RWM_LATE_BRAKE ships default OFF (A/B arm — paper §16.60.1; ladder \
             battery: armed on 110/110 FULL reps but its EFFECT is unresolved)"
        );
        assert!(
            g.echo_line().contains("RWM_LATE_BRAKE=0"),
            "the default echo must NAME the late-brake gate with its 0 value: {}",
            g.echo_line()
        );
        assert!(!g.proactive_pacer && !g.xpath_repair && !g.no_reactive);
        assert!(!g.diag && !g.rdiag && !g.fdiag && !g.trace && !g.pfrac);
        // The ack-cadence gauge (goal-gate "Ack-Cadence Gauge", 2026-08-11)
        // is a DIAG-surface instrument and ships OFF, with the two-sided
        // OFF-VALUE property asserted on the echo (MEASUREMENT DISCIPLINE 15).
        assert!(
            !g.ackdiag,
            "RWM_ACKDIAG ships default OFF (DIAG-surface instrument)"
        );
        assert!(
            g.echo_line().contains("RWM_ACKDIAG=0"),
            "the default echo must NAME the ack-cadence gauge with its 0 value: {}",
            g.echo_line()
        );
        // The raw RTT sample dump (clause `B`'s exact reference, 2026-08-21)
        // is the same class and ships the same way — and it matters more here
        // than for its siblings, because ON it writes megabytes of stderr per
        // path and takes a lock on every RTT sample.
        assert!(
            !g.rtt_dump,
            "RWM_RTT_DUMP ships default OFF (raw-sample dump: megabytes of \
             stderr and a per-sample lock)"
        );
        assert!(
            g.echo_line().contains("RWM_RTT_DUMP=0"),
            "the default echo must NAME the raw-sample dump with its 0 value: {}",
            g.echo_line()
        );
        assert!(
            g.echo_line().contains("RWM_RTT_DUMP_MAX=400000"),
            "the default echo must carry the dump cap's RESOLVED value, so a \
             truncated leg's clause B is readable off its own run: {}",
            g.echo_line()
        );
        // The successor-arrival RAW dump (2026-08-21) is the same class and
        // ships the same way — at the RECEIVER, where the cost is directly
        // goodput-visible. Its QUANTILE line is ungated and always emitted;
        // only the raw record stream is behind this flag, which is what lets a
        // scored pass read the distribution without paying for the dump.
        assert!(
            !g.succ_dump,
            "RWM_SUCC_DUMP ships default OFF (raw per-hole records: megabytes \
             of receiver-side stderr on a lossy cell)"
        );
        assert!(
            g.echo_line().contains("RWM_SUCC_DUMP=0"),
            "the default echo must NAME the successor dump with its 0 value: {}",
            g.echo_line()
        );
        assert!(
            g.echo_line().contains("RWM_SUCC_DUMP_MAX=200000"),
            "the default echo must carry the successor dump cap's RESOLVED \
             value, so a truncated record stream is readable off its own run \
             rather than inferred by whoever derives against it: {}",
            g.echo_line()
        );
        // The dead-wall onset/duration instrument (ADR-0070 validation path
        // step 2, 2026-08-12) is the same class and ships the same way.
        assert!(
            !g.walldiag,
            "RWM_WALLDIAG ships default OFF (DIAG-surface instrument)"
        );
        assert!(
            g.echo_line().contains("RWM_WALLDIAG=0"),
            "the default echo must NAME the dead-wall gauge with its 0 value: {}",
            g.echo_line()
        );
        // The sender CPU decomposition (goal-gate "MEASUREMENT TRUTH item 2 —
        // THE SENDER CPU CEILING", 2026-08-19) is the same class and ships the
        // same way. The two-sided property matters more here than for its
        // siblings: the cell this instrument is built for is sender-CPU-bound,
        // so an arm that silently carried the gauge would be paying for it in
        // exactly the quantity under measurement, and "the gate did not take"
        // must be readable from the run's own output rather than inferred.
        assert!(
            !g.cpuprof,
            "RWM_CPUPROF ships default OFF (DIAG-surface instrument)"
        );
        assert!(
            g.echo_line().contains("RWM_CPUPROF=0"),
            "the default echo must NAME the CPU-decomposition gauge with its 0 value: {}",
            g.echo_line()
        );
        // Numeric defaults
        assert_eq!(g.gen_size, 384);
        assert_eq!(g.pipeline, 2);
        assert_eq!(g.store_path_pool, 2048);
        assert_eq!(g.store_boot, 128);
        assert!((g.store_gain - 2.0).abs() < 1e-12);
        assert!((g.cc_pace_headroom - 1.1).abs() < 1e-12);
        assert!(g.store_override.is_none());
    }
}
