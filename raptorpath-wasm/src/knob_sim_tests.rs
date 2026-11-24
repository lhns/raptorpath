//! Three-knob model tests (plan docs/derivations/visualizer-three-knobs-plan.md
//! §5.4): the T = ∞ identity, its finite limit and liveness, the absolute
//! sender invariants, the consumer and continuity. Test-only; may compare
//! against ∞ (the deny test scans model code only).

use super::*;

const INF: f64 = f64::INFINITY;
const CAP: u32 = 20_000;

/// One path's cell.
#[derive(Clone, Copy)]
pub(crate) struct Cell {
    pub eps: f64,
    pub q: f64,
    pub rtt: u32,
    pub cap: u32,
}

const GOLDEN_CELLS: [(&str, Cell); 6] = [
    ("auto", Cell { eps: 0.05, q: 0.5, rtt: 50, cap: 4 }),
    ("auto", Cell { eps: 0.10, q: 0.3, rtt: 80, cap: 4 }),
    ("bulk", Cell { eps: 0.05, q: 0.5, rtt: 50, cap: 4 }),
    ("bulk", Cell { eps: 0.10, q: 0.3, rtt: 80, cap: 4 }),
    ("realtime", Cell { eps: 0.05, q: 0.5, rtt: 50, cap: 4 }),
    ("realtime", Cell { eps: 0.10, q: 0.3, rtt: 80, cap: 4 }),
];

const TWO_PATH: [Cell; 2] = [
    Cell { eps: 0.05, q: 0.5, rtt: 50, cap: 4 },
    Cell { eps: 0.10, q: 0.3, rtt: 80, cap: 4 },
];

/// The knobs of one run.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Knobs {
    pub t: f64,
    pub h: f64,
    /// λ as a multiple of the aggregate goodput C (∞ = the object).
    pub load: f64,
    pub units: u32,
    pub outage_start: f64,
    pub outage_len: f64,
}

pub(crate) const OBJECT: Knobs =
    Knobs { t: INF, h: INF, load: INF, units: 2000, outage_start: INF, outage_len: 0.0 };

/// C = Σ capᵢ·(1 − εᵢ)·1000 symbols/s (plan §4.1).
fn goodput(paths: &[Cell]) -> f64 {
    paths.iter().map(|c| c.cap as f64 * (1.0 - c.eps) * 1000.0).sum()
}

pub(crate) fn build(paths: &[Cell], hint: &str, delta: Option<f64>, k: Knobs) -> Simulation {
    let n = paths.len();
    Simulation::multipath_knobs(
        paths.iter().map(|c| c.eps).collect(),
        paths.iter().map(|c| c.q).collect(),
        paths.iter().map(|c| c.rtt).collect(),
        paths.iter().map(|c| c.cap).collect(),
        64, hint.into(), None, delta, None,
        k.t, k.h, k.load * goodput(paths), k.units,
        vec![k.outage_start; n], vec![k.outage_len; n],
    )
    .expect("valid knobs")
}

fn build_main(paths: &[Cell], hint: &str) -> Simulation {
    Simulation::multipath(
        paths.iter().map(|c| c.eps).collect(),
        paths.iter().map(|c| c.q).collect(),
        paths.iter().map(|c| c.rtt).collect(),
        paths.iter().map(|c| c.cap).collect(),
        64, hint.into(), None, None, None,
    )
}

pub(crate) fn run(s: &mut Simulation) {
    while !s.is_finished() && s.get_tick() < CAP {
        s.step();
    }
}

/// Every public accessor main's page or its gates read, as exact bits.
fn fingerprint(s: &Simulation) -> Vec<u64> {
    let mut v: Vec<f64> = vec![
        s.get_tick() as f64, s.get_total_src() as f64, s.get_total_fec() as f64,
        s.get_total_arq() as f64, s.get_total_lost() as f64, s.get_cum_decoded() as f64,
        s.get_cum_sent() as f64, s.get_cum_arrived() as f64, s.get_given_up() as f64,
        s.get_r_star(), s.get_r_star_auto(), s.get_p_upper(), s.get_estimated_loss(),
        s.get_sigma2_est(), s.get_delta_eff(), s.get_r_sat(), s.get_derived_w(),
        s.get_saturation_pressure(), s.get_reliability(), s.get_rho(),
        s.get_giveup_budget(), s.get_giveup_residual(), s.get_giveup_deadline(),
        s.get_overhead_floor(), s.get_overhead_floor_realized(), s.get_excess_overhead(),
        s.get_lat_last(), s.get_lat_avg(), s.get_lat_percentile(0.5), s.get_lat_percentile(0.99),
        s.get_jitter(), s.get_retx_buf_size() as f64, s.get_store_occupancy() as f64,
        s.get_store_occupancy_frontier() as f64, s.get_pool_stalls() as f64,
        s.get_frontier() as f64, s.get_phantom_held() as f64, s.get_phantom_avoided() as f64,
        s.get_agg_goodput(), s.get_aggregation_factor(), s.get_completion_exposure(),
    ];
    for i in 0..s.get_num_paths() {
        v.extend([
            s.get_path_sent(i) as f64, s.get_path_arrived(i) as f64, s.get_path_src(i) as f64,
            s.get_path_fec(i) as f64, s.get_path_arq(i) as f64, s.get_path_lost(i) as f64,
            s.get_path_eps_hat(i), s.get_path_goodput_est(i),
        ]);
    }
    v.iter().map(|x| x.to_bits()).collect()
}

/// §5.4 item 2: `multipath_knobs` at the inert knobs equals `multipath` on
/// every public accessor, on the six golden cells and a 2-path cell, and
/// every such run finishes before the cap (liveness).
#[test]
fn knobs_identity_at_infinity() {
    let mut cells: Vec<(&str, Vec<Cell>)> =
        GOLDEN_CELLS.iter().map(|(h, c)| (*h, vec![*c])).collect();
    cells.push(("auto", TWO_PATH.to_vec()));
    cells.push(("realtime", TWO_PATH.to_vec()));
    for (hint, paths) in cells {
        let mut a = build_main(&paths, hint);
        let mut b = build(&paths, hint, None, OBJECT);
        run(&mut a);
        run(&mut b);
        assert!(b.is_finished() && b.get_tick() < CAP, "{hint}: T = ∞ run did not finish");
        assert_eq!(fingerprint(&a), fingerprint(&b), "{hint} {} paths", paths.len());
        assert_eq!(b.get_rho(), 1.0, "the knobs-page path keeps ρ = 1 (§3.3)");
    }
}

/// The stream cells of §5.4 item 2: λ = 0.5·C at Realtime, with and without
/// a 0.3 s outage, on one path and on two.
fn stream_cells() -> Vec<(Vec<Cell>, Knobs)> {
    let stream = Knobs { load: 0.5, ..OBJECT };
    let outage = Knobs { outage_start: 0.3, outage_len: 0.3, ..stream };
    let one = vec![GOLDEN_CELLS[0].1];
    vec![
        (one.clone(), stream),
        (one, outage),
        (TWO_PATH.to_vec(), stream),
        (TWO_PATH.to_vec(), outage),
    ]
}

