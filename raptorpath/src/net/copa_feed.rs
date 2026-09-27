//! The plain-mode Copa delivery feed (feat/copa-sole-cc): send commitments,
//! per-path delivery attribution. Moved verbatim out of `net/mod.rs`
//! (cleanup Stage 3).

use super::*;

/// feat/copa-sole-cc: symbols→bytes conversion for the pass-through substrate
/// window (`RWM_QUIC_CC=passthrough`). Copa-lite's cwnd is in SYMBOLS; quinn's
/// congestion window is in BYTES of packet payload. Plain window mode puts one
/// ~1200-byte symbol per datagram plus wire framing (~30–50 B), so 1250 B per
/// symbol converts the window with a few-percent tolerance — Copa's delay
/// signal absorbs the residual (a slightly generous window shows up as queue
/// and is backed off; a slightly tight one only shaves the probe overshoot).
pub(crate) const COPA_SOLE_BYTES_PER_SYMBOL: u64 = 1250;

/// feat/copa-sole-cc: plain-mode Copa delivery-feed state (see the creation
/// site in `run_impl` for the full design note). Sender-side only: seq→path
/// recorded at send, newly-delivered seqs derived from each WindowAck's
/// cumulative frontier + SACK ranges, attributed per path into the
/// BBR-correct send-interval rate sampler + the Copa cwnd dynamics.
pub(crate) struct CopaFeed {
    /// seq → its send commitments: the LAST (re)send's path + timestamp,
    /// plus the previous DISTINCT-path commitment when the seq was
    /// retransmitted cross-path (the flight-witness input, residual (iii)).
    /// Written at source send and at targeted retransmit (a retransmit
    /// re-snapshots the rate sample, so the eventual ack yields a truthful
    /// send-interval). Removed on attribution; entries for seqs the
    /// frontier passed are gone by then.
    pub(crate) seq_path: DashMap<u64, SendCommit>,
    /// Attribution cursor: the next in-order seq not yet attributed plus the
    /// set of above-frontier seqs already attributed via SACK (so a seq is
    /// attributed exactly once). Bounded by the sender's outstanding store.
    cursor: parking_lot::Mutex<CopaFeedCursor>,
    /// feat/anchor-hygiene (`RWM_PLAIN_RS`): SAMPLING-ONLY mode — the #79
    /// send-interval rate sampler generalized to plain window-reliable mode
    /// under ANY substrate CC. The WindowAck frontier/SACK attribution and
    /// the per-seq BBR rate samples run (so the per-path BtlBw/BDP anchor is
    /// fed CLEAN send-interval Δt instead of the ack-interval over-read that
    /// knee-clamps the percap/store caps — goal-gate "Per-Path Outstanding
    /// Accounting" GUARD RESULTS residual (i)), but Copa does NOT own the
    /// substrate window: no pass-through window writes, and the cwnd
    /// dynamics keep their legacy per-batch-Ack call site/cadence.
    sampling_only: bool,
    /// Residual (iii) fix live: apply the flight-time witness
    /// ([`resolve_flight_path`]) at attribution. Follows `RWM_PLAIN_RS`
    /// (sampling-only feed; `RWM_RS_ATTR=0` = the same-binary legacy
    /// last-sent-path control). The full Copa-sole feed keeps legacy
    /// attribution (its arms are study baselines).
    pub(crate) attr_witness: bool,
    /// DIAG: attributed seqs whose commit history crossed paths.
    attr_cross: AtomicU64,
    /// DIAG: of those, attributions the witness credited to the PREVIOUS
    /// commitment (the spurious-retransmit class — the last flight was
    /// younger than its path's RTprop at ack time).
    attr_witness_prev: AtomicU64,
}

#[derive(Default)]
pub(crate) struct CopaFeedCursor {
    next: u64,
    sacked: std::collections::BTreeSet<u64>,
}

/// One seq's send-commitment history for delivery attribution (residual
/// (iii), branch `feat/store-borrowing`): the LAST (re)send plus the
/// previous DISTINCT-path commitment, so the attribution site can apply
/// the flight-time witness ([`resolve_flight_path`]) instead of blindly
/// crediting the last-sent path.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SendCommit {
    /// Path + send time (µs) of the most recent (re)send.
    pub(crate) last: (u32, u64),
    /// Path + send time of the previous distinct-path commitment, when the
    /// seq was retransmitted CROSS-path (None = single-path history).
    pub(crate) prev: Option<(u32, u64)>,
}

