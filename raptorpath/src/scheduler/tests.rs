use super::*;

fn make_symbol(id: u32, repair: bool) -> WireSymbol {
    WireSymbol {
        block_id: 0,
        payload_id: id,
        is_repair: repair,
        data: vec![0u8; 64].into(),
        backend: FecBackend::Rlc,
    }
}

/// `active_paths()` and `live_paths()` are not interchangeable: the
/// active set additionally filters on spare capacity, so a saturated
/// path (in_flight ≥ cwnd) is live but not active. Every sender phase
/// that aggregates over paths picks one of the two deliberately, and
/// swapping them silently changes the law — the CC pace rate uses
/// `live_paths()` precisely because the active filter dropped a
/// saturated path out of the aggregate (`net/mod.rs`, cc-rate refresh),
/// while the M* RTprop / in-flight-cap / tail-sweep phases use
/// `active_paths()`.
///
/// A fixture of only fresh paths cannot catch a path-set swap: there
/// `in_flight = 0 < cwnd` makes the two sets identical. Assert the
/// divergence itself, at the only state that exhibits it.
#[test]
fn saturated_path_is_live_but_not_active() {
    let mut sched = Scheduler::new(Arc::new(WallClock));
    sched.add_path(0);
    sched.add_path(1);

    // Fresh paths: the trap. Both sets agree, so nothing here can
    // distinguish them — this is exactly the fixture that gave a false
    // pass, asserted so the trap cannot be re-entered unnoticed.
    assert_eq!(
        sched.active_paths(),
        sched.live_paths(),
        "fresh paths make active/live indistinguishable — a fixture of \
         only-fresh paths CANNOT test the distinction"
    );

    // Saturate path 1: up, but no spare capacity.
    {
        let p = sched.path_mut(1).unwrap();
        assert!(p.active, "path must be up for the distinction to bite");
        p.in_flight = p.cwnd;
        assert_eq!(p.available(), 0);
    }

    // Both accessors iterate a HashMap, so the returned order is
    // arbitrary and re-seeded per process — sort before comparing, or
    // this asserts the hasher instead of the path sets.
    assert_eq!(sched.active_paths(), vec![0], "saturated path 1 is NOT active");
    let mut live = sched.live_paths();
    live.sort_unstable();
    assert_eq!(
        live,
        vec![0, 1],
        "saturated path 1 IS live — control traffic and the CC rate \
         aggregate must still see it"
    );

    // And the other direction: a down path with spare capacity is in
    // neither set, so `live_paths()` is not merely "all paths".
    sched.path_mut(0).unwrap().active = false;
    assert!(sched.active_paths().is_empty());
    assert_eq!(sched.live_paths(), vec![1]);
}

#[test]
fn test_best_source_path_picks_lowest_rtt() {
    let mut sched = Scheduler::new(Arc::new(WallClock));
    sched.add_path(0);
    sched.add_path(1);

    sched
        .path_mut(0)
        .unwrap()
        .estimator
        .record_rtt(std::time::Duration::from_millis(100));
    sched
        .path_mut(1)
        .unwrap()
        .estimator
        .record_rtt(std::time::Duration::from_millis(10));

    assert_eq!(sched.best_source_path(), Some(1));
}

#[test]
fn test_best_repair_path_picks_highest_goodput() {
    let mut sched = Scheduler::new(Arc::new(WallClock));
    sched.add_path(0);
    sched.add_path(1);

    // Path 0: low throughput
    sched.path_mut(0).unwrap().estimator.record_batch(10, 9);
    sched.path_mut(0).unwrap().estimator.record_throughput(100.0);

    // Path 1: high throughput
    sched.path_mut(1).unwrap().estimator.record_batch(10, 9);
    sched.path_mut(1).unwrap().estimator.record_throughput(1000.0);

    assert_eq!(sched.best_repair_path(), Some(1));
}

#[test]
fn test_redundant_source_path_picks_different_path() {
    let mut sched = Scheduler::new(Arc::new(WallClock));
    sched.add_path(0);
    sched.add_path(1);
    sched.add_path(2);

    sched
        .path_mut(0)
        .unwrap()
        .estimator
        .record_rtt(std::time::Duration::from_millis(5));
    sched
        .path_mut(1)
        .unwrap()
        .estimator
        .record_rtt(std::time::Duration::from_millis(20));
    sched
        .path_mut(2)
        .unwrap()
        .estimator
        .record_rtt(std::time::Duration::from_millis(50));

    // Primary is 0, redundant should be 1 (second-lowest RTT)
    let redundant = sched.redundant_source_path(0);
    assert_eq!(redundant, Some(1));
}

#[test]
fn test_redundant_source_path_none_with_single_path() {
    let mut sched = Scheduler::new(Arc::new(WallClock));
    sched.add_path(0);

    assert_eq!(sched.redundant_source_path(0), None);
}

#[test]
fn test_best_source_path_skips_full_cwnd() {
    let mut sched = Scheduler::new(Arc::new(WallClock));
    sched.add_path(0);
    sched.add_path(1);

    sched
        .path_mut(0)
        .unwrap()
        .estimator
        .record_rtt(std::time::Duration::from_millis(5));
    sched
        .path_mut(1)
        .unwrap()
        .estimator
        .record_rtt(std::time::Duration::from_millis(50));

    // Fill path 0's cwnd
    let cwnd = sched.path(0).unwrap().cwnd;
    sched.path_mut(0).unwrap().in_flight = cwnd;

    // Should pick path 1 since path 0 has no capacity
    assert_eq!(sched.best_source_path(), Some(1));
}

#[test]
fn test_schedule_prefers_low_rtt_for_source() {
    let mut sched = Scheduler::new(Arc::new(WallClock));
    sched.add_path(0);
    sched.add_path(1);

    // Path 0: high RTT
    sched
        .path_mut(0)
        .unwrap()
        .estimator
        .record_rtt(std::time::Duration::from_millis(100));
    // Path 1: low RTT
    sched
        .path_mut(1)
        .unwrap()
        .estimator
        .record_rtt(std::time::Duration::from_millis(10));

    let source: Vec<_> = (0..5).map(|i| make_symbol(i, false)).collect();
    let result = sched.schedule(source, vec![]);

    // Path 1 (lower RTT) should get symbols first
    let path1_count = result
        .iter()
        .find(|(id, _)| *id == 1)
        .map(|(_, s)| s.len())
        .unwrap_or(0);

    assert!(path1_count > 0, "Low-RTT path should receive source symbols");
}

#[test]
fn test_best_repair_path_avoiding_picks_alternative() {
    let mut sched = Scheduler::new(Arc::new(WallClock));
    sched.add_path(0);
    sched.add_path(1);

    // Path 0: highest goodput
    sched.path_mut(0).unwrap().estimator.record_batch(10, 9);
    sched.path_mut(0).unwrap().estimator.record_throughput(1000.0);

    // Path 1: lower goodput
    sched.path_mut(1).unwrap().estimator.record_batch(10, 9);
    sched.path_mut(1).unwrap().estimator.record_throughput(500.0);

    // Avoiding path 0 should pick path 1
    assert_eq!(sched.best_repair_path_avoiding(0), Some(1));
    // Avoiding path 1 should pick path 0
    assert_eq!(sched.best_repair_path_avoiding(1), Some(0));
}

#[test]
fn test_best_repair_path_avoiding_falls_back_single_path() {
    let mut sched = Scheduler::new(Arc::new(WallClock));
    sched.add_path(0);

    // With only one path, avoiding it should still return it
    assert_eq!(sched.best_repair_path_avoiding(0), Some(0));
}

// -----------------------------------------------------------------------
// Effective delivery time tests
// -----------------------------------------------------------------------

#[test]
fn test_effective_delivery_time() {
    let mut sched = Scheduler::new(Arc::new(WallClock));
    sched.add_path(0);

    let path = sched.path_mut(0).unwrap();
    // Record multiple RTT samples so EWMA converges
    for _ in 0..20 {
        path.estimator.record_rtt(Duration::from_millis(100));
    }
    // Record some loss: 10 sent, 9 received → ~10% loss
    for _ in 0..20 {
        path.estimator.record_batch(10, 9);
    }

    let e = path.effective_delivery_time();
    let rtt = path.estimator.rtt().as_secs_f64();
    let eps = path.estimator.loss_rate();
    let expected = rtt / 2.0 + eps * rtt;
    assert!((e - expected).abs() < 0.001, "E_i={e}, expected={expected}, rtt={rtt}, eps={eps}");
}

#[test]
fn test_correction_rate() {
    let mut sched = Scheduler::new(Arc::new(WallClock));
    sched.add_path(0);

    let path = sched.path_mut(0).unwrap();
    // Record loss to get ~10% loss rate
    for _ in 0..20 {
        path.estimator.record_batch(10, 9);
    }
    let eps = path.estimator.loss_rate();
    let r = path.correction_rate();
    let expected = eps / (1.0 - eps);
    assert!((r - expected).abs() < 0.001, "r={r}, expected={expected}");
}

// -----------------------------------------------------------------------
// Interpolated objective tests (paper §5.7)
// -----------------------------------------------------------------------

#[test]
fn test_realtime_prefers_low_latency_over_low_loss() {
    let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Realtime);
    sched.add_path(0);
    sched.add_path(1);

    // Path 0: low RTT (10ms), high loss (20%)
    sched.path_mut(0).unwrap().estimator.record_rtt(Duration::from_millis(10));
    for _ in 0..20 {
        sched.path_mut(0).unwrap().estimator.record_batch(10, 8);
    }

    // Path 1: high RTT (200ms), low loss (1%)
    sched.path_mut(1).unwrap().estimator.record_rtt(Duration::from_millis(200));
    for _ in 0..20 {
        sched.path_mut(1).unwrap().estimator.record_batch(100, 99);
    }

    // Realtime (w_lat=1, w_bw=0): should prefer path 0 (lower E_i despite higher loss)
    assert_eq!(sched.best_source_path(), Some(0));
}

#[test]
fn test_bulk_prefers_low_overhead() {
    let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
    sched.add_path(0);
    sched.add_path(1);

    // Path 0: low RTT (10ms), high loss (20%) → high r
    sched.path_mut(0).unwrap().estimator.record_rtt(Duration::from_millis(10));
    for _ in 0..20 {
        sched.path_mut(0).unwrap().estimator.record_batch(10, 8);
    }

    // Path 1: high RTT (200ms), low loss (1%) → low r
    sched.path_mut(1).unwrap().estimator.record_rtt(Duration::from_millis(200));
    for _ in 0..20 {
        sched.path_mut(1).unwrap().estimator.record_batch(100, 99);
    }

    // Bulk (w_lat=0, w_bw=1): should prefer path 1 (lower correction rate)
    assert_eq!(sched.best_source_path(), Some(1));
}

#[test]
fn test_schedule_uses_objective_weights() {
    // With Realtime hint, source should go to low-latency path even if it has more loss
    let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Realtime);
    sched.add_path(0);
    sched.add_path(1);

    // Path 0: fast, lossy
    sched.path_mut(0).unwrap().estimator.record_rtt(Duration::from_millis(10));
    for _ in 0..20 {
        sched.path_mut(0).unwrap().estimator.record_batch(10, 8);
    }

    // Path 1: slow, clean
    sched.path_mut(1).unwrap().estimator.record_rtt(Duration::from_millis(200));
    for _ in 0..20 {
        sched.path_mut(1).unwrap().estimator.record_batch(100, 99);
    }

    let source: Vec<_> = (0..5).map(|i| make_symbol(i, false)).collect();
    let result = sched.schedule(source, vec![]);

    let path0_count = result
        .iter()
        .find(|(id, _)| *id == 0)
        .map(|(_, s)| s.len())
        .unwrap_or(0);

    assert!(path0_count > 0, "Realtime should send source on fast path");
}

// -----------------------------------------------------------------------
// Copa-lite congestion control (paper §8.2)
// -----------------------------------------------------------------------

fn millis(ms: u64) -> Duration {
    Duration::from_millis(ms)
}

#[test]
fn test_copa_lite_cwnd_never_below_floor() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    // Establish a 10ms propagation floor.
    for _ in 0..3 {
        sched.path_mut(0).unwrap().record_rtt_sample(millis(10));
    }

    // Hammer with inflated-RTT windows (delay backoffs) ...
    for _ in 0..50 {
        sched.path_mut(0).unwrap().record_rtt_sample(millis(100));
        clock.advance(millis(150));
        sched.ack(0, 4);
        assert!(
            sched.path(0).unwrap().cwnd >= PathState::MIN_CWND,
            "delay backoffs must never take cwnd below the floor"
        );
    }
    // ... and with decode failures (loss steps).
    for _ in 0..100 {
        sched.on_loss(0, false);
    }
    let cwnd = sched.path(0).unwrap().cwnd;
    assert_eq!(cwnd, PathState::MIN_CWND);
    assert!(cwnd >= 8, "floor is 8 symbols, never the historical 2");
}

#[test]
fn test_burst_rtt_spike_does_not_collapse_cwnd() {
    // The failure mode this guards: the initial burst inflates its own RTT
    // samples, dq explodes, and a rate-formula target collapses cwnd
    // to the floor. With the windowed-min filter remembering the
    // propagation floor, a burst costs one gentle ×0.92 backoff.
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    // Learn the 10ms floor and ramp for a few clean RTTs.
    for _ in 0..6 {
        sched.path_mut(0).unwrap().record_rtt_sample(millis(10));
        clock.advance(millis(15));
        sched.ack(0, 8);
    }
    let pre_burst = sched.path(0).unwrap().cwnd;
    assert!(
        pre_burst > PathState::INITIAL_CWND,
        "ramp should have grown cwnd, got {pre_burst}"
    );

    // A burst inflates a full update window of RTT samples 4x.
    for _ in 0..4 {
        sched.path_mut(0).unwrap().record_rtt_sample(millis(40));
    }
    clock.advance(millis(50));
    sched.ack(0, 8);

    let post_burst = sched.path(0).unwrap().cwnd;
    let one_backoff = (pre_burst as f64 * BACKOFF_MULT) as u32;
    assert!(
        post_burst + 1 >= one_backoff,
        "burst must cost at most one gentle backoff: pre={pre_burst}, post={post_burst}"
    );
    assert!(
        post_burst > 2 * PathState::MIN_CWND,
        "burst must not collapse cwnd toward the floor: post={post_burst}"
    );

    // After the burst drains, samples return to the floor and cwnd
    // recovers additively (+2 per update).
    sched.path_mut(0).unwrap().record_rtt_sample(millis(10));
    clock.advance(millis(50));
    sched.ack(0, 8);
    let recovered = sched.path(0).unwrap().cwnd;
    assert_eq!(
        recovered,
        post_burst + ADDITIVE_STEP as u32,
        "post-backoff growth is additive"
    );
}

#[test]
fn test_ramp_multiplicative_until_backoff_then_additive() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    // Clean RTTs at the floor: each per-SRTT update multiplies ×1.5+1.
    let mut prev = sched.path(0).unwrap().cwnd;
    for _ in 0..4 {
        sched.path_mut(0).unwrap().record_rtt_sample(millis(20));
        clock.advance(millis(30));
        sched.ack(0, prev);
        let cur = sched.path(0).unwrap().cwnd;
        assert_eq!(
            cur,
            (prev as f64 * RAMP_GAIN + 1.0).round() as u32,
            "ramp phase is multiplicative"
        );
        assert!(sched.path(0).unwrap().in_slow_start);
        prev = cur;
    }

    // First backoff: inflated window ends the ramp.
    sched.path_mut(0).unwrap().record_rtt_sample(millis(80));
    clock.advance(millis(50));
    sched.ack(0, prev);
    let after_backoff = sched.path(0).unwrap().cwnd;
    assert_eq!(after_backoff, (prev as f64 * BACKOFF_MULT).round() as u32);
    assert!(!sched.path(0).unwrap().in_slow_start);

    // Subsequent clean updates are additive +2 — never multiplicative.
    let mut prev = after_backoff;
    for _ in 0..3 {
        sched.path_mut(0).unwrap().record_rtt_sample(millis(20));
        clock.advance(millis(50));
        sched.ack(0, prev);
        let cur = sched.path(0).unwrap().cwnd;
        assert_eq!(cur, prev + ADDITIVE_STEP as u32, "steady state is additive");
        prev = cur;
    }
}

#[test]
fn test_hint_changes_backoff_threshold() {
    // The protocol hint sets the queue target (paper §8.2).
    // floor = 100ms, windowed min = 118ms → dq = 18ms. The 100→118
    // step also charges the jitter estimator (18/8 = 2.25ms decaying
    // to ~1.72ms over the three samples → ~3.4ms threshold widening):
    //   Realtime target  8ms + 3.4ms → backoff
    //   Auto target   12.5ms + 3.4ms → backoff
    //   Bulk target     25ms + 3.4ms → keep growing
    fn run(hint: ProtocolHint) -> (u32, u32) {
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new_with_hint(clock.clone(), hint);
        sched.add_path(0);
        for _ in 0..3 {
            sched.path_mut(0).unwrap().record_rtt_sample(millis(100));
            clock.advance(millis(150));
            sched.ack(0, 8);
        }
        let pre = sched.path(0).unwrap().cwnd;
        for _ in 0..3 {
            sched.path_mut(0).unwrap().record_rtt_sample(millis(118));
        }
        clock.advance(millis(150));
        sched.ack(0, 8);
        (pre, sched.path(0).unwrap().cwnd)
    }

    let (rt_pre, rt_post) = run(ProtocolHint::Realtime);
    let (auto_pre, auto_post) = run(ProtocolHint::Auto);
    let (bulk_pre, bulk_post) = run(ProtocolHint::Bulk);

    assert!(rt_post < rt_pre, "Realtime backs off at dq=18ms: {rt_pre}->{rt_post}");
    assert!(auto_post < auto_pre, "Auto backs off at dq=18ms: {auto_pre}->{auto_post}");
    assert!(bulk_post > bulk_pre, "Bulk tolerates dq=18ms: {bulk_pre}->{bulk_post}");
}