/// §5.4 item 2, the finite limit: T = 1e6 and H = 1e6 are bit-identical to
/// ∞ on the stream cells, and every T = ∞ run finishes before the cap.
#[test]
fn knobs_finite_limit_equals_infinity() {
    for (paths, k) in stream_cells() {
        let mut inf = build(&paths, "realtime", None, k);
        let mut big = build(&paths, "realtime", None, Knobs { t: 1e6, h: 1e6, ..k });
        run(&mut inf);
        run(&mut big);
        assert!(inf.is_finished() && inf.get_tick() < CAP, "{k:?}: T = ∞ stream did not finish");
        assert_eq!(fingerprint(&inf), fingerprint(&big), "{k:?}");
        assert_eq!(inf.get_cum_decoded(), inf.get_num_source(), "T = ∞ delivers every unit");
        assert_eq!(inf.get_intake_expired() + inf.get_intake_expired_deq(), 0);
    }
}

/// The stream really is a stream: at λ = 0.5·C the first sends follow the
/// hand-over clock (the source arm's gate executes), and the outage loses
/// every datagram sent inside its window.
#[test]
fn stream_follows_the_handover_clock_and_the_outage_executes() {
    let paths = vec![GOLDEN_CELLS[0].1];
    let k = Knobs { load: 0.5, outage_start: 0.3, outage_len: 0.3, ..OBJECT };
    let lambda = 0.5 * goodput(&paths);
    let mut s = build(&paths, "realtime", None, k);
    let mut lost_in_outage = 0u32;
    let mut sent_in_outage = 0u32;
    while !s.is_finished() && s.get_tick() < CAP {
        let before = (s.get_path_sent(0), s.get_path_lost(0));
        let now = s.get_tick() as f64 * TICK_SECS;
        s.step();
        if now >= 0.3 && now < 0.6 {
            sent_in_outage += s.get_path_sent(0) - before.0;
            lost_in_outage += s.get_path_lost(0) - before.1;
        }
    }
    assert!(sent_in_outage > 0 && lost_in_outage == sent_in_outage);
    for &(tick, kind, seq, age) in &s.payload_log {
        if kind == 0 {
            let h = s.h_seq[seq as usize];
            assert!(h <= tick as f64 * TICK_SECS + 1e-12, "sent before hand-over");
            assert!(age >= 0.0);
        }
    }
    // 2 000 units at λ take 2000/λ s to hand over: the run cannot be shorter.
    assert!(s.get_tick() as f64 * TICK_SECS >= 1999.0 / lambda);
}

/// §5.4 item 4, the intake half of L1: no source datagram leaves with an
/// age ≥ T, and the intake rules execute (overload 1.2·C at T = 0.4).
#[test]
fn l1_at_the_intake_is_absolute() {
    for (paths, load) in [(vec![GOLDEN_CELLS[4].1], 1.2), (TWO_PATH.to_vec(), 1.2)] {
        let t = 0.4;
        let mut s = build(&paths, "realtime", None, Knobs { t, load, units: 6000, ..OBJECT });
        run(&mut s);
        assert!(s.is_finished() && s.get_tick() < CAP);
        let src: Vec<_> = s.payload_log.iter().filter(|e| e.1 == 0).collect();
        assert!(!src.is_empty());
        assert!(src.iter().all(|e| e.3 < t), "a source datagram left with age >= T");
        assert!(
            s.get_intake_expired() + s.get_intake_expired_deq() > 0,
            "overload at T = 0.4 must expire units at the intake"
        );
        assert_eq!(s.get_units_out(), s.get_num_source());
    }
}

/// `validate_knobs` errors reach the constructor's caller.
#[test]
fn constructor_rejects_out_of_domain_knobs() {
    let paths = [GOLDEN_CELLS[0].1];
    let try_build = |t: f64, h: f64, lambda: f64| {
        Simulation::multipath_knobs(
            vec![paths[0].eps], vec![paths[0].q], vec![paths[0].rtt], vec![4], 64,
            "continuum".into(), None, Some(50.0), None, t, h, lambda, 2000, vec![INF], vec![0.0],
        )
    };
    assert!(try_build(f64::NAN, 0.0, INF).is_err());
    assert!(try_build(0.0, 0.0, INF).is_err());
    assert!(try_build(-0.4, 0.0, INF).is_err());
    assert!(try_build(0.4, -1.0, INF).is_err());
    assert!(try_build(0.4, f64::NAN, INF).is_err());
    assert!(try_build(0.4, 0.0, 0.0).is_err());
    assert!(try_build(0.4, 0.0, f64::NAN).is_err());
    assert!(try_build(INF, INF, INF).is_ok());
    assert!(try_build(0.005, 0.0, 100.0).is_ok());
}

/// Finite-T cells that exercise the sender laws: streams at Realtime and
/// Bulk with an outage, overload, a lossy long-RTT path, and two paths.
fn finite_t_cells() -> Vec<(&'static str, Vec<Cell>, Knobs)> {
    let one = vec![GOLDEN_CELLS[0].1];
    let lossy = vec![GOLDEN_CELLS[1].1];
    let stream = Knobs { t: 0.4, h: INF, load: 0.5, ..OBJECT };
    let outage = Knobs { outage_start: 0.3, outage_len: 0.3, ..stream };
    vec![
        ("realtime", one.clone(), stream),
        ("realtime", one.clone(), outage),
        ("bulk", one.clone(), outage),
        ("realtime", one, Knobs { load: 1.2, units: 6000, ..stream }),
        ("auto", lossy.clone(), Knobs { t: 0.2, ..stream }),
        ("bulk", lossy, Knobs { t: 0.2, load: 0.8, ..outage }),
        ("realtime", TWO_PATH.to_vec(), outage),
        ("bulk", TWO_PATH.to_vec(), Knobs { t: 0.25, ..outage }),
    ]
}

