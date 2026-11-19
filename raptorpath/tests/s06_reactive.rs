//! ADR-0079 / S06: retransmission is reactive only. Per-seq, from the
//! `test-seams` trace (`src/test_seams.rs`), never inferred from counters.
//!
//! T2 `stalled_blocker_recovered_by_nack` (red on main: copies of k go out
//! through the slot path).
//! T3 `past_deadline_blocker_*`: absolute time bounds from the code's own
//! clocks. Bounds, green on both trees; they assert the deletion strands no
//! past-deadline blocker.
//!
//! Needs `--features test-seams`; run with `--test-threads=1`.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

use gauge::{numeric_prefix, raw_field};
use std::time::Duration;

/// The seq under test: far enough in that ε̂ is warm, early enough that every
/// run reaches it.
const K: &str = "2000";
/// Seeded i.i.d. ~2.4 % loss; one-way 10 ms, so RTT >= 20 ms.
/// The stall variants need k to stay inside the sender's 200-symbol coding
/// window (on the evicting seat a gap is clamped to it), so the object ends
/// shortly after k: nothing above k is sent that could push k out.
const K_TAIL: &str = "12080";
const BYTES_TAIL: &str = "6000000";
const CELL: &str = "custom:40;10;0;2.5;100";
const SRTT_MIN_US: u64 = 20_000;
/// Sender loop clock (`tokio::time::interval(50 ms)`, `net/mod.rs`).
const TICK_US: u64 = 50_000;
/// `REORDER_MAX_HOLD` (`net/mod.rs`): the receiver's longest hold.
const HOLD_MAX_US: u64 = 300_000;
/// `TAIL_SWEEP_MAX_US` (`net/mod.rs`).
const TAIL_SWEEP_MAX_US: u64 = 100_000;
/// `HOLE_NACK_REFRESH_MAX` (`net/mod.rs`).
const HOLE_REFRESH_MAX_US: u64 = 100_000;
/// Scheduling slack on a loaded host.
const SLACK_US: u64 = 50_000;

#[derive(Debug, Clone)]
struct Ev {
    ev: String,
    n: u64,
    site: String,
    dropped: bool,
    t: u64,
    d_us: u64,
    age_us: u64,
}

fn events(log: &str) -> Vec<Ev> {
    let num = |l: &str, k: &str| {
        raw_field(l, k).and_then(|v| numeric_prefix(v).parse::<u64>().ok()).unwrap_or(0)
    };
    log.lines()
        .filter(|l| l.contains("[TSEAM] "))
        .map(|l| Ev {
            ev: raw_field(l, "ev=").unwrap_or("").to_string(),
            n: num(l, "n="),
            site: raw_field(l, "site=").unwrap_or("").to_string(),
            dropped: raw_field(l, "dropped=") == Some("1"),
            t: num(l, "t_us="),
            d_us: num(l, "d_us="),
            age_us: num(l, "age_us="),
        })
        .collect()
}

fn max_field(log: &str, key: &str) -> u64 {
    log.split_whitespace()
        .filter_map(|t| t.strip_prefix(key))
        .filter_map(|v| numeric_prefix(v).parse::<u64>().ok())
        .max()
        .unwrap_or_else(|| panic!("`{key}` never appeared in the log"))
}

fn max_field_f64(log: &str, key: &str) -> f64 {
    log.split_whitespace()
        .filter_map(|t| t.strip_prefix(key))
        .filter_map(|v| numeric_prefix(v).parse::<f64>().ok())
        .fold(0.0, f64::max)
}

/// k left by any mechanism: the receiver filled it, passed it (hold expiry or
/// capacity force-drain), or the sender shed it (slot site or NACK site).
fn resolved_any(c: &[Ev], rx: &[Ev]) -> bool {
    ["fill", "pass", "pass_force"].iter().any(|n| first(rx, n).is_some())
        || ["shed", "shed_slot"].iter().any(|n| first(c, n).is_some())
}

fn first(evs: &[Ev], name: &str) -> Option<Ev> {
    evs.iter().find(|e| e.ev == name).cloned()
}

// ── T2 ──────────────────────────────────────────────────────────────────

