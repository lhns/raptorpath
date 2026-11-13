//! Component bench for the recovery plane alone (paper §7.1). No CC, no
//! scheduler, no multipath placement, no transport, no tokio: a deterministic
//! discrete-event driver feeds synthetic arrival/loss patterns to the shipped
//! recovery laws (`raptorpath::net::*`) and records, per hole, when and why it
//! was served. It pins the plane's clock laws and measured behaviour at the
//! c7 operating point, and the recovery-clock laws (the shipped clamp and the
//! quantile-native window law, paper §7.4) at their
//! published anchor points. It serves `docs/measurement-discipline.md`
//! rule 14: characterize the mechanism at component level before any L1
//! battery.
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
//!   RWM_RB_ARMS       shipped,legacy,sp    gate arms (see `ARMS`)
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
//! THE CLOCK ARGUMENT:
//! every recovery clock in the plane is fed by `pooled_recovery_srtt_us` and
//! by the per-path `(copa_srtt, estimator_rtt)` pair. The estimator's
//! app-echo RTT is STORE-DWELL INCLUSIVE — measured 158 ms at c7 against an
//! 8–10 ms RTprop — while `QuicTransport::wire_rtt` (ADR-0062)
//! excludes the dwell. `RWM_RB_CLOCK=app` feeds the dwell-inclusive clock,
//! `wire` the dwell-free one. NOTHING else differs between the arms.
//!
//! WHAT THIS BENCH CANNOT SEE:
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

// The driver, the axes, the loss model and the shipped-law calls live in
// `tests/common/recovery_model.rs` so that `tests/slack_bench.rs` can take
// this plane's STALL DISTRIBUTION as its input rather than invent one.
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
    let arm_names = list_str("RWM_RB_ARMS", "shipped,legacy,sp");
    let arms: Vec<Arm> =
        ARMS.iter().copied().filter(|a| arm_names.iter().any(|n| n == a.name)).collect();
    let seeds = list_u64("RWM_RB_SEEDS", "42,7");

    println!("\n=== RECOVERY PLANE COMPONENT BENCH ===");
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
        println!("\n--- arm `{}` (recov_mp={} recov_sp={}) ---",
            arm.name, arm.recov_mp, arm.recov_sp);
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
    let fx = fixture_for_horizon(gap_horizon());
    if std::env::var("RWM_FIX_PRINT").is_ok() {
        eprintln!("FIXPRINT horizon={} holes={} app_mix={:?} app_retx={} app_sweeps={} app_p50={} wire_mix={:?} wire_retx={} wire_sweeps={} wire_p50={}",
            gap_horizon(), a.holes.len(), mix(&a), a.counts.retx, a.counts.sweeps, p50(&a), mix(&w), w.counts.retx, w.counts.sweeps, p50(&w));
    }
    assert_eq!(a.holes.len(), fx.holes);
    assert_eq!(mix(&a), fx.app_mix, "app-echo channel mix (time/fast/age/sweep/shed)");
    assert_eq!(a.counts.retx, fx.app_retx);
    assert_eq!(a.counts.sweeps, fx.app_sweeps);
    assert_eq!(p50(&a), fx.app_p50_us);
    assert_eq!(mix(&w), fx.wire_mix, "wire channel mix");
    assert_eq!(w.counts.retx, fx.wire_retx);
    assert_eq!(w.counts.sweeps, fx.wire_sweeps);
    assert_eq!(p50(&w), fx.wire_p50_us);
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
// The driver acks on arrival subject to the 2 ms `GAP_ACK_MIN_US` floor, as
// the shipped receiver does; it does not defer the first advertisement by the
// hole-refresh cadence before any hole was NACKed.
/// One horizon's fixture: every value a bench output at that engine gap horizon.
struct Fix {
    holes: usize,
    app_mix: [u64; 5],
    app_retx: u64,
    app_sweeps: u64,
    app_p50_us: u64,
    wire_mix: [u64; 5],
    wire_retx: u64,
    wire_sweeps: u64,
    wire_p50_us: u64,
}