/// §5.4 item 4, L1 absolute: no payload-bearing datagram (source, resend,
/// or repair-window member) for a seq with age ≥ T; the commitment: no
/// copy below the announced floor; and the mechanisms execute.
#[test]
fn l1_and_the_commitment_are_absolute() {
    let (mut resends, mut members, mut commits, mut cuts) = (0usize, 0usize, 0usize, 0.0f64);
    for (hint, paths, k) in finite_t_cells() {
        let mut s = build(&paths, hint, None, k);
        run(&mut s);
        assert!(s.is_finished() && s.get_tick() < CAP, "{hint} {k:?}: did not finish");
        for &(tick, kind, seq, age) in &s.payload_log {
            assert!(age < k.t, "{hint} {k:?}: kind {kind} seq {seq} at tick {tick} age {age}");
        }
        assert_eq!(s.get_l1_viol(), 0);
        assert_eq!(s.get_commit_viol(), 0, "{hint} {k:?}: a copy below the floor");
        // The same invariant from the staging logs: no resend of a seq at or
        // after its commitment (F_ann passed it then and is a running max).
        let committed: std::collections::HashMap<u64, u32> =
            s.commit_log.iter().map(|&(t, q)| (q, t)).collect();
        for &(tick, kind, seq, _) in &s.payload_log {
            if kind == 1 {
                if let Some(&c) = committed.get(&seq) {
                    assert!(tick <= c, "{hint} {k:?}: seq {seq} resent at {tick} after its commitment at {c}");
                }
            }
        }
        println!("skip_below_floor {hint} {k:?}: {}", s.get_skip_below_floor());
        resends += s.payload_log.iter().filter(|e| e.1 == 1).count();
        members += s.payload_log.iter().filter(|e| e.1 == 2).count();
        commits += s.commit_log.len();
        cuts += s.get_span_cut_by_t();
    }
    assert!(resends > 0 && members > 0, "the resend and repair arms must execute");
    assert!(commits > 0, "the commitment must execute somewhere");
    assert!(cuts > 0.0, "the L1 age edge must cut a repair window somewhere");
}

/// Every handed-over unit lands in exactly one outcome class, per seq and
/// in the counters (the finish sum).
#[test]
fn every_unit_lands_in_exactly_one_outcome_class() {
    for (hint, paths, k) in finite_t_cells() {
        let mut s = build(&paths, hint, None, k);
        run(&mut s);
        let n = s.next_seq as usize;
        let (mut dec, mut pass, mut gu) = (0u32, 0u32, 0u32);
        for i in 0..n {
            let d = !s.lat_by_seq[i].is_nan();
            let p = s.committed_seq[i];
            let g = s.given_up_seqs.contains(&(i as u64));
            assert_eq!(d as u8 + p as u8 + g as u8, 1, "{hint} {k:?}: seq {i} in {d}/{p}/{g}");
            dec += d as u32;
            pass += p as u32;
            gu += g as u32;
        }
        let expired = s.get_intake_expired() + s.get_intake_expired_deq();
        assert_eq!(n as u32 + expired, s.get_num_source(), "seqs + intake expiries = units");
        assert_eq!(dec, s.get_cum_decoded());
        assert_eq!(pass, s.get_passed_undelivered());
        assert_eq!(gu, s.get_given_up());
        assert_eq!(dec + pass + gu + expired, s.get_num_source());
    }
}

/// §5.4 item 4, the brake: a committed hole's store slot is never released
/// by the sender's own decision. It is released only by a decode (3) or by
/// the receiver's floor pass plus one RTT (5), never at the commitment.
#[test]
fn the_brake_releases_slots_only_on_feedback() {
    let mut floor_releases = 0;
    for (hint, paths, k) in finite_t_cells() {
        let mut s = build(&paths, hint, None, k);
        let min_rtt = paths.iter().map(|c| c.rtt.max(2)).min().unwrap();
        run(&mut s);
        let commit_tick: std::collections::HashMap<u64, u32> =
            s.commit_log.iter().map(|&(t, q)| (q, t)).collect();
        for &(tick, seq, cause) in &s.slot_release_log {
            let c = *commit_tick.get(&seq).expect("only committed holes owe a slot");
            assert!(cause == 3 || cause == 5);
            if cause == 5 {
                assert!(tick >= c + min_rtt, "{hint} {k:?}: seq {seq} released {tick} < {c} + RTT");
                floor_releases += 1;
            }
        }
        let released: std::collections::HashSet<u64> =
            s.slot_release_log.iter().map(|e| e.1).collect();
        assert_eq!(released.len(), s.slot_release_log.len(), "a slot is released once");
    }
    assert!(floor_releases > 0, "set-point 5 must execute");
}

/// Per-seq decode tick from the test log.
fn decode_ticks(s: &Simulation) -> Vec<Option<u32>> {
    let mut v = vec![None; s.next_seq as usize];
    for &(q, t) in &s.decode_log {
        v[q as usize] = Some(t);
    }
    v
}

/// The cells for the hold: finite T (the floor passes holes) and T = ∞.
fn hold_cells() -> Vec<(&'static str, Vec<Cell>, Knobs)> {
    let mut v = finite_t_cells();
    v.push(("realtime", vec![GOLDEN_CELLS[0].1], Knobs { load: 0.5, ..OBJECT }));
    v.push(("bulk", vec![GOLDEN_CELLS[1].1], OBJECT));
    v
}

/// §5.4 item 6: H = 0 delivers every unit at its decode tick.
#[test]
fn hold_zero_delivers_at_the_decode_tick() {
    for (hint, paths, k) in hold_cells() {
        let mut s = build(&paths, hint, None, Knobs { h: 0.0, ..k });
        run(&mut s);
        for (q, t) in decode_ticks(&s).iter().enumerate() {
            if let Some(t) = t {
                assert_eq!(s.consumer.delivered_at(q), *t as f64 * TICK_SECS, "{hint} {k:?} seq {q}");
            }
        }
    }
}

/// §5.4 item 6: H = ∞ with T = ∞ is strict order: no out-of-order unit,
/// and each unit is delivered at the tick the contiguous frontier passes it.
#[test]
fn hold_infinite_at_t_infinite_is_strict_order() {
    for (hint, paths, k) in hold_cells().into_iter().filter(|c| c.2.t == INF) {
        let mut s = build(&paths, hint, None, k);
        run(&mut s);
        assert_eq!(s.get_ooo(), 0);
        let mut front = 0u32;
        for (q, t) in decode_ticks(&s).iter().enumerate() {
            front = front.max(t.expect("T = ∞ decodes every unit"));
            assert_eq!(s.consumer.delivered_at(q), front as f64 * TICK_SECS, "{hint} seq {q}");
        }
        assert_eq!(s.get_rel_by_hold() + s.get_rel_by_floor(), 0);
    }
}

