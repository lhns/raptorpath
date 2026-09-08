//! **THE RECEIVER'S DIAGNOSTIC BLOCK, WITH AN EXIT FLUSH.**
//!
//! The `[SUCC]` / `[ETA]` / `[LAT]` / `[LATE]` / `[REQ]` / `[RANK]` readouts
//! are cumulative gauges printed together on a 1 s cadence from the receiver
//! task (`net/receiver.rs`), last line wins. Until 2026-09-08 that cadence was
//! the ONLY emission site, so:
//!
//!   * a loopback object that completes in under a second never printed the
//!     block at all — the VM's 8 MB transfer finishes in ~0.4 s, and the N = 1
//!     `eta_reachability` / `lat_reachability` variants went red there while
//!     staying green on a slower host (goal-gate "OPERATOR SANCTION
//!     (2026-09-08 ~14:00Z)");
//!   * every invocation lost its final partial second of samples.
//!
//! This type owns the five gauges and the `[REQ]` counters so that ONE
//! destructor can print the block once more at the end of the task, marked
//! `final=1`. The receiver's ~90 other locals stay locals (see the module
//! header of `net/receiver.rs`); only the gauges that must share a
//! destructor moved in here, and they are read and fed through plain field
//! access, so nothing about how they are fed changed.
//!
//! **The contract.**
//!
//!   * `render(probe, final)` is the block, as a list of lines, exactly as the
//!     cadence site printed it: the raw `[SUCCDUMP]` batches first (flushed,
//!     never a partial tail), then `[SUCC]`, `[ETA]` (if the ETA gauge is a
//!     receiver site), `[LAT]` (same), `[LATE]` (same), `[REQ]`, `[RANK]`.
//!     A cadence line is BYTE-IDENTICAL to what it was; a final line is that
//!     line plus one trailing ` final=1` token, so a scraper that takes the
//!     LAST line of a kind reads the complete counts and a scraper that counts
//!     lines can drop the marker. The raw-dump lines carry no marker: they are
//!     samples, not readouts.
//!   * `take_final(probe)` renders the final block EXACTLY ONCE: the first call
//!     returns `Some(lines)` (possibly empty — a site that never saw an arrival
//!     stays silent, the two-sided convention every gauge here already
//!     follows), every later call returns `None`. `Drop` calls it, so a task
//!     that is cancelled (its future dropped at runtime teardown) still flushes;
//!     the receiver ALSO calls it at every clean exit of its loop, where the
//!     decoder is still reachable for a fresh `[RANK]` probe. Whichever runs
//!     first wins; the other is a no-op.
//!   * The gate is the cadence site's own: `on` (`RWM_DIAG` or `RWM_FDIAG`) and
//!     `succ.is_receiver_site()`. No new gate, no new default.
//!
//! What SIGKILL does is unchanged: a process killed with SIGKILL runs no
//! destructor and prints nothing. The exit flush covers a clean loop exit
//! (channel closed, shutdown broadcast, the four TUN/decoder failure exits)
//! and a dropped task; it cannot cover a harness that SIGKILLs the server —
//! those still read the last cadence line.

use super::eta::RecvEta;
use super::lat::LatGauge;
use super::late::{LateGauge, RankGauge};
use super::succ::SuccGauge;

/// The marker appended to every readout line of the final block.
pub(crate) const FINAL_MARK: &str = " final=1";

pub(crate) struct RecvDiagBlock {
    /// `RWM_DIAG || RWM_FDIAG` — the cadence site's gate, captured once.
    pub on: bool,
    pub succ: SuccGauge,
    pub eta: RecvEta,
    pub lat: LatGauge,
    pub late: LateGauge,
    pub rank: RankGauge,
    /// `[LATE] sampler_bind`: did the 2 ms `GAP_ACK_MIN_INTERVAL` floor,
    /// rather than the lateness, decide when a hole could be reported at all?
    /// Consumed and reset by each readout.
    pub late_sampler_bound: bool,
    // `[REQ]` (paper 16.83 arms (A)/(B)): what this receiver ASKED FOR.
    // Cumulative, printed on the block's cadence, last line wins — and on
    // BOTH arms, so `on=0 sent=0` is the control's own reading.
    pub req_sent: u64,
    pub req_span_n: u64,
    pub req_m_max: u64,
    pub req_lstar_us: u64,
    pub req_holes_n: u64,
    request_law: bool,
    rank_feedback: bool,
    final_done: bool,
}

