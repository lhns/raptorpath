//! Closed forms of the span law (paper §5.3/§5.6, ADR-0076 phase 6).
//! Kept in their own file, below the engine-callable code in lib.rs, so
//! adding them does not shift the line numbers of the code above.

use crate::span_horizon_b;

/// The span law's recovery deadline D(delta) (paper §5.3/§5.6): past D,
/// recovery belongs to ARQ/deficit, since a repair or retransmit fired at
/// age > D lands after the receiver's own delta-horizon give-up.
///
/// ```text
///     D(delta) = min(b(delta) * RTprop, 2 * RTprop)
/// ```
///
/// `rtprop` in seconds (any consistent time unit); b is `span_horizon_b`.
pub fn span_deadline_d(delta_price: f64, rtprop: f64) -> f64 {
    (span_horizon_b(delta_price) * rtprop).min(2.0 * rtprop)
}

/// The coding quantum / span width A* (paper §5.3): the recovery budget in
/// symbols N_delta = rate * D, clamped into the window. Unrounded; callers
/// quantize to their retained grid.
///
/// ```text
///     A* = clamp(rate * D, 1, max(W, 1))
/// ```
pub fn span_width_a_star(rate: f64, d: f64, w: f64) -> f64 {
    (rate * d).clamp(1.0, w.max(1.0))
}

/// Pipeline depth M* (paper §5.3): generations in flight over 2*RTprop,
/// clamped to the declared cold-start floor 2 and memory ceiling 32
/// (GEN_PIPE_MAX_GENS). `a_star_q` is A* quantized to the retained grid.
///
/// ```text
///     M* = clamp(ceil(rate * 2 * RTprop / max(A*_q, 1)) + 1, 2, 32)
/// ```
pub fn pipeline_depth_m_star(rate: f64, rtprop: f64, a_star_q: f64) -> f64 {
    ((rate * 2.0 * rtprop / a_star_q.max(1.0)).ceil() + 1.0).clamp(2.0, 32.0)
}

/// Trailing offset Delta (paper §5.3, ADR-0064): the span must end Delta
/// behind the send frontier so every member has landed when the repair
/// does; a repair is solvable at arrival iff it trails the frontier by
/// Delta. Floor 1 = the FIFO-per-path minimum; J = jitter anchor.
///
/// ```text
///     Delta = clamp(ceil(rate * max(J, 0)), 1, 64)
/// ```
pub fn trailing_offset_delta(rate: f64, jitter: f64) -> f64 {
    (rate * jitter.max(0.0)).ceil().clamp(1.0, 64.0)
}