/// §5.4 item 6: the consumer is output only (the sender run is the same at
/// every H); each unit's delivery time is non-decreasing in H; a 1–2 %
/// nudge of H = 0.02 moves a delivery by at most the nudge plus one tick;
/// and rel_by_hold + rel_by_floor = released holes.
#[test]
fn delivery_is_monotone_and_continuous_in_the_hold() {
    let grid = [0.0, 0.005, 0.01, 0.02 * 0.98, 0.02, 0.02 * 1.02, 0.05, 0.1, 0.4, INF];
    let mut released = 0;
    for (hint, paths, k) in hold_cells() {
        let runs: Vec<Simulation> = grid
            .iter()
            .map(|&h| {
                let mut s = build(&paths, hint, None, Knobs { h, ..k });
                run(&mut s);
                s
            })
            .collect();
        let n = runs[0].next_seq as usize;
        let at = |s: &Simulation, q: usize| {
            let t = s.consumer.delivered_at(q);
            if t.is_nan() { INF } else { t }
        };
        for (i, s) in runs.iter().enumerate() {
            assert_eq!(fingerprint(s), fingerprint(&runs[0]), "{hint} {k:?}: H moved the sender");
            let marked = s.get_ooo_units().iter().filter(|&&m| m == 1).count() as u32;
            assert_eq!(marked, s.get_ooo(), "{hint} {k:?}: per-unit ooo marker = count");
            assert_eq!(
                s.get_rel_by_hold() + s.get_rel_by_floor(),
                s.get_released_holes(),
                "{hint} {k:?} H = {}", grid[i]
            );
            released += s.get_released_holes();
            if i > 0 {
                for q in 0..n {
                    assert!(at(&runs[i - 1], q) <= at(s, q), "{hint} {k:?} seq {q}: H {} -> {}", grid[i - 1], grid[i]);
                }
            }
        }
        for (a, b) in [(3usize, 4usize), (4, 5)] {
            let dh = grid[b] - grid[a];
            for q in 0..n {
                let (ta, tb) = (at(&runs[a], q), at(&runs[b], q));
                if ta < INF {
                    assert!(tb - ta <= dh + TICK_SECS + 1e-12, "{hint} {k:?} seq {q}: {ta} -> {tb}");
                }
            }
        }
    }
    assert!(released > 0, "holes must be released somewhere");
}

/// Liveness of the consumer at the end of a finite-T run at H = ∞: the
/// sender's finish (every unit resolved, the commitment included) does not
/// leave decoded units held behind a hole the floor never reached.
#[test]
fn no_decoded_unit_is_left_at_the_consumer_at_the_end() {
    for (hint, paths, k) in finite_t_cells() {
        let mut s = build(&paths, hint, None, k);
        run(&mut s);
        let decoded = decode_ticks(&s).iter().filter(|t| t.is_some()).count();
        let delivered = (0..s.next_seq as usize).filter(|&q| !s.consumer.delivered_at(q).is_nan()).count();
        println!(
            "end-of-run {hint} {k:?}: decoded {decoded}, consumer-delivered {delivered}, f_ann {} f_rx {} a_known {} next {} passed {} floors {}",
            s.get_f_ann(), s.get_f_rx(), s.get_a_known(), s.next_seq, s.get_passed_undelivered(), s.get_floors()
        );
        assert_eq!(decoded, delivered, "{hint} {k:?}: decoded units stuck at the consumer");
    }
}

// --- §5.4 item 5: continuity in T at the finite named points --------------

const NUDGES: [f64; 5] = [0.98, 0.99, 1.0, 1.01, 1.02];
const SLOPE_PTS: [f64; 4] = [0.90, 0.95, 1.05, 1.10];

/// (share_T, share_any) of one run.
fn shares(paths: &[Cell], hint: &str, delta: Option<f64>, k: Knobs) -> (f64, f64) {
    let mut s = build(paths, hint, delta, k);
    run(&mut s);
    assert!(s.is_finished() && s.get_tick() < CAP, "{k:?}: did not finish");
    (s.get_share_t(), s.get_share_any())
}

/// The derived per-1 % tolerance (plan §5.4 item 5): 1.5 × the larger
/// measured side slope × 0.01·T + one unit's quantum.
fn tolerance(side: [f64; 4], t0: f64, units: u32) -> f64 {
    let left = (side[1] - side[0]).abs() / (0.05 * t0);
    let right = (side[3] - side[2]).abs() / (0.05 * t0);
    1.5 * left.max(right) * 0.01 * t0 + 1.0 / units as f64
}

/// Neighbouring pairs of the nudge grid within the tolerance.
fn within_bound(series: &[f64; 5], tol: f64) -> Result<(), String> {
    for i in 1..5 {
        let d = (series[i] - series[i - 1]).abs();
        if d > tol {
            return Err(format!(
                "step {d} > tol {tol} between nudges {} and {}",
                NUDGES[i - 1], NUDGES[i]
            ));
        }
    }
    Ok(())
}

/// The resend arrival ages t_k = k·RTT + ô + 1 tick (retry timer = path
/// RTT, ô = the capacity-weighted RTT/2): the nearest to `t0`, for the report.
fn nearest_tk(paths: &[Cell], t0: f64) -> (f64, f64) {
    let o = paths.iter().map(|c| c.cap as f64 * c.rtt as f64).sum::<f64>()
        / paths.iter().map(|c| c.cap as f64).sum::<f64>()
        / 2000.0;
    let mut best = (INF, INF);
    for c in paths {
        for k in 0..200 {
            let tk = k as f64 * c.rtt as f64 / 1000.0 + o + TICK_SECS;
            if (tk - t0).abs() < best.1 {
                best = (tk, (tk - t0).abs());
            }
        }
    }
    best
}

/// Continuity of share_T and share_any through T0·{0.98..1.02} at one
/// cell; returns the share_T series and its tolerance.
pub(crate) fn t_nudge_within_bound(
    paths: &[Cell], hint: &str, delta: Option<f64>, base: Knobs, t0: f64,
) -> ([f64; 5], f64) {
    let side: Vec<(f64, f64)> =
        SLOPE_PTS.iter().map(|m| shares(paths, hint, delta, Knobs { t: t0 * m, ..base })).collect();
    let grid: Vec<(f64, f64)> =
        NUDGES.iter().map(|m| shares(paths, hint, delta, Knobs { t: t0 * m, ..base })).collect();
    let mut out = ([0.0; 5], 0.0);
    for ri in 0..2usize {
        let name = ["share_T", "share_any"][ri];
        let pick = |v: &(f64, f64)| if ri == 0 { v.0 } else { v.1 };
        let sd = [pick(&side[0]), pick(&side[1]), pick(&side[2]), pick(&side[3])];
        let series = [pick(&grid[0]), pick(&grid[1]), pick(&grid[2]), pick(&grid[3]), pick(&grid[4])];
        let tol = tolerance(sd, t0, base.units);
        let (tk, dist) = nearest_tk(paths, t0);
        println!(
            "continuity {hint} N={} T0={t0} {name}: {series:?} tol {tol:.5}; nearest t_k {tk:.4} s ({:.1} ms away)",
            paths.len(),
            dist * 1000.0
        );
        if let Err(e) = within_bound(&series, tol) {
            panic!("{hint} N={} T0={t0} {name}: {e}", paths.len());
        }
        if ri == 0 {
            out = (series, tol);
        }
    }
    out
}

