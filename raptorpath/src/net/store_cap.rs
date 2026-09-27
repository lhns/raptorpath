//! The outstanding-store (flow-control) laws: path-scaled / pooled / honest /
//! three-term caps, the δ-cap setpoint, their `[CCAP]`/`[SUMCAP]`/`[DCAP]`
//! gauges and the `sf=` saturation-filter gauge. Moved verbatim out of
//! `net/mod.rs` (cleanup Stage 3).

use super::*;

/// Path-scaled outstanding-pool cap (task #84, env `RWM_STORE_PATHS`).
///
/// The plain-reliable OUTSTANDING ceiling was a per-transfer constant
/// (`RELIABLE_STORE_MAX` = 1024): the dynamic delay cap latches at it on
/// fast paths (the legacy anchor over-reads), so a MULTIPATH sender is
/// store-starved — the pool that must fund Σ per-path (BDP + one recovery
/// round of runway) does not grow with the path count. Measured same-binary
/// at L1 (see the decl site): the knee is ≈2048 outstanding symbols PER
/// LIVE PATH at both C7 and C8, deeper pools re-enter the bufferbloat
/// collapse.
///
/// Returns `Some(cap)` when the path-scaled law applies — flag on, N ≥ 2
/// live paths, and a positive dynamic base (`pipe_sum` = Σ anchor-BDP, or
/// Σ Copa cwnd under the feed): cap = clamp(gain·N·pipe_sum, floor,
/// N·pool). Returns `None` when the caller must use the legacy single-path
/// law — so N = 1 is bit-exact legacy even with the flag ON.
///
/// **NO LONGER THE SHIPPED FACE — this is the `RWM_SUM_CAP=0` ARM** (paper
/// §16.64, 2026-08-19). The `×N` applies a path-count multiplier to an
/// ALREADY-SUMMED base, so the value is QUADRATIC in N at symmetric inputs
/// where the derivation (`Σᵢ gain·anchorᵢ`) is linear; the `N·knee` ceiling is
/// what had been measured on every dual cell. ADR-0070 finding 2 recorded the
/// multiplier's provenance as ABSENT, the ladder battery ran the A/B that had
/// never been run, and `RWM_SUM_CAP` FLIPPED DEFAULT ON — so the expression
/// below is now the DISPLACED arm, kept re-runnable with its provenance
/// intact and with no deprecation warning (ADR-0066 register row). Its shape
/// is still PINNED by `net::tests::law_shape`
/// (`path_scaled_store_cap_value_is_quadratic_in_n_the_documented_defect`),
/// which now pins the `=0` arm rather than the default.
///
/// Mechanically: this function is the `sum_cap = false` face of
/// [`pooled_store_cap`], which carries both forms in ONE expression so the two
/// cannot drift. Callers that want the SHIPPED law must call
/// [`pooled_store_cap`] with the resolved gate, not this function.
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

/// **THE POOLED OUTSTANDING CAP, BOTH FORMS, ONE EXPRESSION** — paper
/// §16.60/§16.64, gate `RWM_SUM_CAP` (**DEFAULT ON since 2026-08-19**),
/// ADR-0070 finding 2.
///
/// ```text
///   cap = clamp( gain · m · Σᵢ(max_bwᵢ · min_rttᵢ),  floor,  N · knee )
///
///     m = 1   when sum_cap = true    — the SHIPPED law (default, `RWM_SUM_CAP=1`)
///     m = N   when sum_cap = false   — the DISPLACED quadratic (`RWM_SUM_CAP=0`,
///                                      the re-runnable A/B arm)
/// ```
///
/// **THE SHIPPED FORMULA AFTER BOTH 2026-08-19 FLIPS.** `gain` above is
/// [`pool_value_multiplier`], which since the second flip (`RWM_DELTA_CAP` ON,
/// paper §16.71) is the CoDel-derived `1 + q(δ)` rather than the `gain = 2.0`
/// fossil. Composing the two gates — they are INDEPENDENT axes and both now
/// resolve ON — the pooled law this function computes on an unset engine is
///
/// ```text
///   cap = clamp( (1 + q(δ)) · Σᵢ(max_bwᵢ · min_rttᵢ),  floor,  N · knee )
/// ```
///
/// with the `N·knee` ceiling **measured INERT at both scoreable duals**
/// (`pin` = 0.0000 at c7 and c8, goal-gate "Candidates Battery — RESULTS")
/// rather than assumed inert, and with `gain` absent from the shipped VALUE
/// altogether. Either gate set to `=0` re-runs its own displaced arm, and the
/// four combinations remain four distinct formulas taking one code path.
///
/// **What the correction is, and what it is not.** The birth commit's own
/// diagnosis names the quantity the pool must fund: *"Σ per-path (BDP + one
/// recovery round of runway)"*, i.e. `Σᵢ(gain·anchorᵢ) = gain·Σ`. That is
/// **already linear in the path count, because the Σ is**. The shipped
/// expression multiplies the already-summed base by the count a SECOND time,
/// and no line of the birth commit, the doc comment, the decl site or the
/// ledger explains it — ADR-0070 finding 2 records the multiplier's provenance
/// as ABSENT and finds it contradicted by name in three places in this
/// repository. `sum_cap = true` deletes that second multiplication and
/// **nothing else**: gain, floor, ceiling, Σ-set and estimator are untouched,
/// and no constant is introduced.
///
/// **Why one function and not two.** The whole class of defect this repairs is
/// a formula nobody read; a second implementation of a law under review is how
/// a paper and its code diverge inside one commit (§16.57 measured exactly
/// that). The multiplier is therefore a VALUE in one expression, both arms
/// always take the same code path, and the agreement test
/// (`tests/formula_agreement.rs`) drives this function against the paper's
/// transcription rather than against a sibling.
///
/// **Bit-identical at N = 1 BY CONSTRUCTION**: the `n_live < 2` guard returns
/// `None` before the multiplier is read at all, so every single-path cell is
/// byte-identical on both arms without needing a measurement to say so.
///
/// **The clamp is not the law, and the correction moves its crossover.** The
/// `=0` form pins at `gain·N·Σ ≥ N·knee ⟺ Σ ≥ knee/gain` — **the N cancels**,
/// so the pin threshold is a path-count-free constant (1024 symbols), which is
/// why 121/126 dual reps on the old default read exactly `2·knee`. The shipped
/// form pins at `gain·Σ ≥ N·knee ⟺ Σ ≥ N·knee/gain`, i.e. **1024 PER PATH**, so
/// the ceiling's dependence on the path count is restored to the value as well.
/// An arm reading a high pin fraction has measured the clamp and not the law,
/// and MEASUREMENT DISCIPLINE 18 requires it be reported as such — which is
/// what the `[SUMCAP]` echo's `pin=` fraction exists for.
///
/// **Measured on the wire before it shipped** (goal-gate "Ladder Battery —
/// RESULTS", 2026-08-19, rung N): interior at both scoreable duals with `pin`
/// med 0.000, `eng` med 1.000 and `chg_frac` 1.000 — the multiplier's deletion
/// changes the computed cap on 100 % of engaged refreshes, so it is not
/// cosmetic — against a control pinned at exactly 4096. Goodput moved UP at
/// the pre-registered risk cell. The band clause on the c8 cap MAGNITUDE was
/// falsified as written (`cap` 2308.7 vs [2416, 3624]) because the wire
/// presented Σ = 1154.3, below both published anchors: a finding about Σ, with
/// `cap ≡ ask` and `pin = 0` showing the law itself landing faithfully.
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

