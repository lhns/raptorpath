//! Reorder buffer for window-mode receiver.
//!
//! Holds out-of-order recovered symbols and delivers them in sequence order.
//! Expired entries are force-delivered after a timeout.

use bytes::Bytes;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Reorder buffer that holds out-of-order recovered symbols and delivers them
/// in sequence order. Expired entries are force-delivered after a timeout.
pub struct ReorderBuffer {
    /// Pending out-of-order entries: seq → (data, buffered_at)
    pending: BTreeMap<u64, (Bytes, Instant)>,
    /// Next sequence to deliver in order
    next_deliver_seq: u64,
    /// How long to hold an entry before force-delivering
    timeout: Duration,
    /// Maximum entries to buffer before force-draining
    max_buffered: usize,
    /// RWM Phase A (paper 15.7/16.3 RETAIN-UNTIL-ACKED): when true, the
    /// buffer NEVER delivers past a hole — no expiry force-delivery, no
    /// capacity force-drain. Holes are recovered by NACK/repair (the sender
    /// retains sent bytes in its ARQ store and retransmits until
    /// delivered); memory stays bounded by that store's backpressure cap
    /// (un-acked in flight ≤ RELIABLE_STORE_MAX), not by this buffer.
    reliable: bool,
}

impl ReorderBuffer {
    pub fn new(timeout_ms: u64, max_buffered: usize) -> Self {
        Self {
            pending: BTreeMap::new(),
            next_deliver_seq: 0,
            timeout: Duration::from_millis(timeout_ms),
            max_buffered,
            reliable: false,
        }
    }

    /// Reliable-policy buffer (RWM Phase A): in-order delivery with holes
    /// held until recovered — never force-delivered, never force-drained.
    pub fn new_reliable() -> Self {
        Self {
            pending: BTreeMap::new(),
            next_deliver_seq: 0,
            timeout: Duration::ZERO, // unused: expiry never gives up on a hole
            max_buffered: usize::MAX,
            reliable: true,
        }
    }

    /// Push a recovered symbol. Returns a contiguous prefix of deliverable entries.
    /// Uses `Instant::now()` for the buffered timestamp.
    pub fn push(&mut self, seq: u64, data: Bytes) -> Vec<(u64, Bytes)> {
        self.push_with_time(seq, data, Instant::now())
    }

    /// Push a recovered symbol with an explicit timestamp (for MockClock-driven tests).
    /// Returns a contiguous prefix of deliverable entries.
    pub fn push_with_time(&mut self, seq: u64, data: Bytes, now: Instant) -> Vec<(u64, Bytes)> {
        if seq < self.next_deliver_seq {
            // The in-order gate has already advanced past this sequence (an
            // expiry gave up on the hole). Holding a late fill buys no
            // ordering — it would only strand for a full extra timeout
            // until the next drain_expired sweep. Deliver immediately.
            return vec![(seq, data)];
        }
        self.pending.insert(seq, (data, now));

        // Force-drain oldest if over capacity (EVICT policy only — the
        // reliable policy never delivers past a hole; its memory bound is
        // the sender's sent-data store cap, enforced by backpressure there).
        if !self.reliable && self.pending.len() > self.max_buffered {
            return self.force_drain_oldest();
        }

        self.drain_contiguous()
    }

    /// Drain the contiguous prefix starting from `next_deliver_seq`.
    pub fn drain_contiguous(&mut self) -> Vec<(u64, Bytes)> {
        let mut result = Vec::new();
        while let Some((data, _)) = self.pending.remove(&self.next_deliver_seq) {
            result.push((self.next_deliver_seq, data));
            self.next_deliver_seq += 1;
        }
        result
    }

    /// Deliver entries held longer than `timeout`, plus any contiguous prefix.
    ///
    /// Expiring seq k means giving up on the holes before it — so every
    /// pending entry up to k is released too, in order. (Advancing
    /// `next_deliver_seq` past k while younger entries were still pending
    /// used to STRAND them: a hole filled by FEC/retransmit just after a
    /// later entry expired would sit for a full extra timeout.)
    pub fn drain_expired(&mut self, now: Instant) -> Vec<(u64, Bytes)> {
        let mut result = Vec::new();

        // Reliable policy: a hole is never given up on — recovery (NACK /
        // repair / sender tail sweep) fills it, and delivery resumes via
        // the contiguous drain in push. Nothing ever "expires".
        if self.reliable {
            return result;
        }

        // The largest expired sequence determines how far we give up.
        let max_expired = self
            .pending
            .iter()
            .filter(|(_, (_, buffered_at))| now.duration_since(*buffered_at) >= self.timeout)
            .map(|(&seq, _)| seq)
            .max();

        let Some(k) = max_expired else {
            return result;
        };

        // Release every pending entry up to and including k, in order.
        let to_deliver: Vec<u64> = self.pending.range(..=k).map(|(&seq, _)| seq).collect();
        for seq in to_deliver {
            if let Some((data, _)) = self.pending.remove(&seq) {
                result.push((seq, data));
            }
        }
        self.next_deliver_seq = self.next_deliver_seq.max(k + 1);

        // Also drain any newly contiguous entries
        result.extend(self.drain_contiguous());
        result
    }