/// The fixture is a PIN of the model's plane, and the model reads the engine's
/// gap horizon (`gap_horizon()`): the 20-gap set is the bench output captured
/// on the 20-gap plane, the 64-gap set is a bench output captured at `cand53`
/// (lever B: `sack_to_gaps` bounded by `MAX_SACK_RANGES`). A third horizon
/// fails here instead of silently using one of the sets.
fn fixture_for_horizon(h: usize) -> Fix {
    match h {
        20 => Fix {
            holes: FIX_HOLES,
            app_mix: FIX_APP_MIX,
            app_retx: FIX_APP_RETX,
            app_sweeps: FIX_APP_SWEEPS,
            app_p50_us: FIX_APP_P50_US,
            wire_mix: FIX_WIRE_MIX,
            wire_retx: FIX_WIRE_RETX,
            wire_sweeps: FIX_WIRE_SWEEPS,
            wire_p50_us: FIX_WIRE_P50_US,
        },
        64 => Fix {
            holes: 154,
            app_mix: [70, 84, 0, 0, 0],
            app_retx: 154,
            app_sweeps: 2,
            app_p50_us: 12_536,
            wire_mix: [70, 84, 0, 0, 0],
            wire_retx: 156,
            wire_sweeps: 2,
            wire_p50_us: 12_536,
        },
        other => panic!("recovery_bench fixture 2 has bench outputs for gap horizons 20 and 64 only, the engine reports {other}: capture a set"),
    }
}

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
//  THE DERIVED RECOVERY CLAMP — the two recovery clocks' [25 ms, 100 ms]
//  clamp on trial (paper §7.1).
// ═════════════════════════════════════════════════════════════════════════
//
// THE FINDING BEING ACTED ON: L1 attributed c8's collapse mode to an APPENDED DEAD WALL — ~30 % of a
// collapse rep's wall with the sender neither reading source, nor blocked on
// its store cap, nor sending — and named its quantum as the recovery clocks:
// `tail_sweep_timeout_us` and `hole_nack_refresh` are both `2·SRTT` clamped
// to [25, 100] ms, and c8's measured SRTT overshoots the ceiling 7.5×.
//
// THE QUESTION, argument before constant: is the CEILING wrong, or is the ARGUMENT wrong? Both repairs
// are evaluated here SEPARATELY and then composed:
//
//   REPAIR A — derive the clamp away.  (`RWM_DERIVED_SWEEP`: removed after
//              its verdict, tag `archive/RWM_DERIVED_SWEEP`; paper §10.)
//   REPAIR B — change the argument.    Feed the clock the WIRE RTT (Copa /
//              `QuicTransport::wire_rtt`) instead of the store-dwell-
//              inclusive app-echo RTT.
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
// and with an unclamped `cadence = 2·srtt_app` that expression is ZERO for
// every srtt > 0 — the argument that motivated the archived
// `RWM_DERIVED_SWEEP` arm (paper §10); the shipped values are pinned below.
//
// WHAT THE WIRE SAYS, transcribed (medians over the L1 summary records).
// `srtt_app = rtp_med + q_p50`, the records' own decomposition, re-composed.
// The fourth column is the `ping_p50` loaded ICMP probe those records also
// carry — the only wire-clock evidence in them.
const MEASURED: &[(&str, u64, u64, u64)] = &[
    // (cell/arm, RTprop ms, standing queue ms, loaded ICMP p50 ms)
    ("c1-A", 2, 7, 2),
    ("c7-A", 11, 76, 72),
    ("sc2-A", 13, 91, 101),
    ("c8-A", 38, 338, 77),
    ("c8-AU", 40, 424, 82),
];

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

// ───────────────── always-on pins (the claims, BOUNDED) ──────────────────

