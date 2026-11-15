//! Warm-started tail solve: bit-identical to `math::compute_r_star_exact`.
//!
//! The cold solver bisects r on `fail(k) > delta` with k = round(mid*W) and
//! fail(k) = 1 - p_fec_exact(p,q,k/W,W). Where fail is non-increasing in k at
//! every visited k, the returned `hi` depends only on the crossing
//! k* = min{k : fail(k) <= delta}, so k* is found by a gallop+bisect walk from
//! the previous tick's k* and the 40 cold halvings are replayed against
//! `round(mid*W) < k*`. Evidence: `warm_scan.rs` (monotonicity scan, bit
//! equality on recorded sim keys and a grid, hostile hints).

use crate::math;
use std::collections::HashMap;

/// Same formula as `math::repair_count` (private there).
pub(crate) fn rc(r: f64, w: usize) -> usize {
    (r.max(0.0) * w as f64).round() as usize
}

pub(crate) fn fail_at(p: f64, q: f64, w: usize, k: usize) -> f64 {
    1.0 - math::p_fec_exact(p, q, k as f64 / w as f64, w)
}

/// Candidate warm solver. Returns (r, crossing k* if any, distinct probes).
/// Probe-and-walk (gallop + bisect) from `hint_k` to the crossing, then the
/// 40 cold halvings replayed against `round(mid*W) < k*`.
pub(crate) fn warm_r_star(
    p: f64, _q: f64, w: usize, delta: f64, hint_k: Option<usize>,
    eval: &mut impl FnMut(usize) -> f64,
) -> (f64, Option<usize>, usize) {
    if w == 0 || p <= 0.0 { return (0.0, None, 0); }
    let top = rc(2.0, w);
    let mut memo: HashMap<usize, bool> = HashMap::new();
    let mut exceeds = |k: usize| *memo.entry(k).or_insert_with(|| eval(k) > delta);
    // No hint: start at k = 0 and gallop UP through the cheap DP sizes
    // (cost grows with W + k); the crossing, hence the result, is the same.
    let h = hint_k.unwrap_or(0).min(top);
    let kstar: usize;
    if exceeds(h) {
        if h == top { let n = memo.len(); return (2.0, None, n); }
        let (mut lo, mut step) = (h, 1usize);
        let mut hi;
        loop {
            let n = (lo + step).min(top);
            if !exceeds(n) { hi = n; break; }
            if n == top { let m = memo.len(); return (2.0, None, m); }
            lo = n;
            step *= 2;
        }
        while hi - lo > 1 {
            let m = (lo + hi) / 2;
            if exceeds(m) { lo = m } else { hi = m }
        }
        kstar = hi;
    } else if h == 0 {
        kstar = 0;
    } else {
        let (mut hi, mut step) = (h, 1usize);
        let mut lo: Option<usize> = None;
        loop {
            let n = hi.saturating_sub(step);
            if exceeds(n) { lo = Some(n); break; }
            hi = n;
            if n == 0 { break; }
            step *= 2;
        }
        match lo {
            None => kstar = 0,
            Some(mut lo) => {
                while hi - lo > 1 {
                    let m = (lo + hi) / 2;
                    if exceeds(m) { lo = m } else { hi = m }
                }
                kstar = hi;
            }
        }
    }
    let probes = memo.len();
    let (mut lo, mut hi) = (0.0f64, 2.0f64);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if rc(mid, w) < kstar { lo = mid } else { hi = mid }
    }
    (hi, Some(kstar), probes)
}

/// Warm tail solve with the production evaluator. Returns
/// (r, crossing k* if any, distinct DP evaluations). In debug/test builds the
/// result is checked against the cold solver to the bit.
pub(crate) fn warm_tail_solve(
    p: f64, q: f64, w: usize, delta: f64, hint_k: Option<usize>,
) -> (f64, Option<usize>, usize) {
    let (r, ks, probes) = warm_r_star(p, q, w, delta, hint_k, &mut |k| fail_at(p, q, w, k));
    #[cfg(test)]
    counters::record(r, ks, hint_k, w, p);
    #[cfg(any(test, debug_assertions))]
    {
        let cold = math::compute_r_star_exact(p, q, w, delta);
        assert_eq!(
            r.to_bits(), cold.to_bits(),
            "warm tail solve diverged from cold: p={p:e} q={q:e} W={w} delta={delta:e} hint={hint_k:?} warm={r:e} cold={cold:e}"
        );
    }
    (r, ks, probes)
}

/// Branch tallies for the evidence tests (calls that took each branch).
#[cfg(test)]
pub(crate) mod counters {
    use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
    pub static CALLS: AtomicUsize = AtomicUsize::new(0);
    pub static CEILING: AtomicUsize = AtomicUsize::new(0);
    pub static K0: AtomicUsize = AtomicUsize::new(0);
    pub static NOHINT: AtomicUsize = AtomicUsize::new(0);
    pub fn record(r: f64, ks: Option<usize>, hint: Option<usize>, w: usize, p: f64) {
        if w == 0 || p <= 0.0 { return; }
        CALLS.fetch_add(1, Relaxed);
        if ks.is_none() && r == 2.0 { CEILING.fetch_add(1, Relaxed); }
        if ks == Some(0) { K0.fetch_add(1, Relaxed); }
        if hint.is_none() { NOHINT.fetch_add(1, Relaxed); }
    }
    pub fn snapshot() -> [usize; 4] {
        [CALLS.load(Relaxed), CEILING.load(Relaxed), K0.load(Relaxed), NOHINT.load(Relaxed)]
    }
}
