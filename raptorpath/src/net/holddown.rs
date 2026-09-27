//! The hold-down clock (paper §7.4), its `[HOLD]` gauge, the sweep-timeout
//! and hole-refresh laws, and the derived-round echo.

use super::*;

// ── The hold-down clock (experiment arm, `RWM_HOLDDOWN_Q`) ───────────────
//
// Almost every recovery fire answers a receiver gap report rather than a
// timer. The recovery clocks elsewhere set the timer; this one sets the other
// clock: how long the sender waits on a REPORTED hole before answering it with
// a repair.
//
// It is the quantile-native order statistic `T(q) = W_q(1 − q)` with `K = 10`,
// under the same window law and unavailability rule, evaluated on the
// sender's hole-resolution stream instead of the ack stream (paper §7.4).
//
// Nothing ships reading `RWM_HOLDDOWN_Q`. A shipped hold-down would read
// `q*(δ, ρ, r)` from the stationarity condition, continuous in the triangle.

/// The control's observation window: how many hole outstanding-time samples
/// the gauge retains on an arm that commands no level.
///
/// A declared resource bound, stated outside the law: it selects nothing and
/// no arm reads a quantile of it as a clock. It is the largest window on the
/// arm grid (`N(0.010) = 1000`), so the control's distribution is comparable
/// with the arms, and it costs 1000 × 4 B = 4 KiB per path.
pub const HOLD_OBS_WINDOW: usize = 1000;

/// `T(q)`'s window law at the tail level `a = 1 − q`.
///
/// The hold-down commands a level `q` of the hole-resolution distribution, so
/// the exceedance clause binds on `1 − q`: `N = max(⌈K/(1−q)⌉, 2K)`, `K = 10`.
/// This is the same [`qnative_window_n`] reached from the other end, not a
/// second law.
///
/// `None` for a `q` outside `(0, 1)`: at `q ≤ 0` the hold-down is zero, which
/// is the shipped behaviour and is expressed by the gate being absent; at
/// `q ≥ 1` the window law diverges. Garbage therefore resolves to absent.
///
/// The floor is 0.524: `N` is flat at `2K = 20` for every `a ≥ 0.5`, and the
/// level such a window commands is `1 − K/(N+1) = 1 − 10/21 = 0.5238`. No
/// lower level is expressible by this construction.
pub fn holddown_window_n(q: f64) -> Option<usize> {
    if !q.is_finite() || q <= 0.0 || q >= 1.0 {
        return None;
    }
    qnative_window_n(1.0 - q)
}

/// `T(q) = Y_(N−K+1)` — the hold-down, in µs, or `None` when the window law
/// has not been satisfied.
///
/// `None` is information availability, never a mode: with fewer than `N`
/// samples the `K`-th largest is a quantile of a shorter window at a different
/// level, so the caller falls through to the shipped "answer the report now".
/// An arm whose `law_n = 0` never ran its own law and its row is void.
pub fn holddown_us(window: &[u32], q: f64) -> Option<u64> {
    if !q.is_finite() || q <= 0.0 || q >= 1.0 {
        return None;
    }
    qnative_recovery_round_us(window, 1.0 - q)
}

