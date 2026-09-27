//! `[LATE]` — THE HOLE'S LATENESS, BRACKETED, AND THE REQUEST LAW'S OWN
//! HYPOTHETICAL THRESHOLD, COMPUTED READ-ONLY.
//!
//! **The measurand, and why it is bracketed rather than exact.** §16.83's
//! request law is stated on the LATENESS of a hole:
//!
//!     ℓ* = min{ ℓ : w·π₀·f(ℓ) ≤ π₁·c_L(ℓ) } ∧ (H − d)⁺,   REQUEST ⇔ ℓ ≥ ℓ*
//!
//! and the receiver cannot observe a hole's lateness exactly. What it knows
//! is that the missing seq was due some time between the last advance of its
//! own high-water mark and the arrival that exposed the hole — so every hole
//! carries a BRACKET
//!
//!     ℓ ∈ [ now − t_exposed ,  now − t_hi_prev ]
//!
//! whose lower end is what `[SUCC]` already times and whose upper end needs
//! one new field (`hi_at` beside `hi` in `SuccGauge`). Both ends are printed.
//! A law positioned on a bracketed quantity must say which end it used, and
//! the whole point of this gauge is to make that choice visible before any
//! arm exists.
//!
//! **THE RECEIVER-OBSERVABLE FORM, and why it is CONSERVATIVE.** §16.83's own
//! argument: on `[0, ℓ*)` no copy has flown, so the receiver's heal density
//! `ρ̂_heal(ℓ)` IS `π₀·f(ℓ)` exactly — the §16.77.8a censoring vanishes on
//! precisely the evaluation interval — and the survivor fraction `Ŝ_tot(ℓ)`
//! bounds `π₁` from above. Substituting an upper bound for `π₁` makes the
//! inequality bind LATER, so
//!
//!     ℓ*_recv  ≤  ℓ*
//!
//! and the estimate is biased toward the SHIPPED machine (request sooner),
//! never away from it. That direction is the reason this is safe to compute
//! at all.
//!
//! **WHAT IS ESTIMATED, TERM BY TERM, and every constant declared:**
//!
//!   * `ρ̂_heal(ℓ)` — the discrete HAZARD of self-healing: of the holes still
//!     open entering the bucket at `ℓ`, the fraction that closed by their own
//!     ORIGINAL arrival inside it. Original-closed, because a coded fill is a
//!     repair that worked and is not the hole healing itself.
//!   * `Ŝ_tot(ℓ)` — the survivor fraction, holes still unresolved at `ℓ`.
//!   * `c_L(ℓ)` — the cost of a LATE repair relative to a false one. **NOT
//!     INVENTED HERE.** It is carried as the declared unit ratio `w/c_L = 1`
//!     and PRINTED as `w=`, so the whole comparison is `ρ̂_heal ≤ Ŝ_tot` at
//!     the shipped ratio and a different ratio is a rescale of one printed
//!     number rather than a different gauge. A register row, not a law.
//!   * `H` — the store-cap headroom, observed DIRECTLY as the onset of the
//!     arrival stall during a frontier freeze (the `[WIDLE]` machinery's own
//!     gap samples), not derived from `RWM_STORE_GAIN`.
//!   * `d` — the `[FDIAG]` SOURCE class's mean resolution time: how long an
//!     ARQ-resolved hole actually takes.
//!
//! **THE TWO BIND GAUGES**, because every clamp owes one:
//!   * `knee_bind` — the fraction of readouts where `(H − d)⁺` was the
//!     binding term rather than the inequality. **If this is ≈ 1 the request
//!     law IS the store-cap law**, which is a KNEE-BOUND ledger verdict and
//!     not a footnote.
//!   * `sampler_bind` — the fraction of readouts where the 2 ms
//!     `GAP_ACK_MIN_INTERVAL` floor, not the lateness, set when a report
//!     could be made at all. A threshold that always binds turns the law it
//!     gates into a constant.
//!
//! **`-` iff `n = 0`.** Nothing here branches, decides, or is reachable from
//! a control law: `lstar_us` is a HYPOTHETICAL, printed and read by nobody.

use super::succ::{Hist, HoleOutcome, BUCKETS, bucket_lower_edge, bucket_of};

/// The declared cost ratio `w / c_L`. UNIT, and printed as `w=` — a register
/// row, not a tuned constant: the comparison is `w·ρ̂_heal ≤ Ŝ_tot`, so a
/// different ratio rescales one printed number and invents no new gauge.
const W_COST_RATIO: f64 = 1.0;

