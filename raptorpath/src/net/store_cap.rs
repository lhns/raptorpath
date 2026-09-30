//! The outstanding-store (flow-control) laws: path-scaled / pooled / honest /
//! three-term caps, the δ-cap setpoint, their `[CCAP]`/`[SUMCAP]`/`[DCAP]`
//! gauges and the `sf=` saturation-filter gauge (paper §6).

use super::*;

/// Path-scaled outstanding-pool cap (env `RWM_STORE_PATHS`).
///
/// A multipath sender needs a pool that funds Σ per-path (BDP + one recovery
/// round of runway); a per-transfer constant (`RELIABLE_STORE_MAX`) does not
/// grow with the path count. The per-live-path knee is `pool`.
///
/// Returns `Some(cap)` when the path-scaled law applies — flag on, N ≥ 2
/// live paths, and a positive dynamic base (`pipe_sum` = Σ anchor-BDP, or
/// Σ Copa cwnd under the feed): cap = clamp(gain·N·pipe_sum, floor,
/// N·pool). Returns `None` when the caller must use the single-path law, so
/// N = 1 is bit-exact with the flag on.
///
/// This is the `RWM_SUM_CAP=0` arm, not the shipped law (paper §6.1): the
/// `×N` multiplies an already-summed base, so the value is quadratic in N at
/// symmetric inputs. Its shape is pinned by `net::tests::law_shape`
/// (`path_scaled_store_cap_value_is_quadratic_in_n_the_documented_defect`).
/// It is the `sum_cap = false` face of [`pooled_store_cap`], which carries both
/// forms in one expression; callers that want the shipped law call
/// [`pooled_store_cap`] with the resolved gate.
pub fn path_scaled_store_cap(
    on: bool,
    n_live: usize,
    pipe_sum: f64,
    gain: f64,
    floor: usize,
    pool: usize,
) -> Option<usize> {
    pooled_store_cap(on, false, false, 1.0, n_live, pipe_sum, gain, floor, pool)
}

/// The pooled outstanding cap, both forms in one expression — paper §6.1,
/// gates `RWM_SUM_CAP` and `RWM_DELTA_CAP` (both default on).
///
/// ```text
///   cap = clamp( gain · m · Σᵢ(max_bwᵢ · min_rttᵢ),  floor,  N · knee )
///
///     m = 1   when sum_cap = true    — the shipped law
///     m = N   when sum_cap = false   — the quadratic predecessor (A/B arm)
/// ```
///
/// `gain` is [`pool_value_multiplier`], which with `RWM_DELTA_CAP` on is the
/// CoDel-derived `1 + q(δ)`. The two gates are independent axes, so with
/// both on the law is
///
/// ```text
///   cap = clamp( (1 + q(δ)) · Σᵢ(max_bwᵢ · min_rttᵢ),  floor,  N · knee )
/// ```
///
/// Either gate at `=0` re-runs its predecessor; the four combinations are four
/// formulas taking one code path. The pool must fund Σ per-path (BDP + one
/// recovery round of runway), which is already linear in the path count
/// because the Σ is; `sum_cap = true` removes the second multiplication by N
/// and nothing else.
///
/// One function, not two, so a law and its displaced arm cannot drift; the
/// agreement test (`tests/formula_agreement.rs`) drives this function against
/// the paper's transcription.
///
/// Bit-identical at N = 1 by construction: the `n_live < 2` guard returns
/// `None` before the multiplier is read.
///
/// The clamp is not the law. The `=0` form pins at `Σ ≥ knee/gain` (the N
/// cancels: a path-count-free 1024 symbols); the shipped form pins at
/// `Σ ≥ N·knee/gain`, i.e. 1024 per path. An arm reading a high pin fraction
/// has measured the clamp, not the law (`docs/measurement-discipline.md`
/// rule 18) — the `[SUMCAP]` echo's `pin=` fraction reports it.
///
/// Shape pinned by `net::tests::law_shape::sum_store_cap_value_is_linear_in_n_the_template_applied`.
pub fn pooled_store_cap(
    on: bool,
    sum_cap: bool,
    delta_cap: bool,
    b_hint: f64,
    n_live: usize,
    pipe_sum: f64,
    gain: f64,
    floor: usize,
    pool: usize,
) -> Option<usize> {
    if !on || n_live < 2 || pipe_sum <= 0.0 {
        return None;
    }
    let ceiling = n_live.saturating_mul(pool).max(floor);
    Some(
        ((pooled_store_cap_unclamped(sum_cap, delta_cap, b_hint, n_live, pipe_sum, gain).ceil()
            as usize)
            .clamp(floor, ceiling)),
    )
}

/// The CoDel setpoint map `q(δ)` — paper §6.1, gate `RWM_DELTA_CAP`.
///
/// ```text
///   q(δ) = q_lo + (q_hi − q_lo) · ( clamp(b(δ), b_lo, b_hi) − b_lo ) / (b_hi − b_lo)
///
///     q_lo = 0.05   ← RFC 8289 (CoDel) §3.2, the conservative end of the band
///     q_hi = 0.10   ← RFC 8289 §3.2, the Kleinrock peak-power end
///     b_lo = b(Realtime) = ½ , b_hi = b(Bulk) = 2   ← [`delta_budget_b`]
/// ```
///
/// RFC 8289 §3.2 derives the permitted standing queue from Kleinrock power
/// maximisation — *"power is proportional to (1 + 2f − 1/3 f^2) / (1 + f)^2"* —
/// and states the result as *"the ideal range for the permitted standing
/// queue, or the target setpoint, is between 5% and 10% of the TCP
/// connection's RTT"*, with `0.05r` named as the conservative choice and
/// `0.1r` as the point that *"runs the risk of pushing shorter RTT connections
/// over the knee"*.
///
/// The derived quantity is the ratio, never a millisecond: CoDel's
/// `TARGET = 5 ms` is 5 % of its 100 ms `INTERVAL`.
///
/// Affine in the dial with no free parameter: both band endpoints are cited
/// and both dial endpoints are read from [`delta_budget_b`], so the
/// interpolation invents no constant. The `clamp` is the dial's own range
/// (`D(δ) = min(b·RTprop, 2·RTprop)` saturates at `b = 2`). No hint branch,
/// no threshold; `q` is continuous and strictly monotone in `b` (CLAUDE.md's
/// no-mode-switch invariant).
///
/// At the shipped endpoints this is `q(b) = (b + 1)/30`: Realtime 0.0500,
/// Auto 0.0667, Bulk 0.1000.
///
/// Shape pinned by `formula_agreement::published_codel_setpoint_equals_the_engine_map_and_spans_the_derived_band`.
pub fn codel_setpoint_q(b_hint: f64) -> f64 {
    let b_lo = delta_budget_b(ProtocolHint::Realtime);
    let b_hi = delta_budget_b(ProtocolHint::Bulk);
    let t = (b_hint.clamp(b_lo, b_hi) - b_lo) / (b_hi - b_lo);
    CODEL_TARGET_LO + (CODEL_TARGET_HI - CODEL_TARGET_LO) * t
}

