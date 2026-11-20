//! Reach and liveness of the sender's shed sites, deterministically (ADR-0082).
//! Realtime hint, one path, 2.5 % client-to-server loss, seed 42, 6 MB, a 1.5 s
//! blackout over the last 60 symbols (the same seam as `sim_lost_tail`).
//! Run with
//! `RUSTFLAGS="--cfg tokio_unstable" cargo test -p raptorpath --features sim,test-seams --test sim_shed_reach`
//! (`-- --ignored` for the known-defect liveness test).
//!
//! Two seams make the sites reachable in the sim (`src/test_seams.rs`):
//! * `RWM_TEST_TS_SHED_ALL=1` prints every shed / denied / frontier event.
//! * `RWM_TEST_TS_BUDGET_FRAC=0.05` fixes the 1 - rho budget. The sim sender
//!   hands the whole object to the link in ~1 s, before the loss estimator has
//!   a sample, so the derived budget reads 0.0000 for the whole run and no
//!   hole is ever shed (the refusal `denied` fires instead). 0.05 is twice the
//!   link's loss rate.
//! The natural 5 % scattered-loss scenario (30 MB) shed 17 on Windows and 0
//! on Linux while the link streams were keyed by port; it sheds 0 on both
//! now, and its traces still differ between the OSes (tokio's select RNG is
//! drawn a different number of times per OS, `docs/derivations/sim-engine.md`
//! "What a seed fixes"). The blackout is the trigger.
//!
//! Sites: the NACK-path shed (`sender_phases.rs`, ev=`shed`) is reached.
//! The slot-site shed (`emit_source.rs`, ev=`shed_slot`) is NOT reached by
//! that scenario (`slot_site_is_not_reached_here`): `perf::client` hands the
//! whole object over at once and the evicting seat has no intake bound, so
//! every source symbol is emitted at one virtual instant. The slot site runs
//! only at a source emission (`emit_source`), so it sees age 0 against D,
//! eps-at-send 0 (no ack yet) and a repair rate of 0 (no spare window).
//!
//! The paced scenario (`paced`) feeds the same object at 20 Mbit/s, half
//! the link (`SIM_PACE_MBIT`, `sim_support::paced_client`). Emission then
//! interleaves with acks, and the slot site sheds (seed 42, budget forced to
//! 0.05: 90 on Windows, 98 on Linux CI; 0 to 4 per seed with the derived
//! budget, which binds). `witness_slot_site_sheds_paced` gates that, and
//! `frontier_passes_every_shed_paced` that the receiver passed every hole.

mod sim_support;
use sim_support::{run, ChildRun, Scenario};

#[test]
fn child() {
    sim_support::child();
}

/// One child at a time, on one port (the port no longer perturbs the run,
/// `sim_determinism::same_seed_two_ports_*`; kept for one temp dir per run).
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
const PORT: u16 = 47_400;
/// The 6 MB object is 12 147 source symbols; k is 60 from the end, inside the
/// 200-symbol coding window, and every data send from k's first send on is
/// swallowed for 1.5 s (`RWM_TEST_TS_DROP=dead`): the whole tail is a hole
/// older than D (about 13 ms) by the time the sender judges it.
const K: u64 = 12_087;

fn scenario(reliable: bool) -> ChildRun {
    let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    run(&Scenario {
        seed: 42,
        paths: 1,
        bytes: 6_000_000,
        hint: "realtime",
        reliable: Some(reliable),
        loss: 0.025,
        jitter_ms: 4,
        limit_s: 40,
        port: PORT,
        env: vec![
            // A seq that never exists: the per-seq seams stay quiet but `gap_clamped`
            // is armed; SHED_ALL opens the shed trace.
            ("RWM_TEST_TS_SEQ", K.to_string()),
            ("RWM_TEST_TS_DROP", "dead".into()),
            ("RWM_TEST_TS_DEAD_MS", "1500".into()),
            ("RWM_TEST_TS_SHED_ALL", "1".into()),
            ("RWM_TEST_TS_BUDGET_FRAC", "0.05".into()),
        ],
    })
}

