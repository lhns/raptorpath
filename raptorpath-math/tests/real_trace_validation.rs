//! REAL-TRACE validation of the Gilbert-Elliott channel assumption.
//!
//! WHY THIS FILE EXISTS.  The whole r*/oracle result set is proven "for a GE
//! world": every prior rung of the validation ladder (formula ← oracle ← netem)
//! draws loss from a 2-state Gilbert-Elliott Markov chain.  This file tests the
//! bottom-most modelling assumption itself — is GE an ADEQUATE model of REAL
//! link loss w.r.t. our correction-rate r*?  It replays REAL cellular capacity
//! traces, derives a loss process from them honestly, and asks two questions:
//!
//!   (P2) SINGLE-PATH FIDELITY.  Fit GE the way production does, compute r* the
//!        way production does, and run the trace's ACTUAL loss sequence through
//!        the FEC/ARQ window process.  Does the achieved residual hit the target
//!        δ, or does real burst structure make r* UNDER-provision?  And what does
//!        GE MISS (burst-length tail vs geometric; autocorrelation beyond lag-1;
//!        non-stationarity)?
//!
//!   (P3) MULTIPATH DYNAMICS.  Feed two DIFFERENT real traces as two independent
//!        paths into the validated stable-generation coding design and measure
//!        the aggregation factor on real per-path dynamics vs the GE-based ×1.19.
//!
//! TRACE PROVENANCE.  `tests/data/traces/*.down` are real U.S. cellular capacity
//! traces (Verizon/AT&T/T-Mobile LTE/UMTS) recorded with the Saturator tool,
//! Winstein et al., USENIX NSDI 2013, via the mahimahi repo.  Each line is a ms
//! timestamp of a 1500-byte (12 kbit) packet delivery opportunity.  See
//! tests/data/traces/PROVENANCE.md.  These are CAPACITY traces, not loss traces
//! — the loss process is DERIVED (below) by a standard drop-tail queue at the
//! trace's instantaneous capacity, which is the honest way to turn a real
//! capacity fade into a real loss burst.
//!
//! HONEST SCOPE — CORRELATION GAP.  Public single-path traces are INDEPENDENT by
//! construction, so P3 tests real per-path DYNAMICS but NOT path CORRELATION
//! (shared-bottleneck WiFi+LTE losing together).  True correlated-path
//! validation needs simultaneous dual-link capture or a dual-radio hardware
//! testbed; that remains the open milestone (documented in goal-gate.md).
//!
//! NO PRODUCTION CODE CHANGES: everything here is analysis over the public
//! raptorpath_math API (compute_r_star_with_z, compute_r_star_exact,
//! burst_variance_factor, normal_quantile, p_fec_exact).

use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use raptorpath_math::*;
use std::path::PathBuf;

// ===========================================================================
// Trace loading + honest loss derivation
// ===========================================================================

const RHO: f64 = 0.5; // offered load as a fraction of mean capacity
const BUFFER: f64 = 64.0; // drop-tail buffer, packets
const W: usize = 50; // FEC coding window (paper's canonical W)

fn trace_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/traces")
        .join(name)
}

/// Load a mahimahi trace: one ms timestamp per line (a 12 kbit packet delivery
/// opportunity).  Returns per-ms capacity (packets deliverable in each ms).
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

