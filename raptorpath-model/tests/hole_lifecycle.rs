//! S7 acceptance: the evicting seat (ρ < 1) has a lost-tail counterexample;
//! the reliable seat (ρ = 1) at the same bounds passes.

use raptorpath_model::*;
use raptorpath_model::{Action, State};
use stateright::{Checker, HasDiscoveries, Model};
use std::time::Instant;

const L: u8 = 1;
/// Memory guard: a run that reaches this many states stops and FAILS (a
/// partial exploration is not a pass).
const STATE_CAP: usize = 40_000_000;

fn check(seat: Seat, finish: HasDiscoveries) -> (HoleModel, impl Checker<HoleModel>) {
    let model = HoleModel::s7(seat, L);
    let t0 = Instant::now();
    let checker = model.clone().checker().finish_when(finish).target_state_count(STATE_CAP).spawn_bfs().join();
    println!(
        "{seat:?} L={L}: {} unique states, {} states, {:.2?}",
        checker.unique_state_count(),
        checker.state_count(),
        t0.elapsed()
    );
    (model, checker)
}

/// Control: at the same bounds the reliable seat fills every hole, holds
/// nothing at the end and never exceeds the store cap.
#[test]
fn reliable_seat_resolves_every_hole() {
    let (_, checker) = check(Seat::RELIABLE, HasDiscoveries::All);
    assert!(checker.state_count() < STATE_CAP, "state cap reached: exploration incomplete");
    checker.assert_properties();
}

/// KNOWN DEFECT (the lost-tail wedge; diagnosis on branch wedge-diag,
/// bc383f29: `lost_tail_longer_than_window_strands_k_evicting`). Asserts the
/// counterexample EXISTS and has the lost-tail shape: emission stops, the
/// cumulative blocker sits below the coding window's start, every sweep of it
/// is clamped out of the window before the shed judgement, it is never shed
/// or resent, and the receiver holds no entry above it (so neither hold
/// expiry nor force-drain can pass it). Flip this test when the defect is
/// fixed.
fn print_path(model: &HoleModel, name: &str, path: Option<stateright::Path<State, Action>>) -> Option<State> {
    let path = path?;
    println!("counterexample ({name}):");
    for (_, a) in path.clone().into_vec() {
        if let Some(a) = a {
            println!("  {a:?}");
        }
    }
    let end = path.last_state().clone();
    print!("{}", model.fates(&end));
    Some(end)
}

/// The lost-tail shape on a terminal state: emission stopped, the cumulative
/// blocker sits below the coding window's start, every sweep of it was
/// clamped out of the window before the shed judgement, it was never shed or
/// resent, and the receiver holds no entry above it (so neither hold expiry
/// nor force-drain can pass it).
fn assert_lost_tail(model: &HoleModel, end: &State) {
    assert!(model.is_terminal(end));
    assert_eq!(end.total, Some(model.object), "the red trace takes the stop branch");
    let blocker = HoleModel::first_outstanding(end).expect("a seq is held forever");
    let (win_start, _) = model.window_at(end).unwrap();
    assert!(blocker < win_start, "blocker {blocker} below window start {win_start}");
    assert_eq!(end.sweep_seq, blocker);
    assert!(end.sweep_clamped >= model.timer_budget, "every sweep clamped out");
    // Never shed: it is still outstanding (a shed removes it,
    // net/sender_phases.rs:860).
    assert_eq!(end.ever_served & (1 << blocker), 0, "never resent");
    assert_eq!(model.pending_at(end), 0, "receiver holds nothing above the hole");
    assert!(end.received & (1 << blocker) == 0 && end.frontier <= blocker);
}

/// KNOWN DEFECT (the lost-tail wedge; diagnosis on branch wedge-diag,
/// bc383f29: `lost_tail_longer_than_window_strands_k_evicting`). Asserts the
/// counterexamples EXIST and have the lost-tail shape: for the whole object,
/// and for a tail behind a delivered prefix. Flip this test when the defect
/// is fixed.
#[test]
fn evicting_seat_strands_a_lost_tail_known_defect() {
    // BFS stops once every failure property has a counterexample, so the
    // state count printed may be a partial exploration.
    let (model, checker) = check(
        Seat::EVICTING,
        HasDiscoveries::AllOf(
            [P_SENDER_CLEAN, P_RECEIVER_PASSED, P_TAIL_CLEAN, P_RELEASED_PASSED]
                .into_iter()
                .collect(),
        ),
    );
    let end = print_path(&model, P_SENDER_CLEAN, checker.discovery(P_SENDER_CLEAN))
        .expect("the evicting seat must have a lost-tail counterexample");
    assert_lost_tail(&model, &end);

    let end = print_path(&model, P_TAIL_CLEAN, checker.discovery(P_TAIL_CLEAN))
        .expect("the evicting seat must strand a tail behind a delivered prefix");
    assert_lost_tail(&model, &end);
    assert!(end.frontier > 0);
    let tail = model.object - end.frontier;
    assert!(tail > model.window, "the stranded tail ({tail}) is longer than the window");

    // Reported, not acceptance: the receiver side (a sender shed alone does
    // not resolve the receiver's hole).
    if print_path(&model, P_RECEIVER_PASSED, checker.discovery(P_RECEIVER_PASSED)).is_none() {
        println!("{P_RECEIVER_PASSED}: holds");
    }
    if print_path(&model, P_RELEASED_PASSED, checker.discovery(P_RELEASED_PASSED)).is_none() {
        println!("{P_RELEASED_PASSED}: holds");
    }
}
