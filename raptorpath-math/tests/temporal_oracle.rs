//! TEMPORAL multipath aggregation oracle — the CORRECTED oracle (goal
//! `feat/oracle-temporal`).
//!
//! WHY THIS FILE EXISTS.  `multipath_oracle.rs` predicted a coded fungible
//! sliding window reaches ×1.19 aggregation at C8.  L1 REFUTED it: coded-only
//! C8 = 3.93 Mbit/s = 0.26× fast-path-alone, and — the decisive signature —
//! adding ANY second path made a coded window WORSE than a single path on both
//! symmetric (5.5) and heterogeneous (3.9) paths (anti-aggregation).  The old
//! oracle abstracted away TIME: it treated every arrived coded symbol as
//! carrying useful rank for its whole window, with no notion that a symbol is
//! a combination over the sender's window AS OF ITS SEND TIME and only arrives
//! a path-delay later.  This file adds that temporal dynamic and a faithful
//! model of the production reliability layer, and re-derives the verdict.
//!
//! WHAT "TEMPORAL" MEANS HERE (the correction).  A coded symbol is combined
//! over the sender's coding window at send time; it is placed on a path and
//! arrives one one-way-delay later.  The two designs differ in what that means
//! for its value on arrival:
//!
//!   * MOVING WINDOW (naive coded sliding, the L1 build).  The coding anchor
//!     MOVES: by the time a slow-path symbol lands, the sender/receiver
//!     frontier has advanced (on the fast path by ≫ W), so the slow symbol
//!     covers a window region the frontier already passed → it is stranded.
//!     A stranded frontier position is not fungibly fillable (the live window
//!     has moved on); the production stack recovers it per-seq via a targeted
//!     ARQ that is CONGESTION-THROTTLED — under a datagram-loss burst the
//!     ADR-0046 multiplier collapses toward 0 and suppresses recovery until it
//!     re-opens (documented, goal-gate "L2 ws1" + Fungible-Frontier notes).
//!     Because a finite store forbids the fast path from working ahead
//!     (store ≈ W), the fast path idles through that throttled tail.  This is
//!     the anti-aggregation drag.
//!
//!   * STABLE GENERATIONS (the alignment fix).  Partition the source into
//!     fixed generations of ~W symbols and code coded symbols WITHIN each
//!     generation.  A generation's coding target NEVER moves, so a slow-path
//!     symbol for generation g stays useful until g decodes regardless of when
//!     it lands.  A lost symbol is replaced by ANY later coded symbol for the
//!     same generation from EITHER path (fungible cross-path recovery, no
//!     per-seq throttle).  Generations pipeline (M in flight), so the fast
//!     path never idles on a slow generation's tail.
//!
//! HONEST SCOPE.  This is a MODEL.  Its per-path capacity, one-way delay and
//! GE loss are the exact C8 netem params; its fork-join, ack-RTT
//! serialization and finite-store coupling are structural; the ONE term fit to
//! the L1 record is the throttled-recovery collapse penalty (the ADR-0046
//! multiplier collapse), whose magnitude is chosen to reproduce the measured
//! ×0.26 — every other number falls out.  The verdict below does NOT depend on
//! the exact penalty: it depends only on generations STRUCTURALLY avoiding the
//! per-seq throttle, which is the stable-anchor claim under test.

use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Channel model (identical semantics to multipath_oracle.rs)
// ---------------------------------------------------------------------------

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
}

struct Ge { bad: bool }
impl Ge {
    fn draw(&mut self, p: &Path, rng: &mut impl Rng) -> bool {
        self.bad = if self.bad {
            rng.gen::<f64>() >= p.q_bg
        } else {
            rng.gen::<f64>() < p.p_gb
        };
        self.bad
    }
}

// symbol = 1500 B = 12 kbit. rate[mbit/s]/12 = symbols/ms.
fn sym_per_ms(mbit: f64) -> f64 { mbit / 12.0 }
fn c8_fast() -> Path { Path { rate: sym_per_ms(100.0), owd: 5, p_gb: 0.013, q_bg: 0.50 } }
fn c8_slow() -> Path { Path { rate: sym_per_ms(20.0), owd: 20, p_gb: 0.02, q_bg: 0.40 } }

