//! Diagnostic (non-gating) benches split out of `gate_suite.rs`: the
//! per-hint quality sweep and the P1-P5 ablations. Every test here is
//! `#[ignore]`d — they PRINT trade-offs for a reader and assert nothing a gate
//! depends on. They share the gate suite's L0 harness verbatim
//! (`common/gate_harness.rs`).
//!
//! Run: cargo test --test ablation_bench --release -- --ignored --nocapture
// The shared harness carries items (baselines, cell runner) that only the
// gate suite uses.
#![allow(dead_code, unused_imports)]

mod common;

use common::*;
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use raptorpath::control::estimator::LossEstimator;
use raptorpath::control::fec_rate::{p_lost, FecRateController, ProtocolHint};
use raptorpath::control::gilbert_elliott::GilbertElliottEstimator;
use raptorpath::fec::{FecBackend, RlcWindowDecoder, RlcWindowEncoder, WindowDecoder, WindowEncoder, WireSymbol};
use raptorpath::net::reorder::ReorderBuffer;
use raptorpath::scheduler::{Clock, MockClock};
use raptorpath_math::compute_r_star_exact;
use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

include!("common/gate_harness.rs");

/// Diagnostic (non-gating): the quality trade-off per protocol hint.
/// Answers: does Bulk actually finish transfers faster than the baseline,
/// and does Realtime buy lower latency at moderate overhead?
#[test]
#[ignore]
fn quality_hint_sweep() {
    let cells: &[(&str, GateChannel)] = &[
        ("C2-WiFi", C2_WIFI),
        ("C3-LTE", C3_LTE),
        ("C4-Sat", C4_SAT),
        ("C5-BadWiFi", C5_BADWIFI),
    ];
    let hints = [
        ("Bulk", ProtocolHint::Bulk),
        ("Auto", ProtocolHint::Auto),
        ("Realtime", ProtocolHint::Realtime),
    ];
    let trials = 6usize;
    for (name, ch) in cells {
        let mut b_compl = TrialStats::new();
        let mut b_p50 = TrialStats::new();
        let mut b_p99 = TrialStats::new();
        let mut b_oh = TrialStats::new();
        for t in 0..trials {
            let seed = 50_000 + t as u64 * 137 + 42;
            let b = run_baseline(&[*ch], seed);
            b_compl.push(b.completion_s);
            b_p50.push(b.p50_ms);
            b_p99.push(b.p99_ms);
            b_oh.push(b.wire_per_source - 1.0);
        }
        println!(
            "{name} SimRetx: completion={:.3}s p50={:.1}ms p99={:.1}ms overhead={:.1}%",
            b_compl.mean(), b_p50.mean(), b_p99.mean(), b_oh.mean() * 100.0
        );
        for (hname, hint) in &hints {
            let mut compl = TrialStats::new();
            let mut p50 = TrialStats::new();
            let mut p99 = TrialStats::new();
            let mut oh = TrialStats::new();
            for t in 0..trials {
                let seed = 50_000 + t as u64 * 137 + 42;
                let f = run_fec(&[*ch], seed, &cfg(*hint));
                compl.push(f.completion_s);
                p50.push(f.p50_ms);
                p99.push(f.p99_ms);
                oh.push(f.wire_per_source - 1.0);
            }
            println!(
                "{name} {hname:8}: completion={:.3}s ({:.2}x) p50={:.1}ms p99={:.1}ms ({:.2}x) overhead={:.1}%",
                compl.mean(),
                compl.mean() / b_compl.mean(),
                p50.mean(),
                p99.mean(),
                p99.mean() / b_p99.mean(),
                oh.mean() * 100.0
            );
        }
    }
}

