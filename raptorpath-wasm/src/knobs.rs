//! The three-knob laws (proposal, not accepted): lifetime T, order hold H,
//! with δ unchanged. `github/three-knobs:docs/derivations/three-knobs.md`;
//! plan `docs/derivations/visualizer-three-knobs-plan.md` §5.
//!
//! Pure f64 functions, written to `raptorpath-core`'s rules: no I/O, no
//! env, no hint type, no statics, imports only `std`. These are the ONLY
//! places T or H meets a literal or another knob (`tests/knob_deny.rs`).
//! ∞ is a value: every law is inert at T = ∞ / H = ∞ by IEEE arithmetic,
//! with no classification of ∞ anywhere.
//!
//! Names and argument order follow `tools/toy/toy2.py` (toy-stage3-knobs);
//! the toy calls `intake_expired` `intake_expired_fn`, and `ack_timeout` is
//! its method `Sim.R_now`. `tests/knob_vectors.txt` checks them bit for bit.

/// The law revision these functions implement (three-knobs.md section).
pub const KNOB_LAW_REVISION: &str = "17 (Revision 3)";

/// L1 (§2.1, stable since §2): a unit that has outlived T is never sent.
pub fn intake_expired(t: f64, age: f64) -> bool {
    age >= t
}

/// L2 (§2.2; ô per §17.1, Revision 3): a resend is worth sending only if
/// it can arrive before T. `x` is the one-way delay estimate ô.
pub fn resend_admissible(t: f64, age: f64, x: f64) -> bool {
    age + x < t
}

/// L3 readout (§2.3): a delivered unit counts as on time iff age ≤ T.
/// A measurement, never a branch.
pub fn within_lifetime(t: f64, age: f64) -> bool {
    age <= t
}

/// §13.2 (Revision 1, unchanged by §17.10): at the dequeue a unit gives its
/// slot to a younger waiting one when it can no longer arrive in time;
/// `m` is the ack timeout R.
pub fn dequeue_expired(t: f64, age: f64, m: f64) -> bool {
    age + m >= t
}

/// §17.1 (Revision 3, supersedes §15.3): a seq is protected while a resend
/// at its earliest trigger could still arrive before T. `d` is
/// `trigger_delay`, `h` the hand-over time, `o_hat` the one-way term
/// `one_way_fwd`. A seq that is not protected may be passed by the floor.
pub fn protected(t: f64, now: f64, last_send: f64, d: f64, h: f64, o_hat: f64) -> bool {
    now.max(last_send + d) - h + o_hat < t
}

/// §17.1 d(s) (Revision 3): the earliest resend after the last send. A seq
/// already resent waits the cooldown `c`; a never-resent seq waits for its
/// trigger, a report (`srtt_path`) or the sweep reaching it (`p_reach`).
/// The branch is on the seq's state, not on a knob.
pub fn trigger_delay(resent: bool, c: f64, srtt_path: f64, p_reach: f64) -> f64 {
    if resent {
        c
    } else {
        srtt_path.min(p_reach)
    }
}

/// §17.1 ô (Revision 3): the one-way delay of a resend, all queueing
/// assumed on the data direction (Q18). Equals srtt/2 with no queue.
pub fn one_way_fwd(srtt: f64, rtprop: f64) -> f64 {
    srtt - rtprop / 2.0
}

/// L6 (§2.6, stable form): a hole detected at `t_detect` is released at
/// the earlier of its hold and the floor's pass plus the floor hold H_f.
pub fn release_at(t_detect: f64, h: f64, t_floor: f64, h_f: f64) -> f64 {
    (t_detect + h).min(t_floor + h_f)
}

/// L6's bind gauge (FORMULA-FIRST: every min reports which term binds):
/// true when the hold term of `release_at` released the hole (ties go to
/// the hold). A readout, never a branch of the model.
pub fn released_by_hold(t_detect: f64, h: f64, t_floor: f64, h_f: f64) -> bool {
    t_detect + h <= t_floor + h_f
}

/// §13 (RFC 6298 §2, RFC 9002 §6.2.1): the ack timeout R, with `g` the
/// clock granularity and `ack_delay` the peer's maximum ack delay.
pub fn ack_timeout(srtt: f64, rttvar: f64, g: f64, ack_delay: f64) -> f64 {
    srtt + (4.0 * rttvar).max(g) + ack_delay
}

