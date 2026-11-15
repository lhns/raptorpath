//! Evidence for a warm-started tail solve (test-only, no non-test change).
//!
//! `compute_r_star_exact` bisects r on `fail(k) > delta` with
//! k = round(mid*W) and fail(k) = 1 - p_fec_exact(p,q,k/W,W). If fail is
//! non-increasing in k at every visited k, the returned `hi` depends only on
//! the crossing k* = min{k : fail(k) <= delta}, so a solver that finds k*
//! (cheaply, from the last tick's k*) and replays the 40 halvings against
//! `round(mid*W) < k*` is bit-identical. These tests (all #[ignore], run with
//! `--release -- --ignored --nocapture`) measure that on the keys the sim
//! really produces and on a widened (p,q,W) grid.

use super::*;
use std::collections::{BTreeMap, HashMap, HashSet};

type Key = [u64; 4];

fn out_dir() -> std::path::PathBuf {
    std::env::var("WARM_SCAN_DIR").map(Into::into).unwrap_or_else(|_| std::env::temp_dir())
}

fn nthreads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).min(4)
}

/// Same formula as `math::repair_count` (private there).
fn rc(r: f64, w: usize) -> usize {
    (r.max(0.0) * w as f64).round() as usize
}

fn fail_at(p: f64, q: f64, w: usize, k: usize) -> f64 {
    1.0 - math::p_fec_exact(p, q, k as f64 / w as f64, w)
}

/// Cold reference replica (counts distinct k probed); must equal the crate's.
fn cold_counting(p: f64, _q: f64, w: usize, delta: f64, eval: &mut impl FnMut(usize) -> f64) -> (f64, usize) {
    if w == 0 || p <= 0.0 { return (0.0, 0); }
    let mut seen: HashSet<usize> = HashSet::new();
    let mut f = |r: f64| { let k = rc(r, w); seen.insert(k); eval(k) };
    let mut hi = 2.0f64;
    if f(hi) > delta { return (hi, seen.len()); }
    let mut lo = 0.0f64;
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if f(mid) > delta { lo = mid } else { hi = mid }
    }
    (hi, seen.len())
}

