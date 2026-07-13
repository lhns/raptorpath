//! Task #46 — r* BURSTY-LOSS PROVISIONING: validation of the corrected
//! (burst-tail quantile) r* solver against the OLD (GE closed-form) solver,
//! through the #41/#43 oracle machinery.
//!
//! THE CLAIM UNDER TEST (paper Section 8.4.1). The Section 8.4 closed form
//! provisions r* against the GE geometric burst law; on real traces the
//! burst-length tail is 3.8x-26x heavier than geometric, bursts CLUSTER
//! (long memory), and the delivered window-failure misses the delta/eps
//! target by 2-4x beyond the GE-ideal (Section 2.5, MEASURED in
//! real_trace_validation.rs — task #43). The corrected solver provisions
//! against the measured WINDOW LOSS-MASS quantile (the exact failure
//! statistic: a window fails iff its total loss mass exceeds its repair
//! count) via the multi-scale mass tail (`MassStats` + `r_star_mass`).
//! Composition mirrors production `controller_rate` exactly:
//!
//!   r_new = max( r_old,  r_star_mass(mass_stats, W, delta_wf) )
//!
//! THREE ARMS:
//!   (1) GE-SYNTHETIC control: on GE-generated traces the measured mass
//!       tail is the one the Section 8.7 exact DP implies, so r_new must
//!       track r*_exact (within fit/sampling slack) — no over-provisioning
//!       beyond what the GE world itself requires (r_old, the closed
//!       form, sits BELOW r*_exact: that shortfall is Section 8.7's own
//!       documented finding, not a regression of this change).
//!   (2) REAL traces (the five #43 cellular traces, identical loss
//!       derivation): r_new must deliver the window-failure target where
//!       r_old missed by 1.5-4x — or, where no in-window rate up to the
//!       solver ceiling can meet it (deep multi-window fades), return the
//!       ceiling and get as close as the channel allows (feasibility is
//!       reported per cell; an infeasible contract is declared, not
//!       silently missed).
//!   (3) HEAVY-TAIL SYNTHETIC (documented parameters): a semi-Markov
//!       channel with discrete-Weibull(k = 0.5) bursts — the controlled
//!       version of what the real traces show.
//!
//! Helper functions (trace loading, drop-tail loss derivation, GE fit,
//! window replay) MIRROR real_trace_validation.rs verbatim so the two
//! files measure the same process; see that file for the full provenance
//! and methodology commentary.

use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use raptorpath_math::*;
use std::path::PathBuf;

const RHO: f64 = 0.5; // offered load as a fraction of mean capacity
const BUFFER: f64 = 64.0; // drop-tail buffer, packets
const W: usize = 50; // FEC coding window (paper's canonical W)

// ===========================================================================
// Helpers mirrored from real_trace_validation.rs (task #43 methodology)
// ===========================================================================

fn trace_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/traces")
        .join(name)
}

fn load_capacity(name: &str) -> Vec<u32> {
    let text = std::fs::read_to_string(trace_path(name))
        .unwrap_or_else(|e| panic!("cannot read trace {name}: {e}"));
    let ts: Vec<u64> = text
        .split_whitespace()
        .map(|s| s.parse().expect("trace line must be an integer ms"))
        .collect();
    assert!(!ts.is_empty(), "empty trace {name}");
    let t0 = ts[0];
    let span = (ts[ts.len() - 1] - t0) as usize + 1;
    let mut cap = vec![0u32; span];
    for t in ts {
        cap[(t - t0) as usize] += 1;
    }
    cap
}

fn derive_loss(cap: &[u32]) -> Vec<bool> {
    let c_mean: f64 = cap.iter().map(|&c| c as f64).sum::<f64>() / cap.len() as f64;
    let lam = RHO * c_mean;
    let mut q = 0.0f64;
    let mut acc = 0.0f64;
    let mut loss = Vec::with_capacity(cap.len());
    for &c in cap {
        q = (q - c as f64).max(0.0);
        acc += lam;
        let arrivals = acc.floor() as u32;
        acc -= arrivals as f64;
        for _ in 0..arrivals {
            if q + 1.0 <= BUFFER {
                q += 1.0;
                loss.push(false);
            } else {
                loss.push(true);
            }
        }
    }
    loss
}

fn ge_fit(loss: &[bool]) -> (f64, f64) {
    let (mut gg, mut gb, mut bg, mut bb) = (0u64, 0u64, 0u64, 0u64);
    for w in loss.windows(2) {
        match (w[0], w[1]) {
            (false, false) => gg += 1,
            (false, true) => gb += 1,
            (true, false) => bg += 1,
            (true, true) => bb += 1,
        }
    }
    let p = if gg + gb > 0 { gb as f64 / (gg + gb) as f64 } else { 0.0 };
    let q = if bg + bb > 0 { bg as f64 / (bg + bb) as f64 } else { 0.0 };
    (p, q)
}

