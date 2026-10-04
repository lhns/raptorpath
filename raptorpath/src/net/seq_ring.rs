//! [`SeqRing`]: a `VecDeque`-backed map keyed by a (near-)contiguous `u64`
//! sequence number, the drop-in replacement for the window sender's
//! `BTreeMap<u64, V>` per-seq stores.
//!
//! The sender's per-seq maps (`sent_store`, `retransmit_buffer`,
//! `source_path_map`) are keyed by the encoder's source seq, which is
//! assigned `0, 1, 2, ...` and inserted in that order, and are pruned only as
//! a PREFIX (the cumulative-ack `split_off` / the `retain(seq >= floor)`
//! window floor). That key space is a dense interval, so a ring indexed by
//! `seq − base` answers every query the `BTreeMap` answered — `get`,
//! `insert`, `remove`, prefix prune, `range`, first/last key, `len`, the
//! below-`f` count — in O(1) / O(k) with no per-insert node allocation and no
//! per-ack tree rebuild (`split_off` allocated new nodes on every ack).
//!
//! Semantics are exactly the `BTreeMap`'s for ANY key pattern (the
//! differential test in `net/tests` drives both with the same random script):
//! individual removals leave a `None` hole (the `retransmit_buffer`'s shed /
//! NACK removals), holes at either end are trimmed so the first / last key
//! stay O(1), and an insert below `base` or past the end pads with holes.
//! Only the cost model assumes density: a sparse key set would cost one empty
//! slot per absent key between the first and last present key.

use std::collections::VecDeque;

/// A `u64`-keyed map over a dense key interval. Invariant: `slots` is empty,
/// or both its front and back are `Some`; `base` is the key of `slots[0]`;
/// `len` is the number of `Some` slots.
#[derive(Debug, Clone)]
pub struct SeqRing<V> {
    base: u64,
    slots: VecDeque<Option<V>>,
    len: usize,
}

impl<V> Default for SeqRing<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> SeqRing<V> {
    pub fn new() -> Self {
        Self { base: 0, slots: VecDeque::new(), len: 0 }
    }

    /// Number of present keys (`BTreeMap::len`).
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    fn idx(&self, seq: u64) -> Option<usize> {
        if seq < self.base {
            return None;
        }
        let off = seq - self.base;
        if off < self.slots.len() as u64 { Some(off as usize) } else { None }
    }

    /// `BTreeMap::get`.
    #[inline]
    pub fn get(&self, seq: &u64) -> Option<&V> {
        self.idx(*seq).and_then(|i| self.slots[i].as_ref())
    }

    /// `BTreeMap::contains_key`.
    #[inline]
    pub fn contains_key(&self, seq: &u64) -> bool {
        self.get(seq).is_some()
    }

    /// `BTreeMap::insert`: returns the previous value at `seq`, if any.
    pub fn insert(&mut self, seq: u64, v: V) -> Option<V> {
        if self.slots.is_empty() {
            self.base = seq;
            self.slots.push_back(Some(v));
            self.len = 1;
            return None;
        }
        let end = self.base + self.slots.len() as u64;
        if seq >= end {
            // The hot case: the next seq (`seq == end`) is one push.
            for _ in end..seq {
                self.slots.push_back(None);
            }
            self.slots.push_back(Some(v));
            self.len += 1;
            None
        } else if seq >= self.base {
            let slot = &mut self.slots[(seq - self.base) as usize];
            let old = slot.replace(v);
            if old.is_none() {
                self.len += 1;
            }
            old
        } else {
            for _ in (seq + 1)..self.base {
                self.slots.push_front(None);
            }
            self.slots.push_front(Some(v));
            self.base = seq;
            self.len += 1;
            None
        }
    }

    /// `BTreeMap::remove`.
    pub fn remove(&mut self, seq: &u64) -> Option<V> {
        let i = self.idx(*seq)?;
        let old = self.slots[i].take();
        if old.is_some() {
            self.len -= 1;
            self.trim();
        }
        old
    }

    /// Restore the invariant: drop `None` slots at both ends.
    fn trim(&mut self) {
        while let Some(None) = self.slots.front() {
            self.slots.pop_front();
            self.base += 1;
        }
        while let Some(None) = self.slots.back() {
            self.slots.pop_back();
        }
    }

    /// Drop every key `< floor` — `*self = self.split_off(&floor)` and
    /// `retain(|&k, _| k >= floor)` on a `BTreeMap`.
    pub fn prune_below(&mut self, floor: u64) {
        while self.base < floor {
            match self.slots.pop_front() {
                Some(s) => {
                    if s.is_some() {
                        self.len -= 1;
                    }
                    self.base += 1;
                }
                None => break,
            }
        }
        self.trim();
    }