/// RFC 8289 (CoDel) §3.2 — the conservative end of the derived setpoint band.
/// *"a more conservative target of 0.05r offers a good utilization vs. delay
/// trade-off while giving enough headroom to work well with a large variation
/// in real RTT."* Cited and derived (Kleinrock power), not fitted.
pub const CODEL_TARGET_LO: f64 = 0.05;
/// RFC 8289 (CoDel) §3.2 — the peak-power end of the derived setpoint band.
/// *"the ideal range … is between 5% and 10% of the TCP connection's RTT"*;
/// `0.1r` is the point that *"runs the risk of pushing shorter RTT connections
/// over the knee"*, i.e. the Kleinrock optimum itself.
pub const CODEL_TARGET_HI: f64 = 0.10;

/// The δ-priced pool multiplier — paper §6.1, gate `RWM_DELTA_CAP` (default on).
///
/// ```text
///   m(δ) = 1 + q(δ)      when delta_cap = true   — the shipped law (RFC 8289 §3.2)
///        = gain          when delta_cap = false  — the unprovenanced 2.0 (A/B arm)
/// ```
///
/// This is the whole of what `RWM_DELTA_CAP` changes: one factor, in the same
/// position, in the same expression. Written as a named factor rather than two
/// law functions for the reason [`pooled_store_cap`] gives.
///
/// As `q → 0` the pool becomes `Σᵢ bwᵢ·RTpropᵢ` — exactly one BDP per path and
/// zero standing queue — so the derived band is that null plus the power-point
/// allowance.
pub fn pool_value_multiplier(delta_cap: bool, b_hint: f64, gain: f64) -> f64 {
    if delta_cap {
        1.0 + codel_setpoint_q(b_hint)
    } else {
        gain
    }
}

/// The unclamped pooled cap — the law's own value, before either bound.
///
/// Bind fractions must be computed against the value the law asked for, not
/// the number that survived its bounds (`docs/measurement-discipline.md`
/// rule 17(b): a clamp may never be the only thing making a law sane).
/// Exposed so the `[SUMCAP]` gauge and the law-shape tests read the same
/// expression the engine does.
pub fn pooled_store_cap_unclamped(
    sum_cap: bool,
    delta_cap: bool,
    b_hint: f64,
    n_live: usize,
    pipe_sum: f64,
    gain: f64,
) -> f64 {
    // The count multiplier (`RWM_SUM_CAP`) and the value multiplier
    // (`RWM_DELTA_CAP`) are two independent axes of one expression: the first
    // picks how many times the already-summed Σ is counted, the second what
    // each unit of Σ is worth. Both are named factors, not branches over the
    // law (CLAUDE.md's no-mode-switch rule).
    let count_multiplier = if sum_cap { 1.0 } else { n_live as f64 };
    let value_multiplier = pool_value_multiplier(delta_cap, b_hint, gain);
    value_multiplier * count_multiplier * pipe_sum
}

/// Capacity-weighted shared outstanding pool — the pure pooled clamp the
/// `RWM_POOL_ANCHOR` law evaluates.
///
/// The path-scaled pool (`RWM_STORE_PATHS`) scales by path count, which at
/// asymmetric cells over-weights the slow path: a 1/5-rate path contributes
/// to the ceiling exactly like a full-rate path, granting unacked-frontier
/// depth the slow path cannot drain within its recovery round. Under
/// SACK-clocked release (ADR-0060) the pool bounds the unacked-frontier span,
/// so excess depth is span the cumulative frontier must resequence across the
/// slow path's stragglers.
///
/// This law scales by capacity instead: each live path earns depth for its
/// own pipe plus its own recovery round — the honest per-path cap
/// ([`honest_store_cap`]: cap_i = rate_i·(K_i·RTprop_i + (gain−1)·(R +
/// RTprop_i))) — summed as one shared pool, not per-path accounts, so
/// cross-path borrowing stays free (paper §6.1).
///
/// ```text
///   pool = clamp(Σ_i cap_i, floor, N·knee)
/// ```
///
/// Degenerates (unit-tested): symmetric N-path → N × the single-path term;
/// N = 1 → not engaged (`None`), the caller keeps the single-path law
/// bit-exactly; over-read anchors → the terms clamp at the N·knee ceiling ≡
/// the path-scaled law (so the law reads honestly only with the
/// `RWM_PLAIN_RS` send-interval sampler).
///
/// `terms` = the per-live-path honest cap (None until that path's anchor is
/// warm). Returns `None` — the caller falls back to the configured pooled
/// law — unless the gate is on, N ≥ 2, and every live path's anchor is warm
/// (a partial sum would under-provision the unwarm path's share).
pub fn capw_store_cap(
    on: bool,
    terms: &[Option<f64>],
    floor: usize,
    pool: usize,
) -> Option<usize> {
    if !on || terms.len() < 2 || terms.iter().any(|t| !matches!(t, Some(v) if *v > 0.0)) {
        return None;
    }
    let n = terms.len();
    let sum: f64 = terms.iter().map(|t| t.unwrap_or(0.0)).sum();
    let ceiling = n.saturating_mul(pool).max(floor);
    Some((sum.ceil() as usize).clamp(floor, ceiling))
}

/// Windowed-MIN echo-ratio tracker: K_i = the smallest observed
/// echoSRTT_i/RTprop_i over a ~2-half-window (~10 s, the min-RTT window
/// class) — the path's unloaded drain-clock ratio.
///
/// Why the min: the app-echo clock is store-dwell-inclusive (echoSRTT ≈
/// RTprop + own-queue dwell + ack-path/batching overhead), so any loaded
/// statistic of the ratio is self-referential — the store's own queue
/// inflates it, which inflates the cap, which deepens the queue. Own dwell
/// can only raise the ratio, so the window's smallest sample is the
/// ack-path/batching overhead with the least self-queue contamination. It is
/// a windowed statistic, not a latched constant (ADR-0061): the window rolls
/// over two half-window buckets, so a stale unloaded read expires.
#[derive(Debug)]
pub struct EchoRatioMin {
    cur: f64,
    prev: f64,
    start_us: u64,
    half_us: u64,
}

