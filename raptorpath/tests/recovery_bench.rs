//! COMPONENT BENCH — the RECOVERY PLANE ALONE.
//!
//! Goal-gate "Component Benches" (2026-08-08). No CC, no scheduler, no
//! multipath placement, no transport, no tokio: a deterministic
//! discrete-event driver that feeds synthetic arrival/loss patterns to the
//! SHIPPED recovery laws (`raptorpath::net::*`, extracted in the commit
//! before this one) and records, per hole, WHEN and WHY it was served.
//!
//! The standing rule this instrument exists to serve: **no L1 battery until
//! the mechanism is characterized at component level and the component
//! result predicts a number L1 can confirm or refute.** Two consecutive
//! goal-gate attempts (ack-merge, derived patience) were falsified because
//! their PREMISE was mis-measured — facts that cost hours of L1 time and
//! that this file answers in seconds.
//!
//! ```text
//! cargo test --test recovery_bench --release -- --ignored --nocapture
//! ```
//!
//! Env knobs (axes are comma lists; every default is stated):
//!   RWM_RB_RTPROP_MS  5,10,20,50,100,200   RTprop per cell
//!   RWM_RB_LOSS       0.001,0.01,0.026,0.05
//!   RWM_RB_PATTERN    uniform,ge           iid vs Gilbert-Elliott bursty
//!   RWM_RB_PATHS      1,2                  path count (2 ⇒ skew applied)
//!   RWM_RB_CLOCK      app,wire             THE clock ARGUMENT (see below)
//!   RWM_RB_ARMS       shipped,legacy,sp,pd gate arms (see `ARMS`)
//!   RWM_RB_N          6000                 source symbols per cell
//!   RWM_RB_SEEDS      42,7
//!   RWM_RB_MBPS       100.0                source rate (c7 class)
//!   RWM_RB_SKEW_MS    5                    inter-path one-way delay skew
//!   RWM_RB_WIREQ_MS   4                    standing network queue (c7 p0)
//!   RWM_RB_DWELL_MS   144                  sender-store dwell the app-echo
//!                                          RTT includes; RTprop 10 + wireQ 4
//!                                          + 144 = the 158 ms measured at c7
//!   RWM_RB_JITTER_MS  1                    measured RTT jitter (pd arm)
//!   RWM_RB_SHED       0                    arm the δ-honest shed law
//!
//! THE CLOCK ARGUMENT (the first question, §16.40's named successor):
//! every recovery clock in the plane is fed by `pooled_recovery_srtt_us` and
//! by the per-path `(copa_srtt, estimator_rtt)` pair. The estimator's
//! app-echo RTT is STORE-DWELL INCLUSIVE — measured 158 ms at c7 against an
//! 8–10 ms RTprop — while `QuicTransport::wire_rtt` (ADR-0062 / §16.34)
//! excludes the dwell. `RWM_RB_CLOCK=app` feeds the dwell-inclusive clock,
//! `wire` the dwell-free one. NOTHING else differs between the arms.
//!
//! WHAT THIS BENCH CANNOT SEE — stated up front, because that boundary is
//! what keeps L1 honest:
//!   * congestion control. No cwnd, no pacing gate, no Copa backoff: a
//!     retransmit here is never rate-limited or queued behind source data,
//!     so measured hole→service latency is a LOWER BOUND on the real one.
//!   * the NACK budget's congestion modulation (`NackCongestionState`) —
//!     the per-report budget is the un-throttled `MAX_NACK_REPAIRS_PER_NACK`.
//!   * FEC. No proactive repair, no generation/window decode: every hole is
//!     served by ARQ alone, so retx counts are an UPPER bound wherever r > 0
//!     would have covered the hole first.
//!   * the store/reservoir itself — i.e. the very dwell that inflates the
//!     app-echo clock is an INPUT here, not an emergent quantity. The bench
//!     can say what a given dwell does to patience; it cannot say what
//!     changing the clock does to the dwell (that feedback loop is exactly
//!     what L1 must measure).
//!   * scheduler placement, per-path CC coupling, path failover.
//!   * control-plane loss and the real ack/emission interleave.

// The driver, the axes, the loss model and the shipped-law calls now live in
// `tests/common/recovery_model.rs` so that `tests/slack_bench.rs` can take
// this plane's STALL DISTRIBUTION as its input rather than invent one
// (goal-gate "Emission-Slack Bench", 2026-08-09). The move was VERBATIM;
// `recovery_bench_fixtures_pin_the_plane` below is unchanged and is the
// proof of that.
#[path = "common/recovery_model.rs"]
mod recovery_model;

use recovery_model::*;


// ───────────────────────────── reporting ────────────────────────────────

/// A cell's pooled result across seeds.
struct Row {
    rtprop_us: u64,
    loss: f64,
    pattern: Pattern,
    n_paths: usize,
    clock: Clock,
    patience_us: u64,
    pooled_us: u64,
    cooldown_us: u64,
    refresh_us: u64,
    n_holes: u64,
    n_served: u64,
    n_delivered: u64,
    svc_ms: Vec<u64>, // hole → first service, µs, sorted
    del_ms: Vec<u64>, // hole → delivered, µs, sorted
    mix: [u64; 5],
    redundant: u64,
    counts: Counts,
}

fn pool(cells: &[(Cell, Out)]) -> Row {
    let c0 = cells[0].0;
    let o0 = &cells[0].1;
    let mut row = Row {
        rtprop_us: c0.rtprop_us,
        loss: c0.loss,
        pattern: c0.pattern,
        n_paths: c0.n_paths,
        clock: c0.clock,
        patience_us: o0.patience_us,
        pooled_us: o0.pooled_us,
        cooldown_us: o0.cooldown_us,
        refresh_us: o0.refresh_us,
        n_holes: 0,
        n_served: 0,
        n_delivered: 0,
        svc_ms: Vec::new(),
        del_ms: Vec::new(),
        mix: [0; 5],
        redundant: 0,
        counts: Counts::default(),
    };
    for (_, o) in cells {
        row.n_holes += o.holes.len() as u64;
        for h in &o.holes {
            if let Some(t) = h.first_service_us {
                row.n_served += 1;
                row.svc_ms.push(t.saturating_sub(h.lost_at_us));
            }
            if let Some(t) = h.delivered_us {
                row.n_delivered += 1;
                row.del_ms.push(t.saturating_sub(h.lost_at_us));
            }
            if let Some(c) = h.chan {
                let i = CHANS.iter().position(|&x| x == c).unwrap();
                row.mix[i] += 1;
            }
            row.redundant += h.services.saturating_sub(1) as u64;
        }
        row.counts.retx += o.counts.retx;
        row.counts.sweeps += o.counts.sweeps;
        row.counts.reports += o.counts.reports;
        row.counts.supp_cool += o.counts.supp_cool;
        row.counts.supp_law += o.counts.supp_law;
        row.counts.supp_age += o.counts.supp_age;
        row.counts.shed += o.counts.shed;
    }
    row.svc_ms.sort_unstable();
    row.del_ms.sort_unstable();
    row
}