/// **THE CoDel SETPOINT MAP `q(δ)`** — paper §16.67, gate `RWM_DELTA_CAP`.
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
/// **The derived quantity is the RATIO, never a millisecond.** CoDel's shipped
/// `TARGET = 5 ms` is 5 % of its 100 ms `INTERVAL`; porting the millisecond
/// ports nothing (§16.65's FOLKLORE CORRECTION).
///
/// **AFFINE IN THE DIAL, WITH NO FREE PARAMETER.** Both band endpoints are
/// cited and both dial endpoints are READ from [`delta_budget_b`] rather than
/// restated, so linear interpolation between them has zero degrees of freedom
/// and invents no constant. The `clamp` is the DIAL'S OWN RANGE — the shipped
/// `D(δ) = min(b·RTprop, 2·RTprop)` already saturates at `b = 2`, so this
/// inherits exactly the saturation the span law has always had. There is no
/// `if hint ==`, no threshold, and `q` is continuous and strictly monotone in
/// `b` on the whole interval (CLAUDE.md's no-mode-switch invariant, read off
/// the formula itself).
///
/// At the shipped endpoints this collapses to `q(b) = (b + 1)/30`, an
/// algebraic consequence of the four anchors rather than a fifth constant:
/// Realtime 0.0500, Auto 0.0667, Bulk 0.1000.
///
/// Shape pinned by `net::tests::law_shape::codel_setpoint_spans_the_derived_band_continuously`.
pub fn codel_setpoint_q(b_hint: f64) -> f64 {
    let b_lo = delta_budget_b(ProtocolHint::Realtime);
    let b_hi = delta_budget_b(ProtocolHint::Bulk);
    let t = (b_hint.clamp(b_lo, b_hi) - b_lo) / (b_hi - b_lo);
    CODEL_TARGET_LO + (CODEL_TARGET_HI - CODEL_TARGET_LO) * t
}

/// RFC 8289 (CoDel) §3.2 — the conservative end of the derived setpoint band.
/// *"a more conservative target of 0.05r offers a good utilization vs. delay
/// trade-off while giving enough headroom to work well with a large variation
/// in real RTT."* CITED AND DERIVED (Kleinrock power), not fitted.
pub const CODEL_TARGET_LO: f64 = 0.05;
/// RFC 8289 (CoDel) §3.2 — the peak-power end of the derived setpoint band.
/// *"the ideal range … is between 5% and 10% of the TCP connection's RTT"*;
/// `0.1r` is the point that *"runs the risk of pushing shorter RTT connections
/// over the knee"*, i.e. the Kleinrock optimum itself.
pub const CODEL_TARGET_HI: f64 = 0.10;

/// **THE δ-PRICED POOL MULTIPLIER** — paper §16.67/§16.70/§16.71, gate
/// `RWM_DELTA_CAP` (**DEFAULT ON since 2026-08-19**).
///
/// ```text
///   m(δ) = 1 + q(δ)      when delta_cap = true   — the SHIPPED law (default,
///                                                  DERIVED, RFC 8289 §3.2)
///        = gain          when delta_cap = false  — the DISPLACED FOSSIL (2.0,
///                                                  the re-runnable A/B arm)
/// ```
///
/// This is the whole of what `RWM_DELTA_CAP` changes: **one factor, in the
/// same position, in the same expression**. ADR-0070 finding 3 records the
/// pool `gain = 2.0` as a FOSSIL (one BDP of pipe plus one BDP of recovery
/// runway, argued in prose at `ac3bc9d`, swept once at one cell under a
/// different CC family), and §16.65 downgraded even its BBR citation to
/// *"right value, wrong citation"*. `1 + q(δ)` is its derived successor.
///
/// Written as a named factor rather than as two law functions, for the reason
/// [`pooled_store_cap`] gives: a second implementation of a law under review
/// is how a paper and its code diverge inside one commit.
///
/// **The reduction that matters.** As `q → 0` the pool becomes
/// `Σᵢ bwᵢ·RTpropᵢ` — exactly one BDP per path, ZERO standing queue — which is
/// ADR-0071 candidate **(d) ZERO**, the same answer §16.65's newsvendor
/// cross-domain analysis reached independently. The derived band is therefore
/// (d) PLUS the power-point allowance, not a rival to it.
///
/// **Measured on the wire before it shipped** (goal-gate "Candidates Battery —
/// RESULTS", 2026-08-19, rung D): D-LAT six of six — goodput PARITY at every
/// dual on both seeds with `q_p50` down 10–200 ms at every one — interior with
/// the ceiling provably inert at c7 and c8 (`pin` = 0.0000), `eng = 0/0` at the
/// singles, and c8's paired dead wall shortened (p ≈ 0.011). The honest bounds
/// are carried at the gate's decl in `gates.rs`: parity is not a win, c8L is a
/// PARTIAL delivery at `pin` = 0.23, the probe disagrees in sign on one of six
/// rows, and the c8/seed-7 abort class is arm-correlated. `RWM_DELTA_CAP=0`
/// re-runs the displaced fossil with no deprecation warning (ADR-0066 row).
pub fn pool_value_multiplier(delta_cap: bool, b_hint: f64, gain: f64) -> f64 {
    if delta_cap {
        1.0 + codel_setpoint_q(b_hint)
    } else {
        gain
    }
}

/// The UNCLAMPED pooled cap — the law's own value, before either bound.
///
/// MEASUREMENT DISCIPLINE 17(b): *"a clamp may never be the only thing making
/// a law sane"*, and the corollary the `[CCAP]` echo already applies at
/// runtime — the bind fractions must be computed against the value the law
/// ASKED for, not against the number that survived its bounds. Exposed so the
/// `[SUMCAP]` gauge and the law-shape tests read the same expression the
/// engine does instead of re-deriving it.
pub fn pooled_store_cap_unclamped(
    sum_cap: bool,
    delta_cap: bool,
    b_hint: f64,
    n_live: usize,
    pipe_sum: f64,
    gain: f64,
) -> f64 {
    // The count multiplier (`RWM_SUM_CAP`) and the VALUE multiplier
    // (`RWM_DELTA_CAP`) are two INDEPENDENT axes of one expression: the first
    // picks how many times the already-summed Σ is counted, the second picks
    // what each unit of Σ is worth. Both are named factors rather than
    // branches of an `if` over the law — the shape CLAUDE.md's no-mode-switch
    // rule asks for wherever two behaviours must both exist.
    let count_multiplier = if sum_cap { 1.0 } else { n_live as f64 };
    let value_multiplier = pool_value_multiplier(delta_cap, b_hint, gain);
    value_multiplier * count_multiplier * pipe_sum
}

