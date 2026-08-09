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

use raptorpath::net::{shed_deadline_us, tail_sweep_timeout_us};

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
