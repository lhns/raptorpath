//! The scheduler's gate accessors (the Copa wire/compete family, the
//! anchor-hygiene and placement arms, and the ack/release/charge gates) and the
//! resolve-time reads behind them. Re-exported from `crate::scheduler`.

// --- Wire-clocked Copa signal ---
//
// Under the wire signal Copa's delay term is quinn's packet-timed path RTT
// (`Connection::rtt`), which excludes the sender's own dwell in quinn's
// datagram queue; the app-layer echo RTT includes it, so Copa backed off
// against self-inflicted delay. Copa then runs its update law around the
// target rate 1/(δ·d_q), δ mapped continuously from the hint (`copa_delta`,
// paper §8.2). Active by default only when the engine feeds the substrate
// window (`RWM_QUIC_CC=passthrough` or `RWM_COPA_FEED=1`); `RWM_COPA_WIRE`
// forces it either way. Off with the environment unset (ADR-0062).

/// Pure decision function for the wire-signal gate: `qcc` = RWM_QUIC_CC, `feed`
/// = RWM_COPA_FEED as a flag, `wire` = RWM_COPA_WIRE raw value.
pub(crate) fn copa_wire_from_env(qcc: Option<&str>, feed: bool, wire: Option<&str>) -> bool {
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

/// Whether the wire-clocked Copa queue signal (and the δ-mapped update law) is
/// active for this process.
pub fn copa_wire_active() -> bool {
    crate::gates::get().copa_wire
}

/// The resolve-time read behind [`copa_wire_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_copa_wire() -> bool {
    let qcc = std::env::var("RWM_QUIC_CC").ok();
    let wire = std::env::var("RWM_COPA_WIRE").ok();
    let on = copa_wire_from_env(
        qcc.as_deref(),
        crate::config::env_flag("RWM_COPA_FEED", false),
        wire.as_deref(),
    );
    // Liveness echo, two-sided; the value is derived from three knobs, so the
    // inputs print beside the result.
    tracing::info!(
        copa_wire = on,
        quic_cc = qcc.as_deref().unwrap_or("unset"),
        copa_wire_env = wire.as_deref().unwrap_or("unset"),
        "Copa wire-clocked signal (RWM_COPA_WIRE / RWM_QUIC_CC / RWM_COPA_FEED)"
    );
    on
}

/// Pure decision function for the competitive-mode gate: requires both the env
/// flag and the wire-clocked law (only the wire update law consumes δ).
pub(crate) fn copa_compete_from_env(compete_flag: bool, wire_active: bool) -> bool {
    compete_flag && wire_active
}

/// Whether Copa's TCP-competitive mode switching is active for this process.
pub fn copa_compete_active() -> bool {
    crate::gates::get().copa_compete
}

/// The resolve-time read behind [`copa_compete_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_copa_compete(wire_active: bool) -> bool {
    let on = copa_compete_from_env(
        crate::config::env_flag("RWM_COPA_COMPETE", false),
        wire_active,
    );
    // Two-sided liveness echo: the off arm must be provably a control.
    tracing::info!(
        copa_compete = on,
        "Copa TCP-competitive mode (RWM_COPA_COMPETE, requires the wire signal)"
    );
    on
}

/// Whether the pool-anchor law is active (`RWM_POOL_ANCHOR`, experiment arm):
/// at N ≥ 2 live paths the pooled store cap's rate input is each path's
/// send-interval anchor ([`crate::control::SendRateAnchor`], burst-immune,
/// clock-gap discard) instead of the ack-interval windowed max, whose burst
/// peaks over-read under the estimator-cadence ack clock. The unset default
/// follows `RWM_EST_CADENCE`. Consumers: `PathState::charge_in_flight` and the
/// N ≥ 2 dynamic cap; the Copa cwnd feed is untouched.
pub fn pool_anchor_active() -> bool {
    crate::gates::get().pool_anchor
}

/// The resolve-time read behind [`pool_anchor_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_pool_anchor(est_cadence: bool) -> bool {
    crate::config::env_flag(
        "RWM_POOL_ANCHOR",
        est_cadence,
    )
}

/// Whether the O(1) windowed-max rate filter is active (`RWM_HONEST_ANCHOR`,
/// anchor-hygiene family, default on; the `RWM_ANCHOR_HYGIENE` umbrella
/// overrides either way).
///
/// `CopaState`'s BtlBw windowed max (`max_bw`) used to be a full-window fold
/// over `bw_samples` on every accepted sample: invisible when fed per ACK,
/// O(window·rate) work when fed per delivered symbol (`RWM_PLAIN_RS`). On, it
/// is read off a monotonic max-deque beside `bw_samples`: the same statistic to
/// the bit (`bw_mono_front_equals_full_window_fold`), same [1 s, 10 s] window,
/// amortized O(1). The gate selects cost, not behaviour; `=0` runs the fold.
pub fn honest_anchor_active() -> bool {
    crate::gates::get().honest_anchor
}