#[test]
fn test_jitter_widens_backoff_threshold_c2() {
    // Jitter-adjusted queue target. C2-like link: 10ms floor with ±6ms
    // RTT jitter (netem 3ms/direction). Bulk's raw queue-target threshold
    // is 2.5ms — smaller than the jitter — so a plain windowed-min signal
    // reads jitter as a standing queue and backs off nearly every update,
    // pinning cwnd at the floor. With the k×jitter_est widening, a
    // jittery-but-queue-free link must ramp.
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new_with_hint(clock.clone(), ProtocolHint::Bulk);
    sched.add_path(0);

    // Deterministic jitter pattern with min 10ms, spread 6ms — every
    // update window's min sample sits 2-4ms above the 10s floor once
    // the floor has seen a 10ms sample.
    let pattern_ms = [10u64, 14, 12, 16, 13, 15, 12, 14];
    let mut cwnd_track = Vec::new();
    for round in 0..40 {
        // 4 ACK batches per SRTT window, one RTT sample each; skip
        // the true-floor sample in most windows (the windowed min
        // usually does not reach the floor — that is the trap).
        for k in 0..4 {
            let idx = (round * 4 + k) % pattern_ms.len();
            let ms = if round == 0 && k == 0 { 10 } else { pattern_ms[idx].max(12) };
            sched.path_mut(0).unwrap().record_rtt_sample(millis(ms));
        }
        clock.advance(millis(15));
        sched.ack(0, 8);
        cwnd_track.push(sched.path(0).unwrap().cwnd);
    }
    let final_cwnd = *cwnd_track.last().unwrap();
    assert!(
        final_cwnd > 100,
        "jittery queue-free C2 link must ramp past 100 symbols, got {final_cwnd} (track: {cwnd_track:?})"
    );

    // Sanity: a genuine standing queue on the same jittery link still
    // triggers backoff within a few updates — the queue shifts every
    // sample up by 12ms, while the consecutive-difference jitter
    // estimate stays at jitter scale.
    let before = sched.path(0).unwrap().cwnd;
    let mut backed_off = false;
    for round in 0..6 {
        for k in 0..4 {
            let idx = (round * 4 + k) % pattern_ms.len();
            sched
                .path_mut(0)
                .unwrap()
                .record_rtt_sample(millis(pattern_ms[idx] + 12));
        }
        clock.advance(millis(25));
        sched.ack(0, 8);
        if sched.path(0).unwrap().cwnd < before {
            backed_off = true;
            break;
        }
    }
    assert!(backed_off, "a genuine 12ms standing queue must still back off");
}

#[test]
fn test_hint_plumbed_to_paths() {
    let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
    sched.add_path(0);
    assert_eq!(sched.path(0).unwrap().copa.queue_mult, 1.25);

    sched.set_protocol_hint(ProtocolHint::Realtime);
    assert_eq!(sched.path(0).unwrap().copa.queue_mult, 1.08);

    // New paths pick up the current hint.
    sched.add_path(1);
    assert_eq!(sched.path(1).unwrap().copa.queue_mult, 1.08);
}

/// Paper §8.2 / §11.3 row 3: off the wire signal, Copa's price is the constant
/// δ = 0.5 at EVERY hint (the paper's hint->δ map {50, 0.5, 0.005} applies only
/// when the wire gate is on). Pins the constant; the queue-multiplier table is
/// pinned by `test_hint_plumbed_to_paths`.
#[test]
fn copa_delta_is_half_at_every_hint_off_the_wire_signal() {
    use crate::scheduler::copa::{copa_delta_for_hint, COPA_DELTA};
    assert!(
        !crate::scheduler::copa_wire_active(),
        "precondition: the wire signal is off in the test environment (RWM_COPA_WIRE/RWM_QUIC_CC unset)"
    );
    assert_eq!(COPA_DELTA, 0.5);
    for hint in [ProtocolHint::Realtime, ProtocolHint::Auto, ProtocolHint::Bulk] {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), hint);
        sched.add_path(0);
        assert_eq!(
            sched.path(0).unwrap().copa.delta,
            0.5,
            "{hint:?}: Copa delta is the constant 0.5 off-wire (paper §8.2 maps it per hint; this pins the divergence)"
        );
    }
    // The hint map itself (the wire-on value) is {50, 0.5, 0.005}, as the paper says.
    // With no RWM_COPA_DELTA/RWM_DELTA override it is COPA_DELTA / tail_loss_scale.
    for (hint, want) in [
        (ProtocolHint::Realtime, 50.0),
        (ProtocolHint::Auto, 0.5),
        (ProtocolHint::Bulk, 0.005),
    ] {
        let got = copa_delta_for_hint(hint);
        assert!((got - want).abs() < 1e-12, "{hint:?}: wire-on delta {got}, paper {want}");
    }
}

#[test]
fn test_schedule_ack_roundtrip_conserves_in_flight() {
    // The in_flight budget is charged once, at schedule time. A double
    // charge (schedule() and then the paced drain at send time) leaks +1
    // per symbol, jams the TUN gate shut and throttles throughput to the
    // leak-guard decay.
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock);
    sched.add_path(0);

    let source: Vec<_> = (0..8).map(|i| make_symbol(i, false)).collect();
    let assignments = sched.schedule(source, vec![]);
    let scheduled: u32 = assignments.iter().map(|(_, s)| s.len() as u32).sum();
    assert_eq!(scheduled, 8);
    assert_eq!(sched.path(0).unwrap().in_flight, 8);


    // ACK feedback releases everything: budget conserved, gate opens.
    sched.ack(0, 8);
    assert_eq!(
        sched.path(0).unwrap().in_flight,
        0,
        "schedule → send → ack must conserve the in_flight budget"
    );
}

#[test]
fn test_in_flight_expiry_releases_stranded_budget() {
    // ACKs are best-effort datagrams: a lost ACK strands its release
    // forever. The time-based expiry (max(4×SRTT, 250ms)) must reopen
    // the gate at RTT timescale without any feedback at all.
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    let path = sched.path_mut(0).unwrap();
    for _ in 0..4 {
        path.record_rtt_sample(millis(10)); // srtt 10ms → horizon 250ms
    }
    path.charge_in_flight(56);
    assert_eq!(path.in_flight, 56);

    // Well before the horizon: nothing expires.
    clock.advance(millis(100));
    let path = sched.path_mut(0).unwrap();
    path.expire_in_flight();
    assert_eq!(path.in_flight, 56);

    // A partial ACK releases FIFO; the stranded remainder expires
    // once the horizon passes.
    path.release_in_flight(50);
    assert_eq!(path.in_flight, 6);
    clock.advance(millis(200)); // total 300ms > 250ms horizon
    let path = sched.path_mut(0).unwrap();
    path.expire_in_flight();
    assert_eq!(
        path.in_flight, 0,
        "stranded budget must expire at RTT timescale, not the 2s guard"
    );
}

#[test]
fn test_c2_loop_cwnd_grows_past_200_within_5s() {
    // Full C2 loop at the scheduler level (100 Mbit / 10ms RTT / Bulk),
    // mirroring the production wiring: schedule-time budget charge,
    // token-paced sends stamped at wire time (echo-timestamp RTT
    // therefore excludes pacing-queue delay — verified hypothesis:
    // batches are built at send time from the carry), per-datagram
    // ACKs with ~1.3% of them lost (stranding releases), and
    // time-based expiry. cwnd must ramp past 200 symbols within 5
    // simulated seconds and the sender must be ACK-clocked, not
    // leak-guard throttled.
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new_with_hint(clock.clone(), ProtocolHint::Bulk);
    sched.add_path(0);

    const OWD: Duration = Duration::from_millis(5); // 10ms RTT
    let mut carry: u32 = 0; // interleaver + pacing carry (already charged)
    // (ack_arrival, symbols, wire_send_instant)
    let mut acks: VecDeque<(Instant, u32, Instant)> = VecDeque::new();
    let mut wire_counter: u64 = 0;
    let mut total_sent: u64 = 0;

    for _tick in 0..5000 {
        let now = clock.now();

        // Encoder + TUN gate: schedule one 56-symbol block (64KB /
        // 1200B) whenever the committed budget is under cwnd.
        {
            let p = sched.path_mut(0).unwrap();
            p.expire_in_flight();
            if p.in_flight < p.cwnd {
                p.charge_in_flight(56);
                carry += 56;
            }
        }

        // Send the whole carry; the batch timestamp is stamped here
        // (wire time), as in send_interleaved_batches.
        {
            let budget = carry;
            if budget > 0 {
                carry -= budget;
                total_sent += budget as u64;
                // Receiver ACKs each datagram after one RTT; ~1.3% of
                // ACK datagrams are lost (their releases stranded).
                let mut acked = 0;
                for _ in 0..budget {
                    wire_counter += 1;
                    if wire_counter % 77 != 0 {
                        acked += 1;
                    }
                }
                if acked > 0 {
                    acks.push_back((now + OWD * 2, acked, now));
                }
            }
        }

        // Deliver due ACKs: RTT = now − echoed wire timestamp.
        while acks.front().is_some_and(|(t, _, _)| *t <= now) {
            let (_, n, sent_at) = acks.pop_front().unwrap();
            let rtt = now.duration_since(sent_at);
            let p = sched.path_mut(0).unwrap();
            p.record_rtt_sample(rtt);
            p.release_in_flight(n);
            p.on_ack(n);
        }

        clock.advance(millis(1));
    }

    let cwnd = sched.path(0).unwrap().cwnd;
    assert!(
        cwnd > 200,
        "C2 loop must ramp cwnd past 200 symbols within 5s, got {cwnd}"
    );
    // Ack-clocked throughput, not the leak-guard trickle (a jammed gate
    // moves ~450 symbols in 15s; here 5s must move far more).
    assert!(
        total_sent > 20_000,
        "sender must be ack-clocked, not gate-starved: sent {total_sent}"
    );
}

#[test]
fn test_low_floor_clamp_no_spurious_backoff() {
    // LAN-class floor (200us): the backoff threshold clamps at 0.1ms
    // and dq clamps at the same 0.1ms, so sub-clamp jitter (raw dq
    // 80us) can never back off — while a genuine standing queue
    // (raw dq 200us > clamp) still does.
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    for round in 0..10 {
        let sample = if round % 2 == 0 { 200 } else { 280 };
        let p = sched.path_mut(0).unwrap();
        p.record_rtt_sample(Duration::from_micros(sample));
        clock.advance(millis(1)); // >> sub-ms SRTT: update every round
        let before = sched.path(0).unwrap().cwnd;
        sched.ack(0, 8);
        let after = sched.path(0).unwrap().cwnd;
        assert!(
            after >= before,
            "jitter below the dq clamp must not back off (round {round}): {before} -> {after}"
        );
    }
    assert!(
        sched.path(0).unwrap().cwnd > PathState::INITIAL_CWND,
        "LAN ramp should have grown"
    );

    // Sanity: a real standing queue above the clamp does back off.
    let p = sched.path_mut(0).unwrap();
    p.record_rtt_sample(Duration::from_micros(400)); // raw dq 200us
    clock.advance(millis(1));
    let before = sched.path(0).unwrap().cwnd;
    sched.ack(0, 8);
    assert!(
        sched.path(0).unwrap().cwnd < before,
        "genuine LAN queue must still back off"
    );
}

// ===================================================================
// Per-symbol placement law (paper §5.7). The cost is
//   in_flight/cwnd + w_lat·(E_prop/ref_srtt) + w_bw·r + w_div·fate,
// sampled as P(i) ∝ exp(−cost/T).
// ===================================================================

/// Look up a path's probability in a place_probs distribution.
fn prob_of(dist: &[(PathId, f64)], id: PathId) -> f64 {
    dist.iter().find(|(p, _)| *p == id).map(|(_, w)| *w).unwrap_or(0.0)
}

fn set_rtt(sched: &mut Scheduler, id: PathId, ms: u64) {
    // The estimator RTT is an EWMA (α = 0.125) seeded at 50 ms; feed enough
    // samples to converge so tests exercise the intended RTT, not a warm-up
    // blend of it and the seed.
    let p = sched.path_mut(id).unwrap();
    for _ in 0..60 {
        p.estimator.record_rtt(std::time::Duration::from_millis(ms));
    }
}

/// (a) Idle 2-path → placement concentrates on the cheapest (lowest-RTT)
/// path. Softmax "concentrate" = the vast majority of the mass.
#[test]
fn place_idle_concentrates_on_cheapest() {
    let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
    sched.add_path(0);
    sched.add_path(1);
    set_rtt(&mut sched, 0, 10);
    set_rtt(&mut sched, 1, 50); // path 1 is 5× slower
    // both idle (in_flight = 0)
    let dist = sched.place_probs(false, &[]);
    let p0 = prob_of(&dist, 0);
    let p1 = prob_of(&dist, 1);
    assert!(p0 > 0.95, "cheapest path must take the mass, got p0={p0}");
    assert!(p0 > p1);
}

/// (b) As the chosen path's in_flight rises, placement shifts continuously
/// to the other path — no threshold jump. Assert strict monotonic shift.
#[test]
fn place_shifts_monotonically_with_load() {
    let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
    sched.add_path(0);
    sched.add_path(1);
    set_rtt(&mut sched, 0, 10);
    set_rtt(&mut sched, 1, 10); // symmetric: isolate the load term
    let cwnd = sched.path(0).unwrap().cwnd; // 10

    let mut prev_p0 = f64::INFINITY;
    // Sweep in_flight from empty to 2× cwnd (into overdraft) — the path is
    // never removed from the distribution (no capacity filter), so the
    // shift is continuous through saturation, not a jump at cwnd.
    for infl in 0..=(2 * cwnd) {
        sched.path_mut(0).unwrap().in_flight = infl;
        let dist = sched.place_probs(false, &[]);
        let p0 = prob_of(&dist, 0);
        let p1 = prob_of(&dist, 1);
        assert!(
            p0 < prev_p0,
            "p0 must strictly decrease as path-0 load rises: infl={infl} p0={p0} prev={prev_p0}"
        );
        // p1 is its complement (two paths) → strictly increasing.
        assert!((p0 + p1 - 1.0).abs() < 1e-9);
        prev_p0 = p0;
    }
    // Ended favouring the unloaded path.
    assert!(prev_p0 < 0.1, "heavily loaded path should be largely abandoned");
}

/// (c) Water-filling equilibrium: the fixed point of marginal-cost
/// equalisation is `in_flight/cwnd` equal across paths, i.e. in_flight ∝
/// cwnd ∝ capacity. At that stock ratio placement is balanced (both paths
/// used equally) — the signature that the law fills proportional to
/// capacity rather than concentrating.
#[test]
fn place_backlog_waterfills_proportional_to_capacity() {
    let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
    sched.add_path(0);
    sched.add_path(1);
    set_rtt(&mut sched, 0, 10);
    set_rtt(&mut sched, 1, 10);
    // Path 0 has 2× the capacity of path 1.
    sched.path_mut(0).unwrap().cwnd = 20;
    sched.path_mut(1).unwrap().cwnd = 10;
    // Equilibrium stock: in_flight ∝ cwnd ⇒ equal fill fraction 0.4.
    sched.path_mut(0).unwrap().in_flight = 8;
    sched.path_mut(1).unwrap().in_flight = 4;
    let dist = sched.place_probs(false, &[]);
    let p0 = prob_of(&dist, 0);
    let p1 = prob_of(&dist, 1);
    assert!(p0 > 0.1 && p1 > 0.1, "both paths used at equilibrium: p0={p0} p1={p1}");
    assert!((p0 - p1).abs() < 0.05, "balanced at the capacity-proportional fixed point");

    // And off-equilibrium (equal stock, unequal capacity) the law pushes
    // more toward the higher-capacity (lower-fill) path.
    sched.path_mut(0).unwrap().in_flight = 6;
    sched.path_mut(1).unwrap().in_flight = 6;
    let dist2 = sched.place_probs(false, &[]);
    assert!(prob_of(&dist2, 0) > prob_of(&dist2, 1));
}

/// Cross-path repair placement (RWM_XPATH_REPAIR).
/// When the fast path is source-saturated (spare≈0) and the slow path is
/// underutilized (high spare), `place_repair_spare_path` routes repair to the
/// slow path — so proactive repair rides the spare path instead of displacing
/// fast-path source. Symmetric spare → uniform split (no concentration).
#[test]
fn place_repair_spare_routes_to_underutilized_path() {
    let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
    sched.add_path(0); // fast
    sched.add_path(1); // slow
    set_rtt(&mut sched, 0, 10);
    set_rtt(&mut sched, 1, 40);
    // Fast path saturated with source: in_flight == cwnd ⇒ spare == 0.
    // Slow path lightly loaded: spare high.
    sched.path_mut(0).unwrap().cwnd = 40;
    sched.path_mut(0).unwrap().in_flight = 40; // spare 0
    sched.path_mut(1).unwrap().cwnd = 16;
    sched.path_mut(1).unwrap().in_flight = 4; // spare 3.0
    // Every repair should ride the slow (spare) path 1.
    let mut to_slow = 0;
    for _ in 0..200 {
        if sched.place_repair_spare_path() == Some(1) {
            to_slow += 1;
        }
    }
    assert_eq!(to_slow, 200, "repair must ride the spare slow path, got {to_slow}/200 on path 1");

    // Symmetric spare (equal fill fraction) ⇒ uniform split, no concentration.
    sched.path_mut(0).unwrap().cwnd = 20;
    sched.path_mut(0).unwrap().in_flight = 10; // spare 1.0
    sched.path_mut(1).unwrap().cwnd = 20;
    sched.path_mut(1).unwrap().in_flight = 10; // spare 1.0
    let mut c0 = 0;
    for _ in 0..2000 {
        if sched.place_repair_spare_path() == Some(0) {
            c0 += 1;
        }
    }
    assert!(
        (700..=1300).contains(&c0),
        "symmetric spare must split ~evenly (no argmax concentration), got {c0}/2000 on path 0"
    );
}

