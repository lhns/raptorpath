//! Gap-snapshot order: the receiver's snapshot version and the `[SNAPV]`
//! gauge (docs/derivations/dup-diag.md).
//!
//! The receiver sends each gap report on the leg the triggering data arrived
//! on, so reports cross the return legs out of generation order. The sender
//! cannot tell by arrival which report is newest. The order already exists:
//! the v9 `(next_expected, received_above)` pair, compared lexicographically
//! ([`AboveReport`], which the store gate folds the same way). This module
//! holds the newest pair the gap path has seen and the per-return-leg
//! counters that measure how often the gap path judged an older one.

use super::*;

/// One gap batch on the sender's gap channel: the producer's cause label,
/// the return leg it arrived on (`u32::MAX` for the sender's own tail
/// sweep), the gaps, and the receiver snapshot version the gaps were
/// inverted from (`None` for the tail sweep, which inverts no snapshot).
pub type GapBatch = (FireCause, u32, Vec<(u64, u64)>, Option<AboveReport>);

/// The newest gap-snapshot version the sender's gap path has dequeued (`V*`).
///
/// Lives in `SenderState`, constructed with the encoder: one value per
/// transfer, the lifetime of the seq space the version counts. A version
/// from another seq space compared against it would be meaningless (a reset
/// receiver's report reads as stale), which is why it is never shared or
/// carried across a sender run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SnapOrder {
    newest: AboveReport,
}

impl SnapOrder {
    /// Fold a dequeued report's version (every dequeued report, including
    /// the ones coalescing discards: they are snapshots the sender has seen).
    pub fn fold(&mut self, v: AboveReport) {
        self.newest = self.newest.max(v);
    }

    /// The newest version folded so far.
    pub fn newest(&self) -> AboveReport {
        self.newest
    }

    /// Is a report with version `v` older than the newest seen? A report with
    /// no version (the sender's own tail sweep) is never stale. Ties are not
    /// stale: the receiver's timer re-advertisement re-sends an unchanged
    /// snapshot, and an equal pair is an identical received set.
    pub fn is_stale(&self, v: Option<AboveReport>) -> bool {
        v.is_some_and(|v| v < self.newest)
    }
}

/// Coalescing under `RWM_RECOV_SNAPVER`: does a newly dequeued batch replace
/// the one kept so far? Off (shipped): always, the last-dequeued wins. On:
/// only when its snapshot is at least as new, so the coalescer keeps the
/// newest snapshot and not the last to arrive.
pub(crate) fn snapver_coalesce_keeps(
    arm: bool,
    kept: Option<AboveReport>,
    candidate: Option<AboveReport>,
) -> bool {
    !arm || candidate >= kept
}

/// Does the gap path judge this batch's gaps (both loss channels, the shed
/// and the `[HOLD]` stamps)? Off (shipped): always. On: only a snapshot at
/// least as new as `V*`; a batch with no snapshot (the tail sweep) always.
/// A stale snapshot's received set is a subset of the newest one's, so it
/// names no hole the newest does not, only holes since filled.
pub(crate) fn snapver_judges(arm: bool, order: &SnapOrder, v: Option<AboveReport>) -> bool {
    !arm || !order.is_stale(v)
}

/// Per return leg: `[reports, stale, pt_fires_stale, tt_fires_stale]`.
type LegCounts = [u64; 4];

/// The `[SNAPV]` gauge (observation only, counted under `RWM_DIAG`).
///
/// Per return leg (`ack_path` of the gap batch; `-` for the sender's own
/// tail sweep, which carries no snapshot): reports the gap path processed,
/// how many of them were older than the newest dequeued snapshot, and the
/// fires whose judged seq came from such a stale report, split by the loss
/// channel that fired (packet threshold `pt`, every other fire `tt`).
#[derive(Default)]
pub(crate) struct SnapOrderGauge {
    legs: std::collections::BTreeMap<u32, LegCounts>,
}

impl SnapOrderGauge {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// One processed gap batch from return leg `leg`.
    pub(crate) fn on_report(&mut self, leg: u32, stale: bool) {
        let c = self.legs.entry(leg).or_insert([0; 4]);
        c[0] += 1;
        c[1] += stale as u64;
    }

    /// One fire from a batch of return leg `leg`; counted only if the batch
    /// was stale. `packet_threshold` is whether the §6.1.1 channel fired it.
    pub(crate) fn on_fire(&mut self, leg: u32, stale: bool, packet_threshold: bool) {
        if !stale {
            return;
        }
        let c = self.legs.entry(leg).or_insert([0; 4]);
        c[if packet_threshold { 2 } else { 3 }] += 1;
    }

    /// The `[SNAPV]` line, `None` when nothing was counted (an unreached
    /// site stays silent, the `[HOLD]`/`[RACK]` rule).
    pub(crate) fn line(&self) -> Option<String> {
        if self.legs.is_empty() {
            return None;
        }
        let mut s = String::from("[SNAPV] site=sender");
        for (&leg, c) in &self.legs {
            let name = if leg == u32::MAX { "-".to_string() } else { leg.to_string() };
            s.push_str(&format!(
                " p{name}:rep={} stale={} pt_st={} tt_st={}",
                c[0], c[1], c[2], c[3]
            ));
        }
        // The sacrificial trailing constant (see `[HOLD]`).
        s.push_str(&format!(" fa_class={RACK_SPURIOUS_BUDGET:.4}"));
        Some(s)
    }
}

impl Drop for SnapOrderGauge {
    fn drop(&mut self) {
        if let Some(l) = self.line() {
            crate::readout!("{}", l);
        }
    }
}