/// The resolve-time read behind [`honest_anchor_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_honest_anchor() -> bool {
    crate::config::anchor_gate_default("RWM_HONEST_ANCHOR", true)
}

/// A placement arm's flag: [`crate::config::env_flag`], default absent. A
/// value outside the strict boolean dialect (`RWM_PLACE_HOL=of`) is a startup
/// error naming the gate, so a typo can never run a control row labelled as
/// a challenger.
pub(crate) fn place_arm_flag(name: &str) -> bool {
    crate::config::env_flag(name, false)
}

/// Whether the derived placement temperature is active (`RWM_PLACE_T_DERIVED`,
/// experiment arm, paper §5.7): the softmax temperature is the Luce/Gumbel
/// scale of the scheduler's own ETA prediction error,
///
/// ```text
///     T  =  (sqrt(6)/pi) * sigma_e / ref_srtt  =  0.77970 * sigma_e / ref
/// ```
///
/// The softmax is `argmin` under i.i.d. Gumbel noise of scale `T`, so matching
/// `Var(T*G) = pi^2 T^2/6` to `sigma_e^2` fixes `T` with no free parameter.
/// `sigma_e` is the per-path dispersion of `realized - predicted` from the
/// sender `[ETA]` gauge (`net::eta::SenderEta::sigma_us`), pooled as an RMS
/// over the active set. The shipped `0.15` thus claims `sigma_e = 0.19238 *
/// ref`. Off is `place_temperature()` verbatim.
pub fn place_t_derived_active() -> bool {
    crate::gates::get().place_t_derived
}

/// The resolve-time read behind [`place_t_derived_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_place_t_derived() -> bool {
    let on = place_arm_flag("RWM_PLACE_T_DERIVED");
    // Two-sided liveness echo (`docs/measurement-discipline.md` rule 1).
    tracing::info!(
        place_t_derived = on,
        "placement temperature (RWM_PLACE_T_DERIVED, paper §5.7): \
         T = (sqrt6/pi)*sigma_e/ref from the sender's own ETA-error \
         dispersion when ON; the shipped place_temperature() when OFF"
    );
    on
}

/// Whether the placement frontier term is active (`RWM_PLACE_HOL`, experiment
/// arm, paper §5.7): the head-of-line cost added to `cost_i` for source
/// symbols,
///
/// ```text
///     X_i  =  [ delta*s_i  +  kappa*(s_i - H)+ ] / ref
///     s_i  =  [ (now + E_i) - F_hat ]+          the frontier push this placement adds
///     F_hat  =  running max of the stamped ETAs of symbols already placed
///     H      =  the free headroom of the LIVE store-cap law
/// ```
///
/// plus the wire-price ordering term at its derived `W`, which prices the
/// opposite sign of the same difference:
///
/// ```text
///     O_i  =  W * [ F_hat - (now + E_i) ]+ / ref
///     W    =  1 symbol / ( R_ref * tau_cool )        no literal; see place_hol_wire_price
/// ```
///
/// A placement behind the frontier pushes nothing and is free (`s_i = 0`),
/// which makes `X_i` a water-filling incentive rather than a slow-path penalty.
/// `delta` comes from `net::delta_price`, continuous in the dial. `kappa = 1`
/// is a declared upper bound, conservative against frontier-pushing
/// placements; its bind is gauged (`s_i > H`). Off skips the frontier read.
pub fn place_hol_active() -> bool {
    crate::gates::get().place_hol
}

/// The resolve-time read behind [`place_hol_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_place_hol() -> bool {
    let on = place_arm_flag("RWM_PLACE_HOL");
    tracing::info!(
        place_hol = on,
        "placement frontier term (RWM_PLACE_HOL, paper §5.7): \
         X_i = [delta*s_i + kappa*(s_i-H)+]/ref with s_i the frontier push \
         against the sender's own F_hat, plus the (a2) wire-price ordering \
         term at its derived W; ABSENT leaves the shipped cost untouched"
    );
    on
}

/// Whether the derived diversity weight is active (`RWM_PLACE_WDIV_DERIVED`,
/// experiment arm, paper §5.7): the repair diversity weight read off the
/// channel's burst persistence instead of `w_div = 1.0`,
///
/// ```text
///     V_i  =  fate_i * ( p_BB,i - eps_i )+ * srtt_i / ref
///     p_BB  =  1 - p_bg      Gilbert-Elliott bad->bad persistence
///     eps   =  the path's marginal loss rate
/// ```
///
/// A correlated repair costs the excess probability that one burst takes both,
/// which vanishes on a memoryless channel (`p_BB = eps`). Repairs only:
/// `fate_i` is zero for source symbols. A path without a valid GE estimate
/// keeps `w_div * fate_i` and is counted by the `cold_ge` bind gauge.
pub fn place_wdiv_derived_active() -> bool {
    crate::gates::get().place_wdiv_derived
}