/// The `[HOLD]` gauge — the hold-down arm's own instrument, per path.
///
/// It carries:
///
/// * the live `T` estimate and its `n`, per path, so a row whose window never
///   filled reads as such rather than pooled with one that did;
/// * `evals` / `law_n`, the bind-fraction gauge the formula-first clamp rule
///   owes any law that adds a bound;
/// * `sup` / `emit`, the suppression the arm exists to produce;
/// * the realized hold-down delay as a distribution, because a mean would
///   hide the tail the level commands;
/// * `fed`, the estimator's intake, so "the window never filled" and "the
///   window filled and the law still did not run" are distinguishable.
///
/// Two-sided: the gauge is constructed and `evals` counted on the disarmed
/// arm too, and its line prints `q=unset` there (`docs/measurement-discipline.md`
/// rule 15). The disarmed arm allocates nothing that reaches a wire byte;
/// `sup` is structurally zero and the engine is byte-identical.
///
/// Observation only apart from the one suppression the gate exists to make.
pub(crate) struct HoldDownGauge {
    site: &'static str,
    /// The resolved level, or `None` on the shipped arm. A number, never a
    /// branch: every field below is computed identically either way, and the
    /// only thing `None` decides is that `should_hold` returns `false` — the
    /// `T = 0` limit, which is the shipped behaviour.
    q: Option<f64>,
    /// `N(1−q)` — the window law's requirement, resolved once so the line, the
    /// ring's capacity and the estimator cannot drift apart. `None` on the
    /// control, where no law is in force.
    n_req: Option<usize>,
    /// The ring's capacity, which is `n_req` on a treatment arm and
    /// [`HOLD_OBS_WINDOW`] on the control.
    ///
    /// The estimator observes on every arm, so the control measures the
    /// unforced outstanding-time distribution the arms are compared against.
    /// This is observation only: `should_hold` still returns `false` when `q`
    /// is absent, so the control's wire behaviour is the shipped machine's.
    n_obs: usize,
    /// seq → the hole's record: first report time, the path its original flew
    /// on, and the retransmit this sender emitted for it if any.
    /// `BTreeMap` because retirement is a frontier range, which a hash map
    /// cannot do without scanning the whole map on every ack.
    ///
    /// The origin is the sender's own first report, not the receiver's
    /// detection (which `[SUCC]` times from); the two differ by the report's
    /// propagation and the receiver's report cadence. That is why the
    /// estimator is online and self-measuring rather than seeded from
    /// `[SUCC]`'s quantiles.
    pub(crate) first: std::collections::BTreeMap<u64, HoleRec>,
    /// Per path: the freshest `N(1−q)` observed hole-resolution times, µs,
    /// FIFO in arrival order. Capped at `N`, so the ring is exactly the window
    /// the law reads and `make_contiguous` hands the law its own slice; a
    /// longer slice would read a different level of a longer window.
    ///
    /// Resource bound, stated outside the law: `N ≤ 8192` by
    /// [`QNATIVE_WINDOW_MAX`], 4 B per sample, so ≤ 32 KiB per path.
    pub(crate) win: std::collections::HashMap<u32, std::collections::VecDeque<u32>>,
    /// Per path: the live `T` estimate, recomputed on the feed path and read
    /// `O(1)` on the eval path.
    ///
    /// A hold-down is consulted once per reported hole per report, far more
    /// often than the recovery-timer cadence, so the `O(N)` selection runs on
    /// the feed (one per resolved hole) and the eval path is a map lookup. It
    /// is a selection, never a sort (`select_nth_unstable`).
    pub(crate) t_us: std::collections::HashMap<u32, u64>,
    /// Per path: total samples ever fed. Distinguishes "never filled" from
    /// "filled and rolled".
    pub(crate) fed: std::collections::HashMap<u32, u64>,
    /// Per path: (evals, law_n, sup, emit).
    pub(crate) ctr: std::collections::HashMap<u32, [u64; 4]>,
    /// Per path: the realized hold-down delay — the age at which a held hole
    /// finally passed the gate. Log-bucket, ~12 kB fixed, the same `Hist`
    /// `[SUCC]` reports its quantiles from, so the two are comparable.
    hd: std::collections::HashMap<u32, crate::net::succ::Hist>,
    /// The closure-class split, per original path:
    /// `[heal_noretx, heal_retx_young, closed_retx]`. Their sum is `fed` on
    /// that path by construction: the resolutions this gauge feeds are the
    /// resolutions it classifies.
    ///
    /// * `heal_noretx` — resolved and no retransmit was ever emitted for the
    ///   hole: the true self-heal class.
    /// * `heal_retx_young` — a retransmit was emitted, but the resolution came
    ///   sooner than `srtt/2` on the retransmit's path after it, so the
    ///   original filled the hole and the copy was spurious.
    /// * `closed_retx` — the resolution came at or after `srtt/2` past the
    ///   retransmit; attributed to the retransmit.
    ///
    /// The `srtt/2` split is the same half-RTT the age gate uses. It is a
    /// classifier, not a law, and gates nothing; the constant is open.
    cls: std::collections::HashMap<u32, [u64; 3]>,
    /// Per original path, per class: the resolution-time distribution. The
    /// true-heal CDF `F` the theory needs is `clsh[0]` (and `clsh[1]`), which
    /// is why the classes carry their own histograms rather than one pooled.
    clsh: std::collections::HashMap<u32, [crate::net::succ::Hist; 3]>,
    /// Per original path: `[same_path, cross_path, unattributed]` — did the
    /// gap report whose absence resolved the hole arrive on the same path the
    /// original flew? The third slot is the sender's own tail sweep, which
    /// carries no ack and therefore no arrival path (`u32::MAX`); it is counted
    /// separately so the three sum to the same denominator as `cls` and
    /// `xp_frac` is read on the reports only.
    xps: std::collections::HashMap<u32, [u64; 3]>,
    /// Per original path: (age of the seq's live flight at its first report,
    /// the `9/8·max(srtt, ewma)` threshold that would have judged it, how many
    /// were already at or above it). A hole already "ripe" when the receiver
    /// first mentions it cannot be suppressed by the RFC 9002 time threshold,
    /// because the age it measures includes the sender's own queue dwell.
    age: std::collections::HashMap<
        u32,
        (crate::net::succ::Hist, crate::net::succ::Hist, u64),
    >,
    /// Whether generation coding was in force. Under it the SACK→gap producer
    /// is suppressed, so the gap-report path this arm acts on is structurally
    /// empty; the line says which machine it measured, the same contract
    /// `[FCAUSE]` carries.
    gen: bool,
}