/// Derive a per-symbol loss sequence from a capacity trace via a FIFO drop-tail
/// queue.  A rate-controlled sender offers λ = ρ·C_mean packets/ms into a
/// buffer of `BUFFER` packets drained at the trace's instantaneous capacity.
/// A packet that arrives to a full buffer is DROPPED (lost).  A capacity fade
/// (an outage in the real trace) backs the queue up and overflows a burst of
/// packets — the real-world "outage → loss burst" this whole exercise is about.
/// Deterministic (no RNG): reproducible from the trace alone.
/// Returns a vector of per-sent-symbol outcomes, `true` = lost.
fn derive_loss(cap: &[u32]) -> Vec<bool> {
    let c_mean: f64 = cap.iter().map(|&c| c as f64).sum::<f64>() / cap.len() as f64;
    let lam = RHO * c_mean;
    let mut q = 0.0f64; // queue occupancy (packets)
    let mut acc = 0.0f64; // fractional-arrival accumulator
    let mut loss = Vec::with_capacity(cap.len());
    for &c in cap {
        q = (q - c as f64).max(0.0); // drain at instantaneous capacity
        acc += lam;
        let arrivals = acc.floor() as u32;
        acc -= arrivals as f64;
        for _ in 0..arrivals {
            if q + 1.0 <= BUFFER {
                q += 1.0;
                loss.push(false);
            } else {
                loss.push(true); // buffer full → drop
            }
        }
    }
    loss
}

/// Production-style GE point estimate from a loss sequence: state Bad = lost,
/// p̂ = P(Good→Bad), q̂ = P(Bad→Good) as transition-count ratios (the stationary
/// limit of GilbertElliottEstimator with decay→1).
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

fn autocorr(loss: &[bool], lag: usize) -> f64 {
    let n = loss.len();
    let mean = eps_of(loss);
    let x: Vec<f64> = loss.iter().map(|&l| if l { 1.0 } else { 0.0 } - mean).collect();
    let denom: f64 = x.iter().map(|v| v * v).sum();
    if denom <= 0.0 || lag >= n {
        return 0.0;
    }
    let num: f64 = (0..n - lag).map(|i| x[i] * x[i + lag]).sum();
    num / denom
}

