//! Per-symbol placement (paper §5.7): the temperature and
//! the `Scheduler` placement methods (`place_symbol`, `place_probs`,
//! `place_costs`, ...).

use super::*;

/// Placement softmax temperature.
///
/// The placement cost is measured in units of the fastest path's SRTT (the
/// load term is `E_i(load)/ref_srtt`, ≈ 0.5 for the idle fast path). `T` is
/// therefore the softness of the water-filling transition in units of a fast
/// one-way delay: two paths whose costs differ by `T` place at odds e:1 ≈
/// 2.7:1. `T → 0` is the paper's strict best-path (argmin) limit; larger `T`
/// dithers and pulls more traffic onto a slower path (more aggregation, more
/// head-of-line risk on a reliable in-order stream). Underived
/// (open-constants register, paper §11.2). A derived
/// `T = (√6/π)·σ̂_e/ref` arm was removed (tag `archive/RWM_PLACE_T_DERIVED`).
pub(crate) const PLACE_TEMPERATURE: f64 = 0.15;

/// The effective placement temperature: `PLACE_TEMPERATURE`, overridable once
/// per process via the `RWM_PLACE_T` env var.
pub(crate) fn place_temperature() -> f64 {
    crate::gates::get().place_t
}

/// The resolve-time read behind [`place_temperature`].
pub(crate) fn resolve_place_temperature() -> f64 {
    std::env::var("RWM_PLACE_T")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|t| *t > 0.0 && t.is_finite())
        .unwrap_or(PLACE_TEMPERATURE)
}

/// Floor (seconds) for the SRTT reference that de-dimensionalises the
/// propagation-preference term — a div-by-zero guard for the pre-first-sample
/// window, not a tuning knob (any positive value cancels once real RTTs land).
pub(crate) const PLACE_REF_FLOOR_SECS: f64 = 0.001;

impl Scheduler {
    /// Per-symbol placement law (paper §5.7) — one continuous marginal-cost
    /// rule that stripes source and repair symbols across paths with no load
    /// regimes and no case splits, for the reliable window pipeline.
    ///
    /// For each active path `i`:
    ///
    /// ```text
    ///   cost_i = Ê_i(load) / ref_srtt            ← frontier-completion-time
    ///          + w_bw · r_i                       ← correction/bandwidth burden
    ///          + w_div · ρ_fate(s, i)             ← repair diversity
    ///   P(i) ∝ exp(−cost_i / T)
    /// ```
    ///
    /// Two choices make it work for a reliable in-order stream:
    ///
    /// (1) `E_i(load)` is the expected frontier-completion time
    ///     (`expected_delivery_load`): queue drain at the path's pacing rate
    ///     `cwnd/SRTT`, plus propagation, plus loss recovery. Being in time, it
    ///     is capacity-aware — a backlog on the slow path costs more real time —
    ///     so the law water-fills by capacity. A dimensionless `in_flight/cwnd`
    ///     fill instead fills both paths to equal fraction, over-loading the
    ///     low-capacity path and collapsing an in-order frontier.
    ///
    /// (2) `E_i(load)` carries unit weight, not `w_lat`: on a reliable in-order
    ///     stream latency-to-frontier is the completion cost itself, so it is
    ///     always weighted. `w_bw` still adds the wire-waste (loss) penalty that
    ///     is the Bulk-vs-Realtime dial, and the queue term that drives
    ///     water-filling is never gated away.
    ///
    /// Terms:
    ///   - `Ê_i(load)/ref_srtt`: de-dimensionalised by the fastest path's SRTT,
    ///     O(1) and comparable across heterogeneous RTTs; rises continuously
    ///     with `in_flight`, equalised across paths at the water-filling point.
    ///   - `r_i`: correction rate / loss burden (clamped for dead paths).
    ///   - `ρ_fate(s,i)`: repair symbols only — the fraction of the symbols this
    ///     repair covers that path `i` already carried (a repair riding its own
    ///     coverage adds no diversity). `covered_paths` holds one entry per
    ///     covered source symbol (with multiplicity); the continuous form of
    ///     `best_repair_path_avoiding`. Zero for source symbols.
    ///
    /// Temperature `T = PLACE_TEMPERATURE` runs from strict best-path
    /// (T → 0 ⇒ argmin) to dithering. A single path is always chosen.
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