/// PIN 1 — THE SHIPPED CLOCKS, pinned absolutely: the tail sweep and the
/// receiver's hole-refresh cadence are `2·srtt` clamped to [25, 100] ms, with
/// the no-clock fallback at the ceiling (paper §7.4).
#[test]
fn the_shipped_recovery_clocks_are_pinned_absolutely() {
    use raptorpath::net::{
        hole_nack_refresh, tail_sweep_timeout_us, HOLE_NACK_REFRESH_MAX, HOLE_NACK_REFRESH_MIN,
        TAIL_SWEEP_MAX_US, TAIL_SWEEP_MIN_US,
    };
    for srtt_us in [12_500u64, 30_000, 50_000] {
        assert_eq!(tail_sweep_timeout_us(srtt_us), 2 * srtt_us, "inside the band (srtt {srtt_us})");
    }
    for srtt_us in [50_001u64, 87_000, 104_000, 376_000, 464_000] {
        assert_eq!(tail_sweep_timeout_us(srtt_us), TAIL_SWEEP_MAX_US, "pins at 100 ms");
    }
    assert_eq!(tail_sweep_timeout_us(4_000), TAIL_SWEEP_MIN_US);
    for srtt_us in [
        0u64, 1, 999, 1_000, 2_000, 4_000, 12_499, 12_500, 12_501, 20_000, 25_000, 49_999,
        50_000, 50_001, 100_000, 376_000, 1_000_000,
    ] {
        let d = std::time::Duration::from_micros(srtt_us);
        let shipped = (d * 2).clamp(HOLE_NACK_REFRESH_MIN, HOLE_NACK_REFRESH_MAX);
        assert_eq!(hole_nack_refresh(Some(d)), shipped, "srtt = {srtt_us} us (paper §7.4)");
    }
    assert_eq!(hole_nack_refresh(None), HOLE_NACK_REFRESH_MAX);
    // The cadence the shipped law delivers at the transcribed operating
    // points: the cell's effective floor (lower rail at c1, upper rail at
    // c7 and sc2).
    for (cell, srtt_us, shipped_us) in
        [("c1", 2_000u64, 25_000u64), ("c7", 50_000, 100_000), ("sc2", 50_000, 100_000)]
    {
        let d = std::time::Duration::from_micros(srtt_us);
        assert_eq!(
            hole_nack_refresh(Some(d)).as_micros() as u64,
            shipped_us,
            "{cell}: the shipped cadence at this operating point must be {shipped_us} us"
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
    use raptorpath::net::{tail_sweep_timeout_us, TAIL_SWEEP_MAX_US, TAIL_SWEEP_MIN_US};
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
}

/// PIN 3 — **REPAIR B IS NO-OP UNDER THE SHIPPED CEILING.** Feeding the tail
/// sweep the WIRE RTT does not remove the spurious rounds, because the event
/// the sweep waits for is the APP-ECHO ack — the same instant-to-instant
/// difference `estimator.rtt()` measures. Wire-clocking under the shipped
/// ceiling changes NOTHING at c8 (2·77 ms is still above 100 ms, still
/// clamped). (The removed `RWM_DERIVED_SWEEP` arm, which lifted the ceiling,
/// is archived with its verdict; paper §10.)
#[test]
fn wire_clocking_the_tail_sweep_does_not_lift_the_c8_clamp() {
    use raptorpath::net::tail_sweep_timeout_us;
    let (s, w) = measured("c8-A"); // 376 ms when the ack is due; 77 ms wire
    assert!(w < s / 4, "the wire clock is <1/4 the app-echo clock at c8: {w} vs {s}");
    assert_eq!(
        tail_sweep_timeout_us(w),
        tail_sweep_timeout_us(s),
        "wire-clocking under the ceiling is a NO-OP at c8"
    );
    assert_eq!(spurious_rounds(s, tail_sweep_timeout_us(w)), 3);
}

/// PIN 4 — The `U` arm (deeper pool) raises the store dwell, which raises
/// the app-echo SRTT the clock reads — measured, c8-A 376 → c8-AU 464 ms.
/// Under the SHIPPED ceiling the cadence cannot move (both pinned at 100 ms),
/// so the deeper pool buys STRICTLY MORE spurious rounds: 3 → 4.
#[test]
fn the_ceiling_is_blind_to_the_deeper_pool_so_it_buys_more_spurious_rounds() {
    use raptorpath::net::tail_sweep_timeout_us;
    let (a, _) = measured("c8-A");
    let (au, _) = measured("c8-AU");
    assert_eq!((a, au), (376_000, 464_000));
    assert!(au > a, "the deeper pool raises the dwell-inclusive clock");
    assert_eq!(tail_sweep_timeout_us(a), tail_sweep_timeout_us(au));
    assert_eq!(spurious_rounds(a, tail_sweep_timeout_us(a)), 3);
    assert_eq!(
        spurious_rounds(au, tail_sweep_timeout_us(au)),
        4,
        "the deeper pool buys one MORE spurious round under the ceiling"
    );
}

// ════════════════════════════════════════════════════════════════════════
// THE R AXIS, COMPONENT-VERIFIED — paper §7.1
//
// The shipped clamp, scored at the five MEASURED geometries, plus the
// FALSE-ALARM RATE it is predicted to produce against RFC 8985 §6.2 Step 4's
// own published budget. (The rival arms this axis also scored —
// `RWM_DERIVED_SWEEP`, `RWM_RACK_CLOCKS`, `RWM_QUANTILE_CLOCKS` — were refuted
// and removed with their gates; paper §10.)
//
// The headline is about the CONTROL, not the successors: the shipped
// `[25, 100] ms` clamp is predicted to violate RACK's own spurious budget by
// 8–13× at three of five cells.
// ════════════════════════════════════════════════════════════════════════

use raptorpath::net::{tail_sweep_timeout_us, RACK_SPURIOUS_BUDGET};

/// The predicted false-alarm FRACTION from the component model: a cadence that
/// fires `n` extra times per ack round trip wastes `n` of the `n+1` fires.
fn false_alarm_frac(srtt_us: u64, cadence_us: u64) -> f64 {
    let sp = spurious_rounds(srtt_us, cadence_us);
    sp as f64 / (sp + 1) as f64
}

/// The R axis at the five measured geometries, pinned: cadence, spurious
/// rounds and the predicted false-alarm rate for the shipped clamp. If a law changes, the published numbers go red rather
/// than stale.
#[test]
fn the_r_axis_component_arithmetic_is_what_the_paper_publishes() {
    // ── The SENDER site (app-echo clock). ──────────────────────────────
    // (cell, srtt_app, shipped sp)
    for &(name, srtt, ship_sp) in &[
        ("c1-A", 9_000u64, 0u64),
        ("c7-A", 87_000, 0),
        ("sc2-A", 104_000, 1),
        ("c8-A", 376_000, 3),
        ("c8-AU", 464_000, 4),
    ] {
        let ship = tail_sweep_timeout_us(srtt);
        assert_eq!(spurious_rounds(srtt, ship), ship_sp, "{name}: shipped clamp");
    }

    // ── The FALSE-ALARM table, and the finding about the CONTROL. ──────
    let mut control_violations = 0;
    for &(name, srtt, want_ship_fa) in &[
        ("c1-A", 9_000u64, 0.00f64),
        ("c7-A", 87_000, 0.00),
        ("sc2-A", 104_000, 0.50),
        ("c8-A", 376_000, 0.75),
        ("c8-AU", 464_000, 0.80),
    ] {
        let fa_ship = false_alarm_frac(srtt, tail_sweep_timeout_us(srtt));
        assert!((fa_ship - want_ship_fa).abs() < 1e-9, "{name}: shipped fa = {fa_ship}");
        if fa_ship > RACK_SPURIOUS_BUDGET {
            control_violations += 1;
        }
    }
    assert_eq!(
        control_violations, 3,
        "paper §7.1 publishes THREE cells where the SHIPPED clamp violates RFC 8985's \
         own <7 % spurious budget; the arithmetic now says {control_violations}"
    );
}

/// **THE WINDOW LAW AND THE ORDER STATISTIC — ABSOLUTE PINS** (paper §7.4).
///
/// The paper publishes `N(α)` and the exact `Beta(K, N−K+1)` CIs at the
/// α-sweep's own grid, and the UNSEPARATED-BY-CONSTRUCTION set is DERIVED from
/// them BEFORE the run. **If the engine's `N(α)` disagrees with the paper's by
/// one sample, the pre-registered separation set is wrong** — which is why
/// this is pinned absolutely, at the anchor points, and never ordinally
/// (CLAUDE.md testing discipline).
#[test]
fn the_quantile_native_window_law_matches_the_papers_published_grid() {
    use raptorpath::net::{
        qnative_recovery_round_us, qnative_window_n, QNATIVE_EXCEEDANCE_K, QNATIVE_WINDOW_MAX,
        TIMER_GRANULARITY_US,
    };

    // `N(α) = max(⌈K/α⌉, 2K)`, K = 10, at the α-sweep's grid.
    for &(alpha, want) in &[
        (0.002_f64, 5000usize),
        (0.009, 1112),
        (0.05, 200),
        (0.184, 55),
        (0.40, 25),
    ] {
        assert_eq!(
            qnative_window_n(alpha),
            Some(want),
            "N({alpha}) must equal the paper's published window length"
        );
    }
    // THE SYMMETRIC FLOOR `2K` EXISTS AND DOES NOT BIND ON THE SWEPT GRID —
    // stated in the paper as a floor that never binds, asserted here so it can
    // never be mistaken for a tuned one.
    assert_eq!(
        qnative_window_n(0.40),
        Some(25),
        "2K = 20 must NOT bind at the sweep's largest arm"
    );
    assert_eq!(
        qnative_window_n(1.0),
        Some(2 * QNATIVE_EXCEEDANCE_K),
        "at alpha = 1 the floor IS the law"
    );

    // THE CAP IS THE HONEST UNAVAILABILITY. At the
    // contract's own alpha the direct route needs 1e6 samples and must decline
    // rather than extrapolate off a short window.
    assert_eq!(
        qnative_window_n(1e-5),
        None,
        "at the contract's own alpha the direct quantile is UNAVAILABLE and must say so"
    );
    assert_eq!(
        qnative_window_n(QNATIVE_EXCEEDANCE_K as f64 / QNATIVE_WINDOW_MAX as f64),
        Some(QNATIVE_WINDOW_MAX)
    );
    for bad in [0.0, -0.5, 1.5, f64::NAN, f64::INFINITY] {
        assert_eq!(qnative_window_n(bad), None, "alpha = {bad} is outside the law's domain");
    }

    // THE ORDER STATISTIC ITSELF: the K-th LARGEST of the freshest N(alpha).
    // On 1000..=200_000 us in 1000 us steps the 10th largest is 191_000. Values
    // are in the MILLISECOND range on purpose: the law floors at
    // TIMER_GRANULARITY_US for the same information-availability reason every
    // other clock in the chain does, and a fixture built below that floor would
    // assert the FLOOR while claiming to assert the order statistic.
    let ramp: Vec<u32> = (1..=200u32).map(|i| i * 1_000).collect();
    assert_eq!(
        qnative_recovery_round_us(&ramp, 0.05),
        Some(191_000),
        "W_q must be X_(N-K+1) - the K-th largest, i.e. E[tau] = K/(N+1) ~ alpha"
    );
    // A SHORT WINDOW IS A DIFFERENT LAW'S OUTPUT AND MUST RETURN NOTHING.
    assert_eq!(qnative_recovery_round_us(&ramp[..199], 0.05), None);
    // ONLY THE FRESHEST N(alpha) COUNT. Prepending 500 huge stale samples must
    // not move the reading - a longer slice would read a different level.
    let mut stale: Vec<u32> = vec![9_000_000; 500];
    stale.extend_from_slice(&ramp);
    assert_eq!(
        qnative_recovery_round_us(&stale, 0.05),
        Some(191_000),
        "the window is a WINDOW"
    );
    // AND THE GRANULARITY FLOOR IS ASSERTED AS ITSELF, ONCE, RATHER THAN
    // SMUGGLED INTO THE FIXTURE ABOVE. A window of sub-granularity samples
    // floors, exactly as the RACK rounds did.
    let tiny: Vec<u32> = vec![7u32; 200];
    assert_eq!(
        qnative_recovery_round_us(&tiny, 0.05),
        Some(TIMER_GRANULARITY_US),
        "W_q floors at the timer granularity - information availability, not a mode"
    );

    // MONOTONE IN alpha: larger alpha => a lower quantile => a
    // faster clock.
    //
    // THE FIXTURE IS STATIONARY AND NOT A RAMP, AND THAT IS THE POINT. On a
    // monotonically increasing series the K-th largest of the LAST n samples is
    // the same value for every n, so a ramp would report the three arms
    // IDENTICAL and the monotonicity clause would be satisfied by nothing --
    // the degenerate-arm pathology, in a unit test. This is a deterministic stationary
    // series (a full-period multiplicative walk over 1..=1000, scaled to us),
    // so the three arms read three different quantiles of ONE distribution,
    // which is what the law claims to do.
    let long: Vec<u32> = (0..6000u32)
        .map(|i| ((i.wrapping_mul(7919) % 1000) + 1) * 1_000)
        .collect();
    let w002 = qnative_recovery_round_us(&long, 0.002).expect("N(0.002) fits");
    let w050 = qnative_recovery_round_us(&long, 0.05).expect("N(0.05) fits");
    let w400 = qnative_recovery_round_us(&long, 0.40).expect("N(0.40) fits");
    assert!(
        w002 > w050 && w050 > w400,
        "W_q must be strictly decreasing in alpha: {w002} {w050} {w400}"
    );
    // BOUNDED ABOVE BY THE WINDOW'S OWN MAX, structurally. This clock cannot exceed an RTT the path actually realized,
    // which is the property the Cantelli form does not have at any alpha: at
    // alpha = 1e-5 Cantelli asks for srtt + 316*sigma with no ceiling at all.
    let wmax = *long.iter().max().expect("non-empty") as u64;
    assert!(w002 <= wmax, "W_q must never exceed max(window): {w002} > {wmax}");
}
