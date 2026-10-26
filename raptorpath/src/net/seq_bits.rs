//! [`SeqBits`]: the receiver's received-seq set as a ring bitmap, the
//! drop-in replacement for the `BTreeSet<u64>` the receiver task kept
//! (threading R1: a node allocation per source symbol and a tree walk per
//! SACK / `received_above` read, on the one task that is the c1d ceiling).
//!
//! The set is only ever (a) inserted into, (b) probed, (c) read over a range
//! (`received_above`'s frontier moves, the SACK run list) and (d) pruned as a
//! PREFIX (`prune_below`, the receiver's `split_off`). A bitmap anchored at a
//! word-aligned `base` at or below the prune `floor` answers all four with no
//! per-seq allocation: one bit per seq, whole words popped off the front on a
//! prune, zero words pushed on the back as the highest seq grows.
//!
//! **Behaviour the `BTreeSet` did not have, declared and bounded.**
//! * *Below the floor.* A seq below the last prune point is already delivered
//!   and pruned; a late copy of it is a duplicate by definition. `insert`
//!   refuses it (returns `false`, "not newly") instead of re-inserting it
//!   until the next prune. Nothing the receiver reads sits below the
//!   cumulative point, so no observable changes (`len()` aside, a wire-debug
//!   datum).
//! * *Beyond the span.* The bitmap grows on demand up to `max_span` seqs from
//!   its base ([`MAX_SPAN`], 2 MiB of bits). An insert past that is REFUSED
//!   and counted ([`SeqBits::refused`]); the seq is simply not recorded.
//!   The degradation is one-sided: the set may UNDER-claim (a seq that did
//!   arrive is not reported in a SACK, so the sender may redundantly resend
//!   it) but it never claims a seq that was not inserted. Reaching the bound
//!   needs a hole older than ~16 M seqs behind the highest, orders beyond the
//!   sender's store cap; the `BTreeSet` it replaces would have grown without
//!   bound there.

use std::collections::{BTreeSet, VecDeque};

/// Default span cap, in seqs from `base` (a power-of-two number of bits).
pub const MAX_SPAN: u64 = 1 << 24;

/// The read side the receiver's SACK and `received_above` laws need. Both
/// the production [`SeqBits`] and the reference `BTreeSet<u64>` implement it,
/// so the laws are written once and the differential tests drive both.
pub trait ReceivedSeqs {
    /// Membership.
    fn has(&self, seq: u64) -> bool;
    /// `|{s ∈ set : lo ≤ s < hi}|`.
    fn count_in(&self, lo: u64, hi: u64) -> u64;
    /// The maximal runs of consecutive members within `[lo, hi_incl]`,
    /// ascending, as inclusive `(start, end)` (a run is clipped to the
    /// bounds). `f` returns `false` to stop early.
    fn each_run(&self, lo: u64, hi_incl: u64, f: &mut dyn FnMut(u64, u64) -> bool);
}

impl ReceivedSeqs for BTreeSet<u64> {
    fn has(&self, seq: u64) -> bool {
        self.contains(&seq)
    }

    fn count_in(&self, lo: u64, hi: u64) -> u64 {
        if lo >= hi {
            return 0;
        }
        self.range(lo..hi).count() as u64
    }

    fn each_run(&self, lo: u64, hi_incl: u64, f: &mut dyn FnMut(u64, u64) -> bool) {
        if lo > hi_incl {
            return;
        }
        let mut cur: Option<(u64, u64)> = None;
        for &s in self.range(lo..=hi_incl) {
            match cur {
                Some((a, e)) if e + 1 == s => cur = Some((a, s)),
                Some((a, e)) => {
                    if !f(a, e) {
                        return;
                    }
                    cur = Some((s, s));
                }
                None => cur = Some((s, s)),
            }
        }
        if let Some((a, e)) = cur {
            f(a, e);
        }
    }
}

/// A ring bitmap over `u64` seqs. Invariants: `base` is a multiple of 64 and
/// `base ≤ floor`; every bit for a seq `< floor` is zero; `len` is the number
/// of set bits.
#[derive(Debug, Clone)]
pub struct SeqBits {
    base: u64,
    floor: u64,
    words: VecDeque<u64>,
    len: u64,
    max_span: u64,
    refused: u64,
}