impl EchoRatioMin {
    pub fn new(half_us: u64) -> Self {
        Self { cur: f64::INFINITY, prev: f64::INFINITY, start_us: 0, half_us: half_us.max(1) }
    }
    /// Feed one ratio sample (echoSRTT/RTprop, clamped ≥ 1 — a smoothed
    /// echo transiently below the windowed-min floor is clock noise, not a
    /// sub-floor drain) and return the current windowed min.
    pub fn observe(&mut self, ratio: f64, now_us: u64) -> f64 {
        if self.start_us == 0 {
            self.start_us = now_us;
        }
        if now_us.saturating_sub(self.start_us) >= self.half_us {
            self.prev = self.cur;
            self.cur = f64::INFINITY;
            self.start_us = now_us;
        }
        if ratio.is_finite() && ratio > 0.0 {
            self.cur = self.cur.min(ratio.max(1.0));
        }
        self.k()
    }
    /// The current windowed-min ratio (1.0 before any sample).
    pub fn k(&self) -> f64 {
        let m = self.cur.min(self.prev);
        if m.is_finite() { m } else { 1.0 }
    }
    /// Feed one echoSRTT/RTprop observation from raw clocks and return the
    /// windowed min. Seed-identity guard: at the estimator's seed instant
    /// the smoothed echo is the windowed-min sample (bit-equal, ratio ≡ 1)
    /// — an artifact of shared seeding, not a drain-clock measurement, which
    /// would latch the min at 1.0 for a whole window. Samples where
    /// srtt − RTprop ≤ 5 µs are discarded, not clamped.
    pub fn observe_srtt_over_rtprop(
        &mut self,
        srtt: Duration,
        rtprop: Option<Duration>,
        now_us: u64,
    ) -> f64 {
        if let Some(rtp) = rtprop {
            let rtp_s = rtp.as_secs_f64();
            let srtt_s = srtt.as_secs_f64();
            if rtp_s > 0.0 && srtt_s - rtp_s > 5e-6 {
                return self.observe(srtt_s / rtp_s, now_us);
            }
        }
        self.k()
    }
}

/// The recovery engine's per-round latency ceiling, in seconds — the
/// hole-refresh / tail-sweep cadence clamp (`HOLE_NACK_REFRESH_MAX` =
/// `TAIL_SWEEP_MAX_US` = 100 ms). A stalled hole in plain window mode is
/// recovered by the SACK re-advertisement + tail-sweep engine, whose round
/// runs on this clock (2×SRTT clamped [25, 100] ms), not on RTprop — at a
/// short-RTprop cell a recovery round is many wire round trips. The honest
/// cap's runway term must fund it (see [`honest_store_cap`]); the clamp
/// ceiling is the honest worst round, which GE burst loss routinely reaches.
pub const HONEST_RECOVERY_ROUND_S: f64 = TAIL_SWEEP_MAX_US as f64 / 1e6;

/// Honest store cap: the outstanding cap derived on the honest plain-mode
/// anchor (`RWM_PLAIN_RS`), without the loaded echo clock whose
/// dwell→echo→cap feedback parks a slow path.
///
/// Derivation (Little's law on the retention store, on clocks the store's own
/// queue cannot inflate):
///
///   - a retained symbol's unloaded residence is K·RTprop (K = the
///     windowed-min echoSRTT/RTprop ratio, [`EchoRatioMin`]; the loaded echo
///     is used nowhere in this cap). Sustaining rate_i needs
///     rate_i·K_i·RTprop_i outstanding — the residence term;
///   - a hole strands the in-order frontier for one recovery round, which
///     runs on the recovery engine's clock: R = 100 ms
///     ([`HONEST_RECOVERY_ROUND_S`]) plus the retransmit flight RTprop_i.
///     Keeping the pipe fed across it needs (gain−1) rounds of runway — the
///     runway term (gain 2.0 = 1 round):
///
/// ```text
///   cap_i = rate_i·(K_i·RTprop_i + (gain−1)·(R + RTprop_i))
///         = anchor_i·(K_i + gain − 1) + rate_i·(gain−1)·R
/// ```
///
/// where `anchor_i` = BtlBw_i×RTprop_i (`copa_bdp_anchor`). The floor law
/// gain·anchor_i is the R = 0, K = 1 degenerate; the honest form strictly
/// widens it (K ≥ 1, R > 0), so it never shrinks a cap below that law. The
/// caller clamps to [floor, knee/store]; warm-up (no anchor) returns None and
/// the caller keeps its warm-up share. `R` is an open constant (paper §11.2).
pub fn honest_store_cap(
    anchor_bdp: Option<f64>,
    rate: Option<f64>,
    k_ratio: f64,
    gain: f64,
) -> Option<f64> {
    match (anchor_bdp, rate) {
        (Some(a), Some(r)) if a > 0.0 && r > 0.0 => {
            let runway_rounds = (gain - 1.0).max(0.0);
            Some(
                a * (k_ratio.max(1.0) + runway_rounds)
                    + r * runway_rounds * HONEST_RECOVERY_ROUND_S,
            )
        }
        _ => None,
    }
}

/// The `EchoRatioMin` half-window for the store-cap K_i state: ~10 s total
/// = two 5 s half-buckets (the min-RTT window class). Every consumer of the
/// honest per-path cap keys its windowed-min tracker on this one window.
pub const PERCAP_K_HALF_WINDOW_US: u64 = 5_000_000;

/// One honest per-path store-cap term — the law once, with its inputs chosen
/// at the call site:
///
///   1. fetch-or-create this path's windowed-min echo-ratio tracker
///      (`EchoRatioMin::new(PERCAP_K_HALF_WINDOW_US)`),
///   2. feed it this refresh's (srtt, RTprop) sample
///      (`observe_srtt_over_rtprop` — seed-identity guarded),
///   3. evaluate [`honest_store_cap`] on (anchor, rate, K_i, gain).
///
/// K_i is observed for every path this is called on, warm anchor or not —
/// the tracker is a clock statistic, not a cap statistic, and starving it on
/// cold-anchor ticks would make the window's min depend on anchor warmth.
/// Idempotent within a refresh tick.
///
/// `k_raw` (`RWM_HONEST_K`) is the path's raw-sample windowed-min ratio
/// (`PathState::k_raw`), substituted for the tracker's k in the unchanged law
/// — `k_raw.unwrap_or(k)`. The tracker is still observed on every call, so its
/// window state does not depend on the gate; `None` (default) is unchanged
/// behaviour.
pub fn honest_cap_term(
    ks: &mut std::collections::HashMap<u32, EchoRatioMin>,
    id: u32,
    srtt: Duration,
    rtprop: Option<Duration>,
    now_us: u64,
    anchor: Option<f64>,
    rate: Option<f64>,
    gain: f64,
    k_raw: Option<f64>,
) -> Option<f64> {
    let k = ks
        .entry(id)
        .or_insert_with(|| EchoRatioMin::new(PERCAP_K_HALF_WINDOW_US))
        .observe_srtt_over_rtprop(srtt, rtprop, now_us);
    honest_store_cap(anchor, rate, k_raw.unwrap_or(k), gain)
}

/// One path's inputs to [`honest_cap_term`], as read off `PathState` under
/// the scheduler lock. Exists so the collector below can be driven from a
/// component bench with no transport, no tokio and no scheduler
/// (`docs/measurement-discipline.md` rule 14).
#[derive(Debug, Clone, Copy)]
pub struct HonestCapPath {
    pub id: u32,
    /// The cap's RESIDENCE anchor (BtlBw_i·RTprop_i), `None` until warm.
    pub anchor: Option<f64>,
    /// The cap's RUNWAY rate (symbols/s), `None` until warm.
    pub rate: Option<f64>,
    pub srtt: Duration,
    pub rtprop: Option<Duration>,
    /// `RWM_HONEST_K`: the path's raw-sample windowed-min echo ratio
    /// (`PathState::k_raw`), `Some` only with the gate on. Consumers read
    /// `k_raw.unwrap_or(<tracker's k>)`; `None` (the default) leaves the
    /// tracker's k in place.
    pub k_raw: Option<f64>,
}