    /// The smallest key and its value (`BTreeMap::iter().next()`).
    #[inline]
    pub fn first(&self) -> Option<(u64, &V)> {
        self.slots.front().and_then(|s| s.as_ref()).map(|v| (self.base, v))
    }

    /// The largest key (`BTreeMap::keys().next_back()`).
    #[inline]
    pub fn last_key(&self) -> Option<u64> {
        if self.slots.is_empty() { None } else { Some(self.base + self.slots.len() as u64 - 1) }
    }

    /// The present keys in `[lo, hi]` with their values, ascending
    /// (`BTreeMap::range(lo..=hi)`; empty when `hi < lo`).
    pub fn range(&self, lo: u64, hi: u64) -> impl Iterator<Item = (u64, &V)> + '_ {
        let n = self.slots.len() as u64;
        let a = lo.max(self.base).saturating_sub(self.base).min(n);
        let b = if hi < self.base || hi < lo {
            a
        } else {
            (hi - self.base).saturating_add(1).min(n).max(a)
        };
        let base = self.base;
        self.slots
            .range(a as usize..b as usize)
            .enumerate()
            .filter_map(move |(i, s)| s.as_ref().map(|v| (base + a + i as u64, v)))
    }

    /// Every present key with its value, ascending (`BTreeMap::iter`).
    pub fn iter(&self) -> impl Iterator<Item = (u64, &V)> + '_ {
        let base = self.base;
        self.slots
            .iter()
            .enumerate()
            .filter_map(move |(i, s)| s.as_ref().map(|v| (base + i as u64, v)))
    }

    /// The number of present keys `< f` (`BTreeMap::range(..f).count()`).
    /// O(1) while the ring has no holes (the sent store never has any).
    pub fn count_below(&self, f: u64) -> usize {
        if f <= self.base {
            return 0;
        }
        let k = ((f - self.base).min(self.slots.len() as u64)) as usize;
        if self.len == self.slots.len() {
            k
        } else {
            self.slots.range(..k).filter(|s| s.is_some()).count()
        }
    }
}

/// The read-only view of a retained per-seq store that the SACK release law
/// and the store gate need (`net/sack.rs`): implemented by the sender's
/// [`SeqRing`] store and by `BTreeMap` (the model the law's tests drive).
pub trait RetainedSeqs {
    /// Number of retained seqs.
    fn retained_len(&self) -> usize;
    /// The highest retained seq.
    fn last_seq(&self) -> Option<u64>;
    /// Number of retained seqs `< f`.
    fn retained_below(&self, f: u64) -> usize;
    /// Visit the retained seqs in `[start, end]`, ascending.
    fn for_each_seq_in(&self, start: u64, end: u64, f: impl FnMut(u64));
}

impl<V> RetainedSeqs for SeqRing<V> {
    fn retained_len(&self) -> usize {
        self.len()
    }
    fn last_seq(&self) -> Option<u64> {
        self.last_key()
    }
    fn retained_below(&self, f: u64) -> usize {
        self.count_below(f)
    }
    fn for_each_seq_in(&self, start: u64, end: u64, mut f: impl FnMut(u64)) {
        for (k, _) in self.range(start, end) {
            f(k);
        }
    }
}

