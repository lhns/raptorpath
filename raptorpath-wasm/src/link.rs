//! The forward leg of one path (addendum 1 A2, A4 of
//! docs/derivations/visualizer-three-knobs-plan.md): a FIFO bottleneck with a
//! service time, a queue limit in time and a propagation delay, in seconds.
//!
//! Pure functions, the same arithmetic as the offline simulation's
//! `Sim.fwd` (tools/toy/toy2.py on toy-stage3-knobs) with `sv = 1/C`,
//! `q = Q/C` and the propagation delay as its one-way delay; checked bit for
//! bit against it by `tests/link_vectors.rs`. No knob of the three-knob model
//! appears here (`tests/knob_deny.rs`).
//!
//! The defaults are values: `sv = 0`, `q = ∞`, `d = 0` give departure =
//! arrival = the send instant, and the drop test reads `0 ≥ ∞`, false.

/// One send at `t` (s) on a leg whose server is free from `*free` (s).
/// Returns `None` when the queue drops it (backlog `max(0, free − t) ≥ q`;
/// no state changes then), else `(departure, arrival)` and the server is
/// busy until the departure.
pub fn leg_send(free: &mut f64, t: f64, sv: f64, q: f64, d: f64) -> Option<(f64, f64)> {
    if (*free - t).max(0.0) >= q {
        return None;
    }
    let depart = t.max(*free) + sv;
    *free = depart;
    Some((depart, depart + d))
}

/// The arrival a dropped datagram would have had (departure as if served,
/// plus the propagation delay): its feedback is due from there, so a path's
/// feedback stays in arrival order.
pub fn would_arrive(free: f64, t: f64, sv: f64, d: f64) -> f64 {
    t.max(free) + sv + d
}

/// The domain of one path's leg: no negative or NaN value, a queue limit
/// above 0, and a propagation delay within half the path's base RTT
/// `rtt_s` (the rest is the reverse leg).
pub fn validate_link(sv: f64, q: f64, d: f64, rtt_s: f64) -> Result<(), String> {
    if !(sv >= 0.0) {
        return Err(format!("service time must be in [0, inf), got {sv}"));
    }
    if !(q > 0.0) {
        return Err(format!("queue limit must be in (0, inf], got {q}"));
    }
    if !(d >= 0.0 && d <= rtt_s / 2.0) {
        return Err(format!("propagation delay must be in [0, rtt/2 = {}], got {d}", rtt_s / 2.0));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const INF: f64 = f64::INFINITY;

    #[test]
    fn the_default_leg_is_the_send_instant() {
        let mut free = 0.0;
        for t in [0.0, 0.001, 0.001, 0.5, 2.0] {
            let (dep, arr) = leg_send(&mut free, t, 0.0, INF, 0.0).expect("never drops");
            assert_eq!((dep.to_bits(), arr.to_bits()), (t.to_bits(), t.to_bits()));
            assert!(free <= t);
        }
    }

    #[test]
    fn a_bottleneck_queues_then_drops() {
        let mut free = 0.0;
        // 1 ms service, 2.5 ms of buffer, 10 ms propagation; four sends at once.
        let out: Vec<_> = (0..4).map(|_| leg_send(&mut free, 0.0, 0.001, 0.0025, 0.01)).collect();
        let near = |a: f64, b: f64| (a - b).abs() < 1e-15;
        for (k, o) in out[..3].iter().enumerate() {
            let (dep, arr) = o.expect("queued");
            assert!(near(dep, 0.001 * (k + 1) as f64) && near(arr, dep + 0.01), "{k}: {dep} {arr}");
        }
        assert!(out[3].is_none()); // backlog 3 ms ≥ 2.5 ms
        assert!(near(free, 0.003));
        assert!(near(would_arrive(free, 0.0, 0.001, 0.01), 0.014));
    }

    #[test]
    fn the_domain() {
        assert!(validate_link(0.0, INF, 0.0, 0.05).is_ok());
        assert!(validate_link(0.0, INF, 0.025, 0.05).is_ok());
        assert!(validate_link(-1e-9, INF, 0.0, 0.05).is_err());
        assert!(validate_link(f64::NAN, INF, 0.0, 0.05).is_err());
        assert!(validate_link(0.0, 0.0, 0.0, 0.05).is_err());
        assert!(validate_link(0.0, f64::NAN, 0.0, 0.05).is_err());
        assert!(validate_link(0.0, INF, 0.0251, 0.05).is_err());
        assert!(validate_link(0.0, INF, -0.001, 0.05).is_err());
        assert!(validate_link(0.0, INF, f64::NAN, 0.05).is_err());
    }
}