impl Default for SeqBits {
    fn default() -> Self {
        Self::new()
    }
}

impl SeqBits {
    pub fn new() -> Self {
        Self::with_max_span(MAX_SPAN)
    }

    /// A set whose bitmap may span at most `max_span` seqs from its base
    /// (tests use a small one to reach the refusal path cheaply).
    pub fn with_max_span(max_span: u64) -> Self {
        Self { base: 0, floor: 0, words: VecDeque::new(), len: 0, max_span, refused: 0 }
    }

    /// Number of members.
    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Inserts refused for lying beyond the span cap (see the module doc).
    pub fn refused(&self) -> u64 {
        self.refused
    }

    /// One past the highest seq the bitmap covers.
    #[inline]
    fn end(&self) -> u64 {
        self.base + 64 * self.words.len() as u64
    }

    /// Add `seq`; `true` iff it was newly recorded (the `BTreeSet::insert`
    /// answer). A seq below the floor, or beyond the span cap, is not
    /// recorded and answers `false`.
    pub fn insert(&mut self, seq: u64) -> bool {
        if seq < self.floor {
            return false;
        }
        if self.words.is_empty() {
            self.base = self.floor & !63;
        }
        if seq - self.base >= self.max_span {
            self.refused += 1;
            return false;
        }
        let word = ((seq - self.base) / 64) as usize;
        if word >= self.words.len() {
            self.words.resize(word + 1, 0);
        }
        let bit = 1u64 << ((seq - self.base) % 64);
        let w = &mut self.words[word];
        let newly = *w & bit == 0;
        *w |= bit;
        self.len += newly as u64;
        newly
    }

    /// Membership.
    #[inline]
    pub fn contains(&self, seq: u64) -> bool {
        if seq < self.base || seq >= self.end() {
            return false;
        }
        self.words[((seq - self.base) / 64) as usize] >> ((seq - self.base) % 64) & 1 == 1
    }

    /// Drop every member below `x` (the receiver's `split_off(&x)`), and
    /// make `x` the floor. A floor never moves down.
    pub fn prune_below(&mut self, x: u64) {
        if x <= self.floor {
            return;
        }
        self.floor = x;
        if x >= self.end() {
            self.words.clear();
            self.len = 0;
            self.base = x & !63;
            return;
        }
        while self.base + 64 <= x {
            let w = self.words.pop_front().expect("x < end implies a word");
            self.len -= w.count_ones() as u64;
            self.base += 64;
        }
        let off = x - self.base;
        if off > 0 {
            let w = &mut self.words[0];
            let dropped = *w & ((1u64 << off) - 1);
            self.len -= dropped.count_ones() as u64;
            *w &= !((1u64 << off) - 1);
        }
    }

    /// The first set bit at or after `from`, if any.
    fn next_set(&self, from: u64) -> Option<u64> {
        let from = from.max(self.base);
        if from >= self.end() {
            return None;
        }
        let mut i = ((from - self.base) / 64) as usize;
        let mut w = self.words[i] & (!0u64 << ((from - self.base) % 64));
        loop {
            if w != 0 {
                return Some(self.base + 64 * i as u64 + w.trailing_zeros() as u64);
            }
            i += 1;
            if i >= self.words.len() {
                return None;
            }
            w = self.words[i];
        }
    }

    /// The first clear bit at or after `from` (`from` is covered); the
    /// coverage end if the bitmap is set to its last bit.
    fn next_clear(&self, from: u64) -> u64 {
        let mut i = ((from - self.base) / 64) as usize;
        let mut w = !self.words[i] & (!0u64 << ((from - self.base) % 64));
        loop {
            if w != 0 {
                return self.base + 64 * i as u64 + w.trailing_zeros() as u64;
            }
            i += 1;
            if i >= self.words.len() {
                return self.end();
            }
            w = !self.words[i];
        }
    }
}

impl ReceivedSeqs for SeqBits {
    #[inline]
    fn has(&self, seq: u64) -> bool {
        self.contains(seq)
    }