/// Capacity-weighted SHARED outstanding pool — the pure pooled clamp the
/// `RWM_POOL_ANCHOR` law evaluates. (Its own A/B arm, `RWM_STORE_CAPW`, the
/// ADR-0058 "c8 WATCH" follow-up, was refuted as a sizing answer — goal-gate
/// "C8-Aware Pool Law" — and removed; the function survives as the
/// pool-anchor seat's clamp.)
///
/// The path-scaled pool (`RWM_STORE_PATHS`) scales by path COUNT:
/// cap = clamp(gain·N·Σpipe, floor, N·knee), which at asymmetric cells
/// over-weights the slow path — a 1/5-rate path contributes ×N to the
/// ceiling exactly like a full-rate path, so the pool grants unacked-frontier
/// depth the slow path cannot drain within its recovery round. Under
/// SACK-clocked release (ADR-0060) the pool bounds the UNACKED-FRONTIER SPAN
/// (outstanding = retained − SACK-released), so excess depth = the span the
/// cumulative frontier must resequence across the slow path's stragglers —
/// the measured c8 WATCH (stack 0.72–0.76×Σ vs legacy-1024 0.85–0.87×Σ).
///
/// The law here scales by CAPACITY instead: each live path earns depth for
/// its OWN pipe plus its own recovery round — the honest per-path cap law
/// ([`honest_store_cap`]: cap_i = rate_i·(K_i·RTprop_i + (gain−1)·(R +
/// RTprop_i))) — SUMMED AS ONE SHARED POOL, not per-path accounts: admission
/// still gates on the pooled total, so cross-path borrowing stays free
/// (ADR-0058's pooled-vindicated verdict kept; only the SIZING law changes).
///
///   pool = clamp(Σ_i cap_i, floor, N·knee)
///
/// Degenerates (unit-tested): symmetric N-path → N × the single-path term
/// (≈ N×(single pool) — c7 preserved); N = 1 → not engaged (`None`), the
/// caller keeps the legacy law bit-exactly; over-read anchors → the terms
/// clamp at the N·knee ceiling ≡ the path-scaled law (which is why the law
/// reads honestly only with the `RWM_PLAIN_RS` send-interval sampler).
///
/// `terms` = the per-live-path honest cap (None until that path's anchor is
/// warm). Returns `None` — the caller falls back to the CONFIGURED pooled
/// law (path-scaled / legacy) — unless the gate is on, N ≥ 2, and EVERY live
/// path's anchor is warm (a partial sum would under-provision the unwarm
/// path's share of the shared pool).
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

/// Windowed-MIN echo-ratio tracker (feat/percap-honest-cap): K_i = the
/// smallest observed echoSRTT_i/RTprop_i over a ~2-half-window (~10 s, the
/// min-RTT window class) — the path's UNLOADED drain-clock ratio.
///
/// Why the MIN: the app-echo clock is store-dwell-inclusive (echoSRTT ≈
/// RTprop + own-queue dwell + ack-path/batching overhead), so any loaded
/// statistic of the ratio is self-referential — the store's own queue
/// inflates it, which inflates the cap, which deepens the queue (the
/// measured c8 parking spiral, GUARD RESULTS). The windowed MIN is
/// self-queue-PROOF: own dwell can only raise the ratio, so the smallest
/// sample in the window is the honest ack-path/batching overhead with the
/// least self-queue contamination. Anchor-hygiene rule 3 applies: it is a
/// windowed statistic, not a latched constant — the window rolls (two
/// half-window buckets), so a stale unloaded read expires and the ratio
/// re-measures.
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
    /// windowed min. SEED-IDENTITY GUARD: at the estimator's seed instant
    /// the smoothed echo IS the windowed-min sample (bit-equal, ratio ≡ 1)
    /// — an artifact of shared seeding, not a drain-clock measurement.
    /// Feeding it would latch the windowed min at 1.0 for a whole window
    /// (measured at the L1 smoke: khr pinned 1.00 while rtt/rtp read
    /// 16/8 ms). Samples where srtt − RTprop ≤ 5 µs are DISCARDED, not
    /// clamped — no measurement, no sample.
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
/// runs on THIS clock (2×SRTT clamped [25, 100] ms), not on RTprop — at a
/// short-RTprop cell (c2: RTprop ≈ 8 ms) a recovery round is ~12× the wire
/// round trip. The honest cap's runway term must fund it (see
/// [`honest_store_cap`]); using the CLAMP CEILING is the honest worst
/// round: GE burst loss routinely drives the engine to it (retransmits
/// re-lost in the same bad state, sweep-cadence refills — MEASURED: sweeps
/// every ~140 ms live at the c2 cell).
pub const HONEST_RECOVERY_ROUND_S: f64 = TAIL_SWEEP_MAX_US as f64 / 1e6;

/// Honest store cap (feat/percap-honest-cap, the GUARD-RESULTS residual (i)
/// fix): the outstanding cap derived on the HONEST plain-mode anchor
/// (`RWM_PLAIN_RS`), replacing both the knee-clamp fallback that the legacy
/// anchor over-read forced AND the loaded-echo-clock cap law whose
/// dwell→echo→cap feedback parked the c8 slow path.
///
/// Derivation (Little's law on the retention store, decomposed on honest
/// clocks; every term measured or a named engine constant — none
/// inflatable by the store's own queue):
///
///   - a retained symbol's UNLOADED residence is K·RTprop (K = the
///     windowed-min echoSRTT/RTprop ratio, [`EchoRatioMin`] — the measured
///     ack-path/batching overhead; the loaded echo is self-referential and
///     is used NOWHERE in this cap). Sustaining rate_i needs
///     rate_i·K_i·RTprop_i outstanding — the RESIDENCE term;
///   - a hole strands the in-order frontier for one recovery round, which
///     runs on the RECOVERY engine's clock, not the wire's: the SACK
///     re-advertisement / tail-sweep cadence bound R = 100 ms
///     ([`HONEST_RECOVERY_ROUND_S`]) plus the retransmit flight RTprop_i.
///     Keeping the pipe fed across it needs (gain−1) rounds of runway —
///     the RUNWAY term (gain 2.0 = 1 round, the same pipe+runway
///     decomposition as the redirect guard):
///
///   cap_i = rate_i·(K_i·RTprop_i + (gain−1)·(R + RTprop_i))
///         = anchor_i·(K_i + gain − 1) + rate_i·(gain−1)·R
///
/// where `anchor_i` = BtlBw_i×RTprop_i (`copa_bdp_anchor`). The legacy
/// floor law gain·anchor_i is the R = 0, K = 1 degenerate — the honest form
/// strictly widens it (K ≥ 1, R > 0), so honest anchors can never shrink a
/// cap below the legacy law: the headroom the anchor over-read supplied by
/// accident (~12× at c2's 8-ms RTprop — the sc2 −20% datum, "Anchor
/// Hygiene" battery (b)) is now supplied EXPLICITLY from the engine's own
/// recovery cadence + the measured echo-ratio. Cross-checks against
/// independently measured good operating points: sc2 → 10.4k·(K·8ms +
/// 108ms) ≈ 1250+ → latches the legacy-proven 1024 store; c8-slow →
/// ~2k·(K·60ms + 160ms) ≈ 470–500 ≈ the guard session's measured good pin
/// (508, dwell 0.26 s); the c8 knee-parking regime (2048, dwell ≈ 1 s)
/// is unreachable for a c3-class rate. Caller clamps to the principled
/// [floor, knee/store] bounds; warm-up (no anchor) returns None and the
/// caller keeps the legacy warm-up share.
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
/// = two 5 s half-buckets (the min-RTT window class). Module-level since
/// the 2026-08-09 store-cap de-triplication — every consumer of the honest
/// per-path cap must key its windowed-min tracker on the SAME window, and
/// four independently transcribed `EchoRatioMin::new(...)` sites are
/// exactly how that stops being true.
pub const PERCAP_K_HALF_WINDOW_US: u64 = 5_000_000;