/// The same link, ρ < 1, the object fed at 20 Mbit/s and no blackout: the
/// holes are the link's own 2.5 % losses, judged while emission continues.
/// Budget forced to 0.05 (twice the loss rate), as above: the derived one
/// sheds 0 to 4 per seed at the slot site, too few to gate on.
fn paced() -> ChildRun {
    let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    run(&Scenario {
        seed: 42,
        paths: 1,
        bytes: 6_000_000,
        hint: "realtime",
        reliable: Some(false),
        loss: 0.025,
        jitter_ms: 4,
        limit_s: 40,
        port: PORT,
        env: vec![
            ("SIM_PACE_MBIT", "20".into()),
            ("RWM_TEST_TS_SHED_ALL", "1".into()),
            ("RWM_TEST_TS_BUDGET_FRAC", "0.05".into()),
        ],
    })
}

fn field(l: &str, k: &str) -> u64 {
    l.split(' ').find_map(|f| f.strip_prefix(k)).and_then(|v| v.parse().ok()).unwrap_or_else(|| panic!("no {k} in {l}"))
}

fn counts(r: &ChildRun) -> String {
    let shed_seqs: Vec<u64> = r.tseam("shed").into_iter().chain(r.tseam("shed_slot")).map(|l| field(l, "seq=")).collect();
    let (min_shed, max_shed) = (shed_seqs.iter().min(), shed_seqs.iter().max());
    let last_next = r.tseam("frontier").last().map(|l| field(l, "next="));
    let last_diag = r.log.lines().rev().find(|l| l.starts_with("[DIAG]")).unwrap_or("");
    let tok = |k: &str| last_diag.split(' ').find(|w| w.starts_with(k)).unwrap_or("?").to_string();
    // Shape: `sweeps` frozen with the frontier below the shed seqs = the sender
    // is stuck on its first retransmit-buffer entry; a frontier past every
    // shed seq with no completion would be the other shape.
    format!(
        "shed_seq_range=[{min_shed:?}..{max_shed:?}] last_frontier_next={last_next:?} diag[{} {} {}] outcome={} virt_s={:.2} shed(nack)={} shed_slot={} denied={} denied_slot={} clamped_lines={} frontier_lines={}",
        tok("cum="),
        tok("sweeps="),
        tok("shed="),
        r.outcome,
        r.virt_s,
        r.tseam("shed").len(),
        r.tseam("shed_slot").len(),
        r.tseam("denied").len(),
        r.tseam("denied_slot").len(),
        r.tseam("gap_clamped").len(),
        r.tseam("frontier").len()
    )
}

/// The data sender's final cumulative ack: the third field of
/// `[DIAG] cum=src/cod/ack` (`net/diag.rs`), i.e. the receiver's in-order
/// frontier as last reported, read from the last line with the largest src
/// (the peer's own sender prints `[DIAG]` too). The `frontier` seam samples
/// every 100 ms and stops short of the end even in a completed run, so it is
/// not used here.
fn final_cum_ack(r: &ChildRun) -> (u64, u64) {
    let cum = |l: &str| -> Option<(u64, u64)> {
        let c = l.split(' ').find_map(|w| w.strip_prefix("cum="))?;
        let v: Vec<u64> = c.split('/').filter_map(|x| x.parse().ok()).collect();
        (v.len() == 3).then(|| (v[0], v[2]))
    };
    r.log
        .lines()
        .filter(|l| l.starts_with("[DIAG]"))
        .filter_map(cum)
        .fold(None, |best: Option<(u64, u64)>, c| match best {
            Some(b) if b.0 > c.0 => Some(b),
            _ => Some(c),
        })
        .expect("no [DIAG] cum= line")
}

/// The failure shape per site: of each site's shed seqs, how many lie at or
/// above the sender's final cumulative ack (never passed), and the lowest.
fn pass_shape(r: &ChildRun) -> String {
    let (src, ack) = final_cum_ack(r);
    let site = |ev: &str| {
        let seqs: Vec<u64> = r.tseam(ev).iter().map(|l| field(l, "seq=")).collect();
        let stuck: Vec<u64> = seqs.iter().copied().filter(|&s| s >= ack).collect();
        format!("{ev}: {} shed, {} never passed (lowest {:?})", seqs.len(), stuck.len(), stuck.iter().min())
    };
    format!("src={src} final_cum_ack={ack}; {}; {}", site("shed"), site("shed_slot"))
}

