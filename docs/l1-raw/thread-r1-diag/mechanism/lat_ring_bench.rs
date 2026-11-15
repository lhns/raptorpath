#![allow(dead_code)]
// §20 mechanism arm. SeqRing: a5ef8b2 raptorpath/src/net/seq_ring.rs verbatim (the RetainedSeqs trait omitted).
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


// ---- §20 mechanism arm: the [LAT] pending store, three implementations ----
// MAIN  = ce60079 HashMap + FIFO; R1 = a5ef8b2 SeqRing + FIFO + span guard
// (verbatim); FIX = R1 + a delivered floor (records below the last in-order
// delivery are dead: pruned on delivery, refused on arrival, counted lost).
// The SeqRing above is a5ef8b2 net/seq_ring.rs verbatim.
use std::collections::HashMap;
use std::time::Instant;

const PENDING_MAX: usize = 16_384;
const PENDING_SPAN_MAX: u64 = 4 * PENDING_MAX as u64;

trait Store {
    fn insert(&mut self, seq: u64, v: (u32, u64)) -> u64;
    fn take(&mut self, seq: u64) -> (Option<(u32, u64)>, u64);
}

#[derive(Default)]
struct MainStore { map: HashMap<u64, (u32, u64)>, order: VecDeque<u64> }
impl Store for MainStore {
    fn insert(&mut self, seq: u64, v: (u32, u64)) -> u64 {
        let mut lost = 0;
        if self.map.insert(seq, v).is_none() { self.order.push_back(seq); }
        while self.order.len() > PENDING_MAX {
            if let Some(old) = self.order.pop_front() {
                if self.map.remove(&old).is_some() { lost += 1; }
            }
        }
        lost
    }
    fn take(&mut self, seq: u64) -> (Option<(u32, u64)>, u64) { (self.map.remove(&seq), 0) }
}

#[derive(Default)]
struct R1Store { ring: SeqRing<(u32, u64)>, order: VecDeque<u64> }
impl Store for R1Store {
    fn insert(&mut self, seq: u64, v: (u32, u64)) -> u64 {
        let mut lost = 0;
        if let (Some((lo, _)), Some(hi)) = (self.ring.first(), self.ring.last_key()) {
            if seq < lo {
                if hi - seq >= PENDING_SPAN_MAX { return 1; }
            } else {
                let mut lo = lo;
                while seq.saturating_sub(lo) >= PENDING_SPAN_MAX {
                    self.ring.remove(&lo);
                    lost += 1;
                    match self.ring.first() { Some((l, _)) => lo = l, None => break }
                }
            }
        }
        if self.ring.insert(seq, v).is_none() { self.order.push_back(seq); }
        while self.order.len() > PENDING_MAX {
            if let Some(old) = self.order.pop_front() {
                if self.ring.remove(&old).is_some() { lost += 1; }
            }
        }
        lost
    }
    fn take(&mut self, seq: u64) -> (Option<(u32, u64)>, u64) { (self.ring.remove(&seq), 0) }
}

#[derive(Default)]
struct FixStore { r1: R1Store, floor: u64 }
impl Store for FixStore {
    fn insert(&mut self, seq: u64, v: (u32, u64)) -> u64 {
        if seq < self.floor { return 1; } // already delivered: a dead record
        self.r1.insert(seq, v)
    }
    fn take(&mut self, seq: u64) -> (Option<(u32, u64)>, u64) {
        let got = self.r1.ring.remove(&seq);
        let mut lost = 0;
        if seq + 1 > self.floor {
            self.floor = seq + 1;
            let before = self.r1.ring.len();
            self.r1.ring.prune_below(self.floor);
            lost = (before - self.r1.ring.len()) as u64;
        }
        (got, lost)
    }
}

/// In-order single-path stream: every arrival is delivered at once (the c1s
/// `nowait` case). Every `stray_every` arrivals, one duplicate of a seq
/// delivered `stray_back` seqs ago arrives (dup_src; never delivered again).
fn run<S: Store + Default>(n: u64, stray_every: u64, stray_back: u64) -> (f64, u64, u64) {
    let mut s = S::default();
    let (mut lost, mut miss) = (0u64, 0u64);
    let t = Instant::now();
    for seq in 0..n {
        lost += s.insert(seq, (0, seq));
        if stray_every > 0 && seq % stray_every == stray_every - 1 && seq >= stray_back {
            lost += s.insert(seq - stray_back, (0, 0));
        }
        let (got, l) = s.take(seq);
        lost += l;
        if got.is_none() { miss += 1; }
    }
    (t.elapsed().as_nanos() as f64 / n as f64, lost, miss)
}

/// The proposed red test's absolute check, on the extraction: an in-order
/// stream delivered at once, one dead record (a duplicate of a seq delivered
/// 1 000 ago) every 5 600 arrivals. After each delivery the ring may hold
/// nothing below the delivered seq, so its slot span stays O(1). Returns the
/// maximum slot span seen just after an arrival is inserted.
fn max_span_after_insert(fix: bool, n: u64) -> u64 {
    let mut r = R1Store::default();
    let mut f = FixStore::default();
    let mut worst = 0u64;
    for seq in 0..n {
        if seq % 5_600 == 5_599 && seq >= 1_000 {
            if fix { f.insert(seq - 1_000, (0, 0)); } else { r.insert(seq - 1_000, (0, 0)); }
        }
        let ring = if fix { f.insert(seq, (0, seq)); &f.r1.ring } else { r.insert(seq, (0, seq)); &r.ring };
        if let (Some((lo, _)), Some(hi)) = (ring.first(), ring.last_key()) {
            worst = worst.max(hi - lo + 1);
        }
        if fix { f.take(seq); } else { r.take(seq); }
    }
    worst
}

fn main() {
    let n = 300_000u64;
    println!("red-test check (N = 50 000): max ring slot span after an in-order insert: R1 = {}, FIX = {} (bound: 2)",
        max_span_after_insert(false, 50_000), max_span_after_insert(true, 50_000));
    println!("impl  stray_every stray_back  ns_per_arrival  lost  miss");
    for &(every, back) in &[(0u64, 0u64), (5_600, 1_000), (5_600, 8_000), (50_000, 1_000)] {
        for rep in 0..2 {
            let (a, la, ma) = run::<MainStore>(n, every, back);
            let (b, lb, mb) = run::<R1Store>(n, every, back);
            let (c, lc, mc) = run::<FixStore>(n, every, back);
            println!("MAIN  {every:>6} {back:>6}  {a:>10.1}  {la:>5} {ma}  (rep {rep})");
            println!("R1    {every:>6} {back:>6}  {b:>10.1}  {lb:>5} {mb}");
            println!("FIX   {every:>6} {back:>6}  {c:>10.1}  {lc:>5} {mc}");
        }
    }
}