// ---------------------------------------------------------------------------
// Temporal generation/window oracle
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Anchor {
    /// MOVING coding anchor (naive coded sliding window).  The live window
    /// advances with the frontier, so the fast path's fresh coded symbols
    /// always code over the CURRENT window and CANNOT retroactively supply rank
    /// for a prior window position that a slow-path symbol was carrying — that
    /// position is stranded and can only be recovered by a targeted per-seq
    /// ARQ.  Net effect: each window's per-path shares are effectively
    /// PATH-AFFINE (the fast path cannot cover the slow path's share), and the
    /// slow path's share is recovered SAME-PATH under a congestion-throttled
    /// (ADR-0046-collapsed) bucket.  W-insensitive, matching L1 (W=200→2.0,
    /// W=2048→2.4).
    Moving,
    /// STABLE coding anchor (fixed generations).  A generation's coding target
    /// never moves, so any coded symbol for it — from ANY path, at any time —
    /// supplies an interchangeable degree of freedom.  Shares are FUNGIBLE (the
    /// fast path covers the slow path's deficit) and recovery is cross-path at
    /// full rate — no per-seq throttle.
    Stable,
}

struct Cfg {
    /// generation / window size in symbols.
    gen_size: usize,
    /// number of generations the sender may keep in flight (pipeline depth).
    /// 1 == stop-and-wait moving window (store ≈ W); ≥2 == pipelined
    /// generations.
    inflight_gens: usize,
    /// proactive repair overhead (extra coded symbols beyond the K sources).
    r: f64,
    anchor: Anchor,
    /// throttled-recovery stall in ms per collapse event (ADR-0046). Only used
    /// under a Moving anchor.  This is THE one L1-fit constant.
    throttle_ms: u64,
}

// Per-path share of a generation's degrees of freedom.  Under a Moving anchor
// each path must deliver ITS OWN assigned share (fork-join, path-affine); under
// a Stable anchor the shares are fungible and only the TOTAL matters.
fn shares(paths: &[Path], gen_len: usize) -> Vec<u32> {
    let g: Vec<f64> = paths.iter().map(|p| p.goodput()).collect();
    let sum: f64 = g.iter().sum();
    let mut out = vec![0u32; paths.len()];
    let mut cred = vec![0.0f64; paths.len()];
    for _ in 0..gen_len {
        let mut best = 0usize; let mut bc = f64::NEG_INFINITY;
        for i in 0..paths.len() { cred[i] += g[i] / sum; if cred[i] > bc { bc = cred[i]; best = i; } }
        cred[best] -= 1.0;
        out[best] += 1;
    }
    out
}

struct Oracle {
    paths: Vec<Path>,
    order: Vec<usize>, // by goodput desc (fast first)
    ge: Vec<Ge>,
    k: usize,
    cfg: Cfg,
    rng: ChaCha8Rng,
    n: usize,
    n_gen: usize,

    // sender state
    gen_acked: usize,    // contiguous decoded-and-acked generations (frees pipeline)
    sent: Vec<Vec<u32>>, // [gen][path] coded symbols sent
    recov_open: Vec<Vec<u64>>, // [gen][path] tick when throttled recovery re-opens

    // channel
    arrivals: BTreeMap<u64, Vec<(usize, usize)>>, // tick -> [(gen, path)]
    acks: BTreeMap<u64, usize>,

    // receiver state
    got: Vec<Vec<u32>>,  // [gen][path] independent DoF delivered (capped per policy)
    decoded: Vec<bool>,
    decoded_count: usize,
    pending: Vec<bool>,  // Moving anchor: shares met, awaiting reorder/ARQ tax
    decode_at: BTreeMap<u64, Vec<usize>>, // tick -> gens that finish their tax

    assign: Vec<Vec<u32>>, // [gen][path] target DoF (fork-join target under Moving)
    credit: Vec<f64>,
}

