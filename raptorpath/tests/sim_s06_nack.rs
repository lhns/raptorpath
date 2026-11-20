//! ADR-0079 / S06, T2 on the deterministic sim (it was loopback, 9/10 on the
//! candidate: `s06_reactive.rs::stalled_blocker_recovered_by_nack`, now record-only).
//!
//! Reliable seat, N = 1, Auto (slots exist), warm epsilon-hat, the source PACED
//! (`SIM_PACE_MBIT`, `sim_support::paced_client`): an unpaced client emits the
//! whole object at one virtual instant and takes no proactive slot, so the
//! witness below would fail. k's original and its first resend are swallowed
//! (`RWM_TEST_TS_DROP=first:2`). A stalled blocker must be recovered without a
//! copy of k through a proactive slot, and the NACK resends are spaced by the clock.
//!
//! Red on the tree that still copies (S1: taper > 0 or a slot copy of k), green
//! after the deletion (S2). Run with
//! `RUSTFLAGS="--cfg tokio_unstable" cargo test -p raptorpath --features sim,test-seams --test sim_s06_nack -- --nocapture`.

#[path = "common/gauge.rs"]
mod gauge;
mod sim_support;
use gauge::{numeric_prefix, raw_field};
use sim_support::{run, ChildRun, Scenario};

/// The seq under test: far enough in that epsilon-hat is warm.
const K: u64 = 2000;
const PACE_MBIT: f64 = 20.0;
/// Seeded 2.5 % loss, one-way 10 ms, so RTT >= 20 ms.
const SRTT_MIN_US: u64 = 20_000;
/// `HOLE_NACK_REFRESH_MAX` (`net/mod.rs`).
const HOLE_REFRESH_MAX_US: u64 = 100_000;
const SLACK_US: u64 = 50_000;

#[test]
fn child() {
    sim_support::child();
}

#[derive(Debug, Clone)]
struct Ev {
    ev: String,
    n: u64,
    site: String,
    dropped: bool,
    t: u64,
}

fn events(log: &str) -> Vec<Ev> {
    let num = |l: &str, k: &str| raw_field(l, k).and_then(|v| numeric_prefix(v).parse::<u64>().ok()).unwrap_or(0);
    log.lines()
        .filter(|l| l.starts_with("[TSEAM] "))
        .map(|l| Ev {
            ev: raw_field(l, "ev=").unwrap_or("").to_string(),
            n: num(l, "n="),
            site: raw_field(l, "site=").unwrap_or("").to_string(),
            dropped: raw_field(l, "dropped=") == Some("1"),
            t: num(l, "t_us="),
        })
        .collect()
}

/// The largest `key` value on any `[DIAG]` line (both peers print one; the
/// ack-only peer's reads 0). None when no line carries the key.
fn max_field(log: &str, key: &str) -> Option<u64> {
    log.split_whitespace()
        .filter_map(|t| t.strip_prefix(key))
        .filter_map(|v| numeric_prefix(v).parse::<u64>().ok())
        .max()
}

fn scenario() -> ChildRun {
    run(&Scenario {
        seed: 42,
        paths: 1,
        bytes: 6_000_000,
        hint: "auto",
        reliable: Some(true),
        loss: 0.025,
        jitter_ms: 0,
        limit_s: 40,
        port: 47_500,
        env: vec![
            ("SIM_PACE_MBIT", PACE_MBIT.to_string()),
            ("RWM_TEST_TS_SEQ", K.to_string()),
            ("RWM_TEST_TS_DROP", "first:2".into()),
        ],
    })
}

/// One reading of T2, the same assertions as the loopback version, plus the
/// witness that the run took slots. Returns the k-event lines for the digest.
fn t2(r: &ChildRun) -> Vec<String> {
    let all = events(&r.log);
    let tag = format!(" seq={K} ");
    let ks: Vec<&str> = r.log.lines().filter(|l| l.starts_with("[TSEAM] ") && l.contains(&tag)).collect();
    let sent = events(&ks.join("\n"));
    let t0 = sent.iter().find(|e| e.ev == "send").unwrap_or_else(|| panic!("k={K} was never sent:\n{}", r.log)).t;
    let fill = sent
        .iter()
        .find(|e| e.ev == "fill")
        .unwrap_or_else(|| panic!("the receiver never got k={K} (outcome {}):\n{}", r.outcome, r.log))
        .clone();
    let sends: Vec<&Ev> = sent.iter().filter(|e| e.ev == "send").collect();
    let copies = sends.iter().filter(|e| e.n > 1 && e.site == "single").count();
    let nacks: Vec<&&Ev> = sends.iter().filter(|e| e.site == "batch" && e.t <= fill.t).collect();
    let taper = max_field(&r.log, "taper=").expect("no taper= on any [DIAG] line");
    // `pslot=` is the slots taken (S2). The tree before the deletion has no such
    // counter: there a slot is witnessed by the copies it sent (taper), which is
    // all the old tree could count.
    let pslot = max_field(&r.log, "pslot=");
    let slots = pslot.unwrap_or(taper);
    println!(
        "[s06 T2 sim] sends of k: {} (copies via slot path {copies}, NACK/sweep resends before fill {}), taper={taper} pslot={pslot:?}, stall {} us, events {}",
        sends.len(),
        nacks.len(),
        fill.t.saturating_sub(t0),
        all.len()
    );

    // Witness: slots were being taken, so a copy WAS possible.
    assert!(slots > 0, "no proactive slot was taken: the test cannot see a copy (pslot={pslot:?}, taper={taper})");
    // The absolute claim.
    assert_eq!(copies, 0, "a proactive slot sent {copies} copies of the stalled blocker");
    assert_eq!(taper, 0, "taper={taper}");
    // A decode fill of k is a pass only with its witness: the receiver's `fill`
    // event for k and NO delivered send of k before it.
    let delivered_before_fill = sends.iter().any(|e| !e.dropped && e.t <= fill.t);
    if let Some(first_nack) = nacks.first() {
        // NACK(k) >= 1 within the time threshold (9/8 x an SRTT bounded at
        // 200 ms) plus one hole-refresh period.
        let bound = 200_000 * 9 / 8 + HOLE_REFRESH_MAX_US + SLACK_US;
        assert!(
            first_nack.t.saturating_sub(t0) <= bound,
            "first NACK resend {} us after the original, bound {bound}",
            first_nack.t.saturating_sub(t0)
        );
        // At most one resend per SRTT over the stall.
        let stall = fill.t.saturating_sub(t0);
        let cap = stall.div_ceil(SRTT_MIN_US) + 1;
        assert!((nacks.len() as u64) <= cap, "{} NACK resends of k over a {stall} us stall, cap {cap}", nacks.len());
    } else {
        assert!(
            !delivered_before_fill,
            "k={K} filled with no NACK resend, yet a delivered send of k preceded the fill: not a decode fill, and a NACK resend is required"
        );
        println!("[s06 T2 sim] k filled by the decoder (witness: fill event, no delivered send of k before it)");
    }
    ks.iter().map(|s| s.to_string()).collect()
}

/// Reliable seat, Auto, paced: k stalls and is recovered with no slot copy.
/// The scenario runs twice; the traces and the k event lines are identical.
#[test]
fn stalled_blocker_recovered_by_nack_sim() {
    let a = scenario();
    let ka = t2(&a);
    let b = scenario();
    let kb = t2(&b);
    assert!(!a.trace.is_empty(), "empty trace");
    assert_eq!(a.trace, b.trace, "two runs of one seed gave different traces");
    assert_eq!(ka, kb, "two runs of one seed gave different k events");
}