/// The flight-time witness (residual (iii) fix): which path's flight
/// actually delivered an attributed seq.
///
/// The defect it closes: a seq lost (or presumed lost) on path A and
/// retransmitted on path B is attributed to B when its ack arrives — but
/// if the ack arrives SOONER after the retransmit than B's propagation
/// floor, the retransmitted copy cannot have completed the round trip; the
/// delivering flight was the ORIGINAL copy on A (a spurious retransmit —
/// the gap was ack latency, not loss). Blindly crediting B advances B's
/// per-path delivered counter for a symbol that flew on A, and at an
/// asymmetric cell the fast→slow retransmit stream inflates the SLOW
/// path's Δdelivered — the measured ×3–5 slow-path BtlBw over-read under
/// multipath placement (goal-gate HONEST-CAP RESULTS sub-residual (iii)).
///
/// The witness is a pure floor-clock test, no new constants: credit the
/// LAST commitment only if its flight is at least RTprop(last.path) old at
/// ack time; otherwise credit the previous commitment (whose flight is
/// older by construction). An unknown RTprop (warm-up) counts as
/// qualified — legacy attribution, no behavior cliff.
pub(crate) fn resolve_flight_path(
    commit: &SendCommit,
    now_us: u64,
    mut rtprop_us_of: impl FnMut(u32) -> Option<u64>,
) -> u32 {
    match commit.prev {
        None => commit.last.0,
        Some((prev_path, _)) => {
            let age = now_us.saturating_sub(commit.last.1);
            let qualified = rtprop_us_of(commit.last.0).map_or(true, |rtp| age >= rtp);
            if qualified {
                commit.last.0
            } else {
                prev_path
            }
        }
    }
}

impl CopaFeed {
    pub(crate) fn new() -> Self {
        Self {
            seq_path: DashMap::new(),
            cursor: parking_lot::Mutex::new(CopaFeedCursor::default()),
            sampling_only: false,
            attr_witness: false,
            attr_cross: AtomicU64::new(0),
            attr_witness_prev: AtomicU64::new(0),
        }
    }

    /// feat/anchor-hygiene (`RWM_PLAIN_RS`): sampling-only construction.
    /// The flight-time witness (residual (iii)) defaults ON here —
    /// `RWM_RS_ATTR=0` restores legacy last-sent-path attribution as the
    /// same-binary control arm.
    pub(crate) fn new_sampling_only(attr_witness: bool) -> Self {
        Self {
            sampling_only: true,
            attr_witness,
            ..Self::new()
        }
    }

    /// True when this feed also OWNS the CC operating point (the Copa-sole
    /// pass-through mode). Sampling-only mode leaves cwnd dynamics, store-cap
    /// law, and percap pipe derivation on their legacy branches.
    pub(crate) fn owns_cc(&self) -> bool {
        !self.sampling_only
    }

    /// DIAG (residual (iii)): (cross-path-history attributions, of which
    /// witness-credited-to-previous-flight). Read only at the DIAG print.
    pub(crate) fn attr_diag(&self) -> (u64, u64) {
        (
            self.attr_cross.load(Ordering::Relaxed),
            self.attr_witness_prev.load(Ordering::Relaxed),
        )
    }

    /// Record a (re)send of source seq `seq` on `path`. A cross-path
    /// retransmit keeps the previous commitment as the flight-witness
    /// fallback (residual (iii)); a same-path resend just refreshes the
    /// send time (its rate sample is re-snapshotted by `on_src_sent`).
    pub(crate) fn on_sent(&self, seq: u64, path: u32) {
        let now = now_us();
        match self.seq_path.entry(seq) {
            dashmap::mapref::entry::Entry::Occupied(mut e) => {
                let cur = *e.get();
                *e.get_mut() = SendCommit {
                    last: (path, now),
                    prev: if cur.last.0 != path {
                        Some(cur.last)
                    } else {
                        cur.prev
                    },
                };
            }
            dashmap::mapref::entry::Entry::Vacant(v) => {
                v.insert(SendCommit {
                    last: (path, now),
                    prev: None,
                });
            }
        }
    }