/// One stamped hole. Read-only bookkeeping: `should_hold` reads only `t0_us`
/// and `orig_path`; no other field is consulted by any decision site.
#[derive(Clone, Copy)]
pub(crate) struct HoleRec {
    /// The first time the receiver reported this hole to us, µs.
    t0_us: u64,
    /// The path the hole's original flew on.
    orig_path: u32,
    /// `(emission time µs, path)` of the first retransmit this sender emitted
    /// for the hole. A re-fire after the cooldown does not restart it.
    retx: Option<(u64, u32)>,
}

impl HoldDownGauge {
    pub(crate) fn new(site: &'static str, q: Option<f64>) -> Self {
        Self {
            site,
            q,
            n_req: q.and_then(holddown_window_n),
            n_obs: q.and_then(holddown_window_n).unwrap_or(HOLD_OBS_WINDOW),
            first: std::collections::BTreeMap::new(),
            cls: std::collections::HashMap::new(),
            clsh: std::collections::HashMap::new(),
            xps: std::collections::HashMap::new(),
            age: std::collections::HashMap::new(),
            win: std::collections::HashMap::new(),
            t_us: std::collections::HashMap::new(),
            fed: std::collections::HashMap::new(),
            ctr: std::collections::HashMap::new(),
            hd: std::collections::HashMap::new(),
            gen: false,
        }
    }

    pub(crate) fn set_send_generation(&mut self, gen: bool) {
        self.gen = gen;
    }

    /// Is the arm live, i.e. can it suppress? Keyed on `n_req`, not `q`: a `q`
    /// whose window law does not resolve (over [`QNATIVE_WINDOW_MAX`]) cannot
    /// run and must read as disarmed rather than armed-and-silent. The
    /// estimator does not consult this; only the gate does.
    pub(crate) fn armed(&self) -> bool {
        self.n_req.is_some()
    }

    /// One reported hole. Stamps the first report and never a later one: the
    /// estimand is "how long a hole stays outstanding", and a hole re-reported
    /// every `hole_nack_refresh` would otherwise reset its own clock.
    ///
    /// `reported` is the estimand's origin condition, not a mode. The clock
    /// starts when the receiver says the hole exists; the sender's own
    /// tail-sweep timer does not start it. A timer fire on a reported hole is
    /// gated by the same `T` as any other fire on it; a timer fire on a hole
    /// never reported falls through to the shipped behaviour. That is
    /// information availability, and no threshold on any dial enters it.
    ///
    /// `flight` is `(age of the seq's live flight now, the
    /// `9/8·max(srtt, ewma)` threshold for that flight's path)` when the
    /// sender holds a flight for the seq, `None` otherwise. It is recorded on
    /// the first report only, so a re-report does not re-sample it.
    pub(crate) fn on_reported(
        &mut self,
        seq: u64,
        orig_path: u32,
        now_us: u64,
        reported: bool,
        flight: Option<(u64, u64)>,
    ) {
        if !reported {
            return;
        }
        if self.first.contains_key(&seq) {
            return;
        }
        self.first.insert(seq, HoleRec { t0_us: now_us, orig_path, retx: None });
        if let Some((age_us, thr_us)) = flight {
            let e = self.age.entry(orig_path).or_insert_with(|| {
                (crate::net::succ::Hist::default(), crate::net::succ::Hist::default(), 0)
            });
            e.0.add(age_us);
            e.1.add(thr_us);
            if age_us >= thr_us {
                e.2 += 1;
            }
        }
    }