/// **THE REQUEST BAR -- the level `rho_heal` is compared against, and the ONE
/// place the contract's price enters the request law.**
///
/// 16.83.0's inequality, in the receiver's coordinate, is
///
///     w * pi0 * f(l)  <=  pi1 * c_L(l) ,   c_L = P_arq(rho, r) * delta / d
///
/// and 16.83.0 shows the indicator in `c_L` NEVER fires inside the domain, so
/// `c_L` is CONSTANT in `l` on the whole evaluation interval and the price is
/// a pure MULTIPLIER of the cost side. On the receiver's own evidence
/// `pi0*f(l) = rho_heal(l)` and `pi1 = 1 - rho_heal(l)`, so the inequality is
///
///     w * rho_heal(l)  <=  (1 - rho_heal(l)) * c   ==>   rho_heal(l) <= c/(w+c)
///
/// with `c = delta / DELTA_AUTO` the contract's price RELATIVE TO ITS OWN
/// ANCHOR -- a pure ratio, and NOT a new constant: `DELTA_AUTO = 0.5` is the
/// dial's midpoint that `raptorpath_math` already declares and every
/// delta-priced law in the tree already reads. `P_arq(rho, r) = 1` on every
/// shipped seat (16.74.2, rho = 1) and rides inside `c` when it is not.
///
/// **NO THRESHOLD ON delta AND NONE ON rho.** This is ONE expression,
/// continuous and strictly increasing in `delta`, evaluated at whatever point
/// of the dial the contract names. The three protocol hints are NAMED POINTS
/// on it and nothing here branches on which one is in force.
///
/// **SHAPE, and it is 16.83.8's, term for term.**
///
///   * `delta = DELTA_AUTO` ==> `c = 1` ==> `bar = w/(1+w) = 1/2` at the
///     declared unit ratio -- **exactly the number this gauge printed before
///     the price was plumbed**, so the Auto anchor is byte-identical and the
///     dial is an extension of the gauge rather than a replacement of it.
///   * `delta` up (Realtime, 50) ==> `bar` up ==> the crossing is EARLIER ==>
///     `l*` FALLS: *a latency-priced contract waits less.* Realtime does not
///     get a different rule, it gets a smaller number.
///   * `delta` down (Bulk, 0.005) ==> `bar` down ==> `l*` rises to the domain
///     cap: *a Bulk contract waits the whole headroom and no longer.*
///   * `bar` is in `(0, 1)` for every finite positive `delta`, so it can never
///     make the comparison vacuous at either end.
pub fn request_bar(delta_price: f64) -> f64 {
    let c = delta_price.max(1e-12) / raptorpath_math::DELTA_AUTO;
    c / (W_COST_RATIO + c)
}

/// **`k_half(pi0) = ln 2 / (-ln pi0)`** -- 16.80.6(b)'s HALF-COST SPAN, the
/// span at which the cost of a false repair halves, evaluated on a MEASURED
/// `pi0`. No free constant: `ln 2` is the halving and nothing else.
///
/// `pi0 -> 0` (nothing heals itself) gives `k_half -> 0`; `pi0 -> 1` (almost
/// everything does) gives `k_half -> +inf`, which is why the `m` law below
/// clamps against a resource bound and not against taste.
pub fn k_half(pi0: f64) -> f64 {
    if !(pi0 > 0.0) || pi0 >= 1.0 {
        // `pi0 = 0` is the copy's own limit; `pi0 >= 1` is not a probability
        // this estimator can produce from a finite sample without every hole
        // having healed, and is treated as the unbounded end -- clamped by
        // the caller's resource bound, never by a constant chosen here.
        return if pi0 >= 1.0 { f64::INFINITY } else { 0.0 };
    }
    std::f64::consts::LN_2 / (-pi0.ln())
}

/// **`m = clamp(ceil(k_half(pi0)), 1, A*)`** -- 16.83.3's span width, and the
/// ONE place the request's vocabulary is decided.
///
/// `rank_feedback` is arm (B). ABSENT ⇒ `m = 1` unconditionally: the COPY,
/// which is what isolates arm (A)'s TIMING lever from the vocabulary lever.
/// Armed, `m` is a continuous function of the measured `pi0` -- and its
/// `pi0 -> 0` limit is `m = 1` again, so the single-path cells arrive at the
/// copy by themselves rather than by a branch.
///
/// `a_star` is the caller's DECLARED RESOURCE BOUND. At the receiver the
/// sender's retained trailing span is not observable, so the receiver bounds
/// `m` by its OWN outstanding span and the sender's refusal to code beyond
/// what it retains is COUNTED (`WA1`) rather than assumed away.
///
/// `pi0 = None` (no hole has closed yet) ⇒ `m = 1`: with no evidence the law
/// takes its conservative limit, which is the machine that ships.
pub fn request_m(pi0: Option<f64>, rank_feedback: bool, a_star: u64) -> u16 {
    if !rank_feedback {
        return 1;
    }
    let cap = a_star.clamp(1, u16::MAX as u64);
    match pi0 {
        Some(p) => {
            let k = k_half(p).ceil();
            if !k.is_finite() {
                // `pi0 -> 1`: the half-cost span is unbounded and what stops
                // it is the RESOURCE BOUND, never a constant chosen here.
                cap as u16
            } else if k <= 1.0 {
                1
            } else {
                (k as u64).clamp(1, cap) as u16
            }
        }
        None => 1,
    }
}

