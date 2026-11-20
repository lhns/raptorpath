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
/// nothing at the end and never exceeds the store cap; the executes witness
/// shows the unclamped path served a gap below the window start.
#[test]
fn reliable_seat_resolves_every_hole() {
    let (_, checker) = check(Seat::RELIABLE, HasDiscoveries::All);
    assert!(checker.state_count() < STATE_CAP, "state cap reached: exploration incomplete");
    checker.assert_properties();
}

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

/// KNOWN DEFECT (the lost-tail wedge; diagnosis on branch wedge-diag,
/// bc383f29: `lost_tail_longer_than_window_strands_k_evicting`). Asserts the
/// lost-tail counterexamples EXIST (`P_NO_LOST_TAIL`: emission stops, the
/// cumulative blocker sits below the window start, every sweep of it is
/// clamped out before the shed judgement, it is never resent or shed, and the
/// receiver holds no entry above it, so neither hold expiry nor force-drain
/// can pass it), for the whole object and behind a delivered prefix. Flip
/// this test when the defect is fixed.
#[test]
fn evicting_seat_strands_a_lost_tail_known_defect() {
    // BFS stops once every listed property has a counterexample, so the state
    // count printed is a PARTIAL exploration.
    let (model, checker) = check(
        Seat::EVICTING,
        HasDiscoveries::AllOf(
            [
                P_NO_LOST_TAIL,
                P_NO_LOST_TAIL_BEHIND_PREFIX,
                P_SENDER_CLEAN,
                P_RECEIVER_PASSED,
                P_RELEASED_PASSED,
            ]
            .into_iter()
            .collect(),
        ),
    );
    // The shape is the property, so any discovery has it; re-check anyway.
    let end = print_path(&model, P_NO_LOST_TAIL, checker.discovery(P_NO_LOST_TAIL))
        .expect("the evicting seat must strand a blocker below the window");
    assert!(model.lost_tail_shape(&end));
    let end = print_path(
        &model,
        P_NO_LOST_TAIL_BEHIND_PREFIX,
        checker.discovery(P_NO_LOST_TAIL_BEHIND_PREFIX),
    )
    .expect("the evicting seat must strand a tail longer than W behind a delivered prefix");
    assert!(model.lost_tail_shape(&end) && end.frontier > 0);

    // Reported, not acceptance: the broader properties, and the receiver side
    // (a sender shed alone does not resolve the receiver's hole).
    for p in [P_SENDER_CLEAN, P_RECEIVER_PASSED, P_RELEASED_PASSED] {
        if print_path(&model, p, checker.discovery(p)).is_none() {
            println!("{p}: holds");
        }
    }
}