/// One collector for the honest per-path cap terms over a path set.
///
/// The pooled store-cap laws (`RWM_PLAIN_RS` + `RWM_HONEST_CAP`,
/// `RWM_POOL_ANCHOR`) differ only in (a) which rate source fills
/// [`HonestCapPath`] and (b) which path set the caller enumerates. Both are
/// the caller's choice; the loop is not.
///
/// A `None` slot is a path id that no longer resolves to a `PathState`
/// between the caller taking the id list and reading it: it contributes a
/// `None` term (so `capw_store_cap`'s all-warm requirement still refuses to
/// engage on a partial sum) and observes no clock sample.
pub fn honest_cap_terms(
    ks: &mut std::collections::HashMap<u32, EchoRatioMin>,
    paths: &[Option<HonestCapPath>],
    now_us: u64,
    gain: f64,
) -> Vec<Option<f64>> {
    paths
        .iter()
        .map(|slot| {
            slot.and_then(|p| {
                honest_cap_term(
                    ks, p.id, p.srtt, p.rtprop, now_us, p.anchor, p.rate, gain, p.k_raw,
                )
            })
        })
        .collect()
}

// ═══ The three-term outstanding-data limit (RWM_THREE_TERM, off) ═════════
//
// Paper §6.4 (the contract stall) and §10 (the composed cap, refuted). The
// outstanding-data limit is one scalar doing three jobs, each Little's law —
// quantity = rate × time — over signals the engine already measures, with no
// fitted coefficient:
//
//   limit = Σ_i rate_i·K_i·RTprop_i          TERM 1 — network window
//         + Σ_i rate_i·stall(δ, ρ, i)        TERM 2 — emission slack
//         + 2·rate_fast·skew                 TERM 3 — resequencing span
//
// Term 3 is identically zero at a single path — not by a path-count
// predicate or a gate: `skew = (max_i RTprop_i − min_i RTprop_i)/2` over a
// one-element set is zero because max and min are the same number. Every
// consumer sees one formula; the arithmetic supplies the topology.

/// The δ dial's deadline budget b(δ) at the protocol's named points
/// (paper §5.4): Realtime ½, Auto 1, Bulk 2 round trips.
///
/// These are points on a dial, never modes (CLAUDE.md). The hint names a δ
/// exactly once — [`delta_price`] — and b is the continuous
/// `b(δ) = clamp(2^(−½·log₁₀(δ/δ_Auto)), ½, 2)`
/// ([`raptorpath_math::span_horizon_b`], the same function the visualizer
/// reads). The three preset values are pinned bit-exactly by
/// `delta_budget_b_is_the_dial_not_a_mode`.
pub fn delta_budget_b(hint: ProtocolHint) -> f64 {
    delta_budget_b_of(delta_price(hint))
}

/// The one place a hint names a δ (paper §4.1).
///
/// `δ(hint) = δ_Auto / ζ(hint) ∈ {50 Realtime, 0.5 Auto, 0.005 Bulk}` — the
/// map every δ-priced law reads: the span horizon `b(δ)`, the rate mix's
/// bulkness `β(δ)`, the effective tail target `base·ζ(δ)`, and the contract's
/// α. A hint is a named point on this dial and nothing downstream may key on
/// which one it is.
///
/// `RWM_DELTA` (absent by default, `gates::delta_override`) replaces the map
/// with a number, so a run can stand between the presets. `RWM_COPA_DELTA`
/// still outranks it inside the congestion controller alone — see
/// [`RuntimeGates::delta`](crate::gates::RuntimeGates::delta) for the
/// precedence chain.
pub fn delta_price(hint: ProtocolHint) -> f64 {
    crate::gates::delta_override().unwrap_or_else(|| crate::scheduler::hint_delta_price(hint))
}

/// `b` at an arbitrary point on the δ dial — the law itself, with no hint in
/// sight. `delta_budget_b(hint) = delta_budget_b_of(delta_price(hint))`, and
/// the continuity gates sweep this.
pub fn delta_budget_b_of(delta_price: f64) -> f64 {
    raptorpath_math::span_horizon_b(delta_price)
}

/// The contract-declared frontier stall, in seconds — the time TERM 2 is
/// Little's law over (paper §6.4). Declared by (δ, ρ); no statistic of a
/// measured stall distribution is chosen, and no coefficient is fitted.
///
/// ```text
///   stall(δ, ρ) = (1 − ρ)·D(δ)  +  ρ·(9/8·srtt + srtt)
///                 └ shed-eligible ┘  └ retained: RFC 9002 §6.1.2 time
///                   share, bounded      threshold (kTimeThreshold = 9/8,
///                   by the span law's   empirically recommended) plus one
///                   own D(δ)            retransmit round trip ┘
/// ```
///
/// RFC 9002 recommends 9/8 empirically (*"Experience with QUIC shows that 9/8
/// works well"*) rather than deriving it, and RACK (RFC 8985) uses 5/4 for
/// the same job, so the constant is cited and tuned. RFC 9002 §6.1.2 defines
/// the threshold as a multiplier of a smoothed RTT; moving it onto RTprop
/// would make it a fitted coefficient on a new clock.
///
/// * the shed-eligible share (1 − ρ) cannot pin the in-order frontier
///   longer than the span law's own deadline `D(δ)` ([`shed_deadline_us`]):
///   past D a hole is retired rather than served;
/// * the retained share ρ is not sheddable by construction
///   (retain-until-acked), so it must be recovered: detection plus one
///   retransmit flight = 17/8·srtt.
///
/// Continuous in ρ with both terms always computed — the shipped rate law's
/// shape, not a mode bit (CLAUDE.md). Pinned across 21 values of ρ by
/// `three_term_law_is_arithmetic_and_continuous`.
///
/// `srtt_s` is the honest ack clock (see [`ThreeTermTerm`]), never the
/// store-dwell-inclusive app-echo RTT. The queue-free alternative (the same
/// stall on `min_rtt`) is refuted on wire-measured inputs:
/// `slack_bench.rs::the_queue_free_slack_clock_is_refuted_on_the_wire_measured_inputs`.
pub fn contract_stall_s(rho: f64, b_hint: f64, rtprop_s: f64, srtt_s: f64) -> f64 {
    let rho = rho.clamp(0.0, 1.0);
    let rtprop_s = rtprop_s.max(0.0);
    let srtt_s = srtt_s.max(0.0);
    // ONE D(δ): the shipped span-law deadline, reused rather than restated.
    let shed_term = shed_deadline_us(b_hint, (rtprop_s * 1e6) as u64) as f64 / 1e6;
    let retain_term = (9.0 / 8.0) * srtt_s + srtt_s;
    (1.0 - rho) * shed_term + rho * retain_term
}