    /// The retransmit stamp. Called beside `nack_retx_at.insert`, where a copy
    /// of this seq reaches the wire; records the first copy only. Observation
    /// only.
    pub(crate) fn on_retx(&mut self, seq: u64, now_us: u64, path: u32) {
        if let Some(r) = self.first.get_mut(&seq) {
            if r.retx.is_none() {
                r.retx = Some((now_us, path));
            }
        }
    }

    /// The gate: `true` ⇒ this fire is held down and suppressed.
    ///
    /// Returns `false` — the shipped behaviour — whenever the arm is disarmed,
    /// the hole has no first-report stamp, or the window law is not yet
    /// satisfied on this path. All three are information availability: no
    /// threshold on δ or ρ selects anything here, and the only number that
    /// enters is `T`.
    pub(crate) fn should_hold(&mut self, seq: u64, now_us: u64) -> bool {
        let Some(HoleRec { t0_us: t0, orig_path: path, .. }) =
            self.first.get(&seq).copied()
        else {
            // Disarmed (nothing is ever stamped), or a fire on a hole this
            // gauge never saw reported — a timer fire, which this arm does not
            // touch. Counted at the disarmed site below so the line is
            // two-sided.
            let e = self.ctr.entry(u32::MAX).or_insert([0; 4]);
            e[0] += 1;
            e[3] += 1;
            return false;
        };
        let c = self.ctr.entry(path).or_insert([0; 4]);
        c[0] += 1;
        let Some(t) = self.t_us.get(&path).copied() else {
            // The window law has not been satisfied on this path: fall
            // through to the law below it. `law_n` stays put, so the row
            // reads as partial rather than pooled.
            c[3] += 1;
            return false;
        };
        c[1] += 1;
        let age = now_us.saturating_sub(t0);
        if age < t {
            c[2] += 1;
            true
        } else {
            c[3] += 1;
            self.hd.entry(path).or_default().add(age);
            false
        }
    }

    /// Feed one sample into a path's window and re-read the order statistic.
    /// The `O(N)` selection sits here, on the feed path, so the eval path is a
    /// map lookup (see `t_us`).
    fn feed(&mut self, path: u32, sample_us: u64) {
        let n = self.n_obs;
        let s = sample_us.min(u32::MAX as u64) as u32;
        let w = self.win.entry(path).or_default();
        if w.len() >= n {
            w.pop_front();
        }
        w.push_back(s);
        *self.fed.entry(path).or_insert(0) += 1;
        // The law's own output, only where the law is in force.
        if let (Some(nr), Some(q)) = (self.n_req, self.q) {
            if w.len() >= nr {
                if let Some(t) = holddown_us(w.make_contiguous(), q) {
                    self.t_us.insert(path, t);
                }
            }
        }
    }

