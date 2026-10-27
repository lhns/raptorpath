//! The power-seeking store law's pure core: absolute values on synthetic static
//! maps, δ continuity, the ρ hole term, the `[PCAP]` gauges. No transport.

use super::*;

const FLOOR: usize = 10;
/// Inert for the synthetic maps (N·knee at N = 8).
const CEIL: usize = 16_384;

type MapFn<'a> = &'a dyn Fn(f64) -> f64;

/// Drive the seeker on a static map `(D(cap), T(cap))` for `periods` periods,
/// every round lasting exactly its nominal length T. Returns each period's
/// report (`next` is n̄ after the period).
fn drive(sk: &mut PowerSeeker, d: MapFn, t: MapFn, periods: usize) -> Vec<PcapLine> {
    let mut now = 1_000_000u64;
    let mut hist = Vec::new();
    while hist.len() < periods {
        let len = sk.round_len_us();
        now += len;
        let cap = sk.cap_now();
        let wall = len as f64 / 1e6;
        let round = [PathRound {
            id: 0,
            // Scaled so integer rounding of the count cannot masquerade as noise.
            delivered: (d(cap) * wall).round() as u64,
            loop_median_s: Some(t(cap)),
        }];
        hist.extend(sk.on_round(now, &round, FLOOR, CEIL));
    }
    hist
}

fn nbars(lines: &[PcapLine]) -> Vec<f64> {
    lines.iter().map(|l| l.next).collect()
}

fn seeker(b: f64, n0: f64, t_s: f64) -> PowerSeeker {
    let mut sk = PowerSeeker::new(b, codel_setpoint_q(b));
    sk.seed(n0, t_s, 1_000_000, 0);
    sk
}

fn sharp(l: f64, t0: f64) -> (impl Fn(f64) -> f64, impl Fn(f64) -> f64) {
    (move |n: f64| (n / t0).min(l), move |n: f64| t0.max(n / l))
}

/// (L, T0) per cell: argmax of D^α/T on the sharp knee is L·T0 at every α.
const CELLS: [(&str, f64, f64); 4] = [
    ("c2", 9770.0, 0.0145),
    ("c1s", 97_700.0, 0.0030),
    ("c1d", 2.0 * 97_700.0, 0.0030),
    ("c8", 9770.0 + 1955.0, 0.015),
];

/// (a) The absolute argmax values the brief pins, then convergence from 4× and
/// ¼× to within ±q·n* in ≤ 60 periods at every named point of the dial.
#[test]
fn sharp_knee_converges_to_l_t0_from_4x_and_quarter() {
    let want = [("c2", 141.665), ("c1s", 293.1), ("c1d", 586.2), ("c8", 175.8)];
    for ((name, l, t0), (wn, w)) in CELLS.iter().zip(want) {
        assert_eq!(*name, wn);
        assert!((l * t0 - w).abs() < 0.1, "{name}: L·T0 = {}", l * t0);
    }
    for b in [2.0, 1.0, 0.5] {
        let q = codel_setpoint_q(b);
        for (name, l, t0) in CELLS {
            let n_star = l * t0;
            let (d, t) = sharp(l, t0);
            for start in [4.0 * n_star, n_star / 4.0] {
                let mut sk = seeker(b, start, t0);
                let h = nbars(&drive(&mut sk, &d, &t, 60));
                let fin = h[59];
                assert!(
                    (fin - n_star).abs() <= q * n_star,
                    "{name} b={b} start={start:.0}: n̄ = {fin:.1}, n* = {n_star:.1}, ±{:.1}",
                    q * n_star
                );
            }
        }
    }
}

/// The smooth receiver-tail map (c1s numbers): D = L(1 − e^{−n/(L·T0)}),
/// T = T0 + n/L.
fn smooth(l: f64, t0: f64) -> (impl Fn(f64) -> f64, impl Fn(f64) -> f64) {
    (
        move |n: f64| l * (1.0 - (-n / (l * t0)).exp()),
        move |n: f64| t0 + n / l,
    )
}

/// argmax_n D^α/T, found on a log grid and refined by golden section.
fn analytic_argmax(alpha: f64, d: MapFn, t: MapFn) -> f64 {
    let pi = |n: f64| d(n).powf(alpha) / t(n);
    let (mut lo, mut hi) = (1.0f64.ln(), 100_000.0f64.ln());
    let g = (5.0f64.sqrt() - 1.0) / 2.0;
    for _ in 0..200 {
        let (a, b) = (hi - g * (hi - lo), lo + g * (hi - lo));
        if pi(a.exp()) < pi(b.exp()) {
            lo = a;
        } else {
            hi = b;
        }
    }
    ((lo + hi) / 2.0).exp()
}