impl<V> RetainedSeqs for std::collections::BTreeMap<u64, V> {
    fn retained_len(&self) -> usize {
        self.len()
    }
    fn last_seq(&self) -> Option<u64> {
        self.keys().next_back().copied()
    }
    fn retained_below(&self, f: u64) -> usize {
        self.range(..f).count()
    }
    fn for_each_seq_in(&self, start: u64, end: u64, mut f: impl FnMut(u64)) {
        for (&k, _) in self.range(start..=end) {
            f(k);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Differential pin: [`SeqRing`] against the `BTreeMap` it replaces,
    //! driven by the same seeded random script, compared after EVERY op on
    //! every query the sender makes (`get`, `len`, first entry, last key,
    //! `range`, below-`f` count, full iteration) and on the
    //! [`RetainedSeqs`] view the SACK law / store gate read.

    use super::*;
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    use std::collections::{BTreeMap, BTreeSet};

    fn assert_same(ring: &SeqRing<u64>, map: &BTreeMap<u64, u64>, rng: &mut ChaCha8Rng, top: u64) {
        assert_eq!(ring.len(), map.len());
        assert_eq!(ring.is_empty(), map.is_empty());
        assert_eq!(ring.first().map(|(k, &v)| (k, v)), map.iter().next().map(|(&k, &v)| (k, v)));
        assert_eq!(ring.last_key(), map.keys().next_back().copied());
        assert_eq!(
            ring.iter().map(|(k, &v)| (k, v)).collect::<Vec<_>>(),
            map.iter().map(|(&k, &v)| (k, v)).collect::<Vec<_>>()
        );
        for _ in 0..4 {
            let a = rng.gen_range(0..top + 8);
            let b = rng.gen_range(0..top + 8);
            assert_eq!(ring.get(&a), map.get(&a));
            assert_eq!(ring.count_below(a), map.range(..a).count());
            assert_eq!(RetainedSeqs::retained_below(ring, a), RetainedSeqs::retained_below(map, a));
            let (lo, hi) = (a.min(b), a.max(b));
            assert_eq!(
                ring.range(lo, hi).map(|(k, &v)| (k, v)).collect::<Vec<_>>(),
                map.range(lo..=hi).map(|(&k, &v)| (k, v)).collect::<Vec<_>>()
            );
            // hi < lo is empty (the BTreeMap would panic; callers guard it).
            if lo < hi {
                assert_eq!(ring.range(hi, lo).count(), 0);
            }
        }
    }

    /// The sender's access pattern (monotone inserts, prefix prunes at a
    /// non-monotone floor, occasional individual removals, lookups) plus
    /// out-of-pattern inserts (gaps, re-inserts, below-base) so the
    /// semantics are the map's for ANY key pattern.
    #[test]
    fn seq_ring_matches_btreemap_under_random_script() {
        for seed in 0..24u64 {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let mut ring: SeqRing<u64> = SeqRing::new();
            let mut map: BTreeMap<u64, u64> = BTreeMap::new();
            let mut next = rng.gen_range(0..3u64);
            let mut floor = 0u64;
            for step in 0..3000u64 {
                let op = rng.gen_range(0..100);
                match op {
                    0..=54 => {
                        // The hot case: the next consecutive seq.
                        assert_eq!(ring.insert(next, step), map.insert(next, step));
                        next += 1;
                    }
                    55..=59 => {
                        // A gap in the key space.
                        next += rng.gen_range(1..5);
                        assert_eq!(ring.insert(next, step), map.insert(next, step));
                        next += 1;
                    }
                    60..=64 => {
                        // Re-insert / insert anywhere (incl. below base).
                        let k = rng.gen_range(0..next + 2);
                        assert_eq!(ring.insert(k, step), map.insert(k, step));
                        next = next.max(k + 1);
                    }
                    65..=79 => {
                        // Individual removal (shed / NACK), biased to the front.
                        let lo = map.keys().next().copied().unwrap_or(0);
                        let k = if rng.gen_bool(0.5) { lo } else { rng.gen_range(0..next + 2) };
                        assert_eq!(ring.remove(&k), map.remove(&k));
                    }
                    80..=94 => {
                        // Prefix prune at the cumulative point (may regress:
                        // acks from different paths, the EVICT window floor).
                        floor = if rng.gen_bool(0.8) {
                            (floor + rng.gen_range(0..8)).min(next + 3)
                        } else {
                            floor.saturating_sub(rng.gen_range(0..10))
                        };
                        ring.prune_below(floor);
                        map = map.split_off(&floor);
                    }
                    _ => {
                        // retain(seq >= floor) twin.
                        let f = rng.gen_range(0..next + 3);
                        ring.prune_below(f);
                        map.retain(|&k, _| k >= f);
                    }
                }
                assert_same(&ring, &map, &mut rng, next);
            }
        }
    }

    /// The SACK release law and the store gate read the ring exactly as
    /// they read the map (same newly-released seqs, same gate count).
    #[test]
    fn sack_law_reads_ring_as_map() {
        use crate::net::{sack_release_mark, store_gate_released, AboveReport};
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let mut ring: SeqRing<u8> = SeqRing::new();
        let mut map: BTreeMap<u64, u8> = BTreeMap::new();
        let (mut rel_r, mut rel_m) = (BTreeSet::new(), BTreeSet::new());
        let mut next = 0u64;
        let mut ack = 0u64;
        for _ in 0..2000 {
            for _ in 0..rng.gen_range(0..6) {
                ring.insert(next, 0);
                map.insert(next, 0);
                next += 1;
            }
            let a = rng.gen_range(ack..next + 2);
            let b = a + rng.gen_range(0..6);
            assert_eq!(
                sack_release_mark(&ring, &mut rel_r, a, b),
                sack_release_mark(&map, &mut rel_m, a, b)
            );
            if rng.gen_bool(0.3) {
                ack = (ack + rng.gen_range(0..5)).min(next);
                ring.prune_below(ack);
                map = map.split_off(&ack);
                rel_r = rel_r.split_off(&ack);
                rel_m = rel_m.split_off(&ack);
            }
            let rep = AboveReport {
                next_expected: rng.gen_range(0..next + 3),
                received_above: rng.gen_range(0..20),
            };
            assert_eq!(
                store_gate_released(&ring, rel_r.len(), rep),
                store_gate_released(&map, rel_m.len(), rep)
            );
        }
    }
}