/// Empirical window-FAILURE probability of the real loss sequence: walk it in
/// blocks of n = W + round(r·W); a block interleaves W source symbols and the
/// repairs evenly (identical interleaving to monte_carlo_validation.rs); the
/// block FAILS iff surviving repairs < source losses.  This is the exact FEC/ARQ
/// window process, driven by REAL loss instead of a GE draw.
fn real_window_fail(loss: &[bool], r: f64) -> (f64, usize) {
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

// The GE-ideal window-failure at the SAME r is exactly 1 - P_fec_exact(p,q,r,W)
// (the exact transfer-matrix DP for the interleaved GE process, paper §8.7).
// Comparing real_window_fail against this isolates the pure CHANNEL-MODEL
// mismatch: same optimizer, same r, GE-distributed vs real-distributed loss.
fn ge_ideal_window_fail(p: f64, q: f64, r: f64) -> f64 {
    1.0 - p_fec_exact(p, q, r, W)
}

// The five vendored real traces (see PROVENANCE.md).
const TRACES: &[&str] = &[
    "Verizon-LTE-short.down",
    "ATT-LTE-driving-2016.down",
    "TMobile-UMTS-driving.down",
    "TMobile-LTE-short.down",
    "Verizon-LTE-driving.down",
];

// ===========================================================================
// PART 1 — provenance + loss-derivation sanity
// ===========================================================================

#[test]
fn real_trace_loss_derivation() {
    println!("\n=== REAL TRACE LOSS DERIVATION (drop-tail queue, ρ={RHO}, B={BUFFER} pkt) ===");
    println!(
        "{:<28} {:>9} {:>8} {:>7} {:>7} {:>6} {:>8}",
        "trace", "symbols", "eps%", "p", "q", "sig2", "MBL"
    );
    for &name in TRACES {
        let cap = load_capacity(name);
        let loss = derive_loss(&cap);
        let eps = eps_of(&loss);
        let (p, q) = ge_fit(&loss);
        let s2 = burst_variance_factor(p, q);
        let bl = burst_lengths(&loss);
        let mbl = bl.iter().sum::<u32>() as f64 / bl.len().max(1) as f64;
        println!(
            "{name:<28} {:>9} {:>8.2} {:>7.4} {:>7.4} {:>6.2} {:>8.2}",
            loss.len(),
            eps * 100.0,
            p,
            q,
            s2,
            mbl
        );
        // sanity: derivation produced a non-degenerate bursty loss process in a
        // plausible cellular regime.
        assert!(
            loss.len() > 5_000,
            "{name}: too few symbols ({}) for statistics",
            loss.len()
        );
        assert!(
            (0.01..0.35).contains(&eps),
            "{name}: derived eps {eps:.3} outside plausible cellular regime"
        );
        assert!(p > 0.0 && q > 0.0, "{name}: degenerate GE fit p={p} q={q}");
    }
}

// ===========================================================================
// PART 2a — WHAT GE MISSES: burst tail, long-memory autocorrelation,
// non-stationarity.  These are the STRUCTURES a 2-state Markov chain cannot
// represent, quantified against each trace.
// ===========================================================================

#[test]
fn real_trace_ge_mismatch_structure() {
    println!("\n=== GE-MISMATCH STRUCTURE (real vs the fitted GE's own predictions) ===");
    let mut worst_ac_ratio = 0.0f64; // real/GE autocorrelation at lag 20
    let mut max_nonstat_q_spread = 0.0f64;

    for &name in TRACES {
        let cap = load_capacity(name);
        let loss = derive_loss(&cap);
        let (p, q) = ge_fit(&loss);

        // --- (1) autocorrelation: GE (Markov) predicts ρ(L) = (1-p-q)^L;
        //         real loss carries far longer memory (slow fades). ---
        println!("\n{name}  (p={p:.4} q={q:.4})");
        print!("  autocorr  L= 1    2    5   10   20 :  real ");
        let lags = [1usize, 2, 5, 10, 20];
        let mut real_ac = [0.0; 5];
        for (j, &l) in lags.iter().enumerate() {
            real_ac[j] = autocorr(&loss, l);
            print!("{:.3} ", real_ac[j]);
        }
        print!("\n{:>44}", "GE   ");
        for &l in &lags {
            print!("{:.3} ", (1.0 - p - q).powi(l as i32));
        }
        let ge_ac20 = (1.0 - p - q).powi(20).max(1e-9);
        let ac_ratio = real_ac[4] / ge_ac20;
        worst_ac_ratio = worst_ac_ratio.max(ac_ratio);
        println!(
            "\n  → lag-20 memory: real {:.3} vs GE {:.4}  ({:.0}× longer memory than GE predicts)",
            real_ac[4], ge_ac20, ac_ratio
        );

        // --- (2) burst-length tail: geometric (GE) vs empirical extreme. ---
        let bl = burst_lengths(&loss);
        let maxb = *bl.iter().max().unwrap();
        let ge_max_expected = ((-(bl.len() as f64).ln()) / (1.0 - q).ln()).max(1.0); // E[max of n geom]
        println!(
            "  → burst tail: real max burst = {maxb} symbols; a geometric(q={q:.3}) over {} bursts \
             would reach ≈ {:.0}  ({:.1}× heavier extreme tail)",
            bl.len(),
            ge_max_expected,
            maxb as f64 / ge_max_expected
        );

        // --- (3) non-stationarity: fit GE per sixth; p/q drift within one trace. ---
        let seg = loss.len() / 6;
        let mut qs = Vec::new();
        let mut es = Vec::new();
        for s in 0..6 {
            let chunk = &loss[s * seg..(s + 1) * seg];
            let (_, qq) = ge_fit(chunk);
            qs.push(qq);
            es.push(eps_of(chunk));
        }
        let qmin = qs.iter().cloned().fold(f64::INFINITY, f64::min);
        let qmax = qs.iter().cloned().fold(0.0, f64::max);
        let emin = es.iter().cloned().fold(f64::INFINITY, f64::min);
        let emax = es.iter().cloned().fold(0.0, f64::max);
        max_nonstat_q_spread = max_nonstat_q_spread.max(qmax - qmin);
        println!(
            "  → non-stationary: eps swings {:.1}%–{:.1}% and q swings {:.3}–{:.3} across the trace \
             (a single stationary GE cannot represent this)",
            emin * 100.0,
            emax * 100.0,
            qmin,
            qmax
        );
    }

    println!("\n  SUMMARY: real loss carries >{:.0}× the lag-20 memory GE predicts, and eps/q drift \
              by {:.2} within a single trace.", worst_ac_ratio, max_nonstat_q_spread);

    // FINDING GATE: real loss has structurally longer memory than the Markov GE.
    // If GE were adequate, real lag-20 autocorrelation would decay like GE's
    // (1-p-q)^20 ≈ 0.  It does not — it stays high on every trace.
    assert!(
        worst_ac_ratio > 5.0,
        "real loss should carry far longer memory than GE (worst ratio {worst_ac_ratio:.1})"
    );
    assert!(
        max_nonstat_q_spread > 0.05,
        "real loss should be non-stationary (q spread {max_nonstat_q_spread:.3})"
    );
}

// ===========================================================================
// PART 2b — r* FIDELITY on real loss: does production's r* hit δ, or under-
// provision because real burst structure exceeds the GE (and σ²_burst) model?
// ===========================================================================

#[test]
fn real_trace_r_star_fidelity() {
    // Target semantics match the production controller: it sets
    // z = Φ⁻¹(1 - δ/ε), i.e. the per-WINDOW failure target is target_wf = δ/ε.
    // We sweep target_wf directly so the comparison is apples-to-apples, and use
    // values large enough to be measurable in the finite trace (≥ a few
    // thousand windows). δ = ε · target_wf.
    let targets = [0.05, 0.02];

    println!("\n=== r* FIDELITY on REAL loss (W={W}) ===");
    println!(
        "{:<26} {:>6} {:>6} {:>7} | {:>6} {:>7} {:>7} | {:>8} {:>8} | {:>7}",
        "trace", "eps%", "sig2", "tgt_wf", "r*norm", "r*exact", "nwin", "WF_real", "WF_GE", "×GE"
    );

    let mut worst_underprov_ratio = 0.0f64; // real WF / target, over all cases
    let mut worst_real_vs_ge = 0.0f64; // real WF / GE-ideal WF (pure channel-model gap)
    let mut all_real_exceed_ge = true;

    for &name in TRACES {
        let cap = load_capacity(name);
        let loss = derive_loss(&cap);
        let eps = eps_of(&loss);
        let (p, q) = ge_fit(&loss);
        let s2 = burst_variance_factor(p, q);

        for &tgt in &targets {
            let z = normal_quantile(1.0 - tgt);
            // production's closed-form r* (normal approx, σ²_burst margin)
            let r_norm = compute_r_star_with_z(eps, s2, W as f64, z);
            // the model's BEST GE-optimal r* (exact transfer-matrix DP, §8.7)
            let r_exact = compute_r_star_exact(p, q, W, tgt);

            // achieved window-failure of the closed-form r* on REAL loss ...
            let (wf_real, nwin) = real_window_fail(&loss, r_norm);
            // ... vs the GE-ideal at the same r* (1 - P_fec_exact): what the
            // model THINKS this r* achieves.
            let wf_ge = ge_ideal_window_fail(p, q, r_norm);

            let ratio_target = wf_real / tgt;
            let ratio_ge = wf_real / wf_ge.max(1e-6);
            worst_underprov_ratio = worst_underprov_ratio.max(ratio_target);
            worst_real_vs_ge = worst_real_vs_ge.max(ratio_ge);
            if wf_real <= wf_ge {
                all_real_exceed_ge = false;
            }

            println!(
                "{name:<26} {:>6.2} {:>6.2} {:>7.3} | {:>6.3} {:>7.3} {:>7} | {:>8.4} {:>8.4} | {:>6.1}×",
                eps * 100.0,
                s2,
                tgt,
                r_norm,
                r_exact,
                nwin,
                wf_real,
                wf_ge,
                ratio_ge
            );
        }
    }

    println!(
        "\n  FINDING: on real loss the GE-fitted r* UNDER-PROVISIONS.  Worst achieved residual\n\
         \x20 window-failure = {:.1}× the target δ/ε, and {:.1}× worse than the GE-ideal the model\n\
         \x20 predicts for the SAME r* — even with the full σ²_burst margin (r* up to ~55% overhead).\n\
         \x20 The gap beyond GE-ideal is pure CHANNEL-MODEL mismatch: σ²_burst inflates lag-1\n\
         \x20 variance but cannot capture the long-memory / heavy-fade-tail / non-stationary\n\
         \x20 structure quantified in real_trace_ge_mismatch_structure.",
        worst_underprov_ratio, worst_real_vs_ge
    );

    // FINDING GATE.  This test DOCUMENTS an inadequacy, so it asserts the
    // inadequacy is real and material (not that provisioning succeeds):
    //   (i) real residual exceeds the GE-ideal on every trace/target — the
    //       channel-model gap is systematic, not noise;
    //  (ii) the shortfall is material (worst case well above target).
    assert!(
        all_real_exceed_ge,
        "real window-failure should systematically exceed the GE-ideal prediction"
    );
    assert!(
        worst_real_vs_ge > 1.5,
        "channel-model gap should be material (worst real/GE-ideal {worst_real_vs_ge:.1}×)"
    );
    assert!(
        worst_underprov_ratio > 1.5,
        "r* should measurably under-provision on real loss (worst {worst_underprov_ratio:.1}× target)"
    );
}

// ===========================================================================
// PART 3 — MULTIPATH: stable-generation coding on two REAL per-path traces.
// A leaner port of temporal_oracle.rs's VALIDATED stable-anchor generation
// design (fixed generations, pipelined, fungible cross-path recovery), with the
// per-path GE draw REPLACED by a replay of the trace-derived real loss.  We run
// the identical design on (a) real per-path loss and (b) a GE control fitted to
// each path, at the same C8 rates/OWDs that produced the ×1.19 GE reference, so
// any difference is due to REAL per-path DYNAMICS, not the operating point.
// ===========================================================================

#[derive(Clone)]
struct PathSpec {
    rate: f64, // symbols per ms (capacity offered to the coder)
    owd: u64,  // one-way delay, ms
}

fn sym_per_ms(mbit: f64) -> f64 {
    mbit / 12.0
}

/// A per-path loss oracle: replays a real derived loss sequence, or draws GE.
enum Loss {
    Real { seq: Vec<bool>, idx: usize },
    Ge { p: f64, q: f64, bad: bool, rng: ChaCha8Rng },
}
impl Loss {
    fn draw(&mut self) -> bool {
        match self {
            Loss::Real { seq, idx } => {
                let v = seq[*idx % seq.len()];
                *idx += 1;
                v
            }
            Loss::Ge { p, q, bad, rng } => {
                *bad = if *bad {
                    rng.gen::<f64>() >= *q
                } else {
                    rng.gen::<f64>() < *p
                };
                *bad
            }
        }
    }
}

/// Goodput-proportional integer share of a generation across paths.
fn shares(goodput: &[f64], gen_len: usize) -> Vec<u32> {
    let sum: f64 = goodput.iter().sum();
    let mut out = vec![0u32; goodput.len()];
    let mut cred = vec![0.0f64; goodput.len()];
    for _ in 0..gen_len {
        let mut best = 0usize;
        let mut bc = f64::NEG_INFINITY;
        for i in 0..goodput.len() {
            cred[i] += goodput[i] / sum;
            if cred[i] > bc {
                bc = cred[i];
                best = i;
            }
        }
        cred[best] -= 1.0;
        out[best] += 1;
    }
    out
}

/// Stable-anchor, pipelined, fungible-recovery generation oracle (the validated
/// ×1.19 design).  Returns the tick at which all K source symbols are decoded.
struct GenOracle {
    specs: Vec<PathSpec>,
    order: Vec<usize>, // fast first
    loss: Vec<Loss>,
    k: usize,
    gen_size: usize,
    inflight: usize,
    r: f64,
    n_gen: usize,
    assign: Vec<Vec<u32>>,   // [gen][path] proactive share target
    sent: Vec<Vec<u32>>,     // [gen][path] sent
    got: Vec<Vec<u32>>,      // [gen][path] delivered useful DoF
    decoded: Vec<bool>,
    decoded_count: usize,
    gen_acked: usize,
    credit: Vec<f64>,
    arrivals: std::collections::BTreeMap<u64, Vec<usize>>, // tick -> [gen]
    acks: std::collections::BTreeMap<u64, usize>,
}

impl GenOracle {
    fn new(specs: Vec<PathSpec>, loss: Vec<Loss>, k: usize, gen_size: usize, inflight: usize, r: f64) -> Self {
        let n = specs.len();
        let goodput: Vec<f64> = (0..n)
            .map(|i| {
                // goodput proxy from average loss the oracle will actually see
                let e = match &loss[i] {
                    Loss::Real { seq, .. } => seq.iter().filter(|&&l| l).count() as f64 / seq.len() as f64,
                    Loss::Ge { p, q, .. } => p / (p + q),
                };
                specs[i].rate * (1.0 - e)
            })
            .collect();
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &b| goodput[b].partial_cmp(&goodput[a]).unwrap());
        let n_gen = k.div_ceil(gen_size);
        let gen_len = |g: usize| (k - g * gen_size).min(gen_size);
        let assign: Vec<Vec<u32>> = (0..n_gen).map(|g| shares(&goodput, gen_len(g))).collect();
        GenOracle {
            specs,
            order,
            loss,
            k,
            gen_size,
            inflight,
            r,
            n_gen,
            assign,
            sent: vec![vec![0; n]; n_gen],
            got: vec![vec![0; n]; n_gen],
            decoded: vec![false; n_gen],
            decoded_count: 0,
            gen_acked: 0,
            credit: vec![0.0; n],
            arrivals: std::collections::BTreeMap::new(),
            acks: std::collections::BTreeMap::new(),
        }
    }

    fn n(&self) -> usize {
        self.specs.len()
    }
    fn gen_len(&self, g: usize) -> usize {
        (self.k - g * self.gen_size).min(self.gen_size)
    }
    fn total_got(&self, g: usize) -> u32 {
        self.got[g].iter().sum()
    }
    fn complete(&self, g: usize) -> bool {
        self.total_got(g) >= self.gen_len(g) as u32
    }
    fn ack_owd(&self) -> u64 {
        self.specs[self.order[0]].owd
    }

    // TIER 1: proactive own-share (both paths stripe ∝ goodput; the source of
    // aggregation).  TIER 2: fungible cross-path recovery of any residual.
    fn proactive_owes(&self, g: usize, i: usize) -> bool {
        if self.decoded[g] {
            return false;
        }
        let share = self.assign[g][i];
        if share == 0 {
            return false;
        }
        let budget = ((share as f64) * (1.0 + self.r)).ceil() as u32;
        self.sent[g][i] < budget
    }
    fn recovery_owes(&self, g: usize) -> bool {
        !self.decoded[g] && self.total_got(g) < self.gen_len(g) as u32
    }

    fn advance_acks(&mut self, tick: u64) {
        let mut ga = self.gen_acked;
        while ga < self.n_gen && self.decoded[ga] {
            ga += 1;
        }
        if ga > self.gen_acked {
            let at = tick + self.ack_owd();
            let e = self.acks.entry(at).or_insert(ga);
            if ga > *e {
                *e = ga;
            }
        }
    }

    fn run(&mut self) -> u64 {
        let max_tick = 40_000_000u64;
        let mut tick = 0u64;
        loop {
            if let Some(v) = self.acks.remove(&tick) {
                if v > self.gen_acked {
                    self.gen_acked = v;
                }
            }
            if let Some(evs) = self.arrivals.remove(&tick) {
                for g in evs {
                    if self.decoded[g] {
                        continue;
                    }
                    if !self.decoded[g] && self.complete(g) {
                        self.decoded[g] = true;
                        self.decoded_count += 1;
                        self.advance_acks(tick);
                    }
                }
            }
            if self.decoded_count == self.n_gen {
                return tick;
            }

            let mut free = vec![0u32; self.n()];
            for i in 0..self.n() {
                self.credit[i] += self.specs[i].rate;
                free[i] = self.credit[i].floor() as u32;
                self.credit[i] -= free[i] as f64;
            }
            let total: u32 = free.iter().sum();
            let hi = (self.gen_acked + self.inflight).min(self.n_gen);
            'slots: for _ in 0..total {
                let path = match self.order.iter().copied().find(|&i| free[i] > 0) {
                    Some(p) => p,
                    None => break,
                };
                let mut chosen = None;
                for g in self.gen_acked..hi {
                    if self.proactive_owes(g, path) {
                        chosen = Some(g);
                        break;
                    }
                }
                if chosen.is_none() {
                    for g in self.gen_acked..hi {
                        if self.recovery_owes(g) {
                            chosen = Some(g);
                            break;
                        }
                    }
                }
                let g = match chosen {
                    None => {
                        free[path] = 0;
                        if free.iter().all(|&f| f == 0) {
                            break 'slots;
                        }
                        let mut any = false;
                        'chk: for gg in self.gen_acked..hi {
                            for i in 0..self.n() {
                                if free[i] > 0 && (self.proactive_owes(gg, i) || self.recovery_owes(gg)) {
                                    any = true;
                                    break 'chk;
                                }
                            }
                        }
                        if !any {
                            for f in free.iter_mut() {
                                *f = 0;
                            }
                            break 'slots;
                        }
                        continue;
                    }
                    Some(g) => g,
                };
                free[path] -= 1;
                self.sent[g][path] += 1;
                let lost = self.loss[path].draw();
                if !lost {
                    // useful iff the generation still needs a DoF (fungible)
                    if self.total_got(g) < self.gen_len(g) as u32 {
                        let at = tick + self.specs[path].owd;
                        self.arrivals.entry(at).or_default().push(g);
                        self.got[g][path] += 1;
                    }
                }
            }
            tick += 1;
            if tick > max_tick {
                panic!("gen-oracle did not converge");
            }
        }
    }
}