    /// Cross-path repair placement ("repair rides the spare path"; env
    /// `RWM_XPATH_REPAIR`).
    ///
    /// The marginal-cost `place_symbol(true, ..)` softmax biases repair toward
    /// the fast path (lowest frontier-completion-time), so proactive repair
    /// competes with systematic source on the same link: buying early presence
    /// costs source bandwidth. This instead routes repair to the path with the
    /// most spare capacity relative to its load (`max spare_capacity`), i.e.
    /// the underutilized path — the slow path once the fast path is
    /// source-saturated. A fast-path loss is then covered by repair already in
    /// flight on the slow path, without displacing fast-path source.
    ///
    /// Symmetric paths have equal spare, so the near-tie set is picked
    /// uniformly at random — no hard-argmax concentration. Only a genuine
    /// spare-capacity asymmetry (fast saturated / slow idle) steers repair to
    /// one path. Falls back to the softmax placement when fewer than two paths
    /// are up.
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

    /// The softmax placement distribution over paths. Exposed for
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

    /// The cold price of the placement law's SRTT reference
    /// (`RWM_COLD_PLACE`): the active set's fastest measured srtt, or `None`
    /// with the gate off / nothing measured yet. Shared with
    /// the temperature so it divides by exactly the same `ref` the
    /// costs are de-dimensionalised by - two `ref`s would make `c_i/T`
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

    /// `s_i`: the leg's own measurement when it has one, the cold price when
    /// it does not, and `p.srtt()` when there is no cold price. One
    /// expression, no `if cold`.
    fn place_srtt_of(p: &PathState, cold_srtt: Option<f64>) -> f64 {
        p.srtt_measured()
            .map(|d| d.as_secs_f64())
            .or(cold_srtt)
            .unwrap_or_else(|| p.srtt().as_secs_f64())
    }

    /// `ref_srtt` - the fastest active path's SRTT, floored; see
    /// `place_cold_srtt`.
    fn place_ref_srtt(&self, cold_srtt: Option<f64>) -> f64 {
        let ref_srtt = self
            .paths
            .values()
            .filter(|p| p.active)
            .map(|p| Self::place_srtt_of(p, cold_srtt).max(PLACE_REF_FLOOR_SECS))
            .fold(f64::INFINITY, f64::min);
        if ref_srtt.is_finite() {
            ref_srtt
        } else {
            PLACE_REF_FLOOR_SECS
        }
    }

    /// `min_j wire_j` over the active candidates: the second link of the
    /// availability chain `w^_i = wire_i ?: min_j wire_j ?: s_i`. `None` when
    /// no active leg has a wire RTT yet.
    fn place_wire_min(&self) -> Option<f64> {
        let m = self
            .paths
            .values()
            .filter(|p| p.active)
            .filter_map(|p| p.wire_rtt())
            .map(|d| d.as_secs_f64())
            .fold(f64::INFINITY, f64::min);
        m.is_finite().then_some(m)
    }

    /// `E_i(load)` of one leg (paper §5.7), the single seat both the price
    /// (`place_costs`) and the `[ETA]` stamp (`stamp_placement_eta`) read, so
    /// the sender's prediction is the quantity the law priced:
    ///
    /// ```text
    ///   E_i = (I_i/W_i)*s_i + (1/2 + eps_i)*w^_i
    ///   w^_i = wire_i ?: min_j wire_j ?: s_i
    /// ```
    ///
    /// The `?:` chain is an information-availability fallback, as in
    /// `place_srtt_of`; it selects no law.
    fn place_e(&self, p: &PathState, cold_srtt: Option<f64>, wire_min: Option<f64>) -> f64 {
        let s_i = Self::place_srtt_of(p, cold_srtt);
        let w_hat = p.wire_rtt().map(|d| d.as_secs_f64()).or(wire_min).unwrap_or(s_i);
        p.expected_delivery_load_at(s_i, w_hat)
    }

