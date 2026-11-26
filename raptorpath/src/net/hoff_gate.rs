//! The sender hand-off gate (paper §6.8, revision 4), behind the default-off
//! experiment arm `RWM_HOFF_GATE`. INERT SCAFFOLDING: the API the red tests
//! (`hoff_gate_tests`) are written against, with every decision at its
//! no-gate value (admit, spare = +inf, no republish). The law lands in the
//! next commit.

use std::sync::Arc;

use super::hoff::PathIn;
use crate::transport::io_owner::PathView;

pub(crate) fn beta_of(_i: &PathIn, _l_s: f64) -> Option<f64> {
    None
}

pub(crate) fn admits(_b: u64, _beta: Option<f64>) -> bool {
    true
}

/// `r_eff = min(r_law, spare)` at the repair seams.
pub(crate) fn r_eff(r_law: f64, spare: f64) -> f64 {
    r_law.min(spare.max(0.0))
}

pub(crate) fn owner_republishes(_armed: bool, _handed: u64, _dropped: u64, _tx_at_publish: u64) -> bool {
    false
}

pub(crate) fn poll_armed(tx_paused: bool, _gate_paused: bool) -> bool {
    tx_paused
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(crate) struct Terms {
    pub tau_o: Option<f64>,
    pub l_o: f64,
    pub tau_p: Option<f64>,
    pub l_p: f64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GateRow {
    pub beta: Option<f64>,
    pub terms: Terms,
    pub hand_bind: f64,
    pub spare_zero: bool,
    pub spare_bind: f64,
}

#[derive(Default)]
pub(crate) struct HandoffGate {
    on: bool,
    #[allow(dead_code)]
    views: Vec<(u32, Arc<PathView>)>,
}

impl HandoffGate {
    pub(crate) fn new(on: bool) -> Self {
        Self { on, views: Vec::new() }
    }
    pub(crate) fn on(&self) -> bool {
        self.on
    }
    #[cfg(test)]
    pub(crate) fn fix_terms(&mut self, _t: Terms) {}
    pub(crate) fn paused_wait(&mut self, _now_us: u64, _wait_us: u64) {}
    pub(crate) fn iter_work(&mut self, _now_us: u64, _work_us: u64) {}
    pub(crate) fn owner_hists(&mut self, _id: u32, _now_us: u64, _work: &[u64], _timer: &[u64]) {}
    pub(crate) fn beta(&self, _id: u32, _i: &PathIn) -> Option<f64> {
        None
    }
    pub(crate) fn admit(&self, _id: u32, _i: &PathIn) -> bool {
        true
    }
    pub(crate) fn begin_turn(&mut self, _now_us: u64, _waiting: bool, _store_paused: bool, _ins: &[(u32, PathIn)]) -> bool {
        false
    }
    pub(crate) fn check(&mut self, _waiting: bool, _ins: &[(u32, PathIn)]) -> bool {
        true
    }
    pub(crate) fn open_ids(&self, ins: &[(u32, PathIn)]) -> Vec<u32> {
        ins.iter().map(|(id, _)| *id).collect()
    }
    pub(crate) fn note_store_end(&mut self) {}
    pub(crate) fn note_first_send(&mut self, _path: u32) {}
    pub(crate) fn spare_of(&self, _id: u32) -> f64 {
        f64::INFINITY
    }
    pub(crate) fn spare_all(&self) -> f64 {
        f64::INFINITY
    }
    pub(crate) fn take_row(&mut self, _id: u32) -> Option<GateRow> {
        None
    }
    pub(crate) fn owner_timer_wakes(&self) -> u64 {
        0
    }
}