#[test]
fn real_trace_generation_oracle_aggregation() {
    // Two DIFFERENT real traces as two INDEPENDENT paths (honest scope: this is
    // real per-path DYNAMICS, not path CORRELATION — see module header).
    let fast_trace = "Verizon-LTE-short.down";
    let slow_trace = "ATT-LTE-driving-2016.down";
    let fast_loss = derive_loss(&load_capacity(fast_trace));
    let slow_loss = derive_loss(&load_capacity(slow_trace));
    let (pf, qf) = ge_fit(&fast_loss);
    let (ps, qs) = ge_fit(&slow_loss);
    let ef = eps_of(&fast_loss);
    let es = eps_of(&slow_loss);

    // Same C8 rates/OWDs that produced the GE-based ×1.19 reference.
    let specs = vec![
        PathSpec { rate: sym_per_ms(100.0), owd: 5 },
        PathSpec { rate: sym_per_ms(20.0), owd: 20 },
    ];
    let k = 20_000usize;
    let (gen_size, inflight, r) = (640usize, 3usize, 0.10);

    // goodput ceiling with the REAL per-path loss rates.
    let gf = specs[0].rate * (1.0 - ef);
    let gs = specs[1].rate * (1.0 - es);
    let ceiling = (gf + gs) / gf;

    // fast-alone reference times (real & GE) — the denominator of the factor.
    let mut fa_real = GenOracle::new(
        vec![specs[0].clone()],
        vec![Loss::Real { seq: fast_loss.clone(), idx: 0 }],
        k, gen_size, inflight, r,
    );
    let t_fast_real = fa_real.run();

    let mut fa_ge = GenOracle::new(
        vec![specs[0].clone()],
        vec![Loss::Ge { p: pf, q: qf, bad: false, rng: ChaCha8Rng::seed_from_u64(1) }],
        k, gen_size, inflight, r,
    );
    let t_fast_ge = fa_ge.run();

    // dual-path (real per-path loss).
    let mut du_real = GenOracle::new(
        specs.clone(),
        vec![
            Loss::Real { seq: fast_loss.clone(), idx: 0 },
            Loss::Real { seq: slow_loss.clone(), idx: 0 },
        ],
        k, gen_size, inflight, r,
    );
    let t_dual_real = du_real.run();

    // dual-path GE control: same rates/OWDs, GE fitted to each path.
    let mut du_ge = GenOracle::new(
        specs.clone(),
        vec![
            Loss::Ge { p: pf, q: qf, bad: false, rng: ChaCha8Rng::seed_from_u64(2) },
            Loss::Ge { p: ps, q: qs, bad: false, rng: ChaCha8Rng::seed_from_u64(3) },
        ],
        k, gen_size, inflight, r,
    );
    let t_dual_ge = du_ge.run();

    let factor_real = t_fast_real as f64 / t_dual_real as f64;
    let factor_ge = t_fast_ge as f64 / t_dual_ge as f64;

    println!("\n=== PART 3: GENERATION-CODING AGGREGATION on REAL per-path traces ===");
    println!("  fast path: {fast_trace}  eps={:.1}%  (p={pf:.4} q={qf:.4})", ef * 100.0);
    println!("  slow path: {slow_trace}  eps={:.1}%  (p={ps:.4} q={qs:.4})", es * 100.0);
    println!("  rates/OWD: C8 (100 Mbps @5ms + 20 Mbps @20ms); goodput ceiling with real loss = ×{ceiling:.3}");
    println!("  {:<34} {:>10} {:>10} {:>9}", "config", "t_fast", "t_dual", "factor");
    println!("  {:<34} {:>10} {:>10} {:>8.3}×", "REAL per-path loss", t_fast_real, t_dual_real, factor_real);
    println!("  {:<34} {:>10} {:>10} {:>8.3}×", "GE control (fitted p,q)", t_fast_ge, t_dual_ge, factor_ge);
    println!(
        "\n  aggregation efficiency (factor / ceiling): real {:.3}, GE {:.3}",
        factor_real / ceiling,
        factor_ge / ceiling
    );
    println!("  GE-based reference from temporal_oracle.rs (C8 het): ×1.19 (at its own ×1.19 ceiling)");
    println!(
        "\n  VERDICT: stable-generation coding {} on REAL independent per-path dynamics: it aggregates\n\
         \x20 above the fast path (×{factor_real:.3} > 1) and tracks its GE control (×{factor_ge:.3}) and the\n\
         \x20 real goodput ceiling (×{ceiling:.3}).  Real per-path burst structure does NOT break the\n\
         \x20 aggregation mechanic.  (CORRELATION GAP: independent traces cannot test shared-\n\
         \x20 bottleneck correlated loss — that needs dual-radio hardware capture; open milestone.)",
        if factor_real > 1.0 { "HOLDS" } else { "FAILS" }
    );

    // The generation design must still aggregate on real per-path dynamics, and
    // do so about as efficiently as its GE control (real dynamics don't break
    // the mechanic).
    assert!(
        factor_real > 1.0,
        "generation coding must aggregate above the fast path on real traces: ×{factor_real:.3}"
    );
    assert!(
        factor_real <= ceiling + 0.05,
        "cannot exceed the real goodput ceiling ×{ceiling:.3}: got ×{factor_real:.3}"
    );
    assert!(
        (factor_real - factor_ge).abs() < 0.25,
        "real aggregation should track the GE control (real ×{factor_real:.3} vs GE ×{factor_ge:.3})"
    );
}
