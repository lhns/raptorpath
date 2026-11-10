//! `RWM_PLACE_WIRE` (status §38, paper §5.7): the wire-clocked placement
//! base reads a per-leg RTT that is mirrored into the scheduler at every ack
//! batch and path report for EVERY leg, including a leg that carries nothing
//! (measurement-discipline rule 1: the wiring routes there).
//!
//! Own test binary: `RWM_L0_NETEM` is process-global in the child.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

/// One leg's block of a `[DIAG]` line.
#[derive(Debug)]
struct Leg {
    id: u32,
    infl: u64,
    wrtt_ms: f64,
    wpl_us: Option<f64>,
}

/// Split a `[DIAG]` line into its `p<i>:infl=...` blocks. A block runs to the
/// next `p<j>:infl=` token, so the fields it holds are this leg's own.
fn legs(line: &str) -> Vec<Leg> {
    let mut out: Vec<Leg> = Vec::new();
    let mut cur: Option<Leg> = None;
    for tok in line.split_whitespace() {
        if let Some(rest) = tok.strip_prefix('p') {
            if let Some((id, infl)) = rest.split_once(":infl=") {
                if let (Ok(id), Some(infl)) = (
                    id.parse::<u32>(),
                    infl.split('/').next().and_then(|v| v.parse::<u64>().ok()),
                ) {
                    out.extend(cur.take());
                    cur = Some(Leg { id, infl, wrtt_ms: f64::NAN, wpl_us: None });
                    continue;
                }
            }
        }
        if let Some(l) = cur.as_mut() {
            if let Some(w) = tok.strip_prefix("rtt=").and_then(|r| r.split_once("/wrtt=")) {
                let v = w.1.split('/').next().unwrap_or("");
                l.wrtt_ms = gauge::numeric_prefix(v).parse().unwrap_or(f64::NAN);
            } else if let Some(v) = tok.strip_prefix("wpl_us=") {
                l.wpl_us = gauge::numeric_prefix(v).parse().ok();
            }
        }
    }
    out.extend(cur);
    out
}

/// A two-leg transfer in which leg 1 is held out of placement for 1.2 s
/// (`RWM_TEST_PLACE_HOLD`, test-only) and then released: in every `[DIAG]`
/// line after the first ack, each leg's `wpl_us` lies within 1 ms of the
/// transport's `wrtt` (which `[DIAG]` prints rounded to a whole millisecond,
/// hence the 0.5 ms print allowance), including leg 1 while its `infl` is 0.
#[test]
fn wire_srtt_mirrors_every_leg_including_an_idle_one() {
    let env = [
        ("RWM_DIAG", "1"),
        ("RWM_PLAIN_RS", "1"),
        ("RWM_PLACE_WIRE", "1"),
        ("RWM_TEST_PLACE_HOLD", "1:1200"),
        ("RUST_LOG", "raptorpath=info"),
    ];
    let (cli, _srv) = loopback::transfer(loopback::Transfer {
        paths: 2,
        env: &env,
        client_env: &loopback::shaped(Some("c2,c2")),
        bytes: "40000000",
        ..Default::default()
    });

    let g = gauge::require(&cli, "[GATES]", "the sender must echo its gates");
    assert!(g.contains("RWM_PLACE_WIRE=1"), "arm not armed on the sender: {g}");

    let mut seen = 0usize;
    let mut idle_seen = 0usize;
    let mut started = false;
    for line in cli.lines().filter(|l| l.contains("[DIAG]")) {
        let ls = legs(line);
        if ls.len() != 2 {
            continue;
        }
        if !started {
            // Before the first ack there is nothing to mirror yet.
            if ls.iter().all(|l| l.wpl_us.is_some()) {
                started = true;
            } else {
                continue;
            }
        }
        for l in &ls {
            let w = l.wpl_us.unwrap_or_else(|| {
                panic!("p{} has no wpl_us after the mirror started: {line}", l.id)
            });
            assert!(
                (w / 1000.0 - l.wrtt_ms).abs() <= 1.5,
                "p{}: wpl_us={w} vs wrtt={} ms (infl={}): {line}",
                l.id,
                l.wrtt_ms,
                l.infl
            );
            if l.id == 1 && l.infl == 0 {
                idle_seen += 1;
            }
        }
        seen += 1;
    }
    assert!(seen >= 3, "only {seen} usable [DIAG] lines");
    assert!(
        idle_seen >= 2,
        "leg 1 was never observed idle (infl=0) with a mirrored wpl_us ({idle_seen} lines): \
         the placement hold did not execute"
    );
}

/// Control: with the arm absent the mirror is never written, so `wpl_us` is
/// `-` on every leg of every `[DIAG]` line.
#[test]
fn without_the_arm_wpl_us_is_a_dash() {
    let env = [("RWM_DIAG", "1"), ("RWM_PLAIN_RS", "1"), ("RUST_LOG", "raptorpath=info")];
    let (cli, _srv) = loopback::transfer(loopback::Transfer {
        paths: 2,
        env: &env,
        client_env: &loopback::shaped(Some("c2,c2")),
        bytes: "8000000",
        ..Default::default()
    });
    let g = gauge::require(&cli, "[GATES]", "the sender must echo its gates");
    assert!(g.contains("RWM_PLACE_WIRE=0"), "{g}");
    let mut seen = 0;
    for line in cli.lines().filter(|l| l.contains("[DIAG]")) {
        for l in legs(line) {
            assert!(l.wpl_us.is_none(), "p{} printed a wire price with the arm absent: {line}", l.id);
            seen += 1;
        }
    }
    assert!(seen > 0, "no [DIAG] legs parsed");
}
