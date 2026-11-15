//! The plain-mode Copa delivery feed: send commitments and per-path delivery
//! attribution (ADR-0062).

use super::*;

/// Symbols→bytes conversion for the pass-through substrate
/// window (`RWM_QUIC_CC=passthrough`). Copa-lite's cwnd is in SYMBOLS; quinn's
/// congestion window is in BYTES of packet payload. Plain window mode puts one
/// ~1200-byte symbol per datagram plus wire framing (~30–50 B), so 1250 B per
/// symbol converts the window with a few-percent tolerance — Copa's delay
/// signal absorbs the residual (a slightly generous window shows up as queue
/// and is backed off; a slightly tight one only shaves the probe overshoot).
pub(crate) const COPA_SOLE_BYTES_PER_SYMBOL: u64 = 1250;

/// Plain-mode Copa delivery-feed state. Sender-side only: seq→path recorded
/// at send, newly-delivered seqs derived from each WindowAck's cumulative
/// frontier + SACK ranges, attributed per path into the send-interval rate
/// sampler + the Copa cwnd dynamics.
pub(crate) struct CopaFeed {
    /// seq → its send commitment: the last (re)send's path + timestamp.
    /// Written at source send and at targeted retransmit (a retransmit
    /// re-snapshots the rate sample, so the eventual ack yields a truthful
    /// send-interval). Removed on attribution; entries for seqs the
    /// frontier passed are gone by then.
    pub(crate) seq_path: DashMap<u64, SendCommit>,
    /// Attribution cursor: the next in-order seq not yet attributed plus the
    /// set of above-frontier seqs already attributed via SACK (so a seq is
    /// attributed exactly once). Bounded by the sender's outstanding store.
    cursor: parking_lot::Mutex<CopaFeedCursor>,
}

#[derive(Default)]
pub(crate) struct CopaFeedCursor {
    next: u64,
    sacked: std::collections::BTreeSet<u64>,
}

/// One seq's send commitment for delivery attribution: the last (re)send.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SendCommit {
    /// Path + send time (µs) of the most recent (re)send.
    pub(crate) last: (u32, u64),
}

impl CopaFeed {
    pub(crate) fn new() -> Self {
        Self {
            seq_path: DashMap::new(),
            cursor: parking_lot::Mutex::new(CopaFeedCursor::default()),
        }
    }

    /// Record a (re)send of source seq `seq` on `path`; a resend just
    /// refreshes the commitment (its rate sample is re-snapshotted by
    /// `on_src_sent`).
    pub(crate) fn on_sent(&self, seq: u64, path: u32) {
        self.seq_path.insert(seq, SendCommit { last: (path, now_us()) });
    }

    /// Diff one WindowAck against the cursor: returns the seqs this ack
    /// newly proves delivered (frontier advance up to `next_expected`,
    /// exclusive -- wire v9 count semantics, so `0` attributes nothing and
    /// `1` attributes seq 0 -- plus never-before-seen SACKed seqs above it), each exactly
    /// once across the whole ack stream. Out-of-order/duplicate acks yield
    /// an empty diff — never a double attribution.
    pub(crate) fn newly_delivered(&self, next_expected: u64, sack_ranges: &[(u64, u64)]) -> Vec<u64> {
        // Per-ack safety bound: a corrupt/hostile ack must not trap us in a
        // multi-million-seq loop. Honest ranges are bounded by the sender's
        // outstanding store (≤ a few thousand).
        const MAX_PER_ACK: usize = 65_536;
        let mut newly = Vec::new();
        let mut c = self.cursor.lock();
        while c.next < next_expected && newly.len() < MAX_PER_ACK {
            let s = c.next;
            c.next += 1;
            // Already attributed via an earlier SACK → consume the marker.
            if !c.sacked.remove(&s) {
                newly.push(s);
            }
        }
        for &(a, b) in sack_ranges {
            let lo = a.max(c.next);
            let hi = b.min(lo.saturating_add(MAX_PER_ACK as u64));
            for q in lo..=hi {
                if newly.len() >= MAX_PER_ACK {
                    break;
                }
                if c.sacked.insert(q) {
                    newly.push(q);
                }
            }
        }
        newly
    }
}