    /// The resolution signal, read off the receiver's own report. A hole this
    /// sender stamped, which the receiver's newest gap report no longer lists
    /// inside the region that report covers, has been filled at the receiver.
    /// Feed its outstanding time and drop it.
    ///
    /// Resolution is not read off the cumulative ack: the frontier cannot pass
    /// a hole until every earlier hole is also filled, so that sample is a
    /// max-statistic over the whole outstanding set (head-of-line lag, not
    /// this hole's resolution) and inflates `T` by orders of magnitude. The
    /// report is per-hole and exact: a stamped seq inside the report's span
    /// that the report does not list is one the receiver has.
    ///
    /// Must run before this batch's own seqs are stamped, or every hole would
    /// be resolved by the report that first announced it.
    ///
    /// `shed` is the δ-honest shed set and is the one exclusion. A shed hole
    /// was abandoned, not resolved; feeding it would push `T` upward exactly
    /// where the contract had already stopped caring, the one direction this
    /// estimator must not be biased in. It is dropped and fed to nothing.
    ///
    /// `ack_path` is the path the gap report arrived on, and `half_srtt_of`
    /// returns `max(srtt, ewma)/2` for a path — the classifier's only input.
    /// Both are labels: the resolution set, the feed and every wire byte are
    /// unaffected by them.
    pub(crate) fn on_report(
        &mut self,
        gaps: &[(u64, u64)],
        now_us: u64,
        shed: &std::collections::BTreeSet<u64>,
        ack_path: u32,
        half_srtt_of: &dyn Fn(u32) -> u64,
    ) {
        if self.first.is_empty() {
            return;
        }
        // The span this report speaks about. A stamped seq above it is not
        // covered by this report and its absence proves nothing.
        let Some(hi) = gaps.iter().map(|&(_, b)| b).max() else {
            return;
        };
        let resolved: Vec<u64> = self
            .first
            .range(..=hi)
            .map(|(&s, _)| s)
            .filter(|&s| !gaps.iter().any(|&(a, b)| s >= a && s <= b))
            .collect();
        for s in resolved {
            if let Some(rec) = self.first.remove(&s) {
                if shed.contains(&s) {
                    continue;
                }
                let dt = now_us.saturating_sub(rec.t0_us);
                // The closure class, on the same resolutions the feed sees and
                // after the same shed exclusion, so
                // `heal_noretx + heal_retx_young + closed_retx = fed` per path.
                let k = match rec.retx {
                    None => 0usize,
                    Some((tr, pr)) => {
                        if now_us.saturating_sub(tr) < half_srtt_of(pr) {
                            1
                        } else {
                            2
                        }
                    }
                };
                self.cls.entry(rec.orig_path).or_insert([0; 3])[k] += 1;
                self.clsh.entry(rec.orig_path).or_insert_with(Default::default)[k].add(dt);
                let x = self.xps.entry(rec.orig_path).or_insert([0; 3]);
                if ack_path == u32::MAX {
                    x[2] += 1;
                } else if ack_path == rec.orig_path {
                    x[0] += 1;
                } else {
                    x[1] += 1;
                }
                self.feed(rec.orig_path, dt);
            }
        }
    }

    /// Drop every stamped hole the cumulative ack has passed. Prune only — it
    /// feeds nothing: a hole the frontier swept without a report showing it
    /// filled has a resolution time this sender never observed. The map is
    /// bounded by the outstanding set either way.
    pub(crate) fn on_retired(&mut self, ack: u64) {
        if self.first.is_empty() {
            return;
        }
        let mut above = self.first.split_off(&ack.saturating_add(1));
        std::mem::swap(&mut self.first, &mut above);
    }

    /// A quantile of the path's own observation window — the outstanding-time
    /// distribution as this sender saw it, on every arm including the control.
    ///
    /// A gauge, never a clock: read only by `line` at teardown. The hold-down
    /// `T` is `t_us`, the law's order statistic at the law's own `N(1−q)`;
    /// this is a fixed-window description of the same stream.
    pub(crate) fn obs_q(&self, path: u32, p: f64) -> Option<u64> {
        let w = self.win.get(&path)?;
        if w.is_empty() {
            return None;
        }
        let mut v: Vec<u32> = w.iter().copied().collect();
        let idx = (((p * v.len() as f64).ceil() as usize).max(1) - 1).min(v.len() - 1);
        let (_, nth, _) = v.select_nth_unstable(idx);
        Some(*nth as u64)
    }

