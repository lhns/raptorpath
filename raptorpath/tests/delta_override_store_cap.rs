//! `RWM_DELTA` must move δ, not break the store law (shared-core-design F1).
//!
//! `codel_setpoint_q` reads both band ends through `delta_budget_b(hint)`, which
//! goes through `delta_price`, which returns the `RWM_DELTA` override when set.
//! With the override both ends collapse to one value and the interpolation is
//! 0/0, so the shipped value multiplier `1 + q` (RWM_DELTA_CAP, on) is NaN and
//! `pooled_store_cap` pins at its floor. Own test binary: gates resolve once per
//! process, so the variable is set before the first gate read.

use raptorpath::net::{codel_setpoint_q, delta_budget_b_of, pooled_store_cap, CODEL_TARGET_HI, CODEL_TARGET_LO};

#[test]
fn rwm_delta_keeps_the_codel_setpoint_finite_and_the_store_cap_off_its_floor() {
    std::env::set_var("RWM_DELTA", "0.05");
    let b = delta_budget_b_of(0.05);
    let q = codel_setpoint_q(b);
    assert!(q.is_finite(), "q(delta) is {q} with RWM_DELTA set (band ends collapsed onto the override)");
    assert!((CODEL_TARGET_LO..=CODEL_TARGET_HI).contains(&q), "q {q} outside the CoDel band");
    // A two-path pipe of 2 x 190 symbols: the law's value is ~ (1 + q)·380, far above a floor of 10.
    let cap = pooled_store_cap(true, true, true, b, 2, 380.0, 1.0, 10, 2048).unwrap();
    assert!(cap > 10, "store cap pinned at the floor ({cap}) with RWM_DELTA set");
}