/// **THE HOLE-LATENESS GAUGE.** Owned by the receiver task; observation only.
pub struct LateGauge {
    /// **THE CONTRACT'S OWN PRICE AT THIS RECEIVER** -- `net::delta_price`
    /// evaluated on the hint/`RWM_DELTA` the receiver task now carries (16.81
    /// plumbed it; before that this seat read a hard-coded `Auto`). It enters
    /// through [`request_bar`] and nowhere else, and `DELTA_AUTO` reproduces
    /// the pre-plumb number exactly.
    delta: f64,
    /// Holes closed, and the two ends of their lateness bracket.
    n: u64,
    lo: Hist,
    hi: Hist,
    /// Resolution class, and the same/cross-path split of it.
    by_orig: u64,
    /// Of `by_orig`, the holes closed by the SENDER'S COPY — an own-source
    /// arrival, but not a self-heal. Printed as `rtx=`.
    by_retx: u64,
    by_rep: u64,
    by_aban: u64,
    xp_n: u64,
    sp_n: u64,
    /// **ρ̂_heal's raw material**, per bucket: holes that entered it still
    /// open (`at_risk`), and how many of THOSE went on to close by their OWN
    /// ORIGINAL arrival (`at_risk_heal`). Their ratio is the RESIDUAL
    /// SELF-HEAL PROBABILITY at that lateness — "if I keep waiting past ℓ,
    /// what are the odds this hole closes itself?" A coded fill is a repair
    /// that WORKED, not a hole healing itself, and is deliberately absent
    /// from the numerator.
    at_risk: Box<[u64; BUCKETS]>,
    at_risk_heal: Box<[u64; BUCKETS]>,
    /// The `[FDIAG]` SOURCE class, re-read here so `d` sits beside the `H` it
    /// is subtracted from: ARQ-resolved holes and their total time.
    d_n: u64,
    d_us_sum: u64,
    /// The OBSERVED KNEE: arrival-stall onsets during a frontier freeze, in
    /// µs, from the `[WIDLE]` machinery's own gap samples. The store-cap
    /// headroom `H`, measured rather than derived from `RWM_STORE_GAIN`.
    knee: Hist,
    /// Readouts taken, and how many had each clamp binding.
    report_n: u64,
    knee_bind_n: u64,
    sampler_bind_n: u64,
}

impl Default for LateGauge {
    fn default() -> Self {
        Self {
            delta: raptorpath_math::DELTA_AUTO,
            n: 0,
            lo: Hist::default(),
            hi: Hist::default(),
            by_orig: 0,
            by_retx: 0,
            by_rep: 0,
            by_aban: 0,
            xp_n: 0,
            sp_n: 0,
            at_risk: Box::new([0; BUCKETS]),
            at_risk_heal: Box::new([0; BUCKETS]),
            d_n: 0,
            d_us_sum: 0,
            knee: Hist::default(),
            report_n: 0,
            knee_bind_n: 0,
            sampler_bind_n: 0,
        }
    }
}

impl LateGauge {
    /// The receiver's gauge at the contract's own point on the delta dial.
    /// [`Default`] is the `DELTA_AUTO` anchor, which reproduces the
    /// pre-plumb reading bit-for-bit.
    pub fn new(delta_price: f64) -> Self {
        Self { delta: delta_price, ..Self::default() }
    }

    /// The bar this gauge's own price puts on `rho_heal` -- printed on the
    /// line beside `delta=`, so a threshold is never quoted without the level
    /// that produced it.
    pub fn bar(&self) -> f64 {
        request_bar(self.delta)
    }

    /// **`pi0` AS THE RECEIVER SEES IT** -- the residual self-heal probability
    /// at lateness 0. `None` iff no hole has closed, which is the state that
    /// renders `-` rather than 0. Read by 16.83.3's `m` law (arm B).
    pub fn pi0(&self) -> Option<f64> {
        (self.n > 0 && self.at_risk[0] > 0)
            .then(|| self.at_risk_heal[0] as f64 / self.at_risk[0] as f64)
    }