    /// Force-drain the oldest entries to get back under capacity.
    pub fn force_drain_oldest(&mut self) -> Vec<(u64, Bytes)> {
        let mut result = Vec::new();
        while self.pending.len() > self.max_buffered / 2 {
            if let Some((&seq, _)) = self.pending.iter().next() {
                if let Some((data, _)) = self.pending.remove(&seq) {
                    result.push((seq, data));
                    if seq >= self.next_deliver_seq {
                        self.next_deliver_seq = seq + 1;
                    }
                }
            } else {
                break;
            }
        }
        result.extend(self.drain_contiguous());
        result
    }

    /// Number of pending entries in the buffer.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Update the hold timeout (block-mode uses an SRTT-adaptive hold:
    /// the hole must survive two ARQ repair rounds ≈ 4×SRTT).
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// When the oldest pending entry expires (drives the drain timer).
    /// None if nothing is pending — or under the reliable policy, where
    /// nothing ever expires (the recovery timer is armed separately).
    pub fn oldest_deadline(&self) -> Option<Instant> {
        if self.reliable {
            return None;
        }
        self.pending
            .values()
            .map(|&(_, buffered_at)| buffered_at + self.timeout)
            .min()
    }

    /// The next sequence number expected for in-order delivery.
    pub fn next_deliver_seq(&self) -> u64 {
        self.next_deliver_seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(x: u8) -> Bytes {
        Bytes::from(vec![x])
    }

    /// RWM Phase A: the reliable receiver holds delivery at a hole until the
    /// hole is recovered — expiry never force-delivers past it.
    #[test]
    fn reliable_holds_past_hole_until_recovered() {
        let mut rb = ReorderBuffer::new_reliable();
        assert_eq!(rb.push(0, b(0)).len(), 1); // in order → delivered
        // seq 1 lost: 2..=5 arrive and must be held
        for seq in 2..=5u64 {
            assert!(rb.push(seq, b(seq as u8)).is_empty(), "seq {seq} must be held behind the hole");
        }
        // No amount of elapsed time gives up on the hole
        let far_future = Instant::now() + Duration::from_secs(3600);
        assert!(rb.drain_expired(far_future).is_empty());
        assert!(rb.oldest_deadline().is_none(), "reliable buffer never arms an expiry");
        assert_eq!(rb.next_deliver_seq(), 1);
        // The hole is recovered → everything drains in order
        let out = rb.push(1, b(1));
        let seqs: Vec<u64> = out.iter().map(|(s, _)| *s).collect();
        assert_eq!(seqs, vec![1, 2, 3, 4, 5]);
        assert_eq!(rb.next_deliver_seq(), 6);
    }

    /// RWM Phase A: the reliable receiver never force-drains on capacity —
    /// buffering is bounded by the sender's ack-gated window, not evicted here.
    #[test]
    fn reliable_never_force_drains_at_capacity() {
        let mut rb = ReorderBuffer::new_reliable();
        // hole at seq 0; buffer far more than the lossy default cap (500)
        for seq in 1..=800u64 {
            assert!(rb.push(seq, b(0)).is_empty());
        }
        assert_eq!(rb.pending_count(), 800);
        assert_eq!(rb.next_deliver_seq(), 0, "no eviction may skip the hole");
        let out = rb.push(0, b(0));
        assert_eq!(out.len(), 801, "recovering the hole releases the whole prefix");
    }

    /// Contrast: the EVICT-policy buffer force-delivers past holes on expiry
    /// (correct for Realtime's δ — a stale packet is worthless).
    #[test]
    fn evict_policy_force_delivers_on_expiry() {
        let mut rb = ReorderBuffer::new(10, 500);
        let now = Instant::now();
        assert!(rb.push_with_time(1, b(1), now).is_empty()); // hole at 0
        let out = rb.drain_expired(now + Duration::from_millis(20));
        assert_eq!(out.len(), 1, "lossy policy gives up on the hole");
        assert_eq!(rb.next_deliver_seq(), 2);
    }
}