    pub(crate) fn line(&self, path: u32) -> String {
        let c = self.ctr.get(&path).copied().unwrap_or([0; 4]);
        let us = |v: Option<u64>| v.map_or("-".to_string(), |x| x.to_string());
        let hd = self.hd.get(&path);
        // `-` iff the denominator is zero — an absent fraction is never a
        // measured 0 (the `[SUCC]` convention).
        let cls = self.cls.get(&path).copied().unwrap_or([0; 3]);
        let clsh = self.clsh.get(&path);
        let xps = self.xps.get(&path).copied().unwrap_or([0; 3]);
        let age = self.age.get(&path);
        let frac = |num: u64, den: u64| {
            if den == 0 {
                "-".to_string()
            } else {
                format!("{:.4}", num as f64 / den as f64)
            }
        };
        format!(
            "[HOLD] site={} path={} gen={} q={} n_req={} n_obs={} samp_n={} fed={} \
             t_us={} obs_p50_us={} obs_p90_us={} obs_p99_us={} \
             evals={} law_n={} sup={} emit={} hd_p50_us={} hd_p90_us={} hd_p99_us={} \
             hd_mx_us={} hd_n={} \
             hn_n={} hn_p50_us={} hn_p90_us={} hy_n={} hy_p50_us={} hy_p90_us={} \
             cx_n={} cx_p50_us={} cx_p90_us={} sp_n={} xp_n={} up_n={} xp_frac={} \
             age_n={} age_ripe={} ripe_frac={} age_p50_us={} age_p90_us={} \
             thr_p50_us={} fa_class={:.4}",
            self.site,
            if path == u32::MAX {
                "-".to_string()
            } else {
                path.to_string()
            },
            if self.gen { 1 } else { 0 },
            self.q.map_or("unset".to_string(), |v| format!("{v:.6}")),
            self.n_req.map_or("-".to_string(), |v| v.to_string()),
            self.n_obs,
            self.win.get(&path).map_or(0, |w| w.len()),
            self.fed.get(&path).copied().unwrap_or(0),
            us(self.t_us.get(&path).copied()),
            us(self.obs_q(path, 0.50)),
            us(self.obs_q(path, 0.90)),
            us(self.obs_q(path, 0.99)),
            c[0],
            c[1],
            c[2],
            c[3],
            us(hd.and_then(|h| h.quantile(0.50))),
            us(hd.and_then(|h| h.quantile(0.90))),
            us(hd.and_then(|h| h.quantile(0.99))),
            us(hd.map(|h| h.max_us())),
            hd.map_or(0, |h| h.n()),
            // The closure-class table: `hn` heal_noretx, `hy` heal_retx_young,
            // `cx` closed_retx; then the same/cross split and the
            // ripe-at-first-report reading.
            cls[0],
            us(clsh.and_then(|h| h[0].quantile(0.50))),
            us(clsh.and_then(|h| h[0].quantile(0.90))),
            cls[1],
            us(clsh.and_then(|h| h[1].quantile(0.50))),
            us(clsh.and_then(|h| h[1].quantile(0.90))),
            cls[2],
            us(clsh.and_then(|h| h[2].quantile(0.50))),
            us(clsh.and_then(|h| h[2].quantile(0.90))),
            xps[0],
            xps[1],
            xps[2],
            frac(xps[1], xps[0] + xps[1]),
            age.map_or(0, |a| a.0.n()),
            age.map_or(0, |a| a.2),
            frac(age.map_or(0, |a| a.2), age.map_or(0, |a| a.0.n())),
            us(age.and_then(|a| a.0.quantile(0.50))),
            us(age.and_then(|a| a.0.quantile(0.90))),
            us(age.and_then(|a| a.1.quantile(0.50))),
            // The sacrificial trailing constant — stderr has two writers and a
            // `tracing` write can land inside a gauge line's last field.
            RACK_SPURIOUS_BUDGET,
        )
    }
}

impl Drop for HoldDownGauge {
    fn drop(&mut self) {
        // One line per path that saw a fire, plus the unattributed bucket when
        // it is non-empty. A site that never ran the gap loop stays silent, so
        // an absent line can only be read as an unreached site and never as an
        // unset gate — the same rule `[RACK]` and `[QCLK]` use.
        let mut paths: Vec<u32> = self.ctr.keys().copied().collect();
        paths.sort_unstable();
        for p in paths {
            eprintln!("{}", self.line(p));
        }
    }
}

/// The tail-sweep timeout supplied to the sender loop: the clamped law, or
/// the derived round under `RWM_DERIVED_SWEEP` (an env A/B arm, not a dial).
pub fn sweep_timeout_us(derived: bool, srtt_us: u64, jitter_us: u64) -> u64 {
    if derived {
        derived_recovery_round_us(srtt_us, jitter_us)
    } else {
        tail_sweep_timeout_us(srtt_us)
    }
}

