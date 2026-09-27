//! RWM per-symbol placement (paper §16.3/§16.81): the frontier term, the
//! temperature, and the `Scheduler` placement methods (`place_symbol`,
//! `place_probs`, `place_costs`, ...). Moved verbatim out of
//! `scheduler/mod.rs` (cleanup Stage 3).

use super::*;

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
pub(crate) fn place_store_terms() -> (f64, bool) {
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
pub(crate) fn place_wall_now_us() -> u64 {
    crate::net::now_us()
}

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
pub(crate) fn place_temperature() -> f64 {
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

impl Scheduler {
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
}
