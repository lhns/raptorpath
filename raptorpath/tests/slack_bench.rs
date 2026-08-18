//! COMPONENT BENCH — the EMISSION-SLACK term and the RESEQUENCING SPAN.
//!
//! Goal-gate "Emission-Slack Bench" (2026-08-09), GOAL "THREE TERMS, NO
//! CONSTANTS" phase 1.1 + 1.2. No CC, no scheduler, no transport, no tokio,
//! no VM: deterministic, seconds.
//!
//! ```text
//! cargo test --test slack_bench --release -- --ignored --nocapture
//! ```
//!
//! ## THE MODEL ERROR THIS ATTACKS
//!
//! The outstanding-data limit is modelled as ONE quantity (network flow
//! control ≈ rate × RTprop) but does THREE jobs:
//!
//!   1. NETWORK WINDOW      = path rate  × RTprop        (get an ack back)
//!   2. EMISSION SLACK      = emission rate × stall      (keep the wire fed
//!                                                        while the recovery
//!                                                        plane is blocked)
//!   3. RESEQUENCING SPAN   = fast-path rate × skew      (what the receiver
//!                                                        holds waiting on a
//!                                                        straggler)
//!
//! All three are Little's law — quantity = rate × time — and all three are
//! derivable A PRIORI from signals the engine already measures. There is no
//! coefficient in any of them. Job 2 wants the limit LARGE, job 3 wants it
//! SMALL; one knob, opposite demands, which is why results split by topology
//! (job 3 does not exist at N = 1, where skew is 0).
//!
//! ## THIS BENCH DOES NOT DISCOVER A LAW — IT TRIES TO KILL ONE
//!
//! The predicted numbers are written into the ledger BEFORE the bench runs
//! (goal-gate "Emission-Slack Bench", PRE-REGISTRATION; MEASUREMENT
//! DISCIPLINE 11 at component scale). Nothing here fits a coefficient, and
//! nothing here scans a parameter to see which value matches. Where measured
//! and predicted disagree the bench reports the RATIO and names the
//! MECHANISM that owns the gap — it never adjusts a term.
//!
//! ## THE CONTRACT SUPPLIES THE STALL — THERE IS NO STATISTIC TO CHOOSE
//!
//! [`contract_stall_us`] below. The stall a hole imposes on the in-order
//! frontier is already declared by (δ, ρ):
//!   * the shed-eligible share (1 − ρ) is bounded by the span law's own
//!     deadline D(δ) = min(b(δ)·RTprop, 2·RTprop) — `shed_deadline_us`;
//!   * the retained share ρ must actually be RECOVERED: RFC 9002 §6.1.2
//!     detection (9/8·srtt) plus one retransmit round trip (srtt).
//! Both terms are always computed and the answer is continuous in ρ — the
//! shipped rate law's shape, not a mode bit (CLAUDE.md).
//!
//! ## Env knobs (comma lists; every default stated)
//! ```text
//!   RWM_SB_RTPROP_MS 5,10,20,50,100,200
//!   RWM_SB_LOSS      0.001,0.01,0.026,0.05
//!   RWM_SB_PATTERN   uniform,ge
//!   RWM_SB_PATHS     1,2                 (2 ⇒ skew applied)
//!   RWM_SB_RATE      c1,c2,c3            emission-rate classes (below)
//!   RWM_SB_CLOCK     app,wire            THE recovery-clock argument
//!   RWM_SB_SEEDS     42,7
//!   RWM_SB_N         6000                source symbols per cell
//!   RWM_SB_S         the backlog grid (symbols)
//!   RWM_SB_RHO       1.0                 the retention contract
//!   RWM_SB_DELTA_B   0.5                 b(δ); b(Realtime) = ½
//!   RWM_SB_SKEW_MS   5                   inter-path one-way skew
//!   RWM_SB_SPAN_SKEW 0,1,2,5,10,20,40    the §1.2 skew sweep (ms)
//!   RWM_SB_NLIST     1500,3000,6000,12000  transfer-length sweep
//! ```
//!
//! ## WHAT THIS BENCH CANNOT SEE — stated up front
//!
//! * Everything `recovery_bench` cannot see, because it uses that driver
//!   verbatim as its stall source: no congestion control (a retransmit is
//!   never rate-limited ⇒ measured stalls are a LOWER bound), no FEC (ARQ
//!   only ⇒ an UPPER bound on holes wherever r > 0 would have covered
//!   them), no scheduler placement, no control-plane loss, and the store
//!   DWELL is an input rather than an emergent quantity.
//! * **The backlog→stall feedback.** The stall distribution is taken from an
//!   UNCONSTRAINED run and then replayed against each backlog S as a
//!   per-symbol residence delay. A smaller S changes ack timing, loss
//!   correlation and queue occupancy in the real engine; here it does not.
//!   The curve is therefore the OPEN-LOOP idle response.
//! * **The source is backlogged.** The producer always has a symbol ready
//!   and the wire serializes at the cell's rate. A δ-small, source-limited
//!   (realtime) emitter is a different regime and is not modelled.
//! * **One wire.** Retransmits are not charged wire time against the source,
//!   so the idle fraction is the SLACK-induced idle alone, not total wire
//!   utilization.
//! * **Memory.** The backlog is counted in symbols; the byte cost of a large
//!   S (the `WIN_STORE_MAX` ≈ 5 MB argument) is out of scope.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use raptorpath::net::{
    ThreeTermTerm, contract_stall_s, shed_deadline_us, tail_sweep_timeout_us,
    three_term_store_cap,
};

#[path = "common/recovery_model.rs"]
mod recovery_model;

use recovery_model::*;

// ───────────────────── THE THREE TERMS, DERIVED ─────────────────────────

/// TERM 1 — the NETWORK WINDOW (symbols): rate × RTprop. Little's law on the
/// wire: the outstanding needed to keep one path busy for one ack round
/// trip. This is the term the engine already models, and it is here only so
/// the other two are measured ON TOP of it rather than confused with it.
pub fn network_window(rate_sym_s: f64, rtprop_us: u64) -> f64 {
    rate_sym_s * (rtprop_us as f64 / 1e6)
}

/// The CONTRACT-declared stall (µs) that a hole imposes on the in-order
/// frontier, as a function of the declared (δ, ρ) — no statistic is chosen,
/// no coefficient is fitted, and the form is CONTINUOUS in ρ (both terms are
/// always computed; there is no mode bit — CLAUDE.md).
///
/// * The **shed-eligible share (1 − ρ)** is bounded by the span law's own
///   deadline `D(δ) = min(b(δ)·RTprop, 2·RTprop)` ([`shed_deadline_us`],
///   §16.20.3): past D a hole is retired rather than served, so it cannot
///   pin the frontier longer than D.
/// * The **retained share ρ** is not sheddable by construction
///   (RETAIN-UNTIL-ACKED), so the hole must actually be RECOVERED:
///   RFC 9002 §6.1.2 detection (9/8·srtt, `kTimeThreshold` — cited, not
///   magic) plus one retransmit round trip (srtt) = 17/8·srtt.
///
/// `srtt_us` is the HONEST path clock the contract can see (RTprop + the
/// standing wire queue), NOT the store-dwell-inclusive app-echo RTT. That
/// distinction is the whole point of the ratio table this bench produces.
pub fn contract_stall_us(rho: f64, b_hint: f64, rtprop_us: u64, srtt_us: u64) -> u64 {
    let rho = rho.clamp(0.0, 1.0);
    let shed_term = shed_deadline_us(b_hint, rtprop_us) as f64;
    let retain_term = (srtt_us as f64) * 9.0 / 8.0 + (srtt_us as f64);
    ((1.0 - rho) * shed_term + rho * retain_term) as u64
}

/// TERM 2 — the EMISSION SLACK (symbols): emission rate × the contract
/// stall. The backlog that keeps the wire busy across one frontier freeze.
pub fn emission_slack(rate_sym_s: f64, stall_us: u64) -> f64 {
    rate_sym_s * (stall_us as f64 / 1e6)
}

/// The FRONTIER-PINNED FRACTION κ — Little's law a THIRD time, now on the
/// in-order frontier itself. Stall EPISODES arrive at `ε̂·rate/B` per second
/// (a burst of mean length `B` costs ONE episode, and `B` is what §8.3's
/// σ²_burst already estimates), and each pins the frontier for the
/// contract's own `stall(δ, ρ)`. So the frontier is pinned
///
///     κ = min(1, (ε̂·rate/B) · stall)
///
/// of the time. It contains no coefficient — only quantities the estimator
/// already produces — and it supplies the ε̂ → 0 limit the uncorrected slack
/// term misses: with no holes there is nothing to be slack FOR, and the
/// required backlog must fall onto the network window alone.
pub fn frontier_pinned_fraction(rate_sym_s: f64, eps: f64, burst: f64, stall_us: u64) -> f64 {
    ((eps * rate_sym_s / burst.max(1.0)) * (stall_us as f64 / 1e6)).clamp(0.0, 1.0)
}

/// TERM 3 — the RESEQUENCING SPAN (symbols) the RECEIVER must hold.
///
/// Assumption, stated: the receiver must hold everything the FAST path
/// delivers that overtakes an in-flight SLOW-path symbol. A symbol placed on
/// the slow path at t arrives at t + owd_slow; every fast-path symbol sent in
/// [t, t + owd_slow − owd_fast] arrives before it. So the hold is
/// `rate_fast × (owd_slow − owd_fast)` — ONE-WAY delay, because the
/// receiver's in-order frontier is a receive-side object.
pub fn resequencing_span_recv(rate_fast_sym_s: f64, owd_skew_us: u64) -> f64 {
    rate_fast_sym_s * (owd_skew_us as f64 / 1e6)
}

/// TERM 3' — the same geometry seen by the SENDER'S RETENTION STORE, which
/// is the object the store cap actually bounds: the sender must retain a
/// symbol until it is ACKED, so the fast-path symbols retained while one
/// slow-path symbol is unacked span a full ROUND TRIP of skew:
/// `rate_fast × (RTprop_slow − RTprop_fast)` = twice [`resequencing_span_recv`]
/// for symmetric one-way delays. The tasking's formula is this one.
pub fn resequencing_span_store(rate_fast_sym_s: f64, rtprop_skew_us: u64) -> f64 {
    rate_fast_sym_s * (rtprop_skew_us as f64 / 1e6)
}

// ───────────────────── the emission-side simulator ──────────────────────

/// Wire-idle fraction at backlog `cap`, given each source symbol's measured
/// STORE RESIDENCE `d[i]` (send → release from the sender's retention store)
/// and the wire serialization time `g` of one symbol.
///
/// The producer is backlogged; the wire serializes at `g`; a symbol may be
/// emitted only while fewer than `cap` symbols are outstanding. Releases are
/// out of order (the shipped store drains by cumulative frontier AND by
/// SACK-implied delivered intervals), so the census is kept exactly with a
/// min-heap rather than assumed FIFO.
///
/// `d` is replayed as a per-symbol property — see the header's OPEN LOOP
/// boundary note.
pub fn wire_idle_fraction(d: &[u64], g: u64, cap: usize) -> f64 {
    if d.is_empty() || cap == 0 {
        return 1.0;
    }
    let mut pending: BinaryHeap<Reverse<u64>> = BinaryHeap::new();
    let mut prev_send: Option<u64> = None;
    let mut last_end: u64 = 0;
    let mut busy: u64 = 0;
    for &di in d {
        let mut t = prev_send.map(|p| p + g).unwrap_or(0);
        loop {
            while let Some(&Reverse(r)) = pending.peek() {
                if r <= t {
                    pending.pop();
                } else {
                    break;
                }
            }
            if pending.len() < cap {
                break;
            }
            // The store is full: the wire waits for the next release.
            let Reverse(r) = pending.pop().expect("cap > 0 ⇒ non-empty");
            t = t.max(r);
        }
        pending.push(Reverse(t.saturating_add(di)));
        prev_send = Some(t);
        busy += g;
        last_end = t + g;
    }
    if last_end == 0 {
        return 0.0;
    }
    1.0 - (busy as f64 / last_end as f64)
}

/// Per-symbol store residence `d[i]` (µs) from a recovery-plane run: the
/// interval a source symbol occupies the sender's retention store. A symbol
/// never released inside the driver's horizon is charged to the horizon.
pub fn residences(out: &Out) -> Vec<u64> {
    let horizon = out
        .store_release_us
        .iter()
        .filter_map(|r| *r)
        .max()
        .unwrap_or(0)
        .max(out.send_us.last().copied().unwrap_or(0));
    out.send_us
        .iter()
        .zip(out.store_release_us.iter())
        .map(|(&s, r)| r.unwrap_or(horizon).saturating_sub(s))
        .collect()
}