/// The receiver's hole-refresh cadence supplied to the reliable window
/// receiver: the clamped law, or the derived round under `RWM_DERIVED_SWEEP`.
/// With no clock at all both arms fall back to `HOLE_NACK_REFRESH_MAX`
/// (information availability, not a mode). `refresh_floor` is the clamped
/// law's band floor, read only by that arm; `HOLE_NACK_REFRESH_MIN` gives the
/// shipped cadence.
pub fn hole_refresh(
    derived: bool,
    srtt: Option<Duration>,
    jitter_us: u64,
    refresh_floor: Duration,
) -> Duration {
    match (derived, srtt) {
        (true, Some(s)) => {
            Duration::from_micros(derived_recovery_round_us(s.as_micros() as u64, jitter_us))
        }
        (true, None) => HOLE_NACK_REFRESH_MAX,
        (false, s) => hole_nack_refresh_floored(s, refresh_floor),
    }
}

/// Liveness echo for the derived recovery round, one per site per process
/// (`docs/measurement-discipline.md` rule 1: prove the site under test
/// executed; the `[GATES]` value only proves the env var was read).
///
/// Two claims, kept separate because `derived_recovery_round_us` returns
/// exactly `tail_sweep_timeout_us` wherever `2·srtt` already sits inside the
/// `[25, 100] ms` clamp — so "the derived site ran" does not imply "the
/// derived law bound", and an arm that only ran inside the clamp is
/// bit-identical to its control:
///
/// * `ACTIVE` — first evaluation at this site, with the clock that drove it.
///   Proves execution.
/// * `DIVERGED` — first evaluation whose derived round differs from the
///   clamped law. Proves the law bound, and carries both µs values.
///
/// Emitted only on the armed arm, so a battery asserts it present on
/// `RWM_DERIVED_SWEEP=1` and absent on the controls. Observation only.
#[derive(Default)]
pub(crate) struct DerivedRoundEcho {
    pub(crate) ran: bool,
    pub(crate) diverged: bool,
}

/// The phrase drivers count to prove the derived site executed.
pub(crate) const DS_ECHO_RAN: &str = "derived recovery round ACTIVE";
/// The phrase drivers count to prove the derived law bound.
pub(crate) const DS_ECHO_DIVERGED: &str = "derived recovery round DIVERGED";

impl DerivedRoundEcho {
    /// The execution echo's full text.
    pub(crate) fn ran_msg(site: &str, srtt_us: u64, jitter_us: u64, d_us: u64, l_us: u64) -> String {
        format!(
            "{DS_ECHO_RAN} (RWM_DERIVED_SWEEP: \
             round = max(2*srtt, patience_floor(jitter, srtt)), NO ceiling and zero new \
             constants, replacing 2*srtt clamped to [25, 100] ms at both recovery-clock \
             sites; RWM_DERIVED_SWEEP=0 = the shipped clamped control arm) site={site} \
             srtt_us={srtt_us} jitter_us={jitter_us} derived_us={d_us} legacy_us={l_us}"
        )
    }

    /// The binding echo's full text.
    pub(crate) fn diverged_msg(site: &str, srtt_us: u64, jitter_us: u64, d_us: u64, l_us: u64) -> String {
        format!(
            "{DS_ECHO_DIVERGED} from the clamped law (coincidence property: the two laws agree wherever 2*srtt already lies \
             inside [25, 100] ms, so this line, not the execution echo, is what proves the \
             derived round BOUND at this site) site={site} srtt_us={srtt_us} \
             jitter_us={jitter_us} derived_us={d_us} legacy_us={l_us}"
        )
    }

    /// Record one evaluation of the derived round. `derived_us` is the value
    /// the site is using; `legacy_us` is what the clamped law would return for
    /// the same clock.
    pub(crate) fn observe(
        &mut self,
        site: &str,
        srtt_us: u64,
        jitter_us: u64,
        derived_us: u64,
        legacy_us: u64,
    ) {
        if !self.ran {
            self.ran = true;
            info!("{}", Self::ran_msg(site, srtt_us, jitter_us, derived_us, legacy_us));
        }
        if !self.diverged && derived_us != legacy_us {
            self.diverged = true;
            info!("{}", Self::diverged_msg(site, srtt_us, jitter_us, derived_us, legacy_us));
        }
    }
}
