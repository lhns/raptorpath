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
