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
//! The natural 5 % scattered-loss scenario (30 MB) sheds on Windows (17) but
//! not on Linux, so the blackout is the platform-independent trigger.
//!
//! Sites: the NACK-path shed (`sender_phases.rs`, ev=`shed`) is reached.
//! The slot-site shed (`emit_source.rs`, ev=`shed_slot`) is NOT: in every
//! cell explored (loss 2-20 %, 1-300 MB, budget forced open, epsilon at send
//! forced to 0.9) it read 0; see `slot_site_is_not_reached_here`.

mod sim_support;
use sim_support::{run, ChildRun, Scenario};

#[test]
fn child() {
    sim_support::child();
}

/// One child at a time: every scenario uses the same port (a different port
/// perturbs the run, so the witness and the liveness test must share one).
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
const PORT: u16 = 47_400;
/// The 6 MB object is 12 147 source symbols; k is 60 from the end, inside the
/// 200-symbol coding window, and every data send from k's first send on is
/// swallowed for 1.5 s (`RWM_TEST_TS_DROP=dead`): the whole tail is a hole
/// older than D (about 13 ms) by the time the sender judges it.
const K: u64 = 12_087;

fn scenario(reliable: bool) -> ChildRun {
    scenario_with(reliable, Some("0.05"))
}

/// `budget_frac`: the forced 1 - rho budget (`RWM_TEST_TS_BUDGET_FRAC`); `None` leaves the derived budget.
fn scenario_with(reliable: bool, budget_frac: Option<&str>) -> ChildRun {
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
        env: [
            // A seq that never exists: the per-seq seams stay quiet but `gap_clamped`
            // is armed; SHED_ALL opens the shed trace.
            ("RWM_TEST_TS_SEQ", K.to_string()),
            ("RWM_TEST_TS_DROP", "dead".into()),
            ("RWM_TEST_TS_DEAD_MS", "1500".into()),
            ("RWM_TEST_TS_SHED_ALL", "1".into()),
        ]
        .into_iter()
        .chain(budget_frac.map(|f| ("RWM_TEST_TS_BUDGET_FRAC", f.to_string())))
        .collect(),
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
    assert!(r.tseam("shed").len() > 0, "NACK-site shed never ran: {c}");
    assert!(!r.tseam("gap_clamped").is_empty(), "the evicting NACK site never clamped a gap: {c}");
}

/// Witness (green): the NACK-path refusal runs. Without the forced budget the
/// sim sender has no loss sample, the derived budget reads 0 and every past-D
/// hole is refused (`denied`), so the shed site's other branch is reached here.
#[test]
fn witness_nack_denied_when_the_budget_is_zero() {
    let r = scenario_with(false, None);
    let c = counts(&r);
    println!("rho<1, derived budget: {c}");
    assert!(!r.tseam("denied").is_empty(), "the NACK-site refusal never ran: {c}");
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