/// Ablation for P2 (estimated_floor): Copa's propagation floor comes from
/// the running min RTT sample instead of ground-truth paths[i].rtt(). This
/// is an honesty fix (L0 → L1 transfer), not a performance change, so the
/// gate is equivalence-or-better: small shifts are expected, big
/// regressions are not.
#[test]
#[ignore]
fn ablation_p2_estimated_floor() {
    let cells: &[(&str, GateChannel)] = &[("C2-WiFi", C2_WIFI), ("C4-Sat", C4_SAT)];
    let trials = 6usize;
    for (name, ch) in cells {
        let mut on_compl = TrialStats::new();
        let mut on_p50 = TrialStats::new();
        let mut on_p99 = TrialStats::new();
        let mut off_compl = TrialStats::new();
        let mut off_p50 = TrialStats::new();
        let mut off_p99 = TrialStats::new();
        for t in 0..trials {
            let seed = 61_000 + t as u64 * 137 + 42;
            let on = run_fec(
                &[*ch],
                seed,
                &FecConfig { estimated_floor: true, ..cfg(ProtocolHint::Auto) },
            );
            on_compl.push(on.completion_s);
            on_p50.push(on.p50_ms);
            on_p99.push(on.p99_ms);
            let off = run_fec(
                &[*ch],
                seed,
                &FecConfig { estimated_floor: false, ..cfg(ProtocolHint::Auto) },
            );
            off_compl.push(off.completion_s);
            off_p50.push(off.p50_ms);
            off_p99.push(off.p99_ms);
        }
        println!(
            "{name} floor=est  : completion={:.3}s p50={:.1}ms p99={:.1}ms",
            on_compl.mean(), on_p50.mean(), on_p99.mean()
        );
        println!(
            "{name} floor=truth: completion={:.3}s p50={:.1}ms p99={:.1}ms",
            off_compl.mean(), off_p50.mean(), off_p99.mean()
        );
        assert!(
            on_compl.mean() <= 1.10 * off_compl.mean(),
            "{name}: estimated floor regresses completion: {:.3}s vs {:.3}s",
            on_compl.mean(),
            off_compl.mean()
        );
        assert!(
            on_p99.mean() <= 1.15 * off_p99.mean(),
            "{name}: estimated floor regresses p99: {:.1}ms vs {:.1}ms",
            on_p99.mean(),
            off_p99.mean()
        );
    }
}

/// P4a/P6 ablation (paper §4.5, §4.6): Bulk's tail target maps to the
/// completion-exposure glide δ_eff = ε̂ + (0.05 − ε̂)·χ, so the continuous
/// r* is 0 identically in the steady state (χ = 0) — pure ARQ, volume
/// parity with retransmission transports — and ramps only over the final
/// ~1.5 SRTT. tail_fec is OFF in both arms (it is a no-op under the glide
/// anyway; the ramp subsumes the burst).
#[test]
#[ignore]
fn ablation_p4a_bulk_pure_arq() {
    let cells: &[(&str, GateChannel)] = &[("C2-WiFi", C2_WIFI), ("C3-LTE", C3_LTE)];
    let trials = 6usize;
    for (name, ch) in cells {
        let mut arms: Vec<(bool, TrialStats, TrialStats, TrialStats)> = Vec::new();
        for flag in [true, false] {
            let mut compl = TrialStats::new();
            let mut p99 = TrialStats::new();
            let mut oh = TrialStats::new();
            for t in 0..trials {
                let seed = 63_000 + t as u64 * 137 + 42;
                let c = FecConfig {
                    bulk_arq_delta: flag,
                    tail_fec: false,
                    ..cfg(ProtocolHint::Bulk)
                };
                let f = run_fec(&[*ch], seed, &c);
                compl.push(f.completion_s);
                p99.push(f.p99_ms);
                oh.push(f.wire_per_source - 1.0);
            }
            arms.push((flag, compl, p99, oh));
        }
        for (flag, compl, p99, oh) in &arms {
            println!(
                "{name} bulk_arq_delta={:5}: completion={:.3}s±{:.3} p99={:.1}ms overhead={:.1}%",
                flag, compl.mean(), compl.ci95(), p99.mean(), oh.mean() * 100.0
            );
        }
        let (_, on_c, _, on_oh) = &arms[0];
        let (_, off_c, _, off_oh) = &arms[1];
        if *name == "C2-WiFi" {
            assert!(
                on_oh.mean() < 0.5 * off_oh.mean(),
                "{name}: pure-ARQ Bulk must cut overhead by >2x: on={:.2}% vs off={:.2}%",
                on_oh.mean() * 100.0, off_oh.mean() * 100.0
            );
        }
        assert!(
            on_c.mean() <= off_c.mean() * 1.02,
            "{name}: pure-ARQ Bulk must not regress completion: on={:.3}s vs off={:.3}s",
            on_c.mean(), off_c.mean()
        );
    }
}