/// ONE honest per-path store-cap term (the de-triplication, 2026-08-09).
///
/// The body this replaces was transcribed FOUR times inside
/// `run_window_sender`'s dynamic-store-cap block — `capw_terms`, the inline
/// `hsum` loop, `pa_terms`, and the percap account loop — each of them
/// spelling out the same three steps:
///
///   1. fetch-or-create this path's windowed-min echo-ratio tracker
///      (`EchoRatioMin::new(PERCAP_K_HALF_WINDOW_US)`),
///   2. feed it this refresh's (srtt, RTprop) sample
///      (`observe_srtt_over_rtprop` — seed-identity guarded),
///   3. evaluate [`honest_store_cap`] on (anchor, rate, K_i, gain).
///
/// The copies had DRIFTED in their inputs (which rate source, which path
/// set) while claiming to be the same law. The law is here, once; the
/// inputs stay at the call site, where they are a documented choice rather
/// than a transcription accident.
///
/// K_i is observed for EVERY path this is called on, warm anchor or not —
/// the tracker is a clock statistic, not a cap statistic, and starving it
/// on cold-anchor ticks would make the window's min depend on anchor
/// warmth. (Idempotent within a refresh tick: two calls at the same
/// `now_us` with the same sample leave identical state.)
/// goal-gate "Honest Inputs" (`RWM_HONEST_K`): `k_raw` is the path's
/// RAW-sample windowed-min ratio when the gate is on (`PathState::k_raw`),
/// substituted for the smoothed-at-refresh tracker's k in the UNCHANGED law
/// — `k_raw.unwrap_or(k_legacy)`, one formula. The legacy tracker is STILL
/// observed on every call (its window state must not depend on the gate, so
/// the A/B isolates the K source and nothing else); `None` (default) is
/// byte-identical legacy.
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
/// component bench with no transport, no tokio and no scheduler — the
/// MEASUREMENT DISCIPLINE 14 instrument for the store-cap phase.
#[derive(Debug, Clone, Copy)]
pub struct HonestCapPath {
    pub id: u32,
    /// The cap's RESIDENCE anchor (BtlBw_i·RTprop_i), `None` until warm.
    pub anchor: Option<f64>,
    /// The cap's RUNWAY rate (symbols/s), `None` until warm.
    pub rate: Option<f64>,
    pub srtt: Duration,
    pub rtprop: Option<Duration>,
    /// goal-gate "Honest Inputs" (`RWM_HONEST_K`): the path's RAW-sample
    /// windowed-min echo ratio (`PathState::k_raw`), `Some` only with the
    /// gate on. Consumers read `k_raw.unwrap_or(<legacy tracker's k>)` —
    /// ONE formula whose K input the gate re-sources from the raw sample
    /// stream; `None` (the shipped default) is byte-identical legacy.
    pub k_raw: Option<f64>,
}

/// ONE collector for the honest per-path cap terms over a path set.
///
/// The pooled store-cap laws (`RWM_PLAIN_RS` + `RWM_HONEST_CAP`,
/// `RWM_POOL_ANCHOR`; formerly also the removed `RWM_STORE_CAPW`) differ ONLY in (a) which rate
/// source fills [`HonestCapPath`] and (b) which path set the caller
/// enumerates. Both are the caller's choice; the loop is not.
///
/// A `None` slot is a path id that no longer resolves to a `PathState`
/// between the caller taking the id list and reading it: it contributes a
/// `None` TERM (so `capw_store_cap`'s all-warm requirement still refuses to
/// engage on a partial sum) and observes NO clock sample — bit-identical to
/// the `sched.path(id).and_then(..)` shape every call site used before.
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

// ═══ THE THREE-TERM OUTSTANDING-DATA LIMIT (RWM_THREE_TERM) ══════════════
//
// Goal-gate "Three-Term Law" (2026-08-10), paper §16.43 + §16.44. The
// outstanding-data limit is ONE scalar doing THREE jobs, only one of which
// was ever derived. All three are Little's law — quantity = rate × time —
// over signals the engine already measures, and NONE contains a fitted
// coefficient. Two of the three make OPPOSITE demands of the knob, which is
// why every change this month split by topology.
//
//   limit = Σ_i rate_i·K_i·RTprop_i          TERM 1 — NETWORK WINDOW
//         + Σ_i rate_i·stall(δ, ρ, i)        TERM 2 — EMISSION SLACK
//         + 2·rate_fast·skew                 TERM 3 — RESEQUENCING SPAN
//
// THE PROPERTY THIS EXISTS FOR. Term 3 is identically ZERO at a single path
// — not by an `if n_live == 1`, not by a topology predicate, and not by a
// gate: `skew = (max_i RTprop_i − min_i RTprop_i)/2` over a ONE-ELEMENT set
// is zero because max and min are the same number. That is how the
// `active_paths()` vs `live_paths()` branch dies. Every consumer of the
// limit sees ONE formula; the arithmetic supplies the topology.

/// The δ dial's deadline budget b(δ) at the protocol's NAMED POINTS
/// (paper §8.8 / §16.20.3): Realtime ½, Auto 1, Bulk 2 round trips.
///
/// These are POINTS ON A DIAL, never modes (CLAUDE.md). The only law b
/// enters — `D(δ) = min(b·RTprop, 2·RTprop)`, [`shed_deadline_us`] — is
/// continuous and monotone in it, and every consumer treats b as a plain
/// number. Extracted here (2026-08-10) from the span-law site in
/// `emit_source.rs`, which had the same three-arm map transcribed inline
/// and would otherwise have been transcribed a second time by the law
/// below — the `honest_cap_term` de-triplication lesson applied early.
///
/// **THE THREE-ARM MATCH IS GONE (2026-09-08, §16.81).** The hint
/// names a δ exactly once — [`delta_price`] — and b is the paper's own
/// continuous `b(δ) = clamp(2^(−½·log₁₀(δ/δ_Auto)), ½, 2)` evaluated there
/// ([`raptorpath_math::span_horizon_b`], the same function the visualizer
/// reads). The three shipped numbers are unchanged and pinned BIT-EXACTLY by
/// `delta_budget_b_is_the_dial_not_a_mode`; what changed is the SHAPE, which
/// is the whole defect: the values agreed with the paper at the three presets
/// and the engine had no function of δ at all, so nothing between the presets
/// could be evaluated, measured, or tested for continuity.
pub fn delta_budget_b(hint: ProtocolHint) -> f64 {
    delta_budget_b_of(delta_price(hint))
}

/// THE ONE PLACE A HINT NAMES A δ (paper §12.4, §16.81).
///
/// `δ(hint) = δ_Auto / ζ(hint) ∈ {50 Realtime, 0.5 Auto, 0.005 Bulk}` — the
/// map the Copa scheduler already carried, lifted to the seat every δ-priced
/// law reads: the span horizon `b(δ)`, the rate mix's bulkness `β(δ)`, the
/// effective tail target `base·ζ(δ)`, and the contract's α. A hint is a NAMED
/// POINT on this dial and nothing downstream may key on which one it is.
///
/// `RWM_DELTA` (ABSENT by default, `gates::delta_override`) replaces the map
/// with a NUMBER, so a run can stand between the presets. `RWM_COPA_DELTA`
/// still outranks it inside the congestion controller alone — see
/// [`RuntimeGates::delta`](crate::gates::RuntimeGates::delta) for the
/// precedence chain, stated once.
pub fn delta_price(hint: ProtocolHint) -> f64 {
    crate::gates::delta_override().unwrap_or_else(|| crate::scheduler::hint_delta_price(hint))
}

