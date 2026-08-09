//! ONE scheduler read per sender-loop iteration — the `RWM_SCHED_SNAPSHOT`
//! A/B arm.
//!
//! History (net seam pass 2, 2026-08-09): `run_window_sender` takes
//! `scheduler.lock()` in 30-odd distinct places, and a dozen of them
//! independently re-derive the SAME aggregates from the SAME path set —
//! max RTprop, Σ Copa BDP anchor, Σ cwnd/srtt, max/pooled SRTT. Every
//! extraction of a later sender phase has to carry those re-derivations with
//! it, so the seam map named this the second blocker. [`SchedSnapshot`]
//! captures all of them under ONE acquisition at the top of the loop.
//!
//! ⚠ THIS IS NOT A BEHAVIOUR-PRESERVING CHANGE, and that is why it is GATED
//! OFF. Today two phases in the same iteration can read DIFFERENT scheduler
//! states: the M*-depth refresh runs near the loop top and the deficit-
//! spacing read runs after a `select!` await that may have parked for
//! milliseconds, during which acks mutate cwnd/srtt/min_rtt. Reading one
//! snapshot makes every phase agree — which is almost certainly the RIGHT
//! semantics (the skew is a real hazard: a rate read before an ack burst and
//! an RTprop read after it compose into a BDP that never existed) — but it is
//! NOT what any measurement to date was taken against. `RWM_SCHED_SNAPSHOT`
//! therefore defaults OFF: with the gate off no snapshot is captured, no
//! extra lock is taken, and every site runs its original block bit-for-bit.
//! Adjudicating the arm is an OPEN QUESTION for a later battery — see the
//! goal-gate note "Refactor: net seams 2".
//!
//! BEHAVIOUR CONTRACT (gate OFF): `SchedSnapshot::capture` is never called,
//! `sched_snap` is `None`, and each routed site's `None` arm is the verbatim
//! original expression including its own `scheduler.lock()` scope. The gate
//! adds one `Option` test per routed site and nothing else.
//!
//! BEHAVIOUR CONTRACT (gate ON): each field is computed by the SAME
//! expression its site used, over the SAME path set (`live_paths()` vs
//! `active_paths()` is preserved per field — the saturation-filter trap
//! documented at `capw_store_cap` must not be laundered by this seam), so the
//! arm differs from the default ONLY in WHEN the value was read.
//!
//! NOT covered here, deliberately: every site that MUTATES through the lock
//! (`set_place_slack`, `deficit.on_send`, `charge_in_flight`, `path_mut`),
//! every per-symbol PLACEMENT pick (`place_symbol` / `place_repair_spare_path`
//! / `select_source_path` — these are decisions, not readings, and must see
//! the live account state), and the per-path refresh blocks that fold state
//! into `percap_k` / `percap_rr` while holding the lock. Those keep their own
//! acquisitions in both arms.

use crate::scheduler::Scheduler;

/// The per-iteration scheduler reading. Captured under ONE lock; every field
/// is the verbatim derivation of the site it serves.
#[derive(Debug, Clone, Default)]
pub(crate) struct SchedSnapshot {
    /// gen_pipe M* depth input: max over ACTIVE paths of `min_rtt` (falling
    /// back to `srtt` when the path has no RTprop sample yet), seconds.
    pub rtprop_max_s: f64,
    /// Dynamic in-flight cap input: Σ `copa_bdp_anchor()` over ACTIVE paths.
    pub bdp_sum: f64,
    /// Frontier-independent CC pace rate: Σ cwnd/srtt over LIVE paths whose
    /// srtt exceeds 1e-4 s, with the live-path count (min 1) beside it.
    pub cwnd_rate_sum: f64,
    pub cwnd_rate_live: usize,
    /// Tail-sweep deadline input: `pooled_recovery_srtt_us` over the ACTIVE
    /// paths' estimator RTTs.
    pub pooled_est_rtt_us: u64,
    /// Reactive deficit spacing input: max `srtt` over ACTIVE paths (µs),
    /// `None` when no active path resolves (the site's `unwrap_or(50_000)`).
    pub srtt_max_us: Option<u64>,
}

