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
}

impl ReorderBuffer {
    pub fn new(timeout_ms: u64, max_buffered: usize) -> Self {
        Self {
            pending: BTreeMap::new(),
            next_deliver_seq: 0,
            timeout: Duration::from_millis(timeout_ms),
            max_buffered,
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

        // Force-drain oldest if over capacity
        if self.pending.len() > self.max_buffered {
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

    /// The next sequence number expected for in-order delivery.
    pub fn next_deliver_seq(&self) -> u64 {
        self.next_deliver_seq
    }
}
