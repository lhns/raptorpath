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