fn eps_of(loss: &[bool]) -> f64 {
    loss.iter().filter(|&&l| l).count() as f64 / loss.len() as f64
}

fn burst_lengths(loss: &[bool]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut run = 0u32;
    for &l in loss {
        if l {
            run += 1;
        } else if run > 0 {
            out.push(run);
            run = 0;
        }
    }
    if run > 0 {
        out.push(run);
    }
    out
}

/// Empirical window-failure of a loss sequence at rate r: identical block
/// interleaving to real_trace_validation.rs / monte_carlo_validation.rs.
fn window_fail(loss: &[bool], r: f64) -> (f64, usize) {
    let repairs = (r.max(0.0) * W as f64).round() as usize;
    let n = W + repairs;
    if n == 0 {
        return (0.0, 0);
    }
    let nblocks = loss.len() / n;
    if nblocks == 0 {
        return (0.0, 0);
    }
    let mut fails = 0usize;
    for b in 0..nblocks {
        let blk = &loss[b * n..(b + 1) * n];
        let (mut k, mut c) = (0u32, 0u32);
        for (i, &lost) in blk.iter().enumerate() {
            let is_repair = (i + 1) * repairs / n > i * repairs / n;
            if is_repair {
                if !lost {
                    c += 1;
                }
            } else if lost {
                k += 1;
            }
        }
        if c < k {
            fails += 1;
        }
    }
    (fails as f64 / nblocks as f64, nblocks)
}

// ===========================================================================
// The two solvers under comparison
// ===========================================================================

/// Multi-scale sliding block-mass statistics of a loss sequence at block
/// scale w0 (full-trace, undecayed — the oracle-side analogue of the
/// estimator's decayed counters).
fn mass_stats(loss: &[bool], w0: usize) -> MassStats {
    let mut stats = MassStats { block_scale: w0 as f64, ..Default::default() };
    let mut cnt = [0f64; MASS_SCALES];
    let mut ring: Vec<f64> = Vec::new();
    let mut k = 0f64;
    for (i, &lost) in loss.iter().enumerate() {
        if lost {
            k += 1.0;
        }
        if (i + 1) % w0 == 0 {
            ring.push(k);
            k = 0.0;
            for m in 1..=MASS_SCALES {
                if ring.len() >= m {
                    let j: f64 = ring[ring.len() - m..].iter().sum();
                    cnt[m - 1] += 1.0;
                    if j > 0.0 {
                        stats.nz[m - 1] += 1.0;
                        stats.m1[m - 1] += j;
                        stats.m2[m - 1] += j * j;
                    }
                }
            }
        }
    }
    for m in 0..MASS_SCALES {
        if stats.nz[m] > 0.0 {
            stats.m1[m] /= stats.nz[m];
            stats.m2[m] /= stats.nz[m];
            stats.nz[m] /= cnt[m];
        }
    }
    stats
}

/// OLD solver: the Section 8.4 closed form with the sigma2_burst margin —
/// exactly what production computed pre-#46 (and what #43 measured).
fn r_old(eps: f64, s2: f64, tgt_wf: f64) -> f64 {
    let z = normal_quantile(1.0 - tgt_wf);
    compute_r_star_with_z(eps, s2, W as f64, z)
}

/// NEW solver: old PLUS the window-mass quantile term, composed exactly
/// as production `controller_rate` composes it (level rescale to the
/// current loss estimate; on a full-trace fit eps / eps_mass ~ 1, the
/// stationary case).
fn r_new(eps: f64, s2: f64, stats: &MassStats, tgt_wf: f64) -> f64 {
    let scale = eps / stats.eps_mass().max(1e-12);
    r_old(eps, s2, tgt_wf).max(r_star_mass(stats, W as f64, tgt_wf, scale))
}

const TARGETS: [f64; 2] = [0.05, 0.02];

// ===========================================================================
// ARM 1 — GE-synthetic control: no over-provisioning regression on GE
// ===========================================================================