impl RecvDiagBlock {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        on: bool,
        succ: SuccGauge,
        eta: RecvEta,
        lat: LatGauge,
        late: LateGauge,
        request_law: bool,
        rank_feedback: bool,
    ) -> Self {
        Self {
            on,
            succ,
            eta,
            lat,
            late,
            rank: RankGauge::default(),
            late_sampler_bound: false,
            req_sent: 0,
            req_span_n: 0,
            req_m_max: 0,
            req_lstar_us: 0,
            req_holes_n: 0,
            request_law,
            rank_feedback,
            final_done: false,
        }
    }

    /// The cadence site's own emission predicate (minus the clock).
    pub(crate) fn emits(&self) -> bool {
        self.on && self.succ.is_receiver_site()
    }

    /// Has the final block been taken?
    pub(crate) fn final_done(&self) -> bool {
        self.final_done
    }

    /// The `[REQ]` line.
    fn req_line(&self) -> String {
        format!(
            "[REQ] on={} rank={} sent={} spans={} m_max={} lstar_us={} holes={}",
            u8::from(self.request_law),
            u8::from(self.rank_feedback),
            self.req_sent,
            self.req_span_n,
            self.req_m_max,
            self.req_lstar_us,
            self.req_holes_n,
        )
    }

    /// The block, as lines, in the cadence site's order. `rank_probe` is a
    /// fresh `(holes, pivots, tail_overcount)` frontier reading when the
    /// caller can take one; `None` re-reports the latest census (the
    /// destructor cannot reach the decoder). `final_` appends the marker to
    /// every readout line. Does NOT consult the gate: callers do, so the
    /// exactly-once bookkeeping in `take_final` cannot be bypassed by it.
    fn render(&mut self, rank_probe: Option<(u64, u64, u64)>, final_: bool) -> Vec<String> {
        let mark = |s: String| if final_ { s + FINAL_MARK } else { s };
        let mut out = Vec::with_capacity(8);
        // The RAW dump rides its own gate and is flushed here so no recorded
        // sample is ever left in a partial batch.
        out.extend(self.succ.take_dump_lines(true));
        out.push(mark(self.succ.line()));
        // `[ETA]` BESIDE `[SUCC]`, on ITS cadence and under ITS gate, because
        // the two are read together.
        if self.eta.is_receiver_site() {
            out.push(mark(self.eta.line()));
        }
        // `[LAT]`: printed whenever the receiver has delivered anything at all.
        if self.lat.is_receiver_site() {
            out.push(mark(self.lat.line()));
        }
        // `[LATE]`: the sampler observation is consumed HERE and reset.
        if self.late.is_receiver_site() {
            let l = self.late.line(self.late_sampler_bound);
            self.late_sampler_bound = false;
            out.push(mark(l));
        }
        out.push(mark(self.req_line()));
        if let Some((holes, pivots, tail)) = rank_probe {
            self.rank.note(holes, pivots, tail);
        }
        out.push(mark(self.rank.line()));
        out
    }

    /// One CADENCE readout. Empty when the gate is closed.
    pub(crate) fn render_cadence(&mut self, rank_probe: Option<(u64, u64, u64)>) -> Vec<String> {
        if !self.emits() {
            return Vec::new();
        }
        self.render(rank_probe, false)
    }

    /// THE EXIT FLUSH, exactly once. `Some(lines)` the first time (empty when
    /// the gate is closed — a silent site stays silent), `None` ever after.
    pub(crate) fn take_final(
        &mut self,
        rank_probe: Option<(u64, u64, u64)>,
    ) -> Option<Vec<String>> {
        if self.final_done {
            return None;
        }
        self.final_done = true;
        if !self.emits() {
            return Some(Vec::new());
        }
        Some(self.render(rank_probe, true))
    }

    /// Print the final block if it has not been printed yet.
    pub(crate) fn flush_final(&mut self, rank_probe: Option<(u64, u64, u64)>) {
        if let Some(lines) = self.take_final(rank_probe) {
            for l in lines {
                eprintln!("{l}");
            }
        }
    }
}