/// The MEASURED frontier stall (µs) per hole: how long the receiver's
/// in-order frontier was pinned that would not have been pinned had the
/// original arrived — `recv(repair) − (send + owd)`. This is the quantity
/// [`contract_stall_us`] predicts.
pub fn frontier_stalls(out: &Out, n_paths: usize) -> Vec<(Option<Chan>, u64)> {
    let mut v = Vec::new();
    for h in &out.holes {
        // The hole's seq is recoverable from its send time: the driver paces
        // originals at seq·tx_gap, so seq = lost_at_us / tx_gap.
        let seq = (h.lost_at_us / out.tx_gap_us.max(1)) as usize;
        if seq >= out.send_us.len() {
            continue;
        }
        let owd = out.owd_us[seq % n_paths.max(1)];
        if let Some(rx) = h.delivered_us {
            v.push((h.chan, rx.saturating_sub(h.lost_at_us + owd)));
        }
    }
    v
}

// ───────────────────────────── the axes ─────────────────────────────────

/// Emission-rate classes, at the ledger's own numbers: ~1.2 KB symbols.
/// c2 = 10 400 sym/s (the 100 Mbit cell), c3 = 2 000 sym/s (the 20 Mbit
/// slow path), c1 = 26 000 sym/s (the 1 Gbit single, measured 220–260
/// Mbit/s on the shipped default). These are the same constants
/// `tests/store_cap_bench.rs` uses.
const RATE_CLASSES: &[(&str, f64)] = &[("c1", 26_000.0), ("c2", 10_400.0), ("c3", 2_000.0)];

fn rate_of(name: &str) -> Option<f64> {
    RATE_CLASSES.iter().find(|(n, _)| *n == name).map(|(_, r)| *r)
}

fn mbps_of(rate_sym_s: f64) -> f64 {
    rate_sym_s * 1_200.0 * 8.0 / 1e6
}

fn qpct(v: &[u64], q: f64) -> u64 {
    if v.is_empty() {
        return 0;
    }
    let i = ((((v.len() - 1) as f64) * q).round() as usize).min(v.len() - 1);
    v[i]
}

/// The backlog grid, dense enough that a knee within one octave is visible.
const S_GRID: &[usize] = &[
    16, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512, 768, 1024, 1536, 2048, 3072, 4096, 6144, 8192,
    12288, 16384,
];

fn s_grid() -> Vec<usize> {
    let raw = env_str("RWM_SB_S", "");
    if raw.trim().is_empty() {
        S_GRID.to_vec()
    } else {
        raw.split(',').filter_map(|s| s.trim().parse().ok()).collect()
    }
}

/// The smallest grid point whose idle fraction is below `thresh`.
fn s_at(curve: &[(usize, f64)], thresh: f64) -> Option<usize> {
    curve.iter().find(|(_, i)| *i < thresh).map(|(s, _)| *s)
}

struct CellOut {
    rate_name: &'static str,
    rate: f64,
    rtprop_us: u64,
    loss: f64,
    pattern: Pattern,
    n_paths: usize,
    clock: Clock,
    /// The contract prediction, computed BEFORE the run.
    pred_window: f64,
    pred_stall_us: u64,
    pred_slack: f64,
    /// Named engine cadences, for attribution.
    patience_us: u64,
    pooled_us: u64,
    cooldown_us: u64,
    refresh_us: u64,
    sweep_us: u64,
    /// Measured.
    n_holes: usize,
    stall_us: Vec<u64>,
    by_chan: [(u64, Vec<u64>); 5],
    curve: Vec<(usize, f64)>,
    span_p50: u64,
    span_max: u64,
}

fn run_slack_cell(
    rate_name: &'static str,
    rate: f64,
    rtprop_ms: u64,
    loss: f64,
    pattern: Pattern,
    n_paths: usize,
    clock: Clock,
    seeds: &[u64],
    cal: Calib,
    rho: f64,
    b_hint: f64,
    grid: &[usize],
) -> CellOut {
    let rtprop_us = rtprop_ms * 1_000;
    // ── THE PREDICTION, computed before a single symbol is simulated ──
    let srtt_honest = rtprop_us + cal.wireq_us;
    let pred_window = network_window(rate, srtt_honest);
    let pred_stall_us = contract_stall_us(rho, b_hint, rtprop_us, srtt_honest);
    let pred_slack = emission_slack(rate, pred_stall_us);

    let mut stall_us: Vec<u64> = Vec::new();
    let mut by_chan: [(u64, Vec<u64>); 5] =
        [(0, vec![]), (0, vec![]), (0, vec![]), (0, vec![]), (0, vec![])];
    let mut spans: Vec<u64> = Vec::new();
    // The PREDICTED point is added to this cell's grid so `idle@pred` is read
    // AT the prediction rather than at the next grid point above it — the
    // rounding would otherwise bias PS1 toward passing by up to one grid step.
    let grid: Vec<usize> = {
        let mut g = grid.to_vec();
        g.push((pred_window + pred_slack).ceil() as usize);
        g.sort_unstable();
        g.dedup();
        g
    };
    let grid = &grid[..];
    let mut curve_acc: Vec<(usize, f64)> = grid.iter().map(|&s| (s, 0.0)).collect();
    let mut n_holes = 0usize;
    let (mut patience_us, mut pooled_us, mut cooldown_us, mut refresh_us) = (0, 0, 0, 0);

    for &seed in seeds {
        let cell = Cell {
            rtprop_us,
            loss,
            pattern,
            n_paths,
            clock,
            arm: ARMS[0], // `shipped`: RWM_RECOV_MP on, the default stack
            seed,
        };
        let out = run_cell(cell, cal);
        patience_us = out.patience_us;
        pooled_us = out.pooled_us;
        cooldown_us = out.cooldown_us;
        refresh_us = out.refresh_us;
        n_holes += out.holes.len();
        for (chan, s) in frontier_stalls(&out, n_paths) {
            stall_us.push(s);
            if let Some(c) = chan {
                let i = CHANS.iter().position(|&x| x == c).unwrap();
                by_chan[i].0 += 1;
                by_chan[i].1.push(s);
            }
        }
        spans.extend_from_slice(&out.span_samples);
        let d = residences(&out);
        for (k, &s) in grid.iter().enumerate() {
            curve_acc[k].1 += wire_idle_fraction(&d, out.tx_gap_us, s);
        }
    }
    let n = seeds.len().max(1) as f64;
    for e in curve_acc.iter_mut() {
        e.1 /= n;
    }
    stall_us.sort_unstable();
    for c in by_chan.iter_mut() {
        c.1.sort_unstable();
    }
    spans.sort_unstable();

    CellOut {
        rate_name,
        rate,
        rtprop_us,
        loss,
        pattern,
        n_paths,
        clock,
        pred_window,
        pred_stall_us,
        pred_slack,
        patience_us,
        pooled_us,
        cooldown_us,
        refresh_us,
        sweep_us: tail_sweep_timeout_us(pooled_us),
        n_holes,
        stall_us,
        by_chan,
        curve: curve_acc,
        span_p50: qpct(&spans, 0.5),
        span_max: spans.last().copied().unwrap_or(0),
    }
}

// ───────────────────────────── the bench ────────────────────────────────