/// `b` at an ARBITRARY point on the δ dial — the law itself, with no hint in
/// sight. `delta_budget_b(hint) = delta_budget_b_of(delta_price(hint))`, and
/// the continuity gates sweep THIS.
pub fn delta_budget_b_of(delta_price: f64) -> f64 {
    raptorpath_math::span_horizon_b(delta_price)
}

/// The CONTRACT-declared frontier stall, in SECONDS — the time TERM 2 is
/// Little's law over. Declared by (δ, ρ); no statistic of a measured stall
/// distribution is chosen, and no coefficient is fitted.
///
/// ```text
///   stall(δ, ρ) = (1 − ρ)·D(δ)  +  ρ·(9/8·srtt + srtt)
///                 └ shed-eligible ┘  └ retained: RFC 9002 §6.1.2 time
///                   share, bounded      threshold (kTimeThreshold = 9/8,
///                   by the span law's   empirically recommended) plus ONE
///                   own D(δ)            retransmit round trip ┘
/// ```
///
/// PROVENANCE of the 9/8, corrected 2026-08-19 (literature cross-check item
/// 6(d), `docs/research/literature-crosscheck.md`; paper §16.65): RFC 9002
/// RECOMMENDS 9/8 empirically — *"Experience with QUIC shows that 9/8 works
/// well"* — it does not derive it, and RACK (RFC 8985) uses 5/4 for the same
/// job. So the constant is cited AND tuned: the 17/8 below inherits a tuned
/// constant, and earlier descriptions of it as "cited, not magic"/"not
/// fitted" overstated the source.
///
/// * the shed-eligible share (1 − ρ) cannot pin the in-order frontier
///   longer than the span law's own deadline `D(δ)` ([`shed_deadline_us`]):
///   past D a hole is RETIRED rather than served;
/// * the retained share ρ is not sheddable by construction
///   (RETAIN-UNTIL-ACKED), so it must actually be RECOVERED: detection plus
///   one retransmit flight = 17/8·srtt.
///
/// CONTINUOUS in ρ with BOTH terms always computed — the shipped rate law's
/// shape, not a mode bit (CLAUDE.md). Pinned across 21 values of ρ by
/// `three_term_law_is_arithmetic_and_continuous`.
///
/// `srtt_s` is the HONEST ack clock (see [`ThreeTermTerm`]), never the
/// store-dwell-inclusive app-echo RTT — §16.44 route B.
///
/// **THE QUEUE-FREE CLOCK WAS TRIED AND REFUTED (§16.58, 2026-08-18).** The
/// standing charge against this term is that its clock is `K·RTprop` and `K`
/// carries the standing WIRE queue the cap itself stood up (the store DWELL
/// is already excluded — a dwell can only ADD to an echo sample, so it can
/// never lower the windowed MIN; that is route B and it is closed). The
/// candidate — the same stall on the loop-free `min_rtt` — was written as a
/// formula first and then evaluated on §16.57's 833 wire evaluations: at c8
/// the ask must shed **47 %** to clear `WIN_STORE_MAX` and the queue-free
/// clock sheds **1.7 %** (7 778 → 7 642); at c8L it must shed 84 % and sheds
/// 21 %. Both cells STILL PIN. Nothing changed here, and no coefficient was
/// invented to close the gap.
///
/// Two findings came out of writing it down and both constrain any successor:
/// (i) the magnitude is the `17/8`, not the clock — 3.125 BDP at ρ = 1 means
/// ≈2.1 BDP of standing queue, and either δ bounds that or the composition is
/// wrong for a retain-until-acked scope; (ii) RFC 9002 §6.1.2 defines the
/// time threshold as `kTimeThreshold × max(smoothed_rtt, latest_rtt)`, so the
/// `9/8` below is cited ONLY as a multiplier of a SMOOTHED RTT — moving it
/// onto RTprop turns it into a fitted coefficient on a new clock, which is a
/// provenance REGRESSION and is not licensed. (Softened 2026-08-19, cross-check
/// item 6(d): the RFC's own *"Implementations MAY experiment with absolute
/// thresholds, thresholds from previous connections, adaptive thresholds"*
/// clause anticipates such a move — it just blesses no value, so a mover
/// still owes its own derivation; and 9/8 itself is an empirical
/// recommendation, *"Experience with QUIC shows that 9/8 works well"*, with
/// RACK on 5/4.) Pinned by
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
/// scheduler — MEASUREMENT DISCIPLINE 14.
#[derive(Debug, Clone, Copy)]
pub struct ThreeTermPath {
    pub id: u32,
    /// The path's delivered-rate anchor (symbols/s), `None` until warm.
    pub rate: Option<f64>,
    pub srtt: Duration,
    pub rtprop: Option<Duration>,
    /// goal-gate "Honest Inputs" (`RWM_HONEST_K`): the RAW-sample
    /// windowed-min ratio (`PathState::k_raw`), `Some` only with the gate
    /// on — substituted for the refresh-clock tracker's k in the unchanged
    /// law (`k_raw.unwrap_or(legacy)`), which is what carries the jit25 fix
    /// into the `[3T]` window term. `None` (default) = legacy verbatim.
    pub k_raw: Option<f64>,
}

/// One WARM path's three-term term, with the honest clock already resolved.
///
/// `k` is the windowed-MIN echoSRTT/RTprop ratio ([`EchoRatioMin`], the same
/// tracker and the same `PERCAP_K_HALF_WINDOW_US` window every honest cap
/// uses), so `k·rtprop_s` is the ack round trip the sender can HONESTLY see:
/// RTprop plus the standing ack-path/batching overhead, and NOT the store's
/// own dwell. That choice is what closes §16.44's route-B loop in ONE
/// evaluation — see [`three_term_store_cap`].
#[derive(Debug, Clone, Copy)]
pub struct ThreeTermTerm {
    pub rate: f64,
    pub rtprop_s: f64,
    pub k: f64,
}

/// ONE collector for the three-term inputs over a path set — the
/// [`honest_cap_terms`] shape, and deliberately the SAME `EchoRatioMin` map
/// and window, so the engine has exactly one definition of K per path.
///
/// K is observed for EVERY path this is called on, warm anchor or not (the
/// tracker is a CLOCK statistic, not a cap statistic; starving it on
/// cold-anchor ticks would make the window's min depend on anchor warmth).
/// Idempotent within a refresh tick, so calling it beside
/// [`honest_cap_terms`] at the same `now_us` cannot perturb either.
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
            // "Honest Inputs" (`RWM_HONEST_K`): the raw-sample floor when
            // the gate supplies one; the tracker above is still observed on
            // every tick (window state gate-independent).
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

