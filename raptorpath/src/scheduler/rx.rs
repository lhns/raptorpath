//! The scheduler's RX half (threading Q2: ownership by direction).
//!
//! The receiver task owns this struct outright (a local of
//! `net::receiver::run_receiver`, used through plain `&mut`): the
//! incoming-direction observations written per arrived datagram — the RFC 3550
//! arrival jitter (`record_arrival`) and the incoming-loss EWMA fed by
//! `PathBatchTracker` (`record_rx_batch`) — the exploration's class (b)
//! (`docs/thread-p2-scheduler-access.md` §2). Everything else of the
//! scheduler is the TX half ([`super::Scheduler`]), owned by the sender task.
//!
//! The two outputs are published per path into
//! `monitor::stats::CrossDirection` after every update, for the sender's
//! mirror (`LossEstimator::set_rx_mirror`) and the PathReport; the liveness
//! stamp (`touch_path` on a data arrival) is published the same way and acted
//! on by the sender, which owns `active` and the dead-path clock.
//!
//! A path's RX state is created on its first arrival: the receiver never
//! needs the TX half's path set to observe one.

use std::collections::HashMap;

use super::PathId;
use crate::control::RxEstimator;

/// One path's incoming-direction state.
#[derive(Debug, Default)]
pub struct RxPathState {
    pub estimator: RxEstimator,
}

/// The RX half: per path, the incoming-direction estimators.
#[derive(Debug, Default)]
pub struct RxScheduler {
    paths: HashMap<PathId, RxPathState>,
}

impl RxScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// The path's RX state, created on first use.
    #[inline]
    pub fn path_mut(&mut self, id: PathId) -> &mut RxPathState {
        self.paths.entry(id).or_default()
    }

    pub fn path(&self, id: PathId) -> Option<&RxPathState> {
        self.paths.get(&id)
    }
}