#[test]
#[ignore]
fn slack_bench() {
    let mut cal = Calib::from_env();
    cal.n_src = env_u64("RWM_SB_N", 6_000);
    cal.skew_us = env_u64("RWM_SB_SKEW_MS", 5) * 1_000;
    let rtprops = list_u64("RWM_SB_RTPROP_MS", "5,10,20,50,100,200");
    let losses = list_f64("RWM_SB_LOSS", "0.001,0.01,0.026,0.05");
    let patterns: Vec<Pattern> = list_str("RWM_SB_PATTERN", "uniform,ge")
        .iter()
        .map(|s| if s == "ge" { Pattern::Ge } else { Pattern::Uniform })
        .collect();
    let paths = list_u64("RWM_SB_PATHS", "1,2");
    let clocks: Vec<Clock> = list_str("RWM_SB_CLOCK", "app,wire")
        .iter()
        .map(|s| if s == "wire" { Clock::Wire } else { Clock::App })
        .collect();
    let rate_names = list_str("RWM_SB_RATE", "c1,c2,c3");
    let seeds = list_u64("RWM_SB_SEEDS", "42,7");
    let rho = env_f64("RWM_SB_RHO", 1.0);
    let b_hint = env_f64("RWM_SB_DELTA_B", 0.5);
    let grid = s_grid();
    let t0 = std::time::Instant::now();

    println!("\n=== EMISSION-SLACK COMPONENT BENCH (goal-gate \"Emission-Slack Bench\") ===");
    println!(
        "N={} src · wireQ {} ms · dwell {} ms · skew {} ms · seeds {:?} · ρ={} · b(δ)={}",
        cal.n_src,
        cal.wireq_us / 1000,
        cal.dwell_us / 1000,
        cal.skew_us / 1000,
        seeds,
        rho,
        b_hint
    );
    println!(
        "THE THREE TERMS (a priori, no fitted coefficient):\n  \
         window = rate×RTprop · slack = rate×stall(δ,ρ) · span = rate_fast×skew\n  \
         stall(δ,ρ) = (1−ρ)·D(δ) + ρ·(9/8·srtt + srtt), srtt = RTprop + wireQ (HONEST clock)"
    );

    let mut cells: Vec<CellOut> = Vec::new();
    for rn in &rate_names {
        let Some(rate) = rate_of(rn) else { continue };
        let name: &'static str =
            RATE_CLASSES.iter().find(|(n, _)| n == rn).map(|(n, _)| *n).unwrap();
        cal.mbps = mbps_of(rate);
        for &rtprop_ms in &rtprops {
            for &loss in &losses {
                for &pattern in &patterns {
                    for &np in &paths {
                        for &clock in &clocks {
                            cells.push(run_slack_cell(
                                name,
                                rate,
                                rtprop_ms,
                                loss,
                                pattern,
                                np as usize,
                                clock,
                                &seeds,
                                cal,
                                rho,
                                b_hint,
                                &grid,
                            ));
                        }
                    }
                }
            }
        }
    }

    // ── (1) THE RATIO TABLE — measured stall vs the contract's own ──
    println!(
        "\n=== (1) THE RATIO TABLE: measured frontier stall vs the CONTRACT's declared stall ===\n\
         A ratio > 1 is not a wrong constant — it is a MISSING MECHANISM. The named\n\
         cadences are printed alongside so the owner of the gap can be read off.\n"
    );
    println!(
        "{:>4} {:>4} {:>6} {:>5} {:>3} {:>5} | {:>7} | {:>7} {:>7} {:>7} {:>8} | {:>6} {:>6} | {:>8} {:>8} {:>8} {:>8}",
        "rate", "rtp", "loss", "pat", "np", "clk",
        "pred ms",
        "p50 ms", "p90", "p99", "max",
        "p50/pr", "p90/pr",
        "patience", "cooldwn", "sweep", "refresh"
    );
    for c in &cells {
        if c.stall_us.is_empty() {
            continue;
        }
        let pr = c.pred_stall_us.max(1) as f64;
        println!(
            "{:>4} {:>4} {:>6.3} {:>5} {:>3} {:>5} | {:>7.1} | {:>7.1} {:>7.1} {:>7.1} {:>8.1} | {:>6.2} {:>6.2} | {:>8.1} {:>8.1} {:>8.1} {:>8.1}",
            c.rate_name, c.rtprop_us / 1000, c.loss, c.pattern.tag(), c.n_paths, c.clock.tag(),
            ms(c.pred_stall_us),
            ms(qpct(&c.stall_us, 0.50)), ms(qpct(&c.stall_us, 0.90)),
            ms(qpct(&c.stall_us, 0.99)), ms(*c.stall_us.last().unwrap_or(&0)),
            qpct(&c.stall_us, 0.50) as f64 / pr, qpct(&c.stall_us, 0.90) as f64 / pr,
            ms(c.patience_us), ms(c.cooldown_us), ms(c.sweep_us), ms(c.refresh_us)
        );
    }

    // ── (2) THE STALL, BY THE CHANNEL THAT OWNS IT ──
    println!(
        "\n=== (2) WHICH MECHANISM OWNS THE STALL (the driver's own channel label) ===\n\
         No attribution is inferred: `recovery_bench`'s driver records which channel\n\
         ADMITTED each hole's first service. p50 stall per channel, ms.\n"
    );
    println!(
        "{:>4} {:>4} {:>6} {:>5} {:>3} {:>5} | {:>7} | {}",
        "rate", "rtp", "loss", "pat", "np", "clk", "pred ms", "per-channel n / p50 ms"
    );
    for c in &cells {
        if c.stall_us.is_empty() {
            continue;
        }
        let parts: Vec<String> = CHANS
            .iter()
            .enumerate()
            .filter(|(i, _)| c.by_chan[*i].0 > 0)
            .map(|(i, ch)| {
                format!("{} {}/{:.1}", ch.tag(), c.by_chan[i].0, ms(qpct(&c.by_chan[i].1, 0.5)))
            })
            .collect();
        println!(
            "{:>4} {:>4} {:>6.3} {:>5} {:>3} {:>5} | {:>7.1} | {}",
            c.rate_name, c.rtprop_us / 1000, c.loss, c.pattern.tag(), c.n_paths, c.clock.tag(),
            ms(c.pred_stall_us),
            parts.join("  ")
        );
    }

    // ── (3) THE WIRE-IDLE CURVE vs BACKLOG ──
    println!(
        "\n=== (3) WIRE-IDLE FRACTION vs BACKLOG S — the curve, and the predicted point ===\n\
         `pred S` = window + slack = rate×RTprop + rate×stall(δ,ρ), computed a priori.\n\
         `idle@pred` is the bench's verdict ON that number. S1%/S0.1% are the smallest\n\
         GRID points reaching those idle fractions — reported, never fitted.\n"
    );
    println!(
        "{:>4} {:>4} {:>6} {:>5} {:>3} {:>5} | {:>8} {:>8} {:>9} | {:>9} {:>7} | {:>7} {:>7} | {}",
        "rate", "rtp", "loss", "pat", "np", "clk",
        "window", "slack", "pred S",
        "idle@pred", "S 1%", "S .1%", "ratio",
        "idle at S = 64 / 256 / 1024 / 4096 / 16384"
    );
    for c in &cells {
        let pred_s = (c.pred_window + c.pred_slack).ceil() as usize;
        // `pred_s` is in this cell's grid by construction, so this is the idle
        // fraction AT the prediction — no interpolation, no rounding up.
        let idle_at = |s: usize| -> f64 {
            c.curve.iter().find(|(g, _)| *g >= s).map(|(_, i)| *i).unwrap_or(0.0)
        };
        let s1 = s_at(&c.curve, 0.01);
        let s01 = s_at(&c.curve, 0.001);
        let pick = |s: usize| c.curve.iter().find(|(g, _)| *g == s).map(|(_, i)| *i);
        println!(
            "{:>4} {:>4} {:>6.3} {:>5} {:>3} {:>5} | {:>8.0} {:>8.0} {:>9} | {:>8.2}% {:>7} | {:>7} {:>7.2} | {}",
            c.rate_name, c.rtprop_us / 1000, c.loss, c.pattern.tag(), c.n_paths, c.clock.tag(),
            c.pred_window, c.pred_slack, pred_s,
            100.0 * idle_at(pred_s),
            s1.map(|v| v.to_string()).unwrap_or_else(|| ">grid".into()),
            s01.map(|v| v.to_string()).unwrap_or_else(|| ">grid".into()),
            s1.map(|v| v as f64 / pred_s.max(1) as f64).unwrap_or(f64::NAN),
            [64usize, 256, 1024, 4096, 16384]
                .iter()
                .map(|&s| pick(s).map(|i| format!("{:.2}%", 100.0 * i)).unwrap_or("-".into()))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }

    // ── (4) THE SHAPE VERDICT ──
    println!(
        "\n=== (4) THE SHAPE: knee or slope? ===\n\
         `octaves` = log2(S at 10% of the small-S idle ÷ S at 90% of it): the width of\n\
         the transition. A KNEE is ≲1 octave; a SLOPE is ≳3. Reported per cell, not fitted.\n"
    );
    println!(
        "{:>4} {:>4} {:>6} {:>5} {:>3} {:>5} | {:>8} {:>8} {:>8} | {:>8} | {}",
        "rate", "rtp", "loss", "pat", "np", "clk", "idle@min", "S@90%", "S@10%", "octaves", "shape"
    );
    for c in &cells {
        let i0 = c.curve.first().map(|(_, i)| *i).unwrap_or(0.0);
        if i0 <= 1e-9 {
            continue;
        }
        let s90 = c.curve.iter().find(|(_, i)| *i <= 0.90 * i0).map(|(s, _)| *s);
        let s10 = c.curve.iter().find(|(_, i)| *i <= 0.10 * i0).map(|(s, _)| *s);
        let oct = match (s90, s10) {
            (Some(a), Some(b)) if a > 0 => (b as f64 / a as f64).log2(),
            _ => f64::NAN,
        };
        let shape = if oct.is_nan() {
            "unresolved (>grid)"
        } else if oct <= 1.2 {
            "KNEE"
        } else if oct >= 3.0 {
            "SLOPE"
        } else {
            "soft knee"
        };
        println!(
            "{:>4} {:>4} {:>6.3} {:>5} {:>3} {:>5} | {:>7.2}% {:>8} {:>8} | {:>8.2} | {}",
            c.rate_name, c.rtprop_us / 1000, c.loss, c.pattern.tag(), c.n_paths, c.clock.tag(),
            100.0 * i0,
            s90.map(|v| v.to_string()).unwrap_or_else(|| ">grid".into()),
            s10.map(|v| v.to_string()).unwrap_or_else(|| ">grid".into()),
            oct,
            shape
        );
    }

    // ── (5) DOES THE REQUIRED S DEPEND ON THE TRANSFER LENGTH? ──
    //
    // If it does, the term is NOT `rate × <stall statistic>` — the required
    // backlog would then depend on something outside {rate, stall
    // distribution}, and that is a first-class finding.
    println!(
        "\n=== (5) TRANSFER-LENGTH DEPENDENCE (the term's own falsification) ===\n\
         rate × stall carries no N. If S(1%) grows with N, the slack term is\n\
         incomplete in its stated variables. c2, RTprop 10 ms, GE 2.6%, np 2, app clock.\n"
    );
    println!("{:>8} | {:>9} {:>9} {:>9} | {:>10}", "N", "idle@64", "idle@1024", "S 1%", "max stall ms");
    for n in list_u64("RWM_SB_NLIST", "1500,3000,6000,12000") {
        let mut c2 = cal;
        c2.n_src = n;
        c2.mbps = mbps_of(10_400.0);
        let c = run_slack_cell(
            "c2", 10_400.0, 10, 0.026, Pattern::Ge, 2, Clock::App, &seeds, c2, rho, b_hint, &grid,
        );
        let pick = |s: usize| c.curve.iter().find(|(g, _)| *g == s).map(|(_, i)| *i).unwrap_or(0.0);
        println!(
            "{:>8} | {:>8.2}% {:>8.2}% {:>9} | {:>10.1}",
            n,
            100.0 * pick(64),
            100.0 * pick(1024),
            s_at(&c.curve, 0.01).map(|v| v.to_string()).unwrap_or_else(|| ">grid".into()),
            ms(*c.stall_us.last().unwrap_or(&0))
        );
    }

    // ── (6) §1.2 — THE RESEQUENCING SPAN, PREDICTED THEN MEASURED ──
    println!(
        "\n=== (6) THE RESEQUENCING SPAN — prediction first, measurement second ===\n\
         span_recv = rate_fast × (owd_slow − owd_fast). At np = 2 the driver splits the\n\
         source alternately, so rate_fast = rate/2 and owd_skew = the cell's skew.\n\
         Measured at LOSS = 0 so the span is the SKEW term alone.\n"
    );
    println!(
        "{:>4} {:>4} {:>6} | {:>10} {:>10} | {:>9} {:>9} | {:>7}",
        "rate", "rtp", "skew", "pred recv", "pred store", "meas p50", "meas max", "max/pr"
    );
    for rn in &rate_names {
        let Some(rate) = rate_of(rn) else { continue };
        for skew_ms in list_u64("RWM_SB_SPAN_SKEW", "0,1,2,5,10,20,40") {
            let mut sc = cal;
            sc.mbps = mbps_of(rate);
            sc.skew_us = skew_ms * 1_000;
            let cell = Cell {
                rtprop_us: 10_000,
                loss: 0.0,
                pattern: Pattern::Uniform,
                n_paths: 2,
                clock: Clock::App,
                arm: ARMS[0],
                seed: 42,
            };
            let out = run_cell(cell, sc);
            let mut spans = out.span_samples.clone();
            spans.sort_unstable();
            let pred_recv = resequencing_span_recv(rate / 2.0, sc.skew_us);
            let pred_store = resequencing_span_store(rate / 2.0, 2 * sc.skew_us);
            let mx = spans.last().copied().unwrap_or(0);
            println!(
                "{:>4} {:>4} {:>6} | {:>10.0} {:>10.0} | {:>9} {:>9} | {:>7.2}",
                rn, 10, skew_ms, pred_recv, pred_store,
                qpct(&spans, 0.5), mx,
                mx as f64 / pred_recv.max(1.0)
            );
        }
    }

    // ── (7) THE c8 GEOMETRY, ARITHMETIC ──
    println!(
        "\n=== (7) THE c8 GEOMETRY — the span term applied to the cell that failed ===\n"
    );
    let (rf, rtp_f) = (10_400.0f64, 8_000u64); // c2 path
    let rtp_s = 60_000u64; // c3 path
    let d_owd = (rtp_s - rtp_f) / 2;
    let span_recv = resequencing_span_recv(rf, d_owd);
    let span_store = resequencing_span_store(rf, rtp_s - rtp_f);
    println!(
        "  c8 = c2 + c3: rate_fast {:.0} sym/s · RTprop_fast {} ms · RTprop_slow {} ms",
        rf,
        rtp_f / 1000,
        rtp_s / 1000
    );
    println!("  predicted receiver-hold span   = rate_fast × Δowd    = {span_recv:.0} symbols");
    println!("  predicted sender-retention span = rate_fast × ΔRTprop = {span_store:.0} symbols");
    println!(
        "  LEDGER (\"Store-Cap Triplication\", \"Anchor Hygiene\", `honest_store_cap` doc):\n\
     \x20   honest pooled cap at c8 with one path filtered .......... 480–500\n\
     \x20   the guard session's independently measured GOOD pin ..... 508\n\
     \x20   the arm that read −19.6% at seed 7 (cap pinned N×knee) ... 4096\n\
     \x20   the `store_boot_cap` cliff the filter supplied ........... 128"
    );

    println!("\n{} cells in {:.2} s", cells.len(), t0.elapsed().as_secs_f64());
}

// ═══════════ PHASE 1.3 — IS THE COVERAGE DERIVABLE? (goal-gate
// "Coverage: derivable or not") ══════════════════════════════════════════
//
// §16.43 left a split verdict: the stall TIME is derived from (δ, ρ), the
// COVERAGE — where on a ~3-octave slope to sit — was asserted NOT derivable
// and was offered as evidence for a FOURTH contract term. This section is
// the adversarial attack on that assertion. Nothing below fits a
// coefficient; every quantity is either measured or arithmetic.

/// The mean SENDER-STORE OCCUPANCY of the UNCONSTRAINED run, in symbols:
/// Little's law read on the retention store rather than on the wire.
///
/// `Σ residence ÷ (N · g)` — the time-average number of symbols resident
/// while the source emits one symbol every `g`. This is `S*`, and the claim
/// of phase 1.3 is that it is the RIGHT ENDPOINT of the idle slope: the
/// open-loop idle curve is the hyperbola `1 − S/S*`, so the "which point on
/// the slope" question has an arithmetic answer and not a policy one.
pub fn mean_occupancy(d: &[u64], g: u64) -> f64 {
    if d.is_empty() || g == 0 {
        return 0.0;
    }
    let sum: f64 = d.iter().map(|&x| x as f64).sum();
    sum / (d.len() as f64 * g as f64)
}

/// The smallest backlog whose open-loop wire idle is strictly below
/// `target`, found by bisection on the (monotone non-increasing) curve —
/// so the answer is exact in symbols rather than rounded to an octave grid.
/// `None` if `hi` itself does not reach the target.
pub fn s_for_idle(d: &[u64], g: u64, target: f64, hi: usize) -> Option<usize> {
    if wire_idle_fraction(d, g, hi) >= target {
        return None;
    }
    let (mut lo, mut hi) = (1usize, hi);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if wire_idle_fraction(d, g, mid) < target {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    Some(lo)
}

/// The idle fraction the LITTLE'S-LAW HYPERBOLA predicts at backlog `s`,
/// given the run's own mean occupancy `s_star`: a saturated store of `cap`
/// symbols with mean residence `W` admits `cap/W` symbols per second, so
/// the wire (which wants `1/g`) runs at `min(1, s·g/W)` of its rate.
/// Contains no free parameter: `s_star = W/g`.
pub fn hyperbola_idle(s: usize, s_star: f64) -> f64 {
    if s_star <= 0.0 {
        return 0.0;
    }
    (1.0 - (s as f64) / s_star).max(0.0)
}

/// The wire-idle targets the coverage question is asked at. The span from
/// 30 % to 0.1 % is three decades of "how much wire may go idle" — the whole
/// range a fourth contract dial could possibly be asked to express.
const IDLE_TARGETS: [f64; 6] = [0.30, 0.10, 0.03, 0.01, 0.003, 0.001];
const IDLE_TAGS: [&str; 6] = ["S 30%", "S 10%", "S 3%", "S 1%", "S .3%", "S .1%"];

/// Keep the LARGER of two thresholds: a cover has to hold at every seed, and
/// averaging two thresholds would report a number neither seed produced.
fn worst(acc: &mut Option<usize>, v: Option<usize>) {
    if let Some(b) = v {
        *acc = Some(acc.map_or(b, |a| a.max(b)));
    }
}

struct CovRow {
    rate_name: &'static str,
    rtprop_us: u64,
    loss: f64,
    pattern: Pattern,
    n_paths: usize,
    clock: Clock,
    pred_s: f64,
    /// Measured mean store occupancy = the hyperbola's zero.
    s_star: f64,
    /// Max |measured − hyperbola| over the starved half of the curve.
    hyp_dev: f64,
    /// The smallest backlog reaching each of [`IDLE_TARGETS`].
    st: Vec<Option<usize>>,
    /// Measured wire idle AT S* and AT the §16.43 a-priori prediction.
    idle_star: f64,
    idle_pred: f64,
    /// Mean store residence — the quantity `s_star` is Little's law over.
    mean_d_us: f64,
}

fn run_cov_cell(
    rate_name: &'static str,
    rate: f64,
    rtprop_ms: u64,
    loss: f64,
    pattern: Pattern,
    n_paths: usize,
    clock: Clock,
    seeds: &[u64],
    cal: Calib,
    rho: f64,
    b_hint: f64,
) -> CovRow {
    let rtprop_us = rtprop_ms * 1_000;
    let srtt_honest = rtprop_us + cal.wireq_us;
    let pred_s = network_window(rate, srtt_honest)
        + emission_slack(rate, contract_stall_us(rho, b_hint, rtprop_us, srtt_honest));

    // Every threshold is bisected against a COMMON, generous ceiling and
    // then taken as the WORST seed — a cover has to hold at both, and
    // averaging two thresholds would manufacture a number neither seed
    // produced (and, with a per-seed ceiling, could invert their order).
    const HI: usize = 1 << 18;
    let (mut s_star, mut hyp_dev, mut mean_d) = (0.0f64, 0.0f64, 0.0f64);
    let mut st: Vec<Option<usize>> = vec![None; IDLE_TARGETS.len()];
    let (mut idle_star, mut idle_pred) = (0.0f64, 0.0f64);
    for &seed in seeds {
        let cell = Cell { rtprop_us, loss, pattern, n_paths, clock, arm: ARMS[0], seed };
        let out = run_cell(cell, cal);
        let d = residences(&out);
        let g = out.tx_gap_us;
        let ss = mean_occupancy(&d, g);
        s_star += ss;
        // The hyperbola is a SATURATED-store statement, so it is checked
        // where the store is saturated: the starved half, S ≤ 0.7·S*.
        let mut dev = 0.0f64;
        for k in 1..=7 {
            let s = ((k as f64 / 10.0) * ss).round().max(1.0) as usize;
            dev = dev.max((wire_idle_fraction(&d, g, s) - hyperbola_idle(s, ss)).abs());
        }
        hyp_dev = hyp_dev.max(dev);
        for (k, &tgt) in IDLE_TARGETS.iter().enumerate() {
            worst(&mut st[k], s_for_idle(&d, g, tgt, HI));
        }
        idle_star = idle_star.max(wire_idle_fraction(&d, g, ss.round().max(1.0) as usize));
        idle_pred = idle_pred.max(wire_idle_fraction(&d, g, pred_s.ceil().max(1.0) as usize));
        mean_d += d.iter().map(|&x| x as f64).sum::<f64>() / d.len() as f64;
    }
    let n = seeds.len().max(1) as f64;
    CovRow {
        rate_name,
        rtprop_us,
        loss,
        pattern,
        n_paths,
        clock,
        pred_s,
        s_star: s_star / n,
        hyp_dev,
        st,
        idle_star,
        idle_pred,
        mean_d_us: mean_d / n,
    }
}

// ── ROUTE B: CLOSING THE LOOP ───────────────────────────────────────────
//
// §16.43's largest stated boundary is that the bench is OPEN LOOP: it
// replays the store residences of an UNCONSTRAINED run against a
// constrained backlog. But the residence is not an exogenous property of
// the plane — it is what the store DOES, and the estimator's app-echo RTT
// reads it back as the store DWELL (`Calib::dwell_us`, an INPUT at 144 ms
// in phase 1.1/1.2). That is a loop:
//
//   backlog S ──> store dwell ──> app-echo srtt ──> §6.1.2 patience
//        ^                                               │
//        └────────── residence ── frontier stall ────────┘
//
// Little's law CLOSES it with no free parameter. A store bounded at S
// symbols whose departures run at the rate the wire ACHIEVES — one symbol
// per `g` less the idle the backlog itself causes — sustains a mean
// residence of at most S·g/(1 − idle). So the dwell the estimator reads
// obeys the self-map
//
//        dwell  =  min( E[residence | dwell] ,  S·g / (1 − idle(dwell, S)) ).
//
// [`closed_loop_dwell`] iterates it to its fixed point. Note the second
// argument is DESTABILIZING — idling raises the sustainable dwell, which
// raises the patience, which lengthens the stall — so this is the honest
// version of the loop and not a convenient one. On the WIRE clock the loop
// gain is identically ZERO (the dwell is excluded from the estimator's
// argument by construction): the honest clock is the loop-OPENING argument.

/// The self-consistent (dwell, trajectory, residence series, idle) at
/// backlog `s`. The residence series is the one measured AT the fixed-point
/// dwell; `idle` is the wire idle it produces at that same `s`.
fn closed_loop_dwell(
    cell: Cell,
    cal: Calib,
    s: usize,
    iters: usize,
) -> (u64, Vec<u64>, Vec<u64>, f64) {
    let mut c = cal;
    let mut traj: Vec<u64> = Vec::new();
    let mut d: Vec<u64> = Vec::new();
    let (mut dwell, mut idle) = (0u64, 0.0f64);
    for _ in 0..iters.max(1) {
        c.dwell_us = dwell;
        let out = run_cell(cell, c);
        d = residences(&out);
        let g = out.tx_gap_us;
        idle = wire_idle_fraction(&d, g, s);
        let mean_d = d.iter().map(|&x| x as f64).sum::<f64>() / d.len().max(1) as f64;
        let sustainable = (s as f64) * (g as f64) / (1.0 - idle).max(1e-6);
        // 10 s: the driver's own virtual horizon is 60 s past the last send,
        // so a dwell beyond this is a DIVERGED loop, not a fixed point.
        let next = mean_d.min(sustainable).min(10e6) as u64;
        traj.push(next);
        let done = next.abs_diff(dwell) * 1000 <= dwell.max(1);
        dwell = next;
        if done {
            break;
        }
    }
    (dwell, traj, d, idle)
}

fn quant(v: &mut Vec<f64>, q: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((((v.len() - 1) as f64) * q).round() as usize).min(v.len() - 1)]
}

#[test]
#[ignore]
fn coverage_bench() {
    let mut cal = Calib::from_env();
    cal.n_src = env_u64("RWM_SB_N", 6_000);
    cal.skew_us = env_u64("RWM_SB_SKEW_MS", 5) * 1_000;
    let rtprops = list_u64("RWM_SB_RTPROP_MS", "5,10,20,50,100,200");
    let losses = list_f64("RWM_SB_LOSS", "0.001,0.01,0.026,0.05");
    let patterns: Vec<Pattern> = list_str("RWM_SB_PATTERN", "uniform,ge")
        .iter()
        .map(|s| if s == "ge" { Pattern::Ge } else { Pattern::Uniform })
        .collect();
    let paths = list_u64("RWM_SB_PATHS", "1,2");
    let clocks: Vec<Clock> = list_str("RWM_SB_CLOCK", "app,wire")
        .iter()
        .map(|s| if s == "wire" { Clock::Wire } else { Clock::App })
        .collect();
    let rate_names = list_str("RWM_SB_RATE", "c1,c2,c3");
    let seeds = list_u64("RWM_SB_SEEDS", "42,7");
    let rho = env_f64("RWM_SB_RHO", 1.0);
    let b_hint = env_f64("RWM_SB_DELTA_B", 0.5);
    let t0 = std::time::Instant::now();

    println!("\n=== PHASE 1.3 — COVERAGE: DERIVABLE OR NOT (goal-gate \"Coverage: derivable or not\") ===");
    println!(
        "S* = mean store occupancy of the UNCONSTRAINED run = Σd ÷ (N·g) — Little's law\n\
         on the RETENTION STORE. The claim under test: open-loop idle(S) = 1 − S/S*, so the\n\
         'coverage point on a 3-octave slope' is the arithmetic S* and not a policy dial.\n"
    );

    let mut rows: Vec<CovRow> = Vec::new();
    for rn in &rate_names {
        let Some(rate) = rate_of(rn) else { continue };
        let name: &'static str =
            RATE_CLASSES.iter().find(|(n, _)| n == rn).map(|(n, _)| *n).unwrap();
        cal.mbps = mbps_of(rate);
        for &rtprop_ms in &rtprops {
            for &loss in &losses {
                for &pattern in &patterns {
                    for &np in &paths {
                        for &clock in &clocks {
                            rows.push(run_cov_cell(
                                name,
                                rate,
                                rtprop_ms,
                                loss,
                                pattern,
                                np as usize,
                                clock,
                                &seeds,
                                cal,
                                rho,
                                b_hint,
                            ));
                        }
                    }
                }
            }
        }
    }

    let f = |o: Option<usize>| o.map(|v| v.to_string()).unwrap_or_else(|| ">2^18".into());
    println!(
        "{:>4} {:>4} {:>6} {:>5} {:>3} {:>5} | {:>9} {:>9} {:>6} | {:>7} | {} | {:>8} {:>8} | {:>9}",
        "rate", "rtp", "loss", "pat", "np", "clk",
        "pred S", "S*", "S*/pr",
        "hypdev",
        IDLE_TAGS.iter().map(|t| format!("{t:>8}")).collect::<Vec<_>>().join(" "),
        "idle@S*", "idle@pr",
        "mean d ms"
    );
    for r in &rows {
        println!(
            "{:>4} {:>4} {:>6.3} {:>5} {:>3} {:>5} | {:>9.0} {:>9.0} {:>6.2} | {:>7.4} | {} | {:>7.2}% {:>7.2}% | {:>9.2}",
            r.rate_name, r.rtprop_us / 1000, r.loss, r.pattern.tag(), r.n_paths, r.clock.tag(),
            r.pred_s, r.s_star, r.s_star / r.pred_s.max(1.0),
            r.hyp_dev,
            r.st.iter().map(|o| format!("{:>8}", f(*o))).collect::<Vec<_>>().join(" "),
            100.0 * r.idle_star, 100.0 * r.idle_pred,
            r.mean_d_us / 1000.0
        );
    }

    // The index of the 1 % target inside IDLE_TARGETS — the reference point
    // §16.43 reported against.
    const I1: usize = 3;

    // ── SUMMARY (1): the hyperbola identity ──
    for clk in [Clock::App, Clock::Wire] {
        let mut dev: Vec<f64> =
            rows.iter().filter(|r| r.clock == clk).map(|r| r.hyp_dev).collect();
        if dev.is_empty() {
            continue;
        }
        println!(
            "\n(1) HYPERBOLA IDENTITY [{}]  max|measured idle − (1 − S/S*)| over S ≤ 0.7·S*:\n    \
             p50 {:.4}  p90 {:.4}  p99 {:.4}  max {:.4}   ({} cells)",
            clk.tag(),
            quant(&mut dev.clone(), 0.5),
            quant(&mut dev.clone(), 0.9),
            quant(&mut dev.clone(), 0.99),
            quant(&mut dev, 1.0),
            dev.len()
        );
    }

    // ── SUMMARY (2): how wide is the "coverage" choice, really? ──
    println!(
        "\n(2) THE WIDTH OF THE COVERAGE CHOICE — S at each target ÷ S(1%), bisected in\n    \
         SYMBOLS rather than read off an octave grid. This is the entire dynamic range a\n    \
         fourth 'emission-continuity' dial could have.\n"
    );
    println!("{:>5} | {:>8} {:>8} {:>8} {:>8}", "clk", "target", "p50", "p90", "max");
    for clk in [Clock::App, Clock::Wire] {
        for (k, tag) in IDLE_TAGS.iter().enumerate() {
            let v: Vec<f64> = rows
                .iter()
                .filter(|r| r.clock == clk)
                .filter_map(|r| match (r.st[k], r.st[I1]) {
                    (Some(a), Some(b)) => Some(a as f64 / b as f64),
                    _ => None,
                })
                .collect();
            if v.is_empty() {
                continue;
            }
            println!(
                "{:>5} | {:>8} {:>8.3} {:>8.3} {:>8.3}",
                clk.tag(),
                tag,
                quant(&mut v.clone(), 0.5),
                quant(&mut v.clone(), 0.9),
                quant(&mut v.clone(), 1.0)
            );
        }
    }

    // ── SUMMARY (3): the measured requirement against the a-priori term ──
    println!(
        "\n(3) THE MEASURED REQUIREMENT ÷ THE CONTRACT'S OWN a-priori S = window + rate×stall\n"
    );
    println!(
        "{:>5} {:>6} | {:>8} {:>8} {:>8} {:>8} | {:>8}",
        "clk", "loss", "p50", "p90", "max", "S*/pred", "cells"
    );
    for clk in [Clock::App, Clock::Wire] {
        for &loss in &losses {
            let sel: Vec<&CovRow> =
                rows.iter().filter(|r| r.clock == clk && r.loss == loss).collect();
            let v: Vec<f64> =
                sel.iter().filter_map(|r| r.st[I1].map(|s| s as f64 / r.pred_s.max(1.0))).collect();
            let mut ss: Vec<f64> = sel.iter().map(|r| r.s_star / r.pred_s.max(1.0)).collect();
            if v.is_empty() {
                continue;
            }
            println!(
                "{:>5} {:>6.3} | {:>8.3} {:>8.3} {:>8.3} {:>8.3} | {:>8}",
                clk.tag(),
                loss,
                quant(&mut v.clone(), 0.5),
                quant(&mut v.clone(), 0.9),
                quant(&mut v.clone(), 1.0),
                quant(&mut ss, 0.5),
                v.len()
            );
        }
    }

    // ── (4) ROUTE B — THE CLOSED LOOP ──
    println!(
        "\n=== (4) ROUTE B — CLOSING THE LOOP: the dwell is the store's OWN output ===\n\
         dwell = min(S·g, E[residence | dwell]) iterated to its fixed point (Little's law\n\
         on the retention store, no free parameter). The OPEN-LOOP column is phase 1.1's\n\
         curve — an unconstrained 144 ms dwell replayed against a constrained backlog.\n"
    );
    println!(
        "{:>4} {:>4} {:>6} {:>3} {:>5} | {:>8} | {:>8} {:>8} {:>7} | {:>8} {:>8} {:>7} | {:>9} {:>6}",
        "rate", "rtp", "loss", "np", "clk",
        "pred S",
        "open S1%", "open S.1%", "oct",
        "clos S1%", "clos S.1%", "oct",
        "dwell ms", "iters"
    );
    let cl_rt = list_u64("RWM_CB_CL_RTPROP_MS", "5,20,100");
    let cl_loss = list_f64("RWM_CB_CL_LOSS", "0.026");
    let mut cl_ratio: Vec<f64> = Vec::new();
    let mut cl_oct: Vec<f64> = Vec::new();
    let mut op_oct: Vec<f64> = Vec::new();
    // (open S(1%), closed S(1%)) per cell, split by clock so the same
    // geometry can be paired across the clock argument for (4c).
    let mut pair_app: Vec<(Option<usize>, Option<usize>)> = Vec::new();
    let mut pair_wire: Vec<(Option<usize>, Option<usize>)> = Vec::new();
    for rn in &rate_names {
        let Some(rate) = rate_of(rn) else { continue };
        let mut ccal = cal;
        ccal.mbps = mbps_of(rate);
        for &rtprop_ms in &cl_rt {
            for &loss in &cl_loss {
                for &np in &paths {
                    for &clock in &clocks {
                        let srtt_honest = rtprop_ms * 1_000 + ccal.wireq_us;
                        let pred_s = network_window(rate, srtt_honest)
                            + emission_slack(
                                rate,
                                contract_stall_us(rho, b_hint, rtprop_ms * 1_000, srtt_honest),
                            );
                        let (mut o1, mut o01, mut o90) = (None, None, None);
                        let (mut c1, mut c01, mut c90) = (None, None, None);
                        let (mut dwell_at_1, mut iters_max) = (0u64, 0usize);
                        for &seed in &seeds {
                            let cell = Cell {
                                rtprop_us: rtprop_ms * 1_000,
                                loss,
                                pattern: Pattern::Ge,
                                n_paths: np as usize,
                                clock,
                                arm: ARMS[0],
                                seed,
                            };
                            // OPEN loop: phase 1.1's curve — the dwell is an
                            // INPUT fixed at the calibration's 144 ms.
                            let out0 = run_cell(cell, ccal);
                            let d0 = residences(&out0);
                            let g = out0.tx_gap_us;
                            worst(&mut o1, s_for_idle(&d0, g, 0.01, 1 << 18));
                            worst(&mut o01, s_for_idle(&d0, g, 0.001, 1 << 18));
                            worst(&mut o90, s_for_idle(&d0, g, 0.90, 1 << 18));
                            // CLOSED loop: at each candidate S the dwell is
                            // solved for, then the idle is read at that same S.
                            // Bisected with the same monotonicity as the open
                            // curve: a larger cap relaxes the emission
                            // constraint and can only raise the dwell's bound.
                            let hi0 = (8.0 * pred_s).ceil() as usize;
                            let solve = |s: usize| -> (f64, u64, usize) {
                                let (dw, traj, _d, idle) = closed_loop_dwell(cell, ccal, s, 12);
                                (idle, dw, traj.len())
                            };
                            let bisect = |target: f64| -> Option<usize> {
                                let (mut lo, mut hi) = (1usize, hi0);
                                if solve(hi).0 >= target {
                                    return None;
                                }
                                while lo < hi {
                                    let mid = lo + (hi - lo) / 2;
                                    if solve(mid).0 < target {
                                        hi = mid;
                                    } else {
                                        lo = mid + 1;
                                    }
                                }
                                Some(lo)
                            };
                            let (b1, b01, b90) = (bisect(0.01), bisect(0.001), bisect(0.90));
                            if let Some(v) = b1 {
                                if c1.is_none_or(|a: usize| v > a) {
                                    let (_, dw, it) = solve(v);
                                    dwell_at_1 = dw;
                                    iters_max = it;
                                }
                            }
                            worst(&mut c1, b1);
                            worst(&mut c01, b01);
                            worst(&mut c90, b90);
                        }
                        let oct = |a: Option<usize>, b: Option<usize>| match (a, b) {
                            (Some(x), Some(y)) if x > 0 => (y as f64 / x as f64).log2(),
                            _ => f64::NAN,
                        };
                        let (oo, co) = (oct(o90, o1), oct(c90, c1));
                        if oo.is_finite() {
                            op_oct.push(oo);
                        }
                        if co.is_finite() {
                            cl_oct.push(co);
                        }
                        if let Some(v) = c1 {
                            cl_ratio.push(v as f64 / pred_s.max(1.0));
                        }
                        match clock {
                            Clock::App => pair_app.push((o1, c1)),
                            Clock::Wire => pair_wire.push((o1, c1)),
                        }
                        let f = |o: Option<usize>| {
                            o.map(|v| v.to_string()).unwrap_or_else(|| ">8pred".into())
                        };
                        println!(
                            "{:>4} {:>4} {:>6.3} {:>3} {:>5} | {:>8.0} | {:>8} {:>8} {:>7.2} | {:>8} {:>8} {:>7.2} | {:>9.2} {:>6}",
                            rn, rtprop_ms, loss, np, clock.tag(),
                            pred_s,
                            f(o1), f(o01), oo,
                            f(c1), f(c01), co,
                            dwell_at_1 as f64 / 1000.0, iters_max
                        );
                    }
                }
            }
        }
    }
    println!(
        "\n(4a) CLOSED-LOOP S(1%) ÷ the contract's a-priori pred S:\n    \
         p50 {:.3}  p10 {:.3}  p90 {:.3}  max {:.3}  ({} cells)\n\
         (4b) TRANSITION WIDTH (90% idle → 1% idle), octaves:  open p50 {:.2}  CLOSED p50 {:.2}",
        quant(&mut cl_ratio.clone(), 0.5),
        quant(&mut cl_ratio.clone(), 0.1),
        quant(&mut cl_ratio.clone(), 0.9),
        quant(&mut cl_ratio.clone(), 1.0),
        cl_ratio.len(),
        quant(&mut op_oct, 0.5),
        quant(&mut cl_oct, 0.5),
    );
    // (4c) THE CLOCK ARGUMENT, priced open-loop and closed-loop. §16.43's
    // whole tail failure was app ÷ wire; if the loop is what produced it,
    // this ratio must collapse toward 1 when the loop is closed.
    let mut ao: Vec<f64> = Vec::new();
    let mut ac: Vec<f64> = Vec::new();
    for (k, (o_app, c_app)) in pair_app.iter().enumerate() {
        let (o_wire, c_wire) = pair_wire[k];
        if let (Some(a), Some(b)) = (*o_app, o_wire) {
            ao.push(a as f64 / b.max(1) as f64);
        }
        if let (Some(a), Some(b)) = (*c_app, c_wire) {
            ac.push(a as f64 / b.max(1) as f64);
        }
    }
    println!(
        "(4c) S(1%) on the APP clock ÷ the same cell on the WIRE clock:\n    \
         OPEN loop   p50 {:.2}  p90 {:.2}  max {:.2}\n    \
         CLOSED loop p50 {:.2}  p90 {:.2}  max {:.2}   ({} pairs)",
        quant(&mut ao.clone(), 0.5),
        quant(&mut ao.clone(), 0.9),
        quant(&mut ao.clone(), 1.0),
        quant(&mut ac.clone(), 0.5),
        quant(&mut ac.clone(), 0.9),
        quant(&mut ac.clone(), 1.0),
        ac.len()
    );

    // ── (5) ROUTE D — THE LIMITS ──
    println!(
        "\n=== (5) ROUTE D — THE LIMITS: is the coverage determined at the edges? ===\n\
         The ε̂ → 0 axis, measured in the CLOSED loop. If the slope is a property of the\n\
         recovery plane's tail rather than a free dimension of the contract, its width must\n\
         collapse as the plane runs out of holes to serve, and S(1%) must fall onto the\n\
         network window alone. `S1/S.1` is the whole leverage the coverage target has.\n"
    );
    println!(
        "{:>4} {:>4} {:>3} {:>5} {:>7} | {:>8} {:>8} {:>6} {:>8} | {:>8} {:>7} {:>8} {:>7} | {:>8} {:>8}",
        "rate", "rtp", "np", "clk", "loss",
        "window", "pred S", "kappa", "pred κ",
        "clos S1%", "÷pred", "÷window", "÷predκ",
        "octaves", "S.1/S1"
    );
    for rn in &rate_names {
        let Some(rate) = rate_of(rn) else { continue };
        let mut lcal = cal;
        lcal.mbps = mbps_of(rate);
        for &rtprop_ms in &list_u64("RWM_CB_LIM_RTPROP_MS", "20") {
            for &np in &paths {
                for &clock in &clocks {
                    for &loss in &list_f64("RWM_CB_LIM_LOSS", "0.0,0.0001,0.001,0.01,0.026,0.05") {
                        let rtprop_us = rtprop_ms * 1_000;
                        let srtt_honest = rtprop_us + lcal.wireq_us;
                        let window = network_window(rate, srtt_honest);
                        let stall = contract_stall_us(rho, b_hint, rtprop_us, srtt_honest);
                        let pred_s = window + emission_slack(rate, stall);
                        // The ε̂-composed candidate. `MEAN_BURST` is the GE
                        // chain's own mean bad run — the driver's declared
                        // loss model, the quantity §8.3's σ²_burst estimates
                        // in production — not a coefficient chosen here.
                        let kappa = frontier_pinned_fraction(rate, loss, MEAN_BURST, stall);
                        let pred_k = window + emission_slack(rate, stall) * kappa;
                        let cell = Cell {
                            rtprop_us,
                            loss,
                            pattern: Pattern::Ge,
                            n_paths: np as usize,
                            clock,
                            arm: ARMS[0],
                            seed: 42,
                        };
                        let hi0 = (8.0 * pred_s).ceil() as usize;
                        let solve = |s: usize| closed_loop_dwell(cell, lcal, s, 12).3;
                        let bisect = |target: f64| -> Option<usize> {
                            let (mut lo, mut hi) = (1usize, hi0);
                            if solve(hi) >= target {
                                return None;
                            }
                            while lo < hi {
                                let mid = lo + (hi - lo) / 2;
                                if solve(mid) < target {
                                    hi = mid;
                                } else {
                                    lo = mid + 1;
                                }
                            }
                            Some(lo)
                        };
                        let (c1, c01, c90) = (bisect(0.01), bisect(0.001), bisect(0.90));
                        let r = |a: Option<usize>, b: Option<usize>| match (a, b) {
                            (Some(x), Some(y)) if x > 0 => y as f64 / x as f64,
                            _ => f64::NAN,
                        };
                        let f = |o: Option<usize>| {
                            o.map(|v| v.to_string()).unwrap_or_else(|| ">8pred".into())
                        };
                        println!(
                            "{:>4} {:>4} {:>3} {:>5} {:>7.4} | {:>8.0} {:>8.0} {:>6.3} {:>8.0} | {:>8} {:>7.3} {:>8.3} {:>7.3} | {:>8.2} {:>8.3}",
                            rn, rtprop_ms, np, clock.tag(), loss,
                            window, pred_s, kappa, pred_k,
                            f(c1),
                            c1.map(|v| v as f64 / pred_s.max(1.0)).unwrap_or(f64::NAN),
                            c1.map(|v| v as f64 / window.max(1.0)).unwrap_or(f64::NAN),
                            c1.map(|v| v as f64 / pred_k.max(1.0)).unwrap_or(f64::NAN),
                            r(c90, c1).log2(),
                            r(c1, c01)
                        );
                    }
                }
            }
        }
    }

    // ── (6) ROUTE A — CAN δ ARBITRATE? ──
    println!(
        "\n=== (6) ROUTE A — CAN δ ARBITRATE THE CONFLICT? ===\n\
         Slack wants the limit LARGE, span wants it SMALL, and δ prices latency — so δ\n\
         looks like the term that ought to settle it. Backlog above the network window\n\
         stands in the bottleneck queue and adds (S/rate − RTprop) of delay to EVERY\n\
         symbol, which δ bounds by its own deadline D(δ): S ≤ rate·(RTprop + D(δ)).\n\
         That is δ's ONLY entry, and it is a CEILING. Below: the ceiling at each named\n\
         point of the δ dial against the backlog the slack term demands.\n"
    );
    println!(
        "{:>4} {:>4} | {:>8} {:>8} | {}",
        "rate",
        "rtp",
        "window",
        "need S",
        ["b=1/8", "b=1/2 (RT)", "b=1", "b=2 (bulk)"]
            .iter()
            .map(|t| format!("{t:>14}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    for rn in &rate_names {
        let Some(rate) = rate_of(rn) else { continue };
        for &rtprop_ms in &rtprops {
            let rtprop_us = rtprop_ms * 1_000;
            let srtt = rtprop_us + cal.wireq_us;
            let window = network_window(rate, srtt);
            let need = window + emission_slack(rate, contract_stall_us(rho, b_hint, rtprop_us, srtt));
            let cols: Vec<String> = [0.125f64, 0.5, 1.0, 2.0]
                .iter()
                .map(|&b| {
                    let ceil =
                        rate * ((rtprop_us + shed_deadline_us(b, rtprop_us)) as f64 / 1e6);
                    format!("{:>8.0}/{:>5.2}", ceil, need / ceil.max(1.0))
                })
                .collect();
            println!(
                "{:>4} {:>4} | {:>8.0} {:>8.0} | {}",
                rn,
                rtprop_ms,
                window,
                need,
                cols.join(" ")
            );
        }
    }
    println!(
        "\n    Each cell is `ceiling / (need ÷ ceiling)`. A ratio > 1 means δ's own deadline\n    \
         FORBIDS the backlog the slack term requires — the two constraints are infeasible\n    \
         together, not in tension at an interior optimum. A ratio < 1 means the ceiling is\n    \
         SLACK and δ says nothing. δ never lands ON the requirement, at any b(δ)."
    );

    println!("\n{} cells in {:.2} s", rows.len(), t0.elapsed().as_secs_f64());
}

// ────────────────────── regression fixtures (fast, CI) ──────────────────

/// ABSOLUTE assertions on the three DERIVED terms and on the emission
/// simulator, plus the liveness proof that the mechanism under test actually
/// executes (CLAUDE.md testing discipline / MEASUREMENT DISCIPLINE rule 1).
#[test]
fn slack_bench_terms_are_arithmetic_with_no_constants() {
    // TERM 1 — Little's law on the wire, exactly.
    assert_eq!(network_window(10_400.0, 8_000), 83.2, "c2 anchor: 10.4k × 8 ms");
    assert_eq!(network_window(2_000.0, 60_000), 120.0, "c3 anchor: 2k × 60 ms");

    // The CONTRACT stall. ρ = 1 (RETAIN): 17/8 · srtt, no other term.
    let srtt = 12_000u64; // RTprop 8 ms + wireQ 4 ms, the c2 honest clock
    assert_eq!(contract_stall_us(1.0, 0.5, 8_000, srtt), 25_500, "17/8 × 12 ms");
    // ρ = 0 (fully sheddable): the span law's own D(δ) = b·RTprop, b = ½.
    assert_eq!(contract_stall_us(0.0, 0.5, 8_000, srtt), 4_000, "D(δ) = ½ × 8 ms");
    assert_eq!(shed_deadline_us(0.5, 8_000), 4_000, "the shipped law agrees");
    // CONTINUOUS in ρ, no mode bit: the midpoint is the midpoint.
    let (a, b, m) = (
        contract_stall_us(0.0, 0.5, 8_000, srtt),
        contract_stall_us(1.0, 0.5, 8_000, srtt),
        contract_stall_us(0.5, 0.5, 8_000, srtt),
    );
    assert_eq!(m, (a + b) / 2, "stall(ρ) is a straight line in ρ");
    for k in 0..=20 {
        let r = k as f64 / 20.0;
        let s = contract_stall_us(r, 0.5, 8_000, srtt);
        assert!(s >= a && s <= b, "ρ={r} escapes the endpoints");
    }

    // TERM 2 — slack = rate × stall. c2 at ρ = 1: 10.4k × 25.5 ms.
    assert!((emission_slack(10_400.0, 25_500) - 265.2).abs() < 1e-9);

    // TERM 3 — the span, both readings, at the c8 geometry.
    assert!((resequencing_span_recv(10_400.0, 26_000) - 270.4).abs() < 1e-9);
    assert!((resequencing_span_store(10_400.0, 52_000) - 540.8).abs() < 1e-9);
    // The store reading is exactly twice the receiver reading for symmetric
    // one-way delays — the two jobs of the same geometry.
    assert!(
        (resequencing_span_store(10_400.0, 52_000)
            - 2.0 * resequencing_span_recv(10_400.0, 26_000))
        .abs()
            < 1e-9
    );
    // Job 3 DOES NOT EXIST without skew — the structural claim, asserted.
    assert_eq!(resequencing_span_recv(10_400.0, 0), 0.0, "N = 1 / zero skew ⇒ no span term");
}

#[test]
fn slack_bench_emission_sim_is_exact_on_hand_computable_inputs() {
    // A lossless steady flow: every symbol resides for exactly one ack round
    // trip. With g = 100 µs and d = 1 000 µs the pipe holds 10 symbols, so
    // S ≥ 10 is idle-free and S < 10 idles by exactly the shortfall.
    let d = vec![1_000u64; 2_000];
    assert!(wire_idle_fraction(&d, 100, 10) < 1e-9, "S = rate×RTT ⇒ zero idle");
    assert!(wire_idle_fraction(&d, 100, 16) < 1e-9, "more never hurts");
    // At S = 5 the wire runs at 5/10 of its rate ⇒ 50% idle.
    let i5 = wire_idle_fraction(&d, 100, 5);
    assert!((i5 - 0.5).abs() < 1e-3, "S = 5 of 10 ⇒ 50% idle, got {i5}");
    let i2 = wire_idle_fraction(&d, 100, 2);
    assert!((i2 - 0.8).abs() < 1e-3, "S = 2 of 10 ⇒ 80% idle, got {i2}");
    // Monotone non-increasing in S — the term is a COVER, so more of it can
    // never idle the wire more.
    let mut prev = 1.0;
    for s in [1usize, 2, 3, 4, 5, 8, 10, 16, 32] {
        let i = wire_idle_fraction(&d, 100, s);
        assert!(i <= prev + 1e-12, "idle rose from {prev} to {i} at S={s}");
        prev = i;
    }
}

/// PHASE 1.3 — the coverage instrument's own arithmetic. Every assertion is
/// ABSOLUTE and hand-computable; nothing here is ordinal (CLAUDE.md).
#[test]
fn coverage_terms_are_arithmetic_with_no_constants() {
    // κ — Little's law on the FRONTIER. c2 at RTprop 20 (srtt 24 ms, ρ = 1 ⇒
    // stall 51 ms), GE mean burst 8: episodes arrive at ε̂·10 400/8 per
    // second and each pins the frontier 51 ms.
    let stall = contract_stall_us(1.0, 0.5, 20_000, 24_000);
    assert_eq!(stall, 51_000, "17/8 × 24 ms");
    let k = |e: f64| frontier_pinned_fraction(10_400.0, e, 8.0, stall);
    // 0.001 × 10 400 ÷ 8 × 0.051 = 0.06630 exactly.
    assert!((k(0.001) - 0.066_3).abs() < 1e-9, "got {}", k(0.001));
    assert!((k(0.01) - 0.663).abs() < 1e-9);
    // ε̂ → 0 ⇒ NO holes ⇒ nothing to be slack for. The limit the uncorrected
    // term misses by ×3.4 (measured: it predicts 3.125 × the window where
    // the requirement is 0.90 × it).
    assert_eq!(k(0.0), 0.0, "no holes ⇒ no pinned frontier ⇒ no slack term");
    // Saturates at 1 — a frontier cannot be pinned more than always — and is
    // continuous through the clamp (no mode bit).
    assert_eq!(k(1.0), 1.0);
    assert_eq!(k(0.5), 1.0);
    let mut prev = -1.0;
    for i in 0..=40 {
        let v = k(i as f64 / 40.0);
        assert!(v >= prev - 1e-12 && (0.0..=1.0).contains(&v), "κ broke at {i}");
        prev = v;
    }

    // Little's law on the STORE: a constant residence d over N symbols at
    // gap g is a mean occupancy of exactly d/g.
    let d = vec![1_000u64; 2_000];
    assert!((mean_occupancy(&d, 100) - 10.0).abs() < 1e-9);
    assert_eq!(mean_occupancy(&[], 100), 0.0);
    assert_eq!(mean_occupancy(&d, 0), 0.0);

    // The hyperbola: idle = 1 − S/S*, floored at 0, no free parameter.
    assert!((hyperbola_idle(5, 10.0) - 0.5).abs() < 1e-12);
    assert!((hyperbola_idle(2, 10.0) - 0.8).abs() < 1e-12);
    assert_eq!(hyperbola_idle(10, 10.0), 0.0);
    assert_eq!(hyperbola_idle(99, 10.0), 0.0, "a cover cannot go negative");

    // Bisection agrees with the hand-computable emission sim, and this is
    // the whole content of "the coverage is an endpoint, not a dial": on the
    // hyperbola S(α) = (1 − α)·S*, so across the ENTIRE operational band —
    // 3 % idle down to 0.1 %, one and a half decades of target — the answer
    // is the SAME INTEGER, S* = 10. Only the absurd end of the range (30 %
    // idle) moves it at all, and then by ×1.43.
    assert_eq!(s_for_idle(&d, 100, 0.30, 4_096), Some(7));
    assert_eq!(s_for_idle(&d, 100, 0.10, 4_096), Some(9));
    for &t in &[0.03, 0.01, 0.003, 0.001] {
        assert_eq!(s_for_idle(&d, 100, t, 4_096), Some(10), "target {t}");
    }
    assert_eq!(s_for_idle(&d, 100, 0.0, 4_096), None, "0 % idle is never < 0 %");
    // Below S* the sim is the hyperbola exactly, so the "3-octave slope"
    // carries no information: it is 90 % → 10 % idle of ONE parameter.
    // (Tolerance 5e-3: the first `S` symbols leave back-to-back before the
    // store can fill, a start transient of order S²/(10·N) — 4e-4 at S = 9,
    // N = 2000. It is the sim's own edge, not slack in the identity.)
    for s in [1usize, 2, 3, 5, 8, 9] {
        let m = wire_idle_fraction(&d, 100, s);
        assert!((m - hyperbola_idle(s, 10.0)).abs() < 5e-3, "S={s}: {m}");
    }
    assert!(
        ((9.0f64 / 1.0).log2() - 3.17).abs() < 0.01,
        "90%→10% of a hyperbola is 3.17 octaves — the measured median, from no shape at all"
    );
}

/// The COLD-START ACK CORRECTION, bounded rather than described (CLAUDE.md).
/// The driver's ack timer is armed one gap-ack quantum in, before any symbol
/// can have arrived; the "nothing arrived" branch used to defer the next
/// advertisement by the full hole-refresh cadence, holding every symbol
/// emitted inside it. At loss 0 there is no hole at all, so the store
/// residence MUST be one round trip plus at most two ack quanta — never the
/// refresh cadence.
#[test]
fn coverage_cold_start_ack_is_bounded_by_the_gap_ack_floor() {
    let mut cal = Calib::fixture();
    cal.mbps = mbps_of(26_000.0); // c1 — the rate class the artifact SET
    let cell = Cell {
        rtprop_us: 20_000,
        loss: 0.0,
        pattern: Pattern::Ge,
        n_paths: 1,
        clock: Clock::Wire,
        arm: ARMS[0],
        seed: 42,
    };
    let out = run_cell(cell, cal);
    // LIVENESS: the plane ran, the store filled and drained, no hole existed.
    assert!(out.holes.is_empty(), "loss 0 must produce no hole");
    assert_eq!(out.refresh_us, 48_000, "2 × 24 ms Copa — the cadence NOT to be used here");
    let d = residences(&out);
    assert_eq!(d.len(), cal.n_src as usize);
    assert!(d.iter().any(|&x| x > 0), "no symbol ever resided ⇒ nothing measured");
    let worst = *d.iter().max().unwrap();
    assert!(
        worst <= 20_000 + 2 * GAP_ACK_MIN_US,
        "cold-start hold escaped the gap-ack floor: {worst} µs > {} µs (refresh is {})",
        20_000 + 2 * GAP_ACK_MIN_US,
        out.refresh_us
    );
    // And the consequence the correction exists for: at zero loss the
    // required backlog is the NETWORK WINDOW and nothing else.
    let window = network_window(26_000.0, 20_000 + cal.wireq_us);
    let s1 = s_for_idle(&d, out.tx_gap_us, 0.01, 1 << 16).expect("1 % must be reachable");
    assert!(
        (s1 as f64) < 1.25 * window,
        "ε̂ = 0 ⇒ S(1%) must fall onto the window {window:.0}, got {s1}"
    );
}

/// ROUTE B, pinned: the store dwell is an OUTPUT, the loop closes, and the
/// clock argument's cost collapses when it does. Bounds the finding.
#[test]
fn coverage_closed_loop_converges_and_prices_the_clock() {
    let mut cal = Calib::fixture();
    cal.mbps = mbps_of(10_400.0);
    let base = Cell {
        rtprop_us: 5_000,
        loss: 0.026,
        pattern: Pattern::Ge,
        n_paths: 1,
        clock: Clock::App,
        arm: ARMS[0],
        seed: 42,
    };
    let wire = Cell { clock: Clock::Wire, ..base };
    let s = 256usize;

    let (dw_a, traj_a, _, idle_a) = closed_loop_dwell(base, cal, s, 12);
    let (dw_w, traj_w, _, idle_w) = closed_loop_dwell(wire, cal, s, 12);

    // It CONVERGES — the map is a contraction, so the fixed point exists and
    // the iteration finds it well inside the budget.
    assert!(traj_a.len() < 12, "app loop did not converge: {traj_a:?}");
    // On the WIRE clock the loop gain is identically ZERO (the dwell is not
    // in the estimator's argument), so the second iterate already agrees.
    assert_eq!(traj_w.len(), 2, "wire loop is not open: {traj_w:?}");
    // The fixed-point dwell is an OUTPUT, and it is nothing like the 144 ms
    // the phase-1.1 calibration fed in as an INPUT.
    assert!(dw_a > 0 && dw_a < cal.dwell_us / 10, "app fixed-point dwell {dw_a} µs");
    assert!(dw_w > 0 && dw_w < cal.dwell_us / 10, "wire fixed-point dwell {dw_w} µs");

    // THE PRICE OF THE CLOCK, bounded. Open loop, the app clock needs far
    // more backlog for the same idle; closed loop the two agree.
    let open_a = wire_idle_fraction(&residences(&run_cell(base, cal)), 115, s);
    let open_w = wire_idle_fraction(&residences(&run_cell(wire, cal)), 115, s);
    assert!(
        open_a > 4.0 * open_w,
        "open loop: the app clock is supposed to be far worse here ({open_a} vs {open_w})"
    );
    assert!(
        idle_a < 1.5 * idle_w.max(1e-6),
        "closed loop: the clock argument must stop mattering ({idle_a} vs {idle_w})"
    );
}

/// LIVENESS + the pinned component result at the c7-class operating point:
/// the recovery plane really is driven, the stall really is measured, and
/// the measured stall really does exceed the contract's own by the factor
/// the ledger's cadence audit predicts. These are bench OUTPUTS, pinned.
#[test]
fn slack_bench_fixtures_pin_the_slack_term() {
    let mut cal = Calib::fixture();
    cal.mbps = mbps_of(10_400.0);
    let cell = Cell {
        rtprop_us: 10_000,
        loss: 0.026,
        pattern: Pattern::Ge,
        n_paths: 2,
        clock: Clock::App,
        arm: ARMS[0],
        seed: 42,
    };
    let out = run_cell(cell, cal);
    // Liveness: the plane executed and produced holes through named channels.
    assert!(!out.holes.is_empty(), "no holes ⇒ the recovery plane never ran");
    assert!(out.counts.retx > 0, "no retransmit fired");
    let stalls = frontier_stalls(&out, 2);
    assert!(!stalls.is_empty(), "no frontier stall measured");
    assert!(
        stalls.iter().any(|(c, _)| c.is_some()),
        "no stall carries a channel label ⇒ attribution is dead"
    );

    // The residence series must be live and finite.
    let d = residences(&out);
    assert_eq!(d.len(), cal.n_src as usize);
    assert!(d.iter().any(|&x| x > 0), "no symbol ever resided in the store");

    // The idle curve must be monotone non-increasing in S and must actually
    // fall — a flat curve would mean the backlog term does nothing here.
    let mut prev = f64::INFINITY;
    for &s in &[16usize, 64, 256, 1024, 4096, 16384] {
        let i = wire_idle_fraction(&d, out.tx_gap_us, s);
        assert!(i <= prev + 1e-12, "idle rose at S={s}: {prev} → {i}");
        prev = i;
    }
    assert!(
        wire_idle_fraction(&d, out.tx_gap_us, 16) > wire_idle_fraction(&d, out.tx_gap_us, 4096),
        "the backlog term is inert here — the bench would be measuring nothing"
    );

    // THE HEADLINE, pinned: the app-echo clock's patience is 177.75 ms
    // against a contract stall of 17/8 × 14 ms = 29.75 ms — the recovery
    // plane's own cadence, not the declared deadline, sets the stall.
    let srtt_honest = 10_000 + cal.wireq_us;
    assert_eq!(contract_stall_us(1.0, 0.5, 10_000, srtt_honest), 29_750);
    assert_eq!(out.patience_us, 177_750, "the app-echo §6.1.2 threshold");
    assert!(
        out.patience_us > 5 * contract_stall_us(1.0, 0.5, 10_000, srtt_honest),
        "the patience clock is supposed to dwarf the contract stall here: {} vs {}",
        out.patience_us,
        contract_stall_us(1.0, 0.5, 10_000, srtt_honest)
    );
}

// ══════════ PHASE 1.3 — COMPONENT VALIDATION OF THE SHIPPED LAW ══════════
//
// MEASUREMENT DISCIPLINE 14: before any L1 battery, the ENGINE's composed
// arithmetic is evaluated against the requirement THIS bench measured. What
// runs below is not a re-derivation — it is `raptorpath::net::
// three_term_store_cap`, the exact function `run_window_sender` calls under
// `RWM_THREE_TERM`, driven on the bench's own cells.
//
// THE ENGINE-vs-BENCH ADJUDICATION, stated before the numbers (there are
// exactly two divergences, and neither is a coefficient):
//
//  1. THE WINDOW TERM'S CLOCK. The bench writes term 1 as `rate·srtt` with
//     `srtt = RTprop + wireQ`. The engine writes it as `rate·K·RTprop` with
//     K the windowed-MIN echoSRTT/RTprop, because a cap that reads a LOADED
//     srtt inflates its own input (the dwell→echo→cap feedback). On the
//     bench's axes those are the SAME NUMBER — the driver's honest clock is
//     literally `RTprop + wireQ`, so `K = 1 + wireQ/RTprop` exactly — and
//     `three_term_engine_law_is_the_bench_terms_at_the_anchors` asserts the
//     identity rather than asserting a tolerance around it.
//  2. THE SPAN TERM EXISTS IN THE ENGINE AND NOT IN THE BENCH'S `pred_s`.
//     §16.43/§16.44 measured the span separately (PS5/PS6) and never folded
//     it into the composite. So at np = 2 the engine's limit is LARGER than
//     `pred_s` by exactly `rate_total × Δowd`, and at np = 1 the two agree
//     to the ceil quantum. That is a difference in WHAT IS BEING PREDICTED,
//     not a disagreement about a value, and section (A) reports it as such.
//
// The requirement the ratios are taken against is the CLOSED-loop one
// (§16.44 route B) — the open-loop curve is the one whose ×13.5 tail was an
// artifact of running the store at 3× its own derived size.

/// The ENGINE's law on one bench cell. The driver places symbol `seq` on
/// path `seq % np` and gives path i a one-way delay of `RTprop/2 + i·skew`,
/// so each path carries `rate/np` and has round trip `RTprop + 2·i·skew`;
/// its honest clock is that plus the standing wire queue. Those are exactly
/// the inputs `run_window_sender` reads off `PathState` — rate, `min_rtt`,
/// `srtt` — and nothing else is supplied.
fn engine_limit(
    rate: f64,
    rtprop_us: u64,
    n_paths: usize,
    cal: Calib,
    rho: f64,
    b_hint: f64,
) -> (usize, f64, f64, f64) {
    let terms: Vec<Option<ThreeTermTerm>> = (0..n_paths.max(1))
        .map(|i| {
            let rtp_us = rtprop_us + 2 * i as u64 * cal.skew_us;
            let srtt_us = rtp_us + cal.wireq_us;
            Some(ThreeTermTerm {
                rate: rate / n_paths.max(1) as f64,
                rtprop_s: rtp_us as f64 / 1e6,
                k: srtt_us as f64 / rtp_us as f64,
            })
        })
        .collect();
    three_term_store_cap(true, &terms, rho, b_hint, 64).expect("every bench path is warm")
}

#[test]
#[ignore]
fn three_term_bench() {
    let mut cal = Calib::from_env();
    cal.n_src = env_u64("RWM_SB_N", 6_000);
    cal.skew_us = env_u64("RWM_SB_SKEW_MS", 5) * 1_000;
    let rtprops = list_u64("RWM_SB_RTPROP_MS", "5,10,20,50,100,200");
    let paths = list_u64("RWM_SB_PATHS", "1,2");
    let clocks: Vec<Clock> = list_str("RWM_SB_CLOCK", "app,wire")
        .iter()
        .map(|s| if s == "wire" { Clock::Wire } else { Clock::App })
        .collect();
    let rate_names = list_str("RWM_SB_RATE", "c1,c2,c3");
    let seeds = list_u64("RWM_SB_SEEDS", "42,7");
    let rho = env_f64("RWM_SB_RHO", 1.0);
    let b_hint = env_f64("RWM_SB_DELTA_B", 0.5);
    let t0 = std::time::Instant::now();

    println!(
        "\n=== COMPONENT VALIDATION — THE SHIPPED THREE-TERM LAW vs THIS BENCH ===\n\
         The function under test is raptorpath::net::three_term_store_cap, the one\n\
         run_window_sender calls under RWM_THREE_TERM. Nothing below re-derives it.\n"
    );

    // ── (A) THE ENGINE'S LAW vs THE BENCH'S OWN a-priori pred_s ──────────
    println!(
        "(A) ENGINE LIMIT vs the bench's pred_s = rate*srtt + rate*stall(d,rho).\n    \
         np = 1: the two must AGREE (same terms, same clock) - any gap is a defect.\n    \
         np = 2: the engine ADDS the span term the bench measured separately (PS5),\n    \
                 so the excess must equal rate_total x d_owd, exactly.\n"
    );
    println!(
        "{:>4} {:>4} {:>3} | {:>9} {:>9} {:>9} {:>9} | {:>9} {:>9} {:>9}",
        "rate", "rtp", "np", "pred_s", "eng", "eng win", "eng slack", "eng span",
        "pred span", "eng/pred"
    );
    let mut agree_max = 0.0f64;
    let mut span_err_max = 0.0f64;
    for rn in &rate_names {
        let Some(rate) = rate_of(rn) else { continue };
        for &rtprop_ms in &rtprops {
            for &np in &paths {
                let rtprop_us = rtprop_ms * 1_000;
                let srtt_honest = rtprop_us + cal.wireq_us;
                let pred_s = network_window(rate, srtt_honest)
                    + emission_slack(rate, contract_stall_us(rho, b_hint, rtprop_us, srtt_honest));
                let (lim, w, sl, sp) =
                    engine_limit(rate, rtprop_us, np as usize, cal, rho, b_hint);
                // PS5's own measured form: slope = the TOTAL emission rate
                // against the one-way skew, over a retention round trip.
                let pred_span = if np >= 2 { rate * (cal.skew_us as f64 / 1e6) } else { 0.0 };
                span_err_max = span_err_max.max((sp - pred_span).abs() / pred_span.max(1.0));
                if np == 1 {
                    agree_max = agree_max.max(((w + sl) - pred_s).abs() / pred_s.max(1.0));
                }
                println!(
                    "{:>4} {:>4} {:>3} | {:>9.1} {:>9} {:>9.1} {:>9.1} | {:>9.1} {:>9.1} {:>9.3}",
                    rn, rtprop_ms, np, pred_s, lim, w, sl, sp, pred_span,
                    lim as f64 / pred_s.max(1.0)
                );
            }
        }
    }
    println!(
        "\n    worst |(engine window + slack) - pred_s| / pred_s at np=1 = {agree_max:.3e}\n    \
         worst |engine span - rate_total x d_owd| / that             = {span_err_max:.3e}\n    \
         Both are floating-point dust: the engine computes the bench's own terms."
    );

    // ── (B) THE ENGINE'S LAW vs THE CLOSED-LOOP MEASURED REQUIREMENT ────
    println!(
        "\n(B) MEASURED CLOSED-LOOP S(1%) / THE ENGINE'S LIMIT. 16.44 route B: the dwell\n    \
         is solved self-consistently at each candidate backlog, so this is the\n    \
         requirement the store actually has rather than the 3x-oversized replay.\n    \
         A ratio > 1 = the engine UNDER-provisions (the failure that matters); < 1 =\n    \
         it over-covers, the expected direction for a DECLARED-bound stall.\n"
    );
    println!(
        "{:>4} {:>4} {:>6} {:>3} {:>5} | {:>8} {:>8} {:>8} {:>8} | {:>8} {:>8}",
        "rate", "rtp", "loss", "np", "clk", "eng", "win", "slack", "span",
        "meas S1%", "meas/eng"
    );
    let cl_rt = list_u64("RWM_TT_RTPROP_MS", "5,20,100");
    let cl_loss = list_f64("RWM_TT_LOSS", "0.026");
    let mut ratios: Vec<f64> = Vec::new();
    let mut ratios_np1: Vec<f64> = Vec::new();
    let mut ratios_np2: Vec<f64> = Vec::new();
    let mut worst_cell = (0.0f64, String::new());
    for rn in &rate_names {
        let Some(rate) = rate_of(rn) else { continue };
        let mut ccal = cal;
        ccal.mbps = mbps_of(rate);
        for &rtprop_ms in &cl_rt {
            for &loss in &cl_loss {
                for &np in &paths {
                    for &clock in &clocks {
                        let rtprop_us = rtprop_ms * 1_000;
                        let (lim, w, sl, sp) =
                            engine_limit(rate, rtprop_us, np as usize, ccal, rho, b_hint);
                        let hi0 = (8.0 * lim as f64).ceil() as usize;
                        let mut meas: Option<usize> = None;
                        for &seed in &seeds {
                            let cell = Cell {
                                rtprop_us,
                                loss,
                                pattern: Pattern::Ge,
                                n_paths: np as usize,
                                clock,
                                arm: ARMS[0],
                                seed,
                            };
                            let solve = |s: usize| closed_loop_dwell(cell, ccal, s, 12).3;
                            let bisect = || -> Option<usize> {
                                if solve(hi0) >= 0.01 {
                                    return None;
                                }
                                let (mut lo, mut hi) = (1usize, hi0);
                                while lo < hi {
                                    let mid = lo + (hi - lo) / 2;
                                    if solve(mid) < 0.01 {
                                        hi = mid;
                                    } else {
                                        lo = mid + 1;
                                    }
                                }
                                Some(lo)
                            };
                            worst(&mut meas, bisect());
                        }
                        let r = meas.map(|m| m as f64 / lim as f64);
                        if let Some(v) = r {
                            ratios.push(v);
                            if np == 1 {
                                ratios_np1.push(v)
                            } else {
                                ratios_np2.push(v)
                            }
                            if v > worst_cell.0 {
                                worst_cell = (
                                    v,
                                    format!("{rn}/RTprop {rtprop_ms}/np {np}/{}", clock.tag()),
                                );
                            }
                        }
                        println!(
                            "{:>4} {:>4} {:>6.3} {:>3} {:>5} | {:>8} {:>8.0} {:>8.0} {:>8.0} | {:>8} {:>8}",
                            rn, rtprop_ms, loss, np, clock.tag(), lim, w, sl, sp,
                            meas.map(|v| v.to_string()).unwrap_or_else(|| ">8xeng".into()),
                            r.map(|v| format!("{v:.3}")).unwrap_or_else(|| "-".into())
                        );
                    }
                }
            }
        }
    }
    let q = |v: &Vec<f64>, p: f64| quant(&mut v.clone(), p);
    println!(
        "\n(B1) MEASURED / ENGINE — the component-validation distribution:\n    \
         ALL   p50 {:.3}  p90 {:.3}  max {:.3}   ({} cells)\n    \
         np=1  p50 {:.3}  p90 {:.3}  max {:.3}   ({} cells)\n    \
         np=2  p50 {:.3}  p90 {:.3}  max {:.3}   ({} cells)\n    \
         WORST CELL: {:.3} at {}\n    \
         over-covering cells (ratio < 1): {} of {}",
        q(&ratios, 0.5), q(&ratios, 0.9), q(&ratios, 1.0), ratios.len(),
        q(&ratios_np1, 0.5), q(&ratios_np1, 0.9), q(&ratios_np1, 1.0), ratios_np1.len(),
        q(&ratios_np2, 0.5), q(&ratios_np2, 0.9), q(&ratios_np2, 1.0), ratios_np2.len(),
        worst_cell.0, worst_cell.1,
        ratios.iter().filter(|v| **v < 1.0).count(), ratios.len()
    );
    println!("\ndone in {:.2} s", t0.elapsed().as_secs_f64());
}

/// The ENGINE's law IS the bench's terms — asserted as an IDENTITY at the
/// anchors, not as a tolerance (CLAUDE.md: prove the wiring routes there).
#[test]
fn three_term_engine_law_is_the_bench_terms_at_the_anchors() {
    let cal = Calib::fixture(); // wireQ 4 ms, skew 5 ms
    // ── np = 1: window + slack must reproduce `pred_s` EXACTLY, and the
    // span term must be identically zero. This is the whole engine-vs-bench
    // adjudication, at c2 / RTprop 8 ms.
    let (lim, w, sl, sp) = engine_limit(10_400.0, 8_000, 1, cal, 1.0, 0.5);
    assert_eq!(sp, 0.0, "one path ⇒ no span term, by arithmetic");
    assert!((w - network_window(10_400.0, 12_000)).abs() < 1e-9, "window {w}");
    assert!(
        (sl - emission_slack(10_400.0, contract_stall_us(1.0, 0.5, 8_000, 12_000))).abs() < 1e-9,
        "slack {sl}"
    );
    assert_eq!(lim, 391, "ceil(124.8 + 265.2)");
    // The clock identity that licenses the adjudication, and the engine's
    // own stall in seconds against this bench's in µs.
    assert!((contract_stall_s(1.0, 0.5, 0.008, 0.012) * 1e6 - 25_500.0).abs() < 1e-6);

    // ── np = 2 at the same cell: the engine ADDS the span term, and it
    // equals PS5's measured form, rate_total × Δowd = 10 400 × 5 ms.
    let (lim2, w2, sl2, sp2) = engine_limit(10_400.0, 8_000, 2, cal, 1.0, 0.5);
    assert!((sp2 - 52.0).abs() < 1e-9, "span {sp2} must be 10 400 × 5 ms");
    assert!(lim2 > lim, "the span term must COST outstanding, not be free");
    // The lagging path's own clock is longer, so its window and slack rise
    // too — a real dependence on a real signal, not a topology bonus.
    assert!(w2 > w && sl2 > sl);

    // ── The c8 geometry: c2 fast + c3 slow at their OWN rates. The span
    // reads §16.43 PS6's 541 (independently measured good pin 508, +6.5 %).
    let c8 = [
        Some(ThreeTermTerm { rate: 10_400.0, rtprop_s: 0.008, k: 12.0 / 8.0 }),
        Some(ThreeTermTerm { rate: 2_000.0, rtprop_s: 0.060, k: 64.0 / 60.0 }),
    ];
    let (_, _, _, sp8) = three_term_store_cap(true, &c8, 1.0, 0.5, 64).unwrap();
    assert!((sp8 - 540.8).abs() < 1e-9, "c8 span {sp8} vs PS6's 540.8");
    assert!(
        (sp8 - resequencing_span_store(10_400.0, 52_000)).abs() < 1e-9,
        "the engine's span IS this bench's `resequencing_span_store`"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// THE SLACK CLOCK, REFUTED — paper §16.58 (2026-08-18, `fix/cap-law-cluster`)
// ─────────────────────────────────────────────────────────────────────────

/// **THE EXECUTABLE RECORD OF §16.58's REFUTATION.**
///
/// The charge: term 2's clock is `srtt = K·RTprop` with `K` a windowed MIN of
/// `echoSRTT/RTprop`, so the clock carries whatever standing WIRE queue
/// survives the estimator's window and the law sizes itself on a queue it
/// created — `cap → wireQ → srtt → K → slack → cap`. The candidate replaces
/// the stall's argument with the loop-free `min_rtt`:
///
/// ```text
///   shipped   slack = Σ rate·stall(δ, ρ, K·RTprop)   ⇒ at ρ = 1, 2.125·window
///   candidate slack = Σ rate·stall(δ, ρ,   RTprop)   ⇒ at ρ = 1, 2.125·window/K
/// ```
///
/// Scored on §16.57's own `[3T]` decomposition (833 wire evaluations), which
/// is why this is a record and not a new claim: every input below is
/// TRANSCRIBED from the ledger's table, and the candidate's value follows by
/// arithmetic from numbers already published.
///
/// **The refutation, asserted in both directions.** The isolated criterion was
/// that the memory-bound pinning goes INTERIOR at the c8 geometry that read
/// `mem` = 1.000. This test asserts that it does NOT — c8 and c8L still exceed
/// `WIN_STORE_MAX` under the candidate — and that the SECOND criterion (the
/// `slack/window` code identity) IS broken. A refutation with a surviving half
/// is worth exactly as much as the half, and both are pinned so neither can
/// rot into prose.
///
/// Nothing here drives a candidate implementation, because there is none:
/// `contract_stall_s` is unchanged and nothing shipped. The candidate is
/// evaluated as the closed form its own formula gives.
#[test]
fn the_queue_free_slack_clock_is_refuted_on_the_wire_measured_inputs() {
    // §16.57 / goal-gate "Composed-Cap Battery — RESULTS", the `[3T]`
    // decomposition over 833 evaluations, plus the per-cell `K`.
    // (cell, mean window, mean slack, mean span, K)
    const WIRE: &[(&str, f64, f64, f64, f64)] = &[
        ("c1", 201.0, 428.0, 0.0, 1.15),
        ("sc2", 374.0, 794.0, 0.0, 1.14),
        ("c7", 1_261.0, 2_680.0, 118.0, 1.14),
        ("c8", 1_669.0, 3_546.0, 2_563.0, 1.04),
        ("c8L", 7_489.0, 15_915.0, 2_552.0, 1.505),
    ];
    // The shipped retain-until-acked multiplier: `stall(ρ=1) = 9/8·srtt +
    // srtt`, so `slack = 17/8 · window`. Not a constant of this test — it is
    // asserted against the ENGINE's own `contract_stall_s` below.
    const RETAIN_MULT: f64 = 17.0 / 8.0;
    // `net::WIN_STORE_MAX`, the memory bound stated OUTSIDE the law.
    const MEM: f64 = 4096.0;

    // MEASUREMENT DISCIPLINE rule 1 at closed-form scale: if
    // `contract_stall_s` ever stops being 17/8·srtt at ρ = 1, every number
    // below is void, and this fires before any of them is read.
    for &(_, _, _, _, k) in WIRE {
        let (rtprop_s, srtt_s) = (0.038, 0.038 * k);
        let stall = contract_stall_s(1.0, 1.0, rtprop_s, srtt_s);
        assert!(
            (stall - RETAIN_MULT * srtt_s).abs() < 1e-12,
            "the shipped stall is no longer 17/8·srtt at ρ = 1 — §16.58's \
             closed form does not apply"
        );
    }

    let mut ratios: Vec<f64> = Vec::new();
    let mut still_pinning = 0usize;
    for &(cell, window, slack, span, k) in WIRE {
        // (a) THE DEGENERACY, reproduced from the ledger's own numbers: the
        // shipped slack is the window times 17/8 — an identity of the code, so
        // the "three-term" law is a TWO-term law at the shipped scope.
        let shipped_ratio = slack / window;
        assert!(
            (shipped_ratio - RETAIN_MULT).abs() < 5e-3,
            "{cell}: the ledger's slack/window is not the 2.125 identity \
             ({shipped_ratio})"
        );

        // (b) THE CANDIDATE, by its own formula: the same stall on `min_rtt`.
        let cand_slack = RETAIN_MULT * window / k;
        let shipped_total = window + slack + span;
        let cand_total = window + cand_slack + span;
        assert!(
            cand_total < shipped_total,
            "{cell}: the queue-free clock must be a REDUCTION or the candidate \
             is not what it claims"
        );

        // (c) THE CRITERION THAT FAILS. c8 read `mem` = 1.000 and c8L 0.788 in
        // the battery; if the candidate cleared the memory bound there, the
        // item would be FIXED. It does not.
        if cell == "c8" || cell == "c8L" {
            assert!(
                cand_total > MEM,
                "{cell}: the queue-free slack clock CLEARED the memory bound \
                 ({cand_total} < {MEM}) — §16.58's REFUTATION no longer holds \
                 and the paper must be re-scored, not this assertion relaxed"
            );
            still_pinning += 1;
            // And by how much it misses: the ask must shed (1 − MEM/Σ) and the
            // clock sheds far less. BOUNDED, not described.
            let needed = 1.0 - MEM / shipped_total;
            let achieved = 1.0 - cand_total / shipped_total;
            assert!(
                achieved < needed / 2.0,
                "{cell}: the clock now sheds {achieved} of an ask that must \
                 shed {needed} — the refutation's margin has closed"
            );
        } else {
            // The three cells that were interior stay interior: the candidate
            // is never the reason a cell moves the wrong way.
            assert!(cand_total < MEM, "{cell}: the candidate pushed an interior cell out");
        }

        ratios.push(cand_slack / window);
    }

    // (d) THE HALF THAT SURVIVES. The shipped ratio is 2.125 with min == max
    // in 833 of 833 evaluations; the candidate's is `2.125/K` and VARIES, so
    // the term structure de-degenerates. Asserted as min != max, which is the
    // exact shape of the identity being broken.
    let lo = ratios.iter().cloned().fold(f64::INFINITY, f64::min);
    let hi = ratios.iter().cloned().fold(0.0f64, f64::max);
    assert!(
        hi - lo > 0.5,
        "the candidate's slack/window is still an identity (min {lo}, max {hi})"
    );
    assert!((lo - RETAIN_MULT / 1.505).abs() < 1e-9, "the min ratio is c8L's 2.125/K");
    assert!((hi - RETAIN_MULT / 1.04).abs() < 1e-9, "the max ratio is c8's 2.125/K");

    assert_eq!(still_pinning, 2, "both pinning cells must have been scored");
}