/// §5.4 item 5 at both finite named points, one path and two, with the
/// injected positive control: a planted step of 5 × tol in share_T at the
/// centre point must fail the same bound.
#[test]
fn share_is_continuous_in_t_at_the_named_points() {
    let stream = Knobs { h: 0.0, load: 0.5, ..OBJECT };
    // Without an outage these cells deliver everything within 0.4 s (no
    // power, reported); a 0.5 s outage makes T bind.
    let binding = Knobs { outage_start: 0.3, outage_len: 0.5, ..stream };
    for paths in [vec![GOLDEN_CELLS[0].1], vec![GOLDEN_CELLS[1].1], TWO_PATH.to_vec()] {
        for (base, t0) in [(stream, 0.4), (stream, 2.0), (binding, 0.4), (binding, 2.0)] {
            let (series, tol) = t_nudge_within_bound(&paths, "realtime", None, base, t0);
            let mut planted = series;
            planted[2] += 5.0 * tol;
            assert!(within_bound(&planted, tol).is_err(), "the planted step must fail the bound");
        }
    }
}
// --- §5.5: the knob golden rows (proposal branch) -------------------------

/// Deliberate model change: three-knob rows, proposal branch, law revision
/// 17 (Revision 3). Re-capture once per law revision, in its own commit,
/// only after the identity gates pass without capture:
/// `GOLDEN_CAPTURE=1 cargo test -p raptorpath-wasm test_knobs_golden -- --nocapture`.
/// The six main goldens never re-capture on this branch.
#[test]
fn test_knobs_golden() {
    let stream = Knobs { t: 0.4, load: 0.5, ..OBJECT };
    let outage = Knobs { outage_start: 0.3, outage_len: 0.3, ..stream };
    // knobs -> [ticks, src, fec, arq, lost, decoded, sent, arrived],
    //          [share_any, share_T] as f64 bits, [ooo, intake_expired + deq]
    let rows: [(Knobs, [u32; 8], [u64; 2], [u32; 2]); 5] = [
        (Knobs { h: 0.0, ..stream },
         [1054, 2000, 686, 0, 104, 2000, 2686, 2548], [0x3ff0000000000000, 0x3ff0000000000000], [84, 0]),
        (Knobs { h: INF, ..stream },
         [1054, 2000, 686, 0, 104, 2000, 2686, 2548], [0x3ff0000000000000, 0x3ff0000000000000], [0, 0]),
        (Knobs { h: 0.0, ..outage },
         [1146, 2000, 1075, 1051, 296, 2000, 4126, 2850], [0x3ff0000000000000, 0x3ff0000000000000], [245, 0]),
        (Knobs { h: INF, ..outage },
         [1146, 2000, 1075, 1051, 296, 2000, 4126, 2850], [0x3ff0000000000000, 0x3ff0000000000000], [0, 0]),
        (Knobs { h: 0.0, load: 1.2, units: 6000, ..stream },
         [1641, 5068, 1493, 0, 270, 5068, 6561, 6221], [0x3feb078263ab596e, 0x3feb078263ab596e], [232, 932]),
    ];
    let capture = std::env::var("GOLDEN_CAPTURE").is_ok();
    for (k, g8, gs, g2) in rows {
        let mut s = build(&[GOLDEN_CELLS[4].1], "realtime", None, k);
        run(&mut s);
        let got8 = [
            s.get_tick(), s.get_total_src(), s.get_total_fec(), s.get_total_arq(),
            s.get_total_lost(), s.get_cum_decoded(), s.get_cum_sent(), s.get_cum_arrived(),
        ];
        let gots = [s.get_share_any().to_bits(), s.get_share_t().to_bits()];
        let got2 = [s.get_ooo(), s.get_intake_expired() + s.get_intake_expired_deq()];
        println!(
            "knob golden {k:?}: {got8:?}, [{:#x}, {:#x}], {got2:?}  (shares {} {})",
            gots[0], gots[1], s.get_share_any(), s.get_share_t()
        );
        if !capture {
            assert_eq!((got8, gots, got2), (g8, gs, g2), "knob golden diverged: {k:?}");
        }
    }
}

/// Report (not a gate; `--ignored`): the bind gauges at every named point
/// (plan §4.2) under the three workloads, with a 0.3 s outage at 0.3 s.
#[test]
#[ignore]
fn report_bind_gauges_at_the_named_points() {
    let points: [(&str, f64, f64, f64); 6] = [
        ("Bulk", INF, 0.005, INF),
        ("Auto", INF, 0.5, INF),
        ("Realtime object", INF, 50.0, 0.0),
        ("Realtime call", 0.4, 50.0, 0.0),
        ("Object API", INF, 0.5, 0.0),
        ("Buffered video", 2.0, 0.005, 0.0),
    ];
    for (name, t, d, h) in points {
        for (wl, load, units) in [("object", INF, 2000), ("stream", 0.5, 2000), ("overload", 1.2, 6000)] {
            let k = Knobs { t, h, load, units, outage_start: 0.3, outage_len: 0.3 };
            let mut s = build(&[GOLDEN_CELLS[0].1], "continuum", Some(d), k);
            run(&mut s);
            println!(
                "gauges {name:16} {wl:8}: ticks {:5} share_any {:.4} share_T {:.4} by_ref {:.4} \
                 span_cut_by_T {:.4} rel_by_hold {} rel_by_floor {} expired {} deq {} floors {} \
                 passed {} ooo {} p50 {:.3} p99 {:.3} rec {:.3} stall+ {:.3}",
                s.get_tick(), s.get_share_any(), s.get_share_t(), s.get_share_by_ref(),
                s.get_span_cut_by_t(), s.get_rel_by_hold(), s.get_rel_by_floor(),
                s.get_intake_expired(), s.get_intake_expired_deq(), s.get_floors(),
                s.get_passed_undelivered(), s.get_ooo(), s.get_delay_quantile(0.5),
                s.get_delay_quantile(0.99), s.get_recovery_time(), s.get_strict_stall_beyond_outage()
            );
        }
    }
}

/// share_T counts delivered units only: at T = ∞ before any delivery it
/// reads 0, not 1 (an undelivered unit's delay is ∞ and ∞ ≤ ∞).
#[test]
fn share_t_counts_only_delivered_units() {
    let mut s = build(&[GOLDEN_CELLS[0].1], "realtime", None, OBJECT);
    assert_eq!(s.get_share_t(), 0.0);
    for _ in 0..5 {
        s.step();
    }
    assert!(s.get_share_t() <= s.get_share_any());
    run(&mut s);
    assert_eq!(s.get_share_t(), s.get_share_any());
}

// --- Addendum 1 A1 (P9): one class per unit, and the event feed ------------

/// The hold cells at H ∈ {0, 0.02, ∞}.
fn class_cells() -> Vec<(&'static str, Vec<Cell>, Knobs)> {
    let mut v = Vec::new();
    for (hint, paths, k) in hold_cells() {
        for h in [0.0, 0.02, INF] {
            v.push((hint, paths.clone(), Knobs { h, ..k }));
        }
    }
    v
}