/// P4b ablation (paper §4.6): end-of-stream tail FEC — a burst of
/// n_tail = ceil(r_tail × W) repairs covering the final window, r_tail
/// from the exact transfer-matrix computation (paper §4.7). Expected
/// saving: P(≥1 tail loss) × ~1.5 RTT of completion (~10-25 ms at C2);
/// gate is no-regression, the measured saving is reported.
///
/// Under the default Bulk config the χ glide (P6) subsumes the burst, so
/// tail_fec is a no-op here and both arms are identical; the bench remains
/// as a regression tripwire for the flag wiring.
#[test]
#[ignore]
fn ablation_p4b_tail_fec() {
    let cells: &[(&str, GateChannel)] = &[("C2-WiFi", C2_WIFI), ("C4-Sat", C4_SAT)];
    let trials = 6usize;
    for (name, ch) in cells {
        let mut on_compl = TrialStats::new();
        let mut off_compl = TrialStats::new();
        for t in 0..trials {
            let seed = 64_000 + t as u64 * 137 + 42;
            let on = run_fec(
                &[*ch],
                seed,
                &FecConfig { tail_fec: true, ..cfg(ProtocolHint::Bulk) },
            );
            on_compl.push(on.completion_s);
            let off = run_fec(
                &[*ch],
                seed,
                &FecConfig { tail_fec: false, ..cfg(ProtocolHint::Bulk) },
            );
            off_compl.push(off.completion_s);
        }
        println!(
            "{name} tail_fec=on : completion={:.3}s±{:.3}",
            on_compl.mean(), on_compl.ci95()
        );
        println!(
            "{name} tail_fec=off: completion={:.3}s±{:.3} (saving {:.1}ms)",
            off_compl.mean(), off_compl.ci95(),
            (off_compl.mean() - on_compl.mean()) * 1000.0
        );
        assert!(
            on_compl.mean() <= off_compl.mean(),
            "{name}: tail FEC must not regress completion: on={:.3}s vs off={:.3}s",
            on_compl.mean(), off_compl.mean()
        );
    }
}

/// P1 ablation (paper §8.2): the protocol hint sets the Copa queue target,
/// not just the FEC rate. Realtime with a tight target should cut the
/// standing-queue median latency at C2 without giving up meaningful
/// completion time; Bulk's deeper target must not hurt completion.
#[test]
#[ignore]
fn ablation_p1_hint_delay_target() {
    let trials = 6usize;
    let run = |hint: ProtocolHint, flag: bool| {
        let mut compl = TrialStats::new();
        let mut p50 = TrialStats::new();
        let mut p99 = TrialStats::new();
        for t in 0..trials {
            let seed = 60_000 + t as u64 * 137 + 42;
            let c = FecConfig { hint_delay_target: flag, ..cfg(hint) };
            let f = run_fec(&[C2_WIFI], seed, &c);
            compl.push(f.completion_s);
            p50.push(f.p50_ms);
            p99.push(f.p99_ms);
        }
        (compl, p50, p99)
    };

    let (rt_on_c, rt_on_p50, rt_on_p99) = run(ProtocolHint::Realtime, true);
    let (rt_off_c, rt_off_p50, rt_off_p99) = run(ProtocolHint::Realtime, false);
    println!(
        "C2 Realtime on : completion={:.3}s p50={:.2}ms p99={:.2}ms",
        rt_on_c.mean(), rt_on_p50.mean(), rt_on_p99.mean()
    );
    println!(
        "C2 Realtime off: completion={:.3}s p50={:.2}ms p99={:.2}ms",
        rt_off_c.mean(), rt_off_p50.mean(), rt_off_p99.mean()
    );
    // Post-P2 note: with the honest (jitter-free) estimated floor, Auto's
    // 1.125 target is already near-optimal at C2 — most of the median win
    // (17.6 -> 12.8ms) came from fixing the floor. Realtime's tighter
    // target is retained for semantic correctness (and for operating
    // points where the floor estimate is loose), so the ablation gate is
    // no-regression, not a mandated cut (pushing the target to
    // floor+0.15ms buys ~1.5ms of p50 for +8% completion — a bad trade).
    assert!(
        rt_on_p50.mean() <= 1.02 * rt_off_p50.mean(),
        "Realtime p50 with hint delay target ({:.2}ms) regresses vs off ({:.2}ms)",
        rt_on_p50.mean(), rt_off_p50.mean()
    );
    assert!(
        rt_on_p99.mean() <= 1.05 * rt_off_p99.mean(),
        "Realtime p99 with hint delay target ({:.2}ms) regresses vs off ({:.2}ms)",
        rt_on_p99.mean(), rt_off_p99.mean()
    );
    assert!(
        rt_on_c.mean() <= 1.08 * rt_off_c.mean(),
        "Realtime completion with hint delay target ({:.3}s) > 1.08x off ({:.3}s)",
        rt_on_c.mean(), rt_off_c.mean()
    );

    let (bk_on_c, bk_on_p50, bk_on_p99) = run(ProtocolHint::Bulk, true);
    let (bk_off_c, bk_off_p50, bk_off_p99) = run(ProtocolHint::Bulk, false);
    println!(
        "C2 Bulk     on : completion={:.3}s p50={:.2}ms p99={:.2}ms",
        bk_on_c.mean(), bk_on_p50.mean(), bk_on_p99.mean()
    );
    println!(
        "C2 Bulk     off: completion={:.3}s p50={:.2}ms p99={:.2}ms",
        bk_off_c.mean(), bk_off_p50.mean(), bk_off_p99.mean()
    );
    assert!(
        bk_on_c.mean() <= 1.05 * bk_off_c.mean(),
        "Bulk completion with hint delay target ({:.3}s) > 1.05x off ({:.3}s)",
        bk_on_c.mean(), bk_off_c.mean()
    );
}

