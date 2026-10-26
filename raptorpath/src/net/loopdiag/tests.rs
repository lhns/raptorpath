use super::*;
use crate::scheduler::{MockClock, PathState};
use std::sync::Arc;
use std::time::Duration;

/// Install a fresh gauge for this test's thread (the gate-on path without
/// flipping the process-global `OnceLock`), run `f`, uninstall.
fn with_gauge<R>(f: impl FnOnce(&'static LoopDiag) -> R) -> R {
    let g: &'static LoopDiag = Box::leak(Box::new(LoopDiag::new()));
    TEST_GAUGE.with(|c| c.set(Some(g)));
    let r = f(g);
    TEST_GAUGE.with(|c| c.set(None));
    r
}

/// (a) Aggregator, ABSOLUTE: nearest-rank min/p10/p50 over a known set, the
/// overflow count, and the reset after a drain.
#[test]
fn window_quantiles_overflow_and_reset() {
    let mut w = WindowBuf::default();
    // 20 samples 1..=20 in scrambled order: idx = round(19 q).
    for i in 0..20u32 {
        w.push_rtt((i * 7) % 20 + 1);
    }
    w.add_delivered(30);
    w.add_delivered(12);
    let s = w.drain();
    assert_eq!(s.n, 20);
    assert_eq!(s.min_us, Some(1));
    assert_eq!(s.p10_us, Some(3), "round(19*0.10)=2 -> third smallest");
    assert_eq!(s.p50_us, Some(11), "round(19*0.50)=10 -> eleventh smallest");
    assert_eq!((s.overflow, s.dlv), (0, 42));

    // Reset: the next window starts empty.
    let e = w.drain();
    assert_eq!(e, WindowSummary::default());
    assert_eq!((e.n, e.min_us, e.p10_us, e.p50_us, e.dlv), (0, None, None, None, 0));

    // Cap: RTT_CAP are kept, the rest counted; the kept prefix is what the
    // quantiles see.
    for i in 0..(RTT_CAP as u32 + 5) {
        w.push_rtt(1000 + i);
    }
    let s = w.drain();
    assert_eq!((s.n, s.overflow), (RTT_CAP, 5));
    assert_eq!(s.min_us, Some(1000));
    // Overflow is a per-window count: it resets too.
    assert_eq!(w.drain().overflow, 0);
}

/// (b) Gate off: the shipped default resolves no gauge, and driving the real
/// feed sites buffers nothing anywhere.
#[test]
fn gate_off_buffers_nothing() {
    assert!(
        std::env::var("RWM_LOOPDIAG").is_err(),
        "this test asserts the DEFAULT; the environment set RWM_LOOPDIAG"
    );
    assert!(gauge().is_none(), "RWM_LOOPDIAG ships default OFF");
    // A gauge that exists but is not installed sees none of the feeds.
    let bystander = LoopDiag::new();
    let clock = Arc::new(MockClock::new());
    let mut p = PathState::new(0, clock.clone());
    for _ in 0..50 {
        p.record_rtt_sample(Duration::from_millis(10));
        clock.advance(Duration::from_millis(5));
        p.on_ack(4);
    }
    assert_eq!(bystander.offered(), 0);
    assert_eq!(bystander.drain(0), WindowSummary::default());
}

/// Gate on, the wiring: the real feed sites reach the gauge (rule 1: prove the
/// mechanism executes) with the exact samples and counts.
#[test]
fn feed_sites_route_to_the_gauge() {
    with_gauge(|g| {
        let clock = Arc::new(MockClock::new());
        let mut p = PathState::new(3, clock.clone());
        for i in 0..40u64 {
            p.record_rtt_sample(Duration::from_micros(10_000 + i));
            clock.advance(Duration::from_millis(5));
            p.on_ack(4);
        }
        let s = g.drain(3);
        assert_eq!(s.n, 40);
        assert_eq!(s.min_us, Some(10_000));
        // Every record_delivery call, accepted (>= 1 ms elapsed) or not.
        assert_eq!(s.dlv, 160);
        assert_eq!(g.drain(0), WindowSummary::default(), "other paths untouched");
    });
}

type Snap = (Option<Duration>, Option<Duration>, Option<f64>, u32, u64);

fn script(p: &mut PathState, clock: &MockClock) -> Vec<Snap> {
    let mut out = Vec::new();
    for i in 0..300u64 {
        p.record_rtt_sample(Duration::from_micros(10_000 + (i * 7919) % 13_000));
        clock.advance(Duration::from_micros(500 + (i * 131) % 4_000));
        p.on_ack(1 + (i % 9) as u32);
        out.push((
            p.copa_min_rtt(),
            p.srtt_measured(),
            p.btlbw_sym_per_s(),
            p.cwnd,
            p.rtt_jitter_us(),
        ));
    }
    out
}

/// (c) Observation only: a scripted RTT/delivery sequence leaves Copa in a
/// bit-identical state, step by step, with the gauge on and off.
#[test]
fn copa_state_is_bit_identical_gate_on_off() {
    let c_off = Arc::new(MockClock::new());
    let mut off = PathState::new(0, c_off.clone());
    let a = script(&mut off, &c_off);

    let (b, seen) = with_gauge(|g| {
        let c_on = Arc::new(MockClock::new());
        let mut on = PathState::new(0, c_on.clone());
        let b = script(&mut on, &c_on);
        (b, g.drain(0))
    });
    assert_eq!(a, b, "the gauge must not move any Copa quantity");
    assert_eq!(seen.n, 300, "and it must really have been fed (not a vacuous pass)");
    assert!(a.iter().any(|r| r.2.is_some_and(|bw| bw > 0.0)), "the script reaches a BtlBw");
}

/// (d) Line format, byte for byte, and the empty-window `-` rule.
#[test]
fn line_format_is_pinned() {
    let w = WindowSummary {
        n: 120,
        overflow: 2,
        min_us: Some(12_400),
        p10_us: Some(12_900),
        p50_us: Some(14_100),
        dlv: 2_500,
    };
    let copa = CopaReadout { rtprop_us: Some(12_400), jit_us: 310, srtt_us: Some(13_000) };
    let q = QuinnReadout { pace_bps: 100_000_000, cwnd: 300_000, srtt_us: Some(12_800) };
    assert_eq!(
        format_line(4.26, 0, &w, 0.25, &copa, Some(&q)),
        "[LOOPDIAG] t=4.3s p0 n=120 ov=2 rtt_min_us=12400 rtt_p10_us=12900 rtt_p50_us=14100 \
         rtprop_us=12400 jit_us=310 srtt_us=13000 dlv=2500 dlv_rate=10000 \
         q_pace_bps=100000000 q_cwnd=300000 q_srtt_us=12800"
    );
    // Empty window and no quinn snapshot: dashes, never zeros.
    let e = format_line(
        0.5,
        1,
        &WindowSummary::default(),
        0.25,
        &CopaReadout { rtprop_us: None, jit_us: 0, srtt_us: None },
        None,
    );
    assert!(
        e.contains("n=0 ov=0 rtt_min_us=- rtt_p10_us=- rtt_p50_us=- rtprop_us=- jit_us=0 srtt_us=- dlv=0"),
        "{e}"
    );
    assert!(e.ends_with("q_pace_bps=- q_cwnd=- q_srtt_us=-"), "{e}");
    // quinn's pacing_rate = None is published as 0 and prints `-`.
    let z = QuinnReadout { pace_bps: 0, cwnd: 12_000, srtt_us: None };
    let l = format_line(1.0, 0, &w, 0.25, &copa, Some(&z));
    assert!(l.ends_with("q_pace_bps=- q_cwnd=12000 q_srtt_us=-"), "{l}");
}

/// The gauge reads and writes no engine state.
#[test]
fn loopdiag_is_observation_only() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/net/loopdiag.rs"),
    )
    .expect("read src/net/loopdiag.rs");
    let end = src.find("thread_local!").expect("test-gauge marker");
    let code: String = src[..end]
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in [
        "path_mut",
        "&mut Scheduler",
        "record_delivery(",
        "on_delivery_signal",
        "set_cc_window_bytes",
    ] {
        assert!(!code.contains(forbidden), "found `{forbidden}`");
    }
}