    /// **`l*_recv`, THE THRESHOLD THE ARM ACTS ON** (16.83.6 arm A).
    /// Identical to [`Self::lstar_us`] -- the ACTING name and the
    /// HYPOTHETICAL name are ONE function on purpose, so the number an arm
    /// requests at and the number `[LATE]` prints can never be two laws.
    pub fn request_lateness(&self) -> (Option<u64>, bool) {
        self.lstar_us()
    }
    /// **ONE CLOSED HOLE.** `lo_us` / `hi_us` are the two ends of the
    /// lateness bracket (`[SUCC]`'s own timing and the high-water bracket),
    /// `cross` the same/cross-path split, `outcome` the resolution class.
    pub fn note_hole(&mut self, outcome: HoleOutcome, cross: bool, lo_us: u64, hi_us: u64) {
        self.n += 1;
        self.lo.add(lo_us);
        self.hi.add(hi_us.max(lo_us));
        match outcome {
            HoleOutcome::Original => self.by_orig += 1,
            HoleOutcome::Retransmit => {
                self.by_orig += 1;
                self.by_retx += 1;
            }
            HoleOutcome::Repair => self.by_rep += 1,
            HoleOutcome::Abandoned => self.by_aban += 1,
        }
        if cross {
            self.xp_n += 1;
        } else {
            self.sp_n += 1;
        }
        // The survival bookkeeping, on the LOWER end of the bracket — the
        // conservative one, and the same clock `[SUCC]` reports. A hole that
        // closed in bucket `b` was STILL OPEN entering every bucket up to it,
        // so it is at risk in all of them; if it closed by its own original,
        // it is a heal in all of them too, which is what makes the ratio the
        // RESIDUAL probability rather than a per-bucket hazard.
        let b = bucket_of(lo_us);
        let heal = outcome == HoleOutcome::Original;
        for i in 0..=b {
            self.at_risk[i] += 1;
            if heal {
                self.at_risk_heal[i] += 1;
            }
        }
    }

    /// One `[FDIAG]`-class ARQ resolution: `d`'s sample.
    pub fn note_source_resolution(&mut self, us: u64) {
        self.d_n += 1;
        self.d_us_sum = self.d_us_sum.saturating_add(us);
    }

    /// One arrival-stall onset during a frontier freeze — the observed knee
    /// `H`. Fed from the `[WIDLE]` gap machinery; µs.
    pub fn note_knee(&mut self, us: u64) {
        self.knee.add(us);
    }

    /// `d` — the mean ARQ resolution time, µs. `None` iff no sample.
    pub fn d_us(&self) -> Option<u64> {
        (self.d_n > 0).then(|| self.d_us_sum / self.d_n)
    }

    /// `H` — the observed knee (median stall onset), µs. `None` iff no
    /// sample, which is the honest reading at a run that never froze.
    pub fn knee_us(&self) -> Option<u64> {
        self.knee.quantile(0.50)
    }

    /// **`ℓ*_recv`, the HYPOTHETICAL threshold.** Returns
    /// `(lstar_us, knee_bound)` — the second says whether `(H − d)⁺` was the
    /// binding term. `None` when neither term is estimable yet, which is
    /// reported as `-` rather than defaulted to 0 (0 would read "request
    /// immediately", the shipped corner, and would be indistinguishable from
    /// a genuine π₀ → 0 finding).
    pub fn lstar_us(&self) -> (Option<u64>, bool) {
        // **THE CROSSING.** `π₁`, the probability the hole genuinely needs a
        // repair, is `1 − ρ̂_heal(ℓ)` on the receiver's own evidence, so the
        // law's inequality `w·π₀·f ≤ π₁·c_L` reduces — at the DECLARED unit
        // ratio `w = c_L/c_F` — to
        //
        //     ρ̂_heal(ℓ)  ≤  w / (1 + w)
        //
        // which is unit-free, monotone in `ℓ`, and has EXACTLY the two limits
        // §16.83 states: `π₀ → 0` ⇒ `ρ̂_heal ≡ 0` ⇒ `ℓ* = 0`, i.e. request
        // immediately, which IS today's machine at the singles; `π₀ → 1` ⇒
        // `ρ̂_heal` stays high ⇒ the crossing is late and the knee cap below
        // is what binds. No timescale, and no constant but `w`.
        let mut cross: Option<u64> = None;
        if self.n > 0 {
            let bar = self.bar();
            for i in 0..BUCKETS {
                let at_risk = self.at_risk[i];
                if at_risk == 0 {
                    // Nothing survives this long: the wait has no value left.
                    cross = Some(bucket_lower_edge(i));
                    break;
                }
                let rho = self.at_risk_heal[i] as f64 / at_risk as f64;
                if rho <= bar {
                    cross = Some(bucket_lower_edge(i));
                    break;
                }
            }
        }
        // The knee cap `(H − d)⁺`.
        let cap = match (self.knee_us(), self.d_us()) {
            (Some(h), Some(d)) => Some(h.saturating_sub(d)),
            (Some(h), None) => Some(h),
            _ => None,
        };
        match (cross, cap) {
            (Some(c), Some(k)) => (Some(c.min(k)), k <= c),
            (Some(c), None) => (Some(c), false),
            (None, Some(k)) => (Some(k), true),
            (None, None) => (None, false),
        }
    }