/// P5 ablation (paper §4.4): cap r at the saturation point of the p99(r)
/// tail model. Measured problem: at C4-Sat, Realtime's uncapped r (~0.49)
/// has WORSE p99 than Auto (412ms vs 297ms) — past the saturation point,
/// extra repairs displace source symbols and pressure the queue. With the
/// cap, both hints emit min(r_hint, r_sat) and the reversal disappears. At
/// C2 the cap sits above every request (r_sat ~ 0.25) and must be inert.
#[test]
#[ignore]
fn ablation_p5_saturation_cap() {
    let trials = 6usize;
    let run = |ch: GateChannel, hint: ProtocolHint, flag: bool| {
        let mut compl = TrialStats::new();
        let mut p99 = TrialStats::new();
        let mut oh = TrialStats::new();
        for t in 0..trials {
            let seed = 64_000 + t as u64 * 137 + 42;
            let c = FecConfig { saturation_cap: flag, ..cfg(hint) };
            let f = run_fec(&[ch], seed, &c);
            compl.push(f.completion_s);
            p99.push(f.p99_ms);
            oh.push(f.wire_per_source - 1.0);
        }
        (compl, p99, oh)
    };
    let print_arm = |name: &str, arm: &(TrialStats, TrialStats, TrialStats)| {
        println!(
            "{name}: completion={:.3}s p99={:.1}ms overhead={:.1}%",
            arm.0.mean(), arm.1.mean(), arm.2.mean() * 100.0
        );
    };

    // --- C4-Sat: the measured p99 reversal ---
    let rt_on = run(C4_SAT, ProtocolHint::Realtime, true);
    let rt_off = run(C4_SAT, ProtocolHint::Realtime, false);
    let au_on = run(C4_SAT, ProtocolHint::Auto, true);
    let au_off = run(C4_SAT, ProtocolHint::Auto, false);
    print_arm("C4 Realtime cap=on ", &rt_on);
    print_arm("C4 Realtime cap=off", &rt_off);
    print_arm("C4 Auto     cap=on ", &au_on);
    print_arm("C4 Auto     cap=off", &au_off);

    // The reversal is gone: Realtime's tighter hint must no longer buy a
    // WORSE tail than Auto.
    assert!(
        rt_on.1.mean() <= 1.10 * au_on.1.mean(),
        "C4: capped Realtime p99 ({:.1}ms) must be <= 1.10x capped Auto p99 ({:.1}ms)",
        rt_on.1.mean(), au_on.1.mean()
    );
    // And the cap must be a real improvement over uncapped Realtime.
    assert!(
        rt_on.1.mean() < 0.9 * rt_off.1.mean(),
        "C4: capped Realtime p99 ({:.1}ms) must be < 0.9x uncapped ({:.1}ms)",
        rt_on.1.mean(), rt_off.1.mean()
    );

    // --- C2-WiFi: the cap must not bind where saturation is not reached ---
    let c2_on = run(C2_WIFI, ProtocolHint::Realtime, true);
    let c2_off = run(C2_WIFI, ProtocolHint::Realtime, false);
    print_arm("C2 Realtime cap=on ", &c2_on);
    print_arm("C2 Realtime cap=off", &c2_off);
    for (on, off, metric) in [
        (c2_on.0.mean(), c2_off.0.mean(), "completion"),
        (c2_on.1.mean(), c2_off.1.mean(), "p99"),
        (c2_on.2.mean(), c2_off.2.mean(), "overhead"),
    ] {
        assert!(
            (on - off).abs() <= 0.05 * off,
            "C2 Realtime: cap must be inert, {metric} differs >5%: on={on:.4} off={off:.4}"
        );
    }
}
