//! Multipath scheduler: distributes symbols across paths based on
//! throughput, loss, and latency measurements.
//!
//! Unlike round-robin MPTCP, we schedule symbols proportional to each path's
//! effective goodput and route repair symbols preferentially to better paths.
//!
//! Congestion control is Copa-lite (delay-based, paper Sections 12.4-12.5),
//! ported from the L0-proven gate-suite driver (P1+P2 semantics):
//!
//!   - Propagation floor = min RTT sample in a sliding ~10s window.
//!   - Queuing-delay signal = min RTT sample since the last cwnd update
//!     (a windowed MIN, not an EWMA: the min sees through transient
//!     serialization bursts to the standing queue; an EWMA stays inflated
//!     long after the queue drains and causes a backoff spiral).
//!   - Hint-coupled queue target (P1): back off when the windowed min
//!     exceeds floor × {1.08 Realtime, 1.125 Auto, 1.25 Bulk}.
//!   - Two-speed ramp: multiplicative ×1.5+1 per RTT until the first
//!     backoff, then additive +2 / multiplicative ×0.92.
//!   - Token-bucket pacing at cwnd/SRTT with burst allowance max(10, cwnd/8)
//!     (state lives here; the drain in net/mod.rs consumes the tokens).
//!
//! Loss alone does NOT reduce the window — only a standing queue does.
//! This prevents wireless random loss from collapsing throughput.
//! No ProbeRTT phase (natural oscillation refreshes the floor).
//!
//! UNITS: `cwnd`, `in_flight`, and pacing tokens are all in SYMBOLS.
//! Pacing rate = cwnd [symbols] / SRTT [s] = symbols/second.

pub mod clock;
pub use clock::*;

use crate::control::fec_rate::ProtocolHint;
use crate::control::LossEstimator;
use crate::fec::{FecBackend, WireSymbol};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Identifies a network path (e.g., WiFi, LTE, Ethernet).
pub type PathId = u32;

/// Copa congestion control parameter: target queue depth.
/// d_copa = 0.5 targets ~2 packets of queue. See paper Section 12.4.
/// Units: 1/symbols — rate = 1/(d_copa [1/sym] × dq [s]) is symbols/second.
const COPA_DELTA: f64 = 0.5;

// --- Wire-clocked Copa signal + hint→δ mapping (feat/copa-wire-signal) ---
//
// Task #80 named Copa-sole's bulk gap: the CC's delay term was fed the
// APP-LAYER ECHO RTT, which includes the sender's own store/reservoir dwell
// in quinn's datagram queue — Copa backed off against self-inflicted delay
// that is not in the network (arm D: shrinking the reservoir raised
// throughput +13–23% AND tightened the queue — the self-signal term proven).
// Under the wire signal the CC delay term is quinn's PACKET-TIMED path RTT
// (Connection::rtt — measured at the QUIC packet layer, excludes app store
// dwell), and Copa runs its ACTUAL update law around the target rate
// 1/(δ·d_q) with δ mapped continuously from the protocol hint's latency
// price (see `copa_delta`, paper §12.4). Gated: active only when the engine
// owns/feeds the substrate window (RWM_QUIC_CC=passthrough or
// RWM_COPA_FEED=1); RWM_COPA_WIRE=0 forces the legacy app-echo behavior
// (the #80 A/B arm), =1 forces on. Env fully unset ⇒ OFF ⇒ the shipped
// path is byte-identical.

/// Pure decision function for the wire-signal gate (unit-testable without
/// process-global env state): `qcc` = RWM_QUIC_CC, `feed` = RWM_COPA_FEED
/// as a flag, `wire` = RWM_COPA_WIRE raw value.
fn copa_wire_from_env(qcc: Option<&str>, feed: bool, wire: Option<&str>) -> bool {
    let feed_active = qcc
        .map(|v| v.trim().eq_ignore_ascii_case("passthrough"))
        .unwrap_or(false)
        || feed;
    match wire {
        Some(v) => {
            let v = v.trim();
            !(v.is_empty() || v == "0" || v.eq_ignore_ascii_case("false"))
        }
        None => feed_active,
    }
}

/// Whether the wire-clocked Copa queue signal (+ the δ-mapped update law) is
/// active for this process. Read once and cached — consulted on the ack hot
/// path.
pub fn copa_wire_active() -> bool {
    use std::sync::OnceLock;
    static F: OnceLock<bool> = OnceLock::new();
    *F.get_or_init(|| {
        let qcc = std::env::var("RWM_QUIC_CC").ok();
        let wire = std::env::var("RWM_COPA_WIRE").ok();
        let on = copa_wire_from_env(
            qcc.as_deref(),
            crate::config::env_flag("RWM_COPA_FEED", false),
            wire.as_deref(),
        );
        // LIVENESS ECHO (goal-gate "Gate-Forwarding Audit", 2026-08-09):
        // two-sided and composed — this gate's value is DERIVED from three
        // knobs, so the echo prints the inputs beside the result. Resolved
        // once, cached; never on the hot path despite the hot-path readers.
        tracing::info!(
            copa_wire = on,
            quic_cc = qcc.as_deref().unwrap_or("unset"),
            copa_wire_env = wire.as_deref().unwrap_or("unset"),
            "Copa wire-clocked signal (RWM_COPA_WIRE / RWM_QUIC_CC / RWM_COPA_FEED)"
        );
        on
    })
}

/// Hint→δ mapping (paper §12.4, wire-signal addendum): Copa's utility is
/// U = log(throughput) − δ·log(delay), so δ IS the marginal latency price.
/// The protocol hint already declares exactly one price ratio — the
/// tail-loss-target scale ζ (`ProtocolHint::tail_loss_scale`, Realtime 0.01
/// / Auto 1 / Bulk 100: Realtime prices lateness 100× dearer, Bulk 100×
/// cheaper). Anchoring Auto at the Copa-paper default δ = 0.5 gives the
/// continuous, constant-free mapping
///
///   δ(hint) = COPA_DELTA / ζ(hint)   ∈ {50 (Realtime), 0.5 (Auto),
///                                        0.005 (Bulk)}
///
/// Equilibrium standing queue = 1/δ packets (rate = 1/(δ·d_q) at the
/// bottleneck rate μ ⇒ q = 1/δ), i.e. d_q* = 1/(δ·μ): Bulk tolerates 200
/// symbols of queue (≈19 ms at the c2 cell's 10.4 k sym/s — still ~3×
/// tighter than BBR-under's measured 65–87 ms), Realtime targets an
/// essentially empty queue (jitter headroom governs), Auto reproduces the
/// classic δ = 0.5 two-packet target. `over` = RWM_COPA_DELTA (the
/// δ-frontier measurement knob), which overrides the hint when set.
fn copa_delta(hint: ProtocolHint, over: Option<f64>) -> f64 {
    over.filter(|d| d.is_finite() && *d > 0.0)
        .unwrap_or(COPA_DELTA / hint.tail_loss_scale())
}

/// `copa_delta` with the RWM_COPA_DELTA env override applied.
pub(crate) fn copa_delta_for_hint(hint: ProtocolHint) -> f64 {
    // LIVENESS ECHO (goal-gate "Gate-Forwarding Audit", 2026-08-09), emitted
    // ONCE per process even though this function is re-entered on every hint
    // change — the fixed-δ probe of the "Copa Competitive Mode" battery is an
    // arm whose only distinguishing knob is this override, so it needs an
    // assertable echo; the OnceLock keeps it off the repeat path.
    {
        use std::sync::OnceLock;
        static ECHOED: OnceLock<()> = OnceLock::new();
        ECHOED.get_or_init(|| {
            tracing::info!(
                copa_delta_override =
                    std::env::var("RWM_COPA_DELTA").as_deref().unwrap_or("unset"),
                "Copa δ override (RWM_COPA_DELTA; unset = the hint→δ mapping)"
            );
        });
    }
    let over = std::env::var("RWM_COPA_DELTA")
        .ok()
        .and_then(|s| s.parse::<f64>().ok());
    copa_delta(hint, over)
}

// --- Copa TCP-competitive mode (feat/copa-compete, task: roadmap item 6) ---
//
// Copa §2.2 (Arun & Balakrishnan, "Copa: Practical Delay-Based Congestion
// Control for the Internet", NSDI 2018) defines TWO operating modes:
//
//   1. the DEFAULT mode (δ fixed — the paper's 0.5; here the hint-mapped
//      δ(hint), see `copa_delta`), and
//   2. a COMPETITIVE mode "where δ is adjusted dynamically to match the
//      aggressiveness of typical buffer-filling schemes".
//
// Detection (verbatim mechanism from the paper): Copa's own dynamics empty
// the bottleneck queue at least once every 5·RTT when only Copa flows share
// it (paper §3). A concurrent long-running buffer-filling flow (Cubic,
// NewReno) breaks that periodicity. "Hence if the sender sees a 'nearly
// empty' queue in the last 5 RTTs, it remains in the default mode;
// otherwise, it switches to competitive mode. We estimate 'nearly empty' as
// any queuing delay lower than 10% of the rate oscillations in the last
// four RTTs; i.e., d_q < 0.1·(RTTmax − RTTmin) where RTTmax is measured
// over the past four RTTs and RTTmin is our long-term minimum" — the
// RTTmax term self-calibrates the notion of "nearly empty" to the path's
// short-term RTT variance.
//
// Competitive law (paper §2.2): "In competitive mode the sender varies 1/δ
// according to whatever buffer-filling algorithm one wishes to emulate
// (e.g., NewReno, Cubic, etc.). In our implementation we perform AIMD on
// 1/δ based on packet success or loss" — NewReno-style: additive increase
// of 1/δ by 1 per RTT without loss, multiplicative decrease (halve 1/δ) on
// a loss event. "In competitive mode, δ ≤ 0.5. When Copa switches from
// competitive mode to default mode, it resets δ to 0.5."
//
// Composition with the hint→δ mapping (ours): the paper's 0.5 is its
// default-mode δ; ours is δ_base = δ(hint). The faithful generalization
// keeps the hint as the BASE price and lets competition adapt AROUND it:
// competitive mode enters at δ = δ_base, AIMD keeps 1/δ ≥ 1/δ_base (the
// paper's "δ ≤ 0.5" with 0.5 → δ_base), and switch-back resets δ = δ_base.
// The loss signal is quinn's wire-level loss detection (the pass-through
// shim's recorded `congestion_events` — the same packet-timed layer as the
// wire d_q clock); FEC recovery is irrelevant here because the AIMD term
// only prices AGGRESSIVENESS against a loss-based competitor, it never
// gates delivery (loss handling stays the FEC layer's job, §12.1).
//
// Hysteresis is the paper's own: the 5-RTT nearly-empty observation window
// on both edges (a competitive-mode Copa cohort still empties the queue
// every 5 RTT if no buffer-filler is present, so an erroneous or stale
// switch self-corrects within a few RTTs; the paper accepts brief flaps by
// design). Gated: `RWM_COPA_COMPETE` (default OFF) and only meaningful on
// top of the wire-clocked signal (the δ-mapped update law is what the
// adapted δ feeds). Env unset ⇒ every path byte-identical.

/// Nearly-empty threshold coefficient (Copa §2.2: d_q < 0.1·(RTTmax−RTTmin)).
const COMPETE_EMPTY_FRAC: f64 = 0.1;
/// Detection window: no nearly-empty queue in the last 5 RTTs ⇒ competitive.
const COMPETE_WINDOW_RTTS: f64 = 5.0;
/// RTTmax lookback for the nearly-empty calibration (paper: past 4 RTTs).
const COMPETE_RTTMAX_RTTS: f64 = 4.0;
/// Bound on 1/δ in competitive mode: 2/δ (the coupling cap's dither term)
/// may never exceed MAX_CWND, so the AIMD's additive growth cannot decouple
/// cwnd from the store the way the uncapped v1 law did (see the coupling-cap
/// note in `wire_update_cwnd`).
const COMPETE_INV_DELTA_MAX: f64 = PathState::MAX_CWND as f64 / 2.0;

/// Pure decision function for the competitive-mode gate: requires BOTH the
/// env flag and the wire-clocked law (the δ adaptation composes with the
/// wire update law; the legacy app-echo dynamics do not consume δ).
fn copa_compete_from_env(compete_flag: bool, wire_active: bool) -> bool {
    compete_flag && wire_active
}

/// Whether Copa's TCP-competitive mode switching is active for this process.
/// Read once and cached (consulted at CopaState construction).
pub fn copa_compete_active() -> bool {
    use std::sync::OnceLock;
    static F: OnceLock<bool> = OnceLock::new();
    *F.get_or_init(|| {
        let on = copa_compete_from_env(
            crate::config::env_flag("RWM_COPA_COMPETE", false),
            copa_wire_active(),
        );
        // LIVENESS ECHO (goal-gate "Gate-Forwarding Audit", 2026-08-09).
        // Two-sided: the "Copa Competitive Mode + Cross-Traffic" battery's
        // arms differ ONLY in this gate, and it composes with copa_wire —
        // so the echo must fire on the OFF arm too, or the control cannot
        // be shown to have been a control.
        tracing::info!(
            copa_compete = on,
            "Copa TCP-competitive mode (RWM_COPA_COMPETE, requires the wire signal)"
        );
        on
    })
}

/// Whether the pool-anchor honest dual-store law is active for this process
/// (`RWM_POOL_ANCHOR`, goal-gate "Ship The Wins 1"): at N ≥ 2 live paths the
/// pooled-store cap's rate input comes from the per-path hygiene-grade
/// SEND-interval anchor ([`crate::control::SendRateAnchor`] — burst-immune
/// by construction, clock-gap discard) instead of the legacy ack-interval
/// windowed-max, whose burst-peak over-read under the est-cadence ack clock
/// was the §16.35 c7 blocker. ONE COMPOSED RESOLUTION: the unset default
/// rides `RWM_EST_CADENCE` (both OFF with everything unset — the measured
/// composed flip REVERTED on its pre-set c7 clause, 2026-08-07; the est=1
/// opt-in turns pool-anchor ON with it), while `RWM_POOL_ANCHOR=0` under
/// the est opt-in is the est-only decomposition arm (the blocker
/// reproduction). Consumers: the per-path send-event feed
/// (`PathState::charge_in_flight`) and the N ≥ 2 dyn-cap law in net/mod.rs.
/// The Copa cwnd feed (`record_delivery`/`on_ack`) is deliberately
/// UNTOUCHED — the measured −22…−27 c7 RS-composition price stays
/// unreachable. Read once and cached (consulted on the send hot path).
pub fn pool_anchor_active() -> bool {
    use std::sync::OnceLock;
    static F: OnceLock<bool> = OnceLock::new();
    *F.get_or_init(|| {
        crate::config::env_flag(
            "RWM_POOL_ANCHOR",
            crate::control::estimator::est_cadence_active(),
        )
    })
}

/// Whether the DELIVERY-CLOCKED pool rate anchor is active for this process
/// (`RWM_POOL_DELIV`, goal-gate "Ship The Wins 1b" arm A): the N ≥ 2 pool
/// law's rate input gains a per-path [`crate::control::DeliveryRateAnchor`]
/// term — the BBR `GenerateRateSample` statistic as a SHADOW estimator no
/// cwnd consumer can read. The law reads
/// `max(delivery_max_bw, send_ratcheted_mean)`: both are honest LOWER BOUNDS
/// on the bottleneck rate, so the max is the estimator (ONE formula, no
/// branch), and the delivery term is the only one that can ratchet ABOVE the
/// cap-limited carried rate — attempt 1's measured binder (paper §16.36:
/// "a send-derived rate cannot ratchet above the cap-limited carried rate").
///
/// Default = the `pool_anchor_active()` resolution (which rides
/// `RWM_EST_CADENCE`), so everything-unset ⇒ OFF and the est opt-in carries
/// it; `RWM_POOL_DELIV=0` under the est opt-in is exactly attempt 1's arm.
/// Read once and cached (consulted on the send hot path).
///
/// REFUTED and REMOVAL-SCHEDULED (ADR-0066 / goal-gate "DEPRECATION REGISTER"
/// → "Batch-2 removal schedule"): the arm failed its ≥0.97 c7 clause on both
/// seeds while the mechanism landed completely, and the anchor's own doc
/// comment certifies it can reach no cwnd/pacing consumer. Activation now
/// warns via [`crate::config::deprecated_env_flag`]; the sampler stays only
/// as the negative datum's reproduction path until the recovery-plane
/// battery the refutation NAMES has run.
pub fn pool_deliv_active() -> bool {
    use std::sync::OnceLock;
    static F: OnceLock<bool> = OnceLock::new();
    *F.get_or_init(|| {
        crate::config::deprecated_env_flag(
            "RWM_POOL_DELIV",
            pool_anchor_active(),
            "Ship The Wins 1b: the delivery-clocked pool anchor (2026-08-07)",
        )
    })
}

/// Whether the honest ANCHOR-FLOOR BOUND is active (`RWM_FLOOR_BOUND`,
/// goal-gate "Ship The Wins 1b" arm B, default OFF — a pure A/B arm).
///
/// The BtlBw anchor floor (`CopaState::anchor_floor` = gain·max_bw·RTprop)
/// rides the LEGACY ack-interval `max_bw`, which over-reads ×10-class under
/// ack bunching (339–500k sym/s measured at c7 under the est clock vs ≈8–12k
/// truth) and inflated cwnd to 5860 vs the prior default's 1779. This bounds
/// the FLOOR — never cwnd itself — by the honest send-anchor rate the engine
/// already measures: `floor := min(legacy_floor, gain·sr·RTprop)`. With the
/// send anchor cold it is the legacy value verbatim, so it can only remove
/// inflation the over-read injected. Its purpose (attempt 1's second named
/// successor): make the prior default's ACCIDENTAL escape — Σcwnd floating
/// the store below the pool — a DERIVED one.
/// REFUTED and REMOVAL-SCHEDULED (ADR-0066 / goal-gate "DEPRECATION REGISTER"
/// → "Batch-2 removal schedule"): the bound cut the c7 over-read exactly as
/// designed and failed BOTH clauses — c7 0.969/0.969×Σ and c1 396.4/398.0
/// under the 430 PRIMARY (−14% vs unbounded). The refutation is a positive
/// structural finding: the ack-interval over-read is LOAD-BEARING at N = 1.
/// Activation warns via [`crate::config::deprecated_env_flag`].
pub fn floor_bound_active() -> bool {
    use std::sync::OnceLock;
    static F: OnceLock<bool> = OnceLock::new();
    *F.get_or_init(|| {
        crate::config::deprecated_env_flag(
            "RWM_FLOOR_BOUND",
            false,
            "Ship The Wins 1b (2026-08-07)",
        )
    })
}

/// Whether the O(1) windowed-max rate filter is active for this process
/// (`RWM_HONEST_ANCHOR`, goal-gate "Honest Inputs" — anchor-hygiene family
/// member, **DEFAULT ON since 2026-08-11** per the flip battery's F7 and
/// paper §16.51; `=0` is the re-runnable legacy-fold A/B arm, and the
/// `RWM_ANCHOR_HYGIENE` umbrella still overrides in either direction).
///
/// THE MECHANISM IT REPAIRS (measured, not argued): `CopaState`'s BtlBw
/// windowed max (`max_bw`) is recomputed by a FULL-WINDOW FOLD over
/// `bw_samples` on every accepted sample. Fed per-ACK (the legacy
/// `record_delivery`) the fold is invisible; fed PER DELIVERED SOURCE
/// SYMBOL (`rs_on_delivered` under `RWM_PLAIN_RS`) it is O(window·rate)
/// work per second of transfer — a hidden O(n²), and the EXACT defect the
/// `rtt_samples` min-deque already fixed for min_rtt (see `record_rtt`'s
/// monotonic-deque comment: "~42% sender CPU ... MEASURED by perf"). The
/// latency-lever battery's CPU gauge convicts it at c1: `RWM_PLAIN_RS=1`
/// alone inflates sender CPU per delivered byte by +61…64% (CPUCLI
/// 15.0–16.6 s → 24.2–25.4 s for the same 400 MB, 16/16 reps, both seeds)
/// on a sender already at its ~1-core ceiling — which is the whole
/// −35% / D/A 0.64, and why the tax is rate-dependent (fold length ∝ rate)
/// and anti-correlated with store binding (it is not a store effect at
/// all).
///
/// ON ⇒ `max_bw` is read off a monotonic max-deque maintained beside
/// `bw_samples` — the SAME statistic to the bit (front of the deque ==
/// the fold; unit-pinned by `bw_mono_front_equals_full_window_fold`), the
/// same [1 s, 10 s] window, the same evictions, amortized O(1) per sample.
/// ZERO constants: nothing is sampled, subsetted, decayed or approximated.
/// OFF ⇒ the fold runs verbatim (value-identical either way; the gate
/// selects COST, not behavior). Read once and cached (consulted at
/// CopaState construction).
///
/// **DEFAULT ON since 2026-08-11** (goal-gate "Honest Inputs — FLIP
/// BATTERY", falsifier F7 swept: goodput within 2σ at every cell/seed,
/// CPU/byte 0.90–1.03×; value-identical by the unit-pinned equivalence, so
/// any behavioral movement is an instrument alarm, not a result). The
/// legacy fold remains reachable as `RWM_HONEST_ANCHOR=0` — the A/B arm
/// stays re-runnable per the deprecation register.
pub fn honest_anchor_active() -> bool {
    use std::sync::OnceLock;
    static F: OnceLock<bool> = OnceLock::new();
    *F.get_or_init(|| crate::config::anchor_gate_default("RWM_HONEST_ANCHOR", true))
}

/// **`RWM_COLD_PLACE`** (anchor-hygiene family member, default OFF) — hygiene
/// rule 1 at the PLACEMENT site: an unmeasured leg's latency anchor is seeded
/// from MEASUREMENT, not from the 50-ms constant.
///
/// THE DEFECT IT REPAIRS: `place_costs`' load term reads
/// `PathState::srtt()`, which for a leg that has never had an RTT sample is
/// `estimator.rtt()` — still the 50-ms `DEFAULT_SRTT`-class constructor seed.
/// That prices a COLD leg's one-way propagation at 25 ms against a warm c2
/// leg's 4 ms, so the incumbents must reach `in_flight/cwnd ≈ 2.6` before the
/// cold leg can win the argmin. It draws nothing, so it takes no sample, so
/// it stays cold: a FIXED POINT of the estimator.
///
/// **WHERE IT BINDS, AND THE RETRACTION THAT ESTABLISHED THAT.** This was
/// first claimed at the SF bench's `c7x4` symmetric quad, and that claim is
/// RETRACTED (goal-gate "The Quad's Cold-Start Placement Lock-In —
/// RETRACTED", 2026-08-18): the quad's per-path gauges were truncated at
/// `pid < 2`, so the assertion that "measured" the lock-in could not fail,
/// and the quad in fact spreads evenly over all four legs. The reason is
/// mechanical and worth stating, because it bounds this gate's whole scope:
/// when every leg starts cold TOGETHER, the first admission burst runs before
/// any ack returns, all legs tie at the seed price, the `in_flight` term
/// round-robins them, and one RTT later they are all warm — the cold price
/// never gets a cold-vs-warm contrast to express.
///
/// The fixed point therefore forms only where a leg joins a set whose
/// incumbents are ALREADY warm — a LATE JOIN (path migration, a second
/// interface coming up mid-transfer). No SF-bench geometry and no L1 cell has
/// one, so this gate is bounded by
/// `a_late_joining_leg_is_locked_out_by_the_cold_price_and_admitted_without_it`
/// at synthetic states, and measured INERT at every bench cell by
/// `the_cold_start_placement_price_is_inert_wherever_every_leg_starts_cold`.
/// That is why it ships OFF and why no flip is recommended: the only regime
/// it changes has never been measured on a wire.
///
/// THE REPAIR, and why it costs no constant: the cold leg is priced at the
/// path set's own FASTEST MEASURED srtt. The price is another leg's
/// measurement, not a number — the same move `RWM_MSTAR_ANCHOR` makes inside
/// `LossEstimator::record_rtt` (seed from the first sample) and
/// `RWM_HONEST_K` makes for K (`k_raw.unwrap_or(legacy)`): ONE formula, the
/// gate only changes WHICH measurement seeds the unmeasured anchor. It is
/// the standard optimistic-exploration argument stated in the placement
/// objective's own units — exploration is free until measurement says
/// otherwise — and it is SELF-LIMITING without a threshold, because the
/// cold leg's `in_flight/cwnd` term starts charging the moment it is placed
/// on. No `if cold` beyond the `Option::None` the estimator already has, no
/// dial threshold, no round-robin counter.
///
/// OFF is bit-identical by construction: with the gate off the cold price IS
/// `p.srtt()`, i.e. the shipped expression verbatim at every leg.
pub fn cold_place_active() -> bool {
    use std::sync::OnceLock;
    static F: OnceLock<bool> = OnceLock::new();
    *F.get_or_init(|| {
        let on = crate::config::anchor_gate("RWM_COLD_PLACE");
        // LIVENESS ECHO (MEASUREMENT DISCIPLINE item 1/15), two-sided: it
        // prints the OFF value too, so "gate absent" is as checkable as
        // "gate present". Resolved once and cached.
        tracing::info!(
            cold_place = on,
            "cold-start placement price (RWM_COLD_PLACE, anchor-hygiene rule 1): \
             an unmeasured leg's SRTT_i in the §16.3 cost is the active set's \
             fastest MEASURED srtt when ON, the 50-ms DEFAULT_SRTT-class seed \
             when OFF (shipped, bit-identical)"
        );
        on
    })
}

/// Whether the RAW-sample echo-ratio floor is active for this process
/// (`RWM_HONEST_K`, goal-gate "Honest Inputs" — anchor-hygiene family
/// member, default OFF; `RWM_ANCHOR_HYGIENE=1` turns the family on).
///
/// THE MECHANISM IT REPAIRS: K_i (`EchoRatioMin`, the honest caps' and the
/// three-term law's residence-clock ratio) is documented as "the smallest
/// OBSERVED echoSRTT/RTprop" but is fed the SMOOTHED srtt series sampled at
/// the 5 ms dyn-cap refresh clock. The minimum of a smoothed series sits
/// near the MEAN of the underlying distribution, not its floor — the EWMA
/// (α = 1/8) filters out exactly the low excursions a windowed MIN exists
/// to catch — so K READS HIGH wherever the delay distribution is wide:
/// jit25's `[3T]` window term measured ×1.34/1.38 its pre-registered value,
/// the INVERSE of the pre-registered "min reads the low end" direction
/// (goal-gate "Latency Lever — BATTERY", banked as an `EchoRatioMin`
/// finding). RTprop, by contrast, is already the min over RAW samples —
/// the current K is min(smoothed)/min(raw), a statistic that rises with
/// jitter width by construction.
///
/// ON ⇒ the SAME `EchoRatioMin` tracker (same `PERCAP_K_HALF_WINDOW_US`
/// window, same ≥ 1 clamp, same seed-identity guard) is fed the RAW
/// per-sample echo/RTprop ratio at the SAMPLE clock (`record_rtt`), and
/// every K consumer reads that tracker's min — min(raw)/min(raw), the
/// floor the derivation assumed. ZERO constants: the fix changes which
/// measured series feeds the unchanged statistic. OFF ⇒ the smoothed
/// refresh-clock feed runs verbatim. Read once and cached (consulted at
/// CopaState construction).
pub fn honest_k_active() -> bool {
    use std::sync::OnceLock;
    static F: OnceLock<bool> = OnceLock::new();
    *F.get_or_init(|| crate::config::anchor_gate("RWM_HONEST_K"))
}

/// Whether the WINDOW-mode control-datagram MERGE is active for this process
/// (`RWM_ACK_MERGE`, goal-gate "Unlock The Default 1: ack-merge" →
/// "Ack-Merge Flip"; **default ON since 2026-08-08** — `RWM_ACK_MERGE=0` is
/// the opt-out A/B arm).
///
/// The receiver emits up to TWO control datagrams per data message: the SACK
/// `WindowAck` from the window arm, and the legacy per-batch
/// `ControlMessage::Ack` whose send site sits AFTER the window/block branch
/// and therefore fires in window mode too (the recorded code-fact correction
/// at `net/mod.rs`'s Ack arm). quinn-perf sends ~1 ack per ~24 packets.
///
/// **How much of a duplicate it is depends on the CELL, and that is the whole
/// measured story (§16.42).** The `Ack` fires once per symbol batch
/// unconditionally; the `WindowAck` it duplicates fires on FRONTIER ADVANCE.
/// So on a clean single path, where the in-order frontier advances on
/// essentially every batch, the two coincide and the receiver really does
/// send ≈2.0 control datagrams per data message — **measured 1.96 at c1**.
/// Under dual-path striping with GE loss the frontier advances in jumps of
/// ~20–25 seqs, the `WindowAck` rate collapses, and the "duplicate" is
/// ≈4% of the traffic — **measured 1.05 at c7**. §16.39 measured only the
/// dual cell and concluded the premise was refuted; it was refuted THERE and
/// exactly right at the clean cell.
///
/// ON ⇒ 1.000 per data message everywhere, and the goodput/CPU response
/// tracks the density REMOVED, cell by cell: c1 (1.96 → 1.00) +12.7% / +13.0%
/// on the two seeds with receiver CPU per bit −9.1% / −8.4%; c7 (1.05 → 1.00)
/// −0.7% / −0.2% with receiver CPU flat, i.e. within σ of its own control.
///
/// ON ⇒ in WINDOW MODE ONLY the legacy `Ack` is suppressed, the `WindowAck`
/// becomes unconditional (one per data message — exactly the cadence the
/// `Ack` had) and carries the `Ack`'s payload in its v6 cumulative counters,
/// and every consumer of the `Ack` arm is re-homed onto the counter DIFF.
/// BLOCK MODE IS BIT-EXACT: it keeps the legacy `Ack` in full, and
/// `block_arq` is already `None` in window mode so the dup-ack loss channel
/// is structurally out of scope.
///
/// **This gate changes the DATAGRAM COUNT and nothing else.** The delivery
/// statistic (`record_delivery`'s ack-interval windowed max), its cadence,
/// its counts and its consumers are all preserved — deliberately, because
/// with no `CopaFeed` constructed (the shipped default and every arm of the
/// ack-merge battery) that estimator IS the window-mode anchor, and removing
/// it is the measured catastrophic trap recorded at the Ack arm
/// (`max_bw = 0` ⇒ the anchor floor never establishes ⇒ the dynamic store cap
/// sticks at boot 128). Replacing the anchor is a DIFFERENT experiment; three
/// rate sources have already been measured against it (§16.35/§16.36/§16.37)
/// and the c7 ordering did not track anchor honesty.
///
/// Not a dial: it selects no law and no constructor argument on (δ, ρ, r),
/// and nothing keys on a threshold in the triangle (CLAUDE.md's
/// no-mode-switch invariant). The machine is bit-identical under both
/// settings; only the number of control frames differs.
pub fn ack_merge_active() -> bool {
    use std::sync::OnceLock;
    static F: OnceLock<bool> = OnceLock::new();
    *F.get_or_init(|| crate::config::env_flag("RWM_ACK_MERGE", true))
}

/// `RWM_LOSS_SENT_TRUTH` (**default OFF**) — feed the per-path loss estimator
/// the SENDER's own `symbols_sent` delta instead of the receiver's
/// gap-derived `total_expected`. The law, its provenance and its named
/// residual are on [`PathState::sender_truth_loss_delta`]; the defect it
/// removes is documented at the `PathBatchTracker` design note
/// (`net/mod.rs` header item (2)) and measured in goal-gate "Ack-Cadence
/// Measurement (VM)" READOUT 4.
///
/// **Behaviour-changing, hence gated.** The estimate feeds the NACK repair
/// margin (`net/mod.rs:6867`), the NACK congestion multiplier and budget cap
/// (`:6384`/`:6432`), the block-ARQ margins via `worst_loss_rate`
/// (`:7613`), the interleaver taper decay (`:7344`), the shed budget
/// (`emit_source.rs:682`, `receiver.rs:767`/`:1398`) and every placement /
/// scheduling cost that carries an `eps` term (`scheduler/mod.rs:2212`,
/// `:2229`, `:2256`, `:2266`, `:3111`). N = 1 is UNAFFECTED in shape — a
/// single path's batch-seq stream has no other path in it, so the legacy
/// pair is already honest there and this gate only removes its ~1 BDP of
/// startup lag.
///
/// **Not the refuted `RWM_RECOV_MP_SERIAL`.** That build gave each path its
/// own batch-seq NAMESPACE on the WIRE (sender-side, protocol-visible) and
/// was runtime-refuted on the clean substrate (dual-c1 181 → 134, sender CPU
/// x2.4 — goal-gate "Multipath Recovery Suppression", DEPRECATION REGISTER).
/// This changes NO wire format and adds no sender work: both operands
/// already exist and already ride the existing v6 counters. The refutation's
/// mechanism — honest loss re-heating every SRTT/loss-scaled recovery
/// cadence that the poisoned values were accidentally damping — applies to
/// ANY honest-loss build and is exactly why this one ships OFF pending the
/// named cadence re-derivation.
///
/// Not a dial: it selects no law on (delta, rho, r) and nothing keys on a
/// threshold in the triangle (CLAUDE.md's no-mode-switch invariant). It
/// changes which MEASUREMENT feeds one estimator; the laws downstream are
/// the same laws, evaluated at an honest argument.
pub fn loss_sent_truth_active() -> bool {
    use std::sync::OnceLock;
    static F: OnceLock<bool> = OnceLock::new();
    *F.get_or_init(|| crate::config::env_flag("RWM_LOSS_SENT_TRUTH", false))
}

/// `RWM_RELEASE_1TO1` (**default OFF**) — MAKE THE RELEASE 1:1 WITH THE
/// CHARGE. One gate, one quantity: **what releases a LOST symbol's budget
/// slot.**
///
/// Today the answer is two mechanisms, and the first of them is contaminated:
///
/// 1. `control_msg.rs` releases `expected_count - received_count` in BOTH ack
///    arms, where `expected` is `PathBatchTracker`'s GLOBAL-`batch_seq` gap
///    estimate `gap x received` (`net/mod.rs`'s `PathBatchTracker::
///    record_batch`). At N >= 2 that gap is a SCHEDULING artefact — mostly the
///    OTHER path's symbols — so the release is inflated by the same 37-93x the
///    loss estimate was (goal-gate "Cross-Path Loss Contamination" READOUT 4:
///    `ce/cr` 2.05 at c7, 5.59 on c8's slow leg = **~1 and ~5 EXTRA slots
///    released per delivered symbol**). `release_in_flight` saturates at zero,
///    so the excess is spent, not stored: the gauge does not merely
///    mis-report, it **leaks OPEN**. Measured on the deterministic two-path
///    model, the gauge reads `in_flight == 0` on **> 90%** of acks at which
///    the path genuinely has symbols outstanding, which holds
///    `available() = cwnd - in_flight` wide open on evidence the path does not
///    have.
/// 2. [`PathState::expire_in_flight`], a time-based sweep of the charge log
///    itself. This one IS 1:1 by construction — it pops the very entries
///    `charge_in_flight` pushed — but its horizon is
///    `max(4 x SRTT, 250 ms)`, roughly an order of magnitude past the RTT
///    scale at which a symbol's fate is actually decided, so on the shipped
///    path it is a backstop and (1) is the operative release.
///
/// **Under the gate, (1) is DELETED and (2) becomes the whole answer, at the
/// scale the engine already uses to decide a symbol IS lost:** RFC 9002
/// §6.1.2's kTimeThreshold, `9/8 x SRTT`, floored at the same kGranularity
/// analog the recovery plane's own time threshold is floored at
/// (`net::mp_time_threshold_split`, `net::NACK_RETX_COOLDOWN_FLOOR_US`).
/// **No constant is introduced** — 9/8 and the floor are both already in the
/// tree, cited from the same RFC clause, and used for exactly this judgement
/// on the recovery plane.
///
/// THE LAW, on one line:
///
/// ```text
///   released(t)  =  delivered(t)  +  charges older than 9/8 x SRTT
/// ```
///
/// Both terms pop the SAME `in_flight_log` the charge pushed, so the ledger is
/// 1:1 by construction and cannot over-release however the paths are striped.
///
/// **WHY NOT the sender-truth pair**, which is the shape the dispatch that
/// opened this branch proposed and which
/// [`PathState::sender_truth_release_delta`] implements as the recorded
/// negative datum: it is refuted ARITHMETICALLY, not statistically. Charging
/// every send and releasing `d_received` plus `d_sent - d_received` telescopes
/// to `in_flight == outstanding_at_cursor_init`, a CONSTANT — with the cursors
/// starting at zero that constant is zero, so the gauge is pinned on the floor
/// exactly as the contaminated delta pins it. The reason is structural:
/// `d_sent - d_received` is `loss + delta(outstanding)`, so releasing on it
/// releases the in-flight window itself. Item 3's trick works for a RATIO and
/// does not transfer to a LEDGER, which needs the per-symbol identity.
/// Reproduced and bounded by
/// `sender_truth_release_pins_the_gauge_on_the_floor`.
///
/// **Composition with [`charge_recovery_active`].** This gate makes releases
/// 1:1 with CHARGES; that one makes charges equal the TRUE WIRE. Both are
/// needed for `in_flight` to be the wire's occupancy, and each is separately
/// meaningful, so they are separate gates and a battery can attribute.
///
/// Not a dial: it selects no law on (delta, rho, r) and keys on no threshold
/// in the triangle (CLAUDE.md's no-mode-switch invariant).
pub fn release_1to1_active() -> bool {
    use std::sync::OnceLock;
    static F: OnceLock<bool> = OnceLock::new();
    *F.get_or_init(|| crate::config::env_flag("RWM_RELEASE_1TO1", false))
}

/// `RWM_CHARGE_RECOVERY` (**default OFF**) — METER THE TWO RECOVERY CHANNELS
/// THAT ARE NOT METERED.
///
/// The SACK-gap retransmit (`net/mod.rs`, "SACK-gap retransmit") and the NACK
/// repair margin (`net/mod.rs`, "NACK repair margin") each build a
/// `SymbolBatch` and call `transport.send_symbols` with **no
/// `charge_in_flight`, no `consume_pace_tokens`, and no
/// `PathStats::symbols_sent` increment** anywhere on the path. Every OTHER
/// wire channel meters all three at the handoff — the source arm
/// (`emit_source.rs`), the taper correction (`emit_source.rs`), the three
/// generation-coding arms (`net/mod.rs`) and, most directly, the block-ARQ
/// repair batch, whose own comment states the norm this gate restores:
/// *"Charge like any correction: in_flight budget … + pacing tokens"*.
///
/// **The exemption that IS on the record is a different one.** Recovery is
/// deliberately exempt from the ACK-CLOCKED ADMISSION TARGET (deadlock
/// otherwise), and the reactive generation arm states its own position on the
/// congestion question explicitly — *"Recovery is NON-EXEMPT from
/// `cwnd_full`"*. No record anywhere in the tree exempts these two channels
/// from the in-flight ledger, the pacer or the sender's own wire count; the
/// provenance audit found none. Charging cannot deadlock them either, because
/// **neither send site reads `available()` or `cwnd_full`** — they are budgeted
/// by `cached_nack_budget` and the NACK congestion multiplier. The charge
/// therefore makes the SOURCE arm see the occupancy recovery created, which is
/// the whole purpose of the gauge, without gating recovery on it.
///
/// **One gate, one quantity: "are these two channels metered?"** The three
/// meters move together on purpose — they are one act at the peer site
/// (block-ARQ repair charges in_flight, pace tokens and `symbols_sent` in one
/// block), and splitting them would assert an accounting the engine has
/// nowhere else. A battery cannot attribute AMONG the three; that is stated as
/// a listed wire question rather than papered over.
///
/// Not a dial (CLAUDE.md's no-mode-switch invariant): no law on (delta, rho,
/// r) is selected and no threshold is keyed. It adds two counter increments on
/// a path that already exists.
pub fn charge_recovery_active() -> bool {
    use std::sync::OnceLock;
    static F: OnceLock<bool> = OnceLock::new();
    *F.get_or_init(|| crate::config::env_flag("RWM_CHARGE_RECOVERY", false))
}

/// `RWM_PATIENCE_DERIVED` (default OFF) — goal-gate "Unlock The Default 2:
/// derived patience". Replaces the `NACK_RETX_COOLDOWN_FLOOR_US` = 10 ms
/// literal at its two BEHAVIOURAL sites (the RFC 9002 §6.1.2 kGranularity
/// analog inside `mp_time_threshold_split`, and the per-seq retransmit
/// cooldown) with `net::patience_floor_us` = timer granularity + the path's
/// own measured RTT jitter. RFC 9002's 9/8 and packet-threshold 3 untouched.
/// Cached so every read site resolves identically within a process.
///
/// Not a dial: it selects no law and no constructor argument on (δ, ρ, r).
/// REFUTED and REMOVAL-SCHEDULED (ADR-0066 / goal-gate "DEPRECATION REGISTER"
/// → "Batch-2 removal schedule"): eliminated on BOTH population and response
/// — the literal it replaces wins 0 of 177 543 §6.1.2 evaluations at c7, and
/// where it does bind (c1) collapsing it moves nothing ≫σ. The goal closed as
/// a documented STRUCTURAL BOUND. Activation warns via
/// [`crate::config::deprecated_env_flag`]. NOTE: the schedule deletes the LAW
/// only — the `pf=<floor>/<clock>/<mean>` gauge is explicitly excluded, the
/// named successor (a store-dwell-inclusive recovery RTT) needs it verbatim.
pub fn patience_derived_active() -> bool {
    use std::sync::OnceLock;
    static F: OnceLock<bool> = OnceLock::new();
    *F.get_or_init(|| {
        crate::config::deprecated_env_flag(
            "RWM_PATIENCE_DERIVED",
            false,
            "Unlock The Default 2: derived patience (2026-08-07)",
        )
    })
}

/// `RWM_SIDLE_DERIVED` (default OFF) — goal-gate "Unlock The Default 2".
/// DIAG-ONLY and behaviour-inert: the legacy `sidle=`/`[WIDLE] idle=` fields
/// are printed UNCHANGED; this gate adds a SECOND field (`sidle2=`,
/// `idle2=`) computed by `net::stall_threshold_us` over the same event
/// stream, so the fixed-3 ms-threshold artifact question is answered on the
/// SAME runs in every arm, controls included.
/// Deliberately NOT wired to `deprecated_env_flag` even though it was built
/// and closed in the same session as its three deprecated mates: this is an
/// INSTRUMENT whose verdict is a STANDING INSTRUCTION the register issues to
/// future sessions (*where `evt ≫ LOOP_WAKE_US`, read `sidle2`, not
/// `sidle`*). Warning "deprecated, removal scheduled" on a gauge the ledger
/// tells you to switch on would contradict the register. See goal-gate
/// "Batch-2 removal schedule".
pub fn sidle_derived_active() -> bool {
    use std::sync::OnceLock;
    static F: OnceLock<bool> = OnceLock::new();
    *F.get_or_init(|| crate::config::env_flag("RWM_SIDLE_DERIVED", false))
}

/// Floor on the queuing-delay estimate dq, in seconds (0.1 ms).
///
/// Two jobs, both continuity guards (no branch cliffs):
///   - `copa_target_cwnd()` divides by dq; on a LAN where a sample can equal
///     the floor exactly, dq → 0 would explode the target to infinity.
///   - The backoff threshold (queue_mult − 1) × floor collapses toward 0 on
///     sub-millisecond-RTT links; flooring both dq and the threshold at the
///     same 0.1 ms means jitter at the clamp boundary cannot trigger a
///     spurious backoff (dq == threshold is not > threshold).
const DQ_FLOOR_SECS: f64 = 1e-4;

/// Jitter headroom multiplier k in the backoff threshold
/// (queue_mult − 1) × floor + k × jitter_est (paper Section 12.4,
/// jitter-adjusted queue target).
///
/// The P1 mapping assumed path jitter ≪ the queue target. Real links
/// violate that: at L1's C2 cell (10ms floor, ±3ms/direction netem
/// jitter) the Bulk threshold was 2.5ms while a typical RTT sample sat
/// ~6ms above the 10s floor — the windowed-min queue signal measured
/// JITTER, not queue, and every per-SRTT update bought a ×0.92 backoff
/// (cwnd pinned near the floor; measured L1 root cause of the 16x
/// rp-vs-quinn gap at C2). Widening the threshold by k×jitter makes the
/// comparison read "queue above target AND above what jitter alone
/// explains". k = 2 puts the false-backoff rate for a min-of-N window
/// at the few-percent level for the N ≈ 4-30 ACK batches an SRTT holds,
/// while a genuine standing queue (which shifts ALL samples, leaving
/// the consecutive-difference jitter estimate unchanged) still crosses
/// the widened threshold within a few updates. Continuity: jitter → 0
/// recovers the P1 threshold exactly.
const JITTER_HEADROOM: f64 = 2.0;
/// EWMA gain for the consecutive-difference jitter estimator (RFC
/// 3550-style interarrival jitter, gain 1/8 rather than 1/16: the ramp
/// fast-exit consults the threshold from the first ACKs on, so the
/// estimate must converge within tens of samples).
const JITTER_GAIN: f64 = 0.125;

/// EWMA gain for the `rvar_us=` CANDIDATE DISPERSION GAUGE — **RFC 6298 §2's
/// own `β`, and its provenance is the RFC.**
///
/// `RTTVAR ← (1 − β)·RTTVAR + β·|SRTT − R'|`, β = 1/4, verbatim. The same
/// constant RFC 8985 §6.2 inherits for RACK. **CITED, never fitted** — this is
/// the constant the CLAUDE.md FORMULA-FIRST rule asks for a reference for, and
/// the reference is the standard the shipped `rtt_var_sq` EWMA already cites
/// for the identical gain on the SECOND moment.
const SIGMA_CAND_RVAR_GAIN: f64 = 0.25;

/// Window length `L` for the two WINDOW-CLASS candidate dispersion gauges
/// (`qsp_us=`, `msd_us=`) — the count of most-recent raw RTT samples held.
///
/// **Why a window at all, and why 256.** The shipped `sig_us` EWMA at β = 1/4
/// has an effective memory of `N_eff = (2 − β)/β = 7 samples`. It is a
/// SEVEN-SAMPLE estimate no matter what its `n` reads, which is precisely why
/// "converged at `n` ≈ 18 000" did not mean converged: the plain-window
/// primitives measured `σ(c8)` at 0.191 / 3.140 / 54.836 ms across three reps
/// at that same `n` (goal-gate, plain-window scored result §4 — a 287× spread
/// that survived two sessions because the `n` column looked converged). `n`
/// counts how long the gauge has been FED; it does not count what is IN the
/// reading.
///
/// 256 is `L` for two reasons, both stated before any candidate was measured:
/// it is **36× the EWMA's memory**, so the memory axis is separated from the
/// functional axis by more than an order of magnitude; and the `P90` these
/// gauges take needs its tail to rest on real order statistics —
/// `L·(1 − 0.90) = 25.6` clears the standard ≥ 10 requirement by 2.6×. It is
/// also 1.4 % of `c8`'s per-rep sample budget, so a window-class `n_warm = L`
/// clears the pre-registered `C2` bar (`n_warm ≤ 883` at `c8`) by 3.4×.
///
/// **Resource bound, stated OUTSIDE the law** (FORMULA-FIRST): 256 × 4 B =
/// 1 KiB per path, and the sort that reads it is `O(L log L)` at the `[DIAG]`
/// cadence only — the feed site stays `O(1)`.
const SIGMA_CAND_WINDOW: usize = 256;

/// Quantile of the per-update window-min history used as the QUEUE floor
/// (paper Section 12.4, jitter-robust queue floor).
///
/// The queuing-delay signal compares a min-of-N statistic (N ≈ the ACK
/// samples in one SRTT window) against the propagation floor, a
/// min-of-thousands over 10s. On a jittery link those are DIFFERENT
/// statistics: at L1's C2 cell the 10s floor found 7.0ms while a
/// typical window min sits at 12-13ms — a permanent apparent dq of
/// ~5ms with an empty queue (and netem's jitter FIFO correlates
/// consecutive samples, so the consecutive-difference jitter estimate
/// ~0.85ms cannot bridge the gap). Comparing the window min against a
/// low QUANTILE of its own recent distribution is self-calibrating
/// under any jitter correlation structure: queue-free windows sit near
/// their own P10 by construction, while a genuine standing queue
/// shifts every window min up within one SRTT and the 10s-window
/// quantile lags behind — the signal survives. On a clean link every
/// window min equals the floor, the quantile equals the floor, and
/// the P1 semantics are recovered exactly.
const QUEUE_FLOOR_QUANTILE: f64 = 0.10;

/// Startup ramp: multiplicative growth factor per window update, until the
/// first backoff (gate driver P1: cwnd = cwnd × 1.5 + 1).
const RAMP_GAIN: f64 = 1.5;
/// Steady state: additive increase per window update (symbols).
const ADDITIVE_STEP: f64 = 2.0;
/// Backoff: multiplicative decrease when the windowed min RTT exceeds the
/// hint-coupled queue target.
const BACKOFF_MULT: f64 = 0.92;
/// SRTT assumed before the first RTT sample arrives (update cadence only).
const DEFAULT_SRTT: Duration = Duration::from_millis(50);

// --- BtlBw-anchored recovery (paper Section 12.6) ---
//
// The additive +2/SRTT recovery after a delay backoff crawls: from a
// ×0.92 trough it takes dozens of SRTTs to re-fill the pipe, so cwnd sits
// well below BDP (measured L1 C2: p50 ~80-110 symbols vs BDP ~160). We
// already maintain a delivery-rate max-filter (`max_bw`) and a 10s min-RTT
// (`min_rtt`); their product is a BtlBw×RTprop = BDP estimate. Use it to
// (a) pull post-backoff recovery TOWARD BDP proportionally (decaying to
// the gentle +2 probe as cwnd → BDP) and (b) floor cwnd at the estimate so
// a backoff (or a jitter false-positive) cannot crawl cwnd below the pipe.
//
// CRITICAL: `max_bw` is a windowed MAX of COARSE ACK-batch delivery rates
// — no per-packet sampling and no app-limited detection (BBR discards
// app-limited samples precisely because they underestimate BtlBw). For a
// warm-up-limited transfer (the dominant 1.8MB regime) the estimate reads
// LOW exactly when we would want it high. So the anchor is used ONLY to
// RAISE cwnd — a recovery target and a floor, never a cap. A stale/under-
// estimated BtlBw can then only fail to help; it can never suppress cwnd.

/// Minimum delivery-rate samples in the 10s window before the BtlBw anchor
/// is trusted. A handful of coarse samples is too noisy to floor cwnd on.
///
/// PUBLIC because it is now LOAD-BEARING OUTSIDE the scheduler: the store-cap
/// bootstrap floor is DERIVED from it (`net::sender_policy::STORE_CAP_FLOOR`,
/// paper §16.59) rather than being the bare `64` ADR-0070 finding 5 recorded
/// as PROVENANCE ABSENT. The floor's job is to keep enough outstanding that
/// this gate can close; the number of samples the gate wants is this constant,
/// and the derivation cites it instead of restating it.
pub const ANCHOR_MIN_SAMPLES: usize = 8;
/// cwnd_gain on the BtlBw×RTprop BDP estimate for the post-backoff recovery
/// TARGET. 1.0 = aim to re-fill exactly the pipe; the gentle +2 probe (and
/// the hint-coupled queue target) still governs the standing queue ABOVE
/// BDP, so this is not BBR's cwnd_gain=2 (which deliberately buffers 1×BDP).
const ANCHOR_RECOVERY_GAIN: f64 = 1.0;
/// Proportional pull toward the recovery target per SRTT update: the
/// increment is max(ADDITIVE_STEP, α·(target − cwnd)). Continuous and
/// self-decaying — at α=0.25 a trough at 0.5×BDP closes ~90% of the gap in
/// ~8 SRTTs (vs ~40 SRTTs for +2), and the term vanishes into +2 as
/// cwnd → target (no discrete phase, no cliff).
const ANCHOR_PULL_ALPHA: f64 = 0.25;
/// cwnd floor as a multiple of the BtlBw×RTprop estimate. cwnd is never
/// driven below this once the anchor is established (floor, NOT cap).
///
/// 0.85, not 1.0: a floor AT the full BDP estimate pins cwnd there even
/// when the delay signal reports queue-above-target — the L1 C2 cwnd trace
/// showed `above=true` on nearly every update with cwnd held exactly at
/// bdp_anchor, i.e. the floor was maintaining a ~16 ms standing queue the
/// backoff could no longer drain. Flooring at 0.85×BDP keeps cwnd off the
/// 8-symbol collapse (the measured deficiency) while leaving the delay
/// backoff ~15% of authority around BDP to drain a genuine queue; the
/// recovery pull (gain 1.0) still re-fills toward full BDP each clean
/// update, so cwnd oscillates just under the pipe rather than sitting in
/// standing bufferbloat. Because `max_bw` also underestimates during
/// warm-up, the realized floor sits further below true BDP — the safety
/// (see the risk note above and Section 12.6).
const ANCHOR_FLOOR_GAIN: f64 = 0.85;

/// Floor on the in_flight expiry horizon (see `PathState::expire_in_flight`).
/// max(4×SRTT, this): stranded budget (lost best-effort ACK datagrams)
/// releases within ~a quarter second instead of jamming the TUN gate until
/// the 2s leak-guard decay.
const IN_FLIGHT_EXPIRY_MIN: Duration = Duration::from_millis(250);

/// Hint-coupled queue-target multiplier (P1, paper Section 12.4): the
/// standing queue is allowed to raise the windowed min RTT to
/// floor × mult before Copa-lite backs off. Realtime keeps the queue
/// near-empty; Bulk trades a deeper queue for utilization.
fn queue_target_mult(hint: ProtocolHint) -> f64 {
    match hint {
        ProtocolHint::Realtime => 1.08,
        ProtocolHint::Auto => 1.125,
        ProtocolHint::Bulk => 1.25,
    }
}

/// Scheduling weights derived from protocol hint.
/// RWM placement (paper §16.3) softmax temperature.
///
/// The placement cost is measured in units of the FASTEST path's SRTT (the
/// load term is `E_i(load)/ref_srtt`, ≈ 0.5 for the idle fast path). `T` is
/// therefore the softness of the water-filling transition in units of a fast
/// one-way delay: two paths whose costs differ by `T` place at odds e:1 ≈
/// 2.7:1. `T → 0` is the paper's strict best-path (argmin) limit; larger `T`
/// dithers and pulls more traffic onto a slower path (more aggregation, more
/// head-of-line risk on a reliable in-order stream). This is the one dial
/// §16.3 names as a documented constant; L1 measurement tunes it.
pub(crate) const PLACE_TEMPERATURE: f64 = 0.15;

/// The effective placement temperature: `PLACE_TEMPERATURE`, overridable once
/// per process via the `RWM_PLACE_T` env var (the §16.3 dial exposed for L1
/// tuning without a rebuild). Read once and cached.
fn place_temperature() -> f64 {
    use std::sync::OnceLock;
    static T: OnceLock<f64> = OnceLock::new();
    *T.get_or_init(|| {
        std::env::var("RWM_PLACE_T")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|t| *t > 0.0 && t.is_finite())
            .unwrap_or(PLACE_TEMPERATURE)
    })
}

/// Floor (seconds) for the SRTT reference that de-dimensionalises the
/// propagation-preference term — a div-by-zero guard for the pre-first-sample
/// window, NOT a tuning knob (any positive value cancels once real RTTs land).
pub(crate) const PLACE_REF_FLOOR_SECS: f64 = 0.001;

/// Recovery-patience bound on the frontier-slack placement deadline
/// (goal-gate "C8 Slow-Path Conversion"): D_i = min(S, 9/8·srtt_i). 9/8 is
/// RFC 9002's kTimeThreshold — the SAME constant the `RWM_RECOV_MP` hole
/// law's `mp_time_threshold_split` uses — NOT a new tuning dial: a placement
/// later than the hole law's patience is re-served cross-path no matter
/// what the frontier needs, so the placement plane must never budget past
/// it (the 2026-08-06 smoke falsification of the unbounded-S form).
pub(crate) const PLACE_SLACK_RECOV_PATIENCE: f64 = 1.125;

/// Controls the latency vs bandwidth trade-off in the interpolated objective.
/// See paper Section 13.8.
#[derive(Debug, Clone, Copy)]
pub struct SchedulingWeights {
    /// Weight for latency cost: SUM(x_i × E_i)
    pub w_lat: f64,
    /// Weight for bandwidth overhead cost: SUM(x_i × r_i)
    pub w_bw: f64,
    /// Weight for the fate-diversity penalty ρ_fate (RWM per-symbol placement,
    /// paper Section 16.3). Applies to REPAIR symbols only: it is the
    /// continuous form of the old hard `best_repair_path_avoiding` rule — a
    /// repair placed on a path that already carried the window symbols it
    /// covers gains no diversity, so its marginal cost rises. Zero for source.
    pub w_div: f64,
}

impl SchedulingWeights {
    pub fn from_hint(hint: ProtocolHint) -> Self {
        // w_div is hint-independent: fate diversity for a repair is worth the
        // same across workloads (a repair correlated with its coverage is
        // wasted regardless of the (δ, ρ, r) triangle). See place_symbol.
        match hint {
            ProtocolHint::Realtime => Self { w_lat: 1.0, w_bw: 0.0, w_div: 1.0 },
            ProtocolHint::Bulk => Self { w_lat: 0.0, w_bw: 1.0, w_div: 1.0 },
            ProtocolHint::Auto => Self { w_lat: 0.5, w_bw: 0.5, w_div: 1.0 },
        }
    }
}

/// Global correction deficit tracker.
///
/// Tracks `deficit = SUM(epsilon_s for un-ACKed symbols)` — the total expected
/// corrections still needed across all paths. See paper Section 13.4.
///
/// Each sent symbol adds `epsilon_i` (loss rate of its path) to the deficit.
/// Each ACKed symbol removes its send-time `epsilon_s` (confirmed survived).
/// Lost corrections add to the deficit, creating the geometric chain that
/// produces `r = epsilon / (1 - epsilon)`.
#[derive(Debug)]
pub struct CorrectionDeficit {
    /// Per-symbol tracking: (seq, path_id, epsilon_at_send)
    pending: VecDeque<(u64, PathId, f64)>,
    /// Running sum of epsilon_s for all pending symbols.
    total: f64,
}

// on_ack / deficit / pending_count / path_deficit have only #[cfg(test)]
// consumers (the deficit-chain law tests in this file); the live path uses
// on_send + on_ack_cumulative.
#[allow(dead_code)]
impl CorrectionDeficit {
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            total: 0.0,
        }
    }

    /// Record a symbol sent on a path with loss rate epsilon.
    pub fn on_send(&mut self, seq: u64, path_id: PathId, epsilon: f64) {
        self.pending.push_back((seq, path_id, epsilon));
        self.total += epsilon;
    }

    /// Acknowledge a symbol (confirmed received). Removes its epsilon from deficit.
    /// Returns true if the symbol was found and removed.
    pub fn on_ack(&mut self, seq: u64) -> bool {
        if let Some(pos) = self.pending.iter().position(|(s, _, _)| *s == seq) {
            let (_, _, eps) = self.pending.remove(pos).unwrap();
            self.total -= eps;
            if self.total < 0.0 {
                self.total = 0.0; // floating point guard
            }
            true
        } else {
            false
        }
    }

    /// Acknowledge all symbols up to and including `up_to_seq` (cumulative ACK).
    pub fn on_ack_cumulative(&mut self, up_to_seq: u64) {
        while self.pending.front().is_some_and(|(s, _, _)| *s <= up_to_seq) {
            let (_, _, eps) = self.pending.pop_front().unwrap();
            self.total -= eps;
        }
        if self.total < 0.0 {
            self.total = 0.0;
        }
    }

    /// Current total correction deficit.
    pub fn deficit(&self) -> f64 {
        self.total
    }

    /// Number of un-ACKed symbols being tracked.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Per-path deficit: sum of epsilon_s for un-ACKed symbols on a specific path.
    pub fn path_deficit(&self, path_id: PathId) -> f64 {
        self.pending
            .iter()
            .filter(|(_, pid, _)| *pid == path_id)
            .map(|(_, _, eps)| eps)
            .sum()
    }
}

/// Sliding window entry for bandwidth/RTT tracking.
#[derive(Clone, Debug)]
struct BwSample {
    /// Delivery rate in symbols per second.
    delivery_rate: f64,
    /// Timestamp when this sample was taken.
    timestamp: Instant,
}

#[derive(Clone, Debug)]
struct RttSample {
    rtt: Duration,
    timestamp: Instant,
}

/// Cap on rate-sample send records tracked per path (bounds the map when
/// symbols are lost / attributed without a matching send record). ~a few
/// aggregate BDPs; oldest are dropped past this.
const RS_MAX_TRACKED: usize = 8192;

/// A sent SOURCE symbol's BBR delivery-rate-sample state
/// (draft-cheng-iccrg-delivery-rate-estimation), snapshotted at send time and
/// consumed when the symbol is acked to produce ONE rate sample whose Δt is the
/// SEND interval — robust to ack-aggregation and a standing queue.
#[derive(Clone, Copy, Debug)]
struct RsPacket {
    /// `C.delivered` (this path's delivered counter) at the moment of send.
    delivered: u64,
    /// `C.delivered_time` (time `delivered` last advanced) at send.
    delivered_time: Instant,
    /// `C.first_sent_time` (start of the current in-flight send burst) at send.
    first_sent_time: Instant,
    /// When this symbol was sent.
    sent_time: Instant,
    /// The sender was app-limited (starved, not cwnd/pace-limited) at send —
    /// the sample may only RAISE the max-filter, never be read as bw dropping.
    app_limited: bool,
}

/// Copa-lite delay-based congestion control state.
///
/// Copa (Arun & Balakrishnan, NSDI 2018), simplified to the semantics that
/// won the L0 goal gate (tests/gate_suite.rs run_fec driver, P1+P2):
///
///   - Propagation floor: min RTT sample over a sliding ~10s window (P2's
///     estimated floor; windowed rather than lifetime so a route change
///     re-learns within one window).
///   - Queuing-delay signal: min RTT sample since the LAST cwnd update.
///   - Two-speed ramp: ×1.5+1 per update until first backoff, then +2/×0.92.
///   - Backoff when the windowed min exceeds the hint-coupled queue target
///     floor × queue_mult (P1).
///
/// Key properties:
///   - No phases (no Startup/ProbeBw/ProbeRtt state machine)
///   - Natural rate oscillation drains queues without explicit probe phase
///   - Compatible with taper function (no FEC protection gaps)
///   - Delay-based: loss + stable RTT = channel loss (ignore)
///
/// See paper Section 12 (Congestion Control Integration).
#[derive(Debug)]
pub struct CopaState {
    /// Sliding window of bandwidth samples (symbols/sec).
    bw_samples: VecDeque<BwSample>,
    /// goal-gate "Honest Inputs" (`RWM_HONEST_ANCHOR`): monotonic
    /// (non-increasing) MAX-deque maintained beside `bw_samples` — the exact
    /// mirror of the `rtt_samples` min-deque, on the max statistic. Fed and
    /// evicted in lockstep with `bw_samples` (`bw_push_sample` /
    /// `bw_evict_before`), so its front is ALWAYS the full-window fold's
    /// value (unit-pinned). `max_bw` reads it only under `bw_o1`; the deque
    /// itself is maintained unconditionally (O(1) amortized, ≤ the size of
    /// `bw_samples`) so the equality is testable without env plumbing.
    bw_mono: VecDeque<BwSample>,
    /// `RWM_HONEST_ANCHOR` resolved at construction: `max_bw` = the mono
    /// deque's front (O(1)) instead of the per-sample full-window fold
    /// (O(window) — the measured c1 CPU tax under `RWM_PLAIN_RS`).
    /// Value-identical either way.
    bw_o1: bool,
    /// Sliding window of RTT samples.
    rtt_samples: VecDeque<RttSample>,
    /// How long to keep samples in sliding windows (10s).
    window_duration: Duration,
    /// Minimum RTT in the sliding window = estimated propagation floor.
    min_rtt: Option<Duration>,
    /// Maximum delivery rate seen in the current window.
    max_bw: f64,
    /// Smoothed RTT (EWMA 7/8 old + 1/8 new) — pacing-rate denominator and
    /// cwnd-update cadence.
    srtt: Option<Duration>,
    /// Minimum RTT sample since the last cwnd update — the queuing-delay
    /// signal (windowed min, NOT an EWMA; see module docs).
    min_rtt_since_update: Option<Duration>,
    /// Consecutive-difference jitter estimate (seconds): EWMA of
    /// |rtt_i − rtt_{i−1}| at gain 1/8 (RFC 3550-style). Shift-robust by
    /// construction — a standing queue shifts ALL samples and leaves the
    /// consecutive differences at jitter scale, so this measures jitter,
    /// never queue. Widens the backoff threshold (JITTER_HEADROOM).
    jitter_est: f64,
    /// EWMA of the SQUARED deviation from the smoothed RTT (seconds²) —
    /// `var = (1−β)·var + β·(rtt − srtt)²` at RFC 6298 §2's own smoothing
    /// gain `β = 1/4`, the gain that RFC uses for exactly this job on the
    /// FIRST absolute moment (`RTTVAR = (1−β)·RTTVAR + β·|SRTT − R'|`).
    ///
    /// Exists for paper §16.69's DERIVED recovery clock, which needs a genuine
    /// SECOND moment: Cantelli's distribution-free bound is stated in σ, and
    /// the tree's two existing dispersion signals are both mean-ABSOLUTE
    /// statistics (`jitter_est` on consecutive differences, the estimator's
    /// RFC 3550 interarrival jitter). Converting either to σ requires assuming
    /// a distribution, which would turn the derived clock's one
    /// distribution-free guarantee into a fitted coefficient — §16.69.
    ///
    /// Observation only: nothing outside `RWM_QUANTILE_CLOCKS` reads it.
    rtt_var_sq: f64,
    /// How many samples have been folded into `rtt_var_sq` — the EWMA's own
    /// warm-up denominator, and the honest half of the `sig_us=` gauge.
    ///
    /// **This exists because "is σ valid yet?" has a different answer here
    /// than everywhere else in the engine.** `ANCHOR_MIN_SAMPLES` = 8 gates
    /// the DELIVERED-RATE anchor (`bw_samples`); it has nothing to do with
    /// this statistic, which is fed from the RTT sample stream and is
    /// available from the first sample that has an `srtt` to deviate from. But
    /// it is not TRUSTWORTHY from the first sample: the EWMA is seeded at 0
    /// and runs at RFC 6298's β = 1/4, so it carries ≈ (1−β)^n = 0.75^n of its
    /// seed after n samples — 24 % at n = 5, 10 % at n = 8, 1 % at n = 16.
    /// A σ read at n = 2 is biased LOW by roughly half, and a gauge that
    /// reported it as a bare number would hand an L1 parser a warm-up artefact
    /// wearing a measurement's clothes.
    ///
    /// So the count is reported ALONGSIDE σ rather than used to gate it (`n`
    /// in `sig_us=<µs>/n<count>`). Emitting only-when-valid would require a
    /// threshold on n, and a threshold that selects whether a gauge exists is
    /// the same defect as a threshold that selects a law: the reader could not
    /// tell "σ suppressed" from "path never sampled". The parser gets the
    /// number and the evidence about it, and decides.
    rtt_var_n: u64,
    /// **CANDIDATE 2 of 3 — the `rvar_us=` gauge.** RFC 6298 §2's `RTTVAR`,
    /// the MEAN-DEVIATION EWMA: `rvar ← (1−β)·rvar + β·|rtt − srtt|` at the
    /// RFC's own β = 1/4 (`SIGMA_CAND_RVAR_GAIN`).
    ///
    /// **It is the shipped `rtt_var_sq` with the SQUARE removed and nothing
    /// else changed** — same feed site, same β, same lagging `srtt` reference,
    /// same 7-sample memory. That is the entire point of building it: it is
    /// not a competitor, it is the CONTROLLED COMPARISON that isolates one of
    /// the three candidate causes of the 287×. If `rvar`'s dispersion lands
    /// near `√(dispersion of sig_us)`, the culprit is OUTLIER LEVERAGE — the
    /// square, which admits a single excursion as its square — and the memory
    /// is innocent. If `rvar`'s dispersion stays near `sig_us`'s, the square is
    /// innocent and the memory or the reference is the culprit. **Neither
    /// candidate alone can tell those apart; the pair can.**
    ///
    /// Provenance: CITED (RFC 6298 §2; RFC 8985 §6.2 inherits it for RACK).
    /// Observation only: read by nothing but `[DIAG]`.
    rtt_mdev: f64,
    /// Samples folded into [`CopaState::rtt_mdev`] — its warm-up denominator,
    /// on the line beside it. EWMA-class, so the pre-registered `n_warm` is 16
    /// (`0.75^16` = 1.00 % seed retention), the same as `rtt_var_n`'s.
    rtt_mdev_n: u64,
    /// **THE RAW RTT SERIES, last `SIGMA_CAND_WINDOW` samples, µs, FIFO.**
    /// Feeds candidates 1 (`qsp_us`) and 3 (`msd_us`).
    ///
    /// **This cannot reuse `rtt_samples`.** That deque is MONOTONIC — it pops
    /// from the back on every sample that is not larger than the incoming one,
    /// because it exists to serve a windowed MINIMUM in O(1). It therefore does
    /// not hold the series; it holds a lower envelope of it, and a dispersion
    /// statistic taken over an envelope would read the drift and not the
    /// spread. This is a plain FIFO and holds every sample in arrival order,
    /// which is also what makes the SUCCESSIVE differences of candidate 3
    /// meaningful.
    rtt_win: VecDeque<u32>,
    /// Previous raw RTT sample (for the consecutive difference).
    prev_rtt_sample: Option<Duration>,
    /// Per-update window-min history over the sliding window: the queue
    /// floor is a low quantile of these (QUEUE_FLOOR_QUANTILE) — the same
    /// statistic as the queue signal itself, so jitter cannot open a
    /// permanent gap between signal and floor (see const docs).
    win_min_history: VecDeque<(Instant, Duration)>,
    /// RTT samples recorded since the last cwnd update — evidence count
    /// for the ramp fast-exit (a min over ≥3 samples; a min-of-1 is just
    /// one jittery sample and fired false ramp exits at L1's C2).
    samples_since_update: u32,
    /// Window-level jitter estimate (seconds): EWMA (gain 1/4) of
    /// |win_min_i − win_min_{i−1}| between consecutive cwnd updates.
    /// Under correlated jitter the raw-sample consecutive differences
    /// collapse (~0.85ms at C2) while the window min wanders 3-5ms per
    /// update; this estimator sees that amplitude and stays shift-robust
    /// (a standing queue is ONE transition sample).
    win_jitter_est: f64,
    /// Previous update's window min (for the window-level difference).
    prev_win_min: Option<Duration>,
    /// True until the first congestion backoff (multiplicative ramp phase).
    ramping: bool,
    /// Hint-coupled queue-target multiplier (P1): 1.08/1.125/1.25.
    queue_mult: f64,
    /// When the cwnd was last updated (updates run once per SRTT).
    last_cwnd_update: Instant,
    /// Delivered symbols counter for delivery rate calculation.
    delivered: u64,
    /// Timestamp of last delivery measurement.
    last_delivered_time: Instant,
    /// Delivered count at last measurement.
    last_delivered: u64,
    // --- BBR delivery-rate sampling (the send-interval anchor, ADR-0061) ---
    /// Total SOURCE symbols delivered on this path (BBR `C.delivered`). Separate
    /// from `delivered` so the legacy ack-interval anchor stays byte-exact.
    rs_delivered: u64,
    /// Time `rs_delivered` last advanced (BBR `C.delivered_time`).
    rs_delivered_time: Instant,
    /// Send time of the packet that started the current in-flight send burst
    /// (BBR `C.first_sent_time`); advances to each acked packet's send time.
    rs_first_sent_time: Instant,
    /// Outstanding rate-sample send records (seq → snapshot), consumed on ack.
    rs_sent: BTreeMap<u64, RsPacket>,
    // --- DIAG counters (diag/slow-path-anchor) -------------------------------
    // Pure observation — these NEVER affect a control decision (read only at the
    // RWM_DIAG print).  They trace WHY the per-path BtlBw anchor does or does not
    // warm: how many source seqs were snapshotted at send, how many acks were
    // attributed here, and how each attributed ack was classified by the BBR
    // GenerateRateSample guards (interval<MinRTT / zero-delivered / app-limited)
    // vs accepted into the windowed-max filter.
    rs_sent_count: u64,
    rs_applimited_sent: u64,
    rs_attributions: u64,
    rs_no_record: u64,
    rs_rej_interval: u64,
    rs_rej_zero: u64,
    rs_rej_applimited: u64,
    rs_generated: u64,
    /// RWM_RS_TRACE: eprintln each ACCEPTED rate sample above the given
    /// symbols/s threshold with its (delivered, interval, send_elapsed,
    /// ack_elapsed) decomposition — the over-read forensics instrument
    /// (feat/copa-sole-cc). 0 = off (default, no cost on the sample path).
    rs_trace_thresh: f64,
    /// DIAG label for the RSTRACE prints: the owning path's id (u32::MAX
    /// until the owner stamps it). Never read by any control decision.
    pub(crate) rs_trace_path: u32,
    // --- Wire-clocked δ-mapped update law (feat/copa-wire-signal) -----------
    /// Wire mode: the delay term is the packet-timed wire RTT (fed by the
    /// transport seam) and the cwnd update law is Copa's actual
    /// target-rate/velocity dynamics around rate = 1/(δ·d_q). False (default,
    /// env unset) ⇒ every legacy path byte-identical.
    wire_mode: bool,
    /// Copa δ (1/symbols): the hint-mapped latency price (`copa_delta`).
    /// In legacy mode this stays COPA_DELTA so the diagnostic
    /// `copa_target_cwnd` is unchanged.
    delta: f64,
    /// Copa velocity v: the per-SRTT step is v/δ symbols; v doubles once
    /// per update while the direction has persisted ≥ 3 consecutive
    /// updates, and resets to 1 on a direction flip (Copa §2.2's velocity
    /// parameter at per-SRTT granularity — the 3-window hysteresis is what
    /// bounds the overshoot of a pure every-window doubling, MEASURED at
    /// the L1 smoke: cwnd pinned MAX_CWND with 130 ms app-RTT spikes
    /// without it).
    velocity: f64,
    /// Consecutive same-direction update count (velocity hysteresis).
    dir_streak: u32,
    /// Direction of the previous wire-mode update (None = no update yet /
    /// after a backoff reset).
    last_dir_up: Option<bool>,
    // --- Copa §2.2 TCP-competitive mode (feat/copa-compete) -----------------
    /// Mode switching enabled (RWM_COPA_COMPETE && wire mode). False
    /// (default) ⇒ every field below is inert and the law byte-identical.
    compete_on: bool,
    /// The default-mode δ: the hint-mapped base price (`copa_delta`).
    /// `delta` diverges from it only while in competitive mode.
    delta_base: f64,
    /// Currently in competitive mode.
    in_compete: bool,
    /// DIAG: competitive-mode entries (mechanism liveness counter).
    compete_switches: u64,
    /// Last instant a "nearly empty" queue was observed
    /// (d_q < 0.1·(RTTmax−RTTmin), Copa §2.2). None = no sample yet
    /// (treated as recently-empty: default mode, false-positive safe).
    last_nearly_empty: Option<Instant>,
    /// Monotonic (non-increasing) deque of wire RTT samples over the past
    /// ~4 RTTs — front = RTTmax for the nearly-empty calibration. Mirror of
    /// the `rtt_samples` min-deque; O(1) amortized.
    compete_max_deque: VecDeque<(Instant, Duration)>,
    /// A wire-level loss event (quinn congestion event) was recorded since
    /// the last per-SRTT update — the competitive AIMD's MD trigger.
    loss_since_update: bool,
    /// Cumulative congestion-event counter last seen from the pass-through
    /// shim (diffed, not reset — the shim counter is monotone).
    last_cong_events: u64,
    // --- Raw-sample echo-ratio floor (goal-gate "Honest Inputs") ------------
    /// `RWM_HONEST_K` resolved at construction: feed the K tracker below the
    /// RAW per-sample ratio at the sample clock. False (default) ⇒ the
    /// tracker is never fed and `k_raw_ratio()` is `None` — every K consumer
    /// keeps the legacy smoothed-at-refresh feed byte-identically.
    k_raw_on: bool,
    /// The path's raw-fed windowed-min echo-ratio (`EchoRatioMin`, the SAME
    /// window/clamp/guard as the net-side refresh-clock trackers): fed
    /// rtt_raw/RTprop per sample in `record_rtt` under `k_raw_on`.
    k_raw: crate::net::EchoRatioMin,
    /// Monotonic µs epoch for `k_raw`'s window arithmetic.
    k_raw_epoch: Instant,
    /// Injectable clock for time queries.
    clock: Arc<dyn Clock>,
}

impl CopaState {
    fn new(clock: Arc<dyn Clock>, hint: ProtocolHint) -> Self {
        let wire_mode = copa_wire_active();
        let delta = if wire_mode {
            copa_delta_for_hint(hint)
        } else {
            COPA_DELTA
        };
        let now = clock.now();
        Self {
            wire_mode,
            delta,
            velocity: 1.0,
            dir_streak: 0,
            last_dir_up: None,
            compete_on: wire_mode && copa_compete_active(),
            delta_base: delta,
            in_compete: false,
            compete_switches: 0,
            last_nearly_empty: None,
            compete_max_deque: VecDeque::new(),
            loss_since_update: false,
            last_cong_events: 0,
            k_raw_on: honest_k_active(),
            k_raw: crate::net::EchoRatioMin::new(crate::net::PERCAP_K_HALF_WINDOW_US),
            k_raw_epoch: now,
            bw_samples: VecDeque::new(),
            bw_mono: VecDeque::new(),
            bw_o1: honest_anchor_active(),
            rtt_var_sq: 0.0,
            rtt_var_n: 0,
            rtt_mdev: 0.0,
            rtt_mdev_n: 0,
            rtt_win: VecDeque::with_capacity(SIGMA_CAND_WINDOW),
            rtt_samples: VecDeque::new(),
            window_duration: Duration::from_secs(10),
            min_rtt: None,
            max_bw: 0.0,
            srtt: None,
            min_rtt_since_update: None,
            jitter_est: 0.0,
            prev_rtt_sample: None,
            win_min_history: VecDeque::new(),
            samples_since_update: 0,
            win_jitter_est: 0.0,
            prev_win_min: None,
            ramping: true,
            queue_mult: queue_target_mult(hint),
            delivered: 0,
            last_delivered_time: now,
            last_delivered: 0,
            rs_delivered: 0,
            rs_delivered_time: now,
            rs_first_sent_time: now,
            rs_sent: BTreeMap::new(),
            rs_sent_count: 0,
            rs_applimited_sent: 0,
            rs_attributions: 0,
            rs_no_record: 0,
            rs_rej_interval: 0,
            rs_rej_zero: 0,
            rs_rej_applimited: 0,
            rs_generated: 0,
            rs_trace_thresh: std::env::var("RWM_RS_TRACE")
                .ok()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0),
            rs_trace_path: u32::MAX,
            last_cwnd_update: now,
            clock,
        }
    }

    /// Admit one bandwidth sample into the sliding window: `bw_samples` gets
    /// it verbatim (its LENGTH is the anchor-establishment gate —
    /// `ANCHOR_MIN_SAMPLES` — and must not change meaning), and the
    /// monotonic max-deque `bw_mono` gets it with dominated-candidate
    /// eviction (goal-gate "Honest Inputs"): a sample that is older AND no
    /// larger than the new one can never again be the windowed max — any
    /// front-eviction cutoff that spares it spares the newer, larger sample
    /// too — so after eviction the mono deque is strictly decreasing
    /// front→back with increasing timestamps, and its FRONT equals the
    /// full-window fold over `bw_samples` at all times (unit-pinned by
    /// `bw_mono_front_equals_full_window_fold`).
    fn bw_push_sample(&mut self, now: Instant, rate: f64) {
        self.bw_samples.push_back(BwSample {
            delivery_rate: rate,
            timestamp: now,
        });
        while self
            .bw_mono
            .back()
            .is_some_and(|s| s.delivery_rate <= rate)
        {
            self.bw_mono.pop_back();
        }
        self.bw_mono.push_back(BwSample {
            delivery_rate: rate,
            timestamp: now,
        });
    }

    /// Evict bandwidth samples older than `cutoff` from BOTH windows (the
    /// two structures see identical push and eviction sequences — that
    /// lockstep is what makes front == fold an invariant rather than a
    /// coincidence). Called with the legacy 10 s cutoff from
    /// `expire_old_samples` and with the ≈10·RTprop [1 s, 10 s] cutoff from
    /// `rs_on_delivered`, exactly where `bw_samples` was already evicted.
    fn bw_evict_before(&mut self, cutoff: Instant) {
        while self.bw_samples.front().is_some_and(|s| s.timestamp < cutoff) {
            self.bw_samples.pop_front();
        }
        while self.bw_mono.front().is_some_and(|s| s.timestamp < cutoff) {
            self.bw_mono.pop_front();
        }
    }

    /// Recompute `max_bw` after a push/evict. `RWM_HONEST_ANCHOR` selects the
    /// COST of the same value: the mono deque's front (O(1) amortized) or
    /// the legacy full-window fold (O(window) per accepted sample — the
    /// measured c1 sender-CPU tax once `RWM_PLAIN_RS` feeds this per
    /// delivered symbol instead of per ack).
    fn bw_refresh_max(&mut self) {
        self.max_bw = if self.bw_o1 {
            self.bw_mono.front().map_or(0.0, |s| s.delivery_rate)
        } else {
            self.bw_samples
                .iter()
                .map(|s| s.delivery_rate)
                .fold(0.0f64, f64::max)
        };
    }

    /// The legacy full-window fold, unconditionally — the equivalence
    /// oracle for `bw_mono` (test-only).
    #[cfg(test)]
    fn bw_fold(&self) -> f64 {
        self.bw_samples
            .iter()
            .map(|s| s.delivery_rate)
            .fold(0.0f64, f64::max)
    }

    /// Record delivery of `count` symbols.  Returns the computed delivery rate.
    fn record_delivery(&mut self, count: u32) -> f64 {
        self.delivered += count as u64;
        let now = self.clock.now();
        let elapsed = now.duration_since(self.last_delivered_time).as_secs_f64();

        // Need at least 1ms of elapsed time to compute a meaningful rate
        if elapsed < 0.001 {
            // ACK-CADENCE GAUGE (`RWM_ACKDIAG`, net/ackdiag.rs — readout 3):
            // a REJECTED sample. It carries no rate (the filter is untouched)
            // but its `count` did arrive, so it feeds the over-read
            // denominator: the normalizer must see every delivered symbol the
            // sampler saw, or x is inflated by the rejection rate.
            if let Some(g) = crate::net::ackdiag::gauge() {
                g.note_rate_sample(self.rs_trace_path, count, 0.0, false);
            }
            return self.max_bw;
        }

        let delta_delivered = self.delivered - self.last_delivered;
        let rate = delta_delivered as f64 / elapsed;

        self.last_delivered_time = now;
        self.last_delivered = self.delivered;

        if self.rs_trace_thresh > 0.0 && rate >= self.rs_trace_thresh {
            eprintln!(
                "[RSTRACE-LEGACY] path={} rate={:.0} delta={} elapsed_ms={:.2} max_bw={:.0}",
                self.rs_trace_path,
                rate,
                delta_delivered,
                elapsed * 1e3,
                self.max_bw,
            );
        }
        // ACK-CADENCE GAUGE (`RWM_ACKDIAG`, net/ackdiag.rs — readout 3): an
        // ACCEPTED ack-interval sample, i.e. exactly one of the values the
        // windowed max below folds. This is THE statistic matrix row 10 calls
        // "UNVERIFIED — and it is the one that is ALWAYS ON"; the gauge
        // normalizes it at print time by the window's own long-run delivered
        // rate to give the realized over-read x directly.
        if let Some(g) = crate::net::ackdiag::gauge() {
            g.note_rate_sample(self.rs_trace_path, count, rate, true);
        }
        // Add to sliding window
        self.bw_push_sample(now, rate);
        self.expire_old_samples(now);

        // Update max bandwidth
        self.bw_refresh_max();

        rate
    }

    // --- BBR delivery-rate sampling (feat/btlbw-rate-sample) ------------------
    //
    // The legacy `record_delivery` above computes rate = Δdelivered / Δt where
    // Δt is the ACK-ARRIVAL interval (`now − last_delivered_time`).  Under DAPS,
    // acks arrive BATCHED (ack-aggregation): a batch collapses Δt toward zero, so
    // Δdelivered/Δt spikes and the windowed-MAX locks onto the spike — the ~145×
    // over-read L1 DIAG measured (fast bdp 14509 / RTprop 12 ms ⇒ ≈1.2M sym/s vs
    // true ≈8.3k).  A rate anchor 145× too high makes EVERY per-path pace bucket
    // / BDP cap inert (the bucket never binds; outstanding bloats to the deep
    // read-ahead — see temporal_oracle PART 6g).
    //
    // The fix (Cardwell/Cheng, draft-cheng-iccrg-delivery-rate-estimation):
    // sample Δt over the SEND interval — max(send_elapsed, ack_elapsed) — so a
    // batched ack (tiny ack_elapsed) is overridden by the true send spacing and
    // the sample is a correct delivery-rate LOWER BOUND.  The max-filter then
    // maxes over CORRECT samples and converges to the true BtlBw (×1).

    /// BBR `SendPacket`: snapshot the rate-sample state for a sent SOURCE symbol.
    fn rs_on_sent(&mut self, seq: u64, app_limited: bool) {
        self.rs_sent_count += 1; // DIAG
        if app_limited {
            self.rs_applimited_sent += 1; // DIAG
        }
        let now = self.clock.now();
        // No packets in flight → (re)start the send burst window.
        if self.rs_sent.is_empty() {
            self.rs_first_sent_time = now;
            self.rs_delivered_time = now;
        }
        self.rs_sent.insert(
            seq,
            RsPacket {
                delivered: self.rs_delivered,
                delivered_time: self.rs_delivered_time,
                first_sent_time: self.rs_first_sent_time,
                sent_time: now,
                app_limited,
            },
        );
        // Bound the map: drop the oldest snapshots for symbols that were lost or
        // attributed cumulatively without ever matching a send record.
        while self.rs_sent.len() > RS_MAX_TRACKED {
            if let Some(&k) = self.rs_sent.keys().next() {
                self.rs_sent.remove(&k);
            } else {
                break;
            }
        }
    }

    /// BBR `UpdateRateSample` + `GenerateRateSample` for ONE acked SOURCE symbol.
    /// Feeds the windowed-max delivery-rate filter (`max_bw`) a sample whose Δt is
    /// the SEND interval, so batched acks / a standing queue cannot inflate it.
    fn rs_on_delivered(&mut self, seq: u64) {
        self.rs_attributions += 1; // DIAG
        let now = self.clock.now();
        let Some(p) = self.rs_sent.remove(&seq) else {
            // No send record (attributed cumulatively past a dropped record):
            // still advance the delivered cursor so later samples stay correct.
            self.rs_no_record += 1; // DIAG
            self.rs_delivered += 1;
            self.rs_delivered_time = now;
            return;
        };
        // Advance the connection delivered cursor (BBR: C.delivered += len).
        self.rs_delivered += 1;
        self.rs_delivered_time = now;
        // send_elapsed spans the send spacing of the packets from the burst start
        // to this packet; ack_elapsed spans the same deliveries in wall time.
        let send_elapsed = p.sent_time.saturating_duration_since(p.first_sent_time);
        let ack_elapsed = now.saturating_duration_since(p.delivered_time);
        // Advance the burst-window start (BBR: C.first_sent_time = P.sent_time).
        self.rs_first_sent_time = p.sent_time;
        // max() is what makes the sample ack-aggregation robust: a batched ack
        // shrinks ack_elapsed, but send_elapsed preserves the true spacing.
        let interval = send_elapsed.max(ack_elapsed).as_secs_f64();
        let delivered = self.rs_delivered.saturating_sub(p.delivered);
        // Reject samples spanning less than one RTprop (BBR GenerateRateSample:
        // `if interval < MinRTT: return`).  An interval below the propagation RTT
        // cannot reliably estimate the bottleneck rate: it is the classic
        // ack-aggregation / send-burst artefact (a batch of queued symbols acked
        // together over a tiny window), which otherwise reads many× the true link
        // (the DAPS slow-path over-read).  Requiring interval ≥ RTprop forces the
        // sample to average over ≥ one pipe, so a drain burst reads the true
        // bottleneck.  Falls back to a 1 ms absolute floor before an RTprop sample.
        let min_interval = self
            .min_rtt
            .map(|r| r.as_secs_f64())
            .unwrap_or(0.001)
            .max(0.001);
        // DIAG: classify the rejection (split from the combined guard below for
        // per-cause counting; behaviour identical — same early return).
        if interval < min_interval {
            self.rs_rej_interval += 1; // DIAG
            return;
        }
        if delivered == 0 {
            self.rs_rej_zero += 1; // DIAG
            return;
        }
        let rate = delivered as f64 / interval;
        // App-limited samples underestimate bw (the pipe was starved, not full),
        // so they may only RAISE the max-filter, never be read as bw dropping.
        // In a pure windowed-max a low app-limited sample is simply not the max;
        // admit one only when it exceeds the current max (BBR §app-limited).
        if p.app_limited && rate <= self.max_bw {
            self.rs_rej_applimited += 1; // DIAG
            return;
        }
        self.rs_generated += 1; // DIAG
        if self.rs_trace_thresh > 0.0 && rate >= self.rs_trace_thresh {
            eprintln!(
                "[RSTRACE] path={} seq={} rate={:.0} delivered={} interval_ms={:.2} send_ms={:.2} ack_ms={:.2} max_bw={:.0}",
                self.rs_trace_path,
                seq,
                rate,
                delivered,
                interval * 1e3,
                send_elapsed.as_secs_f64() * 1e3,
                ack_elapsed.as_secs_f64() * 1e3,
                self.max_bw,
            );
        }
        self.bw_push_sample(now, rate);
        // Max-filter window ≈ 10·RTprop (BBR's BtlBw filter), clamped to
        // [1s, 10s]: long enough to hold the true BtlBw between acks, short
        // enough that a genuine rate change is not pinned for the full 10s
        // sample window.  Falls back to 10s before a min-RTT sample exists.
        let win = self
            .min_rtt
            .map(|r| (r.as_secs_f64() * 10.0).clamp(1.0, 10.0))
            .unwrap_or(10.0);
        let cutoff = now
            .checked_sub(Duration::from_secs_f64(win))
            .unwrap_or(now);
        self.bw_evict_before(cutoff);
        // goal-gate "Honest Inputs": under RWM_PLAIN_RS this runs once per
        // DELIVERED SOURCE SYMBOL, so the legacy full-window fold here is
        // O(window·rate) per second — the measured c1 sender-CPU tax
        // (+61…64% CPU/byte, latlever CPU gauge). RWM_HONEST_ANCHOR reads
        // the same value off the mono deque in O(1).
        self.bw_refresh_max();
    }

    /// Record an RTT sample: SRTT EWMA, 10s floor window, and the
    /// since-last-update min (queuing-delay signal).
    fn record_rtt(&mut self, rtt: Duration) {
        let now = self.clock.now();

        // SRTT EWMA (RFC 6298 weights, same as the gate driver).
        self.srtt = Some(match self.srtt {
            Some(s) => s.mul_f64(0.875) + rtt.mul_f64(0.125),
            None => rtt,
        });

        // Windowed min for the queuing-delay signal.
        self.min_rtt_since_update = Some(match self.min_rtt_since_update {
            Some(m) => m.min(rtt),
            None => rtt,
        });

        // Consecutive-difference jitter EWMA (shift-robust; see field doc).
        if let Some(prev) = self.prev_rtt_sample {
            let diff = if rtt > prev { rtt - prev } else { prev - rtt };
            self.jitter_est += (diff.as_secs_f64() - self.jitter_est) * JITTER_GAIN;
        }
        self.prev_rtt_sample = Some(rtt);
        self.samples_since_update += 1;

        // Windowed minimum via a MONOTONIC (non-decreasing) deque — O(1)
        // amortised instead of the former O(n) rescan of the entire 10 s
        // sample history on every sample. At L1 (thousands of ACK-driven RTT
        // samples/s over a 10 s window ⇒ ~20k-element deque) that rescan was
        // the single largest sender CPU cost (~42% self, a hidden O(n²) over a
        // transfer — MEASURED by perf). A new sample evicts every pending
        // candidate whose RTT is >= its own: those can never be the window
        // minimum while this newer, smaller-or-equal sample is in the window
        // (and it expires strictly later), so after eviction the deque stays
        // non-decreasing front→back with strictly increasing timestamps. The
        // front is therefore always the current windowed min, and time-based
        // expiry still pops from the (oldest-timestamp) front. Exact same
        // `min_rtt` value as the rescan, just maintained incrementally.
        // §16.69's second moment, fed against the SMOOTHED mean the same way
        // RFC 6298 §2 feeds RTTVAR and at the same β = 1/4. Fed
        // unconditionally; read by nothing on the default arm.
        if let Some(sr) = self.srtt {
            let dev = rtt.as_secs_f64() - sr.as_secs_f64();
            self.rtt_var_sq = 0.75 * self.rtt_var_sq + 0.25 * dev * dev;
            // Counted at the SAME site that feeds the EWMA, so the `[DIAG]`
            // `sig_us=<µs>/n<count>` gauge's denominator can never describe a
            // different sample set than its numerator.
            self.rtt_var_n += 1;
            // CANDIDATE 2 (`rvar_us=`): RFC 6298 §2's mean deviation, fed at
            // the SAME site, from the SAME `dev`, at the SAME β. Identical in
            // every respect to the line above except that the deviation enters
            // LINEARLY instead of squared — which is what makes the pair a
            // decomposition rather than two guesses. Read by nothing.
            self.rtt_mdev += (dev.abs() - self.rtt_mdev) * SIGMA_CAND_RVAR_GAIN;
            self.rtt_mdev_n += 1;
        }
        // CANDIDATES 1 and 3 (`qsp_us=`, `msd_us=`): the raw series, FIFO,
        // last `SIGMA_CAND_WINDOW`. Fed unconditionally and WITHOUT an `srtt`
        // precondition — unlike the two EWMAs above, neither of these gauges
        // takes a deviation against a reference, so neither has to wait for
        // one. O(1): one push, at most one pop. Read by nothing.
        if self.rtt_win.len() == SIGMA_CAND_WINDOW {
            self.rtt_win.pop_front();
        }
        self.rtt_win
            .push_back((rtt.as_micros() as u64).min(u32::MAX as u64) as u32);
        while self.rtt_samples.back().is_some_and(|s| s.rtt >= rtt) {
            self.rtt_samples.pop_back();
        }
        self.rtt_samples.push_back(RttSample {
            rtt,
            timestamp: now,
        });
        self.expire_old_samples(now);
        self.min_rtt = self.rtt_samples.front().map(|s| s.rtt);

        // goal-gate "Honest Inputs" (`RWM_HONEST_K`): feed the K tracker the
        // RAW sample's ratio at the SAMPLE clock, against the freshest floor.
        // The windowed MIN then reads the delay distribution's FLOOR — the
        // quantity the honest-cap/three-term derivations assume — instead of
        // the min of the SMOOTHED series, which sits near the distribution's
        // mean and reads ×1.34-class high under ±25 ms jitter (the measured
        // jit25 inversion). Same tracker type, same window, same ≥ 1 clamp,
        // same seed-identity guard (which here also discards the exact
        // floor-setting sample — the min then reads the second-lowest in
        // window, a negligible upward bias under any dense sample stream,
        // and the guard stays shared discipline rather than forking).
        // Gated: OFF ⇒ nothing is fed and `k_raw_ratio()` is None — every
        // consumer keeps the legacy smoothed feed byte-identically.
        if self.k_raw_on {
            let now_us = now.duration_since(self.k_raw_epoch).as_micros() as u64;
            self.k_raw
                .observe_srtt_over_rtprop(rtt, self.min_rtt, now_us);
        }

        // Copa §2.2 competitive-mode detector sampling (feat/copa-compete):
        // mark the instants at which the queue is "nearly empty". Gated so
        // the shipped/wire-only paths pay nothing.
        if self.compete_on {
            self.compete_note_sample(rtt, now);
        }
    }

    /// Copa §2.2 nearly-empty detector, per wire RTT sample: maintain RTTmax
    /// over the past ~4 RTTs (monotonic max-deque, the mirror of the min
    /// deque above) and mark `last_nearly_empty` whenever the current
    /// queuing delay d_q = sample − RTTmin(long-term) is below
    /// 0.1·(RTTmax − RTTmin). The RTTmax term calibrates "nearly empty" to
    /// the path's short-term RTT variance (paper §2.2); the DQ_FLOOR guard
    /// keeps a zero-variance clean/idle link (RTTmax == RTTmin ⇒ threshold
    /// 0) from reading as "never empty" — a d_q at the clamp floor IS an
    /// empty queue.
    fn compete_note_sample(&mut self, rtt: Duration, now: Instant) {
        while self.compete_max_deque.back().is_some_and(|&(_, r)| r <= rtt) {
            self.compete_max_deque.pop_back();
        }
        self.compete_max_deque.push_back((now, rtt));
        let lookback = self.srtt().mul_f64(COMPETE_RTTMAX_RTTS);
        let cutoff = now.checked_sub(lookback).unwrap_or(now);
        while self
            .compete_max_deque
            .front()
            .is_some_and(|&(t, _)| t < cutoff)
        {
            self.compete_max_deque.pop_front();
        }
        let Some(floor) = self.min_rtt else { return };
        let rtt_max = self
            .compete_max_deque
            .front()
            .map(|&(_, r)| r)
            .unwrap_or(rtt);
        let dq = (rtt.as_secs_f64() - floor.as_secs_f64()).max(0.0);
        let threshold = COMPETE_EMPTY_FRAC * (rtt_max.as_secs_f64() - floor.as_secs_f64());
        if dq <= threshold.max(DQ_FLOOR_SECS) {
            self.last_nearly_empty = Some(now);
        }
    }

    /// Wire-level loss evidence for the competitive AIMD (feat/copa-compete):
    /// fed the pass-through shim's CUMULATIVE `congestion_events` counter for
    /// this path; any advance since the last read marks a loss into the
    /// current update window. No-op unless competitive switching is enabled.
    fn note_congestion_events(&mut self, cumulative: u64) {
        if !self.compete_on {
            return;
        }
        if cumulative > self.last_cong_events {
            self.loss_since_update = true;
        }
        self.last_cong_events = cumulative;
    }

    /// Copa §2.2 mode switching + the competitive AIMD on 1/δ, evaluated once
    /// per SRTT update (the paper's per-RTT cadence). See the module-level
    /// mechanism note at `copa_compete_active`.
    ///
    ///   - default → competitive: no nearly-empty queue observed in the last
    ///     5 RTTs; enter at δ = δ_base (AIMD grows 1/δ from the base price).
    ///   - competitive AIMD (NewReno-emulating, per the paper's
    ///     implementation): loss in the window ⇒ 1/δ ← max(1/δ_base, 1/(2δ));
    ///     otherwise 1/δ ← 1/δ + 1. Invariant: δ ≤ δ_base (the paper's
    ///     "δ ≤ 0.5" generalized to the hint base), 1/δ bounded so the
    ///     coupling cap's 2/δ term stays ≤ MAX_CWND.
    ///   - competitive → default: a nearly-empty queue within the last
    ///     5 RTTs ⇒ reset δ = δ_base (the paper's reset-to-0.5), velocity
    ///     re-measures from 1.
    ///
    /// Skipped during the ramp: the startup burst's own queue is not
    /// competitor evidence, and the velocity law is not live yet.
    fn compete_update(&mut self, now: Instant) {
        if !self.compete_on || self.ramping {
            return;
        }
        let window = self.srtt().mul_f64(COMPETE_WINDOW_RTTS);
        let empty_recent = match self.last_nearly_empty {
            Some(t) => now.saturating_duration_since(t) <= window,
            // No detector evidence yet (no RTT floor/sample): stay default —
            // the false-positive-safe direction.
            None => true,
        };
        if self.in_compete {
            if empty_recent {
                self.in_compete = false;
                self.delta = self.delta_base;
                self.velocity = 1.0;
                self.dir_streak = 0;
                self.last_dir_up = None;
            } else {
                let inv = 1.0 / self.delta;
                let inv = if self.loss_since_update {
                    (inv * 0.5).max(1.0 / self.delta_base)
                } else {
                    (inv + 1.0).min(COMPETE_INV_DELTA_MAX)
                };
                self.delta = 1.0 / inv;
            }
        } else if !empty_recent {
            self.in_compete = true;
            self.compete_switches += 1;
            self.delta = self.delta_base;
        }
        self.loss_since_update = false;
    }

    /// Smoothed RTT, defaulting to 50ms before the first sample.
    fn srtt(&self) -> Duration {
        self.srtt.unwrap_or(DEFAULT_SRTT)
    }

    /// The queue floor: QUEUE_FLOOR_QUANTILE of the recent window-min
    /// history — the same min-of-N statistic as the queue signal itself
    /// (see const docs; falls back to the propagation floor before any
    /// history accumulates). Never below the propagation floor by
    /// construction (every window min is itself an RTT sample).
    fn queue_floor(&self) -> Option<Duration> {
        if self.win_min_history.is_empty() {
            return self.min_rtt;
        }
        let mut v: Vec<Duration> = self.win_min_history.iter().map(|&(_, d)| d).collect();
        let idx = (((v.len() - 1) as f64) * QUEUE_FLOOR_QUANTILE).round() as usize;
        let (_, nth, _) = v.select_nth_unstable(idx);
        Some(*nth)
    }

    /// Whether the standing-queue signal is above the hint-coupled target:
    /// windowed-min RTT − queue_floor (= dq, clamped ≥ 0.1ms) exceeds
    /// (queue_mult − 1) × queue_floor + k × jitter_est (also clamped
    /// ≥ 0.1ms).
    ///
    /// Equivalent to the gate driver's `min_rtt_win > floor × queue_mult`
    /// except for three continuity guards (all vanish on a clean link,
    /// where queue_floor == floor and jitter_est == 0):
    ///   - the dq clamp keeps sub-millisecond-RTT links from backing off
    ///     on sub-clamp noise (see DQ_FLOOR_SECS),
    ///   - the queue floor is a low quantile of the window-min history
    ///     rather than the extreme-value 10s min, so jitter cannot open a
    ///     permanent gap between signal and floor (QUEUE_FLOOR_QUANTILE —
    ///     measured L1 root cause of the C2 throughput collapse), and
    ///   - the k × jitter_est term covers the residual within-window
    ///     spread at small sample counts (JITTER_HEADROOM).
    /// Wire-mode queuing delay d_q (seconds): STANDING wire RTT (the most
    /// recent packet-timed sample — quinn's srtt, already an EWMA over many
    /// per-ack samples: Copa §2's RTTstanding) − propagation floor − jitter
    /// headroom, clamped ≥ DQ_FLOOR_SECS.
    ///
    /// Differences from the legacy signal, all consequences of the wire
    /// clock (feat/copa-wire-signal):
    ///   - The signal is the CURRENT standing estimate, NOT the per-window
    ///     min. MEASURED (L1 c2 smoke, v1 of this law): the δ-sawtooth's
    ///     drain trough falls inside every update window, so a windowed min
    ///     reads "queue empty" at every update — the direction stays up,
    ///     the velocity compounds, and cwnd pins MAX_CWND with 130 ms
    ///     app-RTT spikes. The smoothed standing sample tracks the queue
    ///     the law is actually steering.
    ///   - The floor is the RAW 10 s min (`min_rtt`), not the quantile
    ///     queue floor: wire samples are already smoothed (sample-level
    ///     jitter averaged out), and the law's dither drains the queue to
    ///     ~empty regularly, refreshing the raw min (the quantile floor was
    ///     an app-echo jitter fix, and under a deep Bulk standing queue it
    ///     would creep up to the queue itself within its 10 s window —
    ///     staleness by construction).
    ///   - The jitter headroom is SUBTRACTED from the measured d_q rather
    ///     than added to a threshold, so one adjusted quantity feeds both
    ///     the above-target test and the target-rate law (continuity:
    ///     jitter → 0 recovers plain Copa exactly).
    fn wire_dq_secs(&self) -> Option<f64> {
        let standing = self.prev_rtt_sample?;
        let floor = self.min_rtt?;
        let jitter = self.jitter_est.max(self.win_jitter_est);
        Some(
            (standing.as_secs_f64() - floor.as_secs_f64() - JITTER_HEADROOM * jitter)
                .max(DQ_FLOOR_SECS),
        )
    }

    /// Wire-mode congestion test: is the current rate above Copa's target
    /// rate 1/(δ·d_q)?  cwnd/srtt > 1/(δ·d_q)  ⇔  cwnd·δ·d_q > srtt.
    fn wire_above_target(&self, cwnd: u32) -> bool {
        match self.wire_dq_secs() {
            Some(dq) => cwnd as f64 * self.delta * dq > self.srtt().as_secs_f64(),
            None => false,
        }
    }

    fn queue_above_target(&self, cwnd: u32) -> bool {
        if self.wire_mode {
            return self.wire_above_target(cwnd);
        }
        let (Some(win_min), Some(floor)) = (self.min_rtt_since_update, self.queue_floor()) else {
            return false;
        };
        let floor_s = floor.as_secs_f64();
        let dq = (win_min.as_secs_f64() - floor_s).max(DQ_FLOOR_SECS);
        // Headroom covers whichever jitter evidence is larger: per-sample
        // (consecutive raw-sample differences) or per-window (consecutive
        // window-min differences) — under correlated jitter (a slow RTT
        // wave) only the window-level estimator sees the true amplitude:
        // measured at L1 C2, raw diffs ~0.85ms while window mins wander
        // ~3-5ms between updates. Both are consecutive-difference EWMAs,
        // hence shift-robust: a standing queue contributes ONE transition
        // sample, not a persistent inflation, so congestion detection
        // survives (unlike a quantile-spread term, which a level shift
        // would inflate for a full window).
        let jitter = self.jitter_est.max(self.win_jitter_est);
        let dq_target = ((self.queue_mult - 1.0) * floor_s + JITTER_HEADROOM * jitter)
            .max(DQ_FLOOR_SECS);
        dq > dq_target
    }

    /// Whether a cwnd window update is due (once per SRTT).
    fn should_update(&self, now: Instant) -> bool {
        now.duration_since(self.last_cwnd_update) >= self.srtt()
    }

    /// Per-SRTT window update (gate driver semantics):
    ///   - windowed min above the queue target → backoff ×0.92, end ramp
    ///   - ramping → ×1.5 + 1
    ///   - steady state → +2
    /// Resets the queuing-delay window. Returns the new cwnd (unclamped
    /// against MIN/MAX — the caller clamps).
    fn update_cwnd(&mut self, cwnd: u32) -> u32 {
        let now = self.clock.now();
        self.last_cwnd_update = now;
        // No RTT samples since the last update → no signal, hold.
        let Some(win_min) = self.min_rtt_since_update else {
            return cwnd;
        };
        // Copa §2.2 mode switching + competitive AIMD, per-SRTT cadence,
        // BEFORE the direction test so the adapted δ drives this update's
        // law (no-op unless RWM_COPA_COMPETE && wire mode).
        self.compete_update(now);
        let above = self.queue_above_target(cwnd);
        tracing::debug!(
            cwnd,
            above,
            win_min_us = win_min.as_micros() as u64,
            floor_us = self.min_rtt.map(|d| d.as_micros() as u64),
            qfloor_us = self.queue_floor().map(|d| d.as_micros() as u64),
            jitter_us = (self.jitter_est * 1e6) as u64,
            win_jitter_us = (self.win_jitter_est * 1e6) as u64,
            srtt_us = self.srtt().as_micros() as u64,
            n_samples = self.samples_since_update,
            max_bw = self.max_bw as u64,
            bdp_anchor = self.bdp_anchor().map(|b| b.round() as u64),
            anchor_floor = self.anchor_floor(),
            wire = self.wire_mode,
            delta = self.delta,
            velocity = self.velocity,
            compete = self.in_compete,
            compete_switches = self.compete_switches,
            "copa cwnd update"
        );
        // Record this window's min in the queue-floor history.
        self.win_min_history.push_back((now, win_min));
        let cutoff = now.checked_sub(self.window_duration).unwrap_or(now);
        while self.win_min_history.front().is_some_and(|&(t, _)| t < cutoff) {
            self.win_min_history.pop_front();
        }
        // Window-level consecutive-difference jitter (see field doc).
        if let Some(prev) = self.prev_win_min {
            let diff = if win_min > prev { win_min - prev } else { prev - win_min };
            self.win_jitter_est += (diff.as_secs_f64() - self.win_jitter_est) * 0.25;
        }
        self.prev_win_min = Some(win_min);
        // Capture the wire-mode queue signal BEFORE the window reset below
        // (wire_dq_secs reads min_rtt_since_update).
        let wire_dq = if self.wire_mode { self.wire_dq_secs() } else { None };
        self.min_rtt_since_update = None;
        self.samples_since_update = 0;
        let c = cwnd as f64;
        if self.wire_mode {
            return self.wire_update_cwnd(c, above, wire_dq).round() as u32;
        }
        let next = if above {
            self.ramping = false;
            c * BACKOFF_MULT
        } else if self.ramping {
            c * RAMP_GAIN + 1.0
        } else {
            // Steady state: gentle additive probe, but when a trusted BtlBw
            // anchor says cwnd is below the BDP target (post-backoff trough),
            // pull toward it proportionally — a fast catch-up that decays
            // into the +2 probe as cwnd → target (paper Section 12.6). Only
            // ever RAISES the step above +2 (the anchor never suppresses).
            match self.bdp_anchor() {
                Some(bdp) => {
                    let target = ANCHOR_RECOVERY_GAIN * bdp;
                    if c < target {
                        c + (ANCHOR_PULL_ALPHA * (target - c)).max(ADDITIVE_STEP)
                    } else {
                        c + ADDITIVE_STEP
                    }
                }
                None => c + ADDITIVE_STEP,
            }
        };
        next.round() as u32
    }

    /// Wire-mode per-SRTT update (feat/copa-wire-signal): Copa's actual
    /// dynamics (Arun & Balakrishnan, NSDI 2018 §2) at per-SRTT granularity.
    ///
    ///   target_rate = 1/(δ·d_q)  ⇒  target_cwnd = srtt/(δ·d_q)
    ///   direction   = up if cwnd ≤ target_cwnd, else down
    ///   step        = v/δ symbols per SRTT (Copa: cwnd ± v/(δ·cwnd) per
    ///                 ACK × cwnd ACKs/RTT = v/δ per RTT); v doubles once
    ///                 per update after the direction has persisted ≥ 3
    ///                 updates (Copa §2.2 hysteresis), resets to 1 on a
    ///                 flip.
    ///
    /// Equilibrium: rate = μ (the bottleneck) at a standing queue of 1/δ
    /// packets; the ±v/δ dither around it drains the queue to ~empty every
    /// few updates, which is what keeps the 10 s RTT floor fresh (no
    /// ProbeRTT needed). The legacy +2 additive probe IS this law's up-step
    /// at δ = 0.5, v = 1 — continuity with the P1 semantics.
    ///
    /// Two safety caps, both continuity-preserving:
    ///   - up-step ≤ cwnd (at most double per SRTT — Copa's slow-start
    ///     bound), and the ramp itself stays ×1.5+1 until first above;
    ///   - down-step ≤ max(measured queue μ̂·d_q, (1−0.92)·cwnd): draining
    ///     more than the standing queue would empty the PIPE (utilization
    ///     loss for nothing) — the queue cap lands the trough at ≈BDP; the
    ///     0.08·cwnd floor keeps drain progress alive before a BtlBw
    ///     estimate exists.
    fn wire_update_cwnd(&mut self, c: f64, above: bool, wire_dq: Option<f64>) -> f64 {
        let next = self.wire_update_cwnd_uncapped(c, above, wire_dq);
        // Coupling cap (MEASURED at the L1 c2 smoke, v1/v2 of this law):
        // Copa's fixed point is cwnd* = BDP + 1/δ. Once cwnd exceeds the
        // sender's outstanding store cap, it is DECOUPLED from the wire —
        // the delay signal cannot punish further growth (the queue no
        // longer grows with cwnd) and the jitter-clamped d_q keeps voting
        // "up", so cwnd ratchets to MAX_CWND and the burst tail-drops the
        // path qdisc (cwnd 4 000–7 800 observed vs fixed point ≈ 300).
        // Cap at BDP + 2/δ — the fixed point plus one dither amplitude
        // (the up phase still probes a full base step past equilibrium).
        // max_bw's windowed-MAX under-reads only app-limited flows, and an
        // under-read cap is still > BDP at Bulk's 1/δ (the pipe stays
        // fillable and the samples can read the true rate back up — not
        // the §12.11 circular-cap case, which capped AT the anchor).
        match self.bdp_anchor() {
            Some(bdp) => next.min(bdp + 2.0 / self.delta),
            None => next,
        }
    }

    fn wire_update_cwnd_uncapped(&mut self, c: f64, above: bool, wire_dq: Option<f64>) -> f64 {
        if self.ramping {
            if above {
                // Ramp exit: same gentle ×0.92 first step as the legacy /
                // per-ACK fast exit; the velocity law takes over next update.
                self.ramping = false;
                return c * BACKOFF_MULT;
            }
            return c * RAMP_GAIN + 1.0;
        }
        let Some(dq) = wire_dq else {
            return c; // no queue signal this window — hold
        };
        let up = !above;
        if self.last_dir_up == Some(up) {
            self.dir_streak = self.dir_streak.saturating_add(1);
            if self.dir_streak >= 3 {
                // Direction persisted ≥ 3 updates → double the velocity
                // (bounded so the step can never exceed the cwnd ceiling).
                self.velocity =
                    (self.velocity * 2.0).min(self.delta * PathState::MAX_CWND as f64);
            }
        } else {
            self.dir_streak = 1;
            self.velocity = 1.0;
        }
        self.last_dir_up = Some(up);
        let step = (self.velocity / self.delta).max(1.0);
        if up {
            c + step.min(c)
        } else {
            let queue_syms = if self.max_bw > 0.0 {
                self.max_bw * dq
            } else {
                f64::INFINITY
            };
            let drain = step.min(queue_syms.max(c * (1.0 - BACKOFF_MULT)));
            (c - drain).max(0.0)
        }
    }

    /// Immediate backoff (ramp fast-exit or decode-failure congestion):
    /// ×0.92, end the ramp, restart the update window.
    fn backoff(&mut self, cwnd: u32) -> u32 {
        self.ramping = false;
        self.min_rtt_since_update = None;
        self.samples_since_update = 0;
        self.last_cwnd_update = self.clock.now();
        // Wire mode: a backoff is a down move — reset the velocity streak so
        // the next windowed update re-measures direction from v = 1.
        self.velocity = 1.0;
        self.dir_streak = 1;
        self.last_dir_up = Some(false);
        (cwnd as f64 * BACKOFF_MULT).round() as u32
    }

    /// Classic Copa rate target — DIAGNOSTIC ONLY (the cwnd dynamics above
    /// are the ramp/backoff scheme; this is the closed-form equilibrium).
    ///
    /// Units:
    ///   dq   [s]         = SRTT − floor, clamped ≥ DQ_FLOOR_SECS
    ///   rate [symbols/s] = 1 / (COPA_DELTA [1/symbols] × dq [s])
    ///   cwnd [symbols]   = rate [symbols/s] × SRTT [s]
    ///
    /// (The pre-P7 code multiplied rate by min_rtt and doubled it during
    /// startup; rate × SRTT is the pipe-plus-standing-queue the rate can
    /// keep full over one feedback delay.)
    fn copa_target_cwnd(&self) -> u32 {
        let floor = self.min_rtt.unwrap_or(DEFAULT_SRTT).as_secs_f64();
        let srtt = self.srtt().as_secs_f64();
        let dq = (srtt - floor).max(DQ_FLOOR_SECS);
        // `delta` == COPA_DELTA in legacy mode (byte-identical diagnostic);
        // in wire mode it is the hint-mapped δ the live law targets.
        let rate = 1.0 / (self.delta * dq); // symbols per second
        let cwnd = rate * srtt; // symbols
        (cwnd.round() as u32).clamp(PathState::MIN_CWND, PathState::MAX_CWND)
    }

    /// BtlBw×RTprop BDP estimate in symbols — the active recovery anchor
    /// (paper Section 12.6), or None until it is trustworthy.
    ///
    /// UNITS: max_bw [symbols/s] × min_rtt [s] = symbols (in-flight the
    /// bottleneck rate keeps outstanding over one propagation RTT).
    ///
    /// Gated on ANCHOR_MIN_SAMPLES delivery samples AND a min-RTT sample:
    /// `max_bw` is a windowed MAX of coarse ACK-batch rates with no
    /// per-packet/app-limited accounting, so a handful of samples (or no
    /// RTT floor yet) is not enough to steer cwnd. It STRUCTURALLY
    /// underestimates a warm-up/app-limited flow, which is exactly why it
    /// is only ever used to RAISE cwnd (recovery target + floor), never as
    /// a cap — an underestimate can only fail to help, never suppress.
    fn bdp_anchor(&self) -> Option<f64> {
        if self.bw_samples.len() < ANCHOR_MIN_SAMPLES || self.max_bw <= 0.0 {
            return None;
        }
        let rtprop = self.min_rtt?.as_secs_f64();
        Some(self.max_bw * rtprop)
    }

    /// The per-path bottleneck rate (symbols/s) for scheduler consumers (the
    /// percap store-cap law reads it via `btlbw_sym_per_s`): the pure
    /// windowed-MAX `max_bw`, gated on ANCHOR_MIN_SAMPLES like `bdp_anchor`
    /// (byte-identical to `bdp_anchor()/RTprop`).
    ///
    /// Historical note (DEPRECATION REGISTER, removed 2026-07-27): the
    /// RWM_RATE_WIRE/RWM_RATE_Q robust-quantile de-noise branch was refuted by
    /// its own structural argument — decode-clocked samples are mostly-low, so
    /// the windowed-MAX is near-correct and ANY sub-max quantile UNDER-reads
    /// and throttles ("Slow-Path Anchor Diagnosis STEP 3", 2026-07-13). The
    /// rate-signal need was met by the honest-anchor family (ADR-0061).
    fn effective_btlbw(&self) -> Option<f64> {
        if self.bw_samples.len() < ANCHOR_MIN_SAMPLES {
            return None;
        }
        if self.max_bw > 0.0 { Some(self.max_bw) } else { None }
    }

    /// The cwnd floor from the BtlBw anchor (symbols), or None if not yet
    /// established. A floor, NOT a cap — it only ratchets cwnd UP toward the
    /// pipe, so a stale/underestimated BtlBw cannot suppress the window
    /// (paper Section 12.6). Caller clamps against MAX_CWND.
    fn anchor_floor(&self) -> Option<u32> {
        self.bdp_anchor()
            .map(|bdp| (ANCHOR_FLOOR_GAIN * bdp).round() as u32)
    }

    /// Expire samples older than the sliding window.
    fn expire_old_samples(&mut self, now: Instant) {
        let cutoff = now.checked_sub(self.window_duration).unwrap_or(now);
        self.bw_evict_before(cutoff);
        while self.rtt_samples.front().is_some_and(|s| s.timestamp < cutoff) {
            self.rtt_samples.pop_front();
        }
    }

    fn set_queue_mult(&mut self, mult: f64) {
        self.queue_mult = mult;
    }

    /// Wire mode: re-derive δ when the protocol hint changes (paired with
    /// `set_queue_mult` from `PathState::set_hint`). No-op in legacy mode —
    /// δ stays COPA_DELTA there.
    fn set_hint_delta(&mut self, hint: ProtocolHint) {
        if self.wire_mode {
            self.delta = copa_delta_for_hint(hint);
            // A hint change re-bases the competitive AIMD: drop to default
            // mode at the new base price; the detector re-enters competitive
            // within 5 RTTs if the buffer-filler evidence persists.
            self.delta_base = self.delta;
            self.in_compete = false;
        }
    }

    /// The raw-fed windowed-min echo ratio (`RWM_HONEST_K`), or None with
    /// the gate off (the legacy smoothed-at-refresh feed stays the only K
    /// source). 1.0 before the first raw sample — identical to a cold
    /// legacy tracker, so warm-up has no behavior cliff.
    fn k_raw_ratio(&self) -> Option<f64> {
        if self.k_raw_on {
            Some(self.k_raw.k())
        } else {
            None
        }
    }

    /// Test hook: force the raw-sample K feed on (bypasses the
    /// process-global env cache, which other tests' env vars could race).
    #[cfg(test)]
    fn force_k_raw(&mut self) {
        self.k_raw_on = true;
    }

    /// Test hook: force the O(1) max-filter read (bypasses the
    /// process-global env cache).
    #[cfg(test)]
    fn force_bw_o1(&mut self) {
        self.bw_o1 = true;
    }

    /// Test hook: force wire mode with an explicit δ. Unit tests must not
    /// depend on the process-global env cache (`copa_wire_active`), which
    /// other tests' env vars could race.
    #[cfg(test)]
    fn force_wire(&mut self, delta: f64) {
        self.wire_mode = true;
        self.delta = delta;
        self.delta_base = delta;
    }

    /// Test hook: enable the competitive mode switching on top of a forced
    /// wire mode (bypasses the process-global env caches, which other tests'
    /// env vars could race).
    #[cfg(test)]
    fn force_compete(&mut self) {
        debug_assert!(self.wire_mode, "compete rides the wire law");
        self.compete_on = true;
    }

    fn reset(&mut self) {
        let clock = self.clock.clone();
        let queue_mult = self.queue_mult;
        let delta_base = self.delta_base;
        *self = Self::new(clock, ProtocolHint::Auto);
        self.queue_mult = queue_mult; // hint survives a path reset
        // Wire mode: the hint-mapped BASE δ survives a path reset; a
        // competitive-mode δ does not (fresh path, fresh detection — the
        // detector re-enters competitive within 5 RTTs if warranted).
        self.delta = delta_base;
        self.delta_base = delta_base;
    }

    /// Read the current min_rtt estimate (for diagnostics/benchmarking).
    pub fn min_rtt(&self) -> Option<Duration> {
        self.min_rtt
    }

    /// (diag/slow-path-anchor) Snapshot of the rate-sample anchor DIAG counters:
    /// (sent, applimited_sent, attributions, no_record, rej_interval, rej_zero,
    /// rej_applimited, generated, bw_fill).  Observation only.
    fn rs_diag(&self) -> (u64, u64, u64, u64, u64, u64, u64, u64, usize) {
        (
            self.rs_sent_count,
            self.rs_applimited_sent,
            self.rs_attributions,
            self.rs_no_record,
            self.rs_rej_interval,
            self.rs_rej_zero,
            self.rs_rej_applimited,
            self.rs_generated,
            self.bw_samples.len(),
        )
    }
}

/// Per-path state tracked by the scheduler.
pub struct PathState {
    pub id: PathId,
    pub estimator: LossEstimator,
    /// Congestion window in symbols
    pub cwnd: u32,
    /// Symbols currently in flight
    pub in_flight: u32,
    /// Per-path SOURCE outstanding gauge (BLEST in_flight_i, feat/per-path-
    /// estimator): source symbols whose DAPS placement committed them to THIS
    /// path (`source_path_map`) but which the receiver has not yet
    /// acked/decoded.  Charged at placement (`charge_src`) and released on
    /// per-path ack attribution (`on_src_delivered`), so it tracks TRUE
    /// sent-not-acked ON THIS PATH — the quantity the BLEST BDP cap bounds
    /// (`in_flight_i ≤ gain·BtlBw_i·RTprop_i`).  Distinct from `in_flight`
    /// (the coded-symbol budget released by time-expiry): the cap needs a
    /// source-unit outstanding that matches the source-unit BtlBw the ack
    /// attribution feeds `copa.record_delivery`.  Only driven under DAPS.
    pub src_inflight: u32,
    /// Whether the path is considered usable
    pub active: bool,
    /// Slow-start threshold (kept for legacy test compatibility)
    pub ssthresh: u32,
    /// Whether we are in slow-start phase (Copa startup)
    pub in_slow_start: bool,
    /// Last time we received an RTCP-style report or any data from this path
    pub last_report: Instant,
    /// Maximum datagram size discovered for this path
    pub max_datagram_size: Option<usize>,
    /// Copa delay-based congestion control state.
    copa: CopaState,
    /// Token-bucket pacing: symbols sendable right now. Replenished at
    /// cwnd/SRTT symbols per second, capped at the burst allowance
    /// max(10, cwnd/8). May go NEGATIVE: the drain in net/mod.rs is
    /// batch-granular and lets the final batch overdraft; the debt is
    /// repaid before the next drain, so the average rate stays cwnd/SRTT.
    pace_tokens: f64,
    /// Last time pacing tokens were replenished.
    last_pace_refill: Instant,
    /// FIFO log of in_flight charges (charge instant, symbols) backing the
    /// time-based release in `expire_in_flight`. Invariant (best-effort):
    /// sum of counts == in_flight; direct writes to `in_flight` (tests,
    /// the leak-guard backstop) break it temporarily and all helpers
    /// saturate rather than trust it.
    in_flight_log: VecDeque<(Instant, u32)>,
    /// Pool-anchor honest dual-store law (`RWM_POOL_ANCHOR`, goal-gate
    /// "Ship The Wins 1"): per-path hygiene-grade SEND-interval rate anchor
    /// (ADR-0061 `SendRateAnchor`: ≈SRTT/2 buckets, windowed-max ≈ 8·SRTT,
    /// clock-gap discard + quarantine), fed by this path's own send events
    /// at `charge_in_flight` — every wire send on the path (source,
    /// redundant, retransmit). Burst-immune by construction (Δt spans the
    /// SEND interval on the sender's clock), it is the N ≥ 2 pooled-store
    /// cap's rate input; it feeds NOTHING else (Copa cwnd dynamics keep the
    /// legacy `record_delivery` path byte-identically — the −22…−27 c7
    /// RS-composition price stays unreachable).
    send_anchor: crate::control::SendRateAnchor,
    /// Whether the send-anchor feed is on (resolved once at construction
    /// from `pool_anchor_active()`; test-forcible). OFF ⇒ `charge_in_flight`
    /// is byte-identical to the prior-default path (no clock read, no
    /// bucket work) — the A/B decomposition arm stays cost-honest.
    pool_anchor_feed: bool,
    /// Delivery-clocked pool rate anchor (`RWM_POOL_DELIV`, goal-gate "Ship
    /// The Wins 1b" arm A): the BBR `GenerateRateSample` statistic on this
    /// path's aggregate send/delivery cursors, as a SHADOW estimator. Fed at
    /// `charge_in_flight` (sends) and at the ack arm (`on_pool_delivery`).
    /// Its ONLY consumer is `pool_rate_anchor()` → the N ≥ 2 pool law: the
    /// Copa cwnd feed, `max_bw`, `bdp_anchor`/`anchor_floor`, pacing and
    /// `src_inflight` are all structurally unreachable from here. It is the
    /// one rate source bounded by delivered-packet PHYSICS rather than by the
    /// sender's own admission gate — attempt 1's measured binder.
    deliv_anchor: crate::control::DeliveryRateAnchor,
    /// Whether the delivery-anchor feed is on (resolved once at construction
    /// from `pool_deliv_active()`; test-forcible). OFF ⇒ both feed sites do
    /// no work at all (cost-honest A/B, the `pool_anchor_feed` precedent).
    pool_deliv_feed: bool,
    /// Whether the honest anchor-floor bound is on (`RWM_FLOOR_BOUND`, arm B;
    /// resolved once at construction, test-forcible). OFF ⇒
    /// `clamp_cwnd_with_anchor` is byte-identical to the shipped path.
    floor_bound: bool,
    /// ack-merge (`RWM_ACK_MERGE`, goal-gate "Unlock The Default 1"): the
    /// sender-side CURSOR for the v6 `WindowAck` cumulative counters. The
    /// merged ack carries the receiver's per-path running
    /// `(total_expected, total_received)`; the sender diffs them against
    /// these to recover exactly the `(expected_count, received_count)` pair
    /// the suppressed legacy `Ack` used to deliver per batch. Cumulative and
    /// diffed rather than per-ack sums so a DROPPED control datagram costs
    /// nothing — the next ack carries the whole outstanding delta, which is
    /// the property that makes merging safe on a lossy ack path.
    ack_cum_expected: u64,
    /// See [`Self::ack_cum_expected`].
    ack_cum_received: u64,
    /// `RWM_LOSS_SENT_TRUTH` (default OFF): the SENDER-side cursor over this
    /// path's own `PathStats::symbols_sent`. See
    /// [`Self::sender_truth_loss_delta`].
    loss_sent_cursor: u64,
    /// THE ONE-SIDED-CLAMP WITNESS (paper §16.63's successor hypothesis,
    /// observation only). Counts of samples where the receiver's cumulative
    /// cursor LED the sender's own symbol counter, their summed magnitude, and
    /// the positive loss mass actually fed — see [`Self::loss_clamp_witness`].
    loss_clamp_over_n: u64,
    loss_clamp_over_mass: u64,
    loss_clamp_loss_mass: u64,
    /// `RWM_LOSS_SENT_TRUTH`: the paired cursor over the receiver's clean
    /// per-path `total_received`. Separate from [`Self::ack_cum_received`]
    /// because the two arms advance independently (the legacy cursor pair
    /// keeps driving `release_in_flight` in BOTH arms — see
    /// [`Self::sender_truth_loss_delta`]).
    loss_recv_cursor: u64,
    /// THE REFUTED CANDIDATE's cursor pair (see
    /// [`Self::sender_truth_release_delta`]) — retained as the negative
    /// datum's only reproduction path, with no production call site.
    release_sent_cursor: u64,
    /// See [`Self::release_sent_cursor`].
    release_recv_cursor: u64,
    /// `RWM_RELEASE_1TO1` (default OFF, resolved once at construction): the
    /// lost-symbol release is the charge log's OWN RFC 9002 time-threshold
    /// sweep, and the contaminated `expected - received` term at the ack arms
    /// is not applied. See [`release_1to1_active`] and
    /// [`Self::expire_in_flight`].
    release_1to1: bool,
    /// Injectable clock
    clock: Arc<dyn Clock>,
}

impl PathState {
    /// Minimum congestion window in symbols (never go below this).
    /// 8 rather than the historical 2: an L1 run on a real emulated link
    /// showed the old collapse-to-target dynamics crawling at 2 symbols/RTT
    /// after the first burst; the floor guarantees a usable trickle that
    /// keeps RTT samples (and thus recovery) flowing.
    pub const MIN_CWND: u32 = 8;
    /// Initial congestion window.
    pub const INITIAL_CWND: u32 = 10;
    /// Maximum congestion window.
    pub const MAX_CWND: u32 = 10_000;
}

impl PathState {
    pub fn new(id: PathId, clock: Arc<dyn Clock>) -> Self {
        Self::new_with_hint(id, clock, ProtocolHint::Auto)
    }

    /// Create path state with a protocol hint (sets Copa-lite's
    /// hint-coupled queue target, paper Section 12.4 / P1).
    pub fn new_with_hint(id: PathId, clock: Arc<dyn Clock>, hint: ProtocolHint) -> Self {
        let now = clock.now();
        Self {
            id,
            estimator: LossEstimator::new(),
            cwnd: Self::INITIAL_CWND,
            in_flight: 0,
            src_inflight: 0,
            active: true,
            ssthresh: 64,
            in_slow_start: true,
            last_report: now,
            max_datagram_size: None,
            copa: {
                let mut c = CopaState::new(clock.clone(), hint);
                c.rs_trace_path = id; // DIAG label only (RWM_RS_TRACE prints)
                c
            },
            pace_tokens: Self::INITIAL_CWND as f64,
            last_pace_refill: now,
            in_flight_log: VecDeque::new(),
            send_anchor: crate::control::SendRateAnchor::new(),
            pool_anchor_feed: pool_anchor_active(),
            deliv_anchor: crate::control::DeliveryRateAnchor::new(),
            pool_deliv_feed: pool_deliv_active(),
            floor_bound: floor_bound_active(),
            ack_cum_expected: 0,
            ack_cum_received: 0,
            loss_sent_cursor: 0,
            loss_clamp_over_n: 0,
            loss_clamp_over_mass: 0,
            loss_clamp_loss_mass: 0,
            loss_recv_cursor: 0,
            release_sent_cursor: 0,
            release_recv_cursor: 0,
            release_1to1: release_1to1_active(),
            clock,
        }
    }

    /// `RWM_LOSS_SENT_TRUTH` (default OFF) — the CROSS-PATH-CLEAN loss pair.
    ///
    /// THE LAW, on one line:
    ///
    /// ```text
    ///   eps_p  =  1  -  d(cum_received_p) / d(symbols_sent_p)
    /// ```
    ///
    /// Provenance of both operands: **measured, locally, per path.**
    /// `symbols_sent_p` is `PathStats::symbols_sent` — incremented once at
    /// every wire handoff on this path (source, repair and retransmit alike;
    /// `emit_source.rs:489/581/933`, `net/mod.rs:5735/5792/5906/7282/7722`),
    /// so it is the SENDER's own exact count of what it put on this path.
    /// `cum_received_p` is the receiver's `PathBatchTracker::total_received`,
    /// already on the wire in every v6 `WindowAck` — a pure count of arrivals
    /// on this path, with no sequence arithmetic in it.
    ///
    /// **What it replaces and why.** The shipped pair takes `expected` from
    /// `PathBatchTracker::total_expected` (`net/mod.rs:7576`), which estimates
    /// it as `gap × received` across a **GLOBAL** `batch_seq` gap
    /// (`batch_counter` is one connection-wide `AtomicU64`). At N ≥ 2 a single
    /// path's batch-seq sequence is mostly the OTHER path's symbols, so the
    /// gap is a SCHEDULING artefact and the ratio reads loss that never
    /// happened. Measured on the wire (goal-gate "Ack-Cadence Measurement
    /// (VM)" READOUT 4): `ce/cr` = 2.05 at c7 and 5.59 on c8's slow leg
    /// against realized packet loss of 0.55% and 1.96% — i.e. eps_hat 0.51
    /// and 0.82 against truth, **37–93x**. The same ledgers' `cr/s` column is
    /// exactly this law's reciprocal and reads 0.94–1.01 at those cells.
    ///
    /// **Why deltas of cumulatives and not a snapshot ratio.** Both operands
    /// are monotone cumulative counters, so a dropped ack costs nothing (the
    /// next one carries the whole outstanding delta) — the same property that
    /// makes [`Self::ack_merge_counter_delta`] safe. The cursors only ever
    /// move FORWARD, so a reordered/stale ack yields `(0, 0)`.
    ///
    /// **The named residual: in-flight lag.** `symbols_sent` counts a symbol
    /// at handoff, `cum_received` counts it ~RTT later, so the sent cursor
    /// leads by ≈ in_flight. The offset is CONSTANT in steady state, hence
    /// the DELTAS are unbiased; what it costs is a one-BDP over-read during
    /// the opening ramp (decaying, and in the same direction the legacy pair
    /// errs, so it is never a new over-read) and a matching under-read at the
    /// tail. Bounded by `sender_truth_loss_delta_is_unbiased_under_a_constant_
    /// in_flight_lag`. Subtracting `in_flight` here would remove the offset
    /// but couple this estimate to a gauge whose release is driven by the
    /// contaminated pair — the circularity is deliberately not taken.
    ///
    /// `cum_received == 0` is the same "no counter payload" sentinel the
    /// merged-ack cursor uses (the two timer-driven `WindowAck` sites
    /// broadcast to every live path and carry no per-path counter).
    /// `received` is clamped to `expected` so the derived loss count can
    /// never underflow when the lag runs the other way.
    pub fn sender_truth_loss_delta(
        &mut self,
        symbols_sent: u64,
        cum_received: u64,
    ) -> (u32, u32) {
        if cum_received == 0 {
            return (0, 0);
        }
        let d_expected = symbols_sent.saturating_sub(self.loss_sent_cursor);
        let d_received = cum_received.saturating_sub(self.loss_recv_cursor);
        if d_expected == 0 && d_received == 0 {
            return (0, 0);
        }
        self.loss_sent_cursor = self.loss_sent_cursor.max(symbols_sent);
        self.loss_recv_cursor = self.loss_recv_cursor.max(cum_received);
        // ── THE ONE-SIDED-CLAMP WITNESS (observation only) ───────────────
        // Goal-gate item 3c, REDIRECTED: the RFC 6675 denominator hypothesis
        // was refuted on the code (both operands count retransmits — a matched
        // pair). The labelled successor hypothesis for the T rung's 20 %
        // over-read, and for why it SURVIVES at N = 1 where the attribution
        // error it was built to repair cannot exist, is THIS `min`: two clocks
        // (the sender's own symbol counter and the receiver's cumulative echo)
        // jitter against each other, so `d_received > d_expected` whenever the
        // receiver's cursor momentarily leads. The clamp RECTIFIES every such
        // sample to zero loss rather than to negative loss — and rectifying a
        // zero-mean jitter is a POSITIVE BIAS at any path count, which is
        // exactly the shape of the surviving-at-N=1 result.
        //
        // Three counters, exactly what scoring the hypothesis needs:
        //   (a) how often the receiver led, (b) by how much summed,
        //   (c) the positive loss mass fed, for the ratio.
        // No behaviour change and no wire change: the clamp is untouched and
        // nothing here is read by a decision.
        if d_received > d_expected {
            LCW_OVER_N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            LCW_OVER_MASS.fetch_add(d_received - d_expected, std::sync::atomic::Ordering::Relaxed);
            self.loss_clamp_over_n = self.loss_clamp_over_n.saturating_add(1);
            self.loss_clamp_over_mass =
                self.loss_clamp_over_mass.saturating_add(d_received - d_expected);
        }
        LCW_LOSS_MASS.fetch_add(
            d_expected.saturating_sub(d_received.min(d_expected)),
            std::sync::atomic::Ordering::Relaxed,
        );
        self.loss_clamp_loss_mass = self
            .loss_clamp_loss_mass
            .saturating_add(d_expected.saturating_sub(d_received.min(d_expected)));
        let cap = u32::MAX as u64;
        (
            d_expected.min(cap) as u32,
            d_received.min(d_expected).min(cap) as u32,
        )
    }

    /// The one-sided-clamp witness, whole — `(samples where the receiver's
    /// cursor LED, their summed magnitude, the positive loss mass fed)`.
    ///
    /// The scoreable statistic is `over_mass / loss_mass`: if two-clock jitter
    /// rectification is the mechanism behind §16.63's 20×-and-survives-at-N=1
    /// result, the rectified mass is a large fraction of the loss the
    /// estimator was fed, at EVERY path count including N = 1. Surfaced on the
    /// DIAG/ACKDIAG line as `lcw=<n>/<over_mass>/<loss_mass>`.
    pub fn loss_clamp_witness(&self) -> (u64, u64, u64) {
        (self.loss_clamp_over_n, self.loss_clamp_over_mass, self.loss_clamp_loss_mass)
    }

    /// [`Self::sender_truth_loss_delta`] for the LEGACY per-batch `Ack` arm,
    /// whose `received_count` is a per-batch count rather than a cumulative
    /// counter. Same law, same cursors: the received side is accumulated
    /// here instead of arriving pre-summed on the wire. (`received_count` was
    /// never the contaminated operand — only `expected_count` was.)
    pub fn sender_truth_loss_batch(
        &mut self,
        symbols_sent: u64,
        received_in_batch: u32,
    ) -> (u32, u32) {
        let cum = self.loss_recv_cursor.saturating_add(received_in_batch as u64);
        self.sender_truth_loss_delta(symbols_sent, cum)
    }

    /// **THE REFUTED CANDIDATE — retained as the negative datum's only
    /// reproduction path, with NO production call site.**
    ///
    /// The shape the `fix/accounting-ledger` dispatch proposed for the
    /// lost-symbol release: the same clean operand pair
    /// `RWM_LOSS_SENT_TRUTH` gave the loss ESTIMATOR, applied to the LEDGER —
    ///
    /// ```text
    ///   released_lost_p  =  d(symbols_sent_p)  -  d(cum_received_p)
    /// ```
    ///
    /// **It is refuted ARITHMETICALLY, and the identity is short enough to
    /// state here.** Charge every send and release `d_received` (delivery arm)
    /// plus this term, and the sums telescope:
    ///
    /// ```text
    ///   in_flight  =  sent      -  [ recv + (sent - sent_0) - (recv - recv_0) ]
    ///              =  sent_0 - recv_0
    /// ```
    ///
    /// a CONSTANT — the outstanding at cursor init, which with cursors
    /// starting at zero is **zero**. The gauge is pinned on the floor exactly
    /// as the contaminated `expected - received` pins it, so the defect is
    /// reproduced rather than fixed, and lazily initialising the cursors only
    /// freezes the gauge at a different constant.
    ///
    /// The reason is structural, not a tuning failure: `d_sent - d_received`
    /// is `loss + delta(outstanding)`, so releasing on it releases the
    /// in-flight window itself. **Item 3's trick works for a RATIO** — where
    /// a constant lag cancels in the deltas and leaves the estimate unbiased —
    /// **and does not transfer to a LEDGER**, which needs the per-symbol
    /// identity that the striping destroyed (item 3's own candidate (b): "a
    /// seq that arrived NOWHERE cannot be attributed to a path").
    ///
    /// Reproduced and bounded by `sender_truth_release_pins_the_gauge_on_the_
    /// floor`; the shipped shape is [`release_1to1_active`].
    ///
    /// Cursor mechanics, for the reproduction: `cum_received == 0` is the "no
    /// counter payload" sentinel; cursors only move FORWARD, so a reordered or
    /// duplicated ack yields 0; and the lost count saturates at zero rather
    /// than going negative, so it can never invent budget.
    pub fn sender_truth_release_delta(
        &mut self,
        symbols_sent: u64,
        cum_received: u64,
    ) -> u32 {
        if cum_received == 0 {
            return 0;
        }
        let d_sent = symbols_sent.saturating_sub(self.release_sent_cursor);
        let d_received = cum_received.saturating_sub(self.release_recv_cursor);
        if d_sent == 0 && d_received == 0 {
            return 0;
        }
        self.release_sent_cursor = self.release_sent_cursor.max(symbols_sent);
        self.release_recv_cursor = self.release_recv_cursor.max(cum_received);
        d_sent.saturating_sub(d_received).min(u32::MAX as u64) as u32
    }

    /// [`Self::sender_truth_release_delta`] for the LEGACY per-batch `Ack`
    /// arm, whose `received_count` is a per-batch count rather than a
    /// cumulative counter. Same law, same cursors: the received side is
    /// accumulated here instead of arriving pre-summed on the wire.
    pub fn sender_truth_release_batch(
        &mut self,
        symbols_sent: u64,
        received_in_batch: u32,
    ) -> u32 {
        let cum = self
            .release_recv_cursor
            .saturating_add(received_in_batch as u64);
        self.sender_truth_release_delta(symbols_sent, cum)
    }

    /// ack-merge (`RWM_ACK_MERGE`): advance the v6 cumulative-counter cursor
    /// and return this ack's `(expected, received)` delta — exactly the pair
    /// the suppressed legacy `ControlMessage::Ack` carried per batch.
    ///
    /// `cum_received == 0` is the "no counter payload" sentinel (the two
    /// timer-driven `WindowAck` sites broadcast one message to every live
    /// path and cannot carry a per-path counter), and a reordered/stale ack
    /// yields `(0, 0)` because the cursor only ever moves FORWARD. Both cases
    /// are no-ops, which is what makes the re-homed consumers idempotent
    /// under ack loss, duplication and reordering.
    ///
    /// `received` is clamped to `expected`: the receiver's `expected` is a
    /// batch-gap ESTIMATE (`PathBatchTracker`), so a shrinking gap estimate
    /// must never make the derived loss count underflow.
    pub fn ack_merge_counter_delta(&mut self, cum_expected: u64, cum_received: u64) -> (u32, u32) {
        if cum_received == 0 {
            return (0, 0);
        }
        let d_expected = cum_expected.saturating_sub(self.ack_cum_expected);
        let d_received = cum_received.saturating_sub(self.ack_cum_received);
        if d_received == 0 && d_expected == 0 {
            return (0, 0);
        }
        self.ack_cum_expected = self.ack_cum_expected.max(cum_expected);
        self.ack_cum_received = self.ack_cum_received.max(cum_received);
        let cap = u32::MAX as u64;
        (
            d_expected.min(cap) as u32,
            d_received.min(d_expected).min(cap) as u32,
        )
    }

    /// Update the hint-coupled queue target when the protocol hint changes.
    pub fn set_hint(&mut self, hint: ProtocolHint) {
        self.copa.set_queue_mult(queue_target_mult(hint));
        self.copa.set_hint_delta(hint);
    }

    /// Correction rate r = epsilon / (1 - epsilon).
    /// The (1-epsilon) denominator accounts for corrections-of-corrections.
    /// See paper Section 13.4.
    pub fn correction_rate(&self) -> f64 {
        let eps = self.estimator.loss_rate();
        if eps >= 1.0 {
            return f64::INFINITY;
        }
        eps / (1.0 - eps)
    }

    /// Effective delivery time E_i = RTT_i/2 + epsilon_i × t_recovery_i.
    ///
    /// t_recovery is the expected time to recover a lost symbol. We approximate
    /// it as one RTT (ARQ round-trip) weighted by loss probability. When FEC
    /// is likely to recover (low loss), t_recovery is small. When ARQ is needed
    /// (high loss or aged symbol), t_recovery approaches one full RTT.
    ///
    /// See paper Section 13.5.
    pub fn effective_delivery_time(&self) -> f64 {
        let rtt_secs = self.estimator.rtt().as_secs_f64();
        let eps = self.estimator.loss_rate();
        // t_recovery ≈ RTT (one round-trip for ARQ recovery)
        let t_recovery = rtt_secs;
        rtt_secs / 2.0 + eps * t_recovery
    }

    /// Load-DEPENDENT expected frontier-completion-time `E_i(load)` (seconds) —
    /// the always-on load term of the RWM placement law (paper Section 16.3).
    /// The time a symbol handed to this path now takes to reach the receiver:
    ///
    ///   E_i(load) = in_flight_i / (cwnd_i/SRTT_i)   ← drain the current backlog
    ///             + SRTT_i / 2                        ← one-way propagation
    ///             + eps_i · RTT_i                     ← expected loss recovery
    ///
    /// The queue term uses the path's live PACING RATE (`cwnd/SRTT`), so a
    /// backlog on a low-capacity / high-RTT path costs proportionally MORE real
    /// time than the same backlog on the fast path — this is what makes the law
    /// water-fill by CAPACITY (arrival rate matches drain rate at equilibrium),
    /// not by equal window-fraction (which over-loads the slow path and, on a
    /// reliable in-order stream, collapses the frontier — MEASURED at C8:
    /// dimensionless fill gave 3.4 Mbit/s vs 15.4 fast-path-alone). It rises
    /// CONTINUOUSLY with `in_flight` (past cwnd under overdraft), so spillover
    /// is a smooth equilibrium, not a regime switch. Because it is the delivery
    /// latency of a reliable in-order stream (the completion cost itself), it
    /// carries UNIT weight independent of the protocol hint.
    pub fn expected_delivery_load(&self) -> f64 {
        self.expected_delivery_load_at(self.srtt().as_secs_f64())
    }

    /// `expected_delivery_load` with the path's latency anchor supplied by
    /// the caller, in seconds. Same formula, one free variable: `E_i` is
    /// linear in SRTT_i, and the ONLY thing `RWM_COLD_PLACE` changes is which
    /// measurement stands in for SRTT_i on a leg that has never had a sample.
    /// `expected_delivery_load()` is this at `srtt()`, so no caller of the
    /// nullary form can observe a difference.
    pub fn expected_delivery_load_at(&self, srtt: f64) -> f64 {
        let eps = self.estimator.loss_rate();
        let cwnd = self.cwnd.max(1) as f64;
        let queue_wait = (self.in_flight as f64 / cwnd) * srtt;
        queue_wait + srtt / 2.0 + eps * srtt
    }

    /// Effective goodput: throughput * (1 - loss_rate).
    /// This is what actually gets through to the receiver.
    pub fn effective_goodput(&self) -> f64 {
        let throughput = self.estimator.throughput();
        let loss = self.estimator.loss_rate();
        throughput * (1.0 - loss)
    }

    /// Available capacity: cwnd - in_flight.
    pub fn available(&self) -> u32 {
        self.cwnd.saturating_sub(self.in_flight)
    }

    /// Spare capacity as a fraction of in-flight traffic.
    ///
    /// Returns `(cwnd - in_flight) / in_flight` when in_flight > 0.
    /// Used by the FEC rate controller to ensure repairs don't exceed
    /// available link capacity (the "never hurts" guarantee).
    ///
    /// Returns f64::INFINITY when in_flight is 0 (unlimited spare capacity).
    pub fn spare_capacity(&self) -> f64 {
        if self.in_flight == 0 {
            return f64::INFINITY;
        }
        self.cwnd.saturating_sub(self.in_flight) as f64 / self.in_flight as f64
    }

    /// Copa-lite congestion control: handle acknowledgements.
    ///
    /// The cwnd update runs once per SRTT (gate driver cadence):
    ///   - windowed-min RTT above the queue target → ×0.92, end ramp
    ///   - ramping (before the first backoff) → ×1.5 + 1
    ///   - steady state → +2
    ///
    /// During the ramp the backoff check additionally runs per ACK, so the
    /// exponential phase ends within one feedback message of the first
    /// standing-queue evidence rather than waiting out the SRTT window.
    pub fn on_ack(&mut self, acked: u32) {
        let _rate = self.copa.record_delivery(acked);
        self.on_delivery_signal();
    }

    /// The cwnd-dynamics half of `on_ack`, WITHOUT the legacy ack-interval
    /// `record_delivery` sample (feat/copa-sole-cc). Callers that account
    /// delivery through the BBR-correct send-interval rate sampler
    /// (`on_src_delivered_seq`) use this so the windowed-max BtlBw filter is
    /// fed ONLY clean send-interval samples — the ack-interval Δt spikes
    /// (batched acks / frontier jumps) otherwise latch an over-read anchor
    /// that pins cwnd above BDP via the anchor floor (§16.13's ×145-class
    /// over-read, reproduced ×19 on the plain-mode L0 smoke). The update
    /// rules themselves are byte-identical to `on_ack`'s.
    pub fn on_delivery_signal(&mut self) {
        let now = self.clock.now();

        if self.copa.ramping
            && self.copa.samples_since_update >= 3
            && self.copa.queue_above_target(self.cwnd)
        {
            // Fast ramp exit: gentle ×0.92, NOT a collapse to a
            // rate-formula target (the pre-P7 bug: the initial burst
            // inflated its own RTT samples, dq exploded, and the target
            // dropped to the floor on the very first burst). Requires
            // ≥3 samples of evidence: a partial window's min can be a
            // single jittery sample, and one draw from the jitter tail
            // must not end the exponential ramp (L1 C2 finding).
            self.cwnd = self.copa.backoff(self.cwnd);
        } else if self.copa.should_update(now) {
            self.cwnd = self.copa.update_cwnd(self.cwnd);
        }

        self.clamp_cwnd_with_anchor();

        // Sync legacy fields
        self.in_slow_start = self.copa.ramping;
        if !self.in_slow_start && self.ssthresh > self.cwnd {
            self.ssthresh = self.cwnd;
        }
    }

    // --- Per-path send-interval rate sampling (the CopaFeed's anchor) ---
    //
    // The plain-mode Copa delivery feed (feat/copa-sole-cc / RWM_PLAIN_RS,
    // ADR-0061) attributes each newly-acked SOURCE seq to the path that
    // carried it and drives that path's BBR-correct send-interval sampler,
    // so BtlBw_i / the per-path BDP anchor establish per path.

    /// Charge `n` source symbols to this path's outstanding gauge at
    /// placement time (the seq→path commitment).  Pairs with
    /// `on_src_delivered_seq`.
    pub fn charge_src(&mut self, n: u32) {
        self.src_inflight = self.src_inflight.saturating_add(n);
    }

    /// BBR `SendPacket` for the rate-sample anchor: record this source seq's
    /// send-time state so its ack yields a send-interval rate sample.
    /// Called at placement/send time; pairs with `on_src_delivered_seq`.
    pub fn on_src_sent(&mut self, seq: u64, app_limited: bool) {
        self.copa.rs_on_sent(seq, app_limited);
    }

    /// Per-path ack attribution under the BBR rate-sample anchor: release the
    /// SOURCE outstanding gauge and feed the delivery-rate max-filter a
    /// SEND-INTERVAL sample (robust to ack-aggregation / a standing queue).
    pub fn on_src_delivered_seq(&mut self, seq: u64) {
        self.src_inflight = self.src_inflight.saturating_sub(1);
        self.copa.rs_on_delivered(seq);
    }

    /// Current per-path SOURCE outstanding (BLEST in_flight_i).
    pub fn src_inflight(&self) -> u32 {
        self.src_inflight
    }

    /// Whether the per-path Copa BtlBw/BDP anchor has established (≥
    /// ANCHOR_MIN_SAMPLES delivered-rate samples AND a min-RTT sample) — the
    /// per-path DIAG "established?" signal.
    pub fn anchor_established(&self) -> bool {
        self.copa.bdp_anchor().is_some()
    }

    /// (diag/slow-path-anchor) Per-path rate-sample anchor DIAG counters:
    /// (snapshotted-at-send, of-which-app-limited, acks-attributed-here,
    /// no-send-record, rejected[interval<MinRTT], rejected[zero-delivered],
    /// rejected[app-limited], samples-generated, windowed-max-fill).
    /// Read only under RWM_DIAG; never gates control.
    pub fn rs_diag(&self) -> (u64, u64, u64, u64, u64, u64, u64, u64, usize) {
        self.copa.rs_diag()
    }

    /// Copa-lite congestion control: handle loss events.
    ///
    /// Loss alone does NOT reduce cwnd — channel loss is FEC's job, not
    /// CC's (paper Section 12). The key insight:
    ///   - Loss + FEC recovered → wireless/random loss → ignore entirely
    ///   - Decode failure + standing queue above target → real congestion
    ///     → backoff ×0.92 (same speed as the delay backoff; a decode
    ///     failure adds no extra information beyond the delay signal)
    ///   - Decode failure + empty queue → borderline FEC under-provision,
    ///     not congestion → end the ramp and step down by 1
    pub fn on_loss(&mut self, fec_recovered: bool) {
        if fec_recovered {
            return;
        }
        if self.copa.queue_above_target(self.cwnd) {
            self.cwnd = self.copa.backoff(self.cwnd);
        } else {
            self.copa.ramping = false;
            self.cwnd = self.cwnd.saturating_sub(1);
        }
        self.clamp_cwnd_with_anchor();
        self.in_slow_start = false;
        if self.ssthresh > self.cwnd {
            self.ssthresh = self.cwnd;
        }
    }

    /// Feed an RTT measurement into Copa state.
    /// Call this when processing ACKs/reports that include RTT.
    pub fn record_rtt_sample(&mut self, rtt: Duration) {
        self.copa.record_rtt(rtt);
    }

    /// Wire-level loss evidence for the Copa competitive AIMD
    /// (feat/copa-compete): pass the pass-through shim's cumulative
    /// `congestion_events` counter for this path. No-op unless
    /// RWM_COPA_COMPETE is active.
    pub fn on_wire_congestion_events(&mut self, cumulative: u64) {
        self.copa.note_congestion_events(cumulative);
    }

    /// Copa competitive-mode DIAG snapshot (feat/copa-compete):
    /// (switching enabled, currently competitive, competitive entries,
    /// live δ, base δ). Observation only.
    pub fn copa_compete_diag(&self) -> (bool, bool, u64, f64, f64) {
        (
            self.copa.compete_on,
            self.copa.in_compete,
            self.copa.compete_switches,
            self.copa.delta,
            self.copa.delta_base,
        )
    }

    /// Test hook: force the wire-clocked δ-mapped update law with an
    /// explicit δ (bypasses the process-global env gate).
    #[cfg(test)]
    pub(crate) fn force_wire_for_test(&mut self, delta: f64) {
        self.copa.force_wire(delta);
    }

    /// Test hook: enable Copa §2.2 competitive mode switching (requires a
    /// prior `force_wire_for_test`).
    #[cfg(test)]
    pub(crate) fn force_compete_for_test(&mut self) {
        self.copa.force_compete();
    }

    /// Test hook (GOAL "HONEST INPUTS" phase 3, the c1 lock-blocking probe
    /// in `net::tests`): force the O(1) honest windowed-max deque ON for
    /// this path — the `RWM_HONEST_ANCHOR` (DH-arm) configuration — without
    /// touching the process-global env gate. Value-identical either way
    /// (`bw_mono_front_equals_full_window_fold`); this selects the DH arm's
    /// COST so the bench prices the fixed attribution path, not the fold.
    #[cfg(test)]
    pub(crate) fn force_honest_anchor_for_test(&mut self) {
        self.copa.force_bw_o1();
    }

    /// Test accessor (same probe): the cwnd anchor FLOOR
    /// (`CopaState::anchor_floor` — ANCHOR_FLOOR_GAIN × BtlBw × RTprop).
    /// The floor only ratchets cwnd UP, so it is the wire-bound sender's
    /// resting cwnd LOWER bound — the quantity the saturation predicate
    /// (`available() == 0`, the `active_paths()` filter) compares
    /// outstanding against.
    #[cfg(test)]
    pub(crate) fn anchor_floor_for_test(&self) -> Option<u32> {
        self.copa.anchor_floor()
    }


    /// Read Copa's current min_rtt estimate (for diagnostics/benchmarking).
    pub fn copa_min_rtt(&self) -> Option<Duration> {
        self.copa.min_rtt()
    }

    /// Smoothed RTT estimate (Copa's EWMA; the loss estimator's EWMA as a
    /// fallback before the first Copa sample).
    pub fn srtt(&self) -> Duration {
        match self.copa.srtt {
            Some(s) => s,
            None => self.estimator.rtt(),
        }
    }

    /// `srtt()` ONLY IF it is a MEASUREMENT — `None` for a path that has
    /// never had an RTT sample, where `srtt()` returns the 50-ms
    /// `DEFAULT_SRTT`-class seed instead (hygiene rule 1: `srtt()` cannot
    /// tell a consumer which of the two it just handed over).
    ///
    /// Structurally identical to `srtt()`, term for term — Copa's EWMA
    /// first, the loss estimator's as the pre-Copa fallback — so wherever
    /// this returns `Some(d)`, `srtt() == d` exactly. Consumers that must not
    /// price an unmeasured path with a constant read this
    /// (`Scheduler::place_costs`, `RWM_COLD_PLACE`).
    pub fn srtt_measured(&self) -> Option<Duration> {
        match self.copa.srtt {
            Some(s) => Some(s),
            None => self.estimator.rtt_measured(),
        }
    }

    /// The path's OWN measured RTT jitter, in microseconds — the derived
    /// patience floor's second term (goal-gate "Unlock The Default 2").
    ///
    /// Copa's consecutive-difference estimate (RFC 3550-style EWMA at gain
    /// 1/8, shift-robust: a standing queue shifts all samples and leaves the
    /// consecutive differences at jitter scale) widened by its window-level
    /// twin — `max(jitter_est, win_jitter_est)`, exactly the combination the
    /// Copa backoff threshold already uses, so patience and the CC read the
    /// SAME jitter. Before Copa has an RTT sample both are 0 and the loss
    /// estimator's RFC 3550 §A.8 interarrival jitter stands in.
    ///
    /// Measured, never configured: there is no env knob on this path.
    /// The RTT distribution's standard-deviation estimate (µs) — paper
    /// §16.69. `√(EWMA[(rtt − srtt)²])`, the SECOND moment Cantelli's
    /// distribution-free bound is stated in. `None` before any sample, so the
    /// derived clock gets an information-availability fallback, not a mode.
    pub fn rtt_sigma_us(&self) -> Option<u64> {
        if self.copa.rtt_var_sq <= 0.0 {
            return None;
        }
        Some((self.copa.rtt_var_sq.sqrt() * 1e6) as u64)
    }

    /// How many RTT samples have been folded into [`rtt_sigma_us`]'s EWMA —
    /// the σ gauge's WARM-UP EVIDENCE, reported beside it as
    /// `sig_us=<µs>/n<count>` in the `[DIAG]` line.
    ///
    /// It is NOT gated on `ANCHOR_MIN_SAMPLES`: that constant gates the
    /// delivered-rate anchor and has nothing to say about this statistic. See
    /// `CopaState::rtt_var_n` for what the count means and why it is reported
    /// rather than used as a threshold.
    ///
    /// [`rtt_sigma_us`]: Self::rtt_sigma_us
    pub fn rtt_sigma_samples(&self) -> u64 {
        self.copa.rtt_var_n
    }

    // ---------------------------------------------------------------------
    // THE THREE CANDIDATE DISPERSION GAUGES — goal #101 item 2, paper
    // §16.74.5's named successor. READ-ONLY, READ BY NOTHING but `[DIAG]`.
    //
    // **They are a DECOMPOSITION, not three guesses.** The shipped `sig_us`
    // carries three independent suspect properties at once, and the measured
    // 287× at `c8` cannot say which of them produced it. Each candidate moves
    // exactly one axis away from the shipped estimator, so the differences
    // between them identify the cause:
    //
    //   axis                shipped `sig_us`      `rvar`    `qsp`     `msd`
    //   ------------------  --------------------  --------  --------  --------
    //   memory              7 samples (β = 1/4)   7         L = 256   L = 256
    //   deviation enters    SQUARED               linear    rank      rank
    //   reference           lagging `srtt` EWMA   lagging   none      none
    //
    //   `rvar` vs `sig_us` : isolates the SQUARE   (memory + reference fixed)
    //   `qsp`  vs `rvar`   : isolates the MEMORY   (reference still absent)
    //   `msd`  vs `qsp`    : isolates the REFERENCE (memory + rank fixed)
    //
    // **All three render `-` before their first sample and carry their own
    // sample count beside their value**, the shipped `sig_us=<µs|->/n<count>`
    // convention — with one deliberate repair: `sig_us` returns `None` on a
    // non-positive `rtt_var_sq`, so it renders `-` for "no sample yet" AND for
    // "dispersion is exactly zero", which a parser cannot tell apart. These
    // return `None` **iff the sample set is empty** and report a genuine zero
    // as `0`. Neither is a threshold and neither gates anything: the warm-up
    // exclusions live in the battery's parser, pre-registered in goal-gate
    // "THE SIGMA ESTIMATOR — THE ACCEPTANCE BAR" clause `C3`.
    //
    // **No consumer. No gate. No default.** Nothing in the engine reads any of
    // these; the acceptance bar's battery is a later, VM-side pass.
    // ---------------------------------------------------------------------

    /// The `q`-quantile of an already-sorted slice, by the tree's own
    /// convention (`net::QuantileClockGauge::quantile`) — nearest-rank on
    /// `round((len − 1)·q)`, no interpolation, so two reads of one sample set
    /// always agree and the value is always a sample that actually occurred.
    fn cand_quantile(sorted: &[u32], q: f64) -> u64 {
        if sorted.is_empty() {
            return 0;
        }
        let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
        sorted[idx.min(sorted.len() - 1)] as u64
    }

    /// **CANDIDATE 1 — `qsp_us=`, WINDOWED QUANTILE DISPERSION, UNSCALED.**
    ///
    /// ```text
    ///     qsp  =  P90(rtt)  −  P50(rtt)      over the last L = 256 samples
    /// ```
    ///
    /// **UNSCALED, and that is a decision with a reason rather than an
    /// omission.** The obvious alternative is to divide by 1.2816 — the
    /// Gaussian value of `(P90 − P50)/σ` — and call the result a σ-equivalent.
    /// It is not done, for three reasons:
    ///
    /// 1. **The acceptance bar is scale-free.** `R_total = σ̂_p95/σ̂_p05` and
    ///    §16.74.5's `R_σ̂` are both RATIOS, so no fixed positive scaling
    ///    changes any clause of `S`. The constant would buy the bar nothing.
    /// 2. **The assumption it imports is refuted by the data it would be
    ///    applied to.** A Gaussian conversion is only meaningful on a Gaussian;
    ///    `c8` produced a σ reading of 54.836 ms at a cell whose measured `d`
    ///    is 3.298 ms and whose `RTprop` is 38 ms. That is not a Gaussian tail.
    ///    §16.69's one real virtue is being DISTRIBUTION-FREE, and scaling by a
    ///    Gaussian constant would spend exactly that.
    /// 3. **§16.69's own construction permits the quantile-native route over
    ///    this range.** Its construction line reads `W(α) = F⁻¹_X(1 − α)` — the
    ///    clock IS a quantile — and Cantelli is the distribution-free FALLBACK
    ///    for when only moments are available. An estimator that reports
    ///    quantiles directly does not need the fallback. §16.69 refuted the
    ///    direct route at the CONTRACT's `α = 10⁻⁵` (100 000 samples); over the
    ///    SWEPT range `[0.002, 0.400]` that arithmetic does not bind, and the
    ///    acceptance bar's clause `C2` records exactly where the line falls.
    ///
    /// The Gaussian constant is documented here and applied nowhere, so a
    /// future consumer that wants a σ-equivalent can multiply by `1/1.2816`
    /// with its assumption on the record: **for `X ~ N(µ, σ²)`,
    /// `P90 − P50 = 1.2816·σ`.**
    ///
    /// **Why it should beat the shipped EWMA, argued from the measured data.**
    /// It moves two axes at once. MEMORY: 256 samples against the EWMA's 7, so
    /// a reading is a property of the window and not of wherever the last
    /// seven samples happened to land. OUTLIER LEVERAGE: a quantile moves by
    /// one RANK regardless of an excursion's magnitude — `P90` over `L = 256`
    /// is unmoved by up to 25 arbitrarily large outliers, where the shipped
    /// EWMA admits one 200 ms excursion as `(200 ms)²` and needs ~16 samples
    /// to decay it below 1 %.
    ///
    /// `None` iff no sample has been recorded.
    pub fn rtt_qspread_us(&self) -> Option<u64> {
        if self.copa.rtt_win.is_empty() {
            return None;
        }
        let mut s: Vec<u32> = self.copa.rtt_win.iter().copied().collect();
        s.sort_unstable();
        Some(Self::cand_quantile(&s, 0.90) - Self::cand_quantile(&s, 0.50))
    }

    /// Samples in [`rtt_qspread_us`]'s window RIGHT NOW — **the window FILL,
    /// not the path's lifetime sample count**, and it saturates at
    /// `SIGMA_CAND_WINDOW`.
    ///
    /// That is deliberate and it follows the rule `diag.rs` already states for
    /// `sig_us`: the count must describe the sample set the value was computed
    /// from, so *"the denominator can never describe a different sample set
    /// than its numerator."* A lifetime count beside a windowed value would
    /// describe a different set. It also makes the pre-registered window-class
    /// warm-up test exact: **the window is warm iff `n == L`.**
    ///
    /// [`rtt_qspread_us`]: Self::rtt_qspread_us
    pub fn rtt_qspread_samples(&self) -> u64 {
        self.copa.rtt_win.len() as u64
    }

    /// **CANDIDATE 2 — `rvar_us=`, RFC 6298 §2's `RTTVAR`.**
    ///
    /// ```text
    ///     rvar  ←  (1 − β)·rvar  +  β·|rtt − srtt| ,     β = 1/4
    /// ```
    ///
    /// **Provenance: CITED.** β = 1/4 and the mean-deviation form are RFC 6298
    /// §2 verbatim, inherited by RFC 8985 §6.2 for RACK. Nothing here is
    /// fitted, and the shipped `rtt_var_sq` already cites the same RFC for the
    /// same gain on the second moment — so the two differ by the square alone.
    ///
    /// **It exists to be the CONTROL, not to win.** See `CopaState::rtt_mdev`:
    /// it holds memory and reference fixed against the shipped estimator and
    /// moves only the power the deviation enters at, which is the only way to
    /// attribute the 287× to outlier leverage or acquit it of that.
    ///
    /// Gaussian conversion, documented and not applied: for `X ~ N(µ, σ²)`,
    /// `E|X − µ| = √(2/π)·σ = 0.7979·σ`, so a consumer wanting a σ-equivalent
    /// multiplies by 1.2533. RFC 6298 itself does not: it uses `4·RTTVAR`
    /// directly, which is a mean-deviation multiplier and not a σ one.
    ///
    /// `None` iff no sample has been folded in.
    pub fn rtt_mdev_us(&self) -> Option<u64> {
        if self.copa.rtt_mdev_n == 0 {
            return None;
        }
        Some((self.copa.rtt_mdev * 1e6).max(0.0) as u64)
    }

    /// Samples folded into [`rtt_mdev_us`]'s EWMA. EWMA-class, so its
    /// pre-registered warm-up is `n ≥ 16` — identical to
    /// [`rtt_sigma_samples`], because it is the identical EWMA at the
    /// identical gain, fed at the identical site.
    ///
    /// [`rtt_mdev_us`]: Self::rtt_mdev_us
    /// [`rtt_sigma_samples`]: Self::rtt_sigma_samples
    pub fn rtt_mdev_samples(&self) -> u64 {
        self.copa.rtt_mdev_n
    }

    /// **CANDIDATE 3 — `msd_us=`, THE REFERENCE-FREE DISPERSION.** Median
    /// absolute SUCCESSIVE difference over the same window `L = 256`:
    ///
    /// ```text
    ///     msd  =  median( |rtt_i − rtt_{i−1}| )        over the window
    /// ```
    ///
    /// **THIS CANDIDATE IS ARGUED FROM THE MEASURED σ PROCESS, AND THE
    /// ARGUMENT IS THAT THE 287× IS NOT WHAT IT LOOKS LIKE.** The obvious
    /// reading of the `c8` spread is loss-burst contamination: a lossy cell
    /// produces RTT excursions and the estimator inhales them. **The committed
    /// ledgers refute that reading on their own numbers.** From the
    /// plain-window primitives table, per-cell loss `p` against the measured
    /// rep-to-rep σ spread:
    ///
    /// ```text
    ///     cell   p (per leg)        σ reps (ms)              sup/inf
    ///     c1     0.00015            0.013 / 0.035 / 0.046      3.5×
    ///     sc2    0.0040             0.335 / 0.492 / 1.113      3.3×
    ///     c7     0.0056 / 0.0053    0.480 / 0.499 / 2.321      4.8×
    ///     c8L    0.0039 / 0.0165    0.343 / 0.665 / 4.088     11.9×
    ///     c8     0.0040 / 0.0184    0.191 / 3.140 / 54.836   287×
    /// ```
    ///
    /// **`sc2` and `c8`'s fast leg carry the SAME loss rate (0.0040) and their
    /// σ spreads differ by 87×.** Loss rate does not predict the spread, so a
    /// loss-window-excluding estimator would be excluding the wrong thing —
    /// and it would also need a loss signal to key on, which is a coupling this
    /// gauge has no business introducing.
    ///
    /// **What the data points at instead is the REFERENCE.** The shipped
    /// estimator's deviation is `rtt − srtt`, and `srtt` is itself an EWMA at
    /// β = 1/8 chasing the same series. When the queue takes a LEVEL SHIFT,
    /// `srtt` lags it by ~8 samples and every deviation in that window is a
    /// full step height rather than a dispersion. Squared, that is the 54.836
    /// ms reading — **a "dispersion" 1.4× the cell's own `RTprop` of 38 ms and
    /// 17× its measured `d` of 3.298 ms, which no dispersion of a stationary
    /// RTT about a tracking mean can be.** It is `srtt`'s tracking error
    /// wearing σ's clothes. The two cells with the largest spreads (`c8`,
    /// `c8L`) are also the two with the largest per-leg loss ASYMMETRY (4.6×
    /// and 4.2×), which is a standing-queue-shift generator and not a
    /// loss-rate effect.
    ///
    /// **Successive differencing cancels a level shift exactly** — that is
    /// what the statistic is FOR, and it is the tree's own idea already:
    /// `CopaState::jitter_est` uses consecutive differences and its field doc
    /// gives this exact reason (*"a standing queue shifts ALL samples and
    /// leaves the consecutive differences at jitter scale"*). This candidate is
    /// that insight applied to the dispersion estimator, with the EWMA replaced
    /// by a median so an excursion moves it by one rank instead of by its
    /// magnitude.
    ///
    /// **Provenance: CITED.** The mean/median of absolute successive
    /// differences is the standard robust scale estimator under an unknown
    /// drifting mean (von Neumann's ratio, 1941; the successive-difference
    /// variance estimator of von Neumann, Kent, Bellinson & Hart 1941), and
    /// RFC 3550 §A.8's interarrival jitter is the same construction.
    ///
    /// Gaussian conversion, documented and not applied: for iid `X ~ N(µ, σ²)`
    /// the successive difference is `N(0, 2σ²)`, so
    /// `median|Δ| = 0.6745·√2·σ = 0.9539·σ` — within 5 % of unity, which is a
    /// convenience and not a licence to treat it as σ.
    ///
    /// `None` until at least two samples exist (one difference).
    pub fn rtt_msd_us(&self) -> Option<u64> {
        if self.copa.rtt_win.len() < 2 {
            return None;
        }
        let mut d: Vec<u32> = self
            .copa
            .rtt_win
            .iter()
            .zip(self.copa.rtt_win.iter().skip(1))
            .map(|(a, b)| a.abs_diff(*b))
            .collect();
        d.sort_unstable();
        Some(Self::cand_quantile(&d, 0.50))
    }

    /// SUCCESSIVE DIFFERENCES available to [`rtt_msd_us`] right now — the
    /// window fill minus one, saturating at `SIGMA_CAND_WINDOW − 1`.
    ///
    /// It is the difference count and not the sample count because the
    /// differences are what the median is taken over, and the count beside a
    /// value must describe that value's own sample set.
    ///
    /// [`rtt_msd_us`]: Self::rtt_msd_us
    pub fn rtt_msd_samples(&self) -> u64 {
        (self.copa.rtt_win.len() as u64).saturating_sub(1)
    }

    pub fn rtt_jitter_us(&self) -> u64 {
        let copa_j = self.copa.jitter_est.max(self.copa.win_jitter_est);
        if copa_j > 0.0 {
            (copa_j * 1e6) as u64
        } else {
            self.estimator.jitter_us().max(0.0) as u64
        }
    }

    /// Classic Copa equilibrium target, for diagnostics (see
    /// `CopaState::copa_target_cwnd` for the units derivation).
    pub fn copa_target_cwnd(&self) -> u32 {
        self.copa.copa_target_cwnd()
    }

    /// BtlBw×RTprop BDP anchor estimate in symbols, once established
    /// (paper Section 12.6). None during warm-up / before a min-RTT
    /// sample. Diagnostic/benchmarking accessor.
    pub fn copa_bdp_anchor(&self) -> Option<f64> {
        self.copa.bdp_anchor()
    }

    /// RTprop (Copa windowed-min RTT) for this path (None during warm-up).
    pub fn min_rtt(&self) -> Option<Duration> {
        self.copa.min_rtt()
    }

    /// goal-gate "Honest Inputs" (`RWM_HONEST_K`): the RAW-sample windowed-min
    /// echo-ratio for this path, or None with the gate off. `Some` values are
    /// what the honest-cap / three-term collectors substitute for the
    /// smoothed-at-refresh K (`k_raw.unwrap_or(legacy)` — ONE formula, the
    /// gate changes which measured series feeds it); None ⇒ every consumer
    /// byte-identical to the legacy feed.
    pub fn k_raw(&self) -> Option<f64> {
        self.copa.k_raw_ratio()
    }

    /// BtlBw (bottleneck rate) for this path in symbols/second — the path's
    /// own drain rate = anchor / RTprop = (BtlBw·RTprop)/RTprop.  This is the
    /// BBR-style per-path pacing rate: the slow path's future-offset data
    /// emitted at BtlBw_slow flows at the slow path's drain rate WITHOUT
    /// queuing, so no standing queue (bufferbloat) builds.  None during warm-up
    /// (same trustworthiness gate as `copa_bdp_anchor`).
    pub fn btlbw_sym_per_s(&self) -> Option<f64> {
        // Warm gate: an RTprop sample must exist (same trustworthiness gate as
        // `copa_bdp_anchor`).  The rate itself is `effective_btlbw` — the pure
        // windowed-MAX (byte-identical to the old `anchor/RTprop`).
        self.copa.min_rtt()?;
        self.copa.effective_btlbw()
    }

    /// Clamp cwnd to [MIN_CWND, MAX_CWND] and then raise it to the BtlBw
    /// anchor floor if one is established (paper Section 12.6). The floor
    /// only ratchets cwnd UP (never a cap) and is itself bounded by
    /// MAX_CWND, so an over-read BtlBw cannot exceed the hard ceiling.
    fn clamp_cwnd_with_anchor(&mut self) {
        self.cwnd = self.cwnd.clamp(Self::MIN_CWND, Self::MAX_CWND);
        if let Some(floor) = self.copa.anchor_floor() {
            // Honest anchor-floor BOUND (RWM_FLOOR_BOUND, goal-gate "Ship The
            // Wins 1b" arm B): the legacy floor rides the ack-interval
            // `max_bw`, which over-reads ×10-class under ack bunching (339–500k
            // measured vs ≈8–12k truth ⇒ cwnd 5860 vs 1779). Bound it by the
            // honest send-anchor rate the engine already measures. Still a
            // FLOOR, never a cap: `cwnd.max(...)` below is unchanged, and with
            // the send anchor cold the bound is the legacy value verbatim.
            let floor = if self.floor_bound {
                match (self.send_rate_anchor(), self.copa.min_rtt()) {
                    (Some(sr), Some(rtp)) => {
                        let honest = ANCHOR_FLOOR_GAIN * sr * rtp.as_secs_f64();
                        floor.min(honest.round().max(0.0) as u32)
                    }
                    _ => floor,
                }
            } else {
                floor
            };
            self.cwnd = self.cwnd.max(floor.min(Self::MAX_CWND));
        }
    }

    // --- Token-bucket pacing (paper Section 12.5, gate driver P1) ---
    //
    // UNITS: tokens are SYMBOLS. Refill rate = cwnd [symbols] / SRTT [s]
    // = symbols/second; burst allowance = max(10, cwnd/8) symbols.

    /// Replenish pacing tokens for elapsed wall time.
    pub fn pace_refill(&mut self) {
        let now = self.clock.now();
        let elapsed = now.duration_since(self.last_pace_refill).as_secs_f64();
        self.last_pace_refill = now;
        let srtt = self.srtt().as_secs_f64().max(1e-3);
        let rate = self.cwnd as f64 / srtt; // symbols per second
        let burst = (self.cwnd as f64 / 8.0).max(10.0);
        self.pace_tokens = (self.pace_tokens + rate * elapsed).min(burst);
    }

    /// Current pacing token balance (symbols; may be negative — see field).
    pub fn pace_tokens(&self) -> f64 {
        self.pace_tokens
    }

    /// Consume tokens for `n` symbols just sent (may push balance negative).
    pub fn consume_pace_tokens(&mut self, n: u32) {
        self.pace_tokens -= n as f64;
    }

    /// Time until at least one pacing token is available at the current
    /// refill rate (zero if a token is already available).
    pub fn pace_delay(&self) -> Duration {
        if self.pace_tokens >= 1.0 {
            return Duration::ZERO;
        }
        let srtt = self.srtt().as_secs_f64().max(1e-3);
        let rate = (self.cwnd as f64 / srtt).max(1.0); // symbols per second
        Duration::from_secs_f64((1.0 - self.pace_tokens) / rate)
    }

    // --- in_flight budget accounting (P7 follow-up 2) ---
    //
    // in_flight is a BUDGET GAUGE (symbols committed: interleaver + pacing
    // carry + wire), charged exactly once per symbol at SCHEDULE time and
    // released by ACK feedback. ACKs are best-effort datagrams: a lost ACK
    // strands its release forever, and stranded budget compounds until the
    // TUN gate jams (L1 finding: the gate cycled at the 2s leak-guard
    // cadence instead of the RTT). The FIFO charge log makes releases
    // robust: budget older than max(4×SRTT, 250ms) is delivered-or-lost
    // either way (RFC 9002-style time-threshold, at gauge granularity) and
    // expires. Pacing (cwnd/SRTT tokens) remains the actual rate limiter,
    // so an early expiry can only let the encoder run ahead, never the
    // wire.

    /// Charge `n` symbols against the in_flight budget (at schedule time).
    pub fn charge_in_flight(&mut self, n: u32) {
        if n == 0 {
            return;
        }
        self.in_flight = self.in_flight.saturating_add(n);
        let now = self.clock.now();
        // Pool-anchor feed (RWM_POOL_ANCHOR): every wire send on this path
        // is a send-process sample for the honest dual-store anchor. O(1)
        // amortized; gate resolved once at construction — the =0 arm skips
        // entirely (cost-honest A/B).
        if self.pool_anchor_feed {
            let srtt = self.srtt();
            self.send_anchor.on_send(now, n as u64, srtt);
        }
        // Delivery-anchor SEND cursor (RWM_POOL_DELIV, arm A): the same wire
        // sends, recorded as (instant, cumulative count) so a later delivery
        // event can resolve its send spacing without a per-seq key.
        if self.pool_deliv_feed {
            self.deliv_anchor.on_send(now, n as u64);
        }
        self.in_flight_log.push_back((now, n));
    }

    /// The pool-anchor SEND rate for this path (symbols/s) — the GAP-ROBUST
    /// WINDOWED MEAN (`SendRateAnchor::mean_rate`; the pre-battery
    /// amendment: an admission-gated sender's refill bursts latch a
    /// windowed-max, measured sr=53k vs ≈8.9k truth) — or None before the
    /// first surviving bucket / with the feed off. The N ≥ 2 pooled-store
    /// cap law's honest rate input (goal-gate "Ship The Wins 1").
    /// Read-only: consumes no sample, owns no cwnd dynamics.
    pub fn send_rate_anchor(&self) -> Option<f64> {
        if !self.pool_anchor_feed {
            return None;
        }
        self.send_anchor.mean_rate(self.clock.now(), self.srtt())
    }

    /// (gaps detected, buckets discarded) for the pool-anchor sampler — the
    /// DIAG hygiene gauges.
    pub fn send_anchor_stats(&self) -> (u64, u64) {
        self.send_anchor.stats()
    }

    /// One DELIVERY event for the shadow delivery-clocked anchor
    /// (`RWM_POOL_DELIV`, arm A): `delivered` symbols confirmed received and
    /// `lost` symbols confirmed gone on this path. Both advance the accounted
    /// cursor (a lost symbol left the wire too — that is what keeps the
    /// delivery cursor aligned with the send cursor); only `delivered` enters
    /// the rate numerator. Feeds NOTHING but `pool_rate_anchor()`: no cwnd,
    /// no `max_bw`, no pacing, no `src_inflight`.
    ///
    /// `gap_quarantined` is the process-clock stall verdict already computed
    /// at the ack site (ADR-0061): a poisoned event is dropped here exactly as
    /// the RTT/rate feeds beside it drop it.
    pub fn on_pool_delivery(&mut self, delivered: u32, lost: u32, gap_quarantined: bool) {
        if !self.pool_deliv_feed || gap_quarantined {
            return;
        }
        let now = self.clock.now();
        let (rtprop, srtt) = (self.copa.min_rtt(), self.srtt());
        self.deliv_anchor
            .on_delivery(now, delivered as u64, lost as u64, rtprop, srtt);
    }

    /// THE POOL LAW'S RATE INPUT (goal-gate "Ship The Wins 1b"):
    /// `max(delivery-clocked windowed-max, send-interval ratcheted mean)`.
    ///
    /// ONE formula, no branch, no mode bit: both terms are honest LOWER
    /// BOUNDS on this path's bottleneck rate (the delivery term because its
    /// Δt is `max(send_elapsed, ack_elapsed)` with a ≥ RTprop floor; the send
    /// term because a time-normalized mean of real sends cannot exceed what
    /// flowed), and the pool law wants the bottleneck rate — so the max of
    /// two lower bounds is the estimator, and adding the delivery term can
    /// only raise the pool, never lower it. That ordering is deliberate: it
    /// makes arm A ≥ arm (attempt 1) at every instant, so a measured c7
    /// difference is attributable to exactly the delivery term.
    ///
    /// With `RWM_POOL_DELIV` off this is byte-identical to
    /// `send_rate_anchor()` (attempt 1's law); with `RWM_POOL_ANCHOR` off it
    /// is None (the legacy law runs).
    pub fn pool_rate_anchor(&self) -> Option<f64> {
        let send = self.send_rate_anchor();
        let deliv = if self.pool_deliv_feed {
            self.deliv_anchor.rate(self.clock.now(), self.copa.min_rtt())
        } else {
            None
        };
        match (send, deliv) {
            (Some(s), Some(d)) => Some(s.max(d)),
            (Some(s), None) => Some(s),
            (None, Some(d)) => Some(d),
            (None, None) => None,
        }
    }

    /// The DELIVERY-clocked term alone (DIAG gauge `dr=`): the mechanism
    /// witness that separates arm A from attempt 1 in the logs.
    pub fn deliv_rate_anchor(&self) -> Option<f64> {
        if !self.pool_deliv_feed {
            return None;
        }
        self.deliv_anchor.rate(self.clock.now(), self.copa.min_rtt())
    }

    /// (accepted, short-rejected, gaps, discarded) for the delivery anchor —
    /// DIAG gauges proving the mechanism executed and how its guards fired.
    pub fn deliv_anchor_stats(&self) -> (u64, u64, u64, u64) {
        self.deliv_anchor.stats()
    }

    /// Test hook: force the pool-anchor feed regardless of the process-global
    /// env cache (unit tests must not depend on it — the `force_wire`
    /// pattern).
    #[cfg(test)]
    pub fn force_pool_anchor_feed(&mut self, on: bool) {
        self.pool_anchor_feed = on;
    }

    /// Test hook: force the delivery-anchor feed (`RWM_POOL_DELIV`).
    #[cfg(test)]
    pub fn force_pool_deliv_feed(&mut self, on: bool) {
        self.pool_deliv_feed = on;
    }

    /// Test hook: force the honest anchor-floor bound (`RWM_FLOOR_BOUND`).
    #[cfg(test)]
    pub fn force_floor_bound(&mut self, on: bool) {
        self.floor_bound = on;
    }

    /// Test hook: force the 1:1 release (`RWM_RELEASE_1TO1`). Unit tests must
    /// not depend on the process-global env cache — the `force_wire` pattern.
    #[cfg(test)]
    pub fn force_release_1to1(&mut self, on: bool) {
        self.release_1to1 = on;
    }

    /// Release `n` symbols of budget (ACK feedback: received or
    /// gap-inferred lost). Pops the OLDEST charges first.
    pub fn release_in_flight(&mut self, n: u32) {
        self.in_flight = self.in_flight.saturating_sub(n);
        let mut remaining = n;
        while remaining > 0 {
            match self.in_flight_log.front_mut() {
                Some((_, c)) if *c > remaining => {
                    *c -= remaining;
                    remaining = 0;
                }
                Some((_, c)) => {
                    remaining -= *c;
                    self.in_flight_log.pop_front();
                }
                None => break,
            }
        }
    }

    /// Expire budget charged longer than the horizon ago: its ACK (or the
    /// loss evidence) would have arrived by now — the datagram was delivered
    /// with the ACK lost, or lost with no later batch to reveal the gap.
    /// Either way it is no longer on the wire.
    ///
    /// This sweep is **1:1 with the charge by construction** — it pops the
    /// very `in_flight_log` entries `charge_in_flight` pushed.
    ///
    /// LEGACY horizon: `max(4 x SRTT, 250 ms)`, roughly a decade past the
    /// scale at which a symbol's fate is decided, which makes this a backstop
    /// and leaves the operative release to the contaminated
    /// `expected - received` term at the ack arms.
    ///
    /// `RWM_RELEASE_1TO1` horizon: RFC 9002 §6.1.2's kTimeThreshold,
    /// `9/8 x SRTT`, floored at the same kGranularity analog the recovery
    /// plane's own time threshold uses ([`crate::net::mp_time_threshold_split`],
    /// [`crate::net::NACK_RETX_COOLDOWN_FLOOR_US`]) — the engine's OWN
    /// judgement about when a symbol is lost, applied to the budget it charged
    /// for that symbol. No new constant; see [`release_1to1_active`].
    pub fn expire_in_flight(&mut self) {
        if self.in_flight_log.is_empty() {
            return;
        }
        let horizon = if self.release_1to1 {
            let srtt_us = self.srtt().as_micros() as u64;
            Duration::from_micros(
                crate::net::mp_time_threshold_split(
                    srtt_us,
                    srtt_us,
                    crate::net::NACK_RETX_COOLDOWN_FLOOR_US,
                )
                .0,
            )
        } else {
            (self.srtt() * 4).max(IN_FLIGHT_EXPIRY_MIN)
        };
        let now = self.clock.now();
        while let Some(&(t, c)) = self.in_flight_log.front() {
            if now.duration_since(t) < horizon {
                break;
            }
            self.in_flight = self.in_flight.saturating_sub(c);
            self.in_flight_log.pop_front();
        }
    }
}

/// The multipath scheduler.
///
/// Uses the interpolated objective function from paper Section 13.8:
///   minimize: w_lat × SUM(x_i × E_i) + w_bw × SUM(x_i × r_i)
/// where E_i is effective delivery time and r_i is correction rate per path.
///
/// Source placement is BLOCK-granular (paper Section 13.8 in-order coupling
/// refinement, L2 ws1): one schedule() call = one FEC block = one delivery
/// unit, and under the cross-block in-order delivery contract a block's
/// delivery time is the MAX over the paths its source symbols touch — the
/// linear per-symbol objective silently assumed independent delivery.
/// Measured at L1 C8 (100mbit/10ms + 20mbit/40ms): blocks striped across
/// both paths completed at mean 189 ms vs 17.5 ms for fast-path-only blocks,
/// and 92% of in-order head-of-line waits were caused by blocks touching the
/// slow path. Whole-block affinity bounds the damage to the y_i fraction of
/// blocks actually assigned to the slow path (smooth WRR on B_eff_i).
pub struct Scheduler {
    paths: HashMap<PathId, PathState>,
    clock: Arc<dyn Clock>,
    /// Global correction deficit tracker (paper Section 13.4).
    pub deficit: CorrectionDeficit,
    /// Scheduling weights from protocol hint.
    weights: SchedulingWeights,
    /// Protocol hint — also sets Copa-lite's queue target on each path
    /// (paper Section 12.4 / P1).
    hint: ProtocolHint,
    /// Block-granular source affinity (see struct docs). On by default;
    /// `false` restores per-symbol greedy striping (ablation).
    block_affinity: bool,
    /// Smooth-WRR credit per path for the block-affinity pick.
    affinity_credit: HashMap<PathId, f64>,
    /// Frontier slack S (seconds) for the placement cost (goal-gate "C8
    /// Slow-Path Conversion", env `RWM_PLACE_SLACK`): the load term becomes
    /// max(0, Ê_i − S)/ref_srtt — a path whose expected delivery fits
    /// inside the in-order frontier's need-time costs nothing extra, so
    /// placement deadline-aware water-fills (backlog_i ≈ rate_i·(S−owd_i),
    /// capacity-proportional) instead of starving the slow path on the
    /// propagation term. 0.0 (the default, and whenever the gate is OFF or
    /// N = 1 or the ack-rate EWMA is cold) reproduces the shipped cost
    /// BIT-EXACTLY (max(0, x − 0) = x). Set by the plain reliable window
    /// sender on its 5 ms refresh cadence.
    place_slack_secs: f64,
    /// `RWM_COLD_PLACE` (anchor-hygiene rule 1 at the placement site) as a
    /// per-scheduler VALUE rather than a hot-path env read — the same shape
    /// `place_slack_secs` uses, and for the same reason the estimator's
    /// `force_anchor_hygiene` exists: the process-global `OnceLock` cannot
    /// hold both arms, so an A/B that must measure BOTH directions in one
    /// process (the SF bench's `Place` axis) would otherwise be impossible to
    /// write.
    /// Resolved from `cold_place_active()` at construction; `set_cold_place`
    /// overrides. See `place_costs`.
    cold_place: bool,
}

impl Scheduler {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self::new_with_hint(clock, ProtocolHint::Auto)
    }

    /// Create scheduler with protocol hint for weight configuration and
    /// the per-path Copa-lite queue target.
    pub fn new_with_hint(clock: Arc<dyn Clock>, hint: ProtocolHint) -> Self {
        Self {
            paths: HashMap::new(),
            clock,
            deficit: CorrectionDeficit::new(),
            weights: SchedulingWeights::from_hint(hint),
            hint,
            block_affinity: true,
            affinity_credit: HashMap::new(),
            place_slack_secs: 0.0,
            cold_place: cold_place_active(),
        }
    }

    /// Enable/disable block-granular source affinity (ablation switch;
    /// `false` = legacy per-symbol greedy striping).
    pub fn set_block_affinity(&mut self, enabled: bool) {
        self.block_affinity = enabled;
    }

    /// Override the cold-start placement price for this scheduler
    /// (`RWM_COLD_PLACE`; see the field docs). A/B hook: the process gate is
    /// a cached `OnceLock`, so a battery that scores BOTH arms in one process
    /// sets this instead of racing the environment.
    pub fn set_cold_place(&mut self, enabled: bool) {
        self.cold_place = enabled;
    }

    /// The cold-start placement price setting in force for this scheduler.
    pub fn cold_place(&self) -> bool {
        self.cold_place
    }

    /// Set the frontier slack S (seconds) for the placement cost (goal-gate
    /// "C8 Slow-Path Conversion", `RWM_PLACE_SLACK`). Non-finite / negative
    /// input is treated as 0 (the shipped-identical operating point).
    pub fn set_place_slack(&mut self, secs: f64) {
        self.place_slack_secs = if secs.is_finite() { secs.max(0.0) } else { 0.0 };
    }

    /// Current frontier slack S (seconds) — gauge accessor.
    pub fn place_slack(&self) -> f64 {
        self.place_slack_secs
    }

    pub fn add_path(&mut self, id: PathId) {
        self.paths
            .insert(id, PathState::new_with_hint(id, self.clock.clone(), self.hint));
    }

    pub fn remove_path(&mut self, id: PathId) {
        self.paths.remove(&id);
    }

    pub fn path_mut(&mut self, id: PathId) -> Option<&mut PathState> {
        self.paths.get_mut(&id)
    }

    pub fn path(&self, id: PathId) -> Option<&PathState> {
        self.paths.get(&id)
    }

    pub fn active_paths(&self) -> Vec<PathId> {
        self.paths
            .iter()
            .filter(|(_, p)| p.active && p.available() > 0)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Paths that are up, regardless of remaining cwnd budget.
    ///
    /// Use for CONTROL-PLANE traffic (reports, pings, BlockStart) and
    /// congestion bookkeeping. `active_paths()` filters by spare capacity
    /// (for scheduling DATA) — using it for liveness made a saturated path
    /// invisible: no pings were sent while in_flight >= cwnd, so the peer
    /// declared the path dead mid-transfer (L1 harness finding).
    pub fn live_paths(&self) -> Vec<PathId> {
        self.paths
            .iter()
            .filter(|(_, p)| p.active)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Schedule symbols across paths using the interpolated objective.
    ///
    /// Objective (paper Section 13.8):
    ///   minimize: w_lat × SUM(x_i × E_i) + w_bw × SUM(x_i × r_i)
    ///
    /// Source symbols go to paths with lowest weighted cost.
    /// Repair symbols go to paths with highest effective goodput (maximize decode probability).
    ///
    /// Returns: Vec<(PathId, Vec<WireSymbol>)>
    pub fn schedule(
        &mut self,
        source_symbols: Vec<WireSymbol>,
        repair_symbols: Vec<WireSymbol>,
    ) -> Vec<(PathId, Vec<WireSymbol>)> {
        let mut assignments: HashMap<PathId, Vec<WireSymbol>> = HashMap::new();

        let active_paths: Vec<_> = self
            .paths
            .values()
            .filter(|p| p.active && p.available() > 0)
            .collect();

        if active_paths.is_empty() {
            return vec![];
        }

        // Compute per-path cost for source scheduling using interpolated objective.
        // cost_i = w_lat × E_i + w_bw × r_i
        // Lower cost = better path for source symbols.
        let mut path_costs: Vec<(PathId, f64, u32)> = active_paths
            .iter()
            .map(|p| {
                let e_i = p.effective_delivery_time();
                let r_i = p.correction_rate();
                let r_clamped = if r_i.is_infinite() { 10.0 } else { r_i };
                let cost = self.weights.w_lat * e_i + self.weights.w_bw * r_clamped;
                (p.id, cost, p.available())
            })
            .collect();
        path_costs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // Distribute source symbols.
        //
        // Block-granular affinity (default; see struct docs): one call =
        // one block = one delivery unit — ALL source symbols ride one
        // path, picked by smooth WRR on source-carrying capacity, so a
        // block's completion time is a single path's delivery time rather
        // than the max over every path touched. The pick may exceed the
        // path's remaining cwnd budget: in_flight is charged anyway and
        // the aggregate TUN gate + token-bucket pacing provide the
        // backpressure (same contract as the old overflow-to-best-path).
        if self.block_affinity && !source_symbols.is_empty() {
            let k = source_symbols.len();
            if let Some(pid) = self.pick_affinity_path(k) {
                assignments.entry(pid).or_default().extend(source_symbols);
            }
        } else {
            // Legacy per-symbol striping: lowest-cost paths first, up to
            // each path's spare cwnd budget (ablation mode).
            let mut source_iter = source_symbols.into_iter();
            for &(pid, _, avail) in &path_costs {
                let batch: Vec<_> = source_iter.by_ref().take(avail as usize).collect();
                if batch.is_empty() {
                    break;
                }
                assignments.entry(pid).or_default().extend(batch);
            }
            // Overflow to best path
            for sym in source_iter {
                if let Some(&(pid, _, _)) = path_costs.first() {
                    assignments.entry(pid).or_default().push(sym);
                }
            }
        }

        // Repair symbols: distribute proportional to effective goodput
        let mut paths_by_goodput: Vec<_> = self
            .paths
            .values()
            .filter(|p| p.active)
            .collect();
        paths_by_goodput.sort_by(|a, b| {
            b.effective_goodput()
                .partial_cmp(&a.effective_goodput())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if !paths_by_goodput.is_empty() {
            let total_goodput: f64 = paths_by_goodput.iter().map(|p| p.effective_goodput()).sum();
            let mut repair_iter = repair_symbols.into_iter().peekable();

            if total_goodput > 0.0 {
                for path in &paths_by_goodput {
                    let fraction = path.effective_goodput() / total_goodput;
                    let count = (fraction * repair_iter.len() as f64).ceil() as usize;
                    let batch: Vec<_> = repair_iter.by_ref().take(count).collect();
                    if !batch.is_empty() {
                        assignments.entry(path.id).or_default().extend(batch);
                    }
                }
            }
            // Remaining repair symbols to best goodput path
            for sym in repair_iter {
                if let Some(path) = paths_by_goodput.first() {
                    assignments.entry(path.id).or_default().push(sym);
                }
            }
        }

        // Charge the in_flight budget at SCHEDULE time — the single charge
        // point for block-mode symbols (the paced drain in net/mod.rs must
        // NOT charge again at send time; double-charging leaked +1 per
        // symbol and jammed the TUN gate — L1 finding, P7 follow-up 2).
        for (path_id, syms) in &assignments {
            if let Some(path) = self.paths.get_mut(path_id) {
                path.charge_in_flight(syms.len() as u32);
            }
        }

        assignments.into_iter().collect()
    }

    /// Pick the path for a whole block's source symbols — the block-granular
    /// solution of the Section 13.8 objective (in-order coupling refinement):
    ///
    ///   - w_lat > 0 (Realtime/Auto): the LP solution is degenerate — the
    ///     minimum interpolated-cost path carries blocks until its cwnd
    ///     budget is exhausted, then spills to the next-cheapest (block-
    ///     granular spill; per-symbol spill is what striped blocks across
    ///     paths and made every block pay max_i D_i).
    ///   - w_lat == 0 (Bulk): demand saturates capacity, so the optimum is
    ///     y_i ∝ B_eff_i (Section 13.5, with C_i = the live Copa pacing
    ///     rate cwnd/SRTT — always defined, unlike the delivery-rate EWMA
    ///     which is cold at startup), realized by smooth WRR so consecutive
    ///     blocks alternate as evenly as the weights allow (minimal
    ///     in-order skew). Paths whose delivery time exceeds the fastest
    ///     path's by more than the in-order hold horizon are source-
    ///     ineligible (their blocks would be force-delivered as holes);
    ///     they keep serving corrections/retransmits.
    ///
    /// Paths with exhausted cwnd budget are skipped while any path has
    /// budget (WRR credit keeps accruing, so a briefly-full path gets its
    /// share back later); if ALL budgets are exhausted the pick falls back
    /// to every active path (the TUN gate is the real backpressure —
    /// schedule() must never drop a block).
    fn pick_affinity_path(&mut self, block_symbols: usize) -> Option<PathId> {
        /// In-order hold horizon (mirrors BLOCK_REORDER_MAX_HOLD in
        /// net/mod.rs): a block delivered later than this past its
        /// predecessors expires the receiver hold and surfaces as an
        /// inner-stream hole.
        const HOLD_HORIZON_SECS: f64 = 0.3;
        /// Source-eligibility threshold as a fraction of the horizon.
        /// Eligibility must gate on the block-delivery TAIL (an expiry is
        /// a tail event), but the estimate below is a median-ish model;
        /// ARQ rounds stack the tail to ~3-4x the median (measured C8:
        /// median 134 ms, expiries at 301+ ms), so a median skew above
        /// H/4 already pushes the tail past the horizon.
        const ELIGIBLE_SKEW: f64 = HOLD_HORIZON_SECS / 4.0;

        /// Expected delivery time of a WHOLE block of `k` source symbols
        /// on this path (paper 13.8 refinement, D_i): serialization at
        /// the Copa pacing rate + one-way propagation + an ARQ round at
        /// THIS path's RTT weighted by the per-BLOCK loss probability
        /// 1-(1-eps)^k. The per-symbol E_i (Section 13.5) undercounts by
        /// ~an order of magnitude here: k*eps expected losses make a
        /// recovery round nearly certain for realistic k (measured C8:
        /// eps=4.8%, k=56 -> P_blk = 0.94; B-blocks p50 94 ms vs
        /// E_B = 22 ms).
        fn block_delivery_time(p: &PathState, k: f64) -> f64 {
            let srtt = p.srtt().as_secs_f64().max(1e-3);
            let rate = (p.cwnd as f64 / srtt).max(1.0); // symbols/sec
            // Long-run loss, not the instantaneous EWMA: under GE bursts
            // the EWMA decays to ~0 between bursts and flip-flops the
            // eligibility gate open exactly long enough for the next
            // burst to catch a freshly admitted block (measured C8: B
            // still carried 12% of source, mixed-block p99 1.0 s). The
            // Beta-posterior mean spans bursts and gaps alike.
            let eps = p
                .estimator
                .loss_rate()
                .max(p.estimator.loss_rate_mean())
                .clamp(0.0, 0.99);
            let p_blk = 1.0 - (1.0 - eps).powf(k);
            k / rate + srtt / 2.0 + p_blk * 2.0 * srtt
        }

        let with_budget: Vec<&PathState> = self
            .paths
            .values()
            .filter(|p| p.active && p.available() > 0)
            .collect();
        let cands: Vec<&PathState> = if with_budget.is_empty() {
            self.paths.values().filter(|p| p.active).collect()
        } else {
            with_budget
        };
        if cands.is_empty() {
            return None;
        }

        if self.weights.w_lat > 0.0 {
            // Latency-weighted: min interpolated cost, deterministic
            // tie-break by id.
            return cands
                .iter()
                .min_by(|a, b| {
                    let ca = self.path_cost(a);
                    let cb = self.path_cost(b);
                    ca.partial_cmp(&cb)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(a.id.cmp(&b.id))
                })
                .map(|p| p.id);
        }

        // Bulk: capacity-share WRR over hold-feasible paths (HOL-cost
        // source eligibility: a path whose per-block delivery skew
        // threatens the in-order hold horizon carries NO source — it
        // keeps its repair/retransmit role, which has no ordering
        // deadline and keeps its estimators warm for re-admission).
        //
        // Eligibility is computed over ALL active paths, not just the
        // budget-filtered candidates: when the fast path's cwnd is
        // momentarily full, the slow path used to become the only
        // candidate and pass the skew test against itself (measured C8:
        // B still carried 12% of source through exactly this hole). An
        // ineligible path must not carry source even then — the pick
        // over-commits the eligible path instead (pacing keeps the wire
        // rate at cwnd/SRTT; the aggregate TUN gate closes as the
        // over-commit accumulates).
        let k = (block_symbols as f64).max(1.0);
        let active: Vec<&PathState> = self.paths.values().filter(|p| p.active).collect();
        let d_min = active
            .iter()
            .map(|p| block_delivery_time(p, k))
            .fold(f64::INFINITY, f64::min);
        let eligible: Vec<&&PathState> = active
            .iter()
            .filter(|p| block_delivery_time(p, k) - d_min <= ELIGIBLE_SKEW)
            .collect();
        let cands: Vec<&&PathState> = {
            let with_budget: Vec<&&PathState> = eligible
                .iter()
                .copied()
                .filter(|p| p.available() > 0)
                .collect();
            if with_budget.is_empty() { eligible } else { with_budget }
        };
        let mut weighted: Vec<(PathId, f64)> = cands
            .iter()
            .map(|p| {
                let srtt = p.srtt().as_secs_f64().max(1e-3);
                let rate = p.cwnd as f64 / srtt; // symbols/sec (Copa pacing rate)
                let r = p.correction_rate();
                let r = if r.is_infinite() { 10.0 } else { r };
                (p.id, rate / (1.0 + r)) // B_eff (Section 13.5)
            })
            .collect();
        weighted.sort_unstable_by(|a, b| a.0.cmp(&b.0)); // deterministic order
        let total: f64 = weighted.iter().map(|(_, w)| w).sum();
        if total <= 0.0 {
            return weighted.first().map(|&(id, _)| id);
        }
        // Drop credit for removed paths so a re-added id starts fresh.
        let paths = &self.paths;
        self.affinity_credit.retain(|id, _| paths.contains_key(id));
        let mut pick: Option<(PathId, f64)> = None;
        for &(id, w) in &weighted {
            let credit = self.affinity_credit.entry(id).or_insert(0.0);
            *credit += w / total;
            if pick.is_none() || *credit > pick.unwrap().1 {
                pick = Some((id, *credit));
            }
        }
        let (id, _) = pick?;
        *self.affinity_credit.get_mut(&id).unwrap() -= 1.0;
        Some(id)
    }

    /// Acknowledge received symbols on a path.
    pub fn ack(&mut self, path_id: PathId, count: u32) {
        if let Some(path) = self.paths.get_mut(&path_id) {
            path.release_in_flight(count);
            path.on_ack(count);
        }
    }

    /// Notify the scheduler of a loss event on a path.
    ///
    /// `fec_recovered`: true if the FEC decoder recovered the block despite
    /// the loss (random/wireless loss), false if the block failed to decode
    /// (congestion signal).
    pub fn on_loss(&mut self, path_id: PathId, fec_recovered: bool) {
        if let Some(path) = self.paths.get_mut(&path_id) {
            path.on_loss(fec_recovered);
        }
    }

    /// Record that we received a report/data from a path (keepalive).
    pub fn touch_path(&mut self, path_id: PathId) {
        if let Some(path) = self.paths.get_mut(&path_id) {
            path.last_report = self.clock.now();
            if !path.active {
                tracing::info!(path_id, "path recovered — marking active");
                path.active = true;
                // Reset to startup on recovery (Copa reset keeps the hint's
                // queue target; pacing restarts at the initial burst; the
                // dead path's in-flight budget is gone with it).
                path.cwnd = PathState::INITIAL_CWND;
                path.ssthresh = 64;
                path.in_slow_start = true;
                path.copa.reset();
                path.pace_tokens = PathState::INITIAL_CWND as f64;
                path.last_pace_refill = path.last_report;
                path.in_flight = 0;
                path.in_flight_log.clear();
            }
        }
    }

    /// Check all paths for staleness and deactivate dead ones.
    /// Returns list of path IDs that were deactivated.
    pub fn check_dead_paths(&mut self, timeout: Duration) -> Vec<PathId> {
        let now = self.clock.now();
        let mut deactivated = vec![];
        for path in self.paths.values_mut() {
            if path.active && now.duration_since(path.last_report) > timeout {
                tracing::warn!(path_id = path.id, "path timed out — marking inactive");
                path.active = false;
                deactivated.push(path.id);
            }
        }
        deactivated
    }

    /// Get all path IDs (including inactive).
    pub fn all_path_ids(&self) -> Vec<PathId> {
        self.paths.keys().copied().collect()
    }

    /// Pick the best path for a source symbol: lowest interpolated cost.
    ///
    /// cost_i = w_lat × E_i + w_bw × r_i (paper Section 13.8)
    pub fn best_source_path(&self) -> Option<PathId> {
        self.paths
            .values()
            .filter(|p| p.active && p.available() > 0)
            .min_by(|a, b| {
                let cost_a = self.path_cost(a);
                let cost_b = self.path_cost(b);
                cost_a.partial_cmp(&cost_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| p.id)
    }

    /// Compute the interpolated scheduling cost for a path.
    fn path_cost(&self, path: &PathState) -> f64 {
        let e_i = path.effective_delivery_time();
        let r_i = path.correction_rate();
        let r_clamped = if r_i.is_infinite() { 10.0 } else { r_i };
        self.weights.w_lat * e_i + self.weights.w_bw * r_clamped
    }

    /// Pick the best path for a repair symbol: highest goodput with available capacity.
    pub fn best_repair_path(&self) -> Option<PathId> {
        self.paths
            .values()
            .filter(|p| p.active && p.available() > 0)
            .max_by(|a, b| {
                a.effective_goodput()
                    .partial_cmp(&b.effective_goodput())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| p.id)
    }

    /// Pick the best repair path, preferring to avoid `avoid` for cross-path diversity.
    /// Falls back to `best_repair_path()` if no alternative exists.
    pub fn best_repair_path_avoiding(&self, avoid: PathId) -> Option<PathId> {
        let alt = self
            .paths
            .values()
            .filter(|p| p.active && p.available() > 0 && p.id != avoid)
            .max_by(|a, b| {
                a.effective_goodput()
                    .partial_cmp(&b.effective_goodput())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| p.id);
        alt.or_else(|| self.best_repair_path())
    }

    /// RWM per-symbol placement law (paper Section 16.3) — the ONE continuous
    /// marginal-cost rule that stripes source AND repair symbols across paths
    /// with no load regimes and no case splits. Replaces the single-path
    /// `best_source_path` / `best_repair_path` pair for the reliable window
    /// pipeline.
    ///
    /// For each active path `i`:
    ///
    ///   cost_i = Ê_i(load) / ref_srtt            ← frontier-completion-time
    ///          + w_bw · r_i                       ← correction/bandwidth burden
    ///          + w_div · ρ_fate(s, i)             ← repair diversity
    ///   P(i) ∝ exp(−cost_i / T)
    ///
    /// The paper (§16.3) writes `w_lat·E_i(load) + w_bw·r_i + w_div·ρ_fate`. Two
    /// implementation choices make it work for a reliable in-order stream:
    ///
    /// (1) `E_i(load)` is the expected frontier-completion-TIME
    ///     (`expected_delivery_load`): queue drain at the path's PACING RATE
    ///     `cwnd/SRTT`, plus propagation, plus loss recovery. Being in time, it
    ///     is capacity-aware — a backlog on the slow path costs more real time —
    ///     so the law water-fills by CAPACITY. A dimensionless `in_flight/cwnd`
    ///     fill instead fills both paths to equal FRACTION, over-loading the
    ///     low-capacity path; on an in-order stream that collapses the frontier
    ///     (MEASURED C8: 3.4 Mbit/s vs 15.4 fast-path-alone).
    ///
    /// (2) `E_i(load)` carries UNIT weight, not `w_lat`. The paper's `w_lat ≈ 0`
    ///     for Bulk is a lossy-throughput heuristic; on a RELIABLE in-order
    ///     stream latency-to-frontier is the completion cost itself, so it is
    ///     always weighted. `w_bw` still adds the wire-waste (loss) penalty that
    ///     is the Bulk-vs-Realtime dial. This also satisfies §16.3's requirement
    ///     that the queue signal drive water-filling ("token availability IS the
    ///     marginal-cost signal") — the queue term is never gated away.
    ///
    /// Terms:
    ///   - `Ê_i(load)/ref_srtt`: de-dimensionalised by the fastest path's SRTT,
    ///     O(1) and comparable across heterogeneous RTTs; rises continuously
    ///     with `in_flight`, equalised across paths at the water-filling point.
    ///   - `r_i`: correction rate / loss burden (clamped for dead paths).
    ///   - `ρ_fate(s,i)`: REPAIR symbols only — the fraction of the symbols this
    ///     repair covers that path `i` already carried (a repair riding its own
    ///     coverage adds no diversity). `covered_paths` holds one entry per
    ///     covered source symbol (with multiplicity); the continuous form of
    ///     `best_repair_path_avoiding`. Zero for source symbols.
    ///
    /// Temperature `T = PLACE_TEMPERATURE` is the one dial from strict best-path
    /// (T → 0 ⇒ argmin) to dithering. Single path ⇒ that path always (byte-
    /// identical to the pre-RWM single-path sender).
    ///
    /// Returns the sampled `PathId`, or `None` if no path is up at all.
    pub fn place_symbol(&self, is_repair: bool, covered_paths: &[PathId]) -> Option<PathId> {
        let probs = self.place_probs(is_repair, covered_paths);
        if probs.is_empty() {
            return None;
        }
        let u: f64 = rand::random();
        let mut acc = 0.0;
        for (pid, p) in &probs {
            acc += p;
            if u <= acc {
                return Some(*pid);
            }
        }
        // Floating-point slack: fall through to the last candidate.
        probs.last().map(|(pid, _)| *pid)
    }

    /// Cross-path repair placement (§16.3, the C8 "repair rides the spare path"
    /// realization; env `RWM_XPATH_REPAIR`).
    ///
    /// The marginal-cost `place_symbol(true, ..)` softmax biases repair toward
    /// the FAST path (lowest frontier-completion-time), so proactive repair
    /// competes with systematic source on the same link — the single-path
    /// presence⊥throughput tension (goal-gate "Present-at-Stall"): buying early
    /// presence costs source bandwidth. This instead routes repair to the path
    /// with the MOST spare capacity relative to its load (`max spare_capacity`),
    /// i.e. the UNDERUTILIZED path — the slow path once the fast path is
    /// source-saturated. A fast-path loss is then covered by repair already in
    /// flight on the slow path, WITHOUT displacing fast-path source.
    ///
    /// Symmetric paths (C7) have equal spare, so the near-tie set is picked
    /// UNIFORMLY at random — no hard-argmax concentration (which measured a C7
    /// regression). Only a genuine spare-capacity asymmetry (heterogeneous C8,
    /// fast saturated / slow idle) steers repair to one path. Falls back to the
    /// softmax placement when fewer than two paths are up.
    pub fn place_repair_spare_path(&self) -> Option<PathId> {
        let spares: Vec<(PathId, f64)> = self
            .paths
            .values()
            .filter(|p| p.active)
            .map(|p| (p.id, p.spare_capacity()))
            .collect();
        if spares.len() < 2 {
            return self.place_symbol(true, &[]);
        }
        let max_spare = spares.iter().map(|(_, s)| *s).fold(f64::NEG_INFINITY, f64::max);
        // Near-tie set: within 80% of the max spare (or, for the unbounded
        // in_flight==0 case, all INF paths). Absolute floor 0.25 keeps two
        // lightly-loaded paths in the tie set so they split rather than concentrate.
        let thresh = if max_spare.is_finite() {
            (0.8 * max_spare).min(max_spare - 0.25)
        } else {
            f64::INFINITY // only INF-spare paths qualify
        };
        let candidates: Vec<PathId> = spares
            .iter()
            .filter(|(_, s)| if max_spare.is_finite() { *s >= thresh } else { s.is_infinite() })
            .map(|(pid, _)| *pid)
            .collect();
        if candidates.is_empty() {
            return self.place_symbol(true, &[]);
        }
        let idx = (rand::random::<f64>() * candidates.len() as f64) as usize;
        Some(candidates[idx.min(candidates.len() - 1)])
    }

    /// The softmax placement distribution over paths (paper §16.3). Exposed for
    /// unit-testing the placement law (concentration, continuous spillover,
    /// water-filling, fate steering, T → 0 argmin) without sampling noise.
    /// Returns `(PathId, probability)` summing to 1 over the candidate set.
    pub fn place_probs(&self, is_repair: bool, covered_paths: &[PathId]) -> Vec<(PathId, f64)> {
        self.place_probs_with_temperature(is_repair, covered_paths, place_temperature())
    }

    /// `place_probs` with an explicit temperature — the T dial exposed for
    /// tests (T → 0 ⇒ argmin, the no-cutoffs strict-best-path limit).
    pub fn place_probs_with_temperature(
        &self,
        is_repair: bool,
        covered_paths: &[PathId],
        temperature: f64,
    ) -> Vec<(PathId, f64)> {
        let costs = self.place_costs(is_repair, covered_paths);
        if costs.is_empty() {
            return vec![];
        }
        // The costs from `place_costs` are already dimensionless (the latency
        // term is normalised by the fastest SRTT), so the temperature is a pure
        // dimensionless dial. Shift by the min cost for numerical stability
        // (softmax is shift-invariant).
        let t_eff = temperature.max(f64::MIN_POSITIVE);
        let min_cost = costs
            .iter()
            .map(|(_, c)| *c)
            .fold(f64::INFINITY, f64::min);
        let mut weights: Vec<(PathId, f64)> = costs
            .iter()
            .map(|(pid, c)| (*pid, (-(c - min_cost) / t_eff).exp()))
            .collect();
        let z: f64 = weights.iter().map(|(_, w)| w).sum();
        if z <= 0.0 || !z.is_finite() {
            // Degenerate (T → 0 with ties, or overflow): argmin gets all mass.
            let arg = costs
                .iter()
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(pid, _)| *pid);
            return costs
                .iter()
                .map(|(pid, _)| (*pid, if Some(*pid) == arg { 1.0 } else { 0.0 }))
                .collect();
        }
        for (_, w) in &mut weights {
            *w /= z;
        }
        weights
    }

    /// Per-path marginal placement cost (paper §16.3), over ALL active paths.
    ///
    /// We deliberately do NOT hard-filter on spare capacity. The paper phrases
    /// a full path as "skipped (∞ cost)", but its own no-cutoffs convention
    /// binds mechanisms ("no control law may case-split"), and a hard filter
    /// would make a path vanish discontinuously at `in_flight == cwnd` — the
    /// exact threshold jump the monotonic-spillover requirement forbids. The
    /// `in_flight/cwnd` congestion term IS the continuous form: it climbs past
    /// 1.0 under overdraft, driving a saturated path's softmax mass toward zero
    /// smoothly without ever removing it, so placement never drops a symbol
    /// (the send loop's pacing/backpressure remains the real capacity gate).
    fn place_costs(&self, is_repair: bool, covered_paths: &[PathId]) -> Vec<(PathId, f64)> {
        // ── THE COLD PRICE (`RWM_COLD_PLACE`, anchor-hygiene rule 1) ───────
        // What one second of a leg that has NEVER been measured is worth.
        // Under the gate: the active set's fastest MEASURED srtt — another
        // leg's measurement, not a constant. Off (or with nothing measured
        // yet, i.e. `INFINITY`): `None`, and `srtt_of` below falls back to
        // `p.srtt()`, the shipped expression verbatim.
        //
        // This is the whole fix. Everything below is the shipped law, with
        // SRTT_i read through `srtt_of` instead of `p.srtt()` — one
        // substitution, applied identically to the reference, the deadline
        // and the load term, so the objective (§13.8) keeps its shape and
        // only its COLD-regime inputs change. Once every leg has a sample
        // `srtt_of == p.srtt()` at every leg and the fix is inert.
        let cold_srtt: Option<f64> = if self.cold_place {
            let m = self
                .paths
                .values()
                .filter(|p| p.active)
                .filter_map(|p| p.srtt_measured())
                .map(|d| d.as_secs_f64())
                .fold(f64::INFINITY, f64::min);
            m.is_finite().then_some(m)
        } else {
            None
        };
        // ONE expression, no `if cold`: the leg's own measurement when it has
        // one, the cold price when it does not, and `p.srtt()` when there is
        // no cold price — which is `p.srtt()` unconditionally with the gate
        // off, since `srtt_measured() == Some(d)` implies `srtt() == d`.
        let srtt_of = |p: &PathState| -> f64 {
            p.srtt_measured()
                .map(|d| d.as_secs_f64())
                .or(cold_srtt)
                .unwrap_or_else(|| p.srtt().as_secs_f64())
        };

        let ref_srtt = self
            .paths
            .values()
            .filter(|p| p.active)
            .map(|p| srtt_of(p).max(PLACE_REF_FLOOR_SECS))
            .fold(f64::INFINITY, f64::min);
        let ref_srtt = if ref_srtt.is_finite() {
            ref_srtt
        } else {
            PLACE_REF_FLOOR_SECS
        };

        let w_bw = self.weights.w_bw;
        let w_div = self.weights.w_div;

        let covered_total = covered_paths.len() as f64;

        let cost_of = |p: &PathState| -> f64 {
            // Frontier-completion-time — the always-on load term (unit weight),
            // de-dimensionalised by the fastest SRTT so it is O(1). This single
            // term carries BOTH the congestion signal (queue drain at the pacing
            // rate) and the propagation preference; because it is expressed in
            // TIME it is capacity-aware, so it water-fills by capacity rather
            // than over-loading the slow path.
            //
            // Frontier-slack generalization (goal-gate "C8 Slow-Path
            // Conversion", `RWM_PLACE_SLACK`): only the LATENESS beyond the
            // per-path deadline D_i = min(S, 9/8·srtt_i) is charged —
            // max(0, Ê_i − D_i). S = the frontier slack (need-time budget);
            // the 9/8·srtt_i term is the RECOVERY plane's patience for a
            // flight on this path (RFC 9002 kTimeThreshold — the SAME
            // constant `mp_time_threshold_split` uses): a placement later than
            // that gets re-served cross-path regardless of frontier need,
            // so budgeting past it makes the planes fight (MEASURED, the
            // 2026-08-06 smoke falsification of the unbounded-S form: c8
            // 66.2 Mbit with retxo_p1 = 49%). S = 0 (gate off / N = 1 /
            // cold ack-rate) is bit-exactly the shipped term. With S > 0 a
            // path is free until its backlog's completion time reaches its
            // deadline (deadline-aware water-filling: equilibrium
            // backlog_i ≈ rate_i·(D_i − owd_i)), which ends the
            // Bulk-softmax starvation of the slow path (its idle srtt_i/2
            // propagation term alone was worth e^10:1 odds at T = 0.15)
            // while bounding each placement's lateness continuously — no
            // threshold, no mode, no per-topology branch.
            let srtt_i = srtt_of(p);
            let deadline = self
                .place_slack_secs
                .min(PLACE_SLACK_RECOV_PATIENCE * srtt_i);
            let load = (p.expected_delivery_load_at(srtt_i) - deadline).max(0.0) / ref_srtt;
            // Bandwidth/correction burden (loss/wire waste); the hint's w_bw
            // dial. w_lat does NOT gate placement: on a reliable in-order stream
            // latency-to-frontier is the completion cost itself, already carried
            // by `load` at unit weight, not a per-hint preference.
            let r = p.correction_rate();
            let r = if r.is_infinite() { 10.0 } else { r };
            // Fate diversity (repairs only): fraction of covered symbols on p.
            let fate = if is_repair && covered_total > 0.0 {
                covered_paths.iter().filter(|&&c| c == p.id).count() as f64 / covered_total
            } else {
                0.0
            };
            load + w_bw * r + w_div * fate
        };

        self.paths
            .values()
            .filter(|p| p.active)
            .map(|p| (p.id, cost_of(p)))
            .collect()
    }

    /// Pick a secondary path for redundant source scheduling (different from primary).
    /// Returns None if only one usable path is available.
    pub fn redundant_source_path(&self, primary: PathId) -> Option<PathId> {
        self.paths
            .values()
            .filter(|p| p.active && p.available() > 0 && p.id != primary)
            .min_by(|a, b| {
                let cost_a = self.path_cost(a);
                let cost_b = self.path_cost(b);
                cost_a.partial_cmp(&cost_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| p.id)
    }

    /// Aggregate spare capacity across all active paths.
    ///
    /// Returns the minimum spare_capacity fraction across active paths,
    /// representing the tightest bottleneck. Used to cap FEC repair rate.
    pub fn spare_capacity(&self) -> f64 {
        self.paths
            .values()
            .filter(|p| p.active)
            .map(|p| p.spare_capacity())
            .fold(f64::INFINITY, f64::min)
    }

    /// Get the minimum max_datagram_size across all active paths that have
    /// reported an MTU. Returns None if no active path has a known MTU.
    pub fn min_mtu(&self) -> Option<usize> {
        self.paths
            .values()
            .filter(|p| p.active)
            .filter_map(|p| p.max_datagram_size)
            .min()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new(Arc::new(WallClock))
    }
}

impl Scheduler {
    /// Set protocol hint (updates scheduling weights and each path's
    /// Copa-lite queue target).
    pub fn set_protocol_hint(&mut self, hint: ProtocolHint) {
        self.weights = SchedulingWeights::from_hint(hint);
        self.hint = hint;
        for path in self.paths.values_mut() {
            path.set_hint(hint);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_symbol(id: u32, repair: bool) -> WireSymbol {
        WireSymbol {
            block_id: 0,
            payload_id: id,
            is_repair: repair,
            data: vec![0u8; 64],
            backend: FecBackend::RaptorQ,
        }
    }

    /// `active_paths()` and `live_paths()` are NOT interchangeable: the
    /// active set additionally filters on spare capacity, so a SATURATED
    /// path (in_flight ≥ cwnd) is live but not active. Every sender phase
    /// that aggregates over paths picks one of the two deliberately, and
    /// swapping them silently changes the law — the CC pace rate uses
    /// `live_paths()` precisely because the active filter dropped a
    /// saturated path out of the aggregate (`net/mod.rs`, cc-rate refresh),
    /// while the M* RTprop / in-flight-cap / tail-sweep phases use
    /// `active_paths()`.
    ///
    /// This test exists because the deleted `RWM_SCHED_SNAPSHOT` seam
    /// (2026-08-10, ADR-0066) carried a unit test that CLAIMED to catch a
    /// path-set swap and could not: its fixture only ever added fresh paths,
    /// where `in_flight = 0 < cwnd` makes the two sets IDENTICAL, so
    /// substituting either for the other passed. Assert the divergence
    /// itself, at the only state that exhibits it.
    #[test]
    fn saturated_path_is_live_but_not_active() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);
        sched.add_path(1);

        // Fresh paths: the trap. Both sets agree, so nothing here can
        // distinguish them — this is exactly the fixture that gave a false
        // pass, asserted so the trap cannot be re-entered unnoticed.
        assert_eq!(
            sched.active_paths(),
            sched.live_paths(),
            "fresh paths make active/live indistinguishable — a fixture of \
             only-fresh paths CANNOT test the distinction"
        );

        // Saturate path 1: up, but no spare capacity.
        {
            let p = sched.path_mut(1).unwrap();
            assert!(p.active, "path must be up for the distinction to bite");
            p.in_flight = p.cwnd;
            assert_eq!(p.available(), 0);
        }

        // Both accessors iterate a HashMap, so the RETURNED ORDER is
        // arbitrary and re-seeded per process — sort before comparing, or
        // this asserts the hasher instead of the path sets (it did: the
        // first version of this test was a coin flip on two paths).
        assert_eq!(sched.active_paths(), vec![0], "saturated path 1 is NOT active");
        let mut live = sched.live_paths();
        live.sort_unstable();
        assert_eq!(
            live,
            vec![0, 1],
            "saturated path 1 IS live — control traffic and the CC rate \
             aggregate must still see it"
        );

        // And the other direction: a DOWN path with spare capacity is in
        // neither set, so `live_paths()` is not merely "all paths".
        sched.path_mut(0).unwrap().active = false;
        assert!(sched.active_paths().is_empty());
        assert_eq!(sched.live_paths(), vec![1]);
    }

    #[test]
    fn test_best_source_path_picks_lowest_rtt() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);
        sched.add_path(1);

        sched
            .path_mut(0)
            .unwrap()
            .estimator
            .record_rtt(std::time::Duration::from_millis(100));
        sched
            .path_mut(1)
            .unwrap()
            .estimator
            .record_rtt(std::time::Duration::from_millis(10));

        assert_eq!(sched.best_source_path(), Some(1));
    }

    #[test]
    fn test_best_repair_path_picks_highest_goodput() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);
        sched.add_path(1);

        // Path 0: low throughput
        sched.path_mut(0).unwrap().estimator.record_batch(10, 9);
        sched.path_mut(0).unwrap().estimator.record_throughput(100.0);

        // Path 1: high throughput
        sched.path_mut(1).unwrap().estimator.record_batch(10, 9);
        sched.path_mut(1).unwrap().estimator.record_throughput(1000.0);

        assert_eq!(sched.best_repair_path(), Some(1));
    }

    #[test]
    fn test_redundant_source_path_picks_different_path() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);
        sched.add_path(1);
        sched.add_path(2);

        sched
            .path_mut(0)
            .unwrap()
            .estimator
            .record_rtt(std::time::Duration::from_millis(5));
        sched
            .path_mut(1)
            .unwrap()
            .estimator
            .record_rtt(std::time::Duration::from_millis(20));
        sched
            .path_mut(2)
            .unwrap()
            .estimator
            .record_rtt(std::time::Duration::from_millis(50));

        // Primary is 0, redundant should be 1 (second-lowest RTT)
        let redundant = sched.redundant_source_path(0);
        assert_eq!(redundant, Some(1));
    }

    #[test]
    fn test_redundant_source_path_none_with_single_path() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);

        assert_eq!(sched.redundant_source_path(0), None);
    }

    #[test]
    fn test_best_source_path_skips_full_cwnd() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);
        sched.add_path(1);

        sched
            .path_mut(0)
            .unwrap()
            .estimator
            .record_rtt(std::time::Duration::from_millis(5));
        sched
            .path_mut(1)
            .unwrap()
            .estimator
            .record_rtt(std::time::Duration::from_millis(50));

        // Fill path 0's cwnd
        let cwnd = sched.path(0).unwrap().cwnd;
        sched.path_mut(0).unwrap().in_flight = cwnd;

        // Should pick path 1 since path 0 has no capacity
        assert_eq!(sched.best_source_path(), Some(1));
    }

    #[test]
    fn test_schedule_prefers_low_rtt_for_source() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);
        sched.add_path(1);

        // Path 0: high RTT
        sched
            .path_mut(0)
            .unwrap()
            .estimator
            .record_rtt(std::time::Duration::from_millis(100));
        // Path 1: low RTT
        sched
            .path_mut(1)
            .unwrap()
            .estimator
            .record_rtt(std::time::Duration::from_millis(10));

        let source: Vec<_> = (0..5).map(|i| make_symbol(i, false)).collect();
        let result = sched.schedule(source, vec![]);

        // Path 1 (lower RTT) should get symbols first
        let path1_count = result
            .iter()
            .find(|(id, _)| *id == 1)
            .map(|(_, s)| s.len())
            .unwrap_or(0);

        assert!(path1_count > 0, "Low-RTT path should receive source symbols");
    }

    #[test]
    fn test_best_repair_path_avoiding_picks_alternative() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);
        sched.add_path(1);

        // Path 0: highest goodput
        sched.path_mut(0).unwrap().estimator.record_batch(10, 9);
        sched.path_mut(0).unwrap().estimator.record_throughput(1000.0);

        // Path 1: lower goodput
        sched.path_mut(1).unwrap().estimator.record_batch(10, 9);
        sched.path_mut(1).unwrap().estimator.record_throughput(500.0);

        // Avoiding path 0 should pick path 1
        assert_eq!(sched.best_repair_path_avoiding(0), Some(1));
        // Avoiding path 1 should pick path 0
        assert_eq!(sched.best_repair_path_avoiding(1), Some(0));
    }

    #[test]
    fn test_best_repair_path_avoiding_falls_back_single_path() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);

        // With only one path, avoiding it should still return it
        assert_eq!(sched.best_repair_path_avoiding(0), Some(0));
    }

    // -----------------------------------------------------------------------
    // Correction deficit tests (paper Section 13.4)
    // -----------------------------------------------------------------------

    #[test]
    fn test_deficit_tracks_sends_and_acks() {
        let mut deficit = CorrectionDeficit::new();
        assert_eq!(deficit.deficit(), 0.0);

        deficit.on_send(0, 1, 0.10);
        deficit.on_send(1, 1, 0.10);
        deficit.on_send(2, 2, 0.05);
        assert!((deficit.deficit() - 0.25).abs() < 1e-10);
        assert_eq!(deficit.pending_count(), 3);

        // ACK symbol 1
        assert!(deficit.on_ack(1));
        assert!((deficit.deficit() - 0.15).abs() < 1e-10);
        assert_eq!(deficit.pending_count(), 2);

        // ACK unknown symbol → no change
        assert!(!deficit.on_ack(99));
        assert!((deficit.deficit() - 0.15).abs() < 1e-10);
    }

    #[test]
    fn test_deficit_cumulative_ack() {
        let mut deficit = CorrectionDeficit::new();
        for seq in 0..10 {
            deficit.on_send(seq, 1, 0.10);
        }
        assert!((deficit.deficit() - 1.0).abs() < 1e-10);

        deficit.on_ack_cumulative(4); // ACK 0..=4
        assert_eq!(deficit.pending_count(), 5);
        assert!((deficit.deficit() - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_deficit_per_path() {
        let mut deficit = CorrectionDeficit::new();
        deficit.on_send(0, 1, 0.10);
        deficit.on_send(1, 2, 0.05);
        deficit.on_send(2, 1, 0.10);

        assert!((deficit.path_deficit(1) - 0.20).abs() < 1e-10);
        assert!((deficit.path_deficit(2) - 0.05).abs() < 1e-10);
        assert!((deficit.path_deficit(3) - 0.00).abs() < 1e-10);
    }

    // -----------------------------------------------------------------------
    // Effective delivery time tests (paper Section 13.5)
    // -----------------------------------------------------------------------

    #[test]
    fn test_effective_delivery_time() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);

        let path = sched.path_mut(0).unwrap();
        // Record multiple RTT samples so EWMA converges
        for _ in 0..20 {
            path.estimator.record_rtt(Duration::from_millis(100));
        }
        // Record some loss: 10 sent, 9 received → ~10% loss
        for _ in 0..20 {
            path.estimator.record_batch(10, 9);
        }

        let e = path.effective_delivery_time();
        let rtt = path.estimator.rtt().as_secs_f64();
        let eps = path.estimator.loss_rate();
        let expected = rtt / 2.0 + eps * rtt;
        assert!((e - expected).abs() < 0.001, "E_i={e}, expected={expected}, rtt={rtt}, eps={eps}");
    }

    #[test]
    fn test_correction_rate() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);

        let path = sched.path_mut(0).unwrap();
        // Record loss to get ~10% loss rate
        for _ in 0..20 {
            path.estimator.record_batch(10, 9);
        }
        let eps = path.estimator.loss_rate();
        let r = path.correction_rate();
        let expected = eps / (1.0 - eps);
        assert!((r - expected).abs() < 0.001, "r={r}, expected={expected}");
    }

    // -----------------------------------------------------------------------
    // Interpolated objective tests (paper Section 13.8)
    // -----------------------------------------------------------------------

    #[test]
    fn test_realtime_prefers_low_latency_over_low_loss() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Realtime);
        sched.add_path(0);
        sched.add_path(1);

        // Path 0: low RTT (10ms), high loss (20%)
        sched.path_mut(0).unwrap().estimator.record_rtt(Duration::from_millis(10));
        for _ in 0..20 {
            sched.path_mut(0).unwrap().estimator.record_batch(10, 8);
        }

        // Path 1: high RTT (200ms), low loss (1%)
        sched.path_mut(1).unwrap().estimator.record_rtt(Duration::from_millis(200));
        for _ in 0..20 {
            sched.path_mut(1).unwrap().estimator.record_batch(100, 99);
        }

        // Realtime (w_lat=1, w_bw=0): should prefer path 0 (lower E_i despite higher loss)
        assert_eq!(sched.best_source_path(), Some(0));
    }

    #[test]
    fn test_bulk_prefers_low_overhead() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
        sched.add_path(0);
        sched.add_path(1);

        // Path 0: low RTT (10ms), high loss (20%) → high r
        sched.path_mut(0).unwrap().estimator.record_rtt(Duration::from_millis(10));
        for _ in 0..20 {
            sched.path_mut(0).unwrap().estimator.record_batch(10, 8);
        }

        // Path 1: high RTT (200ms), low loss (1%) → low r
        sched.path_mut(1).unwrap().estimator.record_rtt(Duration::from_millis(200));
        for _ in 0..20 {
            sched.path_mut(1).unwrap().estimator.record_batch(100, 99);
        }

        // Bulk (w_lat=0, w_bw=1): should prefer path 1 (lower correction rate)
        assert_eq!(sched.best_source_path(), Some(1));
    }

    #[test]
    fn test_schedule_uses_objective_weights() {
        // With Realtime hint, source should go to low-latency path even if it has more loss
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Realtime);
        sched.add_path(0);
        sched.add_path(1);

        // Path 0: fast, lossy
        sched.path_mut(0).unwrap().estimator.record_rtt(Duration::from_millis(10));
        for _ in 0..20 {
            sched.path_mut(0).unwrap().estimator.record_batch(10, 8);
        }

        // Path 1: slow, clean
        sched.path_mut(1).unwrap().estimator.record_rtt(Duration::from_millis(200));
        for _ in 0..20 {
            sched.path_mut(1).unwrap().estimator.record_batch(100, 99);
        }

        let source: Vec<_> = (0..5).map(|i| make_symbol(i, false)).collect();
        let result = sched.schedule(source, vec![]);

        let path0_count = result
            .iter()
            .find(|(id, _)| *id == 0)
            .map(|(_, s)| s.len())
            .unwrap_or(0);

        assert!(path0_count > 0, "Realtime should send source on fast path");
    }

    // -----------------------------------------------------------------------
    // P7: Copa-lite production port (paper Sections 12.4-12.5, gate P1+P2)
    // -----------------------------------------------------------------------

    fn millis(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn test_copa_lite_cwnd_never_below_floor() {
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock.clone());
        sched.add_path(0);

        // Establish a 10ms propagation floor.
        for _ in 0..3 {
            sched.path_mut(0).unwrap().record_rtt_sample(millis(10));
        }

        // Hammer with inflated-RTT windows (delay backoffs) ...
        for _ in 0..50 {
            sched.path_mut(0).unwrap().record_rtt_sample(millis(100));
            clock.advance(millis(150));
            sched.ack(0, 4);
            assert!(
                sched.path(0).unwrap().cwnd >= PathState::MIN_CWND,
                "delay backoffs must never take cwnd below the floor"
            );
        }
        // ... and with decode failures (loss steps).
        for _ in 0..100 {
            sched.on_loss(0, false);
        }
        let cwnd = sched.path(0).unwrap().cwnd;
        assert_eq!(cwnd, PathState::MIN_CWND);
        assert!(cwnd >= 8, "floor is 8 symbols, never the historical 2");
    }

    #[test]
    fn test_burst_rtt_spike_does_not_collapse_cwnd() {
        // The pre-P7 failure mode: the initial burst inflates its own RTT
        // samples, dq explodes, and the rate-formula target collapses cwnd
        // to the floor. With the windowed-min filter remembering the
        // propagation floor, a burst costs one gentle ×0.92 backoff.
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock.clone());
        sched.add_path(0);

        // Learn the 10ms floor and ramp for a few clean RTTs.
        for _ in 0..6 {
            sched.path_mut(0).unwrap().record_rtt_sample(millis(10));
            clock.advance(millis(15));
            sched.ack(0, 8);
        }
        let pre_burst = sched.path(0).unwrap().cwnd;
        assert!(
            pre_burst > PathState::INITIAL_CWND,
            "ramp should have grown cwnd, got {pre_burst}"
        );

        // A burst inflates a full update window of RTT samples 4x.
        for _ in 0..4 {
            sched.path_mut(0).unwrap().record_rtt_sample(millis(40));
        }
        clock.advance(millis(50));
        sched.ack(0, 8);

        let post_burst = sched.path(0).unwrap().cwnd;
        let one_backoff = (pre_burst as f64 * BACKOFF_MULT) as u32;
        assert!(
            post_burst + 1 >= one_backoff,
            "burst must cost at most one gentle backoff: pre={pre_burst}, post={post_burst}"
        );
        assert!(
            post_burst > 2 * PathState::MIN_CWND,
            "burst must not collapse cwnd toward the floor: post={post_burst}"
        );

        // After the burst drains, samples return to the floor and cwnd
        // recovers additively (+2 per update).
        sched.path_mut(0).unwrap().record_rtt_sample(millis(10));
        clock.advance(millis(50));
        sched.ack(0, 8);
        let recovered = sched.path(0).unwrap().cwnd;
        assert_eq!(
            recovered,
            post_burst + ADDITIVE_STEP as u32,
            "post-backoff growth is additive"
        );
    }

    #[test]
    fn test_ramp_multiplicative_until_backoff_then_additive() {
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock.clone());
        sched.add_path(0);

        // Clean RTTs at the floor: each per-SRTT update multiplies ×1.5+1.
        let mut prev = sched.path(0).unwrap().cwnd;
        for _ in 0..4 {
            sched.path_mut(0).unwrap().record_rtt_sample(millis(20));
            clock.advance(millis(30));
            sched.ack(0, prev);
            let cur = sched.path(0).unwrap().cwnd;
            assert_eq!(
                cur,
                (prev as f64 * RAMP_GAIN + 1.0).round() as u32,
                "ramp phase is multiplicative"
            );
            assert!(sched.path(0).unwrap().in_slow_start);
            prev = cur;
        }

        // First backoff: inflated window ends the ramp.
        sched.path_mut(0).unwrap().record_rtt_sample(millis(80));
        clock.advance(millis(50));
        sched.ack(0, prev);
        let after_backoff = sched.path(0).unwrap().cwnd;
        assert_eq!(after_backoff, (prev as f64 * BACKOFF_MULT).round() as u32);
        assert!(!sched.path(0).unwrap().in_slow_start);

        // Subsequent clean updates are additive +2 — never multiplicative.
        let mut prev = after_backoff;
        for _ in 0..3 {
            sched.path_mut(0).unwrap().record_rtt_sample(millis(20));
            clock.advance(millis(50));
            sched.ack(0, prev);
            let cur = sched.path(0).unwrap().cwnd;
            assert_eq!(cur, prev + ADDITIVE_STEP as u32, "steady state is additive");
            prev = cur;
        }
    }

    #[test]
    fn test_hint_changes_backoff_threshold() {
        // P1 (paper 12.4): the protocol hint sets the queue target.
        // floor = 100ms, windowed min = 118ms → dq = 18ms. The 100→118
        // step also charges the jitter estimator (18/8 = 2.25ms decaying
        // to ~1.72ms over the three samples → ~3.4ms threshold widening):
        //   Realtime target  8ms + 3.4ms → backoff
        //   Auto target   12.5ms + 3.4ms → backoff
        //   Bulk target     25ms + 3.4ms → keep growing
        fn run(hint: ProtocolHint) -> (u32, u32) {
            let clock = Arc::new(MockClock::new());
            let mut sched = Scheduler::new_with_hint(clock.clone(), hint);
            sched.add_path(0);
            for _ in 0..3 {
                sched.path_mut(0).unwrap().record_rtt_sample(millis(100));
                clock.advance(millis(150));
                sched.ack(0, 8);
            }
            let pre = sched.path(0).unwrap().cwnd;
            for _ in 0..3 {
                sched.path_mut(0).unwrap().record_rtt_sample(millis(118));
            }
            clock.advance(millis(150));
            sched.ack(0, 8);
            (pre, sched.path(0).unwrap().cwnd)
        }

        let (rt_pre, rt_post) = run(ProtocolHint::Realtime);
        let (auto_pre, auto_post) = run(ProtocolHint::Auto);
        let (bulk_pre, bulk_post) = run(ProtocolHint::Bulk);

        assert!(rt_post < rt_pre, "Realtime backs off at dq=18ms: {rt_pre}->{rt_post}");
        assert!(auto_post < auto_pre, "Auto backs off at dq=18ms: {auto_pre}->{auto_post}");
        assert!(bulk_post > bulk_pre, "Bulk tolerates dq=18ms: {bulk_pre}->{bulk_post}");
    }

    #[test]
    fn test_jitter_widens_backoff_threshold_c2() {
        // Jitter-adjusted queue target (paper 12.4). C2-like link: 10ms
        // floor with ±6ms RTT jitter (netem 3ms/direction). Bulk's raw P1
        // threshold is 2.5ms — smaller than the jitter — so the pre-fix
        // windowed-min signal read jitter as a standing queue and backed
        // off nearly every update (measured at L1: cwnd pinned at the
        // floor for 60% of ACKs, 16x throughput gap vs quinn). With the
        // k×jitter_est widening, a jittery-but-queue-free link must ramp.
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new_with_hint(clock.clone(), ProtocolHint::Bulk);
        sched.add_path(0);

        // Deterministic jitter pattern with min 10ms, spread 6ms — every
        // update window's min sample sits 2-4ms above the 10s floor once
        // the floor has seen a 10ms sample.
        let pattern_ms = [10u64, 14, 12, 16, 13, 15, 12, 14];
        let mut cwnd_track = Vec::new();
        for round in 0..40 {
            // 4 ACK batches per SRTT window, one RTT sample each; skip
            // the true-floor sample in most windows (the windowed min
            // usually does NOT reach the floor — that is the trap).
            for k in 0..4 {
                let idx = (round * 4 + k) % pattern_ms.len();
                let ms = if round == 0 && k == 0 { 10 } else { pattern_ms[idx].max(12) };
                sched.path_mut(0).unwrap().record_rtt_sample(millis(ms));
            }
            clock.advance(millis(15));
            sched.ack(0, 8);
            cwnd_track.push(sched.path(0).unwrap().cwnd);
        }
        let final_cwnd = *cwnd_track.last().unwrap();
        assert!(
            final_cwnd > 100,
            "jittery queue-free C2 link must ramp past 100 symbols, got {final_cwnd} (track: {cwnd_track:?})"
        );

        // Sanity: a genuine standing queue on the SAME jittery link still
        // triggers backoff within a few updates — the queue shifts every
        // sample up by 12ms, while the consecutive-difference jitter
        // estimate stays at jitter scale.
        let before = sched.path(0).unwrap().cwnd;
        let mut backed_off = false;
        for round in 0..6 {
            for k in 0..4 {
                let idx = (round * 4 + k) % pattern_ms.len();
                sched
                    .path_mut(0)
                    .unwrap()
                    .record_rtt_sample(millis(pattern_ms[idx] + 12));
            }
            clock.advance(millis(25));
            sched.ack(0, 8);
            if sched.path(0).unwrap().cwnd < before {
                backed_off = true;
                break;
            }
        }
        assert!(backed_off, "a genuine 12ms standing queue must still back off");
    }

    #[test]
    fn test_hint_plumbed_to_paths() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
        sched.add_path(0);
        assert_eq!(sched.path(0).unwrap().copa.queue_mult, 1.25);

        sched.set_protocol_hint(ProtocolHint::Realtime);
        assert_eq!(sched.path(0).unwrap().copa.queue_mult, 1.08);

        // New paths pick up the current hint.
        sched.add_path(1);
        assert_eq!(sched.path(1).unwrap().copa.queue_mult, 1.08);
    }

    #[test]
    fn test_pacing_token_bucket_rate_and_burst() {
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock.clone());
        sched.add_path(0);

        let path = sched.path_mut(0).unwrap();
        // SRTT = 100ms exactly (EWMA of identical samples).
        for _ in 0..4 {
            path.record_rtt_sample(millis(100));
        }
        path.cwnd = 200; // rate = 200/0.1 = 2000 symbols/sec
        path.pace_tokens = 0.0;

        clock.advance(millis(5)); // 2000/s × 5ms = 10 tokens
        sched.path_mut(0).unwrap().pace_refill();
        let tokens = sched.path(0).unwrap().pace_tokens();
        assert!((tokens - 10.0).abs() < 1e-6, "refill rate is cwnd/SRTT, got {tokens}");

        clock.advance(millis(100)); // would add 200 → capped at burst
        sched.path_mut(0).unwrap().pace_refill();
        let tokens = sched.path(0).unwrap().pace_tokens();
        // burst allowance = max(10, cwnd/8) = max(10, 25) = 25
        assert!((tokens - 25.0).abs() < 1e-6, "burst cap is max(10, cwnd/8), got {tokens}");

        // Batch-granular overdraft: consumption may push the bucket negative.
        let path = sched.path_mut(0).unwrap();
        path.consume_pace_tokens(30);
        assert!(path.pace_tokens() < 0.0);
        assert!(path.pace_delay() > Duration::ZERO);

        // Small-cwnd burst floor: max(10, cwnd/8) = 10.
        let path = sched.path_mut(0).unwrap();
        path.cwnd = 16;
        path.pace_tokens = 0.0;
        clock.advance(millis(1000));
        path.pace_refill();
        assert!((path.pace_tokens() - 10.0).abs() < 1e-6);
    }

    #[test]
    fn test_copa_target_cwnd_units() {
        // Units doc-test: floor = SRTT = 100ms → dq clamps at 0.1ms.
        // rate = 1/(0.5 [1/sym] × 1e-4 [s]) = 20000 symbols/s
        // cwnd = 20000 [sym/s] × 0.1 [s] = 2000 symbols
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock);
        sched.add_path(0);
        let path = sched.path_mut(0).unwrap();
        path.record_rtt_sample(millis(100));
        assert_eq!(path.copa_target_cwnd(), 2000);
    }

    #[test]
    fn test_paced_ramp_reaches_block_scale_without_spurious_backoff() {
        // P7 follow-up regression: with SYMBOL-paced sends the standing
        // queue stays near zero, so at C2-like parameters (10ms floor, no
        // competing traffic) the ramp must sail past one 64KB block
        // (~56 symbols) within 15 SRTTs and never back off. The first L1
        // run of batch-granular pacing failed exactly this: every block
        // burst self-queued ~5.4ms > Bulk's 2.5ms threshold and cwnd
        // pinned at ~34, just under one block.
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock.clone()); // Auto: 1.125 target
        sched.add_path(0);

        for round in 0..15 {
            // Token-paced send phase across one RTT: consume only what
            // the bucket allows, in 1ms steps (never a whole-block burst).
            for _ in 0..12 {
                let p = sched.path_mut(0).unwrap();
                p.pace_refill();
                let budget = p.pace_tokens().max(0.0) as u32;
                if budget > 0 {
                    p.consume_pace_tokens(budget);
                }
                clock.advance(millis(1));
            }
            // Paced sends leave only sub-threshold jitter over the floor
            // (alternating 10.0/10.5ms; Auto's backoff needs > 11.25ms).
            let sample = if round % 2 == 0 { 10_000 } else { 10_500 };
            let p = sched.path_mut(0).unwrap();
            p.record_rtt_sample(Duration::from_micros(sample));

            let before = sched.path(0).unwrap().cwnd;
            sched.ack(0, before.min(64));
            let after = sched.path(0).unwrap().cwnd;
            assert!(
                after >= before,
                "paced sends must not trigger a backoff (round {round}): {before} -> {after}"
            );
        }
        let cwnd = sched.path(0).unwrap().cwnd;
        assert!(
            cwnd > 100,
            "ramp must clear one 64KB block (~56 symbols) at C2, got {cwnd}"
        );
    }

    #[test]
    fn test_schedule_ack_roundtrip_conserves_in_flight() {
        // The in_flight budget is charged ONCE, at schedule time. The L1
        // stall (P7 follow-up 2) was a double charge: schedule() charged,
        // then the paced drain charged the same symbols again at send
        // time — +1 leak per symbol, TUN gate jammed shut, throughput
        // throttled to the 2s leak-guard decay (~30 KB/s at C2).
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock);
        sched.add_path(0);

        let source: Vec<_> = (0..8).map(|i| make_symbol(i, false)).collect();
        let assignments = sched.schedule(source, vec![]);
        let scheduled: u32 = assignments.iter().map(|(_, s)| s.len() as u32).sum();
        assert_eq!(scheduled, 8);
        assert_eq!(sched.path(0).unwrap().in_flight, 8);

        // The paced drain charges TOKENS only — in_flight must not move
        // between schedule and ack (this is what net/mod.rs does now).
        sched.path_mut(0).unwrap().consume_pace_tokens(8);
        assert_eq!(sched.path(0).unwrap().in_flight, 8);

        // ACK feedback releases everything: budget conserved, gate opens.
        sched.ack(0, 8);
        assert_eq!(
            sched.path(0).unwrap().in_flight,
            0,
            "schedule → send → ack must conserve the in_flight budget"
        );
    }

    #[test]
    fn test_in_flight_expiry_releases_stranded_budget() {
        // ACKs are best-effort datagrams: a lost ACK strands its release
        // forever. The time-based expiry (max(4×SRTT, 250ms)) must reopen
        // the gate at RTT timescale without any feedback at all.
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock.clone());
        sched.add_path(0);

        let path = sched.path_mut(0).unwrap();
        for _ in 0..4 {
            path.record_rtt_sample(millis(10)); // srtt 10ms → horizon 250ms
        }
        path.charge_in_flight(56);
        assert_eq!(path.in_flight, 56);

        // Well before the horizon: nothing expires.
        clock.advance(millis(100));
        let path = sched.path_mut(0).unwrap();
        path.expire_in_flight();
        assert_eq!(path.in_flight, 56);

        // A partial ACK releases FIFO; the stranded remainder expires
        // once the horizon passes.
        path.release_in_flight(50);
        assert_eq!(path.in_flight, 6);
        clock.advance(millis(200)); // total 300ms > 250ms horizon
        let path = sched.path_mut(0).unwrap();
        path.expire_in_flight();
        assert_eq!(
            path.in_flight, 0,
            "stranded budget must expire at RTT timescale, not the 2s guard"
        );
    }

    #[test]
    fn test_c2_loop_cwnd_grows_past_200_within_5s() {
        // Full C2 loop at the scheduler level (100 Mbit / 10ms RTT / Bulk),
        // mirroring the production wiring: schedule-time budget charge,
        // token-paced sends stamped at WIRE time (echo-timestamp RTT
        // therefore excludes pacing-queue delay — verified hypothesis:
        // batches are built at send time from the carry), per-datagram
        // ACKs with ~1.3% of them lost (stranding releases), and
        // time-based expiry. cwnd must ramp past 200 symbols within 5
        // simulated seconds and the sender must be ACK-clocked, not
        // leak-guard throttled.
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new_with_hint(clock.clone(), ProtocolHint::Bulk);
        sched.add_path(0);

        const OWD: Duration = Duration::from_millis(5); // 10ms RTT
        let mut carry: u32 = 0; // interleaver + pacing carry (already charged)
        // (ack_arrival, symbols, wire_send_instant)
        let mut acks: VecDeque<(Instant, u32, Instant)> = VecDeque::new();
        let mut wire_counter: u64 = 0;
        let mut total_sent: u64 = 0;

        for _tick in 0..5000 {
            let now = clock.now();

            // Encoder + TUN gate: schedule one 56-symbol block (64KB /
            // 1200B) whenever the committed budget is under cwnd.
            {
                let p = sched.path_mut(0).unwrap();
                p.expire_in_flight();
                if p.in_flight < p.cwnd {
                    p.charge_in_flight(56);
                    carry += 56;
                }
            }

            // Pacer: send from the carry under tokens; the batch timestamp
            // is stamped HERE (wire time), as in send_interleaved_batches.
            {
                let p = sched.path_mut(0).unwrap();
                p.pace_refill();
                let budget = (p.pace_tokens().max(0.0) as u32).min(carry);
                if budget > 0 {
                    p.consume_pace_tokens(budget);
                    carry -= budget;
                    total_sent += budget as u64;
                    // Receiver ACKs each datagram after one RTT; ~1.3% of
                    // ACK datagrams are lost (their releases stranded).
                    let mut acked = 0;
                    for _ in 0..budget {
                        wire_counter += 1;
                        if wire_counter % 77 != 0 {
                            acked += 1;
                        }
                    }
                    if acked > 0 {
                        acks.push_back((now + OWD * 2, acked, now));
                    }
                }
            }

            // Deliver due ACKs: RTT = now − echoed wire timestamp.
            while acks.front().is_some_and(|(t, _, _)| *t <= now) {
                let (_, n, sent_at) = acks.pop_front().unwrap();
                let rtt = now.duration_since(sent_at);
                let p = sched.path_mut(0).unwrap();
                p.record_rtt_sample(rtt);
                p.release_in_flight(n);
                p.on_ack(n);
            }

            clock.advance(millis(1));
        }

        let cwnd = sched.path(0).unwrap().cwnd;
        assert!(
            cwnd > 200,
            "C2 loop must ramp cwnd past 200 symbols within 5s, got {cwnd}"
        );
        // Ack-clocked throughput, not the 2s leak-guard trickle (the L1
        // stall sent ~450 symbols in 15s; here 5s must move far more).
        assert!(
            total_sent > 20_000,
            "sender must be ack-clocked, not gate-starved: sent {total_sent}"
        );
    }

    #[test]
    fn test_low_floor_clamp_no_spurious_backoff() {
        // LAN-class floor (200us): the backoff threshold clamps at 0.1ms
        // and dq clamps at the SAME 0.1ms, so sub-clamp jitter (raw dq
        // 80us) can never back off — while a genuine standing queue
        // (raw dq 200us > clamp) still does.
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock.clone());
        sched.add_path(0);

        for round in 0..10 {
            let sample = if round % 2 == 0 { 200 } else { 280 };
            let p = sched.path_mut(0).unwrap();
            p.record_rtt_sample(Duration::from_micros(sample));
            clock.advance(millis(1)); // >> sub-ms SRTT: update every round
            let before = sched.path(0).unwrap().cwnd;
            sched.ack(0, 8);
            let after = sched.path(0).unwrap().cwnd;
            assert!(
                after >= before,
                "jitter below the dq clamp must not back off (round {round}): {before} -> {after}"
            );
        }
        assert!(
            sched.path(0).unwrap().cwnd > PathState::INITIAL_CWND,
            "LAN ramp should have grown"
        );

        // Sanity: a real standing queue above the clamp DOES back off.
        let p = sched.path_mut(0).unwrap();
        p.record_rtt_sample(Duration::from_micros(400)); // raw dq 200us
        clock.advance(millis(1));
        let before = sched.path(0).unwrap().cwnd;
        sched.ack(0, 8);
        assert!(
            sched.path(0).unwrap().cwnd < before,
            "genuine LAN queue must still back off"
        );
    }

    // ===================================================================
    // RWM Phase B — per-symbol placement law (paper §16.3). The cost is
    //   in_flight/cwnd + w_lat·(E_prop/ref_srtt) + w_bw·r + w_div·fate,
    // sampled as P(i) ∝ exp(−cost/T).
    // ===================================================================

    /// Look up a path's probability in a place_probs distribution.
    fn prob_of(dist: &[(PathId, f64)], id: PathId) -> f64 {
        dist.iter().find(|(p, _)| *p == id).map(|(_, w)| *w).unwrap_or(0.0)
    }

    fn set_rtt(sched: &mut Scheduler, id: PathId, ms: u64) {
        // The estimator RTT is an EWMA (α = 0.125) seeded at 50 ms; feed enough
        // samples to converge so tests exercise the intended RTT, not a warm-up
        // blend of it and the seed.
        let p = sched.path_mut(id).unwrap();
        for _ in 0..60 {
            p.estimator.record_rtt(std::time::Duration::from_millis(ms));
        }
    }

    /// (a) Idle 2-path → placement concentrates on the cheapest (lowest-RTT)
    /// path. Softmax "concentrate" = the vast majority of the mass.
    #[test]
    fn place_idle_concentrates_on_cheapest() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
        sched.add_path(0);
        sched.add_path(1);
        set_rtt(&mut sched, 0, 10);
        set_rtt(&mut sched, 1, 50); // path 1 is 5× slower
        // both idle (in_flight = 0)
        let dist = sched.place_probs(false, &[]);
        let p0 = prob_of(&dist, 0);
        let p1 = prob_of(&dist, 1);
        assert!(p0 > 0.95, "cheapest path must take the mass, got p0={p0}");
        assert!(p0 > p1);
    }

    /// (b) As the chosen path's in_flight rises, placement shifts CONTINUOUSLY
    /// to the other path — no threshold jump. Assert strict monotonic shift.
    #[test]
    fn place_shifts_monotonically_with_load() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
        sched.add_path(0);
        sched.add_path(1);
        set_rtt(&mut sched, 0, 10);
        set_rtt(&mut sched, 1, 10); // symmetric: isolate the load term
        let cwnd = sched.path(0).unwrap().cwnd; // 10

        let mut prev_p0 = f64::INFINITY;
        // Sweep in_flight from empty to 2× cwnd (into overdraft) — the path is
        // never removed from the distribution (no capacity filter), so the
        // shift is continuous THROUGH saturation, not a jump at cwnd.
        for infl in 0..=(2 * cwnd) {
            sched.path_mut(0).unwrap().in_flight = infl;
            let dist = sched.place_probs(false, &[]);
            let p0 = prob_of(&dist, 0);
            let p1 = prob_of(&dist, 1);
            assert!(
                p0 < prev_p0,
                "p0 must strictly decrease as path-0 load rises: infl={infl} p0={p0} prev={prev_p0}"
            );
            // p1 is its complement (two paths) → strictly increasing.
            assert!((p0 + p1 - 1.0).abs() < 1e-9);
            prev_p0 = p0;
        }
        // Ended favouring the unloaded path.
        assert!(prev_p0 < 0.1, "heavily loaded path should be largely abandoned");
    }

    /// (c) Water-filling equilibrium: the fixed point of marginal-cost
    /// equalisation is `in_flight/cwnd` equal across paths, i.e. in_flight ∝
    /// cwnd ∝ capacity. At that stock ratio placement is BALANCED (both paths
    /// used equally) — the signature that the law fills proportional to
    /// capacity rather than concentrating.
    #[test]
    fn place_backlog_waterfills_proportional_to_capacity() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
        sched.add_path(0);
        sched.add_path(1);
        set_rtt(&mut sched, 0, 10);
        set_rtt(&mut sched, 1, 10);
        // Path 0 has 2× the capacity of path 1.
        sched.path_mut(0).unwrap().cwnd = 20;
        sched.path_mut(1).unwrap().cwnd = 10;
        // Equilibrium stock: in_flight ∝ cwnd ⇒ equal fill fraction 0.4.
        sched.path_mut(0).unwrap().in_flight = 8;
        sched.path_mut(1).unwrap().in_flight = 4;
        let dist = sched.place_probs(false, &[]);
        let p0 = prob_of(&dist, 0);
        let p1 = prob_of(&dist, 1);
        assert!(p0 > 0.1 && p1 > 0.1, "both paths used at equilibrium: p0={p0} p1={p1}");
        assert!((p0 - p1).abs() < 0.05, "balanced at the capacity-proportional fixed point");

        // And off-equilibrium (equal stock, unequal capacity) the law pushes
        // MORE toward the higher-capacity (lower-fill) path.
        sched.path_mut(0).unwrap().in_flight = 6;
        sched.path_mut(1).unwrap().in_flight = 6;
        let dist2 = sched.place_probs(false, &[]);
        assert!(prob_of(&dist2, 0) > prob_of(&dist2, 1));
    }

    /// Cross-path repair placement (RWM_XPATH_REPAIR, §16.3 C8 realization).
    /// When the FAST path is source-saturated (spare≈0) and the SLOW path is
    /// underutilized (high spare), `place_repair_spare_path` routes repair to the
    /// SLOW path — so proactive repair rides the spare path instead of displacing
    /// fast-path source. Symmetric spare → uniform split (no concentration).
    #[test]
    fn place_repair_spare_routes_to_underutilized_path() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
        sched.add_path(0); // fast
        sched.add_path(1); // slow
        set_rtt(&mut sched, 0, 10);
        set_rtt(&mut sched, 1, 40);
        // Fast path saturated with source: in_flight == cwnd ⇒ spare == 0.
        // Slow path lightly loaded: spare high.
        sched.path_mut(0).unwrap().cwnd = 40;
        sched.path_mut(0).unwrap().in_flight = 40; // spare 0
        sched.path_mut(1).unwrap().cwnd = 16;
        sched.path_mut(1).unwrap().in_flight = 4; // spare 3.0
        // Every repair should ride the SLOW (spare) path 1.
        let mut to_slow = 0;
        for _ in 0..200 {
            if sched.place_repair_spare_path() == Some(1) {
                to_slow += 1;
            }
        }
        assert_eq!(to_slow, 200, "repair must ride the spare slow path, got {to_slow}/200 on path 1");

        // Symmetric spare (equal fill fraction) ⇒ uniform split, no concentration.
        sched.path_mut(0).unwrap().cwnd = 20;
        sched.path_mut(0).unwrap().in_flight = 10; // spare 1.0
        sched.path_mut(1).unwrap().cwnd = 20;
        sched.path_mut(1).unwrap().in_flight = 10; // spare 1.0
        let mut c0 = 0;
        for _ in 0..2000 {
            if sched.place_repair_spare_path() == Some(0) {
                c0 += 1;
            }
        }
        assert!(
            (700..=1300).contains(&c0),
            "symmetric spare must split ~evenly (no argmax concentration), got {c0}/2000 on path 0"
        );
    }

    /// (d) Repair fate steers a repair OFF the path that carried the window
    /// symbols it covers; source placement ignores fate.
    #[test]
    fn place_repair_fate_steers_off_covered_path() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
        sched.add_path(0);
        sched.add_path(1);
        set_rtt(&mut sched, 0, 10);
        set_rtt(&mut sched, 1, 10); // identical paths — only fate differs

        // Source ignores fate → balanced even when all coverage is on path 0.
        let src = sched.place_probs(false, &[0, 0, 0, 0]);
        assert!((prob_of(&src, 0) - prob_of(&src, 1)).abs() < 0.05);

        // Repair whose coverage is entirely on path 0 → steered to path 1.
        let rep = sched.place_probs(true, &[0, 0, 0, 0]);
        assert!(
            prob_of(&rep, 1) > 0.95,
            "repair must avoid its own coverage: p1={}",
            prob_of(&rep, 1)
        );

        // Split coverage → fate equal → balanced again.
        let rep_split = sched.place_probs(true, &[0, 0, 1, 1]);
        assert!((prob_of(&rep_split, 0) - prob_of(&rep_split, 1)).abs() < 0.05);
    }

    /// (e) T → 0 collapses the softmax to argmin (strict best-path, the
    /// no-cutoffs limit).
    #[test]
    fn place_temperature_zero_is_argmin() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
        sched.add_path(0);
        sched.add_path(1);
        set_rtt(&mut sched, 0, 10); // cheaper
        set_rtt(&mut sched, 1, 50);
        let dist = sched.place_probs_with_temperature(false, &[], 1e-9);
        assert!(prob_of(&dist, 0) > 0.999, "T→0 → argmin all mass on path 0");
        assert!(prob_of(&dist, 1) < 1e-3);
    }

    /// Single path ⇒ that path always (byte-identical to the pre-RWM
    /// single-path sender — the law with N=1 is a no-op).
    #[test]
    fn place_single_path_is_identity() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
        sched.add_path(0);
        set_rtt(&mut sched, 0, 20);
        let dist = sched.place_probs(false, &[]);
        assert_eq!(dist.len(), 1);
        assert_eq!(dist[0].0, 0);
        assert!((dist[0].1 - 1.0).abs() < 1e-12);
        // Even heavily overdrafted, the lone path is still chosen.
        sched.path_mut(0).unwrap().in_flight = 10_000;
        assert_eq!(sched.place_symbol(false, &[]), Some(0));
    }

    // ── Frontier-slack placement (goal-gate "C8 Slow-Path Conversion",
    //    `RWM_PLACE_SLACK`) — the law is a STRICT continuous generalization
    //    of the shipped cost: S = 0 bit-identical, S > 0 un-starves the
    //    slow path up to exactly its deadline-feasible backlog. ───────────

    /// S = 0 (the default, and any non-finite/negative setter input)
    /// reproduces the shipped placement distribution bit-exactly.
    #[test]
    fn place_slack_zero_is_bit_identical() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
        sched.add_path(0);
        sched.add_path(1);
        set_rtt(&mut sched, 0, 10);
        set_rtt(&mut sched, 1, 50);
        sched.path_mut(0).unwrap().in_flight = 7;
        let base = sched.place_probs(false, &[]);
        sched.set_place_slack(0.0);
        assert_eq!(base, sched.place_probs(false, &[]));
        sched.set_place_slack(-3.0);
        assert_eq!(base, sched.place_probs(false, &[]));
        sched.set_place_slack(f64::NAN);
        assert_eq!(base, sched.place_probs(false, &[]));
        assert_eq!(sched.place_slack(), 0.0);
    }

    /// Rising S monotonically feeds the slow path: the Bulk softmax's
    /// idle-propagation starvation (p1 ~ e^-10 at S = 0) relaxes toward the
    /// uniform clamp as S covers the slow path's delivery time — no
    /// threshold, strictly non-decreasing in S.
    #[test]
    fn place_slack_monotonically_feeds_the_slow_path() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
        sched.add_path(0);
        sched.add_path(1);
        set_rtt(&mut sched, 0, 10);
        set_rtt(&mut sched, 1, 50); // 5× slower — the c8 shape
        let mut prev_p1 = -1.0;
        for slack_ms in [0u64, 5, 10, 20, 30, 50, 100] {
            sched.set_place_slack(slack_ms as f64 / 1000.0);
            let p1 = prob_of(&sched.place_probs(false, &[]), 1);
            assert!(
                p1 >= prev_p1 - 1e-12,
                "p1 must be non-decreasing in S: S={slack_ms}ms p1={p1} prev={prev_p1}"
            );
            prev_p1 = p1;
        }
        // Starved at S = 0 …
        sched.set_place_slack(0.0);
        assert!(prob_of(&sched.place_probs(false, &[]), 1) < 0.01);
        // … equal-cost (uniform clamp) once S covers both idle delivery times.
        sched.set_place_slack(0.1);
        let p1 = prob_of(&sched.place_probs(false, &[]), 1);
        assert!((p1 - 0.5).abs() < 0.05, "clamped region ⇒ ~uniform, got p1={p1}");
    }

    /// The law still prices LATENESS: a slow path whose backlog's completion
    /// time exceeds S is choked continuously (the c8-pbs unbounded-queue
    /// failure cannot form).
    #[test]
    fn place_slack_still_chokes_beyond_the_deadline() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
        sched.add_path(0);
        sched.add_path(1);
        set_rtt(&mut sched, 0, 10);
        set_rtt(&mut sched, 1, 50);
        sched.set_place_slack(0.05); // S = 50 ms
        // Backlog the slow path to ~2× cwnd: Ê_1 ≈ 2·50 + 25 = 125 ms ≫ S.
        let cwnd1 = sched.path(1).unwrap().cwnd;
        sched.path_mut(1).unwrap().in_flight = 2 * cwnd1;
        let p1 = prob_of(&sched.place_probs(false, &[]), 1);
        assert!(p1 < 0.01, "deadline-exceeded slow path must be choked, got p1={p1}");
    }

    /// The lateness budget never exceeds the recovery plane's patience:
    /// even with S at its 250 ms ceiling, a slow-path backlog whose
    /// completion time exceeds 9/8·srtt_i is charged — the unbounded-S
    /// smoke failure (placement tolerating 250 ms while the hole law
    /// re-serves at ~9/8·srtt: retxo_p1 = 49%) cannot form.
    #[test]
    fn place_slack_deadline_capped_by_recovery_patience() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
        sched.add_path(0);
        sched.add_path(1);
        set_rtt(&mut sched, 0, 10);
        set_rtt(&mut sched, 1, 50); // patience for p1 ≈ 56 ms
        sched.set_place_slack(0.25); // S at the ceiling
        // Backlog p1 so Ê_1 ≈ (8/10)·50 + 25 = 65 ms: inside S, but PAST
        // the 9/8·srtt_1 = 56 ms recovery patience → must be charged.
        sched.path_mut(1).unwrap().in_flight = 8;
        let p1 = prob_of(&sched.place_probs(false, &[]), 1);
        assert!(
            p1 < 0.4,
            "beyond-patience backlog must lose mass even at max S, got p1={p1}"
        );
        // And the charge grows with the backlog (continuous choke).
        sched.path_mut(1).unwrap().in_flight = 20;
        let p1_deep = prob_of(&sched.place_probs(false, &[]), 1);
        assert!(p1_deep < p1, "deeper backlog ⇒ smaller mass ({p1_deep} < {p1})");
    }

    /// Symmetric paths split 50/50 with or without slack — the c7 cell's
    /// placement is untouched by the law (any symmetric cost ⇒ 50/50).
    #[test]
    fn place_slack_symmetric_split_unchanged() {
        for slack in [0.0, 0.08, 0.25] {
            let mut sched =
                Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
            sched.add_path(0);
            sched.add_path(1);
            set_rtt(&mut sched, 0, 20);
            set_rtt(&mut sched, 1, 20);
            sched.path_mut(0).unwrap().in_flight = 5;
            sched.path_mut(1).unwrap().in_flight = 5;
            sched.set_place_slack(slack);
            let dist = sched.place_probs(false, &[]);
            let p0 = prob_of(&dist, 0);
            assert!(
                (p0 - 0.5).abs() < 1e-9,
                "symmetric split must stay 50/50 at S={slack}, got p0={p0}"
            );
        }
    }

    // ── THE COLD-START PLACEMENT PRICE (`RWM_COLD_PLACE`) ─────────────────
    //
    // Provenance of these tests, stated because it is the honest part: the
    // defect they bound was FIRST claimed at the SF bench's `c7x4` symmetric
    // quad, and that claim was RETRACTED — the quad's per-path gauges were
    // truncated at `pid < 2`, so the "lock-in" was an instrument artifact and
    // the quad in fact spreads evenly over all four legs (see
    // `the_symmetric_quad_is_deterministic_and_all_four_legs_carry_and_warm`).
    // The ARITHMETIC in that claim was nevertheless correct, and it binds in a
    // regime the bench has no geometry for: a leg that joins a set whose
    // incumbents are ALREADY warm. Nothing here is measured on a wire; these
    // bound the LAW, at absolute values, and the wire question is listed
    // rather than answered.

    /// A leg that joins a set of ALREADY-WARM incumbents is priced at the
    /// 50-ms `DEFAULT_SRTT`-class seed and cannot win the placement argmin
    /// until the incumbents are >2× overdrawn — and because it wins nothing
    /// it is never measured, so the state is a FIXED POINT. Under
    /// `RWM_COLD_PLACE` the same leg is priced at the set's own fastest
    /// MEASURED srtt and is admitted immediately.
    ///
    /// Absolute, not ordinal: at T → 0 the placement is an argmin, so the
    /// cold leg's probability is exactly 0.0 or exactly 1.0 and there is
    /// nothing to tune. The incumbents' fill fraction is swept so the result
    /// is a PRICE with a crossing, not an exclusion — the OFF arm does admit
    /// the cold leg, but only past a fill the shipped law has no reason to
    /// reach, which is what makes the fixed point stick.
    #[test]
    fn a_late_joining_leg_is_locked_out_by_the_cold_price_and_admitted_without_it() {
        // Two incumbents warm at 8 ms, one leg joining cold. `fill` =
        // in_flight/cwnd on the incumbents; the cold leg has nothing in
        // flight, which is precisely why it looks expensive.
        let build = |fill: f64, cold_place: bool| -> Vec<(PathId, f64)> {
            let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
            sched.set_cold_place(cold_place);
            for id in 0..2 {
                sched.add_path(id);
                set_rtt(&mut sched, id, 8);
                let p = sched.path_mut(id).unwrap();
                p.cwnd = 32;
                p.in_flight = (32.0 * fill) as u32;
            }
            sched.add_path(2); // the late joiner: no RTT sample, ever
            assert!(
                sched.path(2).unwrap().srtt_measured().is_none(),
                "the joining leg must be UNMEASURED or this test proves nothing"
            );
            sched.place_probs_with_temperature(false, &[], f64::MIN_POSITIVE)
        };

        // (1) THE LOCK-OUT. At any fill the shipped stack actually operates
        // at, the cold leg's mass is exactly zero.
        for fill in [0.25_f64, 0.5, 1.0, 2.0] {
            let d = build(fill, false);
            assert_eq!(
                prob_of(&d, 2),
                0.0,
                "gate OFF, incumbents at fill {fill}: the cold leg took mass, so \
                 the 50-ms price is not the exclusion this test bounds"
            );
            assert!(
                prob_of(&d, 0) + prob_of(&d, 1) > 0.999,
                "gate OFF, fill {fill}: the incumbents must hold all the mass"
            );
        }

        // (2) IT IS A PRICE, NOT AN EXCLUSION — the crossing exists, and it
        // sits ABOVE a 2× overdraft. `E_cold = 25 ms` against
        // `E_warm = fill·8 + 4 + eps·8 ms`, so the cold leg wins at
        // `fill > (25 − 4)/8 ≈ 2.6`. A law whose exploration price is only
        // paid by a path already 2.6× past its window is a law that never
        // explores.
        assert_eq!(prob_of(&build(4.0, false), 2), 1.0, "the crossing does not exist");

        // (3) THE FIXED POINT. Under the lock-out the leg draws no symbol, so
        // it takes no sample, so it stays cold: sampling the shipped
        // placement is stationary, not merely improbable.
        {
            let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
            sched.set_cold_place(false);
            for id in 0..2 {
                sched.add_path(id);
                set_rtt(&mut sched, id, 8);
                let p = sched.path_mut(id).unwrap();
                p.cwnd = 32;
                p.in_flight = 16;
            }
            sched.add_path(2);
            for _ in 0..500 {
                assert_ne!(
                    sched.place_symbol(false, &[]),
                    Some(2),
                    "the cold leg drew a symbol — the fixed point is not closed"
                );
            }
        }

        // (4) THE REPAIR. Same states, gate ON: the cold leg is priced at the
        // set's own fastest measured srtt (8 ms ⇒ E = 4 ms) and wins outright
        // the moment the incumbents carry anything at all.
        for fill in [0.25_f64, 0.5, 1.0, 2.0, 4.0] {
            let d = build(fill, true);
            assert_eq!(
                prob_of(&d, 2),
                1.0,
                "gate ON, incumbents at fill {fill}: the cold leg must win — it \
                 is the cheapest path in the set by the objective's own units"
            );
        }

        // (5) AND IT IS SELF-LIMITING WITHOUT A THRESHOLD. The repaired price
        // buys exploration, not a monopoly: once the explored leg carries its
        // own backlog the SAME formula hands the placement back. No counter,
        // no warm-up phase, no `if cold` beyond the estimator's `None`.
        {
            let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
            sched.set_cold_place(true);
            for id in 0..2 {
                sched.add_path(id);
                set_rtt(&mut sched, id, 8);
                let p = sched.path_mut(id).unwrap();
                p.cwnd = 32;
                p.in_flight = 8; // fill 0.25
            }
            sched.add_path(2);
            let p2 = sched.path_mut(2).unwrap();
            p2.cwnd = 32;
            p2.in_flight = 16; // the explored leg is now the LOADED one
            let d = sched.place_probs_with_temperature(false, &[], f64::MIN_POSITIVE);
            assert_eq!(
                prob_of(&d, 2),
                0.0,
                "the repaired price kept feeding a leg that is now the most \
                 loaded in the set — that would be a monopoly, not exploration"
            );
        }
    }

    /// **OFF IS BIT-IDENTICAL, AND SO IS ON ONCE EVERY LEG IS MEASURED.**
    ///
    /// The two halves of the gate's safety claim, as exact `f64` equality
    /// rather than a tolerance:
    ///
    ///   - gate OFF at ANY state ⇒ the shipped expression verbatim (the cold
    ///     price IS `p.srtt()`), so no arm of any battery can move because
    ///     this landed;
    ///   - gate ON with every active leg measured ⇒ `srtt_of == p.srtt()` at
    ///     every leg, so the repair is INERT in the warm regime and the §13.8
    ///     objective it minimizes is untouched there. Only the COLD regime
    ///     changes, which is the whole design claim.
    #[test]
    fn the_cold_price_is_inert_off_and_inert_once_every_leg_is_measured() {
        // A state generator covering both regimes: `cold` = how many of the
        // four legs have never had a sample.
        //
        // BOTH ARMS FROM ONE SCHEDULER (the eighth HashMap-order lesson):
        // `place_probs` normalizes by summing over the paths in the MAP'S
        // iteration order, and float addition is not associative — two
        // separately-built schedulers hash differently, so their inert-arm
        // probabilities can differ at the ULP even when the gate provably
        // changes nothing. Sorting the OUTPUT (below) fixes the zip pairing
        // but not the internal summation order. One instance, flag toggled,
        // makes bit-equality a claim about the GATE instead of the hasher.
        let build = |cold: usize| -> (Vec<(PathId, f64)>, Vec<(PathId, f64)>) {
            let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
            for id in 0..4u32 {
                sched.add_path(id);
                if (id as usize) < 4 - cold {
                    set_rtt(&mut sched, id, 10 + 10 * u64::from(id));
                }
                let p = sched.path_mut(id).unwrap();
                p.cwnd = 16 + 4 * id;
                p.in_flight = 3 * id + 1;
            }
            // SORTED by path id: `place_probs` yields `HashMap` order, so an
            // unsorted zip would compare path 3 against path 1 and "find" a
            // difference that is only the map's.
            sched.set_cold_place(false);
            let mut off = sched.place_probs_with_temperature(false, &[], 0.15);
            off.sort_by_key(|(pid, _)| *pid);
            sched.set_cold_place(true);
            let mut on = sched.place_probs_with_temperature(false, &[], 0.15);
            on.sort_by_key(|(pid, _)| *pid);
            (off, on)
        };

        for cold in 0..=4 {
            let (off, on) = build(cold);
            // MECHANISM LIVENESS: the arms must actually DIFFER somewhere, or
            // the equalities below are vacuous. They differ exactly when some
            // leg is cold and some leg is measured.
            let differs = off
                .iter()
                .zip(on.iter())
                .any(|((_, a), (_, b))| a.to_bits() != b.to_bits());
            let mixed = cold > 0 && cold < 4;
            assert_eq!(
                differs, mixed,
                "cold={cold}: the gate must move the distribution EXACTLY when \
                 the set is mixed (some measured, some not) — differs={differs}"
            );
            if !mixed {
                for ((pa, a), (pb, b)) in off.iter().zip(on.iter()) {
                    assert_eq!(pa, pb);
                    assert_eq!(
                        a.to_bits(),
                        b.to_bits(),
                        "cold={cold}: path {pa} moved {a} → {b}; with no leg cold \
                         (or every leg cold) the two arms are the same formula on \
                         the same inputs and must agree BIT-for-bit"
                    );
                }
            }
        }
    }

    /// The gate is an anchor-hygiene family member and ships OFF, so a
    /// freshly constructed `Scheduler` must carry the shipped price unless
    /// the environment says otherwise. Reads the same cached resolution
    /// `RuntimeGates` echoes, so the echo and the behaviour cannot disagree.
    #[test]
    fn a_fresh_scheduler_carries_the_resolved_cold_place_setting() {
        let sched = Scheduler::new(Arc::new(WallClock));
        assert_eq!(
            sched.cold_place(),
            cold_place_active(),
            "the scheduler's placement price and the process gate disagree — \
             the [GATES] echo would then be describing a different machine"
        );
    }

    // feat/btlbw-rate-sample: the BBR send-interval anchor must read the TRUE
    // bottleneck rate under (a) ack-aggregation (batched acks) and (b) a deep
    // standing queue — the exact conditions that made the legacy ack-interval
    // anchor over-read ~145× at L1.  Driven by a bottleneck-link simulation: we
    // SEND 3× the link rate (a standing queue builds without bound) and the link
    // FIFO-drains at the true rate R; acks are processed in BATCHES (aggregation).
    // The measured BtlBw must track R, NOT the send rate and NOT queue/interval.
    #[test]
    fn rate_sample_anchor_reads_true_btlbw_under_aggregation_and_queue() {
        use std::collections::{BTreeMap, VecDeque};
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new_with_hint(clock.clone(), ProtocolHint::Bulk);
        sched.add_path(0);
        let prop = Duration::from_millis(5); // one-way; RTprop ~ 10ms
        // Seed RTprop so the max-filter window (~10·RTprop) and btlbw product form.
        for _ in 0..4 {
            sched.path_mut(0).unwrap().record_rtt_sample(Duration::from_millis(10));
        }

        let link_r: u64 = 8; // TRUE bottleneck: 8 sym/ms = 8000 sym/s
        let send_s: u64 = 24; // SEND 3× the link rate → standing queue grows
        let tick = Duration::from_millis(1);
        let mut seq: u64 = 0;
        let mut link_fifo: VecDeque<u64> = VecDeque::new(); // waiting for link service
        let mut deliver_due: BTreeMap<u64, Vec<u64>> = BTreeMap::new(); // arrival_us → seqs
        let start = clock.now();
        let us = |c: &MockClock| c.now().duration_since(start).as_micros() as u64;

        for step in 0..300u64 {
            // SEND send_s symbols this ms (overload).
            for _ in 0..send_s {
                sched.path_mut(0).unwrap().on_src_sent(seq, false);
                link_fifo.push_back(seq);
                seq += 1;
            }
            // The LINK serves link_r symbols this ms (the bottleneck); each served
            // symbol arrives one propagation delay later.
            let arrival = us(&clock) + prop.as_micros() as u64;
            for _ in 0..link_r {
                if let Some(s) = link_fifo.pop_front() {
                    deliver_due.entry(arrival).or_default().push(s);
                }
            }
            // ACK AGGREGATION: only "process acks" every 3 ms, delivering ALL due
            // symbols at the SAME clock instant (a batched ack → tiny ack Δt).
            if step % 3 == 0 {
                let nowu = us(&clock);
                let due: Vec<u64> = deliver_due
                    .range(..=nowu)
                    .flat_map(|(_, v)| v.iter().copied())
                    .collect();
                deliver_due.retain(|&t, _| t > nowu);
                for s in due {
                    sched.path_mut(0).unwrap().on_src_delivered_seq(s);
                }
            }
            clock.advance(tick);
        }

        let btlbw = sched
            .path(0)
            .unwrap()
            .btlbw_sym_per_s()
            .expect("anchor must establish");
        let true_rate = (link_r * 1000) as f64; // 8000 sym/s
        // The standing queue is DEEP (send 3× drain for 300ms): outstanding is far
        // above one BDP.  A queue/ack-interval anchor would read many× the link;
        // the send-interval anchor must track the true bottleneck within ~2×.
        assert!(
            btlbw > 0.5 * true_rate && btlbw < 2.0 * true_rate,
            "send-interval BtlBw must track the TRUE link rate {true_rate:.0} sym/s \
             under batched acks + a standing queue, got {btlbw:.0} (send rate was \
             {} sym/s)",
            send_s * 1000
        );
        // Crucially, NOT the ~145× over-read the legacy ack-interval anchor gave.
        assert!(
            btlbw < 10.0 * true_rate,
            "must NOT exhibit the legacy aggregation over-read: {btlbw:.0} vs true {true_rate:.0}"
        );
    }

    // feat/btlbw-rate-sample: an APP-LIMITED (starved) sample that reads below the
    // running max must NOT enter the max-filter (BBR: app-limited samples may only
    // RAISE the anchor, never be read as bw dropping / corrupt a starved interval).
    #[test]
    fn rate_sample_excludes_app_limited_samples_below_the_max() {
        let clock = Arc::new(MockClock::new());
        let mut copa = CopaState::new(clock.clone(), ProtocolHint::Bulk);
        copa.record_rtt(Duration::from_millis(10)); // RTprop = 10 ms

        // Establish a healthy max with a valid sample spanning ≥ RTprop: send
        // seq0, inject 50 deliveries, ack it one RTprop-plus later.
        copa.rs_on_sent(0, false);
        copa.rs_delivered += 50;
        clock.advance(Duration::from_millis(20)); // ≥ RTprop
        copa.rs_on_delivered(0); // rate ≈ 51 / 0.02 s
        let max_before = copa.max_bw;
        let samples_before = copa.bw_samples.len();
        assert!(max_before > 0.0, "baseline max must establish: {max_before}");

        // An APP-LIMITED sample reading BELOW the max must be EXCLUDED: send
        // seq1 app-limited, deliver ONE symbol one RTprop-plus later (low rate,
        // interval ≥ RTprop so the MinRTT guard passes and only app-limited
        // gates it out).
        copa.rs_on_sent(1, true);
        clock.advance(Duration::from_millis(20));
        copa.rs_on_delivered(1);
        assert_eq!(
            copa.bw_samples.len(),
            samples_before,
            "app-limited sample below the max must not enter the filter"
        );
        assert!(
            (copa.max_bw - max_before).abs() < 1.0,
            "app-limited low sample must not change the max: before={max_before} after={}",
            copa.max_bw
        );

        // A non-app-limited sample at a genuinely HIGHER rate (interval ≥ RTprop)
        // is still admitted and raises the max.
        copa.rs_on_sent(2, false);
        copa.rs_delivered += 100;
        clock.advance(Duration::from_millis(20)); // ≥ RTprop
        copa.rs_on_delivered(2); // rate ≈ 101 / 0.02 s > max_before
        assert!(
            copa.max_bw > max_before,
            "a higher genuine sample (interval ≥ RTprop) must raise the max: \
             before={max_before} after={}",
            copa.max_bw
        );

        // A sub-RTprop burst (interval < RTprop) is REJECTED by the MinRTT guard —
        // this is the ack-aggregation / send-burst over-read defence.
        let max_after = copa.max_bw;
        copa.rs_on_sent(3, false);
        copa.rs_delivered += 10_000; // huge delivered count …
        clock.advance(Duration::from_micros(100)); // … over a tiny interval
        copa.rs_on_delivered(3);
        assert!(
            (copa.max_bw - max_after).abs() < 1.0,
            "a sub-RTprop burst must be rejected (no over-read): before={max_after} after={}",
            copa.max_bw
        );
    }

    // ----- Honest Inputs (RWM_HONEST_ANCHOR / RWM_HONEST_K, goal-gate --------
    // ----- "Honest Inputs") --------------------------------------------------

    /// THE equivalence pin for `RWM_HONEST_ANCHOR` (its OFF-value property
    /// and its ON-value property are the SAME property): the monotonic
    /// max-deque's front equals the legacy full-window fold over
    /// `bw_samples` after EVERY push and EVERY eviction, across both feed
    /// paths (per-ack `record_delivery` and per-symbol `rs_on_delivered`),
    /// across window-length changes (min_rtt moving the ≈10·RTprop cutoff)
    /// and across long idle gaps (mass evictions). The gate may therefore
    /// select COST only, never a value — which is what makes this fix
    /// zero-constant by construction.
    #[test]
    fn bw_mono_front_equals_full_window_fold() {
        let clock = Arc::new(MockClock::new());
        let mut legacy = CopaState::new(clock.clone(), ProtocolHint::Bulk);
        let mut o1 = CopaState::new(clock.clone(), ProtocolHint::Bulk);
        o1.force_bw_o1();
        legacy.record_rtt(Duration::from_millis(10));
        o1.record_rtt(Duration::from_millis(10));

        // Deterministic LCG so the stream is reproducible.
        let mut lcg: u64 = 0x9E3779B97F4A7C15;
        let mut rnd = move || {
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (lcg >> 33) as u32
        };
        let mut seq = 0u64;
        for step in 0..4000u32 {
            let r = rnd();
            match r % 4 {
                // Per-symbol samples (the RWM_PLAIN_RS feed path).
                0 | 1 => {
                    legacy.rs_on_sent(seq, false);
                    o1.rs_on_sent(seq, false);
                    let extra = (r >> 8) % 50;
                    legacy.rs_delivered += extra as u64;
                    o1.rs_delivered += extra as u64;
                    clock.advance(Duration::from_millis(11 + (r >> 16) as u64 % 30));
                    legacy.rs_on_delivered(seq);
                    o1.rs_on_delivered(seq);
                    seq += 1;
                }
                // Per-ack samples (the legacy Copa feed path).
                2 => {
                    clock.advance(Duration::from_millis(2 + (r >> 8) as u64 % 20));
                    legacy.record_delivery(1 + r % 100);
                    o1.record_delivery(1 + r % 100);
                }
                // Occasional long gap (mass eviction) and an RTT sample that
                // moves min_rtt, hence the ≈10·RTprop window length.
                _ => {
                    if step % 37 == 0 {
                        clock.advance(Duration::from_millis(1500));
                    }
                    let rtt = Duration::from_millis(5 + (r % 120) as u64);
                    legacy.record_rtt(rtt);
                    o1.record_rtt(rtt);
                }
            }
            let fold_l = legacy.bw_fold();
            let fold_o = o1.bw_fold();
            assert_eq!(fold_l, fold_o, "identical streams, step {step}");
            assert_eq!(
                legacy.max_bw, fold_l,
                "legacy max_bw IS the fold, step {step}"
            );
            assert_eq!(
                o1.max_bw, fold_o,
                "O(1) max_bw equals the full-window fold, step {step}"
            );
            let front = o1.bw_mono.front().map_or(0.0, |s| s.delivery_rate);
            assert_eq!(front, fold_o, "mono front == fold, step {step}");
        }
        assert!(legacy.max_bw > 0.0, "the stream must have produced samples");
    }

    /// `RWM_HONEST_K` law + OFF-value property, at the engine feed site
    /// (`record_rtt`), on a jittered series: RTprop (min over RAW samples)
    /// reads the distribution FLOOR, the smoothed srtt reads near the MEAN
    /// — so the legacy K (windowed-min of the SMOOTHED series over the
    /// floor) reads HIGH by ≈ mean/floor, which is the measured jit25
    /// ×1.34-class inversion. The raw-fed tracker reads the floor ratio
    /// ≈ 1. Gate OFF ⇒ `k_raw_ratio()` is None (nothing is fed, nothing
    /// can consume it).
    #[test]
    fn k_raw_reads_the_jitter_floor_where_the_smoothed_min_reads_high() {
        let clock = Arc::new(MockClock::new());
        // OFF: byte-identical legacy — no ratio exists.
        let mut off = CopaState::new(clock.clone(), ProtocolHint::Bulk);
        off.record_rtt(Duration::from_millis(40));
        assert_eq!(off.k_raw_ratio(), None, "gate OFF ⇒ no raw K");

        // ON: the raw-fed windowed min under ±25 ms uniform jitter around a
        // 40 ms base (the jit25 shape).
        let mut on = CopaState::new(clock.clone(), ProtocolHint::Bulk);
        on.force_k_raw();
        // The net-side legacy tracker, fed the SMOOTHED series at the 5 ms
        // refresh clock — exactly the engine's shipped K feed.
        let mut legacy_k = crate::net::EchoRatioMin::new(crate::net::PERCAP_K_HALF_WINDOW_US);
        let mut lcg: u64 = 42;
        let mut uniform = move || {
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((lcg >> 33) as f64) / (u32::MAX as f64) // [0, 1)
        };
        let t0 = clock.now();
        for _ in 0..2000 {
            clock.advance(Duration::from_millis(5)); // ack + refresh cadence
            let jitter_ms = 50.0 * uniform() - 25.0; // ±25 ms
            let raw = Duration::from_secs_f64((0.040 + jitter_ms / 1e3).max(0.000_07));
            on.record_rtt(raw); // raw feed (the fix) + srtt/min_rtt as shipped
            // Shipped feed: smoothed srtt at the refresh clock.
            let now_us = clock.now().duration_since(t0).as_micros() as u64;
            legacy_k.observe_srtt_over_rtprop(on.srtt.unwrap(), on.min_rtt, now_us);
        }
        let k_raw = on.k_raw_ratio().expect("gate ON ⇒ raw K live");
        let k_legacy = legacy_k.k();
        // Direction + class, both ways (reproduce THEN remove): the smoothed
        // min sits near mean/floor ≈ 40/15 territory, far above 1; the raw
        // min reads the floor.
        assert!(
            k_legacy > 1.2,
            "the smoothed-series windowed min must read HIGH under wide jitter \
             (the jit25 inversion): k_legacy = {k_legacy}"
        );
        assert!(
            k_raw < 1.05,
            "the raw-fed windowed min must read the distribution floor: k_raw = {k_raw}"
        );
        assert!(
            k_legacy > k_raw * 1.2,
            "the bias must be the SMOOTHING's, removed by the raw feed: \
             legacy {k_legacy} vs raw {k_raw}"
        );
    }

    /// Formula agreement for the Copa SRTT estimator (`CopaState::record_rtt`)
    /// against RFC 6298 computed INDEPENDENTLY in the test — the estimator
    /// analogue of `tests/formula_agreement.rs` (pipeline-verification matrix
    /// row 13: the srtt was previously modelled as an oracle, never asserted).
    ///
    /// RFC 6298 §2.2/§2.3, srtt terms only (rttvar/RTO are not part of this
    /// estimator):
    ///
    ///   first measurement R:   SRTT ← R
    ///   subsequent R':         SRTT ← (1 − α)·SRTT + α·R',  α = 1/8
    ///
    /// Three absolute cases:
    ///   1. SEED — the first sample IS the srtt, exactly (and before any
    ///      sample there is no measured srtt at all).
    ///   2. STEADY — 7/8·s + 1/8·r over a known mixed sequence agrees with
    ///      the recursion computed here in f64, within Duration's
    ///      per-step nanosecond rounding (≪ 1 µs over the whole sequence).
    ///   3. FIXED POINT — a constant input is a fixed point (seeded at the
    ///      constant, the EWMA holds it BIT-EXACTLY: 7/8·c + 1/8·c has no
    ///      rounding at whole-millisecond c), and from a perturbed history
    ///      the estimator CONVERGES to the constant at the (7/8)^n rate.
    #[test]
    fn copa_srtt_agrees_with_rfc6298_ewma() {
        let clock = Arc::new(MockClock::new());
        let mut cs = CopaState::new(clock.clone(), ProtocolHint::Bulk);

        // Case 1 — seed: SRTT ← R on the first measurement, exactly.
        assert_eq!(cs.srtt, None, "no sample yet ⇒ no measured srtt");
        cs.record_rtt(millis(48));
        assert_eq!(
            cs.srtt,
            Some(millis(48)),
            "RFC 6298 §2.2: the first sample must BE the srtt, exactly"
        );

        // Case 2 — steady: SRTT ← 7/8·SRTT + 1/8·R over a known mixed
        // sequence (spikes both ways), against the independent f64 recursion.
        let seq_ms: [u64; 10] = [80, 40, 40, 120, 33, 47, 60, 5, 500, 48];
        let mut expect_s = 0.048_f64; // the seed above
        for &ms in &seq_ms {
            clock.advance(millis(5));
            cs.record_rtt(millis(ms));
            expect_s = 0.875 * expect_s + 0.125 * (ms as f64 / 1e3);
            let got_s = cs.srtt.expect("seeded above").as_secs_f64();
            assert!(
                (got_s - expect_s).abs() < 1e-6,
                "RFC 6298 recursion disagrees after the {ms} ms sample: \
                 engine {got_s} s vs independent formula {expect_s} s"
            );
        }

        // Case 3a — fixed point, exact: seeded at a constant, the EWMA must
        // return the constant bit-for-bit on every subsequent sample.
        let mut flat = CopaState::new(clock.clone(), ProtocolHint::Bulk);
        flat.record_rtt(millis(40));
        for _ in 0..32 {
            clock.advance(millis(5));
            flat.record_rtt(millis(40));
            assert_eq!(
                flat.srtt,
                Some(millis(40)),
                "a constant input must be an EXACT fixed point of the EWMA"
            );
        }

        // Case 3b — convergence: from the mixed history above, a constant
        // 40 ms input closes the gap as (7/8)^n. After n = 200 samples the
        // initial offset (< 1 s) is below 1 ns, so the srtt must sit within
        // 1 µs of the input — and agree with the f64 recursion throughout.
        for _ in 0..200 {
            clock.advance(millis(5));
            cs.record_rtt(millis(40));
            expect_s = 0.875 * expect_s + 0.125 * 0.040;
        }
        let got_s = cs.srtt.expect("seeded above").as_secs_f64();
        assert!(
            (got_s - expect_s).abs() < 1e-6,
            "recursion agreement must hold through convergence: \
             engine {got_s} s vs formula {expect_s} s"
        );
        assert!(
            (got_s - 0.040).abs() < 1e-6,
            "a constant input must converge to itself: srtt {got_s} s vs 40 ms"
        );
    }

    /// GOAL "HONEST INPUTS" phase 3 — PROBE 2: jit25's RTprop honesty under
    /// netem's clamped jitter, at component level, with the REAL estimator
    /// stack. Run explicitly:
    ///
    ///   cargo test --release -p raptorpath --lib -- --ignored --nocapture jit25_rtprop
    ///
    /// THE CELL'S ACTUAL CONFIG (tools/l1/adv_cells.sh `jit25`): each
    /// direction is `netem delay 20ms 25ms 25% rate 100mbit`, i.e. one-way
    /// delay = clamp(20 ms + x·25 ms, 0) + ~108 µs serialization (1350 B at
    /// 100 Mbit), x the 25%-autoregressively-correlated uniform variate on
    /// [−1, 1] (netem get_crandom's linear form x_n = ρ·x_{n−1} + (1−ρ)·u_n,
    /// identical in distribution class; approximation disclosed). An RTT
    /// sample sums two independent directions. NOTE the AR(ρ=0.25) marginal
    /// is NARROWER than uniform (sd ≈ 0.45 vs 0.58), so the clamp mass is
    /// ~4%/direction, not the naive 10% — the model computes it rather than
    /// assuming it.
    ///
    /// INSTRUMENT: `CopaState::record_rtt` on a MockClock — the shipped
    /// srtt EWMA (α = 1/8), the shipped 10 s min-window RTprop deque, and
    /// the `RWM_HONEST_K` raw-fed `EchoRatioMin` (forced on) — at swept
    /// RTT-sample cadences; plus direct windowed-min curves over the same
    /// series for the pre-registered window sweep.
    ///
    /// The three pre-registered curves ([P3-JIT] lines): (1) floor-sighting
    /// rate vs window length, (2) windowed-min RTprop vs the distribution's
    /// true floor, (3) the implied K elevation vs window.
    ///
    /// WHAT IT ADJUDICATES: under the UNLOADED jit25 distribution the clamp
    /// floor is NOT rare at dense cadences — every 10 s window re-sights the
    /// floor class, RTprop reads ≪ the 40 ms base, K_raw → 1, and the law's
    /// window term would COLLAPSE (the pre-registration's falsified-LOW
    /// branch, quantified). In the rare-floor regime (sparse cadence) the
    /// fingerprint inverts: RTprop keeps a once-seen deep min and the
    /// windowed K reads ≫ 1.5. The battery measured NEITHER collapse NOR
    /// K ≫ 1.5 (khr ≈ kraw ≈ 1.0–1.5 with an elevated limit): the in-cell
    /// series therefore rides a floor the window genuinely RE-ACHIEVES,
    /// far above the unloaded floor — the loaded link's standing queue.
    /// Real residence, not estimator bias and not floor rarity.
    #[test]
    #[ignore = "measurement: run explicitly with --release --ignored --nocapture"]
    fn jit25_rtprop_floor_sighting_under_netem_clamped_jitter() {
        const BASE_S: f64 = 0.020; // netem delay 20ms
        const JIT_S: f64 = 0.025; // ±25ms
        const RHO: f64 = 0.25; // 25% correlation
        const SER_S: f64 = 0.000_108; // 1350 B @ 100 Mbit, per direction
        const FLOOR_S: f64 = 2.0 * SER_S; // both jitter draws at the clamp
        const T_S: f64 = 30.0; // series length
        const RATE_SYM_S: f64 = 7_600.0; // the battery's measured arm-A class

        // One netem direction: AR-correlated uniform, clamped at 0.
        struct Dir {
            lcg: u64,
            x: f64,
        }
        impl Dir {
            fn new(seed: u64) -> Self {
                Self { lcg: seed, x: 0.0 }
            }
            fn next(&mut self) -> f64 {
                self.lcg = self
                    .lcg
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let u = ((self.lcg >> 33) as f64) / (u32::MAX as f64) * 2.0 - 1.0;
                self.x = RHO * self.x + (1.0 - RHO) * u;
                (BASE_S + self.x * JIT_S).max(0.0) + SER_S
            }
        }

        println!(
            "[P3-JIT] model: clamp(20ms ± 25ms AR(0.25) uniform, 0) + {:.0} µs/dir; \
             true two-way floor = {:.2} ms; base RTT = {:.1} ms; cell rate class {} sym/s",
            SER_S * 1e6,
            FLOOR_S * 1e3,
            2.0 * BASE_S * 1e3,
            RATE_SYM_S
        );

        for &cad in &[20.0f64, 100.0, 1000.0, 7400.0] {
            let clock = Arc::new(MockClock::new());
            let mut cs = CopaState::new(clock.clone(), ProtocolHint::Bulk);
            cs.force_k_raw();
            let (mut fwd, mut back) = (Dir::new(42), Dir::new(7));
            let step = Duration::from_secs_f64(1.0 / cad);
            let n = (T_S * cad) as usize;
            let mut series: Vec<f64> = Vec::with_capacity(n);
            for _ in 0..n {
                clock.advance(step);
                let rtt = fwd.next() + back.next();
                series.push(rtt);
                cs.record_rtt(Duration::from_secs_f64(rtt));
            }
            let rtprop = cs.min_rtt().unwrap().as_secs_f64();
            let srtt = cs.srtt.unwrap().as_secs_f64();
            let k_raw = cs.k_raw_ratio().unwrap();
            let global_min = series.iter().copied().fold(f64::INFINITY, f64::min);
            // Window sweep: non-overlapping windows of W seconds — mean
            // windowed min, floor-sighting fraction, implied K = mean
            // windowed-min ÷ global min (the elevation of a W-horizon floor
            // over the long-run floor).
            print!(
                "[P3-JIT] cadence {cad:>6.0}/s: RTprop(10s win) {:.2} ms, srtt {:.1} ms, \
                 srtt/RTprop {:.1}, K_raw {k_raw:.2}, global min {:.2} ms | implied \
                 3T window term {:.0} sym (cell measured 396–714)\n",
                rtprop * 1e3,
                srtt * 1e3,
                srtt / rtprop,
                global_min * 1e3,
                RATE_SYM_S * k_raw.max(1.0) * rtprop,
            );
            let mut prev_mean = f64::INFINITY;
            for &w_s in &[0.5f64, 1.0, 2.0, 5.0, 10.0] {
                let wlen = ((w_s * cad) as usize).max(1);
                let mut mins = Vec::new();
                let mut sighted = 0usize;
                for chunk in series.chunks(wlen) {
                    if chunk.len() < wlen {
                        break;
                    }
                    let m = chunk.iter().copied().fold(f64::INFINITY, f64::min);
                    if m <= FLOOR_S + 0.001 {
                        sighted += 1;
                    }
                    mins.push(m);
                }
                let mean = mins.iter().sum::<f64>() / mins.len() as f64;
                println!(
                    "[P3-JIT]   window {w_s:>4.1} s: mean windowed-min {:.2} ms, \
                     floor-sighting {:.0}% of windows, implied K(w) {:.2}",
                    mean * 1e3,
                    100.0 * sighted as f64 / mins.len() as f64,
                    mean / global_min,
                );
                // A longer horizon can only read LOWER (min over a superset)
                // — "a derivable better floor" does not exist in the upward
                // direction the elevated limit would need.
                assert!(
                    mean <= prev_mean * 1.02,
                    "windowed min must be non-increasing in the horizon"
                );
                prev_mean = mean;
            }
            // The dense regime (any cadence ≥ ~1000/s): the floor is NOT
            // rare — RTprop reads the floor class, K_raw reads ≈ 1, and the
            // law's window term collapses to the sym class (the
            // falsified-LOW branch the cell did NOT show).
            if cad >= 1000.0 {
                assert!(
                    rtprop < 0.005,
                    "dense sampling must sight the clamp floor: RTprop {rtprop}"
                );
                assert!(
                    k_raw < 1.5,
                    "dense sampling re-achieves the floor in-window: K_raw {k_raw}"
                );
                assert!(
                    srtt / rtprop > 8.0,
                    "the unloaded distribution's srtt/RTprop is an order above \
                     the cell's measured 1.0–1.5 class: {}",
                    srtt / rtprop
                );
            }
        }
        println!(
            "[P3-JIT] adjudication: the cell measured khr ≈ kraw ≈ 1.0–1.5 WITH an \
             elevated limit (window/rate ≈ 50–90 ms) — neither the dense-floor collapse \
             nor the rare-floor K ≫ 1.5 fingerprint. The in-cell series rides a \
             re-achieved floor far above the unloaded clamp floor: the standing queue \
             of the loaded 100 Mbit link. RTprop is honest w.r.t. its own window; the \
             elevation is real residence."
        );
    }

    /// The `RWM_HONEST_ANCHOR` cost curve, in one process (MEASUREMENT
    /// DISCIPLINE 14 — the component instrument for the c1 −35%): the
    /// legacy per-sample full-window fold's cost per delivered symbol GROWS
    /// with the symbol rate (window holds ≈ rate × 1 s samples ⇒ O(rate²)
    /// per second — the measured rate-dependence: D/A 1.00 at 5–9.9 k,
    /// 0.88 at 19 k, 0.64 at 24 k), while the mono-deque read is flat.
    /// `#[ignore]`d: it is a measurement with wall-clock timing; run
    ///   cargo test --release -p raptorpath --lib -- --ignored --nocapture bw_filter_cost
    #[test]
    #[ignore = "measurement: run explicitly with --release --ignored --nocapture"]
    fn bw_filter_cost_is_quadratic_legacy_and_linear_fixed() {
        fn per_delivery_ns(rate_sym_s: u64, o1: bool) -> f64 {
            let clock = Arc::new(MockClock::new());
            let mut copa = CopaState::new(clock.clone(), ProtocolHint::Bulk);
            if o1 {
                copa.force_bw_o1();
            }
            copa.record_rtt(Duration::from_millis(10)); // RTprop 10 ms ⇒ window 1 s
            let step = Duration::from_nanos(1_000_000_000 / rate_sym_s);
            let lag = (rate_sym_s / 50) as u64; // ≈20 ms of in-flight seqs
            let mut send_seq = 0u64;
            // Warm: fill one full window so the deque is at steady state.
            let warm = rate_sym_s * 12 / 10;
            for _ in 0..warm {
                copa.rs_on_sent(send_seq, false);
                if send_seq >= lag {
                    copa.rs_on_delivered(send_seq - lag);
                }
                clock.advance(step);
                send_seq += 1;
            }
            assert!(
                copa.bw_samples.len() as u64 > rate_sym_s / 2,
                "window must be rate-sized: {} at {rate_sym_s}",
                copa.bw_samples.len()
            );
            // Measure N deliveries (with their sends) at steady state.
            let n = 100_000u64;
            let t = std::time::Instant::now();
            for _ in 0..n {
                copa.rs_on_sent(send_seq, false);
                copa.rs_on_delivered(send_seq - lag);
                clock.advance(step);
                send_seq += 1;
            }
            let ns = t.elapsed().as_nanos() as f64 / n as f64;
            println!(
                "[HONEST-BENCH] rate={rate_sym_s} o1={o1} window_samples={} \
                 per_delivery={ns:.0} ns  (per-second-of-transfer cost: {:.0} ms)",
                copa.bw_samples.len(),
                ns * rate_sym_s as f64 / 1e6,
            );
            ns
        }
        // The battery's parity band (9.6 k) and c1's rate class (24 k).
        let legacy_lo = per_delivery_ns(9_600, false);
        let legacy_hi = per_delivery_ns(24_000, false);
        let o1_lo = per_delivery_ns(9_600, true);
        let o1_hi = per_delivery_ns(24_000, true);
        println!(
            "[HONEST-BENCH] legacy 24k/9.6k = {:.2} (rate-dependence), \
             o1 24k = {:.3} of legacy 24k (the removal)",
            legacy_hi / legacy_lo,
            o1_hi / legacy_hi,
        );
        assert!(
            legacy_hi > 1.8 * legacy_lo,
            "the legacy fold's per-delivery cost must GROW with rate \
             (the measured rate-dependent tax): {legacy_lo:.0} → {legacy_hi:.0} ns"
        );
        assert!(
            o1_hi < 0.25 * legacy_hi,
            "the O(1) read must remove the dominant cost at c1's rate: \
             o1 {o1_hi:.0} vs legacy {legacy_hi:.0} ns"
        );
        assert!(
            o1_hi < 3.0 * o1_lo.max(50.0),
            "the O(1) read must be rate-flat: {o1_lo:.0} → {o1_hi:.0} ns"
        );
    }

    // ----- Pool-anchor honest dual-store law (RWM_POOL_ANCHOR, goal-gate -----
    // ----- "Ship The Wins 1") ------------------------------------------------

    /// THE burst-immunity + anchor-consumer-separation law (the §16.35 c7
    /// blocker, at the unit level): a steady send process with the acks
    /// arriving in est-cadence-class BURSTS must (a) drive the LEGACY
    /// ack-interval windowed-max (`record_delivery` via `on_ack` — the Copa
    /// cwnd feed, deliberately UNCHANGED) to a burst-peak over-read, while
    /// (b) the pool-anchor send-interval rate — the N ≥ 2 store-cap law's
    /// input — keeps reading ≈ the true send rate. One PathState, both
    /// consumers, same clock: the separation IS the fix.
    #[test]
    fn pool_anchor_send_rate_is_burst_immune_while_the_copa_feed_over_reads() {
        let clock = Arc::new(MockClock::new());
        let mut path = PathState::new(0, clock.clone());
        path.force_pool_anchor_feed(true);
        path.record_rtt_sample(millis(10)); // srtt/RTprop warm

        // Steady send process: 1 symbol/ms ≈ 1000 sym/s for 2 s, fed at the
        // real feed site (charge_in_flight = every wire send on this path).
        for _ in 0..2000 {
            path.charge_in_flight(1);
            path.release_in_flight(1);
            clock.advance(millis(1));
        }
        let sr = path
            .send_rate_anchor()
            .expect("send anchor warm within the window");
        assert!(
            (sr - 1000.0).abs() / 1000.0 < 0.25,
            "send-interval anchor reads ≈ truth (1000 sym/s), got {sr}"
        );

        // est-cadence ack clock: every ~100 ms a tight burst — a 1-ms-spaced
        // clump whose Δdelivered/Δt spikes ~200× the true rate. The legacy
        // windowed-MAX latches the spike (the measured ×3.4–3.7 further
        // over-read channel); the send anchor must not move. AND the send
        // side bursts too (the amendment's measured defect): each ack burst
        // frees store slots and the admission-gated sender REFILLS at
        // emission speed — a ~5 ms bucket at ~40k sym/s. The windowed-max
        // latches that refill burst (sr=53k-vs-8.9k smoke); the MEAN the
        // law reads must stay ≈ the true carried rate.
        for _ in 0..10 {
            // Steady send process between ack bursts.
            for _ in 0..93 {
                path.charge_in_flight(1);
                path.release_in_flight(1);
                clock.advance(millis(1));
            }
            path.on_ack(1); // re-arm last_delivered_time
            clock.advance(millis(2));
            path.on_ack(400); // ack burst peak: 400 / 2 ms = 200k sym/s
            // Store-refill send burst: 200 symbols in ~5 ms (~40k sym/s).
            for _ in 0..200 {
                path.charge_in_flight(1);
                path.release_in_flight(1);
                clock.advance(Duration::from_micros(25));
            }
        }
        let btlbw = path
            .btlbw_sym_per_s()
            .expect("legacy anchor established (the cwnd feed still runs)");
        assert!(
            btlbw > 10.0 * 1000.0,
            "the LEGACY ack-interval max must show the burst-peak over-read \
             (the unchanged Copa-feed channel), got {btlbw}"
        );
        // Carried truth over the burst phase: (93 + 200) sends per 100 ms
        // cycle ≈ 2 930 sym/s — the mean must read it; the 40k refill peaks
        // and the 200k ack peaks must both be invisible to it.
        let truth = (93.0 + 200.0) / 0.1;
        let sr_after = path.send_rate_anchor().expect("anchor still live");
        assert!(
            (sr_after - truth).abs() / truth < 0.35,
            "the pool anchor must be burst-immune: got {sr_after} vs carried truth {truth}"
        );
        // And the honest pool term derived from it stays in the truth class
        // while the legacy term reads the spike: the store-cap consumer is
        // the one being fixed, the cwnd consumer the one left alone.
        let rtp = path.min_rtt().unwrap().as_secs_f64();
        let honest = crate::net::honest_store_cap(Some(sr_after * rtp), Some(sr_after), 1.0, 2.0)
            .unwrap();
        let legacy_bdp = path.copa_bdp_anchor().unwrap();
        assert!(
            legacy_bdp > 2.0 * (sr_after * rtp),
            "legacy BDP anchor carries the over-read: {legacy_bdp} vs honest pipe {}",
            sr_after * rtp
        );
        assert!(
            honest < 2.0 * (sr_after * rtp + sr_after * crate::net::HONEST_RECOVERY_ROUND_S),
            "honest cap term stays residence+runway-bounded, got {honest}"
        );
    }

    /// `RWM_POOL_ANCHOR=0` (the est-only decomposition arm) and N = 1 cost
    /// honesty: with the feed off, `charge_in_flight` does no anchor work
    /// and the anchor reads None — the prior-default path.
    #[test]
    fn pool_anchor_feed_off_is_inert() {
        let clock = Arc::new(MockClock::new());
        let mut path = PathState::new(0, clock.clone());
        path.force_pool_anchor_feed(false);
        path.record_rtt_sample(millis(10));
        for _ in 0..100 {
            path.charge_in_flight(1);
            clock.advance(millis(1));
        }
        assert_eq!(path.in_flight, 100, "in-flight accounting unchanged");
        assert!(
            path.send_rate_anchor().is_none(),
            "feed off ⇒ no send-anchor samples (byte-identical prior path)"
        );
    }

    // ----- Delivery-clocked pool anchor (RWM_POOL_DELIV, goal-gate ----------
    // ----- "Ship The Wins 1b" arm A) ----------------------------------------

    /// THE arm-A law at the PathState level: with the delivery feed on, the
    /// pool law's rate input (`pool_rate_anchor`) reads the BOTTLENECK a
    /// cap-limited sender's own send mean cannot see — while every cwnd-side
    /// consumer is byte-identical to the arm-1 path. That is attempt 2's
    /// whole claim, wired: the sampler is a SHADOW, and it ratchets.
    #[test]
    fn pool_deliv_rate_ratchets_above_the_send_mean_and_touches_no_cwnd_consumer() {
        let clock = Arc::new(MockClock::new());
        let mut path = PathState::new(0, clock.clone());
        path.force_pool_anchor_feed(true);
        path.force_pool_deliv_feed(true);
        path.record_rtt_sample(millis(10)); // RTprop/SRTT warm

        // An admission-gated sender: 400 symbols emitted (and carried) in a
        // 20 ms burst at ≈20 000 sym/s, then 80 ms idle — long-run mean
        // 4 000 sym/s. This is the measured c7 shape (store refill on SACK
        // release), at unit scale.
        for _ in 0..25 {
            for _ in 0..4 {
                path.charge_in_flight(100);
                clock.advance(millis(5));
                path.on_pool_delivery(100, 0, false);
                path.release_in_flight(100);
            }
            clock.advance(millis(80));
        }
        let sr = path.send_rate_anchor().expect("send anchor warm");
        let dr = path.deliv_rate_anchor().expect("delivery anchor live");
        let pool = path.pool_rate_anchor().expect("pool rate live");
        let mean = 400.0 / 0.1; // 4 000 sym/s carried mean
        assert!(
            sr < mean * 2.0,
            "the SEND term reads the cap-limited mean (attempt 1's binder): {sr}"
        );
        assert!(
            dr > sr * 1.5,
            "THE arm-A claim: the DELIVERY term ratchets above it — dr={dr} sr={sr}"
        );
        assert_eq!(
            pool,
            sr.max(dr),
            "the law reads max(deliv, send) — ONE formula, no branch"
        );
        // SHADOW: no cwnd-side consumer may have moved. The delivery feed
        // never calls record_delivery/on_ack, so the legacy anchor has no
        // samples at all, cwnd is untouched, and src_inflight is zero
        // (falsification-5: no scoped feed may leak it).
        assert!(
            path.btlbw_sym_per_s().is_none(),
            "the delivery feed must NOT feed the legacy/Copa max_bw filter"
        );
        assert!(
            path.copa_bdp_anchor().is_none(),
            "…nor the BDP anchor the cwnd floor rides"
        );
        assert_eq!(
            path.cwnd,
            PathState::INITIAL_CWND,
            "…nor cwnd itself (no delivery signal, no dynamics)"
        );
        assert_eq!(path.src_inflight, 0, "…nor src_inflight (falsification-5)");
    }

    /// `RWM_POOL_DELIV=0` is attempt 1 EXACTLY: no delivery work at either
    /// feed site, and `pool_rate_anchor()` is byte-identical to
    /// `send_rate_anchor()` (the arms are one knob apart — cost-honest A/B).
    #[test]
    fn pool_deliv_feed_off_is_inert_and_equals_attempt_one() {
        let clock = Arc::new(MockClock::new());
        let mut path = PathState::new(0, clock.clone());
        path.force_pool_anchor_feed(true);
        path.force_pool_deliv_feed(false);
        path.record_rtt_sample(millis(10));
        for _ in 0..400 {
            path.charge_in_flight(10);
            clock.advance(millis(1));
            path.on_pool_delivery(10, 0, false);
            path.release_in_flight(10);
        }
        assert!(
            path.deliv_rate_anchor().is_none(),
            "feed off ⇒ no delivery samples exist"
        );
        assert_eq!(
            path.pool_rate_anchor(),
            path.send_rate_anchor(),
            "the pool law reads exactly attempt 1's anchor with the gate off"
        );
        let (ok, short, gaps, disc) = path.deliv_anchor_stats();
        assert_eq!((ok, short, gaps, disc), (0, 0, 0, 0), "no sampler work at all");
    }

    /// A quarantined (stall-poisoned) ack must not reach the delivery
    /// sampler — the same hygiene verdict the RTT/rate feeds beside it obey
    /// (ADR-0061 / `RWM_CLOCK_GAP`).
    #[test]
    fn pool_deliv_drops_quarantined_delivery_events() {
        let clock = Arc::new(MockClock::new());
        let mut path = PathState::new(0, clock.clone());
        path.force_pool_anchor_feed(true);
        path.force_pool_deliv_feed(true);
        path.record_rtt_sample(millis(10));
        for _ in 0..50 {
            path.charge_in_flight(100);
            clock.advance(millis(5));
            path.on_pool_delivery(100, 0, true); // quarantined at the ack site
        }
        assert!(
            path.deliv_rate_anchor().is_none(),
            "quarantined events must produce no samples"
        );
        let (ok, ..) = path.deliv_anchor_stats();
        assert_eq!(ok, 0, "…and no accepted sample");
    }

    // ----- Honest anchor-floor bound (RWM_FLOOR_BOUND, arm B) ---------------

    /// THE arm-B law: an ack-interval over-read inflates the BtlBw anchor
    /// floor (the measured cwnd 5860 vs 1779); the bound must cut the floor
    /// to the honest send-rate pipe — and must stay a FLOOR (cwnd is never
    /// lowered below where the dynamics put it) and legacy-verbatim while
    /// the send anchor is cold.
    #[test]
    fn floor_bound_cuts_the_over_read_floor_but_stays_a_floor() {
        let clock = Arc::new(MockClock::new());
        // Baseline (bound OFF): the over-read floor ratchets cwnd up.
        let mut a = PathState::new(0, clock.clone());
        a.force_pool_anchor_feed(true);
        a.force_floor_bound(false);
        let mut b = PathState::new(0, clock.clone());
        b.force_pool_anchor_feed(true);
        b.force_floor_bound(true);
        for p in [&mut a, &mut b] {
            p.record_rtt_sample(millis(10));
        }
        // Steady honest send process ≈1000 sym/s for 2 s on both.
        for _ in 0..2000 {
            for p in [&mut a, &mut b] {
                p.charge_in_flight(1);
                p.release_in_flight(1);
            }
            clock.advance(millis(1));
        }
        // …and an ack-BURST clock that over-reads the legacy anchor ×100,
        // while the honest send process keeps running underneath it (that is
        // the measured c7 shape: the sender is steady, the ACK CLOCK bunches).
        for _ in 0..20 {
            for _ in 0..98 {
                for p in [&mut a, &mut b] {
                    p.charge_in_flight(1);
                    p.release_in_flight(1);
                }
                clock.advance(millis(1));
            }
            for p in [&mut a, &mut b] {
                p.on_ack(1);
            }
            clock.advance(millis(2));
            for p in [&mut a, &mut b] {
                p.on_ack(400); // 400 / 2 ms = 200k sym/s
            }
        }
        let legacy_bdp = a.copa_bdp_anchor().expect("legacy anchor established");
        let sr = b.send_rate_anchor().expect("send anchor warm");
        let rtp = b.min_rtt().unwrap().as_secs_f64();
        assert!(
            legacy_bdp > 10.0 * sr * rtp,
            "the over-read must be present to bound: legacy={legacy_bdp} honest={}",
            sr * rtp
        );
        assert!(
            b.cwnd < a.cwnd,
            "the bound must cut the inflated floor: bounded={} unbounded={}",
            b.cwnd,
            a.cwnd
        );
        assert!(
            b.cwnd >= PathState::MIN_CWND,
            "…and never below the hard floor: {}",
            b.cwnd
        );
        // Still a FLOOR: with the send anchor COLD the bound is the legacy
        // value verbatim (no path may be throttled by an absent measurement).
        let mut c = PathState::new(0, clock.clone());
        c.force_pool_anchor_feed(false); // no send anchor ⇒ cold
        c.force_floor_bound(true);
        let mut d = PathState::new(0, clock.clone());
        d.force_pool_anchor_feed(false);
        d.force_floor_bound(false);
        for p in [&mut c, &mut d] {
            p.record_rtt_sample(millis(10));
        }
        for _ in 0..20 {
            for p in [&mut c, &mut d] {
                p.on_ack(1);
            }
            clock.advance(millis(2));
            for p in [&mut c, &mut d] {
                p.on_ack(400);
            }
            clock.advance(millis(98));
        }
        assert_eq!(
            c.cwnd, d.cwnd,
            "cold send anchor ⇒ the bound is the legacy floor verbatim"
        );
    }

    // ----- Wire-clocked Copa signal + hint→δ mapping (feat/copa-wire-signal) -----

    #[test]
    fn copa_wire_gate_from_env() {
        // Default ON exactly when the engine owns/feeds the substrate window.
        assert!(copa_wire_from_env(Some("passthrough"), false, None));
        assert!(copa_wire_from_env(Some(" Passthrough "), false, None));
        assert!(copa_wire_from_env(None, true, None)); // RWM_COPA_FEED=1 A/B
        // Shipped default: everything unset ⇒ OFF (byte-identical).
        assert!(!copa_wire_from_env(None, false, None));
        assert!(!copa_wire_from_env(Some("bbr"), false, None));
        assert!(!copa_wire_from_env(Some("cubic"), false, None));
        // RWM_COPA_WIRE=0 reproduces the #80 app-echo arm even under passthrough.
        assert!(!copa_wire_from_env(Some("passthrough"), false, Some("0")));
        assert!(!copa_wire_from_env(Some("passthrough"), true, Some("false")));
        // RWM_COPA_WIRE=1 forces on (e.g. RWM_COPA_FEED-less diagnostics).
        assert!(copa_wire_from_env(None, false, Some("1")));
    }

    #[test]
    fn copa_delta_hint_mapping() {
        // δ(hint) = COPA_DELTA / ζ(hint): the hint's ONE declared price
        // ratio (tail_loss_scale ζ = 0.01/1/100) is the latency price, δ
        // (paper §12.4). No constants beyond the Copa-paper δ=0.5 anchor.
        assert_eq!(copa_delta(ProtocolHint::Auto, None), COPA_DELTA);
        assert_eq!(copa_delta(ProtocolHint::Bulk, None), COPA_DELTA / 100.0);
        assert_eq!(copa_delta(ProtocolHint::Realtime, None), COPA_DELTA * 100.0);
        // Equilibrium queue = 1/δ packets: Bulk 200, Auto 2, Realtime 0.02.
        assert_eq!(1.0 / copa_delta(ProtocolHint::Bulk, None), 200.0);
        // The RWM_COPA_DELTA frontier knob overrides the hint; garbage is ignored.
        assert_eq!(copa_delta(ProtocolHint::Bulk, Some(0.05)), 0.05);
        assert_eq!(copa_delta(ProtocolHint::Bulk, Some(-1.0)), COPA_DELTA / 100.0);
        assert_eq!(
            copa_delta(ProtocolHint::Bulk, Some(f64::NAN)),
            COPA_DELTA / 100.0
        );
    }

    #[test]
    fn wire_dq_keys_on_wire_clock_not_app_echo() {
        // The #80 named mechanism: the app-layer echo RTT includes the
        // sender's OWN store/reservoir dwell, so Copa backed off against
        // self-inflicted delay. Under the wire signal the CC delay term
        // comes ONLY from record_rtt_sample (the packet-timed wire feed);
        // the estimator's app-echo RTT — dwell included — must have zero
        // influence on the cwnd dynamics.
        let clock = Arc::new(MockClock::new());
        let mut path = PathState::new(0, clock.clone());
        path.force_wire_for_test(COPA_DELTA);

        // App echo reads a huge 500 ms (store dwell); the wire reads a clean
        // 10 ms floor. Copa must RAMP — the dwell is not network queue.
        let mut prev = path.cwnd;
        for _ in 0..5 {
            path.estimator.record_rtt(millis(500)); // app echo incl. dwell
            path.record_rtt_sample(millis(10)); // wire clock
            clock.advance(millis(15));
            path.on_ack(prev);
            let cur = path.cwnd;
            assert!(
                cur > prev,
                "wire-clocked Copa must grow through app-layer dwell: {prev}->{cur}"
            );
            prev = cur;
        }

        // Inverse direction: the wire clock now shows a REAL standing queue
        // (60 ms over the 10 ms floor) while the app echo is quiet — Copa
        // must back off on the wire evidence alone.
        for _ in 0..4 {
            path.estimator.record_rtt(millis(10));
            path.record_rtt_sample(millis(60));
        }
        clock.advance(millis(80));
        let pre = path.cwnd;
        path.on_ack(4);
        assert!(
            path.cwnd < pre,
            "a wire-clock queue must back cwnd off: {pre}->{}",
            path.cwnd
        );
    }

    #[test]
    fn wire_velocity_law_doubles_step_and_caps_drain() {
        // Copa's actual update law (paper §12.4 wire addendum): step v/δ per
        // SRTT, v doubling while the direction persists. δ = 0.005 (Bulk) ⇒
        // base step 200 symbols — the small-δ/high-BDP exploitation the +2
        // additive probe could never provide.
        let clock = Arc::new(MockClock::new());
        let mut path = PathState::new(0, clock.clone());
        path.force_wire_for_test(0.005);

        // Drive the PURE law via on_delivery_signal (no delivery samples ⇒
        // no BtlBw anchor ⇒ the coupling cap stays out of the picture —
        // covered by its own test below). Ramp on a clean 10 ms floor, then
        // exit the ramp with a moderate standing queue (40 ms — well above
        // the jitter the square transition charges into the headroom
        // estimators).
        for _ in 0..10 {
            path.record_rtt_sample(millis(10));
            clock.advance(millis(15));
            path.on_delivery_signal();
        }
        assert!(path.cwnd > 100, "ramp must have grown: {}", path.cwnd);
        for _ in 0..4 {
            path.record_rtt_sample(millis(40)); // rate ≫ 1/(δ·dq) at this cwnd
        }
        clock.advance(millis(60));
        path.on_delivery_signal();
        assert!(!path.in_slow_start, "queue evidence must end the ramp");

        // Steady state, clean floor again: base up-step is 1/δ = 200; the
        // velocity doubles only after the direction has persisted ≥ 3
        // updates (Copa §2.2 hysteresis — bounds the overshoot).
        let c0 = path.cwnd;
        let mut cs = vec![c0];
        for _ in 0..3 {
            path.record_rtt_sample(millis(10));
            clock.advance(millis(400)); // > srtt (spiked EWMA) → update due
            path.on_delivery_signal();
            cs.push(path.cwnd);
        }
        let step1 = cs[1] - cs[0];
        let step2 = cs[2] - cs[1];
        let step3 = cs[3] - cs[2];
        assert!(
            step1 >= 190,
            "bulk-δ base step must be ~1/δ = 200: {cs:?}"
        );
        assert!(
            step2 <= step1 + 2,
            "velocity must NOT double before the 3-update streak: {cs:?}"
        );
        assert!(
            step3 >= 2 * step1 - 2,
            "3-update persistent direction must double the velocity: {cs:?}"
        );

        // Down direction: one v/δ step down (velocity resets on the flip),
        // never a collapse.
        let pre = path.cwnd;
        for _ in 0..4 {
            path.record_rtt_sample(millis(60)); // real queue: dq ≈ 50 ms
        }
        clock.advance(millis(400));
        path.on_delivery_signal();
        let post = path.cwnd;
        assert!(post < pre, "above-target must step down: {pre}->{post}");
        assert!(
            post + 210 >= pre,
            "a single down move is one v/δ step: {pre}->{post}"
        );
    }

    #[test]
    fn wire_coupling_cap_bounds_cwnd_at_bdp_plus_two_over_delta() {
        // Once cwnd exceeds the sender's outstanding store, the delay signal
        // is decoupled and a jitter-clamped d_q votes "up" forever (measured
        // v1/v2 ratchet to MAX_CWND). The coupling cap bounds cwnd at the
        // Copa fixed point plus one dither amplitude: BDP + 2/δ.
        let clock = Arc::new(MockClock::new());
        let mut path = PathState::new(0, clock.clone());
        path.force_wire_for_test(0.005);
        // 25 clean-floor updates with a live delivery rate: μ̂ ≈ 50/15 ms ≈
        // 3 333 sym/s, RTprop 10 ms ⇒ BDP ≈ 33; cap ≈ 33 + 400 = 433.
        for _ in 0..25 {
            path.record_rtt_sample(millis(10));
            clock.advance(millis(15));
            path.on_ack(50);
        }
        let bdp = path.copa_bdp_anchor().expect("anchor must be warm");
        let cap = bdp + 2.0 / 0.005;
        assert!(
            (path.cwnd as f64) <= cap + 1.0,
            "cwnd must stay coupled: cwnd={} cap={cap:.0} (bdp={bdp:.0})",
            path.cwnd
        );
        assert!(
            path.cwnd > PathState::MIN_CWND,
            "the cap must not collapse the window: {}",
            path.cwnd
        );
    }

    // --- Copa §2.2 TCP-competitive mode (feat/copa-compete) -----------------

    /// Establish a clean 10 ms wire floor and exit the ramp so the per-SRTT
    /// velocity law (and with it `compete_update`) is live.
    fn compete_warmup(path: &mut PathState, clock: &Arc<MockClock>) {
        for _ in 0..10 {
            path.record_rtt_sample(millis(10));
            clock.advance(millis(15));
            path.on_delivery_signal();
        }
        // Ramp exit on first standing-queue evidence.
        for _ in 0..4 {
            path.record_rtt_sample(millis(60));
        }
        clock.advance(millis(70));
        path.on_delivery_signal();
        assert!(!path.in_slow_start, "warmup must end the ramp");
    }

    /// One per-SRTT update under a NEVER-draining standing queue (60 ms over
    /// the 10 ms floor — a buffer-filling competitor's signature).
    fn queue_update(path: &mut PathState, clock: &Arc<MockClock>) {
        for _ in 0..4 {
            path.record_rtt_sample(millis(60));
        }
        clock.advance(millis(70));
        path.on_delivery_signal();
    }

    #[test]
    fn compete_detection_fires_under_never_draining_queue() {
        // Copa §2.2: no "nearly empty" queue (d_q < 0.1·(RTTmax−RTTmin)) in
        // the last 5 RTTs ⇒ competitive mode; the AIMD then grows 1/δ past
        // the hint base.
        let clock = Arc::new(MockClock::new());
        let mut path = PathState::new(0, clock.clone());
        path.force_wire_for_test(0.005);
        path.force_compete_for_test();
        compete_warmup(&mut path, &clock);
        for _ in 0..40 {
            queue_update(&mut path, &clock);
        }
        let (on, in_compete, switches, delta, base) = path.copa_compete_diag();
        assert!(on, "gate must be on (forced)");
        assert!(in_compete, "a never-draining queue must switch to competitive mode");
        assert!(switches >= 1, "the entry must be counted");
        assert!(
            delta < base,
            "the loss-free AIMD must have grown 1/δ past the base: δ={delta} base={base}"
        );
        assert!(delta <= base, "invariant: δ ≤ δ_base in competitive mode");
    }

    #[test]
    fn compete_detection_quiet_under_draining_queue() {
        // The queue drains to ~the floor every 3rd update (≤ 5 RTTs apart):
        // Copa's own dynamics look like this — mode switching must NOT fire.
        let clock = Arc::new(MockClock::new());
        let mut path = PathState::new(0, clock.clone());
        path.force_wire_for_test(0.005);
        path.force_compete_for_test();
        compete_warmup(&mut path, &clock);
        for _ in 0..30 {
            queue_update(&mut path, &clock);
            queue_update(&mut path, &clock);
            // The drain trough: samples back at the floor mark nearly-empty.
            for _ in 0..4 {
                path.record_rtt_sample(millis(11));
            }
            clock.advance(millis(70));
            path.on_delivery_signal();
        }
        let (_, in_compete, switches, delta, base) = path.copa_compete_diag();
        assert!(
            !in_compete && switches == 0,
            "a regularly-draining queue must stay in default mode (switches={switches})"
        );
        assert_eq!(delta, base, "default mode keeps the hint-mapped base δ");
    }

    #[test]
    fn compete_delta_follows_aimd_on_inverse_delta() {
        // The verified law (Copa §2.2): AIMD on 1/δ — +1 per RTT without
        // loss, halve on loss, floored at the default-mode δ (δ ≤ δ_base).
        // Base δ = 0.5 (the paper's default) makes the arithmetic direct:
        // 1/δ: 2 → 3 → (loss: max(1.5, 2) = 2) → 3 → 4 → (loss) → 2.
        let clock = Arc::new(MockClock::new());
        let mut path = PathState::new(0, clock.clone());
        path.force_wire_for_test(0.5);
        path.force_compete_for_test();
        compete_warmup(&mut path, &clock);
        // Drive updates until the detector enters competitive mode.
        let mut entered = false;
        for _ in 0..20 {
            queue_update(&mut path, &clock);
            if path.copa_compete_diag().1 {
                entered = true;
                break;
            }
        }
        assert!(entered, "never-draining queue must enter competitive mode");
        // Additive increase from the entry base: 1/δ 2 → 3.
        queue_update(&mut path, &clock);
        let d = path.copa_compete_diag().3;
        assert!((d - 1.0 / 3.0).abs() < 1e-12, "AI must be 1/δ += 1: δ={d}");
        // Loss (shim congestion-event counter advanced): 1/δ halves, floored
        // at 1/δ_base — max(3/2, 2) = 2 ⇒ δ back to the base.
        path.on_wire_congestion_events(1);
        queue_update(&mut path, &clock);
        let d = path.copa_compete_diag().3;
        assert!((d - 0.5).abs() < 1e-12, "MD must floor at δ_base: δ={d}");
        // Two clean updates: 2 → 3 → 4.
        queue_update(&mut path, &clock);
        queue_update(&mut path, &clock);
        let d = path.copa_compete_diag().3;
        assert!((d - 0.25).abs() < 1e-12, "AI must continue: δ={d}");
        // A STALE counter read (no advance) is NOT a loss.
        path.on_wire_congestion_events(1);
        queue_update(&mut path, &clock);
        let d = path.copa_compete_diag().3;
        assert!((d - 0.2).abs() < 1e-12, "no counter advance ⇒ no MD: δ={d}");
        // Loss again: max(5/2, 2) = 2.5 ⇒ δ = 0.4 (a real halving above the
        // floor this time).
        path.on_wire_congestion_events(2);
        queue_update(&mut path, &clock);
        let d = path.copa_compete_diag().3;
        assert!((d - 0.4).abs() < 1e-12, "MD must halve 1/δ: δ={d}");
        let (_, _, _, delta, base) = path.copa_compete_diag();
        assert!(delta <= base, "invariant: δ ≤ δ_base throughout");
    }

    #[test]
    fn compete_switches_back_on_drain_and_resets_delta() {
        // Copa §2.2: "When Copa switches from competitive mode to default
        // mode, it resets δ" to the default-mode value (the hint base here).
        let clock = Arc::new(MockClock::new());
        let mut path = PathState::new(0, clock.clone());
        path.force_wire_for_test(0.005);
        path.force_compete_for_test();
        compete_warmup(&mut path, &clock);
        for _ in 0..12 {
            queue_update(&mut path, &clock);
        }
        let (_, in_compete, _, delta, base) = path.copa_compete_diag();
        assert!(in_compete && delta < base, "precondition: competitive, δ adapted");
        // The competitor leaves: the queue drains to the floor.
        for _ in 0..4 {
            path.record_rtt_sample(millis(11));
        }
        clock.advance(millis(70));
        path.on_delivery_signal();
        let (_, in_compete, _, delta, base) = path.copa_compete_diag();
        assert!(!in_compete, "a nearly-empty queue within 5 RTTs must switch back");
        assert_eq!(delta, base, "switch-back must reset δ to the base");
    }

    #[test]
    fn compete_gate_off_never_switches() {
        // RWM_COPA_COMPETE unset (the shipped default): the identical
        // never-draining queue must NOT flip modes or touch δ.
        let clock = Arc::new(MockClock::new());
        let mut path = PathState::new(0, clock.clone());
        path.force_wire_for_test(0.005);
        compete_warmup(&mut path, &clock);
        for _ in 0..40 {
            queue_update(&mut path, &clock);
        }
        let (on, in_compete, switches, delta, base) = path.copa_compete_diag();
        assert!(!on && !in_compete && switches == 0, "gate off ⇒ no switching");
        assert_eq!(delta, base, "gate off ⇒ δ stays the hint base");
    }

    #[test]
    fn compete_env_gate_requires_wire() {
        // The δ adaptation composes with the wire update law only.
        assert!(copa_compete_from_env(true, true));
        assert!(!copa_compete_from_env(true, false));
        assert!(!copa_compete_from_env(false, true));
        assert!(!copa_compete_from_env(false, false));
    }

    #[test]
    fn wire_mode_off_is_byte_identical_legacy() {
        // Env fully unset in the test process ⇒ wire mode off ⇒ the legacy
        // dynamics: steady state is the additive +2, exactly as before.
        let clock = Arc::new(MockClock::new());
        let mut path = PathState::new(0, clock.clone());
        for _ in 0..4 {
            path.record_rtt_sample(millis(20));
            clock.advance(millis(30));
            let c = path.cwnd;
            path.on_ack(c);
        }
        // End the ramp with an inflated window.
        for _ in 0..4 {
            path.record_rtt_sample(millis(80));
        }
        clock.advance(millis(100));
        path.on_ack(4);
        let after = path.cwnd;
        path.record_rtt_sample(millis(20));
        clock.advance(millis(100));
        path.on_ack(4);
        assert_eq!(
            path.cwnd,
            after + ADDITIVE_STEP as u32,
            "legacy steady state must remain the additive +2"
        );
    }

    // ── ack-merge counter re-homing (goal-gate "Unlock The Default 1") ──

    /// A tiny model of the receiver's `PathBatchTracker`: the source of the
    /// v6 cumulative counters, and the source of the legacy per-batch `Ack`
    /// payload. Both come from the SAME accumulator, which is what makes the
    /// equivalence law below a statement about the wire and not about
    /// arithmetic.
    #[derive(Default)]
    struct TrackerModel {
        cum_expected: u64,
        cum_received: u64,
    }
    impl TrackerModel {
        /// Returns the legacy Ack's `(expected_count, received_count)`.
        fn record_batch(&mut self, expected: u32, received: u32) -> (u32, u32) {
            self.cum_expected += expected as u64;
            self.cum_received += received as u64;
            (expected, received)
        }
    }

    /// THE consumer-equivalence law (pre-registered): over a randomized
    /// ack/loss trace in which an arbitrary subset of control datagrams is
    /// DROPPED, the totals the re-homed consumers see from the merged
    /// WindowAck's cumulative counters are EXACTLY the totals they saw from
    /// the per-batch legacy `Ack`.
    ///
    /// This is the property the merge is safe on: the loss feed
    /// (`record_batch`), the in-flight release (delivered + lost) and the
    /// pool-delivery feed are all COUNT-based, so carrying running sums and
    /// diffing them loses nothing an event stream carried — and unlike an
    /// event stream it is robust to ack loss, which a merged ack path must
    /// be (there are now half as many chances to deliver the same counts).
    #[test]
    fn ack_merge_counter_delta_matches_the_legacy_ack_totals_under_ack_loss() {
        let clock = Arc::new(MockClock::new());
        let mut path = PathState::new(0, clock.clone());
        let mut tracker = TrackerModel::default();
        // Deterministic pseudo-random trace (xorshift): batch sizes, loss
        // counts, and which acks reach the sender.
        let mut rng: u64 = 0x9E3779B97F4A7C15;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        let (mut legacy_expected, mut legacy_received) = (0u64, 0u64);
        let (mut merged_expected, mut merged_received) = (0u64, 0u64);
        let mut delivered_acks = 0usize;
        for _ in 0..2000 {
            let received = (next() % 32) as u32 + 1;
            // `expected >= received` always: the tracker's estimate is
            // received scaled by the batch-sequence gap.
            let gap = (next() % 4) as u32 + 1;
            let expected = received * gap;
            let (e, r) = tracker.record_batch(expected, received);
            // What the LEGACY per-batch Ack delivered (it is sent for every
            // batch, so every batch counts).
            legacy_expected += e as u64;
            legacy_received += r as u64;
            // What the MERGED ack delivers — but only when this control
            // datagram survives the wire (~25% dropped).
            if next() % 4 != 0 {
                delivered_acks += 1;
                let (de, dr) =
                    path.ack_merge_counter_delta(tracker.cum_expected, tracker.cum_received);
                merged_expected += de as u64;
                merged_received += dr as u64;
            }
        }
        // Flush: the final ack always lands (the transfer ends on one).
        let (de, dr) = path.ack_merge_counter_delta(tracker.cum_expected, tracker.cum_received);
        merged_expected += de as u64;
        merged_received += dr as u64;
        assert!(
            delivered_acks < 2000,
            "the trace must actually drop acks or it proves nothing"
        );
        assert_eq!(
            merged_expected, legacy_expected,
            "the loss feed's `expected` total must survive the merge exactly"
        );
        assert_eq!(
            merged_received, legacy_received,
            "the delivered total (in-flight release, pool feed, stats) must              survive the merge exactly"
        );
        assert!(
            merged_expected >= merged_received,
            "derived loss = expected - received must never underflow"
        );
    }

    /// `cum_received == 0` is the "no counter payload" sentinel used by the
    /// two timer-driven WindowAck sites (hole re-advertisement, hold-expiry
    /// unwedge), which broadcast ONE message to every live path and so cannot
    /// carry a per-path counter. It must be a total no-op — including on the
    /// cursor, so the next real ack still reports the whole outstanding delta.
    #[test]
    fn ack_merge_timer_ack_sentinel_is_inert_and_loses_no_counts() {
        let clock = Arc::new(MockClock::new());
        let mut path = PathState::new(0, clock.clone());
        assert_eq!(path.ack_merge_counter_delta(0, 0), (0, 0));
        assert_eq!(path.ack_merge_counter_delta(100, 80), (100, 80));
        // A timer ack lands mid-stream: inert, and the cursor does NOT move.
        assert_eq!(path.ack_merge_counter_delta(0, 0), (0, 0));
        assert_eq!(
            path.ack_merge_counter_delta(150, 130),
            (50, 50),
            "the counts the timer ack could not carry are still delivered next"
        );
    }

    /// Duplicated and REORDERED acks are idempotent: the cursor only moves
    /// forward, so a stale ack contributes nothing and cannot double-charge
    /// the loss feed or double-release in-flight budget.
    #[test]
    fn ack_merge_counter_delta_is_idempotent_under_duplication_and_reorder() {
        let clock = Arc::new(MockClock::new());
        let mut path = PathState::new(0, clock.clone());
        assert_eq!(path.ack_merge_counter_delta(200, 180), (200, 180));
        assert_eq!(
            path.ack_merge_counter_delta(200, 180),
            (0, 0),
            "a duplicate ack must be a no-op"
        );
        assert_eq!(
            path.ack_merge_counter_delta(120, 100),
            (0, 0),
            "a REORDERED (stale) ack must be a no-op, not a negative delta"
        );
        assert_eq!(
            path.ack_merge_counter_delta(260, 220),
            (60, 40),
            "and the cursor is still at the newest point, not the stale one"
        );
    }

    /// The derived loss count is `expected - received`, so `received` may
    /// never exceed `expected` however the receiver's batch-gap ESTIMATE
    /// moves. (The estimate is approximate by construction — see
    /// `PathBatchTracker` — and an underflow here would feed the loss
    /// estimator garbage.)
    #[test]
    fn ack_merge_counter_delta_never_lets_received_exceed_expected() {
        let clock = Arc::new(MockClock::new());
        let mut path = PathState::new(0, clock.clone());
        let (e, r) = path.ack_merge_counter_delta(10, 40);
        assert_eq!(e, 10);
        assert_eq!(r, 10, "received is clamped to expected, never above it");
        assert!(e >= r);
    }
}

// ── THE ONE-SIDED-CLAMP WITNESS, process-wide (`[LCW]`) ───────────────
//
// Goal-gate item 3c REDIRECTED. `PathState::loss_clamp_witness` carries the
// per-path counters; these mirror them process-wide so a battery reads ONE
// number per run off a teardown line instead of plumbing `PathState` into the
// diag renderer. Observation only — nothing here is read by a decision, and
// the clamp itself (`d_received.min(d_expected)`) is untouched.
//
// THE HYPOTHESIS THEY SCORE. §16.63 measured the sender-truth loss estimator
// reading 20× in the wrong direction, INCLUDING at N = 1 where the
// cross-path attribution error it was built to repair cannot exist. The RFC
// 6675 denominator explanation was refuted on the code (both operands count
// retransmits — a matched pair). The successor hypothesis is this `min`: the
// sender's own symbol counter and the receiver's cumulative echo are two
// clocks, so `d_received > d_expected` whenever the receiver's cursor
// momentarily leads, and the clamp RECTIFIES every such sample to zero loss
// instead of to negative loss. Rectifying a zero-mean jitter is a POSITIVE
// BIAS at ANY path count — which is exactly the shape of a result that
// survives at N = 1.
//
// The scoreable statistic is `over_mass / loss_mass`: if rectification is the
// mechanism, the rectified mass is a large fraction of the loss mass the
// estimator was actually fed, at every cell and every path count.
pub static LCW_OVER_N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static LCW_OVER_MASS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static LCW_LOSS_MASS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The process-wide one-sided-clamp witness line —
/// `[LCW] over_n=<n> over_mass=<m> loss_mass=<l> rect_frac=<m/l>`.
pub fn lcw_report_line() -> String {
    use std::sync::atomic::Ordering::Relaxed;
    let (n, m, l) = (
        LCW_OVER_N.load(Relaxed),
        LCW_OVER_MASS.load(Relaxed),
        LCW_LOSS_MASS.load(Relaxed),
    );
    let frac = if l == 0 { 0.0 } else { m as f64 / l as f64 };
    format!("[LCW] over_n={n} over_mass={m} loss_mass={l} rect_frac={frac:.4}")
}

/// Reset the process-wide witness — tests only, so one test's samples cannot
/// leak into another's assertion.
pub fn lcw_reset() {
    use std::sync::atomic::Ordering::Relaxed;
    LCW_OVER_N.store(0, Relaxed);
    LCW_OVER_MASS.store(0, Relaxed);
    LCW_LOSS_MASS.store(0, Relaxed);
}