impl Oracle {
    fn new(paths: Vec<Path>, k: usize, cfg: Cfg, seed: u64) -> Self {
        let n = paths.len();
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &b| paths[b].goodput().partial_cmp(&paths[a].goodput()).unwrap());
        let n_gen = k.div_ceil(cfg.gen_size);
        let gen_len = |g: usize| (k - g * cfg.gen_size).min(cfg.gen_size);
        let assign: Vec<Vec<u32>> = (0..n_gen).map(|g| shares(&paths, gen_len(g))).collect();
        Oracle {
            ge: (0..n).map(|_| Ge { bad: false }).collect(),
            credit: vec![0.0; n],
            paths, order, k, cfg,
            rng: ChaCha8Rng::seed_from_u64(seed),
            n, n_gen,
            gen_acked: 0,
            sent: vec![vec![0; n]; n_gen],
            recov_open: vec![vec![0; n]; n_gen],
            arrivals: BTreeMap::new(),
            acks: BTreeMap::new(),
            got: vec![vec![0; n]; n_gen],
            decoded: vec![false; n_gen],
            decoded_count: 0,
            pending: vec![false; n_gen],
            decode_at: BTreeMap::new(),
            assign,
        }
    }

    fn gen_len(&self, g: usize) -> usize {
        (self.k - g * self.cfg.gen_size).min(self.cfg.gen_size)
    }
    fn ack_owd(&self) -> u64 { self.paths[self.order[0]].owd } // acks ride the fast path

    fn total_got(&self, g: usize) -> u32 { self.got[g].iter().sum() }

    fn gen_complete(&self, g: usize) -> bool {
        match self.cfg.anchor {
            // Stable: fungible — any-path DoF, only the total matters.
            Anchor::Stable => self.total_got(g) >= self.gen_len(g) as u32,
            // Moving: path-affine fork-join — every path must deliver its share.
            Anchor::Moving => (0..self.n).all(|i| self.got[g][i] >= self.assign[g][i]),
        }
    }

    /// TIER 1 — proactive, non-redundant own-share work: path `i` pushes its
    /// proportional share of generation `g` (both anchors stripe ∝ goodput so
    /// the slow path does real, non-redundant work — the source of aggregation).
    fn proactive_owes(&self, g: usize, i: usize) -> bool {
        if self.decoded[g] || self.pending[g] { return false; }
        let share = self.assign[g][i];
        if share == 0 { return false; }
        let budget = ((share as f64) * (1.0 + self.cfg.r)).ceil() as u32;
        self.sent[g][i] < budget
    }

    /// TIER 2 — recovery of the residual deficit after proactive shares.  The
    /// two anchors differ HERE, and only here:
    ///   * Stable: FUNGIBLE cross-path — any path may supply any missing DoF at
    ///     full rate (the fast path covers the slow path's shortfall, so a slow
    ///     or lossy path never becomes a long pole).
    ///   * Moving: PATH-AFFINE same-path, congestion-throttled — a stranded
    ///     position can only be recovered on the path that owned it, gated by
    ///     the collapsed ADR-0046 bucket.
    fn recovery_owes(&self, g: usize, i: usize, tick: u64) -> bool {
        if self.decoded[g] || self.pending[g] { return false; }
        match self.cfg.anchor {
            Anchor::Stable => self.total_got(g) < self.gen_len(g) as u32,
            Anchor::Moving => {
                let share = self.assign[g][i];
                if share == 0 || self.got[g][i] >= share { return false; }
                tick >= self.recov_open[g][i]
            }
        }
    }

    fn schedule_arrival(&mut self, tick: u64, gen: usize, path: usize) {
        self.arrivals.entry(tick).or_default().push((gen, path));
    }

    fn advance_acks(&mut self, tick: u64) {
        let mut ga = self.gen_acked;
        while ga < self.n_gen && self.decoded[ga] { ga += 1; }
        if ga > self.gen_acked {
            let at = tick + self.ack_owd();
            let e = self.acks.entry(at).or_insert(ga);
            if ga > *e { *e = ga; }
        }
    }

    fn finish_gen(&mut self, g: usize, tick: u64) {
        if self.decoded[g] { return; }
        self.decoded[g] = true;
        self.decoded_count += 1;
        self.advance_acks(tick);
    }

    fn run(&mut self) -> Option<u64> {
        let max_tick: u64 = 40_000_000;
        let mut tick: u64 = 0;
        loop {
            if let Some(v) = self.acks.remove(&tick) {
                if v > self.gen_acked { self.gen_acked = v; }
            }
            // generations whose reorder/ARQ tax has elapsed now decode.
            if let Some(gs) = self.decode_at.remove(&tick) {
                for g in gs { self.finish_gen(g, tick); }
            }
            if let Some(evs) = self.arrivals.remove(&tick) {
                for (g, i) in evs {
                    if self.decoded[g] || self.pending[g] { continue; }
                    // A delivered coded symbol supplies a useful DoF unless this
                    // path's share is already met (Moving) or the generation is
                    // already full (Stable) — extras are redundant.
                    let useful = match self.cfg.anchor {
                        Anchor::Stable => self.total_got(g) < self.gen_len(g) as u32,
                        Anchor::Moving => self.got[g][i] < self.assign[g][i],
                    };
                    if useful { self.got[g][i] += 1; }
                    if !self.decoded[g] && !self.pending[g] && self.gen_complete(g) {
                        // Moving anchor across N≥2 paths pays a per-window
                        // reorder/ARQ tax (congestion-throttled per-seq recovery
                        // of the cross-path interleaving — present even on
                        // symmetric paths, since delivery is per-seq in-order
                        // beneath the coding).  A stable anchor / single path
                        // decodes out-of-order with no such tax.
                        if self.cfg.anchor == Anchor::Moving && self.n >= 2 {
                            self.pending[g] = true;
                            self.decode_at.entry(tick + self.cfg.throttle_ms).or_default().push(g);
                        } else {
                            self.finish_gen(g, tick);
                        }
                    }
                }
            }
            if self.decoded_count == self.n_gen { return Some(tick); }

            let mut free: Vec<u32> = vec![0; self.n];
            for i in 0..self.n {
                self.credit[i] += self.paths[i].rate;
                free[i] = self.credit[i].floor() as u32;
                self.credit[i] -= free[i] as f64;
            }
            let total: u32 = free.iter().sum();
            let hi = (self.gen_acked + self.cfg.inflight_gens).min(self.n_gen);
            'slots: for _ in 0..total {
                let path = match self.order.iter().copied().find(|&i| free[i] > 0) {
                    Some(p) => p,
                    None => break,
                };
                // TIER 1 first (proactive own-share, lowest gen), then TIER 2
                // (recovery).  This keeps the slow path doing non-redundant
                // work before either path spends slots on recovery.
                let mut chosen = None;
                for g in self.gen_acked..hi {
                    if self.proactive_owes(g, path) { chosen = Some(g); break; }
                }
                if chosen.is_none() {
                    for g in self.gen_acked..hi {
                        if self.recovery_owes(g, path, tick) { chosen = Some(g); break; }
                    }
                }
                let g = match chosen {
                    Some(g) => g,
                    None => {
                        // this path has nothing useful to do (fork-join idle /
                        // backpressure).  It cannot open a later generation
                        // past the pipeline window, so the slot is wasted.
                        free[path] = 0;
                        if (0..self.n).all(|i| free[i] == 0) { break 'slots; }
                        // if no path with a free slot owes anything, end the tick.
                        let mut any = false;
                        'chk: for gg in self.gen_acked..hi {
                            for i in 0..self.n {
                                if free[i] > 0
                                    && (self.proactive_owes(gg, i) || self.recovery_owes(gg, i, tick)) {
                                    any = true; break 'chk;
                                }
                            }
                        }
                        if !any { for f in free.iter_mut() { *f = 0; } break 'slots; }
                        continue;
                    }
                };
                free[path] -= 1;
                self.sent[g][path] += 1;
                // A recovery send (past the proactive budget) re-arms the
                // collapsed congestion bucket for the moving anchor.
                if self.cfg.anchor == Anchor::Moving {
                    let need = self.assign[g][path];
                    let budget = ((need as f64) * (1.0 + self.cfg.r)).ceil() as u32;
                    if self.sent[g][path] > budget {
                        self.recov_open[g][path] = tick + self.cfg.throttle_ms;
                    }
                }
                let lost = self.ge[path].draw(&self.paths[path], &mut self.rng);
                if !lost {
                    self.schedule_arrival(tick + self.paths[path].owd, g, path);
                }
            }

            tick += 1;
            if tick > max_tick { return None; }
        }
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// Returns (t_fast_alone, t_dual, factor = t_fast/t_dual).  factor > 1 == the
/// dual path aggregates above the single fast path.
fn factor(dual: &[Path], k: usize, cfg_of: impl Fn() -> Cfg, seed: u64) -> (u64, u64, f64) {
    let best = *dual.iter()
        .max_by(|a, b| a.goodput().partial_cmp(&b.goodput()).unwrap()).unwrap();
    let mut fa = Oracle::new(vec![best], k, cfg_of(), seed);
    let mut du = Oracle::new(dual.to_vec(), k, cfg_of(), seed);
    let tf = fa.run().expect("fast-alone must finish");
    let td = du.run().expect("dual must finish");
    (tf, td, tf as f64 / td as f64)
}

// Naive coded MOVING window as realized at L1: one window at a time
// (store ≈ W ⇒ inflight_gens = 1, stop-and-wait), coded symbols stranded by
// the moving anchor recovered per-seq through the collapsed ADR-0046 throttle.
fn naive_moving(w: usize, r: f64) -> Cfg {
    Cfg { gen_size: w, inflight_gens: 1, r, anchor: Anchor::Moving, throttle_ms: 190 }
}
// Stable generations (the alignment fix): fixed anchor, pipelined, fungible
// cross-path recovery.
fn aligned_gen(g: usize, m: usize, r: f64) -> Cfg {
    Cfg { gen_size: g, inflight_gens: m, r, anchor: Anchor::Stable, throttle_ms: 0 }
}

// =========================================================================
// PART 1 — FIDELITY: the corrected oracle must REPRODUCE the L1 refutation.
//   L1: coded-only C8 het dual = 0.26× fast-alone; C7 sym dual = 0.36×;
//       coded-only SINGLE = 0.85× (codec cost only); and the decisive
//       signature — DUAL is worse than SINGLE on BOTH sym and het paths.
// =========================================================================

#[test]
fn temporal_fidelity_reproduces_l1_refutation() {
    let k = 20_000usize;
    let w = 640usize; // W_mp at C8
    let r = 0.10;
    let dual_het = [c8_fast(), c8_slow()];
    let dual_sym = [c8_fast(), c8_fast()];

    println!("\n=== PART 1: TEMPORAL ORACLE FIDELITY vs L1 (K={k}, W={w}, r={r}) ===");
    println!("naive coded MOVING window (stop-and-wait store≈W, throttled per-seq recovery)\n");

    let (tf_h, td_h, f_het) = factor(&dual_het, k, || naive_moving(w, r), 0xF00D);
    let (tf_s, td_s, f_sym) = factor(&dual_sym, k, || naive_moving(w, r), 0xF00D);

    // single coded path (the "codec cost only" reference): fast path alone.
    let best = c8_fast();
    let mut single = Oracle::new(vec![best], k, naive_moving(w, r), 0xF00D);
    let t_single = single.run().unwrap();
    // an idealized systematic single path (no coding tax) for the 0.85 ratio:
    let mut sysm = Oracle::new(vec![best], k, aligned_gen(w, 4, r), 0xF00D);
    let t_sys = sysm.run().unwrap();
    let single_vs_sys = t_sys as f64 / t_single as f64;

    println!("  {:<38} {:>10} {:>10} {:>10}", "config", "t_fast", "t_dual", "factor");
    println!("  {:<38} {:>10} {:>10} {:>9.3}x", "C8 HET dual (c2+c3)", tf_h, td_h, f_het);
    println!("  {:<38} {:>10} {:>10} {:>9.3}x", "C7 SYM dual (c2+c2)", tf_s, td_s, f_sym);
    println!("  coded SINGLE vs systematic-single: {:>0.3}x (codec cost only)", single_vs_sys);

    println!("\n  --- side by side with the L1 record ---");
    println!("  {:<26} {:>12} {:>12}", "signature", "L1 measured", "oracle");
    println!("  {:<26} {:>11.2}x {:>11.3}x", "C8 het dual / fast-alone", 0.26, f_het);
    println!("  {:<26} {:>11.2}x {:>11.3}x", "C7 sym dual / fast-alone", 0.36, f_sym);
    println!("  {:<26} {:>11.2}x {:>11.3}x", "coded single / systematic", 0.85, single_vs_sys);
    println!("  {:<26} {:>12} {:>12}",       "dual < single?", "YES (drag)",
        if f_het < 1.0 && f_sym < 1.0 { "YES (drag)" } else { "NO" });

    // FIDELITY GATE.
    // (i) heterogeneous dual reproduces the L1 ×0.26 drag.
    assert!((0.15..0.38).contains(&f_het),
        "C8 het must reproduce the L1 drag (~0.26x): got {f_het:.3}x");
    // (ii) symmetric dual reproduces the L1 ×0.36 collapse.
    assert!((0.20..0.55).contains(&f_sym),
        "C7 sym must reproduce the L1 collapse (~0.36x): got {f_sym:.3}x");
    // (iii) the decisive anti-aggregation signature: dual worse than single on
    //       BOTH symmetric and heterogeneous paths.
    assert!(f_het < 1.0, "het dual must be worse than fast-alone: {f_het:.3}x");
    assert!(f_sym < 1.0, "sym dual must be worse than fast-alone: {f_sym:.3}x");
    // (iv) het worse than sym (heterogeneity compounds the drag).
    assert!(f_het < f_sym,
        "het drag must be deeper than sym: het={f_het:.3} sym={f_sym:.3}");
}

// =========================================================================
// PART 2 — the ALIGNMENT FIX: stable generations, coded WITHIN each generation,
//   pipelined, fungible cross-path recovery.  Does it reach ~×1.19 at C8
//   WITHOUT the dual-worse-than-single drag?
// =========================================================================

#[test]
fn temporal_alignment_fix_generation_coding() {
    let k = 20_000usize;
    let r = 0.10;
    let dual_het = [c8_fast(), c8_slow()];
    let dual_sym = [c8_fast(), c8_fast()];
    let ceiling = (c8_fast().goodput() + c8_slow().goodput()) / c8_fast().goodput();

    println!("\n=== PART 2: ALIGNMENT FIX — stable generation coding (K={k}, r={r}) ===");
    println!("goodput ceiling Σg/g_fast = x{ceiling:.3}\n");
    println!("sweep generation size G × pipeline depth M at C8 (het):");
    println!("  {:>6} | {:>8} {:>8} {:>8}", "G", "M=2", "M=3", "M=4");
    let gens = [256usize, 384, 512, 640, 768, 1024];
    let mut best_het = 0.0f64;
    let mut best_at: (usize, usize) = (0, 0);
    for &g in &gens {
        let mut row = format!("  {g:>6} |");
        for &m in &[2usize, 3, 4] {
            let (_tf, _td, fac) = factor(&dual_het, k, || aligned_gen(g, m, r), 0xF00D);
            row.push_str(&format!(" {fac:>7.3}x"));
            if fac > best_het { best_het = fac; best_at = (g, m); }
        }
        println!("{row}");
    }
    println!("\n  best C8 het aggregation: x{best_het:.3} at G={} M={}", best_at.0, best_at.1);

    // symmetric C7 control (must stay ≥ ~2x minus overhead, no drag).
    let (_a, _b, sym) = factor(&dual_sym, k, || aligned_gen(640, 3, r), 0xF00D);
    println!("  C7 sym (G=640, M=3): x{sym:.3} (ideal ~2.0)");

    println!("\n  --- verdict inputs ---");
    println!("  ceiling             x{ceiling:.3}");
    println!("  aligned-gen C8 het  x{best_het:.3}");
    println!("  aligned-gen C7 sym  x{sym:.3}");

    // VERDICT ASSERTIONS.
    // Aligned generations must AGGREGATE above the fast path at C8 (the fix
    // works) and approach the goodput ceiling.
    assert!(best_het > 1.15,
        "aligned generation coding must reach ~ceiling at C8 het: got x{best_het:.3}");
    assert!(best_het <= ceiling + 0.05,
        "cannot exceed the goodput ceiling x{ceiling:.3}: got x{best_het:.3}");
    // No drag on symmetric paths — must aggregate toward 2x.
    assert!(sym > 1.6,
        "C7 symmetric aligned generations must ~2x (no drag): got x{sym:.3}");
}

// =========================================================================
// PART 2b — CONTRAST: same temporal oracle, one knob at a time, to show the
//   drag is the MOVING anchor + throttled per-seq recovery, and the fix is the
//   STABLE anchor + fungible recovery.  (Also shows the ideal temporal drift
//   ALONE, i.e. fungible recovery, does not drag — it aggregates.)
// =========================================================================

#[test]
fn temporal_lever_decomposition() {
    let k = 20_000usize;
    let w = 640usize;
    let r = 0.10;
    let dual = [c8_fast(), c8_slow()];
    let ceiling = (c8_fast().goodput() + c8_slow().goodput()) / c8_fast().goodput();

    println!("\n=== PART 2b: LEVER DECOMPOSITION at C8 het (K={k}, W={w}) ===");
    println!("goodput ceiling x{ceiling:.3}\n");

    // (a) naive moving window: path-affine fork-join + throttled same-path
    //     recovery, stop-and-wait (M=1).  The full L1 pathology.
    let (_a1, _a2, naive) = factor(&dual, k, || naive_moving(w, r), 0xF00D);
    // (b) moving-anchor path-affine fork-join, but PIPELINED (M=3): does letting
    //     the fast path work ahead on later windows save it, without a stable
    //     anchor / fungible recovery?
    let (_b1, _b2, mov_pipe) = factor(&dual, k,
        || Cfg { gen_size: w, inflight_gens: 3, r, anchor: Anchor::Moving, throttle_ms: 190 },
        0xF00D);
    // (c) stable anchor (fungible shares + cross-path recovery) but stop-and-wait
    //     (M=1): does a stable anchor alone aggregate without pipelining?
    let (_c1, _c2, stable_saw) = factor(&dual, k,
        || Cfg { gen_size: w, inflight_gens: 1, r, anchor: Anchor::Stable, throttle_ms: 0 },
        0xF00D);
    // (d) the full fix: stable anchor + pipelined generations.
    let (_d1, _d2, full) = factor(&dual, k, || aligned_gen(w, 3, r), 0xF00D);

    println!("  {:<54} {:>9}", "config", "factor");
    println!("  {:<54} {:>8.3}x  (== L1 ~0.26, the refutation)", "moving anchor + throttled recovery, M=1 (NAIVE)", naive);
    println!("  {:<54} {:>8.3}x  (pipelining a moving anchor: partial)", "moving anchor + throttled recovery, M=3", mov_pipe);
    println!("  {:<54} {:>8.3}x  (stable anchor alone, no pipeline)", "stable anchor + fungible recovery, M=1", stable_saw);
    println!("  {:<54} {:>8.3}x  (FULL FIX -> ceiling)", "stable anchor + fungible recovery, M=3 (ALIGNED)", full);
    println!("\n  Reading: the drag is the MOVING anchor (path-affine shares +");
    println!("  throttled same-path recovery); the fix is the STABLE anchor");
    println!("  (fungible cross-path shares/recovery).  Stable anchor is the");
    println!("  dominant lever; pipelining is secondary.");

    assert!(naive < 0.5, "naive must reproduce the deep drag: {naive:.3}");
    assert!(full > 1.15, "full fix must aggregate: {full:.3}");
    assert!(full > naive, "the fix must beat the naive design");
}