impl SchedSnapshot {
    /// Capture every derived value the routed sender phases need, under the
    /// caller's single acquisition.
    pub fn capture(sched: &Scheduler) -> Self {
        // — gen_pipe M* depth (verbatim from the depth-refresh block) —
        let rtprop_max_s = sched
            .active_paths()
            .iter()
            .filter_map(|id| {
                sched.path(*id).map(|p| {
                    p.min_rtt()
                        .map(|d| d.as_secs_f64())
                        .unwrap_or_else(|| p.srtt().as_secs_f64())
                })
            })
            .fold(0.0, f64::max);

        // — dynamic in-flight cap (verbatim from the infl-BDP refresh) —
        let bdp_sum: f64 = sched
            .active_paths()
            .iter()
            .filter_map(|id| sched.path(*id).and_then(|p| p.copa_bdp_anchor()))
            .sum();

        // — CC pace rate + live count (verbatim from the cc-rate refresh) —
        let mut r = 0.0f64;
        let mut n = 0usize;
        for id in sched.live_paths() {
            if let Some(p) = sched.path(id) {
                let s = p.srtt().as_secs_f64();
                if s > 1e-4 {
                    r += p.cwnd as f64 / s;
                }
                n += 1;
            }
        }

        // — tail-sweep pooled recovery SRTT (verbatim from the deadline arm) —
        let pooled: Vec<u64> = sched
            .active_paths()
            .iter()
            .filter_map(|id| sched.path(*id))
            .map(|p| p.estimator.rtt().as_micros() as u64)
            .collect();

        // — reactive deficit spacing (verbatim from the react_cap arm) —
        let srtt_max_us = sched
            .active_paths()
            .iter()
            .filter_map(|id| sched.path(*id).map(|p| p.srtt().as_micros() as u64))
            .max();

        Self {
            rtprop_max_s,
            bdp_sum,
            cwnd_rate_sum: r,
            cwnd_rate_live: n.max(1),
            pooled_est_rtt_us: super::pooled_recovery_srtt_us(&pooled),
            srtt_max_us,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::WallClock;
    use std::sync::Arc;

    /// The seam's ONE obligation: `capture` must compute the SAME function of
    /// the SAME scheduler state as the sites it serves. That is what makes
    /// the `RWM_SCHED_SNAPSHOT` arm differ from the default only in WHEN the
    /// read happened — the open A/B question — and not in WHAT was read.
    /// Asserted against the verbatim inline expressions, not against
    /// hand-written expected numbers (an ordinal/eyeball check would not
    /// catch a path-set swap, which is exactly the failure mode here: the
    /// `active_paths()` saturation filter vs `live_paths()`).
    #[test]
    fn capture_matches_the_inline_per_phase_expressions() {
        for n_paths in [0u32, 1, 3] {
            let mut sched = Scheduler::new(Arc::new(WallClock));
            for i in 0..n_paths {
                sched.add_path(i);
            }
            let snap = SchedSnapshot::capture(&sched);

            // M* depth: max RTprop (min_rtt, else srtt) over ACTIVE paths.
            let rtprop_max_s = sched
                .active_paths()
                .iter()
                .filter_map(|id| {
                    sched.path(*id).map(|p| {
                        p.min_rtt()
                            .map(|d| d.as_secs_f64())
                            .unwrap_or_else(|| p.srtt().as_secs_f64())
                    })
                })
                .fold(0.0, f64::max);
            assert_eq!(snap.rtprop_max_s, rtprop_max_s, "rtprop_max_s (n={n_paths})");

            // In-flight cap: Σ Copa BDP anchor over ACTIVE paths.
            let bdp_sum: f64 = sched
                .active_paths()
                .iter()
                .filter_map(|id| sched.path(*id).and_then(|p| p.copa_bdp_anchor()))
                .sum();
            assert_eq!(snap.bdp_sum, bdp_sum, "bdp_sum (n={n_paths})");

            // CC pace rate: Σ cwnd/srtt over LIVE paths (NOT active).
            let mut r = 0.0f64;
            let mut c = 0usize;
            for id in sched.live_paths() {
                if let Some(p) = sched.path(id) {
                    let s = p.srtt().as_secs_f64();
                    if s > 1e-4 {
                        r += p.cwnd as f64 / s;
                    }
                    c += 1;
                }
            }
            assert_eq!(snap.cwnd_rate_sum, r, "cwnd_rate_sum (n={n_paths})");
            assert_eq!(snap.cwnd_rate_live, c.max(1), "cwnd_rate_live (n={n_paths})");

            // Tail sweep: pooled estimator RTT over ACTIVE paths.
            let pooled: Vec<u64> = sched
                .active_paths()
                .iter()
                .filter_map(|id| sched.path(*id))
                .map(|p| p.estimator.rtt().as_micros() as u64)
                .collect();
            assert_eq!(
                snap.pooled_est_rtt_us,
                super::super::pooled_recovery_srtt_us(&pooled),
                "pooled_est_rtt_us (n={n_paths})"
            );

            // Reactive deficit spacing: max srtt over ACTIVE paths, and the
            // site's own 50 ms fallback when there is none.
            let srtt_max = sched
                .active_paths()
                .iter()
                .filter_map(|id| sched.path(*id).map(|p| p.srtt().as_micros() as u64))
                .max();
            assert_eq!(snap.srtt_max_us, srtt_max, "srtt_max_us (n={n_paths})");
            assert_eq!(
                snap.srtt_max_us.unwrap_or(50_000),
                srtt_max.unwrap_or(50_000),
                "srtt_max_us fallback (n={n_paths})"
            );
        }
    }
}