/// Liveness: the transfer completes, and every shed hole is passed by the
/// receiver's in-order frontier within a bound derived from the deadline.
///
/// Bound for a hole shed at t_s: the sender sheds only at age > D (`d_us` on
/// the event), after which nothing serves the hole, so the receiver must
/// resolve it by itself (force-drain or hold expiry). Hold expiry is at most
/// H_max (the largest `hold_us` in force, REORDER_MAX_HOLD = 300 ms), the
/// frontier is sampled every 100 ms, and the shed to receiver delay is at
/// most the largest observed D-scale term, so
///     t_pass(s) <= t_s + H_max + 100 ms + d_us,
/// where t_pass(s) is the first frontier sample with next > s.
fn assert_live(r: &ChildRun, what: &str) {
    let c = counts(r);
    assert_eq!(r.outcome, "client_ok", "{what}: transfer did not complete: {c}");
    let frontier: Vec<(u64, u64, u64)> =
        r.tseam("frontier").iter().map(|l| (field(l, "t_us="), field(l, "next="), field(l, "hold_us="))).collect();
    let h_max = frontier.iter().map(|f| f.2).max().unwrap_or(0).max(300_000);
    for l in r.tseam("shed").into_iter().chain(r.tseam("shed_slot")) {
        let (s, t_s, d) = (field(l, "seq="), field(l, "t_us="), field(l, "d_us="));
        let by = t_s + h_max + 100_000 + d;
        let passed = frontier.iter().find(|f| f.1 > s).map(|f| f.0);
        assert!(passed.is_some_and(|t| t <= by), "{what}: frontier never passed shed seq {s} by t_us={by} (passed at {passed:?}): {c}");
    }
}

/// Witness (green): the NACK-path shed site runs at rho < 1.
#[test]
fn witness_nack_site_sheds_at_rho_below_1() {
    let r = scenario(false);
    let c = counts(&r);
    println!("rho<1: {c}");
    // The account in the same run (ADR-0081). Observed on main (seed 42, forced 5 % budget):
    // H=85 (all NACK site; shed_visit_nack=85), E=11946, A=116, O=0, O_strand=0 at the end,
    // inert_sweeps=10 of 73, clamp_excl=3084 of 3149. O and O_strand do NOT fire here (the
    // sheds clear the outstanding set); inert sweeps and H+E over the budget do.
    let a = r.acct();
    println!("rho<1 account:{}", a.0);
    assert!(a.u("shed_visit_nack") >= a.u("H") && a.u("H") > 0, "the shed check ran {} times for H={}:{}", a.u("shed_visit_nack"), a.u("H"), a.0);
    assert!(a.u("clamp_excl") > 0, "the clamp site never excluded a gap:{}", a.0);
    assert!(a.u("inert_sweeps") > 0, "D1c: no inert sweep (defect fixed? flip):{}", a.0);
    assert!(a.violations().iter().any(|v| v.starts_with("H+E")), "H+E within the budget:{}", a.0);
    assert!(r.tseam("shed").len() > 0, "NACK-site shed never ran: {c}");
    assert!(!r.tseam("gap_clamped").is_empty(), "the evicting NACK site never clamped a gap: {c}");
}

/// Witness (green): the slot site stays at 0 in this scenario. When a change
/// makes it reachable, this fails: flip it to `> 0` and add a liveness arm.
#[test]
fn slot_site_is_not_reached_here() {
    let r = scenario(false);
    let c = counts(&r);
    assert_eq!(r.tseam("shed_slot").len(), 0, "slot site now reached (flip this test): {c}");
}

/// KNOWN DEFECT `rec-evict-blocker-unresolved` (ADR-0079 Amendment 1).
#[test]
#[ignore = "rec-evict-blocker-unresolved: at rho < 1 a hole below the coding window is neither resent nor resolved; the transfer never completes"]
fn liveness_after_shed_at_rho_below_1() {
    let r = scenario(false);
    println!("rho<1: {}", counts(&r));
    assert_live(&r, "rho<1");
}