/// (d) Repair fate steers a repair off the path that carried the window
/// symbols it covers; source placement ignores fate.
#[test]
fn place_repair_fate_steers_off_covered_path() {
    let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
    sched.add_path(0);
    sched.add_path(1);
    set_rtt(&mut sched, 0, 10);
    set_rtt(&mut sched, 1, 10); // identical paths — only fate differs

    // Source ignores fate → balanced even when all coverage is on path 0.
    let src = sched.place_probs(false, &[0, 0, 0, 0]);
    assert!((prob_of(&src, 0) - prob_of(&src, 1)).abs() < 0.05);

    // Repair whose coverage is entirely on path 0 → steered to path 1.
    let rep = sched.place_probs(true, &[0, 0, 0, 0]);
    assert!(
        prob_of(&rep, 1) > 0.95,
        "repair must avoid its own coverage: p1={}",
        prob_of(&rep, 1)
    );

    // Split coverage → fate equal → balanced again.
    let rep_split = sched.place_probs(true, &[0, 0, 1, 1]);
    assert!((prob_of(&rep_split, 0) - prob_of(&rep_split, 1)).abs() < 0.05);
}

/// (e) T → 0 collapses the softmax to argmin (strict best-path, the
/// no-cutoffs limit).
#[test]
fn place_temperature_zero_is_argmin() {
    let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
    sched.add_path(0);
    sched.add_path(1);
    set_rtt(&mut sched, 0, 10); // cheaper
    set_rtt(&mut sched, 1, 50);
    let dist = sched.place_probs_with_temperature(false, &[], 1e-9);
    assert!(prob_of(&dist, 0) > 0.999, "T→0 → argmin all mass on path 0");
    assert!(prob_of(&dist, 1) < 1e-3);
}

/// Single path ⇒ that path always (the law with N=1 is a no-op).
#[test]
fn place_single_path_is_identity() {
    let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
    sched.add_path(0);
    set_rtt(&mut sched, 0, 20);
    let dist = sched.place_probs(false, &[]);
    assert_eq!(dist.len(), 1);
    assert_eq!(dist[0].0, 0);
    assert!((dist[0].1 - 1.0).abs() < 1e-12);
    // Even heavily overdrafted, the lone path is still chosen.
    sched.path_mut(0).unwrap().in_flight = 10_000;
    assert_eq!(sched.place_symbol(false, &[]), Some(0));
}

// ── The cold-start placement price (`RWM_COLD_PLACE`) ─────────────────
//
// The regime these bound is a leg that joins a set whose incumbents are
// already warm (a symmetric quad that starts cold spreads evenly; see
// `the_symmetric_quad_is_deterministic_and_all_four_legs_carry_and_warm`).
// Nothing here is measured on a wire; these bound the law, at absolute
// values.

/// A leg that joins a set of already-warm incumbents is priced at the
/// 50-ms `DEFAULT_SRTT`-class seed and cannot win the placement argmin
/// until the incumbents are >2× overdrawn — and because it wins nothing
/// it is never measured, so the state is a fixed point. Under
/// `RWM_COLD_PLACE` the same leg is priced at the set's own fastest
/// measured srtt and is admitted immediately.
///
/// Absolute, not ordinal: at T → 0 the placement is an argmin, so the
/// cold leg's probability is exactly 0.0 or exactly 1.0 and there is
/// nothing to tune. The incumbents' fill fraction is swept so the result
/// is a price with a crossing, not an exclusion — the off arm does admit
/// the cold leg, but only past a fill the shipped law has no reason to
/// reach, which is what makes the fixed point stick.
#[test]
fn a_late_joining_leg_is_locked_out_by_the_cold_price_and_admitted_without_it() {
    // Two incumbents warm at 8 ms, one leg joining cold. `fill` =
    // in_flight/cwnd on the incumbents; the cold leg has nothing in
    // flight, which is precisely why it looks expensive.
    let build = |fill: f64, cold_place: bool| -> Vec<(PathId, f64)> {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
        sched.set_cold_place(cold_place);
        for id in 0..2 {
            sched.add_path(id);
            set_rtt(&mut sched, id, 8);
            let p = sched.path_mut(id).unwrap();
            p.cwnd = 32;
            p.in_flight = (32.0 * fill) as u32;
        }
        sched.add_path(2); // the late joiner: no RTT sample, ever
        assert!(
            sched.path(2).unwrap().srtt_measured().is_none(),
            "the joining leg must be UNMEASURED or this test proves nothing"
        );
        sched.place_probs_with_temperature(false, &[], f64::MIN_POSITIVE)
    };

    // (1) The lock-out. At any fill the shipped stack actually operates
    // at, the cold leg's mass is exactly zero.
    for fill in [0.25_f64, 0.5, 1.0, 2.0] {
        let d = build(fill, false);
        assert_eq!(
            prob_of(&d, 2),
            0.0,
            "gate OFF, incumbents at fill {fill}: the cold leg took mass, so \
             the 50-ms price is not the exclusion this test bounds"
        );
        assert!(
            prob_of(&d, 0) + prob_of(&d, 1) > 0.999,
            "gate OFF, fill {fill}: the incumbents must hold all the mass"
        );
    }

    // (2) It is a price, not an exclusion — the crossing exists, and it
    // sits above a 2× overdraft. `E_cold = 25 ms` against
    // `E_warm = fill·8 + 4 + eps·8 ms`, so the cold leg wins at
    // `fill > (25 − 4)/8 ≈ 2.6`. A law whose exploration price is only
    // paid by a path already 2.6× past its window is a law that never
    // explores.
    assert_eq!(prob_of(&build(4.0, false), 2), 1.0, "the crossing does not exist");

    // (3) The fixed point. Under the lock-out the leg draws no symbol, so
    // it takes no sample, so it stays cold: sampling the shipped
    // placement is stationary, not merely improbable.
    {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
        sched.set_cold_place(false);
        for id in 0..2 {
            sched.add_path(id);
            set_rtt(&mut sched, id, 8);
            let p = sched.path_mut(id).unwrap();
            p.cwnd = 32;
            p.in_flight = 16;
        }
        sched.add_path(2);
        for _ in 0..500 {
            assert_ne!(
                sched.place_symbol(false, &[]),
                Some(2),
                "the cold leg drew a symbol — the fixed point is not closed"
            );
        }
    }

    // (4) The repair. Same states, gate on: the cold leg is priced at the
    // set's own fastest measured srtt (8 ms ⇒ E = 4 ms) and wins outright
    // the moment the incumbents carry anything at all.
    for fill in [0.25_f64, 0.5, 1.0, 2.0, 4.0] {
        let d = build(fill, true);
        assert_eq!(
            prob_of(&d, 2),
            1.0,
            "gate ON, incumbents at fill {fill}: the cold leg must win — it \
             is the cheapest path in the set by the objective's own units"
        );
    }

    // (5) And it is self-limiting without a threshold. The repaired price
    // buys exploration, not a monopoly: once the explored leg carries its
    // own backlog the same formula hands the placement back. No counter,
    // no warm-up phase, no `if cold` beyond the estimator's `None`.
    {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
        sched.set_cold_place(true);
        for id in 0..2 {
            sched.add_path(id);
            set_rtt(&mut sched, id, 8);
            let p = sched.path_mut(id).unwrap();
            p.cwnd = 32;
            p.in_flight = 8; // fill 0.25
        }
        sched.add_path(2);
        let p2 = sched.path_mut(2).unwrap();
        p2.cwnd = 32;
        p2.in_flight = 16; // the explored leg is now the LOADED one
        let d = sched.place_probs_with_temperature(false, &[], f64::MIN_POSITIVE);
        assert_eq!(
            prob_of(&d, 2),
            0.0,
            "the repaired price kept feeding a leg that is now the most \
             loaded in the set — that would be a monopoly, not exploration"
        );
    }
}

/// Off is bit-identical, and so is on once every leg is measured.
///
/// The two halves of the gate's safety claim, as exact `f64` equality
/// rather than a tolerance:
///
///   - gate off at any state ⇒ the shipped expression verbatim (the cold
///     price is `p.srtt()`);
///   - gate on with every active leg measured ⇒ `srtt_of == p.srtt()` at
///     every leg, so the repair is inert in the warm regime and the
///     placement objective is untouched there. Only the cold regime
///     changes.
#[test]
fn the_cold_price_is_inert_off_and_inert_once_every_leg_is_measured() {
    // A state generator covering both regimes: `cold` = how many of the
    // four legs have never had a sample.
    //
    // Both arms from one scheduler: `place_probs` normalizes by summing
    // over the paths in the map's
    // iteration order, and float addition is not associative — two
    // separately-built schedulers hash differently, so their inert-arm
    // probabilities can differ at the ULP even when the gate provably
    // changes nothing. Sorting the output (below) fixes the zip pairing
    // but not the internal summation order. One instance, flag toggled,
    // makes bit-equality a claim about the gate instead of the hasher.
    let build = |cold: usize| -> (Vec<(PathId, f64)>, Vec<(PathId, f64)>) {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
        for id in 0..4u32 {
            sched.add_path(id);
            if (id as usize) < 4 - cold {
                set_rtt(&mut sched, id, 10 + 10 * u64::from(id));
            }
            let p = sched.path_mut(id).unwrap();
            p.cwnd = 16 + 4 * id;
            p.in_flight = 3 * id + 1;
        }
        // Sorted by path id: `place_probs` yields `HashMap` order, so an
        // unsorted zip would compare path 3 against path 1 and "find" a
        // difference that is only the map's.
        sched.set_cold_place(false);
        let mut off = sched.place_probs_with_temperature(false, &[], 0.15);
        off.sort_by_key(|(pid, _)| *pid);
        sched.set_cold_place(true);
        let mut on = sched.place_probs_with_temperature(false, &[], 0.15);
        on.sort_by_key(|(pid, _)| *pid);
        (off, on)
    };

    for cold in 0..=4 {
        let (off, on) = build(cold);
        // Mechanism liveness: the arms must actually differ somewhere, or
        // the equalities below are vacuous. They differ exactly when some
        // leg is cold and some leg is measured.
        let differs = off
            .iter()
            .zip(on.iter())
            .any(|((_, a), (_, b))| a.to_bits() != b.to_bits());
        let mixed = cold > 0 && cold < 4;
        assert_eq!(
            differs, mixed,
            "cold={cold}: the gate must move the distribution EXACTLY when \
             the set is mixed (some measured, some not) — differs={differs}"
        );
        if !mixed {
            for ((pa, a), (pb, b)) in off.iter().zip(on.iter()) {
                assert_eq!(pa, pb);
                assert_eq!(
                    a.to_bits(),
                    b.to_bits(),
                    "cold={cold}: path {pa} moved {a} → {b}; with no leg cold \
                     (or every leg cold) the two arms are the same formula on \
                     the same inputs and must agree BIT-for-bit"
                );
            }
        }
    }
}

/// The gate is an anchor-hygiene family member and ships off, so a
/// freshly constructed `Scheduler` must carry the shipped price unless
/// the environment says otherwise. Reads the same cached resolution
/// `RuntimeGates` echoes, so the echo and the behaviour cannot disagree.
#[test]
fn a_fresh_scheduler_carries_the_resolved_cold_place_setting() {
    let sched = Scheduler::new(Arc::new(WallClock));
    assert_eq!(
        sched.cold_place(),
        cold_place_active(),
        "the scheduler's placement price and the process gate disagree — \
         the [GATES] echo would then be describing a different machine"
    );
}

// The BBR send-interval anchor must read the true bottleneck rate under
// (a) ack-aggregation (batched acks) and (b) a deep standing queue — the
// conditions under which an ack-interval anchor over-reads by orders of
// magnitude. Driven by a bottleneck-link simulation: we send 3× the link
// rate (a standing queue builds without bound) and the link FIFO-drains at
// the true rate R; acks are processed in batches (aggregation). The
// measured BtlBw must track R, not the send rate and not queue/interval.
#[test]
fn rate_sample_anchor_reads_true_btlbw_under_aggregation_and_queue() {
    use std::collections::{BTreeMap, VecDeque};
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new_with_hint(clock.clone(), ProtocolHint::Bulk);
    sched.add_path(0);
    let prop = Duration::from_millis(5); // one-way; RTprop ~ 10ms
    // Seed RTprop so the max-filter window (~10·RTprop) and btlbw product form.
    for _ in 0..4 {
        sched.path_mut(0).unwrap().record_rtt_sample(Duration::from_millis(10));
    }

    let link_r: u64 = 8; // TRUE bottleneck: 8 sym/ms = 8000 sym/s
    let send_s: u64 = 24; // SEND 3× the link rate → standing queue grows
    let tick = Duration::from_millis(1);
    let mut seq: u64 = 0;
    let mut link_fifo: VecDeque<u64> = VecDeque::new(); // waiting for link service
    let mut deliver_due: BTreeMap<u64, Vec<u64>> = BTreeMap::new(); // arrival_us → seqs
    let start = clock.now();
    let us = |c: &MockClock| c.now().duration_since(start).as_micros() as u64;

    for step in 0..300u64 {
        // Send send_s symbols this ms (overload).
        for _ in 0..send_s {
            sched.path_mut(0).unwrap().on_src_sent(seq, false);
            link_fifo.push_back(seq);
            seq += 1;
        }
        // The link serves link_r symbols this ms (the bottleneck); each served
        // symbol arrives one propagation delay later.
        let arrival = us(&clock) + prop.as_micros() as u64;
        for _ in 0..link_r {
            if let Some(s) = link_fifo.pop_front() {
                deliver_due.entry(arrival).or_default().push(s);
            }
        }
        // Ack aggregation: only "process acks" every 3 ms, delivering all due
        // symbols at the same clock instant (a batched ack → tiny ack Δt).
        if step % 3 == 0 {
            let nowu = us(&clock);
            let due: Vec<u64> = deliver_due
                .range(..=nowu)
                .flat_map(|(_, v)| v.iter().copied())
                .collect();
            deliver_due.retain(|&t, _| t > nowu);
            for s in due {
                sched.path_mut(0).unwrap().on_src_delivered_seq(s);
            }
        }
        clock.advance(tick);
    }

    let btlbw = sched
        .path(0)
        .unwrap()
        .btlbw_sym_per_s()
        .expect("anchor must establish");
    let true_rate = (link_r * 1000) as f64; // 8000 sym/s
    // The standing queue is deep (send 3× drain for 300ms): outstanding is far
    // above one BDP.  A queue/ack-interval anchor would read many× the link;
    // the send-interval anchor must track the true bottleneck within ~2×.
    assert!(
        btlbw > 0.5 * true_rate && btlbw < 2.0 * true_rate,
        "send-interval BtlBw must track the TRUE link rate {true_rate:.0} sym/s \
         under batched acks + a standing queue, got {btlbw:.0} (send rate was \
         {} sym/s)",
        send_s * 1000
    );
    // Crucially, not the orders-of-magnitude ack-interval over-read.
    assert!(
        btlbw < 10.0 * true_rate,
        "must NOT exhibit the legacy aggregation over-read: {btlbw:.0} vs true {true_rate:.0}"
    );
}

// An app-limited (starved) sample that reads below the running max must not
// enter the max-filter (BBR: app-limited samples may only raise the anchor,
// never be read as bw dropping / corrupt a starved interval).
#[test]
fn rate_sample_excludes_app_limited_samples_below_the_max() {
    let clock = Arc::new(MockClock::new());
    let mut copa = CopaState::new(clock.clone(), ProtocolHint::Bulk);
    copa.record_rtt(Duration::from_millis(10)); // RTprop = 10 ms

    // Establish a healthy max with a valid sample spanning ≥ RTprop: send
    // seq0, inject 50 deliveries, ack it one RTprop-plus later.
    copa.rs_on_sent(0, false);
    copa.rs_delivered += 50;
    clock.advance(Duration::from_millis(20)); // ≥ RTprop
    copa.rs_on_delivered(0); // rate ≈ 51 / 0.02 s
    let max_before = copa.max_bw;
    let samples_before = copa.bw_samples.len();
    assert!(max_before > 0.0, "baseline max must establish: {max_before}");

    // An app-limited sample reading below the max must be excluded: send
    // seq1 app-limited, deliver one symbol one RTprop-plus later (low rate,
    // interval ≥ RTprop so the MinRTT guard passes and only app-limited
    // gates it out).
    copa.rs_on_sent(1, true);
    clock.advance(Duration::from_millis(20));
    copa.rs_on_delivered(1);
    assert_eq!(
        copa.bw_samples.len(),
        samples_before,
        "app-limited sample below the max must not enter the filter"
    );
    assert!(
        (copa.max_bw - max_before).abs() < 1.0,
        "app-limited low sample must not change the max: before={max_before} after={}",
        copa.max_bw
    );

    // A non-app-limited sample at a genuinely higher rate (interval ≥ RTprop)
    // is still admitted and raises the max.
    copa.rs_on_sent(2, false);
    copa.rs_delivered += 100;
    clock.advance(Duration::from_millis(20)); // ≥ RTprop
    copa.rs_on_delivered(2); // rate ≈ 101 / 0.02 s > max_before
    assert!(
        copa.max_bw > max_before,
        "a higher genuine sample (interval ≥ RTprop) must raise the max: \
         before={max_before} after={}",
        copa.max_bw
    );

    // A sub-RTprop burst (interval < RTprop) is rejected by the MinRTT guard —
    // this is the ack-aggregation / send-burst over-read defence.
    let max_after = copa.max_bw;
    copa.rs_on_sent(3, false);
    copa.rs_delivered += 10_000; // huge delivered count …
    clock.advance(Duration::from_micros(100)); // … over a tiny interval
    copa.rs_on_delivered(3);
    assert!(
        (copa.max_bw - max_after).abs() < 1.0,
        "a sub-RTprop burst must be rejected (no over-read): before={max_after} after={}",
        copa.max_bw
    );
}