impl Drop for RecvDiagBlock {
    fn drop(&mut self) {
        // The authoritative emission for a DROPPED task (the `[RACK]` /
        // `[RFA]` discipline in `net/mod.rs`); a no-op after a clean-exit
        // flush, by `take_final`'s own flag.
        self.flush_final(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn block(on: bool) -> RecvDiagBlock {
        RecvDiagBlock::new(
            on,
            SuccGauge::new(false, false, 0),
            RecvEta::default(),
            LatGauge::default(),
            LateGauge::new(0.01),
            false,
            false,
        )
    }

    fn fed(on: bool) -> RecvDiagBlock {
        let mut b = block(on);
        // One arrival makes the SUCC gauge a receiver site — the block's gate.
        b.succ.observe_high(1, Instant::now(), 0);
        b
    }

    /// THE INSTRUMENT'S OWN CLAIM: the final block is emitted exactly once,
    /// through whichever door reaches it first, and never twice.
    #[test]
    fn the_final_block_is_taken_exactly_once() {
        let mut b = fed(true);
        assert!(!b.final_done());
        let first = b
            .take_final(Some((3, 1, 2)))
            .expect("first take yields the block");
        assert!(!first.is_empty(), "a fed, gated site must print");
        assert!(b.final_done());
        assert!(b.take_final(None).is_none(), "second take must be a no-op");
        assert!(
            b.take_final(Some((0, 0, 0))).is_none(),
            "and so must every later one"
        );
        // `flush_final` after `take_final` prints nothing: it goes through
        // the same flag. (Drop goes through `flush_final`.)
        b.flush_final(None);
        assert!(b.final_done());
    }

    /// Every READOUT line of the final block carries the marker, exactly once,
    /// as its last token; the cadence render of the same state carries none.
    #[test]
    fn the_marker_is_on_every_final_readout_and_on_no_cadence_line() {
        let mut b = fed(true);
        let cadence = b.render_cadence(Some((2, 2, 0)));
        assert!(!cadence.is_empty());
        for l in &cadence {
            assert!(
                !l.contains("final="),
                "cadence line must be byte-identical: {l}"
            );
        }
        let fin = b.take_final(Some((2, 2, 0))).unwrap();
        assert_eq!(
            fin.len(),
            cadence.len(),
            "the final block has the same lines as a cadence block"
        );
        for (c, f) in cadence.iter().zip(&fin) {
            let tag = c.split_whitespace().next().unwrap();
            assert!(f.starts_with(tag), "{f}");
            assert!(
                f.ends_with(FINAL_MARK),
                "marker must be the last token: {f}"
            );
            assert_eq!(f.matches("final=").count(), 1, "{f}");
        }
        // The kinds a scraper looks for are all present on the final block.
        for tag in ["[SUCC] ", "[REQ] ", "[RANK] "] {
            assert!(
                fin.iter().any(|l| l.starts_with(tag)),
                "missing {tag} in {fin:?}"
            );
        }
    }

    /// Two-sided: a site that never saw an arrival, or a run without the
    /// gate, prints NOTHING — but still counts as flushed, so a later drop
    /// cannot print a block the cadence never did.
    #[test]
    fn a_silent_site_stays_silent_and_is_still_flushed_once() {
        let mut ungated = fed(false);
        assert_eq!(ungated.take_final(None), Some(Vec::new()));
        assert_eq!(ungated.take_final(None), None);
        let mut unfed = block(true);
        assert!(unfed.render_cadence(None).is_empty());
        assert_eq!(unfed.take_final(None), Some(Vec::new()));
        assert_eq!(unfed.take_final(None), None);
    }

    /// A final `[RANK]` without a fresh probe re-reports the latest census
    /// and does not invent a report; with one it counts it.
    #[test]
    fn the_final_rank_line_reports_the_last_census_when_no_probe_is_possible() {
        let mut b = fed(true);
        let _ = b.render_cadence(Some((5, 2, 1)));
        let fin = b.take_final(None).unwrap();
        let rank = fin.iter().find(|l| l.starts_with("[RANK] ")).unwrap();
        assert!(
            rank.contains("holes=5 pivots=2 deficit=3 tail_overcount=1"),
            "{rank}"
        );
        assert!(
            rank.contains("reports=1 "),
            "no probe ⇒ no new report: {rank}"
        );
        let mut b2 = fed(true);
        let _ = b2.render_cadence(Some((5, 2, 1)));
        let fin2 = b2.take_final(Some((0, 0, 0))).unwrap();
        let rank2 = fin2.iter().find(|l| l.starts_with("[RANK] ")).unwrap();
        assert!(
            rank2.contains("empty=1 reports=2 "),
            "a probe is a report: {rank2}"
        );
    }
}