    /// Per-path marginal placement cost, over all active paths.
    ///
    /// There is deliberately no hard filter on spare capacity: it would make a
    /// path vanish discontinuously at `in_flight == cwnd` — the threshold jump
    /// the monotonic-spillover requirement forbids. The `in_flight/cwnd`
    /// congestion term is the continuous form: it climbs past
    /// 1.0 under overdraft, driving a saturated path's softmax mass toward zero
    /// smoothly without ever removing it, so placement never drops a symbol
    /// (the send loop's pacing/backpressure remains the real capacity gate).
    pub(crate) fn place_costs(&self, is_repair: bool, covered_paths: &[PathId]) -> Vec<(PathId, f64)> {
        // ── The cold price (`RWM_COLD_PLACE`, anchor-hygiene rule 1) ───────
        // What one second of a leg that has never been measured is worth.
        // Under the gate: the active set's fastest measured srtt — another
        // leg's measurement, not a constant. Off (or with nothing measured
        // yet): `None`, and `place_srtt_of` falls back to `p.srtt()`.
        //
        // `s_i` is read through `place_srtt_of` identically for the reference,
        // the deadline and the load term. Once every leg has a sample,
        // `place_srtt_of == p.srtt()` at every leg and the gate is inert.
        let cold_srtt: Option<f64> = self.place_cold_srtt();
        let srtt_of = |p: &PathState| Self::place_srtt_of(p, cold_srtt);

        let ref_srtt = self.place_ref_srtt(cold_srtt);

        let w_bw = self.weights.w_bw;
        let w_div = self.w_div();

        // The wire-clocked base (paper §5.7): see `place_e`. `wire_min` is the
        // second link of its availability chain, read once per placement.
        let wire_min = self.place_wire_min();

        let covered_total = covered_paths.len() as f64;

        let cost_of = |p: &PathState| -> f64 {
            // Frontier-completion-time — the always-on load term (unit weight),
            // de-dimensionalised by the fastest SRTT so it is O(1). This single
            // term carries both the congestion signal (queue drain at the pacing
            // rate) and the propagation preference; because it is expressed in
            // time it is capacity-aware, so it water-fills by capacity rather
            // than over-loading the slow path. The queue term keeps the echo
            // `s_i` and is exactly 0 at I_i = 0; the base reads the wire RTT.
            let srtt_i = srtt_of(p);
            let e_i = self.place_e(p, cold_srtt, wire_min).max(0.0);
            let load = e_i / ref_srtt;
            // The `w_cold` bind gauge: this leg's wire RTT was absent and the
            // `min_j wire_j ?: s_i` fallback priced it.
            let (c, n, calls, floor) = self.place_wire_gauge.get();
            self.place_wire_gauge
                .set((c + u64::from(p.wire_rtt().is_none()), n + 1, calls, floor));
            // Bandwidth/correction burden (loss/wire waste); the hint's w_bw
            // dial. w_lat does not gate placement: on a reliable in-order stream
            // latency-to-frontier is the completion cost itself, already carried
            // by `load` at unit weight, not a per-hint preference.
            let r = p.correction_rate();
            // The cold-r price: `correction_rate()` is `inf` on a path with no
            // loss estimate yet, priced at the literal 10.0 (an open constant,
            // paper §11.2). Its bind is counted (`[ETA] site=sender cold_r=`),
            // because every clamp owes a bind-fraction gauge.
            //
            // The cold-GE price: the derived `w_div` reads a Gilbert-Elliott
            // burst probability off the path, and a path with no SRTT
            // measurement has no burst model either, so its diversity term
            // rests on the `w_div` literal alone. Counted for the same reason;
            // no term of the cost changes.
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
            // ── Placement arm 3 (`RWM_PLACE_WDIV_DERIVED`) ────────────────
            // What a correlated repair costs is the excess probability that
            // one burst takes both legs, priced at the round it wastes:
            //
            //     V_i = fate_i * (p_BB,i - eps_i)+ * srtt_i / ref
            //
            // and that excess vanishes on a memoryless channel, which the
            // shipped `w_div = 1.0` does not. Cold rule: a path whose
            // Gilbert-Elliott estimator is not yet valid has no burst model,
            // so it keeps the shipped term - the case the `cold_ge` gauge
            // above already counts. `fate_i == 0` for every source symbol, so
            // this arm cannot move a source placement at all.
            let ge = p.estimator.ge_estimator();
            let div = if self.place_wdiv_derived && ge.is_valid() {
                fate * (1.0 - ge.p_bg() - p.estimator.loss_rate()).max(0.0) * srtt_i / ref_srtt
            } else {
                w_div * fate
            };
            load + w_bw * r + div
        };

        // Deterministic tie-break: `self.paths` is a `HashMap`, so its
        // iteration order varies between processes. The softmax normalisation
        // is a sum, so no probability changes -- but the degenerate branch of
        // `place_probs_with_temperature` resolves an exact tie with `min_by`
        // (which keeps the first minimum) and `place_symbol`'s inverse-CDF
        // walk consumes the candidates in this order. Sorting by id makes both
        // reproducible without moving any probability. The ref-floor witness is
        // a running minimum over the same pass, not a column.
        let mut raw_ref = f64::INFINITY;
        let mut out: Vec<(PathId, f64)> = self
            .paths
            .values()
            .filter(|p| p.active)
            .map(|p| {
                raw_ref = raw_ref.min(srtt_of(p));
                (p.id, cost_of(p))
            })
            .collect();
        out.sort_unstable_by_key(|(id, _)| *id);
        // The `ref_fl` bind gauge: did the 1 ms floor of `ref` bind (the raw
        // fastest srtt is below it)?
        let floor_now = raw_ref.is_finite() && raw_ref < PLACE_REF_FLOOR_SECS;
        let (c, n, calls, floor) = self.place_wire_gauge.get();
        self.place_wire_gauge.set((c, n, calls + 1, floor + u64::from(floor_now)));
        out
    }