// ----- Honest inputs (RWM_HONEST_ANCHOR / RWM_HONEST_K) --------------------

/// The equivalence pin for `RWM_HONEST_ANCHOR` (its off-value property
/// and its on-value property are the same property): the monotonic
/// max-deque's front equals the full-window fold over
/// `bw_samples` after every push and every eviction, across both feed
/// paths (per-ack `record_delivery` and per-symbol `rs_on_delivered`),
/// across window-length changes (min_rtt moving the ≈10·RTprop cutoff)
/// and across long idle gaps (mass evictions). The gate may therefore
/// select cost only, never a value.
#[test]
fn bw_mono_front_equals_full_window_fold() {
    let clock = Arc::new(MockClock::new());
    let mut legacy = CopaState::new(clock.clone(), ProtocolHint::Bulk);
    let mut o1 = CopaState::new(clock.clone(), ProtocolHint::Bulk);
    o1.force_bw_o1();
    legacy.record_rtt(Duration::from_millis(10));
    o1.record_rtt(Duration::from_millis(10));

    // Deterministic LCG so the stream is reproducible.
    let mut lcg: u64 = 0x9E3779B97F4A7C15;
    let mut rnd = move || {
        lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (lcg >> 33) as u32
    };
    let mut seq = 0u64;
    for step in 0..4000u32 {
        let r = rnd();
        match r % 4 {
            // Per-symbol samples (the RWM_PLAIN_RS feed path).
            0 | 1 => {
                legacy.rs_on_sent(seq, false);
                o1.rs_on_sent(seq, false);
                let extra = (r >> 8) % 50;
                legacy.rs_delivered += extra as u64;
                o1.rs_delivered += extra as u64;
                clock.advance(Duration::from_millis(11 + (r >> 16) as u64 % 30));
                legacy.rs_on_delivered(seq);
                o1.rs_on_delivered(seq);
                seq += 1;
            }
            // Per-ack samples (the ack-interval Copa feed path).
            2 => {
                clock.advance(Duration::from_millis(2 + (r >> 8) as u64 % 20));
                legacy.record_delivery(1 + r % 100);
                o1.record_delivery(1 + r % 100);
            }
            // Occasional long gap (mass eviction) and an RTT sample that
            // moves min_rtt, hence the ≈10·RTprop window length.
            _ => {
                if step % 37 == 0 {
                    clock.advance(Duration::from_millis(1500));
                }
                let rtt = Duration::from_millis(5 + (r % 120) as u64);
                legacy.record_rtt(rtt);
                o1.record_rtt(rtt);
            }
        }
        let fold_l = legacy.bw_fold();
        let fold_o = o1.bw_fold();
        assert_eq!(fold_l, fold_o, "identical streams, step {step}");
        assert_eq!(
            legacy.max_bw, fold_l,
            "legacy max_bw IS the fold, step {step}"
        );
        assert_eq!(
            o1.max_bw, fold_o,
            "O(1) max_bw equals the full-window fold, step {step}"
        );
        let front = o1.bw_mono.front().map_or(0.0, |s| s.delivery_rate);
        assert_eq!(front, fold_o, "mono front == fold, step {step}");
    }
    assert!(legacy.max_bw > 0.0, "the stream must have produced samples");
}

/// Formula agreement for the Copa SRTT estimator (`CopaState::record_rtt`)
/// against RFC 6298 computed independently in the test — the estimator
/// analogue of `tests/formula_agreement.rs`.
///
/// RFC 6298 §2.2/§2.3, srtt terms only (rttvar/RTO are not part of this
/// estimator):
///
/// ```text
///   first measurement R:   SRTT ← R
///   subsequent R':         SRTT ← (1 − α)·SRTT + α·R',  α = 1/8
/// ```
///
/// Three absolute cases:
///   1. Seed — the first sample is the srtt, exactly (and before any
///      sample there is no measured srtt at all).
///   2. Steady — 7/8·s + 1/8·r over a known mixed sequence agrees with
///      the recursion computed here in f64, within Duration's
///      per-step nanosecond rounding (≪ 1 µs over the whole sequence).
///   3. Fixed point — a constant input is a fixed point (seeded at the
///      constant, the EWMA holds it bit-exactly: 7/8·c + 1/8·c has no
///      rounding at whole-millisecond c), and from a perturbed history
///      the estimator converges to the constant at the (7/8)^n rate.
#[test]
fn copa_srtt_agrees_with_rfc6298_ewma() {
    let clock = Arc::new(MockClock::new());
    let mut cs = CopaState::new(clock.clone(), ProtocolHint::Bulk);

    // Case 1 — seed: SRTT ← R on the first measurement, exactly.
    assert_eq!(cs.srtt, None, "no sample yet ⇒ no measured srtt");
    cs.record_rtt(millis(48));
    assert_eq!(
        cs.srtt,
        Some(millis(48)),
        "RFC 6298 §2.2: the first sample must BE the srtt, exactly"
    );

    // Case 2 — steady: SRTT ← 7/8·SRTT + 1/8·R over a known mixed
    // sequence (spikes both ways), against the independent f64 recursion.
    let seq_ms: [u64; 10] = [80, 40, 40, 120, 33, 47, 60, 5, 500, 48];
    let mut expect_s = 0.048_f64; // the seed above
    for &ms in &seq_ms {
        clock.advance(millis(5));
        cs.record_rtt(millis(ms));
        expect_s = 0.875 * expect_s + 0.125 * (ms as f64 / 1e3);
        let got_s = cs.srtt.expect("seeded above").as_secs_f64();
        assert!(
            (got_s - expect_s).abs() < 1e-6,
            "RFC 6298 recursion disagrees after the {ms} ms sample: \
             engine {got_s} s vs independent formula {expect_s} s"
        );
    }

    // Case 3a — fixed point, exact: seeded at a constant, the EWMA must
    // return the constant bit-for-bit on every subsequent sample.
    let mut flat = CopaState::new(clock.clone(), ProtocolHint::Bulk);
    flat.record_rtt(millis(40));
    for _ in 0..32 {
        clock.advance(millis(5));
        flat.record_rtt(millis(40));
        assert_eq!(
            flat.srtt,
            Some(millis(40)),
            "a constant input must be an EXACT fixed point of the EWMA"
        );
    }

    // Case 3b — convergence: from the mixed history above, a constant
    // 40 ms input closes the gap as (7/8)^n. After n = 200 samples the
    // initial offset (< 1 s) is below 1 ns, so the srtt must sit within
    // 1 µs of the input — and agree with the f64 recursion throughout.
    for _ in 0..200 {
        clock.advance(millis(5));
        cs.record_rtt(millis(40));
        expect_s = 0.875 * expect_s + 0.125 * 0.040;
    }
    let got_s = cs.srtt.expect("seeded above").as_secs_f64();
    assert!(
        (got_s - expect_s).abs() < 1e-6,
        "recursion agreement must hold through convergence: \
         engine {got_s} s vs formula {expect_s} s"
    );
    assert!(
        (got_s - 0.040).abs() < 1e-6,
        "a constant input must converge to itself: srtt {got_s} s vs 40 ms"
    );
}

/// The `RWM_HONEST_ANCHOR` cost curve, in one process (the component
/// instrument, docs/measurement-discipline.md rule 14): the per-sample
/// full-window fold's cost per delivered symbol grows with the symbol rate
/// (the window holds ≈ rate × 1 s samples ⇒ O(rate²) per second), while
/// the mono-deque read is flat. `#[ignore]`d: it is a measurement with
/// wall-clock timing; run
///
/// ```text
///   cargo test --release -p raptorpath --lib -- --ignored --nocapture bw_filter_cost
/// ```
#[test]
#[ignore = "measurement: run explicitly with --release --ignored --nocapture"]
fn bw_filter_cost_is_quadratic_legacy_and_linear_fixed() {
    fn per_delivery_ns(rate_sym_s: u64, o1: bool) -> f64 {
        let clock = Arc::new(MockClock::new());
        let mut copa = CopaState::new(clock.clone(), ProtocolHint::Bulk);
        if o1 {
            copa.force_bw_o1();
        }
        copa.record_rtt(Duration::from_millis(10)); // RTprop 10 ms ⇒ window 1 s
        let step = Duration::from_nanos(1_000_000_000 / rate_sym_s);
        let lag = (rate_sym_s / 50) as u64; // ≈20 ms of in-flight seqs
        let mut send_seq = 0u64;
        // Warm: fill one full window so the deque is at steady state.
        let warm = rate_sym_s * 12 / 10;
        for _ in 0..warm {
            copa.rs_on_sent(send_seq, false);
            if send_seq >= lag {
                copa.rs_on_delivered(send_seq - lag);
            }
            clock.advance(step);
            send_seq += 1;
        }
        assert!(
            copa.bw_samples.len() as u64 > rate_sym_s / 2,
            "window must be rate-sized: {} at {rate_sym_s}",
            copa.bw_samples.len()
        );
        // Measure N deliveries (with their sends) at steady state.
        let n = 100_000u64;
        let t = std::time::Instant::now();
        for _ in 0..n {
            copa.rs_on_sent(send_seq, false);
            copa.rs_on_delivered(send_seq - lag);
            clock.advance(step);
            send_seq += 1;
        }
        let ns = t.elapsed().as_nanos() as f64 / n as f64;
        println!(
            "[HONEST-BENCH] rate={rate_sym_s} o1={o1} window_samples={} \
             per_delivery={ns:.0} ns  (per-second-of-transfer cost: {:.0} ms)",
            copa.bw_samples.len(),
            ns * rate_sym_s as f64 / 1e6,
        );
        ns
    }
    // A mid rate (9.6 k) and a high single-path rate class (24 k).
    let legacy_lo = per_delivery_ns(9_600, false);
    let legacy_hi = per_delivery_ns(24_000, false);
    let o1_lo = per_delivery_ns(9_600, true);
    let o1_hi = per_delivery_ns(24_000, true);
    println!(
        "[HONEST-BENCH] legacy 24k/9.6k = {:.2} (rate-dependence), \
         o1 24k = {:.3} of legacy 24k (the removal)",
        legacy_hi / legacy_lo,
        o1_hi / legacy_hi,
    );
    assert!(
        legacy_hi > 1.8 * legacy_lo,
        "the legacy fold's per-delivery cost must GROW with rate \
         (the measured rate-dependent tax): {legacy_lo:.0} → {legacy_hi:.0} ns"
    );
    assert!(
        o1_hi < 0.25 * legacy_hi,
        "the O(1) read must remove the dominant cost at c1's rate: \
         o1 {o1_hi:.0} vs legacy {legacy_hi:.0} ns"
    );
    assert!(
        o1_hi < 3.0 * o1_lo.max(50.0),
        "the O(1) read must be rate-flat: {o1_lo:.0} → {o1_hi:.0} ns"
    );
}

// ----- Wire-clocked Copa signal + hint→δ mapping (ADR-0062) -----------------

#[test]
fn copa_wire_gate_from_env() {
    // Default on exactly when the engine owns/feeds the substrate window.
    assert!(copa_wire_from_env(Some("passthrough"), false, None));
    assert!(copa_wire_from_env(Some(" Passthrough "), false, None));
    assert!(copa_wire_from_env(None, true, None)); // RWM_COPA_FEED=1 A/B
    // Shipped default: everything unset ⇒ off.
    assert!(!copa_wire_from_env(None, false, None));
    assert!(!copa_wire_from_env(Some("bbr"), false, None));
    assert!(!copa_wire_from_env(Some("cubic"), false, None));
    // RWM_COPA_WIRE=0 keeps the app-echo law even under passthrough.
    assert!(!copa_wire_from_env(Some("passthrough"), false, Some("0")));
    assert!(!copa_wire_from_env(Some("passthrough"), true, Some("false")));
    // RWM_COPA_WIRE=1 forces on (e.g. RWM_COPA_FEED-less diagnostics).
    assert!(copa_wire_from_env(None, false, Some("1")));
}

/// The placement weights are a dial, not a mode (paper §5.7).
/// Bit-exact at the three presets, plus continuity and monotonicity
/// through every named point.
#[test]
fn scheduling_weights_are_the_dial_not_a_mode() {
    for (h, lat, bw) in [
        (ProtocolHint::Realtime, 1.0f64, 0.0f64),
        (ProtocolHint::Auto, 0.5, 0.5),
        (ProtocolHint::Bulk, 0.0, 1.0),
    ] {
        let w = SchedulingWeights::from_hint(h);
        assert_eq!(w.w_lat, lat, "{h:?}: w_lat");
        assert_eq!(w.w_bw, bw, "{h:?}: w_bw");
        assert_eq!(w.w_div, 1.0, "{h:?}: w_div is hint-independent");
        // The two weights are a partition at every point, exactly.
        assert_eq!(w.w_lat + w.w_bw, 1.0, "{h:?}: the weights must sum to 1");
    }
    // Auto is the exact log-midpoint of the dial — which is what makes the
    // form affine rather than a fit.
    let (r, a, b) = (
        hint_delta_price(ProtocolHint::Realtime).log10(),
        hint_delta_price(ProtocolHint::Auto).log10(),
        hint_delta_price(ProtocolHint::Bulk).log10(),
    );
    assert!(
        ((r + b) / 2.0 - a).abs() < 1e-12,
        "Auto is not the log-midpoint: {r} {a} {b}"
    );
    // Continuous and strictly monotone across the dial, with ±2 % nudges
    // at each preset (CLAUDE.md: no behaviour step at a preset).
    let mut prev = f64::INFINITY;
    for i in 0..=400 {
        let d = 0.005 * (50.0f64 / 0.005).powf(i as f64 / 400.0);
        let w = SchedulingWeights::from_delta(d);
        assert!(w.w_lat >= 0.0 && w.w_lat <= 1.0, "δ={d}: w_lat left [0, 1]");
        assert_eq!(w.w_lat + w.w_bw, 1.0, "δ={d}: the weights must sum to 1");
        assert!(w.w_lat > prev - 1e-15 || i == 0, "δ={d}: w_lat is not increasing");
        prev = w.w_lat;
    }
    for h in [ProtocolHint::Realtime, ProtocolHint::Auto, ProtocolHint::Bulk] {
        let d0 = hint_delta_price(h);
        let w0 = SchedulingWeights::from_delta(d0).w_lat;
        for f in [0.98f64, 1.02] {
            let w1 = SchedulingWeights::from_delta(d0 * f).w_lat;
            assert!(
                (w1 - w0).abs() < 0.01,
                "{h:?}: a {f}× nudge of δ stepped w_lat from {w0} to {w1}"
            );
        }
    }
}

#[test]
fn copa_delta_hint_mapping() {
    // δ(hint) = COPA_DELTA / ζ(hint): the hint's one declared price
    // ratio (tail_loss_scale ζ = 0.01/1/100) is the latency price, δ
    // (paper §4.1). No constants beyond the Copa-paper δ=0.5 anchor.
    assert_eq!(copa_delta(ProtocolHint::Auto, None), COPA_DELTA);
    assert_eq!(copa_delta(ProtocolHint::Bulk, None), COPA_DELTA / 100.0);
    assert_eq!(copa_delta(ProtocolHint::Realtime, None), COPA_DELTA * 100.0);
    // Equilibrium queue = 1/δ packets: Bulk 200, Auto 2, Realtime 0.02.
    assert_eq!(1.0 / copa_delta(ProtocolHint::Bulk, None), 200.0);
    // The RWM_COPA_DELTA frontier knob overrides the hint; garbage is ignored.
    assert_eq!(copa_delta(ProtocolHint::Bulk, Some(0.05)), 0.05);
    assert_eq!(copa_delta(ProtocolHint::Bulk, Some(-1.0)), COPA_DELTA / 100.0);
    assert_eq!(
        copa_delta(ProtocolHint::Bulk, Some(f64::NAN)),
        COPA_DELTA / 100.0
    );
}

#[test]
fn wire_dq_keys_on_wire_clock_not_app_echo() {
    // The app-layer echo RTT includes the sender's own store/reservoir
    // dwell, so Copa on the echo backs off against self-inflicted delay.
    // Under the wire signal the CC delay term comes only from
    // record_rtt_sample (the packet-timed wire feed);
    // the estimator's app-echo RTT — dwell included — must have zero
    // influence on the cwnd dynamics.
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    path.force_wire_for_test(COPA_DELTA);

    // App echo reads a huge 500 ms (store dwell); the wire reads a clean
    // 10 ms floor. Copa must ramp — the dwell is not network queue.
    let mut prev = path.cwnd;
    for _ in 0..5 {
        path.estimator.record_rtt(millis(500)); // app echo incl. dwell
        path.record_rtt_sample(millis(10)); // wire clock
        clock.advance(millis(15));
        path.on_ack(prev);
        let cur = path.cwnd;
        assert!(
            cur > prev,
            "wire-clocked Copa must grow through app-layer dwell: {prev}->{cur}"
        );
        prev = cur;
    }

    // Inverse direction: the wire clock now shows a real standing queue
    // (60 ms over the 10 ms floor) while the app echo is quiet — Copa
    // must back off on the wire evidence alone.
    for _ in 0..4 {
        path.estimator.record_rtt(millis(10));
        path.record_rtt_sample(millis(60));
    }
    clock.advance(millis(80));
    let pre = path.cwnd;
    path.on_ack(4);
    assert!(
        path.cwnd < pre,
        "a wire-clock queue must back cwnd off: {pre}->{}",
        path.cwnd
    );
}