/// Control (not ignored): the same scenario at rho = 1 is live.
#[test]
fn control_liveness_at_rho_1() {
    let r = scenario(true);
    println!("rho=1: {}", counts(&r));
    assert_live(&r, "rho=1");
}

/// KNOWN DEFECT `rec-evict-blocker-unresolved`: once fixed, the account of the
/// completed blackout transfer at rho < 1 is clean. Red on main.
#[test]
#[ignore = "rec-evict-blocker-unresolved: at rho < 1 a hole below the coding window is neither resent nor resolved; inert sweeps, and H+E above the budget"]
fn account_is_clean_after_shed_at_rho_below_1() {
    let r = scenario(false);
    let a = r.acct();
    println!("rho<1 account:{}", a.0);
    assert!(a.violations().is_empty(), "account: {:?}\n{}", a.violations(), a.0);
    // No client_ok gate at rho < 1: the perf server needs every chunk and the link abandons some by design.
}

/// Control (not ignored): the account at rho = 1 is clean after the blackout,
/// and the clamp site ran.
#[test]
fn control_account_is_clean_at_rho_1_after_blackout() {
    let r = scenario(true);
    let a = r.acct();
    println!("rho=1 account:{}", a.0);
    assert_eq!(r.outcome, "client_ok", "transfer did not complete: {}", counts(&r));
    assert!(a.violations().is_empty(), "account: {:?}\n{}", a.violations(), a.0);
    assert!(a.u("clamp_evals") > 0, "the clamp site never ran:{}", a.0);
}

/// Witness (green): the slot site sheds at rho < 1 once emission is paced.
/// Red with the slot-site shed stubbed out (and on a tree without it).
#[test]
fn witness_slot_site_sheds_paced() {
    let r = paced();
    let c = counts(&r);
    println!("paced rho<1: {c}");
    assert!(r.tseam("shed_slot").len() >= 10, "slot-site shed (nearly) never ran: {c}");
    assert!(r.tseam("shed").len() > 0, "NACK-site shed never ran: {c}");
}

/// Frontier liveness of the paced scenario: the sender's final cumulative
/// ack covers every source seq, so every shed hole (both sites) was passed.
/// Completion (`client_ok`) is not asserted: `perf::server` acks an object
/// only when every chunk arrived, which rho < 1 does not promise.
#[test]
fn frontier_passes_every_shed_paced() {
    let r = paced();
    let shape = pass_shape(&r);
    println!("paced rho<1: {} | {shape}", counts(&r));
    let (src, ack) = final_cum_ack(&r);
    assert_eq!(ack, src, "the receiver frontier stopped short: {shape}");
}

/// Record only: the paced scenario with the derived budget (no
/// `RWM_TEST_TS_BUDGET_FRAC`), where the shared 1 - rho budget binds, and
/// with a 100 ms blackout from seq 6000 on (holes that age below the
/// window while emission continues). Prints per-site counts and the shape.
#[test]
#[ignore = "record only: per-site shed counts for the tree comparison (ADR-0079 Amendment 1)"]
fn record_paced_cells() {
    let cells: [(&str, Vec<(&'static str, String)>); 2] = [
        ("derived budget", vec![]),
        (
            "budget 0.05, blackout 100 ms from seq 6000",
            vec![
                ("RWM_TEST_TS_BUDGET_FRAC", "0.05".into()),
                ("RWM_TEST_TS_SEQ", "6000".into()),
                ("RWM_TEST_TS_DROP", "dead".into()),
                ("RWM_TEST_TS_DEAD_MS", "100".into()),
            ],
        ),
    ];
    for (name, extra) in cells {
        let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let mut env = vec![("SIM_PACE_MBIT", "20".to_string()), ("RWM_TEST_TS_SHED_ALL", "1".into())];
        env.extend(extra);
        let r = run(&Scenario {
            seed: 42,
            paths: 1,
            bytes: 6_000_000,
            hint: "realtime",
            reliable: Some(false),
            loss: 0.025,
            jitter_ms: 4,
            limit_s: 40,
            port: PORT,
            env,
        });
        println!("paced [{name}]: {} | {}", counts(&r), pass_shape(&r));
    }
}