/// Candidate warm solver. Returns (r, crossing k* if any, distinct probes).
/// Probe-and-walk (gallop + bisect) from `hint_k` to the crossing, then the
/// 40 cold halvings replayed against `round(mid*W) < k*`.
fn warm_r_star(
    p: f64, _q: f64, w: usize, delta: f64, hint_k: Option<usize>,
    eval: &mut impl FnMut(usize) -> f64,
) -> (f64, Option<usize>, usize) {
    if w == 0 || p <= 0.0 { return (0.0, None, 0); }
    let top = rc(2.0, w);
    let mut memo: HashMap<usize, bool> = HashMap::new();
    let mut exceeds = |k: usize| *memo.entry(k).or_insert_with(|| eval(k) > delta);
    let h = hint_k.unwrap_or(top).min(top);
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

// ---------------------------------------------------------------- recording

struct Chan { eps: f64, q: f64, rtt: u32, cap: u32 }
fn c(eps: f64, q: f64, rtt: u32) -> Chan { Chan { eps, q, rtt, cap: 4 } }

fn channel_sets() -> Vec<Vec<Chan>> {
    vec![
        vec![c(0.05, 0.5, 50)],
        vec![c(0.3, 0.02, 50)],
        vec![c(0.3, 1.0, 50)],
        vec![c(0.01, 0.02, 100)],
        vec![c(0.3, 0.5, 20)],
        vec![c(0.3, 0.02, 50), c(0.05, 1.0, 20)],
        vec![c(0.1, 0.1, 80), c(0.3, 1.0, 30)],
        vec![c(0.0, 0.5, 50), c(0.3, 0.02, 50)],
        vec![c(0.3, 0.02, 50), c(0.05, 1.0, 20), c(0.01, 0.5, 100), c(0.2, 0.05, 60)],
        vec![c(0.3, 1.0, 40), c(0.3, 1.0, 40), c(0.3, 0.02, 40), c(0.3, 0.02, 40)],
    ]
}

/// The page's Auto W: derive_window on the capacity-weighted aggregate true
/// channel and the hint's tail target, latency budget 0.
fn auto_w(ch: &[Chan], delta: f64) -> u32 {
    let tot: f64 = ch.iter().map(|x| x.cap as f64).sum();
    let agg = |f: &dyn Fn(&Chan) -> f64| ch.iter().map(|x| x.cap as f64 * f(x)).sum::<f64>() / tot;
    let eps = agg(&|x| x.eps);
    let srtt = agg(&|x| x.rtt as f64 / 1000.0);
    let sigma2 = agg(&|x| {
        let p = x.eps * x.q / (1.0 - x.eps);
        math::burst_variance_factor(p, x.q)
    });
    let tail = math::sim_tail_target_of_delta(delta);
    (math::derive_window(tail, eps, sigma2, srtt, tot / 0.001, 0.0).round() as u32).max(1)
}

/// One run: its ordered key log (consecutive repeats removed = memo hits).
fn run_one(ch: &[Chan], w: u32, delta: f64, rho: f64) -> Vec<Key> {
    let mut s = Simulation::multipath(
        ch.iter().map(|x| x.eps).collect(), ch.iter().map(|x| x.q).collect(),
        ch.iter().map(|x| x.rtt).collect(), ch.iter().map(|x| x.cap).collect(),
        w, "continuum".into(), None, Some(delta), Some(rho),
    );
    while !s.is_finished() && s.get_tick() < 20_000 { s.step(); }
    let mut v: Vec<Key> = Vec::new();
    for k in &s.r_tail_log { if v.last() != Some(k) { v.push(*k); } }
    v
}

fn record_runs() -> Vec<Vec<Key>> {
    let sets = channel_sets();
    let mut jobs: Vec<(usize, u32, f64, f64)> = Vec::new();
    for (i, ch) in sets.iter().enumerate() {
        for &d in &[0.005, 0.5, 50.0] {
            let mut ws: Vec<u32> = vec![10, 25, 64, 200, 400];
            ws.push(auto_w(ch, d));
            let only: Option<u32> = std::env::var("WARM_W").ok().and_then(|s| s.parse().ok());
            for w in ws.into_iter().filter(|w| only.map_or(true, |o| o == *w)) { for &rho in &[1.0, 0.95] { jobs.push((i, w, d, rho)); } }
        }
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|sc| {
        for _ in 0..nthreads() {
            sc.spawn(|| loop {
                let j = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if j >= jobs.len() { break; }
                let (i, w, d, rho) = jobs[j];
                let keys = run_one(&sets[i], w, d, rho);
                results.lock().unwrap().push((j, keys));
            });
        }
    });
    let mut r = results.into_inner().unwrap();
    r.sort_by_key(|x| x.0);
    println!("recorded {} runs", r.len());
    r.into_iter().map(|x| x.1).collect()
}

fn key_parts(k: &Key) -> (f64, f64, usize, f64) {
    (f64::from_bits(k[0]), f64::from_bits(k[1]), k[2] as usize, f64::from_bits(k[3]))
}

fn dump_keys(runs: &[Vec<Key>]) {
    let mut set: HashSet<Key> = HashSet::new();
    for r in runs { for k in r { set.insert(*k); } }
    let mut v: Vec<Key> = set.into_iter().collect();
    v.sort();
    let mut s = String::from("pg,qg,W,tail_target\n");
    for k in &v {
        let (p, q, w, t) = key_parts(k);
        s += &format!("{p:e},{q:e},{w},{t:e}\n");
    }
    let path = out_dir().join("warm_scan_keys.csv");
    std::fs::write(&path, s).unwrap();
    println!("{} distinct keys -> {}", v.len(), path.display());
    let (mut pmin, mut pmax, mut qmin, mut qmax) = (f64::MAX, 0f64, f64::MAX, 0f64);
    let (mut tmin, mut tmax) = (f64::MAX, 0f64);
    for k in &v {
        let (p, q, _, t) = key_parts(k);
        pmin = pmin.min(p); pmax = pmax.max(p); qmin = qmin.min(q); qmax = qmax.max(q);
        tmin = tmin.min(t); tmax = tmax.max(t);
    }
    println!("recorded pg range [{pmin:e}, {pmax:e}], qg range [{qmin:e}, {qmax:e}], tail_target range [{tmin:e}, {tmax:e}]");
}

// ------------------------------------------------------------------ scanning

#[derive(Default)]
struct Scan {
    curves: usize,
    increases: usize,
    curves_with_increase: usize,
    min_fail_at_increase: f64,   // smaller of fail[k-1], fail[k] over all increases
    max_rel: f64,                // max (fail[k]-fail[k-1])/fail[k-1]
    max_ulps: f64,
    by_decade: BTreeMap<i32, usize>, // decade of fail[k] (the higher value)
    le_half: usize,              // increases with fail[k] <= 0.5
    le_tenth: usize,             // ... <= 0.1
    examples: Vec<String>,
}

fn scan_curve(p: f64, q: f64, w: usize, sc: &mut Scan) {
    let f: Vec<f64> = (0..=2 * w).map(|k| fail_at(p, q, w, k)).collect();
    sc.curves += 1;
    let mut any = false;
    for k in 1..f.len() {
        if f[k] > f[k - 1] {
            any = true;
            sc.increases += 1;
            let lo = f[k - 1].min(f[k]);
            if sc.min_fail_at_increase == 0.0 || lo < sc.min_fail_at_increase { sc.min_fail_at_increase = lo; }
            let rel = (f[k] - f[k - 1]) / f[k - 1].max(1e-300);
            sc.max_rel = sc.max_rel.max(rel);
            let ulp = f[k - 1].abs() * f64::EPSILON;
            if ulp > 0.0 { sc.max_ulps = sc.max_ulps.max((f[k] - f[k - 1]) / ulp); }
            let dec = if f[k] > 0.0 { f[k].log10().floor() as i32 } else { -999 };
            *sc.by_decade.entry(dec).or_insert(0) += 1;
            if f[k] <= 0.5 { sc.le_half += 1; }
            if f[k] <= 0.1 { sc.le_tenth += 1; }
            if sc.examples.len() < 6 && f[k] <= 0.5 {
                sc.examples.push(format!("p={p:e} q={q:e} W={w} k={k}: {:e} -> {:e}", f[k - 1], f[k]));
            }
        }
    }
    if any { sc.curves_with_increase += 1; }
}

fn print_scan(name: &str, sc: &Scan) {
    println!("--- scan: {name}");
    println!("curves {} ; curves with any increase {} ; increases {}", sc.curves, sc.curves_with_increase, sc.increases);
    println!("increases with fail<=0.5: {} ; <=0.1: {}", sc.le_half, sc.le_tenth);
    println!("smallest fail value at any increase: {:e}", sc.min_fail_at_increase);
    println!("max relative increase {:e} ; max increase in ULPs of fail[k-1]: {:.1}", sc.max_rel, sc.max_ulps);
    println!("count by decade of the post-increase fail value:");
    for (d, n) in &sc.by_decade { println!("  1e{d}: {n}"); }
    for e in &sc.examples { println!("  e.g. {e}"); }
}

fn merge(a: &mut Scan, b: Scan) {
    a.curves += b.curves;
    a.increases += b.increases;
    a.curves_with_increase += b.curves_with_increase;
    if b.min_fail_at_increase > 0.0 && (a.min_fail_at_increase == 0.0 || b.min_fail_at_increase < a.min_fail_at_increase) {
        a.min_fail_at_increase = b.min_fail_at_increase;
    }
    a.max_rel = a.max_rel.max(b.max_rel);
    a.max_ulps = a.max_ulps.max(b.max_ulps);
    for (d, n) in b.by_decade { *a.by_decade.entry(d).or_insert(0) += n; }
    a.le_half += b.le_half;
    a.le_tenth += b.le_tenth;
    for e in b.examples { if a.examples.len() < 6 { a.examples.push(e); } }
}

fn par_scan(curves: &[(f64, f64, usize)]) -> Scan {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let total = std::sync::Mutex::new(Scan::default());
    std::thread::scope(|sc| {
        for _ in 0..nthreads() {
            sc.spawn(|| {
                let mut local = Scan::default();
                loop {
                    let j = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if j >= curves.len() { break; }
                    let (p, q, w) = curves[j];
                    scan_curve(p, q, w, &mut local);
                }
                merge(&mut total.lock().unwrap(), local);
            });
        }
    });
    total.into_inner().unwrap()
}

const GRID_W: [usize; 8] = [10, 16, 25, 64, 100, 200, 400, 512];
const GRID_P: [f64; 14] = [1e-5, 1e-4, 1e-3, 3e-3, 0.01, 0.03, 0.06, 0.1, 0.2, 0.3, 0.43, 0.7, 0.95, 1.0];
const GRID_Q: [f64; 11] = [1e-4, 1e-3, 0.01, 0.02, 0.05, 0.1, 0.3, 0.5, 0.8, 0.95, 1.0];

fn grid() -> Vec<(f64, f64, usize)> {
    let mut v = Vec::new();
    for &w in &GRID_W { for &p in &GRID_P { for &q in &GRID_Q { v.push((p, q, w)); } } }
    v
}

fn recorded_curves(runs: &[Vec<Key>]) -> Vec<(f64, f64, usize)> {
    let mut set: HashSet<(u64, u64, usize)> = HashSet::new();
    for r in runs { for k in r { set.insert((k[0], k[1], k[2] as usize)); } }
    let mut v: Vec<(u64, u64, usize)> = set.into_iter().collect();
    v.sort();
    // Large windows cost O(W^2 * 2W) per curve: thin them out evenly.
    let mut out = Vec::new();
    for w in [10usize, 25, 64, 200, 400, 0] {
        let sel: Vec<_> = v.iter()
            .filter(|x| if w == 0 { !matches!(x.2, 10 | 25 | 64 | 200 | 400) } else { x.2 == w })
            .collect();
        let cap = match w { 10 | 25 => 1500, 64 => 800, 0 => 400, _ => 120 };
        let step = (sel.len() / cap).max(1);
        for x in sel.iter().step_by(step) { out.push((f64::from_bits(x.0), f64::from_bits(x.1), x.2)); }
    }
    println!("scanning {} of {} distinct recorded (p,q,W) curves", out.len(), v.len());
    out
}

// ------------------------------------------------------------ equality tally

#[derive(Default)]
struct Tally {
    calls: usize,
    mismatches: usize,
    cold_probes: usize,
    cold_n: usize,
}

/// Cold solve once (the crate's, plus the counting replica), then every hint
/// against it; fail(k) evaluations of one (p,q,W) are shared via `cache`.
/// Returns (k* of the first hint's solve, warm distinct probes per hint).
fn check(
    t: &mut Tally, cache: &mut HashMap<usize, f64>, p: f64, q: f64, w: usize, d: f64,
    hints: &[Option<usize>], tag: &str,
) -> (Option<usize>, Vec<usize>) {
    let cold = math::compute_r_star_exact(p, q, w, d);
    let mut ev = |k: usize| *cache.entry(k).or_insert_with(|| fail_at(p, q, w, k));
    let (cref, cp) = cold_counting(p, q, w, d, &mut ev);
    assert_eq!(cref.to_bits(), cold.to_bits(), "cold replica diverged");
    let mut first = None;
    let mut probes = Vec::new();
    for (i, &hint) in hints.iter().enumerate() {
        let (r, ks, wp) = warm_r_star(p, q, w, d, hint, &mut ev);
        if i == 0 { first = ks; }
        probes.push(wp);
        t.calls += 1;
        t.cold_probes += cp;
        t.cold_n += 1;
        if r.to_bits() != cold.to_bits() {
            t.mismatches += 1;
            if t.mismatches <= 20 {
                println!("MISMATCH[{tag}] p={p:e} q={q:e} W={w} d={d:e} hint={hint:?} warm={r:e} cold={cold:e} k*={ks:?}");
            }
        }
    }
    (first, probes)
}


// --------------------------------------------------------------------- tests

#[test]
fn test_warm_helpers_agree_with_crate_rounding() {
    for w in [1usize, 10, 25, 64, 200, 400, 512] {
        for k in 0..=2 * w { assert_eq!(rc(k as f64 / w as f64, w), k, "W={w} k={k}"); }
    }
}

/// Always-on smoke: the candidate is bit-equal to the cold solver.
#[test]
fn test_warm_candidate_smoke() {
    for &(p, q, w, d) in &[(0.05, 0.5, 25usize, 0.01), (0.2, 0.05, 64, 1e-4), (0.4, 0.02, 25, 1e-9), (1e-4, 0.5, 10, 0.1)] {
        let cold = math::compute_r_star_exact(p, q, w, d);
        for hint in [None, Some(0), Some(2 * w), Some(7)] {
            let (r, _, _) = warm_r_star(p, q, w, d, hint, &mut |k| fail_at(p, q, w, k));
            assert_eq!(r.to_bits(), cold.to_bits(), "p={p} q={q} W={w} d={d} hint={hint:?}");
        }
    }
}

#[test]
#[ignore]
fn test_warm_scan_record_and_dump() {
    let runs = record_runs();
    dump_keys(&runs);
}

#[test]
#[ignore]
fn test_warm_scan_monotonicity() {
    let runs = record_runs();
    dump_keys(&runs);
    let rec = recorded_curves(&runs);
    print_scan("recorded sim keys", &par_scan(&rec));
    let g = grid();
    println!("grid curves: {}", g.len());
    print_scan("widened grid (W 10..512, p 1e-5..1, q 1e-4..1)", &par_scan(&g));
}

/// Bit-equality of the warm candidate vs the cold solver on every recorded
/// key (true previous-call hint first, then hostile hints) and on a grid.
/// `WARM_W=<w>` restricts to one window so the run can be split into short
/// invocations; progress is printed as it goes.
#[test]
#[ignore]
fn test_warm_bit_equality_and_probes() {
    let only_w: Option<usize> = std::env::var("WARM_W").ok().and_then(|s| s.parse().ok());
    let runs = record_runs();
    dump_keys(&runs);
    let mut t = Tally::default();
    let (mut true_p, mut true_n) = (0usize, 0usize);
    let (mut none_p, mut none_n) = (0usize, 0usize);
    let (mut bad_p, mut bad_n) = (0usize, 0usize);
    let mut rng: u64 = 0x9E3779B97F4A7C15;
    for (ri, run) in runs.iter().enumerate() {
        let mut prev: Option<usize> = None;
        let mut cache: HashMap<usize, f64> = HashMap::new();
        let mut cache_key = (0u64, 0u64);
        for k in run {
            let (p, q, w, d) = key_parts(k);
            if only_w.map_or(false, |o| o != w) { break; }
            if (k[0], k[1]) != cache_key { cache.clear(); cache_key = (k[0], k[1]); }
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let rand_hint = (rng >> 33) as usize % (2 * w + 1);
            let hints = [prev, None, Some(0), Some(2 * w), Some(rand_hint)];
            let (ks, pr) = check(&mut t, &mut cache, p, q, w, d, &hints, "recorded");
            if prev.is_none() { none_p += pr[0]; none_n += 1; } else { true_p += pr[0]; true_n += 1; }
            for &x in &pr[1..] { bad_p += x; bad_n += 1; }
            prev = ks.or(prev);
        }
        if ri % 20 == 0 { println!("run {ri}/{}: calls {} mismatches {}", runs.len(), t.calls, t.mismatches); }
    }
    println!("recorded-key phase: calls {}, mismatches {}", t.calls, t.mismatches);
    let targets = [1e-9, 1e-6, 1e-4, 1e-3, 0.005, 0.01, 0.03, 0.05, 0.1, 0.3];
    let mut cells = 0usize;
    for &w in &[10usize, 25, 64, 200, 400] {
        if only_w.map_or(false, |o| o != w) { continue; }
        for &p in &GRID_P { for &q in &GRID_Q {
            let mut cache: HashMap<usize, f64> = HashMap::new();
            for &d in &targets {
                let (ks, pr) = check(&mut t, &mut cache, p, q, w, d, &[None], "grid-nohint");
                none_p += pr[0]; none_n += 1;
                let hints = [Some(0), Some(2 * w), ks, ks.map(|k| k + 1), ks.map(|k| k.saturating_sub(1)), Some(w)];
                check(&mut t, &mut cache, p, q, w, d, &hints, "grid-hint");
                cells += 1;
            }
        } }
        println!("grid W={w} done: calls {} mismatches {}", t.calls, t.mismatches);
    }
    println!("grid: {cells} (p,q,W,delta) cells x 7 hint variants");
    println!("TOTAL calls {} ; MISMATCHES {}", t.calls, t.mismatches);
    let m = |a: usize, b: usize| if b == 0 { 0.0 } else { a as f64 / b as f64 };
    println!("mean distinct probes/call: warm(true prev hint) {:.2} over {true_n}; warm(no hint, incl. first-in-run) {:.2} over {none_n}; warm(hostile hints) {:.2} over {bad_n}",
        m(true_p, true_n), m(none_p, none_n), m(bad_p, bad_n));
    println!("cold mean distinct probes/call: {:.2}", m(t.cold_probes, t.cold_n));
    assert_eq!(t.mismatches, 0);
}