    fn count_in(&self, lo: u64, hi: u64) -> u64 {
        let lo = lo.max(self.base);
        let hi = hi.min(self.end());
        if lo >= hi {
            return 0;
        }
        let (a, b) = (lo - self.base, hi - self.base);
        let (wa, wb) = ((a / 64) as usize, ((b - 1) / 64) as usize);
        let head = !0u64 << (a % 64);
        let tail = !0u64 >> (63 - (b - 1) % 64);
        if wa == wb {
            return (self.words[wa] & head & tail).count_ones() as u64;
        }
        let mut n = (self.words[wa] & head).count_ones() as u64;
        for i in wa + 1..wb {
            n += self.words[i].count_ones() as u64;
        }
        n + (self.words[wb] & tail).count_ones() as u64
    }

    fn each_run(&self, lo: u64, hi_incl: u64, f: &mut dyn FnMut(u64, u64) -> bool) {
        let mut at = lo;
        while at <= hi_incl {
            let Some(start) = self.next_set(at) else { return };
            if start > hi_incl {
                return;
            }
            let end = self.next_clear(start).saturating_sub(1).min(hi_incl);
            if !f(start, end) {
                return;
            }
            if end == u64::MAX {
                return;
            }
            at = end + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::rand_core::{RngCore, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    /// The reference: the `BTreeSet` the receiver used, with the one declared
    /// difference applied (inserts below the last prune point are ignored)
    /// and, when `cap` is given, inserts at or past `base + cap` refused.
    struct Reference {
        set: BTreeSet<u64>,
        floor: u64,
    }

    impl Reference {
        fn insert(&mut self, s: u64) -> bool {
            s >= self.floor && self.set.insert(s)
        }

        fn prune(&mut self, x: u64) {
            if x > self.floor {
                self.floor = x;
                self.set = self.set.split_off(&x);
            }
        }
    }

    fn runs<S: ReceivedSeqs + ?Sized>(s: &S, lo: u64, hi: u64, cap: usize) -> Vec<(u64, u64)> {
        let mut v = Vec::new();
        s.each_run(lo, hi, &mut |a, b| {
            if v.len() >= cap {
                return false;
            }
            v.push((a, b));
            true
        });
        v
    }

    /// The receiver's frontier walk (`while received.contains(f) { f += 1 }`).
    fn frontier<S: ReceivedSeqs + ?Sized>(s: &S, mut f: u64) -> u64 {
        while s.has(f) {
            f += 1;
        }
        f
    }

    /// Within the span the bitmap is the `BTreeSet`: same `insert` answers,
    /// membership, frontier, `received_above`-style counts, SACK run lists
    /// (capped and uncapped) and length, over random arrival orders with
    /// holes, duplicates, prunes and late copies below the prune point.
    #[test]
    fn matches_btreeset_within_the_span() {
        for seed in 0..40u64 {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let mut bits = SeqBits::new();
            let mut want = Reference { set: BTreeSet::new(), floor: 0 };
            let mut next_expected = 0u64;
            let mut top = 0u64;
            for step in 0..4_000u32 {
                match rng.next_u32() % 10 {
                    0..=6 => {
                        // Mostly near the head, sometimes a long way ahead
                        // (a gap) or well behind (a late duplicate).
                        let s = match rng.next_u32() % 20 {
                            0 => top + (rng.next_u32() % 3000) as u64,
                            1 => next_expected.saturating_sub((rng.next_u32() % 300) as u64),
                            _ => next_expected + (rng.next_u32() % 150) as u64,
                        };
                        assert_eq!(bits.insert(s), want.insert(s), "seed {seed} step {step} insert {s}");
                        top = top.max(s);
                    }
                    7 | 8 => {
                        let f = frontier(&bits, next_expected);
                        assert_eq!(f, frontier(&want.set, next_expected), "frontier");
                        next_expected = f;
                        if rng.next_u32() % 5 == 0 {
                            next_expected += (rng.next_u32() % 4) as u64; // a hold-expiry jump
                        }
                    }
                    _ => {
                        let x = next_expected.saturating_sub((rng.next_u32() % 200) as u64);
                        bits.prune_below(x);
                        want.prune(x);
                    }
                }
                assert_eq!(bits.len(), want.set.len(), "len, seed {seed} step {step}");
                if step % 5 == 0 {
                    let (lo, hi) = (next_expected, top + 1);
                    assert_eq!(bits.count_in(lo, u64::MAX), want.set.count_in(lo, u64::MAX));
                    assert_eq!(bits.count_in(lo, hi), want.set.count_in(lo, hi));
                    for cap in [usize::MAX, 64, 3, 1] {
                        assert_eq!(
                            runs(&bits, lo, top, cap),
                            runs(&want.set, lo, top, cap),
                            "sack runs cap {cap}, seed {seed} step {step}"
                        );
                    }
                    for p in [lo, lo + 1, top, top + 7, want.floor, want.floor.saturating_sub(1)] {
                        assert_eq!(bits.contains(p), want.set.contains(&p), "contains {p}");
                    }
                }
            }
            assert_eq!(bits.refused(), 0);
        }
    }

    /// Past the span cap an insert is refused and counted; the set never
    /// claims a seq that was not inserted, claims every seq the reference
    /// holds that fit, and matches the reference exactly on what it kept.
    #[test]
    fn beyond_the_span_it_under_claims_and_never_over_claims() {
        const CAP: u64 = 512;
        for seed in 0..40u64 {
            let mut rng = ChaCha8Rng::seed_from_u64(1_000 + seed);
            let mut bits = SeqBits::with_max_span(CAP);
            let mut inserted: BTreeSet<u64> = BTreeSet::new(); // every seq offered and accepted
            let mut want = Reference { set: BTreeSet::new(), floor: 0 };
            let mut refused = 0u64;
            for step in 0..3_000u32 {
                if rng.next_u32() % 12 == 0 {
                    let x = want.floor + (rng.next_u32() % 400) as u64;
                    bits.prune_below(x);
                    want.prune(x);
                    inserted = inserted.split_off(&x);
                    continue;
                }
                // Heavy-tailed offsets so a good share lands beyond the cap.
                let off = match rng.next_u32() % 4 {
                    0 => (rng.next_u32() % 4000) as u64,
                    _ => (rng.next_u32() % 300) as u64,
                };
                let s = want.floor + off;
                let base_before = if bits.words.is_empty() { bits.floor & !63 } else { bits.base };
                let beyond = s >= base_before + CAP;
                let got = bits.insert(s);
                if beyond {
                    assert!(!got, "a beyond-span insert must be refused (seed {seed} step {step})");
                    refused += 1;
                } else {
                    assert_eq!(got, want.insert(s), "in-span insert answer");
                    inserted.insert(s);
                }
                // Never an over-claim: membership implies a prior accepted insert.
                assert!(bits.len() <= inserted.len());
            }
            assert_eq!(bits.refused(), refused, "every refusal is counted");
            // On everything it kept, it is exactly the reference restricted to
            // what was accepted: same members, same runs.
            let hi = want.floor + 4_100;
            assert_eq!(
                runs(&bits, want.floor, hi, usize::MAX),
                runs(&inserted, want.floor, hi, usize::MAX),
                "kept members, seed {seed}"
            );
            for s in want.floor..hi {
                if bits.contains(s) {
                    assert!(inserted.contains(&s), "over-claimed {s}");
                }
            }
            assert_eq!(bits.len(), inserted.len());
        }
    }

    /// A late copy below the prune floor is a duplicate by definition.
    #[test]
    fn a_seq_below_the_floor_is_not_re_recorded() {
        let mut b = SeqBits::new();
        for s in 0..200 {
            assert!(b.insert(s));
        }
        b.prune_below(150);
        assert_eq!(b.len(), 50);
        assert!(!b.insert(10), "below the floor");
        assert!(!b.contains(10));
        assert!(!b.insert(150), "still a member");
        assert!(b.insert(5_000));
        b.prune_below(1_000_000);
        assert!(b.is_empty());
        assert!(b.insert(1_000_000) && !b.insert(999_999));
    }

    /// Steady state allocates nothing: the ring recycles its words.
    #[test]
    fn steady_state_reuses_its_capacity() {
        let mut b = SeqBits::new();
        for s in 0..100_000u64 {
            b.insert(s);
            if s % 1000 == 999 {
                b.prune_below(s.saturating_sub(2_000));
            }
        }
        assert!(b.words.len() < 100, "span stays near the live window: {}", b.words.len());
        assert!(b.words.capacity() < 512);
    }
}