#[test]
fn wire_velocity_law_doubles_step_and_caps_drain() {
    // Copa's actual update law (paper §8.2): step v/δ per
    // SRTT, v doubling while the direction persists. δ = 0.005 (Bulk) ⇒
    // base step 200 symbols — the small-δ/high-BDP exploitation the +2
    // additive probe could never provide.
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    path.force_wire_for_test(0.005);

    // Drive the pure law via on_delivery_signal (no delivery samples ⇒
    // no BtlBw anchor ⇒ the coupling cap stays out of the picture —
    // covered by its own test below). Ramp on a clean 10 ms floor, then
    // exit the ramp with a moderate standing queue (40 ms — well above
    // the jitter the square transition charges into the headroom
    // estimators).
    for _ in 0..10 {
        path.record_rtt_sample(millis(10));
        clock.advance(millis(15));
        path.on_delivery_signal();
    }
    assert!(path.cwnd > 100, "ramp must have grown: {}", path.cwnd);
    for _ in 0..4 {
        path.record_rtt_sample(millis(40)); // rate ≫ 1/(δ·dq) at this cwnd
    }
    clock.advance(millis(60));
    path.on_delivery_signal();
    assert!(!path.in_slow_start, "queue evidence must end the ramp");

    // Steady state, clean floor again: base up-step is 1/δ = 200; the
    // velocity doubles only after the direction has persisted ≥ 3
    // updates (Copa §2.2 hysteresis — bounds the overshoot).
    let c0 = path.cwnd;
    let mut cs = vec![c0];
    for _ in 0..3 {
        path.record_rtt_sample(millis(10));
        clock.advance(millis(400)); // > srtt (spiked EWMA) → update due
        path.on_delivery_signal();
        cs.push(path.cwnd);
    }
    let step1 = cs[1] - cs[0];
    let step2 = cs[2] - cs[1];
    let step3 = cs[3] - cs[2];
    assert!(
        step1 >= 190,
        "bulk-δ base step must be ~1/δ = 200: {cs:?}"
    );
    assert!(
        step2 <= step1 + 2,
        "velocity must NOT double before the 3-update streak: {cs:?}"
    );
    assert!(
        step3 >= 2 * step1 - 2,
        "3-update persistent direction must double the velocity: {cs:?}"
    );

    // Down direction: one v/δ step down (velocity resets on the flip),
    // never a collapse.
    let pre = path.cwnd;
    for _ in 0..4 {
        path.record_rtt_sample(millis(60)); // real queue: dq ≈ 50 ms
    }
    clock.advance(millis(400));
    path.on_delivery_signal();
    let post = path.cwnd;
    assert!(post < pre, "above-target must step down: {pre}->{post}");
    assert!(
        post + 210 >= pre,
        "a single down move is one v/δ step: {pre}->{post}"
    );
}

#[test]
fn wire_coupling_cap_bounds_cwnd_at_bdp_plus_two_over_delta() {
    // Once cwnd exceeds the sender's outstanding store, the delay signal
    // is decoupled and a jitter-clamped d_q votes "up" forever (cwnd
    // ratchets to MAX_CWND). The coupling cap bounds cwnd at the
    // Copa fixed point plus one dither amplitude: BDP + 2/δ.
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    path.force_wire_for_test(0.005);
    // 25 clean-floor updates with a live delivery rate: μ̂ ≈ 50/15 ms ≈
    // 3 333 sym/s, RTprop 10 ms ⇒ BDP ≈ 33; cap ≈ 33 + 400 = 433.
    for _ in 0..25 {
        path.record_rtt_sample(millis(10));
        clock.advance(millis(15));
        path.on_ack(50);
    }
    let bdp = path.copa_bdp_anchor().expect("anchor must be warm");
    let cap = bdp + 2.0 / 0.005;
    assert!(
        (path.cwnd as f64) <= cap + 1.0,
        "cwnd must stay coupled: cwnd={} cap={cap:.0} (bdp={bdp:.0})",
        path.cwnd
    );
    assert!(
        path.cwnd > PathState::MIN_CWND,
        "the cap must not collapse the window: {}",
        path.cwnd
    );
}

// --- Copa §2.2 TCP-competitive mode ------------------------------------------

/// Establish a clean 10 ms wire floor and exit the ramp so the per-SRTT
/// velocity law (and with it `compete_update`) is live.
fn compete_warmup(path: &mut PathState, clock: &Arc<MockClock>) {
    for _ in 0..10 {
        path.record_rtt_sample(millis(10));
        clock.advance(millis(15));
        path.on_delivery_signal();
    }
    // Ramp exit on first standing-queue evidence.
    for _ in 0..4 {
        path.record_rtt_sample(millis(60));
    }
    clock.advance(millis(70));
    path.on_delivery_signal();
    assert!(!path.in_slow_start, "warmup must end the ramp");
}

/// One per-SRTT update under a never-draining standing queue (60 ms over
/// the 10 ms floor — a buffer-filling competitor's signature).
fn queue_update(path: &mut PathState, clock: &Arc<MockClock>) {
    for _ in 0..4 {
        path.record_rtt_sample(millis(60));
    }
    clock.advance(millis(70));
    path.on_delivery_signal();
}

#[test]
fn compete_detection_fires_under_never_draining_queue() {
    // Copa §2.2: no "nearly empty" queue (d_q < 0.1·(RTTmax−RTTmin)) in
    // the last 5 RTTs ⇒ competitive mode; the AIMD then grows 1/δ past
    // the hint base.
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    path.force_wire_for_test(0.005);
    path.force_compete_for_test();
    compete_warmup(&mut path, &clock);
    for _ in 0..40 {
        queue_update(&mut path, &clock);
    }
    let (on, in_compete, switches, delta, base) = path.copa_compete_diag();
    assert!(on, "gate must be on (forced)");
    assert!(in_compete, "a never-draining queue must switch to competitive mode");
    assert!(switches >= 1, "the entry must be counted");
    assert!(
        delta < base,
        "the loss-free AIMD must have grown 1/δ past the base: δ={delta} base={base}"
    );
    assert!(delta <= base, "invariant: δ ≤ δ_base in competitive mode");
}

#[test]
fn compete_detection_quiet_under_draining_queue() {
    // The queue drains to ~the floor every 3rd update (≤ 5 RTTs apart):
    // Copa's own dynamics look like this — mode switching must not fire.
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    path.force_wire_for_test(0.005);
    path.force_compete_for_test();
    compete_warmup(&mut path, &clock);
    for _ in 0..30 {
        queue_update(&mut path, &clock);
        queue_update(&mut path, &clock);
        // The drain trough: samples back at the floor mark nearly-empty.
        for _ in 0..4 {
            path.record_rtt_sample(millis(11));
        }
        clock.advance(millis(70));
        path.on_delivery_signal();
    }
    let (_, in_compete, switches, delta, base) = path.copa_compete_diag();
    assert!(
        !in_compete && switches == 0,
        "a regularly-draining queue must stay in default mode (switches={switches})"
    );
    assert_eq!(delta, base, "default mode keeps the hint-mapped base δ");
}

#[test]
fn compete_delta_follows_aimd_on_inverse_delta() {
    // The verified law (Copa §2.2): AIMD on 1/δ — +1 per RTT without
    // loss, halve on loss, floored at the default-mode δ (δ ≤ δ_base).
    // Base δ = 0.5 (the paper's default) makes the arithmetic direct:
    // 1/δ: 2 → 3 → (loss: max(1.5, 2) = 2) → 3 → 4 → (loss) → 2.
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    path.force_wire_for_test(0.5);
    path.force_compete_for_test();
    compete_warmup(&mut path, &clock);
    // Drive updates until the detector enters competitive mode.
    let mut entered = false;
    for _ in 0..20 {
        queue_update(&mut path, &clock);
        if path.copa_compete_diag().1 {
            entered = true;
            break;
        }
    }
    assert!(entered, "never-draining queue must enter competitive mode");
    // Additive increase from the entry base: 1/δ 2 → 3.
    queue_update(&mut path, &clock);
    let d = path.copa_compete_diag().3;
    assert!((d - 1.0 / 3.0).abs() < 1e-12, "AI must be 1/δ += 1: δ={d}");
    // Loss (shim congestion-event counter advanced): 1/δ halves, floored
    // at 1/δ_base — max(3/2, 2) = 2 ⇒ δ back to the base.
    path.on_wire_congestion_events(1);
    queue_update(&mut path, &clock);
    let d = path.copa_compete_diag().3;
    assert!((d - 0.5).abs() < 1e-12, "MD must floor at δ_base: δ={d}");
    // Two clean updates: 2 → 3 → 4.
    queue_update(&mut path, &clock);
    queue_update(&mut path, &clock);
    let d = path.copa_compete_diag().3;
    assert!((d - 0.25).abs() < 1e-12, "AI must continue: δ={d}");
    // A stale counter read (no advance) is not a loss.
    path.on_wire_congestion_events(1);
    queue_update(&mut path, &clock);
    let d = path.copa_compete_diag().3;
    assert!((d - 0.2).abs() < 1e-12, "no counter advance ⇒ no MD: δ={d}");
    // Loss again: max(5/2, 2) = 2.5 ⇒ δ = 0.4 (a real halving above the
    // floor this time).
    path.on_wire_congestion_events(2);
    queue_update(&mut path, &clock);
    let d = path.copa_compete_diag().3;
    assert!((d - 0.4).abs() < 1e-12, "MD must halve 1/δ: δ={d}");
    let (_, _, _, delta, base) = path.copa_compete_diag();
    assert!(delta <= base, "invariant: δ ≤ δ_base throughout");
}

#[test]
fn compete_switches_back_on_drain_and_resets_delta() {
    // Copa §2.2: "When Copa switches from competitive mode to default
    // mode, it resets δ" to the default-mode value (the hint base here).
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    path.force_wire_for_test(0.005);
    path.force_compete_for_test();
    compete_warmup(&mut path, &clock);
    for _ in 0..12 {
        queue_update(&mut path, &clock);
    }
    let (_, in_compete, _, delta, base) = path.copa_compete_diag();
    assert!(in_compete && delta < base, "precondition: competitive, δ adapted");
    // The competitor leaves: the queue drains to the floor.
    for _ in 0..4 {
        path.record_rtt_sample(millis(11));
    }
    clock.advance(millis(70));
    path.on_delivery_signal();
    let (_, in_compete, _, delta, base) = path.copa_compete_diag();
    assert!(!in_compete, "a nearly-empty queue within 5 RTTs must switch back");
    assert_eq!(delta, base, "switch-back must reset δ to the base");
}

#[test]
fn compete_gate_off_never_switches() {
    // RWM_COPA_COMPETE unset (the shipped default): the identical
    // never-draining queue must not flip modes or touch δ.
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    path.force_wire_for_test(0.005);
    compete_warmup(&mut path, &clock);
    for _ in 0..40 {
        queue_update(&mut path, &clock);
    }
    let (on, in_compete, switches, delta, base) = path.copa_compete_diag();
    assert!(!on && !in_compete && switches == 0, "gate off ⇒ no switching");
    assert_eq!(delta, base, "gate off ⇒ δ stays the hint base");
}

#[test]
fn compete_env_gate_requires_wire() {
    // The δ adaptation composes with the wire update law only.
    assert!(copa_compete_from_env(true, true));
    assert!(!copa_compete_from_env(true, false));
    assert!(!copa_compete_from_env(false, true));
    assert!(!copa_compete_from_env(false, false));
}

#[test]
fn wire_mode_off_is_byte_identical_legacy() {
    // Env fully unset in the test process ⇒ wire mode off ⇒ the
    // application-echo dynamics: steady state is the additive +2.
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    for _ in 0..4 {
        path.record_rtt_sample(millis(20));
        clock.advance(millis(30));
        let c = path.cwnd;
        path.on_ack(c);
    }
    // End the ramp with an inflated window.
    for _ in 0..4 {
        path.record_rtt_sample(millis(80));
    }
    clock.advance(millis(100));
    path.on_ack(4);
    let after = path.cwnd;
    path.record_rtt_sample(millis(20));
    clock.advance(millis(100));
    path.on_ack(4);
    assert_eq!(
        path.cwnd,
        after + ADDITIVE_STEP as u32,
        "legacy steady state must remain the additive +2"
    );
}

// ── ack-merge counter re-homing (`RWM_ACK_MERGE`) ──

/// A tiny model of the receiver's `PathBatchTracker`: the source of the
/// cumulative counters, and the source of the per-batch `Ack`
/// payload. Both come from the same accumulator, which is what makes the
/// equivalence law below a statement about the wire and not about
/// arithmetic.
#[derive(Default)]
struct TrackerModel {
    cum_expected: u64,
    cum_received: u64,
}
impl TrackerModel {
    /// Returns the per-batch Ack's `(expected_count, received_count)`.
    fn record_batch(&mut self, expected: u32, received: u32) -> (u32, u32) {
        self.cum_expected += expected as u64;
        self.cum_received += received as u64;
        (expected, received)
    }
}

/// The consumer-equivalence law: over a randomized ack/loss trace in which
/// an arbitrary subset of control datagrams is dropped, the totals the
/// re-homed consumers see from the merged WindowAck's cumulative counters
/// are exactly the totals the per-batch `Ack` would deliver.
///
/// This is the property the merge is safe on: the loss feed
/// (`record_batch`), the in-flight release (delivered + lost) and the
/// pool-delivery feed are all count-based, so carrying running sums and
/// diffing them loses nothing an event stream carries — and unlike an
/// event stream it is robust to ack loss, which a merged ack path must
/// be (it has half as many chances to deliver the same counts).
#[test]
fn ack_merge_counter_delta_matches_the_legacy_ack_totals_under_ack_loss() {
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    let mut tracker = TrackerModel::default();
    // Deterministic pseudo-random trace (xorshift): batch sizes, loss
    // counts, and which acks reach the sender.
    let mut rng: u64 = 0x9E3779B97F4A7C15;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let (mut legacy_expected, mut legacy_received) = (0u64, 0u64);
    let (mut merged_expected, mut merged_received) = (0u64, 0u64);
    let mut delivered_acks = 0usize;
    for _ in 0..2000 {
        let received = (next() % 32) as u32 + 1;
        // `expected >= received` always: the tracker's estimate is
        // received scaled by the batch-sequence gap.
        let gap = (next() % 4) as u32 + 1;
        let expected = received * gap;
        let (e, r) = tracker.record_batch(expected, received);
        // What the per-batch Ack delivers (it is sent for every batch, so
        // every batch counts).
        legacy_expected += e as u64;
        legacy_received += r as u64;
        // What the merged ack delivers — but only when this control
        // datagram survives the wire (~25% dropped).
        if next() % 4 != 0 {
            delivered_acks += 1;
            let (de, dr) =
                path.ack_merge_counter_delta(tracker.cum_expected, tracker.cum_received);
            merged_expected += de as u64;
            merged_received += dr as u64;
        }
    }
    // Flush: the final ack always lands (the transfer ends on one).
    let (de, dr) = path.ack_merge_counter_delta(tracker.cum_expected, tracker.cum_received);
    merged_expected += de as u64;
    merged_received += dr as u64;
    assert!(
        delivered_acks < 2000,
        "the trace must actually drop acks or it proves nothing"
    );
    assert_eq!(
        merged_expected, legacy_expected,
        "the loss feed's `expected` total must survive the merge exactly"
    );
    assert_eq!(
        merged_received, legacy_received,
        "the delivered total (in-flight release, pool feed, stats) must              survive the merge exactly"
    );
    assert!(
        merged_expected >= merged_received,
        "derived loss = expected - received must never underflow"
    );
}

/// `cum_received == 0` is the "no counter payload" sentinel used by the
/// two timer-driven WindowAck sites (hole re-advertisement, hold-expiry
/// unwedge), which broadcast one message to every live path and so cannot
/// carry a per-path counter. It must be a total no-op — including on the
/// cursor, so the next real ack still reports the whole outstanding delta.
#[test]
fn ack_merge_timer_ack_sentinel_is_inert_and_loses_no_counts() {
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    assert_eq!(path.ack_merge_counter_delta(0, 0), (0, 0));
    assert_eq!(path.ack_merge_counter_delta(100, 80), (100, 80));
    // A timer ack lands mid-stream: inert, and the cursor does not move.
    assert_eq!(path.ack_merge_counter_delta(0, 0), (0, 0));
    assert_eq!(
        path.ack_merge_counter_delta(150, 130),
        (50, 50),
        "the counts the timer ack could not carry are still delivered next"
    );
}

/// Duplicated and reordered acks are idempotent: the cursor only moves
/// forward, so a stale ack contributes nothing and cannot double-charge
/// the loss feed or double-release in-flight budget.
#[test]
fn ack_merge_counter_delta_is_idempotent_under_duplication_and_reorder() {
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    assert_eq!(path.ack_merge_counter_delta(200, 180), (200, 180));
    assert_eq!(
        path.ack_merge_counter_delta(200, 180),
        (0, 0),
        "a duplicate ack must be a no-op"
    );
    assert_eq!(
        path.ack_merge_counter_delta(120, 100),
        (0, 0),
        "a REORDERED (stale) ack must be a no-op, not a negative delta"
    );
    assert_eq!(
        path.ack_merge_counter_delta(260, 220),
        (60, 40),
        "and the cursor is still at the newest point, not the stale one"
    );
}

