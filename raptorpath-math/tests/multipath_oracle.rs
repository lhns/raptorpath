//! FORMULA- AND WASM-SIM-INDEPENDENT Monte-Carlo oracle for heterogeneous
//! multipath aggregation (paper Section 16, RWM at L1; goal Phase 2/3).
//!
//! This is GROUND TRUTH: it does NOT call compute_r_star, controller_rate,
//! p_fec_*, or the wasm `Simulation`. It models the real per-symbol process
//! from scratch:
//!   - N paths, each with capacity (symbols/ms), one-way delay, GE(p,q) loss.
//!   - a striped placement (work-conserving pull == the §16.3 marginal-cost
//!     fixed point under backlog; plus an explicit goodput-proportional
//!     variant),
//!   - fungible repairs over a sliding coding HORIZON h (eviction: a hole
//!     that ages > h out of the window can no longer be filled by a repair
//!     and must be recovered by targeted cross-path ARQ),
//!   - cross-path ARQ (a lost symbol is retransmitted on the best path after
//!     its detection RTT, with geometric retries),
//!   - an in-order FRONTIER decode (object completes when ALL K source
//!     symbols are resolved anywhere, out of order).
//!
//! Metric: AGGREGATION FACTOR = fast-path-alone completion / dual completion
//! (equivalently throughput_dual / throughput_fast). > 1 == aggregation.

use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use std::collections::{BTreeMap, BinaryHeap};
use std::cmp::Reverse;

#[derive(Clone, Copy)]
struct Path {
    rate: f64, // symbols per ms (capacity)
    owd: u64,  // one-way delay in ms
    p_gb: f64, // GE Good->Bad
    q_bg: f64, // GE Bad->Good
}
impl Path {
    fn eps(&self) -> f64 { self.p_gb / (self.p_gb + self.q_bg) }
    fn goodput(&self) -> f64 { self.rate * (1.0 - self.eps()) }
    fn rtt(&self) -> u64 { 2 * self.owd }
}