/// Attribute one WindowAck's newly-delivered seqs to their paths and run the
/// per-path Copa machinery on them: send-interval rate sample per seq
/// (`on_src_delivered_seq` — feeds the windowed-max BtlBw with clean Δt),
/// in-flight release, the per-SRTT cwnd update/backoff
/// (`on_delivery_signal`), and finally the pass-through substrate window
/// write (no-op unless RWM_QUIC_CC=passthrough). Call after recording the
/// ack's RTT sample so the update sees the freshest queue signal.
pub(crate) fn copa_feed_attribute(
    feed: &CopaFeed,
    ack_path: u32,
    next_expected: u64,
    sack_ranges: &[(u64, u64)],
    sched: &mut Scheduler,
    transport: &Arc<QuicTransport>,
    stats: &Arc<SharedStats>,
) {
    let newly = feed.newly_delivered(next_expected, sack_ranges);
    if newly.is_empty() {
        return;
    }
    let per_path = copa_attribute_newly(feed, ack_path, &newly, sched);
    // The pass-through window writes (a DashMap) after the scheduler pass.
    let mut windows: Vec<(u32, u64)> = Vec::with_capacity(per_path.len());
    for (p, _n) in per_path {
        if let Some(ps) = sched.path_mut(p) {
            // Feed the wire-level loss evidence (the pass-through shim's
            // congestion-event counter) into the competitive AIMD before the
            // update consumes it. No-op unless RWM_COPA_COMPETE is active.
            if crate::scheduler::copa_compete_active() {
                if let Some((ev, _, _)) = transport.cc_passthrough_stats(p) {
                    ps.on_wire_congestion_events(ev);
                }
            }
            // Not release_in_flight here: the per-batch Ack arm does the
            // wire-level in-flight release (it covers repairs too); releasing
            // again per attributed source seq would double-count.
            ps.on_delivery_signal();
            windows.push((p, ps.cwnd as u64 * COPA_SOLE_BYTES_PER_SYMBOL));
            if let Some(st) = stats.path_ref(p) {
                st.cwnd.store(ps.cwnd as u64, Ordering::Relaxed);
                st.in_flight.store(ps.in_flight as u64, Ordering::Relaxed);
            }
        }
    }
    for (p, bytes) in windows {
        transport.set_cc_window_bytes(p, bytes);
    }
}

/// The per-seq attribution loop of [`copa_feed_attribute`], over the
/// sender's scheduler: resolve each newly-delivered seq's
/// carrying path (send record → ack-path fallback) and
/// run that path's send-interval rate sampler (`on_src_delivered_seq`).
/// Returns the per-path attribution counts for the (Copa-sole only) cwnd
/// pass that follows.
///
/// Separate so a component bench can drive the production attribution body
/// (`docs/measurement-discipline.md` rule 1).
/// Sole non-test caller is `copa_feed_attribute`.
pub(crate) fn copa_attribute_newly(
    feed: &CopaFeed,
    ack_path: u32,
    newly: &[u64],
    sched: &mut Scheduler,
) -> std::collections::HashMap<u32, u32> {
    let mut per_path: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for &seq in newly {
        // Attribute to the path whose flight delivered the seq. Default: the
        // path it was last sent on; a seq without a send record (pre-feed
        // traffic, evicted record) falls back to the path the ack arrived on.
        let p = match feed.seq_path.remove(&seq) {
            Some((_, commit)) => commit.last.0,
            None => ack_path,
        };
        if let Some(ps) = sched.path_mut(p) {
            ps.on_src_delivered_seq(seq);
        }
        *per_path.entry(p).or_insert(0) += 1;
    }
    per_path
}
