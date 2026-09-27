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
///
/// THE HINT→δ MAP ITSELF now lives ONCE, in [`hint_delta_price`], and is
/// read through [`crate::net::delta_price`] — the ONE place a hint names a
/// δ (§16.81). This function is the CC's own view of that number:
/// `RWM_COPA_DELTA` overrides it for the CC alone, so a battery can move the
/// contract's δ (`RWM_DELTA`) while pinning the congestion controller.
/// PRECEDENCE: `RWM_COPA_DELTA` ▸ `RWM_DELTA` ▸ the hint's map.
fn copa_delta(hint: ProtocolHint, over: Option<f64>) -> f64 {
    over.filter(|d| d.is_finite() && *d > 0.0)
        .unwrap_or_else(|| crate::net::delta_price(hint))
}

/// The hint→δ MAP, with no override of any kind applied: δ(hint) =
/// COPA_DELTA / ζ(hint) ∈ {50, 0.5, 0.005} (paper §12.4). The ONE
/// transcription of it in the engine; [`crate::net::delta_price`] is its
/// public seat and every δ-priced law reads THAT.
pub(crate) fn hint_delta_price(hint: ProtocolHint) -> f64 {
    COPA_DELTA / hint.tail_loss_scale()
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
/// A placement arm's flag: [`crate::config::env_flag`], default ABSENT. A
/// value outside the strict boolean dialect (`RWM_PLACE_HOL=of`) is a startup
/// error naming the gate, so a typo can never run a control row labelled as
/// a challenger.
fn place_arm_flag(name: &str) -> bool {
    crate::config::env_flag(name, false)
}

/// **`RWM_PLACE_T_DERIVED`** (Track A arm 1, ABSENT by default) - the
/// placement softmax temperature read as the Luce/Gumbel scale of the
/// scheduler's own ETA prediction error (paper 16.81.1):
///
/// ```text
///     T  =  (sqrt(6)/pi) * sigma_e / ref_srtt  =  0.77970 * sigma_e / ref
/// ```
///
/// The softmax IS `argmin` under i.i.d. Gumbel noise of scale `T` (the Luce
/// choice rule), so matching `Var(T*G) = pi^2 T^2/6` to `sigma_e^2` fixes `T`
/// with no free parameter. `sigma_e` is the dispersion of
/// `e = realized - predicted`, already maintained per path by the sender
/// `[ETA]` gauge's tau-lag estimator (`net::eta::SenderEta::sigma_us`), pooled
/// as an RMS over the ACTIVE candidate set.
///
/// **The shipped `0.15` is thereby a falsifiable claim about the wire**:
/// `T = 0.15 <=> sigma_e = 0.19238 * ref` at every cell. This gate does not
/// correct it - it makes the derived form runnable beside it. A `T_sigma` arm
/// that TIES with `CTL` licenses `sigma_e/ref`, never `0.15`.
///
/// OFF is byte-identical by construction: `place_temperature()` verbatim, which
/// the pinned cost/probability table asserts.
pub fn place_t_derived_active() -> bool {
    use std::sync::OnceLock;
    static F: OnceLock<bool> = OnceLock::new();
    *F.get_or_init(|| {
        let on = place_arm_flag("RWM_PLACE_T_DERIVED");
        // LIVENESS ECHO, TWO-SIDED (MEASUREMENT DISCIPLINE item 1): the OFF
        // value prints too, so "gate absent" is as checkable as "gate present".
        tracing::info!(
            place_t_derived = on,
            "placement temperature (RWM_PLACE_T_DERIVED, paper 16.81.1): \
             T = (sqrt6/pi)*sigma_e/ref from the sender's own ETA-error \
             dispersion when ON; the shipped place_temperature() when OFF"
        );
        on
    })
}

/// **`RWM_PLACE_HOL`** (Track A arm 2, ABSENT by default) - the frontier
/// (head-of-line) term the placement law is missing, 16.80.3's `Phi` physics
/// read at the SENDER and added to `cost_i` for SOURCE symbols:
///
/// ```text
///     X_i  =  [ delta*s_i  +  kappa*(s_i - H)+ ] / ref
///     s_i  =  [ (now + E_i) - F_hat ]+          the frontier push this placement adds
///     F_hat  =  running max of the stamped ETAs of symbols already placed
///     H      =  the free headroom of the LIVE store-cap law
/// ```
///
/// plus 16.80.6(a2)'s wire-price ORDERING term at its derived `W`, which
/// prices the opposite sign of the same difference:
///
/// ```text
///     O_i  =  W * [ F_hat - (now + E_i) ]+ / ref
///     W    =  1 symbol / ( R_ref * tau_cool )        no literal; see place_hol_wire_price
/// ```
///
/// A placement that lands BEHIND the frontier pushes nothing and is FREE
/// (`s_i = 0` exactly) - the property that makes `X_i` a water-filling
/// incentive rather than a slow-path penalty. `delta` is read from
/// `net::delta_price`, so the term is continuous in the dial and no hint is
/// ever compared. `kappa = 1` is a DECLARED UPPER BOUND, not a value (D0's
/// own fits put it at 0.0048-0.067), chosen conservative in the direction that
/// DISCOURAGES frontier-pushing placements; its bind is gauged (`s_i > H`).
///
/// OFF is byte-identical by construction: the term is not merely zero, the
/// whole frontier read is skipped and the shipped sum is returned unchanged.
pub fn place_hol_active() -> bool {
    use std::sync::OnceLock;
    static F: OnceLock<bool> = OnceLock::new();
    *F.get_or_init(|| {
        let on = place_arm_flag("RWM_PLACE_HOL");
        tracing::info!(
            place_hol = on,
            "placement frontier term (RWM_PLACE_HOL, paper 16.81.2): \
             X_i = [delta*s_i + kappa*(s_i-H)+]/ref with s_i the frontier push \
             against the sender's own F_hat, plus the (a2) wire-price ordering \
             term at its derived W; ABSENT leaves the shipped cost untouched"
        );
        on
    })
}

/// **`RWM_PLACE_WDIV_DERIVED`** (Track A arm 3, ABSENT by default) - the
/// repair diversity weight read off the channel's own burst persistence
/// instead of the `w_div = 1.0` literal (paper 16.81.3):
///
/// ```text
///     V_i  =  fate_i * ( p_BB,i - eps_i )+ * srtt_i / ref
///     p_BB  =  1 - p_bg      Gilbert-Elliott bad->bad persistence
///     eps   =  the path's marginal loss rate
/// ```
///
/// What a correlated repair costs is the EXCESS probability that one burst
/// takes both, and that excess VANISHES on a memoryless channel
/// (`p_BB = eps`) - which the shipped `1.0` does not. REPAIRS ONLY: `fate_i`
/// is identically zero for source symbols, so this arm cannot move a source
/// placement, and the pinned table's source rows are untouched even with it
/// armed. Cold rule: a path whose GE estimator is not yet valid keeps the
/// shipped `w_div * fate_i` and is already counted by the `cold_ge` bind
/// gauge.
pub fn place_wdiv_derived_active() -> bool {
    use std::sync::OnceLock;
    static F: OnceLock<bool> = OnceLock::new();
    *F.get_or_init(|| {
        let on = place_arm_flag("RWM_PLACE_WDIV_DERIVED");
        tracing::info!(
            place_wdiv_derived = on,
            "placement diversity weight (RWM_PLACE_WDIV_DERIVED, paper 16.81.3): \
             fate*(p_BB - eps)+ * srtt/ref from the path's Gilbert-Elliott \
             burst persistence when ON; the shipped w_div*fate when OFF"
        );
        on
    })
}

/// `kappa` - 16.80.3's non-overlapped stall fraction, as `X_i` uses it.
///
/// **A DECLARED UPPER BOUND, NOT A VALUE.** `kappa = 1` charges every second
/// of frontier stall in full. D0's own fits put the measured non-overlap at
/// 0.0048-0.067 at the four audited cells, so this over-charges the stall leg
/// by 15x to 200x - deliberately, in the direction that discourages
/// frontier-pushing placements. The direction is stated rather than assumed,
/// the constant carries a register row (paper 16.80.12), and its bind is
/// gauged: `[ETA] site=sender hol_sh=` is the fraction of source cost
/// evaluations in which `s_i > H`, i.e. in which `kappa` was reachable at all.
/// A `kappa` that never binds cannot be wrong.
pub(crate) const PLACE_KAPPA: f64 = 1.0;

/// **THE FRONTIER TERM ITSELF** (paper 16.81.2 + 16.80.6(a2)), as a pure
/// function of its inputs so its SHAPE can be asserted without a scheduler:
///
/// ```text
///     X_i + O_i  =  [ delta*push + kappa*(push - H)+ ] / ref  +  W*behind / ref
/// ```
///
/// `push = [(now + E_i) - F_hat]+` and `behind = [F_hat - (now + E_i)]+` are
/// the two signs of ONE difference, so at most one of them is nonzero and the
/// sum is continuous through the crossing. It is `C0` at the knee `push = H`
/// (the `(.)+` kink), non-decreasing in `push`, zero at `push = behind = 0`,
/// and continuous and non-decreasing in `delta` - the properties the
/// continuity gates assert at +/-2 % around each named point on the dial.
///
/// No branch reads a hint, a mode, or a threshold on the dial: the two `(.)+`
/// operators act on MEASUREMENT differences only.
pub(crate) fn place_frontier_cost(
    delta: f64,
    push_s: f64,
    behind_s: f64,
    h_s: f64,
    w: f64,
    ref_srtt: f64,
) -> f64 {
    (delta * push_s + PLACE_KAPPA * (push_s - h_s).max(0.0)) / ref_srtt + w * behind_s / ref_srtt
}

/// `(sqrt(6)/pi)` - the Gumbel scale-to-standard-deviation factor of
/// 16.81.1's variance match. NOT a tuning constant: `Var(Gumbel) = pi^2/6`
/// is arithmetic.
pub(crate) fn place_gumbel_scale() -> f64 {
    6.0_f64.sqrt() / std::f64::consts::PI
}

/// The two STORE-CAP inputs `X_i`'s headroom `H` reads, resolved ONCE per
/// process from the gate struct itself so this law and the store-cap law can
/// never disagree about which cap is live. `RuntimeGates::resolve()` walks the
/// whole environment, which is far too expensive per placement.
fn place_store_terms() -> (f64, bool) {
    use std::sync::OnceLock;
    static G: OnceLock<(f64, bool)> = OnceLock::new();
    *G.get_or_init(|| {
        let g = crate::gates::RuntimeGates::resolve();
        (g.store_gain, g.three_term)
    })
}

/// The engine clock (`net::now_us`, µs) - the SAME clock `net::emit_source`
/// stamps `send_ts_us` with, which is what makes `F_hat` comparable with
/// `now` here.
fn place_wall_now_us() -> u64 {
    crate::net::now_us()
}

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

/// `RWM_SIDLE_DERIVED` (default OFF) — goal-gate "Unlock The Default 2".
/// DIAG-ONLY and behaviour-inert: the legacy `sidle=`/`[WIDLE] idle=` fields
/// are printed UNCHANGED; this gate adds a SECOND field (`sidle2=`,
/// `idle2=`) computed by `net::stall_threshold_us` over the same event
/// stream, so the fixed-3 ms-threshold artifact question is answered on the
/// SAME runs in every arm, controls included.
/// An INSTRUMENT whose verdict is a STANDING INSTRUCTION (*where
/// `evt ≫ LOOP_WAKE_US`, read `sidle2`, not `sidle`*), retained after its
/// three session-mates (`RWM_POOL_DELIV`, `RWM_FLOOR_BOUND`,
/// `RWM_PATIENCE_DERIVED`) were removed as refuted arms.
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

/// Band width `c` for the FIXED-TIME-LAG dispersion gauge `tlag_us=`
/// (**paper §16.75**): a pair is admitted iff its lag falls in `[τ, c·τ]`.
///
/// **DECLARED RESOURCE BOUND, stated as one** (FORMULA-FIRST), not a fitted
/// constant — and it does not change the estimand, which §16.75.2 pins to the
/// band's POSITION at `τ = RTprop`. One octave is the standard dyadic binning
/// of a structure function, and the arithmetic of what it buys is in the paper:
/// at `c8L`'s slow sender leg (581 samples/s, spacing 1.72 ms) against
/// `RTprop ≈ 38 ms` the band `[38, 76] ms` is 22 spacings wide, so a partner
/// exists for essentially every anchor; at the battery's sparsest leg (`c8L`
/// receiver, 23.9 samples/s ⇒ 41.8 ms spacing) exactly one spacing fits, which
/// is the boundary case the `n` column and the parser-side `UNSCOREABLE-THIN`
/// rule exist for.
const SIGMA_TLAG_BAND_C: u32 = 2;

/// Decimation factor `m` for `tlag_us=`: a sample enters the gauge's ring only
/// if `τ/m` has elapsed since the last admitted one (**paper §16.75.4**).
///
/// **DECLARED RESOURCE BOUND, stated as one, and it is a MEMORY measure rather
/// than a filter.** It caps the ring's sample rate at `m/τ` REGARDLESS of the
/// ack rate, so a `SIGMA_CAND_WINDOW`-deep ring spans `256·τ/8 = 32·τ` at every
/// leg. Without it the ring at a 20 kHz sender leg would span 12.8 ms — far
/// less than one `RTprop` — the band `[τ, 2τ]` would contain **no pairs at
/// all**, and the gauge would render `-` at exactly the legs it is built for.
///
/// **It does not change the estimand**: rate invariance comes from selecting
/// pairs by elapsed TIME (§16.75.2), and any fixed `m` yields it. `m` sets only
/// the resolution of the realized lag inside the band — `τ/8`, i.e. 12.5 % of
/// `τ`.
const SIGMA_TLAG_DECIM_M: u32 = 8;

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
        Self::from_delta(crate::net::delta_price(hint))
    }

    /// The placement weights at an ARBITRARY point on the δ dial — the law,
    /// with no hint in sight (§16.81).
    ///
    ///     w_bw(δ) = clamp(½ − ¼·log₁₀(δ/δ_Auto), 0, 1),  w_lat = 1 − w_bw
    ///
    /// **THE THREE-ARM MATCH IS GONE, AND IT WAS ALREADY AFFINE.** The shipped
    /// weights were `1/0`, `0.5/0.5`, `0/1` at Realtime / Auto / Bulk — the
    /// EXACT log-midpoint at Auto, i.e. three samples of one affine function
    /// of the latency price, written out as a `match` because the engine had
    /// no δ to write it in terms of. The `¼` is the dial's own width — δ spans
    /// exactly four decades (0.005 → 50) while the weights span exactly 1 —
    /// and not a fourth constant. This is the paper's own transcription
    /// (§16.81.11), arrangement included.
    ///
    /// **IT LANDED BECAUSE THE BIT-EXACT PIN PASSED**, which is the condition
    /// §16.81.11 states: `log₁₀(δ/δ_Auto)` at the three presets returns
    /// exactly {2, 0, −2}, so `½ − ¼x` is exactly {0, ½, 1} and `1 − w_bw` is
    /// exactly {1, ½, 0}. Had `assert_eq!` failed at any of the three,
    /// `SchedulingWeights` would have stayed a `match` and a DECLARED CORNER.
    /// Pinned by `scheduling_weights_are_the_dial_not_a_mode`.
    ///
    /// `w_div` is hint-independent and stays a CONSTANT, not a corner: fate
    /// diversity for a repair is worth the same across workloads (a repair
    /// correlated with its coverage is wasted regardless of the (δ, ρ, r)
    /// triangle). Its value 1.0 remains UNDERIVED and is a register row.
    /// See `place_symbol`.
    pub fn from_delta(delta_price: f64) -> Self {
        let w_bw = (0.5 - 0.25 * (delta_price.max(1e-12) / raptorpath_math::DELTA_AUTO).log10())
            .clamp(0.0, 1.0);
        Self { w_lat: 1.0 - w_bw, w_bw, w_div: 1.0 }
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
    /// Observation only: read by the `[DIAG] sig_us=` and `[QCLK]` gauges.
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
    /// **THE TIMESTAMPED, TIME-DECIMATED RTT SERIES for candidate 4
    /// (`tlag_us=`, paper §16.75)**: `(arrival instant, rtt µs)`, ascending in
    /// time, at most `SIGMA_CAND_WINDOW` entries.
    ///
    /// **This cannot reuse `rtt_win`.** That FIFO carries no timestamps, so a
    /// lag can only be counted in SAMPLES there — which is precisely the defect
    /// §16.75 exists to remove: a lag-1 successive difference estimates the
    /// structure function `V` at whatever the inter-sample spacing happens to
    /// be, and that spacing is set by the ack rate rather than by the link.
    ///
    /// **And it cannot reuse `rtt_samples` either**, for the same reason
    /// `rtt_win` cannot: that deque is a MONOTONIC min-deque and holds a lower
    /// envelope of the series, not the series.
    ///
    /// Entries are admitted at most one per `RTprop / SIGMA_TLAG_DECIM_M`, so
    /// the ring spans `32 · RTprop` at every sample rate (see the const doc).
    /// Fed unconditionally; read by nothing but `[DIAG]`.
    rtt_tlag: VecDeque<(Instant, u32)>,
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
            rtt_tlag: VecDeque::with_capacity(SIGMA_CAND_WINDOW),
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

        // CANDIDATE 4 (`tlag_us=`, paper §16.75): the TIMESTAMPED, TIME-DECIMATED
        // series. Fed AFTER `min_rtt` is refreshed above, so the admission
        // spacing uses the freshest `τ = RTprop` this sample can see.
        //
        // Admission is a SPACING CAP applied identically at every rate — not a
        // branch on the rate, not a threshold that selects a code path. As the
        // arrival spacing rises through `τ/m` the admitted stream passes
        // continuously from decimated to pass-through (§16.75.8, continuity in
        // the sample rate). With `τ` unavailable nothing is admitted, the pair
        // set stays empty and the gauge renders `-`: there is NO fallback
        // constant, because a fallback constant is a free constant with no
        // provenance and a second code path (§16.75.6 F2).
        //
        // O(1): one `duration_since`, at most one push and one pop.
        if let Some(tau) = self.min_rtt {
            let spacing = tau / SIGMA_TLAG_DECIM_M;
            let admit = match self.rtt_tlag.back() {
                Some((t_last, _)) => now.duration_since(*t_last) >= spacing,
                None => true,
            };
            if admit {
                if self.rtt_tlag.len() == SIGMA_CAND_WINDOW {
                    self.rtt_tlag.pop_front();
                }
                self.rtt_tlag
                    .push_back((now, (rtt.as_micros() as u64).min(u32::MAX as u64) as u32));
            }
        }

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
        // THE RAW SAMPLE DUMP (`RWM_RTT_DUMP`, default OFF) — fed HERE, at the
        // single delegate every ack path funnels through, so the dumped series
        // is EXACTLY the series the five dispersion gauges consume. Clause `B`
        // compares an estimator against its own input; "its own input" has to
        // be literally true, or the comparison is the incommensurable one the
        // scored battery convicted (a 20 Hz ICMP probe against a kHz sender).
        // Null check with the gate off; no engine decision reads it.
        if let Some(d) = crate::net::rttdump::gauge() {
            d.note_rtt(self.id, (rtt.as_micros() as u64).min(u32::MAX as u64) as u32);
        }
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

    /// **CANDIDATE 4 — the τ-LAG PAIR SET. The one place the pairing rule
    /// exists**, so that [`rtt_tlag_us`] and [`rtt_tlag_samples`] can never
    /// describe different sample sets and the `-`-iff-`n == 0` biconditional
    /// holds BY CONSTRUCTION rather than by two functions agreeing.
    ///
    /// Paper §16.75.0, transcribed:
    ///
    /// ```text
    ///     P(τ) = { (i, j(i)) : j(i) = argmax { t_j : t_i − t_j ≥ τ },  j < i
    ///                          admitted iff  t_i − t_{j(i)} ≤ c·τ }
    ///
    ///     τ = RTprop  (MEASURED),   c = SIGMA_TLAG_BAND_C = 2
    /// ```
    ///
    /// Every anchor contributes at most one pair — its most recent admissible
    /// partner — so `|P(τ)|` is a count of anchors and is directly readable as
    /// the gauge's `n`.
    ///
    /// Empty (⇒ the gauge renders `-`) when `τ` is not yet established, when
    /// `τ` is zero, or when the leg's own spacing exceeds `c·τ` so that no
    /// partner is admissible anywhere in the ring. **That last case is the
    /// honest verdict at a leg too thin to hold a τ-lag pair, and it is
    /// reported rather than patched** (§16.75.6 F1).
    ///
    /// `O(L)`: the ring is ascending in time and `j` is non-decreasing in `i`,
    /// so the scan is a two-pointer sweep, at the `[DIAG]` cadence only.
    ///
    /// [`rtt_tlag_us`]: Self::rtt_tlag_us
    /// [`rtt_tlag_samples`]: Self::rtt_tlag_samples
    fn tlag_diffs(&self) -> Vec<u32> {
        let tau = match self.copa.min_rtt {
            Some(t) if !t.is_zero() => t,
            _ => return Vec::new(),
        };
        let hi = tau * SIGMA_TLAG_BAND_C;
        let ring = &self.copa.rtt_tlag;
        let n = ring.len();
        let mut out: Vec<u32> = Vec::with_capacity(n);
        // `j` tracks the LAST index whose lag to the current anchor is still
        // `≥ τ`. Lag decreases in `j` and anchors advance in time, so `j` only
        // ever moves forward.
        let mut j = 0usize;
        for i in 0..n {
            let (ti, vi) = ring[i];
            while j + 1 < i && ti.duration_since(ring[j + 1].0) >= tau {
                j += 1;
            }
            if j < i {
                let (tj, vj) = ring[j];
                let lag = ti.duration_since(tj);
                if lag >= tau && lag <= hi {
                    out.push(vi.abs_diff(vj));
                }
            }
        }
        out
    }

    /// **CANDIDATE 4 — `tlag_us=`, THE RATE-INVARIANT DISPERSION.** Median
    /// absolute difference at a fixed TIME lag `τ = RTprop`, paper §16.75:
    ///
    /// ```text
    ///     σ̂_Δ(τ)  =  median { |rtt(t_i) − rtt(t_j)| : (i, j) ∈ P(τ) }
    /// ```
    ///
    /// **WHY IT EXISTS, AND IT IS NOT AN IMPROVEMENT ON `msd_us` — IT IS A
    /// DIFFERENT ESTIMAND.** For a stationary series with autocorrelation `ρ`,
    /// the median absolute difference at lag `τ` is the structure function
    /// (variogram) evaluated there:
    ///
    /// ```text
    ///     median |X(t + τ) − X(t)|  =  0.6745 · √( 2·(1 − ρ(τ)) ) · σ
    /// ```
    ///
    /// A lag-1 successive difference — which is what [`rtt_msd_us`] computes —
    /// is therefore `V` evaluated at **whatever the inter-sample spacing
    /// happens to be**, and that spacing is set by the ack rate, i.e. by the
    /// traffic. It is not a property of the link and not a property of the
    /// estimator, so **two legs of one run estimate two different quantities.**
    ///
    /// **That is measured, not argued.** The scored VM battery (goal-gate, "THE
    /// SIGMA ESTIMATOR — THE SCORED RESULT" §6) found `R_total` tracking the
    /// sample rate at `rho = −0.548` across the eight sender legs, with the two
    /// thinnest legs (581 and 1 762 samples/s) the two worst readings in the
    /// battery (34.6 and 16.5) — and `msd_us`'s 8.667 on the data path is the
    /// nearest any candidate came to the pre-registered accept bar of 6.0.
    /// Selecting pairs by ELAPSED TIME fixes the estimand before any sample
    /// arrives: a sparse leg and a dense leg select the same τ-differences of
    /// the same process, the sparse leg simply finds fewer of them.
    ///
    /// **Provenance.** `median`, `|·|` — CITED, von Neumann, Kent, Bellinson &
    /// Hart 1941; RFC 3550 §A.8 is the same construction at lag 1. The
    /// extension to a STATED lag is Matheron's variogram (1963) and Allan's
    /// two-sample deviation at averaging time τ (1966). `τ = RTprop` —
    /// MEASURED, and the fraction is **1 and DERIVED**: one `RTprop` is the
    /// smallest lag at which two RTT samples observe distinct path traversals
    /// rather than the same queue occupancy on one traversal, which is the same
    /// ruling `control/anchor.rs` already enforces when it rejects sub-`RTprop`
    /// delivery-rate samples. `c` and `m` — DECLARED RESOURCE BOUNDS with their
    /// arithmetic, in their own const docs.
    ///
    /// **UNSCALED, exactly as `qsp_us` is.** Clause `S`'s `R_total` is a ratio
    /// so no fixed scaling changes it, and the rebuilt clause `B` compares this
    /// reading against the SAME functional computed offline over the same
    /// samples, so no scaling changes that either. The Gaussian conversion
    /// above needs `ρ(τ)` — the very thing being measured — so it is
    /// **documented here and applied nowhere.**
    ///
    /// **Reduces to `msd_us` in the limit** `τ → Δt`: the band collapses onto
    /// the lag-1 pair. The successor contains its predecessor, and the whole of
    /// the claimed improvement is a τ chosen by physics instead of by traffic.
    ///
    /// Read by nothing. `None` **iff** [`rtt_tlag_samples`] is 0.
    ///
    /// [`rtt_msd_us`]: Self::rtt_msd_us
    /// [`rtt_tlag_samples`]: Self::rtt_tlag_samples
    pub fn rtt_tlag_us(&self) -> Option<u64> {
        let mut d = self.tlag_diffs();
        if d.is_empty() {
            return None;
        }
        d.sort_unstable();
        Some(Self::cand_quantile(&d, 0.50))
    }

    /// τ-LAG PAIRS available to [`rtt_tlag_us`] right now — `|P(τ)|`.
    ///
    /// It is the PAIR count and not the sample count because the pairs are what
    /// the median is taken over, and the count beside a value must describe
    /// that value's own sample set. Both come from [`tlag_diffs`], so
    /// `rtt_tlag_us().is_none() ⇔ rtt_tlag_samples() == 0` holds by
    /// construction.
    ///
    /// **A reading resting on fewer than `L/8 = 32` pairs is scored
    /// `UNSCOREABLE-THIN` by the battery's PARSER** (paper §16.75.6 F1). That
    /// rule is declared in the paper, applied off-box, and is deliberately
    /// **not** a threshold anywhere in this crate: `diag.rs`'s standing reason
    /// applies verbatim — a field that disappears below a threshold cannot be
    /// told apart from a path that was never sampled.
    ///
    /// [`rtt_tlag_us`]: Self::rtt_tlag_us
    /// [`tlag_diffs`]: Self::tlag_diffs
    pub fn rtt_tlag_samples(&self) -> u64 {
        self.tlag_diffs().len() as u64
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

    /// Test hook: force the pool-anchor feed regardless of the process-global
    /// env cache (unit tests must not depend on it — the `force_wire`
    /// pattern).
    #[cfg(test)]
    pub fn force_pool_anchor_feed(&mut self, on: bool) {
        self.pool_anchor_feed = on;
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
    /// **THE SENDER-SITE `[ETA]` GAUGE** (`net/eta.rs`). It lives here and not
    /// in the sender loop because its two feed sites -- the PLACEMENT
    /// (`net/emit_source.rs`, which picks the path) and the ACK
    /// (`net/control_msg.rs`, which learns what actually happened) -- already
    /// hold this lock and would otherwise never see each other's state.
    ///
    /// READ BY NOTHING in any law. `place_costs` writes only its bind
    /// counters; `place_symbol`'s probabilities do not depend on it.
    eta: crate::net::eta::SenderEta,
    /// `(cold_r, cold_ge, evaluations)` accumulated by `place_costs`, which is
    /// `&self` all the way up through `place_symbol` -- hence a `Cell`.
    /// Drained into `eta` at the report cadence by `drain_place_bind`; NOTHING
    /// on the placement path takes a lock or reads the gauge.
    place_bind: std::cell::Cell<(u64, u64, u64)>,
    /// **THE TRACK A ARM GAUGES**, same `Cell` discipline and for the same
    /// reason: `place_costs` takes `&self`, and an observation may not need a
    /// write lock the law itself does not need. Drained by
    /// `drain_place_bind`.
    ///
    /// `place_t_gauge` = `(last T_eff, t_cold, n)` - the temperature actually
    /// used, how many resolutions had NO measured dispersion anywhere in the
    /// active set (and therefore fell back to the shipped `T`), and the
    /// denominator.
    place_t_gauge: std::cell::Cell<(f64, u64, u64)>,
    /// `place_hol_gauge` = `(s_i > H binds, source cost evaluations,
    /// argmin-moved count, place_costs calls, last W_live)` - the `kappa`
    /// bind fraction and the WB1-style EXECUTION WITNESS that the term
    /// actually changed a decision.
    place_hol_gauge: std::cell::Cell<(u64, u64, u64, u64, f64)>,
    clock: Arc<dyn Clock>,
    /// Global correction deficit tracker (paper Section 13.4).
    pub deficit: CorrectionDeficit,
    /// Scheduling weights from protocol hint.
    weights: SchedulingWeights,
    /// `RWM_PLACE_T_DERIVED` (Track A arm 1) as resolved for THIS scheduler.
    /// Defaults to the process gate; settable so a unit test can drive both
    /// sides of the arm in one process.
    place_t_derived: bool,
    /// `RWM_PLACE_HOL` (Track A arm 2), same discipline.
    place_hol: bool,
    /// `RWM_PLACE_WDIV_DERIVED` (Track A arm 3), same discipline.
    place_wdiv_derived: bool,
    /// Protocol hint — also sets Copa-lite's queue target on each path
    /// (paper Section 12.4 / P1).
    hint: ProtocolHint,
    /// Block-granular source affinity (see struct docs). On by default;
    /// `false` restores per-symbol greedy striping (ablation).
    block_affinity: bool,
    /// Smooth-WRR credit per path for the block-affinity pick.
    affinity_credit: HashMap<PathId, f64>,
    /// `RWM_COLD_PLACE` (anchor-hygiene rule 1 at the placement site) as a
    /// per-scheduler VALUE rather than a hot-path env read — for the same
    /// reason the estimator's
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
            eta: Default::default(),
            place_bind: std::cell::Cell::new((0, 0, 0)),
            place_t_gauge: std::cell::Cell::new((0.0, 0, 0)),
            place_hol_gauge: std::cell::Cell::new((0, 0, 0, 0, 0.0)),
            place_t_derived: place_t_derived_active(),
            place_hol: place_hol_active(),
            place_wdiv_derived: place_wdiv_derived_active(),
            clock,
            deficit: CorrectionDeficit::new(),
            weights: SchedulingWeights::from_hint(hint),
            hint,
            block_affinity: true,
            affinity_credit: HashMap::new(),
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

    /// `RWM_PLACE_T_DERIVED` (Track A arm 1) for this scheduler.
    pub fn set_place_t_derived(&mut self, enabled: bool) {
        self.place_t_derived = enabled;
    }

    /// Is the derived temperature armed on this scheduler?
    pub fn place_t_derived(&self) -> bool {
        self.place_t_derived
    }

    /// `RWM_PLACE_HOL` (Track A arm 2) for this scheduler.
    pub fn set_place_hol(&mut self, enabled: bool) {
        self.place_hol = enabled;
    }

    /// Is the frontier term armed on this scheduler?
    pub fn place_hol(&self) -> bool {
        self.place_hol
    }

    /// `RWM_PLACE_WDIV_DERIVED` (Track A arm 3) for this scheduler.
    pub fn set_place_wdiv_derived(&mut self, enabled: bool) {
        self.place_wdiv_derived = enabled;
    }

    /// Is the derived diversity weight armed on this scheduler?
    pub fn place_wdiv_derived(&self) -> bool {
        self.place_wdiv_derived
    }

    /// The cold-start placement price setting in force for this scheduler.
    pub fn cold_place(&self) -> bool {
        self.cold_place
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
    /// **THE EFFECTIVE PLACEMENT TEMPERATURE** - `place_temperature()` (the
    /// shipped `0.15`, or `RWM_PLACE_T`) with the arm ABSENT, and 16.81.1's
    /// Luce/Gumbel scale with `RWM_PLACE_T_DERIVED` armed:
    ///
    /// ```text
    ///     T  =  (sqrt(6)/pi) * sigma_e / ref_srtt
    ///     sigma_e  =  pooled RMS, over the ACTIVE candidate set, of the
    ///                 per-path tau-lag dispersion of  e = realized - predicted
    /// ```
    ///
    /// **THE COLD RULE, STATED IN FULL BECAUSE THE ALTERNATIVE WOULD BE A
    /// HIDDEN CONSTANT.** The pool is taken over the paths that HAVE a
    /// dispersion sample - a path with none contributes nothing rather than a
    /// fabricated zero, which would drag `T` toward `argmin` on the strength of
    /// a missing measurement. If NO active path has one, `T_eff` is the
    /// shipped `T` and the `t_cold` counter is bumped, so the fallback is READ
    /// off `[ETA] site=sender t_cold=` rather than assumed.
    ///
    /// `sigma_e -> 0` needs no branch here: `T = 0` reaches
    /// `place_probs_with_temperature`'s existing degenerate handling and
    /// resolves to the argmin, which IS the law's own `T -> 0` limit.
    ///
    /// Observation is a `Cell` write, never a lock: this is on `&self` because
    /// the law is.
    pub(crate) fn place_temperature_eff(&self) -> f64 {
        let shipped = place_temperature();
        if !self.place_t_derived {
            return shipped;
        }
        let cold_srtt = self.place_cold_srtt();
        let ref_srtt = self.place_ref_srtt(cold_srtt);
        let (sum_sq, n) = self
            .paths
            .values()
            .filter(|p| p.active)
            .filter_map(|p| self.eta.sigma_us(p.id))
            .fold((0.0_f64, 0_u64), |(acc, k), sig_us| {
                let x = sig_us as f64 / 1e6;
                (acc + x * x, k + 1)
            });
        let t = if n > 0 {
            place_gumbel_scale() * (sum_sq / n as f64).sqrt() / ref_srtt
        } else {
            shipped
        };
        let (_, cold, k) = self.place_t_gauge.get();
        self.place_t_gauge.set((t, cold + u64::from(n == 0), k + 1));
        t
    }

    pub fn place_probs(&self, is_repair: bool, covered_paths: &[PathId]) -> Vec<(PathId, f64)> {
        self.place_probs_with_temperature(is_repair, covered_paths, self.place_temperature_eff())
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

    /// The COLD PRICE of the placement law's SRTT reference
    /// (`RWM_COLD_PLACE`): the active set's fastest MEASURED srtt, or `None`
    /// with the gate off / nothing measured yet. Lifted out of `place_costs`
    /// VERBATIM so `place_temperature_eff` divides by exactly the same `ref`
    /// the costs are de-dimensionalised by - two `ref`s would make `c_i/T`
    /// meaningless.
    fn place_cold_srtt(&self) -> Option<f64> {
        if self.cold_place {
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
        }
    }

    /// `ref_srtt` - the fastest active path's SRTT, floored. Lifted out of
    /// `place_costs` verbatim; see `place_cold_srtt`.
    fn place_ref_srtt(&self, cold_srtt: Option<f64>) -> f64 {
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
        if ref_srtt.is_finite() {
            ref_srtt
        } else {
            PLACE_REF_FLOOR_SECS
        }
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
    pub(crate) fn place_costs(&self, is_repair: bool, covered_paths: &[PathId]) -> Vec<(PathId, f64)> {
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
        let cold_srtt: Option<f64> = self.place_cold_srtt();
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

        let ref_srtt = self.place_ref_srtt(cold_srtt);

        let w_bw = self.weights.w_bw;
        let w_div = self.weights.w_div;

        let covered_total = covered_paths.len() as f64;

        // ── TRACK A ARM 2 (`RWM_PLACE_HOL`): THE FRONTIER TERM'S INPUTS ────
        // Read ONCE per placement, all of them from live state, and only when
        // the arm is armed AND the symbol is SOURCE (a repair does not extend
        // the cumulative frontier - it fills behind it). `None` otherwise, and
        // the closure below adds a literal `0.0`, so the shipped sum is
        // returned unchanged - which is what the pinned cost table asserts.
        let hol = (self.place_hol && !is_repair).then(|| {
            // `F_hat` - the sender's running max of stamped ETAs, epoch us.
            // ZERO IS THE "NOTHING STAMPED YET" SENTINEL, NOT A TIME: pricing
            // against it would charge every placement its whole distance from
            // the epoch. No frontier => no push to price, and the arm is inert
            // until the gauge has seen one placement.
            let f_hat_us = self.eta.frontier_eta_us();
            let f_hat_s = (f_hat_us > 0).then(|| f_hat_us as f64 / 1e6);
            let now_s = place_wall_now_us() as f64 / 1e6;
            // The CONTRACT's own price, continuous in the dial. `delta_price`
            // is the one seat a hint names a delta, and `RWM_DELTA` moves it
            // between the named points - nothing here compares a hint.
            let delta = crate::net::delta_price(self.hint);
            let (gain, three_term) = place_store_terms();
            // RTprop reference: the fastest MEASURED windowed-min over the
            // active set. With none measured the law falls back to the SRTT
            // reference it already holds - another leg's MEASUREMENT, never a
            // constant (anchor-hygiene rule 1).
            let rtprop_ref_s = self
                .paths
                .values()
                .filter(|p| p.active)
                .filter_map(|p| p.min_rtt())
                .map(|d| d.as_secs_f64())
                .fold(f64::INFINITY, f64::min);
            let rtprop_ref_s = if rtprop_ref_s.is_finite() { rtprop_ref_s } else { ref_srtt };
            // `H` - the FREE HEADROOM of the store-cap law that is actually in
            // force: `(gain-1)*RTprop` for the shipped cap, the contract's own
            // declared stall when `RWM_THREE_TERM` is the live cap. The read
            // selects which CAP law's headroom is quoted; it does not select a
            // placement law, and neither branch invents arithmetic - both are
            // the existing expressions.
            let h_s = if three_term {
                crate::net::contract_stall_s(
                    1.0,
                    crate::net::delta_budget_b_of(delta),
                    rtprop_ref_s,
                    ref_srtt,
                )
            } else {
                (gain - 1.0).max(0.0) * rtprop_ref_s
            };
            // `W` - 16.80.6(a2)'s wire price of one spurious fire, DERIVED
            // FROM LIVE QUANTITIES WITH NO LITERAL OF ITS OWN:
            //
            //     W = P_arq * (T_pay+h)*8 / (R_ref_bits * tau_cool)
            //       = 1 symbol / (R_ref_symbols * tau_cool)
            //
            // - the object's byte size cancels because the scheduler's own
            // state is denominated in SYMBOLS: `cwnd_i/srtt_i` is path i's
            // delivery rate in symbols per second, and `R_ref` is the fastest
            // mover among the active paths. `P_arq = 1` on the retain-until-
            // acked seat (rho = 1), which is the only seat the plain window
            // has. `tau_cool` is the shipped per-seq retransmit cooldown
            // floor, an existing engine constant read rather than restated.
            // At the c8 shape (10 ms srtt, ~89 symbols of cwnd, 10 ms
            // cooldown) this evaluates to 1.1e-2, reproducing the paper's own
            // arithmetic - and PREDICTED INERT against an O(1) load term.
            let r_ref = self
                .paths
                .values()
                .filter(|p| p.active)
                .map(|p| p.cwnd.max(1) as f64 / srtt_of(p).max(PLACE_REF_FLOOR_SECS))
                .fold(0.0_f64, f64::max);
            let tau_cool_s = crate::net::NACK_RETX_COOLDOWN_FLOOR_US as f64 / 1e6;
            let w = if r_ref > 0.0 { 1.0 / (r_ref * tau_cool_s) } else { 0.0 };
            (f_hat_s, now_s, delta, h_s, w)
        });

        // Returns `(shipped cost, the arm's addition)` so the execution
        // witness below can ask whether the addition moved the argmin. With
        // the arm absent the addition is a literal `0.0` and `base + 0.0` is
        // `base` exactly.
        let cost_of = |p: &PathState| -> (f64, f64) {
            // Frontier-completion-time — the always-on load term (unit weight),
            // de-dimensionalised by the fastest SRTT so it is O(1). This single
            // term carries BOTH the congestion signal (queue drain at the pacing
            // rate) and the propagation preference; because it is expressed in
            // TIME it is capacity-aware, so it water-fills by capacity rather
            // than over-loading the slow path.
            //
            // (The frontier-slack generalization `max(0, Ê_i − D_i)`,
            // `RWM_PLACE_SLACK`, was refuted — goal-gate "C8 Slow-Path
            // Conversion" — and removed; D = 0 is the shipped term, kept
            // verbatim below.)
            let srtt_i = srtt_of(p);
            let load = p.expected_delivery_load_at(srtt_i).max(0.0) / ref_srtt;
            // Bandwidth/correction burden (loss/wire waste); the hint's w_bw
            // dial. w_lat does NOT gate placement: on a reliable in-order stream
            // latency-to-frontier is the completion cost itself, already carried
            // by `load` at unit weight, not a per-hint preference.
            let r = p.correction_rate();
            // THE COLD-r PRICE. `correction_rate()` is `inf` on a path with no
            // loss estimate yet, and the shipped law prices that at the
            // literal 10.0 -- one of the open register's unprovenanced
            // constants. The VALUE is untouched here; the BIND is COUNTED,
            // because every clamp owes a bind-fraction gauge (`[ETA]
            // site=sender cold_r=`).
            //
            // THE COLD-GE PRICE, on the same line: Track A's `w_div`
            // derivation reads a Gilbert-Elliott burst probability off the
            // path, and a path with no SRTT measurement has no burst model
            // either, so its diversity term rests on the hint's `w_div`
            // literal alone. Counted for the same reason. NO TERM OF THE COST
            // CHANGES -- this is one `Cell` increment beside an existing
            // branch.
            let cold_r = r.is_infinite();
            let cold_ge = p.srtt_measured().is_none();
            let (br, bg, bn) = self.place_bind.get();
            self.place_bind.set((br + cold_r as u64, bg + cold_ge as u64, bn + 1));
            let r = if cold_r { 10.0 } else { r };
            // Fate diversity (repairs only): fraction of covered symbols on p.
            let fate = if is_repair && covered_total > 0.0 {
                covered_paths.iter().filter(|&&c| c == p.id).count() as f64 / covered_total
            } else {
                0.0
            };
            // ── TRACK A ARM 3 (`RWM_PLACE_WDIV_DERIVED`) ──────────────────
            // What a correlated repair COSTS is the excess probability that
            // one burst takes both legs, priced at the round it wastes:
            //
            //     V_i = fate_i * (p_BB,i - eps_i)+ * srtt_i / ref
            //
            // and that excess VANISHES on a memoryless channel, which the
            // shipped `w_div = 1.0` does not. Cold rule: a path whose
            // Gilbert-Elliott estimator is not yet valid has no burst model,
            // so it keeps the shipped term - the case the `cold_ge` gauge
            // above already counts. `fate_i == 0` for every SOURCE symbol, so
            // this arm cannot move a source placement at all.
            let ge = p.estimator.ge_estimator();
            let div = if self.place_wdiv_derived && ge.is_valid() {
                fate * (1.0 - ge.p_bg() - p.estimator.loss_rate()).max(0.0) * srtt_i / ref_srtt
            } else {
                w_div * fate
            };
            // ── TRACK A ARM 2's term, the only new addend ─────────────────
            //
            //     X_i = [ delta*s_i + kappa*(s_i - H)+ ] / ref
            //     s_i = [ (now + E_i) - F_hat ]+
            //
            // plus the (a2) ordering term `W*[F_hat - (now + E_i)]+/ref`,
            // which prices the OTHER sign of the same difference: a placement
            // that lands behind the frontier pushes nothing (`s_i = 0`
            // EXACTLY - such a placement is free, which is what makes `X` a
            // water-filling incentive rather than a slow-path penalty) but
            // still costs the wire what a spurious re-serve of it would.
            let x_i = hol.as_ref().map_or(0.0, |&(f_hat_s, now_s, delta, h_s, w)| {
                let arrival = now_s + p.expected_delivery_load_at(srtt_i);
                let (push, behind) = f_hat_s
                    .map_or((0.0, 0.0), |f| ((arrival - f).max(0.0), (f - arrival).max(0.0)));
                let over = (push - h_s).max(0.0);
                // THE KAPPA BIND GAUGE. `kappa = 1` is a declared upper bound
                // 15x-200x above D0's measured non-overlap; `over > 0` is the
                // only regime in which it is reachable at all, and a bound
                // that never binds cannot be wrong.
                let (sh, n, moved, calls, _) = self.place_hol_gauge.get();
                self.place_hol_gauge
                    .set((sh + u64::from(over > 0.0), n + 1, moved, calls, w));
                place_frontier_cost(delta, push, behind, h_s, w, ref_srtt)
            });
            (load + w_bw * r + div, x_i)
        };

        // **DETERMINISTIC TIE-BREAK.** `self.paths` is a `HashMap`, so its
        // iteration order is the hasher's and varies between processes. The
        // softmax normalisation is a SUM, so no probability changes -- but
        // `place_probs_with_temperature`'s degenerate branch resolves an exact
        // tie with `min_by` (which keeps the FIRST minimum) and
        // `place_symbol`'s inverse-CDF walk consumes the candidates in this
        // order, so an exact tie could land on either path depending on the
        // hasher. Sorting by id makes both REPRODUCIBLE without moving any
        // probability, which is exactly what the pinned cost table asserts.
        // THE SHIPPED SHAPE IS UNCHANGED, ALLOCATION INCLUDED: one `Vec` of
        // `(id, cost)` on the CTL path. The arm's own SHIPPED-cost column is
        // kept in a SECOND vector that exists only while the arm is armed, so
        // an absent arm costs neither an allocation nor a comparison.
        let mut bases: Vec<(PathId, f64)> = Vec::new();
        let mut out: Vec<(PathId, f64)> = self
            .paths
            .values()
            .filter(|p| p.active)
            .map(|p| {
                let (base, x) = cost_of(p);
                if hol.is_some() {
                    bases.push((p.id, base));
                }
                (p.id, base + x)
            })
            .collect();
        out.sort_unstable_by_key(|(id, _)| *id);
        // **THE EXECUTION WITNESS** (MEASUREMENT DISCIPLINE rule 1: prove the
        // mechanism under test executes). A term whose magnitude is reported
        // but which never changes a DECISION has not been measured. This asks
        // the question `place_probs_with_temperature` will actually ask —
        // first-minimum over the id-sorted candidates — with and without the
        // arm's addition, and counts the disagreements.
        if hol.is_some() {
            bases.sort_unstable_by_key(|(id, _)| *id);
            let arg = |v: &[(PathId, f64)]| -> Option<PathId> {
                v.iter()
                    .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(id, _)| *id)
            };
            let moved_now = arg(&bases) != arg(&out);
            let (sh, n, moved, calls, w) = self.place_hol_gauge.get();
            self.place_hol_gauge
                .set((sh, n, moved + u64::from(moved_now), calls + 1, w));
        }
        out
    }

    /// Move the accumulated `place_costs` cold-price binds into the `[ETA]`
    /// gauge. Called at the sender's report cadence and nowhere else.
    pub fn drain_place_bind(&mut self) {
        let (r, ge, n) = self.place_bind.replace((0, 0, 0));
        self.eta.add_place_bind(r, ge, n);
        // Track A arm 1: the temperature actually used, and how often the
        // cold rule fired. `T_eff` is a LEVEL, so the LAST value is the
        // reading; the two counters are cumulative like every other bind
        // gauge on this line.
        let (t_eff, t_cold, t_n) = self.place_t_gauge.replace((0.0, 0, 0));
        self.eta.add_place_t(t_eff, t_cold, t_n);
        // Track A arm 2: the kappa bind fraction, the argmin-moved witness,
        // and `W` at its live value.
        let (sh, sh_n, moved, calls, w) = self.place_hol_gauge.replace((0, 0, 0, 0, 0.0));
        self.eta.add_place_hol(sh, sh_n, moved, calls, w);
    }

    /// The sender-site `[ETA]` gauge, mutably -- the placement stamp and the
    /// ack. Observation only; nothing downstream of it feeds a law.
    pub fn eta_mut(&mut self) -> &mut crate::net::eta::SenderEta {
        &mut self.eta
    }

    /// The sender-site `[ETA]` gauge, read-only.
    pub fn eta(&self) -> &crate::net::eta::SenderEta {
        &self.eta
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
mod tests;

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