fn mix_str(mix: &[u64; 5]) -> String {
    let tot: u64 = mix.iter().sum();
    if tot == 0 {
        return "-".into();
    }
    CHANS
        .iter()
        .enumerate()
        .filter(|(i, _)| mix[*i] > 0)
        .map(|(i, c)| format!("{}{:.0}%", c.tag(), 100.0 * mix[i] as f64 / tot as f64))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
#[ignore]
fn recovery_bench() {
    let cal = Calib::from_env();
    let rtprops = list_u64("RWM_RB_RTPROP_MS", "5,10,20,50,100,200");
    let losses = list_f64("RWM_RB_LOSS", "0.001,0.01,0.026,0.05");
    let patterns: Vec<Pattern> = list_str("RWM_RB_PATTERN", "uniform,ge")
        .iter()
        .map(|s| if s == "ge" { Pattern::Ge } else { Pattern::Uniform })
        .collect();
    let paths = list_u64("RWM_RB_PATHS", "1,2");
    let clocks: Vec<Clock> = list_str("RWM_RB_CLOCK", "app,wire")
        .iter()
        .map(|s| if s == "wire" { Clock::Wire } else { Clock::App })
        .collect();
    let arm_names = list_str("RWM_RB_ARMS", "shipped,legacy,sp,pd");
    let arms: Vec<Arm> =
        ARMS.iter().copied().filter(|a| arm_names.iter().any(|n| n == a.name)).collect();
    let seeds = list_u64("RWM_RB_SEEDS", "42,7");

    println!("\n=== RECOVERY PLANE COMPONENT BENCH (goal-gate \"Component Benches\") ===");
    println!(
        "N={} src @ {:.1} Mbit/s ({} µs/sym) · skew {} ms · wireQ {} ms · dwell {} ms · jitter {} ms · shed {} · budget {}/report",
        cal.n_src,
        cal.mbps,
        (((1_200.0 * 8.0) / (cal.mbps * 1e6)) * 1e6) as u64,
        cal.skew_us / 1000,
        cal.wireq_us / 1000,
        cal.dwell_us / 1000,
        cal.jitter_us / 1000,
        if cal.shed { "ON" } else { "off" },
        cal.budget
    );
    println!("seeds {seeds:?} · kTimeThreshold 9/8 · kPacketThreshold 3 · GAP_ACK_MIN 2 ms");

    // rows[(arm, cellkey)] for the cross-arm summaries below.
    let mut all: Vec<(&'static str, Row)> = Vec::new();
    let t0 = std::time::Instant::now();

    for arm in &arms {
        println!("\n--- arm `{}` (recov_mp={} recov_sp={} patience_derived={}) ---",
            arm.name, arm.recov_mp, arm.recov_sp, arm.patience_derived);
        println!(
            "{:>4} {:>6} {:>5} {:>3} {:>5} | {:>9} {:>8} {:>8} {:>7} | {:>6} {:>5} | {:>7} {:>7} {:>7} {:>8} | {:<26} | {:>6} {:>6} {:>6} | {}",
            "rtp", "loss", "pat", "np", "clk",
            "patience", "pooled", "cooldwn", "refresh",
            "holes", "svc%",
            "p50 ms", "p90", "p99", "max",
            "channel mix (first service)",
            "retx", "sweep", "redun",
            "supp c/l/a"
        );
        for &rtprop_ms in &rtprops {
            for &loss in &losses {
                for &pattern in &patterns {
                    for &np in &paths {
                        for &clock in &clocks {
                            let cells: Vec<(Cell, Out)> = seeds
                                .iter()
                                .map(|&seed| {
                                    let c = Cell {
                                        rtprop_us: rtprop_ms * 1_000,
                                        loss,
                                        pattern,
                                        n_paths: np as usize,
                                        clock,
                                        arm: *arm,
                                        seed,
                                    };
                                    let o = run_cell(c, cal);
                                    (c, o)
                                })
                                .collect();
                            let r = pool(&cells);
                            println!(
                                "{:>4} {:>6.3} {:>5} {:>3} {:>5} | {:>9.1} {:>8.1} {:>8.1} {:>7.1} | {:>6} {:>4.0}% | {:>7.1} {:>7.1} {:>7.1} {:>8.1} | {:<26} | {:>6} {:>6} {:>6} | {}/{}/{}",
                                rtprop_ms, loss, pattern.tag(), np, clock.tag(),
                                ms(r.patience_us), ms(r.pooled_us), ms(r.cooldown_us), ms(r.refresh_us),
                                r.n_holes,
                                if r.n_holes > 0 { 100.0 * r.n_served as f64 / r.n_holes as f64 } else { 0.0 },
                                ms(pct(&r.svc_ms, 0.50)), ms(pct(&r.svc_ms, 0.90)),
                                ms(pct(&r.svc_ms, 0.99)), ms(*r.svc_ms.last().unwrap_or(&0)),
                                mix_str(&r.mix),
                                r.counts.retx, r.counts.sweeps, r.redundant,
                                r.counts.supp_cool, r.counts.supp_law, r.counts.supp_age
                            );
                            all.push((arm.name, r));
                        }
                    }
                }
            }
        }
    }

    // ── (a) the patience claim, per cell ──
    println!("\n=== (a) PATIENCE vs RTprop — app-echo clock vs wire clock ===");
    println!("{:>4} {:>3} | {:>12} {:>12} | {:>12} {:>12} | {:>10}",
        "rtp", "np", "patience app", "×RTprop", "patience wire", "×RTprop", "app/wire");
    for &rtprop_ms in &rtprops {
        for &np in &paths {
            let g = |clk: Clock| {
                all.iter()
                    .find(|(a, r)| {
                        *a == "shipped"
                            && r.rtprop_us == rtprop_ms * 1000
                            && r.n_paths == np as usize
                            && r.clock == clk
                    })
                    .map(|(_, r)| r.patience_us)
            };
            if let (Some(a), Some(w)) = (g(Clock::App), g(Clock::Wire)) {
                println!(
                    "{:>4} {:>3} | {:>12.1} {:>12.1} | {:>12.1} {:>12.1} | {:>10.2}",
                    rtprop_ms, np,
                    ms(a), a as f64 / (rtprop_ms * 1000) as f64,
                    ms(w), w as f64 / (rtprop_ms * 1000) as f64,
                    a as f64 / w as f64
                );
            }
        }
    }

    // ── (b) does the FAST (packet-threshold) channel get to fire? ──
    println!("\n=== (b) the FAST channel (RFC 9002 §6.1.1, kPacketThreshold=3) ===");
    println!("shipped arm, np=2 (the only configuration in which it is armed):");
    println!("{:>4} {:>6} {:>5} {:>5} | {:>6} {:>7} {:>7} | {:>9} {:>9}",
        "rtp", "loss", "pat", "clk", "holes", "fast", "time", "fast share", "svc p50 ms");
    for (a, r) in &all {
        if *a != "shipped" || r.n_paths != 2 {
            continue;
        }
        let fast = r.mix[1];
        let time = r.mix[0];
        let tot: u64 = r.mix.iter().sum();
        println!(
            "{:>4} {:>6.3} {:>5} {:>5} | {:>6} {:>7} {:>7} | {:>8.1}% {:>9.1}",
            r.rtprop_us / 1000, r.loss, r.pattern.tag(), r.clock.tag(),
            r.n_holes, fast, time,
            if tot > 0 { 100.0 * fast as f64 / tot as f64 } else { 0.0 },
            ms(pct(&r.svc_ms, 0.50))
        );
    }

    // ── (c) THE L1 PREDICTION ──
    println!("\n=== (c) PREDICTION FOR L1 (c7 cell: RTprop 10 ms, GE 2.6 %, np=2) ===");
    for arm in &arms {
        let g = |clk: Clock| {
            all.iter().find(|(a, r)| {
                *a == arm.name
                    && r.rtprop_us == 10_000
                    && (r.loss - 0.026).abs() < 1e-9
                    && r.pattern == Pattern::Ge
                    && r.n_paths == 2
                    && r.clock == clk
            })
        };
        if let (Some((_, a)), Some((_, w))) = (g(Clock::App), g(Clock::Wire)) {
            let rr = |x: u64, y: u64| if y == 0 { f64::NAN } else { x as f64 / y as f64 };
            println!(
                "arm {:<8}: retx {} → {} (×{:.2})  sweeps {} → {} (×{:.2})  redundant {} → {} (×{:.2})  svc p50 {:.1} → {:.1} ms (×{:.2})",
                arm.name,
                a.counts.retx, w.counts.retx, rr(w.counts.retx, a.counts.retx),
                a.counts.sweeps, w.counts.sweeps, rr(w.counts.sweeps, a.counts.sweeps),
                a.redundant, w.redundant, rr(w.redundant, a.redundant),
                ms(pct(&a.svc_ms, 0.50)), ms(pct(&w.svc_ms, 0.50)),
                pct(&w.svc_ms, 0.50) as f64 / pct(&a.svc_ms, 0.50).max(1) as f64
            );
        }
    }
    println!("\n{} cells in {:.2} s", all.len(), t0.elapsed().as_secs_f64());
}

// ────────────────────── regression fixtures (fast, CI) ──────────────────

/// Characteristic bench outputs pinned as fixtures so the recovery plane's
/// behaviour cannot drift silently. These are ABSOLUTE assertions on the
/// plane's own laws at named operating points (CLAUDE.md's testing
/// discipline: ordinal tests do not catch routing bugs), and they double as
/// the liveness proof that the mechanism under test actually executes —
/// every fixture asserts a non-zero service count through a NAMED channel.
///
/// If a law changes deliberately, re-run the bench and update the numbers in
/// the same commit, saying so.
#[test]
fn recovery_bench_fixtures_pin_the_plane() {
    let cal = Calib::fixture();
    let base = Cell {
        rtprop_us: 10_000,
        loss: 0.026,
        pattern: Pattern::Uniform,
        n_paths: 2,
        clock: Clock::App,
        arm: ARMS[0], // shipped
        seed: 42,
    };
    let p50 = |o: &Out| {
        let mut v: Vec<u64> =
            o.holes.iter().filter_map(|h| h.first_service_us.map(|t| t - h.lost_at_us)).collect();
        v.sort_unstable();
        pct(&v, 0.5)
    };
    let mix = |o: &Out| {
        let mut m = [0u64; 5];
        for h in &o.holes {
            if let Some(c) = h.chan {
                m[CHANS.iter().position(|&x| x == c).unwrap()] += 1;
            }
        }
        m
    };

    // ── FIXTURE 1: the clock LAWS at the c7 operating point (dual path).
    // Path 0's app-echo RTT is the c7-measured 158 ms (RTprop 10 + wireQ 4 +
    // dwell 144), so patience = 9/8 × 158 = 177.75 ms; the pooled clock is
    // the SKEWED path's app-echo (2×(5+5) + 4 + 144 = 168 ms) and the
    // per-seq cooldown equals it. The receiver's refresh reads the COPA
    // clock, so it is 2 × 24 = 48 ms in BOTH clock arms.
    let a = run_cell(base, cal);
    assert_eq!(a.patience_us, 177_750, "path-0 §6.1.2 threshold, app-echo");
    assert_eq!(a.pooled_us, 168_000, "pooled recovery clock = max app-echo");
    assert_eq!(a.cooldown_us, 168_000, "per-seq cooldown = pooled clock");
    assert_eq!(a.refresh_us, 48_000, "hole refresh reads COPA, not the estimator");

    let w = run_cell(Cell { clock: Clock::Wire, ..base }, cal);
    assert_eq!(w.patience_us, 15_750, "path-0 §6.1.2 threshold, wire");
    assert_eq!(w.pooled_us, 24_000, "pooled = skewed path's wire RTT");
    assert_eq!(w.cooldown_us, 24_000);
    assert_eq!(w.refresh_us, 48_000, "unchanged: the receiver reads Copa srtt");
    assert_eq!(a.holes.len(), w.holes.len(), "same seed ⇒ same wire losses");
    assert!(
        a.patience_us > 11 * w.patience_us,
        "the clock ARGUMENT, not the constants, sets patience: {} vs {}",
        a.patience_us,
        w.patience_us
    );

    // ── FIXTURE 2: the SHIPPED plane's measured behaviour at that point.
    // Every number below is a bench output, not a derivation.
    assert_eq!(a.holes.len(), FIX_HOLES);
    assert_eq!(mix(&a), FIX_APP_MIX, "app-echo channel mix (time/fast/age/sweep/shed)");
    assert_eq!(a.counts.retx, FIX_APP_RETX);
    assert_eq!(a.counts.sweeps, FIX_APP_SWEEPS);
    assert_eq!(p50(&a), FIX_APP_P50_US);
    assert_eq!(mix(&w), FIX_WIRE_MIX, "wire channel mix");
    assert_eq!(w.counts.retx, FIX_WIRE_RETX);
    assert_eq!(w.counts.sweeps, FIX_WIRE_SWEEPS);
    assert_eq!(p50(&w), FIX_WIRE_P50_US);
    // Liveness: every arm must actually serve holes through a named channel.
    for (tag, o) in [("app", &a), ("wire", &w)] {
        assert!(mix(o).iter().sum::<u64>() > 0, "{tag}: no hole reached a channel");
        assert!(o.counts.retx > 0, "{tag}: no retransmit fired");
    }

    // ── FIXTURE 3: the MP law's N ≤ 1 bypass is BIT-EXACT — at one path the
    // shipped arm must reproduce the legacy age gate exactly. (`mp_hole_ripe`
    // returns true unconditionally at N ≤ 1; this is that claim, measured.)
    let sp1 = run_cell(Cell { n_paths: 1, ..base }, cal);
    let lg1 = run_cell(Cell { n_paths: 1, arm: ARMS[1], ..base }, cal);
    assert_eq!(sp1.pooled_us, 158_000, "np=1: 2×5 + 4 + 144 = the c7 app-echo");
    assert_eq!(sp1.cooldown_us, 158_000);
    assert_eq!(sp1.refresh_us, 28_000, "2 × 14 ms Copa, inside the [25,100] clamp");
    assert_eq!(mix(&sp1), mix(&lg1), "N<=1 bypass must be bit-exact vs legacy");
    assert_eq!(sp1.counts.retx, lg1.counts.retx);
    assert_eq!(p50(&sp1), p50(&lg1));
    assert_eq!(mix(&lg1)[2], lg1.holes.len() as u64, "np=1 shipped ⇒ 100% legacy age gate");

    // ── FIXTURE 4: THE ASYMMETRY `RWM_RECOV_SP` inherits. At N = 1 the
    // §6.1.2 channel waits 9/8·srtt where the legacy gate waits srtt/2 — so
    // on the app-echo clock, arming SP makes recovery STRICTLY SLOWER, by
    // the ratio (9/8)/(1/2) = 2.25. That is a component fact about the
    // clock's ARGUMENT, not about the RFC channel.
    let sp_arm = run_cell(Cell { n_paths: 1, arm: ARMS[2], ..base }, cal);
    assert_eq!(mix(&sp_arm)[0], sp_arm.holes.len() as u64, "SP at np=1 ⇒ 100% time channel");
    assert!(
        p50(&sp_arm) > 2 * p50(&lg1),
        "SP on the app-echo clock must be >2x slower than the legacy gate: {} vs {}",
        p50(&sp_arm),
        p50(&lg1)
    );
    // On the WIRE clock the same arm is fast in absolute terms — the channel
    // was never the problem.
    let sp_wire = run_cell(Cell { n_paths: 1, arm: ARMS[2], clock: Clock::Wire, ..base }, cal);
    assert!(
        p50(&sp_wire) * 5 < p50(&sp_arm),
        "wire-clocked SP must be >5x faster than app-clocked SP: {} vs {}",
        p50(&sp_wire),
        p50(&sp_arm)
    );
}

// The measured fixture values (bench outputs, `Calib::fixture`, seed 42,
// RTprop 10 ms / uniform 2.6 % / np = 2 / arm `shipped`).
//
// RE-PINNED 2026-08-10 (goal-gate "Coverage: derivable or not", the driver's
// COLD-START ACK CORRECTION). The receiver's ack timer is armed at
// `GAP_ACK_MIN_US` = 2 ms, one owd before the first symbol can arrive; the
// "nothing arrived" branch then deferred the next advertisement by the full
// hole-refresh cadence even though no hole had ever been NACKed, holding
// every symbol emitted inside that window in the sender's store. That is a
// startup transient with no counterpart in the shipped receiver (which acks
// ON ARRIVAL subject to the 2 ms floor), and at the fast rate classes it
// SET the measured backlog requirement outright. The correction moves these
// six pins and nothing else — the channel LAWS (fixture 1), the N ≤ 1
// bit-exact bypass (fixture 3) and the SP asymmetry (fixture 4) are
// unchanged:
//   app  p50 16 600 → 14 776 µs        (−11.0 %)
//   wire p50 16 600 → 14 328 µs        (−13.7 %)
//   wire mix [75, 79] → [70, 84]  ·  retx 157 → 156  ·  sweeps 3 → 2
const FIX_HOLES: usize = 154;
/// 100 % FAST: on the app-echo clock the §6.1.2 channel (177.75 ms) never
/// ripens inside a hole's life, so the packet-threshold channel serves
/// EVERY hole. That is the answer to bench question (b), pinned.
const FIX_APP_MIX: [u64; 5] = [0, 154, 0, 0, 0];
const FIX_APP_RETX: u64 = 154;
const FIX_APP_SWEEPS: u64 = 1;
const FIX_APP_P50_US: u64 = 14_776;
/// On the wire clock the time channel ripens at 15.75 ms and takes half the
/// holes — the channel mix, not the retx total, is what the argument moves.
const FIX_WIRE_MIX: [u64; 5] = [70, 84, 0, 0, 0];
const FIX_WIRE_RETX: u64 = 156;
const FIX_WIRE_SWEEPS: u64 = 2;
const FIX_WIRE_P50_US: u64 = 14_328;

// ═════════════════════════════════════════════════════════════════════════
//  THE DERIVED RECOVERY CLAMP (goal-gate "The Derived Recovery Clamp",
//  2026-08-12) — the two recovery clocks' [25 ms, 100 ms] clamp on trial.
// ═════════════════════════════════════════════════════════════════════════
//
// THE FINDING BEING ACTED ON: goal-gate "The Latency-Feedback Source"
// re-attributed c8's collapse mode to an APPENDED DEAD WALL — ~30 % of a
// collapse rep's wall with the sender neither reading source, nor blocked on
// its store cap, nor sending — and named its quantum as the recovery clocks:
// `tail_sweep_timeout_us` and `hole_nack_refresh` are both `2·SRTT` clamped
// to [25, 100] ms, and c8's measured SRTT overshoots the ceiling 7.5×.
//
// THE QUESTION, put the way §16.40 requires (the ARGUMENT before the
// constant): is the CEILING wrong, or is the ARGUMENT wrong? Both repairs
// are evaluated here SEPARATELY and then composed:
//
//   REPAIR A — derive the clamp away.  `derived_recovery_round_us`:
//              max(2·srtt, patience_floor_us(jitter, srtt)), no ceiling.
//   REPAIR B — change the argument.    Feed the clock the WIRE RTT (Copa /
//              `QuicTransport::wire_rtt`) instead of the store-dwell-
//              inclusive app-echo RTT, exactly as §16.40's successor asked.
//
// THE ARITHMETIC THAT DECIDES IT, and why it is arithmetic and not taste.
// The sender arms the sweep at `last_activity + timeout`, where
// `last_activity` is the seq's own `retransmit_buffer` insert instant
// (`net/emit_source.rs:551`, `now_us()` at emission). The ack that would
// RETIRE that seq is timed from the SAME instant: `send_timestamp_us:
// now_us()` on the DATA frame, echoed back and differenced in
// `net/control_msg.rs` — which is precisely what `estimator.rtt()` smooths.
// So the app-echo SRTT is not merely "a clock": it is the MEASURED TIME
// UNTIL THE EVENT THE SWEEP IS WAITING FOR. A sweep cadence below it fires
// on symbols whose acks are not yet due, by construction:
//
//     spurious rounds per tail-blocked symbol = ceil(srtt_app / cadence) − 1
//
// and with the derived (unclamped) law `cadence = 2·srtt_app` that
// expression is ZERO for every srtt > 0 — a theorem about the law, pinned
// below rather than measured.
//
// WHAT THE COMMITTED WIRE SAYS, transcribed (medians over the summary
// records in `docs/l1-raw`; the same rows "The Queue Fix" and "The
// Latency-Feedback Source" read). `srtt_app = rtp_med + q_p50` is those
// sections' own decomposition, re-composed. The fourth column is the
// `ping_p50` LOADED ICMP probe those records also carry and no section had
// read — the only wire-clock evidence in the committed tree, since the DIAG
// line's `wrtt=` field is parsed by `tools/l1/flip_parse.py` and then
// DISCARDED (it reaches no summary record).
const MEASURED: &[(&str, u64, u64, u64)] = &[
    // (cell/arm, RTprop ms, standing queue ms, loaded ICMP p50 ms)
    ("c1-A", 2, 7, 2),
    ("c7-A", 11, 76, 72),
    ("sc2-A", 13, 91, 101),
    ("c8-A", 38, 338, 77),
    ("c8-AU", 40, 424, 82),
];

/// The measured RTT jitter fed to the derived floor: `RWM_RB_JITTER_MS`'s
/// own default (1 ms), i.e. the same number the `pd` arm has used since
/// 2026-08-08 — NOT a constant introduced here.
const JITTER_US: u64 = 1_000;

fn measured(tag: &str) -> (u64, u64) {
    let &(_, rtp, q, w) = MEASURED.iter().find(|r| r.0 == tag).unwrap();
    ((rtp + q) * 1_000, w * 1_000)
}

/// Spurious recovery rounds per tail-blocked symbol at a given cadence: how
/// many times the clock fires before the symbol's OWN ack can arrive.
fn spurious_rounds(srtt_app_us: u64, cadence_us: u64) -> u64 {
    if cadence_us == 0 || srtt_app_us == 0 {
        return 0;
    }
    srtt_app_us.div_ceil(cadence_us).saturating_sub(1)
}

/// The c8 geometry as the driver sees it: RTprop 38 ms, wire queue = the
/// ICMP p50 minus RTprop, dwell = the app-echo clock minus the ICMP p50.
fn c8_calib(n_src: u64) -> Calib {
    Calib {
        n_src,
        mbps: 100.0,
        skew_us: 5_000,
        wireq_us: 39_000,
        dwell_us: 299_000,
        jitter_us: JITTER_US,
        shed: false,
        budget: raptorpath::net::MAX_NACK_REPAIRS_PER_NACK,
    }
}

fn c8_cell() -> Cell {
    Cell {
        rtprop_us: 38_000,
        loss: 0.026,
        pattern: Pattern::Ge,
        n_paths: 2,
        clock: Clock::App,
        arm: ARMS[0],
        seed: 42,
    }
}

/// READOUT (not scored): the cadence and the spurious-round count of every
/// candidate law at every measured geometry, plus the driver at c8.
#[test]
#[ignore = "readout"]
fn derived_clamp_readout() {
    use raptorpath::net::{derived_recovery_round_us, tail_sweep_timeout_us};
    println!("\n=== THE DERIVED RECOVERY CLAMP — cadence at the MEASURED geometry ===");
    println!(
        "srtt_app = rtp_med + q_p50 (docs/l1-raw medians) · wire = loaded ICMP p50 · jitter {} ms",
        JITTER_US / 1_000
    );
    println!(
        "{:<7} {:>9} {:>7} | {:>9} {:>5} | {:>9} {:>5} | {:>9} {:>5} | {:>9} {:>5}",
        "cell", "srtt_app", "wire", "SHIPPED", "spur", "A uncl", "spur", "B wire", "spur", "A+B",
        "spur"
    );
    for &(tag, rtp_ms, q_ms, wire_ms) in MEASURED {
        let s = (rtp_ms + q_ms) * 1_000;
        let w = wire_ms * 1_000;
        let shipped = tail_sweep_timeout_us(s);
        let a = derived_recovery_round_us(s, JITTER_US);
        let b = tail_sweep_timeout_us(w);
        let ab = derived_recovery_round_us(w, JITTER_US);
        println!(
            "{:<7} {:>9} {:>7} | {:>9} {:>5} | {:>9} {:>5} | {:>9} {:>5} | {:>9} {:>5}",
            tag,
            format!("{} ms", s / 1000),
            format!("{} ms", w / 1000),
            format!("{} ms", shipped / 1000),
            spurious_rounds(s, shipped),
            format!("{} ms", a / 1000),
            spurious_rounds(s, a),
            format!("{} ms", b / 1000),
            spurious_rounds(s, b),
            format!("{} ms", ab / 1000),
            spurious_rounds(s, ab),
        );
    }

    println!("\n--- the DRIVER at the c8 geometry (its dwell is a CLOCK, not a delay) ---");
    let cal = c8_calib(6_000);
    let base = c8_cell();
    println!(
        "{:<9} {:>5} | {:>9} {:>9} | {:>6} {:>6} {:>7} | {:>8} {:>8}",
        "arm", "clk", "sweep", "refresh", "holes", "retx", "sweeps", "p50 ms", "p90 ms"
    );
    for (arm_ix, arm_tag) in [(0usize, "shipped"), (4usize, "ds")] {
        for (clock, ctag) in [(Clock::App, "app"), (Clock::Wire, "wire")] {
            let (mut retx, mut sweeps, mut holes) = (0u64, 0u64, 0usize);
            let mut svc: Vec<u64> = Vec::new();
            let (mut sw, mut rf) = (0u64, 0u64);
            for seed in [42u64, 7] {
                let o = run_cell(Cell { arm: ARMS[arm_ix], clock, seed, ..base }, cal);
                retx += o.counts.retx;
                sweeps += o.counts.sweeps;
                holes += o.holes.len();
                sw = raptorpath::net::sweep_timeout_us(
                    ARMS[arm_ix].derived_sweep,
                    o.pooled_us,
                    JITTER_US,
                );
                rf = o.refresh_us;
                svc.extend(
                    o.holes.iter().filter_map(|h| h.first_service_us.map(|t| t - h.lost_at_us)),
                );
            }
            svc.sort_unstable();
            println!(
                "{:<9} {:>5} | {:>9} {:>9} | {:>6} {:>6} {:>7} | {:>8.1} {:>8.1}",
                arm_tag,
                ctag,
                format!("{} ms", sw / 1000),
                format!("{} ms", rf / 1000),
                holes,
                retx,
                sweeps,
                ms(pct(&svc, 0.50)),
                ms(pct(&svc, 0.90)),
            );
        }
    }
}

// ───────────────── always-on pins (the claims, BOUNDED) ──────────────────

/// PIN 1 — THE COINCIDENCE PROPERTY. The derived round reproduces the
/// shipped law EXACTLY over the whole band on which the shipped law's own
/// stated assumption holds (2·srtt inside [25, 100] ms), so the gate is a
/// strict generalization and not a second machine. Outside that band it is
/// the unclamped 2·srtt — which is the point. Also the ROUTING proof
/// (CLAUDE.md discipline rule 1) and the ZERO-NEW-CONSTANTS identity.
#[test]
fn the_derived_round_reproduces_the_shipped_law_inside_the_legacy_band() {
    use raptorpath::net::{
        derived_recovery_round_us, hole_nack_refresh, hole_refresh, patience_floor_us,
        sweep_timeout_us, tail_sweep_timeout_us, HOLE_NACK_REFRESH_MAX, TAIL_SWEEP_MAX_US,
        TAIL_SWEEP_MIN_US,
    };
    // The band where the literal law is not clamping: 2·srtt in [25, 100] ms.
    for srtt_us in (12_500..=50_000).step_by(250) {
        assert_eq!(
            derived_recovery_round_us(srtt_us, JITTER_US),
            tail_sweep_timeout_us(srtt_us),
            "inside the legacy band the derived law must be IDENTICAL (srtt {srtt_us})"
        );
    }
    // Above the ceiling the derived law tracks and the literal one does not.
    for srtt_us in [50_001u64, 87_000, 104_000, 376_000, 464_000] {
        assert_eq!(tail_sweep_timeout_us(srtt_us), TAIL_SWEEP_MAX_US, "literal pins at 100 ms");
        assert_eq!(derived_recovery_round_us(srtt_us, JITTER_US), 2 * srtt_us);
    }
    // Below the floor the derived law is the DERIVED patience floor
    // (`TIMER_GRANULARITY_US` + measured jitter), not the 25 ms literal.
    assert_eq!(tail_sweep_timeout_us(4_000), TAIL_SWEEP_MIN_US);
    assert_eq!(derived_recovery_round_us(4_000, JITTER_US), 8_000);
    assert_eq!(
        derived_recovery_round_us(100, JITTER_US),
        1_100,
        "2·srtt below the derived floor ⇒ the floor (1 ms granularity + 100 µs jitter)"
    );
    // The GATE routes, and OFF is byte-identical to the shipped law.
    for srtt_us in [0u64, 4_000, 30_000, 376_000] {
        assert_eq!(sweep_timeout_us(false, srtt_us, JITTER_US), tail_sweep_timeout_us(srtt_us));
        assert_eq!(
            sweep_timeout_us(true, srtt_us, JITTER_US),
            derived_recovery_round_us(srtt_us, JITTER_US)
        );
        let d = std::time::Duration::from_micros(srtt_us);
        assert_eq!(hole_refresh(false, Some(d), JITTER_US), hole_nack_refresh(Some(d)));
        assert_eq!(
            hole_refresh(true, Some(d), JITTER_US).as_micros() as u64,
            derived_recovery_round_us(srtt_us, JITTER_US)
        );
    }
    // No clock at all ⇒ the legacy fallback in BOTH arms (an
    // information-availability fallback, not a mode).
    assert_eq!(hole_refresh(false, None, JITTER_US), HOLE_NACK_REFRESH_MAX);
    assert_eq!(hole_refresh(true, None, JITTER_US), HOLE_NACK_REFRESH_MAX);
    // ZERO NEW CONSTANTS: the derived round is expressible with only the
    // shipped multiplier and the already-derived patience floor.
    for srtt_us in [0u64, 1, 999, 12_500, 376_000] {
        assert_eq!(
            derived_recovery_round_us(srtt_us, JITTER_US),
            (2 * srtt_us).max(patience_floor_us(JITTER_US, srtt_us))
        );
    }
}

/// PIN 2 — THE DEAD WALL'S QUANTUM, and the cell-keying that says the
/// reading is about c8's queue and not about the law. At every transcribed
/// geometry the shipped cadence and its spurious-round count are ABSOLUTE
/// arithmetic; c1 sits on the FLOOR with zero spurious rounds (the control),
/// c8 sits on the CEILING with three.
#[test]
fn the_shipped_ceiling_generates_the_spurious_rounds_and_c1_is_the_control() {
    use raptorpath::net::{
        derived_recovery_round_us, tail_sweep_timeout_us, TAIL_SWEEP_MAX_US, TAIL_SWEEP_MIN_US,
    };
    // c1: 2·9 ms = 18 ms is BELOW the literal floor, so the shipped clock is
    // the 25 ms floor — and it is still ABOVE the 9 ms ack, so ZERO spurious
    // rounds. The dead wall is not a property of the law.
    let (c1, _) = measured("c1-A");
    assert_eq!(c1, 9_000);
    assert_eq!(tail_sweep_timeout_us(c1), TAIL_SWEEP_MIN_US);
    assert_eq!(spurious_rounds(c1, tail_sweep_timeout_us(c1)), 0, "c1 is the control");
    // c7 / sc2: pinned at the ceiling, 0 and 1 spurious rounds.
    let (c7, _) = measured("c7-A");
    assert_eq!(c7, 87_000);
    assert_eq!(tail_sweep_timeout_us(c7), TAIL_SWEEP_MAX_US);
    assert_eq!(spurious_rounds(c7, TAIL_SWEEP_MAX_US), 0);
    let (sc2, _) = measured("sc2-A");
    assert_eq!(spurious_rounds(sc2, TAIL_SWEEP_MAX_US), 1);
    // c8: the overshoot and its round count.
    let (c8, _) = measured("c8-A");
    assert_eq!(c8, 376_000, "38 + 338, the section's own decomposition");
    assert_eq!(tail_sweep_timeout_us(c8), TAIL_SWEEP_MAX_US);
    assert!(
        c8 as f64 / TAIL_SWEEP_MAX_US as f64 > 3.5,
        "c8 must overshoot the ceiling by >3.5x: {}",
        c8 as f64 / TAIL_SWEEP_MAX_US as f64
    );
    assert_eq!(
        spurious_rounds(c8, TAIL_SWEEP_MAX_US),
        3,
        "three sweeps fire on a tail-blocked c8 symbol before its own ack is due"
    );
    // And the derived law removes them ALL, at every geometry, by arithmetic
    // — `ceil(s / 2s) − 1 == 0` for every s > 0.
    for &(tag, rtp, q, _) in MEASURED {
        let s = (rtp + q) * 1_000;
        assert_eq!(
            spurious_rounds(s, derived_recovery_round_us(s, JITTER_US)),
            0,
            "{tag}: the derived round can never fire before the ack is due"
        );
    }
    for s in [1u64, 7, 1_000, 9_000, 376_000, 1_000_000, 1 << 30] {
        assert_eq!(spurious_rounds(s, derived_recovery_round_us(s, JITTER_US)), 0);
    }
}

/// PIN 3 — **REPAIR B IS REFUTED AT THIS CLOCK, and that is the §16.40
/// discipline paying off in the other direction.** Feeding the tail sweep
/// the WIRE RTT does not remove the spurious rounds, because the event the
/// sweep waits for is the APP-ECHO ack — the same instant-to-instant
/// difference `estimator.rtt()` measures. Wire-clocking under the shipped
/// ceiling changes NOTHING at c8 (2·77 ms is still above 100 ms, still
/// clamped); wire-clocking with the ceiling removed still leaves rounds
/// firing early. Only the app-echo argument WITH the ceiling removed is
/// spurious-free.
#[test]
fn wire_clocking_the_tail_sweep_does_not_lift_the_c8_clamp() {
    use raptorpath::net::{derived_recovery_round_us, tail_sweep_timeout_us};
    let (s, w) = measured("c8-A"); // 376 ms when the ack is due; 77 ms wire
    assert!(w < s / 4, "the wire clock is <1/4 the app-echo clock at c8: {w} vs {s}");

    // REPAIR B alone, under the shipped ceiling: identical to the shipped
    // arm, to the microsecond. 2·77 = 154 ms is still clamped to 100.
    assert_eq!(
        tail_sweep_timeout_us(w),
        tail_sweep_timeout_us(s),
        "wire-clocking under the ceiling is a NO-OP at c8"
    );
    assert_eq!(spurious_rounds(s, tail_sweep_timeout_us(w)), 3);

    // REPAIR A + B: better than shipped, still not zero.
    let ab = derived_recovery_round_us(w, JITTER_US);
    assert_eq!(ab, 154_000);
    assert_eq!(spurious_rounds(s, ab), 2, "wire + unclamp still fires early at c8");

    // REPAIR A on the APP-ECHO argument: zero, and it is the only one.
    let a = derived_recovery_round_us(s, JITTER_US);
    assert_eq!(a, 752_000);
    assert_eq!(spurious_rounds(s, a), 0);

    // The ORDERING is the verdict, stated as an absolute chain.
    let shipped = spurious_rounds(s, tail_sweep_timeout_us(s));
    let b_only = spurious_rounds(s, tail_sweep_timeout_us(w));
    assert_eq!((shipped, b_only, spurious_rounds(s, ab), spurious_rounds(s, a)), (3, 3, 2, 0));
}

/// PIN 4 — **THE DELETION CHAIN'S LOAD-BEARING QUESTION.** The `U` arm
/// (deeper pool) raises the store dwell, which raises the app-echo SRTT the
/// clock reads — measured, c8-A 376 → c8-AU 464 ms. Under the SHIPPED
/// ceiling the cadence cannot move (both pinned at 100 ms), so the deeper
/// pool buys STRICTLY MORE spurious rounds: 3 → 4. Under the derived round
/// the cadence tracks the pool (752 → 928 ms) and the spurious count is 0 at
/// both. **In this model the repair removes the deeper-pool sensitivity of
/// the recovery clock outright** — the property `AU`'s c8 arm needs before
/// it can be considered safe.
#[test]
fn the_derived_round_removes_the_deeper_pools_clock_penalty_and_the_ceiling_does_not() {
    use raptorpath::net::{derived_recovery_round_us, tail_sweep_timeout_us};
    let (a, _) = measured("c8-A");
    let (au, _) = measured("c8-AU");
    assert_eq!((a, au), (376_000, 464_000));
    assert!(au > a, "the deeper pool raises the dwell-inclusive clock");

    // SHIPPED: the cadence is IDENTICAL at both arms — the clock is blind to
    // the pool — while the spurious count rises with it.
    assert_eq!(tail_sweep_timeout_us(a), tail_sweep_timeout_us(au));
    assert_eq!(spurious_rounds(a, tail_sweep_timeout_us(a)), 3);
    assert_eq!(
        spurious_rounds(au, tail_sweep_timeout_us(au)),
        4,
        "the deeper pool buys one MORE spurious round under the ceiling"
    );

    // DERIVED: the cadence tracks the pool exactly, and the penalty is gone.
    let (da, dau) =
        (derived_recovery_round_us(a, JITTER_US), derived_recovery_round_us(au, JITTER_US));
    assert_eq!((da, dau), (752_000, 928_000));
    assert!(dau > da, "the derived cadence MOVES with the pool");
    assert_eq!(spurious_rounds(a, da), 0);
    assert_eq!(spurious_rounds(au, dau), 0);
    // Stated as the invariant rather than the two instances: for ANY dwell
    // the derived clock's spurious count is 0, so no store-cap arm can move
    // it. That is what makes the interaction safe rather than merely better.
    for dwell_ms in (0u64..=1_000).step_by(7) {
        let s = 38_000 + dwell_ms * 1_000;
        assert_eq!(spurious_rounds(s, derived_recovery_round_us(s, JITTER_US)), 0);
    }
}

/// PIN 5 — LIVENESS at the driver: the `ds` arm actually executes and
/// actually moves the plane's two cadences at the c8 geometry, with the
/// wire, the hole count and the clock argument held identical (CLAUDE.md
/// discipline rule 1). This BOUNDS the model claim; it does not score it —
/// the driver's dwell is a CLOCK, not a delay (see the header's boundary),
/// so its ack turnaround is the wire RTT and the spurious-round arithmetic
/// above is NOT reproduced here. Recorded as exactly that.
#[test]
fn the_derived_sweep_arm_executes_and_moves_both_cadences_at_c8() {
    let cal = c8_calib(2_000);
    let base = c8_cell();
    let shipped = run_cell(base, cal);
    let ds = run_cell(Cell { arm: ARMS[4], ..base }, cal);
    assert_eq!(ARMS[4].name, "ds");
    assert!(ARMS[4].derived_sweep && !ARMS[0].derived_sweep);

    // Same wire: the A/B is over the LAWS, never over the losses.
    assert_eq!(shipped.holes.len(), ds.holes.len());
    assert!(!shipped.holes.is_empty(), "the plane must have holes to recover");
    assert_eq!(shipped.pooled_us, ds.pooled_us, "the clock ARGUMENT is unchanged by this gate");

    // The receiver's refresh reads the COPA (wire) clock; the sender's sweep
    // reads the pooled app-echo one. Both pinned at the ceiling shipped.
    assert_eq!(shipped.refresh_us, 100_000, "shipped refresh pinned at the ceiling");
    assert!(ds.refresh_us > shipped.refresh_us, "derived refresh tracks the wire clock");
    assert_eq!(
        raptorpath::net::sweep_timeout_us(false, shipped.pooled_us, JITTER_US),
        100_000
    );
    assert_eq!(
        raptorpath::net::sweep_timeout_us(true, ds.pooled_us, JITTER_US),
        2 * ds.pooled_us
    );

    // And the gate is NOT inert. (`RWM_PATIENCE_DERIVED` was measurably
    // inert at this bench; this one is not, which is worth pinning.)
    assert!(
        ds.counts.sweeps < shipped.counts.sweeps,
        "the derived arm must fire FEWER sweeps: {} vs {}",
        ds.counts.sweeps,
        shipped.counts.sweeps
    );
    // Every arm still recovers: no hole is abandoned by making the clock
    // honest (the receiver's refresh, not the sweep, is the recovery engine).
    for (tag, o) in [("shipped", &shipped), ("ds", &ds)] {
        assert!(o.counts.retx > 0, "{tag}: no retransmit fired");
        let served = o.holes.iter().filter(|h| h.first_service_us.is_some()).count();
        assert!(
            served * 100 >= o.holes.len() * 95,
            "{tag}: only {served}/{} holes reached a channel",
            o.holes.len()
        );
    }
}

// ════════════════════════════════════════════════════════════════════════
// THE R AXIS, COMPONENT-VERIFIED — paper §16.67, §16.67.1, §16.68
//
// Four rival laws for ONE quantity, scored at the same five MEASURED
// geometries on the same arithmetic, plus the FALSE-ALARM RATE each is
// predicted to produce against RFC 8985 §6.2 Step 4's own published budget.
//
// The headline is about the CONTROL, not the successors: the shipped
// `[25, 100] ms` clamp is predicted to violate RACK's own spurious budget by
// 8–13× at three of five cells, and that number has never been measured in
// this tree.
// ════════════════════════════════════════════════════════════════════════

use raptorpath::net::{
    contract_alpha, derived_recovery_round_us, quantile_recovery_round_us,
    rack_recovery_round_us, tail_sweep_timeout_us, RACK_REO_WND_MULT_MAX,
    RACK_SPURIOUS_BUDGET,
};

/// The predicted false-alarm FRACTION from the component model: a cadence that
/// fires `n` extra times per ack round trip wastes `n` of the `n+1` fires.
fn false_alarm_frac(srtt_us: u64, cadence_us: u64) -> f64 {
    let sp = spurious_rounds(srtt_us, cadence_us);
    sp as f64 / (sp + 1) as f64
}

/// **§16.67 + §16.67.1, PINNED.** The whole R axis at the five measured
/// geometries: cadence, spurious rounds and the predicted false-alarm rate for
/// the shipped clamp, `RWM_DERIVED_SWEEP`, `RWM_RACK_CLOCKS` at both ends of
/// RACK's own multiplier range, and `RWM_QUANTILE_CLOCKS`.
///
/// Every number here is published in §16.67/§16.67.1 BEFORE this test existed.
/// The test's job is to make the publication falsifiable: if a law changes,
/// the paper's table goes red rather than stale.
#[test]
fn the_r_axis_component_arithmetic_is_what_the_paper_publishes() {
    // ── The SENDER site (app-echo clock). §16.67's first table. ────────
    // (cell, srtt_app, min_rtt, shipped sp, derived sp, rack mult=1 sp,
    //  rack mult=17 sp)
    for &(name, srtt, mrtt, ship_sp, ds_sp, r1_sp, r17_sp) in &[
        ("c1-A", 9_000u64, 2_000u64, 0u64, 0u64, 8u64, 1u64),
        ("c7-A", 87_000, 11_000, 0, 0, 31, 1),
        ("sc2-A", 104_000, 13_000, 1, 0, 31, 1),
        ("c8-A", 376_000, 38_000, 3, 0, 39, 2),
        ("c8-AU", 464_000, 40_000, 4, 0, 46, 2),
    ] {
        let ship = tail_sweep_timeout_us(srtt);
        let ds = derived_recovery_round_us(srtt, JITTER_US);
        let r1 = rack_recovery_round_us(srtt, mrtt, 1);
        let r17 = rack_recovery_round_us(srtt, mrtt, RACK_REO_WND_MULT_MAX);
        assert_eq!(spurious_rounds(srtt, ship), ship_sp, "{name}: shipped clamp");
        assert_eq!(spurious_rounds(srtt, ds), ds_sp, "{name}: RWM_DERIVED_SWEEP");
        assert_eq!(spurious_rounds(srtt, r1), r1_sp, "{name}: RACK mult=1");
        assert_eq!(spurious_rounds(srtt, r17), r17_sp, "{name}: RACK mult=17");

        // §16.67 result 1: the faithful transplant is TIGHTER than the clamp
        // it replaces at every measured cell, by 8–46× in spurious rounds.
        assert!(r1 < ship, "{name}: RACK mult=1 is not tighter than the shipped clamp");
        // §16.67 result 2: RACK's own ceiling is unreachable within its own
        // multiplier range at the sender site — mult=17 still does not bind it.
        assert!(
            r17 < srtt,
            "{name}: the SRTT ceiling became reachable at RACK's own maximum — \
             §16.67's central finding no longer holds"
        );
    }

    // ── §16.67.1's FALSE-ALARM table, and the finding about the CONTROL. ──
    let mut control_violations = 0;
    for &(name, srtt, mrtt, want_ship_fa) in &[
        ("c1-A", 9_000u64, 2_000u64, 0.00f64),
        ("c7-A", 87_000, 11_000, 0.00),
        ("sc2-A", 104_000, 13_000, 0.50),
        ("c8-A", 376_000, 38_000, 0.75),
        ("c8-AU", 464_000, 40_000, 0.80),
    ] {
        let fa_ship = false_alarm_frac(srtt, tail_sweep_timeout_us(srtt));
        assert!((fa_ship - want_ship_fa).abs() < 1e-9, "{name}: shipped fa = {fa_ship}");
        if fa_ship > RACK_SPURIOUS_BUDGET {
            control_violations += 1;
        }
        // The derived arm clears the budget BY BEING SLOW — recorded so the
        // trade §16.53 measured is visible in the arithmetic, not only in prose.
        let fa_ds = false_alarm_frac(srtt, derived_recovery_round_us(srtt, JITTER_US));
        assert!(fa_ds <= RACK_SPURIOUS_BUDGET, "{name}: DERIVED_SWEEP no longer clears the budget");
        // The RACK arm violates it everywhere at its own initial multiplier.
        let fa_r1 = false_alarm_frac(srtt, rack_recovery_round_us(srtt, mrtt, 1));
        assert!(
            fa_r1 > RACK_SPURIOUS_BUDGET,
            "{name}: RACK mult=1 now clears its own budget — §16.67.1 needs rewriting"
        );
    }
    assert_eq!(
        control_violations, 3,
        "§16.67.1 publishes THREE cells where the SHIPPED clamp violates RFC 8985's \
         own <7 % spurious budget; the arithmetic now says {control_violations}"
    );

    // ── The RECEIVER site (wire clock). §16.67's second table, and the ONE
    // row that behaves as the cross-check's backlog item predicted. ───────
    for &(name, srtt_w, mrtt, r17_cadence, r17_sp) in &[
        ("c8-A", 77_000u64, 38_000u64, 77_000u64, 0u64),
        ("c8-AU", 82_000, 40_000, 82_000, 0),
    ] {
        let r17 = rack_recovery_round_us(srtt_w, mrtt, RACK_REO_WND_MULT_MAX);
        assert_eq!(r17, r17_cadence, "{name}: the ceiling row's cadence");
        assert_eq!(r17, srtt_w, "{name}: the SRTT ceiling is not the binder");
        assert_eq!(spurious_rounds(srtt_w, r17), r17_sp, "{name}: the ceiling row is not clean");
        // "tracking the cadence without the unbounded growth" — the shape the
        // cross-check described, asserted against the unbounded law it replaces.
        assert!(
            r17 < derived_recovery_round_us(srtt_w, JITTER_US),
            "{name}: the bounded law is not below the unbounded one"
        );
    }

    // ── §16.68's REFUTATION, in the same arithmetic. ─────────────────
    // The derived quantile clock at the contract's own α is SLOWER than the
    // already-slow unbounded arm at every cell, which is reason 1 stated as a
    // comparison rather than as a number.
    let alpha = contract_alpha(raptorpath::control::fec_rate::ProtocolHint::Auto);
    for &(name, srtt, sigma) in &[
        ("c1-A", 9_000u64, 1_000u64),
        ("c8-A", 376_000, 10_000),
    ] {
        let w = quantile_recovery_round_us(srtt, sigma, alpha);
        assert!(
            w > derived_recovery_round_us(srtt, JITTER_US),
            "{name}: §16.68's reason 1 (the bound is unusably loose) no longer holds"
        );
        assert_eq!(spurious_rounds(srtt, w), 0, "{name}: the derived clock should never false-alarm");
    }
}

/// **§16.67, ROUTING.** The four laws are RIVALS and the precedence is the
/// published one — quantile ≻ RACK ≻ derived ≻ shipped — at BOTH sites, with
/// each falling back to the next when its own input is unavailable.
#[test]
fn the_r_axis_precedence_is_explicit_and_falls_back_on_information_not_on_a_mode() {
    use raptorpath::net::{hole_refresh_all, sweep_timeout_us_all, hole_nack_refresh};
    use std::time::Duration;
    let (srtt, mrtt, sigma) = (376_000u64, 38_000u64, 10_000u64);
    let a = contract_alpha(raptorpath::control::fec_rate::ProtocolHint::Auto);
    let sw = |q, r, d, m, sg| sweep_timeout_us_all(q, r, d, srtt, JITTER_US, m, sg, 1, a);

    // All OFF is the shipped law, byte-identically.
    assert_eq!(sw(false, false, false, Some(mrtt), Some(sigma)), tail_sweep_timeout_us(srtt));
    // Each law wins over the ones below it, with every input available.
    assert_eq!(sw(false, false, true, Some(mrtt), Some(sigma)), derived_recovery_round_us(srtt, JITTER_US));
    assert_eq!(sw(false, true, true, Some(mrtt), Some(sigma)), rack_recovery_round_us(srtt, mrtt, 1));
    assert_eq!(sw(true, true, true, Some(mrtt), Some(sigma)), quantile_recovery_round_us(srtt, sigma, a));
    // FALLBACK IS ON INFORMATION, NOT ON A MODE: with the law's own input
    // missing it drops to the next armed law, never to an unarmed one.
    assert_eq!(sw(true, true, true, Some(mrtt), None), rack_recovery_round_us(srtt, mrtt, 1));
    assert_eq!(sw(true, true, true, None, None), derived_recovery_round_us(srtt, JITTER_US));
    assert_eq!(sw(true, false, false, None, None), tail_sweep_timeout_us(srtt));

    // The receiver router, same property.
    let sd = Some(Duration::from_micros(srtt));
    let md = Some(Duration::from_micros(mrtt));
    let hr = |q, r, d, m, sg| hole_refresh_all(q, r, d, sd, JITTER_US, m, sg, 1, a);
    assert_eq!(hr(false, false, false, md, Some(sigma)), hole_nack_refresh(sd));
    assert_eq!(
        hr(true, true, true, md, Some(sigma)),
        Duration::from_micros(quantile_recovery_round_us(srtt, sigma, a))
    );
    assert_eq!(
        hr(false, true, true, md, Some(sigma)),
        Duration::from_micros(rack_recovery_round_us(srtt, mrtt, 1))
    );
    assert_eq!(hr(true, false, false, None, None), hole_nack_refresh(sd));
}