#[test]
fn rstar_tail_ge_control_no_overprovisioning() {
    // Section 2.4 scenarios, 2M-symbol GE draws.
    let cases = [("WiFi", 0.013, 0.5), ("LTE", 0.02, 0.4), ("Sat", 0.03, 0.3)];
    println!("\n=== ARM 1: GE-SYNTHETIC control (W={W}, 2M symbols, seed 42) ===");
    println!(
        "{:<6} {:>6} {:>6} | {:>6} {:>6} {:>7} | {:>8} {:>8} | {:>6} {:>6}",
        "chan", "eps%", "tgt", "r_old", "r_new", "r_exact", "WF_old", "WF_new", "o/tgt", "n/tgt"
    );
    for &(name, p, q) in &cases {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let mut bad = false;
        let loss: Vec<bool> = (0..2_000_000)
            .map(|_| {
                bad = if bad { rng.gen::<f64>() >= q } else { rng.gen::<f64>() < p };
                bad
            })
            .collect();
        let eps = eps_of(&loss);
        let (pf, qf) = ge_fit(&loss);
        let s2 = burst_variance_factor(pf, qf);
        let stats = mass_stats(&loss, W);
        for &tgt in &TARGETS {
            let ro = r_old(eps, s2, tgt);
            let rn = r_new(eps, s2, &stats, tgt);
            let rx = compute_r_star_exact(pf, qf, W, tgt);
            let (wf_o, _) = window_fail(&loss, ro);
            let (wf_n, _) = window_fail(&loss, rn);
            println!(
                "{name:<6} {:>6.2} {:>6.3} | {:>6.3} {:>6.3} {:>7.3} | {:>8.4} {:>8.4} | {:>5.2}x {:>5.2}x",
                eps * 100.0, tgt, ro, rn, rx, wf_o, wf_n, wf_o / tgt, wf_n / tgt
            );
            // NO OVER-PROVISIONING REGRESSION: the corrected r* stays at or
            // near the exact GE optimum — the mass-quantile law reproduces,
            // not exceeds, what the GE world itself requires. (r_old sits
            // BELOW r*_exact and under-delivers even on GE — Section 8.7's
            // own documented closed-form gap, visible in WF_old.)
            assert!(
                rn <= 1.2 * rx,
                "{name} tgt={tgt}: r_new {rn:.3} must not exceed 1.2x the exact GE r* {rx:.3}"
            );
            // And it must deliver the target on the GE draw (sampling
            // tolerance: >= 12k windows per cell).
            assert!(
                wf_n <= 1.25 * tgt,
                "{name} tgt={tgt}: corrected r* must deliver on GE (WF {wf_n:.4} vs {tgt})"
            );
        }
    }
}

// ===========================================================================
// ARM 2 — REAL traces: the corrected r* must deliver where the old missed
// ===========================================================================

const TRACES: &[&str] = &[
    "Verizon-LTE-short.down",
    "ATT-LTE-driving-2016.down",
    "TMobile-UMTS-driving.down",
    "TMobile-LTE-short.down",
    "Verizon-LTE-driving.down",
];

#[test]
fn rstar_tail_real_traces_delivers_target() {
    println!("\n=== ARM 2: REAL traces (W={W}; #43 loss derivation) ===");
    println!(
        "{:<26} {:>6} {:>6} | {:>6} {:>6} {:>5} {:>6} | {:>8} {:>8} | {:>6} {:>6}",
        "trace", "eps%", "tgt", "r_old", "r_new", "feas", "nwin", "WF_old", "WF_new", "o/tgt", "n/tgt"
    );
    let mut worst_old_feasible = 0.0f64;
    let mut worst_new_feasible = 0.0f64;
    let mut old_missed_somewhere = false;
    let mut any_feasible = false;
    let mut all_new_improve = true;
    for &name in TRACES {
        let loss = derive_loss(&load_capacity(name));
        let eps = eps_of(&loss);
        let (pf, qf) = ge_fit(&loss);
        let s2 = burst_variance_factor(pf, qf);
        let stats = mass_stats(&loss, W);
        for &tgt in &TARGETS {
            let ro = r_old(eps, s2, tgt);
            let rn = r_new(eps, s2, &stats, tgt);
            // Feasible = the mass solver found a rate below its ceiling.
            // At the ceiling the contract is DECLARED unmeetable in-window
            // (deep multi-window fades) — production clamps at
            // max_overhead and the miss is explicit, not silent.
            let feasible = rn < R_STAR_TAIL_CEILING - 1e-9;
            let (wf_o, nwin_o) = window_fail(&loss, ro);
            let (wf_n, _) = window_fail(&loss, rn);
            if wf_o > 1.5 * tgt {
                old_missed_somewhere = true;
            }
            if wf_n > wf_o + 1e-9 {
                all_new_improve = false;
            }
            if feasible {
                any_feasible = true;
                worst_old_feasible = worst_old_feasible.max(wf_o / tgt);
                worst_new_feasible = worst_new_feasible.max(wf_n / tgt);
            }
            println!(
                "{name:<26} {:>6.2} {:>6.3} | {:>6.3} {:>6.3} {:>5} {:>6} | {:>8.4} {:>8.4} | {:>5.2}x {:>5.2}x",
                eps * 100.0, tgt, ro, rn,
                if feasible { "yes" } else { "NO" },
                nwin_o, wf_o, wf_n, wf_o / tgt, wf_n / tgt
            );
        }
    }
    println!(
        "\n  worst residual/target over FEASIBLE cells: OLD {worst_old_feasible:.2}x  NEW {worst_new_feasible:.2}x"
    );
    // The documented #43 miss must still be visible for the OLD solver...
    assert!(
        old_missed_somewhere && worst_old_feasible > 1.5,
        "old solver should still miss materially on feasible cells (worst {worst_old_feasible:.2}x)"
    );
    assert!(any_feasible, "at least some trace/target cells must be feasible");
    // ...the corrected solver must deliver the target on every FEASIBLE
    // cell within sampling tolerance (nwin is a few hundred on the short
    // traces: +/- 2 sigma at tgt=0.02, nwin=250 is ~0.85x the target)...
    assert!(
        worst_new_feasible <= 1.5,
        "corrected r* must deliver the rho target on feasible cells (worst {worst_new_feasible:.2}x)"
    );
    // ...and never do worse than the old solver anywhere (including the
    // declared-infeasible cells, where it provisions the ceiling).
    assert!(all_new_improve, "corrected r* must never deliver worse than old");
}

