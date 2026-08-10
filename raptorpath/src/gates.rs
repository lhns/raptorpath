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
    /// `RWM_HONEST_ANCHOR` (umbrella default OFF; goal-gate "Honest
    /// Inputs"): the BtlBw windowed-max read off a monotonic max-deque —
    /// value-identical statistic, O(1) amortized instead of the per-sample
    /// full-window fold whose O(window·rate) cost under `RWM_PLAIN_RS` is
    /// the measured c1 −35% (sender CPU/byte +61…64%, latlever CPU gauge).
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

    // NOTE: `RWM_SCHED_SNAPSHOT` (the net-seam-pass-2 per-iteration scheduler
    // snapshot) lived here and was DELETED unmeasured on 2026-08-10 — its
    // stated hazard was not reachable from the sites it served. ADR-0066
    // deprecation register; goal-gate "Scheduler-Snapshot Adjudication".

    // ── Instruments (ADR-0052; no behavior) ───────────────────────────────
    /// `RWM_DIAG` (default OFF): the transport-ceiling / recovery-plane DIAG.
    pub diag: bool,
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
            patience_derived: crate::scheduler::patience_derived_active(),
            sidle_derived: crate::scheduler::sidle_derived_active(),
            win_decouple: env_flag("RWM_WIN_DECOUPLE", false),
            place_slack: env_flag("RWM_PLACE_SLACK", false),
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
            recov_sp: env_flag("RWM_RECOV_SP", false),
            diag: env_flag("RWM_DIAG", false),
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
             RWM_STORE_CAP_UNIFIED={} RWM_THREE_TERM={} \
             RWM_STORE_PERCAP={} RWM_PERCAP_GUARD={} RWM_STORE_BORROW={} \
             RWM_HONEST_CAP={} RWM_POOL_ANCHOR={} RWM_POOL_DELIV={} \
             RWM_FLOOR_BOUND={} RWM_ACK_MERGE={} RWM_PATIENCE_DERIVED={} \
             RWM_SIDLE_DERIVED={} RWM_WIN_DECOUPLE={} RWM_PLACE_SLACK={} \
             RWM_GEN={} RWM_PIPELINE={} RWM_GEN_PIPE={} RWM_GEN_R={} \
             RWM_GEN_RATE={} RWM_GEN_RATE_FLOOR={} RWM_GEN_INFLIGHT={} \
             RWM_OOO_RETAIN={}/{} RWM_WINDOW={} RWM_REPORT_GENS={} \
             RWM_REPAIR_WAIT={} RWM_CODED_SRC={} RWM_NO_REACTIVE={} \
             RWM_XPATH_REPAIR={} RWM_PROACTIVE_PACER={} RWM_REASM_BDP={} \
             RWM_MIN_R={} RWM_CC_PACE={} RWM_CC_PACE_HR={} RWM_REACT_CAP={} \
             RWM_INFL_CAP={} RWM_INFL_BDP={} RWM_COPA_FEED={} RWM_RS_ATTR={} \
             RWM_EMIT_BATCH={} RWM_EMIT_BURST={} RWM_RECOV_MP={} \
             RWM_RECOV_MP_LAW={} RWM_RECOV_MP_LIVE={} RWM_RECOV_SP={} \
             RWM_DIAG={} RWM_RDIAG={} RWM_FDIAG={} \
             RWM_TRACE={} RWM_PFRAC={}",
            b(self.unified), b(self.unified_shed), b(self.taper_r),
            b(self.astar_anchor), b(self.mstar_anchor), b(self.plain_rs),
            b(self.honest_anchor), b(self.honest_k),
            b(self.store_sack_release), b(self.store_paths), self.store_path_pool,
            ou(&self.store_override), self.store_gain, self.store_boot, b(self.store_capw),
            b(self.store_cap_unified), b(self.three_term),
            b(self.store_percap), b(self.percap_guard), b(self.store_borrow),
            b(self.honest_cap), b(self.pool_anchor), b(self.pool_deliv),
            b(self.floor_bound), b(self.ack_merge), b(self.patience_derived),
            b(self.sidle_derived), b(self.win_decouple), b(self.place_slack),
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
            b(self.diag), b(self.rdiag), b(self.fdiag),
            b(self.trace), b(self.pfrac),
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
            !g.honest_anchor,
            "RWM_HONEST_ANCHOR ships default OFF (A/B arm — goal-gate \"Honest Inputs\")"
        );
        assert!(
            !g.honest_k,
            "RWM_HONEST_K ships default OFF (A/B arm — goal-gate \"Honest Inputs\")"
        );
        assert!(
            g.echo_line().contains("RWM_HONEST_ANCHOR=0")
                && g.echo_line().contains("RWM_HONEST_K=0"),
            "the default echo must NAME both Honest-Inputs gates with their 0 value: {}",
            g.echo_line()
        );
        assert!(!g.emit_batch, "emission batching ships OFF (the composed flip reverted)");
        assert_eq!(g.emit_burst, 64);
        assert!(!g.store_capw, "RWM_STORE_CAPW ships default OFF (A/B arm)");
        assert!(!g.win_decouple, "RWM_WIN_DECOUPLE ships default OFF (A/B arm)");
        assert!(!g.place_slack, "RWM_PLACE_SLACK ships default OFF (A/B arm)");
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
        assert!(!g.proactive_pacer && !g.xpath_repair && !g.no_reactive);
        assert!(!g.diag && !g.rdiag && !g.fdiag && !g.trace && !g.pfrac);
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