/// The resolve-time read behind [`place_wdiv_derived_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_place_wdiv_derived() -> bool {
    let on = place_arm_flag("RWM_PLACE_WDIV_DERIVED");
    tracing::info!(
        place_wdiv_derived = on,
        "placement diversity weight (RWM_PLACE_WDIV_DERIVED, paper §5.7): \
         fate*(p_BB - eps)+ * srtt/ref from the path's Gilbert-Elliott \
         burst persistence when ON; the shipped w_div*fate when OFF"
    );
    on
}

/// Whether the cold-start placement price is active (`RWM_COLD_PLACE`,
/// anchor-hygiene family, default off).
///
/// `place_costs` reads `PathState::srtt()`, which for a leg with no RTT sample
/// is the 50 ms `DEFAULT_SRTT`-class seed. A cold leg joining warm incumbents
/// is priced out, draws nothing, takes no sample, and stays cold: a fixed point
/// of the estimator. On, the cold leg is priced at the active set's fastest
/// measured srtt, which costs no constant and is self-limiting because its
/// `in_flight/cwnd` term charges as soon as it is used. It binds only on a late
/// join; when all legs start cold together they warm within one RTT
/// (`a_late_joining_leg_is_locked_out_by_the_cold_price_and_admitted_without_it`,
/// `the_cold_start_placement_price_is_inert_wherever_every_leg_starts_cold`).
/// Off, the cold price is `p.srtt()` verbatim.
pub fn cold_place_active() -> bool {
    crate::gates::get().cold_place
}

/// The resolve-time read behind [`cold_place_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_cold_place() -> bool {
    let on = crate::config::anchor_gate("RWM_COLD_PLACE");
    // Two-sided liveness echo (`docs/measurement-discipline.md` rules 1, 15).
    tracing::info!(
        cold_place = on,
        "cold-start placement price (RWM_COLD_PLACE, anchor-hygiene rule 1): \
         an unmeasured leg's SRTT_i in the §5.7 cost is the active set's \
         fastest MEASURED srtt when ON, the 50-ms DEFAULT_SRTT-class seed \
         when OFF (shipped, bit-identical)"
    );
    on
}

/// Whether the raw-sample echo-ratio floor is active (`RWM_HONEST_K`,
/// anchor-hygiene family, default off).
///
/// K_i (`EchoRatioMin`, the residence-clock ratio of the honest caps and the
/// three-term law) is meant to be the smallest observed echoSRTT/RTprop but is
/// fed the smoothed srtt at the 5 ms refresh clock. The minimum of an EWMA sits
/// near the distribution's mean, so K reads high wherever delay is wide. On,
/// the same tracker (same window, same ≥ 1 clamp) is fed the raw per-sample
/// echo/RTprop ratio at `record_rtt`, making K min(raw)/min(raw).
pub fn honest_k_active() -> bool {
    crate::gates::get().honest_k
}

/// The resolve-time read behind [`honest_k_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_honest_k() -> bool {
    crate::config::anchor_gate("RWM_HONEST_K")
}

/// Whether the window-mode control-datagram merge is active (`RWM_ACK_MERGE`,
/// default on; `=0` is the opt-out arm; paper §9.5).
///
/// The receiver can emit two control datagrams per data message: the SACK
/// `WindowAck` and the per-batch `ControlMessage::Ack`, whose send site also
/// fires in window mode. The duplicate is dense on a clean single path (the
/// frontier advances every batch) and sparse under dual-path striping. On, in
/// window mode only, the `Ack` is suppressed, the `WindowAck` becomes
/// unconditional at the `Ack`'s cadence and carries its payload in the
/// cumulative counters, and every `Ack` consumer reads the counter diff. Block
/// mode keeps the `Ack`.
///
/// Only the datagram count changes: the delivery statistic
/// (`record_delivery`'s ack-interval windowed max) and its cadence are kept,
/// because without a `CopaFeed` it is the window-mode anchor; removing it
/// leaves `max_bw = 0` and the store cap stuck at boot 128. Not a dial: no law
/// on (δ, ρ, r) is selected.
pub fn ack_merge_active() -> bool {
    crate::gates::get().ack_merge
}

/// The resolve-time read behind [`ack_merge_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_ack_merge() -> bool {
    crate::config::env_flag("RWM_ACK_MERGE", true)
}