// ===========================================================================
// ARM 3 — heavy-tail SYNTHETIC (documented parameters)
// ===========================================================================

/// Semi-Markov channel: geometric Good sojourns (onset probability
/// p_onset), discrete-Weibull(theta, k) Bad sojourns drawn by inverse
/// transform. This is the controlled version of the real-trace structure:
/// same mean burst as a GE fit would see, heavy tail GE cannot represent.
fn semi_markov_loss(
    n: usize,
    p_onset: f64,
    theta: f64,
    k: f64,
    seed: u64,
) -> Vec<bool> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut loss = Vec::with_capacity(n);
    while loss.len() < n {
        // Good sojourn ~ Geometric(p_onset)
        let g = (rng.gen::<f64>().ln() / (1.0 - p_onset).ln()).ceil().max(1.0) as usize;
        for _ in 0..g {
            loss.push(false);
        }
        // Bad sojourn ~ discrete Weibull: B = ceil(S^{-1}(U))
        let u: f64 = rng.gen::<f64>().max(1e-300);
        let b = (u.ln() / theta.ln()).powf(1.0 / k).ceil().max(1.0) as usize;
        for _ in 0..b.min(10_000) {
            loss.push(true);
        }
    }
    loss.truncate(n);
    loss
}

#[test]
fn rstar_tail_heavy_synthetic_old_misses_new_delivers() {
    // Parameters (documented): Weibull shape k = 0.5, theta = 0.55
    // (mean burst ~ 3.3 — LTE-class), onset tuned for eps ~ 7%.
    let (theta_true, k_true, p_onset) = (0.55f64, 0.5f64, 0.023f64);
    let loss = semi_markov_loss(2_000_000, p_onset, theta_true, k_true, 42);
    let eps = eps_of(&loss);
    let (pf, qf) = ge_fit(&loss);
    let s2 = burst_variance_factor(pf, qf);
    let stats = mass_stats(&loss, W);
    let bl = burst_lengths(&loss);
    let maxb = bl.iter().max().copied().unwrap_or(0);
    println!("\n=== ARM 3: HEAVY-TAIL SYNTHETIC (Weibull k=0.5 bursts) ===");
    println!(
        "  eps={:.2}%  GE fit (p={pf:.4}, q={qf:.4}, s2={s2:.1})  max burst={maxb}",
        eps * 100.0
    );
    for &tgt in &TARGETS {
        let ro = r_old(eps, s2, tgt);
        let rn = r_new(eps, s2, &stats, tgt);
        let (wf_o, nwin) = window_fail(&loss, ro);
        let (wf_n, _) = window_fail(&loss, rn);
        println!(
            "  tgt={tgt}: r_old={ro:.3} r_new={rn:.3} nwin={nwin}  WF_old={wf_o:.4} ({:.1}x tgt)  WF_new={wf_n:.4} ({:.2}x tgt)",
            wf_o / tgt, wf_n / tgt
        );
        assert!(
            wf_o > 1.5 * tgt,
            "old solver must miss on the heavy-tail channel (WF {wf_o:.4} vs tgt {tgt})"
        );
        assert!(
            wf_n <= 1.15 * tgt,
            "corrected solver must deliver on the heavy-tail channel (WF {wf_n:.4} vs tgt {tgt})"
        );
    }
}
