//! The lost tail, deterministically: every data send from source seq k's
//! first send on is swallowed for 1.5 s (test seam `RWM_TEST_TS_DROP=dead`,
//! as in wedge-diag's `lost_tail_longer_than_window_strands_k_evicting`),
//! Realtime hint, 2.5 % client-to-server loss. Run with
//! `RUSTFLAGS="--cfg tokio_unstable" cargo test -p raptorpath --features sim,test-seams --test sim_lost_tail`.

mod sim_support;
use sim_support::{run, ChildRun, Scenario};

const K: u64 = 11_800;

/// One child at a time per port (the scratch dir is named by port).
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn child() {
    sim_support::child();
}

fn lost_tail(reliable: bool, port: u16) -> ChildRun {
    let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    run(&Scenario {
        seed: 42,
        paths: 1,
        bytes: 6_000_000,
        hint: "realtime",
        reliable: Some(reliable),
        loss: 0.025,
        jitter_ms: 4,
        limit_s: 30,
        port,
        env: vec![
            ("RWM_TEST_TS_SEQ", K.to_string()),
            ("RWM_TEST_TS_DROP", "dead".into()),
            ("RWM_TEST_TS_DEAD_MS", "1500".into()),
        ],
    })
}

fn k_lines<'a>(r: &'a ChildRun, ev: &str) -> Vec<&'a str> {
    let seq = format!(" seq={K} ");
    r.tseam(ev).into_iter().filter(|l| l.contains(&seq)).collect()
}

fn summary(r: &ChildRun) -> String {
    let n = |ev: &str| k_lines(r, ev).len();
    format!(
        "outcome={} virt_s={:.3} wall_s={:.3} k: send={} fill={} pass={} pass_force={} shed={} shed_slot={} gap_clamped_lines={}",
        r.outcome,
        r.virt_s,
        r.wall_s,
        n("send"),
        n("fill"),
        n("pass"),
        n("pass_force"),
        n("shed"),
        n("shed_slot"),
        r.tseam("gap_clamped").len()
    )
}

/// KNOWN DEFECT, roadmap node `rec-evict-blocker-unresolved`: at ρ < 1 the
/// evicting seat strands k. The NACK site clamps gaps to the coding window
/// before the shed check, so once k is below the window it is neither
/// resent nor shed, and the receiver never resolves it (no fill, no pass).
/// This test asserts that the stall EXISTS. FLIP WHEN FIXED: once the fix
/// lands, k must resolve here, and this test must be inverted (assert a
/// fill or a pass of k, as `control_lost_tail_recovers_at_rho_1` does).
#[test]
fn known_defect_lost_tail_strands_k_at_rho_below_1() {
    let r = lost_tail(false, 47_200);
    let s = summary(&r);
    println!("rho<1: {s}");
    // Executes: the seam fired on k and the evicting NACK site ran.
    let sends = k_lines(&r, "send");
    assert!(sends.first().is_some_and(|l| l.contains(" dropped=1 ")), "seam did not swallow k's first send: {s}");
    assert!(!r.tseam("gap_clamped").is_empty(), "the evicting NACK site never clamped a gap: {s}");
    // The defect: k is never resent, shed or resolved within 30 s virtual.
    assert_eq!(sends.len(), 1, "k was resent (the defect may be fixed: flip this test): {s}");
    for ev in ["shed", "shed_slot", "fill", "pass", "pass_force"] {
        assert!(k_lines(&r, ev).is_empty(), "k saw ev={ev} (the defect may be fixed: flip this test): {s}");
    }
}

/// Control: the same lost tail at ρ = 1 (retain until acked) recovers: k is
/// resent, filled, and the transfer completes.
#[test]
fn control_lost_tail_recovers_at_rho_1() {
    let r = lost_tail(true, 47_210);
    let s = summary(&r);
    println!("rho=1: {s}");
    let sends = k_lines(&r, "send");
    assert!(sends.first().is_some_and(|l| l.contains(" dropped=1 ")), "seam did not swallow k's first send: {s}");
    assert!(sends.len() >= 2, "k was never resent: {s}");
    assert!(!k_lines(&r, "fill").is_empty(), "k never reached the receiver: {s}");
    assert_eq!(r.outcome, "client_ok", "transfer did not complete: {s}");
}

/// KNOWN DEFECT `rec-evict-blocker-unresolved`: once fixed, the account of a
/// completed lost-tail transfer at rho < 1 is clean (O = 0, O_strand = 0,
/// no inert sweeps, H + E within the budget). Red on main.
#[test]
#[ignore = "rec-evict-blocker-unresolved: at rho < 1 the lost tail is stranded; the account ends with O > 0"]
fn account_is_clean_after_lost_tail_at_rho_below_1() {
    let r = lost_tail(false, 47_200);
    let a = r.acct();
    println!("rho<1 account:{}", a.0);
    assert!(a.violations().is_empty(), "account: {:?}\n{}", a.violations(), a.0);
    assert_eq!(r.outcome, "client_ok", "transfer did not complete: {}", summary(&r));
}

/// Control: the account at rho = 1 is clean after the same lost tail, and its
/// execution witness shows the gap clamp ran.
#[test]
fn control_account_is_clean_at_rho_1() {
    let r = lost_tail(true, 47_210);
    let a = r.acct();
    println!("rho=1 account:{}", a.0);
    assert_eq!(r.outcome, "client_ok", "transfer did not complete: {}", summary(&r));
    assert!(a.violations().is_empty(), "account: {:?}\n{}", a.violations(), a.0);
    assert!(a.u("clamp_evals") > 0, "the clamp site never ran, so O_strand = 0 proves nothing:{}", a.0);
    assert_eq!((a.u("E"), a.u("O_strand")), (0, 0), "E and O_strand are 0 at rho = 1 by construction:{}", a.0);
}