#[test]
fn smooth_receiver_tail_converges_near_the_numeric_argmax_at_alpha_2() {
    let (l, t0) = (97_700.0, 0.0030);
    let (d, t) = smooth(l, t0);
    let n_star = analytic_argmax(2.0, &d, &t);
    // Absolute: the α = 2 power point of the c1s smooth map.
    assert!((n_star - 564.0).abs() < 5.0, "n* = {n_star}");
    let q = codel_setpoint_q(2.0);
    for start in [4.0 * n_star, n_star / 4.0] {
        let mut sk = seeker(2.0, start, t0);
        let h = nbars(&drive(&mut sk, &d, &t, 60));
        assert!(
            (h[59] - n_star).abs() <= q * n_star,
            "start {start:.0}: n̄ = {:.1}, n* = {n_star:.1}",
            h[59]
        );
    }
}

/// (b) δ continuity: the converged n̄ moves continuously under ±2 % nudges of
/// b around every preset (CLAUDE.md: a behaviour step across a preset is a
/// defect even if each side is correct).
#[test]
fn converged_n_is_continuous_in_delta_through_every_preset() {
    let (l, t0) = (97_700.0, 0.0030);
    let (d, t) = smooth(l, t0);
    let converged = |b: f64| {
        let n_star = analytic_argmax(b, &d, &t);
        let mut sk = seeker(b, 2.0 * n_star, t0);
        let h = nbars(&drive(&mut sk, &d, &t, 140));
        let tail = &h[h.len() - 20..];
        (tail.iter().sum::<f64>() / tail.len() as f64, n_star)
    };
    for preset in [0.5, 1.0, 2.0] {
        let lo = converged((preset * 0.98f64).clamp(0.5, 2.0));
        let mid = converged(preset);
        let hi = converged((preset * 1.02f64).clamp(0.5, 2.0));
        for (name, a, b) in [("lo→mid", lo, mid), ("mid→hi", mid, hi)] {
            assert!(
                (a.0 - b.0).abs() <= 0.04 * b.0,
                "b={preset} {name}: converged n̄ steps {:.1} → {:.1}",
                a.0,
                b.0
            );
            assert!(
                (a.1 - b.1).abs() <= 0.03 * b.1,
                "b={preset} {name}: the argmax steps {:.1} → {:.1}",
                a.1,
                b.1
            );
        }
        // And the seeker sits on the argmax it targets, at every b. 10 %, not
        // tighter: at b = ½ the power curve is flat and the 20-period mean of
        // the limit cycle sits 6.1 % high (185.2 vs 174.5 measured).
        for p in [lo, mid, hi] {
            assert!((p.0 - p.1).abs() <= 0.10 * p.1, "b={preset}: n̄ {:.1} vs n* {:.1}", p.0, p.1);
        }
    }
}

/// (d) The hole term's absolute values (Bulk: b = 2), then continuity in ρ.
#[test]
fn hole_term_absolute_values_and_continuity_in_rho() {
    let p = HolePath { eps: 0.02, rate: 9000.0, rtprop_s: 0.0065, srtt_s: 0.0145 };
    // ρ = 1: ε·D·(9/8·srtt + srtt) = 180 · 0.0308125
    let h1 = power_hole_term(1.0, 2.0, &[p]);
    assert!((h1 - 5.54625).abs() < 1e-9, "{h1}");
    // ρ = 0.9: stall = 0.1·(2·6500 µs) + 0.9·0.0308125 = 0.02903125
    let h9 = power_hole_term(0.9, 2.0, &[p]);
    assert!((h9 - 5.225625).abs() < 1e-9, "{h9}");
    // Continuous across ρ → 1, monotone in ρ, additive over paths.
    let near = power_hole_term(1.0 - 1e-6, 2.0, &[p]);
    assert!((near - h1).abs() < 1e-4 && near <= h1, "{near} vs {h1}");
    assert!((power_hole_term(1.0, 2.0, &[p, p]) - 2.0 * h1).abs() < 1e-9);
    // No loss estimate, no hole.
    assert_eq!(power_hole_term(1.0, 2.0, &[HolePath { eps: 0.0, ..p }]), 0.0);
}