/// The composed three-term outstanding-data limit. Returns
/// `Some((limit, window, slack, span))` — the three terms are returned
/// alongside the total so the DIAG echo can ATTRIBUTE the limit rather than
/// merely report it — or `None` when the law is off or any live path is
/// still cold (a partial sum would under-provision the unwarm path, exactly
/// as [`capw_store_cap`] refuses to engage on one).
///
/// ## TERM 1 — NETWORK WINDOW, `Σ_i rate_i · K_i · RTprop_i`
///
/// Little's law on the wire: the outstanding needed to keep path i busy for
/// one ack round trip. PROVENANCE of the clock: the bench (§16.43/§16.44)
/// writes this term as `rate·srtt` with `srtt = RTprop + wireQ`, and the
/// engine's shipped window laws write it as `rate·RTprop` — the two differ
/// by the standing queue. Neither is used verbatim here, because the ENGINE
/// cannot read a loaded srtt into a cap without the cap inflating its own
/// input (the dwell→echo→cap feedback that parked the c8 slow path;
/// [`honest_store_cap`]). `K_i` is the windowed-MIN echoSRTT/RTprop, which
/// IS the bench's `srtt/RTprop` read on a clock the store cannot inflate —
/// so `rate·K·RTprop` is the bench's own quantity, honestly measured. The
/// engine-vs-bench adjudication is recorded in the goal-gate section, not
/// smoothed over: on the bench's own axes `K = 1 + wireQ/RTprop` exactly.
///
/// **THE CODE-vs-PAPER ADJUDICATION (2026-08-18, `fix/cap-law-cluster`).**
/// §16.56 originally PUBLISHED this term as `rateᵢ·RTpropᵢ` while this
/// function has always computed `rateᵢ·Kᵢ·RTpropᵢ`. §16.57 measured the
/// divergence on the wire (`K` = 1.04–1.505, so the window term ran 4–50 %
/// above the paper) and recorded it as the FORMULA-FIRST rule's first
/// violation. **It is adjudicated in favour of THIS CODE and the paper now
/// carries the dated amendment.** The reason is the term's JOB, not its
/// ancestry: term 1 funds the outstanding that keeps a slot free when the
/// sender wants one, and the time until a slot frees is the round trip the
/// ACK actually takes — `K·RTprop`, RTprop plus the receiver's standing
/// ack-path overhead. `RTprop` alone is the DATA's flight on an empty wire;
/// funding to `rate·RTprop` runs the sender dry for `(K−1)·RTprop` of every
/// round. §16.42 measured that overhead as first-class (`[CTLD]` 1.96 → ≈1.0
/// per data message at c1, worth +12.7/+13.0 % goodput), so pricing it at
/// zero here would not be conservative — it would be wrong. It also makes
/// TERM 1 and TERM 2 share ONE clock, which the published block did not.
///
/// AGREEMENT WITH THE PUBLISHED EXPRESSION IS NOW A TEST, not a reading:
/// `tests/formula_agreement.rs` drives this function against an independent
/// transcription of §16.56 over the dials' named points and the wire's own
/// `K` range. A future edit that reverts this term to `rate·RTprop` without
/// re-amending the paper fails there.
///
/// NOT settled by the above, and deliberately: `K` is a windowed MIN, so it
/// still carries whatever STANDING wire queue survives its ≈10 s window
/// (c8 1.04 vs c8L 1.505 on ONE geometry at 8× the transfer). §16.58 wrote
/// the queue-free replacement as a formula and REFUTED it — at c8 the law
/// must shed 47 % of its ask to clear the memory bound and the queue-free
/// clock sheds 1.7 %. The magnitude lives in TERM 2's `17/8`, not in the
/// clock, and that is the named successor.
///
/// ## TERM 2 — EMISSION SLACK, `Σ_i rate_i · stall(δ, ρ, i)`
///
/// Little's law on the RECOVERY PLANE: the backlog that keeps the wire fed
/// across ONE frontier freeze. The time is [`contract_stall_s`], DECLARED
/// by (δ, ρ) rather than measured, so there is no distribution statistic to
/// choose. Per path, because the stall runs on that path's own clock; the
/// sum's rate factor is Σ rate_i = the total emission rate, which is what
/// the wire actually asks for.
///
/// **The closed dwell loop (§16.44 route B), and why ONE evaluation is the
/// fixed point.** The open-loop form — feeding the store-dwell-inclusive
/// app-echo RTT into the stall — is what produced §16.43's ×13.5 tail, and
/// route B showed that tail was the cost of running the store at 3× its own
/// derived size rather than a property of the clock. The loop is
/// `S → dwell → srtt → patience → stall → S`. Its gain through THIS law is
/// identically ZERO, because `K_i` is a windowed MIN: the store's dwell can
/// only ADD to an echo sample, so it can never lower the window's minimum,
/// and the minimum is the only statistic the law reads. §16.44 measured
/// exactly this — on the wire (dwell-excluding) clock `closed_loop_dwell`
/// terminates at iteration 2, i.e. converged after one update, "the honest
/// clock is the loop-OPENING argument". So the iteration bound here is ONE,
/// and it is one because the map is constant in its own output, not because
/// the iteration was truncated. The residual is stated and BOUNDED rather
/// than described: K's window is `PERCAP_K_HALF_WINDOW_US`×2 ≈ 10 s, so the
/// gain is zero only while ONE un-dwelled sample remains in window; a dwell
/// sustained beyond 10 s would re-open the loop. Pinned by
/// `three_term_law_closes_the_dwell_loop_in_one_evaluation`.
///
/// ## TERM 3 — RESEQUENCING SPAN, `2 · rate_fast · skew`
///
/// The sender must RETAIN a symbol until it is acked, so while one
/// slow-path symbol is unacked the fast path's symbols pile into the same
/// unacked span. `skew` is the ONE-WAY inter-path skew; the store bounds a
/// ROUND TRIP of it, hence the 2. That factor is a DEFINITION BOUNDARY, not
/// a coefficient, and it was IDENTIFIED rather than fitted: §16.43's PS5
/// measured the span as linear in skew with zero intercept and a slope of
/// exactly the TOTAL emission rate, ratio 2.00 ± 0.03 in 18 of 18 non-zero
/// cells across ×13 in rate and ×40 in skew. The engine cannot measure a
/// one-way delay, so `skew` is read off the round-trip spread it CAN
/// measure, `(max RTprop − min RTprop)/2`, and `2·skew` collapses back to
/// the round-trip difference — written out in that form on purpose, so the
/// 2 stays visible instead of being pre-multiplied away.
///
/// **The topology branch, deleted.** Over ONE path `max RTprop = min
/// RTprop`, so `skew = 0` and the term is `0` by arithmetic. There is no
/// path-count predicate anywhere in this function, and adding one would be
/// the defect this law exists to remove. Asserted by
/// `three_term_span_vanishes_continuously_as_skew_goes_to_zero`.
///
/// ## The clamp
///
/// `[floor, WIN_STORE_MAX]`. The ceiling is the MEMORY bound (4096 × ~1.2 KB
/// ≈ 5 MB — [`WIN_STORE_MAX`], the same clamp the removed `win_decouple_cap_ret`
/// uses), NOT part of the law: the per-path 2048 knee the pooled laws clamp
/// to is an empirical fit, and the whole point of this law is to DERIVE what
/// that knee was approximating.
/// TERM 3's GEOMETRY, computed ONCE and reported — the discriminator the c9
/// contract's **C9-L3** is scored on (goal-gate, "c9 — THE ONE GEOMETRY THAT
/// SEPARATES THE TWO SPAN FORMS").
///
/// **Why this type exists at all.** The c9 battery was scored with C9-L1 and
/// C9-L3 both UNSCOREABLE for one reason: `[CCAP]` emitted seven fields and
/// none of them was the span. The engine COMPUTED the span at every refresh
/// and threw it away, so the one geometry in the whole tree that can tell the
/// shipped span form from the crosscheck's un-adopted Σ form produced no
/// reading. That is a SPECIFICATION FAILURE against the gauge, recorded as
/// such, and this is its repair.
///
/// **The field set is decided by the CRITERION, not by convenience.** C9-L1
/// reads *"the `[CCAP]` span field reads 0"* at a symmetric cell. C9-L3 is
/// scored on the **RATIO** of the two candidate forms — anchor-free, predicted
/// at exactly **2.000** at c9h — with an absolute band `[265, 315]` sym whose
/// own spread comes from the `rate_fast` anchor (±5 %) and the RTprop spread
/// (±0.4 %). So the criterion needs FOUR quantities and not one:
///
/// * [`shipped`](Self::shipped) — `rate_fast · (RTprop_max − RTprop_min)`, the
///   law the engine actually runs. C9-L1 is this field reading 0.
/// * [`sigma`](Self::sigma) — `Σ_i rate_i · (RTprop_max − RTprop_i)`, the
///   crosscheck's form. It is NOT a candidate law here and nothing reads it:
///   it exists so the ratio is MEASURED rather than assumed. The crosscheck's
///   instruction stands verbatim — ***"Adopt nothing"*** — and a divergence is
///   a defect finding against the engine, never a licence to switch formulas.
/// * [`rate_fast`](Self::rate_fast) and [`spread_s`](Self::spread_s) — the two
///   anchors of the absolute band. Without them a reading outside BOTH bands
///   is uninterpretable; with them the contract's own disposal rule ("a
///   reading outside both bands falsifies the ANCHORS rather than either
///   formula, and is reported that way") is executable from the log alone.
///
/// **No tie predicate, deliberately.** The two forms diverge "by the COUNT of
/// min-RTprop legs", and the obvious way to gauge that is to count legs whose
/// `rtprop_s` equals the minimum. On the wire no two measured RTprops are ever
/// exactly equal, so such a count would read 1 at every geometry and the gauge
/// would silently report the answer it was built to test. `sigma` is a plain
/// sum over ALL legs with no equality test anywhere, so it degrades smoothly:
/// at c9h's two near-tied fast legs it lands near `2 · shipped` because the
/// arithmetic takes it there, and the RATIO measures the effective count
/// instead of asserting it. This is also the CLAUDE.md no-mode-switch rule
/// applied to an instrument: no threshold, no branch, one formula.
///
/// **Observation only.** Nothing in the engine reads `sigma`, `rate_fast` or
/// `spread_s`; [`three_term_store_cap`] reads `shipped` and only `shipped`,
/// which is why the two cannot drift — there is exactly one site that computes
/// the span geometry and this is it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpanForms {
    /// THE SHIPPED LAW: `2 · rate_fast · skew` = `rate_fast · spread`.
    pub shipped: f64,
    /// The crosscheck's UN-ADOPTED form, `Σ_i rate_i · (RTprop_max − RTprop_i)`.
    /// Reported, never consumed.
    pub sigma: f64,
    /// Rate of the single least-RTprop path (sym/s) — C9-L3's ±5 % anchor.
    pub rate_fast: f64,
    /// `RTprop_max − RTprop_min` in seconds — C9-L3's ±0.4 % anchor.
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

    // `rate_fast` is the rate of the path that ARRIVES FIRST (least RTprop) —
    // the path whose symbols overtake the straggler. Over a one-element set
    // `rtp_max == rtp_min`, so `spread_s == 0` and BOTH forms are identically
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
    // The skew is ONE-WAY and the store bounds a ROUND TRIP of it, hence the
    // 2 — a DEFINITION BOUNDARY, kept visible rather than pre-multiplied away
    // (see `three_term_store_cap`'s TERM 3).
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

    // TERM 1 and TERM 2 — BOTH always computed, for every path.
    let mut window = 0.0f64;
    let mut slack = 0.0f64;
    for t in &warm {
        let srtt_s = t.k.max(1.0) * t.rtprop_s; // the honest ack clock
        window += t.rate * srtt_s;
        slack += t.rate * contract_stall_s(rho, b_hint, t.rtprop_s, srtt_s);
    }

    // TERM 3 — always computed too, and identically 0 over a one-element
    // set because `rtp_max == rtp_min` there. Delegated to `span_forms` so
    // the law and the `[CCAP]` span gauge read from ONE computation of the
    // geometry and cannot drift; only `shipped` is consumed here.
    let span = span_forms(terms)?.shipped;

    let total = window + slack + span;
    let limit = (total.ceil() as usize).clamp(floor.min(WIN_STORE_MAX), WIN_STORE_MAX);
    Some((limit, window, slack, span))
}