/// One live path's inputs to the three-term law, as read off `PathState`
/// under the scheduler lock. Exists (like [`HonestCapPath`]) so the law can
/// be driven from a component bench with no transport, no tokio and no
/// scheduler (`docs/measurement-discipline.md` rule 14).
#[derive(Debug, Clone, Copy)]
pub struct ThreeTermPath {
    pub id: u32,
    /// The path's delivered-rate anchor (symbols/s), `None` until warm.
    pub rate: Option<f64>,
    pub srtt: Duration,
    pub rtprop: Option<Duration>,
    /// `RWM_HONEST_K`: the raw-sample windowed-min ratio
    /// (`PathState::k_raw`), `Some` only with the gate on — substituted for
    /// the tracker's k in the unchanged law (`k_raw.unwrap_or(k)`), including
    /// the `[3T]` window term. `None` (default) leaves the tracker's k.
    pub k_raw: Option<f64>,
}

/// One warm path's three-term term, with the honest clock already resolved.
///
/// `k` is the windowed-MIN echoSRTT/RTprop ratio ([`EchoRatioMin`], the same
/// tracker and window every honest cap uses), so `k·rtprop_s` is the ack
/// round trip the sender can honestly see: RTprop plus the standing
/// ack-path/batching overhead, not the store's own dwell. That choice closes
/// the dwell loop in one evaluation — see [`three_term_store_cap`].
#[derive(Debug, Clone, Copy)]
pub struct ThreeTermTerm {
    pub rate: f64,
    pub rtprop_s: f64,
    pub k: f64,
}

/// One collector for the three-term inputs over a path set — the
/// [`honest_cap_terms`] shape, and deliberately the same `EchoRatioMin` map
/// and window, so the engine has exactly one definition of K per path.
///
/// K is observed for every path this is called on, warm anchor or not (the
/// tracker is a clock statistic, not a cap statistic). Idempotent within a
/// refresh tick, so calling it beside [`honest_cap_terms`] at the same
/// `now_us` cannot perturb either.
pub fn three_term_terms(
    ks: &mut std::collections::HashMap<u32, EchoRatioMin>,
    paths: &[Option<ThreeTermPath>],
    now_us: u64,
) -> Vec<Option<ThreeTermTerm>> {
    paths
        .iter()
        .map(|slot| {
            let p = (*slot)?;
            let k = ks
                .entry(p.id)
                .or_insert_with(|| EchoRatioMin::new(PERCAP_K_HALF_WINDOW_US))
                .observe_srtt_over_rtprop(p.srtt, p.rtprop, now_us);
            // `RWM_HONEST_K`: the raw-sample floor when the gate supplies
            // one; the tracker above is still observed on every tick.
            let k = p.k_raw.unwrap_or(k);
            let rtprop_s = p.rtprop?.as_secs_f64();
            let rate = p.rate.filter(|r| *r > 0.0)?;
            if rtprop_s <= 0.0 {
                return None;
            }
            Some(ThreeTermTerm { rate, rtprop_s, k: k.max(1.0) })
        })
        .collect()
}

/// Term 3's geometry, computed once and reported in `[CCAP]`.
///
/// The field set is what distinguishes the shipped span form from the
/// alternative Σ form on an asymmetric geometry, with both anchors of an
/// absolute band:
///
/// * [`shipped`](Self::shipped) — `rate_fast · (RTprop_max − RTprop_min)`, the
///   law the engine runs.
/// * [`sigma`](Self::sigma) — `Σ_i rate_i · (RTprop_max − RTprop_i)`, the
///   alternative form. Not a candidate law and read by nothing: it exists so
///   the ratio of the two forms is measured rather than assumed.
/// * [`rate_fast`](Self::rate_fast) and [`spread_s`](Self::spread_s) — the two
///   anchors of the absolute band. A reading outside both forms' bands
///   falsifies the anchors rather than either formula, and only these fields
///   can show that.
///
/// No tie predicate: on the wire no two measured RTprops are ever exactly
/// equal, so counting legs at the minimum would read 1 at every geometry.
/// `sigma` is a plain sum over all legs with no equality test, so it degrades
/// smoothly and the ratio measures the effective count instead of asserting
/// it (no threshold, no branch, one formula).
///
/// Observation only: [`three_term_store_cap`] reads `shipped` and only
/// `shipped`, and this is the one site that computes the span geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpanForms {
    /// The shipped law: `2 · rate_fast · skew` = `rate_fast · spread`.
    pub shipped: f64,
    /// The alternative form, `Σ_i rate_i · (RTprop_max − RTprop_i)`.
    /// Reported, never consumed.
    pub sigma: f64,
    /// Rate of the single least-RTprop path (sym/s).
    pub rate_fast: f64,
    /// `RTprop_max − RTprop_min` in seconds.
    pub spread_s: f64,
}

/// Compute [`SpanForms`] over a WARM term set. `None` on an empty set or any
/// cold term, exactly as [`three_term_store_cap`] returns `None`, so the gauge
/// is fed on precisely the ticks the law engaged and `[CCAP]`'s `eng=` stays
/// the one liveness denominator.
pub fn span_forms(terms: &[Option<ThreeTermTerm>]) -> Option<SpanForms> {
    if terms.is_empty() || terms.iter().any(|t| t.is_none()) {
        return None;
    }
    let warm: Vec<ThreeTermTerm> = terms.iter().flatten().copied().collect();

    // `rate_fast` is the rate of the path that arrives first (least RTprop) —
    // the path whose symbols overtake the straggler. Over a one-element set
    // `rtp_max == rtp_min`, so `spread_s == 0` and both forms are identically
    // zero by arithmetic: there is no path-count predicate here either.
    let mut rtp_min = f64::INFINITY;
    let mut rtp_max = 0.0f64;
    let mut rate_fast = 0.0f64;
    for t in &warm {
        if t.rtprop_s < rtp_min {
            rtp_min = t.rtprop_s;
            rate_fast = t.rate;
        }
        rtp_max = rtp_max.max(t.rtprop_s);
    }
    let spread_s = rtp_max - rtp_min;
    // The skew is one-way and the store bounds a round trip of it, hence the
    // 2 — a definition boundary, kept visible rather than pre-multiplied away.
    let skew_s = spread_s / 2.0;
    let shipped = 2.0 * rate_fast * skew_s;
    let sigma = warm.iter().map(|t| t.rate * (rtp_max - t.rtprop_s)).sum();

    Some(SpanForms {
        shipped,
        sigma,
        rate_fast,
        spread_s,
    })
}

