//! The wire-clocked placement base (status §38/§41b, paper §5.7) reads a
//! per-leg RTT that is mirrored into the scheduler at every ack batch and
//! path report for EVERY leg (measurement-discipline rule 1: the wiring
//! routes there). That the read list names an idle leg and that the mirror
//! writes every leg it is handed is pinned at unit level
//! (`scheduler::tests::the_wire_mirror_writes_every_leg`); the test-only
//! placement hold that once forced a leg idle in a real transfer is gone.
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

/// A two-leg transfer: in every `[DIAG]` line after the first ack, each
/// leg's `wpl_us` lies within 1 ms of the transport's `wrtt` (which `[DIAG]`
/// prints rounded to a whole millisecond, hence the 0.5 ms print allowance).
#[test]
fn wire_srtt_mirrors_every_leg() {
    let env = [("RWM_DIAG", "1"), ("RUST_LOG", "raptorpath=info")];
    let (cli, _srv) = loopback::transfer(loopback::Transfer {
        paths: 2,
        env: &env,
        client_env: &loopback::shaped(Some("c2,c2")),
        bytes: "40000000",
        ..Default::default()
    });

    let mut seen = 0usize;
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
        }
        seen += 1;
    }
    assert!(seen >= 3, "only {seen} usable [DIAG] lines");
}