/// Run to the end draining `take_unit_events` every `every` ticks, as the
/// page does per frame; returns the per-unit event history.
fn run_with_events(s: &mut Simulation, every: u32) -> Vec<Vec<(f64, f64, u8)>> {
    let mut hist = vec![Vec::new(); s.get_num_source() as usize];
    let drain = |s: &mut Simulation, hist: &mut Vec<Vec<(f64, f64, u8)>>| {
        let ev = s.take_unit_events();
        assert_eq!(ev.len() % 4, 0, "events are quadruples");
        for e in ev.chunks(4) {
            hist[e[0] as usize].push((e[1], e[2], e[3] as u8));
        }
    };
    while !s.is_finished() && s.get_tick() < CAP {
        s.step();
        if s.get_tick() % every == 0 {
            drain(s, &mut hist);
        }
    }
    drain(s, &mut hist);
    hist
}

/// A1's class re-derived independently from the consumer's delivery log (in
/// delivery order) and the unit order.
fn derive_classes(s: &Simulation) -> Vec<u8> {
    let n_seq = s.next_seq as usize;
    let mut at = vec![f64::NAN; n_seq];
    let mut ooo = vec![false; n_seq];
    let mut max_seen: Option<usize> = None;
    for &(q, t) in &s.consumer.deliver_log {
        assert!(at[q].is_nan(), "seq {q} delivered twice");
        at[q] = t;
        ooo[q] = max_seen.is_some_and(|m| m > q);
        max_seen = Some(max_seen.map_or(q, |m| m.max(q)));
    }
    let o = s.one_way_s();
    (0..s.get_num_source() as usize)
        .map(|u| {
            let Some(q) = (0..n_seq).find(|&q| s.seq_unit[q] as usize == u) else {
                return if (u as u32) < s.get_units_out() { CLASS_EXPIRED } else { CLASS_PENDING };
            };
            if at[q].is_nan() {
                let passed = s.committed_seq[q] || s.given_up_seqs.contains(&(q as u64));
                return if passed { CLASS_UNDELIVERED } else { CLASS_PENDING };
            }
            if ooo[q] {
                CLASS_OUT_OF_ORDER
            } else if at[q] - s.h_seq[q] + o <= s.get_lifetime_s() {
                CLASS_ON_TIME
            } else {
                CLASS_LATE
            }
        })
        .collect()
}

/// A1 native gates: the class is a function of the consumer's delivery ticks
/// (re-derived here); every unit gets exactly one class (none pending at the
/// end of a finished run); the events, concatenated over a run, equal the
/// final arrays, with at most one superseding event per unit, and only from
/// `undelivered`.
#[test]
fn unit_classes_follow_the_consumer_and_the_events_add_up() {
    let mut superseded = 0usize;
    let mut seen = [0u32; N_CLASSES];
    for (hint, paths, k) in class_cells() {
        let mut s = build(&paths, hint, None, k);
        let hist = run_with_events(&mut s, 37);
        assert!(s.is_finished() && s.get_tick() < CAP, "{hint} {k:?}");
        let classes = s.get_unit_classes();
        assert_eq!(classes, derive_classes(&s), "{hint} {k:?}: class != re-derived");
        for &(q, t) in &s.consumer.deliver_log {
            assert_eq!(t.to_bits(), s.consumer.delivered_at(q).to_bits());
        }
        let counts = s.get_class_counts();
        assert_eq!(counts.iter().sum::<u32>(), s.get_num_source());
        assert_eq!(counts[CLASS_PENDING as usize], 0, "{hint} {k:?}: a finished run leaves no unit pending");
        let delays = s.get_unit_delays();
        let marks = s.get_ooo_units();
        for (u, ev) in hist.iter().enumerate() {
            let c = classes[u];
            assert_eq!(marks[u] == 1, c == CLASS_OUT_OF_ORDER, "{hint} {k:?} unit {u}: ooo marker");
            seen[c as usize] += 1;
            let Some(&(h, d, last)) = ev.last() else {
                assert_eq!(c, CLASS_PENDING, "{hint} {k:?} unit {u}: no event for class {c}");
                continue;
            };
            assert_eq!(last, c, "{hint} {k:?} unit {u}: last event != final class");
            assert_eq!(h.to_bits(), s.handover_s(u as u32).to_bits());
            assert_eq!(d.to_bits(), delays[u].to_bits(), "{hint} {k:?} unit {u}: event delay");
            assert!(ev.len() <= 2, "{hint} {k:?} unit {u}: {} events", ev.len());
            if ev.len() == 2 {
                assert_eq!(ev[0].2, CLASS_UNDELIVERED, "{hint} {k:?} unit {u}: only undelivered is superseded");
                superseded += 1;
            }
        }
        // The shares and the overlap count read the same classes.
        let delivered = counts[1] + counts[2] + counts[3];
        let ooo_late = s.get_ooo_late();
        assert!(ooo_late <= counts[CLASS_OUT_OF_ORDER as usize]);
        let late = counts[CLASS_LATE as usize] + ooo_late;
        assert_eq!(s.get_late_share(), late as f64 / delivered.max(1) as f64);
        assert_eq!(counts[CLASS_OUT_OF_ORDER as usize], s.get_ooo());
        assert!(s.take_unit_events().is_empty(), "nothing left after the final drain");
    }
    println!("A1 classes over the cells (pending, on time, late, ooo, undelivered, expired): {seen:?}; superseded {superseded}");
    for c in [CLASS_ON_TIME, CLASS_LATE, CLASS_OUT_OF_ORDER, CLASS_UNDELIVERED, CLASS_EXPIRED] {
        assert!(seen[c as usize] > 0, "class {c} never reached: the gate would be vacuous for it");
    }
}

/// A1: no out-of-order unit at H = ∞ with T = ∞, and at H = 0 the
/// out-of-order units are exactly the late fills of released holes.
#[test]
fn out_of_order_is_zero_when_strict_and_the_late_fills_when_open() {
    let mut fills = 0;
    for (hint, paths, k) in class_cells() {
        let mut s = build(&paths, hint, None, k);
        run(&mut s);
        let ooo = s.get_class_counts()[CLASS_OUT_OF_ORDER as usize];
        if k.t == INF && k.h == INF {
            assert_eq!(ooo, 0, "{hint} {k:?}");
        }
        if k.h == 0.0 {
            assert_eq!(ooo, s.get_late_fills(), "{hint} {k:?}");
            fills += s.get_late_fills();
        }
    }
    assert!(fills > 0, "H = 0 must see late fills somewhere");
}

// --- Addendum 1 A2 (P10): the forward leg, extracted -------------------------