fn line() -> PcapLine {
    PcapLine {
        t_s: 12.3456,
        period: 5,
        nbar: 192.04,
        next: 198.44,
        cap_p: 201.64,
        cap_m: 182.44,
        pi_p: 123_400.0,
        pi_m: 110_000.0,
        e_raw: 0.5123,
        e_clamp: 0.5123,
        bind: 0.0,
        floor_pin: 0.0,
        knee_pin: 0.1666,
        hole: 3.24,
        t_loop_us: 14_500,
        d_p: 9770.4,
        d_m: 9700.2,
        ext: 2,
        sign: Some(0.5),
        conv: 3,
        conv_t_s: 1.2,
        out_pin: 0.0,
    }
}

/// (e) The `[PCAP]` line, byte for byte (the plan-29 parser reads these keys).
#[test]
fn pcap_line_is_byte_pinned() {
    assert_eq!(
        format_pcap_line(&line()),
        "[PCAP] t=12.35s period=5 nbar=192.0 next=198.4 cap_p=201.6 cap_m=182.4 \
         pi_p=1.2340e5 pi_m=1.1000e5 e_raw=0.512 e_clamp=0.512 bind=0.000 floor_pin=0.000 \
         knee_pin=0.167 hole=3.2 t_loop_us=14500 d_p=9770 d_m=9700 ext=2 sign=0.500 \
         conv=3 conv_t=1.20s out_pin=0.000"
    );
    let mut l = line();
    l.sign = None;
    assert!(format_pcap_line(&l).contains(" sign=- "));
    assert_eq!(
        power_liveness_line(2.0, 0.1, 10, 2048),
        "power store law ACTIVE (RWM_STORE_LAW=power): alpha=2.0000 q=0.1000 floor=10 knee=2048"
    );
}

/// One scripted period at α = 2, q = 0.1 with the wanted raw elasticity: equal
/// T, D₊/D₋ = exp(ê·ln((1.05)/(0.95))/α). Returns the report.
fn period(sk: &mut PowerSeeker, now: &mut u64, e: f64, floor: usize, ceil: usize) -> PcapLine {
    let ln_ratio = (1.05f64 / 0.95).ln();
    let wall = 1.0;
    let d_minus = 5000.0;
    let d_plus = d_minus * (e * ln_ratio / sk.alpha()).exp();
    let mut out = None;
    for (phase, d) in [(0, 1.0), (1, d_plus), (2, 1.0), (3, d_minus)] {
        *now += (wall * 1e6) as u64;
        let r = [PathRound {
            id: 0,
            delivered: (d * wall).round() as u64,
            loop_median_s: Some(0.005),
        }];
        let got = sk.on_round(*now, &r, floor, ceil);
        assert_eq!(got.is_some(), phase == 3, "only the − measure closes a period");
        out = got;
    }
    out.unwrap()
}

/// (e) Clamp-bind, floor/knee pins, sign agreement on a scripted sequence.
#[test]
fn bind_pin_and_sign_fractions_on_a_scripted_sequence() {
    let mut sk = PowerSeeker::new(2.0, 0.1);
    sk.seed(100.0, 0.01, 0, 0);
    let mut now = 0u64;
    // ê = +3 (clamped to +1: n̄ → 110), −3 (→ 100), +0.5 (→ 102.4), floor 95.
    let l1 = period(&mut sk, &mut now, 3.0, 95, 1000);
    assert!((l1.e_raw - 3.0).abs() < 0.05, "{}", l1.e_raw);
    assert!((l1.next - 110.0).abs() < 0.01, "{}", l1.next);
    assert_eq!(l1.sign, None);
    let l2 = period(&mut sk, &mut now, -3.0, 95, 1000);
    assert!((l2.next - 100.0).abs() < 0.01, "{}", l2.next);
    assert_eq!((l2.bind, l2.floor_pin, l2.knee_pin), (1.0, 0.0, 0.0));
    assert_eq!(l2.sign, Some(0.0));
    let l3 = period(&mut sk, &mut now, 0.5, 95, 1000);
    assert!((l3.bind - 2.0 / 3.0).abs() < 1e-9);
    assert!((l3.next - 100.0 * 1.1f64.powf(0.5)).abs() < 0.05, "{}", l3.next);
    assert_eq!(l3.sign, Some(0.0));
    // Descend into the floor: ê = −1 from ~104.9 → 95.4 (free), again → 95 pin.
    let _ = period(&mut sk, &mut now, -1.0, 95, 1000);
    let l5 = period(&mut sk, &mut now, -1.0, 95, 1000);
    assert_eq!(l5.next, 95.0);
    assert!((l5.floor_pin - 1.0 / 5.0).abs() < 1e-9, "{}", l5.floor_pin);
    // Climb into the ceiling: ceil 96 pins on the next +1.
    let l6 = period(&mut sk, &mut now, 1.0, 95, 96);
    assert_eq!(l6.next, 96.0);
    assert!((l6.knee_pin - 1.0 / 6.0).abs() < 1e-9, "{}", l6.knee_pin);
    assert_eq!(l6.period, 5);
}