/// The composed three-term outstanding-data limit (`RWM_THREE_TERM`, off;
/// paper §10). Returns `Some((limit, window, slack, span))` — the three terms
/// alongside the total so the DIAG echo can attribute the limit — or `None`
/// when the law is off or any live path is still cold (a partial sum would
/// under-provision the unwarm path, as [`capw_store_cap`] refuses to).
///
/// ## TERM 1 — network window, `Σ_i rate_i · K_i · RTprop_i`
///
/// Little's law on the wire: the outstanding needed to keep path i busy for
/// one ack round trip. The time until a slot frees is the round trip the ACK
/// actually takes — `K·RTprop`, RTprop plus the receiver's standing ack-path
/// overhead; funding only `rate·RTprop` runs the sender dry for
/// `(K−1)·RTprop` of every round. `K_i` is the windowed-MIN
/// echoSRTT/RTprop, read on a clock the store cannot inflate (a loaded srtt
/// would let the cap inflate its own input). Agreement with the published
/// expression is a test: `tests/formula_agreement.rs`.
///
/// `K` is a windowed min, so it still carries whatever standing wire queue
/// survives its ≈10 s window.
///
/// ## TERM 2 — emission slack, `Σ_i rate_i · stall(δ, ρ, i)`
///
/// Little's law on the recovery plane: the backlog that keeps the wire fed
/// across one frontier freeze. The time is [`contract_stall_s`], declared by
/// (δ, ρ) rather than measured. Per path, because the stall runs on that
/// path's own clock; Σ rate_i is the total emission rate.
///
/// One evaluation is the fixed point of the dwell loop
/// `S → dwell → srtt → patience → stall → S`: its gain through this law is
/// zero because `K_i` is a windowed min and the store's dwell can only add to
/// an echo sample. The residual: the gain is zero only while one un-dwelled
/// sample remains in the ≈10 s window (`PERCAP_K_HALF_WINDOW_US`×2); a dwell
/// sustained beyond it would re-open the loop. Pinned by
/// `three_term_law_closes_the_dwell_loop_in_one_evaluation`.
///
/// ## TERM 3 — resequencing span, `2 · rate_fast · skew`
///
/// The sender retains a symbol until it is acked, so while one slow-path
/// symbol is unacked the fast path's symbols pile into the same unacked span.
/// `skew` is the one-way inter-path skew; the store bounds a round trip of
/// it, hence the 2 (a definition boundary, not a coefficient). The engine
/// cannot measure a one-way delay, so `skew` is read off the round-trip
/// spread, `(max RTprop − min RTprop)/2`, written out so the 2 stays visible.
///
/// Over one path `max RTprop = min RTprop`, so the term is `0` by arithmetic;
/// there is no path-count predicate in this function. Asserted by
/// `three_term_span_vanishes_continuously_as_skew_goes_to_zero`.
///
/// ## The clamp
///
/// `[floor, WIN_STORE_MAX]`. The ceiling is the memory bound
/// ([`WIN_STORE_MAX`]), not part of the law: the per-path 2048 knee the
/// pooled laws clamp to is an empirical fit, and this law derives what that
/// knee approximates.
pub fn three_term_store_cap(
    on: bool,
    terms: &[Option<ThreeTermTerm>],
    rho: f64,
    b_hint: f64,
    floor: usize,
) -> Option<(usize, f64, f64, f64)> {
    if !on || terms.is_empty() || terms.iter().any(|t| t.is_none()) {
        return None;
    }
    let warm: Vec<ThreeTermTerm> = terms.iter().flatten().copied().collect();

    // TERM 1 and TERM 2 — both always computed, for every path.
    let mut window = 0.0f64;
    let mut slack = 0.0f64;
    for t in &warm {
        let srtt_s = t.k.max(1.0) * t.rtprop_s; // the honest ack clock
        window += t.rate * srtt_s;
        slack += t.rate * contract_stall_s(rho, b_hint, t.rtprop_s, srtt_s);
    }

    // TERM 3 — always computed too, and identically 0 over a one-element
    // set because `rtp_max == rtp_min` there. Delegated to `span_forms` so
    // the law and the `[CCAP]` span gauge read one computation of the
    // geometry; only `shipped` is consumed here.
    let span = span_forms(terms)?.shipped;

    let total = window + slack + span;
    let limit = (total.ceil() as usize).clamp(floor.min(WIN_STORE_MAX), WIN_STORE_MAX);
    Some((limit, window, slack, span))
}

/// The composed law's per-run `[CCAP]` readout (paper §10).
///
/// Split from its emission so the always-on pins assert the string an L1
/// parser will scrape, and so the two teardown arms share one renderer.
///
/// * `eng=<engaged>/<refreshes>` — mechanism liveness
///   (`docs/measurement-discipline.md` rule 1). `eng=0/N` with
///   `RWM_COMPOSED_CAP=1` in the `[GATES]` echo is a warm-up failure (some
///   live path was cold at every refresh), not a null result.
/// * `cap=` — the realized mean cap.
/// * `mem=` / `floor=` — the bind fractions of the two bounds: `WIN_STORE_MAX`,
///   a memory bound stated outside the law, and `store_cap_floor`. A composed
///   run with `mem` above zero means the memory bound has become the law.
/// * `brake=<closed>/<ticks>` — the late-stage brake's own liveness:
///   `brake=0/N` distinguishes "the brake never bound" from "the brake was
///   never armed".
///
/// The span block (`span=` … `spread_us=`) is [`SpanForms`] averaged over the
/// engaged refreshes:
///
/// * `span=` — the shipped `rate_fast·spread`; reads 0 at a symmetric cell.
/// * `span_sigma=` — the alternative `Σ bwᵢ(RTT_max − RTTᵢ)`, reported so the
///   discriminating ratio is measured, not assumed. No engine path reads it.
/// * `span_ratio=` — `Σ span_sigma / Σ span`, equal to the ratio of the two
///   means because both share the `engaged` denominator. `0.000` when `Σ span`
///   is 0 (every symmetric cell, where the ratio is undefined); a parser must
///   read `span=` first.
/// * `rate_fast=` (sym/s) and `spread_us=` (µs) — the two absolute anchors.
pub fn ccap_report_line(
    refreshes: u64,
    engaged: u64,
    at_mem: u64,
    at_floor: u64,
    cap_sum: f64,
    brake_ticks: u64,
    brake_closed: u64,
    floor: usize,
    span_sum: f64,
    span_sigma_sum: f64,
    rate_fast_sum: f64,
    spread_s_sum: f64,
) -> String {
    let frac = |n: u64, d: u64| if d == 0 { 0.0 } else { n as f64 / d as f64 };
    // Means over engaged refreshes — the ticks on which the law produced a
    // span at all. `eng=0/N` therefore renders a well-defined 0.0 block and
    // never a NaN, the same rule the bind fractions follow.
    let mean = |s: f64| if engaged == 0 { 0.0 } else { s / engaged as f64 };
    format!(
        "[CCAP] eng={}/{} cap={:.1} mem={:.4} floor={:.4} floor_val={} brake={}/{} \
         brake_frac={:.4} span={:.1} span_sigma={:.1} span_ratio={:.3} \
         rate_fast={:.1} spread_us={:.1}",
        engaged,
        refreshes,
        if refreshes == 0 { 0.0 } else { cap_sum / refreshes as f64 },
        frac(at_mem, engaged),
        frac(at_floor, engaged),
        floor,
        brake_closed,
        brake_ticks,
        frac(brake_closed, brake_ticks),
        mean(span_sum),
        mean(span_sigma_sum),
        if span_sum > 0.0 {
            span_sigma_sum / span_sum
        } else {
            0.0
        },
        mean(rate_fast_sum),
        mean(spread_s_sum) * 1e6,
    )
}