    /// Has this gauge ever seen a hole?
    pub fn is_receiver_site(&self) -> bool {
        self.n > 0
    }

    /// `xp_n / (xp_n + sp_n)` — STRUCTURALLY 0 at one path.
    pub fn xp_frac(&self) -> Option<f64> {
        let d = self.xp_n + self.sp_n;
        (d > 0).then(|| self.xp_n as f64 / d as f64)
    }

    /// The `[LATE]` line. Takes the readout's two BIND observations —
    /// whether the 2 ms sampler floor was what limited this report — and
    /// folds them in, so the printed fractions describe the readouts that
    /// were actually taken.
    pub fn line(&mut self, sampler_bound: bool) -> String {
        let (lstar, knee_bound) = self.lstar_us();
        self.report_n += 1;
        if knee_bound {
            self.knee_bind_n += 1;
        }
        if sampler_bound {
            self.sampler_bind_n += 1;
        }
        let opt = |v: Option<u64>| v.map_or_else(|| "-".to_string(), |x| x.to_string());
        let q = |h: &Hist, p: f64| opt(h.quantile(p));
        let frac = |n: u64, d: u64| {
            if d == 0 { "-".to_string() } else { format!("{:.4}", n as f64 / d as f64) }
        };
        // The INGREDIENTS beside the answer: the residual self-heal
        // probability at lateness 0 (that is `π₀` as the receiver sees it)
        // and the survivor fraction at `ℓ*`, which is the observable upper
        // bound on `π₁`. A threshold printed without them is unauditable.
        let rho0 = if self.n > 0 {
            format!("{:.4}", self.at_risk_heal[0] as f64 / self.at_risk[0].max(1) as f64)
        } else {
            "-".to_string()
        };
        let s_tot = match (lstar, self.n) {
            (Some(l), n) if n > 0 => {
                format!("{:.4}", self.at_risk[bucket_of(l)] as f64 / n as f64)
            }
            _ => "-".to_string(),
        };
        format!(
            "[LATE] n={} lo_p50={} lo_p90={} lo_p99={} hi_p50={} hi_p90={} hi_p99={} \
             orig={} rep={} aban={} xp_n={} sp_n={} xp_frac={} d_us={} d_n={} \
             knee_us={} knee_n={} rho_heal0={rho0} s_tot={s_tot} lstar_us={} w={:.2} \
             delta={:.5} bar={:.4} \
             knee_bind={} sampler_bind={} reports={} rtx={}",
            self.n,
            q(&self.lo, 0.50),
            q(&self.lo, 0.90),
            q(&self.lo, 0.99),
            q(&self.hi, 0.50),
            q(&self.hi, 0.90),
            q(&self.hi, 0.99),
            self.by_orig,
            self.by_rep,
            self.by_aban,
            self.xp_n,
            self.sp_n,
            self.xp_frac().map_or_else(|| "-".to_string(), |f| format!("{f:.4}")),
            opt(self.d_us()),
            self.d_n,
            opt(self.knee_us()),
            self.knee.n(),
            opt(lstar),
            W_COST_RATIO,
            self.delta,
            self.bar(),
            frac(self.knee_bind_n, self.report_n),
            frac(self.sampler_bind_n, self.report_n),
            self.report_n,
            self.by_retx,
        )
    }
}

// ── `[RANK]` ────────────────────────────────────────────────────────────

/// **`[RANK]` — THE FRONTIER PROBE, READ UNCONDITIONALLY.**
///
/// `frontier_probe(f + 1, highest_seen − Δ_tail)` returns `(holes, pivots)`
/// over the frontier span: how many seqs the decoder still owes, and how many
/// independent equations it is already holding for them. `deficit =
/// holes − pivots` is the number of MORE equations the span actually needs —
/// the payload of §16.83's `REQUEST = (a, m, k)` vocabulary, and the quantity
/// `rank_in` / `frontier_probe` were written for and NOTHING has ever read.
///
/// It exists today only inside `RWM_FDIAG`'s block. This prints it under the
/// ordinary diagnosis gate, so the receiver's own rank picture is on every
/// diagnosed run rather than on one arm.
///
/// `tail_overcount` is the honest correction: the probe's span runs to the
/// highest seq SEEN, and the last `Δ_tail` of it is in flight rather than
/// missing, so a naive `holes` over-counts by whatever is legitimately still
/// on the wire. It is REPORTED, not silently subtracted.
#[derive(Default)]
pub struct RankGauge {
    reports: u64,
    holes: u64,
    pivots: u64,
    tail_overcount: u64,
    /// The largest deficit seen — the worst the span ever was.
    max_deficit: u64,
    /// Readouts where the span was empty (the frontier was caught up).
    empty: u64,
}