/// The composed law's per-run `[CCAP]` readout (paper §16.56).
///
/// Split from its emission so the always-on pins assert the STRING an L1
/// parser will scrape rather than a side effect, and so the two teardown arms
/// share one renderer.
///
/// The fields, and what each one exists to make un-missable:
///
/// * `eng=<engaged>/<refreshes>` — MECHANISM LIVENESS (MEASUREMENT DISCIPLINE
///   rule 1). `eng=0/N` with `RWM_COMPOSED_CAP=1` in the `[GATES]` echo is a
///   WARM-UP failure (some live path was cold at every refresh), NOT a null
///   result, and the two must never be confused again.
/// * `cap=` — the realized mean cap. The number the whole arm is about.
/// * `mem=` / `floor=` — the BIND FRACTIONS of the only two bounds that
///   survive: `WIN_STORE_MAX`, a memory bound stated OUTSIDE the law, and
///   `store_cap_floor` = 64, the one paroled constant whose provenance
///   ADR-0070 finding 5 records as ABSENT. A composed run with `mem` above
///   zero means the memory bound has become the law — the predecessor's exact
///   defect reproduced, and §16.56 calls that a STOP, not a result.
/// * `brake=<closed>/<ticks>` — the late-stage brake's own liveness. An arm
///   bit-identical to control must read as a NULL RESULT, not a null effect,
///   and `brake=0/N` is the difference between "the brake never bound" and
///   "the brake was never armed".
///
/// ## The SPAN block (`span=` … `spread_us=`) — added 2026-08-19
///
/// The c9 battery scored **C9-L1 and C9-L3 UNSCOREABLE for want of a field**:
/// the engine computed TERM 3 at every refresh and reported seven numbers,
/// none of them the span. Both clauses are read off this block, and the block
/// is exactly [`SpanForms`] averaged over the ENGAGED refreshes — see that
/// type for why each member is required BY THE CRITERION:
///
/// * `span=` — the shipped `rate_fast·spread`, mean over engaged refreshes.
///   **C9-L1 is this field reading 0** at a symmetric cell.
/// * `span_sigma=` — the crosscheck's un-adopted `Σ bwᵢ(RTT_max − RTTᵢ)`, same
///   mean. Reported so the discriminating ratio is MEASURED, not assumed.
///   *Adopt nothing:* no engine path reads it.
/// * `span_ratio=` — `Σ span_sigma / Σ span`, which is exactly
///   `span_sigma / span` above because both means share the `engaged`
///   denominator. **C9-L3 is scored on this number**, predicted at exactly
///   **2.000** at c9h and anchor-free. `0.000` when `Σ span` is 0 — i.e. at
///   every symmetric cell, where the ratio is genuinely undefined and `span=0`
///   is the field that says so. A parser must read `span=` first.
/// * `rate_fast=` (sym/s) and `spread_us=` (µs) — C9-L3's two absolute
///   anchors, ±5 % and ±0.4 % respectively. They make the contract's own
///   disposal rule executable from the log: a span outside BOTH the 281 and
///   562 bands falsifies the ANCHORS rather than either formula, and only
///   these two fields can show that.
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
    // Means over ENGAGED refreshes — the ticks on which the law produced a
    // span at all. `eng=0/N` therefore renders a well-defined 0.0 block and
    // never a NaN, the same rule the bind fractions already follow.
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