/// The visualizer model's delta-continuum -> tail-target map (paper §4.1,
/// §11.3): a monotone log-log interpolation through the three preset
/// anchors (delta_price, tail target): Realtime (50, 1e-7), Auto
/// (0.5, 1e-5), Bulk (0.005, 0.05 = the late-is-fine BULK_TAIL_BUDGET of
/// §4.5; tail >= eps takes r* to 0 = pure ARQ, with no mode bit).
///
/// ```text
///     log10 T(delta) = piecewise-linear in x = log10(delta) through
///         (log10 50, -7), (log10 0.5, -5), (log10 0.005, log10 0.05),
///         held constant outside [0.005, 50]
/// ```
///
/// Continuous everywhere with a slope kink at Auto (continuity, not
/// smoothness). Differs from the engine's t_tail at Bulk; the divergence is
/// bounded in raptorpath-wasm by `test_sim_engine_tail_anchor_divergence_is_bounded`.
pub fn sim_tail_target_of_delta(delta_price: f64) -> f64 {
    let x = delta_price.max(1e-12).log10();
    let (x_rt, x_auto, x_bulk) = (50.0f64.log10(), 0.5f64.log10(), 0.005f64.log10());
    let (y_rt, y_auto, y_bulk) = (-7.0f64, -5.0f64, 0.05f64.log10());
    let y = if x >= x_rt {
        y_rt
    } else if x >= x_auto {
        y_auto + (x - x_auto) * (y_rt - y_auto) / (x_rt - x_auto)
    } else if x >= x_bulk {
        y_auto + (x - x_auto) * (y_bulk - y_auto) / (x_bulk - x_auto)
    } else {
        y_bulk
    };
    10.0f64.powf(y)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- the span law / tail-target closed forms moved from raptorpath-wasm
    // (ADR-0076 phase 6). Absolute values pinned from the model as shipped;
    // +-2% nudges at each preset check continuity + monotonicity
    // (CLAUDE.md no-mode-switch enforcement).

    const PRESETS: [f64; 3] = [50.0, 0.5, 0.005];

    fn rel(a: f64, b: f64) -> f64 {
        (a - b).abs() / b.abs().max(1e-300)
    }

    /// D(delta) = min(b*RTprop, 2*RTprop): 0.5/1/2 RTprop at the presets.
    #[test]
    fn span_deadline_d_pinned_and_continuous() {
        assert!(rel(span_deadline_d(50.0, 0.1), 0.05) < 1e-12);
        assert!(rel(span_deadline_d(0.5, 0.1), 0.1) < 1e-12);
        assert!(rel(span_deadline_d(0.005, 0.1), 0.2) < 1e-9);
        assert_eq!(span_deadline_d(1e-9, 0.1), 0.2, "2*RTprop cap");
        assert_eq!(span_deadline_d(0.5, 0.0), 0.0);
        for &p in &PRESETS {
            let (hi, mid, lo) = (
                span_deadline_d(p * 1.02, 0.1),
                span_deadline_d(p, 0.1),
                span_deadline_d(p * 0.98, 0.1),
            );
            assert!(hi <= mid && mid <= lo, "D non-increasing in delta at {p}");
            assert!(rel(hi, mid) < 0.02 && rel(lo, mid) < 0.02, "D continuous at {p}");
        }
    }

    /// A* = clamp(rate*D, 1, max(W,1)), unrounded; composed with D(delta) it
    /// is continuous and non-increasing in delta through every preset.
    #[test]
    fn span_width_a_star_pinned_and_continuous() {
        assert!(rel(span_width_a_star(200.0, 0.02, 512.0), 4.0) < 1e-12);
        assert_eq!(span_width_a_star(1000.0, 0.2, 64.0), 64.0);
        assert_eq!(span_width_a_star(10.0, 0.002, 512.0), 1.0);
        assert_eq!(span_width_a_star(10.0, 0.5, 0.0), 1.0, "W floor 1");
        assert!(rel(span_width_a_star(300.0, 0.1, 512.0), 30.0) < 1e-12, "unrounded");
        assert!(span_width_a_star(333.0, 0.0123, 512.0).fract() != 0.0, "A* is not rounded");
        // at the presets, rate 1000/s, RTprop 100 ms, W 512: 50 / 100 / 200
        for (&p, want) in PRESETS.iter().zip([50.0, 100.0, 200.0]) {
            let a = |d: f64| span_width_a_star(1000.0, span_deadline_d(d, 0.1), 512.0);
            assert!(rel(a(p), want) < 1e-9, "A*({p})");
            assert!(a(p * 1.02) <= a(p) && a(p) <= a(p * 0.98));
            assert!(rel(a(p * 1.02), a(p)) < 0.02 && rel(a(p * 0.98), a(p)) < 0.02);
        }
    }

    /// M* = clamp(ceil(rate*2*RTprop/max(A*_q,1)) + 1, 2, 32).
    #[test]
    fn pipeline_depth_m_star_pinned_and_monotone() {
        assert_eq!(pipeline_depth_m_star(5000.0, 0.1, 128.0), 9.0);
        assert_eq!(pipeline_depth_m_star(200.0, 0.01, 4.0), 2.0, "floor 2");
        assert_eq!(pipeline_depth_m_star(1e6, 0.1, 1.0), 32.0, "ceiling 32");
        assert_eq!(pipeline_depth_m_star(1000.0, 0.1, 0.0), 201.0f64.min(32.0));
        // A* = 50 / 100 / 200 at the presets (rate 1000, RTprop 0.1):
        // ceil(200/A*)+1 = 5 / 3 / 2 (floor)
        for (&p, want) in PRESETS.iter().zip([5.0, 3.0, 2.0]) {
            let a = span_width_a_star(1000.0, span_deadline_d(p, 0.1), 512.0);
            assert_eq!(pipeline_depth_m_star(1000.0, 0.1, a), want, "M*({p})");
            // depth is a ceil: integer steps by construction, but never
            // decreasing as A* shrinks (delta rises)
            let m = |d: f64| {
                let a = span_width_a_star(1000.0, span_deadline_d(d, 0.1), 512.0);
                pipeline_depth_m_star(1000.0, 0.1, a)
            };
            assert!(m(p * 1.02) >= m(p) && m(p) >= m(p * 0.98));
            assert!(m(p * 1.02) - m(p * 0.98) <= 1.0, "no multi-step jump at {p}");
        }
        // nudges in rate: non-decreasing
        for &r in &[200.0, 1000.0, 5000.0] {
            assert!(pipeline_depth_m_star(r * 1.02, 0.1, 64.0) >= pipeline_depth_m_star(r, 0.1, 64.0));
            assert!(pipeline_depth_m_star(r * 0.98, 0.1, 64.0) <= pipeline_depth_m_star(r, 0.1, 64.0));
        }
    }

    /// Delta = clamp(ceil(rate*max(J,0)), 1, 64).
    #[test]
    fn trailing_offset_delta_pinned_and_monotone() {
        assert_eq!(trailing_offset_delta(200.0, 0.005), 1.0);
        assert_eq!(trailing_offset_delta(5000.0, 0.003), 15.0);
        assert_eq!(trailing_offset_delta(1e6, 1.0), 64.0, "cap 64");
        assert_eq!(trailing_offset_delta(10.0, 0.0), 1.0, "FIFO floor");
        assert_eq!(trailing_offset_delta(10.0, -1.0), 1.0, "negative jitter floors");
        for &r in &[200.0, 1000.0, 5000.0] {
            let t = |x: f64| trailing_offset_delta(x, 0.003);
            assert!(t(r * 1.02) >= t(r) && t(r) >= t(r * 0.98));
            assert!(t(r * 1.02) - t(r * 0.98) <= 1.0);
        }
    }

    /// Tail target: exact at the three anchors, continuous (slope kink at
    /// Auto only) and non-increasing in delta through every preset.
    #[test]
    fn sim_tail_target_of_delta_pinned_and_continuous() {
        assert!(rel(sim_tail_target_of_delta(50.0), 1e-7) < 1e-9);
        assert!(rel(sim_tail_target_of_delta(0.5), 1e-5) < 1e-9);
        assert!(rel(sim_tail_target_of_delta(0.005), 0.05) < 1e-9);
        // held constant beyond the anchors
        assert!(rel(sim_tail_target_of_delta(1e6), 1e-7) < 1e-9);
        assert!(rel(sim_tail_target_of_delta(1e-9), 0.05) < 1e-9);
        for &p in &PRESETS {
            let (hi, mid, lo) = (
                sim_tail_target_of_delta(p * 1.02),
                sim_tail_target_of_delta(p),
                sim_tail_target_of_delta(p * 0.98),
            );
            assert!(hi <= mid && mid <= lo, "tail non-increasing in delta at {p}");
            assert!(rel(hi, mid) < 0.10 && rel(lo, mid) < 0.10, "tail continuous at {p}");
        }
        let mut prev = sim_tail_target_of_delta(100.0);
        for i in 1..200 {
            let d = 100.0 * (1e-4f64 / 100.0).powf(i as f64 / 200.0);
            let t = sim_tail_target_of_delta(d);
            assert!(t >= prev, "monotone along the continuum");
            prev = t;
        }
    }
}