/// GE loss state: `draw` returns true == symbol LOST (netem gemodel: drop iff
/// in the Bad state).
struct Ge { bad: bool }
impl Ge {
    fn draw(&mut self, p: &Path, rng: &mut impl Rng) -> bool {
        self.bad = if self.bad {
            rng.gen::<f64>() >= p.q_bg // stay Bad unless recover (prob q)
        } else {
            rng.gen::<f64>() < p.p_gb // go Bad with prob p
        };
        self.bad
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Place {
    WorkConserving, // pull: any path with a free slot takes the next source
    GoodputProp,    // work-conserving WRR weighted by goodput
    PushStatic,     // NON-work-conserving: each source seq is statically bound
                    // to a path (capacity-weighted); a path may send ONLY its
                    // own queue — a slow path that falls behind (loss) becomes
                    // the long pole and the fast path CANNOT steal its backlog.
}

enum Ev { Source(usize), Repair(usize), Arq(usize) }

struct Oracle {
    paths: Vec<Path>,
    order: Vec<usize>, // path indices sorted by goodput desc (best first)
    ge: Vec<Ge>,
    k: usize,
    r: f64,
    h: usize,       // coding horizon (fungible repair window); usize::MAX = whole object
    atomic: bool,   // true => source path-affine, repairs give NO fungibility
    arq_cross: bool,// true => retransmit on best path; false => same (losing) path
    place: Place,
    // dynamic state
    credit: Vec<f64>,
    stripe_credit: Vec<f64>, // for goodput-proportional WRR
    injected: usize,
    repair_debt: f64,
    frontier: usize,
    src_arrived: Vec<bool>,
    src_send: Vec<(usize, u64)>, // (path, send_tick) of the last transmission
    arq_pending: Vec<bool>,
    arrivals: BTreeMap<u64, Vec<Ev>>,
    repair_pool: BTreeMap<usize, u32>, // gen_pos -> unconsumed arrived repairs
    arq_ready: BinaryHeap<Reverse<(u64, usize)>>,
    // PushStatic: per-path static source assignment (seq bound to a path).
    push_owner: Vec<usize>,   // seq -> owning path (capacity-weighted)
    push_next: Vec<usize>,    // per-path next-unsent index into its owned seqs
    push_queue: Vec<Vec<usize>>, // per-path owned seqs, in order
    rng: ChaCha8Rng,
}

impl Oracle {
    fn new(paths: Vec<Path>, k: usize, r: f64, h: usize, atomic: bool, place: Place, seed: u64) -> Self {
        Self::new_cfg(paths, k, r, h, atomic, true, place, seed)
    }
    #[allow(clippy::too_many_arguments)]
    fn new_cfg(paths: Vec<Path>, k: usize, r: f64, h: usize, atomic: bool, arq_cross: bool, place: Place, seed: u64) -> Self {
        let n = paths.len();
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &b| paths[b].goodput().partial_cmp(&paths[a].goodput()).unwrap());
        // Static capacity-weighted assignment for PushStatic (WRR by capacity).
        let mut push_owner = vec![0usize; k];
        let mut push_queue = vec![Vec::new(); n];
        if place == Place::PushStatic {
            let cap: Vec<f64> = paths.iter().map(|p| p.rate).collect();
            let sum: f64 = cap.iter().sum();
            let mut cred = vec![0.0f64; n];
            for seq in 0..k {
                let mut best = 0usize; let mut bc = f64::NEG_INFINITY;
                for i in 0..n { cred[i] += cap[i] / sum; if cred[i] > bc { bc = cred[i]; best = i; } }
                cred[best] -= 1.0;
                push_owner[seq] = best;
                push_queue[best].push(seq);
            }
        }
        Oracle {
            ge: (0..n).map(|_| Ge { bad: false }).collect(),
            credit: vec![0.0; n],
            stripe_credit: vec![0.0; n],
            paths, order, k, r, h, atomic, arq_cross, place,
            injected: 0, repair_debt: 0.0, frontier: 0,
            src_arrived: vec![false; k],
            src_send: vec![(0, 0); k],
            arq_pending: vec![false; k],
            arrivals: BTreeMap::new(),
            repair_pool: BTreeMap::new(),
            arq_ready: BinaryHeap::new(),
            push_owner,
            push_next: vec![0; n],
            push_queue,
            rng: ChaCha8Rng::seed_from_u64(seed),
        }
    }

    fn schedule(&mut self, tick: u64, ev: Ev) {
        self.arrivals.entry(tick).or_default().push(ev);
    }

    /// Consume one fungible repair whose window covers `seq`, if any.
    fn take_covering_repair(&mut self, seq: usize) -> bool {
        // gen_pos must lie in (seq, seq + h]  (repair at gen_pos covers
        // [gen_pos - h, gen_pos)).
        let hi = seq.saturating_add(self.h).min(usize::MAX);
        let key = self
            .repair_pool
            .range((std::ops::Bound::Excluded(seq), std::ops::Bound::Included(hi)))
            .next()
            .map(|(&k, _)| k);
        if let Some(k) = key {
            let c = self.repair_pool.get_mut(&k).unwrap();
            *c -= 1;
            if *c == 0 { self.repair_pool.remove(&k); }
            true
        } else {
            false
        }
    }

    /// Advance the in-order frontier as far as resolved symbols allow, arming
    /// ARQ for any real hole it stalls on. Returns true when the object is done.
    fn advance_frontier(&mut self, tick: u64) -> bool {
        loop {
            if self.frontier >= self.k { return true; }
            let f = self.frontier;
            if self.src_arrived[f] {
                self.frontier += 1;
                continue;
            }
            // Not arrived. If it has not even been SENT yet, or is still
            // in-flight within its RTT, the frontier just waits (the sender
            // keeps working ahead up to the window). A symbol is a CONFIRMED
            // hole only once overdue by its transmit path's RTT (detection).
            if f >= self.injected { return false; }
            let (op, st) = self.src_send[f];
            let overdue = tick >= st + self.paths[op].rtt();
            if !overdue { return false; }
            // Confirmed hole: race a pooled fungible repair (instant fill,
            // §16.2) against a targeted cross-path ARQ (one more RTT).
            if !self.atomic && self.take_covering_repair(f) {
                self.frontier += 1;
                continue;
            }
            if !self.arq_pending[f] {
                self.arq_pending[f] = true;
                self.arq_ready.push(Reverse((tick, f)));
            }
            return false;
        }
    }

    fn best_path_now(&self, free: &[u32]) -> Option<usize> {
        self.order.iter().copied().find(|&i| free[i] > 0)
    }

    /// Pick the path a SOURCE symbol is striped to (goodput-proportional WRR).
    fn stripe_source(&mut self, free: &[u32]) -> Option<usize> {
        if self.place == Place::WorkConserving {
            // handled by the per-path pull loop; caller passes the path in.
            return None;
        }
        let g: Vec<f64> = (0..self.paths.len())
            .map(|i| if free[i] > 0 { self.paths[i].goodput() } else { 0.0 })
            .collect();
        let sum: f64 = g.iter().sum();
        if sum <= 0.0 { return None; }
        let mut best = None;
        let mut best_c = f64::NEG_INFINITY;
        for i in 0..self.paths.len() {
            if free[i] == 0 { continue; }
            self.stripe_credit[i] += g[i] / sum;
            if self.stripe_credit[i] > best_c { best_c = self.stripe_credit[i]; best = Some(i); }
        }
        if let Some(b) = best { self.stripe_credit[b] -= 1.0; }
        best
    }

    /// Run to completion; return completion time in ms (or None if it never
    /// finishes within the tick cap).
    fn run(&mut self) -> Option<u64> {
        let max_tick: u64 = 5_000_000;
        let mut tick: u64 = 0;
        loop {
            // 1) Deliver arrivals due at this tick.
            if let Some(evs) = self.arrivals.remove(&tick) {
                for ev in evs {
                    match ev {
                        Ev::Source(seq) => self.src_arrived[seq] = true,
                        Ev::Arq(seq) => { self.src_arrived[seq] = true; self.arq_pending[seq] = false; }
                        Ev::Repair(gen) => {
                            if !self.atomic { *self.repair_pool.entry(gen).or_insert(0) += 1; }
                        }
                    }
                }
            }
            // 2) Advance frontier / decode.
            if self.advance_frontier(tick) { return Some(tick); }

            // 3) Per-path sending this tick (best path first so ARQ/repairs
            //    ride the fastest available path — §13.8 preference).
            let mut free: Vec<u32> = vec![0; self.paths.len()];
            for i in 0..self.paths.len() {
                self.credit[i] += self.paths[i].rate;
                free[i] = self.credit[i].floor() as u32;
                self.credit[i] -= free[i] as f64;
            }
            let total: u32 = free.iter().sum();
            for _ in 0..total {
                // drop stale ARQs (already resolved) without wasting a slot
                while let Some(&Reverse((ready, seq))) = self.arq_ready.peek() {
                    if ready <= tick && (seq < self.frontier || self.src_arrived[seq]) {
                        self.arq_ready.pop();
                        self.arq_pending[seq] = false;
                    } else { break; }
                }
                // choose a path with a free slot, best first
                let path = match self.best_path_now(&free) { Some(p) => p, None => break };

                // (a) ARQ retransmit that is due. Cross-path -> best available
                //     path (§13.8); same-path -> the losing path (models a
                //     transport that recovers a symbol only where it was lost).
                if let Some(&Reverse((ready, seq))) = self.arq_ready.peek() {
                    if ready <= tick {
                        let dest = if self.arq_cross { path } else { self.src_send[seq].0 };
                        if free[dest] > 0 {
                            self.arq_ready.pop();
                            free[dest] -= 1;
                            let lost = self.ge[dest].draw(&self.paths[dest], &mut self.rng);
                            if lost {
                                self.arq_ready.push(Reverse((tick + self.paths[dest].rtt(), seq)));
                            } else {
                                self.src_send[seq] = (dest, tick);
                                self.schedule(tick + self.paths[dest].owd, Ev::Arq(seq));
                            }
                            continue;
                        }
                        // designated path busy this tick: fall through, let
                        // `path` do other useful work; ARQ stays queued.
                    }
                }

                // (b) repair when the taper debt says one is due (fungible).
                if self.repair_debt >= 1.0 {
                    self.repair_debt -= 1.0;
                    free[path] -= 1;
                    let gen = self.injected; // covers [injected-h, injected)
                    let lost = self.ge[path].draw(&self.paths[path], &mut self.rng);
                    if !lost { self.schedule(tick + self.paths[path].owd, Ev::Repair(gen)); }
                    continue;
                }

                // (c) new source within the window [frontier, frontier+h).
                let window_ok = self.injected < self.frontier.saturating_add(self.h);
                if self.place == Place::PushStatic {
                    // `path` may ONLY send the front of its OWN static queue,
                    // and only if that seq is inside the window. If its queue
                    // is empty/blocked, it does NOT steal — falls through to
                    // spare (the long-pole pathology).
                    let owned = self.push_next[path] < self.push_queue[path].len();
                    if owned {
                        let seq = self.push_queue[path][self.push_next[path]];
                        if seq < self.frontier.saturating_add(self.h) {
                            self.push_next[path] += 1;
                            self.injected += 1;
                            self.repair_debt += self.r;
                            free[path] -= 1;
                            let lost = self.ge[path].draw(&self.paths[path], &mut self.rng);
                            self.src_send[seq] = (path, tick);
                            if !lost { self.schedule(tick + self.paths[path].owd, Ev::Source(seq)); }
                            continue;
                        }
                    }
                } else if self.injected < self.k && window_ok {
                    // choose carrying path per placement law
                    let dest = if self.place == Place::WorkConserving {
                        path
                    } else {
                        match self.stripe_source(&free) { Some(d) => d, None => path }
                    };
                    let seq = self.injected;
                    self.injected += 1;
                    self.repair_debt += self.r;
                    free[dest] -= 1;
                    let lost = self.ge[dest].draw(&self.paths[dest], &mut self.rng);
                    self.src_send[seq] = (dest, tick);
                    if !lost { self.schedule(tick + self.paths[dest].owd, Ev::Source(seq)); }
                    continue;
                }

                // (d) spare capacity: emit an extra fungible repair to race the
                //     frontier (does nothing in atomic mode -> wasted slot ==
                //     the stall pathology).
                if !self.atomic && self.injected > 0 {
                    free[path] -= 1;
                    let gen = self.injected;
                    let lost = self.ge[path].draw(&self.paths[path], &mut self.rng);
                    if !lost { self.schedule(tick + self.paths[path].owd, Ev::Repair(gen)); }
                    continue;
                }
                // nothing useful to send on this slot
                free[path] -= 1;
            }

            tick += 1;
            if tick > max_tick { return None; }
        }
    }
}

// ---- C8 topology (c2 + c3, exact netem params from tools/l1/lib.sh) ----
// symbol = 1500 B = 12 kbit. rate[mbit/s]/12 = symbols/ms.
fn sym_per_ms(mbit: f64) -> f64 { mbit * 1000.0 / 12.0 / 1000.0 } // = mbit/12
fn c8_fast() -> Path { Path { rate: sym_per_ms(100.0), owd: 5, p_gb: 0.013, q_bg: 0.50 } }
fn c8_slow() -> Path { Path { rate: sym_per_ms(20.0), owd: 20, p_gb: 0.02, q_bg: 0.40 } }

fn factor(paths_dual: &[Path], k: usize, r: f64, h: usize, atomic: bool, place: Place) -> (u64, u64, f64) {
    factor_cfg(paths_dual, k, r, h, atomic, true, place)
}
fn factor_cfg(paths_dual: &[Path], k: usize, r: f64, h: usize, atomic: bool, arq_cross: bool, place: Place) -> (u64, u64, f64) {
    // fast path alone == the single best-goodput path
    let best = *paths_dual.iter().max_by(|a, b| a.goodput().partial_cmp(&b.goodput()).unwrap()).unwrap();
    let mut fa = Oracle::new_cfg(vec![best], k, r, h, atomic, arq_cross, place, 0xF00D);
    let mut du = Oracle::new_cfg(paths_dual.to_vec(), k, r, h, atomic, arq_cross, place, 0xF00D);
    let t_fast = fa.run().expect("fast-alone must finish");
    let t_dual = du.run().expect("dual must finish");
    (t_fast, t_dual, t_fast as f64 / t_dual as f64)
}

// =========================================================================
// Trivial-case validation (Phase 2 gate)
// =========================================================================

#[test]
fn oracle_single_path_is_that_path() {
    // N=1: completion ~ K / goodput (+ owd). Loss-free sanity + goodput sanity.
    let p = Path { rate: 10.0, owd: 5, p_gb: 0.0, q_bg: 1.0 }; // lossless
    let mut o = Oracle::new(vec![p], 5000, 0.0, usize::MAX, false, Place::WorkConserving, 1);
    let t = o.run().unwrap();
    // lossless: K/rate + owd
    let expect = 5000.0 / 10.0 + 5.0;
    let err = (t as f64 - expect).abs() / expect;
    println!("single lossless: t={t} expect~{expect:.0} err={:.3}", err);
    assert!(err < 0.05, "single-path lossless completion off: {t} vs {expect:.0}");

    // lossy single path: completion ~ K / goodput (ARQ re-sends the losses).
    let pl = Path { rate: 10.0, owd: 5, p_gb: 0.02, q_bg: 0.4 };
    let mut o2 = Oracle::new(vec![pl], 5000, 0.05, usize::MAX, false, Place::WorkConserving, 2);
    let t2 = o2.run().unwrap();
    let g = pl.goodput();
    let lower = 5000.0 / pl.rate; // can't beat raw capacity
    println!("single lossy: t={t2} goodput={g:.2} K/rate={lower:.0}");
    assert!(t2 as f64 >= lower * 0.98, "cannot beat raw capacity");
    assert!(t2 as f64 <= 5000.0 / g * 1.6 + 200.0, "lossy single path far worse than K/goodput");
}

#[test]
fn oracle_symmetric_two_path_aggregates() {
    // Two identical paths ~ 2x a single path, minus overhead.
    let p = Path { rate: 8.0, owd: 8, p_gb: 0.01, q_bg: 0.5 };
    let single = {
        let mut o = Oracle::new(vec![p], 8000, 0.05, 4096, false, Place::WorkConserving, 7);
        o.run().unwrap()
    };
    let dual = {
        let mut o = Oracle::new(vec![p, p], 8000, 0.05, 4096, false, Place::WorkConserving, 7);
        o.run().unwrap()
    };
    let f = single as f64 / dual as f64;
    println!("symmetric: single={single} dual={dual} factor=x{f:.3} (ideal 2.0)");
    assert!(f > 1.6, "symmetric 2-path should ~2x (got x{f:.3})");
    assert!(f <= 2.1, "cannot exceed 2x + noise (got x{f:.3})");
}

// =========================================================================
// PHASE 3 — RECONCILE the L0/L1 contradiction at C8
//   L0 wasm sim predicted x1.18 (aggregates); L1 netem measured x0.76.
//   The oracle is independent ground truth. Which does it match, and does a
//   high-enough r cross fast-path-alone?
// =========================================================================

#[test]
fn oracle_c8_reconciliation() {
    let k = 20_000usize; // ~30 MB at 1500 B; ratio is scale-invariant
    let dual = [c8_fast(), c8_slow()];
    let g_ceiling = (c8_fast().goodput() + c8_slow().goodput()) / c8_fast().goodput();
    println!("\n=== C8 ORACLE RECONCILIATION (K={k}) ===");
    println!("goodput ceiling  Sum g_i / g_fast = x{g_ceiling:.3}  (theoretical max aggregation)");
    println!("L0 wasm predicted x1.18 ; L1 netem measured x0.76");

    // (1) ATOMIC unit (block-affine, no cross-path repair fungibility) — the
    //     paper 16.1/16.2 regime-(2) bound. Repairs cannot fill holes; a lost
    //     slow-path symbol serialises at the slow path's rate + RTT.
    println!("\n-- ATOMIC (path-affine units, regime 2) --");
    for &h in &[512usize, 4096, usize::MAX] {
        let (tf, td, fac) = factor(&dual, k, 0.10, h, true, Place::WorkConserving);
        let hl = if h == usize::MAX { "inf".to_string() } else { h.to_string() };
        println!("  atomic  H={hl:>5}  r=0.10  fast={tf} dual={td}  -> x{fac:.3}");
    }

    // (1b) ATOMIC + SAME-PATH recovery (a lost slow-path symbol is BOTH
    //      path-affine AND retransmitted on the slow path — recovered at the
    //      slow path's rate and RTT). This is the specific-source-symbol
    //      dependency the L1 hypothesis names, and the closest analogue of
    //      the measured x0.76.
    println!("\n-- ATOMIC + SAME-PATH recovery (L1 pathology analogue) --");
    for &h in &[512usize, 4096] {
        let (tf, td, fac) = factor_cfg(&dual, k, 0.10, h, true, false, Place::WorkConserving);
        println!("  atomic+samepath  H={h:>5}  fast={tf} dual={td}  -> x{fac:.3}");
    }

    // (2) FUNGIBLE sliding window (RWM), r-sweep at several horizons H.
    println!("\n-- FUNGIBLE (cross-path striped RWM) : r-sweep x H --");
    println!("  {:>6} | {:>8} {:>8} {:>8} {:>8} {:>8}", "H", "r=0.00", "r=0.05", "r=0.10", "r=0.18", "r=0.30");
    let rs = [0.00, 0.05, 0.10, 0.18, 0.30];
    for &h in &[256usize, 1024, 4096, usize::MAX] {
        let hl = if h == usize::MAX { "inf".to_string() } else { h.to_string() };
        let mut row = format!("  {hl:>6} |");
        for &r in &rs {
            let (_tf, _td, fac) = factor(&dual, k, r, h, false, Place::WorkConserving);
            row.push_str(&format!(" {:>7.3}x", fac));
        }
        println!("{row}");
    }

    // (3) Placement law sensitivity (fungible, H=inf, r=0.10): PULL vs PUSH.
    println!("\n-- placement law (fungible, H=inf, r=0.10) : PULL vs PUSH --");
    let (_a, _b, fw) = factor(&dual, k, 0.10, usize::MAX, false, Place::WorkConserving);
    let (_c, _d, fg) = factor(&dual, k, 0.10, usize::MAX, false, Place::GoodputProp);
    let (_e, _f2, fp) = factor(&dual, k, 0.10, usize::MAX, false, Place::PushStatic);
    println!("  work-conserving PULL x{fw:.3} | goodput-prop WRR x{fg:.3} | static PUSH x{fp:.3}");

    // (3b) LEVER DECOMPOSITION at C8 (each lever isolated, best -> worst):
    println!("\n-- LEVER DECOMPOSITION (which fix buys how much) --");
    let (_g1, _g2, l_full) = factor_cfg(&dual, k, 0.10, usize::MAX, false, true, Place::WorkConserving);
    let (_h1, _h2, l_push) = factor_cfg(&dual, k, 0.10, usize::MAX, false, true, Place::PushStatic);
    let (_i1, _i2, l_atom) = factor_cfg(&dual, k, 0.10, usize::MAX, true, true, Place::WorkConserving);
    let (_j1, _j2, l_same) = factor_cfg(&dual, k, 0.10, 4096, true, false, Place::WorkConserving);
    let (_k1, _k2, l_atom_push) = factor_cfg(&dual, k, 0.10, usize::MAX, true, true, Place::PushStatic);
    println!("  TARGET  fungible + pull + cross-path        : x{l_full:.3}  (reachable ceiling)");
    println!("  fungible + PUSH + cross-path                : x{l_push:.3}  (placement MASKED by fungibility)");
    println!("  ATOMIC   + pull + cross-path                : x{l_atom:.3}  (DOMINANT lever = fungibility)");
    println!("  ATOMIC   + PUSH + cross-path                : x{l_atom_push:.3}  (placement effect in atomic regime)");
    println!("  ATOMIC   + pull + SAME-path recovery        : x{l_same:.3}  (cross-path recovery lever)");
    println!("  => ordering (independent-GE oracle): FUNGIBILITY(frontier decode) >> CROSS-PATH recovery >> placement");

    // (4) Find the r-crossing of fast-path-alone at H=inf (whole object).
    let mut cross = None;
    let mut r = 0.0;
    while r <= 0.6 + 1e-9 {
        let (_tf, _td, fac) = factor(&dual, k, r, usize::MAX, false, Place::WorkConserving);
        if fac >= 1.0 { cross = Some((r, fac)); break; }
        r += 0.02;
    }
    match cross {
        Some((rc, fac)) => println!("\nr-CROSSING (H=inf): dual first beats fast-alone at r={rc:.2} (x{fac:.3})"),
        None => println!("\nr-CROSSING (H=inf): dual NEVER beats fast-alone for r in [0,0.6]"),
    }

    // --- VERDICT assertions (lock the reconciliation) ---
    // (i) The true object case (fungible cross-path, whole-object horizon)
    //     AGGREGATES to the goodput ceiling — matching L0 wasm (x1.18), NOT
    //     L1 (x0.76). Aggregation IS achievable in principle.
    let (_x, _y, obj) = factor(&dual, k, 0.00, usize::MAX, false, Place::WorkConserving);
    assert!(obj > 1.15 && obj <= g_ceiling + 0.02,
        "object-case oracle must aggregate to ~ceiling (matches L0): got x{obj:.3}");
    // (ii) The goodput ceiling bounds every achievable factor.
    let (_a, _b, best) = factor(&dual, k, 0.10, usize::MAX, false, Place::WorkConserving);
    assert!(best <= g_ceiling + 0.05, "cannot exceed the goodput ceiling x{g_ceiling:.3}: got x{best:.3}");
    // (iii) L1's sub-unity x0.76 is reproduced ONLY by breaking the transport:
    //       path-affine atomic units are already < 1.0, and same-path (non
    //       cross-path) recovery drives it strictly lower — bracketing 0.76.
    //       => 0.76 is a PRODUCTION pathology, not a fundamental bound.
    let (_p, _q, atom) = factor(&dual, k, 0.10, usize::MAX, true, Place::WorkConserving);
    let (_r, _s, samep) = factor_cfg(&dual, k, 0.10, 4096, true, false, Place::WorkConserving);
    assert!(atom < 1.0, "atomic path-affine must be sub-unity (regime 2): x{atom:.3}");
    assert!(samep < atom, "same-path recovery must be strictly worse than cross-path: x{samep:.3} vs x{atom:.3}");
    assert!(samep < 0.76 && atom > 0.76,
        "L1 x0.76 must sit between atomic-clean and atomic+same-path: samep=x{samep:.3}, atom=x{atom:.3}");
    println!("\n[VERDICT] object(fungible) x{obj:.3} == L0 ceiling ; atomic x{atom:.3} ; \
              atomic+samepath x{samep:.3} brackets L1 x0.76 => production bug, not a fundamental bound");
}