    /// Diff one WindowAck against the cursor: returns the seqs this ack
    /// NEWLY proves delivered (frontier advance up to `received_up_to`,
    /// inclusive, plus never-before-seen SACKed seqs above it), each exactly
    /// once across the whole ack stream. Out-of-order/duplicate acks yield
    /// an empty diff — never a double attribution.
    pub(crate) fn newly_delivered(&self, received_up_to: u64, sack_ranges: &[(u64, u64)]) -> Vec<u64> {
        // Per-ack safety bound: a corrupt/hostile ack must not trap us in a
        // multi-million-seq loop. Honest ranges are bounded by the sender's
        // outstanding store (≤ a few thousand).
        const MAX_PER_ACK: usize = 65_536;
        let mut newly = Vec::new();
        let mut c = self.cursor.lock();
        while c.next <= received_up_to && newly.len() < MAX_PER_ACK {
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

/// feat/copa-sole-cc: attribute one WindowAck's newly-delivered seqs to their
/// paths and run the per-path Copa machinery on them: send-interval rate
/// sample per seq (`on_src_delivered_seq` — feeds the windowed-max BtlBw with
/// clean Δt), in-flight release, the per-SRTT cwnd update/backoff
/// (`on_delivery_signal`), and finally the pass-through substrate window
/// write (no-op unless RWM_QUIC_CC=passthrough). Call AFTER recording the
/// ack's RTT sample so the update sees the freshest queue signal.
pub(crate) fn copa_feed_attribute(
    feed: &CopaFeed,
    ack_path: u32,
    received_up_to: u64,
    sack_ranges: &[(u64, u64)],
    scheduler: &Arc<parking_lot::Mutex<Scheduler>>,
    transport: &Arc<QuicTransport>,
    stats: &Arc<SharedStats>,
) {
    let newly = feed.newly_delivered(received_up_to, sack_ranges);
    if newly.is_empty() {
        return;
    }
    let now = now_us();
    let mut sched = scheduler.lock();
    let per_path = copa_attribute_newly(feed, ack_path, now, &newly, &mut sched);
    // feat/anchor-hygiene (`RWM_PLAIN_RS`): sampling-only mode stops here —
    // the rate samples above are the whole job. The cwnd dynamics keep their
    // legacy per-batch-Ack call site, and the substrate window is whatever
    // RWM_QUIC_CC says (this feed does not own the operating point).
    if !feed.owns_cc() {
        return;
    }
    for (p, _n) in per_path {
        if let Some(ps) = sched.path_mut(p) {
            // feat/copa-compete: feed the wire-level loss evidence (the
            // pass-through shim's recorded congestion-event counter) into the
            // competitive AIMD before the update consumes it. No-op unless
            // RWM_COPA_COMPETE is active.
            if crate::scheduler::copa_compete_active() {
                if let Some((ev, _, _)) = transport.cc_passthrough_stats(p) {
                    ps.on_wire_congestion_events(ev);
                }
            }
            // NOT release_in_flight here: the per-batch Ack arm keeps doing
            // the wire-level in-flight release (it covers repairs too);
            // releasing again per attributed source seq would double-count.
            ps.on_delivery_signal();
            transport.set_cc_window_bytes(p, ps.cwnd as u64 * COPA_SOLE_BYTES_PER_SYMBOL);
            if let Some(st) = stats.path(p) {
                st.cwnd.store(ps.cwnd as u64, Ordering::Relaxed);
                st.in_flight.store(ps.in_flight as u64, Ordering::Relaxed);
            }
        }
    }
}

/// The per-seq attribution loop of [`copa_feed_attribute`], under the
/// scheduler lock the CALLER holds: resolve each newly-delivered seq's
/// carrying path (send record → flight-time witness → ack-path fallback) and
/// run that path's send-interval rate sampler (`on_src_delivered_seq`).
/// Returns the per-path attribution counts for the (Copa-sole only) cwnd
/// pass that follows.
///
/// Extracted 2026-08-11 (GOAL "HONEST INPUTS" phase 3, probe 1) so the c1
/// lock-blocking bench can drive the EXACT production attribution body under
/// the production lock from a two-thread component bench (MEASUREMENT
/// DISCIPLINE rule 1: prove the mechanism under test executes). Sole
/// non-test caller is `copa_feed_attribute`; behavior identical to the
/// pre-extraction inline loop.
pub(crate) fn copa_attribute_newly(
    feed: &CopaFeed,
    ack_path: u32,
    now: u64,
    newly: &[u64],
    sched: &mut Scheduler,
) -> std::collections::HashMap<u32, u32> {
    let mut per_path: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for &seq in newly {
        // Attribute to the path whose FLIGHT delivered the seq. Default:
        // the path it was last sent on; a seq without a send record
        // (pre-feed traffic, evicted record) falls back to the path the
        // ack arrived on — plain in-order acks ride the arrival path.
        // Residual (iii): when the commit history crossed paths, the
        // flight-time witness decides — an ack arriving sooner after a
        // cross-path retransmit than that path's RTprop proves the
        // delivering copy was the ORIGINAL flight, so the retransmit path's
        // delivered counter must NOT advance (the ×3–5 slow-path BtlBw
        // over-read under multipath placement; `resolve_flight_path`).
        let p = match feed.seq_path.remove(&seq) {
            Some((_, commit)) => {
                if commit.prev.is_some() {
                    feed.attr_cross.fetch_add(1, Ordering::Relaxed);
                    let witness = resolve_flight_path(&commit, now, |pid| {
                        sched
                            .path(pid)
                            .and_then(|ps| ps.min_rtt())
                            .map(|d| d.as_micros() as u64)
                    });
                    if witness != commit.last.0 {
                        feed.attr_witness_prev.fetch_add(1, Ordering::Relaxed);
                    }
                    if feed.attr_witness {
                        witness
                    } else {
                        commit.last.0
                    }
                } else {
                    commit.last.0
                }
            }
            None => ack_path,
        };
        if let Some(ps) = sched.path_mut(p) {
            ps.on_src_delivered_seq(seq);
        }
        *per_path.entry(p).or_insert(0) += 1;
    }
    per_path
}