/// The `[SUMCAP]` line — the `×N` deletion's engagement echo (paper §16.62).
///
/// **This gauge exists to make a null READABLE**, which is §16.53's DIVERGED
/// lesson and MEASUREMENT DISCIPLINE 18 applied before the battery rather than
/// after it. `RWM_SUM_CAP` changes one factor in a CLAMPED expression, so there
/// are three completely different ways for an arm to come back saying "no
/// difference", and a mean cap cannot tell them apart:
///
/// * `eng=0/N` — the law never engaged (every refresh fell through to the
///   legacy or boot branch). A WARM-UP failure, not a result.
/// * `chg=0/M` — engaged, and the two arms produced the SAME INTEGER at every
///   refresh. The gate was live and arithmetically inert: a null RESULT, and
///   the honest report is "the clamp still governs", not "the deletion does
///   nothing".
/// * `chg=M/M` with `pin` high — the corrected value is itself pinned at
///   `N·knee`. Under MEASUREMENT DISCIPLINE 18 this is a DEFECT FINDING about
///   the ceiling, and no verdict about the multiplier may be recorded from it,
///   because the arm measured the clamp.
///
/// Fields: `eng=<engaged>/<refreshes>`; `chg=<differed>/<engaged>` with its
/// fraction; `pin=` the fraction of engaged refreshes whose realized cap was
/// the `N·knee` ceiling; `floor=` the same for the derived floor; `cap=` the
/// realized mean and `ask=` the mean UNCLAMPED value the law asked for — the
/// pair that shows directly whether the bound or the law is answering
/// (MEASUREMENT DISCIPLINE 17(b): *a clamp may never be the only thing making
/// a law sane*).
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

/// The `[SUMCAP]` tally, fed at every pooled-law refresh on BOTH arms.
///
/// It records the COUNTERFACTUAL as well as the realized value — at each
/// refresh it computes what the other arm would have produced from the same
/// inputs — because "did this gate change anything" is not answerable from one
/// arm's outputs alone, and answering it from two separate RUNS is what the
/// composed battery had to do and could not do cleanly (§16.57).
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
    /// counterfactual is bounded IDENTICALLY to the realized value. Comparing
    /// two multipliers under two different clamps would not be an A/B of the
    /// multiplier at all — it is the confound this whole section is about.
    floor: usize,
    pool: usize,
    /// The VALUE-multiplier axis this run is on (`RWM_DELTA_CAP`) and the dial
    /// point it reads. Held so the count-multiplier counterfactual is computed
    /// under the SAME value multiplier — the two axes must be varied one at a
    /// time or neither counterfactual means anything.
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
        // The counterfactual: the SAME expression with the COUNT multiplier
        // flipped, under the SAME bounds AND the same value multiplier. It goes
        // through `pooled_store_cap`, so it cannot drift from what the engine
        // actually evaluates.
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
        // Emitted only on the ON arm, so the shipped default's output is
        // unchanged. The control arm's own pin fraction is already carried by
        // the L1 parsers' `occcap_p50` gauge, which is the statistic the
        // 121/126 pinned-rep finding was measured with.
        if self.on {
            eprintln!("{}", self.sumcap_line());
        }
        // The one-sided-clamp witness, once per sender teardown and on EVERY
        // arm — the hypothesis it scores (§16.63's successor) is about the
        // SHIPPED estimator, not about any gate here. Silent when the
        // estimator was never fed.
        if crate::scheduler::LCW_LOSS_MASS.load(std::sync::atomic::Ordering::Relaxed) > 0
            || crate::scheduler::LCW_OVER_N.load(std::sync::atomic::Ordering::Relaxed) > 0
        {
            eprintln!("{}", crate::scheduler::lcw_report_line());
        }
    }
}

/// The `[DCAP]` engagement echo for the δ-priced pool multiplier — paper
/// §16.67, gate `RWM_DELTA_CAP`.
///
/// Same convention as [`sumcap_report_line`], with the counterfactual keyed to
/// the OTHER axis: at every engaged refresh it recomputes what the SHIPPED
/// `gain` would have produced from the same Σ under the same bounds and the
/// same count multiplier, so *"did the derived multiplier change anything"* is
/// answerable from ONE run.
///
/// `q=` carries the resolved CoDel setpoint at this tunnel's dial point, and
/// `b=` the dial number it was mapped from — together they let a battery
/// verify that the dial routed (MEASUREMENT DISCIPLINE 1) rather than only
/// that the env var was read.
///
/// **How a null must read.** `eng=0/0` is NEVER ARMED (the pooled seat was not
/// reached — at N = 1 this is the expected and correct reading, since the law
/// short-circuits before any multiplier); `eng=N/N` with `chg_frac=0.0000`
/// would be armed-and-inert, which cannot happen while `gain != 1+q` and is
/// therefore an instrument failure rather than a result. A high `pin=` means
/// the arm measured the `N·knee` ceiling and not the law, and MEASUREMENT
/// DISCIPLINE 18 requires it be reported as such.
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
    /// evaluated IDENTICALLY except for the one factor under test.
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
        // The counterfactual: the SAME expression with the VALUE multiplier
        // flipped, under the SAME bounds and the SAME count multiplier.
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
            eprintln!("{}", self.dcap_line());
        }
    }
}

// ── The saturation-filter gauge (`sf=`), 2026-08-09 ──────────────────────
//
// MEASUREMENT DISCIPLINE 14's instrument for the store-cap phase, and the
// direct analogue of the `pf=` floor/clock gauge that converted "Unlock The
// Default 2" from an argument into a measurement. The question it answers
// is a POPULATION question, and it is the only one that decides whether the
// documented `active_paths()` filter trap is live or latent here: at the
// dyn-cap refresh instants, how often does `active_paths()` (cwnd −
// in_flight > 0) return FEWER paths than `live_paths()`, and how often does
// it return NONE at all?
//
// A tick where n_active < n_live is a tick where the pooled cap's Σ-anchor
// base was summed over a STRICT SUBSET of the paths whose count (`n_live`)
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
/// ≈ 5 MB (introduced by feat/window-mtu part 1, goal-gate "Window
/// Decoupling + MTU Scaling", whose decoupled law was refuted and removed;
/// the bound survives as the memory clamp of the composed/three-term laws).
pub const WIN_STORE_MAX: usize = 4096;