/// The fed loss is never negative, and it is never more than the counters
/// say: cumulative fed loss == cumulative `(expected − received)` once the
/// carried credit has drained. A delta where `received` leads `expected`
/// (a late arrival the tracker credits back) feeds zero loss and HOLDS the
/// excess; the next deltas' loss is reduced by it. The per-ack returned
/// pair keeps `received <= expected`, so the release (`received` +
/// `expected − received`) is still exactly `expected` per ack.
#[test]
fn ack_merge_counter_delta_never_lets_received_exceed_expected() {
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    let (e, r) = path.ack_merge_counter_delta(10, 40);
    assert_eq!(e, 10);
    assert_eq!(r, 10, "the returned received never exceeds expected");
    assert!(e >= r);
    // The 30 the receiver led by is held, not discarded: the next 30 of
    // loss the counters show is already paid for.
    let (e, r) = path.ack_merge_counter_delta(60, 60);
    assert_eq!((e, r), (50, 50), "30 raw loss, exactly covered by the held credit");
    let (e, r) = path.ack_merge_counter_delta(100, 90);
    assert_eq!((e, r), (40, 30), "10 raw loss, no credit left: 10 fed");
    // Cumulative fed loss 0 + 0 + 10 == cumulative counters 100 − 90.
}

/// S10 / F2 — the red trace: a late-arrival delta sequence where the
/// clamp over-counts. The tracker (F1) reads two holes, then the two late
/// arrivals fill them (a delta with received > expected), then one true
/// loss. The counters end at (30, 28): cumulative loss 2. The clamp threw
/// the late arrivals' credit away and fed 4.
#[test]
fn s10_ack_merge_counter_delta_carries_the_late_arrival_credit() {
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    let mut fed_loss = 0u64;
    for (ce, cr) in [(10u64, 8u64), (10, 10), (20, 20), (30, 28)] {
        let (e, r) = path.ack_merge_counter_delta(ce, cr);
        assert!(r <= e, "a fed pair never has received > expected");
        fed_loss += (e - r) as u64;
    }
    assert_eq!(fed_loss, 30 - 28, "cumulative fed loss == cumulative (expected − received)");
}

/// S10 / F2 — the sender-truth twin carries the same credit: two clocks
/// (the sender's own count, the receiver's echo) let `received` lead
/// `sent` on one ack; the clamp rectified it to zero and over-fed the next.
#[test]
fn s10_sender_truth_loss_delta_carries_the_credit() {
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    let mut fed_loss = 0u64;
    for (sent, cr) in [(10u64, 12u64), (20, 18)] {
        let (e, r) = path.sender_truth_loss_delta(sent, cr);
        assert!(r <= e);
        fed_loss += (e - r) as u64;
    }
    assert_eq!(fed_loss, 20 - 18, "cumulative fed loss == Σsent − Σreceived");
}

// ===================================================================
// The pinned placement cost tables
//
// The placement arms (derived temperature `T = (sqrt6/pi) sigma_e/ref`,
// the HOL term, the derived `w_div`) are absent by default, and with the
// arms off the shipped law must be unchanged. That is provable from a
// table of hand-built states and the cost vectors the engine returns for
// them, to 1e-12: an edit that moves any number in it has changed the
// shipped law. Two tables pin the wire base (paper §5.7): the echo table
// below (no wire RTT anywhere, so w^_i = s_i, the chain's last link) and
// the wired table with the wire-clocked values (further down).
// `place_costs` is `pub(crate)` for this pin; nothing else reads it.
// ===================================================================

/// Build one hand-specified scheduler state. `paths` is
/// `(id, rtt_ms, cwnd, in_flight)`; an `rtt_ms` of `None` leaves the
/// path cold (no RTT sample ever), which is the state the two cold
/// prices bind in.
fn cost_state(
    hint: ProtocolHint,
    paths: &[(PathId, Option<u64>, u32, u32)],
) -> Scheduler {
    let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), hint);
    for &(id, rtt_ms, cwnd, infl) in paths {
        sched.add_path(id);
        if let Some(ms) = rtt_ms {
            set_rtt(&mut sched, id, ms);
        }
        let p = sched.path_mut(id).unwrap();
        p.cwnd = cwnd;
        p.in_flight = infl;
    }
    sched
}

/// The eight states of the table, in one place so the capture harness
/// and the pin can never describe different states.
#[allow(clippy::type_complexity)]
fn cost_table_states() -> Vec<(&'static str, Scheduler, bool, Vec<PathId>)> {
    vec![
        // 1. The single-path collapse.
        ("single-idle", cost_state(ProtocolHint::Auto, &[(0, Some(10), 10, 0)]), false, vec![]),
        // 2. Two symmetric idle paths: the load term alone, equal.
        (
            "dual-symmetric-idle",
            cost_state(ProtocolHint::Auto, &[(0, Some(10), 10, 0), (1, Some(10), 10, 0)]),
            false,
            vec![],
        ),
        // 3. The 5x RTT asymmetry (the c8 shape).
        (
            "dual-asymmetric-rtt",
            cost_state(ProtocolHint::Auto, &[(0, Some(10), 10, 0), (1, Some(50), 10, 0)]),
            false,
            vec![],
        ),
        // 4. The capacity-proportional water-filling fixed point.
        (
            "dual-waterfill",
            cost_state(ProtocolHint::Bulk, &[(0, Some(10), 20, 8), (1, Some(10), 10, 4)]),
            false,
            vec![],
        ),
        // 5. Overdraft: in_flight past cwnd. The term climbs past 1.0
        //    continuously; no path is ever removed.
        (
            "dual-overdraft",
            cost_state(ProtocolHint::Bulk, &[(0, Some(10), 10, 25), (1, Some(10), 10, 0)]),
            false,
            vec![],
        ),
        // 6. A repair with the fate-diversity term live.
        (
            "repair-fate",
            cost_state(ProtocolHint::Auto, &[(0, Some(10), 10, 0), (1, Some(10), 10, 0)]),
            true,
            vec![0, 0, 0, 1],
        ),
        // 7. The cold path: no RTT sample ever, so `srtt()` hands back
        //    the seed and both cold prices bind.
        (
            "cold-path",
            cost_state(ProtocolHint::Auto, &[(0, Some(10), 10, 0), (1, None, 10, 0)]),
            false,
            vec![],
        ),
        // 8. Three paths added out of id order -- the deterministic
        //    tie-break's own witness.
        (
            "triple-out-of-order",
            cost_state(
                ProtocolHint::Realtime,
                &[(2, Some(30), 10, 3), (0, Some(10), 10, 1), (1, Some(20), 10, 2)],
            ),
            false,
            vec![],
        ),
    ]
}

/// The capture harness for the echo table. Ignored by default -- run it with
/// `cargo test -p raptorpath --release -- --ignored capture_place_cost_table
/// --nocapture` to print the table in the exact literal form the pin
/// below expects. It is the only sanctioned way to move the table, and
/// moving it is a declaration that the shipped law changed.
#[test]
#[ignore]
fn capture_place_cost_table() {
    for (name, sched, is_repair, covered) in cost_table_states() {
        let costs = sched.place_costs(is_repair, &covered);
        let body = costs
            .iter()
            .map(|(id, c)| format!("({id}, {c:.17e})"))
            .collect::<Vec<_>>()
            .join(", ");
        println!("            (\"{name}\", &[{body}]),");
    }
}

/// The echo pin: the engine's `place_costs`, to 1e-12, over the eight
/// states above with no wire RTT mirrored (w^_i = s_i) -- and the order, which the deterministic
/// tie-break makes reproducible (state 8 adds its paths 2, 0, 1 and
/// must still return 0, 1, 2).
const PINNED_ECHO_COST_TABLE: &[(&str, &[(PathId, f64)])] = &[
    ("single-idle", &[(0, 5.00000000000000000e-1)]),
    ("dual-symmetric-idle", &[(0, 5.00000000000000000e-1), (1, 5.00000000000000000e-1)]),
    ("dual-asymmetric-rtt", &[(0, 5.00000000000000000e-1), (1, 2.50000000000000000e0)]),
    ("dual-waterfill", &[(0, 9.00000000000000133e-1), (1, 9.00000000000000133e-1)]),
    ("dual-overdraft", &[(0, 3.00000000000000000e0), (1, 5.00000000000000000e-1)]),
    ("repair-fate", &[(0, 1.25000000000000000e0), (1, 7.50000000000000000e-1)]),
    ("cold-path", &[(0, 5.00000000000000000e-1), (1, 2.50000000000000000e0)]),
    ("triple-out-of-order", &[(0, 5.99999999999999978e-1), (1, 1.39999999999999991e0), (2, 2.39999999999999991e0)]),
];

#[test]
fn place_costs_with_no_wire_rtt_price_the_echo_srtt() {
    let states = cost_table_states();
    assert_eq!(states.len(), PINNED_ECHO_COST_TABLE.len(), "the table lost a state");
    for ((name, sched, is_repair, covered), (pname, expect)) in
        states.into_iter().zip(PINNED_ECHO_COST_TABLE.iter())
    {
        assert_eq!(name, *pname, "the table was reordered");
        let got = sched.place_costs(is_repair, &covered);
        assert_eq!(
            got.len(),
            expect.len(),
            "state `{name}`: {} candidates, table says {}",
            got.len(),
            expect.len()
        );
        for (i, ((gid, gc), (eid, ec))) in got.iter().zip(expect.iter()).enumerate() {
            assert_eq!(
                gid, eid,
                "state `{name}` slot {i}: candidate ORDER moved -- the \
                 deterministic tie-break is gone"
            );
            assert!(
                (gc - ec).abs() <= 1e-12,
                "state `{name}` path {gid}: cost {gc:.17e} != pinned {ec:.17e} \
                 (delta {:.3e}) -- THE PLACEMENT LAW MOVED",
                (gc - ec).abs()
            );
        }
    }
}

/// The tie-break is ascending by id and touches no probability: the
/// distribution over a symmetric pair is still exactly 1/2 each, and
/// the candidate list is sorted whatever order the paths were added
/// in. (A `HashMap`'s iteration order is the hasher's, so without the
/// sort this assertion fails at random between processes.)
#[test]
fn place_costs_are_sorted_by_id_and_the_probabilities_are_untouched() {
    let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
    for id in [7, 3, 9, 1, 5] {
        sched.add_path(id);
        set_rtt(&mut sched, id, 10);
    }
    let ids: Vec<PathId> = sched.place_costs(false, &[]).into_iter().map(|(i, _)| i).collect();
    assert_eq!(ids, vec![1, 3, 5, 7, 9], "candidates must be ascending by id");
    let dist = sched.place_probs(false, &[]);
    assert_eq!(dist.iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![1, 3, 5, 7, 9]);
    for (id, p) in &dist {
        assert!((p - 0.2).abs() < 1e-12, "symmetric paths must split exactly: p{id}={p}");
    }
}

// ===================================================================
// The placement arms (paper §5.7, all absent by default)
//
// The order of these tests is the order of the claims they defend:
// (1) with every arm absent the shipped law is byte-identical, in costs
// and in probabilities; (2) each arm's own limits are the ones the
// derivation states; (3) the shape checks -- knee continuity,
// monotonicity, dial continuity -- hold on the law itself; (4) the
// controls (N = 1, the tie-break) still hold with everything armed.
// ===================================================================

/// Gate-off identity, the probability side. The pinned tables assert the
/// costs; this asserts that the softmax over them is the plain softmax at
/// the shipped temperature, to 1e-12 -- because the arms enter through the
/// temperature as well as through the cost, and a temperature change
/// leaves every cost fixed while moving every probability.
#[test]
fn every_arm_absent_leaves_the_probabilities_untouched() {
    for (name, mut sched, is_repair, covered) in cost_table_states() {
        sched.set_place_wdiv_derived(false);
        let got = sched.place_probs(is_repair, &covered);
        let want =
            sched.place_probs_with_temperature(is_repair, &covered, PLACE_TEMPERATURE);
        assert_eq!(got.len(), want.len(), "state `{name}`");
        for ((gid, gp), (wid, wp)) in got.iter().zip(want.iter()) {
            assert_eq!(gid, wid, "state `{name}`: candidate order moved");
            assert!(
                (gp - wp).abs() <= 1e-12,
                "state `{name}` path {gid}: p={gp:.17e} != shipped {wp:.17e} \
                 -- AN ABSENT ARM MOVED THE SHIPPED LAW"
            );
        }
    }
}

/// A placement arm reads the one strict boolean dialect: a typo is a
/// startup error naming the gate, never a silently different row.
#[test]
#[should_panic(expected = "RWM_TEST_PLACE_ARM_GARBAGE")]
fn a_garbage_arm_value_is_an_error_naming_the_gate() {
    std::env::set_var("RWM_TEST_PLACE_ARM_GARBAGE", "of");
    let _ = place_arm_flag("RWM_TEST_PLACE_ARM_GARBAGE");
}

/// The arms are absent by default. A fresh scheduler in a clean
/// environment carries them all off, which is what makes the pinned
/// tables an oracle for every other test in this file.
#[test]
fn the_placement_arms_are_absent_by_default() {
    let sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
    assert!(!sched.place_wdiv_derived(), "RWM_PLACE_WDIV_DERIVED ships ABSENT");
}

/// Arm 3 cannot move a source placement, by construction: `fate_i` is
/// identically zero for source symbols, so the derived diversity weight
/// multiplies zero. Every source row of the pinned table therefore stands
/// with the arm armed -- which is what makes the arm attributable to
/// repairs alone.
#[test]
fn the_derived_diversity_weight_leaves_every_source_placement_alone() {
    for (name, mut sched, is_repair, covered) in cost_table_states() {
        if is_repair {
            continue;
        }
        let shipped = sched.place_costs(false, &covered);
        sched.set_place_wdiv_derived(true);
        let armed = sched.place_costs(false, &covered);
        assert_eq!(armed, shipped, "state `{name}`: arm 3 moved a SOURCE placement");
    }
}

/// Arm 3's own limit, asserted absolutely. On a memoryless channel the
/// bad->bad persistence equals the marginal loss, so the excess
/// `(p_BB - eps)+` -- and with it the whole diversity charge -- collapses,
/// which the shipped `w_div = 1.0` does not do. The assertion is on the
/// term's own arithmetic read off the path's estimator, not on an ordinal
/// comparison, and it is checked against the shipped charge it replaces.
#[test]
fn the_derived_diversity_weight_collapses_on_a_memoryless_channel() {
    let mut sched = cost_state(
        ProtocolHint::Auto,
        &[(0, Some(10), 10, 0), (1, Some(10), 10, 0)],
    );
    // Independent 2 % loss on both legs -- no burst structure at all.
    for pid in [0u32, 1] {
        let p = sched.path_mut(pid).unwrap();
        for i in 0..4000u32 {
            p.estimator.record_symbol(i % 50 != 0);
        }
    }
    let (excess, srtt_i) = {
        let p = sched.path(0).unwrap();
        let ge = p.estimator.ge_estimator();
        assert!(ge.is_valid(), "the fixture must produce a VALID GE estimate");
        ((1.0 - ge.p_bg() - p.estimator.loss_rate()).max(0.0), 0.010_f64)
    };
    let ref_srtt = 0.010_f64;
    let shipped = sched.place_costs(true, &[0, 0, 0, 0]);
    sched.set_place_wdiv_derived(true);
    let armed = sched.place_costs(true, &[0, 0, 0, 0]);
    // Path 0 carries all four covered symbols (fate = 1), path 1 none.
    // The derived charge is the excess, priced at srtt/ref; the shipped
    // one is a flat w_div = 1.0.
    let derived_gap = armed[0].1 - armed[1].1;
    assert!(
        (derived_gap - excess * srtt_i / ref_srtt).abs() < 1e-12,
        "the diversity charge must be exactly fate*(p_BB-eps)+*srtt/ref              (excess {excess}, got {derived_gap})"
    );
    let shipped_gap = shipped[0].1 - shipped[1].1;
    assert!((shipped_gap - 1.0).abs() < 1e-12, "the shipped charge is the flat 1.0");
    assert!(
        derived_gap < 0.25,
        "a memoryless channel must price essentially NO diversity,              where the shipped law prices a full round: {derived_gap}"
    );
}

/// `RWM_PLACE_WDIV` is a parameter of the one law. Two symmetric idle legs,
/// a SACK-gap retransmit whose original went out on leg 0
/// (`place_symbol(true, &[0])`):
///
/// * `w_div = 1.0` (the shipped weight, `None`): the covered leg costs
///   exactly 1.0 more, so P(0) = 1 / (1 + e^(1/T)) with T = 0.15, i.e.
///   0.0012710 -- the number behind "82 % of c8's fast-leg holes go to the
///   slow leg". Pinned absolutely, not ordinally.
/// * `w_div = 0`: no diversity term; the retransmit is placed with exactly a
///   fresh source symbol's probabilities (0.5 / 0.5 here).
/// * `w_div = 0.5`: the gap is exactly 0.5 -- the weight is a continuous
///   dial, not a switch.
#[test]
fn place_wdiv_is_the_diversity_weight_of_a_sack_gap_retransmit() {
    let mk = |w: Option<f64>| {
        let mut s = cost_state(
            ProtocolHint::Auto,
            &[(0, Some(10), 10, 0), (1, Some(10), 10, 0)],
        );
        s.set_place_wdiv(w);
        s
    };
    let t = PLACE_TEMPERATURE;
    let p0 = |gap: f64| 1.0 / (1.0 + (gap / t).exp());

    // Unset: shipped, exactly 1.0 and today's numbers.
    let shipped = mk(None);
    assert_eq!(shipped.w_div(), 1.0, "unset must resolve to exactly the shipped w_div");
    let d = shipped.place_probs(true, &[0]);
    assert!((d[0].1 - 0.001_271_0).abs() < 1e-7, "shipped retransmit P(covered leg) = {}", d[0].1);
    assert!((d[0].1 - p0(1.0)).abs() < 1e-12);
    assert!((d[1].1 - (1.0 - p0(1.0))).abs() < 1e-12);

    // Zero: the retransmit is placed as a fresh source symbol is.
    let zero = mk(Some(0.0));
    let src = zero.place_probs(false, &[]);
    let rtx = zero.place_probs(true, &[0]);
    assert_eq!(rtx, src, "w_div = 0 must remove the diversity term entirely");
    assert!((rtx[0].1 - 0.5).abs() < 1e-12 && (rtx[1].1 - 0.5).abs() < 1e-12);

    // Explicit 1.0 is the shipped law bit for bit.
    assert_eq!(mk(Some(1.0)).place_probs(true, &[0]), shipped.place_probs(true, &[0]));

    // Continuous in between.
    let half = mk(Some(0.5)).place_probs(true, &[0]);
    assert!((half[0].1 - p0(0.5)).abs() < 1e-12, "w_div = 0.5: {}", half[0].1);

    // A source symbol never sees the weight (fate_i == 0).
    assert_eq!(mk(Some(7.0)).place_probs(false, &[]), mk(None).place_probs(false, &[]));
}

