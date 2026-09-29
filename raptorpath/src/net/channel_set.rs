//! The channel's path set: the ONE definition every "what is the channel /
//! how big is the pool" reader enumerates. Re-exported from `net`.
//!
//! Two scheduler sets exist and they answer different questions:
//!
//! - `active_paths()` = active AND `available() > 0` (cwnd − in_flight):
//!   "can I place a symbol on this path NOW". Placement picks
//!   (`schedule` / `best_*_path`) legitimately read it.
//! - `live_paths()` = active: membership, "is this path part of the
//!   channel". A wire-bound sender is cwnd-saturated by definition, so the
//!   placement filter empties exactly while the paths carry the transfer.
//!
//! Every reader here is a property of the channel or the pool — the worst
//! path's ε / RTT / estimator, the Σ of the BDP anchors, the max RTprop /
//! SRTT, and the control-plane broadcast set — so all of them range over
//! [`channel_paths`]. A reader that must survive saturation (all of them)
//! gets the same value whether or not the paths are cwnd-full.

use crate::control::estimator::LossEstimator;
use crate::scheduler::{PathId, PathState, Scheduler};
use std::time::Duration;

/// The channel's membership set. Every pool / worst-path / broadcast
/// reader in this module enumerates it, and so do the recovery clocks
/// ([`super::recovery_clock_paths`]).
pub fn channel_paths(sched: &Scheduler) -> Vec<PathId> {
    sched.active_paths()
}

/// The worst-ε channel path (max `estimator.loss_rate()`, ties to the last
/// maximum) — the path every rate / budget / repair-count reader
/// provisions for.
pub fn worst_eps_channel_path(sched: &Scheduler) -> Option<(PathId, &PathState)> {
    channel_paths(sched)
        .into_iter()
        .filter_map(|id| sched.path(id).map(|p| (id, p)))
        .max_by(|a, b| {
            a.1.estimator
                .loss_rate()
                .partial_cmp(&b.1.estimator.loss_rate())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

/// The worst-ε channel path's estimator.
pub fn worst_eps_estimator(sched: &Scheduler) -> Option<&LossEstimator> {
    worst_eps_channel_path(sched).map(|(_, p)| &p.estimator)
}

/// The channel's worst loss rate (max over the set; 0.0 on an empty set) —
/// the ε̂ the retransmit buffer, the interleaver decay and the block ARQ
/// read.
pub fn channel_worst_loss_rate(sched: &Scheduler) -> f64 {
    channel_paths(sched)
        .into_iter()
        .filter_map(|id| sched.path(id))
        .map(|p| p.estimator.loss_rate())
        .fold(0.0f64, f64::max)
}

/// The P_lost inputs `(srtt_s, rttvar_s, ε)` from the worst-ε channel path;
/// `(0.05, 0.005, 0.0)` only when the channel has no path at all.
pub fn p_lost_inputs(sched: &Scheduler) -> (f64, f64, f64) {
    match worst_eps_channel_path(sched) {
        Some((_, p)) => {
            let srtt = p.estimator.rtt().as_secs_f64();
            (srtt, srtt * 0.1, p.estimator.loss_rate())
        }
        None => (0.05, 0.005, 0.0),
    }
}

/// The NACK congestion inputs `(loss, copa min-RTT)` from the worst-ε
/// channel path; `(0.0, None)` only when the channel has no path at all.
pub fn nack_congestion_inputs(sched: &Scheduler) -> (f64, Option<Duration>) {
    match worst_eps_channel_path(sched) {
        Some((_, p)) => (p.estimator.loss_rate(), p.copa_min_rtt()),
        None => (0.0, None),
    }
}

/// The channel's max RTprop (min-RTT, falling back to SRTT before the
/// first sample), in seconds; 0.0 on an empty set.
pub fn channel_max_rtprop_s(sched: &Scheduler) -> f64 {
    channel_paths(sched)
        .into_iter()
        .filter_map(|id| {
            sched.path(id).map(|p| {
                p.min_rtt()
                    .map(|d| d.as_secs_f64())
                    .unwrap_or_else(|| p.srtt().as_secs_f64())
            })
        })
        .fold(0.0, f64::max)
}

/// Σ of the channel's warm BDP anchors (`copa_bdp_anchor()`), in symbols.
pub fn channel_bdp_anchor_sum(sched: &Scheduler) -> f64 {
    channel_paths(sched)
        .into_iter()
        .filter_map(|id| sched.path(id).and_then(|p| p.copa_bdp_anchor()))
        .sum()
}

/// The channel's max SRTT (µs); `None` on an empty set.
pub fn channel_max_srtt_us(sched: &Scheduler) -> Option<u64> {
    channel_paths(sched)
        .into_iter()
        .filter_map(|id| sched.path(id).map(|p| p.srtt().as_micros() as u64))
        .max()
}

/// The paths a control-plane broadcast (`WindowStart`, `Shutdown`) goes
/// out on. A cwnd-full path is still a member: skipping it would leave the
/// peer without the announce / the shutdown on exactly the paths carrying
/// the transfer.
pub fn control_broadcast_paths(sched: &Scheduler) -> Vec<PathId> {
    channel_paths(sched)
}

/// The plain dyn-store-cap's pool inputs over the channel: Σ of the warm
/// BDP anchors, and (when `want_k`, the honest per-path cap is live) one
/// warm-anchor slot per path in set order for `honest_cap_terms`.
///
/// `unified` is the `RWM_STORE_CAP_UNIFIED` A/B: on reads `live_paths()`,
/// off reads [`channel_paths`].
pub fn store_cap_pool_inputs(
    sched: &Scheduler,
    want_k: bool,
    unified: bool,
) -> (f64, Vec<Option<super::HonestCapPath>>) {
    let set = if unified { sched.live_paths() } else { channel_paths(sched) };
    let mut bdp = 0.0f64;
    let mut slots: Vec<Option<super::HonestCapPath>> = Vec::new();
    for id in set {
        if let Some(p) = sched.path(id) {
            if let Some(a) = p.copa_bdp_anchor() {
                bdp += a;
                if want_k {
                    slots.push(Some(super::HonestCapPath {
                        id,
                        anchor: Some(a),
                        rate: p.btlbw_sym_per_s(),
                        srtt: p.srtt(),
                        rtprop: p.min_rtt(),
                        k_raw: p.k_raw(),
                    }));
                }
            }
        }
    }
    (bdp, slots)
}
