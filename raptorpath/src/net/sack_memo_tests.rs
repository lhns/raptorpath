//! `SackMemo` (incremental SACK marking, status §26 finding 1): differential
//! against the old full-rescan marker, and the absolute cost bound.

use super::sack::*;
use super::seq_ring::RetainedSeqs;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

/// A store that counts the seqs `for_each_seq_in` hands out.
struct Counting {
    m: BTreeMap<u64, ()>,
    visits: Cell<u64>,
}

impl RetainedSeqs for Counting {
    fn retained_len(&self) -> usize {
        self.m.len()
    }
    fn last_seq(&self) -> Option<u64> {
        self.m.last_seq()
    }
    fn retained_below(&self, f: u64) -> usize {
        self.m.retained_below(f)
    }
    fn for_each_seq_in(&self, s: u64, e: u64, mut f: impl FnMut(u64)) {
        self.m.for_each_seq_in(s, e, |q| {
            self.visits.set(self.visits.get() + 1);
            f(q)
        });
    }
}

fn store(range: std::ops::Range<u64>) -> Counting {
    Counting { m: range.map(|k| (k, ())).collect(), visits: Cell::new(0) }
}

/// The old marker (reference, on the uncounted map) beside the memo.
struct Pair {
    memo: SackMemo,
    r_new: BTreeSet<u64>,
    r_ref: BTreeSet<u64>,
    tot_new: u64,
    tot_ref: u64,
    visited: u64,
}

impl Pair {
    fn new() -> Self {
        Pair { memo: SackMemo::new(), r_new: BTreeSet::new(), r_ref: BTreeSet::new(), tot_new: 0, tot_ref: 0, visited: 0 }
    }
    fn range(&mut self, st: &Counting, a: u64, b: u64) {
        if b < a {
            return; // the drain's `if end < start { continue; }`
        }
        let (mut n1, mut n2) = (Vec::new(), Vec::new());
        sack_release_mark_into(&st.m, &mut self.r_ref, a, b, &mut n2);
        self.memo.mark_into(st, &mut self.r_new, a, b, &mut n1, &mut self.visited);
        assert_eq!(n1, n2, "newly for ({a},{b})");
        self.tot_new += n1.len() as u64;
        self.tot_ref += n2.len() as u64;
        assert_eq!(self.r_new, self.r_ref, "released after ({a},{b})");
        assert_eq!(self.tot_new, self.tot_ref);
    }
    fn prune(&mut self, nx: u64) {
        sack_release_prune(&mut self.r_new, nx);
        sack_release_prune(&mut self.r_ref, nx);
        self.memo.prune(nx);
    }
}

#[test]
fn differential_scripted() {
    let mut st = store(0..100);
    let mut p = Pair::new();
    p.range(&st, 10, 20);
    p.range(&st, 10, 20); // re-sent
    p.range(&st, 10, 30); // growing
    p.range(&st, 40, 50);
    p.range(&st, 15, 45); // bridges two intervals
    p.range(&st, 21, 39); // inside
    p.range(&st, 51, 60); // adjacent
    p.range(&st, 200, 300); // above everything
    for k in 100..150 {
        st.m.insert(k, ());
    } // seqs inserted between reports
    p.range(&st, 90, 120);
    p.range(&st, 90, 149);
    p.prune(35);
    st.m = st.m.split_off(&35);
    p.range(&st, 10, 60); // straddles the frontier
    p.range(&st, 0, 10_000);
    p.prune(80);
    p.range(&st, 0, 10_000);
}

/// The MAX_SACK_RANGES prefix: only the first ranges of a report reach the
/// sender, so later reports extend the prefix; the memo must follow.
#[test]
fn differential_prefix_truncated_reports() {
    let st = store(0..2000);
    let mut p = Pair::new();
    let all: Vec<(u64, u64)> = (0..300).map(|i| (10 + i * 5, 12 + i * 5)).collect();
    for n in [64usize, 64, 100, 300] {
        for &(a, b) in &all[..n] {
            p.range(&st, a, b);
        }
    }
}

#[test]
fn differential_randomized() {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut rnd = move |n: u64| {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x % n
    };
    let mut st = store(0..50);
    let mut p = Pair::new();
    let (mut top, mut nx) = (50u64, 0u64);
    for _ in 0..4000 {
        match rnd(10) {
            0..=1 => {
                for _ in 0..rnd(20) {
                    st.m.insert(top, ());
                    top += 1;
                }
            }
            2 => {
                nx += rnd(30);
                p.prune(nx);
                // The store prunes at ack+1, which may lag next_expected.
                let cut = nx.saturating_sub(rnd(3));
                st.m = st.m.split_off(&cut);
            }
            3 => {
                // an individual hole nothing re-fills
                let k = nx + rnd(top.saturating_sub(nx) + 1);
                st.m.remove(&k);
            }
            _ => {
                // a report: several ranges, some re-sent, some growing
                for _ in 0..rnd(5) + 1 {
                    let a = nx + rnd(top.saturating_sub(nx) + 5);
                    let b = a + rnd(40);
                    p.range(&st, a, if rnd(8) == 0 { a.saturating_sub(1) } else { b });
                }
            }
        }
    }
    assert!(p.memo.intervals() < 200);
}

#[test]
fn resent_report_visits_nothing_and_growth_visits_only_the_growth() {
    let st = store(0..1000);
    let mut p = Pair::new();
    let ranges = [(100u64, 300u64), (310, 500), (600, 700)];
    for &(a, b) in &ranges {
        p.range(&st, a, b);
    }
    let first = p.visited;
    assert_eq!(first, 201 + 191 + 101);
    for _ in 0..50 {
        for &(a, b) in &ranges {
            p.range(&st, a, b);
        }
    }
    assert_eq!(p.visited, first, "re-sending the same report visits zero seqs");
    assert_eq!(st.visits.get(), first, "the store itself was asked for no more");
    // A report adding k new seqs visits k, not the span.
    let before = p.visited;
    for &(a, b) in &ranges {
        p.range(&st, a, b);
    }
    p.range(&st, 301, 309); // fills the 9-seq gap between two intervals
    p.range(&st, 501, 505);
    assert_eq!(p.visited - before, 9 + 5);
}

/// Wiring pin (rule 1): the sender's drain goes through the memo and the
/// cumulative prune twin prunes it; the full-rescan marker is not called from
/// the engine (it stays as the test reference).
#[test]
fn the_sender_routes_through_the_memo() {
    let m: String = include_str!("mod.rs").split_whitespace().collect();
    let ph: String = include_str!("sender_phases.rs").split_whitespace().collect();
    assert_eq!(m.matches("sack_memo.mark_into(&st.sent_store,&mutsack_released,").count(), 1, "drain calls the memo");
    assert!(!m.contains("sack_release_mark_into("), "the full-rescan marker is test-reference only");
    assert!(ph.contains("sack_memo.prune(next_expected)"), "prune twin");
}
