//! The `[HOLD]` hole-outcome gauge (observation only).

use super::*;

/// The gauge's observation window: how many hole outstanding-time samples it
/// retains per path.
///
/// A declared resource bound, stated outside any law: it selects nothing. It
/// costs 1000 x 4 B = 4 KiB per path.
pub const HOLD_OBS_WINDOW: usize = 1000;

/// The `[HOLD]` gauge: hole outcomes per original path, observation only.
///
/// It carries `fed`/`samp_n` and the outstanding-time quantiles of the
/// resolved holes, the closure-class table (self-heal / young retransmit /
/// closed by retransmit), the same-path versus cross-path report split, and
/// the ripe-at-first-report reading. `evals` counts the fires that reached
/// the gap-repair gate. Nothing here reaches a wire byte.
pub(crate) struct HoldDownGauge {
    site: &'static str,
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
    /// Per path: the freshest [`HOLD_OBS_WINDOW`] hole-resolution times, µs,
    /// FIFO in arrival order.
    pub(crate) win: std::collections::HashMap<u32, std::collections::VecDeque<u32>>,
    /// Per path: total samples ever fed. Distinguishes "never filled" from
    /// "filled and rolled".
    pub(crate) fed: std::collections::HashMap<u32, u64>,
    /// Per path: fires that reached the gap-repair gate (`u32::MAX` holds the
    /// fires on a hole this gauge never saw reported, e.g. a timer fire).
    pub(crate) ctr: std::collections::HashMap<u32, u64>,
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
    /// Per original path: the `closed_retx` count split by the SEND leg of the
    /// retransmit that closed the hole (leg id -> count). Sums to
    /// `cls[..][2]` by construction; printed as `cx_leg=`.
    cxl: std::collections::HashMap<u32, std::collections::BTreeMap<u32, u64>>,
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

/// One stamped hole. Read-only bookkeeping; no decision site reads it.
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
    pub(crate) fn new(site: &'static str) -> Self {
        Self {
            site,
            first: std::collections::BTreeMap::new(),
            cls: std::collections::HashMap::new(),
            clsh: std::collections::HashMap::new(),
            cxl: std::collections::HashMap::new(),
            xps: std::collections::HashMap::new(),
            age: std::collections::HashMap::new(),
            win: std::collections::HashMap::new(),
            fed: std::collections::HashMap::new(),
            ctr: std::collections::HashMap::new(),
            gen: false,
        }
    }

    pub(crate) fn set_send_generation(&mut self, gen: bool) {
        self.gen = gen;
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

    /// One fire reaching the gap-repair gate. Observation only: counted on
    /// the hole's original path, or the unattributed bucket when this gauge
    /// never saw the hole reported (a timer fire).
    pub(crate) fn on_fire(&mut self, seq: u64) {
        let path = self.first.get(&seq).map_or(u32::MAX, |r| r.orig_path);
        *self.ctr.entry(path).or_insert(0) += 1;
    }

    /// Feed one sample into a path's window.
    fn feed(&mut self, path: u32, sample_us: u64) {
        let s = sample_us.min(u32::MAX as u64) as u32;
        let w = self.win.entry(path).or_default();
        if w.len() >= HOLD_OBS_WINDOW {
            w.pop_front();
        }
        w.push_back(s);
        *self.fed.entry(path).or_insert(0) += 1;
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
                if let (2, Some((_, pr))) = (k, rec.retx) {
                    *self.cxl.entry(rec.orig_path).or_default().entry(pr).or_insert(0) += 1;
                }
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
    /// distribution as this sender saw it. A gauge, never a clock: read only
    /// by `line` at teardown.
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
        let evals = self.ctr.get(&path).copied().unwrap_or(0);
        let us = |v: Option<u64>| v.map_or("-".to_string(), |x| x.to_string());
        // `-` iff the denominator is zero — an absent fraction is never a
        // measured 0 (the `[SUCC]` convention).
        let cls = self.cls.get(&path).copied().unwrap_or([0; 3]);
        let clsh = self.clsh.get(&path);
        let xps = self.xps.get(&path).copied().unwrap_or([0; 3]);
        // `leg:count` pairs ascending by leg, `-` when no retransmit closed a hole.
        let cx_leg = self.cxl.get(&path).filter(|m| !m.is_empty()).map_or("-".to_string(), |m| {
            m.iter().map(|(l, n)| format!("{l}:{n}")).collect::<Vec<_>>().join(",")
        });
        let age = self.age.get(&path);
        let frac = |num: u64, den: u64| {
            if den == 0 {
                "-".to_string()
            } else {
                format!("{:.4}", num as f64 / den as f64)
            }
        };
        format!(
            "[HOLD] site={} path={} gen={} samp_n={} fed={} \
             obs_p50_us={} obs_p90_us={} obs_p99_us={} evals={} \
             hn_n={} hn_p50_us={} hn_p90_us={} hy_n={} hy_p50_us={} hy_p90_us={} \
             cx_n={} cx_p50_us={} cx_p90_us={} sp_n={} xp_n={} up_n={} xp_frac={} \
             age_n={} age_ripe={} ripe_frac={} age_p50_us={} age_p90_us={} \
             thr_p50_us={} cx_leg={} fa_class={:.4}",
            self.site,
            if path == u32::MAX {
                "-".to_string()
            } else {
                path.to_string()
            },
            if self.gen { 1 } else { 0 },
            self.win.get(&path).map_or(0, |w| w.len()),
            self.fed.get(&path).copied().unwrap_or(0),
            us(self.obs_q(path, 0.50)),
            us(self.obs_q(path, 0.90)),
            us(self.obs_q(path, 0.99)),
            evals,
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
            cx_leg,
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
            crate::readout!("{}", self.line(p));
        }
    }
}
