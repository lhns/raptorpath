//! Gap-snapshot order: the receiver's snapshot version and the `[SNAPV]`
//! gauge (docs/derivations/dup-diag.md).
//!
//! The receiver sends each gap report on the leg the triggering data arrived
//! on, so reports cross the return legs out of generation order. The sender
//! cannot tell by arrival which report is newest. The order already exists:
//! the v9 `(next_expected, received_above)` pair, compared lexicographically
//! ([`AboveReport`], which the store gate folds the same way).
//!
//! The snapshot-order law (paper §7.1 "Snapshot order", Appendix A.6):
//!
//! ```text
//!   judge(R)  ⇔  V(R) ≥ V* ,   V = (next_expected, received_above) lexicographic ,
//!                              V* = max V over every gap batch dequeued in this sender run
//!   coalescing keeps the batch of highest V (ties: the later one)
//!   a batch with no snapshot (the tail sweep) is always judged
//! ```
//!
//! This module holds `V*` ([`SnapOrder`]), the law ([`snapver_judges`],
//! [`snapver_coalesce_keeps`]), the gap path's dequeue seam that applies it
//! ([`take_gap_batch`], [`judged_gaps`]), and the `[SNAPV]` gauge.

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

/// The snapshot-order law: is a batch with snapshot version `v` judged?
/// Only if no newer snapshot has been dequeued; ties are judged (the
/// receiver's timer re-advertisement re-sends an unchanged snapshot), and a
/// batch with no snapshot (the tail sweep) always is. A stale snapshot's
/// received set is a subset of the newest one's, so it names no hole the
/// newest does not, only holes since filled.
pub fn snapver_judges(order: &SnapOrder, v: Option<AboveReport>) -> bool {
    !order.is_stale(v)
}

/// Coalescing under the law: does a newly dequeued batch replace the one
/// kept so far? Only when its snapshot is at least as new, so the coalescer
/// keeps the newest snapshot and not the last to arrive.
pub fn snapver_coalesce_keeps(kept: Option<AboveReport>, candidate: Option<AboveReport>) -> bool {
    candidate >= kept
}

/// The gap path's dequeue: the pending batch (taken by the sender's
/// `select!`) if any, else the channel's next batch, coalescing the rest of
/// the queue when `coalesce` (the hole law's gate) to the batch of highest
/// version. Every dequeued batch's version is folded into `order` (`V*`),
/// the discarded ones included. Returns the batch and how many were
/// discarded; `None` when nothing is queued.
pub(crate) fn take_gap_batch(
    pending: &mut Option<GapBatch>,
    rx: &mut tokio::sync::mpsc::Receiver<GapBatch>,
    coalesce: bool,
    order: &mut SnapOrder,
) -> Option<(GapBatch, u64)> {
    let mut fold = |b: &GapBatch| {
        if let Some(v) = b.3 {
            order.fold(v);
        }
    };
    if let Some(g) = pending.take() {
        fold(&g);
        return Some((g, 0));
    }
    let mut g = rx.try_recv().ok()?;
    fold(&g);
    let mut coalesced = 0;
    if coalesce {
        while let Ok(n) = rx.try_recv() {
            fold(&n);
            if snapver_coalesce_keeps(g.3, n.3) {
                g = n;
            }
            coalesced += 1;
        }
    }
    Some((g, coalesced))
}

/// The gaps the loss channels judge for a batch of version `v`: all of them
/// under the law, or none for a stale snapshot. `serve_gaps` iterates this
/// slice, and the `[HOLD]` stamps read it.
pub(crate) fn judged_gaps<'a>(
    order: &SnapOrder,
    v: Option<AboveReport>,
    gaps: &'a [(u64, u64)],
) -> &'a [(u64, u64)] {
    if snapver_judges(order, v) { gaps } else { &[] }
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