/// §3 of the derivation: T ∈ (0, ∞], H ∈ [0, ∞]; NaN is rejected. The
/// only place the knobs meet a domain literal.
pub fn validate_knobs(t: f64, h: f64) -> Result<(), String> {
    if !(t > 0.0) {
        return Err(format!("lifetime T must be in (0, inf], got {t}"));
    }
    if !(h >= 0.0) {
        return Err(format!("order hold H must be in [0, inf], got {h}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const INF: f64 = f64::INFINITY;

    /// One readout of each law as a number, for the ±2 % continuity check:
    /// a boolean law is continuous in T or H iff it flips only at its own
    /// threshold, so a nudge far from the threshold must not change it.
    #[test]
    fn laws_are_continuous_at_the_named_points() {
        for t0 in [0.4, 2.0] {
            for k in [0.98, 0.99, 1.0, 1.01, 1.02] {
                let t = t0 * k;
                // Ages well inside (0.5·t0) and well outside (1.5·t0) the
                // threshold read the same at every nudge.
                assert!(!intake_expired(t, 0.5 * t0) && intake_expired(t, 1.5 * t0));
                assert!(resend_admissible(t, 0.5 * t0, 0.05) && !resend_admissible(t, 1.5 * t0, 0.05));
                assert!(within_lifetime(t, 0.5 * t0) && !within_lifetime(t, 1.5 * t0));
                assert!(!dequeue_expired(t, 0.5 * t0, 0.1) && dequeue_expired(t, 1.5 * t0, 0.1));
                assert!(protected(t, 1.0, 0.9, 0.05, 1.0 - 0.3 * t0, 0.025));
                assert!(!protected(t, 1.0, 0.9, 0.05, 1.0 - 1.5 * t0, 0.025));
            }
        }
        // release_at is linear in H: a 1 % nudge of H moves it by 1 % of H.
        let h0 = 0.02;
        let base = release_at(1.0, h0, INF, 0.0);
        for k in [0.98, 0.99, 1.01, 1.02] {
            let r = release_at(1.0, h0 * k, INF, 0.0);
            assert!((r - base - h0 * (k - 1.0)).abs() < 1e-12);
        }
    }

    #[test]
    fn absolute_anchors_at_the_thresholds() {
        assert!(intake_expired(0.4, 0.4));
        assert!(!intake_expired(0.4, 0.399_999));
        assert!(!resend_admissible(0.4, 0.375, 0.025));
        assert!(resend_admissible(0.4, 0.374, 0.025));
        assert!(within_lifetime(0.4, 0.4));
        assert!(!within_lifetime(0.4, 0.400_001));
        assert!(dequeue_expired(0.4, 0.3, 0.1));
        assert!(!dequeue_expired(0.4, 0.299, 0.1));
        // max(now, last + d) − h + ô: the earliest trigger 1.05 − 0.7 + 0.025 = 0.375.
        assert!(protected(0.376, 1.0, 1.0, 0.05, 0.7, 0.025));
        assert!(!protected(0.374, 1.0, 1.0, 0.05, 0.7, 0.025));
        assert_eq!(trigger_delay(true, 0.06, 0.05, INF), 0.06);
        assert_eq!(trigger_delay(false, 0.06, 0.05, INF), 0.05);
        assert_eq!(trigger_delay(false, 0.06, 0.05, 0.025), 0.025);
        assert_eq!(one_way_fwd(0.05, 0.05), 0.025);
        assert_eq!(one_way_fwd(0.142, 0.098), 0.142 - 0.049);
        assert_eq!(release_at(1.0, 0.02, 1.5, 0.0), 1.0 + 0.02);
        assert_eq!(release_at(1.0, 0.02, 1.01, 0.0), 1.01);
        assert_eq!(ack_timeout(0.05, 0.00625, 0.001, 0.001), 0.05 + 0.025 + 0.001);
        assert_eq!(ack_timeout(0.05, 0.0, 0.001, 0.001), 0.05 + 0.001 + 0.001);
    }

    #[test]
    fn infinity_is_evaluated_by_ieee() {
        assert!(!intake_expired(INF, 1e9));
        assert!(resend_admissible(INF, 1e9, 1e9));
        assert!(within_lifetime(INF, 1e9));
        assert!(!dequeue_expired(INF, 1e9, 1e9));
        assert!(protected(INF, 1e9, 1e9, 1e9, 0.0, 1e9));
        assert_eq!(release_at(1.0, INF, INF, 0.0), INF);
        assert_eq!(release_at(1.0, INF, 2.0, 0.0), 2.0);
        assert_eq!(release_at(1.0, 0.0, INF, 0.0), 1.0);
        assert!(validate_knobs(INF, INF).is_ok());
        assert!(validate_knobs(INF, 0.0).is_ok());
    }

    #[test]
    fn validate_knobs_rejects_the_domain_errors() {
        assert!(validate_knobs(f64::NAN, 0.0).is_err());
        assert!(validate_knobs(0.4, f64::NAN).is_err());
        assert!(validate_knobs(0.0, 0.0).is_err());
        assert!(validate_knobs(-1.0, 0.0).is_err());
        assert!(validate_knobs(0.4, -0.001).is_err());
        assert!(validate_knobs(0.005, 0.0).is_ok());
    }
}