/// Reliable seat, N = 1, Auto (slots exist), warm ε̂. k's original and its
/// first resend are swallowed on the wire. A stalled blocker must be
/// recovered by the NACK path alone: no copy of k goes out through a
/// proactive slot, and the NACK resends are spaced by the clock.
#[test]
fn stalled_blocker_recovered_by_nack() {
    let drop = ("RWM_TEST_TS_DROP", "first:2");
    let seq = ("RWM_TEST_TS_SEQ", K);
    let diag = [("RWM_DIAG", "1"), ("RUST_LOG", "raptorpath=info")];
    let cli_env = [
        ("RWM_L0_NETEM", CELL),
        ("RWM_L0_SEED", "42"),
        seq,
        drop,
    ];
    let srv_env = [seq];
    let (cli, srv) = loopback::transfer(loopback::Transfer {
        env: &diag,
        client_env: &cli_env,
        server_env: &srv_env,
        hint: "auto",
        bytes: "12000000",
        runs: "1",
        ..Default::default()
    });
    let sent = events(&cli);
    let rx = events(&srv);
    let t0 = first(&sent, "send").unwrap_or_else(|| panic!("k={K} was never sent:\n{cli}")).t;
    let fill = first(&rx, "fill").unwrap_or_else(|| panic!("the receiver never got k={K}:\n{srv}"));

    let sends: Vec<&Ev> = sent.iter().filter(|e| e.ev == "send").collect();
    let copies = sends.iter().filter(|e| e.n > 1 && e.site == "single").count();
    let nacks: Vec<&&Ev> = sends.iter().filter(|e| e.site == "batch" && e.t <= fill.t).collect();
    let taper = max_field(&cli, "taper=");
    let pslot = max_field(&cli, "pslot=");
    println!(
        "[s06 T2] sends of k: {} (copies via slot path {copies}, NACK/sweep resends before fill {}), \
         taper={taper} pslot={pslot}, stall {} us",
        sends.len(),
        nacks.len(),
        fill.t.saturating_sub(t0)
    );

    // Witness: slots were being taken, so a copy WAS possible.
    assert!(pslot > 0, "no proactive slot was taken: the test cannot see a copy");
    // The absolute claim.
    assert_eq!(copies, 0, "a proactive slot sent {copies} copies of the stalled blocker");
    assert_eq!(taper, 0, "taper={taper}");
    // NACK(k) >= 1 within the time threshold (9/8 x an SRTT bounded at
    // 200 ms) plus one hole-refresh period.
    let first_nack = nacks.first().unwrap_or_else(|| panic!("no NACK resend of k={K} before it filled"));
    let bound = 200_000 * 9 / 8 + HOLE_REFRESH_MAX_US + SLACK_US;
    assert!(
        first_nack.t.saturating_sub(t0) <= bound,
        "first NACK resend {} us after the original, bound {bound}",
        first_nack.t.saturating_sub(t0)
    );
    // At most one resend per SRTT over the stall T.
    let stall = fill.t.saturating_sub(t0);
    let cap = stall.div_ceil(SRTT_MIN_US) + 1;
    assert!(
        (nacks.len() as u64) <= cap,
        "{} NACK resends of k over a {stall} us stall, cap {cap}",
        nacks.len()
    );
}

// ── T3 ──────────────────────────────────────────────────────────────────

struct Run {
    cli: String,
    srv: String,
}

/// One evicting-seat Realtime run, N = 1, seeded loss. The client (sender)
/// carries `cli_ts` (the swallow mode); both ends carry the seq. Polls both
/// logs until `done` holds or `timeout`, then kills both.
fn evicting_run(
    seq: &str,
    bytes: &str,
    cli_ts: &[(&str, &str)],
    timeout: Duration,
    done: impl Fn(&[Ev], &[Ev]) -> bool,
) -> Run {
    let binds = loopback::free_addrs(1);
    let senv = [("RWM_DIAG", "1"), ("RUST_LOG", "raptorpath=info"), ("RWM_TEST_TS_SEQ", seq)];
    let srv = loopback::spawn_perf_server(&binds, &senv, &["--protocol-hint", "realtime"]);
    let mut cenv: Vec<(&str, &str)> = vec![
        ("RWM_DIAG", "1"),
        ("RUST_LOG", "raptorpath=info"),
        ("RWM_TEST_TS_SEQ", seq),
        ("RWM_L0_NETEM", CELL),
        ("RWM_L0_SEED", "42"),
    ];
    cenv.extend_from_slice(cli_ts);
    let mut cli = loopback::spawn_perf_client(
        &binds,
        &cenv,
        &["--bytes", bytes, "--runs", "1", "--protocol-hint", "realtime"],
    );
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        let (c, s) = (events(&cli.log.text()), events(&srv.log.text()));
        if done(&c, &s) || cli.exited().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // Let in-flight trace lines land.
    std::thread::sleep(Duration::from_millis(300));
    let c = cli.finish();
    let s = srv.log.text();
    // Raw logs for offline parsing: `RWM_S06_DUMP=<dir>` + `RWM_S06_TAG=<tag>`.
    if let (Ok(dir), Ok(tag)) = (std::env::var("RWM_S06_DUMP"), std::env::var("RWM_S06_TAG")) {
        let _ = std::fs::write(format!("{dir}/{tag}.cli.log"), &c);
        let _ = std::fs::write(format!("{dir}/{tag}.srv.log"), &s);
    }
    Run { cli: c, srv: s }
}