/// A run with a forward leg per path and a resend limit.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_link(
    paths: &[Cell], hint: &str, delta: Option<f64>, k: Knobs,
    sv: &[f64], q: &[f64], d: &[f64], limit: f64,
) -> Simulation {
    let n = paths.len();
    Simulation::multipath_link(
        paths.iter().map(|c| c.eps).collect(),
        paths.iter().map(|c| c.q).collect(),
        paths.iter().map(|c| c.rtt).collect(),
        paths.iter().map(|c| c.cap).collect(),
        64, hint.into(), None, delta, None,
        k.t, k.h, k.load * goodput(paths), k.units,
        vec![k.outage_start; n], vec![k.outage_len; n],
        sv.to_vec(), q.to_vec(), d.to_vec(), limit,
    )
    .expect("valid link")
}

/// A2: with the default leg every datagram is delivered inline at its send
/// tick (nothing is ever in flight at a tick's end), the server is never busy
/// past a send (free_p <= now), and every resend's knowledge lead is its
/// path's RTT in ticks.
#[test]
fn the_default_leg_delivers_inline_at_the_send_tick() {
    let mut cells: Vec<(&str, Vec<Cell>, Knobs)> =
        GOLDEN_CELLS.iter().map(|(h, c)| (*h, vec![*c], OBJECT)).collect();
    cells.extend(finite_t_cells());
    cells.push(("auto", TWO_PATH.to_vec(), OBJECT));
    let mut kinds = [0usize; 4];
    for (hint, paths, k) in cells {
        let mut s = build(&paths, hint, None, k);
        while !s.is_finished() && s.get_tick() < CAP {
            s.step();
            assert_eq!(s.get_in_flight(), 0, "{hint} {k:?}: in flight at the end of tick {}", s.get_tick());
        }
        assert_eq!(s.free_after_now, 0, "{hint} {k:?}: free_p > now at a send");
        for &(sent, at, _, kind, _) in &s.leg_log {
            assert_eq!(sent, at, "{hint} {k:?}: kind {kind} sent {sent} delivered {at}");
            kinds[kind as usize] += 1;
        }
        for &(seq, p, lead) in &s.lead_log {
            assert_eq!(lead, paths[p].rtt.max(2), "{hint} {k:?} seq {seq}");
        }
        assert_eq!(s.get_dup(), 0);
        store_is_accounted(&s, hint);
        ack_is_on_its_way(&s, hint);
    }
    assert!(kinds.iter().all(|&n| n > 0), "every datagram kind must be exercised: {kinds:?}");
}

/// A2: the decoder does not depend on arrival order. A run's decoder feeds,
/// replayed into a fresh decoder in their order and in three permutations,
/// recover the same set as the run.
#[test]
fn the_decoder_is_order_independent() {
    let cells = [("auto", vec![GOLDEN_CELLS[1].1]), ("realtime", TWO_PATH.to_vec())];
    for (hint, paths) in cells {
        let mut s = build(&paths, hint, None, OBJECT);
        run(&mut s);
        let mut want: Vec<u64> = s.decode_log.iter().map(|e| e.0).collect();
        want.sort_unstable();
        let replay = |order: &[usize]| {
            let mut dec = math::RlcDecoder::new(8);
            let mut got = Vec::new();
            for &i in order {
                let (a, count, index, ref data) = s.feed_log[i];
                got.extend(if count == u16::MAX {
                    dec.feed_source(a, data)
                } else {
                    dec.feed_repair(a, count, index, data)
                });
            }
            got.sort_unstable();
            got.dedup();
            got
        };
        let n = s.feed_log.len();
        let ident: Vec<usize> = (0..n).collect();
        assert_eq!(replay(&ident), want, "{hint}: the in-order replay");
        let mut rng = 0x2545f4914f6cdd1du64;
        for _ in 0..3 {
            let mut perm = ident.clone();
            for i in (1..n).rev() {
                let j = (xorshift64(&mut rng) * (i + 1) as f64) as usize;
                perm.swap(i, j.min(i));
            }
            assert_eq!(replay(&perm), want, "{hint}: a permuted replay recovered another set");
        }
    }
}

/// The store holds exactly the source slots whose SACK is still on its way,
/// the open (lost, unresolved) holes and the committed holes owed a slot: no
/// slot leaks and none is released twice.
fn store_is_accounted(s: &Simulation, label: &str) {
    let sack: usize = s.paths.iter().map(|p| p.feedback_queue.iter().filter(|e| e.1 && e.2).count()).sum();
    let open = s.symbols.iter().filter(|x| x.lost && !x.recovered).count();
    let owed = s.slot_owed.iter().filter(|&&o| o).count();
    assert_eq!(s.get_store_occupancy() as usize, sack + open + owed, "{label}: store {} != sack {sack} + open {open} + owed {owed}", s.get_store_occupancy());
}

/// A_known cannot stall: every seq is acked, or its ack is queued (set-point
/// 1 or 5), or it is an open hole still being recovered.
fn ack_is_on_its_way(s: &Simulation, label: &str) {
    let mut queued = std::collections::HashSet::new();
    for q in s.ack_due.iter().chain(s.floor_ack_due.iter()) {
        queued.extend(q.iter().map(|e| e.1 as u64));
    }
    let open: std::collections::HashSet<u64> =
        s.symbols.iter().filter(|x| x.lost && !x.recovered).map(|x| x.seq).collect();
    for q in 0..s.next_seq as u64 {
        assert!(s.acked_seq[q as usize] || queued.contains(&q) || open.contains(&q), "{label}: seq {q} can never be acked");
    }
}

/// The checks every leg run must pass: it finishes, each seq is decoded and
/// handed to the consumer at most once, the classes and outcome counts add
/// up, nothing is left in flight and the store is accounted for.
fn leg_run_is_sound(s: &Simulation, label: &str) {
    assert!(s.is_finished() && s.get_tick() < CAP, "{label}: did not finish");
    let mut seen = std::collections::HashSet::new();
    for &(q, _) in &s.decode_log {
        assert!(seen.insert(q), "{label}: seq {q} decoded twice");
    }
    let mut seen = std::collections::HashSet::new();
    for &(q, _) in &s.consumer.deliver_log {
        assert!(seen.insert(q), "{label}: seq {q} delivered twice");
    }
    let counts = s.get_class_counts();
    assert_eq!(counts[CLASS_PENDING as usize], 0, "{label}: units left pending");
    let expired = s.get_intake_expired() + s.get_intake_expired_deq();
    assert_eq!(
        s.get_cum_decoded() + s.get_passed_undelivered() + s.get_given_up() + expired,
        s.get_num_source(),
        "{label}: outcome classes"
    );
    assert_eq!(s.get_in_flight(), 0);
    store_is_accounted(s, label);
    ack_is_on_its_way(s, label);
}