/// The `N = 1` control, with every arm on. A softmax over a singleton
/// is 1 at every temperature and every cost, so the law is the identity on
/// a single path -- which is why single-path cells are controls for every
/// placement arm.
#[test]
fn a_single_path_is_the_identity_with_every_arm_armed() {
    let mut sched = cost_state(ProtocolHint::Auto, &[(0, Some(10), 10, 3)]);
    sched.set_place_wdiv_derived(true);
    let dist = sched.place_probs(false, &[]);
    assert_eq!(dist.len(), 1);
    assert_eq!(dist[0].0, 0);
    assert_eq!(dist[0].1, 1.0, "a singleton candidate set must place at probability 1");
    assert_eq!(sched.place_symbol(false, &[]), Some(0));
}

/// The deterministic tie-break survives every arm. The arms add terms
/// to the cost and change the temperature; neither may touch the ascending
/// candidate order the pinned table rests on.
#[test]
fn the_tie_break_is_still_ascending_by_id_with_every_arm_armed() {
    let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
    for id in [7, 3, 9, 1, 5] {
        sched.add_path(id);
        set_rtt(&mut sched, id, 10);
    }
    sched.set_place_wdiv_derived(true);
    let ids: Vec<PathId> =
        sched.place_costs(false, &[]).into_iter().map(|(i, _)| i).collect();
    assert_eq!(ids, vec![1, 3, 5, 7, 9], "candidates must stay ascending by id");
    let dist = sched.place_probs(false, &[]);
    assert_eq!(dist.iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![1, 3, 5, 7, 9]);
}

/// The cold-price bind gauges count and never decide. State 7 has one
/// cold path, so one of its two cost evaluations pays both cold prices
/// -- and the costs themselves are the pinned ones either way.
#[test]
fn the_cold_price_binds_are_counted_and_change_no_cost() {
    let mut sched =
        cost_state(ProtocolHint::Auto, &[(0, Some(10), 10, 0), (1, None, 10, 0)]);
    let before = sched.place_costs(false, &[]);
    sched.drain_place_bind();
    let l = sched.eta().line();
    // Two per-path evaluations, one of them cold on both prices.
    assert!(l.contains("place_n=2"), "{l}");
    assert!(l.contains("cold_ge=0.5000"), "one of two paths has no SRTT sample: {l}");
    // Reading the gauge cannot have moved the law.
    let after = sched.place_costs(false, &[]);
    assert_eq!(before, after);
}

// ===================================================================
// The wire-clocked placement base (paper §5.7 "The corrected price",
// status §38/§41b: the shipped law)
//
//     E_i  =  (I_i/W_i)·s_i  +  (1/2 + eps_i)·w^_i
//     w^_i =  wire_i  ?:  min_j wire_j  ?:  s_i
//     cost_i = E_i/ref + w_bw·r_i + V_i      ref = max(min_j s_j, 1 ms)
//
// Every state feeds Copa through `record_rtt_sample` under a `MockClock`
// (`set_rtt` feeds only the estimator and leaves Copa's EWMA empty, so it
// cannot build the frozen-srtt state), and every assertion is an absolute
// value from the formula above. The frozen-srtt base this law replaced
// (`w^_i := s_i`, paper §10) survives only as a reference value in the
// tests that bound the shift against it.
// ===================================================================

fn dur_ms(ms: f64) -> Duration {
    Duration::from_secs_f64(ms * 1e-3)
}

/// One hand-built leg. `s_ms` is fed to Copa as 80 identical samples;
/// `lost` of every 100 symbols is fed once to the loss estimator (the EWMA
/// then reads `0.1 · lost/100`); `wire_ms` is the mirrored quinn RTT.
#[derive(Clone, Copy)]
struct WLeg {
    id: PathId,
    s_ms: f64,
    wire_ms: Option<f64>,
    lost: u32,
    cwnd: u32,
    in_flight: u32,
}

fn wleg(id: PathId, s_ms: f64, wire_ms: Option<f64>) -> WLeg {
    WLeg { id, s_ms, wire_ms, lost: 0, cwnd: 100, in_flight: 0 }
}

fn wire_state(hint: ProtocolHint, legs: &[WLeg]) -> Scheduler {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new_with_hint(clock, hint);
    for l in legs {
        sched.add_path(l.id);
        let p = sched.path_mut(l.id).unwrap();
        for _ in 0..80 {
            p.record_rtt_sample(dur_ms(l.s_ms));
        }
        if let Some(w) = l.wire_ms {
            p.set_wire_rtt(dur_ms(w));
        }
        if l.lost > 0 {
            p.estimator.record_batch(100, 100 - l.lost);
        }
        p.cwnd = l.cwnd;
        p.in_flight = l.in_flight;
    }
    sched
}

fn s_of(sched: &Scheduler, id: PathId) -> f64 {
    sched.path(id).unwrap().srtt_measured().unwrap().as_secs_f64()
}

fn eps_of(sched: &Scheduler, id: PathId) -> f64 {
    sched.path(id).unwrap().estimator.loss_rate()
}

fn cost_of_leg(sched: &Scheduler, id: PathId) -> f64 {
    sched
        .place_costs(false, &[])
        .into_iter()
        .find(|(p, _)| *p == id)
        .unwrap()
        .1
}

fn prob_of_leg(sched: &Scheduler, id: PathId) -> f64 {
    sched
        .place_probs(false, &[])
        .into_iter()
        .find(|(p, _)| *p == id)
        .unwrap()
        .1
}

/// The law's cost for one leg, straight from the paper's formula.
fn wire_formula(w_bw: f64, i_over_w: f64, s: f64, w_hat: f64, eps: f64, ref_s: f64) -> f64 {
    (i_over_w * s + (0.5 + eps) * w_hat) / ref_s + w_bw * (eps / (1.0 - eps))
}