/// The `[SUMCAP]` line — the `×N` deletion's engagement echo (paper §6.1).
///
/// `RWM_SUM_CAP` changes one factor in a clamped expression, so there are
/// three different ways for an arm to read "no difference", and a mean cap
/// cannot tell them apart (`docs/measurement-discipline.md` rule 18):
///
/// * `eng=0/N` — the law never engaged (every refresh fell through to the
///   single-path or boot branch): a warm-up failure, not a result.
/// * `chg=0/M` — engaged, and the two arms produced the same integer at every
///   refresh: the gate was live and arithmetically inert — the clamp still
///   governs.
/// * `chg=M/M` with `pin` high — the corrected value is itself pinned at
///   `N·knee`: a defect finding about the ceiling, and no verdict about the
///   multiplier may be drawn from it.
///
/// Fields: `eng=<engaged>/<refreshes>`; `chg=<differed>/<engaged>` with its
/// fraction; `pin=` the fraction of engaged refreshes whose realized cap was
/// the `N·knee` ceiling; `floor=` the same for the derived floor; `cap=` the
/// realized mean and `ask=` the mean unclamped value the law asked for — the
/// pair that shows whether the bound or the law is answering (rule 17(b)).
pub fn sumcap_report_line(
    refreshes: u64,
    engaged: u64,
    differed: u64,
    at_pin: u64,
    at_floor: u64,
    cap_sum: f64,
    ask_sum: f64,
    on: bool,
) -> String {
    let frac = |n: u64, d: u64| if d == 0 { 0.0 } else { n as f64 / d as f64 };
    let mean = |s: f64, d: u64| if d == 0 { 0.0 } else { s / d as f64 };
    format!(
        "[SUMCAP] on={} eng={}/{} chg={}/{} chg_frac={:.4} pin={:.4} floor={:.4} \
         cap={:.1} ask={:.1}",
        on as u8,
        engaged,
        refreshes,
        differed,
        engaged,
        frac(differed, engaged),
        frac(at_pin, engaged),
        frac(at_floor, engaged),
        mean(cap_sum, engaged),
        mean(ask_sum, engaged),
    )
}

/// The `[SUMCAP]` tally, fed at every pooled-law refresh on both arms.
///
/// It records the counterfactual as well as the realized value — at each
/// refresh it computes what the other arm would have produced from the same
/// inputs — so "did this gate change anything" is answerable from one run.
pub(crate) struct SumCapGauge {
    refreshes: u64,
    engaged: u64,
    differed: u64,
    at_pin: u64,
    at_floor: u64,
    cap_sum: f64,
    ask_sum: f64,
    /// `RWM_SUM_CAP` — which arm this run is, and whether the line is emitted.
    on: bool,
    /// The clamp bounds the law is evaluated with, held here so the
    /// counterfactual is bounded identically to the realized value (two
    /// multipliers under two different clamps is not an A/B of the
    /// multiplier).
    floor: usize,
    pool: usize,
    /// The value-multiplier axis this run is on (`RWM_DELTA_CAP`) and the dial
    /// point it reads, so the count-multiplier counterfactual is computed
    /// under the same value multiplier — one axis varied at a time.
    delta_cap: bool,
    b_hint: f64,
}

impl SumCapGauge {
    pub(crate) fn new(
        on: bool,
        floor: usize,
        pool: usize,
        delta_cap: bool,
        b_hint: f64,
    ) -> Self {
        Self {
            refreshes: 0,
            engaged: 0,
            differed: 0,
            at_pin: 0,
            at_floor: 0,
            cap_sum: 0.0,
            ask_sum: 0.0,
            on,
            floor,
            pool,
            delta_cap,
            b_hint,
        }
    }

    /// Record one pooled-law refresh. Observation only — nothing here is read
    /// by any engine decision, and on the default arm the only cost is a
    /// handful of flops off the scheduler lock.
    pub(crate) fn record(&mut self, on: bool, n_live: usize, pipe_sum: f64, gain: f64, realized: usize) {
        self.refreshes += 1;
        self.engaged += 1;
        self.cap_sum += realized as f64;
        self.ask_sum +=
            pooled_store_cap_unclamped(on, self.delta_cap, self.b_hint, n_live, pipe_sum, gain);
        // The counterfactual: the same expression with the count multiplier
        // flipped, under the same bounds and value multiplier. It goes
        // through `pooled_store_cap`, so it cannot drift from what the engine
        // evaluates.
        let other = pooled_store_cap(
            true,
            !on,
            self.delta_cap,
            self.b_hint,
            n_live,
            pipe_sum,
            gain,
            self.floor,
            self.pool,
        );
        if other != Some(realized) {
            self.differed += 1;
        }
        let ceiling = n_live.saturating_mul(self.pool).max(self.floor);
        if realized >= ceiling {
            self.at_pin += 1;
        }
        if realized <= self.floor {
            self.at_floor += 1;
        }
    }

    /// The `[SUMCAP]` line this gauge would emit right now. Split out so the
    /// format pins and the destructor share one renderer.
    pub(crate) fn sumcap_line(&self) -> String {
        sumcap_report_line(
            self.refreshes,
            self.engaged,
            self.differed,
            self.at_pin,
            self.at_floor,
            self.cap_sum,
            self.ask_sum,
            self.on,
        )
    }
}

impl Drop for SumCapGauge {
    fn drop(&mut self) {
        // Emitted only on the ON arm, so the default's output is unchanged
        // (the control arm's pin fraction is carried by the L1 parsers'
        // `occcap_p50` gauge).
        if self.on {
            crate::readout!("{}", self.sumcap_line());
        }
        // The one-sided-clamp witness, once per sender teardown and on every
        // arm — it scores the shipped estimator, not any gate here. Silent
        // when the estimator was never fed.
        if crate::scheduler::LCW_LOSS_MASS.load(std::sync::atomic::Ordering::Relaxed) > 0
            || crate::scheduler::LCW_OVER_N.load(std::sync::atomic::Ordering::Relaxed) > 0
        {
            crate::readout!("{}", crate::scheduler::lcw_report_line());
        }
    }
}

/// The `[DCAP]` engagement echo for the δ-priced pool multiplier — paper
/// §6.1, gate `RWM_DELTA_CAP`.
///
/// Same convention as [`sumcap_report_line`], with the counterfactual keyed to
/// the other axis: at every engaged refresh it recomputes what the fixed
/// `gain` would have produced from the same Σ under the same bounds and the
/// same count multiplier, so "did the derived multiplier change anything" is
/// answerable from one run.
///
/// `q=` carries the resolved CoDel setpoint at this tunnel's dial point, and
/// `b=` the dial number it was mapped from — together they show the dial
/// routed (`docs/measurement-discipline.md` rule 1), not only that the env
/// var was read.
///
/// How a null reads: `eng=0/0` is never armed (the pooled seat was not
/// reached — expected at N = 1, where the law short-circuits before any
/// multiplier); `eng=N/N` with `chg_frac=0.0000` cannot happen while
/// `gain != 1+q` and is an instrument failure. A high `pin=` means the arm
/// measured the `N·knee` ceiling, not the law (rule 18).
pub fn dcap_report_line(
    refreshes: u64,
    engaged: u64,
    differed: u64,
    at_pin: u64,
    at_floor: u64,
    cap_sum: f64,
    ask_sum: f64,
    q: f64,
    b_hint: f64,
    on: bool,
) -> String {
    let frac = |n: u64, d: u64| if d == 0 { 0.0 } else { n as f64 / d as f64 };
    let mean = |s: f64, d: u64| if d == 0 { 0.0 } else { s / d as f64 };
    format!(
        "[DCAP] on={} eng={}/{} chg={}/{} chg_frac={:.4} pin={:.4} floor={:.4} \
         cap={:.1} ask={:.1} q={:.6} b={:.4}",
        on as u8,
        engaged,
        refreshes,
        differed,
        engaged,
        frac(differed, engaged),
        frac(at_pin, engaged),
        frac(at_floor, engaged),
        mean(cap_sum, engaged),
        mean(ask_sum, engaged),
        q,
        b_hint,
    )
}