/// `RWM_LOSS_SENT_TRUTH` (default off, experiment arm): feed the per-path loss
/// estimator the sender's own `symbols_sent` delta instead of the receiver's
/// gap-derived `total_expected`. The law and its residual are on
/// [`PathState::sender_truth_loss_delta`].
///
/// At N ≥ 2 the global `batch_seq` gap is mostly the other path's symbols, so
/// the gap estimate is contaminated; at N = 1 the gate only removes ~1 BDP of
/// startup lag. The estimate feeds the NACK repair margin, the NACK congestion
/// multiplier and budget cap, the block-ARQ margins, the interleaver taper
/// decay, the shed budget and every placement cost with an `eps` term. No wire
/// format changes. It ships off because an honest loss estimate re-heats the
/// SRTT/loss-scaled recovery cadences that the contaminated value damped; those
/// need re-deriving first. Not a dial: it changes which measurement feeds one
/// estimator.
pub fn loss_sent_truth_active() -> bool {
    crate::gates::get().loss_sent_truth
}

/// The resolve-time read behind [`loss_sent_truth_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_loss_sent_truth() -> bool {
    crate::config::env_flag("RWM_LOSS_SENT_TRUTH", false)
}

/// `RWM_RELEASE_1TO1` (default off, experiment arm): make the release of a lost
/// symbol's budget slot 1:1 with its charge.
///
/// Two mechanisms release slots today:
///
/// 1. `control_msg.rs` releases `expected_count - received_count` in both ack
///    arms, where `expected` is `PathBatchTracker`'s global-`batch_seq` gap
///    estimate. At N >= 2 that gap is mostly the other path's symbols, so the
///    release is inflated; `release_in_flight` saturates at zero, so the
///    excess leaks the gauge open (`in_flight == 0` while symbols are
///    outstanding, holding `available()` wide open).
/// 2. [`PathState::expire_in_flight`], a time sweep of the charge log itself:
///    1:1 by construction, but with a `max(4 x SRTT, 250 ms)` horizon it is
///    only a backstop.
///
/// Under the gate (1) is removed and (2) runs at RFC 9002 §6.1.2's
/// kTimeThreshold, `9/8 x SRTT`, floored at the recovery plane's kGranularity
/// analog (`net::mp_time_threshold_split`, `net::NACK_RETX_COOLDOWN_FLOOR_US`).
/// No constant is introduced:
///
/// ```text
///   released(t)  =  delivered(t)  +  charges older than 9/8 x SRTT
/// ```
///
/// Both terms pop the same `in_flight_log` the charge pushed, so the ledger
/// cannot over-release however the paths are striped.
///
/// The sender-truth pair ([`PathState::sender_truth_release_delta`]) is kept as
/// the negative datum: releasing `d_received + (d_sent - d_received)`
/// telescopes to a constant `in_flight`, because `d_sent - d_received` is
/// `loss + delta(outstanding)`
/// (`sender_truth_release_pins_the_gauge_on_the_floor`).
///
/// Composes with [`charge_recovery_active`]: this makes releases 1:1 with
/// charges, that makes charges equal the wire. Not a dial.
pub fn release_1to1_active() -> bool {
    crate::gates::get().release_1to1
}

/// The resolve-time read behind [`release_1to1_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_release_1to1() -> bool {
    crate::config::env_flag("RWM_RELEASE_1TO1", false)
}

/// `RWM_CHARGE_RECOVERY` (default off, experiment arm): meter the SACK-gap
/// retransmit and the NACK repair margin.
///
/// Both build a `SymbolBatch` and call `transport.send_symbols` with no
/// `charge_in_flight`, no `consume_pace_tokens` and no `PathStats::symbols_sent`
/// increment, while every other wire channel (source, taper correction, the
/// generation arms, block-ARQ repair) meters all three at the handoff. Recovery
/// is exempt from the ack-clocked admission target, not from the ledger, and
/// charging cannot deadlock these sites because neither reads `available()` or
/// `cwnd_full`; they are budgeted by `cached_nack_budget` and the NACK
/// congestion multiplier. The three meters move together, as at the peer
/// sites, so a battery cannot attribute among them. Not a dial.
pub fn charge_recovery_active() -> bool {
    crate::gates::get().charge_recovery
}

/// The resolve-time read behind [`charge_recovery_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_charge_recovery() -> bool {
    crate::config::env_flag("RWM_CHARGE_RECOVERY", false)
}

/// `RWM_SIDLE_DERIVED` (default off, instrument): behaviour-inert. The
/// `sidle=`/`[WIDLE] idle=` fields print unchanged and this adds `sidle2=`/
/// `idle2=`, computed by `net::stall_threshold_us` over the same events, so the
/// fixed 3 ms threshold can be checked on the same runs. Where
/// `evt ≫ LOOP_WAKE_US`, read `sidle2`, not `sidle`.
pub fn sidle_derived_active() -> bool {
    crate::gates::get().sidle_derived
}

/// The resolve-time read behind [`sidle_derived_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_sidle_derived() -> bool {
    crate::config::env_flag("RWM_SIDLE_DERIVED", false)
}