/// The frozen-srtt base the wire base replaced (`w^_i := s_i`, paper §10),
/// as a reference value only: the engine no longer computes it.
fn frozen_formula(w_bw: f64, i_over_w: f64, s: f64, eps: f64, ref_s: f64) -> f64 {
    wire_formula(w_bw, i_over_w, s, s, eps, ref_s)
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// The p34 c1d r2 shape: leg 1 gets 2.5 ms floor samples, then a spike
/// to 156 ms (its own backlog dwell), then five seconds of silence on the
/// mock clock. Leg 0 is active at 9 ms. Quinn's wire RTT: 3 ms and 9 ms.
fn idle_leg_state(i0_over_w0: f64) -> Scheduler {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new_with_hint(clock.clone(), ProtocolHint::Auto);
    sched.add_path(0);
    sched.add_path(1);
    {
        let p = sched.path_mut(1).unwrap();
        for _ in 0..40 {
            p.record_rtt_sample(dur_ms(2.5));
        }
        for _ in 0..30 {
            p.record_rtt_sample(dur_ms(156.0));
        }
        p.set_wire_rtt(dur_ms(3.0));
        p.cwnd = 10;
        p.in_flight = 0;
    }
    clock.advance(Duration::from_secs(5));
    {
        let p = sched.path_mut(0).unwrap();
        for _ in 0..40 {
            p.record_rtt_sample(dur_ms(9.0));
        }
        p.set_wire_rtt(dur_ms(9.0));
        p.cwnd = 10;
        p.in_flight = (i0_over_w0 * 10.0).round() as u32;
    }
    sched
}

/// The wired pin: the engine's `place_costs` over the eight table states with
/// every measured leg's wire RTT mirrored at 0.8·s_i (the unmeasured leg of
/// `cold-path` has none, so it takes `min_j wire_j`). The numbers are the
/// formula's, worked by hand (E_i = (I_i/W_i)·s_i + (1/2)·0.8·s_i, ref =
/// fastest s_i, eps = 0, w_bw·r = 0), not captured from the engine; the
/// repair row adds the flat `w_div·fate` of 0.75 / 0.25.
const PINNED_WIRED_COST_TABLE: &[(&str, &[(PathId, f64)])] = &[
    ("single-idle", &[(0, 0.4)]),
    ("dual-symmetric-idle", &[(0, 0.4), (1, 0.4)]),
    ("dual-asymmetric-rtt", &[(0, 0.4), (1, 2.0)]),
    ("dual-waterfill", &[(0, 0.8), (1, 0.8)]),
    ("dual-overdraft", &[(0, 2.9), (1, 0.4)]),
    ("repair-fate", &[(0, 1.15), (1, 0.65)]),
    ("cold-path", &[(0, 0.4), (1, 0.4)]),
    ("triple-out-of-order", &[(0, 0.5), (1, 1.2), (2, 2.1)]),
];

fn wired_table_states() -> Vec<(&'static str, Scheduler, bool, Vec<PathId>)> {
    let mut states = cost_table_states();
    for (_, sched, _, _) in &mut states {
        for id in sched.all_path_ids() {
            let p = sched.path_mut(id).unwrap();
            if let Some(s) = p.srtt_measured() {
                p.set_wire_rtt(s.mul_f64(0.8));
            }
        }
    }
    states
}

#[test]
fn place_costs_match_the_pinned_wired_table() {
    let states = wired_table_states();
    assert_eq!(states.len(), PINNED_WIRED_COST_TABLE.len());
    for ((name, sched, is_repair, covered), (pname, expect)) in
        states.into_iter().zip(PINNED_WIRED_COST_TABLE.iter())
    {
        assert_eq!(name, *pname);
        let got = sched.place_costs(is_repair, &covered);
        assert_eq!(got.len(), expect.len(), "state `{name}`");
        for ((gid, gc), (eid, ec)) in got.iter().zip(expect.iter()) {
            assert_eq!(gid, eid, "state `{name}`: candidate order moved");
            assert!(
                (gc - ec).abs() <= 1e-12,
                "state `{name}` path {gid}: cost {gc:.17e} != formula {ec:.17e}"
            );
        }
    }
}

/// With `wire_i := s_i` on every leg the wire base is the echo-srtt price,
/// the last link of the availability chain, so the table of
/// `place_costs_with_no_wire_rtt_price_the_echo_srtt` holds again to 1e-12.
#[test]
fn wire_equal_to_echo_prices_the_echo_srtt() {
    let states = cost_table_states();
    assert_eq!(states.len(), PINNED_ECHO_COST_TABLE.len());
    for ((name, mut sched, is_repair, covered), (pname, expect)) in
        states.into_iter().zip(PINNED_ECHO_COST_TABLE.iter())
    {
        assert_eq!(name, *pname);
        for id in sched.all_path_ids() {
            let p = sched.path_mut(id).unwrap();
            let s = p.srtt();
            p.set_wire_rtt(s);
        }
        let got = sched.place_costs(is_repair, &covered);
        assert_eq!(got.len(), expect.len(), "state `{name}`");
        for ((gid, gc), (eid, ec)) in got.iter().zip(expect.iter()) {
            assert_eq!(gid, eid, "state `{name}`: candidate order moved");
            assert!(
                (gc - ec).abs() <= 1e-12,
                "state `{name}` path {gid}: cost {gc:.17e} != {ec:.17e} with wire_i = s_i"
            );
        }
    }
}

/// Defect pinned and cured: an idle leg whose echo srtt froze at its own
/// dwell is priced at its wire transit, not at the frozen dwell.
#[test]
fn idle_leg_price_is_its_wire_transit_not_its_frozen_srtt() {
    let sched = idle_leg_state(0.0);
    let (s0, s1) = (s_of(&sched, 0), s_of(&sched, 1));
    assert!(s1 >= 0.100, "the fixture must freeze s_1 at the spike's dwell: {s1}");
    assert!((s0 - 0.009).abs() < 1e-6, "leg 0 is active at 9 ms: {s0}");
    let ref_s = s0; // min_j s_j, above the 1 ms floor
    let eps1 = eps_of(&sched, 1);
    assert_eq!(eps1, 0.0);

    // (1/2 + eps_1)·3 ms / ref, to 1e-12; = 1/6 on these numbers.
    let want = (0.5 + eps1) * 0.003 / ref_s;
    let got = cost_of_leg(&sched, 1);
    assert!((got - want).abs() < 1e-12, "idle leg: {got} != {want}");
    assert!((want - 0.5 * 3.0 / 9.0).abs() < 1e-6, "= 1/6 on these numbers");

    // The frozen-srtt reference price (the defect): (1/2)·s_1 / ref, > 5.
    let frozen = frozen_formula(0.0, 0.0, s1, eps1, ref_s);
    assert!(frozen > 5.0 && got < frozen / 25.0, "got {got}, frozen reference {frozen}");
}

/// The absorbing fixed point is gone: leg 0 carrying I/W = 0.6, leg 1 idle.
/// Leg 1 takes at least half; the frozen-srtt reference puts it below 1e-15.
#[test]
fn idle_leg_is_not_absorbing_at_symmetric_legs() {
    let sched = idle_leg_state(0.6);

    // Absolute: c_0 = (0.6·s_0 + 1/2·9 ms)/s_0 = 1.1, c_1 = 1/6.
    let ref_s = s_of(&sched, 0);
    let c0 = (0.6 * ref_s + 0.5 * 0.009) / ref_s;
    let c1 = 0.5 * 0.003 / ref_s;
    let want = sigmoid((c0 - c1) / PLACE_TEMPERATURE);
    let got = prob_of_leg(&sched, 1);
    assert!(got >= 0.5, "P(leg 1) = {got} < 1/2");
    assert!((got - want).abs() < 1e-12, "P(leg 1) {got} != sigma(gap/T) {want}");

    let s1 = s_of(&sched, 1);
    let frozen_gap = frozen_formula(1.0, 0.0, s1, 0.0, ref_s) - c0;
    assert!(sigmoid(-frozen_gap / PLACE_TEMPERATURE) < 1e-15, "the frozen reference absorbs");
}

/// Equal state ⇒ exactly 1/N, at three loads, 2 and 3 legs.
#[test]
fn symmetric_legs_at_equal_state_split_exactly_half() {
    for n in [2u32, 3] {
        for (cwnd, infl) in [(10u32, 0u32), (10, 5), (10, 15)] {
            let legs: Vec<WLeg> = (0..n as PathId)
                .map(|id| WLeg { lost: 20, cwnd, in_flight: infl, ..wleg(id, 10.0, Some(4.0)) })
                .collect();
            let sched = wire_state(ProtocolHint::Auto, &legs);
            for (id, p) in sched.place_probs(false, &[]) {
                assert!(
                    (p - 1.0 / n as f64).abs() < 1e-12,
                    "N={n} I/W={}: p{id}={p}",
                    infl as f64 / cwnd as f64
                );
            }
        }
    }
}

/// The p27 c8 DEF128 s42 state: s = 16/45 ms, w^ = 14/43 ms,
/// eps = 0.026/0.017, I_0/W_0 = 0.38, I_1 = 0, Bulk. The wire base may move
/// the slow leg's log-probability against the frozen-srtt reference by no
/// more than the open-loop shift `dg` of paper §5.7, which is 0.007 nats here.
#[test]
fn c8_fresh_state_share_is_unchanged() {
    let legs = [
        WLeg { lost: 26, in_flight: 38, ..wleg(0, 16.0, Some(14.0)) },
        WLeg { lost: 17, ..wleg(1, 45.0, Some(43.0)) },
    ];
    let sched = wire_state(ProtocolHint::Bulk, &legs);
    let (s0, s1) = (s_of(&sched, 0), s_of(&sched, 1));
    let (e0, e1) = (eps_of(&sched, 0), eps_of(&sched, 1));
    assert!((e0 - 0.026).abs() < 1e-12 && (e1 - 0.017).abs() < 1e-12);
    let ref_s = s0.min(s1);
    let dg = ((0.5 + e1) * (0.043 - s1) - (0.5 + e0) * (0.014 - s0)) / (ref_s * PLACE_TEMPERATURE);
    assert!((dg.abs() - 0.0075).abs() < 5e-4, "the paper's open-loop shift is 0.007: {dg}");
    // P1 against the frozen reference: sigma(-(c1 - c0)/T) for each base.
    let w_bw = 1.0; // Bulk
    let p1 = |c0: f64, c1: f64| sigmoid(-(c1 - c0) / PLACE_TEMPERATURE);
    let p_ref = p1(
        frozen_formula(w_bw, 0.38, s0, e0, ref_s),
        frozen_formula(w_bw, 0.0, s1, e1, ref_s),
    );
    let p_got = prob_of_leg(&sched, 1);
    let p_want = p1(
        wire_formula(w_bw, 0.38, s0, 0.014, e0, ref_s),
        wire_formula(w_bw, 0.0, s1, 0.043, e1, ref_s),
    );
    assert!((p_got - p_want).abs() < 1e-12, "engine {p_got} != formula {p_want}");
    let shift = (p_got / p_ref).ln().abs();
    assert!(shift <= dg.abs() + 1e-9, "|ln(P1/P1_ref)| = {shift} > |dg| = {}", dg.abs());
    // The shift is real, not a no-op of the base.
    assert!(shift > 1e-4, "the wire base did not execute: {shift}");
}

/// The p27 c8 DEF256 s42 locked state: s = 24/390 ms, w^ = 23/45 ms,
/// I_0 = I_1 = 0, eps = 0.026/0.017, Bulk. The slow leg is sampled again
/// (P >= e^-4); the frozen-srtt reference keeps it out (P < 1e-20).
///
/// Spec note: status §38 amendment 1 quotes a gap of +3.8 nats here; the
/// formula on exactly these inputs gives +3.04 (w_bw·(r_1 - r_0) = -0.063
/// nats of it is the B term), asserted below from the formula.
#[test]
fn locked_c8_leg_is_sampled_again() {
    let legs = [
        WLeg { lost: 26, ..wleg(0, 24.0, Some(23.0)) },
        WLeg { lost: 17, ..wleg(1, 390.0, Some(45.0)) },
    ];
    let sched = wire_state(ProtocolHint::Bulk, &legs);
    let ref_s = s_of(&sched, 0);
    let w_bw = 1.0; // Bulk
    let gap = (wire_formula(w_bw, 0.0, s_of(&sched, 1), 0.045, eps_of(&sched, 1), ref_s)
        - wire_formula(w_bw, 0.0, s_of(&sched, 0), 0.023, eps_of(&sched, 0), ref_s))
        / PLACE_TEMPERATURE;
    assert!((gap - 3.04).abs() < 0.01, "gap on these inputs: {gap}");
    let p = prob_of_leg(&sched, 1);
    assert!((p - sigmoid(-gap)).abs() < 1e-12, "P1 {p} != sigma(-gap) {}", sigmoid(-gap));
    assert!(p >= (-4.0f64).exp(), "the locked leg is sampled again: {p}");
    let gap_ref = (frozen_formula(w_bw, 0.0, s_of(&sched, 1), eps_of(&sched, 1), ref_s)
        - frozen_formula(w_bw, 0.0, s_of(&sched, 0), eps_of(&sched, 0), ref_s))
        / PLACE_TEMPERATURE;
    assert!(sigmoid(-gap_ref) < 1e-20, "frozen-srtt reference: the locked leg stays out");
}

/// The fixed point's premise: c_i is strictly increasing in I_i (the other
/// leg held), with slope s_i/(W_i·ref) exactly.
#[test]
fn own_cost_is_increasing_in_own_in_flight_with_the_wire_base() {
    for target in [0 as PathId, 1] {
        let mk = |infl: u32| {
            let mut legs = [
                WLeg { lost: 20, cwnd: 20, ..wleg(0, 10.0, Some(8.0)) },
                WLeg { lost: 30, cwnd: 20, ..wleg(1, 30.0, Some(25.0)) },
            ];
            legs[target as usize].in_flight = infl;
            wire_state(ProtocolHint::Auto, &legs)
        };
        let ref_s = 0.010;
        let other = 1 - target;
        let base = mk(0);
        let s_t = s_of(&base, target);
        let other_cost = cost_of_leg(&base, other);
        let mut prev = cost_of_leg(&base, target);
        for infl in 1..=40u32 {
            let sched = mk(infl);
            let c = cost_of_leg(&sched, target);
            assert!(c > prev, "leg {target}: cost not strictly increasing at I={infl}");
            let slope = s_t / (20.0 * ref_s);
            assert!(
                ((c - prev) - slope).abs() < 1e-9,
                "leg {target} I={infl}: dc = {} != s/(W·ref) = {slope}",
                c - prev
            );
            assert_eq!(cost_of_leg(&sched, other), other_cost, "the other leg's cost moved");
            prev = c;
        }
    }
}

/// `w^_i` falls back to the best measured wire, then to `s_i`; `w_cold`
/// counts each fallback.
#[test]
fn unmeasured_wire_takes_the_best_measured_wire() {
    let legs = [
        wleg(0, 20.0, Some(8.0)),
        wleg(1, 20.0, None),
        wleg(2, 20.0, Some(5.0)),
    ];
    let mut sched = wire_state(ProtocolHint::Auto, &legs);
    let ref_s = s_of(&sched, 0);
    let c1 = cost_of_leg(&sched, 1);
    assert!(
        (c1 - 0.5 * 0.005 / ref_s).abs() < 1e-12,
        "wire_1 absent must price at min_j wire_j = 5 ms: {c1}"
    );
    assert!((cost_of_leg(&sched, 0) - 0.5 * 0.008 / ref_s).abs() < 1e-12);
    assert!((cost_of_leg(&sched, 2) - 0.5 * 0.005 / ref_s).abs() < 1e-12);
    sched.drain_place_bind();
    // Three place_costs calls above, three legs each; leg 1 cold in each.
    let l = sched.eta().line();
    assert!(l.contains(" w_cold=0.3333 w_n=9 "), "w_cold counts the fallback per leg: {l}");

    // Nothing measured anywhere: w^_i = s_i, the last link of the chain.
    let none = [wleg(0, 20.0, None), wleg(1, 40.0, None)];
    let mut sched = wire_state(ProtocolHint::Auto, &none);
    let ref_s = s_of(&sched, 0);
    for (id, c) in sched.place_costs(false, &[]) {
        let want = 0.5 * s_of(&sched, id) / ref_s;
        assert!((c - want).abs() < 1e-12, "no wire anywhere prices (1/2)·s_i: p{id} {c} != {want}");
    }
    sched.drain_place_bind();
    let l = sched.eta().line();
    assert!(l.contains(" w_cold=1.0000 w_n=2 "), "{l}");
}

/// The base adds no δ dependence of its own: every leg's cost is the formula
/// with `w_bw(δ)` at every point of the dial, including ±2 % around each
/// named point (the wire base only moves the `(1/2 + eps)` term).
#[test]
fn wire_base_is_independent_of_the_dial() {
    let legs = [
        WLeg { lost: 20, in_flight: 50, ..wleg(0, 12.0, Some(11.0)) },
        WLeg { lost: 10, ..wleg(1, 40.0, Some(20.0)) },
    ];
    for h in [ProtocolHint::Realtime, ProtocolHint::Auto, ProtocolHint::Bulk] {
        for f in [0.98f64, 1.0, 1.02] {
            let mut sched = wire_state(ProtocolHint::Auto, &legs);
            let w = SchedulingWeights::from_delta(hint_delta_price(h) * f);
            sched.weights = w;
            let ref_s = s_of(&sched, 0).min(s_of(&sched, 1));
            for id in [0 as PathId, 1] {
                let (i_over_w, w_hat) = if id == 0 { (0.5, 0.011) } else { (0.0, 0.020) };
                let want = wire_formula(w.w_bw, i_over_w, s_of(&sched, id), w_hat, eps_of(&sched, id), ref_s);
                let got = cost_of_leg(&sched, id);
                assert!((got - want).abs() < 1e-12, "{h:?} x{f} leg {id}: {got} != {want}");
            }
        }
    }
}

/// The stamped `E_picked` is the `E_i` the law priced: for the picked leg,
/// `stamp/ref + w_bw·r_i` is exactly its cost (non-repair, V_i = 0), with a
/// wire RTT that differs from the echo srtt so the two bases cannot agree
/// by accident.
#[test]
fn eta_stamp_is_the_priced_e() {
    let legs = [
        WLeg { lost: 10, cwnd: 10, in_flight: 5, ..wleg(0, 40.0, Some(20.0)) },
        wleg(1, 40.0, Some(5.0)),
    ];
    let mut sched = wire_state(ProtocolHint::Auto, &legs);
    let stamp_us = sched.stamp_placement_eta(0, 1_000);
    // E = (I/W)·s + (1/2 + eps)·w = 0.5·40 + 0.51·20 = 30.2 ms (1 µs trunc).
    assert!(stamp_us.abs_diff(30_200) <= 1, "stamp = {stamp_us} µs, priced E = 30200 µs");
    let ref_s = s_of(&sched, 0).min(s_of(&sched, 1));
    let eps = eps_of(&sched, 0);
    let w_bw = sched.weights.w_bw;
    let cost = cost_of_leg(&sched, 0);
    let from_stamp = stamp_us as f64 * 1e-6 / ref_s + w_bw * (eps / (1.0 - eps));
    assert!((cost - from_stamp).abs() < 1e-4, "price {cost} != stamp-derived {from_stamp}");
}

/// The two bind gauges: `w_cold` (the `wire_i` fallback taken) and `ref_fl`
/// (the 1 ms ref floor bound), read off the `[ETA]` line.
#[test]
fn wire_base_bind_gauges_count_the_fallback_and_the_ref_floor() {
    // Fully wired, floor clear: both gauges 0 with real denominators.
    let flip = [
        WLeg { cwnd: 10, in_flight: 20, ..wleg(0, 20.0, Some(20.0)) },
        wleg(1, 150.0, Some(5.0)),
    ];
    let mut sched = wire_state(ProtocolHint::Auto, &flip);
    let _ = sched.place_costs(false, &[]);
    sched.drain_place_bind();
    let l = sched.eta().line();
    assert!(
        l.contains(" ref_fl=0.0000 w_cold=0.0000 w_n=2 w_calls=1"),
        "wired, floor clear: {l}"
    );

    // One leg unwired: half the leg evaluations took the fallback.
    let half = [wleg(0, 20.0, Some(20.0)), wleg(1, 20.0, None)];
    let mut sched = wire_state(ProtocolHint::Auto, &half);
    let _ = sched.place_costs(false, &[]);
    sched.drain_place_bind();
    assert!(sched.eta().line().contains(" w_cold=0.5000 w_n=2 "));

    // min s_j = 0.5 ms: the 1 ms floor binds.
    let fast = [wleg(0, 0.5, Some(0.5)), wleg(1, 4.0, Some(4.0))];
    let mut sched = wire_state(ProtocolHint::Auto, &fast);
    let _ = sched.place_costs(false, &[]);
    sched.drain_place_bind();
    assert!(sched.eta().line().contains(" ref_fl=1.0000 "), "ref floor bind");

    // Before any placement both denominators are 0 and the fields print `-`.
    let sched = wire_state(ProtocolHint::Auto, &flip);
    let l = sched.eta().line();
    assert!(l.contains(" ref_fl=- w_cold=- w_n=0 w_calls=0"), "{l}");
    assert!(!l.contains("w_mv"), "the argmin-moved witness is gone with the old base: {l}");
}

/// `N = 1`: one candidate, nothing to move, and the leg is still counted.
#[test]
fn wire_base_at_one_leg_is_the_identity() {
    let mut sched = wire_state(ProtocolHint::Auto, &[wleg(0, 30.0, Some(3.0))]);
    assert_eq!(sched.place_probs(false, &[]), vec![(0, 1.0)]);
    sched.drain_place_bind();
    assert!(sched.eta().line().contains(" w_n=1 w_calls=1"));
}

/// The mirror: quinn's RTT is stored for every leg the read answers, and
/// the read covers every leg - the idle ones included, which is what keeps an
/// idle leg's wire RTT fresh from traffic on the others.
#[test]
fn the_wire_mirror_writes_every_leg() {
    let mut sched = wire_state(ProtocolHint::Auto, &[wleg(0, 10.0, None), wleg(1, 10.0, None)]);
    sched.mirror_wire_rtts(|id| Some(Duration::from_micros(3_000 + 6_000 * id as u64)));
    assert_eq!(sched.path(0).unwrap().wire_rtt(), Some(Duration::from_micros(3_000)));
    assert_eq!(sched.path(1).unwrap().wire_rtt(), Some(Duration::from_micros(9_000)));
}

// ===================================================================
// Bounds on the sensitivity of the placement constants T = 0.15 and
// w_div = 1.0 (CLAUDE.md rule 17: absolute, continuous, no thresholds).
//
// T and w_div are in units of `ref`, the fastest leg's ECHO srtt, which
// includes send-buffer dwell: in seconds they are load-dependent. This
// test does not say 0.15 is right. It BOUNDS what the constant decides at
// the two operating points that matter, over the whole range the open-
// constants register allows for T and over `ref` x0.5 .. x2, so a change
// that moves the placement outside the bounds fails here rather than in a
// battery.
//
// Fixtures (declared, not measured here): idle legs (in_flight = 0,
// cwnd = 100, no loss), so cost_i = (1/2)*wire_i/ref (+ w_div*fate_i for
// a SACK-gap retransmit, `fate` = 1 on its original leg).
//   c8  leg0: echo 100 ms, wire 21 ms | leg1: echo 300 ms, wire 88 ms
//   c1d leg0: echo  20 ms, wire  2 ms | leg1: echo  20 ms, wire  2 ms
// `ref` is scaled by scaling BOTH echo srtts (wire RTTs stay put).
//
// Bounds are the min/max of the closed form sigmoid(-dC/T) over the grid
// below, widened by 1 %. The shipped point (T = 0.15, scale 1) is pinned.
//
// NEEDS THE COORDINATOR'S COMPILE CHECK: written without cargo.
// ===================================================================

const T_GRID: [f64; 9] = [0.03, 0.05, 0.08, 0.10, 0.15, 0.20, 0.30, 0.45, 0.68];
const REF_SCALES: [f64; 3] = [0.5, 1.0, 2.0];

/// `(echo0_ms, wire0_ms, echo1_ms, wire1_ms)`, idle, scaled in `ref`.
fn two_leg_at(fx: (f64, f64, f64, f64), ref_scale: f64) -> Scheduler {
    wire_state(
        ProtocolHint::Auto,
        &[wleg(0, fx.0 * ref_scale, Some(fx.1)), wleg(1, fx.2 * ref_scale, Some(fx.3))],
    )
}

/// `rel` of the value plus a 1e-12 absolute floor (the values reach 1e-25).
fn within(got: f64, want: f64, rel: f64) -> bool {
    (got - want).abs() <= rel * want.abs() + 1e-12
}

/// One operating point's declared bounds: `(lo, hi)` for P(slow leg gets a
/// source symbol), P(a retransmit stays on leg 0), P(... stays on leg 1);
/// and the pinned shipped-point values `(T = 0.15, scale 1)`.
struct SensBounds {
    p_slow: (f64, f64),
    stay0: (f64, f64),
    stay1: (f64, f64),
    shipped: (f64, f64, f64),
}

fn assert_sensitivity(name: &str, fx: (f64, f64, f64, f64), b: &SensBounds) {
    for &scale in &REF_SCALES {
        let sched = two_leg_at(fx, scale);
        for &t in &T_GRID {
            let src = sched.place_probs_with_temperature(false, &[], t);
            let p_slow = prob_of(&src, 1);
            let r0 = sched.place_probs_with_temperature(true, &[0], t);
            let r1 = sched.place_probs_with_temperature(true, &[1], t);
            let (stay0, stay1) = (prob_of(&r0, 0), prob_of(&r1, 1));
            let at = format!("{name} T={t} ref x{scale}");
            assert!(
                (b.p_slow.0..=b.p_slow.1).contains(&p_slow),
                "{at}: P_slow {p_slow:e} outside {:?}", b.p_slow
            );
            assert!(
                (b.stay0.0..=b.stay0.1).contains(&stay0),
                "{at}: P(retx stays on leg 0) {stay0:e} outside {:?}", b.stay0
            );
            assert!(
                (b.stay1.0..=b.stay1.1).contains(&stay1),
                "{at}: P(retx stays on leg 1) {stay1:e} outside {:?}", b.stay1
            );
        }
    }
    let sched = two_leg_at(fx, 1.0);
    let shipped = |covered: Option<PathId>| match covered {
        None => prob_of(&sched.place_probs(false, &[]), 1),
        Some(o) => prob_of(&sched.place_probs(true, &[o]), o),
    };
    assert!(within(shipped(None), b.shipped.0, 0.01), "{name} shipped P_slow {:e}", shipped(None));
    assert!(within(shipped(Some(0)), b.shipped.1, 0.01), "{name} shipped stay0 {:e}", shipped(Some(0)));
    assert!(within(shipped(Some(1)), b.shipped.2, 0.01), "{name} shipped stay1 {:e}", shipped(Some(1)));
}

/// c8: dC = (1/2)(88-21)/(100 ms x scale) = 0.335/scale, so the slow leg
/// takes 0.097 of the source symbols at the shipped T, at most 0.439 at
/// T = 0.68 with ref x2 and 2e-10 at T = 0.03 with ref x0.5. A SACK-gap
/// retransmit stays on the fast leg with 0.0117 at the shipped T (<= 0.381
/// anywhere on the grid), on the slow leg with 1.4e-4.
#[test]
fn placement_sensitivity_to_t_and_ref_is_bounded_at_c8() {
    assert_sensitivity(
        "c8",
        (100.0, 21.0, 300.0, 88.0),
        &SensBounds {
            p_slow: (1.978e-10, 0.4431),
            stay0: (8.79e-13, 0.3848),
            stay1: (6.6e-25, 0.1538),
            shipped: (0.096797, 0.011735, 1.3637e-4),
        },
    );
}

/// c1d: identical legs, so a source symbol splits exactly 1/2 whatever T and
/// ref are, and a retransmit's only price is w_div = 1.0 on its original
/// leg: it stays with sigmoid(-1/T) = 0.00127 at the shipped T, at most
/// 0.187 at T = 0.68 (the diversity charge is the only thing T decides here).
#[test]
fn placement_sensitivity_to_t_and_ref_is_bounded_at_c1d() {
    assert_sensitivity(
        "c1d",
        (20.0, 2.0, 20.0, 2.0),
        &SensBounds {
            p_slow: (0.5 - 1e-9, 0.5 + 1e-9),
            stay0: (3.30e-15, 0.1888),
            stay1: (3.30e-15, 0.1888),
            shipped: (0.5, 0.0012710, 0.0012710),
        },
    );
}
