//! Bounding test for the give-up deadline D's refresh (node `rec-shed-d-defects`,
//! `docs/derivations/tail-and-hold.md` §3.2, paper §5.6). Run with
//! `RUSTFLAGS="--cfg tokio_unstable" cargo test -p raptorpath --features sim,test-seams --test sim_d_frozen -- --nocapture`
//!
//! DIVERGENCE: the engine refreshes `shed_deadline_us_live` only inside the
//! source-send path (`emit_source.rs`, the span block under `pol.shed_on`), so
//! D is frozen once the last source symbol is sent, whatever the RTT does in
//! the tail. The paper's D = b(δ) · RTprop follows RTprop. The scenario is
//! `sim_shed_reach`'s blackout (the whole object is handed over at one virtual
//! instant, then the tail is judged for 1.5 s), so there are no source sends
//! while acks keep moving the RTT.

mod sim_support;
use sim_support::{run, ChildRun, Scenario};

#[test]
fn child() {
    sim_support::child();
}

const K: u64 = 12_087;

fn scenario() -> ChildRun {
    run(&Scenario {
        seed: 42,
        paths: 1,
        bytes: 6_000_000,
        hint: "realtime",
        reliable: Some(false),
        loss: 0.025,
        jitter_ms: 4,
        limit_s: 40,
        port: 47_410,
        env: vec![
            ("RWM_TEST_TS_SEQ", K.to_string()),
            ("RWM_TEST_TS_DROP", "dead".into()),
            ("RWM_TEST_TS_DEAD_MS", "1500".into()),
            ("RWM_TEST_TS_SHED_ALL", "1".into()),
            ("RWM_TEST_TS_BUDGET_FRAC", "0.05".into()),
        ],
    })
}

fn tok(l: &str, k: &str) -> Option<String> {
    l.split(' ').find_map(|f| f.strip_prefix(k)).map(str::to_string)
}

/// `(src, D_ms, rtt_ms)` of every data-sender `[DIAG]` line: the cumulative
/// source count, the live deadline D (`D=<n>ms`) and the sender's EWMA RTT
/// (the first `rtt=<f>ms`, the max over paths).
fn series(r: &ChildRun) -> Vec<(u64, u64, f64)> {
    r.log
        .lines()
        .filter(|l| l.starts_with("[DIAG]"))
        .filter_map(|l| {
            let d = tok(l, "D=")?.strip_suffix("ms")?.parse().ok()?;
            let src = tok(l, "cum=")?.split('/').next()?.parse().ok()?;
            let rtt = tok(l, "rtt=")?.strip_suffix("ms")?.parse().ok()?;
            Some((src, d, rtt))
        })
        .collect()
}

/// DIVERGENCE (paper §5.6, tail-and-hold §3.2, node `rec-shed-d-defects`): D
/// is refreshed only per source send, so once the last source symbol is out
/// it stays at its last value however the RTT moves. Here the object is
/// handed over at one instant; D = 13 ms (the EWMA RTT of about 26 ms at the
/// time of the last send, times b = 1/2) and stays 13 ms while acks move the
/// sender's EWMA RTT from 211 ms to 928 ms (the burst's own queue). A D
/// that tracked the RTT would be b · RTT, over 100 ms. Replace the
/// "frozen" assertions with "D follows RTT" when D is refreshed outside the
/// source-send path.
#[test]
fn d_is_frozen_while_no_source_is_sent() {
    let r = scenario();
    let s = series(&r);
    let last_src = s.iter().map(|x| x.0).max().expect("no [DIAG] line with D=");
    let tail: Vec<_> = s.iter().filter(|x| x.0 == last_src).collect();
    let (d_min, d_max) = tail.iter().fold((u64::MAX, 0), |a, x| (a.0.min(x.1), a.1.max(x.1)));
    let (rtt_min, rtt_max) = tail.iter().fold((f64::MAX, 0.0f64), |a, x| (a.0.min(x.2), a.1.max(x.2)));
    println!(
        "no-source-send lines={} src={last_src} D={d_min}..{d_max} ms, EWMA RTT {rtt_min:.1}..{rtt_max:.1} ms",
        tail.len()
    );
    assert!(tail.len() >= 10, "too few diag lines after the last source send");
    assert_eq!(d_min, d_max, "D must hold its value from the last source send (13 ms on Windows)");
    assert!(d_max < 30, "D is the early-RTT value, not b times the tail RTT ({d_max} ms)");
    assert!(rtt_max >= 20.0 * d_max as f64, "the RTT must have moved well past D during the tail ({rtt_max} ms)");
    assert!(rtt_max >= 2.0 * rtt_min, "the RTT did not move ({rtt_min}..{rtt_max} ms)");
}