/// The `[DCAP]` tally, fed at every pooled-law refresh on BOTH arms.
pub(crate) struct DeltaCapGauge {
    refreshes: u64,
    engaged: u64,
    differed: u64,
    at_pin: u64,
    at_floor: u64,
    cap_sum: f64,
    ask_sum: f64,
    /// `RWM_DELTA_CAP` — which arm this run is, and whether the line is emitted.
    on: bool,
    /// The bounds and the count-multiplier axis, held so the counterfactual is
    /// evaluated identically except for the one factor under test.
    floor: usize,
    pool: usize,
    sum_cap: bool,
    b_hint: f64,
}

impl DeltaCapGauge {
    pub(crate) fn new(
        on: bool,
        floor: usize,
        pool: usize,
        sum_cap: bool,
        b_hint: f64,
    ) -> Self {
        Self {
            refreshes: 0,
            engaged: 0,
            differed: 0,
            at_pin: 0,
            at_floor: 0,
            cap_sum: 0.0,
            ask_sum: 0.0,
            on,
            floor,
            pool,
            sum_cap,
            b_hint,
        }
    }

    /// Record one pooled-law refresh. Observation only.
    pub(crate) fn record(&mut self, n_live: usize, pipe_sum: f64, gain: f64, realized: usize) {
        self.refreshes += 1;
        self.engaged += 1;
        self.cap_sum += realized as f64;
        self.ask_sum += pooled_store_cap_unclamped(
            self.sum_cap,
            self.on,
            self.b_hint,
            n_live,
            pipe_sum,
            gain,
        );
        // The counterfactual: the same expression with the value multiplier
        // flipped, under the same bounds and the same count multiplier.
        let other = pooled_store_cap(
            true,
            self.sum_cap,
            !self.on,
            self.b_hint,
            n_live,
            pipe_sum,
            gain,
            self.floor,
            self.pool,
        );
        if other != Some(realized) {
            self.differed += 1;
        }
        let ceiling = n_live.saturating_mul(self.pool).max(self.floor);
        if realized >= ceiling {
            self.at_pin += 1;
        }
        if realized <= self.floor {
            self.at_floor += 1;
        }
    }

    /// The `[DCAP]` line this gauge would emit right now.
    pub(crate) fn dcap_line(&self) -> String {
        dcap_report_line(
            self.refreshes,
            self.engaged,
            self.differed,
            self.at_pin,
            self.at_floor,
            self.cap_sum,
            self.ask_sum,
            codel_setpoint_q(self.b_hint),
            self.b_hint,
            self.on,
        )
    }
}

impl Drop for DeltaCapGauge {
    fn drop(&mut self) {
        if self.on {
            crate::readout!("{}", self.dcap_line());
        }
    }
}

// ── The saturation-filter gauge (`sf=`) ──────────────────────────────────
//
// The component-level instrument for the store-cap phase
// (`docs/measurement-discipline.md` rule 14). It answers a population
// question: at the dyn-cap refresh instants, how often does `active_paths()`
// (cwnd − in_flight > 0) return fewer paths than `live_paths()`, and how
// often none at all?
//
// A tick where n_active < n_live is a tick where the pooled cap's Σ-anchor
// base was summed over a strict subset of the paths whose count (`n_live`)
// multiplies it; a tick where n_active = 0 < n_live is a tick where the cap
// fell all the way to `store_boot_cap`.
pub(crate) static STORE_CAP_SF_TICKS: AtomicU64 = AtomicU64::new(0);
pub(crate) static STORE_CAP_SF_LIVE: AtomicU64 = AtomicU64::new(0);
pub(crate) static STORE_CAP_SF_ACTIVE: AtomicU64 = AtomicU64::new(0);
pub(crate) static STORE_CAP_SF_SHORT: AtomicU64 = AtomicU64::new(0);
pub(crate) static STORE_CAP_SF_ZERO: AtomicU64 = AtomicU64::new(0);

/// Record one dyn-cap refresh tick's (n_live, n_active) into the `sf=`
/// gauge. Observation only.
pub(crate) fn store_cap_sf_record(n_live: usize, n_active: usize) {
    STORE_CAP_SF_TICKS.fetch_add(1, Ordering::Relaxed);
    STORE_CAP_SF_LIVE.fetch_add(n_live as u64, Ordering::Relaxed);
    STORE_CAP_SF_ACTIVE.fetch_add(n_active as u64, Ordering::Relaxed);
    if n_active < n_live {
        STORE_CAP_SF_SHORT.fetch_add(1, Ordering::Relaxed);
    }
    if n_active == 0 && n_live > 0 {
        STORE_CAP_SF_ZERO.fetch_add(1, Ordering::Relaxed);
    }
}

/// `sf=` gauge readout: (ticks, Σ n_live, Σ n_active, short ticks, zero
/// ticks). "short" = `active_paths()` returned fewer than `live_paths()`;
/// "zero" = it returned none while paths were live.
pub fn store_cap_sf_gauge() -> (u64, u64, u64, u64, u64) {
    (
        STORE_CAP_SF_TICKS.load(Ordering::Relaxed),
        STORE_CAP_SF_LIVE.load(Ordering::Relaxed),
        STORE_CAP_SF_ACTIVE.load(Ordering::Relaxed),
        STORE_CAP_SF_SHORT.load(Ordering::Relaxed),
        STORE_CAP_SF_ZERO.load(Ordering::Relaxed),
    )
}

/// Zero the `sf=` gauge (component bench / test isolation).
pub fn store_cap_sf_reset() {
    for c in [
        &STORE_CAP_SF_TICKS,
        &STORE_CAP_SF_LIVE,
        &STORE_CAP_SF_ACTIVE,
        &STORE_CAP_SF_SHORT,
        &STORE_CAP_SF_ZERO,
    ] {
        c.store(0, Ordering::Relaxed);
    }
}

/// The retention/memory ceiling of the outstanding store — 4096 × ~1.2 KB
/// ≈ 5 MB; the memory clamp of the composed/three-term laws.
pub const WIN_STORE_MAX: usize = 4096;