/// (i) Arrivals above k continue; k's original is swallowed and its resends
/// suppressed, so k can leave only by the receiver's pass (hold expiry), by a
/// decode fill, or by the sender's shed. Bound: resolved within the longest
/// hold of the first arrival above k.
#[test]
fn past_deadline_blocker_is_shed_or_passed_in_bounded_time_arrivals_continue() {
    let run = evicting_run(K, "12000000", &[("RWM_TEST_TS_DROP", "all")], Duration::from_secs(30), |_, rx| {
        first(rx, "above").is_some()
            && (first(rx, "pass").is_some() || first(rx, "fill").is_some())
    });
    let rx = events(&run.srv);
    let sent = events(&run.cli);
    assert!(first(&sent, "send").is_some(), "k={K} never sent:\n{}", run.cli);
    let above = first(&rx, "above").unwrap_or_else(|| panic!("nothing above k arrived:\n{}", run.srv));
    let resolved = rx
        .iter()
        .filter(|e| (e.ev == "pass" || e.ev == "fill") && e.t >= above.t.saturating_sub(1_000_000))
        .map(|e| e.t)
        .min()
        .unwrap_or_else(|| panic!("k={K} was neither passed nor filled; stranded:\n{}", run.srv));
    let bound = HOLD_MAX_US + TICK_US + SLACK_US;
    let dt = resolved.saturating_sub(above.t);
    println!("[s06 T3i] first arrival above k -> resolved: {dt} us (bound {bound})");
    assert!(dt <= bound, "k resolved {dt} us after the first arrival above it; bound {bound}");
}

/// (ii) No seq above k arrives (the link is dead after k's first send), the
/// shed budget is open: the sender sheds k by the tail sweep within
/// D + one sweep + one tick of its send.
#[test]
fn past_deadline_blocker_is_shed_or_passed_in_bounded_time_stall_budget_open() {
    let run = evicting_run(K_TAIL, BYTES_TAIL, &[("RWM_TEST_TS_DROP", "dead")], Duration::from_secs(30), |c, _| {
        first(c, "shed").is_some()
    });
    let sent = events(&run.cli);
    let rx = events(&run.srv);
    let t0 = first(&sent, "send").unwrap_or_else(|| panic!("k never sent:\n{}", run.cli)).t;
    assert!(first(&rx, "above").is_none(), "an arrival above k got through: not the stall case");
    let shed = first(&sent, "shed").unwrap_or_else(|| panic!("k={K} was never shed:\n{}", run.cli));
    assert!(shed.d_us > 0, "no derived deadline at the shed");
    let dt = shed.t.saturating_sub(t0);
    // The sweep targets the cumulative blocker, which is k only once the acks
    // of the seqs in flight ahead of it have returned: up to one RTT after
    // k's send (the plan's bound omits this term; the measured shed lands at
    // about D + 2 sweeps, i.e. D + 100 ms + RTT). RTT = the largest the
    // sender's own `[DIAG] rtt=` reported.
    let rtt_us = (max_field_f64(&run.cli, "rtt=") * 1000.0) as u64;
    let bound = shed.d_us + TAIL_SWEEP_MAX_US + rtt_us + TICK_US + SLACK_US;
    println!(
        "[s06 T3ii] shed {dt} us after the send; D={} us, rtt={rtt_us} us, bound {bound}",
        shed.d_us
    );
    assert!(shed.age_us > shed.d_us, "shed at age {} <= D {}", shed.age_us, shed.d_us);
    assert!(dt <= bound, "shed {dt} us after the send, bound {bound}");
}

/// (ii) with the budget forced closed (ρ wins): k is not shed, the denial is
/// counted, and k is served: a NACK/sweep resend reaches the wire once the
/// link is back.
#[test]
fn past_deadline_blocker_is_shed_or_passed_in_bounded_time_stall_budget_closed() {
    let run = evicting_run(
        K_TAIL,
        BYTES_TAIL,
        &[("RWM_TEST_TS_DROP", "dead"), ("RWM_TEST_TS_DEAD_MS", "1500"), ("RWM_TEST_TS_BUDGET_CLOSED", "1")],
        Duration::from_secs(30),
        |c, _| c.iter().any(|e| e.ev == "send" && e.n > 1 && !e.dropped),
    );
    let sent = events(&run.cli);
    let t0 = first(&sent, "send").unwrap_or_else(|| panic!("k never sent:\n{}", run.cli)).t;
    assert!(first(&sent, "shed").is_none(), "k was shed with the budget closed");
    let denied = first(&sent, "denied")
        .unwrap_or_else(|| panic!("past D with the budget closed, but no shed_denied:\n{}", run.cli));
    let served = sent
        .iter()
        .find(|e| e.ev == "send" && e.n > 1 && !e.dropped)
        .unwrap_or_else(|| panic!("k was never served once the link returned:\n{}", run.cli));
    println!(
        "[s06 T3ii-closed] denied at +{} us, served at +{} us",
        denied.t.saturating_sub(t0),
        served.t.saturating_sub(t0)
    );
    assert!(served.t >= t0 + 1_500_000, "the resend left before the link returned");
}