/// A thin round (no delivered count, or no loop sample) extends the half by one
/// round without updating; the extension is gauged.
#[test]
fn thin_round_extends_the_half_and_is_gauged() {
    let mut sk = PowerSeeker::new(2.0, 0.1);
    sk.seed(100.0, 0.01, 0, 0);
    let n0 = sk.n_bar();
    let ok = |d: u64, t: Option<f64>| [PathRound { id: 0, delivered: d, loop_median_s: t }];
    assert!(sk.on_round(10_000, &ok(50, Some(0.005)), FLOOR, CEIL).is_none()); // settle
    assert!(sk.on_round(20_000, &ok(0, Some(0.005)), FLOOR, CEIL).is_none()); // D = 0
    assert!(sk.on_round(30_000, &ok(50, None), FLOOR, CEIL).is_none()); // no T
    assert!(sk.on_round(40_000, &ok(50, Some(0.005)), FLOOR, CEIL).is_none()); // + measured
    assert!(sk.on_round(50_000, &ok(50, Some(0.005)), FLOOR, CEIL).is_none()); // settle −
    let l = sk.on_round(60_000, &ok(50, Some(0.005)), FLOOR, CEIL).expect("period closes");
    assert_eq!(l.ext, 2);
    assert!(l.e_raw.abs() < 1e-9);
    assert_eq!(sk.n_bar(), n0, "ê = 0 holds n̄");
}

/// A path with no samples drops out of the T max (data availability); the
/// slowest path with samples sets T.
#[test]
fn t_is_the_max_over_paths_with_samples() {
    let mut sk = PowerSeeker::new(1.0, 0.1);
    sk.seed(100.0, 0.01, 0, 0);
    let r = |t0: Option<f64>, t1: Option<f64>| {
        [
            PathRound { id: 0, delivered: 100, loop_median_s: t0 },
            PathRound { id: 1, delivered: 100, loop_median_s: t1 },
        ]
    };
    assert!(sk.on_round(10_000, &r(None, None), FLOOR, CEIL).is_none());
    assert!(sk.on_round(20_000, &r(Some(0.004), None), FLOOR, CEIL).is_none());
    assert!(sk.on_round(30_000, &r(None, None), FLOOR, CEIL).is_none());
    let l = sk
        .on_round(40_000, &r(Some(0.004), Some(0.009)), FLOOR, CEIL)
        .expect("closes");
    assert_eq!(l.t_loop_us, 9000);
    // D = 200 sym / 10 ms; Π₋ = 20000/0.009, Π₊ = 20000/0.004 → ê < 0.
    assert!((l.pi_m - 20_000.0 / 0.009).abs() < 1e-3);
    assert!((l.pi_p - 20_000.0 / 0.004).abs() < 1e-3);
    assert!(l.e_raw > 0.0);
}

/// The running conv_period on a cap-flat→knee map: from 4×n* the climb down
/// takes ≈ ln 4/ln(1+q) periods, and conv_period lands at the end of it.
#[test]
fn conv_period_marks_the_end_of_the_descent() {
    let (l, t0) = (97_700.0, 0.0030);
    let (d, t) = sharp(l, t0);
    let mut sk = seeker(2.0, 4.0 * l * t0, t0);
    let lines = drive(&mut sk, &d, &t, 60);
    let last = lines.last().expect("periods");
    // 4× down to 1× at ×1/(1+q) per period: ≈ 14.5 periods.
    assert!((10..=25).contains(&last.conv), "conv = {}", last.conv);
    assert!(last.conv_t_s > 0.0 && last.conv_t_s <= last.t_s);
}

/// The accumulator: median of the round's loop samples, the delivered sum, and
/// a reset that keeps nothing.
#[test]
fn accumulator_drains_the_round_median_and_resets() {
    let mut a = PowerAcc::default();
    for us in [9000u32, 1000, 5000] {
        a.note_rtt(us);
    }
    a.note_delivered(7);
    a.note_delivered(5);
    let r = a.drain(3);
    assert_eq!(r, PathRound { id: 3, delivered: 12, loop_median_s: Some(0.005) });
    assert_eq!(a.drain(3), PathRound { id: 3, delivered: 0, loop_median_s: None });
}