impl RankGauge {
    /// One probe reading. `tail_overcount` is the in-flight tail the span
    /// includes; `holes`/`pivots` are `frontier_probe`'s own outputs.
    pub fn note(&mut self, holes: u64, pivots: u64, tail_overcount: u64) {
        self.reports += 1;
        if holes == 0 {
            self.empty += 1;
        }
        self.holes = holes;
        self.pivots = pivots;
        self.tail_overcount = tail_overcount;
        self.max_deficit = self.max_deficit.max(holes.saturating_sub(pivots));
    }

    pub fn is_fed(&self) -> bool {
        self.reports > 0
    }

    /// The `[RANK]` line. The three span quantities are the LATEST reading
    /// (a census, not a cumulative count); `max_deficit` and `reports` are
    /// cumulative, so the last line still carries the run's worst case.
    pub fn line(&self) -> String {
        format!(
            "[RANK] holes={} pivots={} deficit={} tail_overcount={} max_deficit={} \
             empty={} reports={}",
            self.holes,
            self.pivots,
            self.holes.saturating_sub(self.pivots),
            self.tail_overcount,
            self.max_deficit,
            self.empty,
            self.reports,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bracket has two ends and BOTH are printed; the upper end is never
    /// below the lower one.
    #[test]
    fn the_lateness_bracket_reports_both_ends() {
        let mut g = LateGauge::default();
        assert!(!g.is_receiver_site());
        g.note_hole(HoleOutcome::Original, false, 1_000, 4_000);
        // A caller that hands an upper end BELOW the lower one gets the lower
        // one — the bracket can never invert.
        g.note_hole(HoleOutcome::Repair, true, 8_000, 1);
        let l = g.line(false);
        assert!(l.starts_with("[LATE] n=2 "), "{l}");
        assert!(l.contains("orig=1 rep=1 aban=0"), "{l}");
        assert!(l.contains("xp_n=1 sp_n=1 xp_frac=0.5000"), "{l}");
        assert!(l.contains("hi_p99=7680"), "the inverted bracket collapsed to lo: {l}");
    }

    /// `xp_frac ≡ 0` when every hole was closed on its exposing path — the
    /// single-path control, and `-` when nothing closed at all.
    #[test]
    fn cross_path_is_zero_on_one_path_and_absent_when_empty() {
        let mut empty = LateGauge::default();
        let l = empty.line(false);
        assert!(l.contains("n=0"), "{l}");
        assert!(l.contains("xp_frac=-"), "an absent fraction is `-`, never 0: {l}");
        assert!(l.contains("lo_p50=-") && l.contains("hi_p50=-"), "{l}");
        assert!(l.contains("rho_heal0=- s_tot=-"), "the ingredients are `-` too: {l}");
        assert!(l.contains("lstar_us=-"), "an unestimable threshold is `-`: {l}");
        let mut one = LateGauge::default();
        for _ in 0..5 {
            one.note_hole(HoleOutcome::Original, false, 100, 200);
        }
        assert_eq!(one.xp_frac(), Some(0.0));
    }

    /// THE KNEE CAP BINDS AND SAYS SO. With `H − d` below the inequality's
    /// crossing, `ℓ*` is the cap and `knee_bind` reports it.
    #[test]
    fn the_knee_cap_binds_and_the_bind_gauge_says_so() {
        let mut g = LateGauge::default();
        // Every hole heals LATE and by its own original, so the residual
        // self-heal probability stays at 1 until 500 ms and the inequality
        // crosses only there — the `π₀ → 1` limit.
        for _ in 0..100 {
            g.note_hole(HoleOutcome::Original, false, 500_000, 500_000);
        }
        // Before the knee exists, the crossing alone is the threshold and it
        // is LATE — the `π₀ → 1` limit.
        assert_eq!(g.lstar_us(), (Some(524_288), false), "the crossing alone");
        g.note_knee(20_000);
        g.note_source_resolution(5_000);
        let (lstar, knee) = g.lstar_us();
        assert_eq!(lstar, Some(13_432), "(H − d)⁺, H at its bucket edge 18 432 minus d = 5 000");
        assert!(knee, "the cap is what bound");
        let l = g.line(true);
        assert!(l.contains("knee_us=18432 knee_n=1"), "{l}");
        assert!(l.contains("d_us=5000 d_n=1"), "{l}");
        assert!(l.contains("rho_heal0=1.0000"), "every hole healed itself: {l}");
        assert!(l.contains("lstar_us=13432"), "{l}");
        assert!(l.contains("knee_bind=1.0000 sampler_bind=1.0000 reports=1"), "{l}");
        // The declared ratio is on the line, so a different one is a rescale
        // of a printed number rather than a hidden constant.
        assert!(l.contains("w=1.00"), "{l}");
    }

    /// **THE `π₀ → 0` LIMIT, PINNED.** When no hole ever heals itself, the
    /// residual self-heal probability is 0 everywhere and `ℓ* = 0` — REQUEST
    /// IMMEDIATELY, which is exactly the machine that ships today. The law
    /// must contain its own corner, or it is a different law.
    #[test]
    fn no_self_healing_gives_the_shipped_corner() {
        let mut g = LateGauge::default();
        for _ in 0..50 {
            g.note_hole(HoleOutcome::Repair, false, 30_000, 30_000);
        }
        assert_eq!(g.lstar_us(), (Some(0), false), "π₀ → 0 must give ℓ* = 0");
        let l = g.line(false);
        assert!(l.contains("rho_heal0=0.0000 s_tot=1.0000 lstar_us=0"), "{l}");
        assert!(l.contains("knee_bind=0.0000"), "the inequality bound, not the cap: {l}");
    }


    // ── THE PRICE ON THE DIAL: 16.83.8's SHAPE CHECK, AS TESTS ────────

    /// The three protocol hints, as the delta values `net::delta_price` maps
    /// them to. NAMED POINTS on one dial -- the tests below sweep THROUGH
    /// them and never branch on them.
    const REALTIME: f64 = 50.0;
    const AUTO: f64 = 0.5;
    const BULK: f64 = 0.005;

    /// **THE BAR IS ONE CONTINUOUS, STRICTLY MONOTONE FUNCTION OF THE PRICE,
    /// AND A +/-2 % NUDGE AT EVERY NAMED POINT MOVES IT BY LESS THAN 2.1 %.**
    ///
    /// This is the NO-MODE-SWITCH gate for the request law: if any hint
    /// selected a different rule, a nudge across it would step. Nothing here
    /// reads a hint at all -- there is one expression and three evaluations of
    /// it.
    #[test]
    fn the_request_bar_is_continuous_and_monotone_through_the_three_named_points() {
        // THE AUTO ANCHOR IS THE PRE-PLUMB NUMBER, EXACTLY. Before the price
        // reached this seat the bar was `w/(1+w)`; `delta = DELTA_AUTO` must
        // reproduce it bit-for-bit or the dial is a different gauge.
        assert_eq!(
            request_bar(AUTO),
            W_COST_RATIO / (1.0 + W_COST_RATIO),
            "the Auto anchor must reproduce the pre-plumb bar EXACTLY"
        );
        // STRICTLY MONOTONE ACROSS THE WHOLE DIAL, not merely at the presets.
        let mut prev = 0.0_f64;
        let mut d = BULK / 4.0;
        while d < REALTIME * 4.0 {
            let b = request_bar(d);
            assert!(b > prev, "the bar must rise strictly with the price at delta={d}");
            assert!(b > 0.0 && b < 1.0, "the bar must stay a proper level at delta={d}");
            prev = b;
            d *= 1.05;
        }
        // +/-2 % NUDGES AT EVERY NAMED POINT. A behaviour step across a preset
        // is a defect even when each side is individually correct.
        for point in [REALTIME, AUTO, BULK] {
            let (lo, hi) = (request_bar(point * 0.98), request_bar(point * 1.02));
            let mid = request_bar(point);
            assert!(lo < mid && mid < hi, "the dial must not be flat at {point}");
            // **THE BAR IS NEVER MORE SENSITIVE THAN THE PRICE ITSELF.**
            // `bar = c/(1+c)` has `d(ln bar) = d(ln c)/(1+c)`, so a +/-2 %
            // nudge in delta (a 4 % span) can move the bar by AT MOST 4 %,
            // and by far less at the Realtime end. That upper bound IS the
            // continuity claim: the map never amplifies, so no preset can
            // hide a step behind a gain.
            assert!(
                (hi - lo) / mid <= 0.0401,
                "a +/-2 % nudge at delta={point} moved the bar by {:.4} of \
                 itself -- more than the price itself moved, so the map \
                 AMPLIFIES and a preset could hide a step",
                (hi - lo) / mid
            );
        }
    }

    /// **`l*` FALLS AS THE PRICE RISES, ON ONE FIXED HISTOGRAM.** 16.83.8's
    /// delta-monotonicity: *a latency-priced contract waits less*. The data are
    /// identical at all three evaluations -- only the contract moves -- so a
    /// difference can only be the law's own response to the dial.
    #[test]
    fn the_request_lateness_falls_as_the_contract_price_rises() {
        let feed = |g: &mut LateGauge| {
            // A decaying residual self-heal probability: holes that heal by
            // their OWN original at 1 / 4 / 16 / 64 ms, plus a population that
            // never heals itself at all (closed by a repair, very late).
            for (us, k) in [(1_000u64, 40u32), (4_000, 30), (16_000, 20), (64_000, 10)] {
                for _ in 0..k {
                    g.note_hole(HoleOutcome::Original, false, us, us);
                }
            }
            for _ in 0..100 {
                g.note_hole(HoleOutcome::Repair, false, 500_000, 500_000);
            }
        };
        let lstar_at = |d: f64| {
            let mut g = LateGauge::new(d);
            feed(&mut g);
            let (l, knee) = g.request_lateness();
            assert!(!knee, "no knee was fed, so the inequality must be what bound");
            l.expect("a fed gauge has a threshold")
        };
        let (rt, auto, bulk) = (lstar_at(REALTIME), lstar_at(AUTO), lstar_at(BULK));
        assert!(rt <= auto && auto <= bulk, "l* must fall as delta rises: {rt} {auto} {bulk}");
        assert!(rt < bulk, "the dial must actually move l*: {rt} == {bulk}");
        // And it moves MONOTONICALLY across the whole dial, not only at the
        // three named points.
        let mut prev = u64::MAX;
        let mut d = BULK;
        while d < REALTIME {
            let l = lstar_at(d);
            assert!(l <= prev, "l* rose with the price at delta={d}: {l} > {prev}");
            prev = l;
            d *= 1.2;
        }
    }

    /// **THE `pi0 -> 0` LIMIT IS TODAY'S TRIGGER AT EVERY POINT OF THE DIAL.**
    /// 16.83.2: with no true-heal population the inequality holds at `l = 0`
    /// whatever the price is, so the law says REQUEST IMMEDIATELY -- which is
    /// the shipped machine. A law whose corner depended on the contract would
    /// be a different law at the single-path cells.
    #[test]
    fn the_pi0_zero_limit_is_todays_trigger_at_every_price() {
        for d in [REALTIME, AUTO, BULK, 1e-9, 1e6] {
            let mut g = LateGauge::new(d);
            for _ in 0..50 {
                g.note_hole(HoleOutcome::Repair, false, 30_000, 30_000);
            }
            assert_eq!(g.pi0(), Some(0.0), "delta={d}");
            assert_eq!(
                g.request_lateness(),
                (Some(0), false),
                "pi0 -> 0 must give l* = 0 -- today's trigger -- at delta={d}"
            );
        }
    }

    /// **THE KNEE BINDS AT EVERY PRICE, AND THE CAP IS THE SAME NUMBER.**
    /// 16.83.2's `pi0 -> 1` limit: the inequality has no solution, the `AND`
    /// takes over, and `(H - d)+` is a property of the STORE, not of the
    /// contract -- which is exactly what makes `KNEE-BOUND` a statement about
    /// `RWM_STORE_GAIN` and not about the request clock.
    #[test]
    fn the_knee_binds_at_every_price_and_the_cap_is_price_free() {
        for d in [REALTIME, AUTO, BULK] {
            let mut g = LateGauge::new(d);
            for _ in 0..100 {
                g.note_hole(HoleOutcome::Original, false, 500_000, 500_000);
            }
            g.note_knee(20_000);
            g.note_source_resolution(5_000);
            let (lstar, knee) = g.request_lateness();
            // Realtime's bar is 0.990..., and a population that heals itself
            // 100 % of the time never crosses it -- so the cap binds at every
            // point of the dial and the answer is the STORE's number.
            assert!(knee, "the cap must bind at delta={d}");
            assert_eq!(lstar, Some(13_432), "(H - d)+ is price-free: delta={d}");
        }
    }

    /// The line carries the price and the level it produced, so a printed
    /// threshold is auditable without knowing which arm wrote it.
    #[test]
    fn the_line_prints_the_price_and_the_bar_it_produced() {
        let mut g = LateGauge::new(BULK);
        g.note_hole(HoleOutcome::Original, false, 1_000, 1_000);
        let l = g.line(false);
        assert!(l.contains("delta=0.00500"), "{l}");
        assert!(l.contains("bar=0.0099"), "{l}");
        let mut a = LateGauge::default();
        a.note_hole(HoleOutcome::Original, false, 1_000, 1_000);
        assert!(a.line(false).contains("delta=0.50000 bar=0.5000"), "{}", a.line(false));
    }

    /// `[RANK]` reports the deficit and the tail over-count, and its census
    /// fields are the LATEST reading while the worst case is cumulative.
    #[test]
    fn rank_reports_the_deficit_and_keeps_the_worst_case() {
        let mut g = RankGauge::default();
        assert!(!g.is_fed());
        g.note(40, 12, 7);
        g.note(3, 1, 2);
        assert!(g.is_fed());
        let l = g.line();
        assert!(
            l.starts_with("[RANK] holes=3 pivots=1 deficit=2 tail_overcount=2 max_deficit=28"),
            "{l}"
        );
        assert!(l.contains("empty=0 reports=2"), "{l}");
        // A caught-up frontier is COUNTED, not silently skipped.
        g.note(0, 0, 0);
        assert!(g.line().contains("empty=1 reports=3"), "{}", g.line());
    }
}