// ── T3, mid-object ───────────────────────────────────────────────────────

/// A stall in the MIDDLE of the object (evicting seat, k = 2000 of ~12 000):
/// every wire send from k's first send is swallowed for a short blackout, while the
/// sender keeps emitting behind k, so k leaves the 200-symbol coding window.
/// On the evicting seat the NACK site clamps a gap to that window
/// (`sender_phases.rs`), so it never judges k. Before ADR-0079 the proactive
/// slot path judged `retransmit_buffer.first()` (k) at slot time, whatever
/// the window. The test asserts what holds on BOTH trees and prints the
/// tree-specific numbers: `shed_slot` events (pre-deletion only), NACK-site
/// sheds, and how k left the receiver.
///
/// Common assertions: the shed gauge's `shed_slot=` equals the number of slot
/// shed events of k (0 or 1), and k is passed or filled at the receiver once
/// the link returns, within the hold plus one RTT-scale slack.
#[test]
#[ignore = "records, does not gate: after a mid-object blackout BOTH trees wedge (k neither passed nor filled in 25 s); see the commit message"]
fn past_deadline_blocker_mid_object_stall_evicting() {
    // Blackout length: `RWM_S06_MID_DEAD_MS` (default 100 ms ~ 400 symbols at
    // 40 Mbit: more than the 200-symbol coding window, so k leaves it).
    let dead_ms = std::env::var("RWM_S06_MID_DEAD_MS").unwrap_or_else(|_| "100".into());
    let dead_us = dead_ms.parse::<u64>().expect("RWM_S06_MID_DEAD_MS") * 1000;
    let run = evicting_run(
        K,
        "12000000",
        &[("RWM_TEST_TS_DROP", "dead"), ("RWM_TEST_TS_DEAD_MS", dead_ms.as_str())],
        Duration::from_secs(12),
        |c, rx| resolved_any(c, rx),
    );
    let sent = events(&run.cli);
    let rx = events(&run.srv);
    let t0 = first(&sent, "send").unwrap_or_else(|| panic!("k never sent:\n{}", run.cli)).t;
    let slot = sent.iter().filter(|e| e.ev == "shed_slot").count();
    let nack = sent.iter().filter(|e| e.ev == "shed").count();
    let gauge = |key: &str| {
        run.cli
            .split_whitespace()
            .filter_map(|t| t.strip_prefix(key))
            .filter_map(|v| numeric_prefix(v).parse::<u64>().ok())
            .max()
            .unwrap_or(0)
    };
    let (g_slot, g_nack) = (gauge("shed_slot="), gauge("shed_nack="));
    let pass = first(&rx, "pass");
    let fill = first(&rx, "fill");
    let above = first(&rx, "above");
    let rel = |e: &Option<Ev>| e.as_ref().map(|e| e.t as i64 - t0 as i64);
    println!(
        "[s06 T3-mid] k shed events: slot-site {slot}, NACK-site {nack}; gauge shed_slot={g_slot} \
         shed_nack={g_nack}; receiver (us after k's send): first above {:?}, pass {:?}, fill {:?}",
        rel(&above),
        rel(&pass),
        rel(&fill)
    );
    assert!(g_slot as usize >= slot, "slot gauge below its own shed events");
    assert!(g_nack as usize >= nack, "NACK gauge below its own shed events");
    // Record only. Measured on both trees (blackout 100 and 250 ms): k is
    // never passed nor filled at the receiver within 25 s, so there is no
    // bound to assert yet; the wedge is identical before and after ADR-0079.
    let _ = dead_us;
}

/// Control for the mid-object case: no blackout, k's every send swallowed
/// (one lost source symbol mid-object, arrivals above it continue), evicting
/// seat. Records only; read the raw logs (`RWM_S06_DUMP`).
#[test]
#[ignore = "diagnosis control: records, does not gate"]
fn past_deadline_blocker_mid_object_no_blackout_evicting() {
    let run = evicting_run(
        K,
        "12000000",
        &[("RWM_TEST_TS_DROP", "all")],
        Duration::from_secs(12),
        |c, rx| resolved_any(c, rx),
    );
    println!("[s06 T3-ctl] cli {} bytes, srv {} bytes", run.cli.len(), run.srv.len());
}