    /// Move the accumulated `place_costs` cold-price binds into the `[ETA]`
    /// gauge. Called at the sender's report cadence and nowhere else.
    pub fn drain_place_bind(&mut self) {
        let (r, ge, n) = self.place_bind.replace((0, 0, 0));
        self.eta.add_place_bind(r, ge, n);
        // The wire base's bind gauges: fallback taken and ref floor bound.
        let (cold, n, calls, floor) = self.place_wire_gauge.replace((0, 0, 0, 0));
        self.eta.add_place_wire(cold, n, calls, floor);
    }

    /// Stamp the sender's own prediction for a placement and return it, in
    /// µs (0 = no prediction). `E_picked` is the same `E_i` the law priced
    /// (`place_e`, one seat), so the receiver's residual `realized - predicted`
    /// measures the quantity placement acted on.
    pub(crate) fn stamp_placement_eta(&mut self, path: PathId, send_ts_us: u64) -> u64 {
        let cold_srtt = self.place_cold_srtt();
        let wire_min = self.place_wire_min();
        let e = self
            .path(path)
            .map(|p| self.place_e(p, cold_srtt, wire_min))
            .unwrap_or(0.0);
        // Non-finite (a path with no cwnd yet) reads as no prediction — the
        // 0 sentinel — rather than as a fabricated number.
        let us = if e.is_finite() && e > 0.0 { (e * 1e6) as u64 } else { 0 };
        self.eta_mut().stamp(path, send_ts_us, us);
        us
    }
}