/// A2's duplicate rule, driven by a test-only forward delay longer than the
/// retry timer (a second copy leaves while the first is in flight), and at
/// N = 2 by a propagation split (an original on the slow path arrives after
/// a repair on the fast one decoded it). A duplicate counts and does nothing
/// else: no second decode, delivery or class, no slot leak, A_known reaches
/// the end at T = inf.
#[test]
fn a_duplicate_counts_and_does_nothing_else() {
    let mut dups = 0;
    // one path, 60 ms extra forward delay on a 50 ms RTT
    for hint in ["bulk", "auto"] {
        let mut s = build(&[GOLDEN_CELLS[0].1], hint, None, OBJECT);
        s.paths[0].test_extra_s = 0.06;
        run(&mut s);
        leg_run_is_sound(&s, hint);
        assert_eq!(s.get_cum_decoded(), s.get_num_source());
        ack_is_on_its_way(&s, hint);
        dups += s.get_dup();
    }
    // two paths, the slow path's propagation at its full rtt/2
    for hint in ["auto", "realtime"] {
        let mut s = build_link(&TWO_PATH, hint, None, OBJECT, &[0.0; 2], &[INF; 2], &[0.0, 0.04], INF);
        run(&mut s);
        leg_run_is_sound(&s, hint);
        assert_eq!(s.get_cum_decoded(), s.get_num_source());
        ack_is_on_its_way(&s, hint);
        dups += s.get_dup();
    }
    assert!(dups > 0, "the duplicate rule must execute");
    println!("duplicates: {dups}");
}

/// A2's late fill of a committed seq: a copy (or a decode) that arrives
/// after the floor passed its hole moves the unit from passed-undelivered to
/// delivered exactly once, re-classes it at most once (a drain between the
/// commitment and the fill sees `undelivered` first), and its store slot is
/// released once.
#[test]
fn a_late_fill_of_a_committed_seq_moves_the_unit_once() {
    let (mut late_filled, mut superseded) = (0, 0);
    for (hint, paths, k) in finite_t_cells() {
        for extra in [0.02, 0.05] {
            let mut s = build(&paths, hint, None, k);
            for p in &mut s.paths {
                p.test_extra_s = extra;
            }
            let hist = run_with_events(&mut s, 1);
            let label = format!("{hint} {k:?} extra {extra}");
            leg_run_is_sound(&s, &label);
            let committed: std::collections::HashMap<u64, u32> =
                s.commit_log.iter().map(|&(t, q)| (q, t)).collect();
            for &(q, t) in &s.decode_log {
                if let Some(&c) = committed.get(&q) {
                    assert!(t >= c, "{label}: seq {q}");
                    late_filled += 1;
                    let u = s.seq_unit[q as usize] as usize;
                    let ev = &hist[u];
                    let last = ev.last().expect("an event").2;
                    assert!([CLASS_ON_TIME, CLASS_LATE, CLASS_OUT_OF_ORDER].contains(&last), "{label}: unit {u} {ev:?}");
                    assert!(ev.len() == 1 || (ev.len() == 2 && ev[0].2 == CLASS_UNDELIVERED), "{label}: unit {u} {ev:?}");
                    superseded += (ev.len() == 2) as u32;
                }
            }
            let released: std::collections::HashSet<u64> =
                s.slot_release_log.iter().map(|e| e.1).collect();
            assert_eq!(released.len(), s.slot_release_log.len(), "{label}: a slot released twice");
            assert_eq!(s.get_l1_viol(), 0, "{label}");
        }
    }
    assert!(late_filled > 0 && superseded > 0, "the late-fill rule must execute");
    println!("late fills of committed seqs: {late_filled}, superseding events {superseded}");
}

/// A2's knowledge lead, bounded: for every resend the sender learns of at
/// its arrival, the lead is its path's reverse leg, the first whole tick
/// count >= rtt - d (so <= rtt - d + 1 tick, and exactly rtt - d when d lies
/// on the tick grid: the model's reverse leg has no queue).
#[test]
fn the_knowledge_lead_is_the_reverse_leg() {
    let lossy = [GOLDEN_CELLS[1].1];
    let cases: Vec<(&[Cell], Vec<f64>)> = vec![
        (&lossy, vec![0.0]),
        (&lossy, vec![0.010]),
        (&lossy, vec![0.0125]),
        (&lossy, vec![0.040]),
        (&TWO_PATH, vec![0.010, 0.0333]),
    ];
    let mut n = 0;
    for (paths, d) in cases {
        let np = paths.len();
        for hint in ["bulk", "realtime"] {
            let mut s = build_link(paths, hint, None, OBJECT, &vec![0.0; np], &vec![INF; np], &d, INF);
            run(&mut s);
            leg_run_is_sound(&s, hint);
            for &(seq, p, lead) in &s.lead_log {
                let rtt = paths[p].rtt.max(2) as f64 * TICK_SECS;
                let rev = rtt - d[p];
                let lead_s = lead as f64 * TICK_SECS;
                assert_eq!(lead, s.paths[p].reverse_ticks, "seq {seq}");
                assert!(lead_s >= rev - 1e-12 && lead_s - TICK_SECS < rev, "seq {seq}: lead {lead_s} rtt - d {rev}");
                if (d[p] * 1000.0).fract() == 0.0 {
                    assert!((lead_s - rev).abs() < 1e-12, "seq {seq}: on the grid the lead is rtt - d exactly");
                }
                n += 1;
            }
            let max = s.lead_log.iter().map(|e| e.2).max().unwrap_or(0) as f64 * TICK_SECS * 1000.0;
            assert_eq!(s.get_knowledge_lead_max_ms(), max);
        }
    }
    assert!(n > 0, "resends must arrive");
    println!("knowledge lead: {n} resends checked");
}

/// The link's domain reaches the constructor's caller.
#[test]
fn the_link_constructor_rejects_out_of_domain_values() {
    let c = [GOLDEN_CELLS[0].1];
    let try_link = |sv: f64, q: f64, d: f64, lim: f64| {
        Simulation::multipath_link(
            vec![c[0].eps], vec![c[0].q], vec![c[0].rtt], vec![4], 64, "continuum".into(),
            None, Some(50.0), None, INF, INF, INF, 2000, vec![INF], vec![0.0],
            vec![sv], vec![q], vec![d], lim,
        )
    };
    assert!(try_link(0.0, INF, 0.0, INF).is_ok());
    assert!(try_link(0.001, 0.1, 0.025, 10.0).is_ok());
    assert!(try_link(-0.001, INF, 0.0, INF).is_err());
    assert!(try_link(f64::NAN, INF, 0.0, INF).is_err());
    assert!(try_link(0.0, 0.0, 0.0, INF).is_err());
    assert!(try_link(0.0, INF, 0.026, INF).is_err());
    assert!(try_link(0.0, INF, 0.0, -1.0).is_err());
    assert!(try_link(0.0, INF, 0.0, f64::NAN).is_err());
    let s = try_link(0.002, 0.1, 0.01, 10.0).unwrap();
    let got = (s.get_link_service_s(0), s.get_link_queue_s(0), s.get_link_prop_s(0), s.get_resend_limit());
    assert_eq!(got, (0.002, 0.1, 0.01, 10.0));
}
